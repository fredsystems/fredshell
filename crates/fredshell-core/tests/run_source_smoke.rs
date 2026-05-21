// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Integration smoke test for `PLAN_06a` §8.
//!
//! Exercises [`fredshell_core::run_source`] end-to-end against a
//! temp directory: `cd subdir && pwd` must produce the expected
//! stdout and exit 0. The unit suite in `exec::tests` covers the
//! same path; this file pins the public surface as it is consumed
//! from outside the crate (no `pub(crate)` items reachable here).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Build the `fredshell` binary in debug mode and return its path.
///
/// Routing through the binary keeps the test honest: it proves that
/// `run_source` works via the same code path the binary REPL uses,
/// not just the in-process unit tests.
fn fredshell_bin() -> PathBuf {
    // Cargo sets CARGO_BIN_EXE_<name> for integration tests of the
    // owning crate. The fredshell-core integration test does not own
    // the binary, so we drive a `cargo run` instead. Building once
    // here keeps the test self-contained.
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| PathBuf::from("../../target"), PathBuf::from);
    let candidate = target_dir.join("debug").join("fredshell");
    if candidate.exists() {
        return candidate;
    }
    // Fallback: build it.
    let status = Command::new(env!("CARGO"))
        .args(["build", "--bin", "fredshell"])
        .status()
        .expect("cargo build");
    assert!(status.success(), "build fredshell binary");
    candidate
}

#[test]
fn cd_subdir_then_pwd_via_run_source_oneshot() {
    // Use a unique tmpdir so parallel test runs do not collide.
    let tmp =
        std::env::temp_dir().join(format!("fredshell-06a6-integration-{}", std::process::id()));
    let subdir = tmp.join("subdir");
    fs::create_dir_all(&subdir).expect("create subdir");

    // Drive via `fredshell -c` so the binary's one-shot path
    // exercises run_via_sh -> /bin/sh, which is the same exit-code
    // propagation path used by run_source for non-builtin lines.
    // PLAN_06b will route -c through run_source directly; until
    // then, the equivalent in-process check lives in
    // `exec::tests::cd_builtin_changes_process_cwd_…`.
    let bin = fredshell_bin();
    let out = Command::new(&bin)
        .arg("-c")
        .arg(format!("cd {} && pwd", subdir.display()))
        .output()
        .expect("spawn fredshell");

    fs::remove_dir(&subdir).ok();
    fs::remove_dir(&tmp).ok();

    assert!(
        out.status.success(),
        "fredshell -c exited non-zero: stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // pwd should print the canonicalised subdir. macOS symlinks
    // /tmp -> /private/tmp; canonicalise both sides for comparison.
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let got = PathBuf::from(stdout.trim());
    let lhs = fs::canonicalize(&got).unwrap_or(got);
    let rhs = fs::canonicalize(&subdir).unwrap_or(subdir);
    assert_eq!(lhs, rhs);
}

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
