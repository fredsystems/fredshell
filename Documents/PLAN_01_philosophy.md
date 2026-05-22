# PLAN 01 — Philosophy, Goals, and Non-Goals

> Last updated: 2026-05-20 — initial draft.

This document defines what fredshell is, who it is for, what it explicitly will
not be, and how we will know whether it succeeded. Every other planning
document defers to this one when there is a conflict.

## The one-sentence statement

**fredshell is an opinionated, batteries-included Rust shell that runs real
bash scripts and ships the modern interactive UX (prompt, fuzzy history,
completion, builtins) baked in by default — for Linux and macOS users who
treat the shell as a daily-driver tool, not a configuration project.**

## Why this exists

Modern shell users sit in one of two uncomfortable positions:

1. **Plain bash or POSIX `/bin/sh`.** Works everywhere; the interactive
   experience is from the 1990s. Building a comfortable environment requires
   accumulating `.bashrc` lore: `bash-completion`, `fzf`, `starship` or `oh-
my-posh`, `lsd` or `exa`, syntax-highlighting via `ble.sh`, history-merge
   tricks, half a dozen aliases everyone copy-pastes from blog posts. The
   result is fragile, slow to start, and never portable to a new machine.

2. **Heavily-customized zsh or fish.** Better interactive UX out of the box,
   but each has a tax. zsh requires the same configuration archaeology as
   bash (oh-my-zsh, prezto, antigen, znap — pick one and pay the
   complexity). fish has a great default experience but its syntax is
   incompatible with the bash scripts that already run on every server,
   container, and CI runner in existence.

A shell that combines bash-script compatibility with a modern out-of-the-box
interactive experience does not exist. fredshell aims to be that shell.

The audience is not "everyone who uses a shell." It is the user who:

- Reads and writes bash scripts professionally.
- Spends serious time in an interactive shell every day.
- Has tried (and likely tired of) maintaining a 500-line shell config.
- Is comfortable on Linux or macOS, often both.
- Values declarative system configuration (Nix, home-manager, dotfiles
  managed as a flake) — or could be persuaded to.
- Trusts a single well-engineered default over a sprawl of community
  plugins.

This is not the user who wants a structured-data shell (nushell), a
notebook-style REPL (Elvish, oil), or a POSIX-pedant `dash`. Those are good
projects; fredshell is not them.

## Goals

These are the things fredshell **must** be, in roughly descending priority.
Each is the spine of one or more planning documents.

### G1. Real bash-script compatibility

Existing bash scripts — `#!/bin/bash` and `#!/usr/bin/env bash` — must run
unchanged on fredshell when fredshell is invoked as `bash`. The compatibility
target is **bash 5.x, real-world scripts**, not the bash test suite for its
own sake.

Operationally:

- Parse and execute the full POSIX shell grammar plus the bash extensions
  that scripts in the wild actually use: `[[ ]]`, `(( ))`, arrays,
  associative arrays, parameter expansion (`${var:-default}` and the full
  family), command substitution, process substitution (`<(cmd)`,
  `>(cmd)`), here-documents and here-strings, `local`, `declare`,
  brace expansion, extended globbing under `shopt -s extglob`.
- Job control compatible with bash (`fg`, `bg`, `jobs`, `wait`,
  `disown`, `&`, `kill %1`, SIGTSTP/SIGCONT).
- Builtins behave as bash documents them — exit statuses, side effects,
  flag handling.
- Trap handling matches bash semantics for at least `EXIT`, `ERR`,
  `DEBUG`, and the standard signals.

Out of scope for v1: `bash -i` startup-file fidelity (we have our own rc
files), `BASH_SOURCE` runtime introspection in deeply-nested cases,
obscure `shopt` flags nobody sets in practice, the `-o functrace` /
`-o errtrace` interaction edge cases. These are recorded as "compat
debt" and addressed as real-world scripts surface them.

The compat strategy itself — native parser vs adopting `brush-parser`, the
phasing from `/bin/sh -c` fallback to in-process execution — is owned by
`PLAN_06_exec.md`. This document only commits to the goal.

### G2. Modern interactive UX, baked in, zero configuration

A user installing fredshell on a fresh machine and running it gets, by
default, the experience they would have built up over years of `.bashrc`
hacking on bash:

- A starship-style prompt with git status, language version detection,
  shell-context indicators, and async-rendered slow segments.
- fzf-style fuzzy history search bound by default.
- Completion that works for filenames, command flags, git subcommands, and
  the most common tools (`cargo`, `nix`, `kubectl`, `docker`, `ssh`,
  `systemctl`).
- Syntax highlighting of the command line as the user types.
- lsd-style `ls` output (icons, color, alignment) — see ADR 0001.
- Multi-line editing with sensible keybindings (emacs-style by default,
  matching `readline`).
- History that merges across concurrent sessions sanely (no last-write-
  wins eating history).
