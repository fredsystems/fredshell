# AGENTS.md — fredshell Workspace

## Project Overview

fredshell is an opinionated, batteries-included Rust shell intended as a daily-driver
replacement for zsh. Edition 2024, MSRV 1.95.0.

Headline goals:

- Real bash/POSIX script compatibility via a native parser (not `bash -c` forever).
- Baked-in starship-style prompt, `lsd`-style `ls`, fzf-style fuzzy history and completion.
- First-class Nix + home-manager integration.
- Optional AI helpers (natural-language → command, error explanation) with explicit
  privacy boundaries.

The architectural plan is being designed up-front. See:

- `plan.md` — the top-level index of planning documents.
- `Documents/PLAN_*.md` — per-area design docs.
- `Documents/decisions/` — Architecture Decision Records (ADRs).

Until those documents are stable, **agents must not infer architecture from the code**;
the code is a scaffold. Read the planning documents first.

---

## Non-Negotiable Rules

- No `unsafe` code unless explicitly requested.
- Prefer clarity over cleverness.
- No public APIs without tests.
- No breaking changes without explanation.
- All observable behavior must be testable.
- Correctness always takes precedence over performance.
- AGENTS.md is authoritative — agents must not reinterpret, weaken, or "improve" rules
  found here.
- If a rule appears inconsistent with the codebase, stop and ask.
- Respect crate dependency boundaries — never introduce upward dependencies.

### Panic-Free Production Code

- `unwrap()` and `expect()` are forbidden in all production code.
- Panics must never be used to enforce invariants.
- All invariant violations must surface as typed, recoverable errors.
- The only permitted use of `unwrap()` / `expect()` is in test code (`#[cfg(test)]`
  or `tests/`), benchmarks, and `xtask`.
- Any production `unwrap` / `expect` is a correctness bug.
- Enforced via crate-level `#![deny(clippy::unwrap_used, clippy::expect_used)]`.

### Error Handling

- Errors must be explicit, typed, and structured.
- Prefer domain-specific error enums over generic errors.
- Errors must be testable.
- Do NOT use `anyhow` in library crates (`fredshell-core`, `fredshell-prompt`, and any
  future library crates). `anyhow` / `color-eyre` are acceptable in the `fredshell`
  binary entrypoint and in `xtask`.
- Error variants should encode what went wrong, not what to do.

### Dead Code Policy

- `#[allow(dead_code)]` is forbidden in production modules.
- Acceptable uses: test-only helpers, temporary refactors with an explicit TODO and
  an owning task number.
- If code exists in production, it must be reachable or intentionally gated.

---

## Crate-Specific Guidance

The workspace currently contains:

| Crate                   | Role                                                                       | `anyhow` allowed? |
| ----------------------- | -------------------------------------------------------------------------- | ----------------- |
| `fredshell`             | Binary entrypoint, CLI, line editor integration                            | Yes               |
| `fredshell-core`        | Builtins, exec, REPL state machine, parser glue                            | No                |
| `fredshell-prompt`      | Starship-style prompt renderer                                             | No                |
| `fredshell-spec-runner` | Spec harness: loads `*.case.toml`, runs against fredshell, compares output | No                |
| `xtask`                 | Build/CI orchestration                                                     | Yes               |

Additional crates will be added as the plan is executed. When a new crate is created,
this table must be updated in the same commit.

### Dependency Direction

Crates may only depend downward in the table above:

- `fredshell` → may depend on any other crate.
- `fredshell-prompt`, `fredshell-core` → may not depend on `fredshell`.
- Library crates must not depend on `xtask`.

If a change requires an upward dependency, stop and discuss — it indicates a
misplaced responsibility.

---

## Branch & Commit Workflow

### Feature Branches

- All implementation work MUST be done on feature branches, never directly on `main`.
- Branch naming convention: `task-NN/short-description` (e.g., `task-02/cli-config`,
  `task-06/test-gaps`).
- Documentation-only work uses `docs/<topic>` (e.g., `docs/planning`).
- Each major task (as defined in `plan.md`) gets its own branch.
- Subtasks within a task are committed to that task's branch.
- Branches are merged to `main` via pull request after the task is complete.
- The `no-commit-to-branch` pre-commit hook enforces this for `main` and `master`.

### Pre-Commit Hooks

