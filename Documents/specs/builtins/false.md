# `false` — return an unsuccessful result

> Status: draft
> Owner: `PLAN_07`
> Tier: 1
> Sources: bash 5.3 manual §"false"; POSIX.1-2024 §"false"
> Corpus: `tests/spec/builtins_tier1/false_*.case.toml` (one per support row)
> Last updated: 2026-06-28

## 1. Synopsis

`false`

## 2. Description

`false` does nothing and always fails. It is the exact counterpart
of `true`: invoking it sets the shell's last-command exit status to
`1` and produces no output. Its primary use is as a deliberately
failing branch (`if false; then ...; fi`), as a one-shot loop guard
(`while false; do ...; done` never runs the body), and as a no-op
stub where a command is grammatically required and a failure status
is wanted.

The bash builtin `false` ignores every operand and option handed to
it. Unlike the standalone `/usr/bin/false` from GNU coreutils — which
honours `--help` and `--version` — the bash builtin treats `--help`,
`--version`, and any other argument as ignorable noise and still
exits `1`. fredshell matches the bash builtin, not coreutils, because
inside the shell the builtin always shadows the external utility.

This sheet describes fredshell's default (non-`--posix`) `false`
behaviour. Execution is owned by PLAN_12 (Phase B); the `support`
corpus cases below are recorded against bash 5.3p9 and carry
`status = "deferred:PLAN_12"` until the implementation lands.

## 3. Support matrix

| #   | Behaviour                                                                                   | Classification | Corpus                                        |
| --- | ------------------------------------------------------------------------------------------- | -------------- | --------------------------------------------- |
| 3.1 | `false` returns exit status `1`                                                             | support        | `builtins_tier1/false_exit_one.case.toml`     |
| 3.2 | `false a b c` ignores all operands and still returns exit status `1`                        | support        | `builtins_tier1/false_ignores_args.case.toml` |
| 3.3 | `false --help` / `false --version` are ignored (the builtin shadows coreutils) and exit `1` | support        | `builtins_tier1/false_ignores_help.case.toml` |
| 3.4 | `false` writes nothing to stdout or stderr                                                  | support        | `builtins_tier1/false_no_output.case.toml`    |

## 4. Bash quirks

1. **The builtin shadows `/usr/bin/false` and ignores `--help` /
   `--version`.** GNU coreutils `false` prints help or version text
   and exits when handed `--help` or `--version`; the bash builtin
   (which always takes precedence inside the shell) treats those
   arguments as ignorable operands and exits `1` silently (row 3.3).
   Scripts that expect `false --version` to print something are
   relying on the external utility, not the shell builtin — a common
   source of confusion that this sheet pins down deliberately.
2. **Every operand is ignored, never validated.** `false` never
   reports a usage error: `false -x`, `false ''`, and `false a b c`
   all exit `1` with no diagnostic (row 3.2). There is no argument it
   can reject; its exit status is fixed at `1` regardless of input.

## 5. Wontfix rationale

None. Every row in §3 is classified `support`.

## 6. Deferred rows

None. Every row in §3 is classified `support`.

## 8. References

- Bash reference manual §"Bourne Shell Builtins" / `help false`
  (bash 5.3p9).
- POSIX.1-2024 ("Issue 8") shell command language, `false` utility.
- Owning implementation: `Documents/PLAN_12_exec_phase_b.md`
  (Phase B executor; `false` builtin).
- `Documents/PLAN_07_spec_drafting.md` §5 (classification semantics).
