// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Integration smoke tests for `PLAN_11` §8.
//!
//! Exercises the public [`fredshell_core::run_source`] surface as it
//! is consumed from outside the crate (no `pub(crate)` items
//! reachable here). The binary-driven `fredshell -c` smoke test lives
//! in the `fredshell` crate's `tests/oneshot.rs`, which owns the
//! binary and can resolve it via `CARGO_BIN_EXE_fredshell`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[test]
fn run_source_returns_non_zero_for_false() {
    // Direct in-process smoke: bypasses the binary, calls the
    // public API used by both the REPL and the harness. This test
    // measures the v0 fallback-to-sh path, so it opts in explicitly
    // (PLAN_05 §4.2 made `sandboxed()` default to strict).
    use fredshell_core::{ExecEnv, ExternalCommandPolicy, run_source};
    let mut env = ExecEnv::sandboxed(std::env::temp_dir());
    env.external_command_policy = ExternalCommandPolicy::FallbackToSh;
    let result = run_source("false", &mut env).expect("executes");
    assert!(!result.status.is_success());
    assert!(
        !result.exit_requested,
        "non-zero exit without `exit` builtin must not request termination"
    );
}

#[test]
fn run_source_strict_mode_refuses_external_command() {
    // Public-surface check for PLAN_05 §4.2: `sandboxed()`'s default
    // Strict policy makes the dispatcher refuse `/bin/sh` fallback
    // and surface `NoExternalExecutor` instead.
    use fredshell_core::{ExecEnv, ExecError, NoExternalExecutorReason, RunError, run_source};
    let mut env = ExecEnv::sandboxed(std::env::temp_dir());
    let err = run_source("false", &mut env).expect_err("strict refuses external");
    match err {
        RunError::Exec(ExecError::NoExternalExecutor { command, reason }) => {
            assert_eq!(command, "false");
            assert_eq!(reason, NoExternalExecutorReason::PolicyStrict);
        }
        other => panic!("expected NoExternalExecutor, got {other:?}"),
    }
}

#[test]
fn run_source_exit_builtin_sets_exit_requested() {
    use fredshell_core::{ExecEnv, run_source};
    let mut env = ExecEnv::sandboxed(std::env::temp_dir());
    let result = run_source("exit 7", &mut env).expect("executes");
    assert_eq!(result.status.0, 7);
    assert!(result.exit_requested);
}