- The `--no-verify` flag is **FORBIDDEN** on commits. All commits must pass pre-commit
  hooks.
- Pre-commit hooks enforce formatting, linting, and other quality checks.
- If a pre-commit hook fails, the agent MUST fix the issue and commit again — not skip
  the hook.
- Pre-commit hooks live in the nix devshell. Commit from inside `nix develop` (or with
  direnv active) so they have the tools they need.
- The only exception is if the user explicitly requests `--no-verify` for a specific
  commit.

### Commit Discipline

- Commits should be atomic: one logical change per commit.
- Commit messages follow conventional commits format (`feat:`, `fix:`, `test:`,
  `docs:`, `refactor:`, `chore:`).
- Each commit must leave `cargo test --all` passing (no broken intermediate states).

### Plan Subtask Commits

When executing a plan document (`Documents/PLAN_XX_*.md`), each completed subtask must
be committed before moving to the next. This ensures:

- Clear traceability from commits to plan subtasks.
- Safe rollback points if a later subtask introduces problems.
- Clean `git bisect` history.

**Default rule:** One commit per plan subtask. The commit message should reference the
subtask number (e.g., `refactor: 03.4 — extract Job struct from Pipeline`).

**Merging small or interleaved subtasks:** It is acceptable to combine multiple
subtasks into a single commit when:

- The subtasks are small enough that separating them adds noise rather than clarity.
- Multiple sub-agents worked on separate subtasks that modify the same files, and
  splitting the commits into fully atomic units would cause merge conflicts or broken
  intermediate states.
- The subtasks are tightly intertwined (e.g., a type change in one subtask requires a
  signature change tracked by another subtask in the same file).

When merging subtasks into one commit, the commit message must list all subtask
numbers covered (e.g., `refactor: 05.3 + 05.4 — inline expander and remove dead Word
enum`).

---

## Development Environment & Verification

### Build & Test Commands

| Command                                                    | Purpose                                              |
| ---------------------------------------------------------- | ---------------------------------------------------- |
| `cargo xtask check`                                        | Full pre-merge check: fmt + clippy + test + doc      |
| `cargo xtask pc`                                           | Pre-commit equivalent (used by mkCheck in the flake) |
| `cargo xtask coverage`                                     | Generate coverage report (lcov)                      |
| `cargo test --all`                                         | Run all unit and integration tests                   |
| `cargo clippy --all-targets --all-features -- -D warnings` | Lint with strict warnings                            |
| `cargo-machete`                                            | Detect unused dependencies                           |
| `cargo fmt --all -- --check`                               | Check formatting                                     |

### Full Verification Suite

All agents must run the following before reporting completion:

1. `cargo test --all` — all tests pass.
2. `cargo clippy --all-targets --all-features -- -D warnings` — no warnings.
3. `cargo-machete` — no unused dependencies.

If any step fails, fix the issue before proceeding.

### Tooling Notes

- Missing tools indicate an incomplete environment, not broken code.
- Agents must not work around missing tools by modifying logic.
- If additional tooling is required, stop and ask.
- Nix devshell is the preferred environment (`nix develop --impure` or `direnv allow`).

---

## Orchestrator Protocol

When acting as an orchestrator (decomposing a task into sub-agent work), follow this
protocol.

### MANDATORY: Explicit Sub-Agent Intent Scoping

**THIS IS THE SINGLE MOST IMPORTANT RULE IN THIS DOCUMENT.**

Every sub-agent prompt MUST explicitly state the agent's **permitted action class**.
A sub-agent that is told to "understand the code" will decide on its own to start
writing code, committing, and moving to the next task. This has happened repeatedly
in other projects and is unacceptable.

**Action classes** (use these exact words in every sub-agent prompt):

| Action Class       | What the agent MAY do                             | What the agent MUST NOT do                                        |
| ------------------ | ------------------------------------------------- | ----------------------------------------------------------------- |
| **READ-ONLY**      | Read files, search, analyze, report findings      | Edit files, write files, run cargo/git commands that mutate state |
| **CODE-REVIEW**    | Read files, analyze diffs, report issues          | Edit files, write files, commit, run tests                        |
| **IMPLEMENTATION** | Read + edit + write files within the stated scope | Touch files outside scope, commit, push, move to the next subtask |
| **COMMIT**         | Stage and commit specified changes                | Edit code, start new work                                         |

