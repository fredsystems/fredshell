// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Termios state and the raw-mode RAII guard.
//!
//! Owns the `tcgetattr` / `tcsetattr` pair that enters raw mode and
//! the [`RawModeGuard`] that restores the saved cooked-mode termios
//! when dropped (see `PLAN_04` §3.3).
//!
//! ## Raw-mode semantics
//!
//! `enter` captures the current termios with `tcgetattr`, applies the
//! transformation `cfmakeraw` performs (clearing canonical mode,
//! echo, signal generation, input processing, and output processing,
//! plus setting `VMIN=1` / `VTIME=0` for one-byte-at-a-time reads),
//! then writes it back with `tcsetattr(TCSAFLUSH)` so any pending
//! input is discarded before the mode change takes effect. The saved
//! pre-raw termios is stashed in the returned [`RawModeGuard`].
//!
//! ## RAII restoration
//!
//! On drop, the guard calls `tcsetattr(TCSAFLUSH)` with the saved
//! termios. `tcsetattr` is async-signal-safe and cannot block, so
//! invoking it during unwinding is safe. The result is intentionally
//! ignored: if restoration fails the process is most likely already
//! exiting in a degraded state, and there is no useful recovery from
//! drop. The parent terminal emulator restores its own termios on
//! PTY teardown for the SIGKILL / process-death case (`PLAN_04` §3.3).

use std::io;
use std::os::fd::RawFd;

use libc::{TCSAFLUSH, cfmakeraw, tcgetattr, tcsetattr, termios};

