# PLAN_09 — Grammar-Aware Fuzzer and Differential Oracle

> Last updated: 2026-05-22 — initial draft.
> Phase: B. Status: stub (drafted; implementation pending).
> Consumes: PLAN_05 §3 corpus structure; PLAN_08 spec sheets;
> ADR 0003 test-first methodology. Consumed by: PLAN_06 Phase B
> (gating dependency 06.0 — differential green before any Phase B
> subtask lands); PLAN_10 §11 subtask 10.12; PLAN_15 milestones.

PLAN_05 gives fredshell a hand-curated corpus. PLAN_08 gives every
behaviour a prose contract. Both are essential and both are
finite — a human can write a few thousand cases over a release
cycle. fredshell needs more than that.

PLAN_09 is the machine that produces the rest. It is two things in
one document:

1. **A grammar-aware fuzzer** that generates syntactically valid
   bash inputs in known categories at known rates.
2. **A differential oracle** that runs each generated input
   through fredshell and through a pinned bash binary, then
   compares stdout, stderr, and exit status to flag divergences.

The fuzzer alone is uninteresting — random strings rarely
exercise interesting code paths. The differential alone is
uninteresting — there is nothing to compare against without
inputs. Together they form the third leg of the testing tripod
(unit + spec corpus + differential), and they are the artifact
that lets fredshell claim "bash compatibility" without
hand-writing 50,000 cases.

PLAN_09 is Phase B because the fuzzer's grammar weights and the
oracle's category coverage are tuned against the PLAN_08 spec
sheets. Sheets do not exist during Phase A.

## 1. Scope and non-scope

### In scope (v1)

- **Grammar-aware input generation.** A grammar definition
  (`fuzzer/grammar.rs`) that produces strings drawn from a
  weighted context-free-grammar approximation of bash's input
  language.
- **Deterministic replay.** Every fuzzer run is parameterised by
  a seed; the same seed always produces the same inputs in the
  same order. Backed by `rand_chacha::ChaCha20Rng`,
  `ChaCha20Rng::seed_from_u64`. No `rand::thread_rng()` anywhere.
- **Differential execution.** Each generated input is fed to
  fredshell and to a pinned bash subprocess; outputs are
  compared byte-for-byte. Pinned bash version lives in a
  workspace-level env file (already used by the spec runner).
- **Divergence triage.** Differences are normalised through a
  configurable filter pipeline (e.g., PID redaction, timing
  noise) and persisted as new `tests/spec/fuzz/<hash>.case.toml`
  cases. Cases are marked `deferred:fuzz` initially; a human
  must triage them before they are promoted to `deferred:PLAN_06`
  or `wontfix`.
- **Five fuzz tiers (F1–F5).** Tiered by complexity; see §3.
- **CI integration.** F1 runs on every PR; F2 runs nightly; F3
  runs weekly; F4 and F5 are operator-initiated.
- **Coverage reports.** What fraction of grammar productions
  were exercised in a run; which fredshell modules saw new line
  coverage. Reports are HTML, written to
  `target/fuzz-reports/<seed>/`.

### Out of scope (v1)

- **Random byte-stream fuzzing.** Tools like `cargo-fuzz` and
  `afl++` are general-purpose. They are good at finding crashes
  but bad at finding semantic divergence. PLAN*09 may
  \_eventually* integrate `cargo-fuzz` for crash discovery, but
  v1 is grammar-aware only.
- **Coverage-guided fuzzing.** Sanitizer-driven mutation (as
  `libfuzzer` and `honggfuzz` do) is great when the goal is
  crash discovery. It is the wrong tool for behavioural
  parity; we want _semantic_ diversity, not _control-flow_
  diversity. Out of scope.
- **POSIX compliance testing.** PLAN_09 differentially tests
  against bash, not against POSIX. A divergence between bash
  and POSIX is bash's choice; fredshell follows bash (per
  PLAN_01 G1).
