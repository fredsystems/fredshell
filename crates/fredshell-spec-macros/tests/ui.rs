// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Compile-fail tests for the `refuse!` macro (`PLAN_07` §8.2).
//!
//! These assert the compile-time guarantee: a `refuse!` that names a
//! missing sheet, a missing row, or a row whose classification does
//! not match the refusal form must fail to compile, with a
//! diagnostic pointing at the offending argument. `trybuild` compiles
//! each `tests/ui/*.rs` file as a standalone crate and compares the
//! emitted error against the committed `*.stderr` expectation.
//!
//! The fixture spec root is injected via `FREDSHELL_SPECS_ROOT` so the
//! macro resolves against `tests/specs/` rather than the workspace
//! sheets.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[test]
fn ui() {
    // Point the macro at the fixture sheet tree for the duration of
    // the trybuild compilations. trybuild inherits this process's
    // environment for the cargo subprocess it spawns.
    let fixture_root = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/specs");
    // SAFETY: single-threaded test entry; no other thread reads the
    // environment concurrently. set_var is the supported way to pass
    // configuration to the trybuild subprocess.
    unsafe {
        std::env::set_var("FREDSHELL_SPECS_ROOT", fixture_root);
    }

    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
