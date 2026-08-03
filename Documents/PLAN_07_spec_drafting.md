# PLAN_07 — Spec-Sheet Drafting Methodology

> Last updated: 2026-06-28 — handoff consolidation before the
> branch is PR'd (§14 row 08.2-docs): added §15 Cleanup registry
> (entry 08.2e-CU1, the `mixed-line-ending` hook corrupting
> fixtures that contain a CR), a Status column on §10 plus the new
> §10.1 batch-1 checklist (5 of 10 sheets done), the
> symbol-named-builtin filename table in §3 (`colon.md` / `dot.md`
> / `bracket.md`), and a provisional `defer:N` milestone table in
> §5.3. Also corrected four stale `PLAN_16` milestone references to
> `PLAN_19`. `Documents/specs/README.md` gained the corpus-authoring
> runbook (recorder constraints, fixtures-are-golden-data, the
> prettier trap). **A future agent picking this up should read §10.1
> for what is left and §15 before drafting `printf`.**
>
> Earlier on 2026-06-28 — batch-1 drafting continues (08.2): the
> fifth sheet, `echo`, landed with 14 §3 rows (12 `support` backed
> by 12 hermetic corpus cases, 2 `defer` — `\u` / `\U` Unicode
> escapes to milestone 5 because they are locale-dependent and the
> recorder is C-locale, and `xpg_echo`-default escapes to milestone
> 4 pending `shopt`). First moderate sheet of the batch: option
> bundling and
> the parse boundary, the C-escape set, `\c` output truncation, and
> octal-vs-hex escapes. See §14 row 08.2e. Remaining batch-1 sheets
> (`printf`, `set`, `shopt`, `unset`, `trap`) follow one per step.
> Then 08.3 / 08.6, sheet-driven `help` (08.7), refusal corpus
> cases (08.8), and the `pc` / `check` wiring of `check-specs`.
>
> Earlier on 2026-06-28 — batch-1 drafting continued (08.2): the
> fourth sheet, `:` (null command), landed with 6 hermetic corpus
> cases (all `support`; no wontfix / defer rows). It is the first
> sheet to use the optional §7 POSIX-divergence section (special
> built-in assignment-persistence) and the first with a filename
> deviation — by user decision it is `colon.md` (sheet-id `colon`)
> because a literal `:.md` is hostile to tooling. See §14 row 08.2d.
>
> Earlier on 2026-06-28 — batch-1 drafting continued (08.2): the
> third sheet, `false`, landed with 4 hermetic corpus cases (all
> `support`; no wontfix / defer rows — the exact `true` mirror with
> a fixed exit status of `1`). See §14 row 08.2c.
>
> Earlier on 2026-06-28 — batch-1 drafting continued (08.2): the
> second sheet, `true`, landed with 4 hermetic corpus cases (all
> `support`; no wontfix / defer rows). See §14 row 08.2b.
>
> Earlier on 2026-06-28 — batch-1 drafting started (08.2): the
> first sheet, `cd`, landed with 7 hermetic corpus cases. Two
> pre-existing bugs surfaced and were fixed in their own commits
> (recorder argv0 store-path; `cd` builtin global-CWD + `eprintln!`).
> See §14 row 08.2a.
>
> Earlier on 2026-06-28: subtask 08.5 landed: the
> `fredshell-spec-macros` proc-macro crate and the `refuse!` macro,
> plus the `fredshell_core::spec::Refusal` value type. See the §14
> log. Remaining: sheets (08.2 / 08.3 / 08.6), sheet-driven `help`
> (08.7), refusal corpus cases (08.8), and the `pc` / `check` wiring
> of `check-specs`.
>
> Earlier on 2026-06-28: implementation started on branch
> `task-07/spec-drafting`. Subtask 08.1 (template + `Documents/specs`
> README) and 08.4 (`cargo xtask check-specs` cross-reference
> checker) landed; see the new §14 implementation log. Status line
> moved from `stub` to `in progress`. Sheets (08.2 / 08.3 / 08.6),
> the `refuse!` macro (08.5), and the `pc` / `check` wiring of
> `check-specs` remain pending.
>
> Earlier on 2026-05-24: cascade renumber to insert PLAN_10
> embedding (ADR 0006): functional metadata "Consumed by"
> updated — "PLAN_06 Phase B" → "PLAN_12 Phase B" and `10.N`
> subtask prefix → `12.N` (PLAN_13 retains its `12.N` subtask
> IDs per stable-subtask-ID rule). References block file paths
> remapped. Substance unchanged.
>
> Previously (2026-05-23): §4 template canonicalises
> `Tier: feature` (resolves Q08.1 / Q-08-A); §5.3 documents the
> "workarounds are best-effort guidance, not contract" policy
> (resolves Q08.2 / Q-08-B); §3 template adds optional
> `### 3.A`-style sub-headers for long sheets and permits
> multi-row shared corpus references; §2.1 flags `set` and
> `shopt` as the two unusually long sheets (resolves Q08.3 /
> Q-08-C); §8.2 documents the rebuild-coupling trade-off of
> compile-time `refuse!` validation (resolves Q08.4 / Q-08-D).
> §2.2 adds the UTF-8 / locale feature category and §2.3 bumps
> the total to ~80 sheets, covering locale correctness in v1
> while PLAN_08's UTF-8 fuzz tier is post-v1 (Q-09-5).
> Earlier on 2026-05-22 — initial draft.
> Phase: B. Status: in progress (methodology drafted; 08.1 + 08.4 +
> 08.5 landed; sheets, `help`, and refusal corpus cases pending).
> Consumes: PLAN_05 §3 corpus structure, PLAN_05 §11 builtin
> inventory; ADR 0003 test-first methodology; ADR 0001 builtin
> tiers. Consumed by: PLAN_12 Phase B (each PLAN_12 subtask requires
> a `support`-classed sheet before implementation); PLAN_08 (uses
> sheets as the prose oracle for differential cases); PLAN_13
> (each of the eight job-control builtins requires a sheet before
> the corresponding 12.N subtask lands).

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
  in milestone N. `N` is a `PLAN_19` milestone number. Deferred
  rows turn into post-v1 worklist entries.

