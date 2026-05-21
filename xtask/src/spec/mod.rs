// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! `cargo xtask spec` subcommands.
//!
//! Exposes:
//!
//! * `versions` — verifies that the pinned reference toolchain
//!   declared in `tests/spec/REFERENCE.md` matches what the nix
//!   devshell is actually serving (via the `FREDSHELL_REFERENCE_*`
//!   environment variables) and reports drift versus the floating
//!   `nixpkgs` input as advisory output. See `PLAN_05` §4.5.
//! * `record` — record sidecar fixtures (`<case>.stdout`,
//!   `<case>.stderr`, `<case>.exit`) for a `.case.toml` by running
//!   the case under the pinned reference bash. See `PLAN_05` §4.4 /
//!   05.7.
//! * `lint` — static checks over the corpus: schema validation,
//!   orphan-fixture detection, and `PLAN_05` §11.1 builtins drift
//!   versus the pinned reference bash. See `PLAN_05` 05.8.

use std::env;
use std::fs;
use std::path::Path;

use clap::Subcommand;
use color_eyre::eyre::{bail, Result};

mod lint;
mod record;

pub use lint::LintArgs;
pub use record::RecordArgs;

/// Subcommands under `cargo xtask spec`.
#[derive(Subcommand)]
pub enum SpecCmd {
    /// Verify the pinned reference toolchain matches
    /// `tests/spec/REFERENCE.md` and report drift versus the
    /// floating `nixpkgs` input.
    Versions,
    /// Record sidecar fixtures for a `.case.toml` by running the
    /// case under the pinned reference bash (`PLAN_05` 05.7).
    Record(RecordArgs),
    /// Lint the spec corpus: schema validation, orphan-fixture
    /// detection, and `PLAN_05` §11.1 builtins drift versus the
    /// pinned reference bash (`PLAN_05` 05.8).
    Lint(LintArgs),
}

/// Dispatch a `spec` subcommand.
pub fn run(cmd: &SpecCmd) -> Result<()> {
    match cmd {
        SpecCmd::Versions => run_versions(),
        SpecCmd::Record(args) => record::run(args),
        SpecCmd::Lint(args) => lint::run(args),
    }
}

/// Path to the reference doc, resolved relative to the workspace
/// root. `cargo xtask` is invoked from the workspace root so a
/// relative path is sufficient and stable.
pub const REFERENCE_DOC: &str = "tests/spec/REFERENCE.md";

/// Parsed pin from `tests/spec/REFERENCE.md` `[reference]` block.
///
/// Field names mirror the TOML keys verbatim so a future migration
/// to a real TOML parser is a drop-in replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencePin {
    pub bash: String,
    pub coreutils: String,
    pub nixpkgs_rev: String,
    pub nixpkgs_input: String,
    pub pinned_on: String,
}

