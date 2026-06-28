// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Integration tests for the `refuse!` macro's successful expansions.
//!
//! These exercise the happy path: a valid `refuse!` against the
//! fixture sheet at `tests/specs/builtins/cd.md` (the fixture root is
//! injected via `FREDSHELL_SPECS_ROOT`, emitted by `build.rs`). The
//! compile-fail paths (missing row, wrong classification, missing
//! sheet, bad form) are covered by the `trybuild` ui tests in
//! `tests/ui.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use fredshell_core::{Refusal, RefusalKind};
use fredshell_spec_macros::refuse;

#[test]
fn wontfix_expands_to_refusal_with_sheet_summary() {
    let r: Refusal = refuse!(wontfix, "cd", "3.7");
    assert_eq!(r.sheet_id, "cd");
    assert_eq!(r.row, "3.7");
    assert_eq!(
        r.summary,
        "option `-@` (extended attributes) is not supported and will not be implemented"
    );
    assert_eq!(r.sheet_path, "Documents/specs/builtins/cd.md");
    assert_eq!(r.section, "3.7");
    assert_eq!(r.kind, RefusalKind::Wontfix);
    assert_eq!(r.exit_status(), 2);
}

#[test]
fn wontfix_renders_section_5_2_message() {
    let r: Refusal = refuse!(wontfix, "cd", "3.7");
    let expected = "fredshell: cd-3.7: option `-@` (extended attributes) is not supported \
         and will not be implemented. See:\n  Documents/specs/builtins/cd.md §3.7";
    assert_eq!(r.to_string(), expected);
}

#[test]
fn defer_expands_with_milestone_from_sheet_and_named_args() {
    let r: Refusal = refuse!(
        defer,
        "cd",
        "3.9",
        milestone_name = "filesystem-touch builtins",
        workaround = "Use `cd && ls` for now"
    );
    assert_eq!(r.sheet_id, "cd");
    assert_eq!(r.row, "3.9");
    assert_eq!(r.summary, "option `-e`");
    match r.kind {
        RefusalKind::Defer {
            milestone,
            milestone_name,
            workaround,
        } => {
            assert_eq!(milestone, "3");
            assert_eq!(milestone_name, "filesystem-touch builtins");
            assert_eq!(workaround, "Use `cd && ls` for now");
        }
        other => panic!("expected Defer kind, got {other:?}"),
    }
}

#[test]
fn defer_renders_section_5_3_message() {
    let r: Refusal = refuse!(
        defer,
        "cd",
        "3.9",
        milestone_name = "filesystem-touch builtins",
        workaround = "Use `cd && ls` for now"
    );
    let expected = "fredshell: cd-3.9: option `-e`, deferred to milestone 3 \
         (filesystem-touch builtins). Use `cd && ls` for now. See:\n  \
         Documents/specs/builtins/cd.md §3.9";
    assert_eq!(r.to_string(), expected);
}

#[test]
fn defer_accepts_named_args_in_either_order() {
    let r: Refusal = refuse!(
        defer,
        "cd",
        "3.9",
        workaround = "Use `cd && ls` for now",
        milestone_name = "filesystem-touch builtins"
    );
    match r.kind {
        RefusalKind::Defer {
            milestone_name,
            workaround,
            ..
        } => {
            assert_eq!(milestone_name, "filesystem-touch builtins");
            assert_eq!(workaround, "Use `cd && ls` for now");
        }
        other => panic!("expected Defer kind, got {other:?}"),
    }
}
