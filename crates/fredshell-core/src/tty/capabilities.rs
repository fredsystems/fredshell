// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Terminal capability snapshot.
//!
//! Owns the [`Capabilities`] aggregate returned by the startup probe
//! (see `PLAN_04` §5). The struct shape mirrors `PLAN_04` §5.1
//! exactly; each field is populated by either an active probe
//! (response decoded in [`super::probe`]) or an environment-variable
//! heuristic.

/// Detected color support tier.
///
/// Tiers are ordered from least to most capable. The probe stores
/// the highest tier the terminal advertises across the three
/// information sources (`$COLORTERM`, DA1 capabilities, fallback).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorSupport {
    /// No color support was detected (dumb terminal).
    #[default]
    None,
    /// Sixteen-color ANSI palette.
    Ansi16,
    /// 256-color indexed palette.
    Ansi256,
    /// 24-bit direct RGB color (`COLORTERM=truecolor` or
    /// `COLORTERM=24bit`).
    TrueColor,
}

/// Detected OSC 8 hyperlink support.
///
/// OSC 8 has no reliable active probe on most terminals, so this is
/// a three-valued type: `Unknown` is the conservative default when
/// we cannot tell, and emitters that care should treat it the same
/// as `Unsupported`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Osc8Support {
    /// OSC 8 support could not be determined.
    #[default]
    Unknown,
    /// The terminal is known to support OSC 8 hyperlinks
    /// (detected via `$TERM_PROGRAM` heuristic).
    Supported,
    /// The terminal is known to not support OSC 8 hyperlinks.
    Unsupported,
}

/// Snapshot of the terminal's capabilities at session open time.
///
/// All fields default to the most conservative interpretation so
/// `Capabilities::default()` is safe to use when the probe is
/// skipped (`FREDSHELL_NO_PROBE=1`) or in non-interactive mode.
/// Defaults are also what callers see if a specific probe times
/// out: `PLAN_04` §4 forbids silent fallback, so emitters must
/// branch on a typed bool rather than discover features lazily.
#[allow(clippy::struct_excessive_bools)] // Mirrors PLAN_04 §5.1 1:1; each bit is an independent capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// Highest detected color tier.
    pub color: ColorSupport,
    /// Kitty keyboard protocol acknowledged (responded to the
    /// progressive-enhancement query).
    pub kitty_keyboard: bool,
    /// Bracketed paste support. Detected via env-var heuristic in
    /// v1; can be promoted to an active probe later.
    pub bracketed_paste: bool,
    /// Focus reporting (CSI ? 1004) supported.
    pub focus_reporting: bool,
    /// Synchronized output (DEC private mode 2026) supported.
    pub synchronized_output: bool,
    /// OSC 8 hyperlinks supported.
    pub osc8_hyperlinks: Osc8Support,
    /// OSC 52 clipboard write supported.
    pub osc52_clipboard: bool,
    /// OSC 133 semantic prompt markers supported.
    pub osc133_semantic_prompt: bool,
    /// OSC 7 working-directory reporting supported.
    pub osc7_cwd: bool,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{Capabilities, ColorSupport, Osc8Support};

    #[test]
    fn capabilities_default_is_conservative() {
        let c = Capabilities::default();
        assert_eq!(c.color, ColorSupport::None);
        assert_eq!(c.osc8_hyperlinks, Osc8Support::Unknown);
        assert!(!c.kitty_keyboard);
        assert!(!c.bracketed_paste);
        assert!(!c.focus_reporting);
        assert!(!c.synchronized_output);
        assert!(!c.osc52_clipboard);
        assert!(!c.osc133_semantic_prompt);
        assert!(!c.osc7_cwd);
    }

    #[test]
    fn color_support_default_is_none() {
        assert_eq!(ColorSupport::default(), ColorSupport::None);
    }

    #[test]
    fn color_support_variants_are_distinct() {
        assert_ne!(ColorSupport::None, ColorSupport::Ansi16);
        assert_ne!(ColorSupport::Ansi16, ColorSupport::Ansi256);
        assert_ne!(ColorSupport::Ansi256, ColorSupport::TrueColor);
    }

    #[test]
    fn osc8_support_default_is_unknown() {
        assert_eq!(Osc8Support::default(), Osc8Support::Unknown);
    }

    #[test]
    fn capabilities_is_copy() {
        const _: fn() = || {
            fn assert_copy<T: Copy>() {}
            assert_copy::<Capabilities>();
        };
    }
}
