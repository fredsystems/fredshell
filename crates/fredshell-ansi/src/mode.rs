// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! DECSET / DECRST mode encoders.
//!
//! Implements `PLAN_03` §4.5 for the small set of modes a shell
//! actually toggles. Kitty keyboard protocol push/pop has distinct
//! semantics and lives in [`crate::kitty`].
//!
//! The encoded form is:
//!
//! - DECSET: `CSI ? <n> h`
//! - DECRST: `CSI ? <n> l`
//!
//! Where `<n>` is the mode number from the [`Mode`] variant.

use std::io::{self, Write};

use crate::Encode;
use crate::int::{dec_len_u16, itoa5};

/// The terminal modes a shell toggles via DECSET / DECRST.
///
/// This is intentionally a small, closed enumeration. Modes that a
/// shell does not toggle (e.g. autowrap, origin mode) are owned by
/// the terminal and not exposed here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Application cursor keys (DECCKM, mode `1`).
    ApplicationCursorKeys,
    /// Mouse-tracking VT200 normal protocol (mode `1000`). Included
    /// so the line editor can opt in if it ever wants mouse-aware
    /// selection; current `PLAN_07` does not require it.
    MouseVt200,
    /// Mouse SGR encoding (mode `1006`).
    MouseSgr,
    /// Focus-in / focus-out reporting (mode `1004`).
    FocusReporting,
    /// Alternate screen buffer with save/restore cursor (mode
    /// `1049`). The "1049" variant is the one the line editor wants;
    /// the older `47` and `1047` exist but are not exposed.
    AlternateScreen,
    /// Bracketed paste (mode `2004`).
    BracketedPaste,
}

impl Mode {
    /// Numeric mode value as it appears between `?` and the final
    /// byte.
    #[must_use]
    pub const fn number(self) -> u16 {
        match self {
            Self::ApplicationCursorKeys => 1,
            Self::MouseVt200 => 1000,
            Self::FocusReporting => 1004,
            Self::MouseSgr => 1006,
            Self::AlternateScreen => 1049,
            Self::BracketedPaste => 2004,
        }
    }
}

/// DECSET — set the given private mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeSet {
    /// The mode to enable.
    pub mode: Mode,
}

/// DECRST — reset the given private mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeReset {
    /// The mode to disable.
    pub mode: Mode,
}

fn write_mode<W: Write + ?Sized>(w: &mut W, n: u16, final_byte: u8) -> io::Result<()> {
    let mut buf = [0_u8; 5];
    w.write_all(b"\x1b[?")?;
    w.write_all(itoa5(n, &mut buf))?;
    w.write_all(&[final_byte])
}

const fn mode_len(n: u16) -> usize {
    // ESC [ ? <digits> <final>
    3 + dec_len_u16(n) + 1
}

impl Encode for ModeSet {
    fn encode<W: Write + ?Sized>(&self, w: &mut W) -> io::Result<()> {
        write_mode(w, self.mode.number(), b'h')
    }

    fn encoded_len(&self) -> usize {
        mode_len(self.mode.number())
    }
}

impl Encode for ModeReset {
    fn encode<W: Write + ?Sized>(&self, w: &mut W) -> io::Result<()> {
        write_mode(w, self.mode.number(), b'l')
    }

    fn encoded_len(&self) -> usize {
        mode_len(self.mode.number())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{Mode, ModeReset, ModeSet};
    use crate::Encode;

    fn enc<E: Encode>(e: &E) -> Vec<u8> {
        let mut out = Vec::new();
        e.encode(&mut out).unwrap();
        assert_eq!(out.len(), e.encoded_len());
        out
    }

    #[test]
    fn mode_numbers_are_canonical() {
        assert_eq!(Mode::ApplicationCursorKeys.number(), 1);
        assert_eq!(Mode::MouseVt200.number(), 1000);
        assert_eq!(Mode::FocusReporting.number(), 1004);
        assert_eq!(Mode::MouseSgr.number(), 1006);
        assert_eq!(Mode::AlternateScreen.number(), 1049);
        assert_eq!(Mode::BracketedPaste.number(), 2004);
    }

    #[test]
    fn decset_application_cursor_keys() {
        let v = ModeSet {
            mode: Mode::ApplicationCursorKeys,
        };
        assert_eq!(enc(&v), b"\x1b[?1h");
    }

    #[test]
    fn decset_alternate_screen() {
        let v = ModeSet {
            mode: Mode::AlternateScreen,
        };
        assert_eq!(enc(&v), b"\x1b[?1049h");
    }

    #[test]
    fn decset_bracketed_paste() {
        let v = ModeSet {
            mode: Mode::BracketedPaste,
        };
        assert_eq!(enc(&v), b"\x1b[?2004h");
    }

    #[test]
    fn decrst_application_cursor_keys() {
        let v = ModeReset {
            mode: Mode::ApplicationCursorKeys,
        };
        assert_eq!(enc(&v), b"\x1b[?1l");
    }

    #[test]
    fn decrst_bracketed_paste() {
        let v = ModeReset {
            mode: Mode::BracketedPaste,
        };
        assert_eq!(enc(&v), b"\x1b[?2004l");
    }

    #[test]
    fn decset_focus_and_mouse() {
        assert_eq!(
            enc(&ModeSet {
                mode: Mode::FocusReporting
            }),
            b"\x1b[?1004h"
        );
        assert_eq!(
            enc(&ModeSet {
                mode: Mode::MouseVt200
            }),
            b"\x1b[?1000h"
        );
        assert_eq!(
            enc(&ModeSet {
                mode: Mode::MouseSgr
            }),
            b"\x1b[?1006h"
        );
    }
}
