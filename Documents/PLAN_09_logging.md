# PLAN_09 — Structured Logging

> Last updated: 2026-05-24 — initial stub. Created during the
> work-order renumber to give structured logging a permanent
> owning document.
>
> Phase: A. Status: stub (not drafted).
> Consumes: nothing.
> Consumed by: PLAN_02 (workspace conventions), PLAN_10
> (diagnostics as events vs. tracing spans), PLAN_12 (executor
> instrumentation), PLAN_14 (line-editor instrumentation), every
> later subsystem that emits diagnostics.

## Purpose

This document is a stub. It exists so that fredshell's logging
strategy has a single owning plan instead of being decided
ad-hoc per crate.

Logging in fredshell is split into two concerns that must not be
conflated:

1. **Diagnostics** — user-facing messages about shell behaviour
   (syntax errors, command-not-found, refusal of unsupported
   constructs). These are part of the embedding contract per
   ADR 0006 and surface as `ShellEvent::Diagnostic`, not as log
   records.
2. **Internal logging** — developer-facing structured records
   about what the implementation is doing (parser state, executor
   dispatch, builtin entry/exit, line-editor key handling). These
   are owned by this plan.

The two are distinct: an embedder may want diagnostics in its UI
and internal logging in a file, or vice versa, or neither.

## Scope

When drafted, this plan owns:

- The logging crate choice (`tracing` + `tracing-subscriber` is
  the working assumption; this plan ratifies or rejects it).
- The ban on `println!`, `eprintln!`, and `dbg!` in all library
  crates and the `fredshell` binary outside of explicit
  user-output paths.
- Log-level conventions (what `trace` / `debug` / `info` /
  `warn` / `error` mean in fredshell terms).
- Target conventions (per-crate target naming so subscribers can
  filter cleanly).
- Span conventions for the executor and parser (one span per
  command pipeline, one per parse, etc.).
- The diagnostics sink contract — how the binary's REPL wires
  `ShellEvent::Diagnostic` to a renderer that may also write to
  the `tracing` subscriber for capture.
- Test-time logging capture (so `cargo test` output stays clean
  by default but a flag turns on full traces for debugging).
- The interaction with `RUST_LOG` / `FREDSHELL_LOG` env vars and
  any rc-file config (coordinated with PLAN_16).

## Out of scope

- Diagnostics rendering (owned by PLAN_10 + PLAN_15).
- Locale-aware diagnostic strings (owned by PLAN_21).
- The fuzzer's structured output (owned by PLAN_08).

## Why this is Phase A

Logging conventions must be in place before PLAN_12's Phase B
executor starts emitting instrumentation. Retrofitting `tracing`
across an executor that already uses `println!` for debug output
costs more than starting clean. The diagnostics-as-events
contract from ADR 0006 also means every `eprintln!` in the
current scaffold is a correctness bug that must be removed before
the embedding surface is exposed.

## Key questions to resolve when drafted

- **Q09.1** — `tracing` vs. `log`. `tracing` is the working
  assumption because it supports spans and structured fields,
  both of which the executor will want. Confirm or reject.
- **Q09.2** — Subscriber ownership: does the binary install the
  subscriber, or does `fredshell-core` expose a default-subscriber
  helper for embedders that do not have one? ADR 0006 says the
  core does not own I/O; this plan must square that with the
  reality that `tracing` subscribers are I/O.
- **Q09.3** — Test capture: `tracing-test` vs. a custom
  subscriber. The spec runner (PLAN_05) needs clean stdout, so
  test-time logs must go somewhere they do not pollute golden
  comparisons.
- **Q09.4** — Performance budget: prompt rendering (PLAN_15) has
  a tight latency budget. Confirm that `tracing` at `info` level
  with no enabled subscriber is genuinely zero-cost.
- **Q09.5** — `FREDSHELL_LOG` syntax: match `RUST_LOG` exactly or
  diverge for user-friendliness? Default to matching unless a
  concrete reason emerges.

## When this document is drafted

This stub is upgraded to a real plan before PLAN_12 Phase B
subtasks start, because Phase B is the first major instrumentation
consumer. At that point the drafter:

- Adds a real `## N. <section>` body covering crate choice,
  conventions, the diagnostics sink contract, and test-time
  capture.
- Files an entry in `plan.md`'s table flipping this row from
  "stub pending" to "drafted".
- Adds the corresponding subtask grid (numbering `09.N`).
- Adds the `#![deny]` lints to AGENTS.md or to crate roots that
  enforce the `println!` / `eprintln!` ban (coordinate with
  AGENTS.md's "Panic-Free Production Code" section).

## Relationship to other plans

- **PLAN_02** — workspace lints and conventions; the
  `println!` / `eprintln!` ban lands here.
- **PLAN_10** — embedding contract; diagnostics-as-events is
  decided there, internal logging here, and the two must not
  drift.
- **PLAN_12** — first major instrumentation consumer (executor
  spans).
- **PLAN_14** — line-editor instrumentation (key decoder, history
  search timing).
- **PLAN_15** — prompt latency measurement uses spans defined
  here.
- **PLAN_16** — config: `FREDSHELL_LOG` env var and any rc-file
  knob land in the config layering.
- **PLAN_21** — gettext: log messages are developer-facing and
  are NOT translated; this plan ratifies that boundary.
