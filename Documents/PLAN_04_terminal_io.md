# PLAN_04 — Terminal I/O, Signals, and Capability Detection

> Last updated: 2026-05-20 — added §1 slave-side clarification before implementation.
> Phase: A. Status: draft.
> Consumes: PLAN_02 §5, §6.1. Consumed by: PLAN_07 (line editor),
> PLAN_08 (prompt), PLAN_03 (capability boundary).

This document specifies the layer that sits between the kernel's
terminal interface and the rest of fredshell. It owns: raw mode
discipline, signal handling, process-group / job-control plumbing,
terminal feature detection, and the capability struct that callers
of `fredshell-ansi` use to decide which sequences are safe to send.

PLAN_03 deliberately punted on "is it safe to emit this sequence?"
PLAN_04 answers that question, and is the only subsystem that talks
directly to the kernel about the controlling terminal.

## 1. Scope and non-scope

### Slave-side, not master-side

fredshell is a shell, not a terminal emulator. The terminal emulator
(kitty, WezTerm, alacritty, Ghostty, iTerm2, freminal, tmux, sshd, …)
creates the pseudo-terminal pair and passes the slave side to us
through fds 0/1/2, having already set it as our controlling terminal
via `setsid` + `TIOCSCTTY`. **PLAN_04 sits on the slave side.** It
does **not** create PTYs. The reason it opens `/dev/tty` is to obtain
a reliable handle to the controlling terminal regardless of what
fds 0/1/2 actually point at — necessary because any of those may be
redirected to a file or pipe (`fredshell < script.sh`,
`ls | fredshell`, etc.) while the shell still needs to talk to the
user's terminal for prompts, keystrokes, and termios queries. This
is the standard practice followed by bash, zsh, and fish.

If a future feature ever needs to create a PTY (e.g. a `script`-style
session recorder, AI-assisted command playback), that capability gets
its own crate (`fredshell-pty`) introduced by the plan that needs it.
PLAN_04 does not anticipate this and does not pull in
`portable-pty`-style master-side machinery.

### In scope (v1)

- **Raw mode lifecycle.** Enter/restore termios, with crash-safe
  restoration on panic or signal-driven exit.
- **Controlling-terminal acquisition.** Open `/dev/tty` (not stdin),
  detect non-tty stdin/stdout, decide interactive vs. script mode.
- **Signal handling.** Install handlers for SIGINT, SIGTSTP, SIGTTOU,
  SIGTTIN, SIGCHLD, SIGWINCH, SIGALRM, SIGPIPE, SIGHUP, SIGQUIT, and
  SIGTERM, with the policy described in §4.
- **Process-group plumbing.** `setpgid` for spawned children,
  `tcsetpgrp` for foreground transfer, with the SIGTTOU dance
  needed to avoid stopping the shell.
- **Multiplexed wait.** `pselect`/`poll` over the tty fd and a
  self-pipe (or `signalfd` on Linux), per PLAN_02 §6.1.2.
- **Capability detection.** A query-and-cache step run once at
  startup that answers: truecolor? kitty keyboard? OSC 52? OSC 8?
  bracketed paste? focus reporting? synchronized output? Resulting
  `Capabilities` struct is consumed by every subsystem that calls
  into `fredshell-ansi`.
- **Window size tracking.** `TIOCGWINSZ` at startup, refresh on
  SIGWINCH, broadcast to subscribers (line editor, prompt).
- **Cooperative cancellation surface.** Owns the `AtomicBool` flag
  set by SIGINT/SIGALRM handlers (per PLAN_02 §6.1.3).

### Out of scope (v1)

- **Encoding ANSI sequences.** PLAN_03 owns that. PLAN_04 emits
  raw bytes only for terminal probes (see §6).
- **Key decoding.** Translating decoded CSI/SS3 sequences into
  semantic `KeyEvent`s belongs to PLAN_07.
- **Prompt rendering.** PLAN_08.
- **Pipeline fd setup.** That belongs to `fredshell-core::exec`;
  PLAN_04 only provides the signal/wait primitives it needs.
