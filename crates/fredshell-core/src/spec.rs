// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Spec-sheet refusals (`PLAN_07` §5.2 / §5.3 / §8.2).
//!
//! When a builtin is asked to perform a behaviour a spec sheet
//! classifies as `wontfix` or `defer:N`, it does not silently ignore
//! the request and it does not write to a file descriptor — per
//! ADR 0006 the core never writes shell output to `stdout` / `stderr`.
//! Instead it constructs a typed [`Refusal`] and returns it up the
//! dispatch chain. The binary's REPL renders the refusal to the
//! terminal as a diagnostic (`ShellEvent::Diagnostic`, owned by
//! `PLAN_10`); the [`fmt::Display`] impl here produces the exact
//! wording `PLAN_07` §5.2 / §5.3 specify, so the renderer is a thin
//! pass-through and the wording is testable without a terminal.
//!
//! [`Refusal`] values are normally constructed by the
//! [`refuse!`](https://docs.rs) macro from the `fredshell-spec-macros`
//! crate, which validates the sheet id, row number, and
//! classification against the on-disk spec sheet at compile time. The
//! fields are public so the macro can build a value with a struct
//! literal, and so tests can construct refusals directly.

use core::fmt;

/// The exit status a refusal maps to: `POSIX` usage error (`PLAN_07`
/// §5.2).
///
/// Both `wontfix` and `defer` refusals use this status — the user
/// invoked a form the shell will not run, which is a usage error
/// regardless of whether the form is permanently or temporarily
/// unsupported.
pub const REFUSAL_EXIT_STATUS: i32 = 2;

/// Why a behaviour is refused.
///
/// The two variants mirror the two non-`support` classifications a
/// spec-sheet §3 row can carry (`PLAN_07` §5). `Wontfix` is permanent;
/// `Defer` is temporary and therefore carries the extra fields the
/// §5.3 message format requires.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RefusalKind {
    /// The behaviour will never be implemented (`PLAN_07` §5.2).
    Wontfix,
    /// The behaviour will be implemented after a milestone
    /// (`PLAN_07` §5.3).
    Defer {
        /// The `PLAN_16` milestone number after which the behaviour
        /// lands (the `N` in `defer:N`).
        milestone: String,
        /// The human-readable milestone name shown in parentheses in
        /// the §5.3 message.
        milestone_name: String,
        /// The best-effort workaround hint. Mandatory for `defer`
        /// rows (`PLAN_07` §5.3; enforced by `xtask check-specs`).
        workaround: String,
    },
}

/// A typed refusal to perform a spec-sheet behaviour.
///
/// Construct via the `refuse!` macro in normal code; the fields are
/// public so the macro and tests can build values directly. The
/// [`fmt::Display`] impl renders the `PLAN_07` §5.2 / §5.3 wording
/// verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The sheet id: the sheet filename without `.md` (e.g. `cd`).
    pub sheet_id: String,
    /// The §3 row number the refusal cites (e.g. `3.7`).
    pub row: String,
    /// A one-sentence present-tense summary of the refused behaviour,
    /// taken from the §3 row's Behaviour cell.
    pub summary: String,
    /// The workspace-relative path to the sheet, for the `See:` line
    /// (e.g. `Documents/specs/builtins/cd.md`).
    pub sheet_path: String,
    /// The sheet section the user should read (e.g. `3.7`). Rendered
    /// after `§` in the `See:` line.
    pub section: String,
    /// Permanent (`wontfix`) or temporary (`defer`).
    pub kind: RefusalKind,
}

impl Refusal {
    /// The exit status this refusal maps to ([`REFUSAL_EXIT_STATUS`]).
    #[must_use]
    pub const fn exit_status(&self) -> i32 {
        REFUSAL_EXIT_STATUS
    }
}

impl fmt::Display for Refusal {
    /// Render the refusal using the exact `PLAN_07` §5.2 (`wontfix`)
    /// or §5.3 (`defer`) message format.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            RefusalKind::Wontfix => {
                // PLAN_07 §5.2:
                // fredshell: <sheet-id>-<row#>: <summary>. See:
                //   <sheet-path> §<section>
                write!(
                    f,
                    "fredshell: {}-{}: {}. See:\n  {} §{}",
                    self.sheet_id, self.row, self.summary, self.sheet_path, self.section
                )
            }
            RefusalKind::Defer {
                milestone,
                milestone_name,
                workaround,
            } => {
                // PLAN_07 §5.3:
                // fredshell: <sheet-id>-<row#>: <summary>, deferred
                // to milestone <N> (<milestone-name>). <workaround>. See:
                //   <sheet-path> §<section>
                write!(
                    f,
                    "fredshell: {}-{}: {}, deferred to milestone {} ({}). {}. See:\n  {} §{}",
                    self.sheet_id,
                    self.row,
                    self.summary,
                    milestone,
                    milestone_name,
                    workaround,
                    self.sheet_path,
                    self.section
                )
            }
        }
    }
}

impl std::error::Error for Refusal {}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn wontfix_fixture() -> Refusal {
        Refusal {
            sheet_id: "cd".to_owned(),
            row: "3.7".to_owned(),
            summary:
                "option `-@` (extended attributes) is not supported and will not be implemented"
                    .to_owned(),
            sheet_path: "Documents/specs/builtins/cd.md".to_owned(),
            section: "3.7".to_owned(),
            kind: RefusalKind::Wontfix,
        }
    }

    fn defer_fixture() -> Refusal {
        Refusal {
            sheet_id: "cd".to_owned(),
            row: "3.9".to_owned(),
            summary: "option `-e`".to_owned(),
            sheet_path: "Documents/specs/builtins/cd.md".to_owned(),
            section: "3.9".to_owned(),
            kind: RefusalKind::Defer {
                milestone: "3".to_owned(),
                milestone_name: "filesystem-touch builtins".to_owned(),
                workaround: "Use `cd && ls` for now".to_owned(),
            },
        }
    }

    #[test]
    fn wontfix_renders_section_5_2_format() {
        let expected = "fredshell: cd-3.7: option `-@` (extended attributes) is not supported \
             and will not be implemented. See:\n  Documents/specs/builtins/cd.md §3.7";
        assert_eq!(wontfix_fixture().to_string(), expected);
    }

    #[test]
    fn defer_renders_section_5_3_format() {
        let expected = "fredshell: cd-3.9: option `-e`, deferred to milestone 3 \
             (filesystem-touch builtins). Use `cd && ls` for now. See:\n  \
             Documents/specs/builtins/cd.md §3.9";
        assert_eq!(defer_fixture().to_string(), expected);
    }

    #[test]
    fn refusal_exit_status_is_two() {
        assert_eq!(wontfix_fixture().exit_status(), REFUSAL_EXIT_STATUS);
        assert_eq!(defer_fixture().exit_status(), 2);
    }

    #[test]
    fn refusal_implements_std_error() {
        // Compile-time assertion that `Refusal` is usable as a boxed
        // `std::error::Error`, which the dispatch chain relies on.
        fn assert_error<E: std::error::Error>(_: &E) {}
        assert_error(&wontfix_fixture());
    }

    #[test]
    fn refusal_kind_is_equatable() {
        assert_eq!(RefusalKind::Wontfix, RefusalKind::Wontfix);
        assert_ne!(wontfix_fixture().kind, defer_fixture().kind);
    }
}
