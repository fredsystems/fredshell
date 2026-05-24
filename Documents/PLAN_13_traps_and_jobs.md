# PLAN_13 — Traps, Signal Disposition, and Job Control

> Last updated: 2026-05-24 — cascade renumber to insert PLAN_10
> embedding (ADR 0006): functional metadata "Consumes/Consumed by"
> PLAN_11 → PLAN_12 (Phase B). Body refs to `PLAN_19_coproc.md`
> updated to `PLAN_20_coproc.md`. References block file paths
> remapped. Subtask IDs (`12.N`) preserved per stable-subtask-ID
> rule. Substance unchanged.
>
> Previously (2026-05-24): cross-refs remapped for work-order
> renumber: PLAN_06 Phase B → PLAN_11; PLAN_16_coproc → PLAN_19_coproc;
> PLAN_14 (AI) → PLAN_17; PLAN_15 milestones → PLAN_18; PLAN_07
> (line editor) → PLAN_13; PLAN_08 (spec drafting) → PLAN_07;
> PLAN_09 (fuzzer) → PLAN_08. Substance unchanged.
>
> Previously (2026-05-23): §5.1 dispatch-asymmetry note added
> (resolves Q-10-B); §6 notification-dispatch subsection added
> routing through PLAN_13 `yield_terminal` (resolves Q-10-C);
> Q10.3 / Q-10-D resolved by `PLAN_19_coproc.md` stub.
> Phase: B. Status: stub (drafted; implementation pending).
> Consumes: PLAN_02 §4, §5, §6.1; PLAN_04 §4, §7; PLAN_06 Phase A §2;
> PLAN_12 (Phase B). Consumed by: PLAN_12 (gates removal of the
> `/bin/sh -c` fallback for any script that uses `trap`, `wait`, or
> `&`); PLAN_05 (owns the spec-corpus rows previously assigned to
> PLAN_06 for `bg`, `fg`, `jobs`, `kill`, `trap`, `wait`, `disown`,
> `suspend`).

This document specifies the part of fredshell that turns a stream of
parsed commands into something a real bash user expects: backgrounded
jobs, foreground/background transfer, signal disposition, and the
`trap` machinery that ties them together. It owns the data model
(the job table), the control plane (the `trap` builtin family and
signal-dispatch loop), and the boundary contract between
`fredshell-core::exec` and `fredshell-core::tty` (PLAN_04).

PLAN*04 already provides the primitives: `setpgid`, `tcsetpgrp`,
`pselect`/`signalfd`, signal masking, the cooperative-cancellation
flag, and SIGCHLD wait-loop scaffolding. **PLAN_13 is the policy
layer.** It decides \_when* a child is given the controlling
terminal, _what_ happens when SIGINT arrives during a script,
_how_ `trap '...' EXIT` interacts with a non-zero exit, and _which_
of the eight job-control builtins are exposed.

PLAN_13 is Phase B because it cannot be designed correctly without
the spec corpus telling us which bash quirks real-world scripts
depend on. The harness shipped by PLAN_05 already enumerates the
gap: `tests/spec/job_control/background_wait.case.toml`,
`tests/spec/traps_and_signals/trap_exit.case.toml`, and the
deferred rows in PLAN_05 §11 are PLAN_13's executable
acceptance criteria.

## 1. Scope and non-scope

### In scope (v1)

- **Job table.** A `JobTable` struct hung off `ShellState`, holding
  zero or more `Job` records keyed by job spec (`%n`, `%+`, `%-`,
  `%string`, `%?string`, PID). Lifecycle, transitions, and
  serialisation for `jobs -l` / `jobs -p`.
- **Foreground transfer.** Giving the controlling terminal to a
  child process group via the `tcsetpgrp` dance described in
  PLAN_04 §7, with SIGTTOU masked. Reclaiming on child stop or
  exit.
- **Background launch.** Spawning a pipeline with `&`, registering
  it in the job table, _not_ transferring the terminal.
- **Signal disposition.**
  - The `trap` builtin and its three forms (`trap '...' SIG`,
    `trap - SIG`, `trap` / `trap -p`).
  - Pseudo-signals: `EXIT`, `ERR`, `DEBUG`, `RETURN`.
  - Signal inheritance across functions, subshells, and command
    substitutions (the bash rules, not the POSIX defaults).
- **The eight job-control builtins.** `bg`, `fg`, `jobs`, `kill`,
  `wait`, `disown`, `suspend`, and the `trap` family. Plus
  `set -m` / `set +m` (monitor mode) and `set -b` (immediate
  notification) which gate behaviour here.
- **Job-spec parser.** Shared parsing of `%n`, `%%`, `%+`, `%-`,
  `%string`, `%?string` and bare PIDs, used by every job-control
  builtin that takes a job argument.
- **Status reporting.** The "[1]+ Done …" / "[1]+ Stopped …" /
  "[1]+ Terminated …" lines emitted to stderr at the next prompt
  in monitor mode, and at job exit in `set -b` mode.
- **`$!` and `$?` interaction.** `$!` set to the PID of the most
  recent background job; `$?` set by `wait` when given a job spec.

### Out of scope (v1)

