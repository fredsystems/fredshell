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

use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;
use duct::cmd;

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
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Check => {
            cmd!("cargo", "fmt", "--all", "--check").run()?;
            cmd!("cargo", "clippy", "--all-targets", "--", "-D", "warnings").run()?;
            cmd!("cargo", "test", "--workspace").run()?;
            cmd!("cargo", "doc", "--workspace", "--no-deps").run()?;
        }
        Cmd::Pc => {
            cmd!("cargo", "fmt", "--all", "--check").run()?;
            cmd!("cargo", "clippy", "--all-targets", "--", "-D", "warnings").run()?;
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
    }

    Ok(())
}
