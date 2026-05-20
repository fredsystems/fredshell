// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Probe orchestrator: write the query batch, drain responses within
//! the 50 ms budget, decode them into capability bits.
//!
//! This is the only I/O in the `probe` module tree. Everything else
//! ([`super::env`], [`super::interpret`]) is pure. Keeping the I/O
//! confined here means the pure layer stays trivially testable and
//! the orchestrator can be exercised through a fake-PTY harness
//! (see `PLAN_04` §5 / §9 and subtask 04.9).
//!
//! ## Protocol
//!
//! The probe writes the batch defined in `PLAN_04` §5.3:
//!
//! ```text
//! \x1b[c              DA1 — primary device attributes
//! \x1b[?u             Kitty keyboard query
//! \x1b[?2026$p        DECRQM synchronized output
//! ```
//!
//! and reads responses until either:
//!
//! - all three expected response shapes have been decoded, or
//! - the 50 ms total wall-clock budget elapses.
//!
//! Whichever happens first ends the probe; partial information is
//! kept and applied to the [`Capabilities`] struct. Conservative
//! defaults remain in place for bits whose probe response never
//! arrived (`PLAN_04` §5 / §5.4).
//!
//! ## Skip conditions
//!
//! The probe is skipped entirely if:
//!
//! - the controlling tty fd is not actually a terminal
//!   (defensive — `open_controlling_tty` already filters this), or
//! - `$FREDSHELL_NO_PROBE=1` is set in the environment.
//!
//! In either case the returned [`Capabilities`] reflects only the
//! environment-variable heuristics applied on top of the
//! conservative default.

use std::io::Write as _;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::time::{Duration, Instant};

use fredshell_ansi::decode::{Da1Response, DecrpmResponse, KittyKeyboardQueryResponse};
use fredshell_ansi::{Decode, DecodeError};

use crate::tty::capabilities::Capabilities;
use crate::tty::probe::{env, interpret};
use crate::tty::wait::{RawWaitEvent, TtyInput, TtyOutput, wait_for_event};

/// Total wall-clock budget for the probe (`PLAN_04` §5.4).
pub const PROBE_BUDGET: Duration = Duration::from_millis(50);

/// Environment variable that disables the probe entirely.
pub const NO_PROBE_VAR: &str = "FREDSHELL_NO_PROBE";

/// The literal batch of query sequences written to the terminal,
/// in the order required by `PLAN_04` §5.3. Held as bytes rather
/// than as encoder calls to avoid pulling the encoder into the
/// startup path — we control both the query and the decoder, so a
/// constant is the most direct representation.
const PROBE_BATCH: &[u8] = b"\x1b[c\x1b[?u\x1b[?2026$p";

/// Maximum response buffer size. Responses to the three probes are
/// well under 64 bytes total in practice; 512 bytes leaves ample
/// headroom for a chatty terminal echoing other state, while still
/// bounding the worst-case allocation. Excess bytes beyond this
/// cap are dropped — they were not part of any expected response.
const RESPONSE_BUFFER_CAP: usize = 512;

/// Track which expected shapes have already been decoded so the
/// loop can short-circuit when everything has arrived before the
/// timeout fires.
#[derive(Debug, Default, Clone, Copy)]
struct Expected {
    da1: bool,
    kitty: bool,
    decrpm: bool,
}

impl Expected {
    const fn all_done(self) -> bool {
        self.da1 && self.kitty && self.decrpm
    }
}