/// Errors surfaced while parsing the `[reference]` block.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The `[reference]` table header was not found.
    MissingTable,
    /// A required key was absent from the table.
    MissingKey(&'static str),
    /// A key's value was not a double-quoted string.
    NotQuoted(&'static str),
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingTable => write!(f, "missing [reference] table"),
            Self::MissingKey(k) => write!(f, "missing key `{k}` in [reference]"),
            Self::NotQuoted(k) => write!(f, "key `{k}` is not a double-quoted string"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse the `[reference]` block out of a `REFERENCE.md` document.
///
/// The parser is deliberately minimal: it walks lines, finds the
/// `[reference]` header inside a fenced TOML code block, and reads
/// `key = "value"` lines until the next blank line or fence. A real
/// TOML parser is overkill for five keys and would pull a dependency
/// into `xtask` that no other code needs.
pub fn parse_reference(doc: &str) -> Result<ReferencePin, ParseError> {
    // Locate the `[reference]` header anywhere in the document. We
    // do not require it to be inside a fenced block — the markdown
    // fence is for human readability, not for the parser.
    let mut lines = doc.lines();
    let mut found = false;
    for line in lines.by_ref() {
        if line.trim() == "[reference]" {
            found = true;
            break;
        }
    }
    if !found {
        return Err(ParseError::MissingTable);
    }

    let mut bash: Option<String> = None;
    let mut coreutils: Option<String> = None;
    let mut nixpkgs_rev: Option<String> = None;
    let mut nixpkgs_input: Option<String> = None;
    let mut pinned_on: Option<String> = None;

    for line in lines {
        let trimmed = line.trim();
        // Stop at a blank line, a new table header, or the end of
        // the fenced block. This bounds the parser to the
        // `[reference]` table proper.
        if trimmed.is_empty() || trimmed.starts_with('[') || trimmed.starts_with("```") {
            break;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        let parsed = extract_quoted(value);
        match key {
            "bash" => bash = Some(parsed.ok_or(ParseError::NotQuoted("bash"))?.to_owned()),
            "coreutils" => {
                coreutils = Some(parsed.ok_or(ParseError::NotQuoted("coreutils"))?.to_owned());
            }
            "nixpkgs_rev" => {
                nixpkgs_rev = Some(
                    parsed
                        .ok_or(ParseError::NotQuoted("nixpkgs_rev"))?
                        .to_owned(),
                );
            }
            "nixpkgs_input" => {
                nixpkgs_input = Some(
                    parsed
                        .ok_or(ParseError::NotQuoted("nixpkgs_input"))?
                        .to_owned(),
                );
            }
            "pinned_on" => {
                pinned_on = Some(parsed.ok_or(ParseError::NotQuoted("pinned_on"))?.to_owned());
            }
            _ => {}
        }
    }

    Ok(ReferencePin {
        bash: bash.ok_or(ParseError::MissingKey("bash"))?,
        coreutils: coreutils.ok_or(ParseError::MissingKey("coreutils"))?,
        nixpkgs_rev: nixpkgs_rev.ok_or(ParseError::MissingKey("nixpkgs_rev"))?,
        nixpkgs_input: nixpkgs_input.ok_or(ParseError::MissingKey("nixpkgs_input"))?,
        pinned_on: pinned_on.ok_or(ParseError::MissingKey("pinned_on"))?,
    })
}

/// Strip the surrounding `"…"` from a value, returning `None` if the
/// value is not a complete double-quoted string. No escape handling
/// — the pin uses simple alphanumeric / dotted version strings.
fn extract_quoted(value: &str) -> Option<&str> {
    let s = value.strip_prefix('"')?.strip_suffix('"')?;
    Some(s)
}

/// `cargo xtask spec versions` body.
fn run_versions() -> Result<()> {
    let doc_path = Path::new(REFERENCE_DOC);
    let doc = match fs::read_to_string(doc_path) {
        Ok(s) => s,
        Err(e) => bail!("spec versions: failed to read {}: {e}", doc_path.display()),
    };
    let pin = match parse_reference(&doc) {
        Ok(p) => p,
        Err(e) => bail!("spec versions: failed to parse {}: {e}", doc_path.display()),
    };

    // Read the env vars exported by the nix devshell. Absence means
    // the user is not inside the devshell, which makes the verify
    // step impossible and the drift advisory meaningless.
    let ref_bash = env::var("FREDSHELL_REFERENCE_BASH_VERSION").ok();
    let ref_coreutils = env::var("FREDSHELL_REFERENCE_COREUTILS_VERSION").ok();
    let float_bash = env::var("FREDSHELL_FLOATING_BASH_VERSION").ok();
    let float_coreutils = env::var("FREDSHELL_FLOATING_COREUTILS_VERSION").ok();

    let (Some(ref_bash), Some(ref_coreutils)) = (ref_bash.as_deref(), ref_coreutils.as_deref())
    else {
        bail!(
            "spec versions: FREDSHELL_REFERENCE_BASH_VERSION / \
             FREDSHELL_REFERENCE_COREUTILS_VERSION are not set. Run \
             `nix develop` (or activate direnv) before invoking this command."
        );
    };

    println!("fredshell spec versions");
    println!("======================");
    println!();
    println!("Pinned in {}:", doc_path.display());
    println!("  bash       : {}", pin.bash);
    println!("  coreutils  : {}", pin.coreutils);
    println!("  rev        : {}", pin.nixpkgs_rev);
    println!("  pinned on  : {}", pin.pinned_on);
    println!();
    println!("Resolved from `{}` (nix devshell):", pin.nixpkgs_input);
    println!("  bash       : {ref_bash}");
    println!("  coreutils  : {ref_coreutils}");
    println!();

    let mut mismatches: Vec<String> = Vec::new();
    if pin.bash != ref_bash {
        mismatches.push(format!("bash: pin = {}, devshell = {ref_bash}", pin.bash));
    }
    if pin.coreutils != ref_coreutils {
        mismatches.push(format!(
            "coreutils: pin = {}, devshell = {ref_coreutils}",
            pin.coreutils
        ));
    }
    if !mismatches.is_empty() {
        for m in &mismatches {
            eprintln!("error: {m}");
        }
        bail!(
            "spec versions: REFERENCE.md disagrees with the nix devshell. \
             Update the [reference] block or the nixpkgs-reference rev so they match."
        );
    }
    println!("pin matches devshell: ok");

    // Drift advisory: compare against the floating nixpkgs input.
    // Absence here is non-fatal — older devshells may not export
    // these vars.
    println!();
    match (float_bash.as_deref(), float_coreutils.as_deref()) {
        (Some(fb), Some(fc)) => {
            println!("Floating nixos-unstable (advisory):");
            println!("  bash       : {fb}");
            println!("  coreutils  : {fc}");
            let bash_drift = fb != ref_bash;
            let coreutils_drift = fc != ref_coreutils;
            if bash_drift {
                println!("  advisory: nixos-unstable bash is {fb} (pinned: {ref_bash})");
            }
            if coreutils_drift {
                println!("  advisory: nixos-unstable coreutils is {fc} (pinned: {ref_coreutils})");
            }
            if !bash_drift && !coreutils_drift {
                println!("  no drift");
            }
        }
        _ => {
            println!("Floating nixos-unstable version vars not set; skipping drift advisory.");
        }
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
some prose

```toml
[reference]
bash = "5.3p9"
coreutils = "9.10"
nixpkgs_rev = "d233902339c02a9c334e7e593de68855ad26c4cb"
nixpkgs_input = "nixpkgs-reference"
pinned_on = "2026-05-21"
```

more prose
"#;

    #[test]
    fn parse_reference_extracts_all_keys() {
        let pin = parse_reference(FIXTURE).expect("parse");
        assert_eq!(pin.bash, "5.3p9");
        assert_eq!(pin.coreutils, "9.10");
        assert_eq!(pin.nixpkgs_rev, "d233902339c02a9c334e7e593de68855ad26c4cb");
        assert_eq!(pin.nixpkgs_input, "nixpkgs-reference");
        assert_eq!(pin.pinned_on, "2026-05-21");
    }

    #[test]
    fn parse_reference_rejects_missing_table() {
        let err = parse_reference("no table here").unwrap_err();
        assert_eq!(err, ParseError::MissingTable);
    }

    #[test]
    fn parse_reference_rejects_missing_key() {
        let doc = "[reference]\nbash = \"5.3p9\"\n";
        let err = parse_reference(doc).unwrap_err();
        assert_eq!(err, ParseError::MissingKey("coreutils"));
    }

    #[test]
    fn parse_reference_rejects_unquoted_value() {
        let doc = "[reference]\nbash = 5.3p9\ncoreutils = \"9.10\"\nnixpkgs_rev = \"x\"\nnixpkgs_input = \"y\"\npinned_on = \"z\"\n";
        let err = parse_reference(doc).unwrap_err();
        assert_eq!(err, ParseError::NotQuoted("bash"));
    }

    #[test]
    fn parse_reference_stops_at_next_table() {
        let doc = "[reference]\nbash = \"5.3p9\"\ncoreutils = \"9.10\"\nnixpkgs_rev = \"r\"\nnixpkgs_input = \"i\"\npinned_on = \"d\"\n[other]\nbash = \"wrong\"\n";
        let pin = parse_reference(doc).expect("parse");
        assert_eq!(pin.bash, "5.3p9");
    }

    /// Regression test for `PLAN_05` 05.3: the on-disk
    /// `tests/spec/REFERENCE.md` parses cleanly and matches the
    /// versions encoded in `flake.nix`. This is the file's primary
    /// purpose — if someone edits the doc without keeping the
    /// `[reference]` block parseable, this test catches it.
    #[test]
    fn on_disk_reference_doc_parses() {
        // Resolve relative to the workspace root: `cargo test` runs
        // each crate's tests with CWD = that crate's manifest dir.
        let manifest = env!("CARGO_MANIFEST_DIR");
        let path = Path::new(manifest).join("..").join(REFERENCE_DOC);
        let doc =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let pin = parse_reference(&doc).expect("on-disk REFERENCE.md must parse");

        // Pin the values themselves so any version bump must
        // intentionally update this test in the same commit, per
        // the upgrade policy in REFERENCE.md.
        assert_eq!(pin.bash, "5.3p9");
        assert_eq!(pin.coreutils, "9.10");
        assert_eq!(pin.nixpkgs_rev, "d233902339c02a9c334e7e593de68855ad26c4cb");
        assert_eq!(pin.nixpkgs_input, "nixpkgs-reference");
    }
}
