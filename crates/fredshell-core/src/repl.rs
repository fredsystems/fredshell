// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Interactive REPL loop.
//!
//! Currently a stub: reads lines from stdin, dispatches to builtins or
//! `/bin/sh -c`. Will be swapped for a `TerminalSession`-driven loop
//! once `PLAN_04` lands and `PLAN_07` (line editor) builds on top.

use crate::builtins::{self, BuiltinOutcome};
use crate::{CoreError, CoreResult, exec};

pub struct Options {
    pub login: bool,
}

/// Run the interactive REPL until EOF or an `exit` builtin.
///
/// # Errors
///
/// Returns [`CoreError::ReplIo`] if reading from stdin or writing the
/// prompt to stdout fails. Builtin and external-command failures are
/// reported to stderr and do not bubble up.
pub fn run(_opts: Options) -> CoreResult<()> {
    use std::io::{BufRead, Write};

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut line = String::new();

    loop {
        write!(stdout, "fredshell$ ").map_err(CoreError::ReplIo)?;
        stdout.flush().map_err(CoreError::ReplIo)?;

        line.clear();
        let n = stdin
            .lock()
            .read_line(&mut line)
            .map_err(CoreError::ReplIo)?;
        if n == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let argv: Vec<String> = match shell_words::split(trimmed) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("fredshell: parse error: {e}");
                continue;
            }
        };

        match builtins::try_run(&argv)? {
            Some(BuiltinOutcome::Exit(code)) => std::process::exit(code),
            Some(BuiltinOutcome::Handled(_)) => {}
            None => {
                if let Err(e) = exec::run_via_sh(trimmed) {
                    eprintln!("fredshell: {e}");
                }
            }
        }
    }

    Ok(())
}
