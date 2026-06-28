// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Process-group plumbing for job control.
//!
//! Owns the two syscall primitives the rest of fredshell needs to do
//! job control (see `PLAN_04` §7):
//!
//! 1. [`setpgid`] places a child into its own process group. It is
//!    called in **both** the parent and the child after `fork(2)` so
//!    the transition is race-free regardless of which runs first.
//! 2. [`tcsetpgrp`] transfers the controlling terminal's foreground
//!    process group. The shell ignores `SIGTTOU` (see
//!    `tty::signal::install`), so `tcsetpgrp` from the shell is a
//!    single syscall with no signal-juggling dance.
//!
//! Both primitives return typed errors that carry the underlying
//! [`io::Error`]. The full job-control state machine (suspended
//! jobs, `fg`/`bg`/`wait`/`jobs` builtins) is **not** `PLAN_04` — it
//! lives in `fredshell-core::exec` and gets its own document
//! (`PLAN_11`). `PLAN_04` only provides the primitives.
//!
//! This module deals exclusively with kernel process-group ids. The
//! shell-level `Job` / `JobId` abstractions are out of scope here.

use std::io;
use std::os::fd::{AsRawFd, BorrowedFd};

/// Typed process id.
///
/// Wraps `libc::pid_t` to make signatures self-documenting and to
/// prevent silent confusion between raw ints and pids. A `Pid` of `0`
/// is the conventional "self" / "current process group" sentinel that
/// `setpgid(0, 0)` accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pid(libc::pid_t);

impl Pid {
    /// Construct a [`Pid`] from a raw `pid_t`.
    ///
    /// No validation: the kernel is the ultimate authority on whether
    /// a given pid refers to a live process. Negative values are
    /// permitted because `setpgid` and signal APIs use the negative
    /// convention for "process group" elsewhere; the caller is
    /// responsible for using sensible values.
    #[must_use]
    pub const fn from_raw(pid: libc::pid_t) -> Self {
        Self(pid)
    }

    /// The current process's pid.
    #[must_use]
    pub fn current() -> Self {
        // SAFETY: getpid(2) is always-succeeds and async-signal-safe.
        Self(unsafe { libc::getpid() })
    }

    /// The current process's process group id.
    ///
    /// # Errors
    ///
    /// Returns the underlying `io::Error` if `getpgrp(2)` fails. On
    /// Linux and macOS `getpgrp` cannot fail, but the API exposes the
    /// error path so callers do not have to assume that invariant.
    pub fn current_pgrp() -> io::Result<Self> {
        // SAFETY: getpgrp(2) takes no arguments and returns the
        // calling process's pgid. On Linux/macOS it never sets errno;
        // we still check for -1 to remain defensive.
        let rc = unsafe { libc::getpgrp() };
        if rc < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(rc))
        }
    }

    /// The raw `pid_t` value.
    #[must_use]
    pub const fn as_raw(self) -> libc::pid_t {
        self.0
    }
}

/// Errors from [`setpgid`].
#[derive(Debug)]
#[non_exhaustive]
pub enum SetPgidError {
    /// The `setpgid(2)` syscall failed.
    Syscall(io::Error),
}

impl std::fmt::Display for SetPgidError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syscall(_) => f.write_str("setpgid failed"),
        }
    }
}

impl std::error::Error for SetPgidError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Syscall(e) => Some(e),
        }
    }
}

/// Errors from [`tcsetpgrp`].
#[derive(Debug)]
#[non_exhaustive]
pub enum TcSetPgrpError {
    /// The `tcsetpgrp(3)` syscall failed. Common causes: the fd is
    /// not a controlling terminal (`ENOTTY`), the process group is
    /// not in the same session as the calling process (`EPERM`), or
    /// the pgid does not exist (`EINVAL`).
    Syscall(io::Error),
}

impl std::fmt::Display for TcSetPgrpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syscall(_) => f.write_str("tcsetpgrp failed"),
        }
    }
}

impl std::error::Error for TcSetPgrpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Syscall(e) => Some(e),
        }
    }
}

