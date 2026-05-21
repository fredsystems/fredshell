# PLAN_05 — Testing and the Spec Corpus

> Last updated: 2026-05-21 — rewrite. Adds strict execution mode (§4.2,
> §5.2), exhaustive vendoring scope (§11), case-status taxonomy (§12),
> corpus-seeding rule (§3.5), and corrects the reference-bash pinning
> (§4.5: nixpkgs-unstable currently delivers bash 5.3.9, not 5.2).
> Phase: A. Status: draft.
> Operationalizes ADR 0003.

This document defines how fredshell tests its own behavior. It is the
concrete realization of ADR 0003 (test-first compatibility methodology)
and the first planning document drafted in detail, because the harness
described here imposes hard constraints on every later document —
notably PLAN_02 (architecture), PLAN_06b (real bash-compat executor),
PLAN_09 (builtins), and PLAN_13 (milestones).

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
regression; a case with `status = "deferred:PLAN_09a"` matching is a
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
status = "deferred:PLAN_06b"   # see §12 for the taxonomy

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
   time (today: `cd`, `exit`, the few stubs that exist; PLAN_06b
   grows this list aggressively).

2. **Deferred breadth seed.** Write one positive case per category in
   §3.4. Status: `deferred:<owning-plan>`. Total: ~16 cases. These
   all fail today. Their value is enumeration — the failing-case
   list _is_ the v1 work plan. A case marked
   `deferred:PLAN_06b` becomes the single-bullet PR description for
   the corresponding 06b subtask.

