// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Built-in commands.
//!
//! Each builtin returns `Some(exit_status)` when it handled the line,
//! or `None` if the caller should fall through to external execution.
//!
//! Builtins operate on the [`ExecEnv`] passed by the dispatcher: they
//! read and mutate the shell's working directory (`env.cwd`) and
//! environment (`env.env`), and emit any diagnostics through the
//! `env.stderr` writer. They MUST NOT mutate global process state
//! (`std::env::set_current_dir`, `std::env::set_var`) or write to a
//! file descriptor directly (`eprintln!`): the embedding contract
//! (ADR 0006) requires the core to keep all state on the `ExecEnv`
//! and route output through the env writers, which the host renders.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::CoreResult;
use crate::exec::ExecEnv;

#[derive(Debug, Clone, Copy)]
pub enum BuiltinOutcome {
    /// Builtin handled the command; carry an exit status.
    Handled(i32),
    /// Builtin requested shell exit.
    Exit(i32),
}

/// Try to dispatch the command line to a builtin.
///
/// Returns `Ok(Some(outcome))` if a builtin handled the line, `Ok(None)`
/// if the caller should fall through to external execution.
///
/// Builtins receive `env` by mutable reference so they can read and
/// update the shell's working directory and environment and write
/// diagnostics to the env's `stderr` writer.
///
/// # Errors
///
/// Returns an error only if a builtin's underlying syscall fails in a
/// way that cannot be reported as a non-zero exit. Today no builtin
/// produces such errors; the signature reserves the slot for future
/// builtins (e.g. `read`, `wait`).
pub fn try_run(argv: &[String], env: &mut ExecEnv) -> CoreResult<Option<BuiltinOutcome>> {
    let Some(cmd) = argv.first() else {
        return Ok(None);
    };

    match cmd.as_str() {
        "exit" | "quit" => {
            let code = argv.get(1).and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
            Ok(Some(BuiltinOutcome::Exit(code)))
        }
        "cd" => Ok(Some(run_cd(argv, env))),
        _ => Ok(None),
    }
}

/// The `cd` builtin.
///
/// Resolves the target relative to the shell's current working
/// directory (`env.cwd`) and, on success, updates `env.cwd` to the
/// canonicalized destination. It does NOT call
/// `std::env::set_current_dir`: mutating the global process working
/// directory would corrupt every other component sharing the process
/// (notably the spec harness, which loads cases by relative path
/// between executions) and violates the ADR 0006 embedding contract.
///
/// With no argument it changes to `$HOME` (read from `env.env`, not
/// the process environment). On failure it writes a `cd: …`
/// diagnostic to the env's `stderr` writer and returns exit status 1.
fn run_cd(argv: &[String], env: &mut ExecEnv) -> BuiltinOutcome {
    let Some(target) = argv
        .get(1)
        .cloned()
        .or_else(|| env.env.get("HOME").cloned())
    else {
        // No argument and no $HOME: bash prints `cd: HOME not set`.
        let _ = writeln!(env.stderr, "cd: HOME not set");
        return BuiltinOutcome::Handled(1);
    };

    // Resolve relative targets against the shell's cwd, not the
    // process cwd.
    let candidate = {
        let p = Path::new(&target);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            env.cwd.join(p)
        }
    };

    match canonicalize_existing_dir(&candidate) {
        Ok(resolved) => {
            env.cwd = resolved;
            BuiltinOutcome::Handled(0)
        }
        Err(e) => {
            let _ = writeln!(env.stderr, "cd: {target}: {e}");
            BuiltinOutcome::Handled(1)
        }
    }
}

/// Canonicalize `path` and confirm it is a directory, returning a
/// typed error string suitable for the `cd:` diagnostic. Mirrors the
/// failure wording bash uses (`No such file or directory`,
/// `Not a directory`) by surfacing the OS error.
fn canonicalize_existing_dir(path: &Path) -> std::io::Result<PathBuf> {
    let resolved = std::fs::canonicalize(path)?;
    if resolved.is_dir() {
        Ok(resolved)
    } else {
        Err(std::io::Error::from(std::io::ErrorKind::NotADirectory))
    }
}
