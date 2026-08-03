// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! `cargo xtask check-specs` — the spec-sheet cross-reference checker.
//!
//! Per `PLAN_07` §8.1 this command walks every spec sheet under
//! `Documents/specs/` and verifies five invariants:
//!
//! 1. **Support rows resolve.** Every `support` row in §3 of a sheet
//!    names a corpus case at the listed path under `tests/spec/`, and
//!    that case declares `status = "pass"` or
//!    `status = "deferred:PLAN_XX"`.
//! 2. **Cases are owned.** Every corpus case under `tests/spec/` is
//!    referenced by exactly one sheet row. A case referenced by zero
//!    sheets is an orphan; a case referenced by more than one sheet
//!    is a conflict. Both fail.
//! 3. **No unclassified rows.** No §3 row carries the placeholder
//!    classification `???`.
//! 4. **Mandatory sections present, in order.** Every sheet carries
//!    the seven mandatory sections (`## 1. Synopsis` …
//!    `## 6. Deferred rows`, `## 8. References`) in order. The
//!    conditional `## 7. POSIX divergence` may appear between §6 and
//!    §8 but is not required.
//! 5. **Deferred rows carry a workaround.** Every `defer:N` row in §3
//!    is backed by a paragraph in §6 (Deferred rows).
//!
//! The command is not yet wired into `cargo xtask pc` / `check`: the
//! sheet inventory is drafted incrementally across `PLAN_07` subtasks
//! 08.2, 08.3, and 08.6, so check 2 (every case owned) cannot pass
//! until 08.6 lands. The `pc`/`check` wiring is added in the 08.6
//! completion commit, once the inventory is complete. Until then the
//! command is run manually on the `task-07/spec-drafting` branch.
//!
//! The sheet format is defined by `PLAN_07` §4 and the canonical
//! template at `Documents/specs/_TEMPLATE.md`. The parser here is
//! deliberately minimal and line-oriented, matching the house style
//! of the sibling `spec` module: a real Markdown parser would pull a
//! dependency into `xtask` that no other code needs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use color_eyre::eyre::{bail, Result};
use fredshell_spec_runner::{Case, CaseStatus};

/// Workspace-root-relative path to the spec-sheet tree.
const SPECS_ROOT: &str = "Documents/specs";

/// Workspace-root-relative path to the corpus root. Support-row
/// `Corpus` cells are interpreted relative to this directory.
const CORPUS_ROOT: &str = "tests/spec";

/// The seven mandatory section headings, in order. `## 7. POSIX
/// divergence` is conditional (it appears only when fredshell follows
/// bash and POSIX disagrees) and is therefore not in this list; the
/// ordering check tolerates it appearing between §6 and §8.
const MANDATORY_SECTIONS: &[&str] = &[
    "## 1. Synopsis",
    "## 2. Description",
    "## 3. Support matrix",
    "## 4. Bash quirks",
    "## 5. Wontfix rationale",
    "## 6. Deferred rows",
    "## 8. References",
];

/// The optional POSIX-divergence section heading. Permitted between
/// §6 and §8 but never required.
const OPTIONAL_POSIX_SECTION: &str = "## 7. POSIX divergence";

/// The classification recorded in a §3 support-matrix row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    /// `support` — fredshell replicates bash; a corpus case is
    /// required.
    Support,
    /// `wontfix` — fredshell refuses the invocation.
    Wontfix,
    /// `defer:N` — supported after milestone `N`. The payload is the
    /// raw milestone token (e.g. `"2"`).
    Defer(String),
    /// `???` — the unfilled placeholder. Always a violation.
    Unclassified,
}

impl Classification {
    /// Parse the Classification cell of a §3 row. Returns `None` when
    /// the cell is not a recognised classification (the row is then
    /// ignored — only the four recognised forms participate in the
    /// checks).
    fn parse(cell: &str) -> Option<Self> {
        let cell = cell.trim();
        match cell {
            "support" => Some(Self::Support),
            "wontfix" => Some(Self::Wontfix),
            "???" => Some(Self::Unclassified),
            _ => cell
                .strip_prefix("defer:")
                .map(|n| Self::Defer(n.trim().to_owned())),
        }
    }
}

/// One row parsed from a sheet's §3 support matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportRow {
    /// The row number from the first column (e.g. `"3.1"`).
    pub number: String,
    /// The row's classification.
    pub classification: Classification,
    /// The raw `Corpus` cell, stripped of surrounding backticks. Only
    /// meaningful for `support` rows; `n/a …` and empty for the rest.
    pub corpus: String,
}