- **External-binary fuzzing.** The oracle does not run
  external utilities (`grep`, `awk`, `sed`) inside fuzzed
  scripts. Generated scripts use only shell-internal
  primitives, builtins, and a fixed allowlist of guaranteed-
  available coreutils (`echo`, `true`, `false`, `cat`).
- **Distributed fuzzing.** v1 runs on one machine. If we ever
  need to fan out, that is a separate plan.
- **Mutation-based fuzzing of an existing seed corpus.**
  Mutation has its place; v1 prefers fresh generation because
  it is easier to reason about coverage. We may add mutation
  later if generation plateaus.

The boundary rule: **PLAN_05 owns hand-written cases; PLAN_09
owns machine-generated cases.** They share the same on-disk
format (TOML `.case.toml`). They share the same runner
(`fredshell-spec-runner`). They do _not_ share the same
provenance: a hand-written case is a deliberate design
assertion, a fuzz-derived case is empirical evidence that
needs triage.

## 2. Architecture

### 2.1. Crate placement

PLAN_09 introduces one new crate:

```text
crates/fredshell-fuzz/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── grammar/
│   │   ├── mod.rs              // Grammar, Production, Symbol
│   │   ├── corpus.rs           // builtin- and feature-keyed weights
│   │   ├── bash_v1.rs          // the v1 bash grammar definition
│   │   └── tests.rs
│   ├── generator.rs            // Grammar + seed -> Vec<Input>
│   ├── oracle/
│   │   ├── mod.rs              // run_differential, Divergence
│   │   ├── normalize.rs        // PID/timing/tempdir filters
│   │   └── tests.rs
│   ├── triage.rs               // Divergence -> case file or quarantine
│   ├── report.rs               // coverage + divergence HTML
│   └── tiers.rs                // F1..F5 configurations
└── tests/
    └── golden_seed_1234.rs     // seed 1234 produces these N inputs
```

The crate depends on `fredshell-core` (to drive in-process
execution), on `fredshell-spec-runner` (for the case-file
format), on `rand_chacha`, and on `serde` + `toml` (for case
files and reports). It does _not_ depend on `fredshell` (the
binary), per the AGENTS.md dependency-direction rule.

`anyhow` is forbidden here (library crate). The error type is
`enum FuzzError`, defined in `lib.rs`.

### 2.2. Driver

`cargo xtask fuzz` is the operator entry point:

```text
cargo xtask fuzz --tier F1                # default; quick
cargo xtask fuzz --tier F3 --seed 4242
cargo xtask fuzz --tier F2 --seed-from-env
cargo xtask fuzz --replay target/fuzz-reports/<seed>/inputs.txt
cargo xtask fuzz --triage target/fuzz-reports/<seed>/divergences/
```

`--seed-from-env` reads `FREDSHELL_FUZZ_SEED`. `--replay`
reproduces a prior run from a logged input list (which the
oracle always writes). `--triage` opens the divergence
directory and presents them one-by-one for promotion to corpus
case or quarantine.

`xtask fuzz` is the only public entry point. The
`fredshell-fuzz` crate exposes no `main`. This keeps the
release binary lean and prevents accidental fuzzing in
production.

### 2.3. Determinism contract

The single hardest property to maintain in a fuzzer is
determinism. PLAN_09 enforces it three ways:

1. **One RNG.** The driver creates one `ChaCha20Rng` from the
   seed at startup and threads it through every component. No
   component creates its own RNG. No `thread_rng` is allowed
   (enforced by a `deny(clippy::disallowed_methods)` lint with
   the relevant entries).
2. **Stable iteration.** Maps used during generation are
   `BTreeMap`, not `HashMap`. Sets are `BTreeSet`. The
   generator never relies on hash iteration order.
3. **Captured environment.** Generated inputs run in a
   sandboxed `ExecEnv` with a fixed `cwd` (a per-run tempdir),
   a fixed `env` map (only the variables in `oracle/env_allow.rs`),
   and a fixed locale (`LC_ALL=C`). The wall clock and the
   PID are the only sources of non-determinism the oracle
   sees, and both are filtered by `normalize.rs`.

