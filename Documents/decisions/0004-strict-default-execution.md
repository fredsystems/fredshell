# ADR 0004 — Strict-Default Execution

- Status: accepted
- Date: 2026-05-21
- Supersedes: —
- Superseded by: —

## Context

The v0 executor handles a handful of builtins natively (`cd`, `exit`,
`pwd`, `echo`) and historically delegated everything else to
`/bin/sh -c`. This was scaffolding: it let the binary REPL feel
shell-like before the native execve path was written. `ExecEnv` exposed
the choice as `ExternalCommandPolicy { FallbackToSh, Strict }`, with
the binary REPL defaulting to `FallbackToSh` and the spec harness
defaulting to `Strict`.

That split default was wrong. A shell that silently delegates to
`/bin/sh` whenever fredshell does not understand a command produces
exactly the failure mode the project exists to avoid: the user types a
command, something happens, and there is no way to tell whether
fredshell handled it or quietly punted to bash. Worse, the asymmetry
means the spec harness was measuring a stricter shell than the user
was running. The number on the dashboard ("X% of cases pass") had no
operational meaning for the binary REPL because the binary REPL was a
different shell.

Shells operate with elevated blast radius compared to libraries.
Misinterpreting a `cd`, `rm`, or redirection can lose data. A shell
that silently substitutes a different implementation in response to
"feature not yet implemented" is an unsafe default. The correct
default at all stages of the project is to refuse loudly.

The original framing — "the binary REPL needs to keep working during
the v0 → v1 transition" — was a process concern dressed up as a
correctness argument. It is real, but it belongs in an explicit,
opt-in escape hatch, not in the default.

## Decision

Strict execution is the universal default for `ExecEnv`. Both
constructors — `ExecEnv::from_process` (used by the binary REPL) and
`ExecEnv::sandboxed` (used by the spec harness) — return an `ExecEnv`
with `external_command_policy = ExternalCommandPolicy::Strict`.

A temporary escape hatch exists so the binary REPL remains usable for
dogfooding while the native executor is incomplete: setting the
environment variable `FREDSHELL_ALLOW_SH_FALLBACK=1` (exact string
match, à la `RUST_BACKTRACE=1`) before launching fredshell selects
`ExternalCommandPolicy::FallbackToSh` in `ExecEnv::from_process`. Any
other value — empty, `0`, `true`, mixed case — leaves the policy at
`Strict`. `ExecEnv::sandboxed` does not consult any environment
variables; hermeticity is its whole point.

The escape hatch is **temporary**. It will be removed in the release
that marks the native execve path as complete (tracked by `PLAN_06`).
It is documented as such in `ExternalCommandPolicy::FallbackToSh`'s
rustdoc and in `COMPAT.md`.

### Concretely

- `#[derive(Default)]` on `ExternalCommandPolicy` selects `Strict`.
- `ExecEnv::from_process` reads `FREDSHELL_ALLOW_SH_FALLBACK` exactly
  once during construction; the resulting `ExecEnv` is then immutable
  with respect to this decision (the field is `pub` and any host
  can override it, but no environment variable is re-read).
- `ExecEnv::sandboxed` ignores the environment variable entirely.
- The spec harness is unaffected: it already explicitly constructs
  `Strict` and asserts against bash output.

## Consequences

### Positive

- The number on the spec dashboard means the same thing it does in the
  binary REPL. A `pass` in the harness is a `pass` for the user.
- The "fredshell silently shelled out to bash" failure mode is gone by
  construction. Users see `NoExternalExecutor` immediately when
  fredshell does not handle a command, and the error names the missing
  feature.
- The fallback path becomes opt-in, observable, and removable. There
  is no question about whether a given user session is exercising
  fredshell-as-itself or fredshell-plus-bash — the environment answers
  it.
- Documentation collapses to a single rule: "fredshell is strict; the
  escape hatch is `FREDSHELL_ALLOW_SH_FALLBACK=1` and goes away
  before v1.0."

### Negative

- Until the native executor lands `exec_v_p` and friends (`PLAN_06`),
  the out-of-the-box binary REPL is much less useful: typing `ls`,
  `git status`, or any external command produces
  `NoExternalExecutor`. Dogfooders must export
  `FREDSHELL_ALLOW_SH_FALLBACK=1` explicitly, which is friction.
- One environment variable becomes part of the user-facing contract
  and must be honored as such until removed.

### Risks accepted

- **Dogfooding friction slows pre-v1 adoption.** Mitigation: the
  escape hatch exists and is documented. Anyone who reads the README
  will find it; anyone who does not is by definition not yet a user
  of this stage of the project.
- **Removal of the escape hatch is a breaking change.** Mitigation: it
  is documented as temporary in every place it appears (variant
  rustdoc, this ADR, `COMPAT.md`), so its removal is not a surprise.

## Alternatives considered

### Keep `FallbackToSh` as the binary default

Leave the existing split: harness strict, binary fallback. **Rejected.**
This is the status quo and is what motivated the recalibration. The
silent-substitution failure mode is exactly what the project must
avoid, and the dashboard number is meaningless to users while the
defaults disagree.

### Strict default with no escape hatch

Flip to strict and force everyone to either patch the source or wait
for the native executor. **Rejected.** This makes the binary REPL
unusable for dogfooding for the duration of `PLAN_06`, which destroys
the feedback loop. The escape hatch is cheap, scoped, and explicitly
temporary; the cost of having it is much lower than the cost of
losing dogfood signal.

### Strict default with a runtime CLI flag instead of an env var

`fredshell --allow-sh-fallback` as a startup flag. **Rejected (for
now).** CLI flags are part of the long-term UI surface and the
fallback is explicitly temporary. An env var matches the precedent
(`RUST_BACKTRACE`, `NO_COLOR`) for opt-in diagnostic/transitional
toggles. When the fallback is removed, removing one `if env::var(...)`
block is cleaner than removing a CLI flag.

### Strict default with a permissive `--legacy` runtime mode

A broader "act like the old shell" mode covering the fallback plus
other transitional behavior. **Rejected.** The fallback is the only
transitional behavior we know we want to gate. Inventing a `--legacy`
umbrella now means designing a contract for behavior we have not yet
written.

## References

- `PLAN_05_testing.md` §4.2 — the original strict-execution mode for
  the spec harness; this ADR generalizes that policy to the binary
  REPL.
- `PLAN_06_*` — the native execve path whose completion retires the
  escape hatch.
- `crates/fredshell-core/src/exec/env.rs` — the implementation
  (`ExternalCommandPolicy`, `ExecEnv::from_process`,
  `ExecEnv::sandboxed`).
- ADR 0001 — in-process execution; this ADR is the operational
  default that ADR 0001's strategy implies.
- ADR 0003 — test-first compatibility methodology; this ADR removes
  the divergence between the harness's measurement and the binary
  REPL's behavior.
