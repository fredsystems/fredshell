# ADR 0006 — `fredshell-core` Embeddable Library Contract

- Status: accepted
- Date: 2026-05-24
- Supersedes: —
- Superseded by: —

## Context

`fredshell` is being built as a daily-driver shell, but the value of a
correct, well-tested shell core is not limited to the `fredshell`
binary. The primary embedding target is freminal — a terminal
emulator that benefits from owning the shell instead of spawning one
as a child process — and similar embedders (test harnesses, IDE-style
shells, tooling that wants programmatic control over a real shell
session) follow the same shape.

Today `fredshell-core` is structured as a library, but only the
`fredshell` binary actually drives it. Several decisions made without
embedding in mind have already accreted assumptions that would not
survive a second consumer:

- The executor writes directly to inherited file descriptors. An
  embedder that owns the terminal cannot intercept that output without
  redirecting fds, which loses framing.
- Diagnostics are emitted to `stderr` as formatted strings. An embedder
  cannot route diagnostics to its own UI, log them structurally, or
  suppress them.
- Exit status, prompt requests, and other shell-state transitions are
  observable only as side effects on a TTY. There is no typed event
  the embedder can react to.
- Control flow is implicitly driven by the binary's REPL loop. There
  is no documented entry point for "advance the shell by one unit of
  work" or "feed this byte stream into the shell."

This ADR establishes the contract that makes `fredshell-core`
embeddable without breaking the existing binary. It is foundational:
every later subsystem (parser, executor Phase B, line editor,
prompt, traps and jobs) must respect this contract or be reworked
to do so. The cost of getting this wrong scales with how much code
depends on the old shape, so the contract is fixed now, before the
Phase B executor and the native parser land.

The detailed work plan that implements this contract lives in
`PLAN_10_embedding.md`. This ADR records the architectural decision
only.

## Decision

`fredshell-core` exposes its observable behavior to embedders as an
**async `Stream` of typed events**, with input driven by a small set
of `async` methods on a `Shell` handle. Embedders own the event loop,
the runtime, the terminal, and all I/O. The core owns parsing,
execution, builtin dispatch, shell state, and the production of
events; it does not own any file descriptor that an embedder might
want to control.

### Concretely

- **`Shell` handle.** `fredshell-core` exposes a `Shell` type owned by
  the embedder. The `fredshell` binary holds exactly one `Shell` and
  drives it from its REPL; an embedder may hold one per session.
- **Async input methods.** Input is delivered to the `Shell` via
  `async` methods such as `feed_line(&mut self, line: &str)` and
  `feed_bytes(&mut self, bytes: &[u8])`. These methods return when the
  core has consumed the input and produced (or queued) the resulting
  events. They do not block on external command completion;
  completion surfaces as an event on the stream.
- **Typed event stream.** `Shell` exposes
  `fn events(&mut self) -> impl Stream<Item = ShellEvent>` (exact
  shape — owned vs borrowed, single vs split — is a `PLAN_10` design
  question, not an ADR question). `ShellEvent` is an enum covering at
  minimum: `Output { stream, bytes }`, `Diagnostic { level, message,
source_span }`, `ExitStatus { code }`, `PromptRequest { state }`,
  `JobStateChanged { job_id, state }`, and `ReadlineRequest` for
  builtins that consume stdin.
- **Runtime contract.** The core is **runtime-agnostic**: it depends
  on `futures` core traits (`Stream`, `Future`, `AsyncRead`,
  `AsyncWrite`) but not on `tokio`, `async-std`, or `smol`. The
  `fredshell` binary picks `tokio`. Embedders pick whatever they
  already use.
- **No fd writes from core.** `fredshell-core` does not write to
  `stdout`, `stderr`, or any inherited file descriptor for shell
  output, diagnostics, or prompts. All such output is emitted as a
  `ShellEvent`. The binary's REPL is the component that translates
  events into terminal writes.
