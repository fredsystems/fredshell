// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! `pselect` multiplexer for terminal input and signals.
//!
//! Owns the body of [`super::TerminalSession::wait`] (see `PLAN_04`
//! §6 / §8 and subtask 04.6) plus the [`TtyInput`] / [`TtyOutput`]
//! byte channels built on top of the same tty fd.
//!
//! ## Why `pselect`
//!
//! `pselect(2)` is the portable primitive that atomically swaps in a
//! signal mask for the duration of the wait, eliminating the
//! classic race where a signal arrives between checking a flag and
//! entering `select` (`PLAN_04` §4.2). The handler-side mechanism is
//! the self-pipe — `pselect` returns "readable" on the pipe read end
//! as soon as the handler writes its wakeup byte, which the caller
//! then drains via [`super::signal::Handlers::drain`].
//!
//! ## What this layer does **not** do
//!
//! - It does not own the signal handlers (those are in
//!   [`super::signal`]).
//! - It does not decode bytes into key events (that is `PLAN_07`).
//! - It does not own the tty fd; both [`wait_for_event`] and
//!   [`TtyInput`] / [`TtyOutput`] borrow it from
//!   [`super::TerminalSession`].

use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, BorrowedFd, RawFd};
use std::time::Duration;

use libc::{EINTR, FD_ISSET, FD_SET, FD_ZERO, fd_set, pselect, sigemptyset, sigset_t, timespec};

/// Result of a single [`wait_for_event`] call.
///
/// Mirrors [`super::WaitEvent`] but is internal to the `wait`
/// submodule so the multiplexer can be unit-tested without
/// dragging in the rest of `TerminalSession`. The public
/// [`super::TerminalSession::wait`] translates this into
/// [`super::WaitEvent`] after decoding the self-pipe byte through
/// [`super::Signal::from_raw`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawWaitEvent {
    /// The tty fd became readable.
    TtyReadable,
    /// The signal self-pipe became readable (one or more signals
    /// were delivered). The caller drains the pipe to learn which.
    SignalPipeReadable,
    /// Both fds became readable in the same `pselect` return.
    BothReadable,
    /// The supplied deadline elapsed.
    Timeout,
}

/// Block until the tty fd or the signal self-pipe becomes readable,
/// or `deadline` elapses.
///
/// `deadline` of `None` means wait indefinitely. A `deadline` of
/// `Some(Duration::ZERO)` polls without blocking. `EINTR` returns
/// from `pselect` are not retried at this layer — the caller is
/// expected to treat them as a wakeup (a signal handler ran while we
/// were blocked, and the byte is now in the pipe) and re-enter the
/// loop. We surface `EINTR` as [`RawWaitEvent::SignalPipeReadable`]
/// because that is the canonical "go drain the self-pipe" signal.
///
/// # Errors
///
/// Returns the underlying `io::Error` from `pselect(2)` for any
/// error other than `EINTR`.
pub fn wait_for_event(
    tty_fd: BorrowedFd<'_>,
    sig_fd: BorrowedFd<'_>,
    deadline: Option<Duration>,
) -> io::Result<RawWaitEvent> {
    let tty_raw = tty_fd.as_raw_fd();
    let sig_raw = sig_fd.as_raw_fd();
    let nfds = tty_raw.max(sig_raw).saturating_add(1);

    // SAFETY: `fd_set` is POD; we initialize it via `FD_ZERO`
    // immediately. `sigset_t` is POD; we initialize via `sigemptyset`.
    let mut readfds: fd_set = unsafe { std::mem::zeroed() };
    let mut sigmask: sigset_t = unsafe { std::mem::zeroed() };

    // SAFETY: writing to local PODs through their valid out-pointers.
    unsafe {
        FD_ZERO(&raw mut readfds);
        FD_SET(tty_raw, &raw mut readfds);
        FD_SET(sig_raw, &raw mut readfds);
        let rc = sigemptyset(&raw mut sigmask);
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
    }

    let ts_storage: timespec;
    // Build the *const timespec for pselect. `null` means "block
    // forever"; a non-null pointer caps the wait at the supplied
    // duration. We materialize `ts_storage` only when needed so the
    // null-pointer path doesn't depend on any local stack init.
    // `Option::map_or` would require returning the address of a
    // closure-local, which dangles — keep the explicit form.
    #[allow(clippy::option_if_let_else)]
    let ts_ptr: *const timespec = if let Some(d) = deadline {
        ts_storage = duration_to_timespec(d);
        &raw const ts_storage
    } else {
        std::ptr::null()
    };

    // SAFETY: all pointers are either null or point to fully
    // initialized local PODs valid for the call.
    let rc = unsafe {
        pselect(
            nfds,
            &raw mut readfds,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            ts_ptr,
            &raw const sigmask,
        )
    };

    if rc < 0 {
        let err = io::Error::last_os_error();
        if err.raw_os_error() == Some(EINTR) {
            // A signal fired during the wait. The handler has (or
            // will momentarily) written to the self-pipe; the caller
            // should drain it. We don't return BothReadable here
            // because we cannot prove the tty is readable.
            return Ok(RawWaitEvent::SignalPipeReadable);
        }
        return Err(err);
    }

    if rc == 0 {
        return Ok(RawWaitEvent::Timeout);
    }

    // SAFETY: `readfds` was populated by `pselect`; FD_ISSET is a
    // read of the bit array.
    let tty_ready = unsafe { FD_ISSET(tty_raw, &raw const readfds) };
    // SAFETY: same.
    let sig_ready = unsafe { FD_ISSET(sig_raw, &raw const readfds) };

    Ok(match (tty_ready, sig_ready) {
        (true, true) => RawWaitEvent::BothReadable,
        (true, false) => RawWaitEvent::TtyReadable,
        (false, true) => RawWaitEvent::SignalPipeReadable,
        // pselect returned >0 but neither fd is readable: should not
        // happen on POSIX. Treat as a spurious wakeup → timeout so
        // the caller re-enters wait.
        (false, false) => RawWaitEvent::Timeout,
    })
}

