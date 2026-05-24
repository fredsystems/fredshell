# PLAN_10 — `fredshell-core` Embeddability

> Last updated: 2026-05-24 — initial stub. Created during the
> work-order renumber to give the ADR 0006 implementation plan a
> permanent owning document.
>
> Phase: A. Status: stub (not drafted).
> Consumes: ADR 0006 (architectural decision), PLAN_02
> (workspace and crate layout).
> Consumed by: PLAN_09 (diagnostics sink contract), PLAN_11
> (parser output feeds event-emitting executor), PLAN_12
> (executor written against this contract from the start),
> PLAN_13 (job-state events), PLAN_14 (line-editor / core
> boundary), PLAN_15 (prompt-request event).

## Purpose

This document is a stub. It exists so that the implementation
work for ADR 0006 — `fredshell-core` as an embeddable library
exposing an async stream of typed events — has a single owning
plan instead of being decided per-subsystem as each one comes
online.

ADR 0006 fixes the architectural shape:

- `Shell` handle owned by the embedder.
- Async input methods (`feed_line`, `feed_bytes`).
- `events()` returning `impl Stream<Item = ShellEvent>`.
- Runtime-agnostic core (`futures-core`, not `tokio`).
- No fd writes from the core; child processes use core-owned
  pipes and surface bytes as `Output` events.
- `ShellHost` callback trait explicitly rejected.

This plan owns the implementation: the exact API, the binary's
REPL refactor, and the PTY ownership model. It does not relitigate
the shape.

## Scope

When drafted, this plan owns:

- The exact public API of `fredshell-core`: `Shell` constructor,
  `feed_*` async methods, `events()` signature, lifecycle
  (`shutdown`, drop semantics).
- The full `ShellEvent` enum: every variant, every field, and the
  back-pressure / ordering guarantees the stream provides.
- The error model returned from `feed_*` (typed enum, never
  `anyhow`).
- The decision left open by ADR 0006: single multiplexed
  `events()` stream vs. several topic-specific streams.
- The decision left open by ADR 0006: `&mut self`-borrowed
  stream (single-consumer, back-pressured) vs. owned receiver
  (multi-consumer, buffered).
- The PTY ownership model for child processes: per-session,
  per-foreground-job, or on-demand. This interacts with PLAN_13
  (job control) and PLAN_14 (the `yield_terminal` primitive).
- The line-editor / core boundary: are keystrokes fed into the
  core (`feed_bytes`) and the core hosts the line editor, or
  does the embedder run a line editor and call `feed_line` with
  completed lines? The binary uses the latter today; this plan
  decides whether that stays binary-only.
- The binary's REPL refactor: removing every fd write from the
  executor, replacing each with an event emission, and rewriting
  `fredshell::main` as the reference event consumer.
- Test infrastructure for the event stream: a `MockEmbedder`
  that drives `Shell` deterministically and asserts on event
  sequences, replacing some of the L4 PTY-harness coverage with
  ordinary unit tests.

## Out of scope

- The grammar fed into the executor (PLAN_11).
- The executor's own semantics (PLAN_12).
- The job table's internal data structure (PLAN_13).
- The line editor's key decoder and history search (PLAN_14),
  except where they cross the embedding boundary.
- The prompt renderer (PLAN_15), except for the
  `PromptRequest` event.
- Diagnostic rendering (PLAN_09 internal logging, PLAN_15 prompt
  area, PLAN_21 locale).

## Why this is Phase A

ADR 0006 is foundational: every later subsystem either writes
through the event stream or has to be reworked to do so. The
cost of designing PLAN_12 Phase B and PLAN_11 parser against a
synchronous fd-writing core is roughly twice the cost of doing
them right the first time. This plan must be drafted and its
public surface fixed before Phase B subtasks start.

## Key questions to resolve when drafted

- **Q10.1** — `events()` shape: `&mut self`-borrowed single
  stream, owned `Receiver<ShellEvent>`, or topic-split streams.
  Single borrowed stream is the simplest and matches the binary's
  needs; embedders that want fan-out can wrap it.
- **Q10.2** — Error model for `feed_*`: a single `ShellError`
  enum vs. method-specific error types. AGENTS.md prefers typed
  errors; this plan settles which granularity.
- **Q10.3** — PTY ownership: per-session (one PTY allocated when
  `Shell` is constructed and reused) vs. per-foreground-job
  (allocate-and-free around each foreground command) vs.
  on-demand (allocate only when a child requests a TTY). Each
  has trade-offs against PLAN_13 (job control), PLAN_14
  (line-editor handoff), and PLAN_20 (coproc).
- **Q10.4** — Line-editor boundary: keep the line editor in the
  binary (today's shape) or move it into the core behind a
  feature flag so embedders without a TTY can still get
  completion and history. This question is decided here, not in
  PLAN_14.
- **Q10.5** — Async runtime in tests: the binary uses `tokio`;
  the core must be testable without it. Confirm the test
  strategy (probably `futures::executor::block_on` for unit
  tests, `tokio` for integration tests).
- **Q10.6** — `ShellEvent` SemVer policy: the enum is
  `#[non_exhaustive]` from day one, so adding variants is not a
  breaking change. Ratify and document.
- **Q10.7** — Diagnostics sink: ADR 0006 says diagnostics are
  events. PLAN_09 says internal logging is `tracing`. Confirm
  the boundary: is a `ShellEvent::Diagnostic` also recorded as a
  `tracing` event, or are the two channels fully independent?

## When this document is drafted

This stub is upgraded to a real plan before PLAN_12 Phase B
subtasks start. At that point the drafter:

- Adds a real `## N. <section>` body covering the API,
  `ShellEvent`, the PTY model, the line-editor boundary, the
  binary's REPL refactor, and the test harness.
- Files an entry in `plan.md`'s table flipping this row from
  "stub pending" to "drafted".
- Adds the corresponding subtask grid (numbering `10.N`).
- Updates AGENTS.md's crate-table row for `fredshell-core` to
  note the embedding contract and the runtime-agnostic rule.
- Coordinates with PLAN_11 and PLAN_12 to confirm their early
  subtasks consume this contract.

## Relationship to other plans

- **ADR 0006** — the architectural decision this plan
  implements.
- **PLAN_02** — workspace and crate layout; this plan refines
  `fredshell-core`'s public surface within that layout.
- **PLAN_09** — diagnostics-as-events vs. internal logging
  boundary.
- **PLAN_11** — parser whose output feeds the executor that emits
  events.
- **PLAN_12** — first executor written against this contract
  from the start; Phase B subtasks block on this plan being
  drafted.
- **PLAN_13** — job-state events and PTY ownership for
  foreground / background jobs.
- **PLAN_14** — line-editor boundary, `yield_terminal`
  primitive.
- **PLAN_15** — `PromptRequest` event shape.
- **PLAN_20** — `coproc` interacts with the PTY ownership model
  decided here.