- **External processes still own their fds.** When the executor
  spawns a child process, the child's stdio is connected to pipes
  owned by the core, and the core surfaces the bytes read from those
  pipes as `Output` events. The child does not inherit the
  embedder's terminal directly. This is the design point that makes
  freminal-style embedding work; it also makes the binary REPL's
  output composable with future features (recording, replay, AI
  context capture).
- **`ShellHost` is not a callback trait.** The earlier framing of
  "embedder implements `ShellHost` with `on_output`, `on_exit`,
  etc." is rejected here: it inverts control in a way that conflicts
  with the embedder's existing event loop and forces the core to
  call into embedder code at points the embedder cannot easily
  reason about. The `Stream` model lets the embedder pull events at
  whatever rate its own loop runs.
- **The `fredshell` binary is the reference embedder.** Anything the
  binary needs from the core is part of the public embedding surface
  by construction. No private back door.

### What this ADR does not decide

- The exact signatures of `Shell`'s async methods.
- Whether `events()` returns a single multiplexed stream or several
  topic-specific streams (`output_stream()`, `diagnostic_stream()`,
  ...).
- The error model returned from `feed_*` methods (typed enum vs.
  `Result<(), ShellError>` with a single error type).
- Whether the `Stream` borrows `&mut self` (single-consumer,
  back-pressured) or returns an owned receiver (multi-consumer,
  buffered).
- The TTY/PTY ownership model for child processes (PTY pair allocated
  per session vs. per command vs. on demand).
- The fan-out for line-editing input — i.e., whether keystrokes are
  fed into the core or processed by an embedder-owned line editor
  that hands completed lines to the core. The `fredshell` binary
  uses the latter today; `PLAN_10` decides whether that stays
  binary-only or becomes part of the embedding surface.

These belong in `PLAN_10_embedding.md`. The ADR fixes the shape —
async streams of typed events, no fd writes from core, runtime-
agnostic — and lets the plan decide the details.

## Consequences

### Positive

- freminal and similar embedders can drive a real shell with the same
  guarantees the binary gets: real parsing, real execution, real
  state. No `bash -c` shim, no PTY scraping.
- All shell output becomes observable and addressable. Recording,
  replay, AI context capture, structured logging, and IDE-style
  decoration of shell output all become library features rather than
  binary hacks.
- The core has no implicit dependency on a TTY. Tests that exercise
  the executor can assert against the event stream directly, without
  a PTY harness. This collapses several categories of "we need a TTY
  to test this" into ordinary unit tests.
- The fd-write boundary forces the binary's REPL to be a thin
  translator (events → terminal writes). That layer is small,
  testable, and replaceable.
- Runtime-agnostic core means embedders are not forced to adopt
  `tokio`. The binary's choice of `tokio` is an implementation
  detail of the binary, not a contract.

### Negative

- The current synchronous, fd-writing executor must be reworked
  before Phase B. Every output path becomes an event emission. This
  is invasive but localized: the executor is the only component
  that writes to fds today.
- The async machinery (`Stream`, `Future`, `async fn`) is a non-
  trivial dependency added to the core. The binary already uses
  `tokio`; the core gains `futures-core` and friends.
- Diagnostics-as-events removes the convenience of `eprintln!` from
  the core. Every diagnostic must now be a typed value with
  enough context for the embedder to render it sensibly. This is
  more work per diagnostic, but matches the broader project rule
  that errors are explicit, typed, and structured (AGENTS.md).
- The event enum becomes a public API surface. Adding variants is
  cheap pre-1.0 but eventually becomes a SemVer concern.

### Risks accepted

- **Async API drift before 1.0.** The exact shape of `Shell` and
  `ShellEvent` will change as `PLAN_10` materializes. Mitigation:
  the binary is the only embedder pre-1.0, and breaking changes
  cost one PR. freminal integration is gated on the API stabilizing.
