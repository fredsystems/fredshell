// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! `cargo xtask spec record` — record sidecar fixtures from the
//! pinned reference bash.
//!
//! Per `PLAN_05` §4.4, expected outputs (`.stdout`, `.stderr`,
//! `.exit`) are **recorded fixtures** committed to the repo, not
//! produced live during the test run. This subcommand is how those
//! fixtures are created and refreshed.
//!
//! ## Contract
//!
//! 1. The pinned reference bash MUST be available via the
//!    `FREDSHELL_REFERENCE_BASH` env var (absolute path; set by the
//!    nix devshell — see `PLAN_05` §4.5).
//! 2. The version reported by that bash (read from
//!    `FREDSHELL_REFERENCE_BASH_VERSION`) MUST exactly match the
//!    `bash` key in `tests/spec/REFERENCE.md`'s `[reference]` block.
//!    Mismatches refuse to record — fixtures are only valid against
//!    the pinned version.
//! 3. The case is loaded via [`fredshell_spec_runner::Case::load`],
//!    so it sees the same schema the harness will compare against.
//! 4. Execution happens in a fresh [`Sandbox`] with an `<case>.fs/`
//!    skeleton materialized (if present) and the case's `[env]`
//!    block resolved against the sandbox root.
//! 5. The bash process is invoked as `bash -c <script>`, with the
//!    environment scrubbed and replaced by the resolved env. CWD is
//!    the sandbox root.
//! 6. Sidecar files are written following `PLAN_05` §3.2's "present
//!    explicitly when non-default" rule:
//!    - Non-empty stdout → write `<stem>.stdout`; empty → delete it
//!      if present.
//!    - Same for stderr.
//!    - Non-zero exit → write `<stem>.exit` (one integer + newline);
//!      zero → delete it if present.
//!
//! The default-elision rule keeps the corpus diff-friendly: a pure
//! `exit 0` case has no sidecar files at all.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Args;
use color_eyre::eyre::{bail, Result};
use fredshell_spec_runner::{Case, Sandbox};

use super::{parse_reference, REFERENCE_DOC};

/// `cargo xtask spec record` arguments.
#[derive(Args)]
pub struct RecordArgs {
    /// Path to the `.case.toml` to record fixtures for.
    pub case: PathBuf,
}

/// Entry point for `cargo xtask spec record`.
pub fn run(args: &RecordArgs) -> Result<()> {
    let bash_path = require_env("FREDSHELL_REFERENCE_BASH")?;
    let bash_version = require_env("FREDSHELL_REFERENCE_BASH_VERSION")?;

    let doc_path = Path::new(REFERENCE_DOC);
    let doc = fs::read_to_string(doc_path)
        .map_err(|e| color_eyre::eyre::eyre!("read {}: {e}", doc_path.display()))?;
    let pin = parse_reference(&doc)
        .map_err(|e| color_eyre::eyre::eyre!("parse {}: {e}", doc_path.display()))?;

    if pin.bash != bash_version {
        bail!(
            "spec record: FREDSHELL_REFERENCE_BASH_VERSION = {bash_version}, but \
             {} pins bash = {}. Refusing to record; fixtures must be produced \
             against the pinned version. Run `cargo xtask spec versions` to \
             diagnose.",
            doc_path.display(),
            pin.bash,
        );
    }

    if !args.case.exists() {
        bail!(
            "spec record: case file does not exist: {}",
            args.case.display()
        );
    }

    let case = Case::load(&args.case)
        .map_err(|e| color_eyre::eyre::eyre!("load case {}: {e}", args.case.display()))?;

    let sandbox = Sandbox::new().map_err(|e| color_eyre::eyre::eyre!("sandbox: {e}"))?;
    if !sandbox.root_is_utf8() {
        bail!(
            "spec record: sandbox root is not valid UTF-8 ({}); set TMPDIR \
             to a UTF-8 path",
            sandbox.root().display(),
        );
    }
    if let Some(skel) = &case.fs_skeleton {
        sandbox
            .materialize_skeleton(skel)
            .map_err(|e| color_eyre::eyre::eyre!("materialize skeleton {}: {e}", skel.display()))?;
    }
    let resolved_env = sandbox.resolve_env(&case.env);

    let output = Command::new(&bash_path)
        .arg("-c")
        .arg(&case.script)
        .env_clear()
        .envs(&resolved_env)
        .current_dir(sandbox.root())
        .output()
        .map_err(|e| {
            color_eyre::eyre::eyre!(
                "spawn {bash_path:?}: {e} (is FREDSHELL_REFERENCE_BASH correct?)"
            )
        })?;

    let exit_code = output.status.code().unwrap_or(127);

    let stem = sidecar_stem(&args.case);
    let stdout_path = sidecar_path(&stem, "stdout");
    let stderr_path = sidecar_path(&stem, "stderr");
    let exit_path = sidecar_path(&stem, "exit");

    let stdout_action = write_or_remove_bytes(&stdout_path, &output.stdout)?;
    let stderr_action = write_or_remove_bytes(&stderr_path, &output.stderr)?;
    let exit_action = write_or_remove_exit(&exit_path, exit_code)?;

    println!("fredshell spec record");
    println!("====================");
    println!();
    println!("Case      : {}", args.case.display());
    println!("Bash      : {bash_path} ({bash_version})");
    println!("Sandbox   : {}", sandbox.root().display());
    println!("Exit code : {exit_code}");
    println!();
    println!("Sidecars:");
    report_action("stdout", &stdout_path, &stdout_action);
    report_action("stderr", &stderr_path, &stderr_action);
    report_action("exit  ", &exit_path, &exit_action);

    Ok(())
}

