# PLAN_11 — Execution pipeline, Phase B (real semantics)

> Last updated: 2026-05-24 — ShellState §5 owner column corrected
> from PLAN_06 to PLAN_11 for Phase-B-owned fields; §6 builtin
> inventory label updated; cross-ref to PLAN_19_coproc.md fixed;
> PLAN_08 subtask cross-ref renumbered (09.7 → 08.7); Phase B
> parallelism prose updated. Substance unchanged.
>
> Previously (2026-05-23): Document established by the work-order
> renumber. Phase B was previously drafted in PLAN_06 §13 and then
> extracted to a temporary `PLAN_06b_exec_phase_b.md`; this commit
> finalises that extraction by landing it at its work-order slot
> as PLAN_11. The split reflects work-order reality: Phase A (PLAN_06
> §1–§12) is implemented; Phase B is corpus-dependent and gated on
> PLAN_08 F1 and on PLAN_07 spec sheets. Tracking Phase B in its own
> document lets PLAN_06 represent the implemented Phase A scaffold
> without dragging the Phase B drafting weight along.
>
> Status: drafted, gated on 06b.0 (PLAN_08 F1 green on `main`) per
> ADR 0003 + ADR 0004. Subtask numbering retained as `06b.N` for
> continuity with the prior in-PLAN_06 §13 grid and with the
> existing `task-06b/` branch namespace.
>
> Content history: previously lived as §13 of PLAN_06_exec.md, then
> briefly as PLAN_06b before the work-order renumber. All
> cross-references that read "PLAN_06 §13.x" have been retargeted to
> "PLAN_11 §x+1" in the renumber commit; sub-references "§13.N"
> inside this document have been renumbered to "§N+1"
> (e.g. §13.4 → §5).

This document owns Phase B of the parse-and-execute pipeline:
replacing PLAN_06's Phase A stub executor with a real lexer,
recursive-descent parser, native expansion passes, full Tier-1 +
Tier-2 builtin surface, `ShellState`, pipelines, redirections,
arithmetic, control flow, and job-control glue. PLAN_06 retains
the stable public surface this work lands behind.

## 1. Overview

Phase B owns the migration from the v0 stub to a native execution
pipeline. It cannot start before PLAN_05 produces a baseline corpus
pass-rate (ADR 0003), before PLAN_07 produces the per-builtin and
per-feature spec sheets that drive prioritisation, and before
PLAN_08's F1 differential is green (06.0 gate, below).

## 2. Gating dependencies

- **06.0 (gate)** — PLAN_08 F1 differential fuzzer must be green
  against pinned bash 5.3p9 on `main` (PLAN_08 §3.1). No Phase B
  implementation subtask may land while F1 is red. F1 is the
  single signal that says the executor is stable enough that
  divergences observed during Phase B implementation are about
  the change under review, not about pre-existing drift.
- **Per-builtin / per-feature** — each in-scope Tier-1 builtin and
  each grammar feature requires a `support`-classed PLAN_07 sheet
  before its implementation subtask begins. PLAN_07 sheets are
  the prose acceptance criteria; the corpus is the executable
  acceptance criteria.
- **ADR 0004 sunset** — Phase B retires the `/bin/sh -c` fallback
  path and the `FREDSHELL_ALLOW_SH_FALLBACK` escape hatch. The
  sunset commit lands as Phase B's final subtask (§8) and is
  contingent on the corpus pass-rate threshold below.

## 3. Lexer and parser scope

The Phase A stub stores source verbatim and dispatches per line.
Phase B replaces it with a native lexer + parser producing a
typed AST.

**Lexer.** Hand-rolled state-machine lexer in
`crates/fredshell-core/src/parser/lex/`:

- Tokens: word, operator, reserved word, newline, IO number,
  here-doc body.
- Quoting modes: unquoted, single-quoted, double-quoted, ANSI-C
  (`$'...'`), locale-translated (`$"..."` — recognise, do not
  translate in v1; refuse cleanly).
- Backslash handling per quoting mode.
- Comments (`#`) outside quotes.
- Here-doc body capture (delayed lexing until the line
  terminator).
- Position tracking for diagnostics: byte offset, line, column.
- No allocation per token in the common case — token spans are
  `&str` slices over the source buffer.

**Parser.** Recursive-descent over the lexer's token stream. AST
node families:

- `Program` → `CompleteCommand*`.
- `CompleteCommand` → `List` (`;`/`&`/`&&`/`||` separated).
- `List` → `Pipeline+` (`|`/`|&` separated).
- `Pipeline` → `Command+`.
- `Command` = simple | compound | function-definition.
- Simple = `(assignment* redirect* word*)`.
- Compound = `{ ... }`, `( ... )`, `if/elif/else/fi`,
  `while/do/done`, `until/do/done`, `for/do/done`, `case/esac`,
  `select/do/done`, `[[ ... ]]`, `(( ... ))`.
- `FunctionDefinition` = `name () compound` or
  `function name [()] compound`.

Q06B.1 was resolved on 2026-05-23: write our own
recursive-descent parser, for total control over diagnostic
quality and incremental parsing (PLAN_13's highlighter needs
the parser to tolerate a partial line). ADR 0005 (subtask
06b.1) ratifies this resolution before subtask 13B.2 starts.

`coproc` is recognised but refused for v1; reserved word `time`
is recognised and dispatched to the `time` keyword-level
builtin.

## 4. Executor pipeline

The executor consumes the AST and produces side-effects. The
pipeline in execution order:

1. **Expansion.** Six passes per word, in bash's order:
   brace → tilde → parameter/command/arithmetic → word-split →
   pathname → quote-removal. Lives in
   `crates/fredshell-core/src/exec/expand/` with one module per
   pass.
2. **Redirection setup.** Open files, dup fds, capture
   here-docs into pipes (or tempfiles above a configurable
   threshold). Failures are reported as `ExecError::Redirect`
   and do not start the command.
3. **Command resolution.** Look up the command name against, in
   order: aliases → functions → builtins (Tier-1 then Tier-2) →
   `PATH`-resolved external. Resolution returns a
   `ResolvedCommand` enum so dispatch is a single match.
4. **Dispatch.**
   - Builtin: call in-process; `ExitStatus` is the return.
   - Function: push a function-call frame on `ShellState`;
     execute the function body recursively; pop.
   - External: `fork` + `execve` via PLAN_04's `Process` API;
     `wait` if foreground, register in job table if background.
5. **Pipeline composition.** Wire the previous command's
   stdout to the next command's stdin via `pipe(2)`; the entire
   pipeline runs in a fresh process group; exit status is the
   last command's by default, all-status under `set -o
