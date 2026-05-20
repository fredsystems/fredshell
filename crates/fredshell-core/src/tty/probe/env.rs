// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Environment-variable capability heuristics.
//!
//! These rules are the second-priority information source after
//! active probe responses (`PLAN_04` §5.2). They are pure: callers
//! provide a snapshot via [`Env`], so tests never touch the real
//! process environment.
//!
//! The heuristics are deliberately conservative — every bit set
//! here must be defensible against the most hostile terminal that
//! still sets the variable in question. When in doubt, leave the
//! bit clear; the cost of underclaiming a capability is a less
//! pretty prompt, while the cost of overclaiming is broken output.

use crate::tty::capabilities::{Capabilities, ColorSupport, Osc8Support};

/// Snapshot of the subset of environment variables consulted by
/// the capability heuristics.
///
/// All fields are owned `Option<String>` so the snapshot is `Send`
/// and decoupled from the live process environment. Use
/// [`Env::from_process`] to capture the real environment, or
/// construct a literal value in tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    /// Value of `$COLORTERM` (e.g. `truecolor`, `24bit`, `256`).
    pub colorterm: Option<String>,
    /// Value of `$TERM` (e.g. `xterm-256color`, `screen`).
    pub term: Option<String>,
    /// Value of `$TERM_PROGRAM` (e.g. `iTerm.app`, `WezTerm`,
    /// `vscode`).
    pub term_program: Option<String>,
}

impl Env {
    /// Capture the relevant variables from the live process
    /// environment. Variables that are unset or contain invalid
    /// UTF-8 are recorded as `None`.
    #[must_use]
    pub fn from_process() -> Self {
        Self {
            colorterm: std::env::var("COLORTERM").ok(),
            term: std::env::var("TERM").ok(),
            term_program: std::env::var("TERM_PROGRAM").ok(),
        }
    }
}

/// Apply environment-variable heuristics to `caps`.
///
/// This function never *downgrades* a capability: if an active
/// probe response has already set a stronger color tier or marked
/// OSC 8 as `Supported`, those values are preserved. Env-vars are
/// strictly a *floor*, never a ceiling.
///
/// Rules (`PLAN_04` §5.2):
///
/// - `COLORTERM=truecolor` or `COLORTERM=24bit` → [`ColorSupport::TrueColor`].
/// - `COLORTERM` containing `256`, or `TERM` containing `256color`,
///   → at least [`ColorSupport::Ansi256`].
/// - `TERM_PROGRAM` in a known-good allowlist → OSC 8
///   [`Osc8Support::Supported`].
/// - Bracketed paste and focus reporting are assumed supported on
///   any non-dumb `TERM`. These are cheap and overwhelmingly
///   universal on modern terminals; the dumb-terminal carve-out
///   exists for `TERM=dumb` and friends used by editor scripts.
pub fn apply(caps: &mut Capabilities, env: &Env) {
    apply_color(caps, env);
    apply_osc8(caps, env);
    apply_paste_and_focus(caps, env);
}

fn apply_color(caps: &mut Capabilities, env: &Env) {
    let from_env = detect_color(env);
    if color_rank(from_env) > color_rank(caps.color) {
        caps.color = from_env;
    }
}

/// Numeric rank used to order [`ColorSupport`] tiers from least to
/// most capable. Kept private to this module so the public enum
/// does not implicitly invite ordered comparisons elsewhere.
const fn color_rank(c: ColorSupport) -> u8 {
    match c {
        ColorSupport::None => 0,
        ColorSupport::Ansi16 => 1,
        ColorSupport::Ansi256 => 2,
        ColorSupport::TrueColor => 3,
    }
}

fn detect_color(env: &Env) -> ColorSupport {
    if let Some(ct) = env.colorterm.as_deref() {
        let lower = ct.to_ascii_lowercase();
        if lower == "truecolor" || lower == "24bit" {
            return ColorSupport::TrueColor;
        }
        if lower.contains("256") {
            return ColorSupport::Ansi256;
        }
    }
    if let Some(term) = env.term.as_deref() {
        if term.contains("256color") {
            return ColorSupport::Ansi256;
        }
        if !is_dumb_term(term) {
            return ColorSupport::Ansi16;
        }
    }
    ColorSupport::None
}

