// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! `cargo xtask compat` — `PLAN_05` 05.6.
//!
//! Walks `tests/spec/` (tier 1, the only tier in v0), runs every
//! `*.case.toml` through [`fredshell_spec_runner::run_case`], applies
//! the §12 verdict taxonomy via [`fredshell_spec_runner::classify`],
//! and emits two artifacts:
//!
//! * a human-readable summary to stdout, and
//! * an optional machine-readable JSON report (schema v1) to a file
//!   chosen by `--json <path>`.
//!
//! ## Filters
//!
//! * positional `<category>` — restrict to a single category
//!   directory (the first path segment under `tests/spec/`, e.g.
//!   `builtins_tier1`).
//! * `--tier <N>` — restrict to a corpus tier. v0 only knows tier 1;
//!   `--tier 2` / `--tier 3` are accepted but produce an empty set
//!   (those tiers do not yet exist on disk — owned by later subtasks
//!   and `PLAN_13`).
//! * `--status <S>` — restrict to a declared case status. Accepts
//!   `pass`, `fail`, `wontfix`, or `deferred:PLAN_XX`. Mirrors the
//!   §12 taxonomy.
//!
//! ## Exit code
//!
//! `cargo xtask compat` exits 0 unless at least one
//! [`CaseVerdict::Regression`] was recorded, per `PLAN_05` §7.2. This
//! matches `VerdictTally::has_ci_failures`.
//!
//! ## JSON schema v1
//!
//! The on-wire shape is documented in `tests/spec/README.md` and
//! kept stable by the `schema_version: 1` field. Any breaking change
//! to the payload requires bumping the field and adding a migration
//! note in that document.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use clap::Args;
use color_eyre::eyre::{bail, Result};
use fredshell_spec_runner::{
    classify, run_case, Case, CaseOutcome, CaseStatus, CaseVerdict, ReclassifyReason, VerdictTally,
};
use serde::Serialize;

/// `cargo xtask compat` arguments.
#[derive(Args, Debug)]
pub struct CompatArgs {
    /// Restrict to a single category directory (first path segment
    /// under `tests/spec/`). Example: `builtins_tier1`.
    pub category: Option<String>,

    /// Restrict to a corpus tier. v0 only ships tier 1; other tiers
    /// are accepted for forward-compat but match no cases today.
    #[arg(long)]
    pub tier: Option<u8>,

    /// Restrict to a declared case status. Accepts the §12 strings
    /// verbatim: `pass`, `fail`, `wontfix`, `deferred:PLAN_XX`.
    #[arg(long)]
    pub status: Option<String>,

    /// Write the machine-readable JSON report to this path.
    #[arg(long, value_name = "PATH")]
    pub json: Option<PathBuf>,
}

/// Root of the in-tree tier-1 corpus, relative to the workspace
/// (`cargo xtask` runs with the workspace as CWD).
const CORPUS_ROOT: &str = "tests/spec";

/// JSON schema version for `cargo xtask compat` reports. Documented
/// in `tests/spec/README.md`. Bump if any field changes shape.
const SCHEMA_VERSION: u32 = 1;

/// Parsed `--status` filter.
#[derive(Debug, Clone, PartialEq, Eq)]
enum StatusFilter {
    Pass,
    Fail,
    Wontfix,
    Deferred(String),
}

impl StatusFilter {
    fn parse(raw: &str) -> Result<Self> {
        match raw {
            "pass" => Ok(Self::Pass),
            "fail" => Ok(Self::Fail),
            "wontfix" => Ok(Self::Wontfix),
            other => match other.strip_prefix("deferred:") {
                Some(plan) if !plan.is_empty() => Ok(Self::Deferred(plan.to_owned())),
                _ => bail!(
                    "invalid --status {other:?}: expected `pass`, `fail`, `wontfix`, or `deferred:PLAN_XX`"
                ),
            },
        }
    }

    fn matches(&self, status: &CaseStatus) -> bool {
        match (self, status) {
            (Self::Pass, CaseStatus::Pass)
            | (Self::Fail, CaseStatus::Fail)
            | (Self::Wontfix, CaseStatus::Wontfix) => true,
            (Self::Deferred(want), CaseStatus::Deferred(have)) => want == have,
            _ => false,
        }
    }
}

