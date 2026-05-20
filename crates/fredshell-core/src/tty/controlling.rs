// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Controlling-terminal acquisition.
//!
//! Owns the `/dev/tty` open dance and `isatty` checks (see `PLAN_04`
//! §2). The shell opens `/dev/tty` rather than relying on `stdin`
//! because any of fd 0/1/2 may be redirected to a file or pipe while
//! the shell still needs to talk to the user's terminal for prompts,
//! keystroke input, and termios queries.

use std::ffi::CStr;
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};

/// Path passed to `open(2)` to acquire the controlling terminal.
///
/// Exposed as a constant so tests can reference the same string and
/// future platforms (which never differ today: Linux and macOS both
/// use `/dev/tty`) have a single point of change.
const DEV_TTY: &CStr = c"/dev/tty";

/// Open `/dev/tty` for read/write with `O_NOCTTY` and `O_CLOEXEC`.
///
/// `O_NOCTTY` is required: we are acquiring a handle to the existing
/// controlling terminal, not requesting one. `O_CLOEXEC` keeps the fd
/// from leaking into spawned children (children inherit fd 0/1/2 as
/// usual via the normal stdio redirection done at spawn time).
///
/// # Errors
///
/// Returns [`AcquireError::NoControllingTerminal`] if the process has
/// no controlling terminal (kernel reports `ENXIO` or `ENODEV`).
/// Returns [`AcquireError::Open`] wrapping the underlying
/// [`io::Error`] for any other failure (permission denied, too many
/// open files, etc.).
pub fn open_controlling_tty() -> Result<OwnedFd, AcquireError> {
    // SAFETY: `DEV_TTY` is a static, null-terminated C string. `open`
    // returns either a non-negative fd or `-1` with `errno` set; we
    // convert both outcomes into safe Rust values before returning.
    let raw = unsafe {
        libc::open(
            DEV_TTY.as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
        )
    };

    if raw < 0 {
        let err = io::Error::last_os_error();
        return Err(classify_open_error(err));
    }

    // SAFETY: `open` returned a non-negative file descriptor that
    // this process now owns exclusively. Wrapping it in `OwnedFd`
    // takes ownership and guarantees the fd will be closed when the
    // `OwnedFd` is dropped.
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    Ok(fd)
}

/// Classify an `open(2)` failure into a typed [`AcquireError`].
///
/// `ENXIO` (Linux: "no such device or address") and `ENODEV`
/// (macOS / older kernels: "no such device") both indicate the
/// process has no controlling terminal. `ENOENT` can also surface on
/// systems where `/dev/tty` is missing (containers, chroots); we
/// treat that as "no controlling terminal" as well, since the user-
/// visible outcome is identical.
fn classify_open_error(err: io::Error) -> AcquireError {
    match err.raw_os_error() {
        Some(libc::ENXIO | libc::ENODEV | libc::ENOENT) => AcquireError::NoControllingTerminal,
        _ => AcquireError::Open(err),
    }
}

/// Errors returned by [`open_controlling_tty`].
#[derive(Debug)]
#[non_exhaustive]
pub enum AcquireError {
    /// The process has no controlling terminal. This is the normal
    /// outcome for daemons, container init processes, and CI runners.
    NoControllingTerminal,
    /// `/dev/tty` exists and the process has a controlling terminal,
    /// but the open syscall failed for some other reason
    /// (permission denied, file-descriptor exhaustion, I/O error).
    Open(io::Error),
}

impl std::fmt::Display for AcquireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoControllingTerminal => f.write_str("no controlling terminal available"),
            Self::Open(_) => f.write_str("failed to open /dev/tty"),
        }
    }
}

impl std::error::Error for AcquireError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open(e) => Some(e),
            Self::NoControllingTerminal => None,
        }
    }
}

/// Return `true` if the given fd refers to a terminal.
///
/// Thin wrapper around `isatty(3)`. Callers use this to decide
/// between interactive and script mode based on whether fd 0 / 1 are
/// terminals, independently of whether `/dev/tty` is openable.
#[must_use]
pub fn is_tty(fd: BorrowedFd<'_>) -> bool {
    is_tty_raw(fd.as_raw_fd())
}

