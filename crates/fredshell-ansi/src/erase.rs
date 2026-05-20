// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! CSI erase-in-line and erase-in-display encoders.
//!
//! Implements `PLAN_03` §4.3. Encodes the six standard `EL` (erase
//! in line, `CSI n K`) and `ED` (erase in display, `CSI n J`)
//! variants. Parameter `n` is `0` (to end), `1` (to start), or `2`
//! (all). The encoder always emits the parameter explicitly — `CSI
//! K` (omitted parameter) is equivalent to `CSI 0 K` per spec but
//! is harder to grep for in traces.

use std::io::{self, Write};

use crate::Encode;

/// Erase-in-line / erase-in-display variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Erase {
    /// EL with parameter `0`: erase from cursor to end of line.
    InLineToEnd,
    /// EL with parameter `1`: erase from start of line to cursor.
    InLineToStart,
    /// EL with parameter `2`: erase entire line.
    InLineAll,
    /// ED with parameter `0`: erase from cursor to end of display.
    InDisplayToEnd,
    /// ED with parameter `1`: erase from top of display to cursor.
    InDisplayToStart,
    /// ED with parameter `2`: erase entire display.
    InDisplayAll,
}

impl Encode for Erase {
    fn encode<W: Write + ?Sized>(&self, w: &mut W) -> io::Result<()> {
        let bytes: &[u8] = match self {
            Self::InLineToEnd => b"\x1b[0K",
            Self::InLineToStart => b"\x1b[1K",
            Self::InLineAll => b"\x1b[2K",
            Self::InDisplayToEnd => b"\x1b[0J",
            Self::InDisplayToStart => b"\x1b[1J",
            Self::InDisplayAll => b"\x1b[2J",
        };
        w.write_all(bytes)
    }

    fn encoded_len(&self) -> usize {
        // All variants are exactly 4 bytes: ESC [ <digit> <final>.
        4
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::Erase;
    use crate::Encode;

    fn enc(e: Erase) -> Vec<u8> {
        let mut out = Vec::new();
        e.encode(&mut out).unwrap();
        assert_eq!(out.len(), e.encoded_len());
        out
    }

    #[test]
    fn erase_in_line_variants() {
        assert_eq!(enc(Erase::InLineToEnd), b"\x1b[0K");
        assert_eq!(enc(Erase::InLineToStart), b"\x1b[1K");
        assert_eq!(enc(Erase::InLineAll), b"\x1b[2K");
    }

    #[test]
    fn erase_in_display_variants() {
        assert_eq!(enc(Erase::InDisplayToEnd), b"\x1b[0J");
        assert_eq!(enc(Erase::InDisplayToStart), b"\x1b[1J");
        assert_eq!(enc(Erase::InDisplayAll), b"\x1b[2J");
    }

    #[test]
    fn all_variants_are_four_bytes() {
        for v in [
            Erase::InLineToEnd,
            Erase::InLineToStart,
            Erase::InLineAll,
            Erase::InDisplayToEnd,
            Erase::InDisplayToStart,
            Erase::InDisplayAll,
        ] {
            assert_eq!(v.encoded_len(), 4);
        }
    }
}
