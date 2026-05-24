# ADR 0003 — Test-First Compatibility Methodology

- Status: accepted
- Date: 2026-05-20
- Supersedes: —
- Superseded by: —

## Context

fredshell's headline compatibility goal is "real-world bash scripts run
unmodified." That goal is unfalsifiable without a concrete, executable
definition of "real-world bash scripts." Past experience on adjacent
projects (notably freminal, where supported ANSI escape sequences were
discovered ad-hoc from log files and the answer to "what do you support?"
required code archaeology) makes it clear that compatibility cannot be
asserted, only measured.

The planning process therefore has to answer two questions before any
design document for the parser, executor, or builtin layer can be
written:

1. What is the corpus of behavior that defines "compatible"?
2. How is conformance against that corpus measured, continuously, from
   the very first commit?

Without those answers, every downstream design document is guessing at
its own success criteria.

## Decision

Adopt a **test-first compatibility methodology**. Concretely:

### 1. The spec corpus is authoritative on behavior

Compatibility is defined by a curated corpus of executable test cases,
not by prose. Each test case is a small bash script paired with its
expected stdout, stderr, and exit status, captured by running the
script under real `bash` on a reference platform. A passing case is one
where fredshell produces the same observable outputs as bash. A failing
case is one where it does not.

The corpus is the source of truth for "what fredshell must do." Prose
in `PLAN_06_exec.md` and elsewhere describes _strategy_; the
corpus describes _behavior_.

### 2. The corpus has three tiers, sourced explicitly

- **Tier 1 — fredshell's own corpus.** Hand-curated, owned in-tree,
  organized by feature (parameter expansion, redirection, arithmetic,
  arrays, control flow, etc.). Primary CI signal. Coverage target for
  v1: every bash feature fredshell claims to support has at least one
  positive and one negative test case.
- **Tier 2 — oils-spec corpus (Apache 2.0).** Fetched at CI time from
  the oils-for-unix project, not vendored. Provides broad coverage of
  POSIX-overlapping behavior maintained by a community with deep shell
  expertise. Used as a secondary signal; regressions are reported but
  not necessarily blocking until fredshell formally adopts a given
  oils-spec module.
- **Tier 3 — curated real-world script corpus.** A small set of actual
  scripts from the wild (installers, dotfiles bootstrap, CI helpers,
  brew/asdf-style version managers) used to validate goal G1
  ("real-world bash scripts run unmodified"). Scripts are checked in
  with explicit licensing review; scripts under incompatible licenses
  are excluded.

The bash test suite is **GPL** and is not redistributed in any form. It
may be consulted as reference material for individual edge cases, but
no GPL test fixtures are committed to the fredshell repository.

### 3. The harness exists before the implementation

The spec-test harness is the first piece of production code written
after the planning phase ends. It must run in CI from day one, even at
0% pass rate. A planning artifact that cannot be measured against the
harness is not yet ready to become implementation.

This implies hard constraints on the architecture (owned by
`PLAN_02_architecture.md`):

- The parser must be invocable independently of the executor.
- The executor must accept a sandboxable execution environment
  (configurable `$HOME`, `$PATH`, working directory, env vars).
- A non-interactive batch-mode entry point must exist from the
  beginning — not as an afterthought added once the REPL works.

### 4. Two-phase planning

Planning documents are split into two phases:

- **Phase A** docs are corpus-independent and drafted before the corpus
  exists. These include the testing methodology itself (PLAN_05), crate
  architecture (PLAN_02), foundational subsystems (PLAN_03, PLAN_04,
  PLAN_13, PLAN_14), and peripheral design (PLAN_12, PLAN_13, PLAN_14).
- **Phase B** docs are corpus-dependent. They receive stubs during
  Phase A and are fully drafted only after the v1 corpus is curated
  and the harness reports a baseline pass-rate. These are: the bash
  compat executor and Tier-1 builtin inventory (PLAN_06 Phase B),
  spec-sheet drafting (PLAN_07), the fuzzer / differential program
  (PLAN_08), traps and job control (PLAN_12), and the implementation
  roadmap (PLAN_15). Drafting these before the corpus
  exists would mean guessing at priorities; drafting them after means
  data-driven prioritization.

The corpus is **silent** on questions of design taste — config file
format, async runtime choice, internal API shapes, naming. Those
decisions belong to Phase A docs and must not wait on the corpus.

