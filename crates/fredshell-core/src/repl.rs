// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Interactive REPL loop.
//!
//! `run` is the entry point called from the binary. It tries to open
//! a [`TerminalSession`] and run the raw-mode interactive loop; when
//! the process has no controlling terminal (typical in CI, scripted
//! invocations, or `bash <fredshell <<'EOF'` constructs) it falls back
//! to a cooked-stdin loop that delegates to `/bin/sh -c` per line.
//!
//! ## Raw-mode loop (interactive)
//!
//! After entering raw mode, the loop multiplexes the tty fd and the
//! signal self-pipe via [`TerminalSession::wait`]. On a readable tty
//! it reads up to a small buffer of bytes and echoes them back; on
//! `Ctrl-D` (0x04) it exits. Signals are handled inline:
//! [`Signal::WinCh`] refreshes the cached window size,
//! [`Signal::Int`] resets the cancellation flag and writes a newline
//! marker, [`Signal::Hup`] and [`Signal::Term`] break the loop.
//!
//! The byte pump intentionally does NOT decode keystrokes or perform
//! line editing — that is `PLAN_07`'s responsibility. The current
//! loop is the minimum that lets a user see fredshell respond on a
//! real terminal and proves end-to-end wiring of `PLAN_04`.

use std::io::{Read, Write};

use crate::tty::{OpenError, Signal, TerminalSession, TtyInput, TtyOutput, WaitEvent};
use crate::{CoreError, CoreResult, exec};

/// Options threaded through from the binary CLI.
pub struct Options {
    /// Behave as a login shell. Currently consumed only by future
    /// startup-file logic; the REPL loop itself does not branch on it.
    pub login: bool,
}

/// Run the interactive REPL until EOF, `exit`, or a fatal signal.
///
/// Attempts the [`TerminalSession`]-driven raw-mode loop first. On
/// [`OpenError::NoControllingTerminal`] (no `/dev/tty` available)
/// falls back to the cooked-stdin loop so scripted invocations and
/// CI runs still work.
///
/// # Errors
///
/// Returns [`CoreError::Terminal`] for non-recoverable session-open
/// failures, [`CoreError::RawMode`] when raw-mode entry fails on a
/// session that did open, and [`CoreError::ReplIo`] for stdin/stdout
/// errors in the cooked fallback.
pub fn run(opts: &Options) -> CoreResult<()> {
    match TerminalSession::open() {
        Ok(session) => run_interactive(session),
        Err(OpenError::NoControllingTerminal) => run_cooked(opts),
        Err(other) => Err(CoreError::Terminal(other)),
    }
}

/// Interactive raw-mode loop on top of an open [`TerminalSession`].
fn run_interactive(mut session: TerminalSession) -> CoreResult<()> {
    session.enter_raw_mode().map_err(CoreError::RawMode)?;
    write_prompt(&session);

    let cancel = session.cancellation_token();
    drive_raw_loop_session(&mut session, &cancel)?;

    session.leave_raw_mode();
    Ok(())
}

/// Write the 04.10 placeholder prompt to the session's tty. Errors
/// are deliberately swallowed — if the terminal stops accepting
/// output the next `wait` will surface a `SIGHUP` and the REPL will
/// exit cleanly.
fn write_prompt(session: &TerminalSession) {
    if let Some(mut out) = session.output() {
        let _ = out.write_all(b"fredshell$ ");
        let _ = out.flush();
    }
}

