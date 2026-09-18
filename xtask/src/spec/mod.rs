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

/// Path to the flake, resolved relative to the workspace root. This
/// is the authoritative home of the pin: `REFERENCE.md` documents
/// the rev, but `flake.nix` is what nix actually evaluates.
pub const FLAKE_NIX: &str = "flake.nix";

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

/// Errors surfaced while reading an input's pinned rev out of
/// `flake.nix`.
#[derive(Debug, PartialEq, Eq)]
pub enum FlakeParseError {
    /// No `<input>.url = "…";` binding was found for the input.
    InputNotFound(String),
    /// The input's URL carries no 40-character object name, so the
    /// input is not pinned to a revision at all (e.g. it tracks a
    /// branch such as `nixos-unstable`). For the reference input
    /// that is itself the bug: an unpinned oracle drifts silently.
    NotPinned {
        /// The flake input name that was inspected.
        input: String,
        /// The URL found for that input.
        url: String,
    },
}

impl core::fmt::Display for FlakeParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InputNotFound(input) => {
                write!(f, "no `{input}.url = \"…\";` binding found in flake.nix")
            }
            Self::NotPinned { input, url } => write!(
                f,
                "flake input `{input}` is not pinned to a revision: {url}"
            ),
        }
    }
}

impl std::error::Error for FlakeParseError {}

/// Extract the pinned revision for `input` from `flake.nix` source.
///
/// Recognises the dotted form this flake uses:
///
/// ```text
/// nixpkgs-reference.url = "github:nixos/nixpkgs/<rev>";
/// ```
///
/// The revision is the final `/`-separated segment of the URL and
/// must be a 40-character hex object name. A branch name or a bare
/// `github:owner/repo` yields [`FlakeParseError::NotPinned`], which
/// is a real failure for the reference input rather than a parse
/// nicety.
///
/// Deliberately minimal, in the same spirit as [`parse_reference`]:
/// a full Nix parser in `xtask` to read one string would be a poor
/// trade.
pub fn parse_flake_input_rev(flake: &str, input: &str) -> Result<String, FlakeParseError> {
    let needle = format!("{input}.url");
    for line in flake.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with(&needle) {
            continue;
        }
        let Some((_, value)) = trimmed.split_once('=') else {
            continue;
        };
        // Strip the statement terminator before the quotes.
        let value = value.trim().trim_end_matches(';').trim();
        let Some(url) = extract_quoted(value) else {
            continue;
        };
        let rev = url.rsplit('/').next().unwrap_or_default();
        if rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Ok(rev.to_owned());
        }
        return Err(FlakeParseError::NotPinned {
            input: input.to_owned(),
            url: url.to_owned(),
        });
    }
    Err(FlakeParseError::InputNotFound(input.to_owned()))
}

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

/// Read the pinned rev for `input` out of `flake.nix`.
fn read_flake_rev(input: &str) -> Result<String> {
    let flake_path = Path::new(FLAKE_NIX);
    let flake = match fs::read_to_string(flake_path) {
        Ok(s) => s,
        Err(e) => bail!(
            "spec versions: failed to read {}: {e}",
            flake_path.display()
        ),
    };
    match parse_flake_input_rev(&flake, input) {
        Ok(r) => Ok(r),
        Err(e) => bail!(
            "spec versions: failed to read the `{input}` pin from {}: {e}",
            flake_path.display()
        ),
    }
}

