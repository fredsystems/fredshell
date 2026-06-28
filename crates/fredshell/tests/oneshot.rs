// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Integration smoke test for the `fredshell` binary's one-shot
//! (`-c`) path.
//!
//! Drives `fredshell -c "cd subdir && pwd"` against a temp directory
//! and asserts the output and exit status. This test lives in the
//! `fredshell` crate (which owns the binary) so Cargo builds the
//! binary before the test runs and exposes it via
//! `CARGO_BIN_EXE_fredshell` through `assert_cmd::cargo_bin`. The
//! previous home in `fredshell-core` could not see that env var (the
//! core crate does not own the binary) and fell back to a nested
//! `cargo build` that raced the outer `cargo test --workspace`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use assert_cmd::cargo::CommandCargoExt;
use std::process::Command;

#[test]
fn cd_subdir_then_pwd_via_oneshot() {
    // Use a unique tmpdir so parallel test runs do not collide.
    let tmp = std::env::temp_dir().join(format!(
        "fredshell-oneshot-integration-{}",
        std::process::id()
    ));
    let subdir = tmp.join("subdir");
    fs::create_dir_all(&subdir).expect("create subdir");

    // Drive via `fredshell -c` so the binary's one-shot path
    // exercises run_via_sh -> /bin/sh, which is the same exit-code
    // propagation path used by run_source for non-builtin lines.
    let out = Command::cargo_bin("fredshell")
        .expect("locate fredshell binary")
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