fn apply_osc8(caps: &mut Capabilities, env: &Env) {
    if caps.osc8_hyperlinks == Osc8Support::Supported {
        return;
    }
    if let Some(program) = env.term_program.as_deref()
        && is_known_osc8_program(program)
    {
        caps.osc8_hyperlinks = Osc8Support::Supported;
    }
}

fn apply_paste_and_focus(caps: &mut Capabilities, env: &Env) {
    // Conservative: only enable when TERM is explicitly set to a
    // non-dumb value. A missing TERM is treated like a dumb
    // terminal because the consumer may be a script harness or a
    // CI runner that pipes our output somewhere that mis-renders
    // escape sequences.
    let enable = env.term.as_deref().is_some_and(|t| !is_dumb_term(t));
    if enable {
        caps.bracketed_paste = true;
        caps.focus_reporting = true;
    }
}

fn is_dumb_term(term: &str) -> bool {
    matches!(term, "dumb" | "unknown" | "")
}

/// Terminal programs known to support OSC 8 hyperlinks.
///
/// Membership in this list is based on terminal-emulator
/// documentation and behavioral testing. Programs not listed get
/// [`Osc8Support::Unknown`], which emitters must treat as
/// "do not emit hyperlinks."
fn is_known_osc8_program(program: &str) -> bool {
    matches!(
        program,
        "iTerm.app" | "WezTerm" | "vscode" | "Hyper" | "kitty" | "ghostty" | "Tabby" | "alacritty"
    )
}

