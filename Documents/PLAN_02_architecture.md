# PLAN_02 — Architecture

> Last updated: 2026-05-24 — cross-references remapped for the
> work-order renumber (old PLAN_06 Phase B → PLAN_11; old PLAN_08/09/10
> spec/fuzz/jobs → PLAN_07/08/12; old PLAN_11/12/13/14/15 prompt/config/
> nix/AI/milestones → PLAN_14/15/16/17/18). §12 refreshed for PLAN_06
> (Phase A execution-pipeline skeleton implemented; §4.1/§4.2/§4.3/§4.5
> surfaces and §9 bench scaffolding moved to Implemented).
> Phase: A. Status: draft.

This document defines fredshell's crate layout, module boundaries, key
public types, and dependency direction. It is constrained by ADR 0001
(in-process execution + builtin tiers), ADR 0002 (encoder-focused ANSI
crate), ADR 0003 (test-first methodology), and PLAN_05 (the spec
harness, which is the most demanding consumer of `fredshell-core`'s
public API).

If something in PLAN_05 conflicts with this document, PLAN_05 wins.

## 1. Design tenets

These are the binding rules. Specific decisions follow from them.

1. **Library first, binary thin.** Every behavior fredshell offers is
   reachable from a library API. The `fredshell` binary is a thin
   shell over the library: it owns the line editor, the TTY, the
   process arguments, and nothing else of substance. Anything the
   binary can do, a programmatic embedder can do.
2. **Parser separable from executor.** The parser is a pure function
   from `&str` to AST. It performs no I/O, holds no state, and does
   not know that an executor exists. This is non-negotiable; it is
   what makes the spec harness possible and what makes the parser
   independently testable.
3. **Executor takes an explicit environment.** The executor never
   reads ambient state. `std::env::var`, `std::env::current_dir`,
   `std::process::stdout`, and friends do not appear in
   `fredshell-core` outside the public boundary that constructs an
   `ExecEnv` from the host process. All bytes out, all bytes in, all
   env vars, all working-directory operations go through the
   `ExecEnv`.
4. **Synchronous core, async at the edges.** The parser and the
   executor are synchronous Rust. Async lives only where it must:
   AI feature plumbing, possibly background prompt segments, possibly
   completion providers. The core does not import `tokio` or `async-std`.
5. **Typed errors, no `anyhow` in libraries.** Per AGENTS.md. Each
   crate exports its own error enum. The binary may use `color-eyre`
   at the top level. xtask may use `anyhow`.
6. **No panics in production code.** Per AGENTS.md. Errors that
   represent program bugs (e.g., "the dispatch table is empty")
   surface as typed errors with explicit variants, not `expect()`.
7. **Dependency direction is downward.** A crate may only depend on
   crates listed below it in the table in section 3. If a change
   would require an upward dependency, the design is wrong.

## 2. Architectural shape

```text
                  ┌──────────────────────────────────────────┐
                  │   fredshell (binary)                     │
                  │   - argv parsing                         │
                  │   - TTY setup / raw mode                 │
                  │   - REPL loop                            │
                  │   - error reporting to the user          │
                  └─────┬────────────────────┬───────────────┘
                        │                    │
       ┌────────────────▼─┐       ┌──────────▼────────────┐
       │ fredshell-line-  │       │ fredshell-prompt      │
       │ editor (TBD §5)  │       │ - segment renderer    │
       │ - key decoding   │       │ - async slow segments │
       │ - history        │       └──────────┬────────────┘
       │ - completion     │                  │
       └────────────────┬─┘                  │
                        │                    │
                        ▼                    ▼
                 ┌──────────────────────────────────────┐
                 │  fredshell-core                      │
                 │  - parser (pure fn)                  │
                 │  - executor (takes ExecEnv)          │
                 │  - tier-1 builtins                   │
                 │  - tier-2 builtin trait + registry   │
                 │  - dispatch table                    │
                 │  - shell state (vars, fns, aliases,  │
                 │    job table, history view)          │
                 └──────────────┬───────────────────────┘
                                │
                                ▼
                 ┌──────────────────────────────────────┐
                 │  fredshell-ansi                      │
                 │  - encoder API (Write-based)         │
                 │  - minimal decoder (DA1, DSR, etc.)  │
                 └──────────────────────────────────────┘

   fredshell-spec-runner ──depends on──> fredshell-core
   (test-only crate; not a runtime dependency of the binary)
```

The line editor and the prompt are separate crates because they have
distinct concerns (key decoding/redraw vs. segment rendering) and
distinct dependency surfaces (line editor needs raw-mode terminal I/O;
prompt needs git + language detection). They are both consumed by the
`fredshell` binary, neither depends on the other directly.