- Bracketed paste, terminal hyperlinks (OSC 8), clipboard integration via
  OSC 52 where supported.

"Zero configuration" does not mean "uncustomizable." It means the default
experience is the one most users would converge on after spending months
tuning bash or zsh. Every default is overridable; very few should need to
be.

### G3. In-process execution, no `/bin/sh` shellout

Captured in ADR 0001. Everything fredshell can run in its own process, it
does run in its own process. `/bin/sh -c` is never used as an execution
backend in production code.

### G4. Nix-native

fredshell is delivered as a flake. It exposes:

- A package (`packages.<system>.default`).
- A home-manager module under `homeManagerModules.default` that:
  - Installs fredshell into the user environment.
  - Configures it declaratively (config file generated from the module's
    `programs.fredshell.settings` attrset).
  - Optionally sets `$SHELL` to fredshell (`programs.fredshell.defaultShell`).
  - Optionally installs an entry in `/etc/shells` (system module variant).
- A NixOS module for system-level installation.
- A `devShell` that brings in everything needed to develop fredshell
  itself.

The Nix integration is **first-class**, not an afterthought. It is one of
the two main delivery channels (the other being conventional `cargo
install` and distribution packages). It owns its own planning document
(`PLAN_13_nix_integration.md`).

### G5. Predictable performance

Performance is a feature. Specifically:

- **Cold startup to first interactive prompt:** < 50ms on a modern
  machine (M2 Mac, mid-tier Linux laptop). This includes loading config,
  detecting terminal capabilities, and rendering the prompt.
- **Per-keystroke latency in the line editor:** < 1ms from keypress to
  redrawn cursor under typical load. The user must not perceive lag.
- **Prompt re-render between commands:** < 10ms median, < 30ms 99th
  percentile, with async segments (e.g., git status on a large repo)
  excluded from the budget — they render with a placeholder and update
  when ready.
- **Per-command dispatch overhead:** the cost of parsing and dispatching
  a single external command must be small enough that running a tight
  loop of cheap externals (`for i in {1..1000}; do /bin/true; done`) is
  bounded by `fork/exec` cost, not by fredshell's overhead. Target:
  fredshell's per-command overhead is ≤ 20% of the underlying
  `fork/exec` time.

These budgets drive design choices throughout (`PLAN_11_prompt.md`,
`PLAN_07_line_editor.md`, `PLAN_02_architecture.md`). Regressions are
caught by benchmarks per `AGENTS.md`'s mandatory benchmarking rule.

### G6. Correctness over cleverness

fredshell is a daily tool. The user's terminal must not be wedged after a
panic. The user's shell history must not be corrupted by a concurrent
write. Backgrounded jobs must not get lost. Exit codes must propagate
correctly through pipelines. SIGINT must do what the user expects when
they press Ctrl-C.

This goal manifests as:

- No `unwrap()`/`expect()` in production code (per `AGENTS.md`).
- RAII-guarded raw-mode and terminal-state restoration that survives
  panics (per `PLAN_04_terminal_io.md`).
- Conservative defaults; loud failures over silent fallbacks.
- A test culture (per `PLAN_05_testing.md`) that treats subtle behaviors
  as testable invariants.

## Non-goals

Things fredshell **will not** be. Stated explicitly so we can say no without
re-litigating.

### NG1. Not a POSIX-conformant `/bin/sh`

fredshell targets _bash compatibility_, not POSIX shell certification.
The distinction matters:

- POSIX shell _behavior_ is the required substrate. Bash is itself a
  POSIX-compliant shell with extensions, so any shell that correctly
  runs real bash scripts is necessarily also implementing POSIX
  semantics for the overlapping surface (parameter expansion, quoting,
  redirection, control flow, builtins like `cd` / `export` / `set`,
  exit-status propagation). We do not get to skip POSIX; we get to skip
  pursuing _certification_ of POSIX.
- We will not run, target, or claim conformance with the POSIX shell
  conformance test suite. Where bash and POSIX disagree (and they do —
  in `echo` semantics, brace expansion, `[[ ]]`, arrays, process
  substitution, `local`, and others), we follow bash.
- We will not preserve POSIX-only behaviors that bash overrides. We
  will not add a `--posix` mode in v1.

In short: POSIX is an implementation detail of being bash-compatible,
not a goal in its own right.

### NG2. Not Windows-compatible

Linux and macOS only. Windows users are well served by PowerShell and
WSL. Cross-platform abstraction tax (Windows pty quirks, signal model
differences, terminal-feature variance) is not worth paying.

### NG3. Not a structured-data shell

Pipelines are bytes, not values. Tier-2 builtins may produce structured
output behind opt-in flags (e.g., `ls --json`), but the pipeline model is
text-stream — because that is what bash scripts expect and what every
existing Unix tool produces.

