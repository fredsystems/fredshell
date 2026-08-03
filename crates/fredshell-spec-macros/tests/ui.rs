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
//!
//! That injection needs no code here, and in particular no
//! `std::env::set_var`: `build.rs` emits
//! `cargo:rustc-env=FREDSHELL_SPECS_ROOT=<manifest>/tests/specs`, cargo
//! puts that variable in this test binary's environment, and the
//! `cargo` subprocess `trybuild` spawns — and in turn the `rustc` that
//! expands `refuse!` — inherit it. A `set_var` call would be both
//! redundant and, since edition 2024, `unsafe`, which AGENTS.md
//! forbids. `find_specs_root` falls back to ascending from
//! `CARGO_MANIFEST_DIR` when the override is unset *or does not name a
//! directory*, so deleting `tests/specs/` silently retargets these
//! tests at the real sheets — if these expectations start failing for
//! no obvious reason, check that the fixture tree still exists.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
