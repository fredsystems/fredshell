# PLAN_05 — Testing and the Spec Corpus

> Last updated: 2026-05-24 — cascade renumber to insert PLAN_10
> embedding (ADR 0006): §1 prose ref "PLAN_16 (milestones)" updated
> to PLAN_19; References block: PLAN_11_exec_phase_b → PLAN_12;
> PLAN_13_line_editor → PLAN_14; PLAN_12_traps_and_jobs → PLAN_13;
> PLAN_18_milestones → PLAN_19. Substance unchanged.
>
> Earlier on 2026-05-21 — 05.12 close-out: status flipped to
> `implemented`. All 12 subtasks landed; spec harness, corpus
> seeding, CI integration, and `COMPAT.md` rendering are in place.
> Phase: A. Status: implemented.
> Operationalizes ADR 0003.

This document defines how fredshell tests its own behavior. It is the
concrete realization of ADR 0003 (test-first compatibility methodology)
and the first planning document drafted in detail, because the harness
described here imposes hard constraints on every later document —
notably PLAN_02 (architecture), PLAN_06 (executor and Tier-1 builtin
surface, Phase A landed and Phase B drafting), PLAN_13 (traps and job
control), and PLAN_19 (milestones).

If something in this document conflicts with a later plan document,
this document wins until ADR 0003 is superseded.

## 1. Why testing comes before architecture

A shell is largely defined by the behavior of programs it runs. That
behavior is observable (stdout, stderr, exit status, side effects on
the filesystem and environment) but not deducible from prose. Past
experience shows that without a continuous, executable definition of
"correct," shell projects accumulate ad-hoc patches matched to whatever
the author last noticed was broken, and the answer to "what do you
support?" requires reading the source.

fredshell rejects that path. The harness exists first; it runs in CI
from day one at whatever pass-rate it produces, including 0%; and
every later design document is written knowing what it will be
measured against.

**The harness measures fredshell-as-itself, not fredshell-plus-sh.**
This is the central architectural constraint that distinguishes this
document from a generic "add an integration test directory" exercise.
See §4.2 (strict execution mode).

## 2. Test layers

fredshell uses **five** distinct test layers. Each answers a different
question. Confusing the layers (or skipping one) is a planning bug.

| Layer                 | Question answered                                                  | Owner crate(s)                   | Runtime              |
| --------------------- | ------------------------------------------------------------------ | -------------------------------- | -------------------- |
| L1 Unit tests         | Does this function compute the right value?                        | All crates                       | `cargo test`         |
| L2 Integration tests  | Do these modules compose correctly in isolation?                   | All crates, `tests/` directories | `cargo test`         |
| L3 Spec corpus        | Does fredshell produce the same observable output as bash?         | `fredshell-core` + harness crate | `cargo xtask compat` |
| L4 PTY behavior tests | Does the interactive shell behave correctly under a real terminal? | `fredshell` binary               | `cargo xtask pty`    |
| L5 Benchmarks         | Are the performance budgets met?                                   | All performance-sensitive crates | `cargo bench`        |

This document focuses on **L3 (spec corpus)** because that is the layer
ADR 0003 introduces and where the planning leverage lives. L1, L2, L4,
and L5 are addressed at the end of this document and in the respective
PLAN docs.

## 3. The spec corpus

### 3.1. What a spec test is

A spec test is a tuple:

```text
(script, env, expected_stdout, expected_stderr, expected_exit, status)
```

`script` is a bash-language program of arbitrary length, typically a
handful of lines. `env` is a description of the sandboxed execution
environment (working directory contents, environment variables,
`$HOME` location, `$PATH` value). The expected outputs are byte-exact
recordings of what real bash produces when given the same script in
the same environment. `status` is the case-status taxonomy from §12
(`pass`, `fail`, `wontfix`, `deferred:PLAN_XX`).

A spec test **matches expectation** when fredshell, given the same
script and environment, produces stdout, stderr, and exit status equal
to the recorded expected values. The harness compares the
match-result to the case's `status` field — see §12 for the full
state machine. A case with `status = "pass"` failing to match is a
regression; a case with `status = "deferred:PLAN_13"` matching is a
_positive surprise_ that the harness flags.

### 3.2. On-disk layout

Spec tests live under `tests/spec/` in a tier-1 layout:

```text
tests/spec/
  <feature>/                          e.g. parameter_expansion/
    <case>.case.toml                  test metadata + script
    <case>.stdout                     expected stdout (byte-exact)
    <case>.stderr                     expected stderr (byte-exact)
    <case>.exit                       expected exit status (one integer line)
    <case>.fs/                        optional sandbox FS skeleton, copied into $PWD
```

`<case>.case.toml` is the only file required. The other three are
optional defaults (empty stdout, empty stderr, exit 0) and are present
explicitly when non-default. The `<case>.fs/` subdirectory, when
present, is copied recursively into the sandbox before the script runs.

Example case file:

```toml
# tests/spec/parameter_expansion/default_value.case.toml

description = "Default value expansion: ${var:-default} returns default when var is unset"
tags = ["parameter-expansion", "posix-overlap"]
bash_version_min = "5.0"
status = "deferred:PLAN_12"   # see §12 for the taxonomy

[env]
HOME = "$SANDBOX/home"
PATH = "$SANDBOX/bin"
extra = { FOO = "set-value" }

script = """
echo "${UNSET_VAR:-default}"
echo "${FOO:-default}"
echo "${EMPTY_VAR:-default}"
EMPTY_VAR=""
echo "${EMPTY_VAR:-default}"
"""
```

Rationale for TOML over JSON or a custom DSL: human-editable,
multi-line strings are first-class, no escape-quoting hell, already in
the Rust ecosystem. Rationale for one-file-per-case over a directory:
a single `grep` reveals every test mentioning a feature.

### 3.3. Three corpus tiers

Per ADR 0003, the corpus is sourced from three tiers with distinct
licensing and CI policies:

#### Tier 1 — fredshell's own corpus

- Lives in-tree at `tests/spec/`.
- Hand-curated, MIT-licensed alongside the rest of the codebase.
- Organized by feature category (see §3.4).
- **Primary CI signal.** Any pass-rate regression in tier 1 fails the build.
- Coverage requirement for v1: every bash feature fredshell claims to
  support has at least one positive case (works correctly) and one
  negative case (produces the expected error and exit code).

#### Tier 2 — oils-spec corpus

- **Not vendored.** Fetched at CI time from the oils-for-unix project
  (Apache 2.0).
- Pinned to a specific commit hash in `tests/spec-external/oils.lock`.
  Refreshes are deliberate: a human updates the lock file and reviews
  the diff in pass-rate. Auto-refresh is forbidden.
- Translated at fetch time from oils' native test format into
  fredshell's `.case.toml` format by a small `xtask` adapter. The
  translation is deterministic and re-runnable.
- **Secondary CI signal.** Pass-rate is reported but does not block
  builds by default. Individual modules may be promoted to blocking
  once fredshell formally claims support for them; promotion is
  recorded in `tests/spec-external/promoted.toml`.
- Provides breadth on POSIX-overlapping behavior that fredshell's own
  authors would not think to write tests for.

#### Tier 3 — real-world script corpus

- Lives in-tree at `tests/scripts-in-the-wild/`.
- Each script is paired with a `LICENSE` file and a `PROVENANCE.md`
  entry recording where it came from and why it was selected.
- Default policy: **exclude unless licensing is unambiguous and
  compatible with MIT.** Public-domain installer fragments, scripts
  the fredshell author wrote in other projects, scripts under
  MIT/Apache/BSD licenses are eligible. GPL, proprietary, or
  ambiguously-licensed scripts are not.
- Used to validate goal G1 ("real-world bash scripts run unmodified").
- Pass-rate is reported; selected scripts may be promoted to blocking
  status in `tests/scripts-in-the-wild/blocking.toml`.

#### What is **not** in the corpus

- The bash test suite (GPL). It is reference material only. Even
  fetching it at CI time and reporting numbers from it would route GPL
  artifacts through fredshell's CI logs in ways that are legally
  uncomfortable. Avoid entirely.

### 3.4. Feature categories

Tier 1 is organized by feature category. The v1 categories are:

- `parameter_expansion` — `${var}`, `${var:-default}`, `${var/from/to}`,
  `${var:offset:length}`, `${#var}`, `${!ref}`, `${var@op}`, etc.
- `quoting` — single, double, ANSI-C (`$'…'`), locale (`$"…"`),
  backslash, here-string interactions.
- `redirection` — `>`, `>>`, `<`, `<<`, `<<<`, `&>`, `2>&1`, fd
  duplication, fd close, process substitution `<(...)` `>(...)`.
- `pipelines` — `|`, `|&`, exit-status propagation, `set -o pipefail`,
  `PIPESTATUS`.
- `control_flow` — `if`/`elif`/`else`, `case`, `for` (list, C-style,
  `for ((;;))`), `while`, `until`, `select`, `break`/`continue`,
  numeric levels.
- `arithmetic` — `$((...))`, `((...))`, `let`, integer overflow,
  bases, bitwise ops.
- `arrays` — indexed arrays, associative arrays, sparse arrays,
  `${arr[@]}` vs `${arr[*]}`, `+=` append, slicing.
- `functions` — definition syntax, `local`, `return`, recursive calls,
  function vs builtin shadowing.
- `tests_and_conditionals` — `[ ]`, `[[ ]]`, `((...))`, file tests,
  string tests, regex `=~`, glob matching.
- `expansions` — brace expansion `{a,b,c}` and `{1..10}`, tilde
  expansion, pathname expansion, command substitution `$(...)` and
  `` `...` ``.
- `traps_and_signals` — `trap`, `EXIT`, `ERR`, `DEBUG`, signal handling
  in subshells.
- `job_control` — `&`, `wait`, `fg`/`bg`, `jobs`, `$!`, `$?`.
- `builtins_tier1` — POSIX builtins: `cd`, `pwd`, `export`, `unset`,
  `set`, `shift`, `read`, `eval`, `exec`, `:`, `true`, `false`,
  `echo`, `printf`, `test`, `[`. Exhaustive list in §11.1.
- `builtins_tier2_<name>` — one category per tier-2 replacement
  builtin (one for `ls`, one for `cat`, etc.). Validates parity with
  the corresponding coreutils program. Exhaustive list in §11.2.
- `error_handling` — `set -e`, `set -u`, `set -o pipefail`, `||
return`, `trap … ERR`, edge cases bash is known to handle badly.

Each category lives in a directory under `tests/spec/`. A category may
contain subdirectories for sub-features when it grows beyond ~30
cases.

### 3.5. Corpus seeding rule

The corpus is **not** seeded by writing tests for every category
up-front. Doing so produces 200+ cases that all fail and provide no
useful signal beyond "the executor doesn't exist yet."

Instead, PLAN_05 mandates a three-step seeding:

1. **All-pass seed.** Write a `.case.toml` for every Tier-1 builtin
   that is implemented today (§11.1, rows marked `implemented`).
   Status: `pass`. These cases enforce that already-shipped behavior
   does not regress. Approximate size: 6–10 cases at PLAN_05 landing
   time (today: `cd`, `exit`, the few stubs that exist; PLAN_06
   grows this list aggressively).

