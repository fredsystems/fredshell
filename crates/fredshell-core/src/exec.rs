// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Command execution.
//!
//! Phase 1 strategy: shell out to `/bin/sh -c` for any non-builtin line.
//! Phase 2: parse simple `cmd arg1 arg2 | cmd2 > file` ourselves and
//! fork/exec directly, falling back to `/bin/sh -c` for unsupported syntax.

use anyhow::{Context, Result};
use std::process::Command;

/// Execute a command string via `/bin/sh -c` and return its exit code.
pub fn run_via_sh(command: &str) -> Result<()> {
    let status = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .status()
        .with_context(|| format!("failed to invoke /bin/sh -c {command:?}"))?;

    if !status.success() {
        // Propagate the exit code for one-shot mode (`fredshell -c ...`).
        if let Some(code) = status.code() {
            std::process::exit(code);
        }
    }
    Ok(())
}