fn duration_to_timespec(d: Duration) -> timespec {
    // Clamp to i64::MAX seconds; anything beyond that is "forever"
    // for our purposes and well past the longest reasonable shell
    // timeout. `nanos` always fits in `c_long` because it's < 1e9.
    let secs = i64::try_from(d.as_secs()).unwrap_or(i64::MAX);
    let nanos = c_long_from_u32(d.subsec_nanos());
    timespec {
        tv_sec: secs as libc::time_t,
        tv_nsec: nanos,
    }
}

#[allow(clippy::cast_possible_wrap)]
const fn c_long_from_u32(n: u32) -> libc::c_long {
    // n < 1_000_000_000 so the cast is lossless on every supported
    // target (libc::c_long is at least 32 bits and always signed;
    // 1e9 fits in a signed 32-bit integer).
    n as libc::c_long
}

/// Read handle on the tty fd.
///
/// Implements [`Read`] using `read(2)` directly so it respects the
/// current termios (raw mode delivers byte-at-a-time, cooked mode
/// delivers line-at-a-time). The handle borrows the fd from
/// [`super::TerminalSession`]; it never closes it.
///
/// In raw mode, `read` blocks until at least one byte is available
/// (`VMIN=1`, `VTIME=0`); a short read of a single byte is the
/// expected steady state.
#[derive(Debug)]
pub struct TtyInput<'a> {
    fd: BorrowedFd<'a>,
}

impl<'a> TtyInput<'a> {
    /// Construct a fresh reader over an already-borrowed tty fd.
    #[must_use]
    pub const fn new(fd: BorrowedFd<'a>) -> Self {
        Self { fd }
    }
}

impl Read for TtyInput<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            // SAFETY: `self.fd` is a valid borrowed fd; `buf` is a
            // valid mutable slice of the requested length.
            let n = unsafe {
                libc::read(
                    self.fd.as_raw_fd(),
                    buf.as_mut_ptr().cast::<libc::c_void>(),
                    buf.len(),
                )
            };
            if n >= 0 {
                #[allow(clippy::cast_sign_loss)]
                return Ok(n as usize);
            }
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(EINTR) {
                // The signal handler ran; the caller will see the
                // self-pipe byte on the next `wait`. Retry the read
                // so a `Read::read_to_end`-style consumer doesn't
                // see spurious errors from benign signals.
                continue;
            }
            return Err(err);
        }
    }
}

