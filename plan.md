# fredshell — Master Plan

> Last updated: 2026-05-20 — PLAN_06 split into 06a (Phase A execution
> pipeline skeleton, draft) and 06b (Phase B real bash-compat
> executor, stub pending). 06a now sequences between PLAN_04 and
> PLAN_05.

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

Two foundational decisions shape everything else and are recorded as ADRs:

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

## Planning documents

| #   | Document                               | Phase | Status        | Summary                                                                                                                                                 |
| --- | -------------------------------------- | ----- | ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 01  | `Documents/PLAN_01_philosophy.md`      | A     | draft         | Goals, non-goals, target user, success criteria.                                                                                                        |
| 02  | `Documents/PLAN_02_architecture.md`    | A     | draft         | Crate layout, module boundaries, key traits, dependency direction.                                                                                      |
| 03  | `Documents/PLAN_03_ansi.md`            | A     | implemented   | `fredshell-ansi` crate: encoder API, minimal decoder, `Write`-based contract, allocation budget.                                                        |
| 04  | `Documents/PLAN_04_terminal_io.md`     | A     | implemented   | Raw mode discipline, signals, process groups, terminal feature detection, kitty keyboard negotiation.                                                   |
| 05  | `Documents/PLAN_05_testing.md`         | A     | draft         | Spec-test harness, corpus methodology, oils-spec integration, real-world script corpus, CI metrics.                                                     |
| 06a | `Documents/PLAN_06a_exec_skeleton.md`  | A     | draft         | Execution-pipeline public surface: `parse`, `run_source`, `ExecEnv`, `RunResult`, `ExecError`, `Tier2Builtin`. Stub impl wraps today's `dispatch_line`. |
| 06b | `Documents/PLAN_06b_exec_semantics.md` | B     | stub pending  | Native parser strategy, brush-parser evaluation, POSIX-behavior scope, pipelines/redirections/job control. Corpus-dependent.                            |
| 07  | `Documents/PLAN_07_interactive_ux.md`  | A     | draft         | Line editor, key-byte decoder, history, completion, fuzzy search, keybindings, syntax highlighting.                                                     |
| 08  | `Documents/PLAN_08_prompt.md`          | A     | draft         | Starship-style prompt renderer, configuration model, performance budget.                                                                                |
| 09  | `Documents/PLAN_09_builtins.md`        | B     | stub pending  | Builtin inventory by tier, dispatch model, parity targets, override semantics. Corpus-dependent.                                                        |
| 10  | `Documents/PLAN_10_config.md`          | A     | draft pending | Config file format, layering, env vars, rc-file semantics.                                                                                              |
| 11  | `Documents/PLAN_11_nix_integration.md` | A     | draft pending | Home-manager module surface, flake outputs, default-shell story.                                                                                        |
| 12  | `Documents/PLAN_12_ai_features.md`     | A     | draft pending | NL→command, error explanation, provider abstraction, privacy boundaries.                                                                                |
| 13  | `Documents/PLAN_13_milestones.md`      | B     | stub pending  | Phased roadmap: MVP → daily-driver → bash-replacement. Corpus-dependent.                                                                                |

### Two-phase planning

Planning proceeds in two phases:

- **Phase A** drafts the docs whose content does not depend on knowing what
  bash scripts in the wild actually do. These are the architecture-shaped
  docs: testing methodology (PLAN_05), crate layout (PLAN_02), foundational
  subsystems (PLAN_03, PLAN_04, PLAN_07, PLAN_08), and peripheral design
  (PLAN_10, PLAN_11, PLAN_12). PLAN_05 (testing) is intentionally drafted
  before PLAN_02 (architecture) because the spec-test harness imposes
  hard constraints on the architecture (parser separable from executor,
  sandboxable execution environment, clean batch-mode entry point).

- **Phase B** drafts the docs whose content is informed by the spec corpus
  once it exists: bash-execution semantics (PLAN_06b), tier-2 builtin
  inventory and priority (PLAN_09), and the implementation roadmap
  (PLAN_13). These carry "stub pending" status during Phase A and
  receive full drafts only after the v1 corpus has been curated and
  the harness reports a baseline pass-rate. PLAN_06a is Phase A: it
  ships the public surface those Phase B docs build on.

The rationale and methodology are pinned in ADR 0003.

## Architecture Decision Records

| ID   | Title                                           | Status   |
| ---- | ----------------------------------------------- | -------- |
| 0001 | In-process execution and the builtin tier model | accepted |
| 0002 | ANSI encoding crate strategy                    | accepted |
| 0003 | Test-first compatibility methodology            | accepted |

New ADRs are added as significant design questions are resolved. ADRs are
immutable once accepted; superseding decisions get new ADRs that link back.

## How this plan is executed

- Planning work happens on the `docs/planning` branch.
- Each planning document gets its own commit (or small set of commits) on that
  branch, reviewed iteratively.
- Implementation work for each plan section happens on its own task branch
  (`task-NN/<topic>`), per `AGENTS.md`.
- `plan.md` is updated whenever a planning document changes status (draft →
  accepted → implementing → done) or a new document/ADR is added.

## Open questions

These are unresolved as of this draft and will be addressed by the relevant
planning document or ADR:

- Choice of native bash parser: adopt `brush-parser`, fork, or write our own
  (deferred to `PLAN_06b`).
- Line-editor library: build on `reedline`/`rustyline`, or roll our own on top
  of `crossterm`/`termwiz` (deferred to `PLAN_07`).
- Async runtime: required for AI features and background jobs, optional
  elsewhere — scope to be decided in `PLAN_02` and `PLAN_12`.
- Plugin/extension model: out of scope for v1, but the architecture must not
  preclude it.
