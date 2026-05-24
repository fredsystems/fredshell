# PLAN_11 — Native Bash Parser (`fredshell-parser`)

> Last updated: 2026-05-24 — initial stub. Created during the
> work-order renumber to give the native parser its own owning
> document, absorbing subtasks 06b.1 (parser-strategy ADR) and
> 06b.2 (lexer/parser scaffold) from the previous PLAN_06b /
> PLAN_12 grouping.
>
> Phase: B. Status: stub (not drafted; corpus-dependent).
> Consumes: PLAN_05 (test methodology), PLAN_08 (differential
> oracle), PLAN_10 (event-emitting core contract), corpus
> evidence from real-world scripts.
> Consumed by: PLAN_12 (executor consumes the AST), PLAN_13
> (trap / job syntax), PLAN_20 (coproc grammar refusal in v1),
> every later subsystem that touches scripts.

## Purpose

This document is a stub. It exists so that fredshell's bash
parser has a single owning plan instead of being scattered across
PLAN_06b subtask grids and ADR drafts.

Bash compatibility is the headline goal of the v1 product. The
parser is the surface where that goal is either met or quietly
abandoned — `bash -c` works for a while, but the cost of every
later feature scales with the cost of round-tripping through an
external shell. The parser is the foundation of every other Phase
B subsystem.

This plan owns:

- The grammar fredshell targets (a documented subset of bash 5.3
  POSIX plus bash extensions, with explicit refusal for the
  rest).
- The lexer / parser architecture.
- The AST / CST shape consumed by the executor.
- The refusal taxonomy: what fredshell rejects, what it warns
  about, what it silently accepts as bash-compatible.
- Error recovery sufficient for incremental parsing (line editor
  needs partial-input feedback per keystroke).

## Scope

When drafted, this plan owns:

- The parser-strategy ADR (ADR 0005, reserved): hand-written
  recursive descent vs. `lalrpop` / `pest` / `tree-sitter`.
  The working assumption is hand-written recursive descent,
  because bash's grammar is context-sensitive in ways generators
  handle poorly (here-docs, command substitution, `case`
  patterns, alias expansion). ADR 0005 ratifies or rejects this.
- The lexer: tokenisation rules, here-doc handling, quoting
  rules (single, double, ANSI-C, locale-prefixed), command
  substitution boundaries, arithmetic-context vs. command-context
  switching.
- The AST: typed nodes for every construct fredshell supports,
  with explicit refusal nodes for known-unsupported constructs.
- Error recovery: how the parser handles partial input from the
  line editor (PLAN_14) without producing spurious cascading
  errors.
- Source spans: every AST node carries a span suitable for
  diagnostic rendering via `ShellEvent::Diagnostic` (PLAN_10).
- Test methodology: golden-file snapshots (PLAN_05 spec runner),
  differential oracle against bash (PLAN_08 fuzzer), and unit
  tests for grammar productions.
- The refusal vocabulary: every unsupported construct produces
  a `ParseError::Unsupported { feature, suggestion }` whose
  `feature` string is stable enough for users to grep for.
- The `fredshell-parser` crate's public surface: what the
  executor consumes, what the line editor consumes for
  highlighting, what tests consume.

## Out of scope

- Variable expansion, command substitution, arithmetic
  evaluation as runtime semantics (PLAN_12 executor; the parser
  produces the structure, the executor evaluates it).
- Expansion semantics in general (PLAN_12).
- The line editor's highlighting renderer (PLAN_14); this plan
  ships the typed AST, that plan renders it.
- Locale-aware error messages (PLAN_21).

## Why this is Phase B

The parser is corpus-dependent because the grammar fredshell
targets is defined by what real scripts actually use, not by the
bash manual. Drafting before corpus evidence accumulates risks
designing for theoretical bash rather than empirical bash. The
fuzzer (PLAN_08) and spec corpus (PLAN_05) together produce that
evidence; this plan starts drafting when both have enough output
to ground the grammar decisions.

