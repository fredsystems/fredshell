// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Stub parser for the v0 execution pipeline.
//!
//! `PLAN_06a` defers all real parsing to `PLAN_06b`. This module
//! exists so the public surface (`parse`, [`Script`], [`ParseError`],
//! [`ParseErrorKind`]) is stable for `PLAN_05`'s spec harness and the
//! binary REPL today. The v0 implementation accepts any input that
//! does not contain a NUL byte and stores it verbatim inside the
//! opaque [`Script`]; `PLAN_06b` replaces the body with a real
//! tokenizer + grammar without changing the signatures here.
//!
//! See `PLAN_06a` §2.1 for the contract and §3 for how the stub
//! dispatcher consumes a [`Script`].

use std::fmt;

/// An opaque parsed script.
///
/// `Script` deliberately does not expose tokens, AST nodes, or a
/// walker. The harness and the binary only need to be able to pass
/// it to `run_script`; `PLAN_06b` is free to replace the internal
/// representation (currently the raw source text) without breaking
/// either consumer.
#[derive(Debug, Clone)]
pub struct Script {
    // TODO(PLAN_06a.5): consumed by the stub dispatcher in `exec::mod`
    // once `run_source`/`run_script` are wired up. Allowed dead-code
    // until then per AGENTS.md "temporary refactor" exception.
    #[allow(dead_code)]
    pub(crate) source: String,
}

impl Script {
    /// v0 helper: returns the source the script was parsed from.
    ///
    /// Crate-internal because external callers must not depend on
    /// `Script` being source-shaped; `PLAN_06b` removes this.
    // TODO(PLAN_06a.5): used by the stub dispatcher; dead-code until
    // then per AGENTS.md "temporary refactor" exception.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn source(&self) -> &str {
        &self.source
    }
}

/// Reason a parse attempt failed.
///
/// v0 ships a single placeholder variant. `PLAN_06b` replaces this
/// enum with real categorical variants (`UnexpectedToken`,
/// `UnterminatedString`, `UnterminatedHeredoc`,
/// `InvalidParameterExpansion`, etc.). The enum is
/// `#[non_exhaustive]` so adding variants is non-breaking.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseErrorKind {
    /// The v0 stub does not support some construct in the input.
    ///
    /// Today the only thing that triggers this is a NUL byte in the
    /// source. `PLAN_06b` replaces the variant set entirely.
    Unsupported,
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("unsupported"),
        }
    }
}

/// Structured parse-time error.
///
/// Carries a [`ParseErrorKind`] and a human-readable message. v0
/// omits the byte-span field that `PLAN_02` §4.1 specifies because
/// no caller surfaces span information yet; `PLAN_06b` reintroduces
/// it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseError {
    /// Categorical reason for the failure.
    pub kind: ParseErrorKind,
    /// Human-readable description.
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parse a shell-language source string into an opaque [`Script`].
///
/// Pure function: no I/O, no global state, no environment access.
///
/// v0 behaviour: accepts any input without a NUL byte. NUL is
/// rejected because POSIX shell sources are NUL-terminated at the
/// C boundary and treating one as data has confused too many shells
/// to count.
///
/// # Errors
///
/// Returns [`ParseError`] with kind [`ParseErrorKind::Unsupported`]
/// if `source` contains a NUL byte. `PLAN_06b` replaces this body
/// with a real parser and a richer error set without changing the
/// signature.
pub fn parse(source: &str) -> Result<Script, ParseError> {
    if source.as_bytes().contains(&0) {
        return Err(ParseError {
            kind: ParseErrorKind::Unsupported,
            message: "NUL byte in source".to_owned(),
        });
    }
    Ok(Script {
        source: source.to_owned(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_empty_source() {
        let s = parse("").expect("empty source parses");
        assert_eq!(s.source(), "");
    }

    #[test]
    fn parse_accepts_trivial_command() {
        let s = parse("echo hi").expect("trivial command parses");
        assert_eq!(s.source(), "echo hi");
    }

    #[test]
    fn parse_accepts_multiline_source() {
        let src = "cd /tmp\necho one\necho two\n";
        let s = parse(src).expect("multiline parses");
        assert_eq!(s.source(), src);
    }

    #[test]
    fn parse_accepts_arbitrary_utf8() {
        // v0 is permissive; tokens, quoting, expansion are all
        // PLAN_06b's job. The stub just round-trips the bytes.
        let src = "ééé \"quoted $var\" | grep 'x' && echo done";
        let s = parse(src).expect("arbitrary source parses");
        assert_eq!(s.source(), src);
    }

    #[test]
    fn parse_rejects_nul_byte() {
        let src = "echo \0hidden";
        let err = parse(src).expect_err("NUL must be rejected");
        assert_eq!(err.kind, ParseErrorKind::Unsupported);
        assert!(err.message.contains("NUL"));
    }

    #[test]
    fn parse_rejects_leading_nul() {
        let err = parse("\0").expect_err("leading NUL must be rejected");
        assert_eq!(err.kind, ParseErrorKind::Unsupported);
    }

    #[test]
    fn parse_round_trips_via_source_accessor() {
        // Property: source-in, source-out via the crate-internal
        // accessor. Locks the v0 storage shape so the stub dispatcher
        // in 06a.5 has a stable contract.
        for src in ["", "echo hi", "set -e\ntrue\n", "\t   spaces\nand\ttabs"] {
            let s = parse(src).expect("parses");
            assert_eq!(s.source(), src);
        }
    }

    #[test]
    fn parse_error_display() {
        let err = ParseError {
            kind: ParseErrorKind::Unsupported,
            message: "boom".to_owned(),
        };
        assert_eq!(format!("{err}"), "unsupported: boom");
    }

    #[test]
    fn parse_error_implements_std_error() {
        let err = ParseError {
            kind: ParseErrorKind::Unsupported,
            message: "x".to_owned(),
        };
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn parse_error_kind_display() {
        assert_eq!(format!("{}", ParseErrorKind::Unsupported), "unsupported");
    }

    #[test]
    fn script_is_clone() {
        let s = parse("echo").expect("parses");
        let t = s.clone();
        assert_eq!(s.source(), t.source());
    }

    #[test]
    fn debug_impls_are_present() {
        let s = parse("x").expect("parses");
        let _ = format!("{s:?}");
        let _ = format!(
            "{:?}",
            ParseError {
                kind: ParseErrorKind::Unsupported,
                message: "y".to_owned(),
            }
        );
        let _ = format!("{:?}", ParseErrorKind::Unsupported);
    }
}