## 3. Crate inventory

| Layer | Crate                   | Role                                                           | `anyhow`?  | Async? | Exists? |
| ----- | ----------------------- | -------------------------------------------------------------- | ---------- | ------ | ------- |
| App   | `fredshell`             | Binary entry: argv, TTY, REPL loop, error reporting            | yes        | yes    | yes     |
| App   | `fredshell-line-editor` | Key decoding, line buffer, history view, completion plumbing   | no         | no     | no      |
| App   | `fredshell-prompt`      | Starship-style segment renderer with async slow segments       | no         | yes    | yes     |
| Lib   | `fredshell-core`        | Parser, executor, `ExecEnv`, tier-1 + tier-2 builtin dispatch  | no         | no     | yes     |
| Lib   | `fredshell-ansi`        | Encoder-focused ANSI escape-sequence library (ADR 0002)        | no         | no     | yes     |
| Test  | `fredshell-spec-runner` | Spec corpus runner (PLAN_05); depends only on `fredshell-core` | yes (test) | no     | no      |
| Dev   | `xtask`                 | Build/CI orchestration; compat + spec record commands          | yes        | no     | yes     |

The PLAN_06 (bash compat) decision about whether to adopt
`brush-parser` or write our own will determine whether the parser is
an internal module of `fredshell-core` or a separate crate. The
default assumption in this document is **internal module**, on the
grounds that the parser and executor share AST and `Span` types and
their stability is coupled. Splitting them later is cheap if a third
party wants the parser standalone.

### 3.1. Dependency direction

Allowed dependencies (downward only):

- `fredshell` → `fredshell-line-editor`, `fredshell-prompt`,
  `fredshell-core`, `fredshell-ansi`.
- `fredshell-line-editor` → `fredshell-core`, `fredshell-ansi`.
- `fredshell-prompt` → `fredshell-core`, `fredshell-ansi`.
- `fredshell-core` → `fredshell-ansi` (for tier-2 builtins that emit
  styled output; the dependency is narrow and may be revisited).
- `fredshell-ansi` → no other workspace crates.
- `fredshell-spec-runner` → `fredshell-core` only.
- `xtask` → any.

Disallowed:

- `fredshell-core` depending on `fredshell`, `fredshell-prompt`, or
  `fredshell-line-editor`.
- `fredshell-prompt` depending on `fredshell-line-editor` or vice
  versa.
- `fredshell-spec-runner` depending on the `fredshell` binary or its
  app-layer crates.

This is enforced by the workspace `Cargo.toml` and, for tier-1
violations, by a `cargo xtask check-deps` lint that fails CI.

## 4. `fredshell-core` public surface

This is the part of the architecture that the spec harness pins down.
The signatures below are the **shape**, not the final API; later
documents and implementation will refine field names, generic
parameters, and lifetimes. The shape is binding.

### 4.1. The parser

```rust
/// Parse a bash-language source string into an AST.
///
/// Pure function: no I/O, no global state, no environment access.
/// Errors are recoverable and structured.
pub fn parse(source: &str) -> Result<Ast, ParseError>;

pub struct Ast { /* sealed; opaque to consumers, walkable via visitor */ }

pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
    pub message: String,
}

pub enum ParseErrorKind {
    UnexpectedToken,
    UnterminatedString,
    UnterminatedHeredoc,
    InvalidParameterExpansion,
    // … one variant per categorically distinct failure mode.
}

pub struct Span { pub start: usize, pub end: usize }
```

`parse` is the harness's entry point. The harness does not need to walk
the AST; it only needs to know whether parsing succeeded and to pass
the AST to the executor.

### 4.2. The execution environment

```rust
/// The environment a script executes in. Constructed by the host
/// (binary or harness), passed to the executor, owned by the caller.
pub struct ExecEnv {
    /// Working directory. The executor mutates this on `cd`.
    pub cwd: PathBuf,

    /// Environment variables visible to the script. Shell variables
    /// (set without `export`) live elsewhere; see `ShellState`.
    pub env: HashMap<OsString, OsString>,

    /// Standard streams. Boxed so the host can substitute pipes,
    /// in-memory buffers (harness), or terminal-backed handles.
    pub stdin: Box<dyn Read + Send>,
    pub stdout: Box<dyn Write + Send>,
    pub stderr: Box<dyn Write + Send>,

    /// Mutable shell-level state: variables, functions, aliases,
    /// shell options (`set -e`, `shopt`), the job table.
    pub shell: ShellState,

    /// Builtin dispatch table. Constructed once per session;
    /// tests may swap in a minimal or augmented registry.
    pub builtins: BuiltinRegistry,

    /// Path-resolution policy: how `$PATH` is interpreted, whether
    /// the host filesystem is reachable, hashing of resolved paths.
    pub path_policy: PathPolicy,

    /// Signal-handling policy. `Install` (binary default) installs
    /// handlers for SIGINT, SIGTSTP, SIGCHLD, SIGPIPE, SIGALRM and
    /// routes signals to the foreground process group. `None`
    /// (harness default) installs no handlers; child processes
    /// inherit defaults. See §6.1 for the responsiveness model.
    pub signal_policy: SignalPolicy,
}
```

