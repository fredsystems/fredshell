# PLAN_14 — Prompt Renderer

> Last updated: 2026-05-20 — first draft.
> Phase: A. Status: draft.
> Consumes: PLAN_02 (architecture, async strategy), PLAN_03 (encoders),
> PLAN_04 (terminal session), PLAN_13 (line editor frame integration).
> Consumed by: PLAN_13 (frame composition), PLAN_12 (config),
> PLAN_14 (AI segments, optional).

This document specifies the fredshell prompt renderer: the segment
model, the synchronous and asynchronous evaluation paths, the
configuration surface, the integration with the line editor's frame
model, and the performance budget. PLAN_02 §6 commits the binary to
a tokio single-threaded runtime; this document operationalizes that
for prompt segments and records the alternatives that were rejected.

The prompt is the most visible single piece of fredshell. It is also
the most performance-sensitive part of the shell after the keystroke
path. The architecture is shaped around the budget: <10 ms median
re-render, <1 ms for sync-only segments, slow segments must never
block re-render.

## 1. Scope and non-scope

### In scope (v1)

- **Segment model.** Prompt content is a tree of segments
  (directory, git branch, git status, last-status, cmd-duration,
  hostname, user, virtualenv, character, …). Segments are typed,
  composable, and evaluated independently.
- **Synchronous and asynchronous segments.** Sync segments execute
  inline during prompt construction. Async segments run on the
  shared tokio runtime; their first emission is a placeholder
  (or empty), and a subsequent re-render replaces it with the
  resolved value.
- **Right-side prompt (RPS1).** Same model, drawn at the right
  margin of the first prompt row.
- **Continuation prompt (PS2).** Drawn for multi-line buffer
  continuations (PLAN_13 §11.3 invokes it).
- **Transient prompt.** When a command is submitted, the prompt
  collapses to a compact form so scrollback stays clean.
- **Configuration.** Starship-config-compatible TOML for a
  curated subset of modules (§7). fredshell-native extensions
  use namespaced keys.
