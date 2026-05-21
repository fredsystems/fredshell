# `tests/spec/` — the fredshell spec corpus (tier 1)

This directory holds the in-tree, MIT-licensed bash-compat spec
corpus. See `Documents/PLAN_05_testing.md` for the architectural
contract; this README is the operator-facing reference for two
things:

1. how to invoke the harness, and
2. the on-wire JSON schema that `cargo xtask compat --json` emits.

Tier 2 (oils-spec, fetched at CI time) and tier 3 (real-world
scripts) live elsewhere and are added by later subtasks (`PLAN_05`
05.9 onward, and `PLAN_13`).

## Layout

```text
tests/spec/
  REFERENCE.md                   pinned reference toolchain (bash + coreutils)
  README.md                      this file
  <category>/
    <name>.case.toml             required: schema in PLAN_05 §3.2 / §4.1
    <name>.stdout                optional: byte-exact expected stdout (default: empty)
    <name>.stderr                optional: byte-exact expected stderr (default: empty)
    <name>.exit                  optional: expected exit code, one int line (default: 0)
    <name>.fs/                   optional: sandbox FS skeleton, copied into $PWD
```

Each case's category is the first path segment under `tests/spec/`.

## Running the harness

```sh
# Run the full corpus.
cargo xtask compat

# Restrict to a category.
cargo xtask compat builtins_tier1

# Restrict to a tier (v0 only ships tier 1).
cargo xtask compat --tier 1

# Restrict to cases with a given declared status (PLAN_05 §12).
cargo xtask compat --status pass
cargo xtask compat --status deferred:PLAN_06b

# Emit a machine-readable JSON report.
cargo xtask compat --json target/compat-report.json
```

Exit code: `0` when no regressions were recorded, `1` when at least
one case declared `status = "pass"` failed to match (mirrors
`VerdictTally::has_ci_failures`). See `PLAN_05` §7.2.

## JSON report — schema v1

`cargo xtask compat --json <path>` writes a UTF-8 JSON document with
the shape below. The `schema_version` field is the stability
contract: any breaking change must bump the integer and add a
migration note in this file.

```json
{
  "schema_version": 1,
  "corpus_root": "tests/spec",
  "tally": {
    "expected_pass": 1,
    "regression": 0,
    "expected_fail": 0,
    "wontfix_honored": 0,
    "deferred_honored": { "PLAN_06b": 2 },
    "reclassify": 0,
    "total": 3,
    "pass_rate_numerator": 1,
    "pass_rate_denominator": 3,
    "regressions_present": false
  },
  "cases": [
    {
      "path": "builtins_tier1/exit_zero.case.toml",
      "category": "builtins_tier1",
      "tier": 1,
      "status": "pass",
      "outcome": { "kind": "pass" },
      "verdict": { "kind": "expected_pass" }
    }
  ]
}
```

### Top-level fields

| Field            | Type                  | Notes                                                                                            |
| ---------------- | --------------------- | ------------------------------------------------------------------------------------------------ |
| `schema_version` | integer               | `1` for this document. Bump on breaking shape changes.                                           |
| `corpus_root`    | string                | The corpus root the harness walked. Usually `"tests/spec"`.                                      |
| `tally`          | object                | Per-status aggregate counts (see below).                                                         |
| `cases`          | array of case records | One entry per case the harness actually ran (after filters applied), sorted lexically by `path`. |

### `tally` fields

Names mirror `VerdictTally` (see `crates/fredshell-spec-runner/src/verdict.rs`):

| Field                   | Type                      | Notes                                                                              |
| ----------------------- | ------------------------- | ---------------------------------------------------------------------------------- |
| `expected_pass`         | integer                   | `pass` cases that matched. Numerator of the headline pass-rate.                    |
| `regression`            | integer                   | `pass` cases that did NOT match. Non-zero ⇒ CI fails.                              |
| `expected_fail`         | integer                   | `fail` cases that continued to differ.                                             |
| `wontfix_honored`       | integer                   | `wontfix` cases that continued to differ. Excluded from the pass-rate denominator. |
| `deferred_honored`      | object (string → integer) | Per-plan count of `deferred:PLAN_XX` cases that continued to differ.               |
| `reclassify`            | integer                   | `RECLASSIFY` advisories emitted (§12.1). Does not affect exit code.                |
| `total`                 | integer                   | Sum of all verdict buckets above.                                                  |
| `pass_rate_numerator`   | integer                   | Equal to `expected_pass`.                                                          |
| `pass_rate_denominator` | integer                   | `total - wontfix_honored`, per `PLAN_05` §12.                                      |
| `regressions_present`   | boolean                   | `true` iff `regression > 0`. Mirrors `VerdictTally::has_ci_failures`.              |

### `cases[]` fields

| Field      | Type          | Notes                                                                          |
| ---------- | ------------- | ------------------------------------------------------------------------------ |
| `path`     | string        | Forward-slash path relative to `corpus_root`.                                  |
| `category` | string        | First path segment of `path`.                                                  |
| `tier`     | integer       | Currently always `1`. Tier 2 / 3 land later.                                   |
| `status`   | string        | Verbatim §12 status: `"pass"`, `"fail"`, `"wontfix"`, or `"deferred:PLAN_XX"`. |
| `outcome`  | tagged object | See below.                                                                     |
| `verdict`  | tagged object | See below.                                                                     |

### `outcome` variants

The `kind` discriminator selects the payload:

```json
{ "kind": "pass" }

{
  "kind": "mismatch",
  "observed_stdout_b64": "aGkK",
  "observed_stderr_b64": "",
  "observed_exit": 7
}

{
  "kind": "executor_refused",
  "command": "/bin/echo hi",
  "reason": "PolicyStrict"
}
```

`observed_stdout_b64` / `observed_stderr_b64` are standard base64
(RFC 4648) of the raw bytes the harness captured. Encoding is used
unconditionally because the streams are arbitrary bytes (POSIX does
not require UTF-8). Decode with any standard base64 library; the
result is the byte sequence the executor produced.

`reason` is currently rendered as the `Debug` form of
`fredshell_core::NoExternalExecutorReason`. It is informative; do
not parse it.

### `verdict` variants

The `kind` discriminator selects the payload:

```json
{ "kind": "expected_pass" }
{ "kind": "regression" }
{ "kind": "expected_fail" }
{ "kind": "wontfix_honored" }
{ "kind": "deferred_honored", "plan": "PLAN_06b" }
{
  "kind": "reclassify",
  "from": "fail",
  "suggested": "pass",
  "reason": "outcome_matched_despite_non_pass_status"
}
```

`reclassify.from` and `reclassify.suggested` are rendered from
`CaseStatus::Display` and use the exact strings accepted by the
`.case.toml` `status` field — so a tool can suggest a textual diff
without re-parsing.

### Stability

The schema is versioned. Any change that:

- removes a field,
- renames a field,
- changes the type or shape of a field,
- changes the meaning of a value,

must bump `schema_version`. Adding a new optional field to a record
or a new tagged variant is non-breaking and does NOT require a bump,
provided consumers tolerate unknown fields and variants. Future
subtasks (`PLAN_05` 05.10, the CI delta job) consume this schema.

## Owning subtask

This file landed with `PLAN_05` 05.6. The recording harness
(`xtask spec record`) lands with 05.7; the schema-linter
(`xtask spec lint`) with 05.8.