### 4.3. The executor

```rust
/// Execute a parsed AST. Mutates `env` (cwd, vars, shell state) and
/// writes bytes to `env.stdout` / `env.stderr`. Returns the final
/// exit status of the script.
pub fn execute(ast: &Ast, env: &mut ExecEnv) -> Result<ExitStatus, ExecError>;

pub struct ExitStatus(pub i32);

pub enum ExecError {
    /// A builtin or external command was not found.
    CommandNotFound { name: String, span: Span },
    /// A redirection target could not be opened.
    Redirection { target: PathBuf, source: io::Error, span: Span },
    /// `exec` was called with a missing target.
    ExecFailure { source: io::Error, span: Span },
    /// Catastrophic: the host's I/O streams failed.
    HostIo(io::Error),
    /// The executor encountered a state it considers a bug.
    /// Never produced in normal operation; surfaced for tests.
    InternalInvariant { what: &'static str },
    // … additional categorical failures.
}
```

The executor never returns an exit status of "implicit error." Either
the script ran (possibly with a non-zero exit, surfaced in
`ExitStatus`) or the executor itself failed (`ExecError`). These are
different categories and the harness treats them differently.

### 4.4. Tier-1 builtins

Tier-1 builtins are POSIX shell builtins (`cd`, `pwd`, `export`,
`unset`, `set`, `shift`, `read`, `eval`, `exec`, `:`, `true`, `false`,
`echo`, `printf`, `test`, `[`, `trap`, `wait`, `return`, `break`,
`continue`, `source`/`.`).

They live in `fredshell-core::builtins::tier1`. They are not pluggable.
They are dispatched directly by the executor based on the command name
and have access to the full `ExecEnv`, including `shell` and process
machinery.

### 4.5. Tier-2 builtins

Tier-2 builtins (`ls`, `cat`, `du`, `df`, `which`, `head`, `tail`,
`wc`, `sort`, `uniq`, etc., per ADR 0001) implement a trait:

```rust
pub trait Tier2Builtin: Send + Sync {
    /// Canonical name (e.g. "ls"). Lowercase ASCII.
    fn name(&self) -> &'static str;

    /// Optional aliases (e.g. `ll` could route to `ls -l`).
    fn aliases(&self) -> &'static [&'static str] { &[] }

    /// Invoke the builtin. Receives a narrow slice of the executor's
    /// environment — not the full `ExecEnv` — so tier-2 builtins
    /// cannot mutate shell state or the job table.
    fn invoke(&self, ctx: Tier2Ctx<'_>) -> Result<ExitStatus, Tier2Error>;
}

pub struct Tier2Ctx<'a> {
    pub args: &'a [OsString],
    pub cwd: &'a Path,
    pub env: &'a HashMap<OsString, OsString>,
    pub stdin: &'a mut dyn Read,
    pub stdout: &'a mut dyn Write,
    pub stderr: &'a mut dyn Write,
    /// Cooperative cancellation. Set by the signal handler on SIGINT
    /// (and by `timeout`/`read -t` via SIGALRM). See §6.1.3.
    pub cancellation: &'a AtomicBool,
}
```

The narrow context is deliberate: a tier-2 `ls` has no business
touching the job table or installing signal handlers. Tier-2 builtins
that need anything richer are mis-categorized and belong in tier 1.

### 4.6. Dispatch order

Per ADR 0001, the executor resolves a simple command in this order:

1. Aliases (looked up in `ShellState::aliases`).
2. Functions (looked up in `ShellState::functions`).
3. Tier-1 builtins.
4. Tier-2 builtins (subject to `ShellState::opts.tier2_enabled` and
   to user overrides like `enable -n ls`).
5. External executables on `$PATH` (resolved per `PathPolicy`).

The dispatch table is built at session construction time. Looking up
a command is O(1) for tier-1 and tier-2 (hashmap), O(n) for aliases
and functions because both are small.

### 4.7. Module layout inside `fredshell-core`

Provisional:

```text
fredshell-core/src/
  lib.rs               — re-exports the public surface
  ast.rs               — AST types, Span, visitor
  parser/              — parser implementation (module-internal)
    mod.rs
    tokenizer.rs
    grammar.rs
    error.rs
  exec/
    mod.rs             — execute() and the executor state machine
    pipeline.rs        — pipeline + redirection handling
    expansion.rs       — parameter, command, brace, pathname expansion
    arithmetic.rs      — $(()) and (()) evaluation
    job.rs             — job table, process group management
    signal.rs          — signal dispatch policy
    error.rs           — ExecError
  builtins/
    mod.rs             — BuiltinRegistry, dispatch
    tier1/             — one file per POSIX builtin
    tier2/             — one file per replacement builtin
  shell_state.rs       — variables, functions, aliases, opts
  env.rs               — ExecEnv definition + constructors
  path_policy.rs
  signal_policy.rs
```

The exact file split is provisional; what is binding is that
`parser/` and `exec/` are sibling modules that do not depend on each
other except through the AST types defined in `ast.rs`.

## 5. The line editor decision

This is an open question. Two options:

### Option A — Build on `reedline`

`reedline` is the line-editor library used by nushell. It is
maintained, handles raw mode and history reasonably, and is already
in the dependency graph (the current scaffold lists it).

Pros: large surface working out of the box; less code to maintain;
nushell exercises it heavily so corner cases get found.

Cons: opinionated about keybindings and rendering in ways that may
conflict with the kitty-keyboard / fredshell-ansi vision; couples
fredshell's keystroke latency budget to `reedline`'s redraw model;
extending the prompt protocol (multi-line, async segments, syntax
highlighting overlays) means fighting the library.

### Option B — Roll our own on top of `crossterm` or raw `nix`

Build `fredshell-line-editor` as a first-party crate.

Pros: full control over the redraw model, keystroke latency, kitty
keyboard support, and the interaction with `fredshell-ansi` and the
prompt; no impedance mismatch with PLAN_04 (terminal I/O) and PLAN_13
(interactive UX).

Cons: significantly more code to write and to test (the L4 PTY layer
will need to be richer); the line editor is a known hazard area
(reedline has burned ~five years of bug-fixing on edge cases).

### Provisional decision

**Option B** — roll our own — is the provisional direction, gated on
PLAN_13 confirming it. The reasoning:

- The keystroke latency budget (<1ms) is tight. Owning the redraw
  model lets us bound it.
- Kitty keyboard protocol negotiation, bracketed paste, OSC handling,
  and the prompt's async segment integration all want a line editor
  designed around those concerns, not a generic one retrofitted.
- The reedline scaffold-dependency in `Cargo.toml` is from
  bootstrapping and is removed before milestone 1 if Option B holds.

PLAN_13 will record the final decision. Until then, `fredshell-line-
editor` is reserved as a crate name in this document but does not yet
exist in `crates/`.

## 6. Async strategy

The core is synchronous Rust. Async appears in three places:

1. **Prompt async segments.** PLAN_14 owns the model. Likely:
   `tokio` single-threaded runtime owned by the binary, prompt
   segments are futures, slow segments render placeholders and resolve
   asynchronously between keystrokes. The runtime is created in the
   binary and threaded into the prompt crate. The core never sees it.
2. **AI features.** PLAN_17 owns this. Same runtime as the prompt;
   AI providers expose async client APIs. The core does not depend on
   the runtime; the binary mediates between the runtime and the core.
3. **Background completion.** Possibly. Completion can fan out to
   tools (`git`, `cargo`) that take measurable time. If completion
   becomes async, the runtime is the same one as above. PLAN_13
   decides.

Notable non-uses of async:

- The executor is synchronous. Pipelines block on child-process I/O
  using `nix`/`libc` directly. Job control uses real signals and real
  `waitpid`. This is deliberate; async shells produce confusing
  semantics for `wait` and trap handling.
- The parser is synchronous. There is no use case for async parsing.
- The spec harness is synchronous (per PLAN_05 §10 open question).

## 6.1. Responsiveness without async

"Synchronous core" must not be confused with "blocking core." A shell
that blocks indefinitely on a stuck child process, ignores Ctrl-C
while a builtin is running, or freezes its prompt while `git fetch`
hangs is a broken shell. Real bash is synchronous C with careful
signal handling and `poll`/`select`; it is responsive without an
async runtime. fredshell takes the same path, and this section pins
down what that means concretely.

The architecture commits to five mechanisms. PLAN_04 (signals, raw
mode) and PLAN_06 (executor, job control) own the implementations.
This document owns the commitments.

### 6.1.1. Signal-correct foreground job control