/// Entry point invoked from `main.rs`.
pub fn run(args: &CompatArgs) -> Result<()> {
    let corpus_root = Path::new(CORPUS_ROOT);
    if !corpus_root.is_dir() {
        bail!(
            "compat: corpus root `{}` not found. Run from the workspace root.",
            corpus_root.display()
        );
    }

    let status_filter = args
        .status
        .as_deref()
        .map(StatusFilter::parse)
        .transpose()?;

    // v0 only ships tier 1. `--tier N` for N != 1 is honored as
    // "match nothing" so CI invocations like `--tier 2` do not
    // surprise-fail when tier 2 lands.
    let tier_filter = args.tier;

    let case_paths = discover_cases(corpus_root)?;
    let report = run_all(
        corpus_root,
        &case_paths,
        args.category.as_deref(),
        tier_filter,
        status_filter.as_ref(),
    )?;

    print_summary(&report);

    if let Some(json_path) = &args.json {
        write_json(json_path, &report)?;
    }

    if report.tally.regressions_present {
        // Mirror VerdictTally::has_ci_failures — exit non-zero so CI
        // breaks. color-eyre will print a one-line error; we use
        // process::exit to keep the message short and predictable.
        std::process::exit(1);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Case discovery
// ---------------------------------------------------------------------------

/// Recursively collect every `*.case.toml` under `root`. Returns
/// paths sorted lexically so report output is deterministic.
fn discover_cases(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => bail!("compat: failed to read {}: {e}", dir.display()),
    };
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => bail!("compat: read_dir entry in {}: {e}", dir.display()),
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(e) => bail!("compat: stat {}: {e}", path.display()),
        };

        if file_type.is_dir() {
            // Skip per-case `<name>.fs/` skeleton directories — they
            // are inputs, not corpus entries.
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("fs"))
            {
                continue;
            }
            walk(&path, out)?;
        } else if file_type.is_file() && has_case_toml_suffix(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn has_case_toml_suffix(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".case.toml"))
}

// ---------------------------------------------------------------------------
// Run + classify
// ---------------------------------------------------------------------------

/// Aggregate report passed to the summary and JSON writers.
#[derive(Debug)]
struct Report {
    corpus_root: PathBuf,
    tally: TallySnapshot,
    cases: Vec<CaseRecord>,
}

#[derive(Debug)]
struct CaseRecord {
    /// Path relative to the corpus root, with forward slashes for
    /// determinism across platforms.
    relative_path: String,
    category: String,
    tier: u8,
    status: CaseStatus,
    outcome: CaseOutcome,
    verdict: CaseVerdict,
}

#[derive(Debug)]
struct TallySnapshot {
    expected_pass: usize,
    regression: usize,
    expected_fail: usize,
    wontfix_honored: usize,
    deferred_honored: BTreeMap<String, usize>,
    reclassify: usize,
    total: usize,
    pass_rate_numerator: usize,
    pass_rate_denominator: usize,
    regressions_present: bool,
}

impl TallySnapshot {
    fn from_tally(t: &VerdictTally) -> Self {
        Self {
            expected_pass: t.expected_pass,
            regression: t.regression,
            expected_fail: t.expected_fail,
            wontfix_honored: t.wontfix_honored,
            deferred_honored: t.deferred_honored.clone(),
            reclassify: t.reclassify,
            total: t.total(),
            pass_rate_numerator: t.pass_rate_numerator(),
            pass_rate_denominator: t.pass_rate_denominator(),
            regressions_present: t.has_ci_failures(),
        }
    }
}