### 5. Conformance is reported as a number, continuously

The harness produces a pass-rate per feature category and an overall
pass-rate. Both numbers are tracked over time and surfaced in CI
output. Regressions in pass-rate are treated as build failures the same
way clippy warnings are.

This is the operational answer to "what does fredshell support?":
`cargo xtask compat-report`.

## Consequences

### Positive

- Compatibility becomes a measurable engineering target instead of an
  aspiration. Every PR touching the parser or executor has a clear
  signal for whether it improved or regressed conformance.
- The architecture is forced into a shape that supports testing
  (separable parser, sandboxable executor, batch-mode entry) from the
  beginning, rather than retrofitted.
- The avoid-archaeology problem from freminal is structurally
  prevented: `grep` over the corpus answers "is X supported?" in
  seconds.
- License boundaries around the bash GPL test suite and the
  Apache-licensed oils-spec corpus are decided once, here, rather than
  re-litigated when the implementer reaches each suite.
- Phase B docs are written against real data, not guesses.

### Negative

- The harness has to exist before any real shell behavior does. The
  first weeks of implementation produce no visible shell — only a
  test runner reporting "0 of N passing."
- Curating a primary corpus is non-trivial labor and is on the
  critical path before Phase B docs can be drafted.
- Fetching the oils-spec corpus at CI time introduces a network
  dependency and a versioning question (pin a commit; refresh
  deliberately).

### Risks accepted

- **Corpus blind spots.** A behavior not in the corpus is, by
  definition, not measured. Mitigation: the corpus is grown
  continuously, and every bug fix lands with a regression test added
  to the corpus.
- **Bash version drift.** Bash itself changes behavior between
  versions. Mitigation: the reference bash version is pinned per
  platform in CI; corpus expected-output fixtures are regenerated
  deliberately, not auto-refreshed.
- **Real-world corpus licensing.** Real scripts have varied licenses.
  Mitigation: tier-3 corpus entries are reviewed individually; the
  default is to exclude rather than include when licensing is unclear.

## Alternatives considered

### Prose-only specification

Write `PLAN_06_exec.md` as a detailed prose description of which
bash features are supported and how. **Rejected.** Prose is not
executable. It cannot answer "does this PR break feature X" or "what
percentage of feature Y do we support today." Prose specifications of
shell behavior also age poorly as the implementation evolves.

### Vendor the bash test suite

Copy bash's own test suite into the repo and run it. **Rejected.** The
bash test suite is GPL; vendoring it would impose GPL on fredshell, in
conflict with the MIT license decision. Even as a fetch-at-CI signal,
re-distributing GPL-licensed fixtures via CI logs is legally murky and
not worth the risk.

### Defer the harness until after a working shell exists

Build the shell first, then add tests. **Rejected.** This is the path
that produced the freminal archaeology problem. Without the harness in
place from the start, there is no incremental signal on whether
compatibility is improving, and the architecture drifts into shapes
that are hard to test (executor entangled with REPL, parser entangled
with executor, no sandboxable environment).

## References

- `PLAN_01_philosophy.md` — goal G1 (real-world bash scripts run
  unmodified) and non-goal NG1 (POSIX behavior is a substrate; POSIX
  certification is not pursued).
- `PLAN_05_testing.md` — the concrete harness design, corpus layout,
  and CI integration that operationalize this ADR.
- `PLAN_02_architecture.md` — the architectural constraints this ADR
  imposes (separable parser, sandboxable executor, batch-mode entry).
- `PLAN_06_exec.md` — the Phase B compat document whose detail
  (executor semantics and Tier-1 builtin inventory) is informed by
  harness output and corpus frequency analysis.
- `PLAN_08_spec_drafting.md` — the per-builtin and per-feature spec
  sheets whose drafting order is informed by corpus frequency.
- `PLAN_09_fuzzer.md` — the grammar-aware fuzzer and differential
  oracle that expand the corpus beyond hand-written cases.
- `PLAN_10_traps_and_jobs.md` — traps, signal disposition, and job
  control whose detail is informed by harness output.
- `PLAN_15_milestones.md` — the Phase B roadmap whose phasing is
  informed by pass-rate progression.
- ADR 0001 — in-process execution; this ADR is the methodology by
  which tier-2 builtins are measured for parity against bash.
