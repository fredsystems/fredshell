// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! End-to-end smoke test for the 05.4 spec-runner: load the
//! hand-written `tests/spec/builtins_tier1/exit_zero.case.toml`
//! fixture from the repo root and assert the runner reports a pass.
//!
//! This is the only repo-rooted fixture wired up in 05.4; richer
//! corpus coverage lands in 05.9.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use fredshell_spec_runner::{Case, CaseOutcome, CaseVerdict, classify, run_case};

/// Resolve the workspace root from this crate's manifest directory.
/// `CARGO_MANIFEST_DIR` points at `<repo>/crates/fredshell-spec-runner`;
/// the workspace root is two levels up.
fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.pop(); // <repo>/
    p
}

#[test]
fn exit_zero_smoke_case_passes() {
    let case_path = workspace_root().join("tests/spec/builtins_tier1/exit_zero.case.toml");
    assert!(
        case_path.exists(),
        "smoke fixture missing at {}",
        case_path.display()
    );

    let case = Case::load(&case_path).expect("smoke case loads cleanly");
    assert_eq!(case.script, "exit 0\n");
    assert!(case.expected.stdout.is_empty());
    assert!(case.expected.stderr.is_empty());
    assert_eq!(case.expected.exit, 0);

    let result = run_case(&case).expect("smoke case runs without harness-level error");
    match result.outcome {
        CaseOutcome::Pass => {}
        CaseOutcome::Mismatch {
            observed_stdout,
            observed_stderr,
            observed_exit,
        } => panic!(
            "smoke case mismatch: exit={observed_exit}, stdout={:?}, stderr={:?}",
            String::from_utf8_lossy(&observed_stdout),
            String::from_utf8_lossy(&observed_stderr),
        ),
        CaseOutcome::ExecutorRefused { command, reason } => {
            panic!("smoke case refused: `{command}` ({reason})");
        }
        // `CaseOutcome` is `#[non_exhaustive]`; future variants are a
        // surprise the smoke test should fail loudly on.
        other => panic!("unexpected outcome: {other:?}"),
    }

    // 05.5: the `pass`-status smoke fixture must classify as
    // ExpectedPass under the taxonomy.
    let verdict = classify(&case.status, &result.outcome);
    assert_eq!(verdict, CaseVerdict::ExpectedPass);
    assert!(!verdict.is_ci_failure());
}
