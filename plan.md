# fredshell — Master Plan

> Last updated: 2026-05-24 — Cascade renumber to insert
> `PLAN_10_embedding.md` (new) into the work-order queue, per
> ADR 0006. Mapping applied: old 10 (parser) → 11; old 11
> (exec_phase_b) → 12; old 12 (traps_and_jobs) → 13; old 13
> (line_editor) → 14; old 14 (prompt) → 15; old 15 (config) →
> 16; old 16 (nix) → 17; old 17 (ai) → 18; old 18 (milestones)
> → 19; old 19 (coproc) → 20; old 20 (gettext) → 21. PLAN_01–09
> unchanged. New PLAN_10 = embedding (host contract from ADR
> 0006). "Renumbered from old PLAN_NN" annotations dropped from
> the planning table; cascade history lives only in this
> renumber log. Subtask IDs unchanged within each renamed doc.
> Historical Q-IDs preserved.
>
> Earlier on 2026-05-24: Work-order renumber of all PLAN
> documents to reflect actual execution sequence. Mapping
> applied: 06→06 (Phase A only); 07 spec_drafting (was 08);
> 08 fuzzer (was 09); 09 logging (NEW stub); 10 parser (NEW
> stub, absorbs old 06b.1 + 06b.2); 11 exec_phase_b (extracted
> from PLAN_06 §13 / temporary PLAN_06b); 12 traps_and_jobs
> (was 10); 13 line_editor (was 07); 14 prompt (was 11);
> 15 config (was 12); 16 nix (was 13); 17 ai (was 14);
> 18 milestones (was 15); 19 coproc (was 16); 20 gettext
> (NEW stub). PLAN_01–05 unchanged. Subtask IDs in PLAN_08
> renumbered 09.N → 08.N; PLAN_11 retains `06b.N` (continuity
> with `task-06b/` branches); PLAN_12 renumbered 10.N → 12.N.
> Historical Q-IDs (Q06B.N, Q09.N, Q10.N) preserved across docs
> as immutable cross-references to `QUESTIONS_for_review.md`.
>
> Earlier on 2026-05-23: Full QUESTIONS_for_review.md
> question-walk complete: all 18 open questions resolved
> across Q-10-A..D, Q-08-A..D, Q-09-1..5, Q-06B-1..5.
> PLAN_19 stub added as the permanent owning document for
> `coproc` (resolves Q-10-D / Q-06B-2); PLAN_12 §5.1
> dispatch-asymmetry note (Q-10-B) and §6 notification-dispatch
> routing through `yield_terminal` (Q-10-C / Q10.5) landed.
> PLAN_07 §2.2 added `utf8_locale` feature category (~80 total
> sheets) covering UTF-8 correctness in v1; PLAN_18 row notes
> pending M-15-utf8-fuzz milestone scheduled between v1.0
> and v1.1.
>
> Earlier on 2026-05-22: PLAN_11 (Phase B; then PLAN_06 §13)
> drafted: lexer/parser, executor pipeline, `ShellState`,
> builtin inventory by owner, ADR 0004 two-stage sunset, 33-row
> subtask grid, five open questions Q06B.1–5. PLAN_13 scope
> augmented: owns `history`/`fc` builtins, `yield_terminal`
> primitive (answers PLAN_12 Q10.5), and L4 PTY harness.
> PLAN_08 drafted (grammar-aware fuzzer + differential oracle,
> B-phase, gates PLAN_11 Phase B via 06b.0).
>
> Earlier on 2026-05-21: restructured PLAN numbering: PLAN_06a/06b
> collapsed into PLAN_06 (exec); old PLAN_13 narrowed to line editor;
> PLAN_07/09/10 introduced for spec drafting, fuzzer/differential, and
> traps+jobs; prompt/config/nix/ai/milestones shifted to 11/12/13/14/15.
> ADR 0004 (strict-default execution) added.

This is the top-level index of fredshell's planning documents. Read this first.
The actual design lives in the per-area `PLAN_XX_*.md` documents and the ADRs in
`Documents/decisions/`.

## What fredshell is

fredshell is an opinionated, batteries-included Rust shell intended as a daily-driver
replacement for zsh on Linux and macOS. It is **not** a POSIX-only shell, **not** a
"better bash," and **not** a structured-data shell in the nushell sense.

It aims to be:

- **Bash-script compatible.** Real-world bash scripts must run. Compatibility is
  pursued via a native parser and in-process executor, not by shelling out to
  `/bin/sh`.