/// Capture the current termios for `fd` and switch the terminal into
/// raw mode, returning a guard that restores cooked mode on drop.
///
/// # Errors
///
/// Returns the underlying `io::Error` if `tcgetattr` or `tcsetattr`
/// fails. The terminal is unchanged in that case (we only call
/// `tcsetattr` after `tcgetattr` succeeded, and a failed `tcsetattr`
/// is a no-op by POSIX).
pub fn enter(fd: RawFd) -> io::Result<RawModeGuard> {
    // SAFETY: zeroed termios is valid POD; tcgetattr fully overwrites
    // it on success.
    let mut current: termios = unsafe { std::mem::zeroed() };
    // SAFETY: `fd` is a tty fd owned by the caller; `current` is a
    // valid out-pointer.
    let rc = unsafe { tcgetattr(fd, &raw mut current) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }

    let saved = current;
    // SAFETY: in-place mutation of a local POD.
    unsafe { cfmakeraw(&raw mut current) };

    // SAFETY: same arguments-validity rationale as tcgetattr.
    let rc = unsafe { tcsetattr(fd, TCSAFLUSH, &raw const current) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(RawModeGuard { fd, saved })
}

/// RAII guard that restores cooked-mode termios on drop.
///
/// Constructed by [`enter`] (invoked from
/// [`super::TerminalSession::enter_raw_mode`]). The guard owns a
/// copy of the termios captured before raw mode was applied and the
/// fd it was applied to; on drop it calls `tcsetattr(TCSAFLUSH)` to
/// restore.
///
/// The struct intentionally does not implement `Clone` or `Copy`:
/// double-drop would call `tcsetattr` twice, which is harmless on
/// the second call but masks programmer errors where the guard
/// escapes its intended scope.
pub struct RawModeGuard {
    fd: RawFd,
    saved: termios,
}

impl std::fmt::Debug for RawModeGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // libc::termios contains platform-dependent unions that do
        // not implement Debug; show only the fd.
        f.debug_struct("RawModeGuard")
            .field("fd", &self.fd)
            .finish_non_exhaustive()
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        // SAFETY: `self.fd` was a valid tty fd at construction; if
        // the caller closed it before dropping us, tcsetattr returns
        // EBADF and we discard it. tcsetattr is async-signal-safe
        // so this is safe even during unwinding.
        unsafe {
            let _ = tcsetattr(self.fd, TCSAFLUSH, &raw const self.saved);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::enter;
    use libc::{ECHO, ICANON, tcgetattr, termios};
    use std::os::fd::{AsRawFd, OwnedFd};

    /// Open a fresh master/slave PTY pair via `openpty`, returning
    /// the slave end as an `OwnedFd`. The master is leaked because we
    /// never read from it in these tests — the slave is what we
    /// apply termios to.
    fn open_slave_pty() -> Option<OwnedFd> {
        let mut master: libc::c_int = -1;
        let mut slave: libc::c_int = -1;
        // SAFETY: openpty writes two valid fds on success; all
        // other pointer arguments are null which is permitted.
        // `null_mut` is used for the termios / winsize templates so
        // the call type-checks on both Linux (`*const termios`) and
        // macOS/BSD (`*mut termios`): a `*mut T` null coerces to a
        // `*const T` parameter, but not vice versa.
        let rc = unsafe {
            libc::openpty(
                &raw mut master,
                &raw mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if rc != 0 {
            return None;
        }
        // Leak master — keeping it alive ensures the slave fd is
        // not torn down underneath us. We close it manually at the
        // end of the test process.
        std::mem::forget(unsafe { OwnedFd::from_raw_fd(master) });
        Some(unsafe { OwnedFd::from_raw_fd(slave) })
    }

    use std::os::fd::FromRawFd;

    fn read_termios(fd: i32) -> termios {
        // SAFETY: fd is a valid tty.
        let mut t: termios = unsafe { std::mem::zeroed() };
        let rc = unsafe { tcgetattr(fd, &raw mut t) };
        assert_eq!(rc, 0);
        t
    }

    #[test]
    fn enter_clears_icanon_and_echo() {
        let Some(slave) = open_slave_pty() else {
            // openpty unavailable (some sandboxes) — skip rather
            // than fail.
            return;
        };
        let fd = slave.as_raw_fd();
        let before = read_termios(fd);
        assert!(
            before.c_lflag & (ICANON | ECHO) != 0,
            "expected cooked-mode termios before entering raw"
        );

        let guard = enter(fd).unwrap();
        let raw = read_termios(fd);
        assert_eq!(
            raw.c_lflag & ICANON,
            0,
            "ICANON should be cleared in raw mode"
        );
        assert_eq!(raw.c_lflag & ECHO, 0, "ECHO should be cleared in raw mode");

        drop(guard);
        let restored = read_termios(fd);
        // After restoration the lflag should match the pre-raw
        // value bit-for-bit.
        assert_eq!(restored.c_lflag, before.c_lflag);
    }

    #[test]
    fn drop_restores_termios_on_panic_unwind() {
        let Some(slave) = open_slave_pty() else {
            return;
        };
        let fd = slave.as_raw_fd();
        let before = read_termios(fd);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = enter(fd).unwrap();
            panic!("simulated panic inside raw mode");
        }));
        assert!(result.is_err());

        let restored = read_termios(fd);
        assert_eq!(restored.c_lflag, before.c_lflag);
        assert_eq!(restored.c_iflag, before.c_iflag);
        assert_eq!(restored.c_oflag, before.c_oflag);
    }

    #[test]
    fn enter_on_non_tty_returns_error() {
        // /dev/null is not a tty; tcgetattr should fail with ENOTTY.
        // SAFETY: open(2) with a literal C string and no flags.
        let fd = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDWR) };
        assert!(fd >= 0);
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        let err = enter(owned.as_raw_fd()).unwrap_err();
        assert_eq!(err.raw_os_error(), Some(libc::ENOTTY));
    }

    #[test]
    fn enter_twice_on_same_fd_both_succeed() {
        // The guard does not track per-fd state; a second `enter`
        // simply captures the (now-raw) termios as its 'saved'
        // value. This is by design — the public API enforces
        // exactly-once raw-mode entry via TerminalSession, not via
        // the low-level enter helper.
        let Some(slave) = open_slave_pty() else {
            return;
        };
        let fd = slave.as_raw_fd();
        let g1 = enter(fd).unwrap();
        let g2 = enter(fd).unwrap();
        drop(g2);
        drop(g1);
    }
}
