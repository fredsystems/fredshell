// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Terminal capability snapshot.
//!
//! Owns the [`Capabilities`] aggregate returned by the startup probe
//! (see `PLAN_04` §5 and subtask 04.9). This module currently exposes
//! the type-level surface only; the probe orchestration itself lands
//! in subtask 04.9 and the individual probe decoders live under
//! [`super::probe`].

/// Detected color support tier.
///
/// Values are ordered: each tier is a strict superset of the ones
/// before it. The probe stores the most capable tier observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorSupport {
    /// No color support was detected.
    #[default]
    None,
    /// Sixteen-color ANSI palette.
    Ansi16,
    /// 256-color indexed palette.
    Ansi256,
    /// 24-bit direct RGB color (`COLORTERM=truecolor` or detected via
    /// the SGR 38;2 round-trip in the probe).
    TrueColor,
}

/// Detected OSC 8 hyperlink support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Osc8Support {
    /// OSC 8 support is unknown or not advertised.
    #[default]
    Unknown,
    /// The terminal acknowledged OSC 8 during the probe.
    Supported,
    /// The terminal explicitly does not support OSC 8.
    Unsupported,
}

/// Snapshot of the terminal's capabilities at session open time.
///
/// All fields default to the most conservative interpretation so
/// that `Capabilities::default()` is safe to use when the probe is
/// skipped (`FREDSHELL_NO_PROBE=1`) or in non-interactive mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// Highest detected color tier.
    pub color: ColorSupport,
    /// OSC 8 hyperlink support.
    pub osc8: Osc8Support,
    /// Kitty keyboard protocol acknowledged.
    pub kitty_keyboard: bool,
    /// OSC 52 clipboard write acknowledged.
    pub osc52_clipboard: bool,
    /// Synchronized output (mode 2026) acknowledged.
    pub synchronized_output: bool,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{Capabilities, ColorSupport, Osc8Support};

    #[test]
    fn capabilities_default_is_conservative() {
        let c = Capabilities::default();
        assert_eq!(c.color, ColorSupport::None);
        assert_eq!(c.osc8, Osc8Support::Unknown);
        assert!(!c.kitty_keyboard);
        assert!(!c.osc52_clipboard);
        assert!(!c.synchronized_output);
    }

    #[test]
    fn color_support_ordering_is_total() {
        // Sanity: distinct variants compare unequal.
        assert_ne!(ColorSupport::None, ColorSupport::Ansi16);
        assert_ne!(ColorSupport::Ansi16, ColorSupport::Ansi256);
        assert_ne!(ColorSupport::Ansi256, ColorSupport::TrueColor);
    }

    #[test]
    fn osc8_support_default_is_unknown() {
        assert_eq!(Osc8Support::default(), Osc8Support::Unknown);
    }
}
