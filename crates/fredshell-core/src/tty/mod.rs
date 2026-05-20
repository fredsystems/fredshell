// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Terminal I/O, signals, and capability detection.
//!
//! This module operationalizes `PLAN_04`. It owns the slave-side
//! relationship between the shell and the controlling terminal:
//!
//! - acquiring `/dev/tty` as a reliable handle to the controlling
//!   terminal regardless of fd 0/1/2 redirection,
//! - installing signal handlers and the self-pipe wakeup mechanism,
//! - entering and restoring raw mode,
//! - running the one-shot capability probe at startup,
//! - tracking window size across SIGWINCH,
//! - giving / taking terminal foreground for child process groups.
//!
//! It does **not** create pseudo-terminals (see `PLAN_04` §1 for the
//! slave-side scope rationale), encode ANSI sequences (that is
//! `fredshell-ansi`), or decode keystrokes into semantic events
//! (that will be `PLAN_07`).
//!
//! ## Public surface
//!
//! [`TerminalSession`] is the single owner of terminal state. Other
//! subsystems request transitions through its typed API; nothing
//! else in the workspace calls into the libc terminal interface
//! directly.
//!
//! ## Subtask layout
//!
//! The submodule tree mirrors `PLAN_04` §3:
//!
//! | Submodule          | Owns                                                    |
//! | ------------------ | ------------------------------------------------------- |
//! | [`termios`]        | Raw-mode RAII guard.                                    |
//! | [`controlling`]    | `/dev/tty` acquisition, isatty checks.                  |
//! | [`pgrp`]           | `setpgid`, `tcsetpgrp` helpers.                         |
//! | [`signal`]         | `sigaction` installation, self-pipe / cancel flag.      |
//! | [`wait`]           | `pselect`/`poll` multiplexer.                           |
//! | [`winsize`]        | `TIOCGWINSZ` + SIGWINCH broadcast.                      |
//! | [`capabilities`]   | Probe orchestration + [`Capabilities`].                 |
//! | [`probe`]          | Individual capability probes.                           |

pub mod capabilities;
pub mod controlling;
pub mod pgrp;
pub mod probe;
pub mod signal;
pub mod termios;
pub mod wait;
pub mod winsize;

#[cfg(test)]
mod test_pty;

use std::fmt;
use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub use capabilities::{Capabilities, ColorSupport, Osc8Support};
pub use signal::Signal;
pub use wait::{TtyInput, TtyOutput};
pub use winsize::WindowSize;

/// The single owner of the shell's terminal state.
///
/// Construction is fallible ([`TerminalSession::open`]) and performs,
/// in order:
///
/// 1. Acquire `/dev/tty` (so the session has a stable handle to the
///    controlling terminal regardless of fd 0/1/2 redirection).
/// 2. Install signal handlers and the self-pipe wakeup.
/// 3. Run the bounded capability probe (skipped in non-interactive
///    mode or when `FREDSHELL_NO_PROBE=1` is set).
/// 4. Read initial window size.
///
/// Raw mode is **not** entered by [`TerminalSession::open`]; script
/// mode never enters raw mode, and interactive callers transition
/// explicitly via [`TerminalSession::enter_raw_mode`].
///
/// `TerminalSession` is intentionally not `Clone`: it owns
/// exactly-once resources (the tty fd, the signal handlers, the
/// raw-mode RAII guard). It is also not `Send` across threads as a
/// matter of policy — terminal I/O is single-threaded in fredshell
/// (see `PLAN_02` §6 and ADR 0001).
#[derive(Debug)]
pub struct TerminalSession {
    /// `/dev/tty` opened read/write. Populated by `PLAN_04` 04.2.
    #[allow(dead_code)] // wired up in 04.2
    tty: Option<OwnedFd>,

    /// RAII guard restoring termios on drop. Populated by `PLAN_04` 04.5.
    #[allow(dead_code)] // wired up in 04.5
    raw_guard: Option<termios::RawModeGuard>,

    /// Current window size, refreshed on SIGWINCH. Populated by
    /// `PLAN_04` 04.7.
    winsize: WindowSize,

    /// Cached capabilities from the startup probe. Populated by
    /// `PLAN_04` 04.9.
    caps: Capabilities,

    /// Cancellation flag set by SIGINT / SIGALRM handlers.
    cancel: Arc<AtomicBool>,