- **Built-ins first.** Common interactive commands (`ls`, `cat`, `du`, `df`,
  `which`, plus all POSIX shell builtins) execute in-process. Fork/exec is reserved
  for genuinely external programs.
- **Pleasant by default.** Starship-style prompt, fzf-style fuzzy history and
  completion, lsd-style `ls`, syntax highlighting, sensible keybindings — all
  baked in. No `~/.zshrc` archaeology required.
- **Nix-native.** First-class home-manager module; the shell, its config, and its
  plugins are managed declaratively.
- **AI-augmented, optionally.** Natural-language-to-command, error explanation, and
  command suggestion are available behind explicit opt-in with clear privacy
  boundaries.

It explicitly is not:

- A general-purpose programming language.
- A POSIX-conformant `/bin/sh`.
- A drop-in replacement for fish's syntax (we follow bash).
- A Windows shell.

See `Documents/PLAN_01_philosophy.md` for the full philosophy and non-goals.

## Architectural priorities (read first)

Foundational decisions shape everything else and are recorded as ADRs:

- **ADR 0001 — In-process execution and the builtin tier model.** fredshell does
  not shell out to `/bin/sh`. Common externals are replaced by in-process
  builtins in a tiered model. See
  `Documents/decisions/0001-in-process-execution-and-builtin-tiers.md`.
- **ADR 0002 — ANSI encoding crate strategy.** fredshell ships its own
  encoder-focused `fredshell-ansi` crate rather than sharing freminal's decode-
  oriented escape-sequence types. Convergence is acknowledged as a future option.
  See `Documents/decisions/0002-ansi-encoding-crate-strategy.md`.
- **ADR 0003 — Test-first compatibility methodology.** Compatibility is
  defined by an executable spec corpus, not prose. The harness lands before
  the implementation and runs in CI from day one. Planning splits into
  Phase A (corpus-independent docs) and Phase B (corpus-dependent docs).
  See `Documents/decisions/0003-test-first-compatibility-methodology.md`.
- **ADR 0004 — Strict-default execution.** `ExecEnv` defaults to refusing
  to shell out to `/bin/sh -c` in both the binary REPL and the spec
  harness. `FREDSHELL_ALLOW_SH_FALLBACK=1` is a temporary escape hatch,
  removed before v1.0. See
  `Documents/decisions/0004-strict-default-execution.md`.

## Planning documents