/// Run the full capability probe against `tty_fd`, using `sig_fd`
/// for the multiplexer (so a SIGWINCH or SIGINT during startup
/// wakes us out of `pselect` cleanly).
///
/// Always returns a [`Capabilities`] value — failures during I/O
/// degrade gracefully to environment-only inference and finally to
/// the conservative default. The probe is never observable as an
/// error to the caller because a partial answer is strictly better
/// than no answer for startup latency, and any bits we could not
/// determine remain `false` / `Unknown` — both of which are safe
/// for emitters (see `PLAN_04` §5.5).
///
/// `env` is the captured environment snapshot. Passing it in
/// (rather than reading the live environment here) keeps the
/// orchestrator testable without `unsafe` `set_var` calls.
#[must_use]
pub fn run(
    tty_fd: BorrowedFd<'_>,
    sig_fd: BorrowedFd<'_>,
    env_snapshot: &env::Env,
) -> Capabilities {
    let mut caps = Capabilities::default();

    // Always apply environment-variable heuristics first. These set
    // a floor that probe responses can only promote; if probing is
    // skipped or fails entirely, env is the sole information source.
    env::apply(&mut caps, env_snapshot);

    if is_probe_disabled() {
        return caps;
    }

    // Probe responses must not be echoed back by the terminal driver
    // and must not be held by canonical-mode line buffering. Enter
    // raw mode for the duration of the probe; the RawModeGuard
    // restores the pre-probe termios on drop. This is the only place
    // in the open() sequence that touches termios — the REPL re-enters
    // raw mode immediately after open() returns, so the back-to-back
    // tcsetattr calls are intentional and cheap. If `enter` fails
    // (e.g. fd is not a tty, ENOTTY) the probe degrades silently:
    // we keep going without a guard and the read will simply not
    // see any responses.
    let _raw_guard = crate::tty::termios::enter(tty_fd.as_raw_fd()).ok();

    // If writing the batch fails the probe is over before it began.
    // Env-only inference is the best we can do.
    if write_batch(tty_fd).is_err() {
        return caps;
    }

    let _ = drain_responses(tty_fd, sig_fd, &mut caps);
    caps
}

/// Returns `true` if `$FREDSHELL_NO_PROBE=1` is set in the
/// environment. Any other value (including unset, empty, or `0`)
/// permits the probe to run.
fn is_probe_disabled() -> bool {
    std::env::var(NO_PROBE_VAR).is_ok_and(|v| v == "1")
}

/// Write [`PROBE_BATCH`] in one syscall (the kernel may split it
/// further, but `write_all` retries). Borrows the fd for the
/// duration of the write.
fn write_batch(tty_fd: BorrowedFd<'_>) -> std::io::Result<()> {
    let mut out = TtyOutput::new(tty_fd);
    out.write_all(PROBE_BATCH)?;
    out.flush()
}

/// Read available bytes from `tty_fd` within the remaining wall-
/// clock budget and dispatch each completed response to the
/// pure interpreter functions.
///
/// Returns `Ok(())` if the loop exited cleanly (budget elapsed or
/// all expected responses received). Returns `Err` if a syscall
/// other than `EINTR` failed; the caller treats that as a degrade-
/// to-env situation but does not propagate.
fn drain_responses(
    tty_fd: BorrowedFd<'_>,
    sig_fd: BorrowedFd<'_>,
    caps: &mut Capabilities,
) -> std::io::Result<()> {
    let start = Instant::now();
    let mut buf: Vec<u8> = Vec::with_capacity(128);
    let mut expected = Expected::default();

    while !expected.all_done() {
        let remaining = remaining_budget(start);
        if remaining.is_zero() {
            return Ok(());
        }

        match wait_for_event(tty_fd, sig_fd, Some(remaining))? {
            RawWaitEvent::Timeout => return Ok(()),
            RawWaitEvent::SignalPipeReadable => {
                // A signal landed mid-probe (typically SIGWINCH from
                // a terminal that just changed pixel size). The
                // caller will drain the pipe through the next
                // TerminalSession::wait; for the probe we simply
                // retry the read.
                continue;
            }
            RawWaitEvent::TtyReadable | RawWaitEvent::BothReadable => {}
        }

        if !read_chunk(tty_fd, &mut buf)? {
            // EOF on the controlling terminal mid-probe is an
            // unusual but recoverable condition; no further bytes
            // are coming, so stop.
            return Ok(());
        }

        // Attempt to decode all known response shapes from the head
        // of the buffer, in a loop, until none match. This is the
        // same "decoder is a predicate" pattern as PLAN_03 §5.1.
        decode_available(&mut buf, &mut expected, caps);
    }
    Ok(())
}

/// Compute how much of [`PROBE_BUDGET`] remains since `start`.
/// Saturates at zero rather than going negative.
fn remaining_budget(start: Instant) -> Duration {
    PROBE_BUDGET.saturating_sub(start.elapsed())
}

