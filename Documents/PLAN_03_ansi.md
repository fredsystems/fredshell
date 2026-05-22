# PLAN_03 — `fredshell-ansi` Crate Design

> Last updated: 2026-05-20 — implementation complete; merged via task-03/ansi-crate (27edd4e).
> Phase: A. Status: implemented.
> Operationalizes ADR 0002.

## Implementation status

All v1 scope items in this document landed on `main` via the `task-03/ansi-crate`
branch. Subtask commits:

| Subtask | Commit  | Summary                                                                              |
| ------- | ------- | ------------------------------------------------------------------------------------ |
| 03.1    | 5de39ac | Crate skeleton, `Encode` / `Decode` traits, `AnsiError` types.                       |
| 03.2    | f756149 | SGR encoder: `Sgr`, `Color`, `Underline`; zero-alloc `encode`.                       |
| 03.3    | 94148cc | `Cursor` and `Erase` encoders + shared integer-write helpers.                        |
| 03.4    | 6b3b698 | OSC 7 / 8 / 52 / 133 encoders (percent + base64 streaming).                          |
| 03.5    | 4db0e4a | DECSET / DECRST `Mode`s + kitty keyboard push / pop / set.                           |
| 03.6    | 638b54f | Decoders for DA1, DSR cursor position, kitty query, OSC 52 read.                     |
| 03.7    | 7682160 | `encode_all` / `encoded_len_all` / `EncodeDyn`, `encode_checked`, Criterion benches. |
| 03.8    | 40f052b | Port `fredshell-prompt` off `nu-ansi-term` to `fredshell-ansi`.                      |

Open questions in §10 that were resolved during implementation:

- **§10 `encoded_len` enforcement.** `encode_checked` shipped in 03.7 as a
  debug-mode helper that compares the actual written length against
  `encoded_len()`; the trait contract is verified in tests.
- **§10 `bitflags` dependency.** Adopted in 03.5 for `KittyKeyboardFlags`.
- **§10 error type granularity.** Split into encoder-side (`AnsiError`) and
  decoder-side (`DecodeError`) per §5 of this document.
- **§10 diff SGR encoder.** Not shipped in v1; revisit when line editor
  benchmarks exist (PLAN_07).
- **§7.1 / §10 OSC terminator.** v1 emits `ST` universally as planned; no
  caller has requested `BEL`.
- **§10 `Color::Indexed(0..=15)` normalization.** Emits as written.

Carried forward to later plans:

- Capability detection / which sequences are safe to send — PLAN_04.
- Mouse-event decoding — PLAN_07 (if enabled).
- Diff SGR encoder evaluation — gated on PLAN_07 line-editor benchmarks.

This document specifies the `fredshell-ansi` crate: its scope, public
API shape, performance contract, and the small set of structured
responses it must parse. ADR 0002 settled the question of whether
fredshell should share types with `freminal-common` (no) and committed
to an encoder-first design. This document fills in the details.

## 1. Scope and non-scope

### In scope (v1)

- **Encoder API** for the escape sequences a shell emits:
  - SGR (Select Graphic Rendition): bold, dim, italic, underline (with
    style variants and color), reverse, strikethrough, color (16 / 256
    / truecolor) foreground and background, and the corresponding
    resets.
  - CSI cursor movement: up/down/left/right by N, absolute positioning
    (CUP), erase in line (EL), erase in display (ED), save/restore
    cursor (DECSC/DECRC).
  - OSC 7 (current working directory notification).
  - OSC 8 (hyperlinks: set and clear).
  - OSC 52 (clipboard set).
  - OSC 133 (semantic prompt markers A/B/C/D).
  - DECSET / DECRST for the small set of modes a shell toggles:
    bracketed paste (2004), alternate screen (1049), application
    cursor keys (1), focus reporting (1004), mouse modes if any are
    enabled.
  - Bracketed paste enable/disable convenience.
  - Kitty keyboard protocol push/pop sequences (the negotiation
    bytes; the _interpretation_ of received keys belongs to PLAN_07).

- **Minimal decoder** for the structured responses a shell must read:
  - DA1 (Primary Device Attributes) response.
  - DSR (Device Status Report) cursor position response.
  - Kitty keyboard protocol query response.
  - OSC 52 clipboard read response.

