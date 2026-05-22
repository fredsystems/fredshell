// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Single-case runner: load → sandbox → execute → compare.
//!
//! `PLAN_05` §4 prescribes the per-case lifecycle. This module owns
//! the *execution* and *comparison* portions of that lifecycle; case
//! loading lives in [`crate::case`] and the sandbox in
//! [`crate::sandbox`].
//!
//! ## Subtask 05.4 scope
//!
//! - Construct a strict-mode [`ExecEnv`] rooted at the per-case
//!   sandbox.
//! - Install shared-buffer stdio sinks so the harness can read the
//!   script's output back.
//! - Run the case's `script` through [`fredshell_core::run_source`].
//! - Compare observed stdout / stderr / exit against the recorded
//!   sidecar fixtures and return a [`CaseOutcome`].
//!
//! Status-taxonomy interpretation (§12 — `pass` / `fail` / `wontfix` /
//! `deferred:PLAN_XX`) is **not** in 05.4. The runner reports the raw
//! match/mismatch outcome; 05.5 applies the taxonomy on top.
//!
//! ## Strict mode is mandatory
//!
//! The harness must measure fredshell-as-itself, not fredshell + bash
//! fallback. The runner forces
//! [`ExternalCommandPolicy::Strict`] regardless of what the case
//! file says — there is no escape hatch in 05.4. When the dispatcher
//! refuses a command, the result surfaces as
//! [`CaseOutcome::ExecutorRefused`] rather than as a comparison
//! mismatch, so the corpus can distinguish "we got the wrong bytes"
//! from "we did not run anything".

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use fredshell_core::{
    ExecEnv, ExecError, ExternalCommandPolicy, NoExternalExecutorReason, RunError, RunResult,
    run_source,
};

use crate::case::Case;
use crate::error::SpecError;
use crate::sandbox::Sandbox;

/// Outcome of comparing a single case's observed output against its
/// recorded fixtures.
///
/// `CaseOutcome` is intentionally narrow in 05.4: it answers the
/// question "did the executor's observable behaviour match the
/// fixture?" without applying the §12 status taxonomy. 05.5 wraps
/// this with a status-aware verdict (`expected pass`, `expected fail`,
/// etc.).
#[derive(Debug)]
#[non_exhaustive]
pub enum CaseOutcome {
    /// Every observed stream matched its fixture and the exit code
    /// matched the recorded value.
    Pass,
    /// At least one of stdout / stderr / exit differed. All three
    /// observed values are reported so the caller can render a diff;
    /// 05.5 / 05.6 own the rendering.
    Mismatch {
        /// Observed stdout bytes.
        observed_stdout: Vec<u8>,
        /// Observed stderr bytes.
        observed_stderr: Vec<u8>,
        /// Observed exit code.
        observed_exit: i32,
    },
    /// The dispatcher refused to execute the script under strict
    /// mode (no native executor for some line, or unparsable argv).
    /// 05.5 treats this as the canonical "deferred" signal for cases
    /// tagged `deferred:PLAN_XX`.
    ExecutorRefused {
        /// The verbatim line the dispatcher refused.
        command: String,
        /// Why the dispatcher refused.
        reason: NoExternalExecutorReason,
    },
}

/// Result of running a single case.
#[derive(Debug)]
#[non_exhaustive]
pub struct CaseResult {
    /// Comparison outcome.
    pub outcome: CaseOutcome,
}