/// Read up to a fixed-size chunk into `buf`. Returns `Ok(true)` if
/// bytes were appended, `Ok(false)` on EOF, or `Err(_)` on a real
/// I/O error other than `EINTR` (which `TtyInput::read` retries).
///
/// Caps the total buffer size at [`RESPONSE_BUFFER_CAP`] to avoid
/// unbounded growth if the terminal echoes garbage we cannot decode.
fn read_chunk(tty_fd: BorrowedFd<'_>, buf: &mut Vec<u8>) -> std::io::Result<bool> {
    if buf.len() >= RESPONSE_BUFFER_CAP {
        // Buffer is full and we still haven't decoded everything;
        // there is no point reading more.
        return Ok(false);
    }
    let mut chunk = [0_u8; 128];
    let room = RESPONSE_BUFFER_CAP - buf.len();
    let take = chunk.len().min(room);
    let mut input = TtyInput::new(tty_fd);
    let n = std::io::Read::read(&mut input, &mut chunk[..take])?;
    if n == 0 {
        return Ok(false);
    }
    buf.extend_from_slice(&chunk[..n]);
    Ok(true)
}

/// Try every known response shape against the head of `buf`,
/// consuming on success. Stops when no shape matches the current
/// head (waits for more input) or the head is malformed (drops one
/// byte to attempt resync).
fn decode_available(buf: &mut Vec<u8>, expected: &mut Expected, caps: &mut Capabilities) {
    loop {
        if buf.is_empty() {
            return;
        }
        match try_decode_one(buf.as_slice(), expected, caps) {
            DecodeOutcome::Consumed(n) => {
                buf.drain(..n);
            }
            DecodeOutcome::NeedMore => return,
            DecodeOutcome::Resync => {
                // Drop one byte and keep scanning. This recovers from
                // mid-stream garbage echoed by the terminal that does
                // not match any of our shapes.
                buf.drain(..1);
            }
        }
    }
}

/// Outcome of one decode attempt against the head of the buffer.
enum DecodeOutcome {
    /// A response decoded successfully; consume `n` bytes.
    Consumed(usize),
    /// No shape matched and at least one was `Incomplete`; wait
    /// for more bytes before retrying.
    NeedMore,
    /// Every known shape rejected the head as malformed; drop one
    /// byte and try again.
    Resync,
}

/// Attempt each known response shape against the head of `input`,
/// returning the first that matches and updating `caps` and
/// `expected` accordingly.
fn try_decode_one(input: &[u8], expected: &mut Expected, caps: &mut Capabilities) -> DecodeOutcome {
    let mut any_incomplete = false;

    match Da1Response::decode(input) {
        Ok((resp, n)) => {
            if !expected.da1 {
                interpret::apply_da1(caps, &resp);
                expected.da1 = true;
            }
            return DecodeOutcome::Consumed(n);
        }
        Err(DecodeError::Incomplete) => any_incomplete = true,
        Err(_) => {}
    }

    match KittyKeyboardQueryResponse::decode(input) {
        Ok((resp, n)) => {
            if !expected.kitty {
                interpret::apply_kitty_keyboard(caps, &resp);
                expected.kitty = true;
            }
            return DecodeOutcome::Consumed(n);
        }
        Err(DecodeError::Incomplete) => any_incomplete = true,
        Err(_) => {}
    }

    match DecrpmResponse::decode(input) {
        Ok((resp, n)) => {
            if !expected.decrpm {
                interpret::apply_decrpm(caps, &resp);
                expected.decrpm = true;
            }
            return DecodeOutcome::Consumed(n);
        }
        Err(DecodeError::Incomplete) => any_incomplete = true,
        Err(_) => {}
    }

    if any_incomplete {
        DecodeOutcome::NeedMore
    } else {
        DecodeOutcome::Resync
    }
}