/// Dispatch one assembled command line through the execution
/// pipeline. Shared between the raw-mode and cooked-mode loops so
/// feature parity is guaranteed.
///
/// Calls [`exec::run_source`] with a fresh [`exec::ExecEnv`] (the
/// per-line construction cost is one `current_dir` syscall and one
/// `vars_os` walk — well inside the `PLAN_06` §9 budget; `PLAN_06`
/// hoists the env into `ShellState` and reuses it). When the
/// returned [`exec::RunResult`] carries `exit_requested = true`
/// (the user typed the `exit` builtin), the process terminates via
/// [`std::process::exit`] with the requested status, matching the
/// pre-`PLAN_06` cooked-loop behaviour. A [`exec::RunError`] is
/// reported to stderr and the loop continues.
///
/// Empty / whitespace-only lines are handled by the dispatcher
/// itself (see `PLAN_06` §3), so this function does not pre-trim.
///
/// Infallible from the caller's perspective: any error encountered
/// is written to stderr and the loop carries on. The interactive
/// REPL must not abort on a single bad line.
fn dispatch_line(line: &str) {
    let mut env = match exec::ExecEnv::from_process() {
        Ok(env) => env,
        Err(e) => {
            eprintln!("fredshell: cannot construct exec env: {e}");
            return;
        }
    };

    match exec::run_source(line, &mut env) {
        Ok(result) => {
            if result.exit_requested {
                std::process::exit(result.status.0);
            }
            // PLAN_06 will store result.status in ShellState as $?;
            // for v0 we discard it after the line. The harness uses
            // its own code path and does not need this side channel.
        }
        Err(e) => {
            eprintln!("fredshell: {e}");
        }
    }
}

/// Run the byte-pump loop using the session's tty + signal fds.
///
/// Split out so the same logic can be exercised in tests via
/// [`drive_raw_loop`] with a fake PTY.
fn drive_raw_loop_session(
    session: &mut TerminalSession,
    cancel: &crate::tty::CancellationToken,
) -> CoreResult<()> {
    let mut line_buf: Vec<u8> = Vec::with_capacity(128);
    loop {
        match session.wait(None) {
            WaitEvent::Input => {
                let Some(input) = session.input() else {
                    // No tty fd: session was constructed without one;
                    // bail out cleanly.
                    return Ok(());
                };
                let outcome = pump_input_once(input, session.output(), &mut line_buf)
                    .map_err(CoreError::ReplIo)?;
                match outcome {
                    InputOutcome::Continue => {}
                    InputOutcome::LineSubmitted => {
                        // Hand the assembled line to dispatch. Child
                        // processes need cooked stdin and OPOST-on
                        // output, so drop raw mode for the duration
                        // of the command and re-enter afterwards. The
                        // RawModeGuard drops on leave_raw_mode and is
                        // re-created on enter_raw_mode; tcsetattr
                        // costs are negligible at human typing speed.
                        let line = String::from_utf8_lossy(&line_buf).into_owned();
                        line_buf.clear();
                        session.leave_raw_mode();
                        dispatch_line(&line);
                        session.enter_raw_mode().map_err(CoreError::RawMode)?;
                        write_prompt(session);
                    }
                    InputOutcome::Interrupted => {
                        line_buf.clear();
                        cancel.reset();
                        write_prompt(session);
                    }
                    InputOutcome::Eof => return Ok(()),
                }
            }
            WaitEvent::Signal(Signal::WinCh) => {
                // refresh_window_size returns the new size or an
                // io::Error; on error the cached snapshot is
                // unchanged and we keep going.
                let _ = session.refresh_window_size();
            }
            WaitEvent::Signal(Signal::Int) => {
                // Ctrl-C delivered as a real SIGINT (rare in raw mode
                // because ISIG is cleared, but kbd-driver shortcuts
                // or `kill -INT $$` can still raise it). Treat the
                // same way as the in-band 0x03 byte.
                line_buf.clear();
                cancel.reset();
                if let Some(mut out) = session.output() {
                    let _ = out.write_all(b"^C\r\n");
                    let _ = out.flush();
                }
                write_prompt(session);
            }
            WaitEvent::Signal(Signal::Hup | Signal::Term) => {
                // Controlling terminal hung up or graceful shutdown
                // requested — exit cleanly so RawModeGuard restores
                // cooked mode.
                return Ok(());
            }
            WaitEvent::Signal(_) | WaitEvent::Timeout => {
                // Other signals (Chld, Alrm, Usr1, Usr2) are not
                // actioned by the REPL loop itself in 04.10; PLAN_06
                // (job control) consumes Chld and the trap builtin
                // consumes the user-defined ones. Timeout is
                // impossible with `None` but the match is total.
            }
        }
    }
}

