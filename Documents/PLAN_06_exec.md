# PLAN_06 — Execution pipeline

> Last updated: 2026-05-23 — Q06B.1 (parser strategy)
> resolved: in-house recursive-descent, ratified by ADR 0005
> at subtask 06b.1; §13.2 prose updated, §13.8 marked resolved.
> Q06B.3 (here-doc threshold) resolved: 64 KiB pipe/tempfile
> boundary named `HEREDOC_PIPE_MAX`, boundary regression cases
> required in PLAN_08 spec sheets.
> Q06B.4 (`$RANDOM` / `$SECONDS`) resolved: per-case pin via
> PLAN_08 sheet `[harness]` block + harness-only env channel
>
> - `faketime` shim for the reference bash.
>   Q06B.5 (`$"..."`) resolved: parser accepts, executor refuses
>   with `ExecError::Unsupported { feature: "locale_translation" }`;
>   refusal-corpus case under `tests/spec/refusals/`.
>   Q06B.2 (`coproc`) resolved
>   by `PLAN_16_coproc.md` stub; v1 emits a parser refusal.
>   Earlier on 2026-05-22 — Phase B section (§13) expanded from
>   stub to full draft: lexer/parser scope (§13.2), executor
>   pipeline (§13.3), `ShellState` field table (§13.4), builtin
>   inventory by owner (§13.5), ADR 0004 fallback removal in two
>   stages with exit gate (§13.6), 33-row subtask grid (§13.7),
>   open questions (§13.8). Phase A unchanged.
>   Earlier on 2026-05-21 — restructured as a single two-phase
>   document (Phase A skeleton implemented; Phase B semantics not yet
>   drafted) following the PLAN renumber.
>   Phase: A complete, B drafted. Status: Phase A implemented; Phase
>   B awaiting 06b.0 gate (PLAN_09 F1 green on `main`).

This document owns the parse-and-execute pipeline that PLAN_05 (the
spec harness) and the binary REPL call into. It is organised into two
phases:

- **Phase A — skeleton (implemented).** Locks the function signatures
  and type envelopes, ships a stub implementation that delegates to
  the legacy `dispatch_line` path (which shells out to `/bin/sh`), and
  makes the harness and the binary share one code path. Owned by §1–§11
  of this document. Subtasks `06a.1`–`06a.8` are complete.
- **Phase B — real semantics (drafted, gated).** Replaces the stub
  with a real parser, native executor, full Tier-1 + Tier-2 builtin
  surface, `ShellState`, pipelines, redirections, expansion,
  arithmetic, control flow, and job-control glue. Drafted in §13.
  Implementation is gated on subtask 06b.0 (PLAN_09 F1 differential
  green on `main`) per ADR 0003 + ADR 0004.

The split exists because ADR 0003 requires the spec harness (PLAN_05)
to land before any real compatibility work, and Phase B is
corpus-dependent. Phase A's contribution is a stable public entry
point that lets the harness and the binary share one code path while
Phase B is iterated behind it. Replacing the stub executor is then a
localised change behind a fixed public surface.

## 1. Scope (Phase A)

In scope for Phase A:

- The `parse` function signature and the opaque `Script` AST handle.
- The `ExecEnv` struct, minimal v0 field set, with `#[non_exhaustive]`.
- The `run_source` and `run_script` entry points.
- The `RunResult` and `ExecError` shapes, both `#[non_exhaustive]`.
- The `Tier2Builtin` trait and `Tier2Ctx` borrow struct (shape only).
- A stub implementation that satisfies the contract by delegating to
  the existing `repl::dispatch_line` path.
- A single Criterion bench covering parse + execute round-trip on a
  trivial command, to seed §9 budget tracking.

Explicitly out of scope:

- Real parsing (tokenizer, grammar, AST internals, expansion).
- Real execution (pipelines, redirections, job control, expansion,
  arithmetic, control flow).
- Tier-1 builtin implementations beyond what `dispatch_line` already
  handles.
- Tier-2 builtin implementations and the dispatch table.
- `ShellState` internals (variables, functions, aliases, opts, jobs).
- `setitimer` / `SIGALRM` timeout plumbing.
- The pipeline `poll` loop.

Out-of-scope items are owned by Phase B (§13) unless noted otherwise.

## 2. Public surface

All types and functions live in `fredshell-core` and are re-exported
from the crate root. The dispatcher in §3 wires them together; the
binary REPL and the spec harness use only the items below.

### 2.1. `parse`

```rust
/// Parse a shell-language source string into an opaque `Script`.
///
/// Pure function: no I/O, no global state, no environment access.
/// The returned `Script` is consumed by `run_script`.
///
/// v0 implementation: wraps the source as-is. Phase B (§13)
/// replaces the body without changing this signature.
pub fn parse(source: &str) -> Result<Script, ParseError>;

pub struct Script { /* sealed; opaque to consumers */ }

#[non_exhaustive]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub message: String,
}

#[non_exhaustive]
pub enum ParseErrorKind {
    /// v0 placeholder. Phase B (§13) adds real categorical variants
    /// (`UnexpectedToken`, `UnterminatedString`, etc.). Anything that
    /// would fail to parse in v0 (which is nothing — the v0 parser
    /// accepts everything) maps here.
    Unsupported,
}
```

`Script` deliberately does not expose `Span`, AST nodes, or any
walker. The harness does not need to walk the AST; the binary does
not need to walk the AST; Phase B (§13) owns the AST internals and is
free to evolve them without breaking either consumer.

### 2.2. `ExecEnv`

```rust
/// The environment a script executes in. Constructed by the host
/// (binary or harness), passed to the executor, owned by the caller.
///
/// `#[non_exhaustive]` because Phase B (§13) will add fields
/// (`shell: ShellState`, `builtins: BuiltinRegistry`,
/// `path_policy`, `signal_policy`, real `stdin`/`stdout`/`stderr`).
#[non_exhaustive]
pub struct ExecEnv {
    /// Working directory. The executor mutates this on `cd`.
    pub cwd: PathBuf,

