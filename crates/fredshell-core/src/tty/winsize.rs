// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Window size tracking.
//!
//! Owns `TIOCGWINSZ` ioctls and the SIGWINCH-driven refresh
//! (see `PLAN_04` §5.5, §6, and subtask 04.7).
//!
//! ## Ioctl shape
//!
//! `TIOCGWINSZ` fills a `struct winsize { ws_row, ws_col, ws_xpixel,
//! ws_ypixel }` on a tty fd. We translate it into a typed
//! [`WindowSize`] snapshot. The ioctl itself is fast (a few
//! microseconds) and is safe to call from the SIGWINCH wakeup path
//! in the main loop — it is **not** called from the signal handler
//! (which is restricted to async-signal-safe primitives, see
//! [`super::signal`]).
//!
//! ## When [`query`] is called
//!
//! - Once at [`super::TerminalSession::open`] time to populate the
//!   initial snapshot.
//! - Each time the REPL observes a [`super::Signal::WinCh`] wakeup
//!   from [`super::TerminalSession::wait`] and calls
//!   [`super::TerminalSession::refresh_window_size`].
//!
//! ## Failure handling
//!
//! `TIOCGWINSZ` can fail on fds that are not ttys (e.g., a session
//! constructed against a pipe in tests). [`query`] surfaces the
//! underlying `io::Error`; [`super::TerminalSession::open`] treats
//! failure as "fall back to the 80×24 default" rather than aborting,
//! so the shell remains usable even if the initial ioctl is
//! unavailable.

use std::io;
use std::os::fd::{AsRawFd, BorrowedFd};

use libc::{TIOCGWINSZ, ioctl, winsize};

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

/// Query the kernel for the current window size of the tty bound to
/// `fd`.
///
/// # Errors
///
/// Returns the underlying `io::Error` from `ioctl(2)` if the call
/// fails. The most common failure mode is `ENOTTY` when `fd` is not
/// a terminal; callers (notably [`super::TerminalSession::open`])
/// typically translate that into a fallback to
/// [`WindowSize::default`].
pub fn query(fd: BorrowedFd<'_>) -> io::Result<WindowSize> {
    // SAFETY: `winsize` is POD; we initialize it via zeroed and then
    // fully overwrite it on success.
    let mut ws: winsize = unsafe { std::mem::zeroed() };
    // SAFETY: `fd` is a valid borrowed fd; `&raw mut ws` is a valid
    // out-pointer for the `TIOCGWINSZ` payload.
    let rc = unsafe { ioctl(fd.as_raw_fd(), TIOCGWINSZ, &raw mut ws) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(WindowSize {
        cols: ws.ws_col,
        rows: ws.ws_row,
        pixel_width: ws.ws_xpixel,
        pixel_height: ws.ws_ypixel,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::super::test_pty::FakePty;
    use super::{WindowSize, query};
    use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};

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

    #[test]
    fn query_succeeds_on_pty_slave() {
        let Some(pty) = FakePty::open() else {
            return;
        };
        // openpty defaults the slave's winsize to all-zeros unless
        // a template is passed. Either zero or some kernel-default
        // is acceptable here — we only assert the ioctl succeeded
        // and returned valid u16 values (i.e., did not error).
        let ws = query(pty.slave().as_fd()).unwrap();
        // Defaults vary across Linux/macOS; just make sure the call
        // round-trips.
        let _ = (ws.cols, ws.rows, ws.pixel_width, ws.pixel_height);
    }

    #[test]
    fn query_reflects_kernel_set_size() {
        let Some(pty) = FakePty::open() else {
            return;
        };
        // Set a known size on the slave via TIOCSWINSZ from the
        // master side, then query.
        let ws_in = libc::winsize {
            ws_row: 40,
            ws_col: 132,
            ws_xpixel: 1320,
            ws_ypixel: 800,
        };
        // SAFETY: master is a valid fd; ws_in is a valid POD.
        let rc =
            unsafe { libc::ioctl(pty.master().as_raw_fd(), libc::TIOCSWINSZ, &raw const ws_in) };
        assert_eq!(rc, 0);

        let got = query(pty.slave().as_fd()).unwrap();
        assert_eq!(got.cols, 132);
        assert_eq!(got.rows, 40);
        assert_eq!(got.pixel_width, 1320);
        assert_eq!(got.pixel_height, 800);
    }

    #[test]
    fn query_on_non_tty_returns_enotty() {
        // /dev/null is not a tty.
        // SAFETY: open(2) with a literal C string and no flags.
        let fd = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDWR) };
        assert!(fd >= 0);
        // SAFETY: open(2) returned a valid owned fd.
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        let err = query(owned.as_fd()).unwrap_err();
        assert_eq!(err.raw_os_error(), Some(libc::ENOTTY));
    }
}