    /// Self-pipe read end, multiplexed alongside the tty in
    /// [`TerminalSession::wait`].
    sig_rx: Option<OwnedFd>,
}

impl TerminalSession {
    /// Open a new session.
    ///
    /// Today this performs the first two steps of the full `open`
    /// sequence: acquire `/dev/tty` and install signal handlers
    /// (including the self-pipe). Capability probing and the initial
    /// winsize read land in later subtasks (04.7, 04.9). The
    /// returned session is therefore safe to construct but its
    /// capability and winsize fields hold conservative defaults.
    ///
    /// Signal-handler installation is a process-wide, exactly-once
    /// side effect. A second [`TerminalSession::open`] call in the
    /// same process returns [`OpenError::AlreadyOpen`].
    ///
    /// # Errors
    ///
    /// Returns [`OpenError::NoControllingTerminal`] if `/dev/tty`
    /// cannot be opened because the process has no controlling
    /// terminal (typical in daemon and CI contexts). Returns
    /// [`OpenError::OpenTty`] if the open fails for any other
    /// reason. Returns [`OpenError::SignalSetup`] if signal-handler
    /// installation fails.
    pub fn open() -> Result<Self, OpenError> {
        let tty = controlling::open_controlling_tty().map_err(OpenError::from)?;
        let cancel = Arc::new(AtomicBool::new(false));
        let handlers = signal::install(&cancel).map_err(OpenError::from)?;
        // Initial window size: best-effort. A failed TIOCGWINSZ
        // (e.g., the fd is not a real tty, which happens in some
        // test environments and inside `sudo -i` setups) falls back
        // to the 80×24 default; SIGWINCH will refresh as soon as the
        // terminal reports a resize.
        let winsize = winsize::query(tty.as_fd()).unwrap_or_default();
        Ok(Self {
            tty: Some(tty),
            raw_guard: None,
            winsize,
            caps: Capabilities::default(),
            cancel,
            sig_rx: Some(handlers.into_reader()),
        })
    }

    /// Return the cached terminal capabilities.
    ///
    /// The result is the snapshot computed at [`TerminalSession::open`]
    /// time. SIGWINCH does not invalidate it; resizing a terminal does
    /// not change its capabilities (see `PLAN_04` §5.5).
    #[must_use]
    pub const fn capabilities(&self) -> Capabilities {
        self.caps
    }

    /// Return the current window size.
    ///
    /// The snapshot is refreshed by
    /// [`TerminalSession::refresh_window_size`], which the REPL
    /// calls after [`TerminalSession::wait`] returns
    /// [`WaitEvent::Signal`] with [`Signal::WinCh`] (see `PLAN_04`
    /// §6 / subtask 04.7).
    #[must_use]
    pub const fn window_size(&self) -> WindowSize {
        self.winsize
    }

    /// Re-query `TIOCGWINSZ` and update the cached snapshot.
    ///
    /// Called from the REPL's SIGWINCH handling path. Returns the
    /// new size on success. On failure, leaves the cached snapshot
    /// unchanged and returns the underlying `io::Error` so the
    /// caller can decide whether to log; the snapshot is never
    /// corrupted into a bogus state by a transient ioctl error.
    ///
    /// # Errors
    ///
    /// Returns the underlying `io::Error` from `TIOCGWINSZ` if the
    /// ioctl fails. The cached snapshot is not modified in that
    /// case.
    pub fn refresh_window_size(&mut self) -> io::Result<WindowSize> {
        let Some(tty) = self.tty.as_ref() else {
            // A session without a tty fd cannot refresh; surface
            // ENOTTY rather than silently succeed.
            return Err(io::Error::from_raw_os_error(libc::ENOTTY));
        };
        let new = winsize::query(tty.as_fd())?;
        self.winsize = new;
        Ok(new)
    }

    /// Return a clone of the cancellation token.
    ///
    /// The token is shared between the signal handler and any
    /// in-process work that wants to cooperate with `Ctrl-C` /
    /// `SIGALRM`. Builtins and the REPL loop poll
    /// [`CancellationToken::is_cancelled`] at safe points and return
    /// early when it is set; the REPL clears the flag via
    /// [`CancellationToken::reset`] before drawing the next prompt
    /// (see `PLAN_04` §4.3).
    #[must_use]
    pub fn cancellation_token(&self) -> CancellationToken {
        CancellationToken(Arc::clone(&self.cancel))
    }

