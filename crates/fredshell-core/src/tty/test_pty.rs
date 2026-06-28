// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Fake-PTY test harness (test-only).
//!
//! `PLAN_04` §9 calls for a fake-PTY harness that drives
//! [`super::TerminalSession`] and the wait multiplexer without
//! requiring a real interactive terminal. This module is the
//! implementation of that harness.
//!
//! It is **not** a master-side PTY abstraction for production use
//! (see `PLAN_04` §1: fredshell does not create PTYs in v1). It
//! exists purely so tests can:
//!
//! - Obtain a real slave-side fd that `tcgetattr` / `pselect` /
//!   `read` / `write` all accept as a tty.
//! - Drive that slave from the master side by writing bytes from the
//!   test process to simulate "the user pressed a key" or "the
//!   terminal sent a CSI response."
//! - Drain bytes written to the slave to verify what the shell
//!   emitted (escape sequences, prompt redraws, etc.).
//!
//! The whole module is `cfg(test)` — it compiles only for the test
//! binary, so it cannot leak into production code paths.

#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::os::fd::{FromRawFd, OwnedFd};

/// Owned master/slave PTY pair used in tests.
///
/// Both ends are kept alive for the lifetime of the struct. Dropping
/// the `FakePty` closes both descriptors, which causes any pending
/// read on the other end to return EOF — the cleanest cleanup
/// signal for blocked test threads.
#[derive(Debug)]
#[allow(clippy::redundant_pub_crate)]
pub(crate) struct FakePty {
    master: OwnedFd,
    slave: OwnedFd,
}

#[allow(clippy::redundant_pub_crate)]
impl FakePty {
    /// Open a new master/slave pair via `openpty(3)`.
    ///
    /// Returns `None` when `openpty` is unavailable (some sandboxed
    /// CI environments). Tests should treat `None` as "skip" rather
    /// than as a failure — the same convention used by the
    /// `tty::termios` tests.
    pub(crate) fn open() -> Option<Self> {
        let mut master: libc::c_int = -1;
        let mut slave: libc::c_int = -1;
        // SAFETY: openpty writes two valid fds on success; the three
        // null arguments are explicitly permitted (no name buffer,
        // no termios template, no winsize template — slave inherits
        // sensible defaults). `null_mut` is used for the termios /
        // winsize templates so the call type-checks on both Linux
        // (`*const termios`) and macOS/BSD (`*mut termios`): a
        // `*mut T` null coerces to a `*const T` parameter, but not
        // vice versa.
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
        // SAFETY: openpty(3) populated both fds on success; ownership
        // transfers cleanly into OwnedFd.
        let master = unsafe { OwnedFd::from_raw_fd(master) };
        // SAFETY: same.
        let slave = unsafe { OwnedFd::from_raw_fd(slave) };

        // Put the slave into raw mode so blocking reads return after
        // a single byte (VMIN=1, VTIME=0) instead of waiting for a
        // newline. This matches how the real shell uses the tty in
        // interactive mode and prevents tests from hanging on cooked-
        // mode line buffering.
        // SAFETY: slave is a valid tty fd; termios is POD initialized
        // by tcgetattr.
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(std::os::fd::AsRawFd::as_raw_fd(&slave), &raw mut t) == 0 {
                libc::cfmakeraw(&raw mut t);
                let _ = libc::tcsetattr(
                    std::os::fd::AsRawFd::as_raw_fd(&slave),
                    libc::TCSANOW,
                    &raw const t,
                );
            }
        }

        Some(Self { master, slave })
    }

    /// Borrow the master end (test-side: write to simulate keystrokes,
    /// read to inspect emitted bytes).
    pub(crate) const fn master(&self) -> &OwnedFd {
        &self.master
    }

    /// Borrow the slave end (shell-side: what `TerminalSession` would
    /// use as its `/dev/tty` substitute in a real session).
    pub(crate) const fn slave(&self) -> &OwnedFd {
        &self.slave
    }
}