    /// Environment variables visible to the script.
    ///
    /// v0 uses `String` keyed by `String` for test ergonomics and
    /// because no real env handling exists yet. Phase B (§13)
    /// migrates to `HashMap<OsString, OsString>` per PLAN_02 §4.2.
    /// The migration cost is acknowledged: callers that construct
    /// `ExecEnv` in tests will need to update key/value types. There
    /// is no public `ExecEnv::env` accessor today; callers use the
    /// constructors in §2.5.
    pub(crate) env: HashMap<String, String>,
}
```

`stdin`, `stdout`, `stderr`, `shell`, `builtins`, `path_policy`, and
`signal_policy` are intentionally absent in v0. The stub dispatcher
inherits the host's stdio via `Command`. The Phase B (§13) real
executor adds them as boxed handles per PLAN_02 §4.2.

### 2.3. `run_source` and `run_script`

```rust
/// Parse and execute a source string in one call.
///
/// Convenience wrapper: `run_source(s, env) == parse(s).and_then(|s|
/// run_script(&s, env))` with the error types unified. This is the
/// entry point the spec harness uses.
pub fn run_source(source: &str, env: &mut ExecEnv) -> Result<RunResult, RunError>;

/// Execute a pre-parsed `Script`. The binary REPL uses this when it
/// has already parsed the user's input (e.g. to validate before
/// recording in history).
pub fn run_script(script: &Script, env: &mut ExecEnv) -> Result<RunResult, RunError>;
```

`run_source` is the harness's single entry point. The harness does
not call `parse` and `run_script` separately because parse errors and
runtime errors both arrive through `RunError`, which the harness
classifies once.

`run_script` exists for the binary, which may want to parse
defensively (e.g. to reject obviously malformed input before
displaying a continuation prompt) and execute later.

### 2.4. `RunResult` and `RunError`

```rust
#[non_exhaustive]
pub struct RunResult {
    /// Final exit status of the script. `0` for success, non-zero
    /// for failure, per POSIX.
    pub status: ExitStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitStatus(pub i32);

#[non_exhaustive]
pub enum RunError {
    /// Parse-time error. Wraps `ParseError`.
    Parse(ParseError),
    /// Runtime error. Wraps `ExecError`.
    Exec(ExecError),
}

#[non_exhaustive]
pub enum ExecError {
    /// A command (builtin or external) was not found.
    CommandNotFound { name: String },
    /// The host's I/O streams or process machinery failed.
    HostIo(io::Error),
    /// The executor encountered a state it considers a bug. Never
    /// produced in normal operation; surfaced for tests.
    InternalInvariant { what: &'static str },
}
```

Both error enums are `#[non_exhaustive]` so Phase B (§13) can add
variants (`Redirection`, `ExecFailure`, `Span`-bearing variants, etc.)
without breaking match exhaustiveness in callers.

The split between `RunResult.status` (non-zero exit) and `RunError`
(executor itself failed) is binding. The harness treats them as
different categories: a script that exits 1 is not the same event as
the executor refusing to run.

### 2.5. Constructors

```rust
impl ExecEnv {
    /// Construct an `ExecEnv` from the calling process: cwd from
    /// `std::env::current_dir`, env from `std::env::vars_os`. Used
    /// by the binary REPL.
    ///
    /// Returns `HostIo` if `current_dir` fails (e.g. the cwd was
    /// deleted out from under the process).
    pub fn from_process() -> Result<Self, ExecError>;

    /// Construct an empty `ExecEnv` rooted at the given directory
    /// with no inherited env vars. Used by the spec harness for
    /// hermetic tests.
    pub fn sandboxed(cwd: PathBuf) -> Self;
}
```

The harness never calls `from_process`. The binary always calls
`from_process` (or a thin wrapper). This is the seam that lets the
harness run hermetically per PLAN_05.

### 2.6. `Tier2Builtin` and `Tier2Ctx`

The trait and context struct ship in Phase A to lock the shape Phase B
(§13) will consume. No tier-2 builtins are registered in v0; the
registry type exists but is always empty.

```rust
pub trait Tier2Builtin: Send + Sync {
    fn name(&self) -> &'static str;
    fn aliases(&self) -> &'static [&'static str] { &[] }
    fn invoke(&self, ctx: Tier2Ctx<'_>) -> Result<ExitStatus, Tier2Error>;
}

#[non_exhaustive]
pub struct Tier2Ctx<'a> {
    pub args: &'a [String],
    pub cwd: &'a Path,
    pub env: &'a HashMap<String, String>,
    pub stdin: &'a mut dyn io::Read,
    pub stdout: &'a mut dyn io::Write,
    pub stderr: &'a mut dyn io::Write,
    pub cancellation: &'a AtomicBool,
}

#[non_exhaustive]
pub enum Tier2Error {
    HostIo(io::Error),
    InternalInvariant { what: &'static str },
}
```

The `String` / `&Path` types are v0; Phase B (§13) migrates `args` and
`env` to `OsString` together with `ExecEnv::env`.

## 3. Internal dispatcher (stub implementation)

```text
parse(source) ──► Script (wraps source: String)
                         │
                         ▼
run_script(&script, env) ─► dispatch_line(&script.source, env)
                                          │
                                          ▼
                              (today's behavior: tier-1 builtin
                               lookup → shell out to /bin/sh for
                               anything else)
```

The stub:

1. `parse` returns `Ok(Script { source: source.to_owned() })`. The
   only way it fails is if the source contains a NUL byte, which is
   `ParseErrorKind::Unsupported`.
2. `run_script` walks `script.source` line-by-line (split on `\n`,
   skip blank lines) and calls `dispatch_line` on each.
3. `dispatch_line` returns `CoreResult<()>`; the stub maps `Ok(())`
   to `ExitStatus(0)` and `Err` to the appropriate `ExecError`
   variant. The full exit-status plumbing lands with Phase B (§13).

The stub deliberately does not implement multi-line constructs
(`if`, `for`, `while`, function definitions, here-documents). Any
input that would require real parsing executes line-by-line, which
is wrong for those constructs and right for everything the v0 spec
corpus exercises. PLAN_05 §3 lists which corpus tests are expected
to fail against the stub; Phase B (§13) makes them pass.

## 4. Crate placement

Per PLAN_02 §11 and the §12 resolution recorded in PLAN_02, the
parser and executor are internal modules of `fredshell-core`, not
separate crates. v0 module layout:

```text
fredshell-core/src/
  lib.rs               — re-exports: parse, run_source, run_script,
                         Script, ExecEnv, RunResult, RunError,
                         ExecError, ExitStatus, Tier2Builtin,
                         Tier2Ctx, Tier2Error
  exec/
    mod.rs             — run_source, run_script, the stub dispatcher
    env.rs             — ExecEnv, its constructors
    error.rs           — RunError, ExecError, ExitStatus, RunResult
    builtin.rs         — Tier2Builtin, Tier2Ctx, Tier2Error
                         (definitions only; no impls in v0)
  parser/
    mod.rs             — parse, Script, ParseError, ParseErrorKind
                         (stub; Phase B replaces the body)
  builtins/            — existing module, unchanged
  repl.rs              — existing dispatch_line, called by exec/mod.rs
  tty/                 — existing
```

The existing `builtins/` and `repl::dispatch_line` are unchanged in
Phase A. Phase B (§13) folds them into the new `exec/` module.

## 5. Wiring the binary REPL

The binary's read-eval loop changes minimally:

```rust
// Before (current):
core::repl::dispatch_line(&line)?;

// After (Phase A):
let mut env = ExecEnv::from_process()?;
match fredshell_core::run_source(&line, &mut env) {
    Ok(result) => self.last_status = result.status,
    Err(err) => self.report_error(err),
}
```

`ExecEnv` construction is hoisted out of the per-line loop and reused
across iterations once `ShellState` lands in Phase B (§13). For v0 it
is fine to construct per-line; the cost is one `current_dir` syscall
and one `vars_os` walk, which is well inside the §9 budget.

## 6. Wiring the spec harness

PLAN_05 §5 will document the harness side. The contract from Phase A:

- The harness constructs `ExecEnv::sandboxed(tempdir)`.
- For each spec case, the harness calls `run_source(case.input, &mut env)`.
- The harness captures `stdout`, `stderr`, and `exit_status` and
  compares them against the case's expected values.
- v0: `stdout` / `stderr` are captured by setting `Command::stdout`
  / `Command::stderr` to pipes inside `dispatch_line`. The stub
  dispatcher exposes an internal hook the harness uses. Phase B (§13)
  replaces this with real `Box<dyn Write>` plumbing on `ExecEnv`.

The "internal hook" is the one carve-out from the public surface
that Phase A permits, on the grounds that the stub is provisional and
the harness needs _some_ way to capture output before Phase B (§13)
ships real I/O plumbing. It lives in `exec::testing` behind a
crate-internal visibility and is removed when Phase B lands real
stdio on `ExecEnv`.

## 7. Compatibility with PLAN_02

This document is a Phase A refinement of PLAN_02 §4. Where the two
disagree on v0 details, this document wins for v0 and PLAN_02
describes the final target:

| Concern              | PLAN_02 (target)                 | Phase A (v0)                      |
| -------------------- | -------------------------------- | --------------------------------- |
| Env map type         | `HashMap<OsString, OsString>`    | `HashMap<String, String>`         |
| `Tier2Ctx::args`     | `&[OsString]`                    | `&[String]`                       |
| `ExecEnv` stdio      | `Box<dyn Read/Write + Send>`     | absent (inherited via `Command`)  |
| `ExecEnv::shell`     | `ShellState`                     | absent                            |
| `ExecEnv::builtins`  | `BuiltinRegistry`                | absent (stub uses fixed dispatch) |
| `parse` returns      | walkable `Ast`                   | opaque `Script`                   |
| `ExecError` variants | full categorical set with `Span` | minimal stub set                  |

Every v0 cell migrates to the PLAN_02 cell during Phase B (§13), and
PLAN_02 §12 records when each migration completes.

## 8. Testing

Phase A ships with:

- Unit tests for `parse` (NUL byte rejection, round-trip).
- Unit tests for `ExecEnv::sandboxed` and `ExecEnv::from_process`.
- Unit tests for the stub dispatcher: builtin path (`cd`, `exit`),
  external path (`echo hi`), command-not-found path.
- One integration test that exercises `run_source` end-to-end against
  a temp directory: `cd subdir && pwd` produces the expected stdout
  and exit 0.

PLAN_05 owns the spec corpus and its harness; Phase A only ensures
the entry point is callable and behaves consistently.

## 9. Performance

A single Criterion bench seeds the §9 budget tracker in PLAN_02:

```text
benches/exec_roundtrip.rs
  parse_only        — parse("true")
  parse_and_exec    — run_source("true", &mut env)
```

Budgets are not enforced in Phase A; the bench exists so Phase B (§13)
has a baseline. The `parse_only` number should be ~zero (the stub
clones a short string). The `parse_and_exec` number is bounded by
`fork + execve + /bin/sh "true"`, which is ~milliseconds on Linux —
far above what Phase B will achieve, and a useful "before" data point.

## 10. Phase A subtasks

Each subtask is one commit (per `AGENTS.md`). The list is
prescriptive; deviations require a note in §11.

- **06a.1** Add `exec/error.rs` with `RunResult`, `RunError`,
  `ExecError`, `ExitStatus`. Tests for the `Display`/`Debug` impls
  and the `From<ParseError>` / `From<ExecError>` conversions into
  `RunError`.
- **06a.2** Add `exec/env.rs` with `ExecEnv`, `from_process`,
  `sandboxed`. Tests for both constructors, including the
  `current_dir`-deleted failure path.
- **06a.3** Add `parser/mod.rs` with stub `parse`, `Script`,
  `ParseError`, `ParseErrorKind`. Tests for NUL rejection and the
  round-trip property.
- **06a.4** Add `exec/builtin.rs` with `Tier2Builtin`, `Tier2Ctx`,
  `Tier2Error`. No impls. Compile-only test confirming the trait is
  object-safe.
- **06a.5** Add `exec/mod.rs` with `run_source`, `run_script`, the
  stub dispatcher, and the crate-internal output-capture hook.
  Unit tests per §8.
- **06a.6** Wire the binary REPL to call `run_source`. Remove the
  direct `repl::dispatch_line` call from the binary. Integration
  test that the binary still runs `cd` + `pwd` correctly.
- **06a.7** Add `benches/exec_roundtrip.rs` per §9.
- **06a.8** Update `plan.md` to mark PLAN_06 `implemented` and
  PLAN_02 §12 to reflect which §4 sections are now backed by code
  (the surface, not the semantics).

Verification suite (`cargo test --all`, `cargo clippy
--all-targets --all-features -- -D warnings`, `cargo-machete`) runs
after every subtask.

## 11. Implementation log

To be filled as subtasks complete, one row per subtask, format
matching PLAN_04 §14.

| Subtask | Commit | Date       | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ------- | ------ | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 06a.1   | TBD    | 2026-05-20 | Added `exec/error.rs` with `RunResult`, `RunError`, `ExecError`, `ExitStatus`, `ParseErrorPlaceholder`. Converted `exec.rs` to `exec/mod.rs` to enable the directory module. Crate-root re-exports added. `From<ParseError>` deferred to 06a.3 because the real `ParseError` does not yet exist; tracked by `ParseErrorPlaceholder` and its `From` impl, both removed in 06a.3. 14 unit tests added; 137 core tests passing.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 06a.2   | TBD    | 2026-05-20 | Added `exec/env.rs` with `ExecEnv`, `from_process`, `sandboxed`. v0 keeps `env: HashMap<String, String>` per §7; non-UTF-8 vars are silently dropped by `from_process`. Tests serialize on a module-local `GLOBAL_ENV_LOCK: Mutex<()>` because `env::set_var` and `env::set_current_dir` mutate process-global state. 8 unit tests added; 145 core tests passing.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| 06a.3   | TBD    | 2026-05-20 | Added `parser/mod.rs` with stub `parse`, `Script`, `ParseError`, `ParseErrorKind` (single `Unsupported` variant, `#[non_exhaustive]`). v0 stores source verbatim in `Script::source` (`pub(crate)`) and rejects NUL bytes; Phase B replaces the body. Deleted `ParseErrorPlaceholder` from `exec/error.rs` and switched `RunError::Parse` to the real `ParseError`; added `From<ParseError> for RunError`. `Script::source` and the `source()` accessor are gated by `#[allow(dead_code)]` with TODO(`06a.5`) markers per the AGENTS.md "temporary refactor" exception — consumed by the stub dispatcher in 06a.5. 12 parser tests added (137 → 156 core tests passing after deleting the placeholder test).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 06a.4   | TBD    | 2026-05-20 | Added `exec/builtin.rs` defining `Tier2Builtin` (object-safe, `Send + Sync`, default-empty `aliases()`), `Tier2Ctx<'a>` (borrows for `args`, `cwd`, `env`, `stdin`/`stdout`/`stderr` as trait objects, cooperative `cancellation: &AtomicBool`), and `Tier2Error` (`HostIo`, `InternalInvariant`, `#[non_exhaustive]`, `From<io::Error>`). Definitions only — no registry, no impls, no dispatcher wire-up (those land with Phase B). Includes a compile-time object-safety check (`Box<dyn Tier2Builtin>`) and a `Send + Sync` assertion that will fail to compile if a future edit breaks either property. 12 unit tests added; 168 core tests passing.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| 06a.5   | TBD    | 2026-05-20 | Rewrote `exec/mod.rs` with the stub dispatcher: `run_source`, `run_script`, `dispatch_script`, `dispatch_line`, `spawn_via_sh`, `core_error_to_exec`, plus the internal `LineOutcome` and `Capture<'a>` (`Inherit` / `Buffers { stdout, stderr }`) enums. Dispatcher walks `script.source().split('\n')`, trims, skips blank lines, dispatches via `builtins::try_run` (Tier-1), and falls back to `/bin/sh -c <line>` honouring the configured `Capture`. `exit` short-circuits with its requested code; the last executed line's status becomes `RunResult::status`. Unparsable tokenisation (e.g. unterminated quote) is handed straight to `/bin/sh` per §3 v0 simplification. `run_via_sh` retained for one-shot mode (`fredshell -c …`) since it `std::process::exit`s; the new path returns the exit code as a value so the harness can observe it. Added `exec/testing.rs` (`pub(crate) mod`) with `Captured { result, stdout, stderr }` and `run_source_capturing`, both carrying `#[allow(dead_code)]` + `TODO(PLAN_05)` per the AGENTS.md "temporary refactor" exception — consumed by the spec harness once it lands; `Capture::Buffers` carries the same allow. Promoted `GLOBAL_ENV_LOCK` to a `pub(crate) static Mutex<()>` at module level of `exec/env.rs` so every test that spawns `/bin/sh` (or mutates cwd/env) can serialise on it; all spawning tests now acquire the lock to prevent a flake where the `cd_*` tests' transient tmp-cwd would leak into a sibling spawn and cause `/bin/sh` to write a `getcwd` warning to stderr. Dropped the 06a.3 `#[allow(dead_code)]` + `TODO(06a.5)` markers on `Script::source` / `Script::source()` since the dispatcher now consumes them. 22 unit tests added (17 in `exec::tests`, 4 in `exec::testing::tests`, plus carry-over coverage); 190 core tests passing; clippy + machete clean; flake reproduced and verified gone across 15 consecutive full-suite runs. |
| 06a.6   | TBD    | 2026-05-21 | Wired the binary REPL to `run_source`. Extended `RunResult` with `exit_requested: bool` and added `RunResult::new(status)` (non-exiting) and `RunResult::exit(status)` (sets the flag); `dispatch_script` propagates the flag when a line returns `LineOutcome::Exit`. Rewrote `repl::dispatch_line` to construct a fresh `ExecEnv::from_process()` per line, call `run_source`, and `std::process::exit(result.status.0)` only when `exit_requested` is set; on `RunError` it writes the error to stderr and continues, so a single bad line cannot kill an interactive session. `dispatch_line` is now infallible (`fn(&str)`) — `ExecError::HostIo` has no semantically honest mapping into `CoreError`, and `CoreError::ReplIo` is reserved for stdin/stdout failures — so both call sites in `repl.rs` (raw-mode and cooked-mode loops) lost their `?` operators. Removed the now-unused `use crate::builtins::{self, BuiltinOutcome};` from `repl.rs`. Re-exported `run_source` and `run_script` from `lib.rs` so the integration test target can reach them. Added `crates/fredshell-core/tests/run_source_smoke.rs` (new test target) with three integration tests: `cd_subdir_then_pwd_via_run_source_oneshot` drives the built `fredshell -c` binary against a `tempdir` (canonicalising both paths to handle macOS's `/tmp` → `/private/tmp` symlink) to honour §8's literal wording; `run_source_returns_non_zero_for_false` and `run_source_exit_builtin_sets_exit_requested` exercise the public API with `ExecEnv::sandboxed`. Net: +1 unit test in `error.rs`, +2 in `exec::tests` (`exit_propagates_through_dispatch_script`, `non_exit_does_not_set_exit_requested`); 191 core unit tests + 3 integration tests passing; clippy `--all-targets --all-features -D warnings` clean.                                                                                                                                    |
| 06a.7   | TBD    | 2026-05-21 | Added `crates/fredshell-core/benches/exec_roundtrip.rs` (Criterion, `harness = false`) per §9, plus the `[dev-dependencies] criterion = { workspace = true }` and `[[bench]] name = "exec_roundtrip"` entries in `crates/fredshell-core/Cargo.toml`. Two benchmarks: `exec_roundtrip_parse_only` calls `parse("true")`; `exec_roundtrip_parse_and_exec` calls `run_source("true", &mut ExecEnv::sandboxed(temp_dir()))`. Initial measurements on the dev host: `parse_only` 12.330–12.512 ns (median 12.419 ns) and `parse_and_exec` 3.121–3.230 ms (median 3.176 ms). The parse number is effectively noise — v0 only stores the source string verbatim — which matches §9's "should be ~zero" prediction. The exec number sits inside the predicted `fork + execve + /bin/sh "true"` envelope and is the "before" data point Phase B must beat by running `true` as a Tier-1 builtin instead of spawning `/bin/sh`. No code outside the bench file and `Cargo.toml` was touched; 191 core unit tests + 3 integration tests still passing; clippy and machete clean.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| 06a.8   | TBD    | 2026-05-21 | Flipped Phase A from `draft` to `implemented`. Updated the top-level `plan.md` status row for 06a, refreshed its Last-updated header, and rewrote PLAN_02 §12 to reflect that §4.1 (parser surface), §4.2 (`ExecEnv` surface), §4.3 (executor surface: `run_source`, `run_script`, `RunResult`, `RunError`, `ExecError`, `ExitStatus`), §4.5 (`Tier2Builtin` trait shape), and §9 (bench scaffolding) are now backed by code — surface only, semantics still pending Phase B. Cleaned up two stale fragment lines from the PLAN_02 header at the same time. No code changes; verification suite unchanged from 06a.7 (191 core unit + 3 integration tests passing; clippy and machete clean).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |

## 12. Cleanup registry

To be filled if any subtask surfaces a pre-existing bug per the
`AGENTS.md` "pre-existing bugs surfaced during a subtask" rule.

| ID  | Surface | Impact | Fix scope | Status |
| --- | ------- | ------ | --------- | ------ |

## 13. Phase B — real semantics

Phase B owns the migration from the v0 stub to a native execution
pipeline. It cannot start before PLAN_05 produces a baseline corpus
pass-rate (ADR 0003), before PLAN_08 produces the per-builtin and
per-feature spec sheets that drive prioritisation, and before
PLAN_09's F1 differential is green (06.0 gate, below).

### 13.1. Gating dependencies

- **06.0 (gate)** — PLAN_09 F1 differential fuzzer must be green
  against pinned bash 5.3p9 on `main` (PLAN_09 §3.1). No Phase B
  implementation subtask may land while F1 is red. F1 is the
  single signal that says the executor is stable enough that
  divergences observed during Phase B implementation are about
  the change under review, not about pre-existing drift.
- **Per-builtin / per-feature** — each in-scope Tier-1 builtin and
  each grammar feature requires a `support`-classed PLAN_08 sheet
  before its implementation subtask begins. PLAN_08 sheets are
  the prose acceptance criteria; the corpus is the executable
  acceptance criteria.
- **ADR 0004 sunset** — Phase B retires the `/bin/sh -c` fallback
  path and the `FREDSHELL_ALLOW_SH_FALLBACK` escape hatch. The
  sunset commit lands as Phase B's final subtask (§13.7) and is
  contingent on the corpus pass-rate threshold below.

### 13.2. Lexer and parser scope

The Phase A stub stores source verbatim and dispatches per line.
Phase B replaces it with a native lexer + parser producing a
typed AST.

**Lexer.** Hand-rolled state-machine lexer in
`crates/fredshell-core/src/parser/lex/`:

- Tokens: word, operator, reserved word, newline, IO number,
  here-doc body.
- Quoting modes: unquoted, single-quoted, double-quoted, ANSI-C
  (`$'...'`), locale-translated (`$"..."` — recognise, do not
  translate in v1; refuse cleanly).
- Backslash handling per quoting mode.
- Comments (`#`) outside quotes.
- Here-doc body capture (delayed lexing until the line
  terminator).
- Position tracking for diagnostics: byte offset, line, column.
- No allocation per token in the common case — token spans are
  `&str` slices over the source buffer.

**Parser.** Recursive-descent over the lexer's token stream. AST
node families:

- `Program` → `CompleteCommand*`.
- `CompleteCommand` → `List` (`;`/`&`/`&&`/`||` separated).
- `List` → `Pipeline+` (`|`/`|&` separated).
- `Pipeline` → `Command+`.
- `Command` = simple | compound | function-definition.
- Simple = `(assignment* redirect* word*)`.
- Compound = `{ ... }`, `( ... )`, `if/elif/else/fi`,
  `while/do/done`, `until/do/done`, `for/do/done`, `case/esac`,
  `select/do/done`, `[[ ... ]]`, `(( ... ))`.
- `FunctionDefinition` = `name () compound` or
  `function name [()] compound`.

Q06B.1 was resolved on 2026-05-23: write our own
recursive-descent parser, for total control over diagnostic
quality and incremental parsing (PLAN_07's highlighter needs
the parser to tolerate a partial line). ADR 0005 (subtask
06b.1) ratifies this resolution before subtask 13B.2 starts.

`coproc` is recognised but refused for v1; reserved word `time`
is recognised and dispatched to the `time` keyword-level
builtin.

### 13.3. Executor pipeline

The executor consumes the AST and produces side-effects. The
pipeline in execution order:

1. **Expansion.** Six passes per word, in bash's order:
   brace → tilde → parameter/command/arithmetic → word-split →
   pathname → quote-removal. Lives in
   `crates/fredshell-core/src/exec/expand/` with one module per
   pass.
2. **Redirection setup.** Open files, dup fds, capture
   here-docs into pipes (or tempfiles above a configurable
   threshold). Failures are reported as `ExecError::Redirect`
   and do not start the command.
3. **Command resolution.** Look up the command name against, in
   order: aliases → functions → builtins (Tier-1 then Tier-2) →
   `PATH`-resolved external. Resolution returns a
   `ResolvedCommand` enum so dispatch is a single match.
4. **Dispatch.**
   - Builtin: call in-process; `ExitStatus` is the return.
   - Function: push a function-call frame on `ShellState`;
     execute the function body recursively; pop.
   - External: `fork` + `execve` via PLAN_04's `Process` API;
     `wait` if foreground, register in job table if background.
5. **Pipeline composition.** Wire the previous command's
   stdout to the next command's stdin via `pipe(2)`; the entire
   pipeline runs in a fresh process group; exit status is the
   last command's by default, all-status under `set -o
pipefail`.
6. **Job-table side-effects.** PLAN_10 owns the job table; the
   executor's contract is that every external command lands in
   that table with a known state by the time `dispatch` returns.

The expansion code is the single largest source of bash-quirk
risk. Every expansion pass has its own PLAN_08 sheet (the
`expansion/*` family). Implementation does not begin on a pass
until its sheet is `support`-classed and at least one passing
spec case exists.

### 13.4. `ShellState` fields

Phase B promotes `ExecEnv` from "I/O + sandbox" to "I/O +
sandbox + shell state." The new `ShellState` struct (private to
`fredshell-core::exec::state`) holds:

| Field         | Type                               | Owner   | Purpose                                                  |
| ------------- | ---------------------------------- | ------- | -------------------------------------------------------- |
| `variables`   | `Scope` tree                       | PLAN_06 | Shell + environment variables; supports `local` scoping. |
| `functions`   | `BTreeMap<String, FunctionDef>`    | PLAN_06 | User-defined functions; AST captured at definition.      |
| `aliases`     | `BTreeMap<String, String>`         | PLAN_06 | Pre-parse expansion; only at line-start position.        |
| `options`     | `SetOpts`                          | PLAN_06 | `set -o` long-form and `-e/-u/-x/-o pipefail/...` flags. |
| `shopts`      | `ShoptOpts`                        | PLAN_06 | `shopt` flag set (bash-only options).                    |
| `pos_args`    | `Vec<String>`                      | PLAN_06 | `$0`/`$1`.../`$@`.                                       |
| `last_status` | `ExitStatus`                       | PLAN_06 | `$?`.                                                    |
| `last_pid`    | `Option<Pid>`                      | PLAN_06 | `$!`.                                                    |
| `last_arg`    | `Option<String>`                   | PLAN_06 | `$_`.                                                    |
| `traps`       | `TrapTable`                        | PLAN_10 | Slot; PLAN_06 reserves the field but does not populate.  |
| `jobs`        | `JobTable`                         | PLAN_10 | Slot; PLAN_06 reserves the field but does not populate.  |
| `dirs_stack`  | `Vec<PathBuf>`                     | PLAN_10 | `pushd`/`popd`/`dirs`; slot only.                        |
| `umask`       | `mode_t`                           | PLAN_10 | Slot only.                                               |
| `cmd_hash`    | `HashMap<String, PathBuf>`         | PLAN_10 | `hash` builtin cache; slot only.                         |
| `history`     | `&mut dyn HistoryStore` (borrowed) | PLAN_07 | Borrowed from the editor; not owned by `ShellState`.     |

`ShellState` is owned by `ExecEnv` (one field). `ExecEnv` retains
its existing `cwd` / `env` / sandbox flags; those become views
on `ShellState::variables` for the env half. A small migration
window keeps both as separate fields with synchronisation
helpers; the duplicate is removed in §13.6.

`Scope` is a stack of frames; each frame is a
`BTreeMap<String, Variable>` plus an `is_function` flag. `local`
pushes; function return pops. Variable lookup walks the stack
from top to bottom.

### 13.5. Builtin inventory by owner

PLAN_05 §11.1 is the canonical disposition table; PLAN_06 owns
exactly the rows marked PLAN_06 there. Reproduced here as a
checklist (no semantic content; if it disagrees with PLAN_05
§11.1, PLAN_05 wins):

**PLAN_06 — Tier-1 builtins (38).**

`:`, `.`, `[`, `alias`, `break`, `builtin`, `cd` (extend
existing), `command`, `continue`, `declare`, `echo`, `enable`,
`eval`, `exec`, `exit` (already implemented), `export`,
`false`, `let`, `local`, `pwd`, `readonly`, `return`, `set`,
`shift`, `shopt`, `source`, `test`, `true`, `type`\* (split with
PLAN_10), `typeset`, `unalias`, `unset`.

(\*`type` is dual-owned: command-kind resolution table is
PLAN_06; the `-a` exhaustive listing uses PLAN_10's `hash`
cache and PATH search machinery.)

Each builtin lands as its own subtask once its PLAN_08 sheet is
`support`-classed. The largest by surface area are `test`,
`declare`, `set`, `shopt`, and `exec`; the smallest (`:`,
`true`, `false`) ship together.

**PLAN_10 — Tier-1 builtins (19).** Listed for cross-reference
only; implementation tracked in PLAN_10:

`bg`, `caller`, `dirs`, `disown`, `fg`, `getopts`, `hash`,
`help`, `jobs`, `kill`, `logout`, `mapfile`, `popd`, `printf`,
`pushd`, `read`, `readarray`, `suspend`, `times`, `trap`,
`ulimit`, `umask`, `wait`.

**PLAN_07 — Tier-1 builtins (2).** `fc`, `history`. Listed for
cross-reference; implementation tracked in PLAN_07 §8.6.

**Tier-2.** The Tier-2 registry and dispatcher wiring is a
Phase B deliverable (§13.6 subtask 13B.5). Individual Tier-2
implementations (e.g., `ls`, `cat`, `du`) are inventoried by
PLAN_08 sheets and tracked as sub-subtasks under 13B.5; they
are not enumerated here.

### 13.6. ADR 0004 fallback removal

`FREDSHELL_ALLOW_SH_FALLBACK=1` is removed before v1.0. The
sunset is split into two stages:

1. **Stage 1 (mid-Phase-B).** The fallback path remains but emits
   a stderr warning the first time it is hit per process:
   "fredshell: command %q did not match a native parse; falling
   back to /bin/sh -c. This fallback will be removed in v1.0.
   Set FREDSHELL_ALLOW_SH_FALLBACK=0 to make this a hard error
   today." This pressure-tests the corpus: any divergence the
   warning reveals must result in either a new spec case or a
   `refuse!` shim.
2. **Stage 2 (end of Phase B).** The fallback code path is
   deleted. `FREDSHELL_ALLOW_SH_FALLBACK` is no longer read.
   The `spawn_via_sh` helper is deleted. The exit gate for this
   stage is the threshold below.

**Phase B exit gate.** Phase B is complete (and ADR 0004
sunset stage 2 is unlocked) when all of the following are
true:

- Every PLAN_06-owned Tier-1 builtin in §13.5 has a
  `support`-classed PLAN_08 sheet and at least one passing
  corpus case.
- Every expansion pass (§13.3) has a `support`-classed PLAN_08
  sheet and ≥10 passing corpus cases.
- PLAN_09 F1 (every PR), F2 (nightly), and F3 (weekly)
  differential tiers have been green on `main` for 14
  consecutive days.
- Real-world script corpus pass rate ≥ 95% (PLAN_05 §6
  metric).

The threshold is intentionally strict: ADR 0004 promises the
fallback exists _only_ until the native pipeline is good
enough to remove it. Stage 2 lands the day the threshold is
hit; we do not run with a quietly-deprecated fallback.

### 13.7. Subtask grid

Subtask numbering: `06b.N` (Phase B). Format matches §10 / §11.

| Subtask | Surface                                          | Gate                  |
| ------- | ------------------------------------------------ | --------------------- |
| 06b.0   | Phase B gate: PLAN_09 F1 green on `main`         | PLAN_09 09.7 complete |
| 06b.1   | ADR 0005: parser implementation choice           | 06b.0                 |
| 06b.2   | Lexer (`parser/lex/`) + tests                    | 06b.1                 |
| 06b.3   | Parser (`parser/grammar/`) + AST + tests         | 06b.2                 |
| 06b.4   | `ShellState` skeleton + scope tree               | 06b.0                 |
| 06b.5   | Tier-2 registry + dispatcher                     | 06b.4                 |
| 06b.6   | Expansion: brace                                 | 06b.3, 06b.4          |
| 06b.7   | Expansion: tilde                                 | 06b.6                 |
| 06b.8   | Expansion: parameter (incl. `${...}` operators)  | 06b.6                 |
| 06b.9   | Expansion: command substitution                  | 06b.8                 |
| 06b.10  | Expansion: arithmetic                            | 06b.8                 |
| 06b.11  | Expansion: word-split + pathname + quote-removal | 06b.6                 |
| 06b.12  | Redirection setup (incl. here-docs)              | 06b.3, 06b.4          |
| 06b.13  | Pipeline execution + process-group setup         | 06b.12                |
| 06b.14  | Control flow: `if`/`while`/`until`/`for`/`case`  | 06b.3, 06b.4          |
| 06b.15  | Function definitions + call frames               | 06b.4                 |
| 06b.16  | Trivial builtins: `:`/`true`/`false`             | 06b.4                 |
| 06b.17  | Variable builtins: `export`/`readonly`/`unset`   | 06b.4                 |
| 06b.18  | Scope builtins: `local`/`declare`/`typeset`      | 06b.4, 06b.15         |
| 06b.19  | Option builtins: `set`/`shopt`                   | 06b.4                 |
| 06b.20  | `test`/`[` (huge surface — own batch)            | 06b.4                 |
| 06b.21  | `[[ ]]` keyword (parser + executor)              | 06b.3, 06b.20         |
| 06b.22  | `(( ))` keyword + arithmetic eval                | 06b.3, 06b.10         |
| 06b.23  | Alias builtins: `alias`/`unalias`                | 06b.4                 |
| 06b.24  | Reentrant builtins: `eval`/`source`/`.`          | 06b.3, 06b.4          |
| 06b.25  | `command`/`builtin`/`type` (resolution path)     | 06b.4, 06b.5          |
| 06b.26  | `exec` (process-replace + fd manipulation)       | 06b.12                |
| 06b.27  | `enable` (toggle builtin disposition)            | 06b.4, 06b.16         |
| 06b.28  | `let` + arithmetic builtin                       | 06b.22                |
| 06b.29  | Reserved words: `time` keyword integration       | 06b.3                 |
| 06b.30  | ADR 0004 sunset stage 1 (fallback warning)       | 06b.16–06b.29         |
| 06b.31  | Real-world corpus pass-rate baseline             | 06b.30                |
| 06b.32  | Exit-gate verification + ADR 0004 sunset stage 2 | 06b.31                |

Subtasks 06b.6–06b.11 (expansion family) are sequenced as
listed because each pass consumes the previous pass's tokens.
Subtasks 06b.16–06b.29 (builtin family) are mostly
independent and can run in parallel after their gates clear;
the order above reflects priority by frequency of use in the
real-world corpus, not dependency.

PLAN_10 subtasks land in parallel with PLAN_06 Phase B; the
two plans share the §13.4 `ShellState` slots but otherwise
operate independently.

### 13.8. Open questions

- **Q06B.1** — Parser strategy: in-house vs `brush-parser` vs
  fork. **Resolved (2026-05-23):** in-house recursive-descent.
  Rationale captured in ADR 0005 (subtask 06b.1) and supported
  by the prose at §13.2: diagnostic quality, incremental
  parsing for the PLAN_07 highlighter (partial-line
  tolerance), lossless CST for the future formatter, and
  parse-stage / alias gating (per PLAN_09 §11 Q09.3) all
  require parser internals we are unwilling to outsource.
  ADR 0005 authoring remains subtask 06b.1; it ratifies this
  resolution rather than re-litigating it.
- **Q06B.2** — `coproc` support. Default: recognise and refuse
  in v1; defer real implementation to v1.1. **Resolved
  (2026-05-23):** v1 emits `ParseError::Unsupported { feature:
"coproc" }` per `PLAN_16_coproc.md`; full implementation is
  owned by PLAN_16 when picked up post-v1. No Phase B subtask.
- **Q06B.3** — Here-doc temp-file threshold. **Resolved
  (2026-05-23):** bodies ≤ 64 KiB are delivered via pipe;
  bodies > 64 KiB are spilled to a tempfile under `$TMPDIR`
  with `unlink`-immediately-after-open semantics so cleanup
  survives signals. The threshold is a named const
  (`HEREDOC_PIPE_MAX = 64 * 1024`) in the executor module, not
  configurable at runtime. PLAN_08 here-doc spec sheets pin
  the threshold and include at least one case at body size
  `HEREDOC_PIPE_MAX - 1` and one at `HEREDOC_PIPE_MAX + 1` so
  the boundary is regression-tested. FD-table introspection
  (`/proc/self/fd` on Linux, `/dev/fd` on macOS) is permitted
  to diverge from bash at the boundary: bash on Linux uses an
  anonymous pipe up to a similar threshold, bash on macOS
  always uses a tempfile.
- **Q06B.4** — `$RANDOM` and `$SECONDS` determinism in the
  spec harness. **Resolved (2026-05-23):** the harness pins
  both per case. Each case's `[harness]` block (PLAN_08 sheet
  schema) accepts optional `random_seed: u32` and
  `seconds_offset: u64` fields. fredshell consumes them from a
  harness-only env channel (`FREDSHELL_HARNESS_RANDOM_SEED`,
  `FREDSHELL_HARNESS_SECONDS_OFFSET`) that is never exposed
  outside the spec runner. The reference bash receives the
  same pin via `RANDOM=<seed>` injected before script
  execution and a `faketime`-style clock shim for `$SECONDS`.
  When fields are absent, the harness applies workspace
  defaults (`random_seed = 0`, `seconds_offset = 0`) so every
  case is reproducible without per-case config. PLAN_05
  status taxonomy gains no new variant; pinning is an input,
  not a status.
- **Q06B.5** — Locale-translated strings (`$"..."`).
  **Resolved (2026-05-23):** the parser accepts `$"..."` as
  syntactically valid (one CST node with the
  locale-translated marker), and the executor refuses with:

  ```text
  ExecError::Unsupported {
      feature: "locale_translation",
      suggestion: "v1 has no message-catalog loader; \
                   remove the leading `$` to use the \
                   literal string",
  }
  ```

  Refusing cleanly preserves the contract that scripts which
  depend on translations fail loudly rather than silently
  losing them. Full `gettext`-style support is post-v1 work;
  if picked up, it gets its own PLAN_XX owning document.
  PLAN_08 has a refusal-corpus case under
  `tests/spec/refusals/` (`locale_translation.case.toml`)
  asserting the refusal diagnostic text.

### 13.9. Relationship to other plans

- **PLAN_05** — corpus and harness. Phase B is measured by
  corpus pass-rate; the §13.6 exit gate references PLAN_05 §6.
- **PLAN_08** — spec sheets. Each Phase B implementation
  subtask consumes a `support`-classed sheet; no sheet, no
  implementation.
- **PLAN_09** — fuzzer + differential. 06b.0 gates the entire
  phase; F2/F3 thresholds gate ADR 0004 sunset stage 2.
- **PLAN_07** — line editor. Phase B exposes the `history`
  store via a borrowed `HistoryStore` trait object on
  `ShellState`; the `fc` and `history` builtins are dispatched
  by PLAN_06 to entry points whose semantics live in PLAN_07
  §8.6.
- **PLAN_10** — traps and jobs. Phase B reserves the §13.4
  slots (`traps`, `jobs`, `dirs_stack`, `umask`, `cmd_hash`)
  but does not populate them; PLAN_10 owns population.
- **PLAN_15** — milestones. The Phase B exit gate corresponds
  to the v1.0 milestone gate.

## References

- PLAN_02 §4 (public surface target), §12 (implementation status).
- PLAN_05 (spec harness consumer).
- PLAN_08 (spec drafting — per-builtin and per-feature acceptance
  criteria that gate Phase B work).
- PLAN_09 (differential + fuzzer — correctness measurement for
  Phase B).
- PLAN_10 (traps + job control — consumes Phase B's hook points).
- ADR 0003 (test-first compatibility — why Phase A exists at all).
- ADR 0004 (strict-default execution — sunset path for the
  `FREDSHELL_ALLOW_SH_FALLBACK` escape hatch).