### Out of scope (v1)

- General-purpose ANSI/VT100 decoder. fredshell does not interpret
  arbitrary child PTY output as a terminal emulator would. That is
  freminal's domain.
- DCS, APC, SOS, PM sequences except the kitty-keyboard-push/pop
  pieces that ride on CSI.
- Sixel, ReGIS, image-protocol sequences (kitty/iTerm). Not a shell
  concern.
- Terminfo / termcap consultation. fredshell hard-codes the sequences
  it needs; capability _detection_ (which sequences are safe to send)
  is owned by PLAN_04.
- Mouse-event decoding. If the line editor enables mouse modes
  (PLAN_07 decides), the decoder side lives there, not here.

The boundary rule: `fredshell-ansi` knows how to _speak_ terminal; it
does not know how to _be_ a terminal. PLAN*04 owns the question of
\_when* it is safe to speak which dialect.

## 2. Design tenets

1. **Encoder first.** The dominant use case is "emit a sequence."
   The data model is shaped around that, not retrofitted from a
   decoder design (per ADR 0002).
2. **`Write`-based.** Every sequence emits via
   `fn write_to<W: Write>(&self, w: &mut W) -> io::Result<()>`. No
   intermediate `String` or `Vec<u8>` allocation on the hot path.
3. **Strongly typed.** Sequences are constructed from enums and
   newtypes, not free-form `&str`. The compiler enforces
   well-formedness; there is no "unknown parameter" variant in the
   encoder surface.
4. **Allocation-light.** The line editor redraws on every keystroke.
   A redraw must emit zero heap allocations from `fredshell-ansi`.
   Decoded responses, which appear rarely, may allocate where it
   simplifies the API (e.g., OSC 52 clipboard payload).
5. **Spec-faithful.** The bytes emitted match the relevant ECMA-48,
   xterm, kitty, and OSC specifications. Bug-for-bug compatibility
   with specific terminal emulators is not pursued; spec compliance
   is.
6. **No global state.** No static variables, no `OnceCell`-backed feature
   flags, no `set_color_mode()`. The caller decides what to emit;
   the crate emits it.

## 3. Crate metadata

- Name: `fredshell-ansi`.
- License: MIT.
- Public dependencies: none beyond `std`. Internal dependencies kept
  minimal; `bitflags` is acceptable if it simplifies the SGR
  encoder; otherwise nothing.
- No `serde`, no `tracing`, no async runtime, no `clap`.
- `no_std` is not a goal in v1 but is not foreclosed: the crate's
  surface is `Write`-based, which is std today but easy to abstract
  later.
- Error type: `AnsiError`, per AGENTS.md. No `anyhow`. No `expect`.

## 4. Encoder API shape

The API is organized as a small set of types per category, each
implementing a common `Encode` trait. The trait keeps the surface
uniform and makes it easy for the line editor to stream sequences in
sequence without per-type imports.

### 4.1. The `Encode` trait

```rust
/// Common contract for all encodable ANSI sequences.
///
/// Implementations must not allocate. Implementations must write a
/// well-formed sequence and nothing else (no trailing newlines, no
/// resets unless the sequence type implies one).
pub trait Encode {
    fn encode<W: Write>(&self, w: &mut W) -> io::Result<()>;

    /// The exact number of bytes `encode` will write, for buffer
    /// pre-sizing. Implementations must return the correct count;
    /// callers may rely on it.
    fn encoded_len(&self) -> usize;
}
```

`encoded_len` is non-optional. The line editor's redraw loop computes
the total length of the next frame's escape-sequence prelude and
asks the underlying writer (typically a `BufWriter<StdoutLock>`) to
reserve that many bytes. This avoids the `write_all`-then-extend
allocation pattern.

### 4.2. SGR

