// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Command execution.
//!
//! Phase 1 strategy: shell out to `/bin/sh -c` for any non-builtin line.
//! Phase 2: parse simple `cmd arg1 arg2 | cmd2 > file` ourselves and
//! fork/exec directly, falling back to `/bin/sh -c` for unsupported syntax.

use std::process::Command;

use crate::{CoreError, CoreResult};

/// Execute a command string via `/bin/sh -c` and return its exit code.
///
/// # Errors
///
/// Returns [`CoreError::SpawnShell`] if `/bin/sh` cannot be spawned
/// (e.g. missing binary, permission denied). A non-zero exit from the
/// spawned shell is **not** an error here: the function calls
/// [`std::process::exit`] with the shell's exit code so one-shot mode
/// (`fredshell -c …`) propagates it.
pub fn run_via_sh(command: &str) -> CoreResult<()> {
    let status = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .status()
        .map_err(|source| CoreError::SpawnShell {
            command: command.to_owned(),
            source,
        })?;

    if !status.success() {
        // Propagate the exit code for one-shot mode (`fredshell -c ...`).
        if let Some(code) = status.code() {
            std::process::exit(code);
        }
    }
    Ok(())
}
