# `Documents/specs/` — fredshell spec sheets

This directory holds fredshell's **spec sheets**: the prose
acceptance criteria for each Tier-1 builtin and each grammar
feature. A spec sheet is the human-readable half of the
compatibility contract; the executable half is the corpus under
`tests/spec/` (owned by `Documents/PLAN_05_testing.md`).

The methodology — what a sheet is, why it exists, how it is
drafted, reviewed, and linted — is owned by
`Documents/PLAN_07_spec_drafting.md`. This README is the
operator-facing index: it states the directory layout, the
authoring entry point, and how sheets relate to the corpus. When
this README and PLAN_07 disagree, PLAN_07 is authoritative.

## What a spec sheet is

A spec sheet describes the externally observable behaviour
fredshell promises for one builtin or one grammar feature: its
supported flag inventory, its argument grammar, its edge cases,
the bash quirks it inherits, and the explicit list of behaviours
fredshell will _not_ implement, each classified and justified.

A sheet is not a design document. It does not describe code. The
implementation is free to choose any internal shape; the sheet is
the contract with the user. See PLAN_07 §1 for the rationale.

Every behaviour in bash's surface has exactly one of three
classifications (PLAN_07 §5):

- **`support`** — fredshell replicates bash. A corpus case under
  `tests/spec/` is required before implementation.
- **`wontfix`** — fredshell refuses the invocation with a loud,
  deliberate error citing the sheet row.
- **`defer:N`** — fredshell will support this, but not before
  milestone `N` (a `PLAN_19` milestone number).

A sheet with an unclassified row is incomplete and cannot drive a
PLAN_12 or PLAN_13 implementation subtask.

## Layout

```text
Documents/specs/
├── README.md                # this file
├── _TEMPLATE.md             # canonical sheet template; copy to start a sheet
├── builtins/
│   ├── cd.md
│   ├── echo.md
│   ├── exit.md
│   ├── jobs.md
│   ├── trap.md
│   └── ... (~57 files)
└── features/
    ├── parameter_expansion.md
    ├── command_substitution.md
    ├── arithmetic_expansion.md
    ├── brace_expansion.md
    ├── pathname_expansion.md
    ├── here_documents.md
    ├── if_then_else.md
    ├── for_loop.md
    └── ... (~22 files)
```

Filenames are lowercase, underscored, single-token per concept. A
builtin's filename is exactly its invocation name — except for the
three punctuation-named builtins, which take a spelled-out ASCII
name because the glyph breaks globbing and renders the
`<sheet-id>-<row#>` refusal diagnostic as nonsense: `:` is
`colon.md`, `.` is `dot.md`, and `[` is `bracket.md` (`PLAN_07`
§3). A feature's filename is its bash-manual heading slug. Sheets are Markdown and
are read by humans more than by tools; readability wins (PLAN_07
§3).

The total inventory is approximately 80 sheets: ~57 Tier-1 builtin
sheets and ~23 feature sheets (including one UTF-8/locale sheet).
See PLAN_07 §2 for the full breakdown and owner assignments.

## Authoring a sheet

Copy the template; never edit it in place:

```sh
cp Documents/specs/_TEMPLATE.md Documents/specs/builtins/<name>.md
# or
cp Documents/specs/_TEMPLATE.md Documents/specs/features/<name>.md
```

Then follow the per-sheet workflow in PLAN_07 §6: fill the
synopsis and description, enumerate every bash form/flag/edge case
as a row in the support matrix, classify each row, write the
quirks/wontfix/deferred sections, and add a corpus case for every
`support` row. Sheets are reviewed in batches of ten (PLAN_07 §7).

Every sheet must carry the seven mandatory sections, in order:
§1 Synopsis, §2 Description, §3 Support matrix, §4 Bash quirks,
§5 Wontfix rationale, §6 Deferred rows, and §8 References. §7 POSIX
divergence is conditional — include it only when fredshell follows
bash and POSIX disagrees.

Sheets with no `wontfix` or no `defer` rows still carry §5 and §6;
write `None. Every row in §3 is classified support.` rather than
dropping the heading, because the linter requires all seven.

### Probe bash; do not recall it

AGENTS.md forbids guessing shell semantics. Every row must come from
the pinned reference bash, which the devshell exports as
`FREDSHELL_REFERENCE_BASH`:

```sh
"$FREDSHELL_REFERENCE_BASH" -c 'help <builtin>'
env -i "$FREDSHELL_REFERENCE_BASH" --noprofile --norc -c '<probe>' | od -An -c
```

Probe under `env -i` when the behaviour could be
environment-sensitive, because that is what the recorder does (see
below). Pipe through `od` whenever the answer is about bytes —
control characters, trailing newlines, and locale-dependent output
are all invisible to the eye. If `FREDSHELL_REFERENCE_BASH` is unset
you are not in the devshell: run `nix develop --impure` (or
`direnv allow`) rather than substituting the system bash.

