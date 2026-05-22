# fredshell — Master Plan

> Last updated: 2026-05-21 — restructured PLAN numbering: PLAN_06a/06b
> collapsed into PLAN_06 (exec); old PLAN_07 narrowed to line editor;
> PLAN_08/09/10 introduced for spec drafting, fuzzer/differential, and
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

| #   | Document                                   | Phase | Status        | Summary                                                                                                                                                                                                               |
| --- | ------------------------------------------ | ----- | ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 01  | `Documents/PLAN_01_philosophy.md`          | A     | draft         | Goals, non-goals, target user, success criteria.                                                                                                                                                                      |
| 02  | `Documents/PLAN_02_architecture.md`        | A     | draft         | Crate layout, module boundaries, key traits, dependency direction.                                                                                                                                                    |
| 03  | `Documents/PLAN_03_ansi.md`                | A     | implemented   | `fredshell-ansi` crate: encoder API, minimal decoder, `Write`-based contract, allocation budget.                                                                                                                      |
| 04  | `Documents/PLAN_04_terminal_io.md`         | A     | implemented   | Raw mode discipline, signals, process groups, terminal feature detection, kitty keyboard negotiation.                                                                                                                 |
| 05  | `Documents/PLAN_05_testing.md`             | A     | implemented   | Spec-test harness, corpus methodology, oils-spec integration, real-world script corpus, CI metrics.                                                                                                                   |
| 06  | `Documents/PLAN_06_exec.md`                | A/B   | mixed         | Execution pipeline. Skeleton (parse, `run_source`, `ExecEnv`, `RunResult`, `ExecError`, `Tier2Builtin`) implemented; semantics breadth (native parser, pipelines, redirection, expansion, builtins) corpus-dependent. |
| 07  | `Documents/PLAN_07_line_editor.md`         | A     | draft         | Line editor: key-byte decoder, history, completion, fuzzy search, keybindings, syntax highlighting. Includes ADR on `reedline` vs `rustyline` vs from-scratch.                                                        |
| 08  | `Documents/PLAN_08_spec_drafting.md`       | A     | draft pending | Spec sheet template (one per builtin + feature), batch-of-10 review cadence, lint extensions tying `support` rows to corpus cases.                                                                                    |
| 09  | `Documents/PLAN_09_fuzzer_differential.md` | A     | draft pending | Grammar-aware structured fuzzer, differential testing against pinned bash, structural sandbox (`bwrap` / `sandbox-exec`), bash testsuite import strategy.                                                             |
| 10  | `Documents/PLAN_10_traps_and_jobs.md`      | B     | stub pending  | Signal traps, job control, `wait`, `kill`, foreground/background. Corpus-dependent because trap semantics differ from POSIX in well-defined ways.                                                                     |
| 11  | `Documents/PLAN_11_prompt.md`              | A     | draft         | Starship-style prompt renderer, configuration model, performance budget.                                                                                                                                              |
| 12  | `Documents/PLAN_12_config.md`              | A     | draft pending | Config file format, layering, env vars, rc-file semantics.                                                                                                                                                            |
| 13  | `Documents/PLAN_13_nix_integration.md`     | A     | draft pending | Home-manager module surface, flake outputs, default-shell story.                                                                                                                                                      |
| 14  | `Documents/PLAN_14_ai_features.md`         | A     | draft pending | NL→command, error explanation, provider abstraction, privacy boundaries.                                                                                                                                              |
| 15  | `Documents/PLAN_15_milestones.md`          | B     | stub pending  | Phased roadmap: MVP → daily-driver → bash-replacement. Corpus-dependent.                                                                                                                                              |

### Two-phase planning

Planning proceeds in two phases:

- **Phase A** drafts the docs whose content does not depend on knowing what
  bash scripts in the wild actually do. These are the architecture-shaped
  docs: testing methodology (PLAN_05), crate layout (PLAN_02), foundational
  subsystems (PLAN_03, PLAN_04, PLAN_07, PLAN_11), spec-drafting workflow
  (PLAN_08), fuzzer/differential (PLAN_09), and peripheral design
  (PLAN_12, PLAN_13, PLAN_14). PLAN_05 (testing) is intentionally drafted
  before PLAN_02 (architecture) because the spec-test harness imposes
  hard constraints on the architecture (parser separable from executor,
  sandboxable execution environment, clean batch-mode entry point).

- **Phase B** drafts the docs whose content is informed by the spec corpus
  once it exists: the breadth-of-bash-semantics half of PLAN_06,
  traps+jobs (PLAN_10), and the implementation roadmap (PLAN_15). These
  carry "stub pending" status during Phase A and receive full drafts only
  after the v1 corpus has been curated and the harness reports a baseline
  pass-rate. PLAN_06's skeleton half is Phase A: it shipped the public
  surface those Phase B docs build on.

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

- Choice of native bash parser: adopt `brush-parser`, fork, or write our own
  (deferred to `PLAN_06`).
- Line-editor library: build on `reedline`/`rustyline`, or roll our own on top
  of `crossterm`/`termwiz` (deferred to `PLAN_07`).
- Async runtime: required for AI features and background jobs, optional
  elsewhere — scope to be decided in `PLAN_02` and `PLAN_14`.
- Plugin/extension model: out of scope for v1, but the architecture must not
  preclude it.
