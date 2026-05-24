# fredshell — Master Plan

> Last updated: 2026-05-24 — Work-order renumber of all PLAN
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
| 06  | `Documents/PLAN_06_exec.md`            | A       | implemented   | Execution pipeline, Phase A scaffold: public surface, dispatcher, `ExecEnv`, REPL/spec wiring. Phase B content extracted to PLAN_11.                                                                                    |
| 07  | `Documents/PLAN_07_spec_drafting.md`   | A       | drafted       | Spec sheet template (one per builtin + feature), batch-of-10 review cadence, lint extensions tying `support` rows to corpus cases. Renumbered from old PLAN_08.                                                         |
| 08  | `Documents/PLAN_08_fuzzer.md`          | B       | drafted       | Grammar-aware deterministic fuzzer + differential oracle against pinned bash 5.3p9. Five tiers (F1 PR → F5 release gate). Gates PLAN_11 Phase B via 06b.0 = "F1 green on main." Renumbered from old PLAN_09.            |
| 09  | `Documents/PLAN_09_logging.md`         | A       | stub pending  | Structured logging via `tracing` + `tracing-subscriber`. Bans `println!`/`eprintln!`/`dbg!` in library crates and the `fredshell` binary. Defines log levels, target conventions, and the diagnostics sink contract.    |
| 10  | `Documents/PLAN_10_parser.md`          | B       | stub pending  | In-house recursive-descent bash parser (`fredshell-parser`). Owns lexer, AST/CST, error recovery for incremental parsing, refusal taxonomy. Absorbs old 06b.1 (ADR 0005) and 06b.2 (lexer/parser scaffold).             |
| 11  | `Documents/PLAN_11_exec_phase_b.md`    | B       | drafted       | Execution pipeline Phase B: real semantics, `ShellState`, Tier-1 builtins, expansion family, ADR 0004 sunset. Extracted from PLAN_06 §13 / temporary PLAN_06b. Subtask grid retains `06b.N` for `task-06b/` continuity. |
| 12  | `Documents/PLAN_12_traps_and_jobs.md`  | B       | drafted       | Signal traps, job control, `wait`, `kill`, foreground/background. Corpus-dependent because trap semantics differ from POSIX in well-defined ways. Renumbered from old PLAN_10.                                          |
| 13  | `Documents/PLAN_13_line_editor.md`     | A       | drafted       | Line editor: key-byte decoder, history, completion, fuzzy search, keybindings, syntax highlighting. Owns `history`/`fc` builtins, the `yield_terminal` primitive consumed by PLAN_12, and the L4 PTY harness.           |
| 14  | `Documents/PLAN_14_prompt.md`          | A       | draft         | Starship-style prompt renderer, configuration model, performance budget. Renumbered from old PLAN_11.                                                                                                                   |
| 15  | `Documents/PLAN_15_config.md`          | A       | draft pending | Config file format, layering, env vars, rc-file semantics. Renumbered from old PLAN_12.                                                                                                                                 |
| 16  | `Documents/PLAN_16_nix_integration.md` | A       | draft pending | Home-manager module surface, flake outputs, default-shell story. Renumbered from old PLAN_13.                                                                                                                           |
| 17  | `Documents/PLAN_17_ai_features.md`     | A       | draft pending | NL→command, error explanation, provider abstraction, privacy boundaries. Renumbered from old PLAN_14.                                                                                                                   |
| 18  | `Documents/PLAN_18_milestones.md`      | B       | stub pending  | Phased roadmap: MVP → daily-driver → bash-replacement. Corpus-dependent. Pending entries: **M-15-utf8-fuzz** — UTF-8 differential-fuzz tier (`F2-utf8`) scheduled between v1.0 and v1.1 per PLAN_08 §11 Q09.5.          |
| 19  | `Documents/PLAN_19_coproc.md`          | post-v1 | stub          | Coprocesses (`coproc`). Cuts across PLAN_10 parser, PLAN_11 executor, PLAN_12 jobs, PLAN_02 variables. v1 recognises and refuses; full implementation deferred. Renumbered from old PLAN_16.                            |
| 20  | `Documents/PLAN_20_gettext.md`         | A       | stub pending  | Internationalisation strategy: `gettext`-style message catalogues for diagnostic messages, error explanations, and AI-facing prompts. Locale negotiation, fallback chain, catalogue compilation in build.               |

### Two-phase planning

Planning proceeds in two phases:

- **Phase A** drafts the docs whose content does not depend on knowing what
  bash scripts in the wild actually do. These are the architecture-shaped
  docs: testing methodology (PLAN_05), crate layout (PLAN_02), foundational
  subsystems (PLAN_03, PLAN_04), spec-drafting workflow (PLAN_07),
  logging (PLAN_09), line editor (PLAN_13), prompt (PLAN_14), config
  (PLAN_15), nix integration (PLAN_16), AI features (PLAN_17), and
  gettext (PLAN_20). PLAN_05 (testing) is intentionally drafted before
  PLAN_02 (architecture) because the spec-test harness imposes hard
  constraints on the architecture (parser separable from executor,
  sandboxable execution environment, clean batch-mode entry point).

- **Phase B** drafts the docs whose content is informed by the spec corpus
  once it exists: the fuzzer/differential (PLAN_08), the parser (PLAN_10),
  Phase B of the executor (PLAN_11), traps+jobs (PLAN_12), and the
  implementation roadmap (PLAN_18). These carry "stub pending" status
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
  authored in PLAN_11 subtask 06b.1; implementation owned by `PLAN_10`
  (parser).
- Line-editor library: build on `reedline`/`rustyline`, or roll our own on top
  of `crossterm`/`termwiz` (deferred to `PLAN_13`).
- Async runtime: required for AI features and background jobs, optional
  elsewhere — scope to be decided in `PLAN_02` and `PLAN_17`.
- Plugin/extension model: out of scope for v1, but the architecture must not
  preclude it.