/// Return `true` if the given raw fd refers to a terminal.
///
/// Separate from [`is_tty`] so internal callers that already hold a
/// `RawFd` (e.g. inside an unsafe block initializing a session) can
/// query without round-tripping through `BorrowedFd`.
#[must_use]
pub fn is_tty_raw(fd: RawFd) -> bool {
    // SAFETY: `isatty` is documented as safe to call with any
    // integer; it returns 1 if the fd is a tty, 0 otherwise, and
    // sets errno on failure (which we ignore — "not a tty" is the
    // correct interpretation of any failure).
    unsafe { libc::isatty(fd) == 1 }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{AcquireError, DEV_TTY, classify_open_error, is_tty_raw, open_controlling_tty};
    use std::io;

    #[test]
    fn dev_tty_path_is_canonical() {
        assert_eq!(DEV_TTY.to_bytes(), b"/dev/tty");
    }

    #[test]
    fn classify_open_error_maps_enxio_to_no_controlling_terminal() {
        let err = io::Error::from_raw_os_error(libc::ENXIO);
        assert!(matches!(
            classify_open_error(err),
            AcquireError::NoControllingTerminal
        ));
    }

    #[test]
    fn classify_open_error_maps_enodev_to_no_controlling_terminal() {
        let err = io::Error::from_raw_os_error(libc::ENODEV);
        assert!(matches!(
            classify_open_error(err),
            AcquireError::NoControllingTerminal
        ));
    }

    #[test]
    fn classify_open_error_maps_enoent_to_no_controlling_terminal() {
        let err = io::Error::from_raw_os_error(libc::ENOENT);
        assert!(matches!(
            classify_open_error(err),
            AcquireError::NoControllingTerminal
        ));
    }

    #[test]
    fn classify_open_error_preserves_other_io_errors() {
        let err = io::Error::from_raw_os_error(libc::EACCES);
        let classified = classify_open_error(err);
        match classified {
            AcquireError::Open(inner) => {
                assert_eq!(inner.raw_os_error(), Some(libc::EACCES));
            }
            AcquireError::NoControllingTerminal => {
                panic!("EACCES should not classify as NoControllingTerminal");
            }
        }
    }

    #[test]
    fn is_tty_raw_returns_false_for_invalid_fd() {
        // -1 is guaranteed never to be a valid fd; isatty(-1) returns
        // 0 with errno = EBADF. We interpret 0 as "not a tty".
        assert!(!is_tty_raw(-1));
    }

    #[test]
    fn is_tty_raw_returns_false_for_pipe() {
        let mut fds: [libc::c_int; 2] = [-1, -1];
        // SAFETY: `pipe` is safe to call with a writable two-element
        // array of c_int. Return value is 0 on success, -1 on error.
        let rc = unsafe { libc::pipe(fds.as_mut_ptr()) };
        assert_eq!(rc, 0, "pipe(2) failed: {}", io::Error::last_os_error());

        assert!(!is_tty_raw(fds[0]));
        assert!(!is_tty_raw(fds[1]));

        // SAFETY: We own both fds; closing them once is correct.
        unsafe {
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
    }

    #[test]
    fn acquire_error_display_messages() {
        assert_eq!(
            AcquireError::NoControllingTerminal.to_string(),
            "no controlling terminal available"
        );
        let opened = AcquireError::Open(io::Error::from_raw_os_error(libc::EACCES));
        assert_eq!(opened.to_string(), "failed to open /dev/tty");
    }

    #[test]
    fn acquire_error_preserves_source() {
        use std::error::Error;
        let opened = AcquireError::Open(io::Error::from_raw_os_error(libc::EACCES));
        assert!(opened.source().is_some());
        assert!(AcquireError::NoControllingTerminal.source().is_none());
    }

    #[test]
    fn open_controlling_tty_either_succeeds_or_reports_no_terminal() {
        // This test runs in two environments:
        //   - CI / cargo test under nextest: usually no controlling
        //     terminal, expect NoControllingTerminal.
        //   - Interactive developer shell: /dev/tty opens cleanly.
        // We accept either outcome but reject Open(io::Error) (which
        // would indicate a genuine bug like a wrong path or missing
        // permissions on a system that should otherwise work).
        match open_controlling_tty() {
            Ok(fd) => {
                // If we got an fd back, isatty must agree.
                use std::os::fd::AsFd;
                assert!(super::is_tty(fd.as_fd()));
            }
            Err(AcquireError::NoControllingTerminal) => {
                // Expected in CI.
            }
            Err(AcquireError::Open(e)) => {
                panic!("unexpected open(/dev/tty) failure: {e}");
            }
        }
    }
}
