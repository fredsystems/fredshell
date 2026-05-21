// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! `fredshell-spec-runner` CLI — thin debugging harness for one case
//! at a time.
//!
//! Production batch runs go through `cargo xtask compat` (added in
//! 05.6). This binary exists so a developer can inspect a single
//! `.case.toml` interactively:
//!
//! ```text
//! cargo run -p fredshell-spec-runner -- run tests/spec/.../foo.case.toml
//! ```
//!
//! Exit codes:
//!
//! - `0` — case loaded, executed, and matched its fixtures.
//! - `1` — case loaded and executed but did not match.
//! - `2` — case loaded but the dispatcher refused to execute it
//!   (strict-mode `NoExternalExecutor`). 05.5 will fold this into
//!   the taxonomy; today it is its own exit code so developers can
//!   see deferred refusals at a glance.
//! - `64` — case file or fixture failed to load (usage error,
//!   conventionally `EX_USAGE` from `sysexits.h`).
//! - `70` — executor produced an error the harness cannot map
//!   (`SpecError::Executor`; conventionally `EX_SOFTWARE`).

#![forbid(unsafe_code)]
// The binary owns its own error handling; the helper functions below
// return typed `Result` and `main` translates them to process exit
// codes. No `anyhow` (per AGENTS.md library-crate policy).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use fredshell_spec_runner::{Case, CaseOutcome, SpecError, run_case};

#[derive(Debug, Parser)]
#[command(
    name = "fredshell-spec-runner",
    about = "Single-case driver for the fredshell spec corpus (PLAN_05)",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Load and execute one `.case.toml` file.
    Run {
        /// Path to the `.case.toml` file.
        path: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Run { path } => run_one(&path),
    }
}

fn run_one(path: &std::path::Path) -> ExitCode {
    let case = match Case::load(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to load case: {e}");
            return ExitCode::from(64);
        }
    };

    let result = match run_case(&case) {
        Ok(r) => r,
        Err(SpecError::Load(e)) => {
            // Load errors should already have surfaced above; reaching
            // here means a sidecar failure inside run_case. Still a
            // usage error.
            eprintln!("error: sidecar load failure: {e}");
            return ExitCode::from(64);
        }
        Err(SpecError::Sandbox { path, source }) => {
            eprintln!("error: sandbox failure at {}: {source}", path.display());
            return ExitCode::from(70);
        }
        Err(SpecError::Executor(e)) => {
            eprintln!("error: executor failure: {e}");
            return ExitCode::from(70);
        }
        // `SpecError` is `#[non_exhaustive]`; future variants default
        // to the software-error exit code until they get explicit
        // handling.
        Err(other) => {
            eprintln!("error: {other}");
            return ExitCode::from(70);
        }
    };

    match result.outcome {
        CaseOutcome::Pass => {
            println!("pass: {}", path.display());
            ExitCode::SUCCESS
        }
        CaseOutcome::Mismatch {
            observed_stdout,
            observed_stderr,
            observed_exit,
        } => {
            println!("mismatch: {}", path.display());
            println!("  observed exit: {observed_exit}");
            println!(
                "  observed stdout ({} bytes): {}",
                observed_stdout.len(),
                String::from_utf8_lossy(&observed_stdout),
            );
            println!(
                "  observed stderr ({} bytes): {}",
                observed_stderr.len(),
                String::from_utf8_lossy(&observed_stderr),
            );
            ExitCode::from(1)
        }
        CaseOutcome::ExecutorRefused { command, reason } => {
            println!(
                "refused: {} — executor refused `{command}` ({reason})",
                path.display()
            );
            ExitCode::from(2)
        }
        // `CaseOutcome` is `#[non_exhaustive]`; treat unknown future
        // outcomes as software errors so the developer sees them.
        _ => {
            eprintln!("error: unknown case outcome");
            ExitCode::from(70)
        }
    }
}
