// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! SGR (Select Graphic Rendition) encoders.
//!
//! A [`Sgr`] value describes a *complete* graphic style. Encoding
//! emits a single `CSI … m` sequence that, applied from the SGR
//! reset baseline, produces that style. The caller is responsible
//! for emitting [`Sgr::RESET`] at the end of styled output if the
//! surrounding context expects an unstyled baseline (`PLAN_03`
//! §4.2).
//!
//! The encoder is zero-allocation: the line editor redraws on every
//! keystroke and any heap traffic in this path is a regression. The
//! parameter integers are written using a small on-stack scratch
//! buffer (see the crate-internal `int` module) sized for the
//! largest value the SGR surface emits (255).

use std::io::{self, Write};

use crate::Encode;
use crate::int::{dec_len_u8, itoa3};

/// A complete SGR graphic style.
///
/// All boolean fields default to `false` and both color slots
/// default to `None`; the all-default value is equivalent to
/// [`Sgr::RESET`].
///
/// The struct intentionally exposes one `bool` per SGR attribute
/// rather than packing them into a `bitflags` type. This shape
/// matches the spec wording, makes the encoder readable, and lets
/// callers spell `Sgr::RESET.with_bold().with_italic()` without
/// importing a flags type.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sgr {
    /// Bold (SGR 1).
    pub bold: bool,
    /// Dim / faint (SGR 2).
    pub dim: bool,
    /// Italic (SGR 3).
    pub italic: bool,
    /// Underline style (SGR 4 family).
    pub underline: Underline,
    /// Reverse video (SGR 7).
    pub reverse: bool,
    /// Strikethrough (SGR 9).
    pub strikethrough: bool,
    /// Foreground color, if any.
    pub fg: Option<Color>,
    /// Background color, if any.
    pub bg: Option<Color>,
}

/// Underline style.
///
/// The shape of [`Underline::Curly`], [`Underline::Dotted`], and
/// [`Underline::Dashed`] are kitty/VTE extensions of SGR 4
/// (`CSI 4 : n m`). [`Underline::Single`] and [`Underline::Double`]
/// are baseline ECMA-48.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Underline {
    /// No underline.
    #[default]
    None,
    /// Single underline (SGR 4).
    Single,
    /// Double underline (SGR 21 / `4:2`).
    Double,
    /// Curly underline (`4:3`, kitty/VTE extension).
    Curly,
    /// Dotted underline (`4:4`, kitty/VTE extension).
    Dotted,
    /// Dashed underline (`4:5`, kitty/VTE extension).
    Dashed,
}

/// Foreground or background color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// 16-color baseline: black.
    Black,
    /// 16-color baseline: red.
    Red,
    /// 16-color baseline: green.
    Green,
    /// 16-color baseline: yellow.
    Yellow,
    /// 16-color baseline: blue.
    Blue,
    /// 16-color baseline: magenta.
    Magenta,
    /// 16-color baseline: cyan.
    Cyan,
    /// 16-color baseline: white.
    White,
    /// Bright 16-color: black.
    BrightBlack,
    /// Bright 16-color: red.
    BrightRed,
    /// Bright 16-color: green.
    BrightGreen,
    /// Bright 16-color: yellow.
    BrightYellow,
    /// Bright 16-color: blue.
    BrightBlue,
    /// Bright 16-color: magenta.
    BrightMagenta,
    /// Bright 16-color: cyan.
    BrightCyan,
    /// Bright 16-color: white.
    BrightWhite,
    /// 256-color indexed (xterm 256).
    Indexed(u8),
    /// 24-bit truecolor.
    Rgb {
        /// Red channel.
        r: u8,
        /// Green channel.
        g: u8,
        /// Blue channel.
        b: u8,
    },
}

impl Sgr {
    /// The SGR reset value (all attributes off, no colors set).
    ///
    /// Encoding this emits the canonical `CSI 0 m` reset.
    pub const RESET: Self = Self {
        bold: false,
        dim: false,
        italic: false,
        underline: Underline::None,
        reverse: false,
        strikethrough: false,
        fg: None,
        bg: None,
    };

    /// Build an [`Sgr`] with only `color` as the foreground.
    #[must_use]
    pub const fn fg(color: Color) -> Self {
        let mut s = Self::RESET;
        s.fg = Some(color);
        s
    }

