// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! `.case.toml` schema and on-disk loader.
//!
//! Per `PLAN_05` §3.2, a spec case is a `<name>.case.toml` plus
//! optional sidecar fixtures:
//!
//! ```text
//! <name>.case.toml      schema below
//! <name>.stdout         expected stdout (default: empty)
//! <name>.stderr         expected stderr (default: empty)
//! <name>.exit           expected exit code (default: 0)
//! <name>.fs/            optional sandbox FS skeleton (`PLAN_05` §3.2)
//! ```
//!
//! Subtask 05.4 implements the schema for the v0 minimum set:
//!
//! * `description` (required) — human-readable summary.
//! * `script` (required) — the bash source to run.
//! * `status` (required) — one of `pass`, `fail`, `wontfix`,
//!   `deferred:PLAN_XX`. The semantic interpretation (`RECLASSIFY`
//!   etc.) lands in 05.5; 05.4 just round-trips the string.
//! * `tags` (optional) — free-form labels for filtering.
//! * `bash_version_min` (optional) — version gate; advisory in v0
//!   per `PLAN_05` §10.2.
//! * `[env]` (optional) — environment variables; supports `$SANDBOX`
//!   placeholder substitution at runtime (see [`crate::Sandbox`]).
//!
//! Future fields reserved for later subtasks: `stdin` (`PLAN_10`),
//! `args`, `timeout`, per-case dispositional metadata.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::LoadError;

/// A fully-resolved spec case as it appears on disk.
///
/// Construct via [`Case::load`]; the `expected.*` fields come from
/// sidecar files (or their defaults) rather than the TOML itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    /// Absolute or repo-relative path to the `.case.toml` file. Used
    /// for diagnostics and to derive sidecar paths.
    pub path: PathBuf,
    /// Human-readable summary from the case file.
    pub description: String,
    /// The script to execute. Verbatim from the case file.
    pub script: String,
    /// Declared case status (see §12). The harness round-trips it
    /// here; 05.5 owns the interpretation.
    pub status: CaseStatus,
    /// Free-form tags for filtering.
    pub tags: Vec<String>,
    /// Optional version gate; advisory in v0.
    pub bash_version_min: Option<String>,
    /// Environment variables to inject before the script runs.
    pub env: CaseEnv,
    /// Path to an optional sandbox FS skeleton directory
    /// (`<case>.fs/`). [`None`] when the directory does not exist.
    pub fs_skeleton: Option<PathBuf>,
    /// Expected observable outputs, loaded from sidecar files (or
    /// their defaults).
    pub expected: CaseExpected,
}

/// Declared status of a case per `PLAN_05` §12.
///
/// 05.4 round-trips this from the TOML; 05.5 applies the taxonomy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseStatus {
    /// Case is expected to match expectation today.
    Pass,
    /// Case is expected NOT to match. Tracked for parity.
    Fail,
    /// Documented intentional non-goal.
    Wontfix,
    /// Case will match once the named plan lands. The string is
    /// stored verbatim (e.g. `"PLAN_06"`) so 05.5 / xtask compat
    /// can filter on it.
    Deferred(String),
}

impl CaseStatus {
    fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "pass" => Ok(Self::Pass),
            "fail" => Ok(Self::Fail),
            "wontfix" => Ok(Self::Wontfix),
            other => match other.strip_prefix("deferred:") {
                Some(plan) if !plan.is_empty() => Ok(Self::Deferred(plan.to_owned())),
                _ => Err(format!(
                    "expected `pass`, `fail`, `wontfix`, or `deferred:PLAN_XX`; got {other:?}"
                )),
            },
        }
    }
}

impl fmt::Display for CaseStatus {
    /// Render a [`CaseStatus`] using the exact spelling that appears
    /// in `.case.toml` files. Round-trips with the on-disk parser
    /// used by [`Case::load`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pass => f.write_str("pass"),
            Self::Fail => f.write_str("fail"),
            Self::Wontfix => f.write_str("wontfix"),
            Self::Deferred(plan) => write!(f, "deferred:{plan}"),
        }
    }
}

/// Environment block from a case file.
///
/// Keys are sorted (`BTreeMap`) so error diagnostics and tests are
/// deterministic.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseEnv {
    /// `HOME` value. May contain the literal substring `$SANDBOX`
    /// which is resolved against the per-case sandbox root at
    /// runtime.
    pub home: Option<String>,
    /// `PATH` value. Same `$SANDBOX` substitution applies.
    pub path: Option<String>,
    /// Additional environment variables. Same `$SANDBOX` substitution
    /// applies to every value.
    pub extra: BTreeMap<String, String>,
}

/// Expected observable outputs for a case.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseExpected {
    /// Expected stdout, byte-exact.
    pub stdout: Vec<u8>,
    /// Expected stderr, byte-exact.
    pub stderr: Vec<u8>,
    /// Expected exit code.
    pub exit: i32,
}

// ---------------------------------------------------------------------------
// Internal: the on-disk TOML shape.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawCase {
    description: String,
    script: String,
    status: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    bash_version_min: Option<String>,
    #[serde(default)]
    env: Option<RawEnv>,
}

