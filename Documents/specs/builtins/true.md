# `true` — return a successful result

> Status: draft
> Owner: `PLAN_07`
> Tier: 1
> Sources: bash 5.3 manual §"true"; POSIX.1-2024 §"true"
> Corpus: `tests/spec/builtins_tier1/true_*.case.toml` (one per support row)
> Last updated: 2026-06-28

## 1. Synopsis

`true`

## 2. Description

`true` does nothing and always succeeds. It is the canonical
zero-cost success: invoking it sets the shell's last-command exit
status to `0` and produces no output. Its primary use is as a loop
body or condition placeholder (`while true; do ...; done`), as the
"always run" branch of a conditional, and as a no-op stub where a
command is grammatically required but no work should be done.

The bash builtin `true` ignores every operand and option handed to
it. Unlike the standalone `/usr/bin/true` from GNU coreutils — which
honours `--help` and `--version` — the bash builtin treats `--help`,
`--version`, and any other argument as ignorable noise and still
exits `0`. fredshell matches the bash builtin, not coreutils, because
inside the shell the builtin always shadows the external utility.

This sheet describes fredshell's default (non-`--posix`) `true`
behaviour. Execution is owned by PLAN_12 (Phase B); the `support`
corpus cases below are recorded against bash 5.3p9 and carry
`status = "deferred:PLAN_12"` until the implementation lands.

## 3. Support matrix

| #   | Behaviour                                                                                 | Classification | Corpus                                       |
| --- | ----------------------------------------------------------------------------------------- | -------------- | -------------------------------------------- |
| 3.1 | `true` returns exit status `0`                                                            | support        | `builtins_tier1/true_exit_zero.case.toml`    |
| 3.2 | `true a b c` ignores all operands and still returns exit status `0`                       | support        | `builtins_tier1/true_ignores_args.case.toml` |
| 3.3 | `true --help` / `true --version` are ignored (the builtin shadows coreutils) and exit `0` | support        | `builtins_tier1/true_ignores_help.case.toml` |
| 3.4 | `true` writes nothing to stdout or stderr                                                 | support        | `builtins_tier1/true_no_output.case.toml`    |

## 4. Bash quirks

1. **The builtin shadows `/usr/bin/true` and ignores `--help` /
   `--version`.** GNU coreutils `true` prints help or version text and
   exits when handed `--help` or `--version`; the bash builtin (which
   always takes precedence inside the shell) treats those arguments as
   ignorable operands and exits `0` silently (row 3.3). Scripts that
   expect `true --version` to print something are relying on the
   external utility, not the shell builtin — a common source of
   confusion that this sheet pins down deliberately.
2. **Every operand is ignored, never validated.** `true` never reports
   a usage error: `true -x`, `true ''`, and `true a b c` all exit `0`
   with no diagnostic (row 3.2). There is no argument it can reject,
   which is what makes it a safe grammatical filler.

## 5. Wontfix rationale

None. Every row in §3 is classified `support`.

## 6. Deferred rows

None. Every row in §3 is classified `support`.

## 8. References

- Bash reference manual §"Bourne Shell Builtins" / `help true`
  (bash 5.3p9).
- POSIX.1-2024 ("Issue 8") shell command language, `true` utility.
- Owning implementation: `Documents/PLAN_12_exec_phase_b.md`
  (Phase B executor; `true` builtin).
- `Documents/PLAN_07_spec_drafting.md` §5 (classification semantics).