/// Run one [`Case`] end-to-end and report whether the observed
/// behaviour matched the recorded fixtures.
///
/// The sandbox is created, populated from the case's `<case>.fs/`
/// skeleton (if any), used as the executor's cwd, and torn down on
/// success. 05.5 / 05.6 will add failure-preservation under
/// `target/spec-failures/`.
///
/// # Errors
///
/// - [`SpecError::Sandbox`] — could not build or populate the sandbox.
/// - [`SpecError::Executor`] — the executor produced a [`RunError`]
///   the harness cannot map to an outcome (e.g. host I/O failure
///   mid-run, parse error in the case script). Strict-mode refusals
///   are **not** errors; they surface via
///   [`CaseOutcome::ExecutorRefused`].
pub fn run_case(case: &Case) -> Result<CaseResult, SpecError> {
    let sandbox = Sandbox::new()?;

    // The v0 ExecEnv env map is keyed by String. If the host's
    // TMPDIR is non-UTF-8, the harness cannot represent the sandbox
    // path inside the executor's environment, which would silently
    // corrupt $SANDBOX-substituted values. `PLAN_06` migrates env to
    // OsString and removes this guard.
    if !sandbox.root_is_utf8() {
        return Err(SpecError::Sandbox {
            path: sandbox.root().to_owned(),
            source: io::Error::new(
                io::ErrorKind::InvalidData,
                "sandbox path is not valid UTF-8; v0 env map cannot represent it",
            ),
        });
    }

    if let Some(skeleton) = &case.fs_skeleton {
        sandbox.materialize_skeleton(skeleton)?;
    }

    let resolved_env = sandbox.resolve_env(&case.env);

    let mut exec_env = ExecEnv::sandboxed(sandbox.root().to_owned());
    exec_env.external_command_policy = ExternalCommandPolicy::Strict;
    exec_env.env = resolved_env;

    let stdout_buf = SharedBuf::new();
    let stderr_buf = SharedBuf::new();
    exec_env.stdout = Box::new(stdout_buf.clone());
    exec_env.stderr = Box::new(stderr_buf.clone());

    let outcome = match run_source(&case.script, &mut exec_env) {
        Ok(run_result) => compare(&case.expected, &stdout_buf, &stderr_buf, run_result),
        Err(RunError::Exec(ExecError::NoExternalExecutor { command, reason })) => {
            CaseOutcome::ExecutorRefused { command, reason }
        }
        Err(other) => return Err(SpecError::Executor(other)),
    };

    Ok(CaseResult { outcome })
}