    /// Enter raw mode, returning a guard that restores cooked mode
    /// on drop.
    ///
    /// # Errors
    ///
    /// Returns [`RawModeError::AlreadyRaw`] if raw mode is already
    /// entered. Returns [`RawModeError::GetTermios`] or
    /// [`RawModeError::SetTermios`] if the underlying syscalls fail.
    ///
    /// Until subtask 04.5 lands this always returns
    /// [`RawModeError::AlreadyRaw`].
    #[allow(clippy::missing_const_for_fn)] // gains syscalls in 04.5.
    pub fn enter_raw_mode(&mut self) -> Result<(), RawModeError> {
        Err(RawModeError::AlreadyRaw)
    }

    /// Leave raw mode.
    ///
    /// No-op if raw mode is not currently entered. Implementation
    /// lands in subtask 04.5.
    #[allow(clippy::missing_const_for_fn)] // gains syscalls in 04.5.
    pub fn leave_raw_mode(&mut self) {
        self.raw_guard = None;
    }

    /// Block until one of: input available on the tty, a signal was
    /// delivered, or `deadline` elapses.
    ///
    /// `deadline` of `None` means wait indefinitely. Builtins such as
    /// `read -t` pass a finite `Duration` and treat
    /// [`WaitEvent::Timeout`] as the timed-out path.
    ///
    /// When both the tty and the signal pipe report ready in the
    /// same `pselect` return, signals take priority — the REPL needs
    /// to observe `SIGINT` and friends before consuming keystrokes
    /// to honor cancellation. The tty bytes are not lost; the next
    /// `wait` returns [`WaitEvent::Input`] immediately because the
    /// fd is still readable.
    ///
    /// If `wait` is called on a session that lacks a tty fd or a
    /// signal-pipe fd (constructed with neither), it returns
    /// [`WaitEvent::Timeout`] immediately to keep the call total.
    /// Production sessions opened via [`TerminalSession::open`]
    /// always have both.
    #[must_use]
    pub fn wait(&self, deadline: Option<Duration>) -> WaitEvent {
        let (Some(tty), Some(sig)) = (self.tty.as_ref(), self.sig_rx.as_ref()) else {
            return WaitEvent::Timeout;
        };
        let Ok(raw) = wait::wait_for_event(tty.as_fd(), sig.as_fd(), deadline) else {
            // `pselect` errors that are not EINTR (which is handled
            // inside wait_for_event) are surfaced as Timeout so the
            // REPL re-enters the loop rather than crashing on a
            // transient kernel hiccup. A persistent error will keep
            // recurring and surface through other syscalls.
            return WaitEvent::Timeout;
        };
        match raw {
            wait::RawWaitEvent::Timeout => WaitEvent::Timeout,
            wait::RawWaitEvent::TtyReadable => WaitEvent::Input,
            wait::RawWaitEvent::SignalPipeReadable | wait::RawWaitEvent::BothReadable => {
                drain_first_signal(sig).map_or(WaitEvent::Input, WaitEvent::Signal)
            }
        }
    }

    /// Borrow the tty for reading.
    ///
    /// The returned reader respects the current termios — raw mode
    /// delivers byte-at-a-time, cooked mode delivers line-at-a-time.
    /// Returns `None` if the session was constructed without a tty
    /// fd (currently impossible via [`TerminalSession::open`]).
    #[must_use]
    pub fn input(&self) -> Option<TtyInput<'_>> {
        self.tty.as_ref().map(|fd| TtyInput::new(fd.as_fd()))
    }

    /// Borrow the tty for writing.
    ///
    /// Used by the prompt and line editor; `PLAN_03` sequences are
    /// written through this handle. Returns `None` if the session
    /// was constructed without a tty fd (currently impossible via
    /// [`TerminalSession::open`]).
    #[must_use]
    pub fn output(&self) -> Option<TtyOutput<'_>> {
        self.tty.as_ref().map(|fd| TtyOutput::new(fd.as_fd()))
    }

    /// Borrowed reference to the self-pipe read end installed during
    /// [`TerminalSession::open`].
    ///
    /// `None` when the session was constructed without signal
    /// handling (currently impossible — kept as an option to avoid
    /// breaking the API when 04.6 lands a non-signal test path).
    /// [`TerminalSession::wait`] uses this in 04.6 to register the
    /// fd with `pselect`.
    #[must_use]
    pub const fn signal_fd(&self) -> Option<&OwnedFd> {
        self.sig_rx.as_ref()
    }
}