/// A fully parsed sheet, reduced to the fields the checker needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    /// The sheet's id (filename without `.md`), used in diagnostics.
    pub id: String,
    /// The §3 support-matrix rows, in document order.
    pub rows: Vec<SupportRow>,
    /// The `## N. <name>` headings encountered, in document order.
    pub sections: Vec<String>,
    /// The `## 6. Deferred rows` body text, joined with newlines.
    /// Check 5 requires each `defer:N` row number to appear here, so
    /// the prose is retained rather than reduced to a boolean.
    pub deferred_body: String,
}

/// Errors surfaced while parsing a single sheet. Returned as
/// violations rather than aborting the whole run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetError {
    /// A §3 row's Classification cell held the `???` placeholder.
    UnclassifiedRow { row: String },
    /// The mandatory sections were absent or out of order.
    SectionOrder { detail: String },
    /// A `defer:N` row had no backing paragraph in §6.
    MissingWorkaround { row: String },
}

impl core::fmt::Display for SheetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnclassifiedRow { row } => {
                write!(f, "row {row} is unclassified (`???`)")
            }
            Self::SectionOrder { detail } => {
                write!(f, "section structure invalid: {detail}")
            }
            Self::MissingWorkaround { row } => write!(
                f,
                "defer row {row} has no workaround paragraph in §6 (Deferred rows)"
            ),
        }
    }
}

/// Parse a sheet's Markdown body into a [`Sheet`].
///
/// The parser walks lines and recognises three constructs:
///
/// * `## N. <name>` headings (collected into `sections`).
/// * §3 support-matrix table rows (lines beginning with `|` whose
///   first cell is a `<major>.<minor>` row number).
/// * The §6 body (any non-blank, non-heading line after the
///   `## 6. Deferred rows` heading and before the next heading).
///
/// Table rows outside §3 are ignored: only rows whose first cell is a
/// dotted row number and whose third cell is a recognised
/// classification are treated as support rows, which excludes the
/// header (`| #   | Behaviour | … |`) and separator
/// (`| --- | --- | … |`) lines as well as illustrative tables in
/// other sections.
pub fn parse_sheet(id: &str, body: &str) -> Sheet {
    let mut rows: Vec<SupportRow> = Vec::new();
    let mut sections: Vec<String> = Vec::new();
    let mut deferred_body = String::new();

    // Track which `## ` section we are currently inside so the §6
    // body scan does not pick up text from other sections.
    let mut current_section: Option<String> = None;

    for line in body.lines() {
        let trimmed = line.trim();

        if let Some(heading) = parse_h2(trimmed) {
            sections.push(heading.clone());
            current_section = Some(heading);
            continue;
        }

        // §6 body collection: any non-blank line under the Deferred
        // rows heading that is not itself a heading counts as body.
        if current_section.as_deref() == Some("## 6. Deferred rows") && !trimmed.is_empty() {
            deferred_body.push_str(trimmed);
            deferred_body.push('\n');
        }

        if let Some(row) = parse_support_row(trimmed) {
            rows.push(row);
        }
    }

    Sheet {
        id: id.to_owned(),
        rows,
        sections,
        deferred_body,
    }
}

/// Recognise a `## N. <name>` level-2 heading and return it verbatim
/// (trimmed). Returns `None` for any other line, including `###`
/// sub-headers and `#` titles.
fn parse_h2(line: &str) -> Option<String> {
    if line.starts_with("## ") && !line.starts_with("### ") {
        Some(line.to_owned())
    } else {
        None
    }
}

/// Parse a `| 3.1 | … | support | `cat/foo.case.toml` |` table row.
///
/// Returns `None` unless the line is a pipe-delimited row whose first
/// cell is a dotted row number and whose Classification cell parses.
/// This intentionally rejects the header and separator rows and any
/// illustrative table elsewhere in the sheet.
fn parse_support_row(line: &str) -> Option<SupportRow> {
    if !line.starts_with('|') {
        return None;
    }
    // Split on `|`, dropping only the empty leading/trailing fields
    // produced by the surrounding pipes. Interior empty cells are
    // retained: filtering them would shift later columns left and make
    // a blank `Corpus` cell indistinguishable from a 3-column row.
    let mut cells: Vec<&str> = line.split('|').map(str::trim).collect();
    if cells.first().is_some_and(|s| s.is_empty()) {
        cells.remove(0);
    }
    if cells.last().is_some_and(|s| s.is_empty()) {
        cells.pop();
    }
    // Expected columns: # | Behaviour | Classification | Corpus.
    if cells.len() < 4 {
        return None;
    }
    let number = cells[0];
    if !is_row_number(number) {
        return None;
    }
    let classification = Classification::parse(cells[2])?;
    let corpus = strip_code_span(cells[3]);
    Some(SupportRow {
        number: number.to_owned(),
        classification,
        corpus,
    })
}