/// Compare the declared pin against `flake.nix` and the devshell.
///
/// Collects every mismatch before failing, so a stale pin reports
/// the rev *and* both versions in one run rather than making the
/// caller fix them one at a time.
fn verify_pin(
    pin: &ReferencePin,
    flake_rev: &str,
    ref_bash: &str,
    ref_coreutils: &str,
) -> Result<()> {
    let mut mismatches: Vec<String> = Vec::new();
    // The rev is checked first because it is the root cause when the
    // versions happen to agree: two different nixpkgs revs can ship
    // identical bash/coreutils, so a version-only check reports "ok"
    // while the documented pin is stale. That is exactly how
    // REFERENCE.md drifted from flake.nix in #55.
    if pin.nixpkgs_rev != flake_rev {
        mismatches.push(format!(
            "nixpkgs_rev: REFERENCE.md = {}, flake.nix = {flake_rev}",
            pin.nixpkgs_rev
        ));
    }
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
            "spec versions: REFERENCE.md disagrees with flake.nix or the nix \
             devshell. Per the upgrade policy in REFERENCE.md, the [reference] \
             block and the `{}` rev in flake.nix must be updated in the same \
             commit, re-recording any affected fixtures.",
            pin.nixpkgs_input
        );
    }
    Ok(())
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

    // The rev declared in REFERENCE.md is only meaningful if it
    // matches what nix actually evaluates, so read it from the flake
    // rather than trusting the doc.
    let flake_rev = read_flake_rev(&pin.nixpkgs_input)?;

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
    println!("  rev        : {flake_rev}");
    println!();

    verify_pin(&pin, &flake_rev, ref_bash, ref_coreutils)?;
    println!("pin matches flake.nix and devshell: ok");

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

    const REV: &str = "88ae3822eb8aec31f12a4a1895cb064413511177";

    #[test]
    fn parse_flake_input_rev_reads_the_dotted_url_form() {
        let flake = format!("  nixpkgs-reference.url = \"github:nixos/nixpkgs/{REV}\";\n");
        let rev = parse_flake_input_rev(&flake, "nixpkgs-reference").expect("parse");
        assert_eq!(rev, REV);
    }

    /// The input name must match exactly. `nixpkgs-reference` and
    /// `nixpkgs` share a prefix in the other direction, so a naive
    /// `contains` would let the floating input satisfy a query for
    /// the pinned one.
    #[test]
    fn parse_flake_input_rev_does_not_confuse_sibling_inputs() {
        let flake = format!(
            "nixpkgs.url = \"github:nixos/nixpkgs/nixos-unstable\";\n\
             nixpkgs-reference.url = \"github:nixos/nixpkgs/{REV}\";\n"
        );
        let rev = parse_flake_input_rev(&flake, "nixpkgs-reference").expect("parse");
        assert_eq!(rev, REV);
    }

    #[test]
    fn parse_flake_input_rev_rejects_an_unpinned_input() {
        let flake = "nixpkgs.url = \"github:nixos/nixpkgs/nixos-unstable\";\n";
        let err = parse_flake_input_rev(flake, "nixpkgs").unwrap_err();
        assert_eq!(
            err,
            FlakeParseError::NotPinned {
                input: "nixpkgs".to_owned(),
                url: "github:nixos/nixpkgs/nixos-unstable".to_owned(),
            }
        );
    }

    /// A truncated digest is not a revision. Renovate renders short
    /// digests in PR titles, so a hand-copied `88ae382` must fail
    /// rather than silently compare unequal to the full rev.
    #[test]
    fn parse_flake_input_rev_rejects_a_short_digest() {
        let flake = "nixpkgs-reference.url = \"github:nixos/nixpkgs/88ae382\";\n";
        let err = parse_flake_input_rev(flake, "nixpkgs-reference").unwrap_err();
        assert!(matches!(err, FlakeParseError::NotPinned { .. }));
    }

    #[test]
    fn parse_flake_input_rev_reports_a_missing_input() {
        let err = parse_flake_input_rev("{}\n", "nixpkgs-reference").unwrap_err();
        assert_eq!(
            err,
            FlakeParseError::InputNotFound("nixpkgs-reference".to_owned())
        );
    }

    fn pin_fixture() -> ReferencePin {
        ReferencePin {
            bash: "5.3p15".to_owned(),
            coreutils: "9.11".to_owned(),
            nixpkgs_rev: REV.to_owned(),
            nixpkgs_input: "nixpkgs-reference".to_owned(),
            pinned_on: "2026-09-18".to_owned(),
        }
    }

    #[test]
    fn verify_pin_accepts_a_consistent_pin() {
        verify_pin(&pin_fixture(), REV, "5.3p15", "9.11").expect("consistent pin");
    }

    /// The #55 scenario exactly: the rev is stale but both versions
    /// still agree, because the two revs ship the same bash and
    /// coreutils. A version-only check passes here; this must not.
    #[test]
    fn verify_pin_rejects_a_stale_rev_even_when_versions_agree() {
        let err = verify_pin(
            &pin_fixture(),
            "aec71e3ada2e0b6bebd3d84c01523eb137dff06f",
            "5.3p15",
            "9.11",
        )
        .expect_err("stale rev must fail");
        assert!(
            err.to_string().contains("disagrees with flake.nix"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn verify_pin_rejects_a_version_mismatch() {
        let err = verify_pin(&pin_fixture(), REV, "5.4p1", "9.11")
            .expect_err("bash version mismatch must fail");
        assert!(
            err.to_string().contains("disagrees with flake.nix"),
            "unexpected error: {err}"
        );
    }

    /// The guard that #55 was missing: `REFERENCE.md` and `flake.nix`
    /// must name the same rev. Both files are read from disk so this
    /// fails if either drifts, regardless of whether the bash and
    /// coreutils versions happen to agree.
    #[test]
    fn on_disk_reference_doc_rev_matches_the_flake() {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let root = Path::new(manifest).join("..");

        let doc = fs::read_to_string(root.join(REFERENCE_DOC)).expect("read REFERENCE.md");
        let pin = parse_reference(&doc).expect("parse REFERENCE.md");

        let flake = fs::read_to_string(root.join(FLAKE_NIX)).expect("read flake.nix");
        let flake_rev =
            parse_flake_input_rev(&flake, &pin.nixpkgs_input).expect("read the pin from flake.nix");

        assert_eq!(
            pin.nixpkgs_rev, flake_rev,
            "REFERENCE.md pins {} but flake.nix pins {flake_rev}; per the \
             upgrade policy both must change in the same commit",
            pin.nixpkgs_rev
        );
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
        assert_eq!(pin.bash, "5.3p15");
        assert_eq!(pin.coreutils, "9.11");
        assert_eq!(pin.nixpkgs_rev, "88ae3822eb8aec31f12a4a1895cb064413511177");
        assert_eq!(pin.nixpkgs_input, "nixpkgs-reference");
    }
}
