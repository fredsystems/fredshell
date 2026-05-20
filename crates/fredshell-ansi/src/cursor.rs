// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! CSI cursor-movement encoders.
//!
//! Implements `PLAN_03` §4.3. Coordinates are 1-indexed per VT
//! convention; constructors that take coordinates validate them and
//! return [`crate::EncodeError::InvalidCoordinate`] for zero values.
//!
//! The relative-move variants ([`Cursor::Up`], [`Cursor::Down`],
//! [`Cursor::Left`], [`Cursor::Right`]) treat `0` as a no-op: the
//! encoder writes nothing and [`Cursor::encoded_len`] returns 0.
//! This matches what most terminal emulators do with `CSI 0 A` (a
//! one-line move) and gives callers a uniform "tell the encoder how
//! far to move; it figures out whether to emit anything" surface.

use std::io::{self, Write};

use crate::int::{dec_len_u16, itoa5};
use crate::{Encode, EncodeError};

/// Cursor-movement sequences.
///
/// All variants except [`Cursor::Save`] / [`Cursor::Restore`] carry
/// numeric parameters. [`Cursor::Goto`] uses 1-indexed row/column
/// per the VT spec; construct it via [`Cursor::goto`] for validated
/// input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    /// Move cursor up by N rows. `0` is a no-op.
    Up(u16),
    /// Move cursor down by N rows. `0` is a no-op.
    Down(u16),
    /// Move cursor left by N columns. `0` is a no-op.
    Left(u16),
    /// Move cursor right by N columns. `0` is a no-op.
    Right(u16),
    /// Move cursor to absolute (row, col) — both 1-indexed.
    ///
    /// Construct via [`Cursor::goto`] to reject `0`.
    Goto {
        /// 1-indexed row.
        row: u16,
        /// 1-indexed column.
        col: u16,
    },
    /// DECSC — save cursor position and attributes.
    Save,
    /// DECRC — restore cursor position and attributes.
    Restore,
}

impl Cursor {
    /// Build a [`Cursor::Goto`] with validated 1-indexed coordinates.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError::InvalidCoordinate`] if `row` or `col`
    /// is zero.
    pub const fn goto(row: u16, col: u16) -> Result<Self, EncodeError> {
        if row == 0 {
            return Err(EncodeError::InvalidCoordinate { field: "row" });
        }
        if col == 0 {
            return Err(EncodeError::InvalidCoordinate { field: "col" });
        }
        Ok(Self::Goto { row, col })
    }
}

/// Write `CSI <n> <final>` for relative moves. Emits nothing if
/// `n == 0`.
fn write_csi_n<W: Write + ?Sized>(w: &mut W, n: u16, final_byte: u8) -> io::Result<()> {
    if n == 0 {
        return Ok(());
    }
    let mut buf = [0_u8; 5];
    w.write_all(b"\x1b[")?;
    w.write_all(itoa5(n, &mut buf))?;
    w.write_all(&[final_byte])
}

const fn csi_n_len(n: u16) -> usize {
    if n == 0 { 0 } else { 2 + dec_len_u16(n) + 1 }
}

impl Encode for Cursor {
    fn encode<W: Write + ?Sized>(&self, w: &mut W) -> io::Result<()> {
        match *self {
            Self::Up(n) => write_csi_n(w, n, b'A'),
            Self::Down(n) => write_csi_n(w, n, b'B'),
            Self::Right(n) => write_csi_n(w, n, b'C'),
            Self::Left(n) => write_csi_n(w, n, b'D'),
            Self::Goto { row, col } => {
                let mut buf = [0_u8; 5];
                w.write_all(b"\x1b[")?;
                w.write_all(itoa5(row, &mut buf))?;
                w.write_all(b";")?;
                w.write_all(itoa5(col, &mut buf))?;
                w.write_all(b"H")
            }
            Self::Save => w.write_all(b"\x1b7"),
            Self::Restore => w.write_all(b"\x1b8"),
        }
    }

    fn encoded_len(&self) -> usize {
        match *self {
            Self::Up(n) | Self::Down(n) | Self::Left(n) | Self::Right(n) => csi_n_len(n),
            Self::Goto { row, col } => 2 + dec_len_u16(row) + 1 + dec_len_u16(col) + 1,
            Self::Save | Self::Restore => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::Cursor;
    use crate::{Encode, EncodeError};

    fn enc(c: Cursor) -> Vec<u8> {
        let mut out = Vec::new();
        c.encode(&mut out).unwrap();
        assert_eq!(out.len(), c.encoded_len());
        out
    }

    #[test]
    fn relative_moves_emit_csi_n_letter() {
        assert_eq!(enc(Cursor::Up(1)), b"\x1b[1A");
        assert_eq!(enc(Cursor::Down(2)), b"\x1b[2B");
        assert_eq!(enc(Cursor::Right(10)), b"\x1b[10C");
        assert_eq!(enc(Cursor::Left(255)), b"\x1b[255D");
    }

    #[test]
    fn relative_zero_is_noop() {
        assert_eq!(enc(Cursor::Up(0)), b"");
        assert_eq!(enc(Cursor::Down(0)), b"");
        assert_eq!(enc(Cursor::Left(0)), b"");
        assert_eq!(enc(Cursor::Right(0)), b"");
        assert_eq!(Cursor::Up(0).encoded_len(), 0);
    }

    #[test]
    fn goto_one_indexed() {
        let c = Cursor::goto(1, 1).unwrap();
        assert_eq!(enc(c), b"\x1b[1;1H");
        let c = Cursor::goto(24, 80).unwrap();
        assert_eq!(enc(c), b"\x1b[24;80H");
    }

    #[test]
    fn goto_rejects_zero_row() {
        assert_eq!(
            Cursor::goto(0, 1),
            Err(EncodeError::InvalidCoordinate { field: "row" })
        );
    }

    #[test]
    fn goto_rejects_zero_col() {
        assert_eq!(
            Cursor::goto(5, 0),
            Err(EncodeError::InvalidCoordinate { field: "col" })
        );
    }

    #[test]
    fn save_restore_emit_decsc_decrc() {
        assert_eq!(enc(Cursor::Save), b"\x1b7");
        assert_eq!(enc(Cursor::Restore), b"\x1b8");
    }

    #[test]
    fn relative_max_u16() {
        assert_eq!(enc(Cursor::Up(65_535)), b"\x1b[65535A");
    }
}