- **Mouse input.** Deferred. If/when the line editor enables it,
  decoding lives in PLAN_07; PLAN_04 only flips the DECSET bits.
- **Terminfo / termcap.** fredshell ships hard-coded sequences
  (per PLAN_03); capability decisions come from runtime probes,
  not from a terminfo database.

The boundary rule: `PLAN_04` owns _when_ it is safe to speak which
dialect, and _how_ to listen to the kernel. `PLAN_03` owns _what_
the dialect looks like on the wire. `PLAN_07` owns _meaning_ once
bytes are decoded into key events.

## 2. Design tenets

1. **Synchronous, signal-correct.** No async runtime. The
   primitives are `pselect`, `poll`, `sigaction`, `setpgid`,
   `tcsetpgrp`. PLAN_02 §6 settled this.
2. **One owner per piece of global terminal state.** Termios state,
   the foreground process group, and the signal mask are all
   exactly-once resources. PLAN_04 owns them. Other subsystems
   request transitions through a typed API.
3. **Crash-safe restoration.** Raw mode is restored on every exit
   path, including panic, SIGTERM, and `exit` in a child that
   somehow re-enters parent code. RAII + a `libc::atexit`-equivalent
   guard, see §3.3.
4. **No silent capability fallback.** If a capability probe fails
   or times out, the result is `Capabilities { kitty_keyboard:
false, ... }`, not an error. Callers see a typed `bool` and
   choose. PLAN_03 sequences are then simply not emitted; nothing
   degrades silently inside `fredshell-ansi`.
5. **Probes are bounded.** Capability detection has a hard time
   budget (§5.4). If the terminal does not answer in time, we
   assume the conservative answer and move on. Startup latency
   budget (PLAN_02 §6) is non-negotiable.
6. **Tested without a TTY where possible.** The pure logic (state
   machine for the SIGTTOU dance, capability parser, signal-mask
   composition) is unit-tested. The PTY-driven behavior lives
   behind a small trait so a fake PTY can drive tests (§9).

## 3. Crate placement and module layout

PLAN_04's code lives in `fredshell-core` for v1. A future split
into `fredshell-tty` is possible if the line editor and prompt
both grow direct dependencies on it; for now, keeping it inside
`fredshell-core` avoids a premature crate boundary.

```text
crates/fredshell-core/src/
  tty/
    mod.rs              — public surface: TerminalSession
    termios.rs          — raw mode RAII guard
    controlling.rs      — /dev/tty acquisition, isatty checks
    pgrp.rs             — setpgid / tcsetpgrp helpers
    signal.rs           — sigaction installation, self-pipe / signalfd
    wait.rs             — pselect/poll multiplexer
    winsize.rs          — TIOCGWINSZ + SIGWINCH broadcast
    capabilities.rs     — probe orchestration + Capabilities struct
    probe/
      truecolor.rs
      kitty_keyboard.rs
      osc.rs            — OSC 8 / OSC 52 probes
      synchronized.rs   — DECSET 2026
      bracketed_paste.rs
```

The public surface is small: `TerminalSession`, `Capabilities`,
`WindowSize`, `CancellationToken`, and a handful of error enums.

### 3.1. `TerminalSession`

```rust
pub struct TerminalSession {
    /// `/dev/tty` opened read/write.
    tty: OwnedFd,
    /// RAII guard restoring termios on drop.
    raw_guard: Option<RawModeGuard>,
    /// Current window size, refreshed on SIGWINCH.
    winsize: WindowSize,
    /// Cached capabilities from the startup probe.
    caps: Capabilities,
    /// Cancellation flag set by SIGINT/SIGALRM handlers.
    cancel: Arc<AtomicBool>,
    /// Self-pipe read end, multiplexed alongside the tty in pselect.
    sig_rx: OwnedFd,
}
```

Construction is fallible (`TerminalSession::open() -> Result<Self,
OpenError>`). `open()` does, in order: acquire `/dev/tty`, install
signal handlers, run the capability probe (bounded), read initial
winsize, but **does not** enter raw mode. Raw mode is a separate
transition (`session.enter_raw_mode()`) because script mode never
enters it.