fn run_all(
    corpus_root: &Path,
    case_paths: &[PathBuf],
    category_filter: Option<&str>,
    tier_filter: Option<u8>,
    status_filter: Option<&StatusFilter>,
) -> Result<Report> {
    let mut tally = VerdictTally::new();
    let mut cases: Vec<CaseRecord> = Vec::new();

    for case_path in case_paths {
        let relative = relative_to(corpus_root, case_path);
        let category = category_of(&relative);
        let tier = 1_u8; // v0: every in-tree case is tier 1.

        if let Some(want) = category_filter {
            if category != want {
                continue;
            }
        }
        if let Some(want) = tier_filter {
            if tier != want {
                continue;
            }
        }

        let case = match Case::load(case_path) {
            Ok(c) => c,
            Err(e) => bail!("compat: failed to load {}: {e}", case_path.display()),
        };

        if let Some(filter) = status_filter {
            if !filter.matches(&case.status) {
                continue;
            }
        }

        let case_result = match run_case(&case) {
            Ok(r) => r,
            Err(e) => bail!("compat: failed to run {}: {e}", case_path.display()),
        };

        let verdict = classify(&case.status, &case_result.outcome);
        tally.record(&verdict);

        cases.push(CaseRecord {
            relative_path: relative,
            category,
            tier,
            status: case.status,
            outcome: case_result.outcome,
            verdict,
        });
    }

    Ok(Report {
        corpus_root: corpus_root.to_owned(),
        tally: TallySnapshot::from_tally(&tally),
        cases,
    })
}

/// Render the path relative to the corpus root using forward
/// slashes. We accept lossy conversion here — on a UTF-8 filesystem
/// this is the identity, and the harness already refuses non-UTF-8
/// sandbox roots elsewhere.
fn relative_to(root: &Path, case_path: &Path) -> String {
    let rel = case_path.strip_prefix(root).unwrap_or(case_path);
    rel.to_string_lossy().replace('\\', "/")
}

/// First path segment of a relative case path. Empty string when the
/// case is directly under the corpus root (none today).
fn category_of(relative: &str) -> String {
    relative
        .split('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("(root)")
        .to_owned()
}

// ---------------------------------------------------------------------------
// Human-readable summary
// ---------------------------------------------------------------------------

fn print_summary(report: &Report) {
    println!("fredshell compat report");
    println!("=======================");
    println!();
    println!("Corpus root  : {}", report.corpus_root.display());
    println!("Cases run    : {}", report.cases.len());
    println!();
    println!("Tally:");
    println!("  expected_pass    : {}", report.tally.expected_pass);
    println!("  regression       : {}", report.tally.regression);
    println!("  expected_fail    : {}", report.tally.expected_fail);
    println!("  wontfix_honored  : {}", report.tally.wontfix_honored);
    if report.tally.deferred_honored.is_empty() {
        println!("  deferred_honored : (none)");
    } else {
        println!("  deferred_honored :");
        for (plan, count) in &report.tally.deferred_honored {
            println!("    {plan} : {count}");
        }
    }
    println!("  reclassify       : {}", report.tally.reclassify);
    println!("  total            : {}", report.tally.total);

    let (num, den) = (
        report.tally.pass_rate_numerator,
        report.tally.pass_rate_denominator,
    );
    if den == 0 {
        println!("  pass rate        : n/a ({num}/{den})");
    } else {
        // xtask is allowed `as` casts (AGENTS.md). For two small
        // usize counters bounded by the corpus size, the round-trip
        // through f64 is lossless in practice.
        #[allow(clippy::cast_precision_loss)]
        let pct = (num as f64 / den as f64) * 100.0;
        println!("  pass rate        : {num}/{den} ({pct:.1}%)");
    }

    // Reclassify lines first — these are the §12.1 signals that PR
    // authors need to act on even though CI does not fail.
    let reclassify_lines: Vec<&CaseRecord> = report
        .cases
        .iter()
        .filter(|c| c.verdict.is_reclassify())
        .collect();
    if !reclassify_lines.is_empty() {
        println!();
        for c in reclassify_lines {
            if let CaseVerdict::Reclassify {
                from,
                suggested,
                reason,
            } = &c.verdict
            {
                println!(
                    "RECLASSIFY: {} (status `{from}` → `{suggested}`): {reason}",
                    c.relative_path
                );
            }
        }
    }

    // Regressions are the build-breaking signals. Print each so the
    // operator can find them without re-running with `--json`.
    let regressions: Vec<&CaseRecord> = report
        .cases
        .iter()
        .filter(|c| matches!(c.verdict, CaseVerdict::Regression))
        .collect();
    if !regressions.is_empty() {
        println!();
        for c in &regressions {
            println!("REGRESSION: {}", c.relative_path);
        }
    }

    println!();
    if report.tally.regressions_present {
        println!("result: REGRESSIONS PRESENT (CI fails)");
    } else {
        println!("result: ok");
    }
}

// ---------------------------------------------------------------------------
// JSON v1 report
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct JsonReport<'a> {
    schema_version: u32,
    corpus_root: String,
    tally: JsonTally<'a>,
    cases: Vec<JsonCase<'a>>,
}