- **The signal primitives themselves.** Installing `sigaction`
  handlers, reading from `signalfd` / the self-pipe, masking and
  unmasking — all owned by PLAN*04 §4. PLAN_13 \_subscribes* to
  PLAN_04's signal delivery, it does not duplicate it.
- **`fc` and `history`.** Owned by PLAN_14 (line editor / history
  subsystem). PLAN_13 has no opinion on command history.
- **Coprocesses (`coproc`).** A grammar-level construct with its own
  bidirectional-pipe and PID-management semantics. Deferred to a
  later phase; owned by `PLAN_20_coproc.md` (stub).
- **Loadable bash builtins.** `enable -f` for dynamically loaded
  builtins is a Tier-2-via-`dlopen` problem that is out of scope
  for v1 entirely.
- **Process accounting.** `times` is a Tier-1 builtin owned by
  PLAN_12 (it reads `times(2)` and prints; no policy decisions).
  PLAN_13 does not touch it.
- **POSIX-only job control mode.** Bash has a `--posix` mode that
  alters some trap and job semantics (most notably the disposition
  of `trap` in subshells). v1 implements the default-bash semantics
  only; `--posix` differences are tracked but not gated.

The boundary rule, complementary to PLAN_04 §1's: **PLAN_04 owns the
syscalls; PLAN_13 owns the policy.** A SIGCHLD arrives at PLAN_04's
self-pipe; PLAN_04 calls `waitpid(-1, …, WNOHANG | WUNTRACED)` in a
loop and hands a `Vec<ChildStatusChange>` to PLAN_13's reaper;
PLAN_13 updates the job table, runs any installed `trap` handlers,
and decides whether to print a status line. PLAN_04 never reads or
writes the job table.

## 2. Design tenets

1. **Bash semantics by default.** Real-world scripts depend on bash
   quirks (e.g., `trap` in a subshell, inherited disposition of
   `SIGINT` during command substitution). Where bash and POSIX
   disagree, we follow bash. The differential harness (PLAN_08) is
   the arbiter when prose is ambiguous.

2. **Synchronous reaping.** Job-table updates happen in the main
   REPL thread, not in a signal handler. The signal handler does
   exactly two things: set a flag (`PLAN_04` already owns this) and
   write a byte to the self-pipe. The main loop wakes, drains
   `waitpid`, and applies status changes atomically.

3. **No `Arc<Mutex<JobTable>>`.** The job table lives on
   `ShellState`, which lives on `ExecEnv`, which is single-threaded
   by construction (PLAN_02 §4.2). If a future feature requires
   cross-thread access it gets a separate plan; PLAN_13 does not
   pre-pay that cost.

4. **Traps are not handlers.** A user's `trap '...' SIGINT` does
   _not_ install a `sigaction`. It registers a disposition record
   in `ShellState::traps` and asks PLAN*04 to keep the signal
   \_undefaulted*. The script body, between commands, polls the
   cancellation flag and runs registered trap bodies inline. This
   is the only safe way to execute arbitrary shell code in response
   to a signal.

5. **Pseudo-signals are first-class.** `EXIT`, `ERR`, `DEBUG`, and
   `RETURN` are stored in the same trap table as real signals,
   keyed by an enum that has variants for both. Their dispatch
   sites are different (EXIT at shell-exit, ERR after each command
   with non-zero status when `set -E` is in effect, DEBUG before
   each simple command when `set -T` is in effect, RETURN at
   function return), but their registration and inspection share
   one code path.

6. **Job-spec parsing is one function.** Every builtin that takes a
   job argument (`fg %1`, `bg %1`, `kill %1`, `wait %1`, `disown
%1`) calls the same `parse_job_spec(&str, &JobTable) ->
Result<JobRef, JobSpecError>`. Duplicating this is a category of
   bug bash itself has had.

7. **Status reporting is observable.** The "[1]+ Done" lines are
   not log output; they are part of the contract with the user and
   are testable. They are written through `ExecEnv::stderr`, never
   directly to fd 2.

8. **Monitor mode is the default for interactive shells, off for
   scripts.** Same as bash. `set -m` flips it; PLAN_13 reads
   `ShellState::opts.monitor` on every relevant decision.

## 3. Crate placement and module layout

PLAN_13's code lives in `crates/fredshell-core/src/`. No new crate
is introduced. The module layout:

```text
crates/fredshell-core/src/
├── exec/
│   ├── job/
│   │   ├── mod.rs          // JobTable, Job, JobState, JobRef
│   │   ├── spec.rs         // parse_job_spec, JobSpec, JobSpecError
│   │   ├── reaper.rs       // apply_child_status_change, post-reap hook
│   │   ├── notify.rs       // status-line rendering, set -b vs. monitor
│   │   └── tests.rs
│   ├── trap/
│   │   ├── mod.rs          // TrapTable, TrapKind, TrapDisposition
│   │   ├── pseudo.rs       // EXIT / ERR / DEBUG / RETURN dispatch
│   │   ├── signals.rs      // mapping of signal names <-> libc::c_int
│   │   └── tests.rs
│   └── builtins/
│       ├── jobs_builtin.rs
│       ├── fg.rs
│       ├── bg.rs
│       ├── kill.rs
│       ├── wait.rs
│       ├── disown.rs
│       ├── suspend.rs
│       └── trap_builtin.rs
└── state.rs                // ShellState gains `jobs: JobTable`
                            // and `traps: TrapTable`
```

