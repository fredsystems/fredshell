// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Minimal line-oriented parser for a spec sheet's §3 support matrix.
//!
//! This mirrors the row-recognition logic in
//! `xtask::check_specs` (`PLAN_07` §8.4): a real Markdown parser would
//! pull a heavy dependency into a proc-macro crate for no benefit. A
//! support row is a pipe-delimited table line whose first cell is a
//! dotted row number and whose third cell is a recognised
//! classification, which naturally excludes the header and separator
//! rows and any illustrative table elsewhere in the sheet.

/// The classification recorded in a §3 support-matrix row, reduced to
/// the cases `refuse!` cares about. `support` and the `???`
/// placeholder are represented by `Other` because `refuse!` never
/// targets them — a `refuse!` against a `support` row is a
/// classification mismatch, reported with the raw label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    /// `wontfix`.
    Wontfix,
    /// `defer:N`; payload is the milestone token `N`.
    Defer(String),
    /// Any other classification (`support`, `???`, unrecognised).
    Other(String),
}

impl Classification {
    /// Parse a Classification cell.
    fn parse(cell: &str) -> Self {
        let cell = cell.trim();
        match cell {
            "wontfix" => Self::Wontfix,
            _ => cell.strip_prefix("defer:").map_or_else(
                || Self::Other(cell.to_owned()),
                |n| Self::Defer(n.trim().to_owned()),
            ),
        }
    }

    /// The human-readable label for diagnostics.
    pub fn label(&self) -> String {
        match self {
            Self::Wontfix => "wontfix".to_owned(),
            Self::Defer(n) => format!("defer:{n}"),
            Self::Other(s) => s.clone(),
        }
    }
}

/// One parsed §3 support-matrix row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The row number from the first column (e.g. `3.1`).
    pub number: String,
    /// The Behaviour cell, stripped of surrounding backticks, used as
    /// the refusal summary.
    pub summary: String,
    /// The row's classification.
    pub classification: Classification,
}

/// Parse every recognisable §3 support-matrix row from a sheet body.
pub fn parse_support_matrix(body: &str) -> Vec<Row> {
    body.lines().filter_map(parse_row).collect()
}

/// Parse a single `| 3.1 | `cd dir` | support | … |` table row.
/// Returns `None` for any line that is not a support-matrix data row.
fn parse_row(line: &str) -> Option<Row> {
    let line = line.trim();
    if !line.starts_with('|') {
        return None;
    }
    let cells: Vec<&str> = line
        .split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    // Columns: # | Behaviour | Classification | Corpus.
    if cells.len() < 3 {
        return None;
    }
    let number = cells[0];
    if !is_row_number(number) {
        return None;
    }
    Some(Row {
        number: number.to_owned(),
        summary: strip_code_span(cells[1]),
        classification: Classification::parse(cells[2]),
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
/// present, and trim.
fn strip_code_span(cell: &str) -> String {
    let cell = cell.trim();
    cell.strip_prefix('`')
        .and_then(|s| s.strip_suffix('`'))
        .unwrap_or(cell)
        .trim()
        .to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const SHEET: &str = "\
## 3. Support matrix

| #   | Behaviour      | Classification | Corpus            |
| --- | -------------- | -------------- | ----------------- |
| 3.1 | `cd dir`       | support        | `b/cd_dir.toml`   |
| 3.7 | `-@`           | wontfix        | n/a — see §5       |
| 3.9 | `-e`           | defer:3        | n/a                |
";

    #[test]
    fn parses_all_data_rows_skipping_header_and_separator() {
        let rows = parse_support_matrix(SHEET);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].number, "3.1");
        assert_eq!(rows[0].summary, "cd dir");
        assert_eq!(
            rows[0].classification,
            Classification::Other("support".to_owned())
        );
    }

    #[test]
    fn parses_wontfix_row() {
        let rows = parse_support_matrix(SHEET);
        let row = rows.iter().find(|r| r.number == "3.7").unwrap();
        assert_eq!(row.classification, Classification::Wontfix);
        assert_eq!(row.summary, "-@");
    }

    #[test]
    fn parses_defer_row_with_milestone() {
        let rows = parse_support_matrix(SHEET);
        let row = rows.iter().find(|r| r.number == "3.9").unwrap();
        assert_eq!(row.classification, Classification::Defer("3".to_owned()));
    }

    #[test]
    fn classification_label_round_trips() {
        assert_eq!(Classification::Wontfix.label(), "wontfix");
        assert_eq!(Classification::Defer("2".to_owned()).label(), "defer:2");
        assert_eq!(
            Classification::Other("support".to_owned()).label(),
            "support"
        );
    }

    #[test]
    fn is_row_number_rejects_non_rows() {
        assert!(is_row_number("3.1"));
        assert!(!is_row_number("#"));
        assert!(!is_row_number("---"));
        assert!(!is_row_number("3"));
    }
}