/// Outcome of [`pump_input_once`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputOutcome {
    /// Bytes were consumed (possibly zero); keep looping.
    Continue,
    /// User pressed Enter (CR / LF); the byte pump has already
    /// written CRLF and the caller should re-emit the prompt.
    LineSubmitted,
    /// User pressed Ctrl-C (0x03); the byte pump has already
    /// written `^C\r\n` and the caller should reset cancellation
    /// and re-emit the prompt.
    Interrupted,
    /// EOF was reached (Ctrl-D on an empty buffer, or `read(2)`
    /// returned 0). Exit the loop.
    Eof,
}

/// Read one batch of bytes from `input`, accumulate printable bytes
/// into `line_buf`, and echo them through `output`. Handles the
/// small set of control bytes the 04.10 loop recognises:
///
/// | Byte             | Action                                                |
/// |------------------|-------------------------------------------------------|
/// | `0x04` Ctrl-D    | EOF if `line_buf` is empty; otherwise no-op (no echo).|
/// | `0x03` Ctrl-C    | Clears `line_buf`, echoes `^C\r\n`, `Interrupted`.    |
/// | `\r` / `\n`      | Echoes `\r\n`, returns `LineSubmitted`.               |
/// | `0x7F` / `0x08`  | Erase last byte of `line_buf`; emit `\b \b` if any.   |
/// | other            | Appended to `line_buf`, echoed verbatim.              |
///
/// Control bytes are checked in scan order — the first one found
/// determines the outcome and any later bytes in the same `read` are
/// discarded. The 04.10 byte pump is intentionally minimal; `PLAN_07`
/// replaces it with a real keystroke decoder + line editor.
///
/// Note: `cfmakeraw` clears `OPOST`, so the terminal driver does
/// not translate `\n` into `\r\n` on output. We emit CRLF explicitly
/// for both line-submit and Ctrl-C to keep the cursor in column 0
/// on the next row.
fn pump_input_once(
    mut input: TtyInput<'_>,
    output: Option<TtyOutput<'_>>,
    line_buf: &mut Vec<u8>,
) -> std::io::Result<InputOutcome> {
    let mut buf = [0u8; 64];
    let n = input.read(&mut buf)?;
    if n == 0 {
        return Ok(InputOutcome::Eof);
    }

    // Walk the batch in order. Printable bytes are appended to the
    // line buffer and queued for echo; control bytes are handled
    // inline and may terminate the batch early.
    let mut echo: Vec<u8> = Vec::with_capacity(n);
    let mut outcome = InputOutcome::Continue;
    for &b in &buf[..n] {
        match b {
            0x04 => {
                // Ctrl-D: EOF only if the line buffer is empty
                // (matches bash). Otherwise the keystroke is
                // silently dropped — a real line editor would
                // forward-delete here; the 04.10 pump does not.
                if line_buf.is_empty() && echo.is_empty() {
                    flush_echo(output, &echo)?;
                    return Ok(InputOutcome::Eof);
                }
                break;
            }
            0x03 => {
                line_buf.clear();
                outcome = InputOutcome::Interrupted;
                break;
            }
            b'\r' | b'\n' => {
                outcome = InputOutcome::LineSubmitted;
                break;
            }
            0x7F | 0x08 => {
                // Backspace / DEL. Erase last byte of the buffer,
                // and emit `\b \b` to wipe the on-screen glyph.
                // If the buffer is empty (or only contains bytes
                // queued for echo this batch), nothing to erase.
                if line_buf.pop().is_some() {
                    echo.extend_from_slice(b"\x08 \x08");
                }
            }
            _ => {
                line_buf.push(b);
                echo.push(b);
            }
        }
    }

    let Some(mut out) = output else {
        return Ok(outcome);
    };

    if !echo.is_empty() {
        out.write_all(&echo)?;
    }
    match outcome {
        InputOutcome::Interrupted => {
            out.write_all(b"^C\r\n")?;
        }
        InputOutcome::LineSubmitted => {
            out.write_all(b"\r\n")?;
        }
        InputOutcome::Continue | InputOutcome::Eof => {}
    }
    out.flush()?;
    Ok(outcome)
}

