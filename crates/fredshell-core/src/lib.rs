// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

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