```rust
pub struct Sgr {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: Underline,
    pub reverse: bool,
    pub strikethrough: bool,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
}

pub enum Underline {
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

pub enum Color {
    Black, Red, Green, Yellow, Blue, Magenta, Cyan, White,
    BrightBlack, BrightRed, /* … */ BrightWhite,
    Indexed(u8),               // 0–255
    Rgb { r: u8, g: u8, b: u8 },
}

impl Sgr {
    pub const RESET: Sgr = /* all-off */;
    pub fn fg(color: Color) -> Self { /* … */ }
    pub fn bg(color: Color) -> Self { /* … */ }
    pub fn with_bold(mut self) -> Self { /* … */ }
    // … builder methods, all const-friendly.
}

impl Encode for Sgr { /* … */ }
```

A `Sgr` value is a _complete style_. The encoder emits a single
`CSI … m` sequence with the parameters needed to reach that style
from the SGR-reset baseline. The caller is responsible for emitting
`Sgr::RESET` at the end of styled output if the surrounding context
expects an unstyled baseline.

Open question: a _diff-encoder_ (`Sgr::transition(from, to)`) that
emits only the parameters that changed between two adjacent styles
could halve redraw bandwidth. v1 ships the absolute encoder;
benchmarks decide whether the diff form is worth adding.

### 4.3. CSI cursor and erase

```rust
pub enum Cursor {
    Up(u16),
    Down(u16),
    Left(u16),
    Right(u16),
    Goto { row: u16, col: u16 },     // 1-indexed, per VT spec
    Save,
    Restore,
}

pub enum Erase {
    InLineToEnd,
    InLineToStart,
    InLineAll,
    InDisplayToEnd,
    InDisplayToStart,
    InDisplayAll,
}

impl Encode for Cursor { /* … */ }
impl Encode for Erase { /* … */ }
```

`Cursor::Up(0)` is a no-op (writes nothing). `Cursor::Goto { row: 0,
col: 0 }` is invalid input; the type's constructor or a `try_new`
returns `AnsiError::InvalidCoordinate`. Callers that want a 0-indexed
API wrap it; the crate sticks to the spec convention.

### 4.4. OSC sequences

```rust
pub struct Osc7 { pub cwd: PathBuf, pub hostname: Option<String> }
pub enum Osc8 {
    Set { uri: String, id: Option<String> },
    Clear,
}
pub struct Osc52Set { pub payload: Vec<u8> }   // raw bytes; encoder base64s
pub enum Osc133 { PromptStart, CommandStart, CommandEnd, OutputStart }
```

`Osc52Set` is the one place where the encoder accepts an owned buffer
because base64 encoding the payload requires a working buffer. The
encoder writes the base64 stream directly to `W` without
intermediate allocation beyond a fixed-size on-stack scratch buffer.