#[derive(Debug, Default, Deserialize)]
struct RawEnv {
    #[serde(default, rename = "HOME")]
    home: Option<String>,
    #[serde(default, rename = "PATH")]
    path: Option<String>,
    #[serde(default)]
    extra: BTreeMap<String, String>,
}

impl Case {
    /// Load a `.case.toml` file plus its sidecar fixtures.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError`] if the file cannot be read, the TOML
    /// fails to parse, the status field is malformed, or a sidecar
    /// fixture is unreadable.
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let raw_text = fs::read_to_string(path).map_err(|source| LoadError::Read {
            path: path.to_owned(),
            source,
        })?;
        let raw: RawCase = toml::from_str(&raw_text).map_err(|e| LoadError::Schema {
            path: path.to_owned(),
            message: e.to_string(),
        })?;

        let status = CaseStatus::parse(&raw.status).map_err(|message| LoadError::Schema {
            path: path.to_owned(),
            message,
        })?;

        let env = raw
            .env
            .map(|e| CaseEnv {
                home: e.home,
                path: e.path,
                extra: e.extra,
            })
            .unwrap_or_default();

        let stem = sidecar_stem(path);
        let stdout = read_sidecar_bytes(&stem, "stdout")?;
        let stderr = read_sidecar_bytes(&stem, "stderr")?;
        let exit = read_exit_sidecar(&stem)?;

        let fs_skeleton_path = sidecar_path(&stem, "fs");
        let fs_skeleton = if fs_skeleton_path.is_dir() {
            Some(fs_skeleton_path)
        } else {
            None
        };

        Ok(Self {
            path: path.to_owned(),
            description: raw.description,
            script: raw.script,
            status,
            tags: raw.tags,
            bash_version_min: raw.bash_version_min,
            env,
            fs_skeleton,
            expected: CaseExpected {
                stdout,
                stderr,
                exit,
            },
        })
    }
}

