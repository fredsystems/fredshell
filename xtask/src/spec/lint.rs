// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! `cargo xtask spec lint` — validate the spec corpus.
//!
//! Per `PLAN_05` 05.8, the linter runs three independent checks:
//!
//! 1. **Schema**: every `.case.toml` under `tests/spec/` loads via
//!    [`fredshell_spec_runner::Case::load`] without error.
//! 2. **Orphan fixtures**: every `<stem>.{stdout,stderr,exit}` file
//!    and every `<stem>.fs/` directory under `tests/spec/` has a
//!    matching `<stem>.case.toml`. A fixture without an owner means
//!    the case was renamed or deleted without cleaning up.
//! 3. **§11.1 drift**: the set of bash builtins reported by
//!    `bash -c 'enable -a'` matches [`EXPECTED_BUILTINS`]. The
//!    constant is the §11.1 inventory verbatim; drift means bash's
//!    builtin surface changed and the plan doc needs an update in
//!    the same commit as the bump.
//!
//! Each check produces a per-violation diagnostic line plus a count.
//! The command exits with status 1 if any check produced at least
//! one violation.
//!
//! The drift check requires the nix devshell (it needs
//! `FREDSHELL_REFERENCE_BASH`). Outside the devshell, pass
//! `--skip-builtins-drift` to suppress that check (schema + orphan
//! checks still run). CI always runs the full set; the flag exists
//! for local iteration only.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Args;
use color_eyre::eyre::{bail, Result};
use fredshell_spec_runner::Case;

/// Workspace-root-relative path to the corpus root.
const CORPUS_ROOT: &str = "tests/spec";

/// `cargo xtask spec lint` arguments.
#[derive(Args)]
pub struct LintArgs {
    /// Skip the `enable -a` drift check (still runs schema + orphan
    /// checks). Set this when running outside the nix devshell.
    #[arg(long)]
    pub skip_builtins_drift: bool,
}

/// Entry point for `cargo xtask spec lint`.
pub fn run(args: &LintArgs) -> Result<()> {
    println!("fredshell spec lint");
    println!("===================");
    println!();

    let corpus_root = Path::new(CORPUS_ROOT);
    if !corpus_root.is_dir() {
        bail!(
            "spec lint: corpus root {} is not a directory",
            corpus_root.display()
        );
    }

    let mut violations: usize = 0;

    let (case_paths, fixture_paths) = enumerate_corpus(corpus_root)?;

    violations += check_schema(&case_paths);
    violations += check_orphans(&case_paths, &fixture_paths);
    if args.skip_builtins_drift {
        println!("[builtins] skipped (--skip-builtins-drift)");
    } else {
        violations += check_builtins_drift()?;
    }

    println!();
    if violations == 0 {
        println!("OK ({} cases checked)", case_paths.len());
        Ok(())
    } else {
        bail!("spec lint: {violations} violation(s) found");
    }
}

/// One pass of the corpus tree to collect both case files and
/// fixture-like files. Returning them together keeps the walker
/// single-pass and the orphan check straightforward.
fn enumerate_corpus(root: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let mut cases: Vec<PathBuf> = Vec::new();
    let mut fixtures: Vec<PathBuf> = Vec::new();
    walk(root, &mut cases, &mut fixtures)?;
    cases.sort();
    fixtures.sort();
    Ok((cases, fixtures))
}

