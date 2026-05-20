// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Starship-style prompt rendering.
//!
//! The eventual goal is starship-config compatibility for a sensible
//! subset of modules (`directory`, `git_branch`, `git_status`, `status`,
//! `cmd_duration`, `character`). For now we expose a single render
//! entrypoint returning a string of ANSI escape codes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptConfig {
    /// Preset name. Reserved for future use ("starship-like", "minimal", ...).
    #[serde(default)]
    pub preset: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PromptContext {
    pub cwd: std::path::PathBuf,
    pub last_status: i32,
}

#[must_use]
pub fn render(_cfg: &PromptConfig, ctx: &PromptContext) -> String {
    use nu_ansi_term::Color;

    let cwd = ctx.cwd.file_name().map_or_else(
        || ctx.cwd.display().to_string(),
        |s| s.to_string_lossy().into_owned(),
    );

    let arrow = if ctx.last_status == 0 {
        Color::Green.paint("❯")
    } else {
        Color::Red.paint("❯")
    };

    format!("{} {} ", Color::Cyan.bold().paint(cwd), arrow)
}