/// Strip the `.case.toml` suffix from a case path so sidecars can be
/// derived. For `foo/bar.case.toml` returns `foo/bar`.
fn sidecar_stem(path: &Path) -> PathBuf {
    // The path is required to end in `.case.toml`; this helper strips
    // both extensions. If the suffix is missing we still produce a
    // path with the file stem so the caller gets a coherent error
    // when the sidecar fails to load.
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

fn read_sidecar_bytes(stem: &Path, ext: &str) -> Result<Vec<u8>, LoadError> {
    let p = sidecar_path(stem, ext);
    match fs::read(&p) {
        Ok(b) => Ok(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(LoadError::Sidecar { path: p, source }),
    }
}

fn read_exit_sidecar(stem: &Path) -> Result<i32, LoadError> {
    let p = sidecar_path(stem, "exit");
    let text = match fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(source) => return Err(LoadError::Sidecar { path: p, source }),
    };
    let trimmed = text.trim();
    trimmed
        .parse::<i32>()
        .map_err(|_| LoadError::BadExitFixture {
            path: p,
            contents: trimmed.to_owned(),
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_case(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(format!("{name}.case.toml"));
        fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn case_status_parse_known_values() {
        assert_eq!(CaseStatus::parse("pass").unwrap(), CaseStatus::Pass);
        assert_eq!(CaseStatus::parse("fail").unwrap(), CaseStatus::Fail);
        assert_eq!(CaseStatus::parse("wontfix").unwrap(), CaseStatus::Wontfix);
        assert_eq!(
            CaseStatus::parse("deferred:PLAN_06").unwrap(),
            CaseStatus::Deferred("PLAN_06".to_owned())
        );
    }

    #[test]
    fn case_status_parse_rejects_unknown() {
        assert!(CaseStatus::parse("maybe").is_err());
        // Bare `deferred:` without a plan name is rejected.
        assert!(CaseStatus::parse("deferred:").is_err());
    }

    #[test]
    fn case_status_display_round_trips_with_parse() {
        for raw in ["pass", "fail", "wontfix", "deferred:PLAN_06"] {
            let parsed = CaseStatus::parse(raw).unwrap();
            assert_eq!(parsed.to_string(), raw);
        }
    }

    #[test]
    fn sidecar_stem_strips_case_toml_suffix() {
        let p = Path::new("tests/spec/foo/bar.case.toml");
        let stem = sidecar_stem(p);
        assert_eq!(stem, PathBuf::from("tests/spec/foo/bar"));
    }

    #[test]
    fn sidecar_stem_handles_path_without_suffix() {
        // Pathological input still produces something useful.
        let p = Path::new("weird");
        let stem = sidecar_stem(p);
        assert_eq!(stem, PathBuf::from("weird"));
    }

    #[test]
    fn load_minimal_case_with_no_sidecars_uses_defaults() {
        let dir = TempDir::new().unwrap();
        let path = write_case(
            dir.path(),
            "minimal",
            r#"
description = "minimal"
status = "pass"
script = "exit 0\n"
"#,
        );
        let c = Case::load(&path).unwrap();
        assert_eq!(c.description, "minimal");
        assert_eq!(c.status, CaseStatus::Pass);
        assert_eq!(c.script, "exit 0\n");
        assert!(c.tags.is_empty());
        assert!(c.bash_version_min.is_none());
        assert!(c.env.home.is_none());
        assert!(c.fs_skeleton.is_none());
        assert!(c.expected.stdout.is_empty());
        assert!(c.expected.stderr.is_empty());
        assert_eq!(c.expected.exit, 0);
    }

    #[test]
    fn load_reads_sidecar_fixtures() {
        let dir = TempDir::new().unwrap();
        let path = write_case(
            dir.path(),
            "withfx",
            r#"
description = "withfx"
status = "pass"
script = "echo hi\n"
"#,
        );
        fs::write(dir.path().join("withfx.stdout"), b"hi\n").unwrap();
        fs::write(dir.path().join("withfx.stderr"), b"warn\n").unwrap();
        fs::write(dir.path().join("withfx.exit"), "0\n").unwrap();

        let c = Case::load(&path).unwrap();
        assert_eq!(c.expected.stdout, b"hi\n");
        assert_eq!(c.expected.stderr, b"warn\n");
        assert_eq!(c.expected.exit, 0);
    }

    #[test]
    fn load_detects_fs_skeleton_directory() {
        let dir = TempDir::new().unwrap();
        let path = write_case(
            dir.path(),
            "withfs",
            r#"
description = "withfs"
status = "pass"
script = "true\n"
"#,
        );
        fs::create_dir(dir.path().join("withfs.fs")).unwrap();
        fs::write(dir.path().join("withfs.fs/file.txt"), b"contents").unwrap();

        let c = Case::load(&path).unwrap();
        let skel = c.fs_skeleton.expect("fs skeleton detected");
        assert_eq!(skel, dir.path().join("withfs.fs"));
    }

    #[test]
    fn load_returns_schema_error_on_missing_required_field() {
        let dir = TempDir::new().unwrap();
        let path = write_case(
            dir.path(),
            "broken",
            "description = \"x\"\nstatus = \"pass\"\n",
        );
        let err = Case::load(&path).unwrap_err();
        match err {
            LoadError::Schema { message, .. } => assert!(message.contains("script")),
            other => panic!("expected Schema, got {other:?}"),
        }
    }

    #[test]
    fn load_returns_schema_error_on_bad_status() {
        let dir = TempDir::new().unwrap();
        let path = write_case(
            dir.path(),
            "badstatus",
            r#"
description = "x"
script = "true\n"
status = "weird"
"#,
        );
        let err = Case::load(&path).unwrap_err();
        match err {
            LoadError::Schema { message, .. } => assert!(message.contains("weird")),
            other => panic!("expected Schema, got {other:?}"),
        }
    }

    #[test]
    fn load_returns_bad_exit_fixture_on_garbage() {
        let dir = TempDir::new().unwrap();
        let path = write_case(
            dir.path(),
            "badexit",
            r#"
description = "x"
script = "true\n"
status = "pass"
"#,
        );
        fs::write(dir.path().join("badexit.exit"), "not-a-number\n").unwrap();
        let err = Case::load(&path).unwrap_err();
        match err {
            LoadError::BadExitFixture { contents, .. } => {
                assert_eq!(contents, "not-a-number");
            }
            other => panic!("expected BadExitFixture, got {other:?}"),
        }
    }

    #[test]
    fn load_returns_read_error_for_missing_case_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nope.case.toml");
        let err = Case::load(&path).unwrap_err();
        match err {
            LoadError::Read { .. } => {}
            other => panic!("expected Read, got {other:?}"),
        }
    }

    #[test]
    fn load_parses_env_block() {
        let dir = TempDir::new().unwrap();
        let path = write_case(
            dir.path(),
            "envcase",
            r#"
description = "envcase"
script = "true\n"
status = "pass"

[env]
HOME = "$SANDBOX/home"
PATH = "$SANDBOX/bin"
extra = { FOO = "bar", BAZ = "qux" }
"#,
        );
        let c = Case::load(&path).unwrap();
        assert_eq!(c.env.home.as_deref(), Some("$SANDBOX/home"));
        assert_eq!(c.env.path.as_deref(), Some("$SANDBOX/bin"));
        assert_eq!(c.env.extra.get("FOO").map(String::as_str), Some("bar"));
        assert_eq!(c.env.extra.get("BAZ").map(String::as_str), Some("qux"));
    }

    #[test]
    fn load_parses_tags_and_version_gate() {
        let dir = TempDir::new().unwrap();
        let path = write_case(
            dir.path(),
            "meta",
            r#"
description = "meta"
script = "true\n"
status = "deferred:PLAN_06"
tags = ["parameter-expansion", "posix-overlap"]
bash_version_min = "5.0"
"#,
        );
        let c = Case::load(&path).unwrap();
        assert_eq!(c.status, CaseStatus::Deferred("PLAN_06".to_owned()));
        assert_eq!(c.tags, vec!["parameter-expansion", "posix-overlap"]);
        assert_eq!(c.bash_version_min.as_deref(), Some("5.0"));
    }
}