- **Frame integration.** The prompt produces a `Vec<FrameRow>`
  consumed by PLAN_13's frame builder, not a flat ANSI string.
  Style is per-cell (matches the editor's frame model).
- **Refresh on background events.** Job status changes
  (`[1]+ Done ...`), async segment completion, and SIGWINCH
  trigger a re-render between keystrokes.
- **Caching.** Per-segment caches with explicit invalidation
  rules (cwd change, git HEAD change, status code change, …).
  No general-purpose memoization — every cache is a deliberate
  decision per segment kind.

### Out of scope (v1)

- **Custom segment scripting.** Users cannot write Lua / Wasm /
  shell scripts as segments. Only the curated set of typed
  segments is available. Adding a new segment kind requires a
  fredshell source change.
- **Rainbow / "powerline" graphical chrome.** Powerline
  separators _are_ supported as a styling option; multi-segment
  background-color blending with custom gradients is not.
- **Full starship parity.** Starship has ~80 modules; fredshell
  v1 ships ~15. The config schema is starship-shaped where
  shapes overlap, divergent where v1 omits a module.
- **Inline async indicators.** A spinner that animates while a
  segment resolves. Animations require a timer; deferred.
- **Per-segment fonts / ligatures / icon-pack negotiation.**
  Users configure their terminal's font; fredshell does not.

## 2. Design tenets

1. **Sync segments are free.** Every sync segment must complete
   in <100 µs in steady state. Anything slower is async by
   construction. "Slow sync" is a bug.
2. **Async segments never block the prompt.** First render of an
   async segment shows the placeholder; resolution triggers a
   refresh. The prompt is drawn now, not after.
3. **Re-render is cheap.** Diff-based via PLAN_13's frame model.
   Re-rendering the full prompt every keystroke is acceptable
   only because the frame diff makes the byte cost trivial.
4. **No user code on the hot path.** Configuration is data, not
   code. There is no Lua, no eval. Adding a primitive means
   adding Rust.
5. **Caching is explicit.** Every cached value has a documented
   invalidation rule. No "magic" memoization. A stale prompt is
   worse than a slow prompt.
6. **Correctness on resize.** SIGWINCH triggers a re-render that
   produces a new frame; PLAN_13 owns the redraw mechanics.
   The prompt must be reconstructable from `PromptContext` alone.

## 3. Module layout

```text
crates/fredshell-prompt/src/
  lib.rs              — public surface: PromptRenderer, PromptContext
  config/
    mod.rs            — PromptConfig type + serde
    parse.rs          — starship-compatible TOML loader
  segment/
    mod.rs            — Segment trait, SegmentKind
    sync.rs           — synchronous segment dispatch
    async_seg.rs      — async segment driver
    cache.rs          — per-segment cache infrastructure
  modules/
    mod.rs            — registry mapping SegmentKind → impl
    directory.rs      — cwd with home-tilde, truncation
    git_branch.rs     — gix-based branch name
    git_status.rs     — gix-based status counts (async)
    status.rs         — last exit code
    duration.rs       — last cmd duration
    user.rs           — username (with ssh-only mode)
    hostname.rs       — hostname (with ssh-only mode)
    venv.rs           — Python / Node / Rust toolchain marker
    character.rs      — the trailing prompt character (❯, $, …)
    jobs.rs           — running job count
    time.rs           — current time with format
    shell_level.rs    — SHLVL indicator
    container.rs      — docker / podman / nix-shell marker
    ai_hint.rs        — optional, gated by PLAN_14 feature flag
  render/
    mod.rs            — segment list → Vec<FrameRow>
    layout.rs         — left/right alignment, separators
    transient.rs      — transient-prompt collapse
```

The crate stays in the same dependency position as the scaffold
(`fredshell-prompt` depends on `fredshell-core` and `fredshell-ansi`,
nothing depends on it except the binary and PLAN_13's frame
builder consumes its output via a typed return value).

## 4. Public surface

```rust
/// Owned by the binary. Created once, threaded into the line
/// editor and the REPL state.
pub struct PromptRenderer {
    config: PromptConfig,
    runtime: tokio::runtime::Handle,
    state: PromptState,    // caches, in-flight async segments
}

pub struct PromptContext<'a> {
    pub cwd: &'a Path,
    pub home: &'a Path,
    pub last_status: i32,
    pub last_duration: Option<Duration>,
    pub jobs_running: usize,
    pub shell_level: u32,
    pub is_ssh: bool,
    pub env: &'a EnvSnapshot,
    /// Width available for the prompt row (used by truncation /
    /// directory shortening). Coming from PLAN_04.
    pub width: u16,
}

/// What the line editor consumes.
pub struct PromptOutput {
    pub left: Vec<FrameRow>,
    pub right: Option<Vec<FrameRow>>,
    /// Columns occupied by the prompt on the first visual row.
    /// Fed into PLAN_13's WrapContext.first_line_indent.
    pub first_line_indent: u16,
    /// Continuation indent for subsequent visual rows of the
    /// command's row 0 (typically zero; non-zero if PS2-style
    /// indent is configured).
    pub continuation_indent: u16,
    /// Whether any async segments are still in flight. The line
    /// editor uses this to know whether a future re-render will
    /// have new content.
    pub async_pending: bool,
}

impl PromptRenderer {
    pub fn render(&mut self, ctx: &PromptContext<'_>) -> PromptOutput;

    /// Called by the line editor between keystrokes to check
    /// whether async segments have resolved and a re-render
    /// would produce different content.
    pub fn poll_async(&mut self) -> AsyncProgress;

    /// Render the transient form (post-submit collapse).
    pub fn render_transient(&self, ctx: &PromptContext<'_>) -> PromptOutput;

    /// Render the continuation prompt (PS2).
    pub fn render_continuation(&self, ctx: &PromptContext<'_>, depth: u32)
        -> PromptOutput;
}

pub enum AsyncProgress {
    /// No async segments outstanding.
    Idle,
    /// At least one segment resolved; line editor should re-render.
    Resolved,
    /// Segments still in flight; no change yet.
    Pending,
}
```

`render` is synchronous. It computes all sync segments inline, and
it _kicks off_ async segments (issuing them onto the runtime) but
does not wait. Async segments contribute a placeholder on first
render and the resolved value on a subsequent render.

## 5. Segment model

### 5.1. The `Segment` trait

```rust
pub enum SegmentKind {
    Directory, GitBranch, GitStatus, Status, Duration,
    User, Hostname, Venv, Character, Jobs, Time, ShellLevel,
    Container, AiHint, Custom(&'static str),
}

pub trait Segment: Send + Sync {
    fn kind(&self) -> SegmentKind;

    /// Compute the segment's content. May return Pending for
    /// segments that defer to async resolution.
    fn render(
        &self,
        ctx: &PromptContext<'_>,
        cache: &mut SegmentCache,
    ) -> SegmentRender;
}

pub enum SegmentRender {
    /// Resolved content; the renderer composes it directly.
    Ready(SegmentBody),
    /// Async resolution scheduled. Body is the placeholder.
    Pending {
        placeholder: SegmentBody,
        future: BoxFuture<'static, SegmentBody>,
    },
    /// Segment elects not to render this prompt (e.g., git
    /// branch outside a repo).
    Empty,
}

pub struct SegmentBody {
    /// Pre-styled cells. Style is computed by the segment from
    /// the config plus its own state (e.g., red branch name on
    /// dirty repo).
    pub cells: Vec<FrameCell>,
}
```

The trait is closed in practice: the only `impl Segment` types
live in `crates/fredshell-prompt/src/modules/`. The `Custom`
variant exists to allow PLAN_14's AI hint segment to live in a
feature-gated module without being part of `SegmentKind`'s
exhaustive match in the core registry; it is not user-extensible.

### 5.2. Sync vs async classification

A segment is sync or async by construction, recorded on the type:

| Segment kind        | Class         | Reason                                                     |
| ------------------- | ------------- | ---------------------------------------------------------- |
| `Directory`         | sync          | Pure pathbuf manipulation.                                 |
| `Status`            | sync          | Read from `PromptContext`.                                 |
| `Duration`          | sync          | Read from `PromptContext`.                                 |
| `User` / `Hostname` | sync          | Cached for shell lifetime.                                 |
| `Character`         | sync          | Trivial.                                                   |
| `ShellLevel`        | sync          | Env var read once.                                         |
| `Jobs`              | sync          | Read from `PromptContext`.                                 |
| `Time`              | sync          | `std::time` call.                                          |
| `Venv`              | sync          | Env var probes; bounded I/O via cache.                     |
| `GitBranch`         | sync (cached) | `gix` HEAD read; cached on cwd, invalidated on cwd change. |
| `GitStatus`         | async         | `gix` status traversal; can take seconds in large repos.   |
| `Container`         | sync (cached) | `/proc` and `/run` probes; cached.                         |
| `AiHint`            | async         | LLM call.                                                  |

The line between sync and async is the 100 µs budget (tenet 1).
A segment becomes async the moment its steady-state cost cannot
fit. `GitBranch` is sync because reading HEAD is fast and easily
cached; `GitStatus` is async because traversing a 50k-file
working tree is not negotiable from sync.

### 5.3. Per-segment caches

Each segment owns its cache. The trait gives it a typed
`SegmentCache` slot, opaque to the renderer:

```rust
pub trait SegmentCache: Any + Send {
    /// Invalidation hook. Called by the renderer when a
    /// known-relevant context change occurs. Return true if
    /// the cached value is now stale.
    fn invalidate(&mut self, signal: InvalidationSignal) -> bool;
}

pub enum InvalidationSignal {
    CwdChanged,
    EnvChanged(&'static str),
    StatusChanged,
    JobsChanged,
    GitHeadChanged,
    Resize,
    Manual,
}
```

The renderer routes invalidation signals to caches when
`PromptContext` deltas are detected. Each cache decides whether
to invalidate. Stale-on-purpose is forbidden; a cache that
returns wrong data is worse than a segment that always recomputes.

## 6. Async evaluation

### 6.1. Runtime ownership

The binary creates a `tokio::runtime::Builder::new_current_thread`
runtime at startup. The runtime handle is passed to:

- `PromptRenderer` (this crate).
- `CompletionEngine` if PLAN_13/PLAN_06 elects async completion.
- `AiClient` if PLAN_14 is enabled.

`fredshell-core` does not see the runtime. The synchronous
executor blocks on real syscalls; it never enters the runtime.

The single-threaded current-thread runtime is the right choice
because:

- Prompt segments are I/O-bound, not CPU-bound. There is no
  benefit to a multi-threaded runtime.
- A current-thread runtime can be polled in chunks between
  keystrokes without spawning OS threads.
- Cancellation (drop the future) is uniform.

### 6.2. The async driver

`render` issues async segments by:

1. Calling `Segment::render`, receiving `SegmentRender::Pending {
placeholder, future }`.
2. Spawning `future` onto the runtime via the handle.
3. Storing the resulting `JoinHandle<SegmentBody>` in
   `PromptState::in_flight`, keyed by segment id.
4. Emitting `placeholder` into the current frame.

The line editor calls `poll_async` between keystrokes (PLAN_13's
event loop has a natural quiescent point after each keystroke
is fully processed). `poll_async`:

1. Polls each `JoinHandle` non-blocking via the runtime handle.
2. For each completed future, replaces the segment's stored body
   in `PromptState`.
3. Returns `Resolved` if any segment completed (so the line
   editor re-renders), `Pending` otherwise, `Idle` if no
   segments are in flight.

A subsequent `render` call uses the resolved bodies in place of
placeholders. The renderer does _not_ re-issue segments whose
last result is still valid per their cache rules.

### 6.3. Cancellation and timeouts

- Each async segment has a per-segment timeout (default 2 s,
  configurable). On timeout the future is dropped and the
  segment shows the placeholder permanently for that prompt.
- When the cwd changes (or any other invalidation that affects
  the segment), in-flight futures for affected segments are
  dropped. The next `render` re-issues them.
- On shell exit, the runtime is shut down with a 100 ms grace
  period; pending futures are dropped.

The drop-future pattern requires segment futures to be
cancellation-safe. Concretely: each future is a single `gix`
call or a single HTTP request, not a sequence. If a segment
needs multi-step async work, it composes the steps into one
future at the segment level.

### 6.4. Refresh between keystrokes

The line editor's main loop (PLAN_13 §11) shape:

```text
loop {
    let event = wait_for_event_or_async_progress();
    match event {
        Key(k) => process_key(k),
        Resize(s) => process_resize(s),
        AsyncResolved => trigger_redraw(),
        Sigchld(j) => process_job_change(j),
    }
    if any_state_changed { redraw(); }
}
```

`AsyncResolved` is delivered via a channel from the runtime
into the main loop's event source. The main loop's `poll`/
`pselect` wait on:

- Terminal input fd (PLAN_04).
- Self-pipe for signals (PLAN_04).
- Async-progress eventfd (linux) or self-pipe (macos) fed by
  the tokio runtime when a segment completes.

This is the same multiplexing pattern as PLAN_02 §6.1.2; the
runtime's wakeup is just another fd in the `poll` set.

## 7. Configuration

### 7.1. Schema (starship-compatible subset)

```toml
# Top-level format string controls segment ordering and
# separators. Same syntax as starship.
format = "$directory$git_branch$git_status$character"

# Right-side prompt.
right_format = "$status$duration$time"

[directory]
truncation_length = 3
truncate_to_repo = true
home_symbol = "~"
style = "cyan bold"

[git_branch]
symbol = " "
style = "purple bold"

[git_status]
ahead = "⇡${count}"
behind = "⇣${count}"
modified = "[!${count}](red)"
untracked = "[?${count}](yellow)"
staged = "[+${count}](green)"

[character]
success_symbol = "[❯](green)"
error_symbol = "[❯](red)"

[status]
disabled = false
format = "[$status]($style) "
style = "red bold"

# fredshell-native extensions live under fredshell.* namespace
# to avoid conflicts with future starship modules.
[fredshell.transient]
enabled = true
format = "$directory $character"
```

The loader rejects unknown top-level modules in v1 (no silent
ignore). Unknown _keys_ within a known module log a warning.
Unknown values for typed fields (e.g., a non-color in `style`)
are errors.

### 7.2. Style language

Starship-compatible color/style strings: `"red"`, `"bold red"`,
`"#ff8800"`, `"bg:blue fg:white"`, `"underline 240"`. Implemented
on top of PLAN_03's `Sgr` type, with a small parser. The grammar
is documented inline; the parser is a unit-tested module.

Style strings produce `Sgr` values; `Sgr` values feed into
`FrameCell::style` per cell. There is no "styled string"
intermediate type — segments produce `FrameCell` directly.

### 7.3. Format-string expansion

The `format` field is expanded by a small template engine:

- `$segment` — render the segment, or empty if `Empty`.
- `[text](style)` — text with inline style.
- `${var}` — context variable interpolation (cwd, status, …).
- Bare text — passes through as cells.

The template engine is a pure function over `(format_str,
context, segment_outputs) → Vec<FrameCell>`. No shell-injection
risk because it does not eval anything.

### 7.4. Layered configuration

PLAN_12 owns the configuration loader. Prompt config is one
layer of the broader fredshell config, loaded from:

1. `$XDG_CONFIG_HOME/fredshell/prompt.toml` (fredshell-native).
2. `$STARSHIP_CONFIG` or `$XDG_CONFIG_HOME/starship.toml` if
   `prompt.starship_compat = true` is set in fredshell.toml.
3. Built-in defaults.

Layer 2 is opt-in; users who want to share config with starship
on other machines can flip the bit.

## 8. Frame integration

### 8.1. Output shape

`render` returns `PromptOutput` (§4). The `left` field is a
`Vec<FrameRow>`, typically of length 1 (most prompts are
single-line) but may be longer (multi-line prompts are
supported). Each `FrameRow` is `Vec<FrameCell>` matching PLAN_13's
frame model exactly: same cell type, same style representation.

PLAN_13's frame builder concatenates prompt rows, buffer rows,
and menu rows into the final frame. There is no string-based
intermediate; the prompt produces cells, the editor produces
cells, the diff layer compares cells.

### 8.2. First-line indent and continuation indent

The prompt computes `first_line_indent` as the visual width
(cells, not bytes) of the last row of the prompt. PLAN_13 uses
this when wrapping the buffer's first logical row.

`continuation_indent` is configurable; default 0. If a user sets
it (e.g., to 2 for visual indent on wrapped lines), PLAN_13's
wrap module applies it to non-first visual slices of row 0.

### 8.3. Right prompt

The right prompt is computed independently and rendered after
the left prompt. Its position is `width - right_visual_width`,
or skipped if `left_visual_width + right_visual_width + 1 >
width`. Truncation rules are starship-compatible: right prompt
disappears under pressure, left prompt truncates segments
according to per-segment rules.

Right-prompt cells go into PLAN_13's frame at the appropriate
column; they do not produce a separate row.

### 8.4. Transient prompt

When a command is submitted, the line editor calls
`render_transient` and re-emits the now-final prompt row using
the transient form. This collapses verbose prompts (e.g.,
multi-line with status info) to a compact form for clean
scrollback. The mechanism is a redraw of the prompt row only;
buffer rows are unchanged.

PLAN_13's submission path is responsible for triggering the
transient redraw before yielding to the executor.

## 9. Caching semantics

Cache discipline per segment:

| Segment      | Cached on                          | Invalidates on                      |
| ------------ | ---------------------------------- | ----------------------------------- |
| `Directory`  | `(cwd, home, width)`               | cwd change, resize                  |
| `GitBranch`  | `cwd` (canonicalized to repo root) | cwd change, GitHeadChanged          |
| `GitStatus`  | repo root + last-modified of index | cwd change, async re-poll on signal |
| `Status`     | not cached                         | every render                        |
| `Duration`   | not cached                         | every render                        |
| `User`       | shell lifetime                     | never                               |
| `Hostname`   | shell lifetime                     | never                               |
| `Venv`       | env var snapshot                   | env change                          |
| `Character`  | last_status                        | every render                        |
| `Jobs`       | jobs_running                       | every render                        |
| `Time`       | not cached                         | every render                        |
| `ShellLevel` | env var snapshot at start          | never                               |
| `Container`  | shell lifetime                     | never                               |

`GitHeadChanged` is detected by the executor: when a command
that may modify HEAD completes (`git commit`, `git checkout`,
`git rebase`, …) the executor signals the prompt. In v1 the
heuristic is "the command was `git` and exited 0"; PLAN_06 may
refine this with parser awareness.

Filesystem watchers (`inotify`/`kqueue`) for git index changes
are deferred. The current invalidation rule means the prompt
shows stale git status if the repo is changed by another
process; users can hit Enter on an empty line to force
re-evaluation. This trade is intentional: filesystem watchers
are stateful and platform-divergent.

## 10. Performance

### 10.1. Budget

Per PLAN_02 §9:

- Prompt render < 10 ms median (sync segments only).
- Re-render in steady state (no segment changed) < 1 ms.
- Async segment dispatch overhead < 100 µs.

Within those:

- Each sync segment < 100 µs steady state.
- Cache hit < 10 µs.
- Format-string expansion < 200 µs.
- Conversion to `Vec<FrameRow>` < 200 µs.

### 10.2. Allocations

The hot path (re-render with no segment change) must produce
zero allocations. Cells are written into a reusable `Vec<FrameCell>`
buffer owned by `PromptState`. The frame returned to PLAN_13
is a view over that buffer (cloned only at the editor's frame
boundary, not on every keystroke).

This requires:

- Segment outputs cached as `Vec<FrameCell>` per segment, not
  re-rendered on every prompt unless invalidated.
- Format-string expansion produces references into segment
  output buffers, with copies only at the boundary.

The first-render path may allocate; the steady-state path may
not. Benchmarks (§13) enforce both.

### 10.3. Async overhead

Spawning a tokio task is ~1 µs. Polling a current-thread runtime
non-blocking is ~100 ns. These are well within the budget; the
real cost is the segment work itself, which is by definition
deferred off the prompt path.

### 10.4. Resize cost

A resize triggers full re-render of the prompt frame. Sync
segments recompute; async segments emit cached resolved values
(if any) at the new width. The cost is the sum of sync segment
costs, which the budget already covers.

If `Directory` truncation depends on width, resize re-runs its
truncation logic. This is included in the sync budget.

## 11. Implementation phasing

### 11.1. Phase 0 — Replace scaffold (week 1)

Replace `fredshell-prompt::render` (current scaffold returning a
`String` from nu-ansi-term) with a sync-only `PromptRenderer`
that returns `Vec<FrameRow>` for two segments: `Directory` and
`Character`. Removes the `nu-ansi-term` workspace dep. Wire
into the binary; PLAN_13's Phase 0 cooked-mode scaffold consumes
the new output.

### 11.2. Phase 1 — Sync segments (weeks 2–4)

All sync segments per §5.2. Configuration loader (§7) for the
sync subset. Format-string expansion. Per-segment caches. Right
prompt. PLAN_13 frame integration. Benchmarks against budget.

### 11.3. Phase 2 — Async runtime, GitStatus (weeks 5–8)

Tokio runtime in the binary; runtime handle threaded into
`PromptRenderer`. `GitStatus` segment as the first async
consumer. `poll_async` wired into PLAN_13's main loop via
eventfd. Cancellation and timeout. Drop-on-cwd-change.

### 11.4. Phase 3 — Transient, continuation, polish (weeks 9–12)

Transient prompt. Continuation prompt. Container detection.
Venv detection. Time / shell level. Starship-config
compatibility shim. Documentation of the full schema.

### 11.5. Phase 4 — AI hint segment (deferred to PLAN_14)

Optional. Lives behind a feature flag. Same async-segment
plumbing as `GitStatus`.

## 12. Considered and rejected

### 12.1. Pure threads + channels for slow segments

A simpler model: spawn an OS thread per slow segment, communicate
over `mpsc::channel`. No tokio dependency in the prompt crate.

Rejected for three reasons:

1. **Thread cost.** Spawning a thread is ~50 µs; spawning a
   tokio task is ~1 µs. For prompts that frequently invalidate
   slow segments (cwd-heavy navigation), thread spawn cost is
   measurable.
2. **Cancellation.** Cancelling a thread mid-syscall is not a
   thing in safe Rust. Tokio futures are dropped, and the
   underlying I/O is cancelled by the runtime where possible.
3. **Ecosystem alignment.** `gix` exposes async operations.
   HTTP clients (PLAN_14) are async-first. Standardizing on
   tokio for I/O-bound work in the binary keeps the surface
   uniform.

The cost of "tokio in the prompt crate" is a single dependency
on `tokio` with the `rt` feature (no `rt-multi-thread`, no
`net`, no `fs`). Compile time impact is bounded.

### 12.2. Starship-as-subprocess

A pragmatic fallback: shell out to `starship prompt` and capture
its ANSI output. Requires no fredshell prompt code.

Rejected. Starship's per-prompt cost is dominated by process
startup (~10 ms on Linux, more on macOS). That blows the budget
by itself. Re-renders on every keystroke (for live-updating
content) are infeasible. Caching starship's output works for
static prompts only; dynamic-status updates regress to "blink
out, blink in." Worse, starship cannot integrate with PLAN_13's
frame model — it produces ANSI strings that PLAN_13 would have
to parse to extract style. Cell-level integration is non-
negotiable for the diff-based redraw.

### 12.3. User-scriptable segments (Lua / shell / Wasm)

Tempting; users want to write `format_segment(name = "kube",
command = "kubectl config current-context")`.

Rejected. Per tenet 4: configuration is data, not code. User
scripts on the prompt path mean:

- Unbounded latency (a script can take any amount of time).
- Sandboxing concerns (we are evaluating user-supplied code on
  every prompt).
- Maintenance burden (a Lua VM or Wasm runtime in the binary).
- Bug surface (user scripts that produce malformed output break
  the frame).

The pragmatic substitute is the `Custom("name")` segment kind
with a typed implementation in fredshell source. Users who want
"my kube context" segment file an issue or PR. The set of
useful prompt segments is finite; we add them.

### 12.4. ANSI-string output instead of `Vec<FrameRow>`

The current scaffold returns `String`. PLAN_13's frame model is
cell-based.

Rejected (i.e., the scaffold is replaced in Phase 0). Returning
a string forces PLAN_13 to either re-parse the ANSI escapes (to
recover style for diff) or to treat the prompt as opaque (no
diff for prompt rows). Neither is acceptable.

Cell-based output costs the prompt crate one dependency on
PLAN_13's frame types. That dependency is via `fredshell-core`
(where the frame types live, per PLAN_13 §3), which the prompt
already depends on.

### 12.5. Filesystem watchers for git status

`notify` / `inotify` to invalidate `GitStatus` cache on index
changes from other processes.

Deferred to a later revision, not rejected outright. Reasons to
defer:

- Adds a stateful background subsystem that has to coexist with
  cwd changes and shell exit cleanly.
- Cross-platform behavior is divergent (kqueue on macOS, inotify
  on Linux, polling fallback).
- The user-visible benefit is small; the prompt updates on the
  next Enter regardless.

V1 ships without watchers; the trade is documented in §9.

## 13. Testing strategy

Per PLAN_05:

- **Unit tests** for: format-string expansion (a battery of
  templates × contexts → expected cells), style-string parser,
  per-segment rendering with mocked context, cache invalidation
  rules, async segment dispatch (mock runtime), transient and
  continuation rendering.
- **Property tests** for: format-string expansion never panics
  on arbitrary input; cache invalidation never produces stale
  data after a known-relevant signal.
- **Snapshot tests** for: every default segment in a few
  representative contexts (cwd inside repo, cwd outside repo,
  status 0 / 130, ssh / non-ssh, …); the full default prompt
  in 80-column and 200-column widths.
- **Integration tests** for: prompt + line-editor frame
  composition produces expected combined frame; resize
  triggers expected re-render; async segment lifecycle (issue,
  poll, complete, refresh).
- **Benchmarks** (Criterion) for: full sync render, re-render
  with no change (must be near-zero), per-segment render,
  async dispatch overhead, format-string expansion, resize.

Performance regression threshold from AGENTS.md (>15% triggers
justification) applies to every prompt benchmark.

## 14. Open questions

- **`gix` vs spawning `git`.** `gix` is pure Rust and avoids
  the subprocess cost; it is also less mature than `libgit2`
  bindings. For `GitBranch` (sync, cached) we use `gix` because
  startup cost matters. For `GitStatus` (async) we use `gix` if
  its status traversal is competitive; if not, we shell out to
  `git status --porcelain` from the async future. Decided at
  implementation time against benchmark data.
- **Time segment update cadence.** Every keystroke updates the
  time segment; in steady state typing produces visible time
  ticks. This is correct but may surprise users used to bash's
  one-update-per-prompt. Default behavior matches every-render;
  a config knob for "update on submit only" is open.
- **Right-prompt under wrap.** When the buffer wraps such that
  the cursor is on a visual row past row 0, where does the
  right prompt go? Options: stay on row 0 (visible only above
  the wrap), follow the cursor (re-render every wrap change),
  hide. Lean toward "stay on row 0" with hide-on-overlap.
  Decided when implementing PLAN_13's wrap integration.
- **Transient prompt and history search.** When the user enters
  Ctrl-R history search, does the prompt go transient? Probably
  yes (the search overlay obscures the prompt anyway). Decided
  with PLAN_13 §8.5.
- **Starship-config fidelity.** How close to bug-for-bug? V1
  supports the schema for the listed modules. Edge cases of
  starship's format string (recursion, escape sequences) are
  matched best-effort; users with deep starship configs may
  see divergence. We document this rather than chasing parity.
- **AI hint placement.** The AI hint segment (PLAN_14) wants
  to render under the prompt as a separate row, not within the
  prompt row. The frame model accommodates this (the prompt
  emits multiple `FrameRow`s); the config schema needs a
  decision on whether AI hint is a segment or a separate row.
  Lean toward separate row, keyed by `[fredshell.ai_hint]`.

## 15. Relationship to other plans

- **PLAN_02** commits the architecture: tokio runtime in the
  binary, sync core, prompt is the canonical async-at-the-edge
  example. PLAN_14 implements that commitment.
- **PLAN_03** provides `Sgr` and the encoders the renderer's
  output eventually flows through (via PLAN_13's diff layer).
- **PLAN_04** owns the terminal width fed into `PromptContext`,
  the SIGWINCH path that triggers re-render, and the self-pipe
  that delivers async-progress wakeups to the main loop.
- **PLAN_13** owns the frame model the prompt emits into, the
  redraw mechanics, the wrap context that consumes
  `first_line_indent`, and the main-loop integration that calls
  `poll_async` between keystrokes.
- **PLAN_06** (completion) shares the tokio runtime if it
  elects async. Independent decision.
- **PLAN_12** (config) owns the loader; PLAN_14 contributes the
  prompt-specific schema.
- **PLAN_14** (AI) contributes an optional segment. Same async
  plumbing.