#[derive(Debug, Serialize)]
struct JsonTally<'a> {
    expected_pass: usize,
    regression: usize,
    expected_fail: usize,
    wontfix_honored: usize,
    deferred_honored: &'a BTreeMap<String, usize>,
    reclassify: usize,
    total: usize,
    pass_rate_numerator: usize,
    pass_rate_denominator: usize,
    regressions_present: bool,
}

#[derive(Debug, Serialize)]
struct JsonCase<'a> {
    path: &'a str,
    category: &'a str,
    tier: u8,
    status: String,
    outcome: JsonOutcome,
    verdict: JsonVerdict,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum JsonOutcome {
    Pass,
    Mismatch {
        observed_stdout_b64: String,
        observed_stderr_b64: String,
        observed_exit: i32,
    },
    ExecutorRefused {
        command: String,
        reason: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum JsonVerdict {
    ExpectedPass,
    Regression,
    ExpectedFail,
    WontfixHonored,
    DeferredHonored {
        plan: String,
    },
    Reclassify {
        from: String,
        suggested: String,
        reason: String,
    },
}

fn outcome_to_json(outcome: &CaseOutcome) -> JsonOutcome {
    let b64 = base64::engine::general_purpose::STANDARD;
    // CaseOutcome is `#[non_exhaustive]`; future variants must be
    // mapped here. Until then the wildcard arm keeps the match total
    // for external semver but the in-crate match is exhaustive — the
    // scoped allow silences `unreachable_patterns`.
    #[allow(unreachable_patterns)]
    match outcome {
        CaseOutcome::Pass => JsonOutcome::Pass,
        CaseOutcome::Mismatch {
            observed_stdout,
            observed_stderr,
            observed_exit,
        } => JsonOutcome::Mismatch {
            observed_stdout_b64: b64.encode(observed_stdout),
            observed_stderr_b64: b64.encode(observed_stderr),
            observed_exit: *observed_exit,
        },
        CaseOutcome::ExecutorRefused { command, reason } => JsonOutcome::ExecutorRefused {
            command: command.clone(),
            reason: format!("{reason:?}"),
        },
        _ => JsonOutcome::ExecutorRefused {
            command: String::new(),
            reason: "unknown_outcome_variant".to_owned(),
        },
    }
}

fn verdict_to_json(verdict: &CaseVerdict) -> JsonVerdict {
    #[allow(unreachable_patterns)]
    match verdict {
        CaseVerdict::ExpectedPass => JsonVerdict::ExpectedPass,
        CaseVerdict::Regression => JsonVerdict::Regression,
        CaseVerdict::ExpectedFail => JsonVerdict::ExpectedFail,
        CaseVerdict::WontfixHonored => JsonVerdict::WontfixHonored,
        CaseVerdict::DeferredHonored { plan } => {
            JsonVerdict::DeferredHonored { plan: plan.clone() }
        }
        CaseVerdict::Reclassify {
            from,
            suggested,
            reason,
        } => JsonVerdict::Reclassify {
            from: from.to_string(),
            suggested: suggested.to_string(),
            reason: reclassify_reason_str(reason).to_owned(),
        },
        _ => JsonVerdict::Reclassify {
            from: String::new(),
            suggested: String::new(),
            reason: "unknown_verdict_variant".to_owned(),
        },
    }
}

#[allow(clippy::missing_const_for_fn)]
fn reclassify_reason_str(reason: &ReclassifyReason) -> &'static str {
    #[allow(unreachable_patterns)]
    match reason {
        ReclassifyReason::OutcomeMatchedDespiteNonPassStatus => {
            "outcome_matched_despite_non_pass_status"
        }
        _ => "unknown_reclassify_reason",
    }
}

fn build_json<'a>(report: &'a Report) -> JsonReport<'a> {
    let cases: Vec<JsonCase<'a>> = report
        .cases
        .iter()
        .map(|c| JsonCase {
            path: &c.relative_path,
            category: &c.category,
            tier: c.tier,
            status: c.status.to_string(),
            outcome: outcome_to_json(&c.outcome),
            verdict: verdict_to_json(&c.verdict),
        })
        .collect();

    JsonReport {
        schema_version: SCHEMA_VERSION,
        corpus_root: report.corpus_root.to_string_lossy().to_string(),
        tally: JsonTally {
            expected_pass: report.tally.expected_pass,
            regression: report.tally.regression,
            expected_fail: report.tally.expected_fail,
            wontfix_honored: report.tally.wontfix_honored,
            deferred_honored: &report.tally.deferred_honored,
            reclassify: report.tally.reclassify,
            total: report.tally.total,
            pass_rate_numerator: report.tally.pass_rate_numerator,
            pass_rate_denominator: report.tally.pass_rate_denominator,
            regressions_present: report.tally.regressions_present,
        },
        cases,
    }
}

