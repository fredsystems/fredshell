// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Signal handling: `sigaction` installation and the self-pipe.
//!
//! Owns the signal-handler registration table from `PLAN_04` §4. A
//! single process-wide self-pipe is established at startup; the
//! handler writes the signal number as one byte to the write end and
//! [`super::TerminalSession::wait`] selects on the read end alongside
//! the tty fd (subtask 04.6).
//!
//! ## Async-signal-safety
//!
//! The handler does exactly two things, both async-signal-safe:
//!
//! 1. For `SIGINT` / `SIGALRM`: atomically set the global cancel flag.
//! 2. For every caught signal: `write(2)` one byte to the self-pipe.
//!
//! No allocation, no locking, no `println!`. The pipe is in
//! `O_NONBLOCK` mode so the handler can never block; a full pipe is
//! benign because the buffered byte already represents "a signal of
//! some kind was delivered" — the main loop drains it and reaps state
//! anyway.
//!
//! ## Globals
//!
//! Two `static AtomicI32`s hold the write-end fd and the address of
//! the cancel flag's `AtomicBool`. They are populated once by
//! [`install`] and read by [`handler`] thereafter. The shell installs
//! handlers at most once per process; re-installation is a programmer
//! error and returns [`InstallError::AlreadyInstalled`].

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering};

use libc::{
    EAGAIN, EINTR, F_GETFL, F_SETFD, F_SETFL, FD_CLOEXEC, O_NONBLOCK, SA_RESTART, SIG_IGN, SIGALRM,
    SIGCHLD, SIGHUP, SIGINT, SIGTERM, SIGTTIN, SIGTTOU, SIGUSR1, SIGUSR2, SIGWINCH, c_int, pipe,
    sigaction, sigemptyset,
};

/// Identifies which signal woke [`super::TerminalSession::wait`].
///
/// Only signals fredshell installs handlers for appear here. POSIX
/// signal numbers we explicitly ignore (`SIGTTOU`, `SIGTTIN`,
/// `SIGQUIT`), leave at default disposition (`SIGPIPE`), or that have
/// no shell-level meaning are not represented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Signal {
    /// `SIGINT` — user pressed `Ctrl-C`.
    Int,
    /// `SIGTERM` — graceful termination request.
    Term,
    /// `SIGHUP` — controlling terminal hung up.
    Hup,
    /// `SIGWINCH` — terminal window resized.
    WinCh,
    /// `SIGCHLD` — a child process changed state.
    Chld,
    /// `SIGALRM` — timer expired (used by `read -t`, capability probe).
    Alrm,
    /// `SIGUSR1` — reserved for user-defined use.
    Usr1,
    /// `SIGUSR2` — reserved for user-defined use.
    Usr2,
}

impl Signal {
    /// Decode the byte the handler wrote to the self-pipe.
    ///
    /// Returns `None` for any byte that does not correspond to one of
    /// the signals fredshell catches. The main loop treats unknown
    /// bytes as "spurious wakeup" and re-enters `pselect`.
    #[must_use]
    pub const fn from_raw(value: c_int) -> Option<Self> {
        Some(match value {
            SIGINT => Self::Int,
            SIGTERM => Self::Term,
            SIGHUP => Self::Hup,
            SIGWINCH => Self::WinCh,
            SIGCHLD => Self::Chld,
            SIGALRM => Self::Alrm,
            SIGUSR1 => Self::Usr1,
            SIGUSR2 => Self::Usr2,
            _ => return None,
        })
    }

    /// The POSIX signal number this variant represents.
    #[must_use]
    pub const fn as_raw(self) -> c_int {
        match self {
            Self::Int => SIGINT,
            Self::Term => SIGTERM,
            Self::Hup => SIGHUP,
            Self::WinCh => SIGWINCH,
            Self::Chld => SIGCHLD,
            Self::Alrm => SIGALRM,
            Self::Usr1 => SIGUSR1,
            Self::Usr2 => SIGUSR2,
        }
    }
}

/// Errors returned by [`install`].
#[derive(Debug)]
#[non_exhaustive]
pub enum InstallError {
    /// `pipe(2)` failed when creating the self-pipe.
    PipeCreate(io::Error),
    /// `fcntl(2)` failed when applying `O_NONBLOCK` or `FD_CLOEXEC`
    /// to one of the pipe ends.
    Fcntl(io::Error),
    /// `sigaction(2)` failed when installing a handler. The numeric
    /// signal that failed is included for diagnostics.
    Sigaction { signum: c_int, source: io::Error },
    /// [`install`] was called more than once in the same process.
    /// Signal-handler installation is exactly-once.
    AlreadyInstalled,
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PipeCreate(_) => f.write_str("failed to create self-pipe"),
            Self::Fcntl(_) => f.write_str("failed to configure self-pipe flags"),
            Self::Sigaction { signum, .. } => {
                write!(f, "failed to install handler for signal {signum}")
            }
            Self::AlreadyInstalled => f.write_str("signal handlers are already installed"),
        }
    }
}