| #   | Document                               | Phase   | Status        | Summary                                                                                                                                                                                                                 |
| --- | -------------------------------------- | ------- | ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 01  | `Documents/PLAN_01_philosophy.md`      | A       | draft         | Goals, non-goals, target user, success criteria.                                                                                                                                                                        |
| 02  | `Documents/PLAN_02_architecture.md`    | A       | draft         | Crate layout, module boundaries, key traits, dependency direction.                                                                                                                                                      |
| 03  | `Documents/PLAN_03_ansi.md`            | A       | implemented   | `fredshell-ansi` crate: encoder API, minimal decoder, `Write`-based contract, allocation budget.                                                                                                                        |
| 04  | `Documents/PLAN_04_terminal_io.md`     | A       | implemented   | Raw mode discipline, signals, process groups, terminal feature detection, kitty keyboard negotiation.                                                                                                                   |
| 05  | `Documents/PLAN_05_testing.md`         | A       | implemented   | Spec-test harness, corpus methodology, oils-spec integration, real-world script corpus, CI metrics.                                                                                                                     |
| 06  | `Documents/PLAN_06_exec.md`            | A       | implemented   | Execution pipeline, Phase A scaffold: public surface, dispatcher, `ExecEnv`, REPL/spec wiring. Phase B content extracted to PLAN_12.                                                                                    |
| 07  | `Documents/PLAN_07_spec_drafting.md`   | A       | drafted       | Spec sheet template (one per builtin + feature), batch-of-10 review cadence, lint extensions tying `support` rows to corpus cases.                                                                                      |
| 08  | `Documents/PLAN_08_fuzzer.md`          | B       | drafted       | Grammar-aware deterministic fuzzer + differential oracle against pinned bash 5.3p9. Five tiers (F1 PR → F5 release gate). Gates PLAN_12 Phase B via 06b.0 = "F1 green on main."                                         |
| 09  | `Documents/PLAN_09_logging.md`         | A       | stub          | Structured logging via `tracing` + `tracing-subscriber`. Bans `println!`/`eprintln!`/`dbg!` in library crates and the `fredshell` binary. Defines log levels, target conventions, and the diagnostics sink contract.    |
| 10  | `Documents/PLAN_10_embedding.md`       | A       | stub          | `fredshell-core` embeddability contract per ADR 0006: `Shell` handle, async `Stream` of typed `ShellEvent`s, runtime-agnostic core, no fd writes from core. Owns PTY ownership model and the binary REPL refactor.      |
| 11  | `Documents/PLAN_11_parser.md`          | B       | stub          | In-house recursive-descent bash parser (`fredshell-parser`). Owns lexer, AST/CST, error recovery for incremental parsing, refusal taxonomy. Absorbs old 06b.1 (ADR 0005) and 06b.2 (lexer/parser scaffold).             |
| 12  | `Documents/PLAN_12_exec_phase_b.md`    | B       | drafted       | Execution pipeline Phase B: real semantics, `ShellState`, Tier-1 builtins, expansion family, ADR 0004 sunset. Extracted from PLAN_06 §13 / temporary PLAN_06b. Subtask grid retains `06b.N` for `task-06b/` continuity. |
| 13  | `Documents/PLAN_13_traps_and_jobs.md`  | B       | drafted       | Signal traps, job control, `wait`, `kill`, foreground/background. Corpus-dependent because trap semantics differ from POSIX in well-defined ways.                                                                       |
| 14  | `Documents/PLAN_14_line_editor.md`     | A       | drafted       | Line editor: key-byte decoder, history, completion, fuzzy search, keybindings, syntax highlighting. Owns `history`/`fc` builtins, the `yield_terminal` primitive consumed by PLAN_13, and the L4 PTY harness.           |
| 15  | `Documents/PLAN_15_prompt.md`          | A       | draft         | Starship-style prompt renderer, configuration model, performance budget.                                                                                                                                                |
| 16  | `Documents/PLAN_16_config.md`          | A       | draft pending | Config file format, layering, env vars, rc-file semantics.                                                                                                                                                              |
| 17  | `Documents/PLAN_17_nix_integration.md` | A       | draft pending | Home-manager module surface, flake outputs, default-shell story.                                                                                                                                                        |
| 18  | `Documents/PLAN_18_ai_features.md`     | A       | draft pending | NL→command, error explanation, provider abstraction, privacy boundaries.                                                                                                                                                |
| 19  | `Documents/PLAN_19_milestones.md`      | B       | stub pending  | Phased roadmap: MVP → daily-driver → bash-replacement. Corpus-dependent. Pending entries: **M-15-utf8-fuzz** — UTF-8 differential-fuzz tier (`F2-utf8`) scheduled between v1.0 and v1.1 per PLAN_08 §11 Q09.5.          |
| 20  | `Documents/PLAN_20_coproc.md`          | post-v1 | stub          | Coprocesses (`coproc`). Cuts across PLAN_11 parser, PLAN_12 executor, PLAN_13 jobs, PLAN_02 variables. v1 recognises and refuses; full implementation deferred.                                                         |
| 21  | `Documents/PLAN_21_gettext.md`         | A       | stub          | Internationalisation strategy: `gettext`-style message catalogues for diagnostic messages, error explanations, and AI-facing prompts. Locale negotiation, fallback chain, catalogue compilation in build.               |

### Two-phase planning

Planning proceeds in two phases:

- **Phase A** drafts the docs whose content does not depend on knowing what
  bash scripts in the wild actually do. These are the architecture-shaped
  docs: testing methodology (PLAN_05), crate layout (PLAN_02), foundational
  subsystems (PLAN_03, PLAN_04), spec-drafting workflow (PLAN_07),
  logging (PLAN_09), embedding contract (PLAN_10), line editor (PLAN_14),
  prompt (PLAN_15), config (PLAN_16), nix integration (PLAN_17),
  AI features (PLAN_18), and gettext (PLAN_21). PLAN_05 (testing) is
  intentionally drafted before PLAN_02 (architecture) because the
  spec-test harness imposes hard constraints on the architecture
  (parser separable from executor, sandboxable execution environment,
  clean batch-mode entry point).

- **Phase B** drafts the docs whose content is informed by the spec corpus
  once it exists: the fuzzer/differential (PLAN_08), the parser (PLAN_11),
  Phase B of the executor (PLAN_12), traps+jobs (PLAN_13), and the
  implementation roadmap (PLAN_19). These carry "stub pending" status
  during Phase A and receive full drafts only after the v1 corpus has
  been curated and the harness reports a baseline pass-rate. PLAN_06
  (Phase A executor scaffold) is the foundation those Phase B docs build on.

The rationale and methodology are pinned in ADR 0003.

## Architecture Decision Records