3. **Stop.** Do not add more cases until the feature lands. New cases
   are added by the plan document implementing the feature (e.g.,
   PLAN_06b owns `arithmetic`, `control_flow`, and most of
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
<line>` for anything that is not a Tier-1 builtin (PLAN_06a §3 v0
simplification). In strict mode, the dispatcher must instead return
a typed `ExecError::NoExternalExecutor { command, reason }` so the
case fails honestly. This is what makes the pass-rate read
"fredshell-as-itself" rather than "fredshell-plus-dash."

The mechanism is owned by PLAN_05a (the implementation subtask
list below): a new field on `ExecEnv` — `pub external_command_policy:
ExternalCommandPolicy` — with two variants:

- `ExternalCommandPolicy::FallbackToSh` (default; what the REPL uses
  today).
- `ExternalCommandPolicy::Strict` (what the harness uses; refusal
  produces `NoExternalExecutor`).

The default in `ExecEnv::from_process()` is `FallbackToSh` so the
binary REPL is unchanged. The default in `ExecEnv::sandboxed()` is
`Strict` so the harness gets the correct behavior without opt-in.
PLAN_06b removes the `FallbackToSh` variant entirely once the native
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

**Current state (2026-05-21):** the flake uses `nixos-unstable` and
does not pin bash explicitly. As of this writing,
`nixos-unstable` delivers `bash-5.3p9` and `coreutils-9.7` on
Linux. These are recorded as the v1 reference until the flake adds
explicit pinning.

**Required follow-up (PLAN_05a subtask):** pin bash and coreutils in
the flake explicitly so the reference does not drift when
`nixos-unstable` rolls forward. Until then, every `xtask spec
record` run captures the actual versions in
`tests/spec/REFERENCE.md` as a header comment in each fixture.

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
with `status = "deferred:PLAN_06b"` that suddenly matches is a signal
that PLAN_06b has made progress or that a different change incidentally
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

**Status:** ✓ implemented in PLAN_06a (stub parser; real parser in
PLAN_06b).

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
    // path resolution policy, ShellState (functions, aliases, jobs)
    // — added by PLAN_06b and PLAN_09.
}

pub enum ExternalCommandPolicy {
    FallbackToSh,
    Strict,
}

pub fn run_source(source: &str, env: &mut ExecEnv)
    -> Result<RunResult, RunError>;
```

The concrete signature evolves with PLAN_06b, but the shape is
non-negotiable: no implicit globals, no calls to `std::env::var` at
the leaves, no `println!` macros — every byte of output goes through
`env.stdout` or `env.stderr`. PLAN_02 §4 owns the exact API.

**Status:** `PLAN_06a` landed `ExecEnv { cwd, env, last_status }` plus
the `Capture::Buffers` mechanism on the dispatcher as a sibling
parameter. `PLAN_05` requires moving capture _onto_ `ExecEnv` itself
and adding the `external_command_policy` field. This is the first
implementation subtask in PLAN_05a — see §13.

`stdin` is reserved on `ExecEnv` for PLAN_09 (read builtin); v1 of
the harness writes empty stdin and tests that consume stdin are
deferred per §10.

### 5.3. Batch-mode entry point

A non-interactive batch entry point exists from day one. The harness
calls it. The REPL is built **on top of** this entry point, not
alongside it. There is never a moment in the development history when
the only way to run a script through fredshell is via the line
editor.

**Status:** ✓ implemented in PLAN_06a (06a.6: REPL routes through
`run_source`).

### 5.4. Builtin dispatch must be testable

Tier-2 builtins must be invocable directly from the harness without
spinning up a full REPL or process. Each tier-2 builtin exposes an
`invoke(env, args) -> ExitStatus` method that the spec test for that
builtin calls.

**Status:** ✓ trait surface (`Tier2Builtin`, `Tier2Ctx`, `Tier2Error`)
landed in PLAN_06a 06a.4. No implementations yet; PLAN_09 owns
inventory and dispatch.

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
editor to exist. Its design is owned by PLAN_07 (interactive UX)
with input from PLAN_04 (terminal I/O). It is mentioned here because
shell behavior in interactive mode genuinely cannot be exercised by
L3, and deferring the design entirely would be a mistake.

### 6.4. L5 — Benchmarks

Criterion benchmarks at `benches/` per AGENTS.md performance
discipline. Cover the prompt renderer, parser, line edit dispatch,
and history search. Required before/after numbers for any change to
those areas. PLAN_06a 06a.7 seeded the executor bench
(`exec_roundtrip`); PLAN_06b grows it.

## 7. CI integration

### 7.1. xtask commands

- `cargo xtask compat` — run the full spec corpus, emit report.
- `cargo xtask compat <category>` — run a single category.
- `cargo xtask compat --tier 1` — restrict to a tier.
- `cargo xtask compat --status deferred:PLAN_06b` — restrict to a
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
  until PLAN_06b removes it.
- ~~Capture mechanism — `ExecEnv` field vs. sibling function.~~
  Resolved: §5.2. Capture moves onto `ExecEnv`.
- ~~Reference bash version.~~ Resolved: §4.5. Records actual current
  flake versions (bash 5.3.9, coreutils 9.7) and mandates explicit
  pinning as a PLAN_05a subtask.
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
  slot reserved; semantics deferred to PLAN_09 (when `read` lands).
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
looking references (PLAN_09a etc.) are intentional — the table
forces those documents to exist.

| Builtin     | Category   | Disposition   | Plan        | Notes                                             |
| ----------- | ---------- | ------------- | ----------- | ------------------------------------------------- |
| `:`         | POSIX      | Vendor:Tier-1 | PLAN_06b    | No-op; sets `$?` to 0.                            |
| `.`         | POSIX      | Vendor:Tier-1 | PLAN_06b    | Source a file; reentrant call into parser.        |
| `[`         | POSIX      | Vendor:Tier-1 | PLAN_06b    | Synonym for `test`.                               |
| `alias`     | bash       | Vendor:Tier-1 | PLAN_06b    | Requires alias table on `ShellState`.             |
| `bg`        | POSIX      | Vendor:Tier-1 | PLAN_06b    | Requires job table.                               |
| `break`     | POSIX      | Vendor:Tier-1 | PLAN_06b    | Control flow; integer level.                      |
| `builtin`   | bash       | Vendor:Tier-1 | PLAN_06b    | Bypass aliases/functions.                         |
| `caller`    | bash       | Vendor:Tier-1 | PLAN_09a    | Function call stack introspection.                |
| `cd`        | POSIX      | Vendor:Tier-1 | implemented | PLAN_06a — partial; full flag surface TBD.        |
| `command`   | POSIX      | Vendor:Tier-1 | PLAN_06b    | Bypass functions and aliases.                     |
| `continue`  | POSIX      | Vendor:Tier-1 | PLAN_06b    | Control flow.                                     |
| `declare`   | bash       | Vendor:Tier-1 | PLAN_06b    | Synonym `typeset`; requires scoped variables.     |
| `dirs`      | bash       | Vendor:Tier-1 | PLAN_09a    | Directory stack.                                  |
| `disown`    | bash       | Vendor:Tier-1 | PLAN_09a    | Job-table edit.                                   |
| `echo`      | POSIX/bash | Vendor:Tier-1 | PLAN_06b    | bash quirks: `-n`, `-e`, `-E`.                    |
| `enable`    | bash       | Vendor:Tier-1 | PLAN_06b    | Toggle builtin status.                            |
| `eval`      | POSIX      | Vendor:Tier-1 | PLAN_06b    | Reentrant parse + execute.                        |
| `exec`      | POSIX      | Vendor:Tier-1 | PLAN_06b    | Replace process; fd manipulation.                 |
| `exit`      | POSIX      | Vendor:Tier-1 | implemented | PLAN_06a — full.                                  |
| `export`    | POSIX      | Vendor:Tier-1 | PLAN_06b    | Mark variable for environment.                    |
| `false`     | POSIX      | Vendor:Tier-1 | PLAN_06b    | Trivial; sets `$?` to 1.                          |
| `fc`        | bash       | Vendor:Tier-1 | PLAN_07     | History subsystem; owned by line editor.          |
| `fg`        | POSIX      | Vendor:Tier-1 | PLAN_06b    | Job control.                                      |
| `getopts`   | POSIX      | Vendor:Tier-1 | PLAN_09a    | Argument parsing for scripts.                     |
| `hash`      | POSIX      | Vendor:Tier-1 | PLAN_09a    | PATH lookup cache.                                |
| `help`      | bash       | Vendor:Tier-1 | PLAN_09a    | Self-documenting; sourced from this table.        |
| `history`   | bash       | Vendor:Tier-1 | PLAN_07     | Owned by line editor.                             |
| `jobs`      | POSIX      | Vendor:Tier-1 | PLAN_06b    | Job table.                                        |
| `kill`      | POSIX      | Vendor:Tier-1 | PLAN_06b    | Signal delivery; also a coreutils binary.         |
| `let`       | bash       | Vendor:Tier-1 | PLAN_06b    | Arithmetic eval; ties to `((...))`.               |
| `local`     | bash       | Vendor:Tier-1 | PLAN_06b    | Scoped variable; function scope.                  |
| `logout`    | bash       | Vendor:Tier-1 | PLAN_09a    | Login-shell-only exit.                            |
| `mapfile`   | bash       | Vendor:Tier-1 | PLAN_09a    | Read into array; synonym `readarray`.             |
| `popd`      | bash       | Vendor:Tier-1 | PLAN_09a    | Directory stack.                                  |
| `printf`    | POSIX      | Vendor:Tier-1 | PLAN_09a    | Non-trivial format string parser.                 |
| `pushd`     | bash       | Vendor:Tier-1 | PLAN_09a    | Directory stack.                                  |
| `pwd`       | POSIX      | Vendor:Tier-1 | PLAN_06b    | Trivial; `-L` / `-P` flags.                       |
| `read`      | POSIX      | Vendor:Tier-1 | PLAN_09a    | Line-editor implications; needs `ExecEnv::stdin`. |
| `readarray` | bash       | Vendor:Tier-1 | PLAN_09a    | Synonym `mapfile`.                                |
| `readonly`  | POSIX      | Vendor:Tier-1 | PLAN_06b    | Variable attribute.                               |
| `return`    | POSIX      | Vendor:Tier-1 | PLAN_06b    | Function exit.                                    |
| `set`       | POSIX      | Vendor:Tier-1 | PLAN_06b    | Shell-options table.                              |
| `shift`     | POSIX      | Vendor:Tier-1 | PLAN_06b    | Positional-args manipulation.                     |
| `shopt`     | bash       | Vendor:Tier-1 | PLAN_06b    | Bash-specific options table.                      |
| `source`    | bash       | Vendor:Tier-1 | PLAN_06b    | Synonym `.`.                                      |
| `suspend`   | bash       | Vendor:Tier-1 | PLAN_09a    | Self-SIGTSTP.                                     |
| `test`      | POSIX      | Vendor:Tier-1 | PLAN_06b    | Huge surface — file tests, string tests, etc.     |
| `times`     | POSIX      | Vendor:Tier-1 | PLAN_09a    | Process times; thin libc.                         |
| `trap`      | POSIX      | Vendor:Tier-1 | PLAN_06b    | Signal disposition table.                         |
| `true`      | POSIX      | Vendor:Tier-1 | PLAN_06b    | Trivial.                                          |
| `type`      | POSIX      | Vendor:Tier-1 | PLAN_09a    | Identify command kind; uses §11 table.            |
| `typeset`   | bash       | Vendor:Tier-1 | PLAN_06b    | Synonym `declare`.                                |
| `ulimit`    | POSIX      | Vendor:Tier-1 | PLAN_09a    | `setrlimit` wrapper.                              |
| `umask`     | POSIX      | Vendor:Tier-1 | PLAN_09a    | `umask` syscall wrapper.                          |
| `unalias`   | bash       | Vendor:Tier-1 | PLAN_06b    | Alias-table edit.                                 |
| `unset`     | POSIX      | Vendor:Tier-1 | PLAN_06b    | Variable / function removal.                      |
| `wait`      | POSIX      | Vendor:Tier-1 | PLAN_06b    | Wait for jobs/pids.                               |

#### Bash reserved words (not builtins, but exercised by spec tests)

The harness exercises these through the parser/executor path, not via
builtin dispatch:

`!`, `case`, `coproc`, `do`, `done`, `elif`, `else`, `esac`, `fi`,
`for`, `function`, `if`, `in`, `select`, `then`, `time`, `until`,
`while`, `{`, `}`, `[[`, `]]`, `((`, `))`.

All are owned by PLAN_06b. `coproc` and `time` are open questions:
`coproc` is a parser-level construct with non-trivial semantics
(may be deferred to a later phase); `time` is a keyword-level builtin
that the parser must recognize.

### 11.2. Coreutils 9.7 — exhaustive

Sourced from `ls $(nix-store -q --references $(which coreutils))/bin`
on the pinned reference coreutils. All 109 binaries listed below.

Disposition vocabulary inherits from §11.1, with one addition:

- **Vendor:Tier-2** — fredshell ships a native replacement in a
  dedicated crate (e.g., `fredshell-coreutils` or one crate per
  binary; layout TBD by `PLAN_09`). PATH-resolution is still
  permitted; `command -p` always reaches the external. Vendored
  variants are the _default_ dispatch when the binary name is
  invoked unadorned.
- **Vendor:Tier-2-partial** — fredshell vendors a _subset_ of flags
  matching common usage; uncommon flags fall through to PATH. Used
  for binaries with large but bimodal flag surfaces (e.g., `find`,
  `sed`).

| Command     | Disposition   | Plan     | Notes                                                      |
| ----------- | ------------- | -------- | ---------------------------------------------------------- |
| `[`         | (see §11.1)   |          | Builtin in bash; coreutils provides for non-shell callers. |
| `b2sum`     | PATH-resolve  |          | Crypto sum; rare.                                          |
| `base32`    | PATH-resolve  |          |                                                            |
| `base64`    | PATH-resolve  |          |                                                            |
| `basename`  | Vendor:Tier-2 | PLAN_09b | Trivial; hot in scripts.                                   |
| `basenc`    | PATH-resolve  |          | Multi-encoding base.                                       |
| `cat`       | Vendor:Tier-2 | PLAN_09b | Trivial; removes a fork on `cat file`.                     |
| `chcon`     | PATH-resolve  |          | SELinux; platform-specific.                                |
| `chgrp`     | Vendor:Tier-2 | PLAN_09b | Filesystem core.                                           |
| `chmod`     | Vendor:Tier-2 | PLAN_09b | Filesystem core.                                           |
| `chown`     | Vendor:Tier-2 | PLAN_09b | Filesystem core.                                           |
| `chroot`    | PATH-resolve  |          | Rare; privilege-sensitive.                                 |
| `cksum`     | PATH-resolve  |          |                                                            |
| `comm`      | PATH-resolve  |          | Niche set-comparison tool.                                 |
| `coreutils` | PATH-resolve  |          | Multi-call binary; not a target.                           |
| `cp`        | Vendor:Tier-2 | PLAN_09b | Filesystem core.                                           |
| `csplit`    | PATH-resolve  |          |                                                            |
| `cut`       | Vendor:Tier-2 | PLAN_09b | Hot in pipelines.                                          |
| `date`      | PATH-resolve  |          | GNU date's format/parsing is a separate world.             |
| `dd`        | PATH-resolve  |          | Sharp edges; low value to vendor.                          |
| `df`        | PATH-resolve  |          | Mount-aware; libc-heavy.                                   |
| `dir`       | PATH-resolve  |          | `ls` alias; we vendor `ls`.                                |
| `dircolors` | PATH-resolve  |          | Config helper for `ls`.                                    |
| `dirname`   | Vendor:Tier-2 | PLAN_09b | Trivial; hot in scripts.                                   |
| `du`        | PATH-resolve  |          | Filesystem traversal — defer.                              |
| `echo`      | (see §11.1)   |          | Bash builtin.                                              |
| `env`       | Vendor:Tier-2 | PLAN_09b | Trivial; hot in shebangs.                                  |
| `expand`    | PATH-resolve  |          |                                                            |
| `expr`      | PATH-resolve  |          | Shell handles most expr cases natively.                    |
| `factor`    | PATH-resolve  |          | Niche.                                                     |
| `false`     | (see §11.1)   |          | Bash builtin.                                              |
| `fmt`       | PATH-resolve  |          |                                                            |
| `fold`      | PATH-resolve  |          |                                                            |
| `groups`    | PATH-resolve  |          |                                                            |
| `head`      | Vendor:Tier-2 | PLAN_09b | Hot in pipelines.                                          |
| `hostid`    | PATH-resolve  |          |                                                            |
| `id`        | PATH-resolve  |          | Defer; minor UX win.                                       |
| `install`   | PATH-resolve  |          |                                                            |
| `join`      | PATH-resolve  |          |                                                            |
| `kill`      | (see §11.1)   |          | Bash builtin; coreutils version is for non-shell callers.  |
| `link`      | PATH-resolve  |          |                                                            |
| `ln`        | Vendor:Tier-2 | PLAN_09b | Filesystem core.                                           |
| `logname`   | PATH-resolve  |          |                                                            |
| `ls`        | Vendor:Tier-2 | PLAN_09b | `lsd`-style output is a stated v1 goal.                    |
| `md5sum`    | PATH-resolve  |          | Crypto sum.                                                |
| `mkdir`     | Vendor:Tier-2 | PLAN_09b | Filesystem core.                                           |
| `mkfifo`    | PATH-resolve  |          | Rare.                                                      |
| `mknod`     | PATH-resolve  |          | Rare; privilege-sensitive.                                 |
| `mktemp`    | Vendor:Tier-2 | PLAN_09b | Hot in scripts.                                            |
| `mv`        | Vendor:Tier-2 | PLAN_09b | Filesystem core.                                           |
| `nice`      | PATH-resolve  |          |                                                            |
| `nl`        | PATH-resolve  |          |                                                            |
| `nohup`     | PATH-resolve  |          |                                                            |
| `nproc`     | PATH-resolve  |          |                                                            |
| `numfmt`    | PATH-resolve  |          |                                                            |
| `od`        | PATH-resolve  |          |                                                            |
| `paste`     | PATH-resolve  |          |                                                            |
| `pathchk`   | PATH-resolve  |          |                                                            |
| `pinky`     | PATH-resolve  |          |                                                            |
| `pr`        | PATH-resolve  |          |                                                            |
| `printenv`  | Vendor:Tier-2 | PLAN_09b | Trivial.                                                   |
| `printf`    | (see §11.1)   |          | Bash builtin.                                              |
| `ptx`       | PATH-resolve  |          |                                                            |
| `pwd`       | (see §11.1)   |          | Bash builtin.                                              |
| `readlink`  | Vendor:Tier-2 | PLAN_09b | Filesystem hot path.                                       |
| `realpath`  | Vendor:Tier-2 | PLAN_09b | Filesystem hot path.                                       |
| `rm`        | Vendor:Tier-2 | PLAN_09b | Filesystem core.                                           |
| `rmdir`     | Vendor:Tier-2 | PLAN_09b | Filesystem core.                                           |
| `runcon`    | PATH-resolve  |          | SELinux.                                                   |
| `seq`       | Vendor:Tier-2 | PLAN_09b | Trivial; hot in for-loops.                                 |
| `sha1sum`   | PATH-resolve  |          | Crypto sum.                                                |
| `sha224sum` | PATH-resolve  |          |                                                            |
| `sha256sum` | PATH-resolve  |          |                                                            |
| `sha384sum` | PATH-resolve  |          |                                                            |
| `sha512sum` | PATH-resolve  |          |                                                            |
| `shred`     | PATH-resolve  |          |                                                            |
| `shuf`      | PATH-resolve  |          |                                                            |
| `sleep`     | Vendor:Tier-2 | PLAN_09b | Trivial.                                                   |
| `sort`      | Vendor:Tier-2 | PLAN_09b | Hot in pipelines.                                          |
| `split`     | PATH-resolve  |          |                                                            |
| `stat`      | PATH-resolve  |          | Platform-specific quirks.                                  |
| `stdbuf`    | PATH-resolve  |          |                                                            |
| `stty`      | PATH-resolve  |          | Terminal control — PLAN_04 owns the interactive side.      |
| `sum`       | PATH-resolve  |          |                                                            |
| `sync`      | PATH-resolve  |          |                                                            |
| `tac`       | PATH-resolve  |          |                                                            |
| `tail`      | Vendor:Tier-2 | PLAN_09b | Hot in pipelines and log-following.                        |
| `tee`       | Vendor:Tier-2 | PLAN_09b | Small; hot in pipelines.                                   |
| `test`      | (see §11.1)   |          | Bash builtin.                                              |
| `timeout`   | PATH-resolve  |          | Wraps execve+SIGALRM.                                      |
| `touch`     | Vendor:Tier-2 | PLAN_09b | Filesystem core.                                           |
| `tr`        | Vendor:Tier-2 | PLAN_09b | Hot in pipelines.                                          |
| `true`      | (see §11.1)   |          | Bash builtin.                                              |
| `truncate`  | PATH-resolve  |          |                                                            |
| `tsort`     | PATH-resolve  |          |                                                            |
| `tty`       | PATH-resolve  |          |                                                            |
| `uname`     | PATH-resolve  |          | Hot but the GNU output is canonical.                       |
| `unexpand`  | PATH-resolve  |          |                                                            |
| `uniq`      | Vendor:Tier-2 | PLAN_09b | Hot in pipelines.                                          |
| `unlink`    | PATH-resolve  |          |                                                            |
| `uptime`    | PATH-resolve  |          |                                                            |
| `users`     | PATH-resolve  |          |                                                            |
| `vdir`      | PATH-resolve  |          | `ls -l` alias.                                             |
| `wc`        | Vendor:Tier-2 | PLAN_09b | Hot in pipelines.                                          |
| `who`       | PATH-resolve  |          |                                                            |
| `whoami`    | PATH-resolve  |          |                                                            |
| `yes`       | Vendor:Tier-2 | PLAN_09b | Trivial.                                                   |

### 11.3. Non-coreutils externals considered

These are not part of coreutils but are heavy hitters worth a row:

| Command                 | Disposition               | Plan     | Notes                                                            |
| ----------------------- | ------------------------- | -------- | ---------------------------------------------------------------- |
| `find`                  | Vendor:Tier-2-partial     | PLAN_09c | `fd`-style UX; common flags vendored, exotic flags PATH-resolve. |
| `xargs`                 | Vendor:Tier-2             | PLAN_09c | Small; hot.                                                      |
| `grep`                  | Vendor:Tier-2-partial     | PLAN_09c | `ripgrep`-style UX; basic flag surface.                          |
| `sed`                   | Vendor:Tier-2-partial     | PLAN_09c | Most common transforms only.                                     |
| `awk`                   | Never                     |          | Full POSIX awk is its own interpreter; out of scope.             |
| `git`                   | PATH-resolve              |          | Provided by user environment.                                    |
| `mount` / `umount`      | PATH-resolve              |          | util-linux; privilege-sensitive.                                 |
| `ps` / `top`            | PATH-resolve              |          | procps; OS-specific.                                             |
| `tar` / `gzip` / `zstd` | PATH-resolve              |          | Dedicated tools.                                                 |
| `which`                 | (use bash `type` builtin) |          | `type -P` covers it.                                             |

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
when PLAN_06b implements `${var:-default}`, every
`deferred:PLAN_06b` case for that feature flips to a `RECLASSIFY`
notice; the 06b PR updates them to `pass` and the v1 pass-rate ticks
up by a measurable amount.

### 12.2. Status field is the work plan

`cargo xtask compat --status deferred:PLAN_06b` lists every case
PLAN_06b is responsible for. The list is the literal task inventory
for that document. When the list is empty (every `deferred:PLAN_06b`
case has been reclassified to `pass`), PLAN_06b's exit criterion is
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

- PLAN_06a is implemented (✓ merged 2026-05-21 as `3ebec50`).
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
  stdout/stderr). The PLAN_06a `exec/testing.rs` helper is rewritten
  on top of the new `ExecEnv` shape and de-`#[allow(dead_code)]`-ed.