/// True when `body` cites `row` as a row number, e.g. `3.8` in
/// "**3.8 / 3.9 — `-L` / `-P` …**".
///
/// The match is digit-boundary aware: a bare `contains` would let a
/// §6 paragraph documenting only `3.13` satisfy a `defer` row `3.1`,
/// so an occurrence followed by another digit or a further `.` does
/// not count.
fn mentions_row(body: &str, row: &str) -> bool {
    body.match_indices(row).any(|(idx, _)| {
        let after = body[idx + row.len()..].chars().next();
        // Reject `3.1` matching inside `3.13` or `3.1.2`.
        !matches!(after, Some(c) if c.is_ascii_digit() || c == '.')
    })
}

/// A §3 row number is `<digits>.<digits>` (e.g. `3.1`, `3.12`).
fn is_row_number(s: &str) -> bool {
    match s.split_once('.') {
        Some((major, minor)) => {
            !major.is_empty()
                && !minor.is_empty()
                && major.bytes().all(|b| b.is_ascii_digit())
                && minor.bytes().all(|b| b.is_ascii_digit())
        }
        None => false,
    }
}

/// Strip a single surrounding pair of backticks from a cell, if
/// present, and trim. Leaves non-code cells (e.g. `n/a — see §5`)
/// untouched apart from trimming.
fn strip_code_span(cell: &str) -> String {
    let cell = cell.trim();
    cell.strip_prefix('`')
        .and_then(|s| s.strip_suffix('`'))
        .unwrap_or(cell)
        .trim()
        .to_owned()
}

/// Validate a parsed sheet's intrinsic structure (checks 3, 4, 5).
/// Cross-corpus checks (1, 2) are performed separately because they
/// need the corpus-case index.
pub fn validate_sheet_structure(sheet: &Sheet) -> Vec<SheetError> {
    let mut errors: Vec<SheetError> = Vec::new();

    // Check 4: mandatory sections present, in order, with §7
    // tolerated between §6 and §8.
    if let Err(detail) = check_section_order(&sheet.sections) {
        errors.push(SheetError::SectionOrder { detail });
    }

    for row in &sheet.rows {
        // Check 3: no unclassified rows.
        if row.classification == Classification::Unclassified {
            errors.push(SheetError::UnclassifiedRow {
                row: row.number.clone(),
            });
        }
        // Check 5: every defer row needs a §6 workaround paragraph.
        if matches!(row.classification, Classification::Defer(_))
            && !mentions_row(&sheet.deferred_body, &row.number)
        {
            errors.push(SheetError::MissingWorkaround {
                row: row.number.clone(),
            });
        }
    }

    errors
}

/// Check that the seven mandatory headings appear, in order, in the
/// observed heading list. The optional §7 heading is permitted but
/// not required. Extra `## ` headings that are not part of the
/// mandatory set are ignored (the template uses only the canonical
/// set, but tolerating extras keeps the check robust).
fn check_section_order(observed: &[String]) -> Result<(), String> {
    // Filter the observed headings down to the canonical set
    // (mandatory + optional POSIX), preserving order.
    let canonical: Vec<&str> = observed
        .iter()
        .map(String::as_str)
        .filter(|h| MANDATORY_SECTIONS.contains(h) || *h == OPTIONAL_POSIX_SECTION)
        .collect();

    // Every mandatory section must be present.
    for want in MANDATORY_SECTIONS {
        if !canonical.contains(want) {
            return Err(format!("missing mandatory section `{want}`"));
        }
    }

    // The mandatory sections must appear in the prescribed order. We
    // build the expected sequence (mandatory in order, with the
    // optional §7 slotted between §6 and §8 if present) and compare.
    let mut expected: Vec<&str> = Vec::new();
    for s in MANDATORY_SECTIONS {
        if *s == "## 8. References" && canonical.contains(&OPTIONAL_POSIX_SECTION) {
            expected.push(OPTIONAL_POSIX_SECTION);
        }
        expected.push(s);
    }

    if canonical != expected {
        return Err(format!(
            "sections out of order: expected {expected:?}, found {canonical:?}"
        ));
    }
    Ok(())
}

/// A reference from a sheet's support row to a corpus case path.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CorpusRef {
    sheet_id: String,
    row: String,
    /// Path relative to [`CORPUS_ROOT`].
    corpus_rel: String,
}