**Every sub-agent prompt MUST include ALL of the following:**

1. **Action class** — One of the four above, stated at the top of the prompt in
   bold/caps.
2. **Exact scope** — Which files/modules the agent may read or modify. Be specific.
3. **Deliverable** — What the agent must return. "A summary of X" or "Modified files
   A, B, C with change Y."
4. **Explicit prohibitions** — What the agent must NOT do. Always include at minimum:
   "Do NOT edit any files" (for read-only), or "Do NOT commit" (for implementation),
   or "Do NOT proceed to the next subtask" (always).
5. **Stop condition** — When to stop. "Stop after reporting findings." or "Stop after
   the verification suite passes."

**Example — read-only exploration:**

```text
ACTION CLASS: READ-ONLY. Do NOT edit, write, or create any files. Do NOT run git
commit.

Read the following files and report back:
- crates/fredshell-core/src/exec.rs
- crates/fredshell-core/src/repl.rs

Return: The full function signatures, how a typed line flows from stdin to /bin/sh,
and where built-in dispatch happens.

Stop after reporting. Do NOT write code. Do NOT proceed to implementation.
```

**Example — scoped implementation:**

```text
ACTION CLASS: IMPLEMENTATION. You may edit files listed below. Do NOT commit.
Do NOT proceed to the next subtask. Do NOT touch files outside this list.

Scope: crates/fredshell-core/src/builtins.rs

Task: Add an `export` builtin that parses `KEY=VALUE` arguments and calls
`std::env::set_var`. Return `BuiltinOutcome::Handled(0)` on success, `Handled(1)`
on parse error. Add unit tests covering: valid assignment, multiple assignments,
malformed input (no `=`), empty value.

Verification: Run `cargo test -p fredshell-core` and
`cargo clippy --all-targets -- -D warnings`.

Stop condition: Report back with files modified, summary of changes, and
verification results. Do NOT commit. Do NOT update plan documents. Do NOT start the
next subtask.
```

**If a sub-agent prompt does not contain an explicit action class and stop condition,
the orchestrator has failed.** The resulting sub-agent behavior is undefined and the
orchestrator is solely responsible for any damage.

### Task Decomposition

1. **Analyze scope** — Identify which crates and files are affected.
2. **Identify parallelism** — Tasks touching different crates or independent features
   can run in parallel. Tasks with data dependencies must be sequential.
3. **Define sub-tasks** — Each sub-task must specify:
   - Exact scope (which files/modules to modify).
   - Clear success criteria.
   - Verification steps.
   - What NOT to touch (scope boundaries).
4. **Launch sub-agents** — Use parallel sub-agents for independent work. Chain
   sequential sub-agents for dependent work.
5. **Verify integration** — After all sub-agents report, run the full verification
   suite to confirm the combined changes work together.

### Parallelism Patterns

**Crate-level:** Different sub-agents work on different crates simultaneously. Safe
when changes do not cross crate boundaries in incompatible ways.

**Task-type:** One sub-agent implements, another writes tests, another reviews. The
implementation sub-agent must complete before the test sub-agent starts if tests
depend on new APIs.

**Feature-level:** Different sub-agents implement independent features. Safe when
features do not modify the same files.

### Scope Enforcement

- Sub-agents must not modify files outside their assigned scope.
- If a sub-agent discovers it needs to modify files outside scope, it must stop and
  report back.
- The orchestrator resolves cross-scope dependencies by reassigning or sequencing
  work.

---

## Sub-Agent Execution Protocol

When executing a focused task assigned by an orchestrator (or directly by the user):

1. **Read the full task description** before starting any work.
2. **Execute only the assigned scope** — do not refactor unrelated code.
3. **Keep diffs minimal and focused** — one concern per change.
4. **Run the verification suite** before reporting completion:
   - `cargo test --all`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - `cargo-machete`
5. **Report back** with:
   - Summary of changes made.
   - Files modified.
   - Verification results.
   - Any issues or questions discovered.
6. **If blocked or unclear, stop and report** — do not guess.

---

## Multi-Step Task Protocol

For tasks with ordered dependencies (e.g., multi-phase refactors), follow this
protocol:

1. Read the entire task document before doing anything.
2. Find the first incomplete step.
3. Execute that one step and nothing else.
4. Run the verification suite — confirm it passes.
5. Update the tracking document: mark the step complete, add a brief completion note.
6. Stop and post a summary — wait for user confirmation before continuing.

**Do not execute multiple steps in one session, even if they seem small.**
**Do not proceed to the next step without explicit user confirmation.**
**Each step must leave `cargo test --all` passing.**

---

## Mandatory Testing & Benchmarking Rules

These rules apply to ALL agents and ALL implementation work going forward.

### Testing Is Mandatory

- Every new feature, bug fix, or refactor MUST include tests that cover the
  new/changed behavior.
- "It compiles and existing tests pass" is insufficient — new code must have NEW
  tests.
- If an area has no existing tests, the implementing agent must create the test
  infrastructure (test module, test helpers, fixtures) as part of the task.
- Task completion is contingent on all tests passing. A task is NOT complete until
  `cargo test --all` passes with zero failures.

### Benchmarking for Performance-Sensitive Code

Performance is a feature of an interactive shell — prompt latency, line-edit
latency, and history search latency are the most user-visible numbers in the
product.

- If changes touch the **prompt renderer**, **line editor wiring**, **parser**,
  **history search**, or **completion**, the agent MUST capture benchmark numbers
  **before and after** the change and include them in the completion report.
- If no appropriate benchmark exists for the code being changed, the agent MUST
  create a new benchmark (Criterion) as part of the task before proceeding with the
  change.
- Performance regressions must be justified and documented, or the change must be
  revised.

Benchmark recording format:

```text
| Benchmark | Before | After | Change |
| --- | --- | --- | --- |
```

#### Regression Threshold

Any regression > 15% on a relevant benchmark must be justified in the completion
report or the change must be revised. Regressions ≤ 15% may be acceptable if the
change provides correctness or maintainability benefits that outweigh the
performance cost.

### Plan Document Maintenance

- Each major task has a planning document in `Documents/PLAN_XX_*.md`.
- The agent executing a task MUST update its plan document with:
  - Subtask completion status (mark completed items, add completion dates).
  - Any deviations from the plan with justification.
  - Benchmark results if applicable.
  - Issues discovered during implementation.
- The master plan (`plan.md`) must also be updated when a major task changes status
  (started, blocked, completed).

### Pre-Existing Bugs Surfaced During a Subtask

When a sub-agent discovers a bug that is genuinely outside the current subtask's
scope:

1. The sub-agent MUST stop and report the bug. It MUST NOT fix the bug as part of
   the current subtask, even if the fix is small.
2. The orchestrator MUST file the bug as a numbered cleanup entry in the host task's
   plan section. The cleanup subtask is part of the task — not a separate task, not
   a TODO comment in code, not a tracking issue elsewhere.
3. The cleanup subtask MUST include: the surface point (commit + subtask), the bug's
   impact, the scope of the fix, suggested approach, verification criteria, and
   scheduling constraints (which later subtasks depend on it being fixed).
4. The original subtask's completion notes link to the cleanup entry by number. The
   completion notes do NOT carry the full bug description — that lives only in the
   cleanup entry.
5. Informal "known issues" sections in plan documents are NOT used. Every surfaced
   bug is either resolved or has a numbered cleanup entry.

### Task Completion Criteria

A task is complete ONLY when ALL of the following are true:

1. All subtasks in the plan document are marked complete.
2. `cargo test --all` passes.
3. `cargo clippy --all-targets --all-features -- -D warnings` passes.
4. `cargo-machete` passes.
5. Benchmarks show no unexplained regressions (for prompt / line edit / parser /
   history / completion changes).
6. Plan document is updated with completion status and notes.

---

## Working Modes

Agents may be instructed to operate in one of the following modes:

### READ_ONLY_AUDIT

- No code changes.
- Identify broken invariants, dead code, or inconsistencies.

### DESIGN_CRITIQUE

- Compare implementation to intended architecture.
- Identify architectural drift or unclear responsibilities.

### TEST_GAP_ANALYSIS

- Identify missing test coverage.
- Describe untested scenarios.

### PATCH_PROPOSAL

- Describe intended changes.
- Explain why they are correct.
- Identify risks.

### PATCH_IMPLEMENTATION

- Implement only the approved proposal.
- Keep diffs minimal.
- Update tests as needed.

---

## Testing Philosophy

Testing is first-class code.