2. **Deferred breadth seed.** Write one positive case per category in
   §3.4. Status: `deferred:<owning-plan>`. Total: ~16 cases. These
   all fail today. Their value is enumeration — the failing-case
   list _is_ the v1 work plan. A case marked
   `deferred:PLAN_12` becomes the single-bullet PR description for
   the corresponding PLAN_06 Phase B subtask.

3. **Stop.** Do not add more cases until the feature lands. New cases
   are added by the plan document implementing the feature (e.g.,
   PLAN_06 owns `arithmetic`, `control_flow`, and most of
   `parameter_expansion`). Each feature plan's exit criterion
   includes "tier-1 pass-rate in category X is ≥Y% with N cases."

This rule keeps PLAN_05 finishable: the corpus is ~26 cases at
landing, not 500. The corpus grows with the code, owned by whoever
ships the feature.

The status taxonomy that makes this work is in §12.

## 4. The harness

### 4.1. Crate layout

The harness is a Rust crate, `fredshell-spec-runner`, depending only
on `fredshell-core` (the public parser and executor surface) plus
standard testing/utility crates. It exposes:

- A library API for embedding the runner (used by `cargo test`
  integration and by xtask).
- A binary entry point invoked by `cargo xtask compat`.

The harness does **not** depend on the `fredshell` binary crate. It
exercises `fredshell-core` directly. This is a hard constraint and
the primary reason PLAN_02 must keep the parser and executor
separable from the REPL.

`fredshell-spec-runner` is a **library crate** per the AGENTS.md
dependency direction table (cannot depend on `xtask`). It adds a new
row to the crate-status table in AGENTS.md (`anyhow` not allowed).

### 4.2. Execution model — strict mode

This is the load-bearing section of PLAN_05.

For each spec test, the harness:

1. Creates a fresh sandbox directory under a per-run scratch root
   (`$CARGO_TARGET_TMPDIR` or equivalent).
2. Materializes `<case>.fs/` into the sandbox if present.
3. Resolves `$SANDBOX` placeholders in the `env` block to the
   absolute sandbox path.
4. Constructs an isolated `ExecEnv` in **strict mode**: empty
   environment except the variables in `env`, working directory set
   to the sandbox, `$PATH` containing only what the case requests (no
   inheritance from the host), and — critically — the executor's
   `/bin/sh` fallback is **disabled**.
5. Invokes `fredshell_core::run_source` on the script, capturing
   stdout, stderr, and exit status to byte buffers.
6. Compares against the expected outputs.
7. Cross-references the comparison result with the case's `status`
   field per §12 to decide pass / fail / unexpected-pass / setup-error.
8. Tears down the sandbox.

Sandboxes are torn down on pass and on harness errors. On test
failure the sandbox is preserved under `target/spec-failures/<case-id>/`
to support debugging.

**Strict mode is the single most important architectural addition
PLAN_05 makes.** Today's `run_source` falls back to `/bin/sh -c
<line>` for anything that is not a Tier-1 builtin (PLAN_06 §3 v0
simplification). In strict mode, the dispatcher must instead return
a typed `ExecError::NoExternalExecutor { command, reason }` so the
case fails honestly. This is what makes the pass-rate read
"fredshell-as-itself" rather than "fredshell-plus-dash."

The mechanism is owned by PLAN_05 (the implementation subtask
list below): a new field on `ExecEnv` — `pub external_command_policy:
ExternalCommandPolicy` — with two variants:

- `ExternalCommandPolicy::FallbackToSh` (default; what the REPL uses
  today).
- `ExternalCommandPolicy::Strict` (what the harness uses; refusal
  produces `NoExternalExecutor`).

The default in `ExecEnv::from_process()` is `FallbackToSh` so the
binary REPL is unchanged. The default in `ExecEnv::sandboxed()` is
`Strict` so the harness gets the correct behavior without opt-in.
PLAN_06 removes the `FallbackToSh` variant entirely once the native
executor lands; until then it remains a documented v0 affordance for
interactive use.

### 4.3. Determinism requirements

- No network access during test execution. The sandbox `$PATH` does
  not include external network tools, and the harness asserts on the
  environment shape before running. (Network _fetches_ of tier-2 oils
  happen out-of-band, in a CI prepare step, not during test runs.)
- No reliance on the host's `$HOME`, `$USER`, `$TMPDIR`, or any other
  ambient env var.
- No reliance on host filesystem layout. `/tmp`, `/etc`, etc. are not
  used; the harness rejects test cases that reference them.
- Time-dependent tests are forbidden. Cases that exercise `$SECONDS`
  or similar must mock or skip.
- Random tests are forbidden unless seeded explicitly.

### 4.4. Bash-as-oracle vs. recorded fixtures

The expected outputs (`.stdout`, `.stderr`, `.exit`) are **recorded
fixtures**, not produced live by invoking bash during the test run.
The harness does not require bash to be installed on the machine
running the tests.

A separate xtask command, `cargo xtask spec record <case>`, runs the
case under the pinned reference bash and writes the resulting
fixtures to disk. Authoring a new test case is:

1. Write the `.case.toml`.
2. Run `cargo xtask spec record <case>` to generate fixtures.
3. Review the generated fixtures (they are committed to the repo).
4. Run `cargo xtask compat <case>` to confirm fredshell matches (or
   confirm that it fails in the way the `status` field predicts).

Fixture regeneration is deliberate. A blanket `xtask spec record-all`
exists but its output must be reviewed line-by-line before commit.

### 4.5. Reference bash version

The reference bash is pinned per platform in `tests/spec/REFERENCE.md`.
The pin is established by what the fredshell nix flake delivers.

**Current state (2026-05-21, post-05.3):** the flake adds a
dedicated `nixpkgs-reference` input (pinned to
`d233902339c02a9c334e7e593de68855ad26c4cb`, the same rev the floating
`nixpkgs` input was locked at when 05.3 landed) and exposes
`packages.<system>.bashReference` and
`packages.<system>.coreutilsReference`. At this pin, the reference
toolchain is `bash-5.3p9` and `coreutils-9.10`. The devshell exports
`FREDSHELL_REFERENCE_BASH` / `FREDSHELL_REFERENCE_COREUTILS` (and
their `_VERSION` siblings, plus matching `FREDSHELL_FLOATING_*`
vars) so the spec harness consumes the pinned binaries by absolute
path and never falls back to the host bash.

The pin is documented in `tests/spec/REFERENCE.md` with a machine-
readable `[reference]` TOML block; `cargo xtask spec versions` parses
that block, verifies it matches what the devshell is serving, and
reports drift versus the floating `nixpkgs` input as advisory output
(non-fatal). See `tests/spec/REFERENCE.md` for the bump policy.

The host system bash (e.g. macOS's bash 3.2) is never used as an
oracle. Behavior differences between bash 5.3 and other versions are
out of scope for v1; if a real-world script depends on bash 5.x-only
behavior, that is expected.

### 4.6. Pass-rate reporting

After a run, the harness produces:

- A per-category pass-rate (`parameter_expansion: 42/50`).
- A per-tier pass-rate (`tier-1: 412/450, tier-2: 1100/2400, tier-3: 14/20`).
- A per-status breakdown (`pass: 412 / fail: 30 / wontfix: 8 / deferred: 187`).
- An overall pass-rate.
- A JSON report at `target/spec-report.json` for CI consumption.
- A human-readable summary on stdout.

The JSON schema is stable and versioned. CI compares the current
report against the previous main-branch report. Any tier-1 regression
fails the build. Tier-2 and tier-3 regressions warn but do not fail
unless the affected module is in the `promoted.toml` or `blocking.toml`
list.

**Unexpected passes are flagged but do not fail the build.** A case
with `status = "deferred:PLAN_12"` that suddenly matches is a signal
that PLAN_06 has made progress or that a different change incidentally
landed the feature. The harness emits a `RECLASSIFY` line in the
report; the human reviewer updates the case's status in a follow-up PR.

## 5. Architectural constraints exported to PLAN_02

These are the constraints the harness imposes on the rest of the
architecture. PLAN_02 must satisfy them.

### 5.1. Separable parser

The parser must be invocable as a pure function:

```rust
fn parse(source: &str) -> Result<Script, ParseError>
```

with no I/O, no global state, no dependency on the executor. Pure
parser tests (golden AST snapshots) are a separate L2 layer that does
not require the harness machinery.

**Status:** ✓ Phase A landed in PLAN_06 (stub parser sufficient to
drive the existing builtins and the spec harness). The real
bash-compatible parser is PLAN_06 Phase B.

### 5.2. Sandboxable executor with strict mode and capture

The executor must accept an explicit environment. The shape mandated
by PLAN_05 is:

```rust
pub struct ExecEnv {
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub last_status: i32,
    pub stdout: Box<dyn Write + Send>,
    pub stderr: Box<dyn Write + Send>,
    pub external_command_policy: ExternalCommandPolicy,
    // … extension points for tier-2 builtin overrides, signal mask,
    // path resolution policy, ShellState (functions, aliases) —
    // added by PLAN_06 Phase B; job-table slot filled in by PLAN_13.
}

pub enum ExternalCommandPolicy {
    FallbackToSh,
    Strict,
}

pub fn run_source(source: &str, env: &mut ExecEnv)
    -> Result<RunResult, RunError>;
```

The concrete signature evolves with PLAN_06 Phase B, but the shape is
non-negotiable: no implicit globals, no calls to `std::env::var` at
the leaves, no `println!` macros — every byte of output goes through
`env.stdout` or `env.stderr`. PLAN_02 §4 owns the exact API.

**Status:** PLAN*06 Phase A landed `ExecEnv { cwd, env, last_status }`
plus the `Capture::Buffers` mechanism on the dispatcher as a sibling
parameter. PLAN_05 requires moving capture \_onto* `ExecEnv` itself
and adding the `external_command_policy` field. This is the first
implementation subtask in PLAN_05 — see §13.

`stdin` is reserved on `ExecEnv` for PLAN_06 (read builtin); v1 of
the harness writes empty stdin and tests that consume stdin are
deferred per §10.

### 5.3. Batch-mode entry point

A non-interactive batch entry point exists from day one. The harness
calls it. The REPL is built **on top of** this entry point, not
alongside it. There is never a moment in the development history when
the only way to run a script through fredshell is via the line
editor.

**Status:** ✓ implemented in PLAN_06 (06a.6: REPL routes through
`run_source`).

### 5.4. Builtin dispatch must be testable

Tier-2 builtins must be invocable directly from the harness without
spinning up a full REPL or process. Each tier-2 builtin exposes an
`invoke(env, args) -> ExitStatus` method that the spec test for that
builtin calls.

**Status:** ✓ trait surface (`Tier2Builtin`, `Tier2Ctx`, `Tier2Error`)
landed in PLAN_06 Phase A (subtask 06a.4). No implementations yet;
PLAN_06 Phase B owns the Tier-1 inventory and dispatch.

## 6. Other test layers

### 6.1. L1 — Unit tests

Standard `#[cfg(test)] mod tests` in every module. Required for every
non-trivial function. Hermetic. Order-independent. Coverage target
100% across library crates per AGENTS.md.