    /// Build an [`Sgr`] with only `color` as the background.
    #[must_use]
    pub const fn bg(color: Color) -> Self {
        let mut s = Self::RESET;
        s.bg = Some(color);
        s
    }

    /// Builder: enable bold.
    #[must_use]
    pub const fn with_bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Builder: enable dim.
    #[must_use]
    pub const fn with_dim(mut self) -> Self {
        self.dim = true;
        self
    }

    /// Builder: enable italic.
    #[must_use]
    pub const fn with_italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Builder: set the underline style.
    #[must_use]
    pub const fn with_underline(mut self, underline: Underline) -> Self {
        self.underline = underline;
        self
    }

    /// Builder: enable reverse video.
    #[must_use]
    pub const fn with_reverse(mut self) -> Self {
        self.reverse = true;
        self
    }

    /// Builder: enable strikethrough.
    #[must_use]
    pub const fn with_strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    /// Builder: set the foreground color.
    #[must_use]
    pub const fn with_fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    /// Builder: set the background color.
    #[must_use]
    pub const fn with_bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    /// Returns `true` iff this value is the SGR reset baseline.
    #[must_use]
    pub const fn is_reset(&self) -> bool {
        !self.bold
            && !self.dim
            && !self.italic
            && matches!(self.underline, Underline::None)
            && !self.reverse
            && !self.strikethrough
            && self.fg.is_none()
            && self.bg.is_none()
    }
}

/// Returns the underline parameter bytes (without leading `;`) and
/// their length. `Underline::None` returns an empty slice.
const fn underline_param(u: Underline) -> &'static [u8] {
    match u {
        Underline::None => b"",
        Underline::Single => b"4",
        Underline::Double => b"4:2",
        Underline::Curly => b"4:3",
        Underline::Dotted => b"4:4",
        Underline::Dashed => b"4:5",
    }
}

impl Color {
    /// Length in bytes of this color's parameter list, separators
    /// included (e.g. `38;5;231` = 8). The leading `;` between this
    /// parameter list and any preceding parameter is the caller's
    /// responsibility.
    const fn param_len(self, foreground: bool) -> usize {
        match self {
            Self::Black
            | Self::Red
            | Self::Green
            | Self::Yellow
            | Self::Blue
            | Self::Magenta
            | Self::Cyan
            | Self::White
            | Self::BrightBlack
            | Self::BrightRed
            | Self::BrightGreen
            | Self::BrightYellow
            | Self::BrightBlue
            | Self::BrightMagenta
            | Self::BrightCyan => 2,
            Self::BrightWhite => {
                if foreground {
                    2 // 97
                } else {
                    3 // 107
                }
            }
            Self::Indexed(n) => 2 + 1 + 1 + 1 + dec_len_u8(n),
            Self::Rgb { r, g, b } => {
                2 + 1 + 1 + 1 + dec_len_u8(r) + 1 + dec_len_u8(g) + 1 + dec_len_u8(b)
            }
        }
    }

    fn write_params<W: Write>(self, w: &mut W, foreground: bool) -> io::Result<()> {
        let mut buf = [0_u8; 3];
        match self {
            Self::Black => w.write_all(if foreground { b"30" } else { b"40" }),
            Self::Red => w.write_all(if foreground { b"31" } else { b"41" }),
            Self::Green => w.write_all(if foreground { b"32" } else { b"42" }),
            Self::Yellow => w.write_all(if foreground { b"33" } else { b"43" }),
            Self::Blue => w.write_all(if foreground { b"34" } else { b"44" }),
            Self::Magenta => w.write_all(if foreground { b"35" } else { b"45" }),
            Self::Cyan => w.write_all(if foreground { b"36" } else { b"46" }),
            Self::White => w.write_all(if foreground { b"37" } else { b"47" }),
            Self::BrightBlack => w.write_all(if foreground { b"90" } else { b"100" }),
            Self::BrightRed => w.write_all(if foreground { b"91" } else { b"101" }),
            Self::BrightGreen => w.write_all(if foreground { b"92" } else { b"102" }),
            Self::BrightYellow => w.write_all(if foreground { b"93" } else { b"103" }),
            Self::BrightBlue => w.write_all(if foreground { b"94" } else { b"104" }),
            Self::BrightMagenta => w.write_all(if foreground { b"95" } else { b"105" }),
            Self::BrightCyan => w.write_all(if foreground { b"96" } else { b"106" }),
            Self::BrightWhite => w.write_all(if foreground { b"97" } else { b"107" }),
            Self::Indexed(n) => {
                w.write_all(if foreground { b"38;5;" } else { b"48;5;" })?;
                w.write_all(itoa3(n, &mut buf))
            }
            Self::Rgb { r, g, b } => {
                w.write_all(if foreground { b"38;2;" } else { b"48;2;" })?;
                w.write_all(itoa3(r, &mut buf))?;
                w.write_all(b";")?;
                w.write_all(itoa3(g, &mut buf))?;
                w.write_all(b";")?;
                w.write_all(itoa3(b, &mut buf))
            }
        }
    }
}