- **05.3** Pin bash and coreutils in `flake.nix` explicitly (current
  unstable: bash 5.3.9, coreutils 9.7). Add a regression test in
  `xtask` that asserts the pinned versions match
  `tests/spec/REFERENCE.md`.

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
  follow-up (PLAN_05-tier2 if needed, or merged into PLAN_06b's
  exit criteria).
- Tier-3 real-world script selection. Owned by PLAN_13 (milestones).
- L4 PTY harness. Owned by PLAN_07.
- The `external_command_policy` removal once PLAN_06b lands native
  execve. Owned by PLAN_06b.
- Any actual implementation of builtins or executor features from
  §11. Owned by PLAN_06b, PLAN_09a, PLAN_09b, PLAN_09c.

### 13.4. Verification per subtask

Every subtask runs the full verification suite per AGENTS.md:

1. `cargo test --all`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo-machete`
4. After 05.6 lands: `cargo xtask compat` produces a report.

## 14. Implementation log

To be filled as subtasks complete, one row per subtask, format
matching PLAN_06a §11.

| Subtask | Commit | Date       | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| ------- | ------ | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 05.1    | TBD    | 2026-05-21 | Added `ExternalCommandPolicy` enum (`FallbackToSh` default, `Strict`) and routed the dispatcher through it. `ExecEnv::from_process()` defaults to `FallbackToSh`; `ExecEnv::sandboxed()` defaults to `Strict`. New `ExecError::NoExternalExecutor { command, reason }` variant with `NoExternalExecutorReason::{PolicyStrict, UnparsableArgv}`. 12 new unit tests + 1 integration test cover the strict path; existing tests opt into `FallbackToSh` via the test helper. Workspace: 207 unit / 5 integration tests passing; clippy clean (one scoped `needless_pass_by_ref_mut` allow on `dispatch_line` with a forward-compat rationale for 05.2 / 06b mutations); machete clean.                                                                                                                                                                                                                                                                                        |
| 05.2    | TBD    | 2026-05-21 | Moved stdio onto `ExecEnv` as `stdout: Box<dyn Write + Send>` / `stderr: Box<dyn Write + Send>`, defaulting to `io::stdout()` / `io::stderr()` in both constructors. Manual `Debug` impl renders writers as `"<dyn Write>"`. Removed the `Capture` enum from `dispatch_script`; `spawn_via_sh` now always pipes child stdio and copies it through the env writers (uniform path; `PLAN_06b` reclaims the extra copy via inherited fds when writers are real stdio). New `exec::testing` module (gated `#[cfg(test)]`) ships `SharedBuf` (`Arc<Mutex<Vec<u8>>>` newtype, `Write` + `Clone`) and `run_source_capturing`, which swaps shared sinks onto a caller-supplied env, runs `parse + dispatch_script`, and restores prior writers on both success and `RunError::Parse` paths. Bench `exec_roundtrip_parse_and_exec` opts into `FallbackToSh` for the strict-default sandbox env. 210 lib + 4 smoke + 5 prompt + 112 ansi tests passing; clippy clean; machete clean. |