Determinism is tested by a golden-file test
(`tests/golden_seed_1234.rs`) that asserts seed 1234 always
produces a specific list of inputs. Breaking the determinism
contract breaks this test.

## 3. The five fuzz tiers

Tiers are not difficulty levels — they are time/coverage
budgets. Each tier has a target number of inputs, a wall-clock
budget, and a grammar-weighting profile.

| Tier | Inputs | Wall clock | Grammar profile            | Where run       |
| ---- | ------ | ---------- | -------------------------- | --------------- |
| F1   | 1k     | <30 s      | uniform over §4 grammar    | every PR (CI)   |
| F2   | 10k    | <5 min     | uniform                    | nightly CI      |
| F3   | 100k   | <2 h       | uniform                    | weekly CI       |
| F4   | 1M     | <24 h      | weighted toward `defer:`   | operator-driven |
| F5   | 10M    | <1 week    | exhaustive (depth-bounded) | release gate    |

### 3.1. F1 — PR sanity

The smallest tier; the goal is "does any PR introduce a
regression in basic parser/expander behaviour?" 1k inputs is
small enough that a bad seed will not block a PR for an hour.
F1 is allowed to add or update entries in
`tests/spec/fuzz/regressions/` but not to promote divergences;
that is a human step.

### 3.2. F2 — nightly soak

10k inputs is enough to find shallow grammar gaps in 2–5
minutes. Run on `main` only, not on PRs. Divergences are
auto-filed as GitHub issues with the seed and the minimised
input.

### 3.3. F3 — weekly sweep

