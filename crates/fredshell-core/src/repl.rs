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

use crate::builtins::{self, BuiltinOutcome};
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

/// Run the byte-pump loop using the session's tty + signal fds.
///
/// Split out so the same logic can be exercised in tests via
/// [`drive_raw_loop`] with a fake PTY.
fn drive_raw_loop_session(
    session: &mut TerminalSession,
    cancel: &crate::tty::CancellationToken,
) -> CoreResult<()> {
    loop {
        match session.wait(None) {
            WaitEvent::Input => {
                let Some(input) = session.input() else {
                    // No tty fd: session was constructed without one;
                    // bail out cleanly.
                    return Ok(());
                };
                let outcome =
                    pump_input_once(input, session.output()).map_err(CoreError::ReplIo)?;
                match outcome {
                    InputOutcome::Continue => {}
                    InputOutcome::LineSubmitted => {
                        write_prompt(session);
                    }
                    InputOutcome::Interrupted => {
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

/// Read one batch of bytes from `input` and echo them through
/// `output`. Handles the small set of control bytes the 04.10 loop
/// recognises:
///
/// | Byte         | Action                                           |
/// |--------------|--------------------------------------------------|
/// | `0x04` Ctrl-D| Returns [`InputOutcome::Eof`] (no echo).         |
/// | `0x03` Ctrl-C| Echoes `^C\r\n`, returns `Interrupted`.          |
/// | `\r` (0x0D)  | Echoes `\r\n`, returns `LineSubmitted`.          |
/// | `\n` (0x0A)  | Echoes `\r\n`, returns `LineSubmitted`.          |
/// | other        | Echoed verbatim; loop continues.                 |
///
/// Control bytes are checked in scan order — the first one found in
/// the batch determines the outcome and any later bytes in the same
/// `read` are discarded. This is a pragmatic compromise for the
/// 04.10 byte pump; `PLAN_07` replaces it with a real keystroke
/// decoder + line buffer.
///
/// Note: `cfmakeraw` clears `OPOST`, so the terminal driver does
/// not translate `\n` into `\r\n` on output. We emit CRLF explicitly
/// for both line-submit and Ctrl-C to keep the cursor in column 0
/// on the next row.
fn pump_input_once(
    mut input: TtyInput<'_>,
    output: Option<TtyOutput<'_>>,
) -> std::io::Result<InputOutcome> {
    let mut buf = [0u8; 64];
    let n = input.read(&mut buf)?;
    if n == 0 {
        return Ok(InputOutcome::Eof);
    }

    // Scan for a recognised control byte. The position determines
    // how many leading bytes we echo before the control action.
    let mut echo_end = n;
    let mut outcome = InputOutcome::Continue;
    for (i, b) in buf[..n].iter().enumerate() {
        match *b {
            0x04 => {
                // Ctrl-D: bytes before EOT are discarded (the byte
                // pump is not a line buffer; see 04.10 commit notes).
                return Ok(InputOutcome::Eof);
            }
            0x03 => {
                echo_end = i;
                outcome = InputOutcome::Interrupted;
                break;
            }
            b'\r' | b'\n' => {
                echo_end = i;
                outcome = InputOutcome::LineSubmitted;
                break;
            }
            _ => {}
        }
    }

    let Some(mut out) = output else {
        return Ok(outcome);
    };

    if echo_end > 0 {
        out.write_all(&buf[..echo_end])?;
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

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let argv: Vec<String> = match shell_words::split(trimmed) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("fredshell: parse error: {e}");
                continue;
            }
        };

        match builtins::try_run(&argv)? {
            Some(BuiltinOutcome::Exit(code)) => std::process::exit(code),
            Some(BuiltinOutcome::Handled(_)) => {}
            None => {
                if let Err(e) = exec::run_via_sh(trimmed) {
                    eprintln!("fredshell: {e}");
                }
            }
        }
    }

    Ok(())
}

/// Test-only entry point: drive the raw-mode byte pump against an
/// explicit tty fd and signal fd, without owning a full
/// [`TerminalSession`].
///
/// Returns when the input fd reports EOF or yields a Ctrl-D byte.
/// `Continue`, `LineSubmitted`, and `Interrupted` all keep looping.
#[cfg(test)]
fn drive_raw_loop(
    tty_fd: std::os::fd::BorrowedFd<'_>,
    _sig_fd: std::os::fd::BorrowedFd<'_>,
) -> std::io::Result<()> {
    loop {
        let input = TtyInput::new(tty_fd);
        let output = TtyOutput::new(tty_fd);
        match pump_input_once(input, Some(output))? {
            InputOutcome::Eof => return Ok(()),
            InputOutcome::Continue | InputOutcome::LineSubmitted | InputOutcome::Interrupted => {}
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
        let outcome = pump_input_once(input, Some(output)).unwrap();
        assert_eq!(outcome, InputOutcome::Continue);

        // Read the echoed bytes back from the master.
        let mut master_reader = std::fs::File::from(pty.master().try_clone().unwrap());
        let mut buf = [0u8; 16];
        let n = master_reader.read(&mut buf).unwrap();
        assert!(n >= 2, "expected at least 2 echoed bytes, got {n}");
        assert!(buf[..n].windows(2).any(|w| w == b"hi"));
    }

    #[test]
    fn pump_input_once_ctrl_d_returns_eof() {
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
        let outcome = pump_input_once(input, Some(output)).unwrap();
        assert_eq!(outcome, InputOutcome::Eof);
    }

    #[test]
    fn pump_input_once_ctrl_d_in_batch_returns_eof() {
        let Some(pty) = open_pty() else {
            return;
        };
        {
            let mut master_writer = std::fs::File::from(pty.master().try_clone().unwrap());
            // "ab" then Ctrl-D in the same batch — EOF wins, bytes
            // before EOT are intentionally discarded (the 04.10 loop
            // is a byte-pump, not a line buffer).
            master_writer.write_all(b"ab\x04").unwrap();
            master_writer.flush().unwrap();
        }

        let input = TtyInput::new(pty.slave().as_fd());
        let output = TtyOutput::new(pty.slave().as_fd());
        let outcome = pump_input_once(input, Some(output)).unwrap();
        assert_eq!(outcome, InputOutcome::Eof);
    }

    #[test]
    fn pump_input_once_ctrl_c_returns_interrupted() {
        let Some(pty) = open_pty() else {
            return;
        };
        {
            let mut master_writer = std::fs::File::from(pty.master().try_clone().unwrap());
            // 0x03 = ETX = Ctrl-C
            master_writer.write_all(&[0x03]).unwrap();
            master_writer.flush().unwrap();
        }

        let input = TtyInput::new(pty.slave().as_fd());
        let output = TtyOutput::new(pty.slave().as_fd());
        let outcome = pump_input_once(input, Some(output)).unwrap();
        assert_eq!(outcome, InputOutcome::Interrupted);

        // The pump echoes "^C\r\n" on Ctrl-C.
        let mut master_reader = std::fs::File::from(pty.master().try_clone().unwrap());
        let mut buf = [0u8; 16];
        let n = master_reader.read(&mut buf).unwrap();
        assert!(buf[..n].windows(4).any(|w| w == b"^C\r\n"));
    }

    #[test]
    fn pump_input_once_enter_returns_line_submitted() {
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
        let outcome = pump_input_once(input, Some(output)).unwrap();
        assert_eq!(outcome, InputOutcome::LineSubmitted);

        // Echoed bytes should include "hi" followed by an explicit
        // CRLF (OPOST is cleared by cfmakeraw).
        let mut master_reader = std::fs::File::from(pty.master().try_clone().unwrap());
        let mut buf = [0u8; 32];
        let n = master_reader.read(&mut buf).unwrap();
        assert!(buf[..n].windows(4).any(|w| w == b"hi\r\n"));
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
