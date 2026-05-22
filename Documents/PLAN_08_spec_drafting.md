# PLAN_08 — Spec-Sheet Drafting Methodology

> Last updated: 2026-05-22 — initial draft.
> Phase: B. Status: stub (methodology drafted; sheets pending).
> Consumes: PLAN_05 §3 corpus structure, PLAN_05 §11 builtin
> inventory; ADR 0003 test-first methodology; ADR 0001 builtin
> tiers. Consumed by: PLAN_06 Phase B (each PLAN_06 subtask requires
> a `support`-classed sheet before implementation); PLAN_09 (uses
> sheets as the prose oracle for differential cases); PLAN_10
> (each of the eight job-control builtins requires a sheet before
> the corresponding 10.N subtask lands).

PLAN_05 (testing) gives us the executable definition of correctness:
the corpus. ADR 0003 says the corpus is the source of truth. But the
corpus is built one case at a time, and each case is one
behaviour-shape probe. A case answers "does fredshell match bash
for this exact input?" — it does not answer "what is the full set
of inputs we will ever match?"

That second question is what spec sheets answer. A spec sheet is
the prose acceptance criteria for one builtin or one grammar
feature: its supported flag inventory, its argument grammar, its
edge cases, the bash quirks it inherits, and — most importantly —
the explicit list of behaviours we will _not_ implement, with a
classification (`wontfix` / `defer:N`) and a reason.

A spec sheet is not a design document. It does not describe code.
It describes the externally observable behaviour fredshell promises
to provide. The implementation is free to choose any internal
shape it likes; the sheet is the contract with the user.

## 1. Why sheets

Without spec sheets, every builtin implementation has the same
failure mode: the implementer reads bash's man page once, writes
the "obvious" subset, and ships. Six months later a user files a
bug for a flag the implementer never read, or for a quirk the man
page mentioned in passing. The fix is reactive, the test was an
afterthought, and the cycle repeats.

Spec sheets force the inventory step to happen _before_ code is
written. The implementer must enumerate every flag, every form,
every edge case in bash's documented surface, _and explicitly
classify each one_:

- **`support`** — fredshell will replicate bash. A corpus case
  is required before implementation; the case is the executable
  half of the contract.
- **`wontfix`** — fredshell will not implement this. The user
  invoking it will get a loud, deliberate error citing the sheet.
  See §6.
- **`defer:N`** — fredshell will eventually support this, but not
  in milestone N. `N` is a PLAN_15 milestone number. Deferred
  rows turn into post-v1 worklist entries.

Every behaviour in bash's surface has exactly one classification.
A sheet with un-classified rows is incomplete; it cannot drive a
PLAN_06 / PLAN_10 subtask.

## 2. What gets a sheet

The inventory, sourced from PLAN_05 §11 plus the bash reference
manual:

### 2.1. Tier-1 builtins (one sheet each)

Approximately 57 Tier-1 builtins from PLAN_05 §11. Owners:

- 40 sheets owned by PLAN\*06 Phase B (`:`, `.`, `[`, `alias`,
  `break`, `builtin`, `cd`, `command`, `continue`, `declare`,
  `echo`, `enable`, `eval`, `exec`, `exit`, `export`, `false`,
  `let`, `local`, `pwd`, `readonly`, `return`, `set`, `shift`,
  `shopt`, `source`, `test`, `times` (from PLAN_06), `true`,
  `typeset`, `unalias`, `unset`, plus the 8 already-implemented
  rows kept for reference).
- 8 sheets owned by PLAN_10 (`bg`, `fg`, `jobs`, `kill`, `wait`,
  `disown`, `suspend`, `trap`).
- 2 sheets owned by PLAN_07 (`fc`, `history`).
- 7 sheets in "PLAN_10 extended utilities" category: `caller`,
  `dirs`, `getopts`, `hash`, `help`, `logout`, `mapfile`, `popd`,
  `printf`, `pushd`, `read`, `readarray`, `type`, `ulimit`, `umask`.

### 2.2. Grammar features (one sheet each)