Every behaviour in bash's surface has exactly one classification.
A sheet with un-classified rows is incomplete; it cannot drive a
PLAN_06 / PLAN_13 subtask.

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
- 8 sheets owned by PLAN_13 (`bg`, `fg`, `jobs`, `kill`, `wait`,
  `disown`, `suspend`, `trap`).
- 2 sheets owned by PLAN_14 (`fc`, `history`).
- 7 sheets in "PLAN_13 extended utilities" category: `caller`,
  `dirs`, `getopts`, `hash`, `help`, `logout`, `mapfile`, `popd`,
  `printf`, `pushd`, `read`, `readarray`, `type`, `ulimit`, `umask`.

Two sheets in this set are unusually long because of bash's
broad option surfaces: `set` (~80 rows: one per `-o` longopt
plus combinatoric edge rows) and `shopt` (~50 rows). Drafters
of those two sheets should expect the volume and section
the support matrix with `### 3.A`-style sub-headers per §4.
All other Tier-1 sheets are 5–25 rows.

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
- **UTF-8 and locale behaviour:** byte-vs-char `${#var}` and
  `${var:offset:len}`, multibyte glob ranges and bracket
  expressions, `LC_COLLATE` ordering for `case` and `[[` `<`/`>`,
  `LC_CTYPE` effects on `[[:alpha:]]`-style classes, UTF-8
  identifier bytes in variable names (where bash accepts them),
  `$'\uXXXX'` and `$'\UXXXXXXXX'` escapes, `printf %b`
  multibyte handling. This category exists because PLAN_08's
  v1 fuzzer is `LC_ALL=C`-only (PLAN_08 §11 Q09.5); locale
  correctness is therefore hand-curated here until the
  post-v1 `F2-utf8` fuzz tier ships (`PLAN_19` milestone
  M-15-utf8-fuzz). Cases live in `tests/spec/utf8_locale/`.

That is approximately 22 feature sheets plus one UTF-8/locale
sheet for ~23 total.

### 2.3. Total

Tier-1 builtins: ~57 sheets. Features: ~23 sheets (including
the UTF-8/locale sheet from §2.2). **Total: ~80 sheets.** This
number is the basis for the batch-of-10 review cadence in §7.

### 2.4. What does _not_ get a sheet

- **Tier-2 builtins.** Per ADR 0001 they are "userspace
  utilities" whose contract is determined by usage, not by
  matching bash. They get individual planning when they are
  proposed.
- **`coproc`.** Deferred from v1 entirely (PLAN_13 §12 Q10.3).
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

**Exception — symbol-named builtins.** Three Tier-1 builtins are
named with punctuation rather than letters: `:`, `.`, and `[`. Using
the glyph as the filename is hostile to tooling — it breaks glob
patterns, and because the sheet-id is the filename stem it renders
the `<sheet-id>-<row#>` refusal diagnostic as nonsense like
`:-3.1`. These sheets therefore take a spelled-out ASCII filename,
and the sheet-id is that filename:

| Builtin | Sheet filename | Sheet-id  |
| ------- | -------------- | --------- |
| `:`     | `colon.md`     | `colon`   |
| `.`     | `dot.md`       | `dot`     |
| `[`     | `bracket.md`   | `bracket` |

The sheet's H1 and §1 Synopsis still carry the real invocation name,
and the sheet opens with an HTML comment recording the deviation so
the exception is visible where it applies. `colon.md` landed in
subtask 08.2d; `dot.md` and `bracket.md` are drafted under 08.6.
No other sheet may deviate from the invocation-name rule.

Sheets are Markdown. Markdown is not a clever choice — it is the
worst format that still works — but it is what the rest of the
plan documents use and it renders adequately on GitHub. Sheets
are read by humans more than by tools; readability wins.

## 4. Sheet template

Every sheet has the same top-level structure. Deviations are not
permitted; the template is enforced by `cargo xtask check-specs`
(added in subtask 08.4).

A single template covers both builtin sheets and feature sheets.
The only field that differs is `Tier:`:

- Builtin sheets (path under `Documents/specs/builtins/`) must
  carry `Tier: 1` or `Tier: 2`.
- Feature sheets (path under `Documents/specs/features/`) must
  carry the canonical marker `Tier: feature`.

The linter rejects any other combination: a builtin sheet with
`Tier: feature` is an error, as is a feature sheet with a
numeric tier. This gives the cross-kind type-checking benefit
without forcing two templates.