pipefail`.
6. **Job-table side-effects.** PLAN_12 owns the job table; the
   executor's contract is that every external command lands in
   that table with a known state by the time `dispatch` returns.

The expansion code is the single largest source of bash-quirk
risk. Every expansion pass has its own PLAN_07 sheet (the
`expansion/*` family). Implementation does not begin on a pass
until its sheet is `support`-classed and at least one passing
spec case exists.

## 5. `ShellState` fields

Phase B promotes `ExecEnv` from "I/O + sandbox" to "I/O +
sandbox + shell state." The new `ShellState` struct (private to
`fredshell-core::exec::state`) holds:

| Field         | Type                               | Owner   | Purpose                                                  |
| ------------- | ---------------------------------- | ------- | -------------------------------------------------------- |
| `variables`   | `Scope` tree                       | PLAN_11 | Shell + environment variables; supports `local` scoping. |
| `functions`   | `BTreeMap<String, FunctionDef>`    | PLAN_11 | User-defined functions; AST captured at definition.      |
| `aliases`     | `BTreeMap<String, String>`         | PLAN_11 | Pre-parse expansion; only at line-start position.        |
| `options`     | `SetOpts`                          | PLAN_11 | `set -o` long-form and `-e/-u/-x/-o pipefail/...` flags. |
| `shopts`      | `ShoptOpts`                        | PLAN_11 | `shopt` flag set (bash-only options).                    |
| `pos_args`    | `Vec<String>`                      | PLAN_11 | `$0`/`$1`.../`$@`.                                       |
| `last_status` | `ExitStatus`                       | PLAN_11 | `$?`.                                                    |
| `last_pid`    | `Option<Pid>`                      | PLAN_11 | `$!`.                                                    |
| `last_arg`    | `Option<String>`                   | PLAN_11 | `$_`.                                                    |
| `traps`       | `TrapTable`                        | PLAN_12 | Slot; PLAN_11 reserves the field but does not populate.  |
| `jobs`        | `JobTable`                         | PLAN_12 | Slot; PLAN_11 reserves the field but does not populate.  |
| `dirs_stack`  | `Vec<PathBuf>`                     | PLAN_12 | `pushd`/`popd`/`dirs`; slot only.                        |
| `umask`       | `mode_t`                           | PLAN_12 | Slot only.                                               |
| `cmd_hash`    | `HashMap<String, PathBuf>`         | PLAN_12 | `hash` builtin cache; slot only.                         |
| `history`     | `&mut dyn HistoryStore` (borrowed) | PLAN_13 | Borrowed from the editor; not owned by `ShellState`.     |

`ShellState` is owned by `ExecEnv` (one field). `ExecEnv` retains
its existing `cwd` / `env` / sandbox flags; those become views
on `ShellState::variables` for the env half. A small migration
window keeps both as separate fields with synchronisation
helpers; the duplicate is removed in §7.

`Scope` is a stack of frames; each frame is a
`BTreeMap<String, Variable>` plus an `is_function` flag. `local`
pushes; function return pops. Variable lookup walks the stack
from top to bottom.

## 6. Builtin inventory by owner

PLAN_05 §11.1 is the canonical disposition table; PLAN_11 owns
exactly the rows marked PLAN_06 there (Phase B implements the
builtins assigned to the executor in PLAN_05; the `PLAN_06`
label in PLAN_05 §11.1 predates the Phase A/B split). Reproduced
here as a checklist (no semantic content; if it disagrees with
PLAN_05 §11.1, PLAN_05 wins):

**PLAN_11 — Tier-1 builtins (38).**

`:`, `.`, `[`, `alias`, `break`, `builtin`, `cd` (extend
existing), `command`, `continue`, `declare`, `echo`, `enable`,
`eval`, `exec`, `exit` (already implemented), `export`,
`false`, `let`, `local`, `pwd`, `readonly`, `return`, `set`,
`shift`, `shopt`, `source`, `test`, `true`, `type`\* (split with
PLAN_12), `typeset`, `unalias`, `unset`.

(\*`type` is dual-owned: command-kind resolution table is
PLAN_11; the `-a` exhaustive listing uses PLAN_12's `hash`
cache and PATH search machinery.)

Each builtin lands as its own subtask once its PLAN_07 sheet is
`support`-classed. The largest by surface area are `test`,
`declare`, `set`, `shopt`, and `exec`; the smallest (`:`,
`true`, `false`) ship together.

**PLAN_12 — Tier-1 builtins (19).** Listed for cross-reference
only; implementation tracked in PLAN_12:

`bg`, `caller`, `dirs`, `disown`, `fg`, `getopts`, `hash`,
`help`, `jobs`, `kill`, `logout`, `mapfile`, `popd`, `printf`,
`pushd`, `read`, `readarray`, `suspend`, `times`, `trap`,
`ulimit`, `umask`, `wait`.

**PLAN_13 — Tier-1 builtins (2).** `fc`, `history`. Listed for
cross-reference; implementation tracked in PLAN_13 §8.6.

**Tier-2.** The Tier-2 registry and dispatcher wiring is a
Phase B deliverable (§7 subtask 13B.5). Individual Tier-2
implementations (e.g., `ls`, `cat`, `du`) are inventoried by
PLAN_07 sheets and tracked as sub-subtasks under 13B.5; they
are not enumerated here.

## 7. ADR 0004 fallback removal

`FREDSHELL_ALLOW_SH_FALLBACK=1` is removed before v1.0. The
sunset is split into two stages:

1. **Stage 1 (mid-Phase-B).** The fallback path remains but emits
   a stderr warning the first time it is hit per process:
   "fredshell: command %q did not match a native parse; falling
   back to /bin/sh -c. This fallback will be removed in v1.0.
   Set FREDSHELL_ALLOW_SH_FALLBACK=0 to make this a hard error
   today." This pressure-tests the corpus: any divergence the
   warning reveals must result in either a new spec case or a
   `refuse!` shim.
2. **Stage 2 (end of Phase B).** The fallback code path is
   deleted. `FREDSHELL_ALLOW_SH_FALLBACK` is no longer read.
   The `spawn_via_sh` helper is deleted. The exit gate for this
   stage is the threshold below.

**Phase B exit gate.** Phase B is complete (and ADR 0004
sunset stage 2 is unlocked) when all of the following are
true:

- Every PLAN_06-owned Tier-1 builtin in §6 has a
  `support`-classed PLAN_07 sheet and at least one passing
  corpus case.
- Every expansion pass (§4) has a `support`-classed PLAN_07
  sheet and ≥10 passing corpus cases.
- PLAN_08 F1 (every PR), F2 (nightly), and F3 (weekly)
  differential tiers have been green on `main` for 14
  consecutive days.
- Real-world script corpus pass rate ≥ 95% (PLAN_05 §6
  metric).

The threshold is intentionally strict: ADR 0004 promises the
fallback exists _only_ until the native pipeline is good
enough to remove it. Stage 2 lands the day the threshold is
hit; we do not run with a quietly-deprecated fallback.

## 8. Subtask grid

Subtask numbering: `06b.N` (Phase B). Format matches PLAN_06 §10 / §11.

| Subtask | Surface                                          | Gate                  |
| ------- | ------------------------------------------------ | --------------------- |
| 06b.0   | Phase B gate: PLAN_08 F1 green on `main`         | PLAN_08 08.7 complete |
| 06b.1   | ADR 0005: parser implementation choice           | 06b.0                 |
| 06b.2   | Lexer (`parser/lex/`) + tests                    | 06b.1                 |
| 06b.3   | Parser (`parser/grammar/`) + AST + tests         | 06b.2                 |
| 06b.4   | `ShellState` skeleton + scope tree               | 06b.0                 |
| 06b.5   | Tier-2 registry + dispatcher                     | 06b.4                 |
| 06b.6   | Expansion: brace                                 | 06b.3, 06b.4          |
| 06b.7   | Expansion: tilde                                 | 06b.6                 |
| 06b.8   | Expansion: parameter (incl. `${...}` operators)  | 06b.6                 |
| 06b.9   | Expansion: command substitution                  | 06b.8                 |
| 06b.10  | Expansion: arithmetic                            | 06b.8                 |
| 06b.11  | Expansion: word-split + pathname + quote-removal | 06b.6                 |
| 06b.12  | Redirection setup (incl. here-docs)              | 06b.3, 06b.4          |
| 06b.13  | Pipeline execution + process-group setup         | 06b.12                |
| 06b.14  | Control flow: `if`/`while`/`until`/`for`/`case`  | 06b.3, 06b.4          |
| 06b.15  | Function definitions + call frames               | 06b.4                 |
| 06b.16  | Trivial builtins: `:`/`true`/`false`             | 06b.4                 |
| 06b.17  | Variable builtins: `export`/`readonly`/`unset`   | 06b.4                 |
| 06b.18  | Scope builtins: `local`/`declare`/`typeset`      | 06b.4, 06b.15         |
| 06b.19  | Option builtins: `set`/`shopt`                   | 06b.4                 |
| 06b.20  | `test`/`[` (huge surface — own batch)            | 06b.4                 |
| 06b.21  | `[[ ]]` keyword (parser + executor)              | 06b.3, 06b.20         |
| 06b.22  | `(( ))` keyword + arithmetic eval                | 06b.3, 06b.10         |
| 06b.23  | Alias builtins: `alias`/`unalias`                | 06b.4                 |
| 06b.24  | Reentrant builtins: `eval`/`source`/`.`          | 06b.3, 06b.4          |
| 06b.25  | `command`/`builtin`/`type` (resolution path)     | 06b.4, 06b.5          |
| 06b.26  | `exec` (process-replace + fd manipulation)       | 06b.12                |
| 06b.27  | `enable` (toggle builtin disposition)            | 06b.4, 06b.16         |
| 06b.28  | `let` + arithmetic builtin                       | 06b.22                |
| 06b.29  | Reserved words: `time` keyword integration       | 06b.3                 |
| 06b.30  | ADR 0004 sunset stage 1 (fallback warning)       | 06b.16–06b.29         |
| 06b.31  | Real-world corpus pass-rate baseline             | 06b.30                |
| 06b.32  | Exit-gate verification + ADR 0004 sunset stage 2 | 06b.31                |

Subtasks 06b.6–06b.11 (expansion family) are sequenced as
listed because each pass consumes the previous pass's tokens.
Subtasks 06b.16–06b.29 (builtin family) are mostly
independent and can run in parallel after their gates clear;
the order above reflects priority by frequency of use in the
real-world corpus, not dependency.

PLAN_12 subtasks land in parallel with PLAN_11 Phase B; the
two plans share the §5 `ShellState` slots but otherwise
operate independently.

## 9. Open questions

- **Q06B.1** — Parser strategy: in-house vs `brush-parser` vs
  fork. **Resolved (2026-05-23):** in-house recursive-descent.
  Rationale captured in ADR 0005 (subtask 06b.1) and supported
  by the prose at §3: diagnostic quality, incremental
  parsing for the PLAN_13 highlighter (partial-line
  tolerance), lossless CST for the future formatter, and
  parse-stage / alias gating (per PLAN_08 §11 Q09.3) all
  require parser internals we are unwilling to outsource.
  ADR 0005 authoring remains subtask 06b.1; it ratifies this
  resolution rather than re-litigating it.
- **Q06B.2** — `coproc` support. Default: recognise and refuse
  in v1; defer real implementation to v1.1. **Resolved
  (2026-05-23):** v1 emits `ParseError::Unsupported { feature:
"coproc" }` per `PLAN_19_coproc.md`; full implementation is
  owned by PLAN_19 when picked up post-v1. No Phase B subtask.
- **Q06B.3** — Here-doc temp-file threshold. **Resolved
  (2026-05-23):** bodies ≤ 64 KiB are delivered via pipe;
  bodies > 64 KiB are spilled to a tempfile under `$TMPDIR`
  with `unlink`-immediately-after-open semantics so cleanup
  survives signals. The threshold is a named const
  (`HEREDOC_PIPE_MAX = 64 * 1024`) in the executor module, not
  configurable at runtime. PLAN_07 here-doc spec sheets pin
  the threshold and include at least one case at body size
  `HEREDOC_PIPE_MAX - 1` and one at `HEREDOC_PIPE_MAX + 1` so
  the boundary is regression-tested. FD-table introspection
  (`/proc/self/fd` on Linux, `/dev/fd` on macOS) is permitted
  to diverge from bash at the boundary: bash on Linux uses an
  anonymous pipe up to a similar threshold, bash on macOS
  always uses a tempfile.
- **Q06B.4** — `$RANDOM` and `$SECONDS` determinism in the
  spec harness. **Resolved (2026-05-23):** the harness pins
  both per case. Each case's `[harness]` block (PLAN_07 sheet
  schema) accepts optional `random_seed: u32` and
  `seconds_offset: u64` fields. fredshell consumes them from a
  harness-only env channel (`FREDSHELL_HARNESS_RANDOM_SEED`,
  `FREDSHELL_HARNESS_SECONDS_OFFSET`) that is never exposed
  outside the spec runner. The reference bash receives the
  same pin via `RANDOM=<seed>` injected before script
  execution and a `faketime`-style clock shim for `$SECONDS`.
  When fields are absent, the harness applies workspace
  defaults (`random_seed = 0`, `seconds_offset = 0`) so every
  case is reproducible without per-case config. PLAN_05
  status taxonomy gains no new variant; pinning is an input,
  not a status.
- **Q06B.5** — Locale-translated strings (`$"..."`).
  **Resolved (2026-05-23):** the parser accepts `$"..."` as
  syntactically valid (one CST node with the
  locale-translated marker), and the executor refuses with:

  ```text
  ExecError::Unsupported {
      feature: "locale_translation",
      suggestion: "v1 has no message-catalog loader; \
                   remove the leading `$` to use the \
                   literal string",
  }
  ```

  Refusing cleanly preserves the contract that scripts which
  depend on translations fail loudly rather than silently
  losing them. Full `gettext`-style support is post-v1 work;
  if picked up, it gets its own PLAN_XX owning document.
  PLAN_07 has a refusal-corpus case under
  `tests/spec/refusals/` (`locale_translation.case.toml`)
  asserting the refusal diagnostic text.

## 10. Relationship to other plans

- **PLAN_05** — corpus and harness. Phase B is measured by
  corpus pass-rate; the §7 exit gate references PLAN_05 §6.
- **PLAN_07** — spec sheets. Each Phase B implementation
  subtask consumes a `support`-classed sheet; no sheet, no
  implementation.
- **PLAN_08** — fuzzer + differential. 06b.0 gates the entire
  phase; F2/F3 thresholds gate ADR 0004 sunset stage 2.
- **PLAN_13** — line editor. Phase B exposes the `history`
  store via a borrowed `HistoryStore` trait object on
  `ShellState`; the `fc` and `history` builtins are dispatched
  by PLAN_11 to entry points whose semantics live in PLAN_13
  §8.6.
- **PLAN_12** — traps and jobs. Phase B reserves the §5
  slots (`traps`, `jobs`, `dirs_stack`, `umask`, `cmd_hash`)
  but does not populate them; PLAN_12 owns population.
- **PLAN_15** — milestones. The Phase B exit gate corresponds
  to the v1.0 milestone gate.