The `_builtin` suffix on `jobs_builtin.rs` and `trap_builtin.rs`
disambiguates from the `jobs` and `trap` module/type names. Other
builtin files use the bare command name because there is no
collision.

`ShellState` is defined in PLAN_12 (Phase B); PLAN_13 adds two fields.
The struct layout for those fields is fixed by this document:

```rust
pub struct ShellState {
    // ... fields from PLAN_12 (Phase B) ...
    pub jobs: JobTable,
    pub traps: TrapTable,
}
```

## 4. The job table

### 4.1. Data model

```rust
pub struct JobTable {
    next_id: JobId,
    jobs: BTreeMap<JobId, Job>,
    current: Option<JobId>,   // "%+" / "%%"
    previous: Option<JobId>,  // "%-"
    last_async_pid: Option<libc::pid_t>, // backs "$!"
}

pub struct JobId(NonZeroU32);

pub struct Job {
    pub id: JobId,
    pub pgid: libc::pid_t,
    pub processes: Vec<Process>,   // one per pipeline stage
    pub state: JobState,
    pub command: String,           // the original source text
    pub notified: bool,            // status already shown to user?
    pub started_at: Instant,
}

pub struct Process {
    pub pid: libc::pid_t,
    pub state: ProcessState,
    pub argv0: String,             // for "[1]+ Stopped grep foo"
}

pub enum JobState {
    Running,
    Stopped(libc::c_int),          // signal that stopped it
    Done(ExitStatus),              // exited cleanly
    Terminated(libc::c_int),       // killed by signal
}

pub enum ProcessState {
    Running,
    Stopped(libc::c_int),
    Exited(ExitStatus),
    Terminated(libc::c_int),
}
```

`JobId` starts at 1 and increases monotonically; bash reuses
slots only after a `wait` or `disown` removes the entry, and we
match this. `BTreeMap` keeps iteration in id order, which is the
order `jobs` prints.

`current` and `previous` follow bash's rules:

- A newly stopped job becomes `current`; the old `current` becomes
  `previous`.
- A newly started background job becomes `current` only if there
  is no other current job; otherwise `previous`.
- When a job is removed, `previous` is promoted to `current` and
  some other running/stopped job becomes `previous`.

These rules are encoded in `JobTable::on_state_change` and
verified by unit tests against a corpus of bash transcripts.

### 4.2. Lifecycle

The five state transitions:

| From              | To              | Trigger                              |
| ----------------- | --------------- | ------------------------------------ |
| (not present)     | Running         | `JobTable::insert_background`        |
| Running           | Stopped(sig)    | SIGCHLD with `WIFSTOPPED`            |
| Stopped(sig)      | Running         | `SIGCONT` from `bg`, `fg`, or `kill` |
| Running / Stopped | Done(status)    | SIGCHLD with `WIFEXITED`             |
| Running / Stopped | Terminated(sig) | SIGCHLD with `WIFSIGNALED`           |

A `Done` or `Terminated` job stays in the table until one of:

- The user runs `jobs` (which prints terminal-state jobs and then
  removes them — bash quirk).
- The user runs `wait %n` (which reports status and removes).
- A new prompt is issued in monitor mode (which prints
  notification and removes).
- The user runs `disown` (which removes without reporting).

The exact "remove on prompt" timing is encoded in
`JobTable::reap_completed(env) -> Vec<NotifyLine>` called once
per REPL iteration immediately before prompt rendering.

### 4.3. Public API

```rust
impl JobTable {
    pub fn new() -> Self;

    // Insert; called by the executor after spawning a pipeline.
    pub fn insert_background(
        &mut self,
        pgid: libc::pid_t,
        processes: Vec<Process>,
        command: String,
    ) -> JobId;

    pub fn insert_foreground(
        &mut self,
        pgid: libc::pid_t,
        processes: Vec<Process>,
        command: String,
    ) -> JobId;

    // Called by reaper.rs when SIGCHLD-driven waitpid returns.
    pub fn apply_status_change(
        &mut self,
        pid: libc::pid_t,
        status: WaitStatus,
    ) -> Option<JobStateChange>;

    // Called once per REPL iteration before prompt rendering.
    pub fn reap_completed(
        &mut self,
        opts: &ShellOpts,
    ) -> Vec<NotifyLine>;

    // Lookup.
    pub fn resolve(&self, spec: &JobSpec) -> Result<JobId, JobSpecError>;
    pub fn get(&self, id: JobId) -> Option<&Job>;
    pub fn current(&self) -> Option<JobId>;
    pub fn previous(&self) -> Option<JobId>;

    // Mutation by builtins.
    pub fn continue_in_background(&mut self, id: JobId) -> Result<(), JobError>;
    pub fn continue_in_foreground(&mut self, id: JobId) -> Result<(), JobError>;
    pub fn disown(&mut self, id: JobId) -> Result<(), JobError>;
    pub fn remove(&mut self, id: JobId) -> Option<Job>;

    // Iteration (for `jobs`).
    pub fn iter(&self) -> impl Iterator<Item = (JobId, &Job)>;
}
```

`JobError` is a small enum:

```rust
pub enum JobError {
    NoSuchJob(JobSpec),
    NoCurrentJob,
    NotStopped(JobId),
    NotJobControl,   // returned when set +m is in effect
    AlreadyDisowned(JobId),
}
```

Following AGENTS.md: no `unwrap`, no `expect`, no `anyhow`. All
error variants are testable.

### 4.4. Job-spec grammar

Per bash JOB SPECIFICATIONS:

```text
job_spec := '%' job_selector
job_selector := digits         // "%1"
              | '%'            // "%%"
              | '+'            // "%+"
              | '-'            // "%-"
              | string         // "%emacs" — prefix-match
              | '?' string     // "%?em" — substring-match
              | (empty)        // "%" alone — same as "%+"
```

Bare PIDs (no `%`) are also valid in the contexts that accept
them (`kill`, `wait`). `parse_job_spec` returns
`Result<JobRef, JobSpecError>` where `JobRef` is either a
`JobId` or a `Pid`. Builtins that only accept job specs (`fg`,
`bg`, `disown`) reject `JobRef::Pid` with a typed error.

`JobSpecError` distinguishes:

```rust
pub enum JobSpecError {
    Empty,
    InvalidPid(String),
    InvalidJobId(String),
    AmbiguousString { matched: Vec<JobId> },
    NoMatch(String),
    NoCurrentJob,
    NoPreviousJob,
}
```

bash uses three different error messages for these (`no such job`,
`ambiguous job spec`, `no current job`); fredshell matches them
verbatim because shell scripts grep for these strings.

## 5. Trap table and signal disposition

### 5.1. Data model

```rust
pub struct TrapTable {
    entries: HashMap<TrapKind, TrapDisposition>,
}

pub enum TrapKind {
    Signal(SignalSpec),  // any libc::SIG* we expose
    Exit,
    Err,
    Debug,
    Return,
}

pub enum TrapDisposition {
    // User-installed command string. Re-parsed at each invocation
    // (this matches bash; it lets traps read variables at fire time).
    Command(String),

    // `trap '' SIG`  — signal is ignored.
    Ignore,

    // `trap - SIG`   — restore default behaviour. The entry is
    // typically deleted rather than stored as Default, but we
    // keep the variant for `trap -p` reporting clarity.
    Default,
}

pub struct SignalSpec {
    pub number: libc::c_int,
    pub name: &'static str,  // canonical bash spelling, no "SIG" prefix
}
```

`SignalSpec` is interned: there is one `&'static str` per
recognised signal, sourced from a `const` table in
`trap/signals.rs`. We expose every signal in
`bash --version`'s list, plus the platform-specific RT signals
on Linux (`SIGRTMIN+0` through `SIGRTMAX`); RT signals are
recognised but treated as opaque integers.

**Dispatch asymmetry note.** Although `TrapKind` unifies real
signals and pseudo signals into one table for storage and for
the `trap` builtin's CLI symmetry, the dispatch sites differ:

- `TrapKind::Signal(_)` entries are fired from the async-signal
  path: the OS signal handler writes to the self-pipe (§5.4),
  the executor's read loop drains the pipe and looks up the
  `Signal(_)` key.
- `TrapKind::Exit | Err | Debug | Return` entries are fired
  from synchronous executor hooks (PLAN_12 §4) — there is no
  signal handler involved.

To prevent the two dispatch paths from confusing each other,
the self-pipe drain (§5.5) carries a `debug_assert!` that the
key it looks up is a `Signal(_)` variant, and the executor's
pseudo-trap hooks carry the inverse assertion. These are pure
debug checks; release builds rely on the call-site type
discipline.

### 5.2. Signal-name resolution

The `trap` builtin accepts signal names in four forms:

- Bare name without `SIG` prefix: `INT`, `TERM`, `USR1`.
- With `SIG` prefix: `SIGINT`, `SIGTERM`. Bash accepts this; POSIX
  does not require it; we accept it.
- Decimal number: `2`, `15`.
- Pseudo-signal names: `EXIT`, `ERR`, `DEBUG`, `RETURN`.

`signals::resolve(name: &str) -> Option<TrapKind>` performs
case-insensitive matching for bash compatibility (bash accepts
`trap '...' int`).

### 5.3. Pseudo-signal semantics

| Pseudo | Fires when                                   | Inherited by |
| ------ | -------------------------------------------- | ------------ |
| EXIT   | Shell exits (including via `exit` builtin)   | Subshells    |
| ERR    | Simple command exits non-zero, `set -E` on   | If `set -E`  |
| DEBUG  | Before each simple command, `set -T` on      | If `set -T`  |
| RETURN | Function/sourced-script returns, `set -T` on | If `set -T`  |

"Inherited by subshells" means a `(...)` group runs with the same
trap installed unless explicitly cleared. Functions inherit the
parent's traps for all real signals; they inherit DEBUG/RETURN
_only_ if `set -T` (`functrace`) is on, and ERR _only_ if
`set -E` (`errtrace`) is on. This is the bash rule and matters
for real-world scripts that use `set -eET`.

EXIT fires exactly once, even if the shell exits via a signal
that is itself trapped. The trap's exit status does not override
`$?` unless the trap itself runs `exit`.