### 6.2. L2 — Integration tests

`tests/` directories within each crate. Used for cross-module
behavior that does not warrant the full spec corpus apparatus —
e.g., parser AST snapshots, builtin unit-level behavior with mock
environments, prompt renderer with a mock terminal.

### 6.3. L4 — PTY behavior tests

A separate harness exercises the interactive shell through a real
PTY. Owns:

- Line editor behavior (key sequences in → expected screen contents out).
- Signal handling (Ctrl-C, Ctrl-Z, SIGWINCH).
- Job control end-to-end.
- Bracketed paste, kitty keyboard, terminal feature negotiation.

This layer is **not** ready in milestone 1. It requires the line
editor to exist. Its design is owned by PLAN_14 (interactive UX)
with input from PLAN_04 (terminal I/O). It is mentioned here because
shell behavior in interactive mode genuinely cannot be exercised by
L3, and deferring the design entirely would be a mistake.

### 6.4. L5 — Benchmarks

Criterion benchmarks at `benches/` per AGENTS.md performance
discipline. Cover the prompt renderer, parser, line edit dispatch,
and history search. Required before/after numbers for any change to
those areas. PLAN_06 06a.7 seeded the executor bench
(`exec_roundtrip`); PLAN_06 grows it.

## 7. CI integration

### 7.1. xtask commands

- `cargo xtask compat` — run the full spec corpus, emit report.
- `cargo xtask compat <category>` — run a single category.
- `cargo xtask compat --tier 1` — restrict to a tier.
- `cargo xtask compat --status deferred:PLAN_12` — restrict to a
  status (useful for "show me what 06b unblocks").
- `cargo xtask spec record <case>` — regenerate fixtures for one
  case.
- `cargo xtask spec record-all` — regenerate everything (review
  required).
- `cargo xtask spec fetch-oils` — refresh the tier-2 oils-spec lock.
- `cargo xtask spec lint` — validate `.case.toml` schema, check for
  orphaned fixtures, check provenance entries for tier-3, verify the
  §11 vendoring table against `bash -c 'enable -a'` from the pinned
  reference bash (so the table flags drift if bash adds a builtin).

### 7.2. CI workflow

The compat job runs on every PR and every push to main. It:

1. Restores cached external corpora (oils-spec) keyed on the lock file.
2. Runs `cargo xtask compat` with the full corpus.
3. Compares the JSON report against the last main-branch report
   (downloaded from CI artifacts).
4. **Fails the build if:**
   - A tier-1 case with `status = "pass"` did not match
     expectation (true regression).
   - A promoted tier-2 case regressed.
   - A blocking tier-3 case regressed.
5. **Warns but does not fail if:**
   - A case with `status = "deferred:PLAN_XX"` matched expectation
     (positive surprise — `RECLASSIFY` line in report).
   - Tier-2/tier-3 non-promoted cases regressed.
6. Uploads the report and any preserved sandbox failures as CI
   artifacts.

### 7.3. Pass-rate visibility

The current pass-rate per category is rendered to a generated
`COMPAT.md` file at the repo root by `cargo xtask compat --update-readme`.
This file is the user-visible answer to "what does fredshell
support?" and is regenerated on every main-branch build.

`COMPAT.md` has two sections:

- **Implemented**, sourced from `status = "pass"` cases. Tells users
  what works.
- **In progress**, sourced from `status = "deferred:PLAN_XX"` cases,
  grouped by plan document. Tells users what is being worked on next
  and where to follow it.

`status = "wontfix"` cases do not appear in `COMPAT.md`; they appear
only in §11 as a documented non-goal.

## 8. Success metrics

These metrics are targets, not commitments. Phase B documents may
refine them once the baseline is measured.

### 8.1. v1 targets (tier 1)

- ≥ 95% pass on `parameter_expansion`, `quoting`, `redirection`,
  `pipelines`, `control_flow`, `arithmetic`, `expansions`,
  `tests_and_conditionals`.
- ≥ 90% pass on `arrays`, `functions`, `error_handling`.
- ≥ 80% pass on `traps_and_signals`, `job_control`.
- 100% pass on `builtins_tier1`. POSIX builtins are not optional.

### 8.2. v1 targets (tier 3 real-world scripts)

- ≥ 10 hand-selected real-world scripts run unmodified.
- Selection skews toward installers and CI helpers (high-impact,
  broad feature surface).

### 8.3. v1 targets (tier 2 oils-spec)

- Aspirational only. Reported, not committed. A baseline number is
  established once the corpus is fetched; v1 commits to "no
  regression from baseline" rather than an absolute floor.

### 8.4. PLAN_05 landing targets (immediate)

What the corpus must look like the day PLAN_05 is marked
`implemented`:

- ~6–10 tier-1 cases with `status = "pass"`, exercising every Tier-1
  builtin from §11.1 that is currently implemented. Pass-rate on
  these: 100%.
- ~16 tier-1 cases (one per §3.4 category) with `status =
"deferred:<owning-plan>"`. Pass-rate on these: 0%, expected.
- No tier-2 cases (oils-spec not yet fetched).
- No tier-3 cases (real-world script selection deferred).
- `cargo xtask compat` produces a valid JSON report.
- CI runs the compat job on every PR.
- `COMPAT.md` is generated and committed.

This is the v0 yardstick. Everything else grows from here, owned by
the plan document implementing the feature.

## 9. License hygiene

Restating the boundaries:

- **fredshell tier-1 corpus**: MIT, alongside fredshell.
- **oils-spec tier-2 corpus**: Apache 2.0. Fetched at CI time, not
  vendored. Translation adapter is MIT. The fetched cache lives in
  `target/` and is never committed.
- **Real-world tier-3 scripts**: per-script license review. MIT,
  Apache 2.0, BSD, or public domain only. Each script has a sibling
  `LICENSE` file and a `PROVENANCE.md` entry.
- **Bash test suite**: GPL. Reference only. No fetching, no
  vendoring, no derivation.

`cargo xtask spec lint` enforces the per-script license file
requirement for tier 3 and the absence of any tier-1 file under a
non-MIT license header.

## 10. Open questions

These are unresolved as of this rewrite. Resolutions land in this
document, not in side notes.

### 10.1. Resolved by this rewrite

- ~~Strict-mode vs. fallback execution.~~ Resolved: §4.2. Harness
  uses `ExternalCommandPolicy::Strict`; binary keeps `FallbackToSh`
  until PLAN_06 removes it.
- ~~Capture mechanism — `ExecEnv` field vs. sibling function.~~
  Resolved: §5.2. Capture moves onto `ExecEnv`.
- ~~Reference bash version.~~ Resolved: §4.5. v1 reference is
  `bash-5.3p9` + `coreutils-9.10`, pinned via the `nixpkgs-reference`
  flake input (05.3) and documented in `tests/spec/REFERENCE.md`.
- ~~Vendoring scope.~~ Resolved: §11 is exhaustive for bash
  builtins; curated for coreutils with explicit dispositions.
- ~~Case-status taxonomy.~~ Resolved: §12.
- ~~Corpus seeding size.~~ Resolved: §3.5. ~26 cases at landing,
  then stop.

### 10.2. Still open

- **TOML vs. KDL for `.case.toml`.** TOML is the default; KDL is a
  candidate if multi-line ergonomics prove painful in practice.
  Decision deferred until the first 50 cases exist.
- **Per-test bash version pinning.** Some cases will eventually need
  bash-version-specific assertions. Whether to express that in the
  case file (`bash_version_min`) or via category-level policy is
  open. The case-file approach is provisional.
- **Async harness.** The harness is synchronous in v1. If spec-corpus
  size grows past ~5000 cases the runtime starts to matter; a
  parallel/rayon-based runner becomes worth building. Out of scope
  for v1.
- **Coverage of stdin-driven scripts.** Cases that consume stdin
  need an additional field in `.case.toml` (`stdin = "…"`). Schema
  slot reserved; semantics deferred to PLAN_06 (when `read` lands).
- **Promotion criteria for tier-2 modules.** What gates an oils-spec
  module being moved from "reported" to "blocking"? Provisional
  answer: ≥ 90% pass on that module, sustained over two months,
  with an explicit ADR-style decision recorded.
- **`bash_version_min` enforcement.** The case file accepts the
  field; the harness's current behavior is to ignore it. When and
  how to enforce (skip the case? run against a different reference
  bash?) is deferred until a case actually needs it.

## 11. Vendoring scope — what fredshell will and will not implement

This section is the auditable inventory of every command fredshell
might implement natively. It is the source of truth for the `status
= "deferred:PLAN_XX"` annotations in `.case.toml` files: each
deferred case names the plan that owns the work, and the work itself
appears as a row here.

### 11.1. Bash builtins — exhaustive

Sourced from `bash -c 'enable -a' | awk '{print $NF}' | sort` on the
pinned reference bash (currently bash 5.3.9 from `nixos-unstable`).
All 57 builtins. The harness's `xtask spec lint` re-runs this command
during linting and flags drift if the set changes.

Disposition vocabulary:

- **Vendor:Tier-1** — fredshell will ship a native implementation
  inside `fredshell-core`. Required for v1.
- **PATH-resolve** — fredshell will resolve via `$PATH` (i.e., not a
  builtin in fredshell). Some bash builtins are also coreutils
  external commands (`echo`, `printf`, `kill`, `test`, `[`,
  `true`, `false`, `pwd`). When bash and POSIX both define them as
  builtins, fredshell follows bash.
- **Defer** — implemented later than v1; row notes the rough phase.
- **Never** — explicit non-goal; case for this builtin marked
  `wontfix`.

Every row's `Plan` column is the owning plan document. Forward-
looking references (PLAN_13 etc.) are intentional — the table
forces those documents to exist.