This plan is also blocked on PLAN_10 being drafted: the parser's
error path emits diagnostics that flow through the event stream,
and the parser is reentrant across `feed_line` / `feed_bytes`
calls. Both shapes depend on PLAN_10's decisions.

## Key questions to resolve when drafted

- **Q11.1** — Parser strategy (ADR 0005 content): hand-written
  recursive descent vs. parser generator vs. `tree-sitter`.
  Working assumption: hand-written. Ratify.
- **Q11.2** — Alias expansion: bash expands aliases at parse
  time, before tokenisation completes, with rules that interact
  with quoting and command position. Do we match exactly,
  approximate, or refuse? Corpus-dependent.
- **Q11.3** — Here-doc handling: bash's here-doc lexing is a
  one-pass-with-deferred-body trick. Document the implementation
  approach.
- **Q11.4** — Incremental parsing for the line editor: does the
  line editor call the full parser on every keystroke and get a
  partial AST plus errors, or does it use a separate
  highlight-only lexer? Affects PLAN_14.
- **Q11.5** — AST stability: is the AST a public type
  (`fredshell-parser` re-exports it) or internal to the
  executor? Affects line-editor highlighting and any future
  static-analysis tooling.
- **Q11.6** — Refusal granularity: every refused construct gets
  a stable `feature` token in `ParseError::Unsupported`. List
  the initial vocabulary so users can grep diagnostics
  consistently (`coproc`, `select`, `let`, `time` pipeline,
  `[[ ... =~ ... ]]`, ...).
- **Q11.7** — Source-span representation: byte offsets,
  line/column pairs, or both. Diagnostics rendering favours
  line/column; incremental parsing favours byte offsets. Pick
  one canonical form and provide a cheap conversion.
- **Q11.8** — Unicode in identifiers and operators: bash is
  byte-oriented. fredshell's source handling is UTF-8 (PLAN_03).
  Confirm the boundary: bytes through the lexer, UTF-8 in
  diagnostic strings.

## When this document is drafted

This stub is upgraded to a real plan when:

1. PLAN_08 has produced enough corpus differential data to
   identify which grammar productions matter in real scripts.
2. PLAN_10 has been drafted (parser error path needs the event
   stream contract).
3. PLAN_12 Phase B is ready to start consuming a real AST.

At that point the drafter:

- Lands ADR 0005 (parser strategy).
- Adds a real `## N. <section>` body covering lexer,
  parser, AST, error recovery, and test methodology.
- Files an entry in `plan.md`'s table flipping this row from
  "stub pending" to "drafted".
- Adds the corresponding subtask grid (numbering `11.N`,
  preserving any `06b.1` / `06b.2` cross-references as
  historical IDs in the renumber-log entry).
- Adds a `fredshell-parser` crate-table row to AGENTS.md
  (coordinate with PLAN_02).
- Coordinates with PLAN_12 so the AST shape matches what the
  executor needs and with PLAN_14 so the incremental-parsing
  contract matches the line editor's needs.

## Relationship to other plans

- **PLAN_02** — adds the `fredshell-parser` crate to the
  workspace.
- **PLAN_05** — golden-file snapshots for parser output.
- **PLAN_08** — differential oracle against bash drives grammar
  decisions.
- **PLAN_10** — diagnostics-as-events contract for parser
  errors.
- **PLAN_12** — executor consumes the AST; this plan ships, that
  one evaluates.
- **PLAN_13** — trap syntax and job-control syntax are parsed
  here.
- **PLAN_14** — line editor consumes partial parser output for
  highlighting and incremental error display.
- **PLAN_20** — `coproc` grammar refusal in v1 lives in this
  plan.
- **PLAN_21** — diagnostic message catalogues are locale-aware;
  the `feature` token in `ParseError::Unsupported` is NOT
  translated (it is a stable grep target).