```markdown
# `<name>` — <one-line bash summary>

> Status: <draft | review | approved | superseded>
> Owner: PLAN_XX
> Tier: <1 | 2 | feature> # `feature` only on feature sheets
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

For every `defer:N` row in §3, one paragraph plus a `PLAN_19`
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
life as `status = "deferred:PLAN_12"` (or 10, or 07), and flips
to `status = "pass"` in the subtask that ships the
implementation. This is the same workflow PLAN_05 §11 already
describes; PLAN_07 sheets are the prose half of that contract.

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
`PLAN_19` milestone number). The user invoking it gets a
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

**Provisional milestone numbering.** `Documents/PLAN_19_milestones.md`
does not exist yet, so the milestone numbers used by `defer:N` rows
are provisional and are recorded here to keep them consistent across
sheets. Drafters MUST reuse an existing row rather than invent a new
number; if a deferred behaviour does not fit any theme below, stop
and raise it rather than adding a number unilaterally.

| N   | Provisional theme          | Meaning for `defer:N` rows                                                              | First used  |
| --- | -------------------------- | --------------------------------------------------------------------------------------- | ----------- |
| 3   | Filesystem-touch builtins  | Needs real filesystem semantics the v0 corpus sandbox cannot express (notably symlinks) | `cd` 3.8    |
| 4   | `shopt` shell options      | Gated on `shopt` existing, because the behaviour is toggled by a shell option           | `cd` 3.12   |
| 5   | UTF-8 / locale correctness | Locale-dependent; needs the recorder to run under an explicit UTF-8 locale (§2.2)       | `echo` 3.13 |

When `PLAN_19` is drafted it supersedes this table: the numbers are
remapped to real milestone IDs in one pass across all sheets, and
this subsection is replaced by a pointer to `PLAN_19`.

**Workarounds are best-effort guidance, not contract.** The
workaround is the sheet drafter's good-faith hint about how to
emulate the missing behaviour until milestone N lands; it is
_not_ a binding promise. If the workaround later breaks because
of unrelated changes (a builtin's flag set narrows, an
expansion pass tightens, a corpus case revises edge handling),
the workaround is updated in the next sheet review — but
fredshell does not owe a backwards-compatible fix to keep the
original workaround working. Users following workarounds should
treat them like any other deferred-feature guidance: useful
today, subject to revision.

This policy is intentional. Promoting workarounds to contract
would require a corpus case per workaround (estimated
150–200 additional v1 cases across the sheet inventory), most
of which would be brittle (workarounds frequently emulate
multi-step behaviours that change shape as adjacent features
mature). The cost-benefit does not survive the v1 corpus
budget. Drafters who want a stronger guarantee for a specific
workaround should propose promoting the underlying row to
`support` instead.

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
related behaviours are reviewed together (batch 2 = PLAN_13
job-control builtins; batch 3 = grammar features for
expansions; etc.).

A batch is reviewed by reading all 10 sheets back-to-back and
filing comments at the batch level. Comments fall into three
classes: row-classification disputes ("`echo`-3.4 should be
defer, not wontfix"), inventory gaps ("`set` is missing
`-o privileged`"), and template violations. The batch's owning
PLAN doc is updated to record landing.

## 8. Spec-runner integration

PLAN_05's spec runner already understands case status. PLAN_07
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

**Rebuild coupling — accepted trade-off.** Compile-time
validation means every sheet edit triggers a rebuild of every
crate containing a `refuse!` referencing that sheet. During
the high-churn drafting phase (subtasks 08.2 and 08.6), this
will cause frequent `fredshell-core` rebuilds. This cost is
accepted in exchange for the build-time guarantee that no
broken refusal can ship: a misspelled row ID, a deleted row,
or a row whose classification flipped to `support` (meaning
the refusal must be removed) all fail at the call site, not
in CI. Implementations target `proc_macro2` + a small
`syn`-style parser over the sheet's §3 table, so the per-call
expansion cost is bounded; the dominant cost is one
`include_str!` per crate per sheet referenced.

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

| Subtask | Surface                                                               | Owner     | Gate                           | Status                     |
| ------- | --------------------------------------------------------------------- | --------- | ------------------------------ | -------------------------- |
| 08.1    | Author `Documents/specs/_TEMPLATE.md` and `Documents/specs/README.md` | `PLAN_07` | none                           | Done (§14)                 |
| 08.2    | Draft and review batch 1 (10 sheets: `cd`, trivial builtins, state)   | `PLAN_07` | 08.1                           | In progress — 5 of 10 done |
| 08.3    | Draft and review batch 2 (`PLAN_13` job-control builtins)             | `PLAN_07` | 08.2, `PLAN_13` reviewed       | Not started                |
| 08.4    | `cargo xtask check-specs` cross-reference checker                     | `PLAN_07` | 08.1                           | Done (§14)                 |
| 08.5    | `refuse!` macro and unit tests                                        | `PLAN_07` | 08.1                           | Done (§14)                 |
| 08.6    | Draft and review batches 3–8 (~60 sheets, owner-grouped)              | `PLAN_07` | 08.2                           | Not started                |
| 08.7    | Sheet-driven `help` builtin                                           | `PLAN_06` | 08.1, `PLAN_06` Phase B `help` | Not started                |
| 08.8    | First wontfix refusal corpus cases (`tests/spec/refusals/`)           | `PLAN_07` | 08.5                           | Not started                |

Subtasks 08.2 and 08.3 unblock `PLAN_13`'s implementation;
subtask 08.6 unblocks `PLAN_06` Phase B's implementation.

### 10.1. Batch-1 sheet checklist (subtask 08.2)

The ten batch-1 sheets, in the §7 recommended order. One sheet per
commit; the log ID is the §14 row. Sheets are drafted against the
pinned reference bash (`FREDSHELL_REFERENCE_BASH`, bash 5.3p9) and
every `support` row carries a corpus case with
`status = "deferred:PLAN_12"`.

| #   | Sheet    | Log ID | Status  | Notes                                                              |
| --- | -------- | ------ | ------- | ------------------------------------------------------------------ |
| 1   | `cd`     | 08.2a  | Done    | 12 rows; 7 support, 1 wontfix, 4 defer                             |
| 2   | `true`   | 08.2b  | Done    | 4 rows, all support                                                |
| 3   | `false`  | 08.2c  | Done    | 4 rows, all support; `true` mirror                                 |
| 4   | `:`      | 08.2d  | Done    | 6 rows, all support; `colon.md` per §3; first §7 use               |
| 5   | `echo`   | 08.2e  | Done    | 14 rows; 12 support, 2 defer; surfaced cleanup 08.2e-CU1           |
| 6   | `printf` | —      | Pending | Escape-heavy — read cleanup 08.2e-CU1 (§15) before drafting        |
| 7   | `set`    | —      | Pending | ~80 rows; use `### 3.A` sub-headers per §4                         |
| 8   | `shopt`  | —      | Pending | ~50 rows; owns the `defer:4` dependency of `cd` 3.12 / `echo` 3.14 |
| 9   | `unset`  | —      | Pending | State-mutating                                                     |
| 10  | `trap`   | —      | Pending | Most complex Tier-1 builtin; sets the per-flag detail bar          |