| Builtin     | Category   | Disposition   | Plan        | Notes                                             |
| ----------- | ---------- | ------------- | ----------- | ------------------------------------------------- |
| `:`         | POSIX      | Vendor:Tier-1 | PLAN_06     | No-op; sets `$?` to 0.                            |
| `.`         | POSIX      | Vendor:Tier-1 | PLAN_06     | Source a file; reentrant call into parser.        |
| `[`         | POSIX      | Vendor:Tier-1 | PLAN_06     | Synonym for `test`.                               |
| `alias`     | bash       | Vendor:Tier-1 | PLAN_06     | Requires alias table on `ShellState`.             |
| `bg`        | POSIX      | Vendor:Tier-1 | PLAN_13     | Requires job table.                               |
| `break`     | POSIX      | Vendor:Tier-1 | PLAN_06     | Control flow; integer level.                      |
| `builtin`   | bash       | Vendor:Tier-1 | PLAN_06     | Bypass aliases/functions.                         |
| `caller`    | bash       | Vendor:Tier-1 | PLAN_13     | Function call stack introspection.                |
| `cd`        | POSIX      | Vendor:Tier-1 | implemented | PLAN_06 — partial; full flag surface TBD.         |
| `command`   | POSIX      | Vendor:Tier-1 | PLAN_06     | Bypass functions and aliases.                     |
| `continue`  | POSIX      | Vendor:Tier-1 | PLAN_06     | Control flow.                                     |
| `declare`   | bash       | Vendor:Tier-1 | PLAN_06     | Synonym `typeset`; requires scoped variables.     |
| `dirs`      | bash       | Vendor:Tier-1 | PLAN_13     | Directory stack.                                  |
| `disown`    | bash       | Vendor:Tier-1 | PLAN_13     | Job-table edit.                                   |
| `echo`      | POSIX/bash | Vendor:Tier-1 | PLAN_06     | bash quirks: `-n`, `-e`, `-E`.                    |
| `enable`    | bash       | Vendor:Tier-1 | PLAN_06     | Toggle builtin status.                            |
| `eval`      | POSIX      | Vendor:Tier-1 | PLAN_06     | Reentrant parse + execute.                        |
| `exec`      | POSIX      | Vendor:Tier-1 | PLAN_06     | Replace process; fd manipulation.                 |
| `exit`      | POSIX      | Vendor:Tier-1 | implemented | PLAN_06 — full.                                   |
| `export`    | POSIX      | Vendor:Tier-1 | PLAN_06     | Mark variable for environment.                    |
| `false`     | POSIX      | Vendor:Tier-1 | PLAN_06     | Trivial; sets `$?` to 1.                          |
| `fc`        | bash       | Vendor:Tier-1 | PLAN_14     | History subsystem; owned by line editor.          |
| `fg`        | POSIX      | Vendor:Tier-1 | PLAN_13     | Job control.                                      |
| `getopts`   | POSIX      | Vendor:Tier-1 | PLAN_13     | Argument parsing for scripts.                     |
| `hash`      | POSIX      | Vendor:Tier-1 | PLAN_13     | PATH lookup cache.                                |
| `help`      | bash       | Vendor:Tier-1 | PLAN_13     | Self-documenting; sourced from this table.        |
| `history`   | bash       | Vendor:Tier-1 | PLAN_14     | Owned by line editor.                             |
| `jobs`      | POSIX      | Vendor:Tier-1 | PLAN_13     | Job table.                                        |
| `kill`      | POSIX      | Vendor:Tier-1 | PLAN_13     | Signal delivery; also a coreutils binary.         |
| `let`       | bash       | Vendor:Tier-1 | PLAN_06     | Arithmetic eval; ties to `((...))`.               |
| `local`     | bash       | Vendor:Tier-1 | PLAN_06     | Scoped variable; function scope.                  |
| `logout`    | bash       | Vendor:Tier-1 | PLAN_13     | Login-shell-only exit.                            |
| `mapfile`   | bash       | Vendor:Tier-1 | PLAN_13     | Read into array; synonym `readarray`.             |
| `popd`      | bash       | Vendor:Tier-1 | PLAN_13     | Directory stack.                                  |
| `printf`    | POSIX      | Vendor:Tier-1 | PLAN_13     | Non-trivial format string parser.                 |
| `pushd`     | bash       | Vendor:Tier-1 | PLAN_13     | Directory stack.                                  |
| `pwd`       | POSIX      | Vendor:Tier-1 | PLAN_06     | Trivial; `-L` / `-P` flags.                       |
| `read`      | POSIX      | Vendor:Tier-1 | PLAN_13     | Line-editor implications; needs `ExecEnv::stdin`. |
| `readarray` | bash       | Vendor:Tier-1 | PLAN_13     | Synonym `mapfile`.                                |
| `readonly`  | POSIX      | Vendor:Tier-1 | PLAN_06     | Variable attribute.                               |
| `return`    | POSIX      | Vendor:Tier-1 | PLAN_06     | Function exit.                                    |
| `set`       | POSIX      | Vendor:Tier-1 | PLAN_06     | Shell-options table.                              |
| `shift`     | POSIX      | Vendor:Tier-1 | PLAN_06     | Positional-args manipulation.                     |
| `shopt`     | bash       | Vendor:Tier-1 | PLAN_06     | Bash-specific options table.                      |
| `source`    | bash       | Vendor:Tier-1 | PLAN_06     | Synonym `.`.                                      |
| `suspend`   | bash       | Vendor:Tier-1 | PLAN_13     | Self-SIGTSTP.                                     |
| `test`      | POSIX      | Vendor:Tier-1 | PLAN_06     | Huge surface — file tests, string tests, etc.     |
| `times`     | POSIX      | Vendor:Tier-1 | PLAN_13     | Process times; thin libc.                         |
| `trap`      | POSIX      | Vendor:Tier-1 | PLAN_13     | Signal disposition table.                         |
| `true`      | POSIX      | Vendor:Tier-1 | PLAN_06     | Trivial.                                          |
| `type`      | POSIX      | Vendor:Tier-1 | PLAN_13     | Identify command kind; uses §11 table.            |
| `typeset`   | bash       | Vendor:Tier-1 | PLAN_06     | Synonym `declare`.                                |
| `ulimit`    | POSIX      | Vendor:Tier-1 | PLAN_13     | `setrlimit` wrapper.                              |
| `umask`     | POSIX      | Vendor:Tier-1 | PLAN_13     | `umask` syscall wrapper.                          |
| `unalias`   | bash       | Vendor:Tier-1 | PLAN_06     | Alias-table edit.                                 |
| `unset`     | POSIX      | Vendor:Tier-1 | PLAN_06     | Variable / function removal.                      |
| `wait`      | POSIX      | Vendor:Tier-1 | PLAN_13     | Wait for jobs/pids.                               |

#### Bash reserved words (not builtins, but exercised by spec tests)

The harness exercises these through the parser/executor path, not via
builtin dispatch:

`!`, `case`, `coproc`, `do`, `done`, `elif`, `else`, `esac`, `fi`,
`for`, `function`, `if`, `in`, `select`, `then`, `time`, `until`,
`while`, `{`, `}`, `[[`, `]]`, `((`, `))`.

All are owned by PLAN_06. `coproc` and `time` are open questions:
`coproc` is a parser-level construct with non-trivial semantics
(may be deferred to a later phase); `time` is a keyword-level builtin
that the parser must recognize.

### 11.2. Coreutils 9.10 — exhaustive

Sourced from `ls $(nix-store -q --references $(which coreutils))/bin`
on the pinned reference coreutils. All 109 binaries listed below.

Disposition vocabulary inherits from §11.1, with one addition:

- **Vendor:Tier-2** — fredshell ships a native replacement in a
  dedicated crate (e.g., `fredshell-coreutils` or one crate per
  binary; layout TBD by `PLAN_06`). PATH-resolution is still
  permitted; `command -p` always reaches the external. Vendored
  variants are the _default_ dispatch when the binary name is
  invoked unadorned.
- **Vendor:Tier-2-partial** — fredshell vendors a _subset_ of flags
  matching common usage; uncommon flags fall through to PATH. Used
  for binaries with large but bimodal flag surfaces (e.g., `find`,
  `sed`).