### Writing corpus cases that actually record

Cases are recorded with
`cargo run -p xtask -- spec record <case>.case.toml`. The recorder
spawns the pinned bash with `env_clear()` inside a sandbox, which
constrains what a case may do. These are the constraints that have
bitten in practice:

- **No external commands.** With a cleared environment there is no
  `PATH`, so every coreutils binary exits 127. Use shell builtins only:
  `$(<file)` instead of `cat file`, `${PWD##*/}` instead of
  `basename`, and parameter expansion instead of `sed`/`tr`.
- **The locale is effectively `C`.** No `LANG`/`LC_*` is set, so
  multibyte behaviour (`\u`/`\U` escapes, multibyte globs, collation)
  does not reproduce. Classify locale-dependent rows `defer:5` and
  leave them to the UTF-8/locale sheet (`PLAN_07` §2.2).
- **Symlinks do not materialise.** The runner's
  `copy_dir_recursive` skips them in v0, so a `.fs/` skeleton cannot
  carry one. Behaviours that only differ in the presence of a symlink
  must be `defer:3`, not `support` (see `cd` 3.8 / 3.9).
- **Empty directories need a `.keep`.** git does not track empty
  directories, so every leaf directory in a `<case>.fs/` skeleton
  needs a tracked file in it.
- **`[env]` has renamed fields.** `HOME` and `PATH` are dedicated
  keys; anything else goes under `[env.extra]`. `$SANDBOX` in a value
  is substituted with the sandbox root, which is how a case asserts
  against an absolute path without hardcoding one.
- **The `script` field processes escapes.** TOML `"""…"""` is a
  _basic_ string, so `\t` becomes a real tab. Write `\\t` when bash
  should receive the two characters `\t`.
- **Never hand-write a fixture.** Record it, then read it back with
  `od -An -c` and confirm it says what the row claims.

### Fixtures are golden data — keep formatting hooks off them

Recorded `.stdout` / `.stderr` / `.exit` files are oracle output, not
source code. A fixture containing a lone CR is currently rewritten by
the `mixed-line-ending` pre-commit hook, which corrupts it silently
and then rejects the commit; `trailing-whitespace` and
`end-of-file-fixer` are equally hazardous for output that
legitimately ends in whitespace or lacks a final newline. This is
tracked as cleanup `08.2e-CU1` in `PLAN_07` §15. Until it is fixed,
prefer asserting such behaviour by comparison — capture the output
and test it against an ANSI-C `$'…'` literal, emitting only printable
tokens — and never re-stage a fixture a hook has just modified
without re-reading its bytes.

### Markdown that survives the formatter

`prettier` runs on every commit and resolves CommonMark emphasis
before code spans. An unbackticked `PLAN_07` in prose opens an
emphasis run on its underscore, and prettier then re-pairs the
surrounding backtick delimiters — which rewrites `PLAN_07` to
`PLAN*07`, mangles nearby globs such as `echo_*.case.toml`, and eats
the spaces around later code spans. Backtick every `PLAN_NN`, path,
and glob, in prose and in table cells alike. After editing, run
`prettier --write` then re-read the changed lines to confirm they
survived.

## Sheet status

The `Status` line at the top of each sheet is one of (PLAN_07 §9):

- `draft` — in progress.
- `review` — part of an open review batch.
- `approved` — review batch closed.
- `superseded` — replaced by a newer sheet.

Sheets do not carry version numbers. The `Sources` line cites the
bash version and POSIX revision used to draft the sheet; the
authoritative oracle versions are pinned in `tests/spec/REFERENCE.md`.

## Relationship to the corpus and the linter

- `tests/spec/` (PLAN_05) holds the corpus cases that the
  `support` rows reference by path. The two trees are
  complementary: this directory is prose, that directory is
  executable.
- `cargo xtask check-specs` (PLAN_07 subtask 08.4) walks every
  sheet and verifies that each `support` row points to an existing
  corpus case, that no row is unclassified, that the mandatory
  sections are present and ordered, and that every `defer:N` row
  carries a workaround paragraph.
- The `refuse!` macro (PLAN_07 subtask 08.5) reads sheet rows at
  compile time so that `wontfix` and `defer:N` refusals cite the
  sheet by row number and cannot drift from the prose.

## References

- `Documents/PLAN_07_spec_drafting.md` — methodology, template,
  classifications, review cadence, linter integration (authority).
- `Documents/PLAN_05_testing.md` — corpus structure and status
  taxonomy.
- `tests/spec/README.md` — corpus harness and case schema.
- `tests/spec/REFERENCE.md` — pinned bash/coreutils oracle versions.
- `Documents/decisions/0001-in-process-execution-and-builtin-tiers.md`
  — Tier-1 / Tier-2 definitions.
- `Documents/decisions/0003-test-first-compatibility-methodology.md`
  — the corpus as ground truth, which sheets annotate.