- **Runtime-agnostic costs.** Staying off `tokio` in the core means
  giving up `tokio`-only conveniences (named tasks, runtime metrics,
  `tokio::select!`). Mitigation: the core's async surface is
  small — event emission and a few `async fn` entry points. The
  shapes we lose are not on the critical path.
- **PTY ownership ambiguity for child processes.** The core needs
  to allocate PTYs for jobs that expect a TTY (interactive editors,
  `less`, `vim`). Whether one PTY is shared per session or one is
  allocated per foreground job is unresolved here. `PLAN_10`
  decides; the wrong answer is recoverable because PTY allocation
  is encapsulated inside the executor.

## Alternatives considered

### Callback-based `ShellHost` trait (inversion of control)

The original sketch in the project's working notes. Embedder
implements `trait ShellHost { fn on_output(&mut self, bytes: &[u8]);
fn on_exit(&mut self, code: i32); ... }`; core calls the embedder.
**Rejected.** The embedder is already running an event loop (terminal
emulator, IDE, test harness). Inverting control means the core
re-enters the embedder at moments the embedder cannot easily reason
about — usually from inside an async task spawned by the core's
runtime. Back-pressure is impossible to express. Errors raised by
the embedder during a callback have nowhere clean to go. The
`Stream`-of-events model lets the embedder pull events at its own
rate, with ordinary `?` for errors.

### Synchronous `step()` driven by the host

`shell.step()` returns a `Vec<ShellEvent>` and the embedder calls it
on its frame tick. **Rejected.** Shell execution is inherently
async: child processes do not complete on a fixed tick, builtins
that read stdin block, and signal handling and job control require
non-trivial scheduling. A synchronous `step()` either (a) blocks
the embedder's event loop for unbounded time, or (b) buffers events
internally and lies about when work happens, which makes back-
pressure and ordering inscrutable. Async streams are the honest
shape.

### Tokio-required core

Require the core to run on `tokio`; let embedders adopt `tokio` or
not embed. **Rejected.** freminal does not currently use `tokio`
and has no reason to adopt it just to embed a shell. The
runtime-agnostic constraint costs us very little (the core's async
surface is narrow) and unlocks the embedder population that already
exists. The binary remains on `tokio` because the binary is allowed
to choose.

### Keep the current fd-writing executor; expose a "tap" for embedders

Add an optional output redirection hook that copies fd writes to an
embedder-supplied sink. **Rejected.** This is a half-measure that
preserves the worst property of the current design: the core
believes it owns the terminal. Embedders get bytes but no
structure — they cannot distinguish a builtin's diagnostic from a
child's stdout from a prompt repaint. The cost of doing the
restructuring is high once Phase B lands; doing it now is cheaper
than doing it later.

### Defer the decision until after Phase B

Let the executor evolve as it needs to and revisit embeddability
when freminal integration starts. **Rejected.** Phase B is the work
that hardens the executor's fd handling. Locking in the fd-writing
shape during Phase B and then removing it is roughly twice the work
of building Phase B against the event-emitting shape from the
start.

## References

- `PLAN_02_architecture.md` — workspace structure; a library-first
  architecture section will land alongside this ADR's implementation
  plan.
- `PLAN_10_embedding.md` — the work plan that implements this ADR.
  Owns the concrete `Shell` and `ShellEvent` design, the PTY
  ownership model, and the binary's REPL refactor to consume events.
- `PLAN_11_parser.md` — the native parser, whose output feeds the
  executor that emits events under this contract.
- `PLAN_12_exec_phase_b.md` — the Phase B executor, which is the
  first executor written against this contract from the start.
- `AGENTS.md` — "Errors must be explicit, typed, and structured."
  Diagnostics-as-events is the embedding-facing realization of that
  rule.
- ADR 0001 — in-process execution; this ADR refines the I/O boundary
  of that decision.
- ADR 0004 — strict-default execution; the strict-vs-fallback choice
  remains, but both paths now emit events rather than writing to
  fds.