/// Flush a queued echo buffer to `output` if one is provided.
/// Used by the Ctrl-D branch when we have already collected leading
/// echoable bytes that should still hit the screen before the loop
/// exits on EOF.
fn flush_echo(output: Option<TtyOutput<'_>>, echo: &[u8]) -> std::io::Result<()> {
    if let Some(mut out) = output
        && !echo.is_empty()
    {
        out.write_all(echo)?;
        out.flush()?;
    }
    Ok(())
}

/// Cooked-mode fallback loop, used when the process has no
/// controlling terminal. Preserves the pre-04.10 behavior.
fn run_cooked(_opts: &Options) -> CoreResult<()> {
    use std::io::BufRead;

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut line = String::new();

    loop {
        write!(stdout, "fredshell$ ").map_err(CoreError::ReplIo)?;
        stdout.flush().map_err(CoreError::ReplIo)?;

        line.clear();
        let n = stdin
            .lock()
            .read_line(&mut line)
            .map_err(CoreError::ReplIo)?;
        if n == 0 {
            break;
        }

        dispatch_line(&line);
    }

    Ok(())
}

/// Test-only entry point: drive the raw-mode byte pump against an
/// explicit tty fd and signal fd, without owning a full
/// [`TerminalSession`].
///
/// Returns when the input fd reports EOF.
/// `Continue`, `LineSubmitted`, and `Interrupted` all keep looping.
#[cfg(test)]
fn drive_raw_loop(
    tty_fd: std::os::fd::BorrowedFd<'_>,
    _sig_fd: std::os::fd::BorrowedFd<'_>,
) -> std::io::Result<()> {
    let mut line_buf: Vec<u8> = Vec::new();
    loop {
        let input = TtyInput::new(tty_fd);
        let output = TtyOutput::new(tty_fd);
        match pump_input_once(input, Some(output), &mut line_buf)? {
            InputOutcome::Eof => return Ok(()),
            InputOutcome::Continue | InputOutcome::Interrupted => {}
            InputOutcome::LineSubmitted => {
                // In the real loop dispatch_line would consume this;
                // tests just discard.
                line_buf.clear();
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{InputOutcome, drive_raw_loop, pump_input_once};
    use crate::tty::{TtyInput, TtyOutput};
    use std::io::{Read, Write};
    use std::os::fd::AsFd;

    /// Helper: open a fake PTY pair for raw-loop tests. Returns
    /// `None` when `openpty(3)` is unavailable (sandboxed CI), in
    /// which case the test silently passes.
    fn open_pty() -> Option<crate::tty::test_pty::FakePty> {
        crate::tty::test_pty::FakePty::open()
    }

    #[test]
    fn pump_input_once_echoes_bytes() {
        let Some(pty) = open_pty() else {
            return;
        };
        // Write a few bytes on the master so the slave read sees them.
        {
            let mut master_writer = std::fs::File::from(pty.master().try_clone().unwrap());
            master_writer.write_all(b"hi").unwrap();
            master_writer.flush().unwrap();
        }

        let input = TtyInput::new(pty.slave().as_fd());
        let output = TtyOutput::new(pty.slave().as_fd());
        let mut line_buf = Vec::new();
        let outcome = pump_input_once(input, Some(output), &mut line_buf).unwrap();
        assert_eq!(outcome, InputOutcome::Continue);
        assert_eq!(line_buf, b"hi");

        // Read the echoed bytes back from the master.
        let mut master_reader = std::fs::File::from(pty.master().try_clone().unwrap());
        let mut buf = [0u8; 16];
        let n = master_reader.read(&mut buf).unwrap();
        assert!(n >= 2, "expected at least 2 echoed bytes, got {n}");
        assert!(buf[..n].windows(2).any(|w| w == b"hi"));
    }

    #[test]
    fn pump_input_once_ctrl_d_on_empty_buffer_returns_eof() {
        let Some(pty) = open_pty() else {
            return;
        };
        {
            let mut master_writer = std::fs::File::from(pty.master().try_clone().unwrap());
            // 0x04 = EOT = Ctrl-D
            master_writer.write_all(&[0x04]).unwrap();
            master_writer.flush().unwrap();
        }

        let input = TtyInput::new(pty.slave().as_fd());
        let output = TtyOutput::new(pty.slave().as_fd());
        let mut line_buf = Vec::new();
        let outcome = pump_input_once(input, Some(output), &mut line_buf).unwrap();
        assert_eq!(outcome, InputOutcome::Eof);
    }

    #[test]
    fn pump_input_once_ctrl_d_with_pending_buffer_is_dropped() {
        // bash semantics: Ctrl-D on a non-empty line is silently
        // ignored (a real line editor would forward-delete; 04.10
        // drops it). The buffer survives so the user can keep typing.
        let Some(pty) = open_pty() else {
            return;
        };
        {
            let mut master_writer = std::fs::File::from(pty.master().try_clone().unwrap());
            master_writer.write_all(b"ab\x04").unwrap();
            master_writer.flush().unwrap();
        }

        let input = TtyInput::new(pty.slave().as_fd());
        let output = TtyOutput::new(pty.slave().as_fd());
        let mut line_buf = Vec::new();
        let outcome = pump_input_once(input, Some(output), &mut line_buf).unwrap();
        // Ctrl-D after "ab" terminates the batch but does NOT
        // return EOF — the buffer is non-empty.
        assert_ne!(outcome, InputOutcome::Eof);
        assert_eq!(line_buf, b"ab");
    }

    #[test]
    fn pump_input_once_ctrl_c_clears_buffer_and_returns_interrupted() {
        let Some(pty) = open_pty() else {
            return;
        };
        {
            let mut master_writer = std::fs::File::from(pty.master().try_clone().unwrap());
            // "abc" then Ctrl-C: buffer must be cleared.
            master_writer.write_all(b"abc\x03").unwrap();
            master_writer.flush().unwrap();
        }

        let input = TtyInput::new(pty.slave().as_fd());
        let output = TtyOutput::new(pty.slave().as_fd());
        let mut line_buf = Vec::new();
        let outcome = pump_input_once(input, Some(output), &mut line_buf).unwrap();
        assert_eq!(outcome, InputOutcome::Interrupted);
        assert!(line_buf.is_empty());

        // The pump echoes the leading "abc" then "^C\r\n".
        let mut master_reader = std::fs::File::from(pty.master().try_clone().unwrap());
        let mut buf = [0u8; 32];
        let n = master_reader.read(&mut buf).unwrap();
        assert!(buf[..n].windows(4).any(|w| w == b"^C\r\n"));
    }

    #[test]
    fn pump_input_once_enter_returns_line_submitted_with_buffered_text() {
        let Some(pty) = open_pty() else {
            return;
        };
        {
            let mut master_writer = std::fs::File::from(pty.master().try_clone().unwrap());
            master_writer.write_all(b"hi\r").unwrap();
            master_writer.flush().unwrap();
        }

        let input = TtyInput::new(pty.slave().as_fd());
        let output = TtyOutput::new(pty.slave().as_fd());
        let mut line_buf = Vec::new();
        let outcome = pump_input_once(input, Some(output), &mut line_buf).unwrap();
        assert_eq!(outcome, InputOutcome::LineSubmitted);
        assert_eq!(line_buf, b"hi");

        // Echoed bytes should include "hi" followed by an explicit
        // CRLF (OPOST is cleared by cfmakeraw).
        let mut master_reader = std::fs::File::from(pty.master().try_clone().unwrap());
        let mut buf = [0u8; 32];
        let n = master_reader.read(&mut buf).unwrap();
        assert!(buf[..n].windows(4).any(|w| w == b"hi\r\n"));
    }

    #[test]
    fn pump_input_once_backspace_erases_last_byte() {
        let Some(pty) = open_pty() else {
            return;
        };
        {
            let mut master_writer = std::fs::File::from(pty.master().try_clone().unwrap());
            // "abc" then DEL (0x7F) — buffer should end as "ab".
            master_writer.write_all(b"abc\x7f").unwrap();
            master_writer.flush().unwrap();
        }

        let input = TtyInput::new(pty.slave().as_fd());
        let output = TtyOutput::new(pty.slave().as_fd());
        let mut line_buf = Vec::new();
        let outcome = pump_input_once(input, Some(output), &mut line_buf).unwrap();
        assert_eq!(outcome, InputOutcome::Continue);
        assert_eq!(line_buf, b"ab");

        // Echo should contain "abc" followed by `\b \b`.
        let mut master_reader = std::fs::File::from(pty.master().try_clone().unwrap());
        let mut buf = [0u8; 32];
        let n = master_reader.read(&mut buf).unwrap();
        assert!(buf[..n].windows(3).any(|w| w == b"\x08 \x08"));
    }

    #[test]
    fn pump_input_once_backspace_on_empty_buffer_is_noop() {
        let Some(pty) = open_pty() else {
            return;
        };
        {
            let mut master_writer = std::fs::File::from(pty.master().try_clone().unwrap());
            master_writer.write_all(b"\x7f").unwrap();
            master_writer.flush().unwrap();
        }

        let input = TtyInput::new(pty.slave().as_fd());
        let output = TtyOutput::new(pty.slave().as_fd());
        let mut line_buf = Vec::new();
        let outcome = pump_input_once(input, Some(output), &mut line_buf).unwrap();
        assert_eq!(outcome, InputOutcome::Continue);
        assert!(line_buf.is_empty());
    }

    #[test]
    fn pump_input_once_buffer_accumulates_across_calls() {
        let Some(pty) = open_pty() else {
            return;
        };
        let mut master = std::fs::File::from(pty.master().try_clone().unwrap());
        let mut line_buf = Vec::new();

        master.write_all(b"foo").unwrap();
        master.flush().unwrap();
        let input = TtyInput::new(pty.slave().as_fd());
        let output = TtyOutput::new(pty.slave().as_fd());
        let _ = pump_input_once(input, Some(output), &mut line_buf).unwrap();
        assert_eq!(line_buf, b"foo");

        master.write_all(b"bar").unwrap();
        master.flush().unwrap();
        let input = TtyInput::new(pty.slave().as_fd());
        let output = TtyOutput::new(pty.slave().as_fd());
        let _ = pump_input_once(input, Some(output), &mut line_buf).unwrap();
        assert_eq!(line_buf, b"foobar");
    }

    #[test]
    fn drive_raw_loop_exits_on_ctrl_d() {
        let Some(pty) = open_pty() else {
            return;
        };
        {
            let mut master_writer = std::fs::File::from(pty.master().try_clone().unwrap());
            master_writer.write_all(&[0x04]).unwrap();
            master_writer.flush().unwrap();
        }

        // We pass the slave fd as both tty and "signal" fd; the test
        // helper does not actually select on the signal fd, so this
        // is just a stand-in for the signature.
        drive_raw_loop(pty.slave().as_fd(), pty.slave().as_fd()).unwrap();
    }
}
