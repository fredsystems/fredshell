// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Result and error envelopes for the execution pipeline.
//!
//! These types form the public surface that `PLAN_05`'s spec harness
//! and the binary REPL both consume. See `PLAN_06a` §2.4.
//!
//! All envelopes are `#[non_exhaustive]` so `PLAN_06b` can add fields
//! and variants additively without breaking match exhaustiveness in
//! callers.

use std::fmt;
use std::io;

use crate::parser::ParseError;

/// Exit status returned by a successfully-executed script.
///
/// `0` means success, non-zero means the script itself signalled
/// failure (per POSIX). A non-zero exit is **not** a [`RunError`];
/// see the type-level note on [`RunError`] for the distinction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitStatus(pub i32);

impl ExitStatus {
    /// The canonical success status (`0`).
    pub const SUCCESS: Self = Self(0);

    /// Returns `true` if the status is `0`.
    #[must_use]
    pub const fn is_success(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exit {}", self.0)
    }
}

/// Outcome of a successful run.
///
/// "Successful" here means the executor itself did not fail. The
/// script may still have exited non-zero; see [`ExitStatus`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct RunResult {
    /// Final exit status of the script.
    pub status: ExitStatus,
}

impl RunResult {
    /// Construct a [`RunResult`] from an [`ExitStatus`].
    #[must_use]
    pub const fn new(status: ExitStatus) -> Self {
        Self { status }
    }
}

/// Error returned by the execution pipeline.
///
/// `RunError` describes failures **of the executor itself** — parse
/// errors, host I/O failures, executor invariant violations. A
/// script that exits non-zero is reported via [`RunResult::status`],
/// not via this enum. The harness classifies the two distinctly.
#[derive(Debug)]
#[non_exhaustive]
pub enum RunError {
    /// Parse-time failure. The source did not parse.
    Parse(ParseError),
    /// Runtime failure. The executor refused to run or aborted.
    Exec(ExecError),
}

/// Runtime failure produced by the executor.
///
/// These are categorical failures of the executor itself: a command
/// could not be found, the host's I/O machinery failed, the executor
/// reached a state it considers a bug. Script-level non-zero exit
/// codes are **not** represented here.
#[derive(Debug)]
#[non_exhaustive]
pub enum ExecError {
    /// A command (builtin or external) was not found.
    CommandNotFound {
        /// The command name as it appeared in the script.
        name: String,
    },
    /// The host's I/O streams or process machinery failed.
    HostIo(io::Error),
    /// The executor encountered a state it considers a bug.
    ///
    /// Never produced in normal operation; surfaced for tests.
    InternalInvariant {
        /// Short static description of the violated invariant.
        what: &'static str,
    },
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(_) => f.write_str("parse error"),
            Self::Exec(_) => f.write_str("execution error"),
        }
    }
}

impl std::error::Error for RunError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(source) => Some(source),
            Self::Exec(source) => Some(source),
        }
    }
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandNotFound { name } => write!(f, "command not found: {name}"),
            Self::HostIo(_) => f.write_str("host I/O failure"),
            Self::InternalInvariant { what } => write!(f, "internal invariant violated: {what}"),
        }
    }
}

impl std::error::Error for ExecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::HostIo(source) => Some(source),
            Self::CommandNotFound { .. } | Self::InternalInvariant { .. } => None,
        }
    }
}

impl From<ExecError> for RunError {
    fn from(err: ExecError) -> Self {
        Self::Exec(err)
    }
}