/// Read a required env var, surfacing a recording-context error
/// message rather than a bare `VarError`.
fn require_env(name: &str) -> Result<String> {
    match env::var(name) {
        Ok(v) if !v.is_empty() => Ok(v),
        _ => bail!(
            "spec record: {name} is not set. Run `nix develop` (or activate \
             direnv) before invoking this command — the recording bash is \
             provided by the devshell."
        ),
    }
}

/// Outcome of a sidecar write for diagnostic reporting.
#[derive(Debug, PartialEq, Eq)]
enum SidecarAction {
    /// Wrote a new file (no prior version existed).
    Created,
    /// Overwrote an existing file with new contents.
    Updated,
    /// Existing file already matched the new contents.
    Unchanged,
    /// Deleted a previously-existing file because the value is now
    /// the default.
    Removed,
    /// No file existed and none was needed.
    SkippedDefault,
}

fn write_or_remove_bytes(path: &Path, bytes: &[u8]) -> Result<SidecarAction> {
    if bytes.is_empty() {
        if path.exists() {
            fs::remove_file(path)
                .map_err(|e| color_eyre::eyre::eyre!("remove {}: {e}", path.display()))?;
            Ok(SidecarAction::Removed)
        } else {
            Ok(SidecarAction::SkippedDefault)
        }
    } else {
        let existed = path.exists();
        let same = if existed {
            fs::read(path).is_ok_and(|prior| prior == bytes)
        } else {
            false
        };
        if same {
            Ok(SidecarAction::Unchanged)
        } else {
            fs::write(path, bytes)
                .map_err(|e| color_eyre::eyre::eyre!("write {}: {e}", path.display()))?;
            Ok(if existed {
                SidecarAction::Updated
            } else {
                SidecarAction::Created
            })
        }
    }
}

fn write_or_remove_exit(path: &Path, code: i32) -> Result<SidecarAction> {
    if code == 0 {
        if path.exists() {
            fs::remove_file(path)
                .map_err(|e| color_eyre::eyre::eyre!("remove {}: {e}", path.display()))?;
            Ok(SidecarAction::Removed)
        } else {
            Ok(SidecarAction::SkippedDefault)
        }
    } else {
        let text = format!("{code}\n");
        let existed = path.exists();
        let same = if existed {
            fs::read_to_string(path).is_ok_and(|prior| prior == text)
        } else {
            false
        };
        if same {
            Ok(SidecarAction::Unchanged)
        } else {
            fs::write(path, &text)
                .map_err(|e| color_eyre::eyre::eyre!("write {}: {e}", path.display()))?;
            Ok(if existed {
                SidecarAction::Updated
            } else {
                SidecarAction::Created
            })
        }
    }
}

