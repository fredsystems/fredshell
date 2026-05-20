// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Kitty keyboard protocol push / pop / set encoders.
//!
//! Implements `PLAN_03` §4.5 for the kitty progressive-enhancement
//! keyboard protocol. Push and pop have stack semantics distinct
//! from DECSET / DECRST, so they have their own type rather than
//! living in [`crate::mode`].
//!
//! Wire forms (per the kitty keyboard protocol specification):
//!
//! - Push:  `CSI > <flags> u`
//! - Pop:   `CSI < <n> u`           — pops `n` levels (default 1).
//! - Set:   `CSI = <flags> ; <mode> u` — set with mode
//!   `1` (set), `2` (or-with), or `3` (and-not).
//! - Query: `CSI ? u`
//!
//! The flags themselves are a `u8` bitfield ([`KittyKeyboardFlags`])
//! with named constants for the five protocol-defined bits. A plain
//! bitfield is used rather than a `bitflags!` macro to keep the
//! crate dependency-free at this size.

use std::io::{self, Write};

use crate::Encode;
use crate::int::{dec_len_u8, dec_len_u16, itoa3, itoa5};

/// Kitty keyboard protocol enhancement flags.
///
/// The five bits correspond to the five protocol levels described
/// in the kitty keyboard specification. Combine with bitwise OR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KittyKeyboardFlags(u8);

impl KittyKeyboardFlags {
    /// Disambiguate escape codes (bit 1).
    pub const DISAMBIGUATE: Self = Self(0b0_0001);
    /// Report event types (press, repeat, release) (bit 2).
    pub const REPORT_EVENT_TYPES: Self = Self(0b0_0010);
    /// Report alternate keys (shifted, base layout) (bit 4).
    pub const REPORT_ALTERNATE_KEYS: Self = Self(0b0_0100);
    /// Report all keys as escape codes (bit 8).
    pub const REPORT_ALL_KEYS_AS_ESCAPE_CODES: Self = Self(0b0_1000);
    /// Report associated text with key events (bit 16).
    pub const REPORT_ASSOCIATED_TEXT: Self = Self(0b1_0000);

    /// Empty flag set (no enhancements, equivalent to plain VT
    /// keyboard handling).
    pub const NONE: Self = Self(0);

    /// Construct from raw bits. Bits outside the defined set are
    /// retained and emitted; the encoder does not validate the
    /// caller's choice.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// Underlying bit value.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Bitwise OR.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl core::ops::BitOr for KittyKeyboardFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

impl core::ops::BitOrAssign for KittyKeyboardFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Push the given flag set onto the keyboard mode stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KittyKeyboardPush {
    /// Flags to push.
    pub flags: KittyKeyboardFlags,
}

/// Pop `count` levels from the keyboard mode stack.
///
/// `count` is clamped to at least `1` on the wire; constructing
/// with `0` is treated as `1` because the kitty spec defines the
/// pop operand as omitted-or-positive-integer (default 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KittyKeyboardPop {
    /// Number of stack levels to pop.
    pub count: u16,
}

/// Mode argument for [`KittyKeyboardSet`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KittyKeyboardSetMode {
    /// `1` — replace the current flags with the given set.
    Set,
    /// `2` — bitwise-OR the given flags with the current set.
    Or,
    /// `3` — clear the given flags from the current set.
    AndNot,
}

impl KittyKeyboardSetMode {
    const fn number(self) -> u8 {
        match self {
            Self::Set => 1,
            Self::Or => 2,
            Self::AndNot => 3,
        }
    }
}

/// Set keyboard flags on the *current* stack level (no push).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KittyKeyboardSet {
    /// Flags operand.
    pub flags: KittyKeyboardFlags,
    /// How `flags` combines with the existing state.
    pub mode: KittyKeyboardSetMode,
}

/// Query the current keyboard flags. Wire form: `CSI ? u`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KittyKeyboardQuery;

impl Encode for KittyKeyboardPush {
    fn encode<W: Write + ?Sized>(&self, w: &mut W) -> io::Result<()> {
        let mut buf = [0_u8; 3];
        w.write_all(b"\x1b[>")?;
        w.write_all(itoa3(self.flags.bits(), &mut buf))?;
        w.write_all(b"u")
    }

    fn encoded_len(&self) -> usize {
        // ESC [ > <digits> u
        3 + dec_len_u8(self.flags.bits()) + 1
    }
}

impl Encode for KittyKeyboardPop {
    fn encode<W: Write + ?Sized>(&self, w: &mut W) -> io::Result<()> {
        let mut buf = [0_u8; 5];
        let n = self.count.max(1);
        w.write_all(b"\x1b[<")?;
        w.write_all(itoa5(n, &mut buf))?;
        w.write_all(b"u")
    }