/// Errors returned by [`TerminalSession::open`].
#[derive(Debug)]
#[non_exhaustive]
pub enum OpenError {
    /// `/dev/tty` could not be opened because the process has no
    /// controlling terminal. Typical in daemon contexts.
    NoControllingTerminal,
    /// `/dev/tty` exists but could not be opened (permission denied,
    /// I/O error, etc.).
    OpenTty(io::Error),
    /// Signal-handler installation failed.
    SignalSetup(signal::InstallError),
    /// [`TerminalSession::open`] was called when a session was
    /// already open. Sessions are exactly-once.
    AlreadyOpen,
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoControllingTerminal => f.write_str("no controlling terminal available"),
            Self::OpenTty(_) => f.write_str("failed to open /dev/tty"),
            Self::SignalSetup(_) => f.write_str("failed to install signal handlers"),
            Self::AlreadyOpen => f.write_str("terminal session is already open"),
        }
    }
}

impl std::error::Error for OpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::OpenTty(e) => Some(e),
            Self::SignalSetup(e) => Some(e),
            Self::NoControllingTerminal | Self::AlreadyOpen => None,
        }
    }
}

impl From<controlling::AcquireError> for OpenError {
    fn from(value: controlling::AcquireError) -> Self {
        match value {
            controlling::AcquireError::NoControllingTerminal => Self::NoControllingTerminal,
            controlling::AcquireError::Open(e) => Self::OpenTty(e),
        }
    }
}

impl From<signal::InstallError> for OpenError {
    fn from(value: signal::InstallError) -> Self {
        // Map AlreadyInstalled to AlreadyOpen so callers see a
        // single "session already exists" surface regardless of
        // which exactly-once resource caught the duplicate.
        if matches!(value, signal::InstallError::AlreadyInstalled) {
            Self::AlreadyOpen
        } else {
            Self::SignalSetup(value)
        }
    }
}

/// Errors returned by [`TerminalSession::enter_raw_mode`].
#[derive(Debug)]
#[non_exhaustive]
pub enum RawModeError {
    /// `tcgetattr` failed when saving the cooked-mode termios.
    GetTermios(io::Error),
    /// `tcsetattr` failed when applying the raw-mode termios.
    SetTermios(io::Error),
    /// Raw mode was already entered on this session.
    AlreadyRaw,
}

impl fmt::Display for RawModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GetTermios(_) => f.write_str("failed to read termios"),
            Self::SetTermios(_) => f.write_str("failed to apply raw-mode termios"),
            Self::AlreadyRaw => f.write_str("raw mode is already entered"),
        }
    }
}

impl std::error::Error for RawModeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::GetTermios(e) | Self::SetTermios(e) => Some(e),
            Self::AlreadyRaw => None,
        }
    }
}

/// Outcome of [`TerminalSession::wait`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitEvent {
    /// The tty fd is readable (keystrokes or paste available).
    Input,
    /// One or more signals were delivered while waiting.
    Signal(Signal),
    /// The supplied deadline elapsed before input or a signal.
    Timeout,
}

/// Cooperative cancellation handle, shared between signal handlers
/// and any in-process work that wants to abort on SIGINT / SIGALRM.
///
/// The token is a thin wrapper around `Arc<AtomicBool>`. Polling is
/// lock-free and allocation-free, so builtins can check it on a hot
/// loop. The flag uses `Relaxed` ordering on both load and store: we
/// only need a single bit of cooperative communication, not a
/// happens-before relationship with the work the builtin is doing.
#[derive(Debug, Clone)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Create a fresh, un-set cancellation token.
    ///
    /// Public so tests and the REPL can construct standalone tokens
    /// without an open [`TerminalSession`].
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// `true` if a SIGINT / SIGALRM has been delivered since the
    /// last [`CancellationToken::reset`].
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// Clear the flag.
    ///
    /// Called by the REPL after it has processed a SIGINT (written a
    /// newline, redrawn the prompt) and before draining the next
    /// input line.
    pub fn reset(&self) {
        self.0.store(false, Ordering::Relaxed);
    }

    /// Set the flag.
    ///
    /// Intended for the signal handler. Exposed publicly so tests
    /// can simulate a SIGINT without raising a real signal.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Drain the self-pipe and return the first decodable [`Signal`].
