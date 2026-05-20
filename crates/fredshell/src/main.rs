// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! fredshell binary entrypoint.
//!
//! This is a *very* early skeleton. The MVP plan is:
//!   1. Reedline-driven REPL with a baked-in starship-style prompt.
//!   2. Builtins: cd, exit, export, alias, history.
//!   3. External commands via fork/exec on Unix.
//!   4. Bash fallback (`bash -c`) for anything we can't yet parse.
//!   5. Layer on: fzf-style history, lsd builtin, AI helpers.

use anyhow::Result;
use clap::Parser;

/// fredshell — an opinionated Rust shell.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Run a single command and exit (like `bash -c`).
    #[arg(short = 'c', long = "command")]
    command: Option<String>,

    /// Behave as a login shell.
    #[arg(short = 'l', long = "login")]
    login: bool,

    #[command(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,
}

fn main() -> Result<()> {
    color_eyre::install().ok();
    let cli = Cli::parse();
    init_tracing(&cli);

    if let Some(cmd) = cli.command.as_deref() {
        return fredshell_core::run_oneshot(cmd);
    }

    fredshell_core::repl::run(fredshell_core::repl::Options { login: cli.login })
}

fn init_tracing(cli: &Cli) {
    use tracing_subscriber::{EnvFilter, fmt};

    let level = cli.verbosity.log_level_filter().to_string();
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("fredshell={level},warn")));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init()
        .ok();
}