impl std::error::Error for InstallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PipeCreate(e) | Self::Fcntl(e) => Some(e),
            Self::Sigaction { source, .. } => Some(source),
            Self::AlreadyInstalled => None,
        }
    }
}

/// Set of installed handlers and the self-pipe read end.
///
/// The struct is the *handle* the rest of the tty module uses to
/// observe signal delivery: [`Handlers::reader`] is the fd selected
/// on by [`super::TerminalSession::wait`] in subtask 04.6, and
/// [`Handlers::drain`] consumes pending bytes after `pselect` reports
/// readability. The write end is owned by the static state populated
/// in [`install`] and lives for the rest of the process.
#[derive(Debug)]
pub struct Handlers {
    reader: OwnedFd,
}

impl Handlers {
    /// Borrowed read end of the self-pipe.
    #[must_use]
    pub const fn reader(&self) -> &OwnedFd {
        &self.reader
    }

    /// Consume the read end (used when transferring ownership into
    /// [`super::TerminalSession`]).
    #[must_use]
    pub fn into_reader(self) -> OwnedFd {
        self.reader
    }

    /// Drain currently-pending bytes from the self-pipe into `buf`,
    /// returning the slice that was filled.
    ///
    /// The pipe is non-blocking, so this returns `Ok(&[])` once the
    /// pipe is empty rather than blocking. Each byte in the returned
    /// slice is the `c_int` (as a `u8`-truncated) signal number that
    /// was delivered, decodable via [`Signal::from_raw`].
    ///
    /// `EINTR` is retried transparently; any other error surfaces.
    ///
    /// # Errors
    ///
    /// Returns the underlying `io::Error` from `read(2)` for any
    /// error other than `EINTR`, `EAGAIN`, or `EWOULDBLOCK`.
    pub fn drain<'b>(&self, buf: &'b mut [u8]) -> io::Result<&'b [u8]> {
        drain_pipe(self.reader.as_raw_fd(), buf)
    }
}

/// Drain a non-blocking self-pipe-style fd into `buf`.
///
/// Used by [`Handlers::drain`] and by
/// [`super::TerminalSession::wait`] which holds the read end as a
/// bare [`OwnedFd`] inside [`super::TerminalSession`].
///
/// # Errors
///
/// Returns the underlying `io::Error` from `read(2)` for any error
/// other than `EINTR`, `EAGAIN`, or `EWOULDBLOCK`.
pub fn drain_pipe(fd: RawFd, buf: &mut [u8]) -> io::Result<&[u8]> {
    if buf.is_empty() {
        return Ok(&buf[..0]);
    }
    loop {
        // SAFETY: `fd` is owned by the caller; `buf` is a valid
        // mutable slice; we pass its real length.
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast::<libc::c_void>(), buf.len()) };
        if n >= 0 {
            // Truncation cannot happen because n <= buf.len() <= isize::MAX.
            #[allow(clippy::cast_sign_loss)]
            let n = n as usize;
            return Ok(&buf[..n]);
        }
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(EINTR) => {}
            // On Linux and macOS EAGAIN == EWOULDBLOCK; both
            // mean "non-blocking read found no data."
            Some(EAGAIN) => return Ok(&buf[..0]),
            _ => return Err(err),
        }
    }
}

/// Write end of the self-pipe, as a raw fd shared with the async
/// signal handler. `-1` means "not installed yet."
///
/// Stored as `AtomicI32` because the handler can read it concurrently
/// with the (one-shot) installer. `Ordering::Acquire` on the load
/// pairs with `Ordering::Release` on the store so the handler always
/// observes a fully-initialized pipe.
static SIG_PIPE_WRITE_FD: AtomicI32 = AtomicI32::new(-1);

/// Pointer to the cancellation flag's `AtomicBool`. `null` until
/// [`install`] runs. The handler dereferences this for `SIGINT` and
/// `SIGALRM`.
static CANCEL_FLAG_PTR: AtomicPtr<AtomicBool> = AtomicPtr::new(std::ptr::null_mut());

