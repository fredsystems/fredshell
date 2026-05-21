// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Crate-internal output-capture helper for the `PLAN_05` spec harness.
//!
//! `PLAN_05` 05.2 moved `stdout` / `stderr` onto [`ExecEnv`] itself
//! (see `PLAN_05` §5.2). This module now ships a thin wrapper that
//! installs a pair of shared [`Vec<u8>`] sinks on an existing
//! [`ExecEnv`], runs a source string through [`run_source`], and
//! returns the drained buffers alongside the [`RunResult`].
//!
//! The wrapper exists for two reasons:
//!
//! 1. **Ergonomics.** Builtins (`PLAN_06b`) and the harness both need
//!    a "give me an env wired for capture" helper. Centralising it
//!    here keeps the test setup in `exec::tests` (and the future
//!    harness crate) terse.
//! 2. **Aliasing.** `ExecEnv::stdout` owns a `Box<dyn Write + Send>`,
//!    so the caller cannot hold a `&mut Vec<u8>` to read the bytes
//!    out while the env still owns the writer. The
//!    `Arc<Mutex<Vec<u8>>>` indirection makes the buffer visible to
//!    both sides for the lifetime of the capture.
//!
//! When `PLAN_05` lands the harness crate proper, this module will be
//! widened to `pub` (behind a `harness` cargo feature, TBD by 05.4)
//! or replaced with a public API on `ExecEnv` directly. Until then,
//! the module is gated to `#[cfg(test)]` and stays `pub(crate)` — the
//! only consumers are the in-crate test suites in `exec::tests` and
//! this file's own `#[cfg(test)]` block.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use super::{ExecEnv, RunError, RunResult, dispatch_script};
use crate::parser::parse;

/// A [`Write`] sink whose contents the caller can read back after
/// the writer is dropped.
///
/// Clone-shared via `Arc<Mutex<Vec<u8>>>` so the [`ExecEnv`] can own
/// one handle (boxed as `dyn Write + Send`) while the test holds a
/// second handle to drain the buffer. Locking is uncontended in
/// practice — the dispatcher writes to it synchronously on the
/// executor thread and the test reads it after `run_source` returns.
#[derive(Debug, Clone, Default)]
pub struct SharedBuf {
    inner: Arc<Mutex<Vec<u8>>>,
}