fn write_json(path: &Path, report: &Report) -> Result<()> {
    let json = build_json(report);
    let serialized = match serde_json::to_string_pretty(&json) {
        Ok(s) => s,
        Err(e) => bail!("compat: failed to serialize JSON report: {e}"),
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = fs::create_dir_all(parent) {
                bail!(
                    "compat: failed to create {} for JSON report: {e}",
                    parent.display()
                );
            }
        }
    }
    if let Err(e) = fs::write(path, format!("{serialized}\n")) {
        bail!("compat: failed to write {}: {e}", path.display());
    }
    println!("wrote JSON report: {}", path.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn status_filter_parses_each_variant() {
        assert_eq!(StatusFilter::parse("pass").unwrap(), StatusFilter::Pass);
        assert_eq!(StatusFilter::parse("fail").unwrap(), StatusFilter::Fail);
        assert_eq!(
            StatusFilter::parse("wontfix").unwrap(),
            StatusFilter::Wontfix
        );
        assert_eq!(
            StatusFilter::parse("deferred:PLAN_06b").unwrap(),
            StatusFilter::Deferred("PLAN_06b".to_owned())
        );
    }

    #[test]
    fn status_filter_rejects_garbage() {
        assert!(StatusFilter::parse("nonsense").is_err());
        assert!(StatusFilter::parse("deferred:").is_err());
    }

    #[test]
    fn status_filter_matches_case_status() {
        let f = StatusFilter::Deferred("PLAN_06b".to_owned());
        assert!(f.matches(&CaseStatus::Deferred("PLAN_06b".to_owned())));
        assert!(!f.matches(&CaseStatus::Deferred("PLAN_09a".to_owned())));
        assert!(!f.matches(&CaseStatus::Pass));

        let p = StatusFilter::Pass;
        assert!(p.matches(&CaseStatus::Pass));
        assert!(!p.matches(&CaseStatus::Fail));
    }

    #[test]
    fn discover_cases_finds_only_case_toml_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let cat = root.join("cat_a");
        fs::create_dir_all(&cat).unwrap();
        fs::write(cat.join("one.case.toml"), b"x").unwrap();
        fs::write(cat.join("two.case.toml"), b"x").unwrap();
        fs::write(cat.join("README.md"), b"x").unwrap();
        fs::write(cat.join("ignored.toml"), b"x").unwrap();
        // `.fs/` skeleton directories must be skipped.
        let skel = cat.join("one.fs");
        fs::create_dir_all(&skel).unwrap();
        fs::write(skel.join("not_a_case.case.toml"), b"x").unwrap();

        let found = discover_cases(root).unwrap();
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["one.case.toml", "two.case.toml"]);
    }

    #[test]
    fn category_of_uses_first_segment() {
        assert_eq!(
            category_of("builtins_tier1/exit_zero.case.toml"),
            "builtins_tier1"
        );
        assert_eq!(category_of("nested/deep/case.case.toml"), "nested");
        assert_eq!(category_of(""), "(root)");
    }

    #[test]
    fn build_json_emits_schema_version_one_with_pass_case() {
        let r = Report {
            corpus_root: PathBuf::from("tests/spec"),
            tally: TallySnapshot {
                expected_pass: 1,
                regression: 0,
                expected_fail: 0,
                wontfix_honored: 0,
                deferred_honored: BTreeMap::new(),
                reclassify: 0,
                total: 1,
                pass_rate_numerator: 1,
                pass_rate_denominator: 1,
                regressions_present: false,
            },
            cases: vec![CaseRecord {
                relative_path: "builtins_tier1/exit_zero.case.toml".to_owned(),
                category: "builtins_tier1".to_owned(),
                tier: 1,
                status: CaseStatus::Pass,
                outcome: CaseOutcome::Pass,
                verdict: CaseVerdict::ExpectedPass,
            }],
        };
        let json = build_json(&r);
        let s = serde_json::to_string(&json).unwrap();
        assert!(s.contains("\"schema_version\":1"));
        assert!(s.contains("\"kind\":\"pass\""));
        assert!(s.contains("\"kind\":\"expected_pass\""));
        assert!(s.contains("\"regressions_present\":false"));
    }

    #[test]
    fn build_json_encodes_mismatch_bytes_as_base64() {
        let r = Report {
            corpus_root: PathBuf::from("tests/spec"),
            tally: TallySnapshot {
                expected_pass: 0,
                regression: 1,
                expected_fail: 0,
                wontfix_honored: 0,
                deferred_honored: BTreeMap::new(),
                reclassify: 0,
                total: 1,
                pass_rate_numerator: 0,
                pass_rate_denominator: 1,
                regressions_present: true,
            },
            cases: vec![CaseRecord {
                relative_path: "x/y.case.toml".to_owned(),
                category: "x".to_owned(),
                tier: 1,
                status: CaseStatus::Pass,
                outcome: CaseOutcome::Mismatch {
                    observed_stdout: b"hi\n".to_vec(),
                    observed_stderr: Vec::new(),
                    observed_exit: 7,
                },
                verdict: CaseVerdict::Regression,
            }],
        };
        let json = build_json(&r);
        let s = serde_json::to_string(&json).unwrap();
        // base64("hi\n") = "aGkK"
        assert!(s.contains("\"observed_stdout_b64\":\"aGkK\""));
        assert!(s.contains("\"observed_exit\":7"));
        assert!(s.contains("\"kind\":\"regression\""));
    }

    #[test]
    fn build_json_records_deferred_plan_in_verdict_and_tally() {
        let mut deferred = BTreeMap::new();
        deferred.insert("PLAN_06b".to_owned(), 2);
        let r = Report {
            corpus_root: PathBuf::from("tests/spec"),
            tally: TallySnapshot {
                expected_pass: 0,
                regression: 0,
                expected_fail: 0,
                wontfix_honored: 0,
                deferred_honored: deferred,
                reclassify: 0,
                total: 2,
                pass_rate_numerator: 0,
                pass_rate_denominator: 2,
                regressions_present: false,
            },
            cases: vec![CaseRecord {
                relative_path: "x/y.case.toml".to_owned(),
                category: "x".to_owned(),
                tier: 1,
                status: CaseStatus::Deferred("PLAN_06b".to_owned()),
                outcome: CaseOutcome::Pass,
                verdict: CaseVerdict::DeferredHonored {
                    plan: "PLAN_06b".to_owned(),
                },
            }],
        };
        let json = build_json(&r);
        let s = serde_json::to_string(&json).unwrap();
        assert!(s.contains("\"deferred_honored\":{\"PLAN_06b\":2}"));
        assert!(s.contains("\"kind\":\"deferred_honored\""));
        assert!(s.contains("\"plan\":\"PLAN_06b\""));
    }
}