impl Encode for Sgr {
    fn encode<W: Write>(&self, w: &mut W) -> io::Result<()> {
        // The reset baseline emits "CSI 0 m" — explicit, single
        // parameter, distinct from "CSI m" (which terminals also
        // accept as a reset but is less greppable in traces).
        if self.is_reset() {
            return w.write_all(b"\x1b[0m");
        }

        w.write_all(b"\x1b[")?;
        let mut first = true;
        let sep = |w: &mut W, first: &mut bool| -> io::Result<()> {
            if *first {
                *first = false;
                Ok(())
            } else {
                w.write_all(b";")
            }
        };

        if self.bold {
            sep(w, &mut first)?;
            w.write_all(b"1")?;
        }
        if self.dim {
            sep(w, &mut first)?;
            w.write_all(b"2")?;
        }
        if self.italic {
            sep(w, &mut first)?;
            w.write_all(b"3")?;
        }
        let u = underline_param(self.underline);
        if !u.is_empty() {
            sep(w, &mut first)?;
            w.write_all(u)?;
        }
        if self.reverse {
            sep(w, &mut first)?;
            w.write_all(b"7")?;
        }
        if self.strikethrough {
            sep(w, &mut first)?;
            w.write_all(b"9")?;
        }
        if let Some(fg) = self.fg {
            sep(w, &mut first)?;
            fg.write_params(w, true)?;
        }
        if let Some(bg) = self.bg {
            sep(w, &mut first)?;
            bg.write_params(w, false)?;
        }
        w.write_all(b"m")
    }