- Every non-trivial behavior must be tested.
- Tests must document a specific invariant.
- Success and failure cases are required unless impossible.
- Bug fixes must include regression tests.

Tests must be:

- Hermetic.
- Order-independent.
- Focused on observable behavior.
- Written for humans first.

Duplication in tests is acceptable if it improves clarity.

Coverage target: 100% across library crates. The binary crate may dip below this
where coverage requires a full TTY environment that cannot be simulated reliably.

### Shell-Specific Testing Notes

A shell is harder to test than a library because most of its behavior involves PTYs,
job control, signal handling, and child processes. Standard practice in fredshell:

- **Parser** tests are pure data-in / AST-out — golden-file snapshots.
- **Builtins** are tested by calling them directly with a mock environment.
- **Execution** is tested by spawning real child processes against temp directories.
- **REPL** is tested via a pty-driven harness (one will be added when needed).
- **Bash compatibility** is tested by running the same script through fredshell and
  `bash`, then comparing stdout/stderr/exit code (see `Documents/PLAN_*_compat.md`).

---

## Code Style

- Idiomatic, clippy-clean Rust.
- Prefer small, composable functions.
- Avoid macros unless clearly justified.
- Document public types and functions.
- Prefer explicit types for clarity.
- Follow standard Rust naming conventions.

### Copyright Headers

- Every Rust source file (`.rs`) MUST begin with the following four-line header,
  verbatim, before any other content (including module-level `//!` docs,
  attributes, or `use` statements):

  ```rust
  // Copyright (C) 2026 Fred Clausen
  // Use of this source code is governed by an MIT-style
  // license that can be found in the LICENSE file or at
  // https://opensource.org/licenses/MIT.
  ```

- The year is the year the file was first created. Do NOT use a date range.
  Existing headers are not updated when the file is modified in later years —
  the year reflects original authorship.
- The header is followed by a single blank line, then the rest of the file.
- This applies to all crates, including `xtask` and test files.
- Generated files (e.g., `build.rs` output, `OUT_DIR` artifacts) are exempt.

### Numeric Conversions

- Raw `as` casts are forbidden for numeric type conversions in production code.
- Use `conv2` crate traits for all numeric conversions:
  - `ValueFrom` / `ValueInto` for lossless conversions that may fail
    (e.g., `usize` → `i32`).
  - `ApproxFrom` / `ApproxInto` with `RoundToZero` for float conversions
    (e.g., `usize` → `f32`).
  - `ConvUtil::value_as` and `ConvUtil::approx_as` for inline conversions.
- `as` casts are permitted only for:
  - Casts that are guaranteed lossless by the type system (e.g., `u8` → `u32`).
  - Test code (`#[cfg(test)]` or `tests/`).
  - Benchmark code.
  - `xtask`.
- When a conversion can fail, handle the error explicitly — do not use `.unwrap()` on
  the conversion result in production code.

`conv2` will be added to the workspace dependencies the first time a production
crate needs it.

---

## AI-Specific Rules

- Do NOT invent APIs.
- Do NOT guess shell or POSIX semantics — check the specification or compare against
  bash on a real test.
- Do NOT silently change behavior.
- Do NOT refactor unrelated code.
- Do NOT create new markdown files unless explicitly requested.
- If intent is unclear, stop and ask.

---

## Documentation Rules

- Do NOT create new markdown files by default.
- Documentation must serve a clear, durable purpose.
- Propose documentation changes before creating files.
- Avoid duplicating information already present.

### Planning Documents

- `plan.md` is the top-level index. Every plan document is linked from it.
- `Documents/PLAN_XX_<topic>.md` files own a single subsystem each. Cross-cutting
  concerns get their own document.
- `Documents/decisions/NNNN-<slug>.md` are Architecture Decision Records — short,
  immutable records of _why_ a decision was made. New decisions get new files; old
  decisions are superseded, not edited.
- Any change to a planning document must update the "Last updated" header line with
  the date and a brief note on the change.

---

## When to Stop

Stop and ask if:

- Requirements are ambiguous.
- A change would weaken invariants.
- Behavior is unclear or under-specified.
- The agent is tempted to "fill in" missing semantics.
- The agent feels unsure but thinks it can guess.
- A sub-task requires modifying files outside its assigned scope.

Correctness > completeness > speed.