OSC string-terminator is `ST` (`ESC \`), not `BEL`. v1 emits `ST`
universally; if a real-world terminal turns up that only honors
`BEL`, PLAN_04 owns the policy decision and `fredshell-ansi` grows
a feature flag.

### 4.5. Modes (DECSET/DECRST)

```rust
pub enum Mode {
    ApplicationCursorKeys,        // 1
    AlternateScreen,              // 1049
    BracketedPaste,               // 2004
    FocusReporting,               // 1004
    KittyKeyboard,                // CSI > flags ; mode u (separate type)
    // … only the modes a shell actually toggles
}

pub struct ModeSet { pub mode: Mode }
pub struct ModeReset { pub mode: Mode }

impl Encode for ModeSet { /* CSI ? n h */ }
impl Encode for ModeReset { /* CSI ? n l */ }
```

Kitty keyboard protocol gets a dedicated type because it has
push/pop semantics distinct from DECSET/DECRST:

```rust
pub struct KittyKeyboardPush { pub flags: KittyKeyboardFlags }
pub struct KittyKeyboardPop;
pub struct KittyKeyboardSet { pub flags: KittyKeyboardFlags }
```

The flags themselves are a `bitflags!` set. The encoder emits the
canonical `CSI > flags ; mode u` form.

### 4.6. Convenience: stream helpers

```rust
/// Encode several sequences in order, writing them to a single
/// writer. Equivalent to a manual loop but spelled out for clarity.
pub fn encode_all<W: Write>(w: &mut W, items: &[&dyn Encode]) -> io::Result<()>;

/// Sum of `encoded_len()` for several items.
pub fn encoded_len_all(items: &[&dyn Encode]) -> usize;
```

These exist because the line editor frequently writes
"cursor-goto + erase-to-end + new-content" as an atomic operation.
A single dispatched-through-trait loop is fine; the dyn-call
overhead is negligible compared to the I/O.

## 5. Decoder API shape

The decoder surface is small. Each response type implements a
`Decode` trait:

```rust
pub trait Decode: Sized {
    /// Attempt to decode a complete response from `input`. Returns
    /// `Ok((decoded, consumed))` on success, where `consumed` is
    /// the number of bytes used from the front of `input`. Returns
    /// `Err(Incomplete)` if the input is a valid prefix but not yet
    /// complete (caller should buffer more). Returns
    /// `Err(Malformed)` if the input cannot be a valid response.
    fn decode(input: &[u8]) -> Result<(Self, usize), DecodeError>;
}

pub enum DecodeError {
    Incomplete,
    Malformed { at: usize, reason: &'static str },
}

pub struct Da1Response { /* DA1 ID, parameters */ }
pub struct DsrCursorPosition { pub row: u16, pub col: u16 }
pub struct KittyKeyboardQueryResponse { pub flags: KittyKeyboardFlags }
pub struct Osc52ReadResponse { pub payload: Vec<u8> }   // owned; decoded
```

The decoder is **byte-oriented and incremental**. The line-editor
input loop reads bytes from the tty, buffers them, and tries to
decode known response types. Unknown sequences are passed through to
the line-editor's key decoder unchanged (the key decoder, owned by
PLAN_07, has its own logic for CSI sequences that represent keys).

The decoder is _not_ a full ANSI state machine. It is a set of
"does this byte slice match one of these four known shapes?"
matchers. Anything not matching is not the decoder's problem.

### 5.1. What the decoder does not handle

- Mixed-input streams where escape sequences are interleaved with
  printable text. The shell never reads such streams: the only place
  the shell reads from the tty is the input loop, and that loop
  hand-routes between "this is a key" and "this is a query response."
- Streaming via async. The decoder is `&[u8] -> Result<…>` pure.
  The caller owns buffering.
- Recovery from malformed input mid-sequence. On `Malformed`, the
  caller is expected to drop bytes up to the next plausible start
  (ESC) and retry.

## 6. Performance contract

The line editor's per-keystroke budget is <1ms (PLAN_02 §9). A
substantial fraction of that budget is the redraw write. The encoder
must therefore:

- Emit zero heap allocations for any sequence other than `Osc8::Set`
  with an owned `uri` and `Osc52Set` (whose payload is already owned
  by the caller).
- Encode a typical line-editor redraw frame
  (`cursor-goto + erase-line + sgr + content + sgr-reset`) in well
  under 100µs on the reference platform.
- Make `encoded_len()` exact and cheap (≤10ns per call). The line
  editor calls it before every frame.

A bench suite at `crates/fredshell-ansi/benches/` covers:

- `sgr_encode_simple` (single SGR write).
- `sgr_encode_complex` (full style with truecolor fg+bg).
- `redraw_frame` (the full per-keystroke frame above).
- `da1_decode`, `dsr_decode`, `kkbd_decode`, `osc52_decode`.

Per AGENTS.md, every change to this crate captures before/after
numbers for these benches.

## 7. Compatibility and the "what does my terminal support" problem

`fredshell-ansi` deliberately does not know what a terminal supports.
It emits bytes; the caller decides which bytes are safe.

Capability detection (querying DA1, parsing the response, deciding
"this terminal supports truecolor / kitty keyboard / OSC 52 /
hyperlinks") is **PLAN_04's** responsibility. The terminal-I/O layer
runs a capability probe at startup, builds a `TerminalCapabilities`
struct, and the prompt / line editor consult that struct before
asking `fredshell-ansi` to emit a sequence.

This split is enforced by the crate-dependency rule: `fredshell-ansi`
has no dependency on capability-detection logic. If a caller wants
truecolor, they emit truecolor; whether that was a good idea is not
the encoder's problem.

### 7.1. The "ST vs BEL" question, revisited

Some terminals historically required `BEL` (`0x07`) instead of `ST`
(`ESC \`) to terminate OSC sequences. v1 emits `ST` universally. If
PLAN_04 detects a terminal that requires `BEL`, two options:

- The terminal capability struct flips a global preference and a
  `set_osc_terminator(OscTerminator)` thread-local controls
  `fredshell-ansi` output. **Rejected**: violates "no global state."
- The caller chooses per-emit which terminator to use via a per-OSC
  type field. **Provisional**: adds noise but is composable.
- The encoder grows a configuration value passed through
  `EncodeCtx` (a new `&Ctx` parameter to `Encode::encode`).
  **Open**: changes the trait shape.

Decision deferred until PLAN_04 produces a concrete list of
terminals that need `BEL` in 2026. None of the user's daily-driver
terminals (kitty, WezTerm, alacritty, Ghostty, iTerm2) require it.

## 8. Testing

Spec tests for `fredshell-ansi` are unit-level (L1 in PLAN_05): every
encoder writes exactly the expected byte sequence, every decoder
parses exactly the expected input. Golden-file snapshots are
acceptable but not required; inline `assert_eq!` against literal
`b"\x1b[…"` byte strings is the preferred style because it makes the
test self-documenting.

Property-based tests (via `proptest` if added to the dependency
graph) cover:

- Round-trip: every value whose encoded form is parseable by the
  decoder round-trips.
- `encoded_len(x) == encode(x).len()` for all `x`.

The crate is exercised end-to-end via the line editor (L4 PTY
tests, PLAN_07) and indirectly through the prompt (L2 integration
tests in `fredshell-prompt`).

## 9. Migration path

The current scaffold has `nu-ansi-term` listed in workspace deps,
used (or to be used) by `fredshell-prompt`. The migration path:

1. `fredshell-ansi` lands as a new crate. The existing
   `fredshell-prompt` continues to use `nu-ansi-term` temporarily.
2. `fredshell-prompt` is ported to `fredshell-ansi` once the SGR
   surface is complete. The migration is mechanical because the
   shape (build a styled span, emit) is similar.
3. `nu-ansi-term` is removed from workspace dependencies.

The line editor (`fredshell-line-editor`, when it exists) uses
`fredshell-ansi` from the start.

## 10. Open questions

- **`Encode::encoded_len` correctness enforcement.** The contract is
  "exact." A debug-assert pattern (`encode` writes to a counting
  writer, panics in debug if the count diverges from
  `encoded_len`) could catch drift. Decision: yes, ship it in debug
  builds. Detail goes in the crate README.
- **Diff SGR encoder.** `Sgr::transition(from, to)`. v1 ships
  absolute encoder; revisit per benchmark data.
- **`bitflags` dependency.** Either accept the small dependency for
  ergonomics on `KittyKeyboardFlags` or hand-roll. Default:
  `bitflags` if and only if v1 grows >1 flag-set type.
- **OSC terminator config.** See §7.1.
- **Error type granularity.** Single `AnsiError` enum across encoder
  and decoder, or split `EncodeError` / `DecodeError`? Default:
  split, because the failure modes are different shapes.
- **`Color::Indexed(u8)` for 0–15.** The first 16 indexed colors are
  semantically equivalent to the named colors. Whether the encoder
  normalizes `Indexed(0..=15)` to the named form (smaller output)
  or emits the indexed form as written is an open ergonomics
  question. Default: emit as written; let the caller normalize if
  desired.

## References

- `Documents/decisions/0002-ansi-encoding-crate-strategy.md` — the
  ADR this document operationalizes.
- `Documents/PLAN_02_architecture.md` — `fredshell-ansi` slot in the
  crate inventory and the performance budget allocation.
- `Documents/PLAN_04_terminal_io.md` (pending) — capability
  detection, raw mode, signal handling, terminal feature negotiation.
- `Documents/PLAN_07_line_editor.md` (pending) — line editor and
  the per-keystroke redraw loop that is `fredshell-ansi`'s primary
  consumer.
- `Documents/PLAN_11_prompt.md` (pending) — prompt segment renderer,
  secondary consumer.
- ECMA-48 (CSI), xterm ctlseqs (OSC and DECSET), kitty keyboard
  protocol specification.