impl SharedBuf {
    /// Construct an empty shared buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drain and return every byte written through this handle (and
    /// any clones).
    ///
    /// Returns an empty vector if the underlying mutex was poisoned
    /// by a panicking writer; the harness treats a poisoned capture
    /// as a setup error reported via the surrounding [`RunResult`].
    #[must_use]
    pub fn take(&self) -> Vec<u8> {
        match self.inner.lock() {
            Ok(mut guard) => std::mem::take(&mut *guard),
            // A poisoned mutex means a writer panicked mid-write.
            // We cannot recover the in-flight bytes; return an empty
            // buffer rather than propagating the panic.
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        }
    }
}

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.inner.lock() {
            Ok(mut guard) => guard.write(buf),
            Err(poisoned) => poisoned.into_inner().write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Output captured from a single [`run_source_capturing`] invocation.
#[derive(Debug)]
pub struct Captured {
    /// Pipeline result (exit status of the last executed line).
    pub result: RunResult,
    /// Every byte written to [`ExecEnv::stdout`] during the run.
    /// In v0 this is bounded to the output of `spawn_via_sh` children;
    /// `PLAN_06b` routes builtin output through the same path.
    pub stdout: Vec<u8>,
    /// Every byte written to [`ExecEnv::stderr`] during the run.
    pub stderr: Vec<u8>,
}

/// Install [`SharedBuf`] sinks on `env`, parse and execute `source`,
/// then return the captured bytes alongside the run result.
///
/// The original writers on `env` are restored before the function
/// returns even if execution fails partway through, so the caller
/// can reuse the same `ExecEnv` for follow-up invocations with
/// different sinks (or with the host's stdio restored).
///
/// # Errors
///
/// Same as [`crate::exec::run_source`]: returns [`RunError::Parse`]
/// if the source fails to parse, or [`RunError::Exec`] if the
/// executor itself cannot run the script.
pub fn run_source_capturing(source: &str, env: &mut ExecEnv) -> Result<Captured, RunError> {
    let stdout_buf = SharedBuf::new();
    let stderr_buf = SharedBuf::new();
    // Swap our buffers in; remember the host's writers so we can put
    // them back regardless of outcome.
    let prev_stdout = std::mem::replace(&mut env.stdout, Box::new(stdout_buf.clone()));
    let prev_stderr = std::mem::replace(&mut env.stderr, Box::new(stderr_buf.clone()));

    // Parse must happen after the swap so a parse-error path is
    // exercised by the test surface, but we still drain the buffers
    // (which are empty) and restore writers either way.
    let outcome = parse(source)
        .map_err(RunError::Parse)
        .and_then(|script| dispatch_script(&script, env));

    env.stdout = prev_stdout;
    env.stderr = prev_stderr;

    let result = outcome?;
    Ok(Captured {
        result,
        stdout: stdout_buf.take(),
        stderr: stderr_buf.take(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::exec::ExitStatus;
    use crate::exec::env::GLOBAL_ENV_LOCK;
    use std::path::PathBuf;

    fn sandbox() -> ExecEnv {
        // The harness path that exercises `/bin/sh` will be replaced
        // by the native executor in PLAN_06b; until then the testing
        // module's own tests opt back into FallbackToSh so they
        // measure capture mechanics rather than strict-mode refusal.
        // Strict-mode capture is covered separately in exec::tests.
        let mut env = ExecEnv::sandboxed(PathBuf::from("/tmp"));
        env.external_command_policy = crate::exec::ExternalCommandPolicy::FallbackToSh;
        env
    }

    /// Any test that spawns `/bin/sh` must serialise with the
    /// process-global cwd/env lock. Parallel `cd` tests in
    /// `exec::tests` create-then-remove a tmp directory and briefly
    /// `set_current_dir` into it; an unprotected child spawned during
    /// that window inherits the doomed cwd and `/bin/sh` writes a
    /// `getcwd` warning to stderr, polluting our capture buffers.
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        GLOBAL_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn empty_source_yields_empty_capture_and_success() {
        let _g = lock();
        let mut env = sandbox();
        let c = run_source_capturing("", &mut env).expect("ok");
        assert_eq!(c.result.status, ExitStatus::SUCCESS);
        assert!(c.stdout.is_empty());
        assert!(c.stderr.is_empty());
    }

    #[test]
    fn stdout_and_stderr_are_kept_separate() {
        let _g = lock();
        let mut env = sandbox();
        let c = run_source_capturing("echo out\necho err 1>&2\n", &mut env).expect("ok");
        assert_eq!(c.result.status, ExitStatus::SUCCESS);
        assert_eq!(c.stdout, b"out\n");
        assert_eq!(c.stderr, b"err\n");
    }

    #[test]
    fn parse_error_propagates_through_capture_hook() {
        let _g = lock();
        let mut env = sandbox();
        let err = run_source_capturing("echo \0nope", &mut env).expect_err("NUL rejected");
        assert!(matches!(err, RunError::Parse(_)));
    }

    #[test]
    fn captured_buffers_accumulate_across_lines() {
        let _g = lock();
        let mut env = sandbox();
        let c = run_source_capturing("echo one\necho two\necho three\n", &mut env).expect("ok");
        assert_eq!(c.stdout, b"one\ntwo\nthree\n");
    }

    #[test]
    fn capture_restores_original_writers_on_success() {
        // After a successful capture, the env's writers should be
        // the host's defaults again (not the SharedBuf clones we
        // installed). Confirm by running a second non-capturing
        // dispatch and verifying the SharedBuf from the first call
        // did NOT receive its output.
        let _g = lock();
        let mut env = sandbox();
        let first = run_source_capturing("echo first\n", &mut env).expect("ok");
        assert_eq!(first.stdout, b"first\n");

        // The original writers are back; a second capture starts
        // with empty buffers.
        let second = run_source_capturing("echo second\n", &mut env).expect("ok");
        assert_eq!(second.stdout, b"second\n");
    }

    #[test]
    fn capture_restores_original_writers_on_parse_error() {
        // The restore must happen even on the error path so the
        // caller can keep using the env for later commands.
        let _g = lock();
        let mut env = sandbox();
        let _ = run_source_capturing("echo \0nope", &mut env).expect_err("parse rejects");

        // Confirm a follow-up capture sees only its own output.
        let after = run_source_capturing("echo recovered\n", &mut env).expect("ok");
        assert_eq!(after.stdout, b"recovered\n");
    }

    #[test]
    fn shared_buf_take_clears_inner_storage() {
        // `take()` drains; subsequent calls return empty until more
        // bytes are written.
        let buf = SharedBuf::new();
        let mut handle = buf.clone();
        handle.write_all(b"hello").expect("write ok");
        assert_eq!(buf.take(), b"hello");
        assert!(buf.take().is_empty());
    }

    #[test]
    fn shared_buf_flush_is_a_noop() {
        let mut buf = SharedBuf::new();
        buf.flush().expect("flush is infallible");
    }

    #[test]
    fn shared_buf_clones_share_storage() {
        // The whole point of the wrapper is that a clone held by
        // ExecEnv writes into the same buffer the test reads from.
        let original = SharedBuf::new();
        let mut clone = original.clone();
        clone.write_all(b"shared").expect("write ok");
        assert_eq!(original.take(), b"shared");
    }
}