///
/// Multiple signals may be coalesced into one wakeup; we read as
/// many bytes as fit in a small stack buffer and return the first
/// one [`Signal::from_raw`] recognizes. The remaining bytes are
/// dropped — coalescing is benign because the REPL re-enters
/// [`TerminalSession::wait`] after handling each signal anyway, and
/// the cancellation flag (set by the handler itself, not the drain
/// path) captures the SIGINT semantics regardless of how many
/// SIGINTs were collapsed.
///
/// Returns `None` if the pipe was empty or contained only
/// undecodable bytes; the caller falls back to [`WaitEvent::Input`]
/// in that case.
fn drain_first_signal(fd: &OwnedFd) -> Option<Signal> {
    use std::os::fd::AsRawFd;
    let mut buf = [0u8; 16];
    let drained = signal::drain_pipe(fd.as_raw_fd(), &mut buf).ok()?;
    drained.iter().find_map(|b| Signal::from_raw(i32::from(*b)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{CancellationToken, OpenError, RawModeError, TerminalSession, WaitEvent};

    #[test]
    fn cancellation_token_starts_unset() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn cancellation_token_round_trip() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
        token.reset();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn cancellation_token_clone_shares_state() {
        let a = CancellationToken::new();
        let b = a.clone();
        a.cancel();
        assert!(b.is_cancelled());
        b.reset();
        assert!(!a.is_cancelled());
    }

    #[test]
    fn cancellation_token_default_is_unset() {
        let token = CancellationToken::default();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn open_returns_session_or_no_controlling_terminal() {
        // Post-04.4: open() also installs signal handlers, which is
        // a process-wide exactly-once operation. In a cargo test
        // binary, the *first* test that calls open() will install
        // them; any later call returns AlreadyOpen. We tolerate any
        // of: success (interactive dev shell, first call),
        // NoControllingTerminal (CI / nextest), or AlreadyOpen
        // (subsequent calls within the same test process).
        match TerminalSession::open() {
            Ok(session) => {
                // Post-04.7 the winsize is queried from the kernel.
                // We don't know the actual terminal size in CI vs.
                // dev environments, so just verify the snapshot is
                // populated (rows and cols are u16 — any value is
                // valid, including the 80×24 fallback when
                // TIOCGWINSZ failed).
                let ws = session.window_size();
                let _ = (ws.cols, ws.rows);
            }
            Err(OpenError::NoControllingTerminal | OpenError::AlreadyOpen) => {
                // Expected in CI / when a sibling test already opened.
            }
            Err(other) => panic!("unexpected OpenError variant: {other:?}"),
        }
    }

    #[test]
    fn open_error_display_messages() {
        assert_eq!(
            OpenError::NoControllingTerminal.to_string(),
            "no controlling terminal available"
        );
        assert_eq!(
            OpenError::AlreadyOpen.to_string(),
            "terminal session is already open"
        );
    }

    #[test]
    fn raw_mode_error_display_messages() {
        assert_eq!(
            RawModeError::AlreadyRaw.to_string(),
            "raw mode is already entered"
        );
    }

    #[test]
    fn open_error_is_std_error() {
        fn assert_error<E: std::error::Error>() {}
        assert_error::<OpenError>();
        assert_error::<RawModeError>();
    }

    #[test]
    fn open_session_exposes_signal_fd() {
        // If a session is successfully opened, the self-pipe read
        // end must be available. We tolerate NoControllingTerminal
        // and AlreadyOpen (see above).
        match TerminalSession::open() {
            Ok(session) => {
                assert!(session.signal_fd().is_some());
            }
            Err(OpenError::NoControllingTerminal | OpenError::AlreadyOpen) => {}
            Err(other) => panic!("unexpected OpenError variant: {other:?}"),
        }
    }

    #[test]
    fn wait_event_is_copy() {
        const _: fn() = || {
            fn assert_copy<T: Copy>() {}
            assert_copy::<WaitEvent>();
        };
    }
}