The grammar inventory, sourced from PLAN_05 §3.4 and bash's
reference manual:

- **Quoting:** single-quote, double-quote, ANSI-C (`$'...'`),
  locale-translated (`$"..."`), backslash escape, here-doc
  quoting, here-string.
- **Expansions:** parameter expansion (with all its forms —
  `${var}`, `${var:-default}`, `${var:?msg}`, `${var:+alt}`,
  `${#var}`, `${var:offset:len}`, `${var#pattern}`,
  `${var##pattern}`, `${var%pattern}`, `${var%%pattern}`,
  `${var/from/to}`, `${var//from/to}`, `${var^pat}`, `${var,pat}`,
  `${!prefix*}`, `${!name[@]}`, etc.), command substitution
  (both forms), arithmetic expansion (`$((...))`), brace
  expansion (sequence and list), tilde expansion, pathname
  expansion (globbing), process substitution
  (`<(...)`/`>(...)`), word splitting.
- **Redirection:** `>`, `>>`, `<`, `<<`, `<<-`, `<<<`,
  `>|`, `&>`, `&>>`, `>&n`, `<&n`, `n>&m`, `n<&m`, `>&-`,
  `<&-`, `n>&-`, `n<&-`, `[n]<>`.
- **Control flow:** `if/elif/else/fi`, `while/do/done`,
  `until/do/done`, `for/in/do/done`, C-style `for ((;;))`,
  `select`, `case/esac`, `break`, `continue`.
- **Compound commands:** `{ ...; }`, `( ... )`, `[[ ... ]]`,
  `(( ... ))`, function definition (`name() { ... }` and
  `function name { ... }`).
- **Pipelines and lists:** `|`, `|&`, `&&`, `||`, `;`, `&`,
  `!` (pipeline negation).

That is approximately 22 feature sheets.

### 2.3. Total

Tier-1 builtins: ~57 sheets. Features: ~22 sheets. **Total: ~79
sheets.** This number is the basis for the batch-of-10 review
cadence in §7.

### 2.4. What does _not_ get a sheet

- **Tier-2 builtins.** Per ADR 0001 they are "userspace
  utilities" whose contract is determined by usage, not by
  matching bash. They get individual planning when they are
  proposed.
- **`coproc`.** Deferred from v1 entirely (PLAN_10 §12 Q10.3).
  It will get a sheet when its owning plan exists.
- **POSIX-`--posix` mode.** Not a v1 target; sheets describe
  default-bash semantics only. POSIX-only behaviour is noted in
  a "POSIX divergence" subsection per sheet but is not the
  contract.
- **Loadable builtins (`enable -f`).** Out of scope.

## 3. Sheet file layout

Sheets live under `Documents/specs/`:

```text
Documents/specs/
├── README.md                # this layout, indexed
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

Filenames are lowercase, underscored, single-token per concept.
A builtin's filename is exactly its invocation name. A feature's
filename is its bash-manual heading slug.

Sheets are Markdown. Markdown is not a clever choice — it is the
worst format that still works — but it is what the rest of the
plan documents use and it renders adequately on GitHub. Sheets
are read by humans more than by tools; readability wins.

## 4. Sheet template

Every sheet has the same top-level structure. Deviations are not
permitted; the template is enforced by `cargo xtask check-specs`
(added in subtask 08.4).

```markdown
# `<name>` — <one-line bash summary>

> Status: <draft | review | approved | superseded>
> Owner: PLAN_XX
> Tier: <1 | 2> # builtins only
> Sources: bash X.Y manual §"NAME"; POSIX.1-2024 §"NAME"
> Corpus: tests/spec/<category>/<case>.case.toml (one per support row)
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

For every `defer:N` row in §3, one paragraph plus a PLAN_15
milestone reference. The paragraph names the missing-feature
dependency (e.g., "requires Tier-2 process accounting") and
states the post-v1 reclassification target.

## 7. POSIX divergence

Subsection appearing only when fredshell follows bash and POSIX
disagrees. Records what POSIX would require, what bash does
(and we do), and which `--posix` flag toggles the difference
in bash. Not a contract — informational.

