# PLAN_21 — Internationalisation (`gettext`-style catalogues)

> Last updated: 2026-05-24 — initial stub. Created during the
> work-order renumber to give i18n strategy a permanent owning
> document.
>
> Phase: A. Status: stub (not drafted).
> Consumes: PLAN_09 (logging boundary — internal logs are NOT
> translated), PLAN_10 (diagnostics-as-events contract), PLAN_16
> (config layering for locale negotiation).
> Consumed by: every subsystem that emits user-facing text:
> PLAN_12 (executor diagnostics), PLAN_14 (line-editor prompts),
> PLAN_15 (prompt renderer), PLAN_18 (AI-facing prompts).

## Purpose

This document is a stub. It exists so that fredshell's i18n
strategy has a single owning plan instead of being decided per
subsystem when each one starts emitting user-facing text.

The headline rule:

- **User-facing text is translated.** Diagnostics, error
  explanations, prompts, AI-facing system prompts, help text.
- **Developer-facing text is not translated.** `tracing` log
  messages, `ParseError::Unsupported { feature }` tokens, panic
  messages (if any survive review), spec-runner output.
- **Identifiers and stable tokens are not translated.** The
  `feature` string in refusal errors is a stable grep target;
  the user-readable explanation that accompanies it is
  translated.

## Scope

When drafted, this plan owns:

- The catalogue format: `gettext` `.po` / `.mo` files are the
  working assumption because the tooling and translator
  ecosystem already exists. Alternatives (`fluent`, custom
  TOML) are considered.
- The Rust binding: `gettext-rs` (binding to system libintl) vs.
  `gettext-utils` (pure-Rust) vs. a custom minimal loader. Trade-
  offs are deployment surface (libintl dependency) vs. catalogue
  format fidelity vs. binary size.
- The string-marking macro: `t!("message")` or `tr!(...)` or
  similar. Must be greppable, must support plurals, must support
  positional arguments without losing translator context.
- Locale negotiation: `LC_ALL` > `LC_MESSAGES` > `LANG` >
  fallback chain. fredshell-specific overrides via config
  (PLAN_16).
- Catalogue compilation: where `.po` files live in the repo,
  how they are compiled to `.mo`, where the compiled artifacts
  ship in the Nix build (PLAN_17).
- The translator workflow: how new strings are extracted, how
  translators receive context, how stale strings are pruned.
- Test methodology: at least one non-English locale must be
  exercised in CI to catch missing translations and
  bad-format-string errors before release.
- The boundary with `ShellEvent::Diagnostic` (PLAN_10): does the
  core emit the message-id and let the embedder translate, or
  does the core emit the translated string? Each has
  consequences for embedders that want their own translations
  (freminal) and for testing.

## Out of scope

- Internal `tracing` logs (PLAN_09 — explicitly not
  translated).
- Stable refusal tokens (PLAN_11 — the `feature` string is a
  grep target, not user-facing prose).
- Source-level Unicode handling (PLAN_03 — that is encoding,
  not localisation).
- The prompt's own rendering (PLAN_15 — but the strings the
  prompt embeds, if any, flow through this plan's catalogue).

## Why this is Phase A

The string-marking convention must be in place before
diagnostics are written, or every diagnostic site has to be
revisited later. Retrofitting i18n is one of the most expensive
late-stage changes a project can do; making it cheap up front
costs a macro and a marking discipline.

This plan is Phase A and may be drafted in parallel with PLAN_09
and PLAN_10. It is consumed by every later subsystem that emits
user-facing text.

## Key questions to resolve when drafted

- **Q21.1** — Catalogue format: `gettext` `.po` (familiar tools,
  C-style format strings) vs. `fluent` (Mozilla, designed for
  modern UX, native plural / gender support, smaller tooling
  ecosystem) vs. custom TOML (trivial to implement, no
  translator tooling). Working assumption: `gettext`. Ratify.
- **Q21.2** — Binding: `gettext-rs` (system libintl, runtime
  dependency) vs. pure-Rust loader (no system dependency, must
  reimplement parts of libintl). Nix-friendly answer is the
  pure-Rust loader; confirm.
- **Q21.3** — Diagnostic translation boundary: core emits
  translated strings (simpler for the binary, harder for
  embedders that want their own translations) vs. core emits
  message-id + arguments and the embedder translates (cleaner
  per ADR 0006 but more work). Likely the latter.
- **Q21.4** — Default locale on failure: bash falls back to C
  locale silently. Do we match, warn, or refuse? Refusing
  breaks scripts; matching is the safest default.
- **Q21.5** — Pluralisation: `gettext` plural-forms vs. `fluent`
  CLDR rules. Decided alongside Q21.1.
- **Q21.6** — Test locales: which non-default locale do we run
  in CI? A locale with substantially different plural rules
  (Polish, Russian) catches more bugs than a Western European
  locale.
- **Q21.7** — AI features (PLAN_18) need locale-aware system
  prompts: how do these flow through the catalogue, and does
  the provider receive the user's locale as context?
- **Q21.8** — Help text: `--help` output, builtin help, error
  explanations. Are these in the same catalogue, or split
  (long-form help in a separate catalogue to keep the diagnostic
  catalogue small)?

## When this document is drafted

This stub is upgraded to a real plan before PLAN_12 Phase B
subtasks start emitting diagnostics, because every diagnostic
site is a string-marking site. At that point the drafter:

- Adds a real `## N. <section>` body covering format choice,
  binding, marking macro, locale negotiation, catalogue
  workflow, and test methodology.
- Files an entry in `plan.md`'s table flipping this row from
  "stub pending" to "drafted".
- Adds the corresponding subtask grid (numbering `21.N`).
- Coordinates with PLAN_10 on the diagnostic-translation
  boundary (Q21.3) before PLAN_12 Phase B starts.
- Coordinates with PLAN_16 on locale config layering.
- Coordinates with PLAN_17 on catalogue compilation in the
  Nix build.

## Relationship to other plans

- **PLAN_09** — internal logs are explicitly not translated;
  this plan ratifies the boundary.
- **PLAN_10** — diagnostics-as-events contract decides whether
  the core emits message-ids or translated strings (Q21.3).
- **PLAN_11** — `ParseError::Unsupported { feature }` tokens
  are stable grep targets and NOT translated; the accompanying
  human-readable explanation IS.
- **PLAN_12** — first major consumer (executor diagnostics).
- **PLAN_14** — line-editor prompts and history-search prompts.
- **PLAN_15** — prompt renderer; any embedded strings flow
  through this catalogue.
- **PLAN_16** — locale config layering.
- **PLAN_17** — catalogue compilation in the Nix build.
- **PLAN_18** — AI-facing system prompts are locale-aware.
