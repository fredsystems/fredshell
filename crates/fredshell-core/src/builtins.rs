// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Built-in commands.
//!
//! Each builtin returns `Some(exit_status)` when it handled the line,
//! or `None` if the caller should fall through to external execution.

use crate::CoreResult;

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
/// # Errors
///
/// Returns an error only if a builtin's underlying syscall fails in a
/// way that cannot be reported as a non-zero exit. Today no builtin
/// produces such errors; the signature reserves the slot for future
/// builtins (e.g. `read`, `wait`).
pub fn try_run(argv: &[String]) -> CoreResult<Option<BuiltinOutcome>> {
    let Some(cmd) = argv.first() else {
        return Ok(None);
    };

    match cmd.as_str() {
        "exit" | "quit" => {
            let code = argv.get(1).and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
            Ok(Some(BuiltinOutcome::Exit(code)))
        }
        "cd" => {
            let target = argv
                .get(1)
                .cloned()
                .or_else(|| std::env::var("HOME").ok())
                .unwrap_or_else(|| ".".to_string());
            match std::env::set_current_dir(&target) {
                Ok(()) => Ok(Some(BuiltinOutcome::Handled(0))),
                Err(e) => {
                    eprintln!("cd: {target}: {e}");
                    Ok(Some(BuiltinOutcome::Handled(1)))
                }
            }
        }
        _ => Ok(None),
    }
}
