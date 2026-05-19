//! Built-in commands.
//!
//! Each builtin returns `Some(exit_status)` when it handled the line,
//! or `None` if the caller should fall through to external execution.

use anyhow::Result;

#[derive(Debug, Clone, Copy)]
pub enum BuiltinOutcome {
    /// Builtin handled the command; carry an exit status.
    Handled(i32),
    /// Builtin requested shell exit.
    Exit(i32),
}

pub fn try_run(argv: &[String]) -> Result<Option<BuiltinOutcome>> {
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