// ColorSupport must order least → most capable for `apply_color`'s
// "never downgrade" comparison to work; that ordering is encoded in
// `color_rank` above rather than via a `PartialOrd` impl on the
// public type.

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{Env, apply};
    use crate::tty::capabilities::{Capabilities, ColorSupport, Osc8Support};

    fn env(colorterm: Option<&str>, term: Option<&str>, term_program: Option<&str>) -> Env {
        Env {
            colorterm: colorterm.map(str::to_owned),
            term: term.map(str::to_owned),
            term_program: term_program.map(str::to_owned),
        }
    }

    #[test]
    fn colorterm_truecolor_sets_truecolor() {
        let mut caps = Capabilities::default();
        apply(&mut caps, &env(Some("truecolor"), Some("xterm"), None));
        assert_eq!(caps.color, ColorSupport::TrueColor);
    }

    #[test]
    fn colorterm_24bit_sets_truecolor() {
        let mut caps = Capabilities::default();
        apply(&mut caps, &env(Some("24bit"), Some("xterm"), None));
        assert_eq!(caps.color, ColorSupport::TrueColor);
    }

    #[test]
    fn colorterm_truecolor_is_case_insensitive() {
        let mut caps = Capabilities::default();
        apply(&mut caps, &env(Some("TrueColor"), Some("xterm"), None));
        assert_eq!(caps.color, ColorSupport::TrueColor);
    }

    #[test]
    fn colorterm_256_sets_ansi256() {
        let mut caps = Capabilities::default();
        apply(&mut caps, &env(Some("256"), Some("xterm"), None));
        assert_eq!(caps.color, ColorSupport::Ansi256);
    }

    #[test]
    fn term_256color_sets_ansi256() {
        let mut caps = Capabilities::default();
        apply(&mut caps, &env(None, Some("xterm-256color"), None));
        assert_eq!(caps.color, ColorSupport::Ansi256);
    }

    #[test]
    fn term_xterm_sets_ansi16() {
        let mut caps = Capabilities::default();
        apply(&mut caps, &env(None, Some("xterm"), None));
        assert_eq!(caps.color, ColorSupport::Ansi16);
    }

    #[test]
    fn term_dumb_leaves_color_none() {
        let mut caps = Capabilities::default();
        apply(&mut caps, &env(None, Some("dumb"), None));
        assert_eq!(caps.color, ColorSupport::None);
    }

    #[test]
    fn empty_env_leaves_color_none() {
        let mut caps = Capabilities::default();
        apply(&mut caps, &env(None, None, None));
        assert_eq!(caps.color, ColorSupport::None);
    }

    #[test]
    fn env_never_downgrades_existing_truecolor() {
        let mut caps = Capabilities {
            color: ColorSupport::TrueColor,
            ..Capabilities::default()
        };
        // Env only advertises 256, but we already have TrueColor
        // from an active probe — must not downgrade.
        apply(&mut caps, &env(Some("256"), Some("xterm"), None));
        assert_eq!(caps.color, ColorSupport::TrueColor);
    }

    #[test]
    fn env_promotes_when_stronger_than_existing() {
        let mut caps = Capabilities {
            color: ColorSupport::Ansi16,
            ..Capabilities::default()
        };
        apply(&mut caps, &env(Some("truecolor"), Some("xterm"), None));
        assert_eq!(caps.color, ColorSupport::TrueColor);
    }

    #[test]
    fn known_term_program_enables_osc8() {
        let mut caps = Capabilities::default();
        apply(&mut caps, &env(None, Some("xterm"), Some("WezTerm")));
        assert_eq!(caps.osc8_hyperlinks, Osc8Support::Supported);
    }

    #[test]
    fn iterm_enables_osc8() {
        let mut caps = Capabilities::default();
        apply(&mut caps, &env(None, Some("xterm"), Some("iTerm.app")));
        assert_eq!(caps.osc8_hyperlinks, Osc8Support::Supported);
    }

    #[test]
    fn unknown_term_program_leaves_osc8_unknown() {
        let mut caps = Capabilities::default();
        apply(
            &mut caps,
            &env(None, Some("xterm"), Some("SomeRandomTerminal")),
        );
        assert_eq!(caps.osc8_hyperlinks, Osc8Support::Unknown);
    }

    #[test]
    fn env_does_not_downgrade_osc8_supported() {
        let mut caps = Capabilities {
            osc8_hyperlinks: Osc8Support::Supported,
            ..Capabilities::default()
        };
        apply(
            &mut caps,
            &env(None, Some("xterm"), Some("SomeRandomTerminal")),
        );
        assert_eq!(caps.osc8_hyperlinks, Osc8Support::Supported);
    }

    #[test]
    fn non_dumb_term_enables_paste_and_focus() {
        let mut caps = Capabilities::default();
        apply(&mut caps, &env(None, Some("xterm-256color"), None));
        assert!(caps.bracketed_paste);
        assert!(caps.focus_reporting);
    }

    #[test]
    fn dumb_term_leaves_paste_and_focus_off() {
        let mut caps = Capabilities::default();
        apply(&mut caps, &env(None, Some("dumb"), None));
        assert!(!caps.bracketed_paste);
        assert!(!caps.focus_reporting);
    }

    #[test]
    fn missing_term_leaves_paste_and_focus_off() {
        // Conservative: if we can't even see TERM, don't claim
        // bracketed paste — the consumer may be a script harness.
        let mut caps = Capabilities::default();
        apply(&mut caps, &env(None, None, None));
        assert!(!caps.bracketed_paste);
        assert!(!caps.focus_reporting);
    }

    #[test]
    fn env_does_not_touch_synchronized_output() {
        let mut caps = Capabilities::default();
        apply(&mut caps, &env(Some("truecolor"), Some("xterm"), None));
        assert!(!caps.synchronized_output);
    }

    #[test]
    fn env_does_not_touch_kitty_keyboard() {
        let mut caps = Capabilities::default();
        apply(&mut caps, &env(Some("truecolor"), Some("xterm"), None));
        assert!(!caps.kitty_keyboard);
    }

    #[test]
    fn from_process_does_not_panic() {
        // Just exercise the syscall path — value depends on the
        // host environment so we only check the function returns.
        let _ = Env::from_process();
    }
}