/// Write handle on the tty fd.
///
/// Implements [`Write`] using `write(2)`. Short writes are looped
/// inside [`Write::write_all`] (the default impl handles that); a
/// bare `write` returns the count `write(2)` returned. `flush` is a
/// no-op because the kernel tty driver does not buffer at the user
/// level — termios output flags govern any kernel-side translation.
///
/// `EINTR` is retried transparently inside both `write` and
/// `flush` so callers do not see spurious failures from benign
/// signals like `SIGWINCH`.
#[derive(Debug)]
pub struct TtyOutput<'a> {
    fd: BorrowedFd<'a>,
}

impl<'a> TtyOutput<'a> {
    /// Construct a fresh writer over an already-borrowed tty fd.
    #[must_use]
    pub const fn new(fd: BorrowedFd<'a>) -> Self {
        Self { fd }
    }

    /// Raw fd for callers that need to pass it to other syscalls
    /// (e.g., `tcsetpgrp` in `PLAN_04` §7). The borrow tracks the
    /// underlying [`BorrowedFd`].
    #[must_use]
    pub fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl Write for TtyOutput<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            // SAFETY: `self.fd` is a valid borrowed fd; `buf` is a
            // valid slice of the requested length.
            let n = unsafe {
                libc::write(
                    self.fd.as_raw_fd(),
                    buf.as_ptr().cast::<libc::c_void>(),
                    buf.len(),
                )
            };
            if n >= 0 {
                #[allow(clippy::cast_sign_loss)]
                return Ok(n as usize);
            }
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(EINTR) {
                continue;
            }
            return Err(err);
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        // The kernel tty driver does not buffer at the user level.
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::super::test_pty::FakePty;
    use super::{RawWaitEvent, TtyInput, TtyOutput, wait_for_event};
    use std::io::{Read, Write};
    use std::os::fd::{AsFd, AsRawFd};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn timeout_returns_timeout_event() {
        let Some(pty) = FakePty::open() else {
            return;
        };
        let (sig_r, _sig_w) = make_pipe();

        let start = Instant::now();
        let ev = wait_for_event(
            pty.slave().as_fd(),
            sig_r.as_fd(),
            Some(Duration::from_millis(20)),
        )
        .unwrap();
        let elapsed = start.elapsed();
        assert_eq!(ev, RawWaitEvent::Timeout);
        assert!(elapsed >= Duration::from_millis(15), "elapsed: {elapsed:?}");
    }