    fn encoded_len(&self) -> usize {
        let n = self.count.max(1);
        3 + dec_len_u16(n) + 1
    }
}

impl Encode for KittyKeyboardSet {
    fn encode<W: Write + ?Sized>(&self, w: &mut W) -> io::Result<()> {
        let mut buf = [0_u8; 3];
        w.write_all(b"\x1b[=")?;
        w.write_all(itoa3(self.flags.bits(), &mut buf))?;
        w.write_all(b";")?;
        w.write_all(itoa3(self.mode.number(), &mut buf))?;
        w.write_all(b"u")
    }

    fn encoded_len(&self) -> usize {
        // ESC [ = <flags> ; <mode> u
        3 + dec_len_u8(self.flags.bits()) + 1 + dec_len_u8(self.mode.number()) + 1
    }
}

impl Encode for KittyKeyboardQuery {
    fn encode<W: Write + ?Sized>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(b"\x1b[?u")
    }

    fn encoded_len(&self) -> usize {
        4
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{
        KittyKeyboardFlags, KittyKeyboardPop, KittyKeyboardPush, KittyKeyboardQuery,
        KittyKeyboardSet, KittyKeyboardSetMode,
    };
    use crate::Encode;

    fn enc<E: Encode>(e: &E) -> Vec<u8> {
        let mut out = Vec::new();
        e.encode(&mut out).unwrap();
        assert_eq!(out.len(), e.encoded_len());
        out
    }

    #[test]
    fn flags_named_bits() {
        assert_eq!(KittyKeyboardFlags::DISAMBIGUATE.bits(), 1);
        assert_eq!(KittyKeyboardFlags::REPORT_EVENT_TYPES.bits(), 2);
        assert_eq!(KittyKeyboardFlags::REPORT_ALTERNATE_KEYS.bits(), 4);
        assert_eq!(
            KittyKeyboardFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES.bits(),
            8
        );
        assert_eq!(KittyKeyboardFlags::REPORT_ASSOCIATED_TEXT.bits(), 16);
    }

    #[test]
    fn flags_bitor_combines() {
        let f = KittyKeyboardFlags::DISAMBIGUATE | KittyKeyboardFlags::REPORT_EVENT_TYPES;
        assert_eq!(f.bits(), 3);
    }

    #[test]
    fn flags_bitor_assign() {
        let mut f = KittyKeyboardFlags::DISAMBIGUATE;
        f |= KittyKeyboardFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES;
        assert_eq!(f.bits(), 9);
    }

    #[test]
    fn push_zero_flags() {
        let v = KittyKeyboardPush {
            flags: KittyKeyboardFlags::NONE,
        };
        assert_eq!(enc(&v), b"\x1b[>0u");
    }

    #[test]
    fn push_typical_disambiguate_plus_events() {
        let v = KittyKeyboardPush {
            flags: KittyKeyboardFlags::DISAMBIGUATE | KittyKeyboardFlags::REPORT_EVENT_TYPES,
        };
        assert_eq!(enc(&v), b"\x1b[>3u");
    }

    #[test]
    fn push_all_flags_two_digits() {
        let v = KittyKeyboardPush {
            flags: KittyKeyboardFlags::from_bits(31),
        };
        assert_eq!(enc(&v), b"\x1b[>31u");
    }

    #[test]
    fn pop_default_is_one() {
        let v = KittyKeyboardPop { count: 0 };
        assert_eq!(enc(&v), b"\x1b[<1u");
        let v = KittyKeyboardPop { count: 1 };
        assert_eq!(enc(&v), b"\x1b[<1u");
    }

    #[test]
    fn pop_multi() {
        let v = KittyKeyboardPop { count: 7 };
        assert_eq!(enc(&v), b"\x1b[<7u");
    }

    #[test]
    fn set_modes() {
        for (mode, n) in [
            (KittyKeyboardSetMode::Set, b'1'),
            (KittyKeyboardSetMode::Or, b'2'),
            (KittyKeyboardSetMode::AndNot, b'3'),
        ] {
            let v = KittyKeyboardSet {
                flags: KittyKeyboardFlags::DISAMBIGUATE,
                mode,
            };
            let mut want = b"\x1b[=1;".to_vec();
            want.push(n);
            want.push(b'u');
            assert_eq!(enc(&v), want);
        }
    }

    #[test]
    fn query_is_csi_question_u() {
        assert_eq!(enc(&KittyKeyboardQuery), b"\x1b[?u");
    }
}