100k inputs. Targets one bash semantic category per week
(round-robin through PLAN_08's feature sheet list). Run
overnight; results posted to a `Documents/fuzz_reports/`
directory in-repo.

### 3.4. F4 — pre-release deep dive

1M inputs with a grammar profile that up-weights deferred-row
categories (the things PLAN_08 says we will eventually
support). Used before each PLAN_15 milestone to catch
regressions in surface area we are about to claim.

### 3.5. F5 — release gate

10M inputs run before any release tagged `vN.0.0`. This is the
"are we really bash-compatible enough to call it 1.0?" tier.
F5 may use depth-bounded exhaustive enumeration over the
grammar in addition to random sampling.

## 4. Grammar definition

### 4.1. Approach

The grammar is a context-free grammar with weighted productions
plus a depth limit. It is _not_ a full bash grammar — bash's
real grammar is context-sensitive in awkward places (e.g.,
`[[` is a reserved word only at the start of a command).
PLAN*09's grammar approximates bash and then post-validates
each generated input by running it through fredshell's own
parser. Inputs that fredshell's parser rejects are discarded
\_before* being sent to the differential oracle. (Parser
divergence is a separate concern; we test it directly with
the spec corpus, not with the fuzzer.)

### 4.2. Top-level structure

```text
script  := command (separator command)*
command := simple_command
         | pipeline
         | compound_command
         | function_def
         | variable_assignment

simple_command := word+ redirection*
pipeline       := command ('|' command)+
compound_command := if_stmt | while_stmt | for_stmt | case_stmt
                  | brace_group | subshell | conditional | arithmetic

separator := ';' | '&' | newline | '&&' | '||'

word := unquoted_word | single_quoted | double_quoted
      | ansi_c_quoted | command_substitution | parameter_expansion
      | arithmetic_expansion | brace_expansion | tilde_prefix

# ... continued in fuzzer/grammar/bash_v1.rs ...
```

Each terminal has a small alphabet (typically `[a-z]` or `[0-9]`
plus a few specials) to keep generated inputs readable in
divergence reports. Identifiers are drawn from a 16-element
namespace (`a`, `b`, `c`, …, `p`) so collisions and
shadowing actually happen.

### 4.3. Weighting profiles

The weight table maps each production to a numeric weight.
Profiles override weights:

- **uniform**: every production weight = 1. This is F1–F3.
- **deferred-heavy**: PLAN_08 `defer:` rows have weight 4;
  `wontfix` rows have weight 0; `support` rows have weight 2.
  This is F4.
- **exhaustive**: depth-bounded enumeration replaces weighted
  random sampling. Up to depth D, every production is visited
  exactly once. D is 6 in v1; F5 may push it to 8 if the
  budget allows.

Weight tables live in `grammar/corpus.rs` and are loaded from
the PLAN_08 sheet front-matter (each sheet's `Sources` line
gains an optional `fuzz-weight: N` field). When a sheet is
added or its classification changes, the weight table updates
automatically.

### 4.4. Depth limit

Without a depth limit, weighted grammar sampling produces
runaway inputs (a `command` may contain a `command` may contain
…). The limit is per-derivation: each non-terminal expansion
decrements a budget that starts at 16. When the budget hits
zero, only terminal-producing productions are allowed.

This produces inputs that are _wide_ rather than _deep_: lots
of simple commands separated by `;`, not deeply nested
`( ( ( ... ) ) )`. That matches real-world script statistics.

## 5. Differential oracle

### 5.1. Execution model

For each generated input string `S`:

1. Run `S` through fredshell's parser. Reject if it fails to
   parse (do not count as a divergence — parser bugs are
   tracked separately).
2. Spawn `bash --noprofile --norc -c "$S"` in the sandbox
   environment. Capture stdout, stderr, exit status, wall
   clock.
3. Drive `S` through fredshell's in-process executor with the
   same sandbox environment. Capture stdout, stderr, exit
   status, wall clock.
4. Pass both outputs through `normalize.rs` (§5.2).
5. Compare. If any of (stdout, stderr, exit) differ, record a
   `Divergence` (§5.3).

### 5.2. Normalisation filters

Outputs are normalised before comparison to eliminate
unavoidable, semantically-irrelevant differences:

- **PID redaction.** Any decimal integer that appears in
  output and equals a known child PID is replaced with the
  literal `<PID>`. Same for parent PID and `$$`.
- **Tempdir redaction.** Any absolute path under the run's
  tempdir is replaced with `<TMPDIR>`.
- **Wall-clock redaction.** `time` builtin output, `date`
  output, and any timestamp-like patterns are replaced with
  `<TIME>`. (date is not in the allowlist so this is rare.)
- **Locale-stable error format.** Both processes run with
  `LC_ALL=C`, but bash's error messages sometimes vary by
  build (the `bash:` prefix may be `bash 5.3:` in some
  packagings). The normaliser strips a leading `bash:`,
  `bash 5.3:`, or `fredshell:` so the comparison is on the
  error text, not the prefix.
- **Trailing-whitespace tolerance.** A single optional
  trailing `\n` difference is ignored.

Normalisation rules live in `oracle/normalize.rs` and are
tested via golden files (`tests/normalize_*.rs`).

### 5.3. Divergence record

When outputs differ, the oracle writes a
`Divergence` to `target/fuzz-reports/<seed>/divergences/<hash>.toml`:

```toml
hash = "sha256-of-input"
input = "<the input string>"
seed = 4242
tier = "F2"
generated_at = "2026-05-22T14:32:00Z"

[bash]
stdout = "..."
stderr = "..."
exit = 0

[fredshell]
stdout = "..."
stderr = "..."
exit = 0

[diff]
stdout_diff = "..."   # unified diff
stderr_diff = "..."
exit_diff = "0 -> 1"

[triage]
status = "open"        # open | promoted | wontfix | flake
notes = ""
```

The `hash` is the SHA-256 of the input string, which is also
the filename. Re-encountering the same divergence later
overwrites the file (timestamps, seed updated) but does not
spam the report directory.

### 5.4. Minimisation

A divergence's input is minimised before being filed. The
minimiser runs in `oracle/minimize.rs`:

1. Start with the original input.
2. Try removing each top-level command in turn; keep the
   removal if the divergence still reproduces.
3. Try shortening identifiers (rename `xyz` to `a`).
4. Try collapsing whitespace.
5. Stop when no single edit further reduces size while still
   reproducing the divergence.

Minimisation is bounded: 30 seconds per input. Inputs that
fail to minimise within the budget are filed with the
original (unminimised) form and flagged `[unminimised]`.

## 6. Triage workflow

A divergence file is _evidence_, not a corpus case. Promotion
to a corpus case is a human decision.

`cargo xtask fuzz --triage <dir>` opens each `divergence
<hash>.toml` and presents:

- The minimised input.
- The diffs.
- A best-guess classification (PLAN_08 sheet name and row
  number, if the divergence overlaps a row).

The operator chooses one of:

1. **promote** — copy to `tests/spec/fuzz/<category>/<name>.case.toml`
   with `status = "deferred:PLAN_06"` (or whatever PLAN owns
   it). The triage file is updated with `status = "promoted"`
   and the destination path.
2. **wontfix** — record a wontfix justification in the PLAN_08
   sheet's §3 table (adds a new row). The triage file is
   updated; the input is added to `tests/spec/refusals/` as a
   refusal corpus case.
3. **flake** — the divergence does not reproduce in isolation
   (timing, OS state, scheduler). The input is moved to
   `target/fuzz-reports/quarantine/`; we do not file it but we
   do log the seed for later investigation.

A divergence may not be left in `open` status indefinitely: a
weekly CI job warns if any divergence file in
`target/fuzz-reports/` has been `open` for more than 7 days.

## 7. Coverage reporting

Two coverage signals matter:

### 7.1. Grammar coverage

For each tier run, the report lists what fraction of grammar
productions were exercised at least once. Target: F1 ≥ 60%,
F2 ≥ 85%, F3 ≥ 95%, F4 ≥ 99%, F5 = 100%.

A production that is never exercised after F3 is either
unreachable (a grammar bug) or grossly under-weighted
(a profile bug). Either way it is flagged.

### 7.2. Source coverage

Lines/branches in `fredshell-core` exercised by fuzzer inputs,
measured via the existing `cargo xtask coverage` infrastructure
(`cargo-llvm-cov`). The fuzzer's coverage delta vs. the spec
corpus alone is a useful "is the fuzzer finding new code paths"
signal.

Both reports are HTML. Both are uploaded as CI artifacts.

## 8. Subtasks

| Subtask | Surface                                            | Gate       |
| ------- | -------------------------------------------------- | ---------- |
| 09.1    | Crate scaffold (`fredshell-fuzz`); `FuzzError`     | none       |
| 09.2    | Grammar definition (`grammar/bash_v1.rs`) + tests  | 09.1       |
| 09.3    | Generator (seed → `Vec<Input>`) + golden seed test | 09.2       |
| 09.4    | Oracle (`run_differential`) + normalisation        | 09.3       |
| 09.5    | Minimiser + tests                                  | 09.4       |
| 09.6    | `cargo xtask fuzz` driver (F1 only)                | 09.4, 09.5 |
| 09.7    | F1 CI integration on every PR                      | 09.6       |
| 09.8    | F2 nightly + auto-issue filer                      | 09.6       |
| 09.9    | Triage CLI (`xtask fuzz --triage`)                 | 09.6       |
| 09.10   | F3/F4/F5 tier configurations + reports             | 09.6, 09.9 |
| 09.11   | Coverage report integration                        | 09.7       |
| 09.12   | Promote first fuzz-derived case to `pass` (smoke)  | 09.9       |

Subtasks 09.1–09.7 are the **PLAN_06 Phase B gate**. Once F1 is
green on `main`, the 06.0 gate is satisfied and Phase B
subtasks may begin landing.

## 9. Performance contract

The fuzzer is the most performance-sensitive subsystem outside
of the hot REPL path. Budgets:

- One input from generator: < 1 ms (P50), < 10 ms (P99).
- One differential round: < 100 ms (P50), < 1 s (P99). The
  bottleneck is bash subprocess spawn; fredshell in-process is
  faster.
- F1 (1k inputs, single thread): < 30 s wall.
- F2 (10k inputs, 4 threads): < 5 min wall.

Threading is per-input: generator → oracle is parallelised
across N threads (default: `num_cpus`). The RNG is per-thread,
but its seed is derived deterministically from the master seed
plus the thread index, so per-thread determinism is preserved.

## 10. Relationship to bash version

The pinned bash version lives in `tools/bash-version.txt` (one
line, current value `5.3p9`). The CI Nix derivation provides
exactly that version on `$FREDSHELL_REFERENCE_BASH`. Updating
the pinned bash is a deliberate change with its own PR:

1. Update `tools/bash-version.txt`.
2. Update the Nix derivation.
3. Run F3 on the new bash and triage any new divergences.
4. Land the PR.

Bash updates are expected to introduce divergences — when
bash fixes a bug, fredshell needs to follow. PLAN_09 makes
this discoverable rather than surprising.

## 11. Open questions

- **Q09.1** — Should generated inputs run with a fixed
  `umask`? Default: yes, `umask 022` for both processes,
  documented in §2.3. Alternative: leave umask uncontrolled
  and add an output-mode-redaction filter. The fixed-umask
  choice is simpler.
- **Q09.2** — Should we run the oracle against `dash` and
  `mksh` in addition to `bash`? POSIX-only divergences with
  bash are interesting because they tell us where bash itself
  is non-portable. Default: not in v1, but the oracle API is
  general enough to add this later.
- **Q09.3** — How much of the grammar should respect
  user-defined functions and aliases? Defining a function and
  then calling it is a useful pattern, but it doubles the
  effective grammar depth and complicates normalisation.
  Default: functions are in grammar; aliases are not (aliases
  interact poorly with our parser stage gating).
- **Q09.4** — Should fuzz-derived cases live under
  `tests/spec/fuzz/` (separate top-level category) or be
  interleaved with hand-written cases under the natural
  category (e.g., `tests/spec/parameter_expansion/`)? Default:
  separate `fuzz/` subtree for clarity; the case file's
  `[meta]` block records its provenance.
- **Q09.5** — `LC_ALL=C` is the obvious locale, but some
  bash quirks only appear under UTF-8 locales (notably
  `$'...'` and pattern matching). Should there be a UTF-8
  fuzz tier? Probably yes, eventually. Tracked.

## 12. Relationship to other plans

- **PLAN_05** — corpus harness; PLAN_09 produces additional
  cases consumed by the same runner. Status taxonomy gains
  `deferred:fuzz` for unminified pending triage.
- **PLAN_06 Phase B** — gating dependency. Subtask 06.0 is
  "PLAN_09 F1 green on `main`."
- **PLAN_08** — sheets drive grammar weights (§4.3) and
  classify wontfix divergences (§6).
- **PLAN_10** — job-control divergences are filtered by the
  grammar profile (no `&` in F1 to avoid timing flakes).
- **PLAN_15** — F4 and F5 are gates on milestone transitions.
- **ADR 0003** — establishes differential testing as the
  methodology; this document is the implementation.

## 13. References

- `Documents/PLAN_05_testing.md` — §3 corpus categories, §12
  status taxonomy.
- `Documents/PLAN_06_exec.md` — §13 Phase B gating on
  PLAN_09 F1.
- `Documents/PLAN_08_spec_drafting.md` — sheet front-matter
  consumed by `grammar/corpus.rs`.
- `Documents/PLAN_10_traps_and_jobs.md` — categories the
  fuzzer carefully avoids in F1 (job control, traps).
- `Documents/decisions/0003-test-first-compatibility-methodology.md`
  — establishes differential testing as the methodology.
- `rand_chacha` crate documentation, `ChaCha20Rng::seed_from_u64`.
- bash reference manual, the "DEFINITIONS" and
  "SHELL GRAMMAR" sections.