/// Set by [`install`] to enforce the exactly-once contract.
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Install signal handlers and create the self-pipe.
///
/// `cancel` is the `AtomicBool` shared with
/// [`super::CancellationToken`]; the handler sets it on `SIGINT` and
/// `SIGALRM`. The caller is responsible for keeping the `Arc` alive
/// for the lifetime of the process — the handler treats the pointer
/// as `'static`.
///
/// `SIGTTOU` and `SIGTTIN` are installed as `SIG_IGN` so the shell
/// can call `tcsetpgrp` without stopping itself (`PLAN_04` §4.1).
/// `SIGQUIT` is intentionally not installed; the shell relies on the
/// kernel-default ignore behavior in interactive mode (set by the
/// REPL via a separate `sigaction(SIG_IGN)` call when entering
/// interactive mode in 04.10).
///
/// # Errors
///
/// Returns [`InstallError::AlreadyInstalled`] if called twice in the
/// same process. Returns [`InstallError::PipeCreate`] or
/// [`InstallError::Fcntl`] if the self-pipe could not be set up.
/// Returns [`InstallError::Sigaction`] if any handler installation
/// fails; partially-installed handlers are left in place because
/// `sigaction` has no rollback primitive and leaving the previously-
/// installed handlers active is strictly safer than restoring
/// `SIG_DFL`.
pub fn install(cancel: &std::sync::Arc<AtomicBool>) -> Result<Handlers, InstallError> {
    if INSTALLED.swap(true, Ordering::AcqRel) {
        return Err(InstallError::AlreadyInstalled);
    }

    let (reader, writer) = make_self_pipe()?;

    // Publish the writer fd and the cancel-flag pointer *before*
    // installing handlers, so the first delivered signal sees both.
    let cancel_ptr: *const AtomicBool = std::sync::Arc::as_ptr(cancel);
    CANCEL_FLAG_PTR.store(cancel_ptr.cast_mut(), Ordering::Release);
    SIG_PIPE_WRITE_FD.store(writer.as_raw_fd(), Ordering::Release);
    // Leak the writer end intentionally: the handler needs a stable
    // raw fd for the process lifetime, and we have no shutdown path
    // (the shell exits via `exit(2)` which closes everything).
    std::mem::forget(writer);

    for signum in CAUGHT_SIGNALS {
        install_handler(*signum)?;
    }
    install_ignore(SIGTTOU)?;
    install_ignore(SIGTTIN)?;

    Ok(Handlers { reader })
}

const CAUGHT_SIGNALS: &[c_int] = &[
    SIGINT, SIGTERM, SIGHUP, SIGWINCH, SIGCHLD, SIGALRM, SIGUSR1, SIGUSR2,
];

fn make_self_pipe() -> Result<(OwnedFd, OwnedFd), InstallError> {
    let mut fds: [c_int; 2] = [-1, -1];
    // SAFETY: `fds` is a valid 2-element array of c_int.
    let rc = unsafe { pipe(fds.as_mut_ptr()) };
    if rc != 0 {
        return Err(InstallError::PipeCreate(io::Error::last_os_error()));
    }
    // SAFETY: pipe(2) on success populates both fds with owned descriptors.
    let reader = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    // SAFETY: same.
    let writer = unsafe { OwnedFd::from_raw_fd(fds[1]) };

    set_cloexec(reader.as_raw_fd()).map_err(InstallError::Fcntl)?;
    set_cloexec(writer.as_raw_fd()).map_err(InstallError::Fcntl)?;
    set_nonblock(reader.as_raw_fd()).map_err(InstallError::Fcntl)?;
    set_nonblock(writer.as_raw_fd()).map_err(InstallError::Fcntl)?;

    Ok((reader, writer))
}