    #[test]
    fn zero_duration_polls_without_blocking() {
        let Some(pty) = FakePty::open() else {
            return;
        };
        let (sig_r, _sig_w) = make_pipe();

        let start = Instant::now();
        let ev = wait_for_event(
            pty.slave().as_fd(),
            sig_r.as_fd(),
            Some(Duration::from_millis(0)),
        )
        .unwrap();
        assert_eq!(ev, RawWaitEvent::Timeout);
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[test]
    fn tty_input_wakes_wait() {
        let Some(pty) = FakePty::open() else {
            return;
        };
        let (sig_r, _sig_w) = make_pipe();

        // Write a byte to the master end so the slave becomes readable.
        // SAFETY: pty.master() is a valid fd.
        let master_fd = pty.master().as_raw_fd();
        let n = unsafe { libc::write(master_fd, b"x".as_ptr().cast::<libc::c_void>(), 1) };
        assert_eq!(n, 1);

        let ev = wait_for_event(
            pty.slave().as_fd(),
            sig_r.as_fd(),
            Some(Duration::from_secs(1)),
        )
        .unwrap();
        assert_eq!(ev, RawWaitEvent::TtyReadable);
    }

    #[test]
    fn signal_pipe_wakes_wait() {
        let Some(pty) = FakePty::open() else {
            return;
        };
        let (sig_r, sig_w) = make_pipe();

        // SAFETY: sig_w is a valid fd we own.
        let n = unsafe {
            libc::write(
                sig_w.as_raw_fd(),
                b"\x02".as_ptr().cast::<libc::c_void>(),
                1,
            )
        };
        assert_eq!(n, 1);

        let ev = wait_for_event(
            pty.slave().as_fd(),
            sig_r.as_fd(),
            Some(Duration::from_secs(1)),
        )
        .unwrap();
        assert_eq!(ev, RawWaitEvent::SignalPipeReadable);
    }

    #[test]
    fn both_fds_ready_reports_both() {
        let Some(pty) = FakePty::open() else {
            return;
        };
        let (sig_r, sig_w) = make_pipe();

        // SAFETY: master is a valid fd.
        let n1 = unsafe {
            libc::write(
                pty.master().as_raw_fd(),
                b"x".as_ptr().cast::<libc::c_void>(),
                1,
            )
        };
        assert_eq!(n1, 1);
        // SAFETY: sig_w is valid.
        let n2 = unsafe {
            libc::write(
                sig_w.as_raw_fd(),
                b"\x02".as_ptr().cast::<libc::c_void>(),
                1,
            )
        };
        assert_eq!(n2, 1);

        // Small sleep so both writes propagate.
        thread::sleep(Duration::from_millis(5));

        let ev = wait_for_event(
            pty.slave().as_fd(),
            sig_r.as_fd(),
            Some(Duration::from_secs(1)),
        )
        .unwrap();
        assert_eq!(ev, RawWaitEvent::BothReadable);
    }

    #[test]
    fn tty_input_reads_bytes() {
        let Some(pty) = FakePty::open() else {
            return;
        };
        // SAFETY: master is a valid fd; the buffer is owned.
        let n = unsafe {
            libc::write(
                pty.master().as_raw_fd(),
                b"hello".as_ptr().cast::<libc::c_void>(),
                5,
            )
        };
        assert_eq!(n, 5);

        let mut input = TtyInput::new(pty.slave().as_fd());
        let mut buf = [0u8; 16];
        let got = input.read(&mut buf).unwrap();
        assert!(got >= 1, "read returned {got}");
        // PTY may deliver in one chunk; first byte must be 'h'.
        assert_eq!(buf[0], b'h');
    }

    #[test]
    fn tty_input_empty_buf_returns_zero() {
        let Some(pty) = FakePty::open() else {
            return;
        };
        let mut input = TtyInput::new(pty.slave().as_fd());
        let mut buf: [u8; 0] = [];
        assert_eq!(input.read(&mut buf).unwrap(), 0);
    }

    #[test]
    fn tty_output_writes_bytes() {
        let Some(pty) = FakePty::open() else {
            return;
        };
        let mut output = TtyOutput::new(pty.slave().as_fd());
        let n = output.write(b"world").unwrap();
        assert_eq!(n, 5);
        output.flush().unwrap();

        // Read it back from the master end.
        let mut buf = [0u8; 16];
        // SAFETY: master fd owned by pty; buf is valid.
        let got = unsafe {
            libc::read(
                pty.master().as_raw_fd(),
                buf.as_mut_ptr().cast::<libc::c_void>(),
                buf.len(),
            )
        };
        assert!(got >= 1);
        #[allow(clippy::cast_sign_loss)]
        let got = got as usize;
        // tty may translate \n on output; here we wrote no newlines.
        assert_eq!(&buf[..got.min(5)], &b"world"[..got.min(5)]);
    }

    #[test]
    fn tty_output_empty_buf_returns_zero() {
        let Some(pty) = FakePty::open() else {
            return;
        };
        let mut output = TtyOutput::new(pty.slave().as_fd());
        assert_eq!(output.write(&[]).unwrap(), 0);
    }

    #[test]
    fn tty_output_exposes_raw_fd() {
        let Some(pty) = FakePty::open() else {
            return;
        };
        let output = TtyOutput::new(pty.slave().as_fd());
        assert_eq!(output.as_raw_fd(), pty.slave().as_raw_fd());
    }

    /// Helper: make a non-blocking pipe pair so the wait tests can
    /// simulate the self-pipe without touching the real signal
    /// machinery.
    fn make_pipe() -> (std::os::fd::OwnedFd, std::os::fd::OwnedFd) {
        use std::os::fd::FromRawFd;
        let mut fds: [libc::c_int; 2] = [-1, -1];
        // SAFETY: fds is a valid 2-element array.
        let rc = unsafe { libc::pipe(fds.as_mut_ptr()) };
        assert_eq!(rc, 0);
        // SAFETY: pipe(2) wrote two valid owned descriptors.
        let r = unsafe { std::os::fd::OwnedFd::from_raw_fd(fds[0]) };
        // SAFETY: same.
        let w = unsafe { std::os::fd::OwnedFd::from_raw_fd(fds[1]) };
        // Non-blocking so tests cannot accidentally hang.
        // SAFETY: r is a valid fd.
        let flags = unsafe { libc::fcntl(r.as_raw_fd(), libc::F_GETFL) };
        // SAFETY: same.
        unsafe {
            libc::fcntl(r.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
        (r, w)
    }
}