**Batch-1 exit criteria.** All ten sheets landed, then the batch is
reviewed back-to-back per §7 and the batch-level comments are filed.
`xtask check-specs` remains red until 08.6 completes the inventory —
see §8.1 and the 08.4 row in §14; that is expected, not a
regression, and it is deliberately not wired into `xtask pc` or CI
until 08.6.

## 11. Open questions

- **Q08.1** — Should feature sheets and builtin sheets share a
  template, or is the feature template slightly different
  (e.g., no Tier line)? **Resolved (2026-05-23):** single
  template; feature sheets carry the canonical marker
  `Tier: feature` (linted by path). See §4.
- **Q08.2** — Are `defer:N` workarounds binding? If we promise
  "use `cd && ls`" and that breaks for someone, do we owe them
  a fix? **Resolved (2026-05-23):** no, workarounds are
  best-effort guidance, not contract. See §5.3 for the
  policy and rationale.
- **Q08.3** — How do we handle bash's many `-o` longopts for
  `set` and `shopt`? Each is conceptually a row. That makes
  the `set` sheet ~80 rows long. **Resolved (2026-05-23):**
  one row per `-o` option; the §4 template permits
  `### 3.A`-style sub-headers in long sheets for readability
  and allows multiple support rows to reference a shared
  corpus case when their behaviours are meaningfully verified
  together. See §2.1 (long-sheet note) and §3 in the template.
- **Q08.4** — Should `refuse!` validate sheet row references
  at compile time (`include_str!`) or at runtime (offline
  `xtask check-specs` only)? **Resolved (2026-05-23):**
  compile-time validation per §8.2. Rebuild-coupling cost
  during the drafting phase is accepted in exchange for the
  guarantee that no broken refusal can ship.

## 12. Relationship to other plans

- **PLAN_05** — corpus and harness; PLAN_07 sheets reference
  PLAN_05 cases by path; PLAN_05 §11 is the inventory PLAN_07
  exhausts. PLAN_07 does not change the harness.
- **PLAN_12** — every PLAN_12 (Phase B) subtask is gated
  on a `support`-classed sheet existing. PLAN_12 already
  cites PLAN_07.
- **PLAN_08** — uses sheets as the prose oracle when deciding
  what to fuzz. The fuzzer's expectation file format
  references sheet row numbers.
- **PLAN_13** — eight job-control builtin sheets are batch 2.
  Each PLAN_13 subtask is gated on its sheet being approved.
- **`PLAN_19`** — milestone-N labels in `defer:N` rows point at
  `PLAN_19` milestone definitions.

## 13. References

- `Documents/PLAN_05_testing.md` — corpus structure, status
  taxonomy, builtin inventory.
- `Documents/PLAN_12_exec_phase_b.md` — Phase B subtasks gated on
  sheets.
- `Documents/PLAN_08_fuzzer.md` (pending) — differential oracle.
- `Documents/PLAN_13_traps_and_jobs.md` — job-control builtin
  sheets.
- `Documents/PLAN_19_milestones.md` (pending) — milestone
  numbering used by `defer:N`.
- `Documents/decisions/0001-in-process-execution-and-builtin-tiers.md`
  — Tier-1 / Tier-2 definitions referenced in §2.
- `Documents/decisions/0003-test-first-compatibility-methodology.md`
  — establishes the corpus as ground truth, which sheets
  annotate.
- Bash reference manual, current version pinned in each sheet's
  `Sources` line.
