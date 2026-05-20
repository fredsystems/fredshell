# ADR 0001 — In-Process Execution and the Builtin Tier Model

- Status: accepted
- Date: 2026-05-20
- Supersedes: —
- Superseded by: —

## Context

A shell's executor has three plausible execution strategies for any given command
line:

1. **Shell out.** Forward the line to `/bin/sh -c "<line>"`. Cheap to implement;
   pays a `fork`+`execve`+sub-shell-startup cost per command; behavior depends on
   whatever `/bin/sh` happens to be (dash on Debian, bash on macOS, ash on
   Alpine); cannot mutate the parent shell's state.
2. **Fork/exec for everything.** Parse the line natively, then `fork`+`execve`
   each external command directly. No `/bin/sh` middleman, but every `ls`, `cat`,
   `which`, `cd` (wait, `cd` _can't_ be external) goes through the OS process-
   creation path. Standard Unix shell behavior modulo builtins.
3. **In-process for common cases.** Parse natively, dispatch as many common
   commands as possible to in-process implementations (builtins). Only genuinely
   external programs go through fork/exec.

The first option is what the current scaffold does as a Phase-1 placeholder. It
is unacceptable as a long-term strategy because:

- `/bin/sh` behavior varies by distribution. Bash-script compatibility is
  impossible to guarantee through a sub-shell we do not control.
- Every command incurs sub-shell startup latency (`fork`+`exec`+rc-file load).
- Builtins that mutate shell state (`cd`, `export`, `set`) cannot work through a
  sub-shell at all.
- Structured integration with the rest of fredshell (prompt context, completion
  cache, history correlation) requires in-process execution.

The second option is correct for POSIX semantics but leaves significant UX value
on the table. Interactive shells call `ls` and `cat` orders of magnitude more
often than any other external. Replacing them with in-process implementations
unlocks:

- Performance: no per-invocation fork/exec cost for the hottest interactive
  commands.
- Consistency: `ls` behaves identically on Linux and macOS regardless of whether
  GNU or BSD coreutils are installed.
- UX: theming, icons, structured output, integration with the prompt and
  completion system.
- Portability: behavior does not change when the user is in a stripped-down
  container or a chrooted environment.

The cost is that each in-process replacement is a maintenance surface — users
invoke `ls` and `find` with arbitrary flags assuming a specific implementation's
behavior.

## Decision

fredshell adopts the third strategy. Concretely:

1. **`/bin/sh` is never invoked as an execution backend.** The Phase-1
   `/bin/sh -c` fallback is a temporary scaffold that will be removed as soon as
   the native executor is functional. No production code path will ever
   delegate to `sh -c`.

2. **Commands are dispatched through a tiered builtin model:**
   - **Tier 1 — Shell builtins.** Mandatory in-process; POSIX requires them to
     run in the shell's own process because they mutate shell state or shell-
     local context. These include (non-exhaustive): `cd`, `export`, `set`,
     `unset`, `alias`, `unalias`, `exit`, `source`/`.`, `eval`, `exec`, `read`,
     `pwd`, `:`, `true`, `false`, `test`/`[`, `[[`, `command`, `type`, `hash`,
     `umask`, `ulimit`, `trap`, `wait`, `kill`, `jobs`, `fg`, `bg`, `shift`,
     `getopts`, `times`, `let`, `declare`/`typeset`, `local`, `readonly`,
     `return`, `break`, `continue`, `printf`, `echo`, `pushd`, `popd`, `dirs`.
     The exact inventory is owned by `PLAN_09_builtins.md`.

   - **Tier 2 — Replacement builtins.** Common externals reimplemented in-
     process for UX, performance, and cross-platform consistency. Initial
     candidates: `ls`, `cat`, `du`, `df`, `which`, `head`, `tail`, `wc`, `sort`,
     `uniq`, `grep` (basic), `find` (basic). Each tier-2 builtin is **opt-out**
     per the configuration model — users can disable any tier-2 builtin and
     fall back to the `$PATH` binary. Tier-2 builtins must accept a documented
     subset of the corresponding GNU/BSD flag surface; the subset is recorded
     in the per-builtin documentation. Tier-2 builtins target _useful parity_,
     not _bug-for-bug parity_.

   - **Tier 3 — External programs.** Everything else. Resolved via `$PATH`,
     executed via `fork`+`execve` (or `posix_spawn` where appropriate).

3. **Dispatch order:**
   1. Alias expansion.
   2. Function lookup (user-defined shell functions).
   3. Tier-1 builtin lookup (always wins; cannot be shadowed).
   4. Tier-2 builtin lookup (unless disabled in config or invoked via
      `command <name>` or `\<name>`).
   5. `$PATH` resolution → tier-3 external execution.

4. **`command <name>` and `\<name>`** bypass aliases, functions, and tier-2
   builtins, forcing dispatch to start at tier-1-or-external (matching bash's
   `command` semantics).

## Consequences

### Positive

- Bash-script compatibility is the project's responsibility, not delegated to
  whichever `/bin/sh` happens to be installed.
- Shell state mutation works correctly with no special-case bridging.
- Interactive performance is dominated by the actual work, not by sub-shell
  startup.
- The executor becomes the single place where command dispatch is reasoned
  about, simplifying testing and tracing.
- Tier-2 builtins give us a controlled surface for cross-platform UX work
  (icons, color, structured output) without forking every coreutils program.

### Negative

- Every tier-1 builtin is code we own and must test. POSIX shell builtins are a
  meaningful surface (~40 commands).
- Tier-2 builtins are an _unbounded_ surface if we let scope creep. Discipline
  is required: a tier-2 builtin only earns its place if it is invoked
  frequently enough interactively to justify the maintenance cost.
- Users will hit cases where a tier-2 builtin lacks a flag their script uses.
  The `command <name>` escape hatch must be reliable and documented.
- We forfeit the (small) ability to inherit bug-compatibility with whatever
  `/bin/sh` ships on a given system. Some scripts assuming dash-specific or
  bash-specific quirks will break in fredshell-as-script-host until the
  native parser/executor matches the relevant quirk.

### Risks and mitigations

- **Risk: tier-2 sprawl.** Every "we should also build `cp` and `mv`" request
  expands the maintenance burden.
  **Mitigation:** new tier-2 entries require an ADR or a documented entry in
  `PLAN_09` with usage-frequency justification.

- **Risk: behavioral divergence between tier-2 and the external binary the
  user expects.**
  **Mitigation:** every tier-2 builtin documents its supported flag subset;
  unsupported flags produce a clear "use `command <name>` to invoke the
  external" error, not silent misbehavior.

- **Risk: surprises for script authors who `#!/usr/bin/env fredshell` a script
  that relies on a specific `ls` flag.**
  **Mitigation:** tier-2 builtins are off by default in non-interactive
  invocations (script mode). Configuration toggles control this.

## Alternatives considered

- **Stay with `/bin/sh -c`.** Rejected: shell-state mutation impossible,
  performance unacceptable, behavior nondeterministic across systems.
- **Fork/exec everything; no tier-2.** Viable and simpler. Rejected because
  we lose the cross-platform UX consistency and interactive-performance wins
  that motivate building a new shell in the first place. We can fall back to
  this position by disabling tier-2 globally if needed.
- **Nushell-style structured pipelines as the primary model.** Out of scope
  for fredshell — we target bash compatibility, which is fundamentally
  text-stream-oriented. Structured output may appear inside individual tier-2
  builtins (e.g., `ls --json`), but it is not the pipeline model.

## References

- `PLAN_09_builtins.md` — the actual builtin inventory and per-builtin parity
  targets.
- `PLAN_02_architecture.md` — where the executor and dispatch table live.
- `PLAN_06_bash_compat.md` — the native parser/executor that this ADR
  presupposes.
- `PLAN_05_testing.md` — the spec-test harness that measures tier-2 parity
  against bash.