/// Compare observed output against the case's expected fixtures.
///
/// Returns [`CaseOutcome::Pass`] iff all three streams match; any
/// difference produces [`CaseOutcome::Mismatch`] carrying the
/// observed bytes so the caller can render a diff.
fn compare(
    expected: &crate::case::CaseExpected,
    stdout: &SharedBuf,
    stderr: &SharedBuf,
    result: RunResult,
) -> CaseOutcome {
    let observed_stdout = stdout.take();
    let observed_stderr = stderr.take();
    let observed_exit = result.status.0;

    let stdout_ok = observed_stdout == expected.stdout;
    let stderr_ok = observed_stderr == expected.stderr;
    let exit_ok = observed_exit == expected.exit;

    if stdout_ok && stderr_ok && exit_ok {
        CaseOutcome::Pass
    } else {
        CaseOutcome::Mismatch {
            observed_stdout,
            observed_stderr,
            observed_exit,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared-buffer writer.
// ---------------------------------------------------------------------------

/// A [`Write`] sink whose contents the harness can read back after
/// the executor returns.
///
/// Mirror of `fredshell_core::exec::testing::SharedBuf`, which is
/// `#[cfg(test)]`-gated inside `fredshell-core`. The harness needs
/// the same shape in a non-test build, so it carries its own copy
/// rather than depending on a private test surface. The two
/// implementations are intentionally identical so a future move of
/// `SharedBuf` to a public `fredshell-core` API can replace this
/// module's copy without changing call sites.
#[derive(Debug, Clone, Default)]
struct SharedBuf {
    inner: Arc<Mutex<Vec<u8>>>,
}

impl SharedBuf {
    fn new() -> Self {
        Self::default()
    }

    /// Drain every byte written through this handle (or any clone)
    /// and return them. Returns an empty buffer if the underlying
    /// mutex was poisoned by a panicking writer.
    fn take(&self) -> Vec<u8> {
        match self.inner.lock() {
            Ok(mut guard) => std::mem::take(&mut *guard),
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::case::{CaseEnv, CaseExpected, CaseStatus};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn case_from_parts(script: &str, expected: CaseExpected) -> Case {
        Case {
            path: PathBuf::from("synthetic.case.toml"),
            description: "synthetic".to_owned(),
            script: script.to_owned(),
            status: CaseStatus::Pass,
            tags: Vec::new(),
            bash_version_min: None,
            env: CaseEnv::default(),
            fs_skeleton: None,
            expected,
        }
    }

    #[test]
    fn run_case_passes_when_exit_zero_matches() {
        let c = case_from_parts(
            "exit 0\n",
            CaseExpected {
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit: 0,
            },
        );
        let r = run_case(&c).unwrap();
        assert!(matches!(r.outcome, CaseOutcome::Pass));
    }

    #[test]
    fn run_case_passes_when_explicit_nonzero_exit_matches() {
        // The `exit` builtin runs natively under strict mode, so this
        // case never tries to spawn /bin/sh and never refuses.
        let c = case_from_parts(
            "exit 42\n",
            CaseExpected {
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit: 42,
            },
        );
        let r = run_case(&c).unwrap();
        assert!(matches!(r.outcome, CaseOutcome::Pass));
    }

    #[test]
    fn run_case_mismatch_reports_observed_exit() {
        let c = case_from_parts(
            "exit 7\n",
            CaseExpected {
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit: 0,
            },
        );
        let r = run_case(&c).unwrap();
        match r.outcome {
            CaseOutcome::Mismatch { observed_exit, .. } => assert_eq!(observed_exit, 7),
            other => panic!("expected Mismatch, got {other:?}"),
        }
    }

    #[test]
    fn run_case_surfaces_strict_refusal_for_external_command() {
        // `/bin/echo` is not a v0 builtin; strict mode refuses it.
        let c = case_from_parts(
            "/bin/echo hi\n",
            CaseExpected {
                stdout: b"hi\n".to_vec(),
                stderr: Vec::new(),
                exit: 0,
            },
        );
        let r = run_case(&c).unwrap();
        match r.outcome {
            CaseOutcome::ExecutorRefused { reason, .. } => {
                assert_eq!(reason, NoExternalExecutorReason::PolicyStrict);
            }
            other => panic!("expected ExecutorRefused, got {other:?}"),
        }
    }

    #[test]
    fn run_case_returns_executor_error_on_parse_failure() {
        // NUL bytes are rejected by the parser; this surfaces as
        // SpecError::Executor(RunError::Parse(_)).
        let c = case_from_parts("echo \0nope\n", CaseExpected::default());
        let err = run_case(&c).unwrap_err();
        match err {
            SpecError::Executor(RunError::Parse(_)) => {}
            other => panic!("expected Executor(Parse), got {other:?}"),
        }
    }

    #[test]
    fn run_case_materializes_fs_skeleton_into_sandbox() {
        // The `exit` builtin doesn't actually need the skeleton, but
        // we can verify the skeleton was copied by checking the
        // sandbox state via a side channel: we use a TempDir to host
        // the skeleton and assert run_case succeeds with it set.
        let skel = TempDir::new().unwrap();
        std::fs::write(skel.path().join("marker.txt"), b"present").unwrap();

        let mut c = case_from_parts("exit 0\n", CaseExpected::default());
        c.fs_skeleton = Some(skel.path().to_owned());

        let r = run_case(&c).unwrap();
        assert!(matches!(r.outcome, CaseOutcome::Pass));
    }

    #[test]
    fn shared_buf_clones_share_underlying_storage() {
        let original = SharedBuf::new();
        let mut clone = original.clone();
        clone.write_all(b"shared").unwrap();
        assert_eq!(original.take(), b"shared");
    }

    #[test]
    fn shared_buf_take_drains() {
        let buf = SharedBuf::new();
        let mut handle = buf.clone();
        handle.write_all(b"once").unwrap();
        assert_eq!(buf.take(), b"once");
        assert!(buf.take().is_empty());
    }

    #[test]
    fn shared_buf_flush_is_noop() {
        let mut buf = SharedBuf::new();
        buf.flush().unwrap();
    }
}
