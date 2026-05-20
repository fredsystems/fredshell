// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Starship-style prompt rendering.
//!
//! The eventual goal is starship-config compatibility for a sensible
//! subset of modules (`directory`, `git_branch`, `git_status`,
//! `status`, `cmd_duration`, `character`). For now we expose a
//! single render entrypoint returning a string of ANSI escape codes.
//!
//! Styling is driven by [`fredshell_ansi`]: the prompt builds
//! [`Sgr`](fredshell_ansi::sgr::Sgr) values, encodes them to bytes,
//! interleaves the styled payload, and emits a final
//! [`Sgr::RESET`](fredshell_ansi::sgr::Sgr::RESET) so the
//! surrounding line baseline is unstyled.

use fredshell_ansi::Encode;
use fredshell_ansi::sgr::{Color, Sgr};
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

/// Render the prompt to a string of UTF-8 text and ANSI escape
/// sequences, terminated with a single trailing space.
///
/// The returned string is safe to write to stdout as-is. Every
/// styled span is bracketed by an [`Sgr`] start and [`Sgr::RESET`],
/// so the cursor lands on an unstyled baseline.
#[must_use]
pub fn render(_cfg: &PromptConfig, ctx: &PromptContext) -> String {
    let cwd = ctx.cwd.file_name().map_or_else(
        || ctx.cwd.display().to_string(),
        |s| s.to_string_lossy().into_owned(),
    );

    let arrow_color = if ctx.last_status == 0 {
        Color::Green
    } else {
        Color::Red
    };

    // Pre-size: two SGR starts, two SGR resets, the cwd payload,
    // the arrow glyph, and one trailing space.
    let cwd_style = Sgr::fg(Color::Cyan).with_bold();
    let arrow_style = Sgr::fg(arrow_color);
    let mut out = String::with_capacity(
        cwd_style.encoded_len()
            + cwd.len()
            + Sgr::RESET.encoded_len()
            + 1 // separator space
            + arrow_style.encoded_len()
            + "❯".len()
            + Sgr::RESET.encoded_len()
            + 1, // trailing space
    );

    write_sgr(&mut out, &cwd_style);
    out.push_str(&cwd);
    write_sgr(&mut out, &Sgr::RESET);
    out.push(' ');
    write_sgr(&mut out, &arrow_style);
    out.push('❯');
    write_sgr(&mut out, &Sgr::RESET);
    out.push(' ');

    out
}

/// Encode an [`Sgr`] into a [`String`] via the byte-level
/// [`Encode`] surface.
///
/// SGR sequences are pure ASCII, so appending the encoded bytes to
/// a UTF-8 string is sound. We route through a tiny `io::Write`
/// adapter that pushes each ASCII byte as a `char`, avoiding any
/// intermediate `Vec<u8>` allocation.
fn write_sgr(out: &mut String, sgr: &Sgr) {
    struct AsciiSink<'a>(&'a mut String);
    impl std::io::Write for AsciiSink<'_> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            // SGR encoder emits only ASCII bytes by contract.
            // Debug-assert in dev; trust in release.
            debug_assert!(
                buf.iter().all(u8::is_ascii),
                "Sgr encoder emitted non-ASCII bytes",
            );
            // ASCII bytes are valid single-byte UTF-8 chars.
            for &b in buf {
                self.0.push(char::from(b));
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut sink = AsciiSink(out);
    // The sink never errors, so the io::Result is always Ok.
    let _ = sgr.encode(&mut sink);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{PromptConfig, PromptContext, render};
    use std::path::PathBuf;

    fn cfg() -> PromptConfig {
        PromptConfig::default()
    }

    #[test]
    fn render_success_status_uses_green_arrow() {
        let ctx = PromptContext {
            cwd: PathBuf::from("/home/fred/projects/fredshell"),
            last_status: 0,
        };
        let out = render(&cfg(), &ctx);

        // Cyan bold for cwd basename, green for arrow, both reset.
        // Expected bytes: CSI 1;36m fredshell CSI 0m space CSI 32m ❯ CSI 0m space.
        let expected = "\x1b[1;36mfredshell\x1b[0m \x1b[32m❯\x1b[0m ";
        assert_eq!(out, expected);
    }

    #[test]
    fn render_failure_status_uses_red_arrow() {
        let ctx = PromptContext {
            cwd: PathBuf::from("/tmp/x"),
            last_status: 1,
        };
        let out = render(&cfg(), &ctx);
        let expected = "\x1b[1;36mx\x1b[0m \x1b[31m❯\x1b[0m ";
        assert_eq!(out, expected);
    }

    #[test]
    fn render_falls_back_to_full_path_when_no_basename() {
        // A trailing slash leaves no `file_name`; the full display
        // form is used.
        let ctx = PromptContext {
            cwd: PathBuf::from("/"),
            last_status: 0,
        };
        let out = render(&cfg(), &ctx);
        let expected = "\x1b[1;36m/\x1b[0m \x1b[32m❯\x1b[0m ";
        assert_eq!(out, expected);
    }

    #[test]
    fn render_ends_with_space() {
        let ctx = PromptContext {
            cwd: PathBuf::from("/home"),
            last_status: 0,
        };
        let out = render(&cfg(), &ctx);
        assert!(out.ends_with(' '));
    }

    #[test]
    fn render_terminates_with_sgr_reset_before_trailing_space() {
        let ctx = PromptContext {
            cwd: PathBuf::from("/home"),
            last_status: 7,
        };
        let out = render(&cfg(), &ctx);
        // The final reset ends with `m`, then the trailing space.
        assert!(out.ends_with("\x1b[0m "));
    }
}