    fn encoded_len(&self) -> usize {
        if self.is_reset() {
            // "ESC [ 0 m" = 4 bytes.
            return 4;
        }
        // "ESC [" + params + "m" = 3 + params.
        let mut params = 0_usize;
        let mut count = 0_usize; // number of parameters emitted

        let mut add = |bytes: usize| {
            if count > 0 {
                params += 1; // separator
            }
            params += bytes;
            count += 1;
        };

        if self.bold {
            add(1);
        }
        if self.dim {
            add(1);
        }
        if self.italic {
            add(1);
        }
        let u = underline_param(self.underline);
        if !u.is_empty() {
            add(u.len());
        }
        if self.reverse {
            add(1);
        }
        if self.strikethrough {
            add(1);
        }
        if let Some(fg) = self.fg {
            add(fg.param_len(true));
        }
        if let Some(bg) = self.bg {
            add(bg.param_len(false));
        }
        3 + params
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{Color, Sgr, Underline};
    use crate::Encode;

    fn encode_to_vec<E: Encode>(value: &E) -> Vec<u8> {
        let mut out = Vec::new();
        value.encode(&mut out).unwrap();
        assert_eq!(
            out.len(),
            value.encoded_len(),
            "encoded_len mismatch: wrote {} bytes, encoded_len reported {}",
            out.len(),
            value.encoded_len(),
        );
        out
    }

    #[test]
    fn reset_encodes_csi_zero_m() {
        assert_eq!(encode_to_vec(&Sgr::RESET), b"\x1b[0m");
    }

    #[test]
    fn default_is_reset() {
        assert!(Sgr::default().is_reset());
        assert_eq!(encode_to_vec(&Sgr::default()), b"\x1b[0m");
    }

    #[test]
    fn bold_only() {
        let s = Sgr::RESET.with_bold();
        assert_eq!(encode_to_vec(&s), b"\x1b[1m");
    }

    #[test]
    fn bold_italic_underline_single() {
        let s = Sgr::RESET
            .with_bold()
            .with_italic()
            .with_underline(Underline::Single);
        assert_eq!(encode_to_vec(&s), b"\x1b[1;3;4m");
    }

    #[test]
    fn underline_curly_dotted_dashed_double() {
        for (u, bytes) in [
            (Underline::Single, b"\x1b[4m" as &[u8]),
            (Underline::Double, b"\x1b[4:2m"),
            (Underline::Curly, b"\x1b[4:3m"),
            (Underline::Dotted, b"\x1b[4:4m"),
            (Underline::Dashed, b"\x1b[4:5m"),
        ] {
            let s = Sgr::RESET.with_underline(u);
            assert_eq!(encode_to_vec(&s), bytes, "underline={u:?}");
        }
    }

    #[test]
    fn basic_fg_colors() {
        for (c, n) in [
            (Color::Black, b"30"),
            (Color::Red, b"31"),
            (Color::Green, b"32"),
            (Color::Yellow, b"33"),
            (Color::Blue, b"34"),
            (Color::Magenta, b"35"),
            (Color::Cyan, b"36"),
            (Color::White, b"37"),
        ] {
            let s = Sgr::fg(c);
            let mut want = b"\x1b[".to_vec();
            want.extend_from_slice(n);
            want.push(b'm');
            assert_eq!(encode_to_vec(&s), want);
        }
    }

    #[test]
    fn basic_bg_colors() {
        let s = Sgr::bg(Color::Red);
        assert_eq!(encode_to_vec(&s), b"\x1b[41m");
    }

    #[test]
    fn bright_fg_white_is_two_digits() {
        let s = Sgr::fg(Color::BrightWhite);
        assert_eq!(encode_to_vec(&s), b"\x1b[97m");
    }

    #[test]
    fn bright_bg_white_is_three_digits() {
        let s = Sgr::bg(Color::BrightWhite);
        assert_eq!(encode_to_vec(&s), b"\x1b[107m");
    }

    #[test]
    fn indexed_fg() {
        let s = Sgr::fg(Color::Indexed(231));
        assert_eq!(encode_to_vec(&s), b"\x1b[38;5;231m");
    }

    #[test]
    fn indexed_bg_single_digit() {
        let s = Sgr::bg(Color::Indexed(7));
        assert_eq!(encode_to_vec(&s), b"\x1b[48;5;7m");
    }

    #[test]
    fn rgb_fg() {
        let s = Sgr::fg(Color::Rgb {
            r: 1,
            g: 22,
            b: 255,
        });
        assert_eq!(encode_to_vec(&s), b"\x1b[38;2;1;22;255m");
    }

    #[test]
    fn rgb_bg() {
        let s = Sgr::bg(Color::Rgb { r: 0, g: 0, b: 0 });
        assert_eq!(encode_to_vec(&s), b"\x1b[48;2;0;0;0m");
    }

    #[test]
    fn full_style_truecolor_fg_and_bg() {
        let s = Sgr::RESET
            .with_bold()
            .with_dim()
            .with_italic()
            .with_underline(Underline::Curly)
            .with_reverse()
            .with_strikethrough()
            .with_fg(Color::Rgb {
                r: 10,
                g: 20,
                b: 30,
            })
            .with_bg(Color::Indexed(255));
        assert_eq!(
            encode_to_vec(&s),
            b"\x1b[1;2;3;4:3;7;9;38;2;10;20;30;48;5;255m"
        );
    }

    #[test]
    fn parameter_order_is_stable() {
        // Order: bold, dim, italic, underline, reverse, strike, fg,
        // bg. This order is part of the public byte-level contract;
        // changing it will break any downstream golden tests.
        let s = Sgr {
            bold: true,
            dim: true,
            italic: true,
            underline: Underline::Single,
            reverse: true,
            strikethrough: true,
            fg: Some(Color::Red),
            bg: Some(Color::Blue),
        };
        assert_eq!(encode_to_vec(&s), b"\x1b[1;2;3;4;7;9;31;44m");
    }
}