### NG4. Not a programming language

The shell language is a shell language. We will not add Lua, Python,
Lisp, or our own DSL embedded in the prompt or config. Configuration is
declarative TOML; control flow happens in shell scripts.

### NG5. Not a plugin ecosystem

fredshell has no plugin API for v1. The reason "zero configuration" can
work is that we own the entire UX: prompt, completion, syntax
highlighting, fuzzy search are all _internal subsystems_, not pluggable
extensions. Adding a plugin API now would foreclose the ability to make
the default experience coherent. A plugin model may be revisited post-v1
when the core is stable enough that we know what stable extension points
look like.

### NG6. Not a terminal multiplexer

We do not replace tmux. We integrate with it (OSC 7 for cwd, OSC 133 for
semantic prompts, kitty keyboard protocol pass-through when negotiated)
but the multiplexer remains a separate concern.

### NG7. Not a network shell

No remote-execution features in the shell itself. `ssh` is a perfectly
good external program; we make it pleasant to use via completion and
prompt indicators, but we do not embed ssh.

### NG8. Not a file manager

`ls` is a tier-2 builtin with good output; we will not build mc, ranger,
or a TUI file browser into the shell.

### NG9. Not bug-compatible with bash

We aim to run real-world bash scripts. We do not aim to reproduce every
bash bug. Where bash has documented quirks that scripts rely on, we
match them. Where bash has undocumented quirks that nobody depends on,
we are free to diverge.

## Target user, concretely

A persona to anchor design decisions:

> **The user is a software engineer or systems person who lives in the
> terminal 4+ hours a day, writes bash scripts as part of their job, has
> tried at least two of (`oh-my-zsh`, `prezto`, `fish`, `starship`, `fzf
shell integration`), and has at some point given up on a custom shell
> config because it was unmaintainable. They run Linux or macOS, they
> use git daily, they probably use either Nix/home-manager or
> dotfiles-as-a-repo, and they value a tool that makes one good
> decision they can live with over twenty configurable knobs they have
> to choose between.**

When a design question arises and is genuinely 50/50, choose the option
this user would prefer.

## Success criteria

We will know fredshell is succeeding when:

1. **The author is using it as their login shell** on at least one
   machine, full-time, without falling back to bash or zsh for tasks.
2. **A bash script picked at random from a real-world repository** (the
   first useful result for "bash script GitHub" or similar) runs to
   completion in fredshell with identical observable behavior to bash 5.
3. **First-time-user setup** consists of: install via Nix or `cargo
install`, run, and use. No configuration file is required for the
   default experience to be pleasant.
4. **Performance budgets in G5 are met** on the reference hardware and
   stay met over the project's lifetime (enforced by benchmarks).
5. **A second daily user exists** and has not requested a major change
   to defaults that would conflict with the philosophy in this
   document.

We will know fredshell is **failing** if any of:

1. The author drifts back to bash or zsh for interactive work because
   fredshell is missing something or annoying in some way.
2. Tier-2 builtin maintenance grows faster than the rest of the
   codebase.
3. Bash-script compatibility regresses on real scripts already
   shown to work.
4. The Nix integration story bit-rots.
5. The "zero configuration" promise is broken — a typical user needs
   to write a config to get a usable experience.

## Tensions and how we resolve them

Some goals will conflict in practice. The resolution rules:

- **Compatibility vs UX:** bash-compat wins when running a script
  (`fredshell -c '...'`, `#!/usr/bin/env fredshell`, or
  `--bash-mode`). UX wins in the interactive prompt. The two modes are
  explicitly distinguished (see ADR 0001's tier-2-disabled-in-script-
  mode default).

- **Correctness vs performance:** correctness wins. Then we work on
  performance until budgets are met without correctness regressions.

- **Defaults vs configurability:** strong defaults win. We add a config
  knob only when at least two users credibly disagree about the right
  default, or the right default is genuinely environment-dependent.

- **Built-in vs external:** see ADR 0001's tier model. The default
  position for "should this be a builtin?" is **no**, unless it
  satisfies the tier-2 criteria (interactive frequency, cross-platform
  consistency value, performance value).

- **Native vs upstream:** prefer well-engineered upstream crates when
  they fit. Roll our own when the upstream's design conflicts with the
  performance, allocation, or testability constraints of an interactive
  shell. See ADR 0002 for the exemplar (we roll our own ANSI encoder
  rather than adopting one that does not match our shape).

## What this document is not

This document does not specify:

- The crate layout or module boundaries (`PLAN_02_architecture.md`).
- The bash compatibility strategy or phasing (`PLAN_06_exec.md`).
- Any concrete API design (the relevant `PLAN_NN_*.md`).
- The roadmap or milestone ordering (`PLAN_15_milestones.md`).

If you find yourself wanting to add an interface sketch or a phase plan
to this document, you are in the wrong document.