The executor installs handlers for `SIGINT`, `SIGTSTP`, `SIGCHLD`,
and `SIGPIPE` at session construction (when `SignalPolicy::Install`
is in effect; the spec harness uses `SignalPolicy::None`). Signals
delivered to the shell from the controlling TTY are routed to the
foreground process group, not absorbed silently. Ctrl-C is observable
to running children; the shell itself ignores `SIGINT` once it has a
foreground pgid that owns the tty.

This is the standard POSIX job-control model. It is not optional and
it is not negotiable.

### 6.1.2. Multiplexed wait + input via `pselect`/`poll`

The executor never calls a bare blocking `waitpid` while the shell
also wants to remain interactive. When the shell is in the REPL state
between commands, it waits for either input on the tty fd or
`SIGCHLD` (background job completion) using `pselect` with an
appropriate signal mask. When a foreground job is running, the shell
yields the tty to the child's pgid and waits on `SIGCHLD` for that
pgid specifically.

Background jobs (`&`) make progress because their state is reaped
opportunistically via `waitpid(WNOHANG)` from the `SIGCHLD` handler
(or, on platforms where `signalfd` is preferable, via the equivalent
read). The prompt re-renders job-status changes (`[1]+ Done ...`) the
next time it draws.

### 6.1.3. Cancellation tokens for long-running builtins

`Tier2Ctx` exposes a cancellation flag:

```rust
pub struct Tier2Ctx<'a> {
    pub args: &'a [OsString],
    pub cwd: &'a Path,
    pub env: &'a HashMap<OsString, OsString>,
    pub stdin: &'a mut dyn Read,
    pub stdout: &'a mut dyn Write,
    pub stderr: &'a mut dyn Write,
    /// Set by the signal handler when SIGINT is delivered while
    /// this builtin owns the foreground. Builtins that loop MUST
    /// check this between iterations and return early with the
    /// conventional 130 exit status when set.
    pub cancellation: &'a AtomicBool,
}
```

Tier-1 builtins receive the same flag through the broader executor
state. A tier-2 builtin that loops without checking cancellation is
a correctness bug; the spec corpus includes cancellation tests for
the builtins where this matters (`grep -r`, `find`, `sort` over
large inputs, etc.) once those are implemented.

The flag is the **only** cross-thread synchronization primitive
required by the executor. Atomics, not channels, not async.

### 6.1.4. Timeout via `setitimer` / `SIGALRM`

`timeout` (the bash-compatible builtin / external command) and
`read -t` are implemented via `setitimer(ITIMER_REAL, ...)` plus a
`SIGALRM` handler that sets the cancellation flag and signals the
relevant pgid. No async timer runtime is involved.

This generalizes: any built-in deadline behavior in the executor
flows through the same mechanism. The cost is one global per-session
itimer slot, multiplexed by a priority queue if multiple deadlines
are concurrently active.

### 6.1.5. `poll`-driven pipeline I/O

Pipeline fd plumbing uses `poll` (or `epoll`/`kqueue` where
beneficial) rather than blocking reads on individual fds. A stuck
producer in `cmd1 | cmd2 | cmd3` does not prevent the consumer side
from receiving `SIGPIPE` and exiting cleanly. Process substitutions
(`<(cmd)`, `>(cmd)`) use the same machinery.

Implementation lives in `fredshell-core::exec::pipeline`. The
abstraction is narrow enough that the harness can substitute an
in-memory transport for tests that exercise pipeline semantics
without spawning real children.

### Summary

| Concern                       | Mechanism                                | Owner           |
| ----------------------------- | ---------------------------------------- | --------------- |
| Ctrl-C interrupts foreground  | Signal routing to foreground pgid        | PLAN_04, exec   |
| Ctrl-Z stops foreground       | Signal routing + tty handoff             | PLAN_04, exec   |
| Background jobs progress      | `SIGCHLD` handler + `waitpid(WNOHANG)`   | exec::job       |
| Hung child does not freeze UI | `pselect` multiplex of tty + SIGCHLD     | binary + exec   |
| Builtin cancellation          | `AtomicBool` in `Tier2Ctx`               | core API        |
| Timeouts                      | `setitimer` + `SIGALRM`                  | exec            |
| Pipeline stuck producer       | `poll`-driven fd loop                    | exec::pipeline  |
| `git fetch` hangs prompt      | Prompt async segment + tokio (in binary) | PLAN_14, binary |

Nothing in this list requires an async runtime. The shell stays
responsive because POSIX signals and `pselect`/`poll` exist and are
the right tools for this job.

## 7. Error strategy

Each library crate exports a single top-level error enum:

```rust
// fredshell-core
pub enum CoreError {
    Parse(ParseError),
    Exec(ExecError),
    // …
}

// fredshell-ansi
pub enum AnsiError {
    Io(io::Error),
    UnknownEscape { /* … */ },
    // …
}
```

