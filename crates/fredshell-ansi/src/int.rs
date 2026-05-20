// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Internal integer-to-decimal helpers.
//!
//! Encoders write integer parameters as ASCII digits without
//! allocating. These helpers render `u8` and `u16` into a small
//! on-stack buffer and return the populated slice.
//!
//! The `dec_len_*` helpers are `const fn` so callers in
//! [`crate::Encode::encoded_len`] can compute exact lengths cheaply.

/// On-stack decimal renderer for `u8`. At most 3 ASCII digits.
pub fn itoa3(value: u8, buf: &mut [u8; 3]) -> &[u8] {
    if value >= 100 {
        buf[0] = b'0' + value / 100;
        buf[1] = b'0' + (value / 10) % 10;
        buf[2] = b'0' + value % 10;
        &buf[..3]
    } else if value >= 10 {
        buf[0] = b'0' + value / 10;
        buf[1] = b'0' + value % 10;
        &buf[..2]
    } else {
        buf[0] = b'0' + value;
        &buf[..1]
    }
}

/// Decimal length of `value` as a `u8`.
pub const fn dec_len_u8(value: u8) -> usize {
    if value >= 100 {
        3
    } else if value >= 10 {
        2
    } else {
        1
    }
}

/// On-stack decimal renderer for `u16`. At most 5 ASCII digits.
pub fn itoa5(value: u16, buf: &mut [u8; 5]) -> &[u8] {
    let len = dec_len_u16(value);
    let mut v = value;
    for i in (0..len).rev() {
        buf[i] = b'0' + u8::try_from(v % 10).unwrap_or(0);
        v /= 10;
    }
    &buf[..len]
}

/// Decimal length of `value` as a `u16`.
pub const fn dec_len_u16(value: u16) -> usize {
    if value >= 10_000 {
        5
    } else if value >= 1_000 {
        4
    } else if value >= 100 {
        3
    } else if value >= 10 {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{dec_len_u8, dec_len_u16, itoa3, itoa5};

    #[test]
    fn itoa3_boundaries() {
        let mut b = [0_u8; 3];
        assert_eq!(itoa3(0, &mut b), b"0");
        assert_eq!(itoa3(9, &mut b), b"9");
        assert_eq!(itoa3(10, &mut b), b"10");
        assert_eq!(itoa3(99, &mut b), b"99");
        assert_eq!(itoa3(100, &mut b), b"100");
        assert_eq!(itoa3(255, &mut b), b"255");
    }

    #[test]
    fn dec_len_u8_boundaries() {
        assert_eq!(dec_len_u8(0), 1);
        assert_eq!(dec_len_u8(9), 1);
        assert_eq!(dec_len_u8(10), 2);
        assert_eq!(dec_len_u8(99), 2);
        assert_eq!(dec_len_u8(100), 3);
        assert_eq!(dec_len_u8(255), 3);
    }

    #[test]
    fn itoa5_boundaries() {
        let mut b = [0_u8; 5];
        assert_eq!(itoa5(0, &mut b), b"0");
        assert_eq!(itoa5(7, &mut b), b"7");
        assert_eq!(itoa5(42, &mut b), b"42");
        assert_eq!(itoa5(999, &mut b), b"999");
        assert_eq!(itoa5(1000, &mut b), b"1000");
        assert_eq!(itoa5(9999, &mut b), b"9999");
        assert_eq!(itoa5(10_000, &mut b), b"10000");
        assert_eq!(itoa5(65_535, &mut b), b"65535");
    }

    #[test]
    fn dec_len_u16_boundaries() {
        assert_eq!(dec_len_u16(0), 1);
        assert_eq!(dec_len_u16(9), 1);
        assert_eq!(dec_len_u16(10), 2);
        assert_eq!(dec_len_u16(99), 2);
        assert_eq!(dec_len_u16(100), 3);
        assert_eq!(dec_len_u16(999), 3);
        assert_eq!(dec_len_u16(1000), 4);
        assert_eq!(dec_len_u16(9999), 4);
        assert_eq!(dec_len_u16(10_000), 5);
        assert_eq!(dec_len_u16(65_535), 5);
    }
}
