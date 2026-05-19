//! Core shell primitives: builtins, command execution, REPL loop.

pub mod builtins;
pub mod exec;
pub mod repl;

use anyhow::Result;

/// Run a single command string and exit (like `bash -c "..."`).
///
/// For now we delegate to `/bin/sh -c` so we get full POSIX/bash behaviour
/// while the native parser is still in development.
pub fn run_oneshot(command: &str) -> Result<()> {
    exec::run_via_sh(command)
}
