// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Window size tracking.
//!
//! Owns `TIOCGWINSZ` ioctls and the SIGWINCH-driven refresh
//! (see `PLAN_04` §5.5 and subtask 04.7).

/// Snapshot of a terminal's pixel and cell dimensions, as reported
/// by `TIOCGWINSZ` (`struct winsize`).
///
/// `cols` / `rows` are character cells; `pixel_width` /
/// `pixel_height` are pixel dimensions when the terminal reports
/// them and zero otherwise. Defaults to an 80×24 cell grid with
/// unknown pixel dimensions so that `WindowSize::default()` is a
/// reasonable starting point before the first ioctl runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSize {
    /// Width in character cells.
    pub cols: u16,
    /// Height in character cells.
    pub rows: u16,
    /// Width in pixels, or `0` if the terminal does not report it.
    pub pixel_width: u16,
    /// Height in pixels, or `0` if the terminal does not report it.
    pub pixel_height: u16,
}

impl Default for WindowSize {
    fn default() -> Self {
        Self {
            cols: 80,
            rows: 24,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::WindowSize;

    #[test]
    fn default_is_eighty_by_twenty_four() {
        let w = WindowSize::default();
        assert_eq!(w.cols, 80);
        assert_eq!(w.rows, 24);
        assert_eq!(w.pixel_width, 0);
        assert_eq!(w.pixel_height, 0);
    }

    #[test]
    fn window_size_is_copy() {
        const _: fn() = || {
            fn assert_copy<T: Copy>() {}
            assert_copy::<WindowSize>();
        };
    }
}