| Command     | Disposition   | Plan    | Notes                                                      |
| ----------- | ------------- | ------- | ---------------------------------------------------------- |
| `[`         | (see §11.1)   |         | Builtin in bash; coreutils provides for non-shell callers. |
| `b2sum`     | PATH-resolve  |         | Crypto sum; rare.                                          |
| `base32`    | PATH-resolve  |         |                                                            |
| `base64`    | PATH-resolve  |         |                                                            |
| `basename`  | Vendor:Tier-2 | PLAN_06 | Trivial; hot in scripts.                                   |
| `basenc`    | PATH-resolve  |         | Multi-encoding base.                                       |
| `cat`       | Vendor:Tier-2 | PLAN_06 | Trivial; removes a fork on `cat file`.                     |
| `chcon`     | PATH-resolve  |         | SELinux; platform-specific.                                |
| `chgrp`     | Vendor:Tier-2 | PLAN_06 | Filesystem core.                                           |
| `chmod`     | Vendor:Tier-2 | PLAN_06 | Filesystem core.                                           |
| `chown`     | Vendor:Tier-2 | PLAN_06 | Filesystem core.                                           |
| `chroot`    | PATH-resolve  |         | Rare; privilege-sensitive.                                 |
| `cksum`     | PATH-resolve  |         |                                                            |
| `comm`      | PATH-resolve  |         | Niche set-comparison tool.                                 |
| `coreutils` | PATH-resolve  |         | Multi-call binary; not a target.                           |
| `cp`        | Vendor:Tier-2 | PLAN_06 | Filesystem core.                                           |
| `csplit`    | PATH-resolve  |         |                                                            |
| `cut`       | Vendor:Tier-2 | PLAN_06 | Hot in pipelines.                                          |
| `date`      | PATH-resolve  |         | GNU date's format/parsing is a separate world.             |
| `dd`        | PATH-resolve  |         | Sharp edges; low value to vendor.                          |
| `df`        | PATH-resolve  |         | Mount-aware; libc-heavy.                                   |
| `dir`       | PATH-resolve  |         | `ls` alias; we vendor `ls`.                                |
| `dircolors` | PATH-resolve  |         | Config helper for `ls`.                                    |
| `dirname`   | Vendor:Tier-2 | PLAN_06 | Trivial; hot in scripts.                                   |
| `du`        | PATH-resolve  |         | Filesystem traversal — defer.                              |
| `echo`      | (see §11.1)   |         | Bash builtin.                                              |
| `env`       | Vendor:Tier-2 | PLAN_06 | Trivial; hot in shebangs.                                  |
| `expand`    | PATH-resolve  |         |                                                            |
| `expr`      | PATH-resolve  |         | Shell handles most expr cases natively.                    |
| `factor`    | PATH-resolve  |         | Niche.                                                     |
| `false`     | (see §11.1)   |         | Bash builtin.                                              |
| `fmt`       | PATH-resolve  |         |                                                            |
| `fold`      | PATH-resolve  |         |                                                            |
| `groups`    | PATH-resolve  |         |                                                            |
| `head`      | Vendor:Tier-2 | PLAN_06 | Hot in pipelines.                                          |
| `hostid`    | PATH-resolve  |         |                                                            |
| `id`        | PATH-resolve  |         | Defer; minor UX win.                                       |
| `install`   | PATH-resolve  |         |                                                            |
| `join`      | PATH-resolve  |         |                                                            |
| `kill`      | (see §11.1)   |         | Bash builtin; coreutils version is for non-shell callers.  |
| `link`      | PATH-resolve  |         |                                                            |
| `ln`        | Vendor:Tier-2 | PLAN_06 | Filesystem core.                                           |
| `logname`   | PATH-resolve  |         |                                                            |
| `ls`        | Vendor:Tier-2 | PLAN_06 | `lsd`-style output is a stated v1 goal.                    |
| `md5sum`    | PATH-resolve  |         | Crypto sum.                                                |
| `mkdir`     | Vendor:Tier-2 | PLAN_06 | Filesystem core.                                           |
| `mkfifo`    | PATH-resolve  |         | Rare.                                                      |
| `mknod`     | PATH-resolve  |         | Rare; privilege-sensitive.                                 |
| `mktemp`    | Vendor:Tier-2 | PLAN_06 | Hot in scripts.                                            |
| `mv`        | Vendor:Tier-2 | PLAN_06 | Filesystem core.                                           |
| `nice`      | PATH-resolve  |         |                                                            |
| `nl`        | PATH-resolve  |         |                                                            |
| `nohup`     | PATH-resolve  |         |                                                            |
| `nproc`     | PATH-resolve  |         |                                                            |
| `numfmt`    | PATH-resolve  |         |                                                            |
| `od`        | PATH-resolve  |         |                                                            |
| `paste`     | PATH-resolve  |         |                                                            |
| `pathchk`   | PATH-resolve  |         |                                                            |
| `pinky`     | PATH-resolve  |         |                                                            |
| `pr`        | PATH-resolve  |         |                                                            |
| `printenv`  | Vendor:Tier-2 | PLAN_06 | Trivial.                                                   |
| `printf`    | (see §11.1)   |         | Bash builtin.                                              |
| `ptx`       | PATH-resolve  |         |                                                            |
| `pwd`       | (see §11.1)   |         | Bash builtin.                                              |
| `readlink`  | Vendor:Tier-2 | PLAN_06 | Filesystem hot path.                                       |
| `realpath`  | Vendor:Tier-2 | PLAN_06 | Filesystem hot path.                                       |
| `rm`        | Vendor:Tier-2 | PLAN_06 | Filesystem core.                                           |
| `rmdir`     | Vendor:Tier-2 | PLAN_06 | Filesystem core.                                           |
| `runcon`    | PATH-resolve  |         | SELinux.                                                   |
| `seq`       | Vendor:Tier-2 | PLAN_06 | Trivial; hot in for-loops.                                 |
| `sha1sum`   | PATH-resolve  |         | Crypto sum.                                                |
| `sha224sum` | PATH-resolve  |         |                                                            |
| `sha256sum` | PATH-resolve  |         |                                                            |
| `sha384sum` | PATH-resolve  |         |                                                            |
| `sha512sum` | PATH-resolve  |         |                                                            |
| `shred`     | PATH-resolve  |         |                                                            |
| `shuf`      | PATH-resolve  |         |                                                            |
| `sleep`     | Vendor:Tier-2 | PLAN_06 | Trivial.                                                   |
| `sort`      | Vendor:Tier-2 | PLAN_06 | Hot in pipelines.                                          |
| `split`     | PATH-resolve  |         |                                                            |
| `stat`      | PATH-resolve  |         | Platform-specific quirks.                                  |
| `stdbuf`    | PATH-resolve  |         |                                                            |
| `stty`      | PATH-resolve  |         | Terminal control — PLAN_04 owns the interactive side.      |
| `sum`       | PATH-resolve  |         |                                                            |
| `sync`      | PATH-resolve  |         |                                                            |
| `tac`       | PATH-resolve  |         |                                                            |
| `tail`      | Vendor:Tier-2 | PLAN_06 | Hot in pipelines and log-following.                        |
| `tee`       | Vendor:Tier-2 | PLAN_06 | Small; hot in pipelines.                                   |
| `test`      | (see §11.1)   |         | Bash builtin.                                              |
| `timeout`   | PATH-resolve  |         | Wraps execve+SIGALRM.                                      |
| `touch`     | Vendor:Tier-2 | PLAN_06 | Filesystem core.                                           |
| `tr`        | Vendor:Tier-2 | PLAN_06 | Hot in pipelines.                                          |
| `true`      | (see §11.1)   |         | Bash builtin.                                              |
| `truncate`  | PATH-resolve  |         |                                                            |
| `tsort`     | PATH-resolve  |         |                                                            |
| `tty`       | PATH-resolve  |         |                                                            |
| `uname`     | PATH-resolve  |         | Hot but the GNU output is canonical.                       |
| `unexpand`  | PATH-resolve  |         |                                                            |
| `uniq`      | Vendor:Tier-2 | PLAN_06 | Hot in pipelines.                                          |
| `unlink`    | PATH-resolve  |         |                                                            |
| `uptime`    | PATH-resolve  |         |                                                            |
| `users`     | PATH-resolve  |         |                                                            |
| `vdir`      | PATH-resolve  |         | `ls -l` alias.                                             |
| `wc`        | Vendor:Tier-2 | PLAN_06 | Hot in pipelines.                                          |
| `who`       | PATH-resolve  |         |                                                            |
| `whoami`    | PATH-resolve  |         |                                                            |
| `yes`       | Vendor:Tier-2 | PLAN_06 | Trivial.                                                   |

### 11.3. Non-coreutils externals considered

These are not part of coreutils but are heavy hitters worth a row:

| Command                 | Disposition               | Plan    | Notes                                                            |
| ----------------------- | ------------------------- | ------- | ---------------------------------------------------------------- |
| `find`                  | Vendor:Tier-2-partial     | PLAN_06 | `fd`-style UX; common flags vendored, exotic flags PATH-resolve. |
| `xargs`                 | Vendor:Tier-2             | PLAN_06 | Small; hot.                                                      |
| `grep`                  | Vendor:Tier-2-partial     | PLAN_06 | `ripgrep`-style UX; basic flag surface.                          |
| `sed`                   | Vendor:Tier-2-partial     | PLAN_06 | Most common transforms only.                                     |
| `awk`                   | Never                     |         | Full POSIX awk is its own interpreter; out of scope.             |
| `git`                   | PATH-resolve              |         | Provided by user environment.                                    |
| `mount` / `umount`      | PATH-resolve              |         | util-linux; privilege-sensitive.                                 |
| `ps` / `top`            | PATH-resolve              |         | procps; OS-specific.                                             |
| `tar` / `gzip` / `zstd` | PATH-resolve              |         | Dedicated tools.                                                 |
| `which`                 | (use bash `type` builtin) |         | `type -P` covers it.                                             |

### 11.4. Explicitly out of scope

- The full GNU coreutils manifest beyond §11.2 rows marked
  Vendor:Tier-2 or Vendor:Tier-2-partial. Default is PATH-resolve.
- `util-linux`, `procps`, `iproute2`, `systemd-*`. Never vendored.
- Vendor-specific GNU extensions on commands fredshell _does_ vendor
  (e.g., a Tier-2 `ls` ships its own flag surface; it does not
  promise `ls --time-style=full-iso` compatibility).
- Anything not listed in §11.1, §11.2, or §11.3. New entries require
  an ADR or a plan-document update.

## 12. Case status taxonomy

Every `.case.toml` has a required `status` field. The harness
interprets it as follows:

| Status             | Meaning                                            | CI behavior                                                                   |
| ------------------ | -------------------------------------------------- | ----------------------------------------------------------------------------- |
| `pass`             | Case is expected to match expectation today.       | Mismatch fails the build (regression).                                        |
| `fail`             | Case is expected NOT to match. Tracked for parity. | Mismatch is fine. Match flags `RECLASSIFY`.                                   |
| `wontfix`          | Case documents an intentional non-goal.            | Excluded from pass-rate. Match flags `RECLASSIFY`.                            |
| `deferred:PLAN_XX` | Case is expected to match once PLAN_XX lands.      | Mismatch is fine. Match flags `RECLASSIFY`. Counted in "pending work" bucket. |

### 12.1. The `RECLASSIFY` signal

Any case that produces a result inconsistent with its declared status
emits a `RECLASSIFY` line in the harness report. The PR author is
expected to update the case's status in the same PR or open a
follow-up. CI warns but does not fail.

This is the mechanism that keeps the corpus honest as features land:
when PLAN_06 implements `${var:-default}`, every
`deferred:PLAN_12` case for that feature flips to a `RECLASSIFY`
notice; the 06b PR updates them to `pass` and the v1 pass-rate ticks
up by a measurable amount.

### 12.2. Status field is the work plan

`cargo xtask compat --status deferred:PLAN_12` lists every case
PLAN_06 is responsible for. The list is the literal task inventory
for that document. When the list is empty (every `deferred:PLAN_12`
case has been reclassified to `pass`), PLAN_06's exit criterion is
met.

This is what makes "the test suite tells us what to build" concrete:
the failing-case list is the work plan, queryable from the command
line.

### 12.3. Status migration rules

- `deferred:PLAN_XX` → `pass` requires a `cargo xtask compat
--status pass` run showing the case matches.
- `pass` → anything else (including `fail` or `deferred`) requires an
  explicit note in the PR — pass-rate regressions are
  load-bearing.
- `fail` → `pass` is the normal feature-landing flow.
- `wontfix` is rare and requires an ADR or a §11 row pointing at
  "Never" or "PATH-resolve."

## 13. Implementation plan (subtasks)

This section is binding for the implementer. Each subtask is one
commit per AGENTS.md.

PLAN_05 is **not** split into 05a/05b/05c on first reading. If the
total subtask count exceeds ~10, the document will be split at that
point. For now it is one task.

### 13.1. Preconditions (before any 05 subtask)

- PLAN_06 is implemented (✓ merged 2026-05-21 as `3ebec50`).
- `ExecEnv` exists with `cwd`, `env`, `last_status` fields.

### 13.2. Subtasks

- **05.1** Add `external_command_policy: ExternalCommandPolicy` to
  `ExecEnv`. `ExternalCommandPolicy::FallbackToSh` is the default
  for `ExecEnv::from_process()`; `ExternalCommandPolicy::Strict` is
  the default for `ExecEnv::sandboxed()`. Update `exec/mod.rs` to
  return `ExecError::NoExternalExecutor { command }` instead of
  spawning `/bin/sh` when policy is `Strict`. Tests for both code
  paths. The binary REPL is unaffected.

- **05.2** Move output capture from `exec/testing.rs::Capture::Buffers`
  onto `ExecEnv` as `stdout` and `stderr` fields
  (`Box<dyn Write + Send>`). `ExecEnv::sandboxed` accepts a builder
  that lets the harness inject `Vec<u8>` sinks. The existing
  `Capture::Inherit` becomes the default (writes to host
  stdout/stderr). The PLAN_06 `exec/testing.rs` helper is rewritten
  on top of the new `ExecEnv` shape and de-`#[allow(dead_code)]`-ed.

- **05.3** Pin bash and coreutils in `flake.nix` via a dedicated
  `nixpkgs-reference` input (current pin: `bash-5.3p9`,
  `coreutils-9.10`). Expose `bashReference` / `coreutilsReference`
  packages. Devshell exports `FREDSHELL_REFERENCE_*` and
  `FREDSHELL_FLOATING_*` env vars. Document the pin and the bump
  policy in `tests/spec/REFERENCE.md` as a parseable `[reference]`
  TOML block. Add `cargo xtask spec versions` that verifies the doc
  matches the devshell and reports drift versus the floating
  `nixpkgs` input as advisory. Regression test in `xtask` asserts
  `tests/spec/REFERENCE.md` parses and matches the pinned values.

- **05.4** Create the `fredshell-spec-runner` crate. Library +
  binary. Library API: `run_case(path: &Path) -> CaseResult`.
  `.case.toml` schema parser. Sandbox setup/teardown. Single-case
  runner. Tests: a hand-written minimal case file exercised by the
  unit suite.

