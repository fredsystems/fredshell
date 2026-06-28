<!--
This is the canonical spec-sheet template. Do not edit it to draft a
sheet; copy it first:

  cp Documents/specs/_TEMPLATE.md Documents/specs/builtins/<name>.md
  cp Documents/specs/_TEMPLATE.md Documents/specs/features/<name>.md

The template and its required structure are defined by PLAN_07 §4
(Documents/PLAN_07_spec_drafting.md). `cargo xtask check-specs`
(PLAN_07 subtask 08.4) verifies that every sheet carries the
mandatory sections, in order, with no unclassified rows. The
placeholder text below (angle-bracketed tokens, `???` rows) is
illustrative and must be replaced when a sheet is drafted.

The seven mandatory sections are §1 Synopsis, §2 Description,
§3 Support matrix, §4 Bash quirks, §5 Wontfix rationale,
§6 Deferred rows, and §8 References. §7 POSIX divergence is
conditional: it appears only when fredshell follows bash and POSIX
disagrees (PLAN_07 §4, §7).
-->

# `<name>` — `<one-line bash summary>`

> Status: <draft | review | approved | superseded>
> Owner: PLAN_XX
> Tier: <1 | 2 | feature> # `feature` only on feature sheets
> Sources: bash X.Y manual §"NAME"; POSIX.1-2024 §"NAME"
> Corpus: `tests/spec/<category>/<case>.case.toml` (one per support row)
> Last updated: YYYY-MM-DD

## 1. Synopsis

The bash manual's SYNOPSIS line, verbatim. Quoted, not rewritten.

## 2. Description

Two to four paragraphs describing what the thing _is_, in
fredshell's own words. This is the only narrative section. The
rest of the sheet is tables.

## 3. Support matrix

The behaviour inventory. Every row has a Behaviour, a
Classification, and (for support rows) a corpus reference. The
table is the contract.

| #   | Behaviour              | Classification | Corpus                        |
| --- | ---------------------- | -------------- | ----------------------------- |
| 3.1 | `<form>` with `<flag>` | support        | `<category>/<case>.case.toml` |
| 3.2 | `<edge case>`          | support        | `<category>/<case>.case.toml` |
| 3.3 | `<obscure form>`       | wontfix        | n/a — see §5                  |
| 3.4 | `<grammar extension>`  | defer:2        | n/a                           |

Every row's Behaviour cell is one sentence in present tense,
referencing exact bash syntax in backticks. Vague rows
("supports all forms") are forbidden; each form is one row.

**Sub-headers for long sheets.** Sheets with more than roughly
30 rows (notably `set` and `shopt`, both ~50–80 rows) may
section the support matrix with `### 3.A — <category>`-style
sub-headers grouping related rows for readability. Sub-headers
are advisory only; they do not affect row numbering (rows
remain `3.1`, `3.2`, …) and `xtask check-specs` ignores them
when validating the matrix. Sheets with 30 rows or fewer should
omit sub-headers.

**Multi-row case references.** The default is one corpus case
per `support` row. A single corpus case _may_ be referenced by
multiple support rows when those rows describe behaviours
whose contract is meaningfully verified together (e.g., the
three `set` error-handling options exercised in combination).
The linter validates the forward direction (every `support`
row points to a case file that exists); the reverse direction
(multiple rows pointing to the same case) is permitted.
Drafters reusing a case must verify its assertions cover each
referenced row's specific behaviour.

## 4. Bash quirks

Numbered list of behaviours bash does that POSIX does not require.
Each quirk gets a row in §3 (because we still classify it), but
this section explains _why_ bash does it and what real-world
scripts depend on. This is the high-value section: it is the
rubber-stamp every future reader will skip to.

## 5. Wontfix rationale

For every `wontfix` row in §3, one paragraph explaining why. The
paragraph must answer: what does the user lose, and what is the
suggested alternative? Wontfix errors are emitted with the row's
number in the error message (e.g., "wontfix: cd-3.7"); users
file bugs by quoting the row number.

## 6. Deferred rows

For every `defer:N` row in §3, one paragraph plus a PLAN_16
milestone reference. The paragraph names the missing-feature
dependency (e.g., "requires Tier-2 process accounting") and
states the post-v1 reclassification target.

## 7. POSIX divergence

Subsection appearing only when fredshell follows bash and POSIX
disagrees. Records what POSIX would require, what bash does
(and we do), and which `--posix` flag toggles the difference
in bash. Not a contract — informational.

## 8. References

- Bash reference manual §`"<NAME>"` (URL or version).
- POSIX.1-2024 §`"<NAME>"` if applicable.
- Owning PLAN section.
- ADR(s) that justify any classification choice.