| ID   | Title                                           | Status   |
| ---- | ----------------------------------------------- | -------- |
| 0001 | In-process execution and the builtin tier model | accepted |
| 0002 | ANSI encoding crate strategy                    | accepted |
| 0003 | Test-first compatibility methodology            | accepted |
| 0004 | Strict-default execution                        | accepted |
| 0006 | `fredshell-core` embeddable library contract    | accepted |

New ADRs are added as significant design questions are resolved. ADRs are
immutable once accepted; superseding decisions get new ADRs that link back.

## How this plan is executed

- Each planning document gets its own commit (or small set of commits) on a
  topic branch, reviewed iteratively.
- Implementation work for each plan section happens on its own task branch
  (`task-NN/<topic>`), per `AGENTS.md`.
- `plan.md` is updated whenever a planning document changes status (draft →
  accepted → implementing → done) or a new document/ADR is added.

## Open questions

These are unresolved as of this draft and will be addressed by the relevant
planning document or ADR:

- Choice of native bash parser: adopt `brush-parser`, fork, or write our own.
  **Resolved (2026-05-23):** in-house recursive-descent. ADR 0005 to be
  authored in PLAN_12 subtask 06b.1; implementation owned by `PLAN_11`
  (parser).
- Line-editor library: build on `reedline`/`rustyline`, or roll our own on top
  of `crossterm`/`termwiz` (deferred to `PLAN_14`).
- Async runtime: required for AI features and background jobs, optional
  elsewhere — scope to be decided in `PLAN_02` and `PLAN_18`.
- Plugin/extension model: out of scope for v1, but the architecture must not
  preclude it.

## Cleanup items

Numbered cleanup entries for bugs surfaced during planning work that fall
outside the surfacing subtask's scope. Each entry stays here until resolved
or migrated to its owning PLAN doc.

### CL-01 — Stale "(config)" and "(AI segments)" cross-references in PLAN_14 / PLAN_15

- **Surface point.** Cascade-renumber commit on branch
  `docs/work-order-renumber` (the commit that lands the PLAN_10
  embedding insertion and cascades old PLAN_10..20 → new
  PLAN_11..21).
- **Impact.** Two planning documents contain cross-references that point
  to the wrong PLAN by name:
  - `Documents/PLAN_14_line_editor.md` — top-blockquote "Consumed by"
    line ends with `PLAN_13 (config)`. Config is PLAN_16
    post-cascade; the line should read `PLAN_16 (config)`. The bug
    predates the cascade (pre-cascade said `PLAN_12 (config)`, also
    wrong — config was PLAN_15 at that time).
  - `Documents/PLAN_15_prompt.md` — top-blockquote "Consumed by" line
    ends with `PLAN_13 (config), PLAN_15 (AI segments, optional)`.
    Config is PLAN_16; AI is PLAN_18. The line should read
    `PLAN_16 (config), PLAN_18 (AI segments, optional)`. Body prose
    (§3.2, §4.5, §10.1, §11.5, etc.) also references `PLAN_15` and
    `PLAN_14` in places where the intended target is the AI features
    doc (PLAN_18) or the line-editor doc (PLAN_14); a fuller audit
    is required to enumerate every site.
- **Scope of fix.** Two documents — `PLAN_14_line_editor.md` and
  `PLAN_15_prompt.md`. Cross-references only; no behavioural or
  structural changes. Audit every `PLAN_NN` reference in the body of
  both docs and remap to the semantically intended target, using the
  parenthetical hint (`(config)`, `(AI segments)`, etc.) as the
  authority.
- **Suggested approach.** Build a small inventory table mapping each
  current `PLAN_NN (hint)` reference to the intended PLAN number
  based on the current planning table in `plan.md`. Apply edits
  manually (the cascade-sweep tooling is by-number-only and cannot
  resolve semantic intent). Append a renumber-log entry to each doc
  noting the semantic-fix pass (distinct from the cascade pass).
- **Verification criteria.**
  - `rg -n '\bPLAN_(14|15|16|18)\b' Documents/PLAN_14_line_editor.md
Documents/PLAN_15_prompt.md` returns no stale references when
    paired with their parenthetical hints.
  - The audit table is recorded in the commit message or in a short
    note appended to each doc's renumber-log entry.
  - No other PLAN doc is modified (single-purpose commit).
- **Scheduling constraints.** Independent — does not block any
  PLAN-XX task. Should be fixed before PLAN_14 (line editor) or
  PLAN_15 (prompt) implementation work starts, so implementers do
  not chase the wrong doc. No earlier task depends on this fix.