- **05.5** Implement the case-status taxonomy (§12) in the harness.
  Comparison logic distinguishes `pass`/`fail`/`wontfix`/`deferred`
  outcomes. Generates the per-status breakdown. Emits `RECLASSIFY`
  lines.

- **05.6** Add the `xtask compat` command. Walks `tests/spec/`,
  runs every case, produces the JSON report and the human-readable
  summary. JSON schema version 1. Documented in
  `tests/spec/README.md`.

- **05.7** Add the `xtask spec record` command. Invokes the pinned
  reference bash via nix, captures stdout/stderr/exit, writes
  fixtures. Refuses to run if bash is not the pinned version.

- **05.8** Add the `xtask spec lint` command. Validates `.case.toml`
  schema, detects orphaned fixtures, verifies §11.1 against `bash
-c 'enable -a'` from the pinned bash. Fails on drift.

- **05.9** Seed the corpus per §3.5. ~6–10 `pass` cases for
  implemented Tier-1 builtins; one `deferred:<plan>` case per §3.4
  category. Run `xtask spec record` for the `pass` cases; hand-write
  fixtures for the `deferred` cases by running bash manually and
  capturing.

- **05.10** Add CI integration. `cargo xtask compat` runs on every
  PR. JSON report uploaded as an artifact. PR comment summarizes the
  pass-rate delta vs. main.

- **05.11** Generate `COMPAT.md` via `cargo xtask compat
--update-readme`. Commit it. Add a pre-commit check that
  regenerates it.

- **05.12** Update `plan.md` to mark PLAN_05 `implemented` and refresh
  PLAN_02 §12 (§4 sections newly backed by harness coverage) and
  AGENTS.md (new `fredshell-spec-runner` crate row).

### 13.3. Out of scope for PLAN_05

The following are explicitly _not_ part of this task. Each has its
own owning document.

- Tier-2 oils-spec fetching and translation. Owned by PLAN_05's
  follow-up (PLAN_05-tier2 if needed, or merged into PLAN_06's
  exit criteria).
- Tier-3 real-world script selection. Owned by PLAN_16 (milestones).
- L4 PTY harness. Owned by PLAN_14.
- The `external_command_policy` removal once PLAN_06 lands native
  execve. Owned by PLAN_06.
- Any actual implementation of builtins or executor features from
  §11. Owned by PLAN_06, PLAN_13, PLAN_06, PLAN_06.

### 13.4. Verification per subtask

Every subtask runs the full verification suite per AGENTS.md:

1. `cargo test --all`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo-machete`
4. After 05.6 lands: `cargo xtask compat` produces a report.

## 14. Implementation log

To be filled as subtasks complete, one row per subtask, format
matching PLAN_06 §11.

| Subtask | Commit | Date       | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| ------- | ------ | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 05.1    | TBD    | 2026-05-21 | Added `ExternalCommandPolicy` enum (`FallbackToSh` default, `Strict`) and routed the dispatcher through it. `ExecEnv::from_process()` defaults to `FallbackToSh`; `ExecEnv::sandboxed()` defaults to `Strict`. New `ExecError::NoExternalExecutor { command, reason }` variant with `NoExternalExecutorReason::{PolicyStrict, UnparsableArgv}`. 12 new unit tests + 1 integration test cover the strict path; existing tests opt into `FallbackToSh` via the test helper. Workspace: 207 unit / 5 integration tests passing; clippy clean (one scoped `needless_pass_by_ref_mut` allow on `dispatch_line` with a forward-compat rationale for 05.2 / 06b mutations); machete clean.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 05.2    | TBD    | 2026-05-21 | Moved stdio onto `ExecEnv` as `stdout: Box<dyn Write + Send>` / `stderr: Box<dyn Write + Send>`, defaulting to `io::stdout()` / `io::stderr()` in both constructors. Manual `Debug` impl renders writers as `"<dyn Write>"`. Removed the `Capture` enum from `dispatch_script`; `spawn_via_sh` now always pipes child stdio and copies it through the env writers (uniform path; `PLAN_06` reclaims the extra copy via inherited fds when writers are real stdio). New `exec::testing` module (gated `#[cfg(test)]`) ships `SharedBuf` (`Arc<Mutex<Vec<u8>>>` newtype, `Write` + `Clone`) and `run_source_capturing`, which swaps shared sinks onto a caller-supplied env, runs `parse + dispatch_script`, and restores prior writers on both success and `RunError::Parse` paths. Bench `exec_roundtrip_parse_and_exec` opts into `FallbackToSh` for the strict-default sandbox env. 210 lib + 4 smoke + 5 prompt + 112 ansi tests passing; clippy clean; machete clean.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 05.3    | TBD    | 2026-05-21 | Pinned the reference toolchain via a dedicated `nixpkgs-reference` flake input at rev `d233902339c02a9c334e7e593de68855ad26c4cb` (`bash-5.3p9`, `coreutils-9.10`). Exposed `packages.<system>.bashReference` and `packages.<system>.coreutilsReference`. Devshell exports `FREDSHELL_REFERENCE_BASH` / `FREDSHELL_REFERENCE_COREUTILS` (absolute paths) plus `_VERSION` siblings and matching `FREDSHELL_FLOATING_*` vars for drift advisories. Created `tests/spec/REFERENCE.md` with the bump policy and a parseable `[reference]` TOML block. New `cargo xtask spec versions` subcommand parses the doc, verifies it matches the devshell env, and reports drift versus floating `nixpkgs` as advisory output. New `xtask::spec` module with 6 unit tests (parser happy/error paths + on-disk doc regression test). PLAN_05 §4.5 / §11.2 / §13.3 updated to reflect the new versions (9.7 → 9.10). 112 ansi + 210 lib + 4 smoke + 5 prompt + 6 xtask tests passing; clippy clean; machete clean.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 05.4    | TBD    | 2026-05-21 | Created the `fredshell-spec-runner` crate (library + binary). Library API: `Case::load` (parses `*.case.toml` with the `PLAN_05` §4.1 schema), `Sandbox` (tempdir-backed, `materialize_skeleton`, `resolve_env` with `$SANDBOX` substitution, `root_is_utf8`), `run_case` (forces `ExternalCommandPolicy::Strict`, installs `Arc<Mutex<Vec<u8>>>` stdio sinks on a sandboxed `ExecEnv`, compares observed stdout/stderr/exit against the case spec). `CaseOutcome::{Pass, Mismatch { observed_stdout, observed_stderr, observed_exit }, ExecutorRefused { command, reason }}` and top-level `SpecError` are both `#[non_exhaustive]`. `ExecError::NoExternalExecutor` surfaces as `ExecutorRefused`, not a hard error. Binary (`fredshell-spec-runner <case>`) uses clap derive with exit codes 0 / 1 / 2 / 64 / 70. First fixture `tests/spec/builtins_tier1/exit_zero.case.toml`. 9 runner + 11 case + 6 sandbox + 6 error unit tests + 1 smoke integration test, all green; rustdoc clean; clippy clean; machete clean. Pre-existing `fredshell-core` / `fredshell-prompt` rustdoc warnings remain untouched.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| 05.5    | TBD    | 2026-05-21 | Implemented the §12 verdict taxonomy as a total `classify(&CaseStatus, &CaseOutcome) -> CaseVerdict` function in a new `verdict` module. `CaseVerdict::{ExpectedPass, Regression, ExpectedFail, WontfixHonored, DeferredHonored { plan }, Reclassify { from, suggested, reason }}` and `ReclassifyReason` are both `#[non_exhaustive]`. Strict-mode `ExecutorRefused` is a `Regression` on `pass` cases and honored on `fail` / `wontfix` / `deferred`. `VerdictTally` aggregates counts plus a per-plan `deferred_honored: BTreeMap<String, usize>` (keyed for 05.6's `xtask compat --status deferred:<plan>` queries) and exposes `pass_rate_numerator` / `pass_rate_denominator` (= `total - wontfix_honored`, per §12) and `has_ci_failures` (true only when `regressions > 0`). Added `fmt::Display for CaseStatus` (used by `Reclassify` rendering). Binary now prints `outcome:` + `verdict:` + grep-stable `RECLASSIFY:` lines and exits 0 for all non-`Regression` verdicts (including `Reclassify`, which is advisory in v0), 1 only for `Regression`. Trivial accessors marked `const fn` to satisfy clippy nursery. 19 new verdict tests + 1 new case round-trip test (54 spec-runner unit + 1 smoke total); `cargo xtask check` green; only pre-existing `fredshell-core` / `fredshell-prompt` rustdoc warnings remain.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| 05.6    | TBD    | 2026-05-21 | Added `cargo xtask compat` subcommand: walks `tests/spec/**/*.case.toml` (skipping `.fs` sandbox skeletons), filters by tier / category / status, runs each case through `fredshell-spec-runner`, classifies via `classify`, and aggregates with `VerdictTally`. v0 scope is tier-1-only (`--tier 2/3` accepted as forward-compat no-ops). `--status` parser at `xtask/src/compat.rs` accepts `pass` / `fail` / `wontfix` / `deferred:PLAN_XX` (rejects empty plan suffix). `--json <path>` writes schema v1 (top-level `schema_version: 1`, `tally`, `cases[]`) with tagged enums `{"kind":"..."}` for `outcome` / `verdict` (snake_case) and base64 (RFC 4648) `*_b64` fields for non-UTF-8 mismatch stdout/stderr. Summary printer renders tally + per-plan deferred counts + reclassify advisories. Exits 1 via `std::process::exit` iff `VerdictTally::has_ci_failures()` (regressions present). `relative_path` always emits `/` separators for cross-platform JSON determinism. `xtask` exemption uses `#[allow(clippy::cast_precision_loss)]` on the percentage `f64` conversion; `reclassify_reason_str` carries an outer `#[allow(clippy::missing_const_for_fn)]` (function attribute, not statement attribute). Workspace deps: pinned `serde_json = "=1.0.149"`, `base64 = "=0.22.1"`, `fredshell-spec-runner` workspace entry. New `tests/spec/README.md` documents corpus layout, harness invocation, full JSON schema v1, and stability rules (new optional fields / tagged variants are non-breaking and do not bump `schema_version`). 14 xtask unit tests + manual e2e (1/1 pass on the seed fixture; `--json` produces valid v1 doc; unknown status rejected) all green; clippy + machete clean.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 05.7    | TBD    | 2026-05-21 | Added `cargo xtask spec record <case>` subcommand for producing sidecar fixtures from the pinned reference bash. Refactored `xtask/src/spec.rs` into `xtask/src/spec/{mod,record}.rs` so future `lint` (05.8) can sit alongside without bloating one file. Recorder reads `FREDSHELL_REFERENCE_BASH` (absolute path, devshell-provided) and `FREDSHELL_REFERENCE_BASH_VERSION`, parses `tests/spec/REFERENCE.md` via the existing `parse_reference`, and refuses to run when the env-reported version disagrees with the doc pin (per `PLAN_05` §4.5). Loads the case via `fredshell_spec_runner::Case::load`, materializes the `<case>.fs/` skeleton in a fresh `Sandbox`, resolves the `[env]` block (`$SANDBOX` substitution), and invokes `bash -c <script>` with `env_clear()` + the resolved env, CWD = sandbox root. Sidecars are written per `PLAN_05` §3.2's "present explicitly when non-default" rule: empty stdout/stderr and exit 0 trigger removal of existing sidecars (or no-op when none exist); non-default values produce `Created` / `Updated` / `Unchanged` actions reported by file. `cargo run` ambiguity fixed in a sibling chore commit by narrowing `default-members` to `crates/fredshell` so the recorder is reached via `cargo run -p xtask` without surprises. 11 new record unit tests (sidecar write/remove/unchanged paths, stem stripping) + manual e2e (record from devshell against `exit_zero.case.toml` → all-defaults skip; synthetic hello/warn/exit-7 case → all three sidecars created with byte-exact contents; re-run → all `Unchanged`; outside-devshell → refused with `FREDSHELL_REFERENCE_BASH` diagnostic, exit 1; spoofed version mismatch → refused with REFERENCE.md diff, exit 1). 25 xtask + 54 spec-runner + 210 core + 112 ansi + 5 prompt + 4 smoke + 1 spec-runner integration tests passing; clippy + machete clean.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| 05.8    | TBD    | 2026-05-21 | Added `cargo xtask spec lint` subcommand performing three checks over `tests/spec/`. (1) **Schema**: every `*.case.toml` is loaded via `fredshell_spec_runner::Case::load`; load errors are reported per-file with the parser's diagnostic. (2) **Orphan fixtures**: any `*.stdout` / `*.stderr` / `*.exit` sidecar or `*.fs/` skeleton directory without a matching `<stem>.case.toml` is flagged. (3) **§11.1 builtins drift**: runs the pinned reference bash via `FREDSHELL_REFERENCE_BASH -c 'enable -a'`, normalizes the output (last whitespace token per line, handling `enable -n` disabled entries), and diffs against an internal `EXPECTED_BUILTINS` constant (57 entries, sorted, baked from `PLAN_05` §11.1). A `--skip-builtins-drift` flag bypasses the bash invocation for local dev outside the devshell; CI runs the full set. Corpus walker collects cases and sidecars in a single pass and treats any directory with a `.fs` extension as a fixture (not recursed). Lint exits 0 on clean / 1 with a per-violation report on any failure. New `xtask/src/spec/lint.rs` (487 lines) with 12 unit tests covering walker semantics (nested dirs, `.fs` skip), schema pass/fail, orphan detection by sidecar type, drift detection (missing, unexpected, both), constant invariants (sorted, unique, plan §11.1 count = 57). Initial `EXPECTED_BUILTINS` constant overcounted by 4 (`bind`, `compgen`, `complete`, `compopt` are not in the actual bash 5.3p9 `enable -a` output despite being readline-class builtins); count test caught the drift and the constant was trimmed to match. Manual e2e: clean corpus → exit 0; planted orphan sidecar → reports orphan, exit 1; malformed `.case.toml` → reports parser error, exit 1; outside devshell without `--skip-builtins-drift` → diagnostic and refusal; with the flag → schema + orphans run, drift skipped. 37 xtask (+12 lint) + 54 spec-runner + 210 core + 112 ansi + 5 prompt + 4 smoke + 1 spec-runner integration tests passing; clippy + machete clean.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 05.9    | TBD    | 2026-05-21 | Seeded the spec corpus per §3.5. All-pass tier-1 seed: 7 cases under `tests/spec/builtins_tier1/` (`exit_zero`, `exit_nonzero`, `exit_default`, `exit_short_circuits`, `exit_after_blank_lines`, `only_blank_lines`, `empty_script`) — these exercise every code path of the implemented `exit` / `quit` builtins and the dispatcher's blank-line / short-circuit behavior. Deferred breadth: 14 `deferred:<plan>` cases covering 13 of the §3.4 categories — `parameter_expansion/default_value`, `quoting/mixed_quotes`, `redirection/redirect_stdout_to_file`, `pipelines/simple_pipe`, `control_flow/if_else`, `arithmetic/arith_basic`, `arrays/indexed_array`, `functions/simple_function`, `tests_and_conditionals/string_eq`, `expansions/brace_expansion`, `error_handling/set_e_aborts` (all `deferred:PLAN_12`), `traps_and_signals/trap_exit`, `job_control/background_wait` (both `deferred:PLAN_13`), plus `builtins_tier1/exit_nonnumeric_arg` (`deferred:PLAN_12`, documents the `exit abc` divergence: bash → 2 + stderr diagnostic; fredshell v0 → 0). The `builtins_tier2_*` category was intentionally left empty pending a coreutils-pin mechanism — see `05.9-CU2`. **Process notes:** all 14 deferred sidecars recorded via `cargo xtask spec record` against the pinned bash 5.3p9 (exported `FREDSHELL_REFERENCE_BASH` / `_VERSION` outside the devshell from `/nix/store/i27rhb3nr65rkrwz36bchkwmav6ggsmn-bash-5.3p9`). Two cases (`simple_pipe`, `redirect_stdout_to_file`) were rewritten to use only bash builtins (`echo`, `read`, `printf`, `{ ...; }`) instead of `wc` / `cat` so they record hermetically without a `PATH` — see `05.9-CU2` for the broader externals story. The `xtask spec record` recorder uses `env_clear() + envs(case.env)`, so cases referencing external commands surface as `exit 127` until that gap is closed. **Verification:** `cargo run -p xtask -- compat` reports 21 cases (7 ExpectedPass + 12 deferred:PLAN_12 + 2 deferred:PLAN_13), exit 0; `cargo run -p xtask -- spec lint --skip-builtins-drift` reports 21 schema-OK + 19 orphans-OK + builtins skipped, exit 0. 37 xtask + 54 spec-runner + 210 core + 112 ansi + 5 prompt + 4 smoke + 1 spec-runner integration tests passing; clippy + machete clean.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| 05.10   | TBD    | 2026-05-21 | Added CI integration for `cargo xtask compat`. New `compat` job in `.github/workflows/ci.yml` runs on every PR (existing `pull_request` trigger) and now also on push to `main` (new trigger added to satisfy `PLAN_05` §7.2). The job checks out with `fetch-depth: 0`, brings up the same Determinate Nix devshell as `nix-checks` (so the pinned bash is available for any future `xtask` step that needs it — e.g. the §11.1 builtins-drift check landed in 05.8), and runs `cargo xtask compat --json target/compat.json` inside `nix develop --impure`. The resulting v1-schema JSON is uploaded as a `compat-report` artifact via `actions/upload-artifact@043fb46d…` (v7.0.1) with `if-no-files-found: error` and `retention-days: 30`; `if: always()` ensures the artifact survives a `Regression` exit-1 for postmortem. The `ci-success` gate now waits on `compat` in addition to `nix-checks` / `check`. CI failure policy matches `xtask compat`'s exit semantics from 05.6 — fail iff `VerdictTally::has_ci_failures()` (any `Regression`); `DeferredHonored` / `WontfixHonored` / `Reclassify` are non-fatal — per `PLAN_05` §7.2 points 4–5. The PR-comment with pass-rate delta vs. main (the second half of §7.2) is deferred to `05.10-CU1` because it requires net-new xtask code (`compat --diff <baseline.json>`), a way to download main's last artifact via the GitHub API, and a sticky-comment action; not a 05.10-blocker. **Verification:** local smoke test `cargo run -p xtask -- compat --json target/compat.json` produced a valid v1 doc (schema_version=1, 21 cases, 7 ExpectedPass + 12 deferred:PLAN_12 + 2 deferred:PLAN_13), exit 0. The workflow YAML passes the existing `check-github-actions` pre-commit hook. 37 xtask + 54 spec-runner + 210 core + 112 ansi + 5 prompt + 4 smoke + 1 spec-runner integration tests passing; clippy + machete clean.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| 05.11   | TBD    | 2026-05-21 | Added `cargo xtask compat --update-readme` and `--check-readme` (mutually exclusive via clap `conflicts_with`). `--update-readme` runs the corpus, renders `COMPAT.md` at the workspace root via `render_compat_md(&Report) -> String`, and writes it. `--check-readme` renders the same body in memory and `bail!`s if it differs from (or is missing) the committed file, with an action-message pointing the reader at `--update-readme`. Per `PLAN_05` §7.3, `COMPAT.md` has two sections: **Implemented** (cases with `status = "pass"`, grouped by category) and **In progress** (cases with `status = "deferred:PLAN_XX"`, grouped by plan). Categories within Implemented and plans within In progress are sorted lexically; case lists within each group are sorted by relative path. `fail` and `wontfix` cases are excluded per §7.3. A `<!-- generated by ... -->` banner at the top of the file makes hand-edits obvious and participates in the byte-exact comparison. A `## Summary` block reports `Cases total`, `Passing: num / den`, and a percentage `Pass rate`. **Pre-commit wiring:** `cargo xtask pc` now invokes `cargo xtask compat --check-readme` after fmt / clippy / machete / test, so every commit verifies `COMPAT.md` is in sync with the current corpus. The plan's original intent was a dedicated `compat-readme` hook in `.pre-commit-config.yaml`, but that file is generated by the upstream `FredSystems/pre-commit-checks` flake which exposes only `extraExcludes` — there is no `extraHooks` injection point. Routing through `pc` provides the same guarantee transitively (the existing `xtask-check` hook calls `pc` on every commit) and keeps 05.11 atomic; upstreaming a real hook is filed as `05.11-CU1`. **Schema discipline:** path-only labels (no per-case `description` field) — adding a description would ripple into `Case`, the lint, the runner, and all 21 existing fixtures, far outside 05.11's scope. **Tests:** 10 new unit tests in `xtask/src/compat.rs` cover the renderer (banner present, category / plan grouping & sort order, fail / wontfix exclusion, empty-corpus message, pass-rate percentage rendering) and both file ops (`check_readme` succeeds on match, fails on drift, fails when missing; `update_readme` writes the rendered body byte-for-byte). `update_readme` / `check_readme` take an explicit `&Path` to keep tests hermetic without cwd mutation. Initial generated `COMPAT.md` checked in at 46 lines (7 implemented cases in `builtins_tier1`, 14 deferred across PLAN_06 and PLAN_13, 33.3 % pass rate). **Trailing-newline normalization:** `render_compat_md` collapses any `\n\n` suffix down to a single `\n` so the byte-exact `--check-readme` comparison stays in agreement with the repo's `end-of-file-fixer` pre-commit hook (which strips trailing blank lines on every commit). Without this, the first commit-after-update would fail `--check-readme` because the hook had silently re-trimmed the file. **Verification:** 47 xtask (+10 readme tests) + 54 spec-runner + 210 core + 112 ansi + 5 prompt + 4 smoke + 1 spec-runner integration tests passing; clippy + machete clean. |
| 05.12   | TBD    | 2026-05-21 | Marked PLAN_05 `implemented`. Updated `plan.md` row 05 status `draft` → `implemented`; updated `Documents/PLAN_02_architecture.md` §12 (moved `fredshell-spec-runner` from "Deferred" to "Implemented" with a description covering `Case` / `Sandbox` / `run_case`, the verdict taxonomy, `cargo xtask compat` + `spec record` + `spec lint`, and the 21-case seed corpus). `AGENTS.md` crate table already carried the `fredshell-spec-runner` row from 05.4. Bumped PLAN_05's "Last updated" header and status line. No code changes. **Verification:** `cargo xtask check` green (fmt + clippy + machete + test + doc); 47 xtask + 54 spec-runner + 210 core + 112 ansi + 5 prompt + 4 smoke + 1 spec-runner integration tests passing.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |

