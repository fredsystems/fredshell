// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Core shell primitives: builtins, command execution, REPL loop.
//!
//! `fredshell-core` is a library crate and therefore does **not**
//! depend on `anyhow`. Errors are exposed through typed enums whose
//! variants describe what went wrong, not what to do (per AGENTS.md
//! and ADR 0002's library-crate error policy).
//!
//! The top-level entry points return [`CoreResult`], which carries
//! [`CoreError`]. The binary entry point converts these into its own
//! `anyhow::Result` at the application boundary via `?`, because
//! [`CoreError`] implements [`std::error::Error`].

pub mod builtins;
pub mod exec;
pub mod parser;
pub mod repl;
pub mod tty;

pub use exec::builtin::{Tier2Builtin, Tier2Ctx, Tier2Error};
pub use exec::env::ExecEnv;
pub use exec::error::{ExecError, ExitStatus, RunError, RunResult};
pub use parser::{ParseError, ParseErrorKind, Script, parse};

use std::fmt;
use std::io;

/// Convenience alias for `Result<T, CoreError>`.
pub type CoreResult<T> = Result<T, CoreError>;

/// Top-level error type for `fredshell-core`.
///
/// Each variant describes a distinct failure surface in the crate.
/// Variants carry the underlying I/O error or sub-error so callers
/// can match on intent without losing detail.
#[derive(Debug)]
#[non_exhaustive]
pub enum CoreError {
    /// Failed to spawn `/bin/sh` for fallback execution.
    SpawnShell {
        /// The command string that was being executed.
        command: String,
        /// Underlying OS error from the spawn.
        source: io::Error,
    },
    /// An I/O error from the REPL's stdin/stdout handling.
    ReplIo(io::Error),
    /// A builtin reported a recoverable error that bubbled up rather
    /// than being converted to a non-zero exit status. Reserved for
    /// future builtins (e.g. `read`, `wait`) whose failures cannot
    /// be reduced to an exit code.
    Builtin(BuiltinError),
    /// Failed to open or operate the terminal session in interactive
    /// mode. The REPL falls back to cooked-stdin mode for the
    /// `NoControllingTerminal` variant; other variants are
    /// propagated.
    Terminal(tty::OpenError),
    /// `tcsetattr` or `tcgetattr` failed when entering raw mode.
    RawMode(tty::RawModeError),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpawnShell { command, .. } => {
                write!(f, "failed to invoke /bin/sh -c {command:?}")
            }
            Self::ReplIo(_) => f.write_str("REPL I/O error"),
            Self::Builtin(_) => f.write_str("builtin error"),
            Self::Terminal(_) => f.write_str("terminal session error"),
            Self::RawMode(_) => f.write_str("failed to enter raw mode"),
        }
    }
}

impl std::error::Error for CoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SpawnShell { source, .. } | Self::ReplIo(source) => Some(source),
            Self::Builtin(source) => Some(source),
            Self::Terminal(source) => Some(source),
            Self::RawMode(source) => Some(source),
        }
    }
}

/// Errors a builtin may surface to the REPL.
///
/// Today no builtin produces these; the type reserves the slot for
/// future builtins (e.g. `read`, `wait`) whose failure modes are
/// richer than a non-zero exit status.
#[derive(Debug)]
#[non_exhaustive]
pub enum BuiltinError {}

impl fmt::Display for BuiltinError {
    #[allow(clippy::uninhabited_references)] // Intentionally uninhabited today; variants land later.
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Currently uninhabited; no variants to format. When variants
        // are added (e.g. `read`, `wait`) this match becomes real.
        match *self {}
    }
}

impl std::error::Error for BuiltinError {}

/// Run a single command string and exit (like `bash -c "..."`).
///
/// For now we delegate to `/bin/sh -c` so we get full POSIX/bash
/// behaviour while the native parser is still in development.
///
/// # Errors
///
/// Returns [`CoreError::SpawnShell`] if `/bin/sh` cannot be spawned.
/// A non-zero exit from the spawned shell is **not** an error; this
/// function calls [`std::process::exit`] with the shell's code so
/// one-shot mode (`fredshell -c ...`) propagates it to the caller.
pub fn run_oneshot(command: &str) -> CoreResult<()> {
    exec::run_via_sh(command)
}
