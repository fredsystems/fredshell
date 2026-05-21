// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Crate-internal output-capture hook for the `PLAN_05` spec harness.
//!
//! v0 does **not** plumb `stdin`/`stdout`/`stderr` through
//! [`crate::exec::ExecEnv`] (see `PLAN_06a` §6 and §7 for the
//! migration). The harness still needs *some* way to capture child
//! stdout/stderr to compare against expected fixtures, so 06a ships
//! this carve-out: a crate-internal `run_source_capturing` that runs
//! the stub dispatcher with [`crate::exec::Capture::Buffers`].
//!
//! When `PLAN_06b` lands real `Box<dyn Write>` plumbing on
//! [`crate::exec::ExecEnv`], this module is removed and the harness
//! constructs the env with its capture writers directly.
//!
//! The hook is `pub(crate)`: only code within `fredshell-core` may
//! use it. The harness lives at `crates/fredshell-harness/` (created
//! by `PLAN_05`) and will live in this crate's tree until extracted,
//! at which point `PLAN_05` will widen visibility to `pub` behind a
//! `harness` cargo feature or replace this hook entirely with the
//! `PLAN_06b` stdio plumbing.

use super::{Capture, ExecEnv, RunError, RunResult, dispatch_script};
use crate::parser::parse;

/// Output captured from a single `run_source_capturing` invocation.
// TODO(PLAN_05): consumed by the spec harness once it lands. The
// module is `pub(crate)`, so clippy prefers `pub` items inside it.
// Dead-code allow per AGENTS.md "temporary refactor" exception:
// reachable from tests until PLAN_05 wires the harness in.
#[allow(dead_code)]
#[derive(Debug)]
pub struct Captured {
    /// Pipeline result (exit status of the last executed line).
    pub result: RunResult,
    /// Aggregate stdout across every spawned `/bin/sh -c` child in
    /// the script. Builtin stdout (which v0 writes to the process's
    /// real stdout) is **not** captured here.
    pub stdout: Vec<u8>,
    /// Aggregate stderr across every spawned `/bin/sh -c` child in
    /// the script. Builtin stderr is **not** captured here.
    pub stderr: Vec<u8>,
}

/// Parse `source`, execute it with stdout/stderr captured into
/// buffers, and return the combined result.
///
/// Equivalent to [`crate::exec::run_source`] except that spawned
/// children have their stdout/stderr piped into the returned
/// [`Captured`] buffers instead of inheriting the host's stdio.
/// Builtin output is not captured in v0 — see the module docs.
///
/// # Errors
///
/// Same as [`crate::exec::run_source`]: returns [`RunError::Parse`]
/// if the source fails to parse, or [`RunError::Exec`] if the
/// executor itself cannot run the script.
// TODO(PLAN_05): see `Captured` above for the dead-code rationale.
#[allow(dead_code)]
pub fn run_source_capturing(source: &str, env: &mut ExecEnv) -> Result<Captured, RunError> {
    let script = parse(source)?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut capture = Capture::Buffers {
        stdout: &mut stdout,
        stderr: &mut stderr,
    };
    let result = dispatch_script(&script, env, &mut capture)?;
    Ok(Captured {
        result,
        stdout,
        stderr,
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
        // testing.rs's existing tests exercise the v0 fallback-to-sh
        // surface (`echo`, redirections, etc.). 05.1 made sandboxed()
        // default to Strict; 05.2 rewrites this module on top of the
        // new ExecEnv shape and these tests get redesigned then.
        // Until then, opt back into fallback so the existing assertions
        // continue to measure what 06a actually does.
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
}