## 15. Cleanup registry

To be filled if any subtask surfaces a pre-existing bug per the
AGENTS.md "pre-existing bugs surfaced during a subtask" rule.

| ID        | Surface                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | Impact                                                                                                                                                                                                                                                                                                                                                                                                         | Fix scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | Status                                                                                                                                                                                                                                                                                                                                                                  |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 05.9-CU1  | Surfaced during 05.9 (corpus seeding). `fredshell_spec_runner::run_case` does not acquire `fredshell_core::exec::env::GLOBAL_ENV_LOCK` and does not save/restore the process cwd before/after running a case. The `cd` builtin calls `std::env::set_current_dir`, mutating shared process state; the `export` builtin (PLAN_06) will mutate `env::set_var`. Today this is masked because the only implemented mutating builtin is `cd`, and the all-pass seed avoids it on purpose, but the moment 06b lands `export` / `cd`-with-side-effect cases or 05.10 enables parallel case execution, cases will contaminate each other and `cargo test --all` will go non-deterministic.                                                                                                                                                                                                                                                                                                                                                                                                 | High once 06b lands; latent today. Affects every `pass` case that mutates global state and every parallel test runner that exercises the spec-runner library API. Surfaces as flaky `cargo xtask compat` runs and intermittent test failures rather than a deterministic bug, which makes it hard to diagnose after the fact.                                                                                  | Acquire `GLOBAL_ENV_LOCK` for the duration of `run_case`. Snapshot `env::current_dir()` and `env::vars_os()` before the executor runs and restore both on return (including the error path). Unit-test by running two cases back-to-back where the first mutates cwd / env and asserting the second sees the original values. The lock must be re-exported (or wrapped) for cross-crate use — it is `pub(crate)` today.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | Open. Must be fixed before 05.10 (CI integration) so parallel-test isolation is correct, and before PLAN_06 lands its first `cd` / `export` `pass` case. Tracked here per AGENTS.md "pre-existing bugs surfaced during a subtask" rule.                                                                                                                                 |
| 05.9-CU2  | Surfaced during 05.9 (corpus seeding). `cargo xtask spec record` invokes bash with `env_clear() + envs(case.env)`, so any case whose script calls an external program (anything not in §11.1) records as `exit 127 "command not found"` unless the case explicitly declares a `PATH` in its `[env]` block. There is no project-wide mechanism to point `PATH` at the pinned `coreutils-9.10` without hardcoding a host-specific nix store hash (multiple `*-coreutils-9.10` paths can co-exist on a single host). `PLAN_05` §3.4 expects a `builtins_tier2_*` category (`cat`, `ls`, `cp`, `mv`, `rm`, ...), and §11.2 maps that category to `PLAN_06`, but the harness cannot record fixtures for those cases today. The 05.9 seed sidesteps this by (a) rewriting two seed-breadth cases (`pipelines/simple_pipe`, `redirection/redirect_stdout_to_file`) to use only bash builtins and (b) leaving `tests/spec/builtins_tier2_*/` empty.                                                                                                                                       | Medium. Blocks any tier-2 `pass` / `deferred` case that exercises a coreutils binary, which is roughly half of `PLAN_06`'s acceptance corpus. Also blocks faithful representation of common bash idioms in tier-3 (real-world scripts) since most invoke coreutils. Today the gap is purely "we cannot record these cases yet" — it does not affect the harness, the runner, or any committed fixture.         | (1) Expose `FREDSHELL_REFERENCE_COREUTILS_BIN` from the devshell (the absolute store path's `/bin` directory; the flake already pins `coreutilsReference`). (2) Teach `xtask spec record` to substitute a `$COREUTILS` placeholder in `case.env.path` before passing the env to bash. (3) Document the placeholder in `tests/spec/README.md` and add a §3.5 / §4.4 note that tier-2 cases SHOULD use `PATH = "$COREUTILS"` rather than hardcoded store paths. (4) Backfill at least one `builtins_tier2_*` case (`cat_simple` is the obvious candidate) to exercise the new path end-to-end. Optionally extend the harness runner side too once `PLAN_06` lands the first tier-2 `pass` case (today the runner runs fredshell, not bash, so no PATH plumbing is needed yet).                                                                                                                                                                            | Open. Should be fixed during `PLAN_06` (tier-2 builtins) so the first tier-2 case has a working recording path. Not blocking for 05.10 (CI), 05.11 (`COMPAT.md`), or 05.12 (mark implemented) — those operate on the corpus as-is.                                                                                                                                      |
| 05.10-CU1 | Surfaced during 05.10 (CI integration). `PLAN_05` §7.2 specifies that the compat CI job should "compare the JSON report against the last main-branch report (downloaded from CI artifacts)" and produce a PR comment summarizing the pass-rate delta vs. `main`. 05.10 shipped only the artifact-upload half: `cargo xtask compat --json target/compat.json` runs in CI and the v1 JSON is uploaded as `compat-report`, but no diff or PR comment is produced. Without this follow-up there is no merge-time visibility into "this PR makes us 3 cases worse" or "this PR moves 2 deferred cases to pass" — both of which are §7.2's stated value. The push-to-main trigger added in 05.10 ensures a `main` baseline artifact exists for the comparison to consume.                                                                                                                                                                                                                                                                                                               | Medium. Affects reviewer ergonomics, not correctness — every regression is still caught by the per-PR job exit code (via `has_ci_failures()`), and every reclassify still produces a `RECLASSIFY:` line in the job log. The missing piece is the surfaced-in-the-PR delta summary that §7.2 promises. Once the corpus grows past the 05.9 seed (~21 cases), eyeballing the raw JSON or job log will not scale. | (1) Add `cargo xtask compat --diff <baseline.json>` to xtask: parses two v1 reports, classifies each case across both, emits a delta report with new regressions, new passes, reclassify deltas, and aggregate pass-rate change. Output as both human-readable text and a `--markdown` flag for the PR body. Unit-tested via golden-file fixtures of synthetic v1 docs. (2) Extend the CI workflow's `compat` job: on `pull_request` events, download main's latest `compat-report` artifact via `gh run download` (or the API) and run `cargo xtask compat --diff baseline/compat.json --markdown > delta.md`. (3) Post `delta.md` as a sticky PR comment via `marocchino/sticky-pull-request-comment` (pin by SHA). Requires `pull-requests: write` permission on the workflow. (4) Decide how to handle the bootstrap case (PR opened before any main baseline exists) — likely emit a "no baseline" placeholder rather than failing.                | Open. Should land before the corpus grows substantially (rough target: before `PLAN_06` lands its first batch of `pass` reclassifications), but is not blocking 05.11 / 05.12. Tracked here per the AGENTS.md "scope-deferred follow-up" pattern; not a regression bug.                                                                                                 |
| 05.11-CU1 | Surfaced during 05.11 (`COMPAT.md` regen + pre-commit guarantee). `PLAN_05` §7.3 specifies a dedicated pre-commit hook that runs `cargo xtask compat --check-readme` and fails the commit when `COMPAT.md` is out of date. The hook config in `.pre-commit-config.yaml` is owned by the upstream `FredSystems/pre-commit-checks` flake (lock `cb27f0e…`), which only exposes `extraExcludes` — there is no `extraHooks` injection point today. 05.11 routes the guarantee through `cargo xtask pc` instead (every commit already runs `pc` via the existing `xtask-check` hook, and `pc` now invokes `compat --check-readme`), which is functionally equivalent but bundles the failure under the broader xtask-check hook rather than surfacing it as a dedicated step. Additionally, `compat --update-readme` and `--check-readme` re-run the entire corpus on every invocation; a `--from-json <path>` mode that reuses an already-produced v1 report would let CI render `COMPAT.md` from the same JSON it uploads as the `compat-report` artifact instead of double-running. | Low. Today's `pc`-based wiring catches every drift case the dedicated hook would catch; the difference is purely surfaced in the failure output (the hook name shown to the developer is `xtask-check` rather than `compat-readme`). The double-run cost is also small at 21 cases but will grow linearly as the corpus expands toward `PLAN_06` / `PLAN_06` (hundreds of cases).                              | (1) Upstream a `compat-readme` hook to `FredSystems/pre-commit-checks` (or extend the flake module to accept `extraHooks` so downstream projects can register their own), then add the dedicated hook entry to fredshell's `flake.nix`. (2) Add `cargo xtask compat --from-json <path>` that reads a v1 JSON report and re-derives a `Report` for `render_compat_md` / `check_readme` without re-running the corpus; covered by golden-file unit tests against synthetic v1 docs. (3) Wire the CI `compat` job to call `--update-readme --from-json target/compat.json` (or `--check-readme --from-json`) so README generation reuses the artifact it already uploads. (4) Remove the `compat --check-readme` line from `cargo xtask pc` once the dedicated hook is in place, to avoid double-checking on every commit. Scheduling: not a blocker for any downstream subtask; can land anytime after the `pre-commit-checks` upstream change is merged. | Open. Should land after the `pre-commit-checks` upstream gains an `extraHooks` injection point (or a dedicated `compat-readme` hook entry). Not blocking 05.12 (mark PLAN_05 implemented) or any PLAN_06 / PLAN_13 subtask — today's `pc`-based wiring catches the same drift. Tracked here per the AGENTS.md "scope-deferred follow-up" pattern; not a regression bug. |

## References

- `Documents/decisions/0003-test-first-compatibility-methodology.md`
  — the methodology this document operationalizes.
- `Documents/PLAN_01_philosophy.md` — goals G1, G2 and non-goal NG1
  define what compatibility means.
- `Documents/PLAN_02_architecture.md` — the architecture that
  satisfies the constraints in §5.
- `Documents/PLAN_06_exec.md` (Phase A implemented) —
  supplies the `ExecEnv`, `run_source`, and dispatcher this document
  mutates. Phase B (the real executor) lives in PLAN_12; see entry
  below.
- `Documents/PLAN_12_exec_phase_b.md` (Phase B drafted) — the real
  executor and owner of most `deferred:PLAN_12` cases.
- `Documents/PLAN_14_line_editor.md` (pending) — owner of the
  L4 PTY harness referenced in §6.3 and the `fc`/`history`/`bind`
  builtins.
- `Documents/PLAN_13_traps_and_jobs.md` (Phase B stub) — owner of
  the job-control and signal-disposition builtins in the §11 table
  (rows marked PLAN_13).
- `Documents/PLAN_19_milestones.md` (Phase B stub) — milestone 1
  ships this harness plus the L3 layer at whatever pass-rate.
- `AGENTS.md` — testing philosophy, panic-free production code
  rules, and crate-status table that this document extends.
