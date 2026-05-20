# fredshell — Master Plan

> Last updated: 2026-05-20 — PLAN_01 first draft landed.

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

## Planning documents

| #   | Document                               | Status        | Summary                                                                                               |
| --- | -------------------------------------- | ------------- | ----------------------------------------------------------------------------------------------------- |
| 01  | `Documents/PLAN_01_philosophy.md`      | draft         | Goals, non-goals, target user, success criteria.                                                      |
| 02  | `Documents/PLAN_02_architecture.md`    | draft pending | Crate layout, module boundaries, key traits, dependency direction.                                    |
| 03  | `Documents/PLAN_03_ansi.md`            | draft pending | `fredshell-ansi` crate: encoder API, minimal decoder, `Write`-based contract, allocation budget.      |
| 04  | `Documents/PLAN_04_terminal_io.md`     | draft pending | Raw mode discipline, signals, process groups, terminal feature detection, kitty keyboard negotiation. |
| 05  | `Documents/PLAN_05_bash_compat.md`     | draft pending | Native parser strategy, brush-parser evaluation, POSIX scope, phasing.                                |
| 06  | `Documents/PLAN_06_interactive_ux.md`  | draft pending | Line editor, key-byte decoder, history, completion, fuzzy search, keybindings, syntax highlighting.   |
| 07  | `Documents/PLAN_07_prompt.md`          | draft pending | Starship-style prompt renderer, configuration model, performance budget.                              |
| 08  | `Documents/PLAN_08_builtins.md`        | draft pending | Builtin inventory by tier, dispatch model, parity targets, override semantics.                        |
| 09  | `Documents/PLAN_09_config.md`          | draft pending | Config file format, layering, env vars, rc-file semantics.                                            |
| 10  | `Documents/PLAN_10_nix_integration.md` | draft pending | Home-manager module surface, flake outputs, default-shell story.                                      |
| 11  | `Documents/PLAN_11_ai_features.md`     | draft pending | NL→command, error explanation, provider abstraction, privacy boundaries.                              |
| 12  | `Documents/PLAN_12_testing.md`         | draft pending | Unit/integration/PTY/bash-diff harnesses, coverage strategy.                                          |
| 13  | `Documents/PLAN_13_milestones.md`      | draft pending | Phased roadmap: MVP → daily-driver → bash-replacement.                                                |

## Architecture Decision Records

| ID   | Title                                           | Status   |
| ---- | ----------------------------------------------- | -------- |
| 0001 | In-process execution and the builtin tier model | accepted |
| 0002 | ANSI encoding crate strategy                    | accepted |

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
  (deferred to `PLAN_05`).
- Line-editor library: build on `reedline`/`rustyline`, or roll our own on top
  of `crossterm`/`termwiz` (deferred to `PLAN_06`).
- Async runtime: required for AI features and background jobs, optional
  elsewhere — scope to be decided in `PLAN_02` and `PLAN_11`.
- Plugin/extension model: out of scope for v1, but the architecture must not
  preclude it.