/// Entry point for `cargo xtask check-specs`.
pub fn run() -> Result<()> {
    println!("fredshell check-specs");
    println!("=====================");
    println!();

    let specs_root = Path::new(SPECS_ROOT);
    if !specs_root.is_dir() {
        bail!(
            "check-specs: spec root {} is not a directory",
            specs_root.display()
        );
    }

    let sheet_paths = enumerate_sheets(specs_root)?;
    println!("[sheets] {} sheet file(s)", sheet_paths.len());

    let mut violations: usize = 0;
    let mut all_refs: Vec<CorpusRef> = Vec::new();

    for path in &sheet_paths {
        let id = sheet_id(path);
        let body = fs::read_to_string(path)
            .map_err(|e| color_eyre::eyre::eyre!("read {}: {e}", path.display()))?;
        let sheet = parse_sheet(&id, &body);

        // Checks 3, 4, 5: intrinsic structure.
        for err in validate_sheet_structure(&sheet) {
            violations += 1;
            println!("  FAIL {}: {err}", path.display());
        }

        // Check 1: support rows resolve to a valid corpus case.
        for row in &sheet.rows {
            if row.classification == Classification::Support {
                violations += check_support_row_resolves(
                    Path::new(CORPUS_ROOT),
                    &sheet.id,
                    row,
                    &mut all_refs,
                );
            }
        }
    }

    // Check 2: every corpus case is referenced by exactly one row.
    let corpus_cases = enumerate_corpus_cases(Path::new(CORPUS_ROOT))?;
    violations += check_every_case_owned(&corpus_cases, &all_refs);

    println!();
    if violations == 0 {
        println!("OK ({} sheet(s) checked)", sheet_paths.len());
        Ok(())
    } else {
        bail!("check-specs: {violations} violation(s) found");
    }
}

/// Check 1 for a single support row: the named corpus case must exist
/// and declare `pass` or `deferred:PLAN_XX`. Records the reference in
/// `all_refs` for the check-2 ownership pass. Returns the number of
/// violations (0 or 1).
///
/// `corpus_root` is passed explicitly (rather than read from the
/// [`CORPUS_ROOT`] constant) so tests can point it at a temp
/// directory without mutating the process-wide working directory,
/// keeping them hermetic and order-independent.
fn check_support_row_resolves(
    corpus_root: &Path,
    sheet_id: &str,
    row: &SupportRow,
    all_refs: &mut Vec<CorpusRef>,
) -> usize {
    let corpus_rel = row.corpus.clone();
    if corpus_rel.is_empty() {
        println!(
            "  FAIL {sheet_id}-{}: support row has empty Corpus cell",
            row.number
        );
        return 1;
    }

    all_refs.push(CorpusRef {
        sheet_id: sheet_id.to_owned(),
        row: row.number.clone(),
        corpus_rel: corpus_rel.clone(),
    });

    let case_path = corpus_root.join(&corpus_rel);
    let case = match Case::load(&case_path) {
        Ok(c) => c,
        Err(e) => {
            println!(
                "  FAIL {sheet_id}-{}: corpus case {} does not load: {e}",
                row.number,
                case_path.display()
            );
            return 1;
        }
    };

    match &case.status {
        CaseStatus::Pass | CaseStatus::Deferred(_) => 0,
        other => {
            println!(
                "  FAIL {sheet_id}-{}: corpus case {} has status `{other}`, \
                 must be `pass` or `deferred:PLAN_XX`",
                row.number,
                case_path.display()
            );
            1
        }
    }
}

/// Check 2: every corpus case under `tests/spec/` is referenced by
/// exactly one sheet row. Zero references is an orphan; more than one
/// is a conflict. Returns the violation count.
fn check_every_case_owned(corpus_cases: &[PathBuf], all_refs: &[CorpusRef]) -> usize {
    // Index references by their corpus-relative path.
    let mut by_path: BTreeMap<String, Vec<&CorpusRef>> = BTreeMap::new();
    for r in all_refs {
        by_path.entry(r.corpus_rel.clone()).or_default().push(r);
    }

    println!(
        "[ownership] {} corpus case(s), {} support reference(s)",
        corpus_cases.len(),
        all_refs.len()
    );

    let mut violations: usize = 0;
    for case in corpus_cases {
        let rel = corpus_rel_path(case);
        match by_path.get(&rel) {
            None => {
                violations += 1;
                println!("  FAIL orphan corpus case {rel} is referenced by no sheet row");
            }
            Some(refs) if refs.len() > 1 => {
                violations += 1;
                let owners: Vec<String> = refs
                    .iter()
                    .map(|r| format!("{}-{}", r.sheet_id, r.row))
                    .collect();
                println!(
                    "  FAIL corpus case {rel} is referenced by {} rows ({})",
                    refs.len(),
                    owners.join(", ")
                );
            }
            Some(_) => {}
        }
    }

    if violations == 0 {
        println!("[ownership] OK");
    } else {
        println!("[ownership] {violations} ownership violation(s)");
    }
    violations
}

