// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Interpret decoded probe responses into capability bits.
//!
//! Pure functions: they consume the response structs produced by
//! `fredshell_ansi::decode` and update a [`Capabilities`] in place.
//! No I/O.

use fredshell_ansi::decode::{
    Da1Response, DecrpmResponse, KittyKeyboardQueryResponse, Osc52ReadResponse,
};

use crate::tty::capabilities::{Capabilities, ColorSupport};

/// VT capability code reported in a DA1 response that indicates
/// 132-column mode support — a weak but commonly observed proxy for
/// "at least ANSI 16 colors." See `PLAN_03` §6.
const DA1_CAP_132_COLUMNS: u16 = 1;
/// VT capability code: `ReGIS` graphics. Terminals advertising
/// `ReGIS` generally also advertise full color support.
const DA1_CAP_REGIS: u16 = 3;
/// VT capability code: `Sixel` graphics. Same color implication as
/// `ReGIS`.
const DA1_CAP_SIXEL: u16 = 4;

/// Update `caps` with information drawn from a DA1 response.
///
/// DA1 is the weakest of the probe responses for color detection
/// because the standard capability codes were defined in the era of
/// VT 200/300 hardware; modern terminals echo them mostly out of
/// tradition. We therefore use DA1 only as a *lower bound*: if the
/// terminal advertises 132-column mode, sixel, or `ReGIS`, we promote
/// `color` to at least [`ColorSupport::Ansi16`] when no stronger
/// signal has been recorded. Truecolor and 256-color detection come
/// from `$COLORTERM` in [`super::env::apply`].
pub fn apply_da1(caps: &mut Capabilities, response: &Da1Response) {
    if caps.color != ColorSupport::None {
        return;
    }
    for &code in response.capabilities() {
        if matches!(code, DA1_CAP_132_COLUMNS | DA1_CAP_REGIS | DA1_CAP_SIXEL) {
            caps.color = ColorSupport::Ansi16;
            return;
        }
    }
}

/// Mark kitty keyboard protocol as supported.
///
/// Presence of a parseable response — regardless of the reported
/// flags — is sufficient evidence that the terminal speaks the
/// progressive-enhancement protocol. The flags themselves describe
/// the current top-of-stack state, which fredshell does not yet
/// consume; see `PLAN_13` for the keyboard mode push/pop dance.
pub const fn apply_kitty_keyboard(caps: &mut Capabilities, _response: &KittyKeyboardQueryResponse) {
    caps.kitty_keyboard = true;
}

/// Update `caps` from a DECRPM mode-report response.
///
/// Only mode 2026 (synchronized output) is currently consumed; other
/// modes are ignored so future probes can land without coordinating
/// with this function. The state value only matters for the
/// recognized/unrecognized distinction (see `DecrpmState::is_supported`).
pub const fn apply_decrpm(caps: &mut Capabilities, response: &DecrpmResponse) {
    if response.mode == 2026 && response.state.is_supported() {
        caps.synchronized_output = true;
    }
}

/// Mark OSC 52 clipboard write as supported.
///
/// A parseable OSC 52 response indicates the terminal both
/// understood the query and was willing to expose clipboard state.
/// We treat any read response (including an empty payload) as
/// "supported"; the absence of a response after the timeout is the
/// only signal for "not supported."
pub const fn apply_osc52(caps: &mut Capabilities, _response: &Osc52ReadResponse) {
    caps.osc52_clipboard = true;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{apply_da1, apply_decrpm, apply_kitty_keyboard, apply_osc52};
    use crate::tty::capabilities::{Capabilities, ColorSupport};
    use fredshell_ansi::Decode;
    use fredshell_ansi::decode::{
        Da1Response, DecrpmResponse, KittyKeyboardQueryResponse, Osc52ReadResponse,
    };

    fn decode<T: Decode>(input: &[u8]) -> T {
        T::decode(input).unwrap().0
    }

    #[test]
    fn apply_da1_promotes_color_when_sixel_advertised() {
        let mut caps = Capabilities::default();
        let response: Da1Response = decode(b"\x1b[?64;4c");
        apply_da1(&mut caps, &response);
        assert_eq!(caps.color, ColorSupport::Ansi16);
    }

    #[test]
    fn apply_da1_promotes_color_when_132_columns_advertised() {
        let mut caps = Capabilities::default();
        let response: Da1Response = decode(b"\x1b[?64;1c");
        apply_da1(&mut caps, &response);
        assert_eq!(caps.color, ColorSupport::Ansi16);
    }

    #[test]
    fn apply_da1_does_not_demote_existing_truecolor() {
        let mut caps = Capabilities {
            color: ColorSupport::TrueColor,
            ..Capabilities::default()
        };
        // Even a DA1 with no useful capabilities must not demote
        // a color tier that env-vars already established.
        let response: Da1Response = decode(b"\x1b[?64c");
        apply_da1(&mut caps, &response);
        assert_eq!(caps.color, ColorSupport::TrueColor);
    }

    #[test]
    fn apply_da1_leaves_color_none_when_no_known_caps() {
        let mut caps = Capabilities::default();
        // Code 22 (color) is in DA1 but we are intentionally
        // conservative and don't use it as a truecolor signal.
        let response: Da1Response = decode(b"\x1b[?64;22c");
        apply_da1(&mut caps, &response);
        assert_eq!(caps.color, ColorSupport::None);
    }

    #[test]
    fn apply_kitty_keyboard_sets_bit_regardless_of_flags() {
        let mut caps = Capabilities::default();
        let response: KittyKeyboardQueryResponse = decode(b"\x1b[?0u");
        apply_kitty_keyboard(&mut caps, &response);
        assert!(caps.kitty_keyboard);
    }

    #[test]
    fn apply_kitty_keyboard_with_nonzero_flags_also_sets_bit() {
        let mut caps = Capabilities::default();
        let response: KittyKeyboardQueryResponse = decode(b"\x1b[?15u");
        apply_kitty_keyboard(&mut caps, &response);
        assert!(caps.kitty_keyboard);
    }

    #[test]
    fn apply_decrpm_2026_set_marks_synchronized_output() {
        let mut caps = Capabilities::default();
        let response: DecrpmResponse = decode(b"\x1b[?2026;1$y");
        apply_decrpm(&mut caps, &response);
        assert!(caps.synchronized_output);
    }

    #[test]
    fn apply_decrpm_2026_reset_marks_synchronized_output() {
        // "Reset" (mode known but currently off) is still support.
        let mut caps = Capabilities::default();
        let response: DecrpmResponse = decode(b"\x1b[?2026;2$y");
        apply_decrpm(&mut caps, &response);
        assert!(caps.synchronized_output);
    }

    #[test]
    fn apply_decrpm_2026_not_recognized_leaves_bit_unset() {
        let mut caps = Capabilities::default();
        let response: DecrpmResponse = decode(b"\x1b[?2026;0$y");
        apply_decrpm(&mut caps, &response);
        assert!(!caps.synchronized_output);
    }

    #[test]
    fn apply_decrpm_ignores_other_modes() {
        let mut caps = Capabilities::default();
        // Mode 25 = cursor visibility — not a capability we read.
        let response: DecrpmResponse = decode(b"\x1b[?25;1$y");
        apply_decrpm(&mut caps, &response);
        assert!(!caps.synchronized_output);
    }

    #[test]
    fn apply_osc52_marks_clipboard_supported() {
        let mut caps = Capabilities::default();
        let response: Osc52ReadResponse = decode(b"\x1b]52;c;aGk=\x1b\\");
        apply_osc52(&mut caps, &response);
        assert!(caps.osc52_clipboard);
    }
}