impl From<ParseError> for RunError {
    fn from(err: ParseError) -> Self {
        Self::Parse(err)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn exit_status_success_is_zero() {
        assert_eq!(ExitStatus::SUCCESS.0, 0);
        assert!(ExitStatus::SUCCESS.is_success());
        assert!(!ExitStatus(1).is_success());
        assert!(!ExitStatus(-1).is_success());
    }

    #[test]
    fn exit_status_display() {
        assert_eq!(format!("{}", ExitStatus(0)), "exit 0");
        assert_eq!(format!("{}", ExitStatus(127)), "exit 127");
        assert_eq!(format!("{}", ExitStatus(-1)), "exit -1");
    }

    #[test]
    fn run_result_new_round_trips() {
        let r = RunResult::new(ExitStatus(42));
        assert_eq!(r.status, ExitStatus(42));
    }

    #[test]
    fn run_result_is_copy() {
        // Compile-time check: RunResult is Copy.
        fn assert_copy<T: Copy>() {}
        assert_copy::<RunResult>();
        let r = RunResult::new(ExitStatus::SUCCESS);
        let r2 = r;
        let r3 = r;
        assert_eq!(r.status, r2.status);
        assert_eq!(r.status, r3.status);
    }

    #[test]
    fn exec_error_display_command_not_found() {
        let err = ExecError::CommandNotFound {
            name: "frobnicate".to_owned(),
        };
        assert_eq!(format!("{err}"), "command not found: frobnicate");
    }

    #[test]
    fn exec_error_display_host_io() {
        let err = ExecError::HostIo(io::Error::other("disk on fire"));
        assert_eq!(format!("{err}"), "host I/O failure");
        // Source chain is preserved.
        let source = std::error::Error::source(&err).expect("HostIo carries a source");
        assert_eq!(source.to_string(), "disk on fire");
    }

    #[test]
    fn exec_error_display_internal_invariant() {
        let err = ExecError::InternalInvariant {
            what: "pipe ends crossed",
        };
        assert_eq!(
            format!("{err}"),
            "internal invariant violated: pipe ends crossed"
        );
    }

    #[test]
    fn exec_error_source_is_none_for_non_io_variants() {
        let cnf = ExecError::CommandNotFound {
            name: "x".to_owned(),
        };
        assert!(std::error::Error::source(&cnf).is_none());
        let inv = ExecError::InternalInvariant { what: "x" };
        assert!(std::error::Error::source(&inv).is_none());
    }

    #[test]
    fn run_error_display() {
        let parse = RunError::Parse(ParseError {
            kind: crate::parser::ParseErrorKind::Unsupported,
            message: "boom".to_owned(),
        });
        let exec = RunError::Exec(ExecError::CommandNotFound {
            name: "x".to_owned(),
        });
        assert_eq!(format!("{parse}"), "parse error");
        assert_eq!(format!("{exec}"), "execution error");
    }

    #[test]
    fn run_error_source_chains_to_inner() {
        let exec = RunError::Exec(ExecError::CommandNotFound {
            name: "x".to_owned(),
        });
        let source = std::error::Error::source(&exec).expect("Exec carries a source");
        assert_eq!(source.to_string(), "command not found: x");

        let parse = RunError::Parse(ParseError {
            kind: crate::parser::ParseErrorKind::Unsupported,
            message: "syntax error at line 1".to_owned(),
        });
        let source = std::error::Error::source(&parse).expect("Parse carries a source");
        assert_eq!(source.to_string(), "unsupported: syntax error at line 1");
    }

    #[test]
    fn from_exec_error_for_run_error() {
        let exec = ExecError::CommandNotFound {
            name: "ls".to_owned(),
        };
        let run: RunError = exec.into();
        match run {
            RunError::Exec(ExecError::CommandNotFound { name }) => assert_eq!(name, "ls"),
            other => panic!("expected Exec(CommandNotFound), got {other:?}"),
        }
    }

    #[test]
    fn from_parse_error_for_run_error() {
        let p = ParseError {
            kind: crate::parser::ParseErrorKind::Unsupported,
            message: "nope".to_owned(),
        };
        let run: RunError = p.into();
        match run {
            RunError::Parse(inner) => assert_eq!(inner.message, "nope"),
            other => panic!("expected Parse, got {other:?}"),
        }
    }

    #[test]
    fn debug_impls_are_present() {
        // Smoke-test Debug derivations.
        let _ = format!("{:?}", RunResult::new(ExitStatus::SUCCESS));
        let _ = format!("{:?}", ExitStatus(0));
        let _ = format!("{:?}", ExecError::HostIo(io::Error::other("x")));
        let _ = format!(
            "{:?}",
            RunError::Exec(ExecError::InternalInvariant { what: "y" })
        );
        let _ = format!(
            "{:?}",
            ParseError {
                kind: crate::parser::ParseErrorKind::Unsupported,
                message: "z".to_owned()
            }
        );
    }
}