/// `tests/spec/cat/foo.case.toml` → `cat/foo.case.toml` (relative to
/// [`CORPUS_ROOT`], forward slashes).
fn corpus_rel_path(case: &Path) -> String {
    case.strip_prefix(CORPUS_ROOT)
        .unwrap_or(case)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Enumerate every `.md` sheet under `Documents/specs/`, excluding the
/// template (`_TEMPLATE.md`) and the index (`README.md`). Recurses
/// into `builtins/` and `features/`.
fn enumerate_sheets(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = Vec::new();
    walk_sheets(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_sheets(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in
        fs::read_dir(dir).map_err(|e| color_eyre::eyre::eyre!("read_dir {}: {e}", dir.display()))?
    {
        let entry = entry
            .map_err(|e| color_eyre::eyre::eyre!("read_dir entry in {}: {e}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| color_eyre::eyre::eyre!("file_type {}: {e}", path.display()))?;
        if file_type.is_dir() {
            walk_sheets(&path, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        // Skip the template, the index, and any non-Markdown file.
        if name == "_TEMPLATE.md" || name == "README.md" {
            continue;
        }
        #[allow(clippy::case_sensitive_file_extension_comparisons)]
        let is_md = name.ends_with(".md");
        if is_md {
            out.push(path);
        }
    }
    Ok(())
}

/// Enumerate every `.case.toml` under the corpus root.
fn enumerate_corpus_cases(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = Vec::new();
    walk_cases(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_cases(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in
        fs::read_dir(dir).map_err(|e| color_eyre::eyre::eyre!("read_dir {}: {e}", dir.display()))?
    {
        let entry = entry
            .map_err(|e| color_eyre::eyre::eyre!("read_dir entry in {}: {e}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| color_eyre::eyre::eyre!("file_type {}: {e}", path.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if file_type.is_dir() {
            // `.fs/` skeleton dirs are fixtures, not case containers.
            #[allow(clippy::case_sensitive_file_extension_comparisons)]
            let is_fs_dir = name.ends_with(".fs");
            if !is_fs_dir {
                walk_cases(&path, out)?;
            }
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        #[allow(clippy::case_sensitive_file_extension_comparisons)]
        let is_case = name.ends_with(".case.toml");
        if is_case {
            out.push(path);
        }
    }
    Ok(())
}

/// `Documents/specs/builtins/cd.md` → `cd`.
fn sheet_id(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    name.strip_suffix(".md").unwrap_or(name).to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(path: &Path, body: &str) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    /// A minimal but structurally valid sheet body with the seven
    /// mandatory sections and a single support row.
    fn valid_sheet_body(corpus: &str) -> String {
        format!(
            "# `cd` — change the working directory\n\
             \n\
             ## 1. Synopsis\n\nx\n\n\
             ## 2. Description\n\nx\n\n\
             ## 3. Support matrix\n\n\
             | #   | Behaviour | Classification | Corpus |\n\
             | --- | --------- | -------------- | ------ |\n\
             | 3.1 | `cd dir`  | support        | `{corpus}` |\n\n\
             ## 4. Bash quirks\n\nx\n\n\
             ## 5. Wontfix rationale\n\nx\n\n\
             ## 6. Deferred rows\n\nNone.\n\n\
             ## 8. References\n\nx\n"
        )
    }

    fn pass_case() -> &'static str {
        "description = \"x\"\nstatus = \"pass\"\nscript = \"true\\n\"\n"
    }

    // --- Classification::parse -------------------------------------

    #[test]
    fn classification_parses_each_form() {
        assert_eq!(
            Classification::parse("support"),
            Some(Classification::Support)
        );
        assert_eq!(
            Classification::parse("wontfix"),
            Some(Classification::Wontfix)
        );
        assert_eq!(
            Classification::parse("???"),
            Some(Classification::Unclassified)
        );
        assert_eq!(
            Classification::parse("defer:2"),
            Some(Classification::Defer("2".to_owned()))
        );
        assert_eq!(
            Classification::parse("defer: 3"),
            Some(Classification::Defer("3".to_owned()))
        );
        assert_eq!(Classification::parse("nonsense"), None);
    }

    // --- row-number recognition ------------------------------------

    #[test]
    fn is_row_number_accepts_dotted_numbers() {
        assert!(is_row_number("3.1"));
        assert!(is_row_number("3.12"));
        assert!(is_row_number("10.4"));
    }

    #[test]
    fn is_row_number_rejects_non_rows() {
        assert!(!is_row_number("#"));
        assert!(!is_row_number("---"));
        assert!(!is_row_number("3"));
        assert!(!is_row_number("3."));
        assert!(!is_row_number(".1"));
        assert!(!is_row_number("Behaviour"));
    }

    // --- strip_code_span -------------------------------------------

    #[test]
    fn strip_code_span_unwraps_backticks() {
        assert_eq!(strip_code_span("`cat/foo.case.toml`"), "cat/foo.case.toml");
        assert_eq!(strip_code_span("n/a — see §5"), "n/a — see §5");
        assert_eq!(strip_code_span("  `x`  "), "x");
    }

    // --- parse_support_row -----------------------------------------

    #[test]
    fn parse_support_row_reads_a_support_row() {
        let row = parse_support_row("| 3.1 | `cd dir` | support | `builtins/cd_dir.case.toml` |")
            .expect("row");
        assert_eq!(row.number, "3.1");
        assert_eq!(row.classification, Classification::Support);
        assert_eq!(row.corpus, "builtins/cd_dir.case.toml");
    }

    #[test]
    fn parse_support_row_ignores_header_and_separator() {
        assert!(parse_support_row("| #   | Behaviour | Classification | Corpus |").is_none());
        assert!(parse_support_row("| --- | --- | --- | --- |").is_none());
    }

    #[test]
    fn parse_support_row_ignores_non_table_lines() {
        assert!(parse_support_row("Some prose.").is_none());
        assert!(parse_support_row("## 3. Support matrix").is_none());
    }

    #[test]
    fn parse_support_row_reads_wontfix_and_defer() {
        let w = parse_support_row("| 3.3 | `-@` | wontfix | n/a — see §5 |").expect("row");
        assert_eq!(w.classification, Classification::Wontfix);
        let d = parse_support_row("| 3.4 | `-e` | defer:2 | n/a |").expect("row");
        assert_eq!(d.classification, Classification::Defer("2".to_owned()));
    }

    // --- mentions_row / interior cells (regression) ------------------

    #[test]
    fn mentions_row_requires_a_digit_boundary() {
        // A §6 paragraph documenting only 3.13 must not satisfy row 3.1.
        let body = "- **3.13 — Unicode escapes.** Deferred to milestone 5.\n";
        assert!(mentions_row(body, "3.13"));
        assert!(!mentions_row(body, "3.1"));
    }

    #[test]
    fn mentions_row_accepts_a_row_cited_mid_sentence() {
        let body = "- **3.8 / 3.9 — symlink resolution.** Deferred.\n";
        assert!(mentions_row(body, "3.8"));
        assert!(mentions_row(body, "3.9"));
    }

    #[test]
    fn defer_row_without_its_own_workaround_is_rejected() {
        // Two defer rows, but §6 documents only 3.2.
        let body = concat!(
            "## 1. Synopsis\n\nx\n\n## 2. Description\n\nx\n\n",
            "## 3. Support matrix\n\n",
            "| #   | Behaviour | Classification | Corpus |\n",
            "| --- | --------- | -------------- | ------ |\n",
            "| 3.1 | a         | defer:3        | n/a    |\n",
            "| 3.2 | b         | defer:3        | n/a    |\n\n",
            "## 4. Bash quirks\n\nx\n\n## 5. Wontfix rationale\n\nx\n\n",
            "## 6. Deferred rows\n\n- **3.2 — b.** Use y for now.\n\n",
            "## 8. References\n\nx\n",
        );
        let sheet = parse_sheet("x", body);
        let errors = validate_sheet_structure(&sheet);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                SheetError::MissingWorkaround { row } if row == "3.1"
            )),
            "expected a MissingWorkaround for 3.1, got {errors:?}"
        );
    }

    #[test]
    fn blank_corpus_cell_is_parsed_and_reported() {
        // A blank interior Corpus cell must survive splitting so the
        // empty-cell diagnostic is reachable, rather than the row being
        // silently skipped as a 3-column line.
        let row = parse_support_row("| 3.1 | cd dir | support |  |")
            .expect("row with a blank Corpus cell should still parse");
        assert_eq!(row.number, "3.1");
        assert!(row.corpus.is_empty());
    }

    // --- parse_sheet ------------------------------------------------

    #[test]
    fn parse_sheet_collects_sections_and_rows() {
        let sheet = parse_sheet("cd", &valid_sheet_body("builtins/cd_dir.case.toml"));
        assert_eq!(sheet.id, "cd");
        assert_eq!(sheet.rows.len(), 1);
        assert_eq!(sheet.rows[0].number, "3.1");
        assert!(sheet.sections.contains(&"## 3. Support matrix".to_owned()));
        assert!(!sheet.deferred_body.is_empty());
    }

    #[test]
    fn parse_sheet_detects_empty_deferred_section() {
        let body = "## 1. Synopsis\n\nx\n\n## 6. Deferred rows\n\n## 8. References\n";
        let sheet = parse_sheet("x", body);
        assert!(sheet.deferred_body.is_empty());
    }

    #[test]
    fn parse_sheet_ignores_sub_headers_in_section_list() {
        let body = "## 3. Support matrix\n\n### 3.A — modes\n\n## 4. Bash quirks\n";
        let sheet = parse_sheet("x", body);
        assert!(sheet.sections.contains(&"## 3. Support matrix".to_owned()));
        assert!(sheet.sections.contains(&"## 4. Bash quirks".to_owned()));
        assert!(!sheet.sections.iter().any(|s| s.starts_with("### ")));
    }

    // --- check_section_order ---------------------------------------

    #[test]
    fn section_order_accepts_canonical_set() {
        let sheet = parse_sheet("cd", &valid_sheet_body("builtins/cd_dir.case.toml"));
        assert!(check_section_order(&sheet.sections).is_ok());
    }

    #[test]
    fn section_order_accepts_optional_posix_between_6_and_8() {
        let mut sections: Vec<String> =
            MANDATORY_SECTIONS.iter().map(|s| (*s).to_owned()).collect();
        // Insert §7 before §8 (the last element).
        sections.insert(sections.len() - 1, OPTIONAL_POSIX_SECTION.to_owned());
        assert!(check_section_order(&sections).is_ok());
    }

    #[test]
    fn section_order_rejects_missing_section() {
        let mut sections: Vec<String> =
            MANDATORY_SECTIONS.iter().map(|s| (*s).to_owned()).collect();
        sections.remove(3); // drop §4.
        assert!(check_section_order(&sections).is_err());
    }

    #[test]
    fn section_order_rejects_out_of_order() {
        let mut sections: Vec<String> =
            MANDATORY_SECTIONS.iter().map(|s| (*s).to_owned()).collect();
        sections.swap(1, 2); // §2 and §3 swapped.
        assert!(check_section_order(&sections).is_err());
    }

    #[test]
    fn section_order_rejects_posix_in_wrong_place() {
        // §7 before §6 is not allowed.
        let sections = vec![
            "## 1. Synopsis".to_owned(),
            "## 2. Description".to_owned(),
            "## 3. Support matrix".to_owned(),
            "## 4. Bash quirks".to_owned(),
            "## 5. Wontfix rationale".to_owned(),
            OPTIONAL_POSIX_SECTION.to_owned(),
            "## 6. Deferred rows".to_owned(),
            "## 8. References".to_owned(),
        ];
        assert!(check_section_order(&sections).is_err());
    }

    // --- validate_sheet_structure ----------------------------------

    #[test]
    fn validate_structure_passes_for_valid_sheet() {
        let sheet = parse_sheet("cd", &valid_sheet_body("builtins/cd_dir.case.toml"));
        assert!(validate_sheet_structure(&sheet).is_empty());
    }

    #[test]
    fn validate_structure_flags_unclassified_row() {
        let body = valid_sheet_body("builtins/cd_dir.case.toml")
            .replace("| support  ", "| ???      ")
            .replace("support        |", "???            |");
        let sheet = parse_sheet("cd", &body);
        let errors = validate_sheet_structure(&sheet);
        assert!(errors
            .iter()
            .any(|e| matches!(e, SheetError::UnclassifiedRow { .. })));
    }

    #[test]
    fn validate_structure_flags_defer_without_workaround() {
        // A defer row but an empty §6 body.
        let body = "## 1. Synopsis\n\nx\n\n\
             ## 2. Description\n\nx\n\n\
             ## 3. Support matrix\n\n\
             | 3.1 | `-e` | defer:2 | n/a |\n\n\
             ## 4. Bash quirks\n\nx\n\n\
             ## 5. Wontfix rationale\n\nx\n\n\
             ## 6. Deferred rows\n\n\
             ## 8. References\n\nx\n";
        let sheet = parse_sheet("cd", body);
        let errors = validate_sheet_structure(&sheet);
        assert!(errors
            .iter()
            .any(|e| matches!(e, SheetError::MissingWorkaround { .. })));
    }

    #[test]
    fn validate_structure_defer_with_workaround_ok() {
        let body = "## 1. Synopsis\n\nx\n\n\
             ## 2. Description\n\nx\n\n\
             ## 3. Support matrix\n\n\
             | 3.1 | `-e` | defer:2 | n/a |\n\n\
             ## 4. Bash quirks\n\nx\n\n\
             ## 5. Wontfix rationale\n\nx\n\n\
             ## 6. Deferred rows\n\n\
             - **3.1 — `-e`.** Use `cd && ls` for now.\n\n\
             ## 8. References\n\nx\n";
        let sheet = parse_sheet("cd", body);
        assert!(validate_sheet_structure(&sheet).is_empty());
    }

    // --- check_support_row_resolves --------------------------------

    #[test]
    fn support_row_resolves_against_pass_case() {
        let tmp = TempDir::new().unwrap();
        write(&tmp.path().join("builtins/cd_dir.case.toml"), pass_case());

        let row = SupportRow {
            number: "3.1".to_owned(),
            classification: Classification::Support,
            corpus: "builtins/cd_dir.case.toml".to_owned(),
        };
        let mut refs = Vec::new();
        let v = check_support_row_resolves(tmp.path(), "cd", &row, &mut refs);

        assert_eq!(v, 0);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].corpus_rel, "builtins/cd_dir.case.toml");
    }

    #[test]
    fn support_row_flags_missing_case() {
        let tmp = TempDir::new().unwrap();

        let row = SupportRow {
            number: "3.1".to_owned(),
            classification: Classification::Support,
            corpus: "builtins/nope.case.toml".to_owned(),
        };
        let mut refs = Vec::new();
        let v = check_support_row_resolves(tmp.path(), "cd", &row, &mut refs);

        assert_eq!(v, 1);
    }

    #[test]
    fn support_row_flags_empty_corpus_cell() {
        let tmp = TempDir::new().unwrap();
        let row = SupportRow {
            number: "3.1".to_owned(),
            classification: Classification::Support,
            corpus: String::new(),
        };
        let mut refs = Vec::new();
        assert_eq!(
            check_support_row_resolves(tmp.path(), "cd", &row, &mut refs),
            1
        );
        assert!(refs.is_empty());
    }

    #[test]
    fn support_row_flags_wontfix_status_case() {
        let tmp = TempDir::new().unwrap();
        write(
            &tmp.path().join("builtins/cd_dir.case.toml"),
            "description = \"x\"\nstatus = \"wontfix\"\nscript = \"true\\n\"\n",
        );

        let row = SupportRow {
            number: "3.1".to_owned(),
            classification: Classification::Support,
            corpus: "builtins/cd_dir.case.toml".to_owned(),
        };
        let mut refs = Vec::new();
        let v = check_support_row_resolves(tmp.path(), "cd", &row, &mut refs);

        assert_eq!(v, 1);
    }

    // --- check_every_case_owned ------------------------------------

    #[test]
    fn every_case_owned_passes_with_exact_one_ref() {
        let cases = vec![PathBuf::from("tests/spec/builtins/cd_dir.case.toml")];
        let refs = vec![CorpusRef {
            sheet_id: "cd".to_owned(),
            row: "3.1".to_owned(),
            corpus_rel: "builtins/cd_dir.case.toml".to_owned(),
        }];
        assert_eq!(check_every_case_owned(&cases, &refs), 0);
    }

    #[test]
    fn every_case_owned_flags_orphan() {
        let cases = vec![PathBuf::from("tests/spec/builtins/cd_dir.case.toml")];
        let refs: Vec<CorpusRef> = Vec::new();
        assert_eq!(check_every_case_owned(&cases, &refs), 1);
    }

    #[test]
    fn every_case_owned_flags_duplicate_reference() {
        let cases = vec![PathBuf::from("tests/spec/builtins/cd_dir.case.toml")];
        let refs = vec![
            CorpusRef {
                sheet_id: "cd".to_owned(),
                row: "3.1".to_owned(),
                corpus_rel: "builtins/cd_dir.case.toml".to_owned(),
            },
            CorpusRef {
                sheet_id: "pwd".to_owned(),
                row: "3.2".to_owned(),
                corpus_rel: "builtins/cd_dir.case.toml".to_owned(),
            },
        ];
        assert_eq!(check_every_case_owned(&cases, &refs), 1);
    }

    // --- enumeration helpers ---------------------------------------

    #[test]
    fn enumerate_sheets_skips_template_and_readme() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(&root.join("_TEMPLATE.md"), "x");
        write(&root.join("README.md"), "x");
        write(&root.join("builtins/cd.md"), "x");
        write(&root.join("features/for_loop.md"), "x");
        write(&root.join("builtins/notes.txt"), "x");

        let sheets = enumerate_sheets(root).unwrap();
        let names: Vec<String> = sheets
            .iter()
            .map(|p| {
                p.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert!(names.contains(&"builtins/cd.md".to_owned()));
        assert!(names.contains(&"features/for_loop.md".to_owned()));
        assert!(!names.iter().any(|n| n.contains("_TEMPLATE")));
        assert!(!names.iter().any(|n| n.contains("README")));
        assert!(!names.iter().any(|n| n.contains("notes.txt")));
    }

    #[test]
    fn enumerate_corpus_cases_skips_fs_dirs() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(&root.join("cat/foo.case.toml"), pass_case());
        fs::create_dir_all(root.join("cat/foo.fs/sub")).unwrap();
        write(&root.join("cat/foo.fs/sub/inner.case.toml"), "ignored");

        let cases = enumerate_corpus_cases(root).unwrap();
        let names: Vec<String> = cases
            .iter()
            .map(|p| {
                p.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert!(names.contains(&"cat/foo.case.toml".to_owned()));
        assert!(!names.iter().any(|n| n.contains("inner.case.toml")));
    }

    #[test]
    fn sheet_id_strips_md_suffix() {
        assert_eq!(sheet_id(Path::new("Documents/specs/builtins/cd.md")), "cd");
    }

    #[test]
    fn corpus_rel_path_is_relative_to_corpus_root() {
        let p = Path::new("tests/spec/builtins/cd_dir.case.toml");
        assert_eq!(corpus_rel_path(p), "builtins/cd_dir.case.toml");
    }
}