Variants are categorical, not free-text. A new failure mode means a
new variant. `thiserror` is acceptable for the boilerplate; the
variants and their `Display` impls are hand-written.

The binary's top-level error type wraps the library errors with
`color-eyre` context and renders user-friendly messages. No
`anyhow::Error` crosses a library boundary.

## 8. State ownership

Where does each piece of state live?

| State                         | Lives in                       | Mutated by                 |
| ----------------------------- | ------------------------------ | -------------------------- |
| Shell variables               | `ShellState::vars`             | Executor, `set`, `unset`   |
| Environment variables         | `ExecEnv::env`                 | Executor, `export`         |
| Functions                     | `ShellState::functions`        | Executor (definition)      |
| Aliases                       | `ShellState::aliases`          | Executor (`alias` builtin) |
| Shell options (`set`/`shopt`) | `ShellState::opts`             | Executor                   |
| Job table                     | `ShellState::jobs`             | Executor                   |
| Current working directory     | `ExecEnv::cwd`                 | Executor (`cd`)            |
| Builtin registry              | `ExecEnv::builtins`            | Constructed at session     |
| Command history               | App-layer (line editor crate)  | Line editor                |
| Prompt state                  | App-layer (`fredshell-prompt`) | Prompt renderer            |
| Completion cache              | App-layer (line editor crate)  | Completion provider        |
| Tty / raw-mode state          | App-layer (`fredshell` binary) | Binary                     |

The boundary is sharp: `fredshell-core` knows about variables, jobs,
and aliases. It does not know about history or the tty. The harness
constructs an `ExecEnv` with empty shell state and exercises the core
without any app-layer crate present.

## 9. Performance budget allocations

The global budgets from PLAN_01 (G5) decompose roughly as:

| Budget                              | Owner                | Allocation                                                                                      |
| ----------------------------------- | -------------------- | ----------------------------------------------------------------------------------------------- |
| Cold start <50ms                    | Binary               | argv parse + config load + line editor init + first prompt render. Core init is ~negligible.    |
| Per-keystroke <1ms                  | Line editor + prompt | Decode key (<50µs) + line buffer mutation (<50µs) + redraw (<900µs) + prompt re-eval if needed. |
| Prompt re-render <10ms median       | Prompt               | Sync segments only; async segments may take longer and render placeholders.                     |
| Exec overhead ≤20% vs raw fork/exec | Core                 | Parser + dispatch + ExecEnv setup. Measured in benches `bench/exec_overhead.rs`.                |

The benchmark suite (per AGENTS.md and PLAN_05 L5) covers each
budget. Regressions >15% require justification per AGENTS.md.

## 10. What is **not** in this document

These belong to other docs and are not re-litigated here:

- The actual bash grammar coverage and parser strategy (PLAN_06
  Phase B; per-feature spec sheets in PLAN_07).
- The line-editor design details (PLAN_13).
- The prompt segment protocol (PLAN_14).
- The terminal I/O machinery — raw mode, signals, process groups,
  feature detection (PLAN_04).
- The ANSI encoder API surface (PLAN_03).
- The config file format (PLAN_15).
- The Nix module surface (PLAN_16).
- The AI feature provider abstraction (PLAN_17).
- The tier-2 builtin inventory and priority order (PLAN_11;
  per-builtin spec sheets in PLAN_07).
- Job control (`trap`, `wait`, `kill`, `jobs`, `fg`, `bg`, `disown`,
  `suspend`) and signal-disposition tables (PLAN_12).
- The differential harness, fuzzer, and sandbox hardening (PLAN_08).
- The milestone schedule (PLAN_18, Phase B).

## 11. Open questions

- **Parser as separate crate?** Default: internal module of
  `fredshell-core`. Revisit if a third-party embedder wants the
  parser standalone.
- **Async runtime choice (tokio vs smol).** Default: `tokio` with the
  `rt` feature only (no `rt-multi-thread` unless a concrete need
  surfaces). PLAN_14 / PLAN_17 confirm.
- **Line editor: reedline vs own.** Provisional: own. PLAN_13 confirms.
- **`fredshell-ansi` as a dependency of `fredshell-core`.** Tier-2
  builtins benefit from styled output, but pulling ANSI into the
  core makes the boundary slightly less clean. Alternative: tier-2
  builtins receive a `&dyn StyleWriter` from the app layer. PLAN_03
  and PLAN_06 settle.
- **`OsString` vs `String` at API boundaries.** The signatures above
  use `OsString` for env vars and args (correct for POSIX) but the
  ergonomics tax for tests is real. PLAN_06 surfaces concrete cases
  and the decision is settled there.