## 15. Cleanup registry

To be filled if any subtask surfaces a pre-existing bug per the
AGENTS.md "pre-existing bugs surfaced during a subtask" rule.

| ID  | Surface | Impact | Fix scope | Status |
| --- | ------- | ------ | --------- | ------ |

## References

- `Documents/decisions/0003-test-first-compatibility-methodology.md`
  — the methodology this document operationalizes.
- `Documents/PLAN_01_philosophy.md` — goals G1, G2 and non-goal NG1
  define what compatibility means.
- `Documents/PLAN_02_architecture.md` — the architecture that
  satisfies the constraints in §5.
- `Documents/PLAN_06a_exec_skeleton.md` — implemented; supplies the
  `ExecEnv`, `run_source`, and dispatcher this document mutates.
- `Documents/PLAN_06b_executor.md` (Phase B stub) — the real
  executor; owner of most `deferred:PLAN_06b` cases.
- `Documents/PLAN_07_interactive_ux.md` (pending) — owner of the
  L4 PTY harness referenced in §6.3 and the `fc`/`history`/`bind`
  builtins.
- `Documents/PLAN_09_builtins.md` (Phase B stub) — owner of tier-2
  builtin parity targets and the rows in §11 marked PLAN_09a /
  PLAN_09b / PLAN_09c.
- `Documents/PLAN_13_milestones.md` (Phase B stub) — milestone 1
  ships this harness plus the L3 layer at whatever pass-rate.
- `AGENTS.md` — testing philosophy, panic-free production code
  rules, and crate-status table that this document extends.