fn report_action(label: &str, path: &Path, action: &SidecarAction) {
    let verb = match action {
        SidecarAction::Created => "created",
        SidecarAction::Updated => "updated",
        SidecarAction::Unchanged => "unchanged",
        SidecarAction::Removed => "removed (default)",
        SidecarAction::SkippedDefault => "skipped (default)",
    };
    println!("  {label}  {verb:<18}  {}", path.display());
}

/// Strip the `.case.toml` suffix from a case path so sidecars can be
/// derived. Mirrors the helper in `fredshell-spec-runner::case` but
/// is kept local to `xtask` to avoid widening that crate's public
/// API just for one consumer.
fn sidecar_stem(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let stem = file_name.strip_suffix(".case.toml").unwrap_or(file_name);
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    parent.join(stem)
}

fn sidecar_path(stem: &Path, ext: &str) -> PathBuf {
    let mut p = stem.as_os_str().to_owned();
    p.push(".");
    p.push(ext);
    PathBuf::from(p)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn sidecar_stem_strips_case_toml_suffix() {
        let p = Path::new("tests/spec/foo/bar.case.toml");
        assert_eq!(sidecar_stem(p), PathBuf::from("tests/spec/foo/bar"));
    }

    #[test]
    fn sidecar_path_appends_extension() {
        let stem = PathBuf::from("tests/spec/foo/bar");
        assert_eq!(
            sidecar_path(&stem, "stdout"),
            PathBuf::from("tests/spec/foo/bar.stdout")
        );
    }

    #[test]
    fn write_or_remove_bytes_skips_default_when_empty_and_absent() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("x.stdout");
        let action = write_or_remove_bytes(&p, b"").unwrap();
        assert_eq!(action, SidecarAction::SkippedDefault);
        assert!(!p.exists());
    }

    #[test]
    fn write_or_remove_bytes_removes_existing_when_now_empty() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("x.stdout");
        fs::write(&p, b"stale").unwrap();
        let action = write_or_remove_bytes(&p, b"").unwrap();
        assert_eq!(action, SidecarAction::Removed);
        assert!(!p.exists());
    }

    #[test]
    fn write_or_remove_bytes_creates_when_absent() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("x.stdout");
        let action = write_or_remove_bytes(&p, b"hi\n").unwrap();
        assert_eq!(action, SidecarAction::Created);
        assert_eq!(fs::read(&p).unwrap(), b"hi\n");
    }

    #[test]
    fn write_or_remove_bytes_reports_unchanged_when_identical() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("x.stdout");
        fs::write(&p, b"hi\n").unwrap();
        let action = write_or_remove_bytes(&p, b"hi\n").unwrap();
        assert_eq!(action, SidecarAction::Unchanged);
    }

    #[test]
    fn write_or_remove_bytes_updates_when_different() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("x.stdout");
        fs::write(&p, b"old").unwrap();
        let action = write_or_remove_bytes(&p, b"new").unwrap();
        assert_eq!(action, SidecarAction::Updated);
        assert_eq!(fs::read(&p).unwrap(), b"new");
    }

    #[test]
    fn write_or_remove_exit_skips_zero() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("x.exit");
        let action = write_or_remove_exit(&p, 0).unwrap();
        assert_eq!(action, SidecarAction::SkippedDefault);
        assert!(!p.exists());
    }

    #[test]
    fn write_or_remove_exit_removes_existing_zero() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("x.exit");
        fs::write(&p, "42\n").unwrap();
        let action = write_or_remove_exit(&p, 0).unwrap();
        assert_eq!(action, SidecarAction::Removed);
    }

    #[test]
    fn write_or_remove_exit_writes_nonzero_with_trailing_newline() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("x.exit");
        let action = write_or_remove_exit(&p, 42).unwrap();
        assert_eq!(action, SidecarAction::Created);
        assert_eq!(fs::read_to_string(&p).unwrap(), "42\n");
    }

    #[test]
    fn write_or_remove_exit_reports_unchanged_when_identical() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("x.exit");
        fs::write(&p, "7\n").unwrap();
        let action = write_or_remove_exit(&p, 7).unwrap();
        assert_eq!(action, SidecarAction::Unchanged);
    }
}