- POSIX.1-2024 ("Issue 8") shell command-language section.

## 14. Implementation log

To be filled as subtasks complete, one row per subtask, format
matching PLAN_05 §14. `Commit` is `TBD` until the task branch
merges to `main`.

| Subtask | Commit | Date       | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ------- | ------ | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 08.1    | TBD    | 2026-06-28 | Authored `Documents/specs/_TEMPLATE.md` (canonical spec-sheet template per §4: seven mandatory sections §1 Synopsis … §6 Deferred rows + §8 References, with the conditional §7 POSIX divergence) and `Documents/specs/README.md` (indexed layout reference per §3: directory tree, the three classifications, authoring workflow, status values, and the relationship to the `tests/spec/` corpus and the 08.4 / 08.5 tooling). The template's leading HTML comment documents the `cp`-then-edit workflow and the mandatory-section rule. Markdown docs do not carry the `.rs` copyright header. Bare angle-bracket placeholders that trip MD033 (`<one-line bash summary>`, `<category>/<case>`, `<NAME>`) were backticked to lint clean while preserving the fill-in intent — a minor, deliberate deviation from the plan's literal template text. All pre-commit hooks green (markdownlint 0 errors, prettier, codespell, xtask-check). No Rust touched.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| 08.4    | TBD    | 2026-06-28 | Added `cargo xtask check-specs` (`xtask/src/check_specs.rs`, top-level `Cmd::CheckSpecs`). Walks `Documents/specs/` (excluding `_TEMPLATE.md` / `README.md`) and enforces the five §8.1 invariants: (1) every `support` §3 row names a corpus case that loads via `fredshell_spec_runner::Case::load` and declares `status` `pass` or `deferred:PLAN_XX`; (2) every `tests/spec/**/*.case.toml` is referenced by exactly one sheet row (zero = orphan, >1 = conflict; both fail); (3) no §3 row carries the `???` placeholder; (4) the seven mandatory headings are present and in order, with the optional §7 tolerated between §6 and §8; (5) every `defer:N` row is backed by a §6 workaround paragraph. The sheet parser is line-oriented (no Markdown-parser dependency), matching the sibling `spec` module's house style; row recognition keys on a dotted row number plus a recognised classification cell, which excludes header/separator rows and illustrative tables. The corpus root is injected into `check_support_row_resolves` so the 31 unit tests stay hermetic (no `set_current_dir`). **Deliberately not yet wired into `xtask pc` / `check`:** check 2 cannot pass until the sheet inventory is complete (08.2 / 08.3 / 08.6), so against the current tree the command correctly reports all 21 corpus cases as orphans and exits 1; the `pc` / `check` wiring lands in the 08.6 completion commit. 31 new unit tests (classification/row/section parsing, section-order validation, all five checks); `cargo test --workspace` + `clippy --all-targets --all-features -D warnings` + `cargo-machete` clean.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| 08.2a   | TBD    | 2026-06-28 | Batch 1, sheet 1 of 10: `cd`. Authored `Documents/specs/builtins/cd.md` (12 §3 rows: 7 `support`, 1 `wontfix` (`-@` extended attributes, the §5.2 canonical example), 4 `defer` (`-L`/`-P` symlink resolution and `-e` deferred to milestone 3 because the spec-runner's `copy_dir_recursive` skips symlinks in v0; `cdable_vars` deferred to milestone 4 pending `shopt`)). Added 7 hermetic corpus cases under `tests/spec/builtins_tier1/cd_*.case.toml` (`status = "deferred:PLAN_12"`), each using only shell builtins + `.fs/` skeletons (no external coreutils, per the recorder's `env_clear()` constraint) and `${PWD##*/}` instead of `basename` for sandbox-independent output. Fixtures recorded against bash 5.3p9. `check-specs` reports the `cd` sheet clean (all 7 support rows resolve, sections valid, defer rows carry §6 workarounds); the remaining global check-2 orphan failures are the not-yet-sheeted corpus cases (08.6). `spec lint` + `compat` green (7 cd cases honored as `deferred:PLAN_12`); `COMPAT.md` regenerated. Two pre-existing bugs surfaced and were fixed in their own commits before this one: the recorder baking bash's `/nix/store` path into `.stderr` fixtures (argv0 fix), and the `cd` builtin mutating global process CWD + `eprintln!` from core (env.cwd + env.stderr fix). Remaining batch-1 sheets (`echo`, `printf`, `true`, `false`, `:`, `set`, `shopt`, `unset`, `trap`) follow in subsequent steps.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 08.5    | TBD    | 2026-06-28 | Created the `fredshell-spec-macros` proc-macro crate and the `refuse!` macro (§8.2). Added a typed `Refusal` value (`fredshell_core::spec`: `Refusal`, `RefusalKind::{Wontfix, Defer{milestone, milestone_name, workaround}}`, `REFUSAL_EXIT_STATUS = 2`) whose `Display` renders the exact §5.2 / §5.3 wording; per ADR 0006 the core returns this value rather than writing to a file descriptor (the binary's REPL renders it as a diagnostic). `refuse!` reads the referenced sheet at **compile time**, parses the §3 table (reusing the check-specs line-oriented row recogniser), and fails compilation if the sheet/row is missing or the classification does not match the form. Grammar: `refuse!(wontfix, "<id>", "<row>")` and `refuse!(defer, "<id>", "<row>", milestone_name = "…", workaround = "…")` — the milestone `N` comes from the §3 `defer:N` cell; the milestone name and workaround are §6 prose and so are passed as named args (a minor, documented extension of §8.2's single wontfix example, since compile-time §6-prose extraction would be fragile). Sheet resolution ascends from `CARGO_MANIFEST_DIR` to `Documents/specs/`, overridable via `FREDSHELL_SPECS_ROOT` (used by the test fixture sheet at `tests/specs/builtins/cd.md`, injected through a `build.rs` `rustc-env`). New deps: `proc-macro2` / `quote` / `syn` (production) and `trybuild` (dev); workspace members + AGENTS.md crate table + dependency-direction note updated in this change. The macro crate carries a test-only dev-dependency cycle back to `fredshell-core` for the expansion tests (Cargo permits dev-dep cycles). Tests: 5 `Refusal` rendering tests in core, 5 `sheet`-parser unit tests, 5 `refuse!` expansion integration tests (wontfix + defer happy paths, named-arg order independence), and 5 `trybuild` compile-fail cases (missing row, wrong classification, missing sheet, bad form, wontfix-on-defer-row) proving the §8.2 compile-time guarantee. `cargo test --workspace` + `clippy --all-targets --all-features -D warnings` + `cargo-machete` + `cargo fmt --check` all clean.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 08.2b   | TBD    | 2026-06-28 | Batch 1, sheet 2 of 10: `true`. Authored `Documents/specs/builtins/true.md` (4 §3 rows, all `support`; no `wontfix` / `defer` rows — §5 and §6 carry an explicit "None." per the seven-mandatory-section rule). Probed bash 5.3p9 (`help true`, behaviour probes): the builtin ignores every operand and option — including `--help` / `--version` — and always exits `0` with no output, in deliberate contrast to GNU coreutils `/usr/bin/true`, which the builtin shadows inside the shell (captured as §4 quirks 1–2 and row 3.3). Added 4 hermetic corpus cases under `tests/spec/builtins_tier1/true_*.case.toml` (`status = "deferred:PLAN_12"`): `true_exit_zero`, `true_ignores_args`, `true_ignores_help`, `true_no_output`. The no-output case reads its redirect files with `$(<file)` rather than `cat` to stay within the recorder's `env_clear()` no-external-coreutils constraint. Fixtures recorded against bash 5.3p9. `check-specs` reports the `true` sheet clean (all 4 support rows resolve, sections valid); the 4 new cases drop out of the orphan list, leaving the global count at the expected drafting-window 21 (still red, not yet wired into `pc` / `check` — that lands at 08.6). `spec lint --skip-builtins-drift`, `compat` (result: ok, no regressions), `cargo test --workspace`, `clippy --all-targets -D warnings`, `cargo-machete`, `cargo fmt --check`, `markdownlint-cli2`, and `prettier --check` all green; `COMPAT.md` regenerated. No Rust touched. Remaining batch-1 sheets (`echo`, `printf`, `false`, `:`, `set`, `shopt`, `unset`, `trap`) follow in subsequent steps.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| 08.2c   | TBD    | 2026-06-28 | Batch 1, sheet 3 of 10: `false`. Authored `Documents/specs/builtins/false.md` (4 §3 rows, all `support`; no `wontfix` / `defer` rows — the exact `true` mirror, §5 and §6 carry an explicit "None."). Probed bash 5.3p9 (`help false`, behaviour probes): the builtin ignores every operand and option — including `--help` / `--version` — and always exits `1` with no output, in deliberate contrast to GNU coreutils `/usr/bin/false`, which the builtin shadows inside the shell (captured as §4 quirks 1–2 and row 3.3). Added 4 hermetic corpus cases under `tests/spec/builtins_tier1/false_*.case.toml` (`status = "deferred:PLAN_12"`): `false_exit_one`, `false_ignores_args`, `false_ignores_help`, `false_no_output`. The no-output case reads its redirect files with `$(<file)` rather than `cat` to stay within the recorder's `env_clear()` no-external-coreutils constraint; the failing exit is observed via `echo "exit=$?"` so the recorded status is the trailing `echo`'s `0`. Fixtures recorded against bash 5.3p9. `check-specs` reports the `false` sheet clean (all 4 support rows resolve, sections valid); the 4 new cases drop out of the orphan list, leaving the global count at the expected drafting-window 21 (still red, not yet wired into `pc` / `check` — that lands at 08.6). `spec lint --skip-builtins-drift`, `compat` (result: ok, no regressions), `cargo test --workspace`, `clippy --all-targets -D warnings`, `cargo-machete`, `cargo fmt --check`, `markdownlint-cli2`, and `prettier --check` all green; `COMPAT.md` regenerated. No Rust touched. Remaining batch-1 sheets (`echo`, `printf`, `:`, `set`, `shopt`, `unset`, `trap`) follow in subsequent steps.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| 08.2d   | TBD    | 2026-06-28 | Batch 1, sheet 4 of 10: `:` (null command). Authored `Documents/specs/builtins/colon.md` (6 §3 rows, all `support`; no `wontfix` / `defer` rows). **Filename deviation (user decision):** `PLAN_07` §3.1 says a builtin's sheet filename is exactly its invocation name, but a literal `:.md` is hostile to tooling — broken globs and a `:-3.1` `<sheet-id>-<row#>` `refuse!` diagnostic. The sheet is named `colon.md` (sheet-id `colon`); the H1 and §1 Synopsis carry the real `:` name, and a leading HTML comment records the deviation. This is also the **first sheet to exercise the optional §7 POSIX-divergence section** (special built-in assignment persistence) — the `check-specs` section-order check tolerates §7 between §6 and §8, confirmed clean. Probed bash 5.3p9 (`help :`, side-effect probes): `:` expands its arguments (so `${var=word}` assigns and `$(cmd)` runs) and performs redirections (so `: > file` truncates), unlike `true` which treats operands as inert; it always exits `0`. As a POSIX special built-in, `var=value :` prefix assignments persist only under `--posix` — default bash (and fredshell, row 3.6 + §7) discards them. Added 6 hermetic corpus cases under `tests/spec/builtins_tier1/colon_*.case.toml` (`status = "deferred:PLAN_12"`): `colon_exit_zero`, `colon_ignores_args`, `colon_arg_assign`, `colon_cmd_subst`, `colon_truncate`, `colon_assign_scope`. All cases use only shell builtins + `$(<file)` (no external coreutils, per the recorder's `env_clear()` constraint). Fixtures recorded against bash 5.3p9. `check-specs` reports the `colon` sheet clean (all 6 support rows resolve, sections valid incl. §7); the 6 new cases drop out of the orphan list, leaving the global count at the expected drafting-window 21 (still red, not yet wired into `pc` / `check` — that lands at 08.6). `spec lint --skip-builtins-drift`, `compat` (result: ok, no regressions), `cargo test --workspace`, `clippy --all-targets -D warnings`, `cargo-machete`, `cargo fmt --check`, `markdownlint-cli2`, and `prettier --check` all green; `COMPAT.md` regenerated. No Rust touched. Remaining batch-1 sheets (`echo`, `printf`, `set`, `shopt`, `unset`, `trap`) follow in subsequent steps.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 08.2e   | TBD    | 2026-06-28 | Batch 1, sheet 5 of 10: `echo`. Authored `Documents/specs/builtins/echo.md` (14 §3 rows: 12 `support`, 2 `defer`) — the first moderate sheet of the batch. Probed bash 5.3p9 (`help echo` plus byte-level `od` probes under `env -i`): arguments are space-joined with a trailing newline; `-n` suppresses the newline; `-e` enables the C-escape set (`\t`, `\n`, `\r`, `\\`, `\a`, `\b`, `\e`, `\f`, `\v`, octal `\0nnn`, hex `\xHH`); `-E` (the default) keeps escapes literal; flags bundle (`-ne`); option parsing stops at the first word that is not a `-`-prefixed run of `n` / `e` / `E`, so `-nx`, `--`, `--help`, and `-x` all print verbatim (no end-of-options or help handling — §4 quirk 1); `\c` truncates the whole output including the newline (§4 quirk 3); and echo's octal needs a leading zero (`\0101`) unlike `printf` (§4 quirk 4). Two `defer` rows: 3.13 `\u` / `\U` Unicode escapes → milestone 5 (locale-dependent — under the recorder's `env_clear()` C locale bash prints them verbatim instead of emitting UTF-8 bytes, so they belong to the `PLAN_07` §2.2 UTF-8/locale category and `tests/spec/utf8_locale/`), and 3.14 `xpg_echo`-default escape interpretation → milestone 4 (pending the `shopt` sheet, mirroring `cd`'s `cdable_vars`). §7 records the POSIX divergence (POSIX `echo` defines none of `-n` / `-e` / `-E`). Added 12 hermetic corpus cases under `tests/spec/builtins_tier1/echo_*.case.toml` (`status = "deferred:PLAN_12"`), all shell-builtin-only, using `$(<file)` where bytes must be captured. Confirmed the runner's TOML `"""` script field is an escape-processing basic string, so `\\t` in a case becomes the literal `\t` bash sees. **Surfaced tooling conflict:** the repository's `mixed-line-ending` pre-commit hook rewrites a raw CR to LF, which silently corrupted the first `echo_e_escapes` fixture (its `\r` byte became `\n`) and rejected the commit. The case was redesigned to compare each `-e` result against its ANSI-C `$'...'` equivalent and emit only printable `name=ok` tokens, so no control bytes reach a fixture; the octal (3.11) and hex (3.12) cases still assert golden printable bytes, which anchors the escape machinery independently of `$'...'`. Any future fixture needing a literal CR will hit the same hook — see the cleanup entry proposed in the completion report. Fixtures recorded against bash 5.3p9. `check-specs` reports the `echo` sheet clean (all 12 support rows resolve, sections valid incl. §7, both `defer` rows carry §6 workarounds); the 12 new cases drop out of the orphan list, leaving the global count at the expected drafting-window 21 (still red, not yet wired into `pc` / `check` — that lands at 08.6). `spec lint --skip-builtins-drift`, `compat` (result: ok, no regressions), `cargo test --workspace`, `clippy --all-targets -D warnings`, `cargo-machete`, `cargo fmt --check`, `markdownlint-cli2`, and `prettier --check` all green; `COMPAT.md` regenerated. No Rust touched. Remaining batch-1 sheets (`printf`, `set`, `shopt`, `unset`, `trap`) follow in subsequent steps. |

| 08.2-docs | TBD | 2026-06-28 | Handoff-quality consolidation of the batch-1 drafting documentation, before the branch is opened as a PR. No sheets or corpus cases changed. (1) Added §15 Cleanup registry with entry 08.2e-CU1 for the `mixed-line-ending` hook corrupting recorded fixtures that contain a lone CR — previously that defect existed only as prose inside the 08.2e log cell, which the AGENTS.md "pre-existing bugs surfaced during a subtask" rule forbids ("informal known issues sections are NOT used"). (2) Added a Status column to the §10 subtask table and a new §10.1 batch-1 sheet checklist (5 of 10 done, per-sheet log IDs, per-sheet notes, and the batch exit criteria), so progress is legible without reading five multi-thousand-character §14 cells. (3) Recorded the symbol-named-builtin filename exception in §3 as a table — `:` → `colon.md`, `.` → `dot.md`, `[` → `bracket.md` (names chosen by the user) — because the invocation-name rule cannot apply to punctuation and `.` and `[` are still unsheeted, so the 08.2d decision would otherwise be re-litigated. (4) Added a provisional `defer:N` milestone table to §5.3 (3 = filesystem-touch, 4 = `shopt`, 5 = UTF-8/locale) with an instruction to reuse rather than invent numbers, since `Documents/PLAN_19_milestones.md` does not exist yet and the numbers were being coined per-sheet. (5) Expanded `Documents/specs/README.md` §"Authoring a sheet" with the probe-bash rule, the recorder constraints that actually bite (no `PATH` so coreutils exit 127 — use `$(<file)` / `${PWD##*/}`; effectively `C` locale; symlinks do not materialise; `.keep` in `.fs/` leaf dirs; `[env]` renamed fields and `$SANDBOX`; TOML `"""` processes escapes so `\\t` yields `\t`), the fixtures-are-golden-data warning pointing at 08.2e-CU1, and the prettier/CommonMark trap. **Bonus fix:** four stale `PLAN_16` milestone references in §1, §2.2, §6 and §12 were corrected to `PLAN_19` — `PLAN_16` is the config plan; `PLAN_19` is milestones. These were cascade-renumber leftovers (§13 already pointed at `PLAN_19_milestones.md`) and would have sent a reader to the wrong document. `markdownlint-cli2` and `prettier --check` clean and prettier verified idempotent; `cargo test --workspace`, `clippy --all-targets -D warnings`, `cargo-machete`, `cargo fmt --check`, `spec lint`, and `compat` unaffected and green. No Rust touched. |

## 15. Cleanup registry

Pre-existing bugs and tooling defects surfaced by a subtask, per the
AGENTS.md "pre-existing bugs surfaced during a subtask" rule. Format
matches `PLAN_05` §15.

| ID        | Surface                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              | Impact                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | Fix scope                                                                                     | Status |
| --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 08.2e-CU1 | Surfaced during 08.2e (`echo` sheet). The repository's `mixed-line-ending` pre-commit hook rewrites a lone CR to LF in any file it is given, including recorded spec fixtures under `tests/spec/`. The `echo_e_escapes` case originally asserted `echo -e` escape interpretation by emitting the control characters directly; its `.stdout` fixture therefore contained a legitimate `\r` byte, which the hook silently rewrote to `\n` — corrupting the fixture and then rejecting the commit. Fixtures are golden oracle data recorded from the pinned bash; no formatting hook should be allowed to edit them. The sibling `trailing-whitespace` and `end-of-file-fixer` hooks have the same hazard for any fixture whose expected output legitimately ends in whitespace or lacks a trailing newline (for example a `printf` case with no `\n`, or an `echo -n` case recorded without the `[END]` marker trick). | Medium, and the failure mode is silent-then-loud: the hook edits the fixture before the commit is rejected, so a careless re-stage commits a wrong golden value. It has already cost one drafting cycle. `printf` is the next batch-1 sheet (§10.1) and is the most escape-heavy builtin in the inventory, so it is the most likely next casualty; `read` (CRLF handling) and any future `\r`-bearing case are also exposed. 08.2e worked around it inside the case by comparing each `-e` result against its ANSI-C `$'...'` equivalent and emitting only printable `name=ok` tokens, which keeps the row's contract genuinely tested but is a per-case dodge, not a fix. | Exclude recorded fixtures from the whitespace/line-ending hooks: add `tests/spec/.\*\.(stdout | stderr | exit)$`to the`mixed-line-ending`, `trailing-whitespace`, and `end-of-file-fixer`excludes. The hook set comes from the upstream`FredSystems/pre-commit-checks`flake, which exposes`extraExcludes`(the same injection point 05.11-CU1 discusses), so this is a`flake.nix`change and needs no upstream work. Verify with a deliberate regression fixture containing a lone CR:`git commit`must leave the bytes untouched. Then re-record`echo_e_escapes` in its direct byte-emitting form and confirm the CR survives a commit round-trip, or keep the ANSI-C comparison form and note that the row is verified by equivalence — either is acceptable once the hook can no longer corrupt fixtures. | Open. Should be fixed before the `printf` sheet (08.2, sheet 6 of 10) so that sheet can record escape fixtures directly instead of reinventing the comparison dodge. Not blocking any already-landed sheet. Tracked here per the AGENTS.md "pre-existing bugs surfaced during a subtask" rule. |
