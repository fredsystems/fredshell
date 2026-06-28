# `:` — null command

> Status: draft
> Owner: `PLAN_07`
> Tier: 1
> Sources: bash 5.3 manual §"Bourne Shell Builtins" (`:`); POSIX.1-2024 §"Special Built-In Utilities" (`:`)
> Corpus: `tests/spec/builtins_tier1/colon_*.case.toml` (one per support row)
> Last updated: 2026-06-28

<!--
Filename deviation (PLAN_07 §3.1): the builtin's invocation name is
`:`, but a literal `:.md` is hostile to tooling — it breaks glob
patterns and renders the `<sheet-id>-<row#>` refuse! diagnostic as
`:-3.1`. By user decision this sheet is named `colon.md` with
sheet-id `colon`; the H1 above and §1 Synopsis carry the real `:`
invocation name. See PLAN_07 §14 (08.2d).
-->

## 1. Synopsis

`: [arguments]`

## 2. Description

`:` is the null command. It expands its arguments and performs any
redirections, then does nothing and returns exit status `0`. Its
arguments are evaluated for their side effects — parameter
expansion, command substitution, and arithmetic expansion all run —
but the resulting words are discarded rather than executed.

`:` is most often used where the grammar requires a command but no
work should be done: as the always-true loop condition
(`while :; do ...; done`), as an empty `then` or `else` branch
(`if cond; then :; fi`), and — the load-bearing idiom — to trigger
an argument's side effect, such as assigning a default with
`: "${var=default}"` or truncating a file with `: > file`.

`:` is a POSIX _special_ built-in. The one user-visible consequence
of that status in fredshell's default (non-`--posix`) mode is
covered by row 3.6 and §7: variable assignments that prefix a
`:` invocation do _not_ persist into the shell environment in
default bash mode, unlike a true POSIX special-builtin invocation.

This sheet describes fredshell's default (non-`--posix`) `:`
behaviour. Execution is owned by PLAN_12 (Phase B); the `support`
corpus cases below are recorded against bash 5.3p9 and carry
`status = "deferred:PLAN_12"` until the implementation lands.

## 3. Support matrix

| #   | Behaviour                                                                                           | Classification | Corpus                                        |
| --- | --------------------------------------------------------------------------------------------------- | -------------- | --------------------------------------------- |
| 3.1 | `:` returns exit status `0`                                                                         | support        | `builtins_tier1/colon_exit_zero.case.toml`    |
| 3.2 | `: a b c` discards its operands, writes nothing, and returns exit status `0`                        | support        | `builtins_tier1/colon_ignores_args.case.toml` |
| 3.3 | `: "${var=word}"` expands its arguments, so the `${var=word}` assignment side effect occurs         | support        | `builtins_tier1/colon_arg_assign.case.toml`   |
| 3.4 | `: "$(cmd)"` runs command substitution in its arguments for the side effect, discarding output      | support        | `builtins_tier1/colon_cmd_subst.case.toml`    |
| 3.5 | `: > file` performs its redirections, truncating `file` to zero length                              | support        | `builtins_tier1/colon_truncate.case.toml`     |
| 3.6 | `var=value :` assignments do not persist into the shell environment in default (non-`--posix`) mode | support        | `builtins_tier1/colon_assign_scope.case.toml` |

## 4. Bash quirks

1. **Arguments are expanded, not just ignored.** Unlike `true`,
   which treats every operand as inert noise, `:` expands its
   arguments and only then discards them (rows 3.3, 3.4). This makes
   `: "${var=default}"` a standard one-liner for assigning a default
   without producing output, and `: "$(command)"` a way to run a
   command purely for its side effects. Scripts rely on this
   distinction; it is the reason `:` and `true` are not
   interchangeable.
2. **Redirections still happen.** `:` performs its redirections
   before returning, so `: > file` is the canonical zero-dependency
   file-truncation idiom (row 3.5). The command does nothing, but the
   `> file` redirect opens and truncates the target as a side effect
   of running the command at all.

## 5. Wontfix rationale

None. Every row in §3 is classified `support`.

## 6. Deferred rows

None. Every row in §3 is classified `support`.

## 7. POSIX divergence

`:` is a POSIX _special_ built-in. POSIX requires that variable
assignments prefixed to a special built-in (`var=value :`) persist
in the current shell environment after the command completes. Bash
honours this only in `--posix` mode; in its default mode the
assignment is scoped to the (empty) command invocation and is
discarded afterwards (row 3.6). fredshell follows bash's default
behaviour: prefix assignments to `:` do not persist. The
divergence is toggled in bash by `set -o posix` / `--posix`, which
is not a v1 target (PLAN_07 §2.4). This subsection is informational;
the contract is the default-mode behaviour in row 3.6.

## 8. References

- Bash reference manual §"Bourne Shell Builtins" / `help :`
  (bash 5.3p9).
- POSIX.1-2024 ("Issue 8") shell command language, §"Special
  Built-In Utilities" (`:`).
- Owning implementation: `Documents/PLAN_12_exec_phase_b.md`
  (Phase B executor; `:` builtin).
- `Documents/PLAN_07_spec_drafting.md` §5 (classification semantics),
  §2.4 (`--posix` mode out of scope), §3.1 (filename convention;
  see the `colon.md` deviation note above).
