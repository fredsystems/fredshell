# PLAN_05 — Testing and the Spec Corpus

> Last updated: 2026-05-20 — first draft.
> Phase: A. Status: draft.
> Operationalizes ADR 0003.

This document defines how fredshell tests its own behavior. It is the
concrete realization of ADR 0003 (test-first compatibility methodology)
and the first planning document drafted in detail, because the harness
described here imposes hard constraints on every later document —
notably PLAN_02 (architecture), PLAN_06 (bash compat), PLAN_09
(builtins), and PLAN_13 (milestones).

If something in this document conflicts with a later plan document, this
document wins until ADR 0003 is superseded.

## 1. Why testing comes before architecture

A shell is largely defined by the behavior of programs it runs. That
behavior is observable (stdout, stderr, exit status, side effects on the
filesystem and environment) but not deducible from prose. Past
experience shows that without a continuous, executable definition of
"correct," shell projects accumulate ad-hoc patches matched to whatever
the author last noticed was broken, and the answer to "what do you
support?" requires reading the source.

fredshell rejects that path. The harness exists first; it runs in CI
from day one at whatever pass-rate it produces, including 0%; and every
later design document is written knowing what it will be measured
against.

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
(script, env, expected_stdout, expected_stderr, expected_exit)
```

`script` is a bash-language program of arbitrary length, typically a
handful of lines. `env` is a description of the sandboxed execution
environment (working directory contents, environment variables, `$HOME`
location, `$PATH` value). The expected outputs are byte-exact
recordings of what real bash produces when given the same script in the
same environment.

A spec test **passes** when fredshell, given the same script and
environment, produces stdout, stderr, and exit status equal to the
recorded expected values. A spec test **fails** when any of the three
differs.

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

Rationale for TOML over JSON or a custom DSL: human-editable, multi-line
strings are first-class, no escape-quoting hell, already in the Rust
ecosystem. Rationale for one-file-per-case over a directory: a single
`grep` reveals every test mentioning a feature.

### 3.3. Three corpus tiers

Per ADR 0003, the corpus is sourced from three tiers with distinct
licensing and CI policies:

#### Tier 1 — fredshell's own corpus

- Lives in-tree at `tests/spec/`.
- Hand-curated, MIT-licensed alongside the rest of the codebase.
- Organized by feature category (see section 3.4).
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
  `echo`, `printf`, `test`, `[`.
- `builtins_tier2_<name>` — one category per tier-2 replacement
  builtin (one for `ls`, one for `cat`, etc.). Validates parity with
  the corresponding coreutils program.
- `error_handling` — `set -e`, `set -u`, `set -o pipefail`, `||
return`, `trap … ERR`, edge cases bash is known to handle badly.

Each category lives in a directory under `tests/spec/`. A category may
contain subdirectories for sub-features when it grows beyond ~30 cases.

## 4. The harness

### 4.1. Crate layout

The harness is a Rust crate, `fredshell-spec-runner`, depending only on
`fredshell-core` (the public parser and executor surface) plus standard
testing/utility crates. It exposes:

- A library API for embedding the runner (used by `cargo test` integration
  and by xtask).
- A binary entry point invoked by `cargo xtask compat`.

The harness does **not** depend on the `fredshell` binary crate. It
exercises `fredshell-core` directly. This is a hard constraint and the
primary reason PLAN_02 must keep the parser and executor separable from
the REPL.

### 4.2. Execution model

For each spec test, the harness:

1. Creates a fresh sandbox directory under a per-run scratch root
   (`$CARGO_TARGET_TMPDIR` or equivalent).
2. Materializes `<case>.fs/` into the sandbox if present.
3. Resolves `$SANDBOX` placeholders in the `env` block to the absolute
   sandbox path.
4. Constructs an isolated execution environment: empty environment
   except the variables in `env`, working directory set to the
   sandbox, `$PATH` containing only what the case requests (no
   inheritance from the host).
5. Invokes the fredshell-core parser+executor on the script in
   non-interactive batch mode, capturing stdout, stderr, and exit
   status to byte buffers.
6. Compares against the expected outputs.
7. Emits a structured result: pass / fail (with diff) / error (harness
   itself failed to set up).
8. Tears down the sandbox.

Sandboxes are torn down on pass and on harness errors. On test failure
the sandbox is preserved under `target/spec-failures/<case-id>/` to
support debugging.

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
fixtures**, not produced live by invoking bash during the test run. The
harness does not require bash to be installed on the machine running
the tests.

A separate xtask command, `cargo xtask spec record <case>`, runs the
case under the pinned reference bash and writes the resulting fixtures
to disk. Authoring a new test case is:

1. Write the `.case.toml`.
2. Run `cargo xtask spec record <case>` to generate fixtures.
3. Review the generated fixtures (they are committed to the repo).
4. Run `cargo xtask compat <case>` to confirm fredshell matches.

Fixture regeneration is deliberate. A blanket `xtask spec record-all`
exists but its output must be reviewed line-by-line before commit.

### 4.5. Reference bash version

The reference bash is pinned per platform in a `tests/spec/REFERENCE.md`
file. v1 baseline:

- Linux: bash 5.2.x as packaged by the fredshell nix flake.
- macOS: bash 5.2.x as packaged by the fredshell nix flake.

The host system bash (e.g. macOS's bash 3.2) is never used as an oracle.
Behavior differences between bash 5.2 and other versions are out of
scope for v1; if a real-world script depends on bash 5.x-only behavior,
that is expected.

### 4.6. Pass-rate reporting

After a run, the harness produces:

- A per-category pass-rate (`parameter_expansion: 42/50`).
- A per-tier pass-rate (`tier-1: 412/450, tier-2: 1100/2400,
tier-3: 14/20`).
- An overall pass-rate.
- A JSON report at `target/spec-report.json` for CI consumption.
- A human-readable summary on stdout.

The JSON schema is stable and versioned. CI compares the current
report against the previous main-branch report. Any tier-1 regression
fails the build. Tier-2 and tier-3 regressions warn but do not fail
unless the affected module is in the `promoted.toml` or
`blocking.toml` list.

## 5. Architectural constraints exported to PLAN_02

These are the constraints the harness imposes on the rest of the
architecture. PLAN_02 must satisfy them.

### 5.1. Separable parser

The parser must be invocable as a pure function:

```rust
fn parse(source: &str) -> Result<Ast, ParseError>
```

with no I/O, no global state, no dependency on the executor. Pure
parser tests (golden AST snapshots) are a separate L2 layer that does
not require the harness machinery.

### 5.2. Sandboxable executor

The executor must accept an explicit environment:

```rust
pub struct ExecEnv {
    pub cwd: PathBuf,
    pub env: HashMap<OsString, OsString>,
    pub stdin: Box<dyn Read + Send>,
    pub stdout: Box<dyn Write + Send>,
    pub stderr: Box<dyn Write + Send>,
    // … extension points for tier-2 builtin overrides, signal mask,
    // path resolution policy, etc.
}

pub fn execute(ast: &Ast, env: &mut ExecEnv) -> Result<ExitStatus, ExecError>
```

The concrete signature evolves, but the shape is non-negotiable: no
implicit globals, no calls to `std::env::var` at the leaves, no
`println!` macros — every byte of output goes through `env.stdout` or
`env.stderr`. PLAN_02 owns the exact API.

### 5.3. Batch-mode entry point

A non-interactive batch entry point exists from day one. The harness
calls it. The REPL is built **on top of** this entry point, not
alongside it. There is never a moment in the development history when
the only way to run a script through fredshell is via the line editor.

### 5.4. Builtin dispatch must be testable

Tier-2 builtins must be invocable directly from the harness without
spinning up a full REPL or process. Each tier-2 builtin exposes an
`invoke(env, args) -> ExitStatus` method that the spec test for that
builtin calls.

## 6. Other test layers

### 6.1. L1 — Unit tests

Standard `#[cfg(test)] mod tests` in every module. Required for every
non-trivial function. Hermetic. Order-independent. Coverage target 100%
across library crates per AGENTS.md.

### 6.2. L2 — Integration tests

`tests/` directories within each crate. Used for cross-module behavior
that does not warrant the full spec corpus apparatus — e.g., parser AST
snapshots, builtin unit-level behavior with mock environments, prompt
renderer with a mock terminal.

### 6.3. L4 — PTY behavior tests

A separate harness exercises the interactive shell through a real PTY.
Owns:

- Line editor behavior (key sequences in → expected screen contents
  out).
- Signal handling (Ctrl-C, Ctrl-Z, SIGWINCH).
- Job control end-to-end.
- Bracketed paste, kitty keyboard, terminal feature negotiation.

This layer is **not** ready in milestone 1. It requires the line editor
to exist. Its design is owned by PLAN_07 (interactive UX) with input
from PLAN_04 (terminal I/O). It is mentioned here because shell
behavior in interactive mode genuinely cannot be exercised by L3, and
deferring the design entirely would be a mistake.

### 6.4. L5 — Benchmarks

Criterion benchmarks at `benches/` per AGENTS.md performance discipline.
Cover the prompt renderer, parser, line edit dispatch, and history
search. Required before/after numbers for any change to those areas.

## 7. CI integration

### 7.1. xtask commands

- `cargo xtask compat` — run the full spec corpus, emit report.
- `cargo xtask compat <category>` — run a single category.
- `cargo xtask compat --tier 1` — restrict to a tier.
- `cargo xtask spec record <case>` — regenerate fixtures for one case.
- `cargo xtask spec record-all` — regenerate everything (review required).
- `cargo xtask spec fetch-oils` — refresh the tier-2 oils-spec lock.
- `cargo xtask spec lint` — validate `.case.toml` schema, check for
  orphaned fixtures, check provenance entries for tier-3.

### 7.2. CI workflow

The compat job runs on every PR and every push to main. It:

1. Restores cached external corpora (oils-spec) keyed on the lock file.
2. Runs `cargo xtask compat` with the full corpus.
3. Compares the JSON report against the last main-branch report
   (downloaded from CI artifacts).
4. Fails the build if tier-1 regresses, or if any promoted tier-2 or
   blocking tier-3 case regresses.
5. Uploads the report and any preserved sandbox failures as CI
   artifacts.

### 7.3. Pass-rate visibility

The current pass-rate per category is rendered to a generated
`COMPAT.md` file at the repo root by `cargo xtask compat --update-readme`.
This file is the user-visible answer to "what does fredshell support?"
and is regenerated on every main-branch build.

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
- Selection skews toward installers and CI helpers (high-impact, broad
  feature surface).

### 8.3. v1 targets (tier 2 oils-spec)

- Aspirational only. Reported, not committed. A baseline number is
  established once the corpus is fetched; v1 commits to "no regression
  from baseline" rather than an absolute floor.

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

These are unresolved as of this draft and will be settled before
implementation begins.

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
- **Coverage of stdin-driven scripts.** Cases that consume stdin need
  an additional field in `.case.toml` (`stdin = "…"`). Schema slot
  reserved; semantics deferred.
- **Promotion criteria for tier-2 modules.** What gates an oils-spec
  module being moved from "reported" to "blocking"? Provisional
  answer: ≥ 90% pass on that module, sustained over two months,
  with an explicit ADR-style decision recorded.

## References

- `Documents/decisions/0003-test-first-compatibility-methodology.md`
  — the methodology this document operationalizes.
- `Documents/PLAN_01_philosophy.md` — goals G1, G2 and non-goal NG1
  define what compatibility means.
- `Documents/PLAN_02_architecture.md` (pending) — the architecture
  that satisfies the constraints in section 5.
- `Documents/PLAN_06_bash_compat.md` (Phase B stub) — the compat
  strategy informed by harness output.
- `Documents/PLAN_07_interactive_ux.md` (pending) — owner of the L4
  PTY harness referenced in section 6.3.
- `Documents/PLAN_09_builtins.md` (Phase B stub) — owner of tier-2
  builtin parity targets that this harness measures.
- `Documents/PLAN_13_milestones.md` (Phase B stub) — milestone 1
  ships this harness plus the L3 layer at whatever pass-rate.
- `AGENTS.md` — testing philosophy and panic-free production code
  rules that this document inherits.