ERR fires _after_ the command has set `$?`. Inside the ERR trap,
`$?` reads the non-zero status that triggered the trap.

DEBUG fires _before_ the command runs; its exit status can
suppress the command via `extdebug` mode (bash quirk). v1 does
not implement `extdebug` suppression; we record this as cleanup
item §15.1.

### 5.4. Dispatch loop integration

PLAN_04 §4 already runs a SIGCHLD/SIGINT/SIGTSTP/SIGWINCH handler
that writes to a self-pipe. The REPL main loop, between commands,
drains the self-pipe and calls:

```rust
fn process_pending_signals(env: &mut ExecEnv) -> ControlFlow<ExitStatus, ()>;
```

This function, owned by PLAN_13, does:

1. For each delivered real signal `sig`:
   - If `env.shell.traps.entries.contains(TrapKind::Signal(sig))`,
     run the trap body in the current shell context.
   - Otherwise apply the default disposition (which for SIGINT in
     an interactive shell is "abandon the current command line,
     redraw prompt"; PLAN_14 will own the redraw side).
2. If `env.shell.opts.errtrace && last_status != 0`, fire ERR.
3. If `env.shell.opts.functrace`, fire DEBUG (between simple
   commands) and RETURN (on function exit).
4. Return `ControlFlow::Break(status)` if the trap ran `exit`.

The function is _called from the executor's command loop_, not from
a signal handler. The signal handler's only job (per PLAN_04 §4)
is to wake the main loop.

### 5.5. Trap inheritance across subshells

When the executor forks for a `(...)` group, command substitution,
or pipeline stage:

- The child clears ERR and DEBUG and RETURN entries (bash default).
- The child clears all real-signal traps that were `Command(_)`,
  resetting them to default disposition. `Ignore` is inherited.
- EXIT is cleared in the child (it would fire when the subshell
  exits, but bash explicitly clears it to avoid double-firing).

This is implemented as `TrapTable::for_subshell(&self) -> TrapTable`
called in the executor's fork path.

## 6. The eight job-control builtins

Each builtin in this section is a Tier-1 builtin owned by PLAN_13.
Spec sheets for each (per PLAN_07) live under
`Documents/specs/builtins/`. This section gives the contract; the
sheets give the per-flag, per-edge-case behaviour.

### 6.0. Notification dispatch

All job-status notification lines emitted by this section — the
default `"[N]+ Done …"` / `"[N]+ Terminated …"` lines printed at
prompt time, and the immediate notifications printed under
`set -b` — flow through a single dispatch helper:

- **If a line-editor session is active** (interactive shell at
  the prompt, raw mode engaged), route the line through
  `editor.yield_terminal(|stdout| { … })`. PLAN_14 §9.5 owns the
  primitive and its three invariants (total state restoration,
  no keystrokes lost, SIGWINCH honoured during yield). PLAN_13
  never touches raw mode directly.
- **Otherwise** (non-interactive shell, or interactive shell
  with a foreground pipeline running and no prompt up), write
  the line directly to stderr. There is no editor state to
  corrupt.

The choice is made per-notification, not per-shell-mode: a
trap firing under `set -b` while a foreground command is
mid-run uses the direct-stderr path; the same trap firing
between commands while the prompt is drawn uses the yield
path. The helper that picks between the two lives in
`crates/fredshell-core/src/jobs/notify.rs`.

### 6.1. `trap` — the centrepiece

Forms:

```text
trap                          # equivalent to `trap -p`
trap -l                       # list signal names with numbers
trap -p                       # print all installed traps as
                              #   reinstall-able commands
trap -p SIG ...               # print only the named traps
trap CMD SIG ...              # install CMD for each SIG
trap - SIG ...                # reset each SIG to default
trap '' SIG ...               # ignore each SIG
trap CMD                      # bash extension: install CMD for
                              #   EXIT (if CMD is a non-signal,
                              #   non-empty string; rare)
```

Exit status: 0 on success, 1 on unknown signal name.

Reinstall-output format: `trap -- '<escaped-cmd>' SIG`. The leading
`--` and the single-quoting are exact-match for bash output;
scripts pipe this through `eval` to clone trap state.

Tests: `tests/spec/traps_and_signals/trap_exit.case.toml` already
exists; v1 adds `trap_signal_basic`, `trap_p_roundtrip`,
`trap_clear`, `trap_ignore`, `trap_unknown_signal`,
`trap_in_subshell`, `trap_err_with_seteE`, `trap_debug_with_setT`.

### 6.2. `jobs`

Forms:

```text
jobs                # default: "[1]+ Running command &"
jobs -l             # add PIDs
jobs -p             # PIDs only, one per line
jobs -n             # only jobs whose state changed since last `jobs`
jobs -r             # only running
jobs -s             # only stopped
jobs -x CMD ARGS    # replace job specs in ARGS with PIDs, then exec
```

The `-x` form is unusual: it parses `ARGS`, substitutes every
`%n` with the corresponding PID, and execs the result. It is
the bash-blessed way to write `kill -9 $(jobs -p)`-style code
portably.

Side effect: a `Done` or `Terminated` job that appears in
`jobs` output is removed from the table immediately after.

### 6.3. `fg` and `bg`

```text
fg [job_spec]      # default %+; transfer terminal, wait
bg [job_spec ...]  # default %+; SIGCONT, no terminal transfer
```

`fg` is the most complex builtin in this set. It must:

1. Resolve the job spec; error if not found or no current job.
2. If the job is `Stopped`, send SIGCONT to its process group.
3. Mask SIGTTOU, then `tcsetpgrp` the job's pgid onto the
   controlling-terminal fd from PLAN_04.
4. Wait for the job to stop or exit (`waitpid` with `WUNTRACED`).
5. Reclaim the terminal with `tcsetpgrp` to the shell's own pgid.
6. Unmask SIGTTOU.
7. Set `$?` from the job's status.
8. If the job stopped (didn't exit), update the job table and
   keep the entry; if it exited, remove the entry.

Exit status: the status of the job (when it exits), 128+sig (when
it terminates), or 128+sig (when it stops — same encoding bash
uses; not a separate status).

### 6.4. `kill`

```text
kill [-s SIG | -SIG | -n NUM] target ...
kill -l [exit_status]
kill -L
```

`kill` is both a builtin and a coreutils binary; the builtin must
exist because it accepts job specs (which the external binary
cannot resolve) and because it must update the job table when it
delivers SIGCONT / SIGSTOP / fatal signals to a known job.

`-l` without args lists all signals; with an integer arg between
128 and 255 it prints the corresponding signal name (bash quirk:
status 130 → "INT"). `-L` is a bash alias for `-l`.

### 6.5. `wait`

```text
wait                    # wait for all current background jobs
wait -n [target ...]    # wait for any of the given (or any) to exit
wait -p VAR [target ...]   # bash 5.1+: set VAR to the job that exited
wait target ...         # wait for each in turn
```

Exit status: status of the (last) job waited on. `wait -n` sets
`$?` to the exiting job's status; `wait -p` additionally sets
the named variable.

`wait` interacts with traps in a non-obvious way: if a trapped
signal arrives during `wait`, `wait` returns 128+sig and the
trap fires before `wait`'s caller resumes. This is tested by
`wait_interrupted_by_trap.case.toml`.

### 6.6. `disown`

```text
disown [-ar] [-h] [job_spec ...]
```

Removes a job from the table without killing it. Flags:

- `-a`: all jobs.
- `-r`: only running jobs.
- `-h`: don't remove; instead mark so SIGHUP is not sent at shell
  exit.

`-h` requires a new bit on `Job`: `nohup_on_exit: bool`.

### 6.7. `suspend`

```text
suspend [-f]
```

Sends SIGTSTP to the shell itself. `-f` forces suspension even in
a login shell (otherwise an error). This is a one-line builtin
once the signal-delivery primitives exist, but the `-f` check
requires `ShellOpts::login`.

## 7. Bash quirks the corpus pins down

This list is the bug-bait inventory: each entry is something bash
does that POSIX does not require, that real-world scripts depend
on, and that PLAN*10's implementation \_must* replicate. Each entry
will have a corpus case before the corresponding subtask lands.

1. **Last pipeline stage's status is `$?`.** Even if `set -o
pipefail` is off, the rightmost stage wins. With pipefail the
   leftmost non-zero wins. `$PIPESTATUS` array reports all stages.

2. **`wait` without args waits for everything, returns 0.**
   Including jobs that have already exited but not been reaped.

3. **`trap '...' EXIT` fires before `$?` is read by the parent.**
   So `bash -c 'trap "echo $?" EXIT; false'` prints `1`.

4. **`trap` is reset in subshells; EXIT is also cleared.**
   `bash -c 'trap "echo parent" EXIT; ( true )'` prints "parent"
   once, not twice.

5. **`SIGINT` while reading interactively aborts the line, not
   the shell.** In a script, default behaviour is to terminate
   the script (unless trapped).

6. **`SIGINT` during a `wait` returns 128+2 and the trap fires
   before `wait`'s caller resumes.**

7. **`kill -0 PID` returns 0 if the process exists and we can
   signal it, 1 otherwise.** Used by scripts to check liveness.

8. **`jobs` in a subshell sees only that subshell's jobs.** Which
   for `( ... )` is none until something is backgrounded.

9. **`%?string` matches against the command line, not argv[0].**
   So `%?grep` matches `find . | grep foo`.

10. **`fg %1` on a job that has already exited prints "no such
    job" with exit status 1**, even if there are pending status
    notifications.

11. **A trap installed inside a function persists after the
    function returns**, unless `local -` was used or the trap
    was cleared explicitly.

12. **`set -e` does _not_ trigger ERR for the command in the
    condition of `if`, `while`, `until`, `&&`, `||`, or the
    negated `!`.** Same exemptions apply to ERR.

13. **`exit` inside a trap on a real signal uses the signal's
    128+sig status by default**, not 0. So `trap 'exit' INT;
sleep 100; ^C` exits 130, not 0.

14. **`disown -h` does not remove from the table.** Only
    suppresses SIGHUP at shell exit. The job still appears in
    `jobs` output.

15. **`bg %1` on a job that is not stopped is an error** with
    "bg: job %1 already in background" and status 1.

## 8. Public API summary

Everything PLAN_13 exposes to the rest of the codebase, in one
place:

```rust
// crates/fredshell-core/src/exec/job/mod.rs
pub struct JobTable { /* §4.1 */ }
pub struct Job { /* §4.1 */ }
pub struct JobId( /* §4.1 */ );
pub enum JobState { /* §4.1 */ }
pub enum JobError { /* §4.3 */ }
pub struct NotifyLine { /* see §10 */ }
pub struct JobStateChange { /* see §10 */ }

// crates/fredshell-core/src/exec/job/spec.rs
pub fn parse_job_spec(input: &str) -> Result<JobSpec, JobSpecError>;
pub enum JobSpec { /* §4.4 */ }
pub enum JobRef { JobId(JobId), Pid(libc::pid_t) }

// crates/fredshell-core/src/exec/trap/mod.rs
pub struct TrapTable { /* §5.1 */ }
pub enum TrapKind { /* §5.1 */ }
pub enum TrapDisposition { /* §5.1 */ }

// Top-level dispatch
pub fn process_pending_signals(
    env: &mut ExecEnv,
) -> ControlFlow<ExitStatus, ()>;
```

Builtins are registered through the existing Tier-1 dispatch
table from PLAN_06; PLAN_13 contributes eight entries.

## 9. Testing strategy

### 9.1. Unit tests (L1)

In `crates/fredshell-core/src/exec/job/tests.rs` and
`crates/fredshell-core/src/exec/trap/tests.rs`. Coverage targets:

- Job-spec parser: every form, every error variant, ambiguity
  detection, case sensitivity.
- Job state transitions: every entry in §4.2's table, plus
  current/previous promotion rules from §4.1.
- Trap-table insert/lookup/clear, including subshell-clear
  semantics (§5.5).
- Signal-name resolution: case insensitivity, `SIG` prefix
  tolerance, decimal-number form, unknown-name error.

These run without forking; they exercise pure data structures.

### 9.2. Integration tests (L2/L3)

The spec corpus owns the integration tests. Categories:

- `tests/spec/traps_and_signals/` — at least 12 cases covering
  §7 quirks 3, 4, 5, 6, 11, 12, 13.
- `tests/spec/job_control/` — at least 15 cases covering §7
  quirks 1, 2, 8, 9, 10, 14, 15, plus the basic forms of every
  builtin in §6.

Each case marked `pass` becomes a regression guard. Cases marked
`deferred:PLAN_13` are the implementation worklist.

### 9.3. PTY harness (L4)

Several behaviours can only be tested with a real PTY:

- `fg` foreground transfer (requires a controlling tty).
- `^C` SIGINT delivery during command read (requires PLAN_14's
  line editor).
- Monitor-mode status-line printing at prompt time.

The PTY harness is owned by PLAN_14 §"pty harness". PLAN_13
contributes test cases against it; PLAN_14 owns the harness
machinery.

### 9.4. Differential testing (L5)

Every Tier-1 builtin in §6 must pass through PLAN_08's
differential program before its corpus case is marked `pass`.
The differential program runs the same script under fredshell
and pinned bash, then compares stdout, stderr, and exit status.
For traps, stderr matters too (the "[1]+ Done …" lines).

## 10. Performance contract

Job-table operations are O(log n) in number of jobs (`BTreeMap`).
A `SIGCHLD`-driven reap costs:

- One `waitpid(-1, …, WNOHANG)` syscall per ready child.
- One `BTreeMap` lookup per status change.
- Zero allocations on the hot path (PIDs and signals are
  primitives; `NotifyLine` reuses a `String` from a thread-local
  pool).

Bench target: a tight `while true; do (sleep 0.001 &) ; wait;
done` loop reaches steady state at within 10% of bash's
throughput on the same hardware. Measured by
`exec_job_throughput` Criterion bench, added in subtask 12.6.

Trap dispatch on the hot path (no traps installed, no pending
signals) is one atomic load: `pending_signals.load(Relaxed)`.

## 11. Migration and rollout

PLAN_13 is Phase B and lands after the PLAN_08 differential
harness is green against bash. Recommended landing order:

| Subtask | Surface                                     | Gate                       |
| ------- | ------------------------------------------- | -------------------------- |
| 12.1    | Job table data model + unit tests           | none                       |
| 12.2    | Job-spec parser + tests                     | 12.1                       |
| 12.3    | Trap table data model + unit tests          | none (parallel with 12.1)  |
| 12.4    | Signal-name resolution + tests              | 12.3                       |
| 12.5    | Reaper + `process_pending_signals` plumbing | 12.1, 12.3, PLAN_04 §4     |
| 12.6    | `jobs`, `disown`, `suspend` builtins        | 12.1, 12.2                 |
| 12.7    | `bg`, `fg` builtins (incl. terminal xfer)   | 12.6, PLAN_04 §7           |
| 12.8    | `kill`, `wait` builtins                     | 12.6                       |
| 12.9    | `trap` builtin (all forms)                  | 12.3, 12.4, 12.5           |
| 12.10   | EXIT / ERR / DEBUG / RETURN pseudo-signals  | 12.9                       |
| 12.11   | Monitor-mode notification at prompt         | 12.6, 12.10, PLAN_14 ready |
| 12.12   | Differential parity sweep against bash      | all above, PLAN_08 green   |

Each subtask is a single PR with a corpus case (or set of cases)
flipping from `deferred:PLAN_13` to `pass`.

## 12. Open questions

These are unresolved at draft time. They do not block landing the
plan document; they are flagged for the implementation phase.

- **Q10.1** — Should `disown -a` followed by shell exit send
  SIGHUP to the disowned jobs? Bash does not. We will not. But
  this should be explicitly tested.
- **Q10.2** — How does `wait -n` interact with `set -e`? If `-n`
  picks up a non-zero-exit job, does `set -e` abort? Bash: yes,
  unless the `wait` is in a condition context. We need a
  decision and a test before 12.8.
- **Q10.3** — `coproc`. Deferred from v1. **Resolved:** the
  eventual implementation is owned by `PLAN_20_coproc.md`
  (stub landed 2026-05-23). PLAN_13's role when that work
  picks up is to provide the job-table entry and the
  `NAME_PID` binding; PLAN_12 will own grammar and executor.
  v1 emits a parser refusal.
- **Q10.4** — Should we expose `JobStateChange` notifications via
  a future PLAN_18 (AI) hook so the assistant can see "your build
  job just failed"? Probably yes, but PLAN_18 is not drafted yet
  and we don't pre-build the API.
- **Q10.5** — `set -b` (immediate notification) requires printing
  status lines from `process_pending_signals`, which can be
  called mid-command. The terminal state may be in raw mode
  (line editor active). **Resolved:** PLAN_14 §9.5 owns the
  `yield_terminal` primitive; PLAN_13 §6 routes all
  job-notification lines through it when an editor session is
  active, and writes directly to stderr otherwise.

## 13. Relationship to other plans

- **PLAN_02** — adds `ShellState::jobs` and `ShellState::traps`
  fields per §3 above. `ExecEnv` gains a `signal_policy` field
  per PLAN_12's prediction.
- **PLAN_04** — consumes `tcsetpgrp`/`setpgid` from §7, the
  self-pipe / signalfd from §4, the SIGTTOU mask helper from §4.
  PLAN_04 does not change; PLAN_13 only consumes.
- **PLAN_05** — owns the spec-corpus rows for `bg`, `fg`, `jobs`,
  `kill`, `wait`, `trap`, `disown`, `suspend`. PLAN_13's
  acceptance criteria are PLAN_05 cases flipping to `pass`.
- **PLAN_06 Phase A** — exposes Tier-1 dispatch; PLAN_13
  contributes eight builtin entries to the inventory.
- **PLAN_12 (Phase B)** — depends on PLAN_13 to remove the
  `/bin/sh -c` fallback for any script using `trap`, `wait`, or
  `&`. PLAN_12 already cites PLAN_13 as the owner.
- **PLAN_14** — owns the L4 PTY harness PLAN_13 uses for `fg`
  testing; owns the "yield terminal for status line" primitive
  PLAN_13 needs in monitor mode (Q10.5).
- **PLAN_07** — produces the per-builtin spec sheets for the
  eight builtins in §6 before their corresponding subtasks land.
- **PLAN_08** — produces the differential harness whose
  green-against-bash status gates the PLAN_13 implementation
  phase (per ADR 0003).

## 14. Implementation log

Empty; no subtasks landed yet. Each subtask completion appends a
dated entry with PR hash and a one-line summary.

## 15. Cleanup items surfaced during implementation

- **15.1** — `extdebug` mode (DEBUG-trap return value can
  suppress the next command). v1 ignores DEBUG's exit status.
  If a corpus case demands it, this becomes a subtask 12.10.x.

## References

- `Documents/PLAN_02_architecture.md` — `ExecEnv` and
  `ShellState` shape; §4 (data model), §5 (subsystem layout),
  §6.1 (signal plumbing).
- `Documents/PLAN_04_terminal_io.md` — §4 signal policy, §7
  process-group plumbing, §5 capability detection. PLAN_13
  consumes; PLAN_04 does not change.
- `Documents/PLAN_05_testing.md` — §3.4 corpus categories
  (`job_control`, `traps_and_signals`), §11 builtin inventory
  (rows assigned to PLAN_13), §12 status taxonomy
  (`deferred:PLAN_13`).
- `Documents/PLAN_06_exec.md` — Phase A dispatch surface.
- `Documents/PLAN_12_exec_phase_b.md` — Phase B; this document
  is what the Phase B §13 grid punted to.
- `Documents/PLAN_14_line_editor.md` (pending) — PTY harness;
  "yield terminal" primitive.
- `Documents/PLAN_07_spec_drafting.md` (pending) — per-builtin
  spec sheets for §6.
- `Documents/PLAN_08_fuzzer.md` (pending) — differential
  parity oracle.
- `Documents/decisions/0001-in-process-execution-and-builtin-tiers.md`
  — places the eight job-control builtins in Tier-1.
- `Documents/decisions/0003-test-first-compatibility-methodology.md`
  — establishes that the corpus, not prose, is the source of
  truth for "what fredshell must do."
- `Documents/decisions/0004-strict-default-execution.md` — the
  `/bin/sh -c` fallback PLAN_13 indirectly retires for scripts
  using job control or traps.
- bash reference manual, "JOB CONTROL" and "SIGNALS" sections.