### 3.2. Error types

```rust
pub enum OpenError {
    NoControllingTerminal,
    OpenTty(io::Error),
    SignalSetup(io::Error),
    AlreadyOpen,
}

pub enum RawModeError {
    GetTermios(io::Error),
    SetTermios(io::Error),
    AlreadyRaw,
}
```

No `anyhow` (PLAN_04 is `fredshell-core`, library crate).

### 3.3. RAII for raw mode

```rust
struct RawModeGuard {
    tty_fd: RawFd,
    saved: libc::termios,
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        // tcsetattr is async-signal-safe; safe to call from Drop
        // even during unwinding.
        unsafe {
            libc::tcsetattr(self.tty_fd, libc::TCSAFLUSH, &self.saved);
        }
    }
}
```

Drop covers normal exit and panics. For signal-driven termination
(SIGTERM, SIGHUP), the signal handler sets the cancellation flag
and the main loop runs cleanup. For SIGKILL or genuine process
death, the kernel cannot help and neither can we — but the parent
process (e.g., the user's terminal emulator) restores its own
termios on PTY teardown.

## 4. Signal policy

This section concretizes PLAN_02 §6.1.1.

| Signal   | Interactive shell                                        | Script mode                                     | Notes                               |
| -------- | -------------------------------------------------------- | ----------------------------------------------- | ----------------------------------- |
| SIGINT   | Caught: set cancel flag, write `\n`, redraw prompt.      | Default. Aborts current command unless trapped. | Children get default action.        |
| SIGTSTP  | Ignored on the shell itself. Children get default.       | N/A in non-job-control mode.                    | Children stop via tty driver.       |
| SIGTTOU  | Ignored. Required for `tcsetpgrp` to not stop the shell. | Ignored.                                        | See §4.1.                           |
| SIGTTIN  | Ignored. Same reason.                                    | Ignored.                                        |                                     |
| SIGCHLD  | Caught: write byte to self-pipe; main loop reaps.        | Same.                                           | No `WNOHANG` busy-loop.             |
| SIGWINCH | Caught: refresh winsize, broadcast.                      | Caught: refresh winsize (some scripts care).    |                                     |
| SIGALRM  | Caught: set cancel flag (used by `read -t`, `timeout`).  | Same.                                           | Per PLAN_02 §6.1.4.                 |
| SIGPIPE  | Default (terminate). Children inherit default.           | Default.                                        | Builtins handle EPIPE explicitly.   |
| SIGHUP   | Caught: send SIGHUP to all jobs, exit cleanly.           | Default.                                        | Optional `nohup`-style suppression. |
| SIGQUIT  | Ignored on the shell itself. Children get default.       | Ignored on the shell.                           | Avoids core-dumping the shell.      |
| SIGTERM  | Caught: clean shutdown, restore termios, exit.           | Default.                                        |                                     |

### 4.1. The SIGTTOU dance

When the shell calls `tcsetpgrp(tty, child_pgid)` to give a pipeline
foreground access to the terminal, the kernel would normally
deliver SIGTTOU to the shell (which is now a background-process-
group writer to the controlling terminal). The standard remedy is
to install SIGTTOU as `SIG_IGN` for the duration of the call.
PLAN_04 always has SIGTTOU and SIGTTIN ignored on the shell process
once interactive mode is entered; this is set once at startup and
never changed, so there is no transient window.

### 4.2. Self-pipe vs `signalfd`

The portable mechanism is the self-pipe trick: the signal handler
writes one byte to a pipe whose read end is included in the
`pselect` mask. On Linux, `signalfd` is slightly cleaner but
non-portable. v1 uses the self-pipe everywhere. A future Linux-
specific optimization may switch to `signalfd`; the public API
does not need to change.

Signal handlers do exactly two things:

1. For SIGINT/SIGALRM: atomically set the cancel flag.
2. For all caught signals: write one byte (`signal_number as u8`)
   to the self-pipe.

That is the entire handler. All real work happens in the main
loop after `pselect` returns.

### 4.3. Cancellation token

`Arc<AtomicBool>` shared between the signal handler and any
in-process work that wants to cooperate. Public API:

```rust
#[derive(Clone)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn is_cancelled(&self) -> bool { /* Relaxed load */ }
    pub fn reset(&self) { /* called by main loop after handling */ }
}
```

`reset` is called by the REPL after it has processed a SIGINT
(written newline, redrawn prompt). Builtins that ran during the
SIGINT see the flag set and return early; the REPL then clears it
before the next prompt.

## 5. Capability detection

The capability probe runs once, immediately after signal setup,
before raw mode is entered. It writes a small batch of query
sequences to the controlling terminal, reads responses with a
bounded timeout, and produces a `Capabilities` struct.

### 5.1. `Capabilities`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub color: ColorSupport,
    pub kitty_keyboard: bool,
    pub bracketed_paste: bool,
    pub focus_reporting: bool,
    pub synchronized_output: bool,
    pub osc8_hyperlinks: Osc8Support,
    pub osc52_clipboard: bool,
    pub osc133_semantic_prompt: bool,
    pub osc7_cwd: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSupport {
    None,           // dumb terminal
    Ansi16,
    Ansi256,
    Truecolor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Osc8Support {
    Unknown,        // probe inconclusive; conservative emitters skip
    Supported,
    Unsupported,
}
```

`Osc8Support` has a three-valued logic because OSC 8 has no reliable
probe response on most terminals; we infer it from `$TERM_PROGRAM`
and `$COLORTERM` rather than a query. Other capabilities are
boolean: either we got the expected response, or we did not.

### 5.2. Signal sources for capabilities

Capability information comes from three sources, in this order:

1. **Environment variables** (synchronous, free). `$COLORTERM`,
   `$TERM_PROGRAM`, `$TERM`, `$KITTY_WINDOW_ID`, `$WT_SESSION`,
   `$WEZTERM_EXECUTABLE`, `$ITERM_SESSION_ID`.
2. **Active probes** (write query, read response, with timeout).
   DA1, kitty keyboard query, DECRQM for synchronized output.
3. **Conservative defaults** if both fail.

The probe phase is skipped entirely if stdin is not a tty (script
mode) or if `$FREDSHELL_NO_PROBE=1` is set (escape hatch for
debugging or unusual hosts).

### 5.3. Probe batch

The probes are written as a single batched `write(2)`:

```text
\x1b[c              DA1 — primary device attributes
\x1b[?u             Kitty keyboard query
\x1b[?2026$p        DECRQM synchronized output
```

Then a single `read(2)` loop drains responses until either all
expected response shapes have been seen or the timeout fires. The
decoder is the small set of shapes in PLAN_03 §6 (the decoder side
of `fredshell-ansi`).

### 5.4. Timeout

Total budget: **50 ms**. On a local terminal, responses arrive in
well under 1 ms; 50 ms is dominated by SSH round-trip in the
pathological case. Anything slower is treated as no-response.
This budget is part of the startup latency budget in PLAN_02 §6;
the rest of startup (parser, builtin registry, prompt warmup) must
fit in the remaining ≤30 ms of the 50 ms cold-start target. Failure
to meet either half is a release blocker.

### 5.5. Caching

`Capabilities` is computed once per session and stored in
`TerminalSession`. SIGWINCH does not invalidate it (resizing a
terminal does not change its capabilities). Re-probing is possible
but not exposed in v1.

## 6. Window size

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSize {
    pub rows: u16,
    pub cols: u16,
    pub pixel_width: u16,   // 0 if unknown
    pub pixel_height: u16,  // 0 if unknown
}
```

Populated from `TIOCGWINSZ` at startup and on each SIGWINCH.
Subscribers (line editor, prompt) get a snapshot, not a live
reference; subscribers re-read after handling the SIGWINCH wakeup
delivered through the self-pipe.

A pixel-dimension probe (XTWINOPS 14) is **not** run by default;
it is too rarely useful and adds latency. A future image-protocol
feature may opt in.

## 7. Process-group plumbing

Job control needs two operations:

1. **Place children in their own process group.** `setpgid(child,
child)` runs in both parent and child after `fork`, so the
   transition is race-free regardless of scheduling.
2. **Transfer terminal foreground.** `tcsetpgrp(tty, pgid)` moves
   the controlling-terminal foreground process group. SIGTTOU is
   ignored on the shell, so this is a single syscall.

```rust
impl TerminalSession {
    pub fn give_foreground(&self, pgid: Pid) -> io::Result<()>;
    pub fn take_foreground(&self) -> io::Result<()>;
}
```

`take_foreground` sets the foreground back to the shell's own
process group after a foreground job finishes or stops. This is
called from the REPL loop, not from the signal handler.

The full job-control state machine (suspended jobs list, `fg` /
`bg` / `wait` / `jobs` builtins) is **not** PLAN_04. PLAN_04
provides the syscall primitives; the state machine is part of
`fredshell-core::exec` and gets its own document (PLAN_06).

## 8. Public API summary

```rust
// Top-level: open a session.
pub fn open() -> Result<TerminalSession, OpenError>;

impl TerminalSession {
    pub fn capabilities(&self) -> Capabilities;
    pub fn window_size(&self) -> WindowSize;
    pub fn cancellation_token(&self) -> CancellationToken;

    pub fn enter_raw_mode(&mut self) -> Result<(), RawModeError>;
    pub fn leave_raw_mode(&mut self);

    pub fn give_foreground(&self, pgid: Pid) -> io::Result<()>;
    pub fn take_foreground(&self) -> io::Result<()>;

    /// Block until one of: input available on the tty, a signal
    /// was delivered, or `deadline` elapses. Returns which of the
    /// three woke us. Builtins like `read -t` call this directly.
    pub fn wait(&self, deadline: Option<Duration>) -> WaitEvent;

    /// Borrow the tty for reading. The returned reader respects
    /// the current termios; raw mode must already be entered for
    /// keystroke-by-keystroke reads.
    pub fn input(&self) -> TtyInput<'_>;

    /// Borrow the tty for writing. Used by the prompt and line
    /// editor; PLAN_03 sequences are written through this handle.
    pub fn output(&self) -> TtyOutput<'_>;
}

pub enum WaitEvent {
    Input,            // tty fd is readable
    Signal(Signal),   // one or more signals were delivered
    Timeout,
}
```

`Signal` is a small enum of the signals PLAN_04 catches; not every
libc signal needs a variant.

## 9. Testing strategy

Per PLAN_05, the testing strategy is:

- **Unit-tested without a TTY:**
  - Capability response parsing (feed bytes, expect struct).
  - Signal-mask composition (the set of signals blocked during
    `pselect` is derived from a config struct; that derivation is
    pure).
  - The conservative-defaults logic when probes time out.
  - Env-var heuristics (given a `HashMap<&str, &str>`, produce a
    partial `Capabilities`).

- **Integration-tested with a fake PTY:**
  - A small helper opens a pty pair and drives `TerminalSession`
    against the slave fd. The test process writes responses to the
    master fd to simulate a terminal.
  - Covers: probe batching, timeout, raw-mode round-trip,
    SIGWINCH delivery, self-pipe wakeup, `give_foreground` /
    `take_foreground` with a real child.

- **Integration-tested against real terminals:**
  - A `cargo xtask tty-probe` command opens `/dev/tty`, runs the
    probe, and prints the detected `Capabilities`. This is not
    a CI test; it is a developer tool for verifying against
    terminals we cannot script (kitty, WezTerm, alacritty,
    Ghostty, iTerm2, Apple Terminal, gnome-terminal, konsole,
    foot, Windows Terminal via WSL).
  - Results are tabulated in a developer-facing matrix; this is
    how regressions in capability detection get caught between
    releases.

- **Property tests** for the response decoder: random byte streams
  do not panic; valid responses round-trip through encode/decode
  where applicable (DA1, DSR, kitty query).

A `MockTerminal` trait will be considered if the fake-PTY harness
proves too slow or too coupled to Linux pty semantics. For now,
the assumption is that fake PTYs are good enough on Linux and
macOS — the two supported platforms — and that we accept the
coupling.

## 10. Performance contract

PLAN_04 is not on the keystroke hot path. The hot path is:

1. `pselect` returns "tty readable".
2. `read(tty)` into a small fixed buffer.
3. Hand bytes to PLAN_07's key decoder.
4. PLAN_07 produces a `KeyEvent`.
5. Line editor mutates buffer, calls PLAN_03 encoders to redraw.

PLAN_04's contribution is steps 1 and 2: one syscall to wait, one
syscall to read. Neither allocates. The keystroke latency budget
(PLAN_02 §6: <1 ms median) is spent in steps 3–5, not in PLAN_04.

Startup contribution:

- Signal handler installation: a handful of `sigaction` calls,
  well under 1 ms.
- Capability probe: up to 50 ms wall, but typically <5 ms.
- Initial `TIOCGWINSZ`: one syscall, microseconds.

The 50 ms cold-start budget assumes capability probing on the
critical path. If that becomes a problem, the probe can be moved
off the critical path: the REPL starts with conservative defaults
and the probe runs concurrently with the first prompt render,
updating capabilities before any sequence that depends on them
is emitted. This is an optimization, not v1.

## 11. Migration and rollout

There is no existing code to migrate; PLAN_04 is greenfield. The
rollout sequence is:

1. Land `TerminalSession::open()` with signal setup and `/dev/tty`
   acquisition. No raw mode, no probes. Unit-tested.
2. Add capability probe + response decoder. Fake-PTY integration
   tests.
3. Add raw-mode RAII guard and `enter/leave_raw_mode`.
4. Add `wait()` multiplexer and self-pipe.
5. Add process-group helpers (`give/take_foreground`).
6. Wire into the REPL: replace the current "read line from stdin"
   stub with a real `TerminalSession`-driven loop. PLAN_07 then
   builds on top.

Each step is independently testable and committable.

## 12. Open questions

These are recorded so the next pass of the document settles them
rather than rediscovering them.

- **signalfd on Linux.** Worth a feature flag at v1, or strictly
  v2? Leaning v2 — the self-pipe is portable and fast enough.
- **`SIGUSR1` / `SIGUSR2`.** Reserved for user `trap` builtins, or
  caught by the shell for internal use? Bash leaves them to the
  user; we will too, but trap support is a separate concern
  (PLAN_06).
- **Probe ordering with `nohup`-style invocations.** If stdin is a
  pty but stdout/stderr are pipes, do we still probe? Current
  answer: probe only if `/dev/tty` is openable; this handles the
  common cases.
- **Per-platform termios flags.** macOS and Linux differ slightly
  on which flags are needed for "raw enough." A small platform
  module abstracts the difference; details settled in
  implementation, not in this plan.
- **Restoring termios on `exec`.** When a builtin `exec`s a child
  that replaces the shell process, the termios state at exec time
  is what the child inherits. Bash restores cooked mode before
  exec; we will do the same. Implementation detail; noted here so
  the RAII guard's `Drop` does not also fire (it does, harmlessly,
  but the explicit restoration is what the child relies on).

## 13. Relationship to other plans

- **PLAN_02** §5 enumerated the responsibilities; §6.1 picked the
  primitives. PLAN_04 implements them.
- **PLAN_03** defines the byte-level sequences PLAN*04's probe
  writes and reads. The probe is the first consumer of PLAN_03's
  encoder \_and* decoder.
- **PLAN_05** specifies the fake-PTY harness used by PLAN_04's
  integration tests.
- **PLAN_06** (exec / job control) consumes
  `give_foreground` / `take_foreground` and the `CancellationToken`.
- **PLAN_07** (line editor) consumes `wait()` and the raw-mode
  transition, and decodes the bytes PLAN_04's `input()` returns.
- **PLAN_08** (prompt) consumes `Capabilities` to decide which
  PLAN_03 sequences are safe.