## 8. References

- Bash reference manual §"<NAME>" (URL or version).
- POSIX.1-2024 §"<NAME>" if applicable.
- Owning PLAN section.
- ADR(s) that justify any classification choice.
```

The template lives at `Documents/specs/_TEMPLATE.md`. New sheets
copy it. `xtask check-specs` verifies that every sheet has
exactly the seven mandatory sections, in order, with no rows in
§3 that lack a classification.

## 5. The three classifications, in detail

### 5.1. `support`

The behaviour is part of fredshell's contract. Required
artifacts:

- One spec corpus case (`tests/spec/<category>/<case>.case.toml`).
- An entry in §3 of the sheet referencing the case path.
- An implementation that makes the case pass.

The case is written _before_ the implementation. The case starts
life as `status = "deferred:PLAN_06"` (or 10, or 07), and flips
to `status = "pass"` in the subtask that ships the
implementation. This is the same workflow PLAN_05 §11 already
describes; PLAN_08 sheets are the prose half of that contract.

### 5.2. `wontfix`

The behaviour will not be implemented. fredshell will refuse the
invocation with a loud, deliberate error message:

```text
fredshell: cd-3.7: option '-@' (extended attributes) is not
supported and will not be implemented. See:
  Documents/specs/builtins/cd.md §3.7
```

The error message format is fixed:

```text
fredshell: <sheet-id>-<row#>: <one-sentence summary>. See:
  <sheet-path> §<section>
```

`<sheet-id>` is the sheet filename without `.md`. Refusal is
exit status 2 (POSIX usage error). The error is printed to
`stderr`. The error message is itself tested — a corpus case
under `tests/spec/refusals/` verifies the exact wording.

The point of the loud refusal is to make wontfix a deliberate
product-design choice, visible to the user, citing a public
document. It is _not_ to be friendly — friendly errors invite
"can you just add it?" requests. Loud refusal closes the
conversation.

### 5.3. `defer:N`

The behaviour will be supported, but not before milestone N (a
PLAN_15 milestone number). The user invoking it gets a
different error:

```text
fredshell: cd-3.9: option '-e' is deferred to milestone 3
(filesystem-touch builtins). Use `cd && ls` for now.
See:
  Documents/specs/builtins/cd.md §3.9
```

Format:

```text
fredshell: <sheet-id>-<row#>: <one-sentence summary>, deferred
to milestone <N> (<milestone-name>). <workaround>. See:
  <sheet-path> §<section>