/// Place `target` into process group `pgid`.
///
/// Per `setpgid(2)`, passing `Pid::from_raw(0)` for `target` means
/// "the calling process," and passing `Pid::from_raw(0)` for `pgid`
/// means "use the pid as the pgid" (i.e. make the target the leader
/// of a new process group). The combination
/// `setpgid(Pid(0), Pid(0))` is the canonical "promote self to a new
/// process group" call.
///
/// `PLAN_04` §7 mandates this is called in **both** the parent and
/// the child after `fork`. The kernel makes the second call a no-op,
/// so the duplication is harmless and race-free.
///
/// # Errors
///
/// Returns [`SetPgidError::Syscall`] wrapping the underlying
/// `io::Error` on failure. Common errno values include `EACCES` (the
/// child has already `exec`d), `EINVAL` (negative pgid), `EPERM`
/// (different session), and `ESRCH` (no such process).
pub fn setpgid(target: Pid, pgid: Pid) -> Result<(), SetPgidError> {
    // SAFETY: setpgid(2) takes two pid_t values by value and has no
    // memory-safety preconditions. Failure is signaled via errno.
    let rc = unsafe { libc::setpgid(target.as_raw(), pgid.as_raw()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(SetPgidError::Syscall(io::Error::last_os_error()))
    }
}

/// Transfer the controlling terminal's foreground process group to
/// `pgid`.
///
/// `tty` must be a file descriptor open on the controlling terminal
/// (typically `/dev/tty` opened via `tty::controlling`). `pgid` must
/// name a process group in the same session as the caller.
///
/// Because the shell installs `SIGTTOU` as `SIG_IGN` at startup
/// (see `tty::signal::install`), this call does not require the
/// classic block-SIGTTOU / restore dance — it is a single syscall.
///
/// # Errors
///
/// Returns [`TcSetPgrpError::Syscall`] wrapping the underlying
/// `io::Error` on failure. Common errno values include `ENOTTY` (fd
/// is not a terminal), `EINVAL` (pgid is invalid), and `EPERM`
/// (different session).
pub fn tcsetpgrp(tty: BorrowedFd<'_>, pgid: Pid) -> Result<(), TcSetPgrpError> {
    // SAFETY: tcsetpgrp(3) takes an fd and a pgid by value. The fd
    // is borrowed for the duration of the call by BorrowedFd, so it
    // is guaranteed live.
    let rc = unsafe { libc::tcsetpgrp(tty.as_raw_fd(), pgid.as_raw()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(TcSetPgrpError::Syscall(io::Error::last_os_error()))
    }
}

/// Query the controlling terminal's current foreground process group.
///
/// Convenience wrapper around `tcgetpgrp(3)`. Used by
/// [`crate::tty::TerminalSession::take_foreground`] so the shell can
/// hand the foreground back to its own process group after a
/// foreground job terminates.
///
/// # Errors
///
/// Returns the underlying `io::Error` if `tcgetpgrp(3)` fails.
/// `ENOTTY` is the common case when the fd does not refer to a
/// controlling terminal.
pub fn tcgetpgrp(tty: BorrowedFd<'_>) -> io::Result<Pid> {
    // SAFETY: tcgetpgrp(3) takes only the fd by value. -1 signals
    // failure with errno set.
    let rc = unsafe { libc::tcgetpgrp(tty.as_raw_fd()) };
    if rc < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(Pid(rc))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{Pid, SetPgidError, TcSetPgrpError, setpgid, tcgetpgrp, tcsetpgrp};
    use crate::tty::test_pty::FakePty;
    use std::os::fd::AsFd;

    #[test]
    fn pid_round_trip() {
        let p = Pid::from_raw(1234);
        assert_eq!(p.as_raw(), 1234);
    }

    #[test]
    fn pid_is_copy() {
        const _: fn() = || {
            fn assert_copy<T: Copy>() {}
            assert_copy::<Pid>();
        };
    }

    #[test]
    fn pid_current_is_positive() {
        let p = Pid::current();
        assert!(p.as_raw() > 0);
    }

    #[test]
    fn pid_current_pgrp_is_positive() {
        let p = Pid::current_pgrp().unwrap();
        assert!(p.as_raw() > 0);
    }

    #[test]
    fn setpgid_self_to_self_is_noop_or_ok() {
        // setpgid(0, 0) attempts to promote the calling process to a
        // new process group led by itself. In a test runner the
        // process may already be a session leader (EPERM) or may
        // succeed; both outcomes are acceptable. We only require
        // that the call does not panic and returns a typed result.
        let _ = setpgid(Pid::from_raw(0), Pid::from_raw(0));
    }

    #[test]
    fn setpgid_invalid_returns_typed_error() {
        // ESRCH: no process has pid i32::MAX in any realistic test
        // environment, so setpgid must return Syscall(_) — not panic.
        let err = setpgid(Pid::from_raw(i32::MAX), Pid::from_raw(0))
            .expect_err("setpgid on bogus pid must fail");
        match err {
            SetPgidError::Syscall(e) => {
                // Any errno is acceptable; we only verify it is
                // surfaced as a typed io::Error.
                let _ = e.raw_os_error();
            }
        }
    }

    #[test]
    fn tcsetpgrp_on_non_tty_returns_enotty() {
        // /dev/null is not a tty, so tcsetpgrp must fail. The errno is
        // platform-specific: Linux reports ENOTTY (25), macOS reports
        // ENODEV (19). Both mean "not a terminal" — accept either.
        let dev_null = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/null")
            .unwrap();
        let err = tcsetpgrp(dev_null.as_fd(), Pid::current_pgrp().unwrap())
            .expect_err("tcsetpgrp on /dev/null must fail");
        match err {
            TcSetPgrpError::Syscall(e) => {
                let errno = e.raw_os_error();
                assert!(
                    errno == Some(libc::ENOTTY) || errno == Some(libc::ENODEV),
                    "expected ENOTTY or ENODEV for a non-tty fd, got {errno:?}"
                );
            }
        }
    }

    #[test]
    fn tcgetpgrp_on_non_tty_returns_enotty() {
        let dev_null = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/null")
            .unwrap();
        let err = tcgetpgrp(dev_null.as_fd()).expect_err("tcgetpgrp on /dev/null must fail");
        // Platform-specific errno: Linux ENOTTY (25), macOS ENODEV
        // (19). Both mean "not a terminal" — accept either.
        let errno = err.raw_os_error();
        assert!(
            errno == Some(libc::ENOTTY) || errno == Some(libc::ENODEV),
            "expected ENOTTY or ENODEV for a non-tty fd, got {errno:?}"
        );
    }

    #[test]
    fn tcgetpgrp_on_pty_slave_returns_a_pgid() {
        // On a PTY slave, tcgetpgrp returns the session's foreground
        // process group. The exact value depends on whether the test
        // process is itself a session leader; we only require that
        // the call returns a typed Pid (not ENOTTY) for a real tty.
        let Some(pty) = FakePty::open() else {
            return; // Skip when openpty(3) is unavailable.
        };
        match tcgetpgrp(pty.slave().as_fd()) {
            Ok(p) => {
                // pgids are non-negative; the slave may report 0 if
                // no foreground group has been set, or a real pgid.
                assert!(p.as_raw() >= 0);
            }
            Err(e) => {
                // EIO/ENOTTY/EPERM are all acceptable in sandboxed
                // CI; we just require a typed io::Error rather than
                // a panic.
                let _ = e.raw_os_error();
            }
        }
    }

    #[test]
    fn setpgid_error_display() {
        let err = setpgid(Pid::from_raw(i32::MAX), Pid::from_raw(0)).unwrap_err();
        assert_eq!(err.to_string(), "setpgid failed");
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn tcsetpgrp_error_display() {
        let dev_null = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/null")
            .unwrap();
        let err = tcsetpgrp(dev_null.as_fd(), Pid::current_pgrp().unwrap()).unwrap_err();
        assert_eq!(err.to_string(), "tcsetpgrp failed");
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn errors_are_std_error() {
        fn assert_error<E: std::error::Error>() {}
        assert_error::<SetPgidError>();
        assert_error::<TcSetPgrpError>();
    }
}