- **Job-control granularity.** Whether `ShellState::jobs` is the
  primary owner of process-group state or whether the binary owns
  the tty side and the core owns the bookkeeping side is open.
  PLAN_04 and PLAN_06 jointly own.

## 12. Implementation status

This section tracks which parts of the architecture have backing code
and which are still specification-only. It is updated whenever a
PLAN_NN document flips status.

### Implemented

- **`fredshell-ansi`** (PLAN_03 — implemented). Encoder-focused crate,
  minimal decoder (DA1, DSR, kitty, DECRPM). Allocation budget met.
- **`fredshell-core::tty`** (PLAN_04 — implemented). Terminal I/O,
  signals, capability detection, raw-mode RAII, pselect multiplexer,
  process-group plumbing. Backs §6.1.1 (signal handlers installed,
  foreground pgid routing), §6.1.2 (`pselect` multiplex of tty +
  self-pipe), §6.1.3 (the cancellation `AtomicBool` exists as
  `CancellationToken`, exposed by `TerminalSession`).
- **`fredshell-core::repl`** (PLAN_04 04.10 — implemented). REPL on
  top of `TerminalSession`; raw-mode byte-pump with cooked-stdin
  fallback; dispatches each line through `exec::run_source` (PLAN_06
  Phase A, subtask 06a.6).
- **§4.1 Parser surface** (PLAN_06 Phase A — surface only). `parse`,
  `Script`, `ParseError`, `ParseErrorKind` exist as a stub: the v0
  parser stores the source string verbatim and rejects NUL bytes.
  Real tokenisation, AST, and `ParseErrorKind` variants land with
  PLAN_11.
- **§4.2 `ExecEnv`** (PLAN_06 Phase A — surface only). `ExecEnv` exists
  with `from_process` and `sandboxed` constructors and v0 fields
  (`cwd`, `env: HashMap<String, String>`, `last_status`). Full
  `ShellState` fields and the Tier-2 builtin registry land with
  PLAN_11.
- **§4.3 Executor surface** (PLAN_06 Phase A — surface only).
  `run_source`, `run_script`, `RunResult { status, exit_requested }`,
  `RunError`, `ExecError`, `ExitStatus` exist. The v0 dispatcher walks
  lines, routes Tier-1 builtins through `builtins::try_run`, and falls
  back to `/bin/sh -c <line>` for everything else. Real executor lands
  with PLAN_11.
- **§4.5 Tier-2 trait surface** (PLAN_06 Phase A — surface only).
  `Tier2Builtin` (object-safe, `Send + Sync`), `Tier2Ctx<'a>`, and
  `Tier2Error` exist with a compile-time object-safety check. No
  impls; no registry; dispatch lands with PLAN_11.
- **§9 Bench scaffolding** (PLAN_06 Phase A — surface only).
  `crates/fredshell-core/benches/exec_roundtrip.rs` exists with
  `parse_only` and `parse_and_exec` Criterion benches, seeding the
  performance budget tracker. Baseline numbers recorded in
  PLAN_06 §11 row 06a.7. Real budget enforcement lands with
  PLAN_11.
- **`fredshell`** (binary, scaffold). Wires `TerminalSession` and
  the REPL; installs `SIGQUIT=SIG_IGN`. Argv / one-shot mode work.
- **`fredshell-spec-runner`** (PLAN_05 — implemented). Library +
  binary spec-corpus harness. `Case::load`, `Sandbox`, and
  `run_case` execute `*.case.toml` cases against fredshell in
  strict execution mode (no `/bin/sh` fallback) and compare
  observed stdout / stderr / exit against recorded fixtures.
  Verdict taxonomy (`ExpectedPass` / `Regression` / `ExpectedFail` /
  `WontfixHonored` / `DeferredHonored` / `Reclassify`) classifies
  outcomes against case status. `cargo xtask compat` walks the
  corpus, emits a v1 JSON report, renders `COMPAT.md`, and gates
  CI on regressions. `cargo xtask spec record` produces sidecars
  against the pinned reference bash; `cargo xtask spec lint`
  validates schema, detects orphan fixtures, and checks §11.1
  builtins drift. 21-case seed corpus committed.

### Specification-only (waiting for implementation)

- **§4.1 Parser semantics**. The v0 parser does not tokenise; it
  stores the source string verbatim. Real lexer, AST, and
  POSIX-grammar coverage land with PLAN_11.
- **§4.2 `ExecEnv` semantics**. `ExecEnv` does not yet carry
  functions, aliases, shell options, or a job table. `ShellState`
  lands with PLAN_11; the job-control slice on top of it
  is owned by PLAN_12.
