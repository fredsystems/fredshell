# PLAN_19 — Coprocesses (`coproc`)

> Last updated: 2026-05-23 — stub created to give `coproc`
> a permanent owning document (resolves Q-10-D / Q-06B-2).
> Phase: post-v1. Status: stub (not drafted; deferred from v1).
> Consumes (when drafted): PLAN_10 lexer/parser, PLAN_06
> executor pipeline, PLAN_12 §6 job-control builtins, PLAN_02
> `ShellState`. Consumed by: nothing in v1.

## Purpose

This document is a placeholder. It exists so that the eventual
implementation of bash's `coproc` construct has a single,
unambiguous owning plan — not a question scattered across PLAN_06
(parser), PLAN_12 (jobs), and PLAN_02 (variables).

`coproc` is **explicitly out of scope for v1.** v1 recognises the
reserved word and refuses cleanly. This stub records the binding
for the future implementer; it does not commit fredshell to a
schedule.

## What `coproc` is

`coproc` is a bash compound-command form that launches a command
as a co-process — a background job whose stdin and stdout are
connected to the parent shell via two pipes. The pipe file
descriptors are exposed in an array variable so the parent can
read from and write to the child:

```bash
coproc mypipe { grep foo; }
echo "hello foo" >&"${mypipe[1]}"
read line <&"${mypipe[0]}"
echo "got: $line"
wait "$mypipe_PID"
```

Two syntactic forms exist:

- **Named:** `coproc NAME compound-command` — FDs land in
  `NAME[0]` and `NAME[1]`, PID lands in `NAME_PID`.
- **Anonymous:** `coproc compound-command` — FDs land in the
  default `COPROC[0]` / `COPROC[1]`, PID in `COPROC_PID`.

bash 5.x permits at most one anonymous coprocess at a time and
warns if a second is started while the first is still running.

## Why it cuts across plans

`coproc` is not localised to one subsystem:

- **PLAN_06 (parser):** `coproc` is a reserved word introducing a
  new compound-command form. The grammar gains a `CoprocCmd`
  production with an optional name and a body that is itself any
  compound command. Lexer must not treat `coproc` as a plain
  command word when it appears in command position.
- **PLAN_06 (executor):** spawning the child requires building a
  pipe pair (parent-write → child-stdin, child-stdout →
  parent-read), forking with the standard FD redirection setup,
  closing the child-side FDs in the parent, and exposing the
  parent-side FDs as an array variable.
- **PLAN_12 (jobs):** the spawned child is a background job and
  must appear in `jobs`, respond to `kill %N`, count toward
  `wait`, and be reaped by the standard SIGCHLD path. The PID is
  exposed via the `NAME_PID` scalar.
- **PLAN_02 (variables):** the FD-carrying array variable
  (`COPROC` or user-named) must be writeable by the executor at
  spawn time and readable from script code. The variable's
  lifecycle is tied to the job's: when the job exits, the
  variable is _not_ automatically cleared (matches bash), but the
  FDs in it become invalid.

A single owning document avoids re-arguing the layering at
implementation time.

## v1 behaviour (refusal)

For v1, PLAN_11 §3 (lexer/parser) recognises `coproc` as a
reserved word in command position and emits:

```text
ParseError::Unsupported {
    feature: "coproc",
    suggestion:
        "fredshell v1 does not implement `coproc`; \
         see Documents/PLAN_16_coproc.md",
}
```

The error message embeds the literal token `fredshell:coproc:`
so users can grep for it.

The PLAN_06 expansion-test sheet for reserved words must include
one case asserting this refusal (sheet path TBD when PLAN_06
Phase B sheets are written, owner subtask `06b.<TBD>`).

## When this document is drafted

This stub is upgraded to a real plan when **all** of the
following are true:

1. v1 has shipped (or v1.1 scope is being planned).
2. Real-world corpus or user reports show non-negligible
   `coproc` usage in scripts fredshell is expected to run.
3. PLAN_06 parser is stable enough to extend without churn.

At that point the drafter:

- Adds a real `## N. <section>` body covering grammar,
  executor, jobs, and variable bindings.
- Files an entry in `plan.md`'s table flipping this row from
  "stub" to "drafted".
- Adds the corresponding subtask grid (numbering TBD; suggest
  `16.0` through `16.N`).
- Adds spec sheets under `Documents/specs/features/coproc/`
  per PLAN_07.
- Coordinates with PLAN_12 to amend §6 (`jobs` / `kill`
  interaction) and with PLAN_06 to add the grammar production.

## Open questions (deferred)

These are not resolved by this stub; they are tracked here so
they do not get lost:

- **Q16.1** — Anonymous `coproc` reuse: bash warns if a second
  anonymous coprocess starts while the first is running but
  permits it. Do we match (warn-and-permit), strict-refuse
  (error), or strict-permit (no warning)?
- **Q16.2** — FD inheritance into spawned children of the
  parent: bash leaks the coproc FDs into other children unless
  `>&-` / `<&-` is used. Do we match, or close-on-exec by
  default?
- **Q16.3** — Interaction with `set -e`: if the coproc exits
  non-zero, does the parent shell error out? bash does not,
  because the coproc is a background job. Confirm and
  encode.

## Relationship to other plans

- **PLAN_06** — owns parser refusal in v1, owns full grammar
  - executor when this plan is drafted.
- **PLAN_12** — owns the job entry, the `NAME_PID` binding,
  and the reaper path.
- **PLAN_02** — owns the `COPROC` / `NAME` array binding.
- **PLAN_15** — milestones doc references this stub under
  "v1.1 candidates" once that section is populated.
