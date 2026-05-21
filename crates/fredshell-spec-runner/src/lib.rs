// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! `fredshell-spec-runner` — the L3 spec-corpus harness.
//!
//! See `Documents/PLAN_05_testing.md` for the architectural contract.
//! This crate implements the harness in two surfaces:
//!
//! * A library API ([`run_case`], [`Case`], [`CaseResult`]) used by
//!   `cargo xtask compat` (added in 05.6) and by in-crate unit tests.
//! * A thin binary (`fredshell-spec-runner`, `src/main.rs`) that
//!   exists for manual debugging — `cargo xtask compat` is the
//!   production driver.
//!
//! ## Subtask 05.4 scope
//!
//! 05.4 lands:
//!
//! * The `.case.toml` schema ([`Case`], [`CaseExpected`], etc.).
//! * A hermetic sandbox model ([`Sandbox`]).
//! * A single-case runner ([`run_case`]) that compares observed
//!   output against recorded fixtures using strict-mode execution.
//! * One hand-written minimal case at
//!   `tests/spec/builtins_tier1/exit_zero.case.toml` exercised by
//!   the unit suite.
//!
//! ## Subtask 05.5 scope
//!
//! 05.5 adds the case-status taxonomy on top of 05.4's raw outcomes:
//!
//! * [`CaseVerdict`] — interpret a [`CaseOutcome`] in light of the
//!   declared [`CaseStatus`] (`pass` / `fail` / `wontfix` /
//!   `deferred:PLAN_XX`).
//! * [`classify`] — total function mapping `(status, outcome) →
//!   verdict`.
//! * [`VerdictTally`] — per-status aggregation used by the harness
//!   binary and (in 05.6) by `cargo xtask compat`.
//! * `RECLASSIFY` is the §12.1 signal: emitted as
//!   [`CaseVerdict::Reclassify`] and accumulated in
//!   [`VerdictTally::reclassify`].
//!
//! ## Crate policy
//!
//! Per `AGENTS.md`, this is a **library crate** and does not depend on
//! `anyhow`. All errors are typed via [`SpecError`]. `unwrap` /
//! `expect` are forbidden outside `#[cfg(test)]`.

#![forbid(unsafe_code)]

pub mod case;
pub mod error;
pub mod runner;
pub mod sandbox;
pub mod verdict;

pub use case::{Case, CaseEnv, CaseExpected, CaseStatus};
pub use error::{LoadError, SpecError};
pub use runner::{CaseOutcome, CaseResult, run_case};
pub use sandbox::Sandbox;
pub use verdict::{CaseVerdict, ReclassifyReason, VerdictTally, classify};