- **§4.3 Executor semantics**. The v0 dispatcher delegates external
  commands to `/bin/sh -c`. Real fork/exec, pipelines, and
  redirections land with PLAN_11; job control lands with
  PLAN_12.
- **§4.4 Tier-1 builtins**. Only `cd`, `exit`, and a few stubs exist
  in `fredshell-core::builtins`. The full Tier-1 set lands
  incrementally across PLAN_11 (per the §11.1 inventory in
  PLAN_05_testing); job-control-related builtins (`trap`, `wait`,
  `kill`, `jobs`, `fg`, `bg`, `disown`, `suspend`) are owned by
  PLAN_12.
- **§4.5 Tier-2 builtin impls**. The trait exists; no builtins
  implement it yet. Per-builtin specifications live in PLAN_07
  sheets; inventory and dispatch land with PLAN_11.
- **§4.6 Dispatch order**. Today's `dispatch_line` does
  builtin-then-fallback-to-/bin/sh. The full alias → function →
  tier-1 → tier-2 → external order lands with PLAN_11.
- **§6.1.4 Timeouts** (`setitimer` + `SIGALRM`). PLAN_04 catches
  `SIGALRM` and sets the cancellation flag; no `setitimer` wiring
  exists. Wired by PLAN_12 when `read -t` / `timeout` land.
- **§6.1.5 Pipeline `poll` loop**. No pipeline executor exists.
  Lands with PLAN_11.
- **§8 `ShellState`** (vars, functions, aliases, opts). Not
  implemented. Lands with PLAN_11; the `jobs` slot is
  filled in by PLAN_12.
- **§9 Performance budget enforcement**. Bench scaffolding exists
  (06a.7) but budgets are not enforced. Enforcement lands with
  PLAN_11.

### Deferred

- **`fredshell-line-editor`** crate — waits for PLAN_13.
- **§3.1 `cargo xtask check-deps` lint** — not implemented. Dep
  direction is enforced by convention today. Added when a violation
  is attempted or before milestone 1.

### Recently resolved open questions

- **`fredshell-ansi` as a dep of `fredshell-core`.** Resolved by
  PLAN_04: `fredshell-core` already depends on `fredshell-ansi` (the
  capability probe consumes the decoder). The "`&dyn StyleWriter`
  from the app layer" alternative is dropped.
- **`OsString` vs `String`.** Resolved by PLAN_06 Phase A: the
  skeleton uses `String` for v0 (test ergonomics, no real env-var
  handling yet). PLAN_11 promotes to `OsString` when real env
  handling lands. Recorded as a known migration cost in PLAN_06 §7.
- **Parser as separate crate.** Resolved by PLAN_06 Phase A:
  internal module of `fredshell-core`. Revisit only if a third-party
  embedder asks.

## References

- `Documents/decisions/0001-in-process-execution-and-builtin-tiers.md`
  — the tier model this architecture enforces.
- `Documents/decisions/0002-ansi-encoding-crate-strategy.md`
  — `fredshell-ansi` as a separate crate.
- `Documents/decisions/0003-test-first-compatibility-methodology.md`
  — why the parser and executor must be separable.
- `Documents/PLAN_01_philosophy.md` — goals G1–G6 and non-goals.
- `Documents/PLAN_05_testing.md` — the constraints this document
  satisfies (separable parser, sandboxable executor, batch entry,
  testable tier-2 dispatch).
- `Documents/PLAN_03_ansi.md` (pending) — the ANSI crate consumed by
  the app-layer crates and (tentatively) by tier-2 builtins.
- `Documents/PLAN_04_terminal_io.md` (pending) — terminal feature
  detection, raw mode, signal handling, process groups.
- `Documents/PLAN_06_exec.md` — execution pipeline (Phase A
  skeleton implemented; Phase B parser, executor, full builtin set,
  and Tier-2 inventory not yet drafted).
- `Documents/PLAN_13_line_editor.md` — line editor design,
  finalizes Option A/B.
- `Documents/PLAN_07_spec_drafting.md` — per-builtin and
  per-feature spec sheets; gates PLAN_11 (exec Phase B) work.
- `Documents/PLAN_08_fuzzer.md` — differential harness, fuzzer,
  sandbox hardening, bash testsuite import.
- `Documents/PLAN_12_traps_and_jobs.md` — signal disposition
  (`trap`), job control (`jobs`, `fg`, `bg`, `wait`, `kill`),
  `setitimer` wiring.
- `Documents/PLAN_14_prompt.md` — prompt segment protocol
  and async runtime usage.
- `Documents/PLAN_17_ai_features.md` (pending) — AI runtime usage.
- `AGENTS.md` — dependency direction rules, panic-free production,
  typed errors.