fn set_cloexec(fd: RawFd) -> io::Result<()> {
    // SAFETY: fd is a valid descriptor for the calling process.
    let rc = unsafe { libc::fcntl(fd, F_SETFD, FD_CLOEXEC) };
    if rc == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn set_nonblock(fd: RawFd) -> io::Result<()> {
    // SAFETY: fd is a valid descriptor for the calling process.
    let flags = unsafe { libc::fcntl(fd, F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: same.
    let rc = unsafe { libc::fcntl(fd, F_SETFL, flags | O_NONBLOCK) };
    if rc == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn install_handler(signum: c_int) -> Result<(), InstallError> {
    // SAFETY: `sigaction` is POD; `sigemptyset` initializes the mask field.
    let mut action: sigaction = unsafe { std::mem::zeroed() };
    // SAFETY: writing to a local POD struct.
    let rc = unsafe { sigemptyset(&raw mut action.sa_mask) };
    if rc != 0 {
        return Err(InstallError::Sigaction {
            signum,
            source: io::Error::last_os_error(),
        });
    }
    action.sa_flags = SA_RESTART;
    // Cast via a typed function pointer first, then to usize. A
    // direct `handler as usize` triggers clippy::fn_to_numeric_cast.
    let fn_ptr: extern "C" fn(c_int) = handler;
    action.sa_sigaction = fn_ptr as usize;

    // SAFETY: `action` is a fully-initialized sigaction; we pass a
    // null old-action because we do not need to restore.
    let rc = unsafe { sigaction(signum, &raw const action, std::ptr::null_mut()) };
    if rc != 0 {
        return Err(InstallError::Sigaction {
            signum,
            source: io::Error::last_os_error(),
        });
    }
    Ok(())
}

fn install_ignore(signum: c_int) -> Result<(), InstallError> {
    // SAFETY: zeroed sigaction is valid; we then set the disposition.
    let mut action: sigaction = unsafe { std::mem::zeroed() };
    // SAFETY: writing to a local POD struct.
    let rc = unsafe { sigemptyset(&raw mut action.sa_mask) };
    if rc != 0 {
        return Err(InstallError::Sigaction {
            signum,
            source: io::Error::last_os_error(),
        });
    }
    action.sa_sigaction = SIG_IGN;

    // SAFETY: `action` is a fully-initialized sigaction.
    let rc = unsafe { sigaction(signum, &raw const action, std::ptr::null_mut()) };
    if rc != 0 {
        return Err(InstallError::Sigaction {
            signum,
            source: io::Error::last_os_error(),
        });
    }
    Ok(())
}

/// Async-signal-safe signal handler.
///
/// MUST contain only async-signal-safe operations: atomic loads /
/// stores and `write(2)`. No allocation, no locking, no formatting.
extern "C" fn handler(signum: c_int) {
    // Preserve errno across the handler, since `write(2)` may clobber it.
    // SAFETY: `__errno_location` / `__error` returns a thread-local
    // pointer that is valid for the lifetime of the thread; reading
    // and restoring through it is async-signal-safe.
    let errno_save = unsafe { *libc::__errno_location() };

    if signum == SIGINT || signum == SIGALRM {
        let ptr = CANCEL_FLAG_PTR.load(Ordering::Acquire);
        if !ptr.is_null() {
            // SAFETY: the pointer was published by `install` and the
            // backing `Arc` is kept alive by the caller for the
            // process lifetime. `AtomicBool::store` is async-signal-
            // safe (it is a single atomic instruction on all
            // supported architectures).
            unsafe { (*ptr).store(true, Ordering::Relaxed) };
        }
    }

    let fd = SIG_PIPE_WRITE_FD.load(Ordering::Acquire);
    if fd >= 0 {
        // Truncating signum (always 1..=64 on POSIX) to a byte.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let byte = signum as u8;
        let buf = [byte];
        // SAFETY: `write` is async-signal-safe; we ignore short
        // writes / EAGAIN because the pipe being full already means
        // "a wakeup byte is buffered."
        let _ = unsafe { libc::write(fd, buf.as_ptr().cast::<libc::c_void>(), 1) };
    }

    // SAFETY: same justification as the save above.
    unsafe { *libc::__errno_location() = errno_save };
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{
        CANCEL_FLAG_PTR, Handlers, INSTALLED, SIG_PIPE_WRITE_FD, Signal, make_self_pipe,
        set_cloexec, set_nonblock,
    };
    use libc::{F_GETFD, F_GETFL, FD_CLOEXEC, O_NONBLOCK};
    use std::os::fd::AsRawFd;
    use std::sync::atomic::Ordering;

    // NOTE: `install` is exercised by 04.6 / 04.10 integration tests
    // because installing process-wide sigactions inside a cargo test
    // harness perturbs the test runner. Here we cover the pure /
    // syscall-free pieces (Signal mapping) and the self-pipe
    // plumbing in isolation.

    #[test]
    fn signal_is_copy() {
        const _: fn() = || {
            fn assert_copy<T: Copy>() {}
            assert_copy::<Signal>();
        };
    }

    #[test]
    fn signal_variants_are_distinct() {
        assert_ne!(Signal::Int, Signal::Term);
        assert_ne!(Signal::WinCh, Signal::Chld);
        assert_ne!(Signal::Usr1, Signal::Usr2);
    }

    #[test]
    fn signal_from_raw_round_trip() {
        for s in [
            Signal::Int,
            Signal::Term,
            Signal::Hup,
            Signal::WinCh,
            Signal::Chld,
            Signal::Alrm,
            Signal::Usr1,
            Signal::Usr2,
        ] {
            assert_eq!(Signal::from_raw(s.as_raw()), Some(s));
        }
    }

    #[test]
    fn signal_from_raw_rejects_unknown() {
        // SIGQUIT is explicitly not in the caught set.
        assert_eq!(Signal::from_raw(libc::SIGQUIT), None);
        assert_eq!(Signal::from_raw(0), None);
        assert_eq!(Signal::from_raw(-1), None);
    }

    #[test]
    fn make_self_pipe_sets_cloexec_and_nonblock() {
        let (reader, writer) = make_self_pipe().unwrap();
        for fd in [reader.as_raw_fd(), writer.as_raw_fd()] {
            // SAFETY: fd is owned by the local pipe ends.
            let fl = unsafe { libc::fcntl(fd, F_GETFL) };
            assert!(fl != -1);
            assert!(fl & O_NONBLOCK != 0, "O_NONBLOCK not set on fd {fd}");

            // SAFETY: fd is owned by the local pipe ends.
            let fd_flags = unsafe { libc::fcntl(fd, F_GETFD) };
            assert!(fd_flags != -1);
            assert!(fd_flags & FD_CLOEXEC != 0, "FD_CLOEXEC not set on fd {fd}");
        }
    }

    #[test]
    fn drain_on_empty_pipe_returns_empty_slice() {
        let (reader, _writer) = make_self_pipe().unwrap();
        let handlers = Handlers { reader };
        let mut buf = [0u8; 8];
        let drained = handlers.drain(&mut buf).unwrap();
        assert!(drained.is_empty());
    }

    #[test]
    fn drain_returns_written_bytes_in_order() {
        let (reader, writer) = make_self_pipe().unwrap();
        // POSIX signal numbers fit in a u8 (max 64-ish on Linux);
        // truncation is by design here — the handler writes the
        // single byte the main loop decodes via Signal::from_raw.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let bytes: [u8; 3] = [
            libc::SIGINT as u8,
            libc::SIGCHLD as u8,
            libc::SIGWINCH as u8,
        ];
        // SAFETY: we own `writer` and the buffer is valid for 3 bytes.
        let n =
            unsafe { libc::write(writer.as_raw_fd(), bytes.as_ptr().cast::<libc::c_void>(), 3) };
        assert_eq!(n, 3);

        let handlers = Handlers { reader };
        let mut buf = [0u8; 8];
        let drained = handlers.drain(&mut buf).unwrap();
        assert_eq!(drained.len(), 3);
        assert_eq!(Signal::from_raw(i32::from(drained[0])), Some(Signal::Int));
        assert_eq!(Signal::from_raw(i32::from(drained[1])), Some(Signal::Chld));
        assert_eq!(Signal::from_raw(i32::from(drained[2])), Some(Signal::WinCh));
    }

    #[test]
    fn drain_with_empty_buf_is_noop() {
        let (reader, _writer) = make_self_pipe().unwrap();
        let handlers = Handlers { reader };
        let mut buf: [u8; 0] = [];
        let drained = handlers.drain(&mut buf).unwrap();
        assert!(drained.is_empty());
    }

    #[test]
    fn set_cloexec_is_idempotent() {
        let (reader, _writer) = make_self_pipe().unwrap();
        set_cloexec(reader.as_raw_fd()).unwrap();
        set_cloexec(reader.as_raw_fd()).unwrap();
    }

    #[test]
    fn set_nonblock_is_idempotent() {
        let (reader, _writer) = make_self_pipe().unwrap();
        set_nonblock(reader.as_raw_fd()).unwrap();
        set_nonblock(reader.as_raw_fd()).unwrap();
    }

    #[test]
    fn globals_start_uninitialized_unless_installed() {
        // If a previous test in this binary called `install`, these
        // will be populated — skip the assertions in that case.
        // Otherwise verify the defaults match the documented values.
        if INSTALLED.load(Ordering::Acquire) {
            return;
        }
        assert_eq!(SIG_PIPE_WRITE_FD.load(Ordering::Acquire), -1);
        assert!(CANCEL_FLAG_PTR.load(Ordering::Acquire).is_null());
    }
}