fn walk(dir: &Path, cases: &mut Vec<PathBuf>, fixtures: &mut Vec<PathBuf>) -> Result<()> {
    for entry in
        fs::read_dir(dir).map_err(|e| color_eyre::eyre::eyre!("read_dir {}: {e}", dir.display()))?
    {
        let entry = entry
            .map_err(|e| color_eyre::eyre::eyre!("read_dir entry in {}: {e}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| color_eyre::eyre::eyre!("file_type {}: {e}", path.display()))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy().into_owned();

        if file_type.is_dir() {
            // `.fs/` directories are fixtures, not corpus entries to
            // recurse into. Track them for orphan checking. Corpus
            // names are author-controlled and lowercase by convention.
            #[allow(clippy::case_sensitive_file_extension_comparisons)]
            let is_fs_dir = name_str.ends_with(".fs");
            if is_fs_dir {
                fixtures.push(path);
            } else {
                walk(&path, cases, fixtures)?;
            }
        } else if file_type.is_file() {
            // Corpus filenames are author-controlled and lowercase by
            // convention; the case-insensitive lint does not apply.
            #[allow(clippy::case_sensitive_file_extension_comparisons)]
            let is_case = name_str.ends_with(".case.toml");
            #[allow(clippy::case_sensitive_file_extension_comparisons)]
            let is_sidecar = name_str.ends_with(".stdout")
                || name_str.ends_with(".stderr")
                || name_str.ends_with(".exit");
            if is_case {
                cases.push(path);
            } else if is_sidecar {
                fixtures.push(path);
            }
            // Anything else (README.md, REFERENCE.md, ad-hoc docs)
            // is ignored — the linter has no opinion about it.
        }
    }
    Ok(())
}

/// Schema check: each `.case.toml` must load via [`Case::load`].
fn check_schema(cases: &[PathBuf]) -> usize {
    let mut violations: usize = 0;
    println!("[schema] {} case file(s)", cases.len());
    for path in cases {
        if let Err(e) = Case::load(path) {
            violations += 1;
            println!("  FAIL {}: {e}", path.display());
        }
    }
    if violations == 0 {
        println!("[schema] OK");
    } else {
        println!("[schema] {violations} failing case file(s)");
    }
    violations
}

/// Orphan check: every fixture file/dir must have a matching case
/// file. The fixture stem is everything before the last `.stdout` /
/// `.stderr` / `.exit` / `.fs` segment.
fn check_orphans(cases: &[PathBuf], fixtures: &[PathBuf]) -> usize {
    let case_stems: BTreeSet<PathBuf> = cases.iter().map(|p| case_stem(p)).collect();
    let mut violations: usize = 0;
    println!("[orphans] {} fixture(s)", fixtures.len());
    for fx in fixtures {
        let stem = fixture_stem(fx);
        if !case_stems.contains(&stem) {
            violations += 1;
            println!(
                "  FAIL orphan fixture {} (expected {}.case.toml)",
                fx.display(),
                stem.display()
            );
        }
    }
    if violations == 0 {
        println!("[orphans] OK");
    } else {
        println!("[orphans] {violations} orphan fixture(s)");
    }
    violations
}

/// `<dir>/foo.case.toml` → `<dir>/foo`.
fn case_stem(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let stem = file_name.strip_suffix(".case.toml").unwrap_or(file_name);
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    parent.join(stem)
}

/// `<dir>/foo.stdout` (or `.stderr`, `.exit`, `.fs`) → `<dir>/foo`.
fn fixture_stem(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let stem = file_name
        .strip_suffix(".stdout")
        .or_else(|| file_name.strip_suffix(".stderr"))
        .or_else(|| file_name.strip_suffix(".exit"))
        .or_else(|| file_name.strip_suffix(".fs"))
        .unwrap_or(file_name);
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    parent.join(stem)
}

/// `bash -c 'enable -a'` drift check.
///
/// Returns the number of violations (a single drift error counts as
/// 1 regardless of how many builtins differ; the diagnostic shows
/// the full diff).
fn check_builtins_drift() -> Result<usize> {
    let bash = match std::env::var("FREDSHELL_REFERENCE_BASH") {
        Ok(v) if !v.is_empty() => v,
        _ => bail!(
            "spec lint: FREDSHELL_REFERENCE_BASH is not set. Run `nix develop` \
             (or activate direnv), or pass --skip-builtins-drift."
        ),
    };
    let output = Command::new(&bash)
        .arg("-c")
        .arg("enable -a")
        .output()
        .map_err(|e| color_eyre::eyre::eyre!("spawn {bash:?}: {e}"))?;
    if !output.status.success() {
        bail!(
            "spec lint: `{bash} -c 'enable -a'` exited with status {:?}",
            output.status.code()
        );
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|e| color_eyre::eyre::eyre!("enable -a stdout was not UTF-8: {e}"))?;
    let observed: BTreeSet<String> = parse_enable_a(&stdout);
    let expected: BTreeSet<String> = EXPECTED_BUILTINS.iter().map(|&s| s.to_owned()).collect();

    println!(
        "[builtins] observed {} from `enable -a`, expected {} from PLAN_05 §11.1",
        observed.len(),
        expected.len()
    );

    let missing: Vec<&String> = expected.difference(&observed).collect();
    let unexpected: Vec<&String> = observed.difference(&expected).collect();

    if missing.is_empty() && unexpected.is_empty() {
        println!("[builtins] OK");
        return Ok(0);
    }

    for m in &missing {
        println!("  FAIL §11.1 lists `{m}` but `enable -a` does not (removed?)");
    }
    for u in &unexpected {
        println!("  FAIL `enable -a` reports `{u}` but §11.1 does not list it (new?)");
    }
    println!(
        "[builtins] drift: {} missing, {} unexpected (update PLAN_05 §11.1 and EXPECTED_BUILTINS in lock-step)",
        missing.len(),
        unexpected.len()
    );
    Ok(1)
}

/// Parse the output of `bash -c 'enable -a'`. Each line looks like
/// `enable <name>` or `enable -n <name>` (when a builtin was
/// disabled). We extract the last whitespace-separated token.
fn parse_enable_a(stdout: &str) -> BTreeSet<String> {
    stdout
        .lines()
        .filter_map(|line| line.split_whitespace().next_back().map(str::to_owned))
        .filter(|s| !s.is_empty())
        .collect()
}

/// The canonical set of bash builtins per `PLAN_05` §11.1.
///
/// Sourced from `bash -c 'enable -a' | awk '{print $NF}' | sort` on
/// the pinned reference bash (currently 5.3p9 from
/// `nixos-unstable`). 57 entries. Must be kept in lock-step with
/// §11.1: any change to one updates the other in the same commit.
const EXPECTED_BUILTINS: &[&str] = &[
    ".",
    ":",
    "[",
    "alias",
    "bg",
    "break",
    "builtin",
    "caller",
    "cd",
    "command",
    "continue",
    "declare",
    "dirs",
    "disown",
    "echo",
    "enable",
    "eval",
    "exec",
    "exit",
    "export",
    "false",
    "fc",
    "fg",
    "getopts",
    "hash",
    "help",
    "history",
    "jobs",
    "kill",
    "let",
    "local",
    "logout",
    "mapfile",
    "popd",
    "printf",
    "pushd",
    "pwd",
    "read",
    "readarray",
    "readonly",
    "return",
    "set",
    "shift",
    "shopt",
    "source",
    "suspend",
    "test",
    "times",
    "trap",
    "true",
    "type",
    "typeset",
    "ulimit",
    "umask",
    "unalias",
    "unset",
    "wait",
];

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use tempfile::TempDir;

    fn write(path: &Path, body: &str) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    #[test]
    fn case_stem_strips_case_toml_suffix() {
        let p = Path::new("tests/spec/cat/foo.case.toml");
        assert_eq!(case_stem(p), PathBuf::from("tests/spec/cat/foo"));
    }

    #[test]
    fn fixture_stem_strips_each_known_extension() {
        for ext in ["stdout", "stderr", "exit", "fs"] {
            let p = PathBuf::from(format!("tests/spec/cat/foo.{ext}"));
            assert_eq!(fixture_stem(&p), PathBuf::from("tests/spec/cat/foo"));
        }
    }

    #[test]
    fn fixture_stem_leaves_unknown_extension_alone() {
        let p = Path::new("tests/spec/cat/foo.txt");
        assert_eq!(fixture_stem(p), PathBuf::from("tests/spec/cat/foo.txt"));
    }

    #[test]
    fn parse_enable_a_extracts_last_token_per_line() {
        let stdout = "enable .\nenable :\nenable -n echo\nenable test\n";
        let parsed = parse_enable_a(stdout);
        let expected: BTreeSet<String> = [".", ":", "echo", "test"]
            .iter()
            .map(|&s| s.to_owned())
            .collect();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_enable_a_ignores_blank_lines() {
        let parsed = parse_enable_a("\n\n");
        assert!(parsed.is_empty());
    }

    #[test]
    fn enumerate_corpus_walks_nested_dirs_and_separates_cases_from_fixtures() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            &root.join("cat1/foo.case.toml"),
            "description = \"x\"\nstatus = \"pass\"\nscript = \"true\\n\"\n",
        );
        write(&root.join("cat1/foo.stdout"), "hi\n");
        write(&root.join("cat1/foo.stderr"), "warn\n");
        write(&root.join("cat1/foo.exit"), "7\n");
        write(
            &root.join("cat2/bar.case.toml"),
            "description = \"x\"\nstatus = \"pass\"\nscript = \"true\\n\"\n",
        );
        // `.fs` subdir with contents — must NOT be recursed into.
        fs::create_dir_all(root.join("cat2/bar.fs/sub")).unwrap();
        write(&root.join("cat2/bar.fs/sub/inner.case.toml"), "ignored");
        // README at root is ignored entirely.
        write(&root.join("README.md"), "");

        let (cases, fixtures) = enumerate_corpus(root).unwrap();
        let case_names: Vec<String> = cases
            .iter()
            .map(|p| p.strip_prefix(root).unwrap().display().to_string())
            .collect();
        assert!(case_names.contains(&"cat1/foo.case.toml".to_owned()));
        assert!(case_names.contains(&"cat2/bar.case.toml".to_owned()));
        // The `.fs/`-contained file MUST NOT be enumerated as a case.
        assert!(!case_names.iter().any(|n| n.contains("inner.case.toml")));

        let fx_names: BTreeSet<String> = fixtures
            .iter()
            .map(|p| p.strip_prefix(root).unwrap().display().to_string())
            .collect();
        assert!(fx_names.contains("cat1/foo.stdout"));
        assert!(fx_names.contains("cat1/foo.stderr"));
        assert!(fx_names.contains("cat1/foo.exit"));
        assert!(fx_names.contains("cat2/bar.fs"));
    }

    #[test]
    fn check_orphans_passes_when_every_fixture_has_a_case() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let case = root.join("a/foo.case.toml");
        write(&case, "");
        let fx = root.join("a/foo.stdout");
        write(&fx, "");
        let violations = check_orphans(&[case], &[fx]);
        assert_eq!(violations, 0);
    }

    #[test]
    fn check_orphans_flags_fixture_without_case() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let fx = root.join("a/foo.stdout");
        write(&fx, "");
        let violations = check_orphans(&[], &[fx]);
        assert_eq!(violations, 1);
    }

    #[test]
    fn check_schema_passes_for_valid_case() {
        let tmp = TempDir::new().unwrap();
        let case = tmp.path().join("a.case.toml");
        write(
            &case,
            "description = \"x\"\nstatus = \"pass\"\nscript = \"true\\n\"\n",
        );
        let violations = check_schema(&[case]);
        assert_eq!(violations, 0);
    }

    #[test]
    fn check_schema_flags_malformed_case() {
        let tmp = TempDir::new().unwrap();
        let case = tmp.path().join("a.case.toml");
        write(&case, "this is not toml = = =");
        let violations = check_schema(&[case]);
        assert_eq!(violations, 1);
    }

    #[test]
    fn expected_builtins_is_sorted_and_unique() {
        let set: BTreeSet<&&str> = EXPECTED_BUILTINS.iter().collect();
        assert_eq!(
            set.len(),
            EXPECTED_BUILTINS.len(),
            "EXPECTED_BUILTINS contains duplicates"
        );
        let mut sorted = EXPECTED_BUILTINS.to_vec();
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            EXPECTED_BUILTINS.to_vec(),
            "EXPECTED_BUILTINS not sorted"
        );
    }

    #[test]
    fn expected_builtins_count_matches_plan_section_11_1() {
        // PLAN_05 §11.1 declares 57 builtins as of bash 5.3p9.
        assert_eq!(EXPECTED_BUILTINS.len(), 57);
    }
}
