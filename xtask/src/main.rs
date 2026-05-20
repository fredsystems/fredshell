// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! xtask — project automation runner.
//!
//! Subcommands:
//!   check     — cargo fmt --check, clippy, test, doc
//!   pc        — pre-commit equivalent (invoked from the nix devshell)
//!   coverage  — cargo llvm-cov producing lcov.info
//!   tty-probe — open a `TerminalSession` against the developer's real
//!               controlling terminal and print the detected
//!               `Capabilities` + initial `WindowSize`. Per `PLAN_04`
//!               §9 this is a developer tool, not a CI test — it must
//!               be run from an interactive terminal.

use clap::{Parser, Subcommand};
use color_eyre::eyre::{bail, Result};
use duct::cmd;
use fredshell_core::tty::TerminalSession;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run fmt + clippy + tests + doc.
    Check,
    /// Pre-commit equivalent invoked by mkCheck.
    Pc,
    /// Generate an lcov coverage report.
    Coverage,
    /// Just run the test suite.
    Test,
    /// Open the controlling terminal, run the capability probe, and
    /// print the detected `Capabilities` + initial `WindowSize`.
    ///
    /// Diagnostic tool described in `PLAN_04` §9. Must be run from an
    /// interactive terminal — fails fast in CI / non-tty contexts.
    TtyProbe,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Check => {
            cmd!("cargo", "fmt", "--all", "--check").run()?;
            cmd!("cargo", "clippy", "--all-targets", "--", "-D", "warnings").run()?;
            cmd!("cargo-machete").run()?;
            cmd!("cargo", "test", "--workspace").run()?;
            cmd!("cargo", "doc", "--workspace", "--no-deps").run()?;
        }
        Cmd::Pc => {
            cmd!("cargo", "fmt", "--all", "--check").run()?;
            cmd!("cargo", "clippy", "--all-targets", "--", "-D", "warnings").run()?;
            cmd!("cargo-machete").run()?;
            cmd!("cargo", "test", "--workspace").run()?;
        }
        Cmd::Coverage => {
            cmd!(
                "cargo",
                "llvm-cov",
                "--workspace",
                "--lcov",
                "--output-path",
                "lcov.info",
            )
            .run()?;
        }
        Cmd::Test => {
            cmd!("cargo", "test", "--workspace").run()?;
        }
        Cmd::TtyProbe => {
            run_tty_probe()?;
        }
    }

    Ok(())
}

/// Open a `TerminalSession` against the developer's real controlling
/// terminal, print the detected capabilities and initial window
/// size, then drop the session (which restores termios via the RAII
/// guard, even though we never entered raw mode).
///
/// Per `PLAN_04` §9 this is a developer-facing tool, not a CI test.
/// It refuses to run when no controlling terminal is available so
/// `cargo xtask tty-probe` in CI fails loudly rather than printing a
/// misleading "all-defaults" report.
fn run_tty_probe() -> Result<()> {
    let session = match TerminalSession::open() {
        Ok(s) => s,
        Err(e) => bail!("tty-probe: failed to open terminal session: {e}"),
    };

    let caps = session.capabilities();
    let ws = session.window_size();

    println!("fredshell tty-probe");
    println!("==================");
    println!();
    println!("Window size:");
    println!("  rows         : {}", ws.rows);
    println!("  cols         : {}", ws.cols);
    println!("  pixel_width  : {}", ws.pixel_width);
    println!("  pixel_height : {}", ws.pixel_height);
    println!();
    println!("Capabilities:");
    println!("  color                   : {:?}", caps.color);
    println!("  kitty_keyboard          : {}", caps.kitty_keyboard);
    println!("  bracketed_paste         : {}", caps.bracketed_paste);
    println!("  focus_reporting         : {}", caps.focus_reporting);
    println!("  synchronized_output     : {}", caps.synchronized_output);
    println!("  osc8_hyperlinks         : {:?}", caps.osc8_hyperlinks);
    println!("  osc52_clipboard         : {}", caps.osc52_clipboard);
    println!(
        "  osc133_semantic_prompt  : {}",
        caps.osc133_semantic_prompt
    );
    println!("  osc7_cwd                : {}", caps.osc7_cwd);

    Ok(())
}