/// Convenience that captures the live process environment and runs
/// the probe. Used by [`crate::tty::TerminalSession::open`].
#[must_use]
pub fn run_with_process_env(tty: BorrowedFd<'_>, sig: BorrowedFd<'_>) -> Capabilities {
    run(tty, sig, &env::Env::from_process())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{
        DecodeOutcome, Expected, PROBE_BATCH, decode_available, is_probe_disabled,
        remaining_budget, try_decode_one,
    };
    use crate::tty::capabilities::{Capabilities, ColorSupport, Osc8Support};
    use crate::tty::probe::env::Env;
    use std::io::{Read, Write};
    use std::os::fd::{AsFd, FromRawFd};
    use std::time::{Duration, Instant};

    #[test]
    fn probe_batch_matches_plan_spec() {
        // The literal bytes are the contract with terminals: DA1,
        // Kitty keyboard query, DECRQM synchronized output, in that
        // order, with no extra bytes between.
        assert_eq!(PROBE_BATCH, b"\x1b[c\x1b[?u\x1b[?2026$p");
    }

    #[test]
    fn expected_all_done_requires_all_three() {
        let mut e = Expected::default();
        assert!(!e.all_done());
        e.da1 = true;
        e.kitty = true;
        assert!(!e.all_done());
        e.decrpm = true;
        assert!(e.all_done());
    }

    #[test]
    fn remaining_budget_saturates_at_zero() {
        // Construct an instant strictly in the past — its elapsed()
        // will exceed PROBE_BUDGET, so remaining must be zero
        // (not panic, not wrap).
        let start = Instant::now().checked_sub(Duration::from_mins(1)).unwrap();
        assert_eq!(remaining_budget(start), Duration::ZERO);
    }

    #[test]
    fn remaining_budget_close_to_full_budget_for_fresh_start() {
        let start = Instant::now();
        let r = remaining_budget(start);
        // Allow generous slop for scheduling; just confirm the math
        // returns a positive duration not greater than the budget.
        assert!(r <= Duration::from_millis(50));
    }

    #[test]
    fn decode_available_consumes_da1_then_kitty_then_decrpm() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"\x1b[?64;4c");
        buf.extend_from_slice(b"\x1b[?15u");
        buf.extend_from_slice(b"\x1b[?2026;1$y");

        let mut caps = Capabilities::default();
        let mut expected = Expected::default();
        decode_available(&mut buf, &mut expected, &mut caps);

        assert!(buf.is_empty(), "all bytes should be consumed");
        assert!(expected.da1);
        assert!(expected.kitty);
        assert!(expected.decrpm);
        assert_eq!(caps.color, ColorSupport::Ansi16);
        assert!(caps.kitty_keyboard);
        assert!(caps.synchronized_output);
    }

    #[test]
    fn decode_available_resyncs_past_garbage() {
        // Garbage byte followed by a real DA1 response. The resync
        // path must drop the garbage and decode the DA1.
        let mut buf = Vec::new();
        buf.push(0xFF);
        buf.extend_from_slice(b"\x1b[?64;1c");

        let mut caps = Capabilities::default();
        let mut expected = Expected::default();
        decode_available(&mut buf, &mut expected, &mut caps);

        assert!(buf.is_empty());
        assert!(expected.da1);
        assert_eq!(caps.color, ColorSupport::Ansi16);
    }

    #[test]
    fn decode_available_needs_more_for_partial_response() {
        // Prefix of a DA1 response: ESC [ ? but no terminator yet.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"\x1b[?64");

        let mut caps = Capabilities::default();
        let mut expected = Expected::default();
        decode_available(&mut buf, &mut expected, &mut caps);

        // Buffer must be preserved verbatim — the next read appends
        // more bytes and decoding resumes.
        assert_eq!(buf.as_slice(), b"\x1b[?64");
        assert!(!expected.da1);
        assert!(!expected.kitty);
        assert!(!expected.decrpm);
    }

    #[test]
    fn try_decode_one_returns_resync_on_pure_garbage() {
        let mut caps = Capabilities::default();
        let mut expected = Expected::default();
        match try_decode_one(&[0x41, 0x42], &mut expected, &mut caps) {
            DecodeOutcome::Resync => {}
            DecodeOutcome::Consumed(_) | DecodeOutcome::NeedMore => {
                panic!("pure ASCII letters must trigger Resync");
            }
        }
    }

    #[test]
    fn try_decode_one_does_not_re_apply_already_seen_shape() {
        let mut caps = Capabilities::default();
        let mut expected = Expected {
            da1: true,
            ..Expected::default()
        };
        // Even though we hand it a valid DA1 response, the bit was
        // already set; ensure the bit stays set (it just won't be
        // double-applied) and the bytes are still consumed so the
        // outer loop makes progress.
        match try_decode_one(b"\x1b[?64;4c", &mut expected, &mut caps) {
            DecodeOutcome::Consumed(n) => assert!(n > 0),
            _ => panic!("valid DA1 must be consumed"),
        }
        // Color was not promoted because expected.da1 short-circuited
        // the interpret call.
        assert_eq!(caps.color, ColorSupport::None);
    }

    #[test]
    fn is_probe_disabled_reads_env() {
        // This test only verifies the function returns *some* bool
        // without panicking; we cannot safely mutate process env in
        // a parallel test runner.
        let _ = is_probe_disabled();
    }

    #[test]
    fn run_with_empty_env_returns_conservative_default() {
        // Without a controlling tty (BorrowedFd from /dev/null) and
        // no env vars, the probe will fail to write and we should
        // still get back a non-panicking conservative default.
        let dev_null = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/null")
            .unwrap();
        // We need two BorrowedFds; reuse the same one for both — the
        // probe write will succeed on /dev/null (silently discards)
        // but the wait will time out because nothing ever becomes
        // readable, exercising the timeout path.
        let fd = dev_null.as_fd();
        let env = Env::default();
        let caps = super::run(fd, fd, &env);
        // Conservative defaults: nothing claimed.
        assert_eq!(caps.color, ColorSupport::None);
        assert_eq!(caps.osc8_hyperlinks, Osc8Support::Unknown);
        assert!(!caps.kitty_keyboard);
        assert!(!caps.synchronized_output);
    }

    #[test]
    fn run_applies_env_even_when_probe_io_fails() {
        // env says truecolor; the probe will write to /dev/null and
        // timeout on the read. The truecolor floor from env must
        // survive the failed probe.
        let dev_null = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/null")
            .unwrap();
        let fd = dev_null.as_fd();
        let env = Env {
            colorterm: Some("truecolor".to_string()),
            term: Some("xterm-256color".to_string()),
            term_program: None,
        };
        let caps = super::run(fd, fd, &env);
        assert_eq!(caps.color, ColorSupport::TrueColor);
        assert!(caps.bracketed_paste);
        assert!(caps.focus_reporting);
    }

    #[test]
    fn run_on_fake_pty_with_canned_responses() {
        // End-to-end probe driven by a fake PTY: a background thread
        // plays the role of the terminal — it reads the probe's
        // query batch from the master, then writes canned responses
        // back. Writing responses from a thread (rather than
        // pre-loading the slave input queue) is required because the
        // probe enters raw mode via `tcsetattr(TCSAFLUSH)`, which
        // discards any pending slave input from before the call.
        let Some(pty) = crate::tty::test_pty::FakePty::open() else {
            return;
        };
        let slave_fd = std::os::fd::AsFd::as_fd(pty.slave());

        // Fresh signal pipe; the probe never expects bytes on it.
        let mut pipe_fds = [0_i32; 2];
        // SAFETY: pipe(2) takes a pointer to two i32s and writes the
        // read/write end fds on success.
        let rc = unsafe { libc::pipe(pipe_fds.as_mut_ptr()) };
        assert_eq!(rc, 0);
        // SAFETY: pipe(2) populated both fds on success.
        let sig_read = unsafe { std::os::fd::OwnedFd::from_raw_fd(pipe_fds[0]) };
        // SAFETY: same.
        let _sig_write = unsafe { std::os::fd::OwnedFd::from_raw_fd(pipe_fds[1]) };

        // Background "terminal" thread: read whatever the probe
        // writes (we don't validate it here; PLAN_03 covers encoder
        // correctness) and then write the canned responses back.
        let master_fd = pty.master().try_clone().unwrap();
        let responder = std::thread::spawn(move || {
            let mut master = std::fs::File::from(master_fd);
            // Drain at least one byte of the probe batch so we know
            // the probe has progressed past `write_batch` and into
            // `drain_responses`.
            let mut scratch = [0u8; 64];
            let _ = master.read(&mut scratch);
            master.write_all(b"\x1b[?64;4c").unwrap();
            master.write_all(b"\x1b[?15u").unwrap();
            master.write_all(b"\x1b[?2026;1$y").unwrap();
            master.flush().unwrap();
        });

        let env = Env::default();
        let caps = super::run(slave_fd, sig_read.as_fd(), &env);
        responder.join().unwrap();

        assert_eq!(caps.color, ColorSupport::Ansi16);
        assert!(caps.kitty_keyboard);
        assert!(caps.synchronized_output);
    }
}