```

The workaround is mandatory and is the most useful field for
the user. A `defer` row without a workaround is forbidden by
`xtask check-specs`.

When milestone N lands and the row is implemented, the row's
classification flips to `support`, the workaround field is
removed, and the corpus case is added.

## 6. The drafting workflow

### 6.1. Per-sheet workflow

1. **Copy the template.** `cp Documents/specs/_TEMPLATE.md
Documents/specs/builtins/<name>.md`.
2. **Fill §1 and §2.** SYNOPSIS quoted verbatim; Description in
   one's own words.
3. **Enumerate behaviours.** Read bash's manual entry start to
   finish. Every form, every flag, every edge case is one row
   in §3 with classification `???`.
4. **Read the POSIX entry.** Add `defer` or `support` rows for
   POSIX-only behaviours bash does not document (rare but
   real, e.g., `cd -P` strict POSIX semantics).
5. **Classify each row.** This is the hard step. Defaults:
   - If the row is in PLAN_05 §11 with PLAN_06/10/07 owner and
     is in the core usage envelope (anything used by ≥1 in
     1000 scripts from a representative corpus), classify
     `support`.
   - If the row is documented but historically unused
     (e.g., `cd -e`, `echo -E` on a system where `xpg_echo`
     defaults true), classify `defer:N` with N=3 (post-v1
     polish milestone).
   - If the row is a bash extension that conflicts with
     another goal (e.g., `enable -f` dynamic loading conflicts
     with the static-binary tenet), classify `wontfix`.
6. **Write §4 (quirks).** One paragraph per quirk; reference
   the §3 row numbers.
7. **Write §5 and §6.** Rationale and milestone references for
   wontfix / defer rows.
8. **Add corpus cases.** Every `support` row gets a
   `tests/spec/<category>/<case>.case.toml` with
   `status = "deferred:PLAN_XX"`.
9. **Submit for review.** Sheets go through review in batches of
   10 — see §7.

### 6.2. Per-feature workflow

Identical to builtin workflow, with two changes:

- §1 SYNOPSIS becomes "Forms" — a code block listing every
  bash syntactic form.
- §3 rows often have a "Tested via builtin X" cross-reference
  (e.g., parameter-expansion rows reference `echo` cases or
  `printf` cases).

Feature sheets are typically twice as long as builtin sheets
because the surface area is broader.

## 7. Batch-of-10 review cadence

Sheets are reviewed in batches of 10, not one-by-one. Rationale:

- A single sheet, reviewed in isolation, is hard to compare
  against its siblings; cross-cutting classifications drift.
- Ten sheets is the largest batch a single reviewer can hold in
  context.
- Ten sheets is also the granularity at which "are our wontfix
  decisions consistent?" becomes answerable.

The first batch (sheets 1–10) is the slowest because it sets
the bar for everything that follows. Recommended order for
batch 1:

1. `cd` (the simplest sheet that exercises every section).
2. `echo`, `printf`, `true`, `false`, `:` (the trivial-builtin
   shape).
3. `set`, `shopt`, `unset` (state-mutating; cross-cutting
   classifications).
4. `trap` (the most complex Tier-1 builtin; sets the bar for
   per-flag detail).

Subsequent batches are organised by owning PLAN doc so that
related behaviours are reviewed together (batch 2 = PLAN_10
job-control builtins; batch 3 = grammar features for
expansions; etc.).

A batch is reviewed by reading all 10 sheets back-to-back and
filing comments at the batch level. Comments fall into three
classes: row-classification disputes ("`echo`-3.4 should be
defer, not wontfix"), inventory gaps ("`set` is missing
`-o privileged`"), and template violations. The batch's owning
PLAN doc is updated to record landing.

## 8. Spec-runner integration

PLAN_05's spec runner already understands case status. PLAN_08
extends it with three integrations:

### 8.1. Cross-reference checker

`cargo xtask check-specs` walks every sheet and verifies:

- Every `support` row in §3 has a corpus case at the listed
  path, and that case has `status = "pass"` or
  `status = "deferred:PLAN_XX"`.
- Every corpus case under `tests/spec/` is referenced by exactly
  one sheet row.
- No row has classification `???`.
- The template's seven sections are present, in order.
- All `defer:N` rows have a workaround paragraph.

This runs in CI; broken cross-references fail the build.

### 8.2. Wontfix / defer error generator

Builtin implementations dispatch to a shared `refuse!` macro
that takes the sheet ID and row number and emits the §5.2 / §5.3
error message format. Centralising the format means changes to
the error template (e.g., adding a colour) flow through every
builtin automatically.

```rust
// Inside cd's flag parser:
if flag == "@" {
    return refuse!(wontfix, "cd", "3.7");
}
```

`refuse!` reads the sheet at compile time (via `include_str!`)
and extracts the row text. If the row does not exist, compile
fails. If the row is not classified `wontfix`, compile fails.
This is the link between prose and code.

### 8.3. Sheet-driven help text

`help <builtin>` (the bash builtin) reads its content from the
corresponding spec sheet's §2 (Description). This means the
sheet _is_ the user-facing documentation — no separate help
text to drift.

## 9. Versioning

Sheets do not have version numbers. They have:

- A `Status` line: `draft` (in progress), `review` (batch open),
  `approved` (batch closed), `superseded` (replaced by a newer
  sheet).
- A `Sources` line citing the bash version and POSIX revision
  used to draft the sheet. When a new bash major version ships,
  every sheet is reviewed for new behaviour; new rows are added
  with `defer:N+1` until they are intentionally supported.

There is no global "spec version" because the spec is the
corpus, not the sheets. Sheets are commentary on the corpus.

## 10. Subtasks

| Subtask | Surface                                                               | Owner   | Gate                         |
| ------- | --------------------------------------------------------------------- | ------- | ---------------------------- |
| 08.1    | Author `Documents/specs/_TEMPLATE.md` and `Documents/specs/README.md` | PLAN_08 | none                         |
| 08.2    | Draft and review batch 1 (10 sheets: `cd`, trivial builtins, state)   | PLAN_08 | 08.1                         |
| 08.3    | Draft and review batch 2 (PLAN_10 job-control builtins)               | PLAN_08 | 08.2, PLAN_10 reviewed       |
| 08.4    | `cargo xtask check-specs` cross-reference checker                     | PLAN_08 | 08.1                         |
| 08.5    | `refuse!` macro and unit tests                                        | PLAN_08 | 08.1                         |
| 08.6    | Draft and review batches 3–8 (~60 sheets, owner-grouped)              | PLAN_08 | 08.2                         |
| 08.7    | Sheet-driven `help` builtin                                           | PLAN_06 | 08.1, PLAN_06 Phase B `help` |
| 08.8    | First wontfix refusal corpus cases (`tests/spec/refusals/`)           | PLAN_08 | 08.5                         |

Subtasks 08.2 and 08.3 unblock PLAN_10's implementation;
subtask 08.6 unblocks PLAN_06 Phase B's implementation.

## 11. Open questions

- **Q08.1** — Should feature sheets and builtin sheets share a
  template, or is the feature template slightly different
  (e.g., no Tier line)? Default: same template, leave the Tier
  line blank for features. Alternative: two templates. The
  consistency win probably beats the small empty-field cost.
- **Q08.2** — Are `defer:N` workarounds binding? If we promise
  "use `cd && ls`" and that breaks for someone, do we owe them
  a fix? Default: no, the workaround is best-effort guidance,
  not contract.
- **Q08.3** — How do we handle bash's many `-o` longopts for
  `set` and `shopt`? Each is conceptually a row. That makes
  the `set` sheet ~80 rows long. Default: one row per `-o`
  option, accept the sheet length; the table is the contract.

## 12. Relationship to other plans

- **PLAN_05** — corpus and harness; PLAN_08 sheets reference
  PLAN_05 cases by path; PLAN_05 §11 is the inventory PLAN_08
  exhausts. PLAN_08 does not change the harness.
- **PLAN_06 Phase B** — every PLAN_06 Phase B subtask is gated
  on a `support`-classed sheet existing. PLAN_06 §13 already
  cites PLAN_08.
- **PLAN_09** — uses sheets as the prose oracle when deciding
  what to fuzz. The fuzzer's expectation file format
  references sheet row numbers.
- **PLAN_10** — eight job-control builtin sheets are batch 2.
  Each PLAN_10 subtask is gated on its sheet being approved.
- **PLAN_15** — milestone-N labels in `defer:N` rows point at
  PLAN_15 milestone definitions.

## 13. References

- `Documents/PLAN_05_testing.md` — corpus structure, status
  taxonomy, builtin inventory.
- `Documents/PLAN_06_exec.md` — Phase B subtasks gated on
  sheets.
- `Documents/PLAN_09_fuzzer.md` (pending) — differential oracle.
- `Documents/PLAN_10_traps_and_jobs.md` — job-control builtin
  sheets.
- `Documents/PLAN_15_milestones.md` (pending) — milestone
  numbering used by `defer:N`.
- `Documents/decisions/0001-in-process-execution-and-builtin-tiers.md`
  — Tier-1 / Tier-2 definitions referenced in §2.
- `Documents/decisions/0003-test-first-compatibility-methodology.md`
  — establishes the corpus as ground truth, which sheets
  annotate.
- Bash reference manual, current version pinned in each sheet's
  `Sources` line.
- POSIX.1-2024 ("Issue 8") shell command-language section.
