# PLAN_14 — Interactive UX and Line Editor

> Last updated: 2026-05-24 — cascade renumber to insert PLAN_10
> embedding (ADR 0006); document renamed PLAN_13 → PLAN_14. Body
> cross-references swept by tooling. Substance unchanged. Note:
> this document's "Consumed by" metadata contains a pre-existing
> stale "PLAN_NN (config)" cross-reference that pre-dates the
> work-order renumber; it is NOT corrected by the cascade sweep
> and remains tracked as a separate cleanup item.
>
> Previously (2026-05-22): scope augmentation: PLAN_14 now
> explicitly owns the `history` and `fc` builtins (new §8.6),
> the `yield_terminal` primitive consumed by PLAN_13 trap
> delivery (new §9.5, answers PLAN_13 Q10.5), and the L4 PTY
> harness for end-to-end editor tests (new §12.7). Prompt
> rendering remains delegated to PLAN_15.
> Earlier on 2026-05-20 — first draft; §5 rewritten around
> `Vec<LogicalRow>` of TChar with render-only soft wrap; §9.3
> expanded with wrap module, RowLayout, VisualCursor; §10 adds
> rejected-alternatives subsections; §12 expanded with mandatory
> coverage matrix and round-trip properties.
> Phase: A. Status: draft.
> Settles the line-editor question left open in PLAN_02 §5.
> Consumes: PLAN_03 (encoders), PLAN_04 (terminal session).
> Consumed by: PLAN_15 (prompt), PLAN_06 (completion + builtin
> dispatch for `history`/`fc`), PLAN_13 (trap delivery via
> `yield_terminal`), PLAN_13 (config).

This document specifies the line editor and surrounding interactive
UX: keystroke decoding, buffer model, keymap dispatch, history,
hints, syntax highlighting, completion glue, and the redraw loop.
It commits fredshell to building its own line editor rather than
adapting `reedline`. §10 records why.

PLAN_14 is the largest subsystem in fredshell by line count and by
calendar time. The document is honest about that. The implementation
phases (§11) are designed so that the rest of the shell — parser,
exec, builtins, prompt — can be built and tested in parallel with
the editor, against a deliberate scaffold (§11.1).

## 1. Scope and non-scope

### In scope (v1)

- **Keystroke decoding.** Raw bytes from
  `TerminalSession::input()` → semantic `KeyEvent`s. Covers CSI,
  SS3, kitty keyboard protocol levels 0–4, bracketed paste, focus
  in/out, and modifier disambiguation.
- **Buffer model.** A Unicode-correct, multiline edit buffer with
  grapheme-cluster cursor positioning and width-aware rendering.
- **Keymap dispatch.** Emacs and vi modes, both as v1
  requirements. Configurable bindings. Chord support
  (`Ctrl-X Ctrl-E`). Vi includes counts, operators, text objects,
  registers, `.` repeat, marks within the line (no cross-line
  marks in v1).
- **History.** Bash-compatible semantics: `HISTFILE`, `HISTSIZE`,
  `HISTFILESIZE`, `HISTCONTROL`, `HISTIGNORE`, `histappend`, and
  the history-expansion syntax (`!!`, `!$`, `!^`, `!*`, `!:N`,
  `!string`, `!?string?`). On-disk format readable by bash and
  zsh. Concurrent-write safe.
- **History search.** Substring (`Ctrl-R`), prefix (up-arrow with
  partial input), and fzf-style fuzzy overlay (`Ctrl-T` or
  configurable).
- **Hints / autosuggestions.** Fish-style ghost text drawn from
  history, with cheap accept-word and accept-line.
- **Syntax highlighting.** Live, incremental highlight of the
  in-progress command line driven by the bash-compatible parser
  (PLAN_06). Tolerates incomplete input.
- **Multiline editing.** Continuation prompts, intelligent Enter
  (submit if the parser says the line is complete; insert newline
  otherwise), brace/quote matching, visual indent on continuation
  lines.
- **External editor integration.** `Ctrl-X Ctrl-E` (emacs) and
  `v` in vi-normal mode invoke `$EDITOR` with the current buffer
  and re-import on exit.
- **Undo / redo / kill ring / region.** Per emacs conventions, with
  vi `u` / `Ctrl-R` mapped onto the same underlying history.
- **Bracketed paste.** Disable highlighting, hints, and completion
  while the paste is in flight; insert exactly what was pasted,
  newlines included; do not submit on Enter inside a paste.
- **Completion glue.** PLAN*07 owns the \_menu* and trigger
  semantics; PLAN*09 owns \_candidate generation*. The seam is
  documented in §7.
- **Redraw loop.** Diff-based; emits only the bytes needed via
  PLAN_03 encoders; respects `Capabilities::synchronized_output`.
- **`history` and `fc` builtins.** PLAN_14 owns the in-process
  implementations of the `history` and `fc` builtins. They are
  thin wrappers over the same history store the interactive
  editor uses; PLAN_06 dispatches to them by name. See §8.6.
- **Terminal yield primitive.** A `yield_for_one_line` operation
  that hands the controlling terminal to a foreground child for
  the duration of one read-line/read-line-equivalent and reclaims
  it afterwards, without tearing down the editor state. Consumed
  by PLAN_13 trap delivery (Q10.5) and by external-editor
  integration. See §9.5.

### Out of scope (v1)

- **Mouse input.** Deferred. The decoder layer is structured so
  that mouse can be added later without disrupting key handling.
- **Bidi text rendering.** RTL scripts render left-to-right with
  logical cursor order in v1. A future revision may revisit.
- **Cross-line vi marks, macros (`q`), and `:ex` commands.** Vi
  mode in v1 is "vim-without-the-ex-line."
- **Image protocols inside the buffer.** Not a shell concern.
- **Plugin / scripting hooks for the editor.** Hooks belong in
  PLAN_13 (config); v1 ships a fixed set of `EditCommand`s
  exposed by name through configuration.

## 2. Design tenets

1. **Keystroke latency is the headline metric.** PLAN_02 §6 sets a
   <1 ms median, <5 ms p99 budget for keystroke → screen. The
   buffer model, keymap dispatch, and redraw loop are designed
   around that budget. Anything that allocates on the hot path is
   a bug.
2. **Unicode correctness is non-negotiable.** Grapheme clusters,
   not bytes; cell widths from a fredshell-owned table, not from
   `unicode-width`'s defaults; emoji ZWJ sequences render as a
   single cluster.
3. **Diff-based redraw.** Each redraw produces a target frame
   (cells + styles); the renderer emits only the differences
   from the previous frame. No "clear and repaint."
4. **Composable, testable layers.** Decoding, buffer, keymap, and
   render are independent modules with pure interfaces. Each is
   unit-tested without a terminal.
5. **No global state.** The editor is a value (`LineEditor`),
   constructed with explicit dependencies. Multiple editors can
   coexist in tests; the runtime happens to use exactly one.
6. **Bash-compatible where users notice.** History format, history
   expansion, `set -o emacs`/`set -o vi`, `READLINE_LINE` /
   `READLINE_POINT` semantics for bindable functions — all of
   these are user-visible and match bash.
7. **Configurable, not extensible.** Users bind keys to a fixed
   set of named `EditCommand`s. They do not write Lua. Extension
   happens by adding `EditCommand` variants in fredshell source,
   not by user plugins.

## 3. Module layout

```text
crates/fredshell-core/src/edit/
  mod.rs              — public surface: LineEditor, EditOutcome
  key/
    mod.rs            — KeyEvent, Modifiers
    decode.rs         — bytes → KeyEvent state machine
    kitty.rs          — kitty keyboard protocol decoding
    paste.rs          — bracketed paste tracker
  buffer/
    mod.rs            — Buffer (grapheme-indexed, multiline)
    width.rs          — cell-width table and cluster width
    cursor.rs         — grapheme-position cursor with anchor
    undo.rs           — undo/redo ring
  keymap/
    mod.rs            — Keymap trait, dispatch
    emacs.rs          — default emacs bindings
    vi.rs             — vi normal/insert/visual + operators
    chord.rs          — multi-key chord tracker
  command/
    mod.rs            — EditCommand enum and execution
  history/
    mod.rs            — History trait + bash-compatible impl
    file.rs           — read/write HISTFILE with concurrent safety
    expand.rs         — !! / !$ / !N expansion
    search.rs         — substring, prefix, fuzzy
  hint/
    mod.rs            — Hinter trait + history-based hinter
  highlight/
    mod.rs            — Highlighter trait, parser-driven impl
  complete/
    mod.rs            — CompletionMenu (rendering + selection)
    fzf.rs            — fzf-style overlay menu
  render/
    mod.rs              — frame model
    diff.rs             — frame-to-frame diff
    paint.rs            — diff → PLAN_03 byte stream
  yield_terminal.rs     — terminal-yield primitive (§9.5)
  scaffold/
    mod.rs              — cooked-mode bridge (see §11.1)
```

The `history` and `fc` builtin entry points live in
`crates/fredshell-core/src/builtins/history.rs` and
`builtins/fc.rs` (PLAN_06's module tree); they hold a
`&mut dyn HistoryStore` whose concrete implementation lives in
`edit::history`. See §8.6.

The PTY harness (§12.7) lives in
`crates/fredshell-core/tests/pty/` plus a `pty_harness` module
under `fredshell-core::testing` that is compiled only with the
`test-support` feature or under `cfg(test)`.

`fredshell-core::edit` is the public surface; everything below is
private. The crate boundary stays inside `fredshell-core` for v1
so the parser and exec can share types without an extra crate
boundary; a future split to `fredshell-edit` is possible if the
prompt or completion subsystems need to depend on it directly
without depending on the rest of `fredshell-core`.

## 4. Key decoding

The decoder is a small state machine that consumes bytes and emits
`KeyEvent`s. It is not a full ANSI parser — it only recognizes the
sequences that represent keystrokes, bracketed paste markers, and
focus events. Anything else (e.g., capability-probe responses) is
already handled by PLAN_04 before bytes reach the editor.

### 4.1. `KeyEvent`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub mods: Modifiers,
    /// Present only under kitty keyboard protocol level ≥ 2.
    pub kind: KeyEventKind,
    /// Associated text under kitty protocol level 4. Empty otherwise.
    pub text: SmallString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Char(char),
    Enter,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    F(u8),
    Esc,
    Null,
    Menu,
    KeypadBegin,
    Media(MediaKey),
    Modifier(ModifierKey),    // bare modifier press, kitty L≥3
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyEventKind { Press, Repeat, Release }

bitflags::bitflags! {
    pub struct Modifiers: u8 {
        const SHIFT      = 1 << 0;
        const ALT        = 1 << 1;
        const CTRL       = 1 << 2;
        const SUPER      = 1 << 3;
        const HYPER      = 1 << 4;
        const META       = 1 << 5;
        const CAPS_LOCK  = 1 << 6;
        const NUM_LOCK   = 1 << 7;
    }
}
```

The wide-enough representation supports kitty protocol levels 0–4.
On legacy terminals (no kitty protocol), only `SHIFT`, `ALT`, and
`CTRL` are populated, `kind` is always `Press`, and `text` is empty.

### 4.2. Decoder state machine

States:

- `Ground` — waiting for the first byte.
- `Esc` — saw `0x1B`; waiting to disambiguate Alt-X vs CSI vs SS3.
- `Csi` — accumulating parameters of a `CSI ... <final>` sequence.
- `Ss3` — accumulating an `ESC O ...` sequence.
- `Paste` — between `CSI 200 ~` and `CSI 201 ~`; bytes are paste
  payload, not keys.
- `Utf8(n)` — collecting `n` continuation bytes of a UTF-8 scalar.

Transitions are pure; the state machine is unit-tested by feeding
byte slices and asserting `KeyEvent` outputs.

### 4.3. Kitty keyboard protocol

When `Capabilities::kitty_keyboard` is true and the editor has
issued the level-4 push sequence (via PLAN_03), the decoder
recognizes the extended CSI form:

```text
CSI keycode ; modifiers ; text_codepoints u
```

with `event_type` and associated text. This gives us:

- Disambiguated `Ctrl-I` vs `Tab` and `Ctrl-M` vs `Enter`.
- `Shift-Enter` as a distinct key (so vi-insert can insert a
  literal newline without leaving insert mode).
- Key release events, which the editor ignores in v1 but the
  decoder still emits so future features can use them.

On terminals without the protocol, the disambiguation is
unavailable and the editor falls back to the conventional
mappings.

### 4.4. Bracketed paste

`CSI 200 ~` enters paste mode; bytes are accumulated verbatim
into a paste buffer until `CSI 201 ~`. The editor inserts the
entire payload as a single edit (one undo group), suppresses
highlighting and hints for the duration, and never treats an
embedded `\n` as a submission. PLAN*04 enables bracketed paste
at startup (capability permitting); the editor cannot rely on
the terminal \_not* sending pastes through.

## 5. Buffer model

### 5.0. What the buffer is, and what it is not

This section establishes the load-bearing distinction that drives
every other choice in §5. Getting it wrong leads to either a
freminal-style 1D-to-2D rewrite or a terminal-emulator-shaped
buffer that fights the rest of the shell.

**The buffer stores the in-progress command line, only.**
Specifically:

- The text the user has typed since the prompt was drawn, up to
  Enter.
- Cursor position within that text.
- Selection / mark state.
- Per-line undo and redo entries.
- Vi-mode state (operator pending, count, register).

It does **not** store:

- The prompt itself. PLAN_15 produces it; the editor knows only
  how many columns it occupies on the starting visual row.
- Previously executed commands or their output. Those belong to
  the host terminal emulator (kitty, alacritty, foot, …), which
  fredshell is a guest inside. The host terminal emulator owns the
  screen; we own one command in flight.
- Any "screen" or "viewport" or scrollback concept.

This is the structural difference between a shell line editor and
a terminal emulator. Freminal owns the screen because freminal
_is_ the terminal emulator. Fredshell does not own the screen,
and pretending it does is the route to wrong abstractions.

The empirical consequence: buffer sizes are small. Median
interactive command is ~20–40 characters. 95th percentile is
~150. Pasted heredocs and scripts can be kilobytes, occasionally.
A 50-row buffer is a heavy day; 500 rows is unusual.

#### Storage shape: `Vec<LogicalRow>`, hard-newline split

The buffer is a vector of logical rows, where a row splits on a
hard `\n` only — not on visual wrap.

The alternatives considered:

- **Flat `Vec<TChar>`.** Simpler indexing, single integer cursor.
  Loses on multi-line paste followed by edits in early rows: a
  500-line paste of ~100 chars/line followed by `Ctrl-A` and one
  insertion shifts ~50 000 cells. Rejected.
- **`Vec<LogicalRow>` with soft-wrap segments stored inside
  rows.** Storage tracks visual layout. SIGWINCH, prompt-width
  changes, and width-affecting edits all invalidate stored
  soft-wrap state. The invalidation surface bleeds into every
  consumer of the buffer. Rejected; rationale recorded in §10.
- **Window-shaped `Vec<Line<TChar>>` (freminal's terminal-grid
  model).** Correct for a terminal emulator because the
  application produces in screen-coordinate terms and the
  emulator owns the screen. Wrong for a shell because the user
  produces in logical terms and we do not own the screen.
  Rejected.

The chosen shape — logical rows split on hard `\n`, soft wrap
computed at render — keeps storage independent of window width,
prompt width, and the host terminal emulator's behavior, while
keeping
multi-line-paste edits cheap.

#### Soft wrap: render-only

Soft wrap is a function of:

- Logical row contents (which clusters, with which widths).
- Current terminal width.
- Current first-line indent (prompt width on row 0; zero
  thereafter unless a continuation prompt is present).

All three change over time, the first via edits, the second via
SIGWINCH, the third via prompt updates. None of them belong in
buffer storage. The wrap module (§9.3) computes wrap on demand
during frame construction.

The cost is one wrap-math walk per affected row per redraw. For
shell-sized buffers this is microseconds. The benefit is that
SIGWINCH and prompt updates do not touch the buffer, and edits
do not maintain derived visual state.

### 5.1. TChar (lifted from freminal)

The cell type is `TChar`, a small-string-optimized grapheme
cluster carrying inline display width. It is lifted by-copy
from freminal (`freminal-buffer::TChar`), MIT-to-MIT, and lives
as a module inside `fredshell-core` (`fredshell-core::edit::
buffer::tchar`). It is not a dependency on `freminal-buffer`;
the freminal crate carries terminal-grid concerns (scrollback,
alt-screen, scroll regions, head/continuation cells for
wide-char-on-grid placement) that fredshell does not need.

```rust
/// One grapheme cluster + its display width.
///
/// Lifted from freminal's `TChar`; attribution preserved in the
/// module header. The load-bearing invariant is that one TChar
/// represents exactly one logical cursor position.
pub struct TChar {
    /// UTF-8 bytes of the cluster, inline. Most clusters fit in
    /// 1–4 bytes; combining sequences and ZWJ emoji can be
    /// longer. Heap-allocates only past the inline budget.
    bytes: ClusterBytes,
    /// Display width in cells: 0 (combining / variation
    /// selectors), 1, or 2 (CJK, emoji).
    width: u8,
}

impl TChar {
    pub fn as_str(&self) -> &str { /* ... */ }
    pub fn width(&self) -> u8 { self.width }
}
```

What the lift keeps:

- Inline small-string storage: the steady state is no allocation
  per cluster.
- Grapheme-cluster boundary discipline: a `TChar` is constructed
  only at a UAX #29 boundary.
- Width metadata stored once, at construction, never recomputed.
- The invariant that one `TChar` = one logical cursor position.
  This is what makes `LogicalPos { row, col }` semantically
  clean: `col` is a `TChar` index, not a byte offset and not a
  column count.

What the lift drops:

- SGR / format fields. fredshell stores style on the rendered
  frame, not on the buffer cell. A `TChar` in the buffer carries
  no color or attribute.
- Head / continuation cells. Freminal stores wide characters as
  `(head, continuation)` cell pairs because it places cells onto
  a fixed-width grid. fredshell's buffer is not a grid; a wide
  character is one `TChar` with `width == 2`, no continuation.
- Any reference to terminal-grid coordinates.

### 5.2. Buffer type

```rust
pub struct Buffer {
    /// Logical rows, split on hard '\n' only. Never on visual
    /// wrap. A single empty row is the empty buffer.
    rows: Vec<LogicalRow>,
    /// Cursor as a (row, col) TChar index.
    cursor: LogicalPos,
    /// Selection anchor; equals cursor when no selection.
    anchor: LogicalPos,
    /// Preferred visual column for vertical motion. Set on
    /// horizontal motion or absolute positioning; consumed by
    /// MoveUp / MoveDown so repeated vertical motion preserves
    /// the user's column intent across short intervening lines.
    preferred_column: Option<u16>,
    /// Cached visual cursor position. Derived from `cursor` and
    /// the current wrap context. Invalidated on edit, SIGWINCH,
    /// prompt-width change. Recomputed lazily.
    visual_cursor: VisualCursorCache,
}

pub struct LogicalRow {
    cells: Vec<TChar>,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct LogicalPos {
    pub row: usize,
    pub col: usize,    // TChar index within row
}
```

Cursor invariants:

- `cursor.row < rows.len()`.
- `cursor.col <= rows[cursor.row].cells.len()` (one past last is
  end-of-row, where insertion appends).
- `anchor` follows the same invariants; equals `cursor` when no
  selection.
- The cursor is always at a grapheme-cluster boundary by
  construction (TChar = one cluster).

### 5.3. Edit operations

All edits go through a small set of primitives:

- `insert_tchar(pos, tc)` and `insert_str(pos, &str)` —
  segments the input into TChars at construction and splices
  into the affected row. Multi-line input splits the row at
  `pos`, inserts whole new rows for the middle, and joins the
  tail.
- `delete_range(start, end)` — half-open in `LogicalPos`.
  Within-row deletion is `Vec::drain` on the row's cells.
  Cross-row deletion removes whole intermediate rows and joins
  the partial start/end rows.
- `replace_range(start, end, &str)` — fused delete + insert.

Every edit:

1. Updates `rows`.
2. Updates `cursor` and `anchor` if they are inside or after the
   affected range.
3. Invalidates `visual_cursor` and any cached row layouts (§9.3)
   for the affected rows.
4. Pushes an undo record.

Edits never touch wrap state, because there is none in storage.

### 5.4. Grapheme segmentation

`unicode-segmentation` (UAX #29) provides cluster boundaries at
TChar construction. Segmenting a 200-character paste is well
under 100 µs; segmenting individual keystrokes is single-digit
microseconds.

### 5.5. Cell width

Width is determined at TChar construction and stored inline. The
width table is fredshell-owned, derived from the Unicode property
database and adjusted for the modern terminal consensus on:

- Emoji presentation: `U+1F600`-style emoji are width 2 regardless
  of presentation selector, matching kitty / WezTerm / iTerm2 /
  foot.
- Variation selectors: width 0; do not advance the cursor.
- ZWJ sequences: the whole cluster is width 2 (or the width of the
  base emoji), not the sum of parts.
- East Asian Ambiguous: width 1. Terminals that render ambiguous
  characters as width 2 are non-conforming for the modern
  consensus; fredshell prioritizes the larger user base.
- Combining marks: width 0.
- Control characters: width 0 (they should not appear in the
  buffer; if they do, they render as `^X` with width 2 via an
  explicit escape mechanism, but the underlying TChar still
  records the original cluster).

The width table is built into a static array indexed by codepoint
(generation-vs-vendoring decision in §13). Cluster width is the
max width of any codepoint in the cluster, with the exceptions
above for ZWJ sequences.

A future revision may make the table configurable for users with
older terminals; v1 commits to the modern consensus.

### 5.6. Cursor and selection semantics

- The cursor is always at a TChar boundary (i.e., a grapheme
  boundary), enforced by the type.
- A selection is `[min(cursor, anchor), max(cursor, anchor))`,
  half-open in `LogicalPos` order (row-major, then col).
- Operations like "delete-word-backward" are computed on the
  TChar stream within and across rows, and respect shell-aware
  word boundaries (§6.4).

### 5.7. Future extraction

`fredshell-core::edit::buffer` is a candidate for extraction to
its own crate (e.g., `fredshell-edit`) when a separate
line-editor consumer emerges (a standalone `fredshell-readline`
helper, a syntax-highlighter binary, …). For v1 the module stays
inside `fredshell-core` to avoid a premature crate boundary.

## 6. Keymap and commands

### 6.1. `EditCommand`

A closed enum of every primitive the editor supports. Roughly 120
variants in v1; the categories:

- **Cursor motion.** `MoveCharLeft`, `MoveCharRight`,
  `MoveWordLeft` (shell-aware), `MoveBigWordLeft` (whitespace),
  `MoveLineStart`, `MoveLineEnd`, `MoveBufferStart`,
  `MoveBufferEnd`, `MoveToColumn(usize)`, `MoveUp`, `MoveDown`,
  `MoveToMatchingBracket`.
- **Edit.** `InsertChar(char)`, `InsertText(SmallString)`,
  `DeleteCharLeft`, `DeleteCharRight`, `DeleteWordLeft`,
  `DeleteWordRight`, `DeleteToLineEnd`, `DeleteToLineStart`,
  `DeleteSelection`, `TransposeChars`, `TransposeWords`,
  `UppercaseWord`, `LowercaseWord`, `CapitalizeWord`.
- **Selection / region.** `SetMark`, `ClearMark`, `SwapPointMark`,
  `SelectAll`, `SelectWord`, `SelectLine`.
- **Kill ring.** `Kill(Region)`, `Yank`, `YankPop`, `Copy(Region)`.
- **Undo.** `Undo`, `Redo`.
- **History.** `HistoryPrev`, `HistoryNext`, `HistorySearchBack`
  (Ctrl-R overlay), `HistoryFuzzy` (fzf overlay), `HistoryAccept`.
- **Completion.** `CompleteOrCycle`, `CompleteCancel`,
  `CompleteAccept`, `CompletePartial`.
- **Submission.** `Submit`, `SubmitForce` (skip continuation
  check), `CancelLine` (Ctrl-C inside the editor).
- **Mode.** `EnterViNormal`, `EnterViInsert`, `EnterViVisual`,
  `EnterEmacs` (the latter mostly for vi-mode `:set imap`-style
  toggles).
- **External.** `EditInExternalEditor`, `ExpandHistory`,
  `InsertLastArgument` (`Alt-.`).
- **Vi operators.** `ViOperator(Op)` where `Op` is `Delete`,
  `Change`, `Yank`, `ToUpper`, `ToLower`, `Filter`. Operators
  combine with motions and text objects through the vi-mode
  dispatcher (§6.3), not through direct keymap entries.

Adding a new primitive means adding a variant here. Users cannot;
this is intentional (tenet 7).

### 6.2. `Keymap`

```rust
pub trait Keymap {
    /// Feed a key. Returns Some(commands) when the keymap has
    /// resolved to a (possibly empty) sequence of EditCommands.
    /// Returns None when the keymap is mid-chord and waiting for
    /// more input.
    fn feed(&mut self, key: KeyEvent) -> Option<Vec<EditCommand>>;

    /// Cancel an in-progress chord (e.g., on focus loss).
    fn reset(&mut self);

    /// Snapshot of the current mode for the prompt to render
    /// (vi-normal, vi-insert, emacs).
    fn mode(&self) -> EditorMode;
}
```

The chord resolver is a trie over `KeyEvent` sequences. Built once
per keymap construction; the runtime cost is a hash lookup per
key.

### 6.3. Vi mode dispatcher

Vi is not a flat keymap. It is a small parser:

```text
operator? count? motion-or-text-object
```

The dispatcher accumulates `Count` and `Operator` state, then
expects a motion or text object. Motions reuse the cursor-motion
`EditCommand`s; text objects (`iw`, `aw`, `ip`, `ap`, `i"`, `a"`,
…) are recognized in a small sub-parser.

Registers (`"ay`, `"ap`, etc.) are an `Option<char>` prefix on the
state. The `.` repeat stores the last operator+motion+count
triple and replays it.

Insert mode is a flat keymap with `Esc` → vi-normal. Visual mode
is vi-normal with the selection anchored.

### 6.4. Shell-aware word boundaries

`MoveWordLeft` in emacs and `b` in vi-normal both use the same
notion of "word." That notion is:

- A _small word_ is a maximal run of characters in one of three
  classes:
  - Alphanumeric + `_`
  - Shell metacharacters (`|`, `&`, `;`, `(`, `)`, `<`, `>`, etc.)
  - Whitespace (skipped between words)
- A _big word_ (`B`/`W` in vi) is a maximal run of non-whitespace.

This matches bash readline's `vi-fwd-word` / `vi-bwd-word` more
closely than emacs's default, which is alphanumeric-only. Users
who want strict alphanumeric word boundaries can rebind to
`MoveAlphaWordLeft`.

## 7. Completion seam

PLAN*07 owns \_how completion appears*; PLAN*09 owns \_what is
offered*. The seam:

```rust
pub trait CompletionProvider {
    /// Called when a completion is triggered. Receives a snapshot
    /// of the buffer and cursor position. Returns candidates
    /// with metadata for menu rendering.
    fn complete(&self, ctx: CompletionContext<'_>)
        -> Vec<CompletionCandidate>;
}

pub struct CompletionContext<'a> {
    pub buffer: &'a str,
    pub cursor_byte: usize,
    pub word_start_byte: usize,
    pub word: &'a str,
    /// The parsed command preceding the word, if available.
    /// Allows command-specific completion (e.g., `git checkout
    /// <TAB>` knows we are completing a branch).
    pub command: Option<&'a ParsedCommand>,
}

pub struct CompletionCandidate {
    pub replacement: String,
    pub display: String,
    pub description: Option<String>,
    pub kind: CandidateKind,   // file, dir, command, var, history, …
    pub score: f32,            // for fuzzy ranking
}
```

The editor handles trigger (Tab, configurable), menu display
(columnar list or fzf-style overlay, per config), partial
completion (insert the longest common prefix), and accept/cancel.
The provider does not see the menu; the editor does not see the
candidate-generation logic.

This seam lets PLAN_06 be drafted independently (Phase B). For v1
of the editor, a stub provider returning filename completions
keeps the editor functional while PLAN_06 is being designed.

## 8. History

### 8.1. Bash-compatible semantics

- `HISTFILE` — path, default `$XDG_STATE_HOME/fredshell/history`.
- `HISTSIZE` — in-memory entry count, default 10000.
- `HISTFILESIZE` — on-disk entry count, default 10000.
- `HISTCONTROL` — set of `ignorespace`, `ignoredups`, `ignoreboth`,
  `erasedups`.
- `HISTIGNORE` — colon-separated patterns; entries matching any
  pattern are not stored.
- `histappend` — append vs overwrite on shell exit.
- `HISTTIMEFORMAT` — strftime format for `history` builtin output.

### 8.2. On-disk format

Bash's format: one entry per line, optionally preceded by a
timestamp comment `#1234567890`. Multi-line entries use backslash
continuation. fredshell reads and writes this format so users can
share `HISTFILE` between fredshell and bash.

A sidecar file (`history.fredshell`) stores fredshell-specific
metadata (exit code, working directory, duration) keyed by entry
hash. Bash ignores the sidecar; fredshell uses it for richer
history search.

### 8.3. Concurrent writes

Multiple shells writing the same `HISTFILE` is the common case
(tmux panes). The write path:

1. Append the entry to an in-memory ring.
2. On shell exit (or `history -a`), open `HISTFILE` with
   `O_APPEND`, write entries added since the last sync, close.
3. `O_APPEND` makes concurrent appends atomic for entries under
   `PIPE_BUF` (4096 bytes on Linux); longer entries are wrapped
   in a `flock` advisory lock.

This matches bash's behavior closely enough that interleaving
fredshell and bash sessions in the same `HISTFILE` does not
corrupt it.

### 8.4. History expansion

The `!` syntax (`!!`, `!$`, `!^`, `!*`, `!N`, `!-N`, `!string`,
`!?string?`, `!:N`, `!:N-M`, modifiers like `:h`, `:t`, `:r`, `:e`,
`:s/old/new/`) is expanded _before_ the line is submitted, so the
user can confirm. Expansion happens on `Enter` if `histexpand` is
on (default in interactive mode); if expansion changes the line,
the new line is shown and the user presses Enter again to submit.
This matches bash's `histverify` set option.

### 8.5. Search

- **Substring (`Ctrl-R`).** Overlay showing the most recent match
  for the query; `Ctrl-R` again steps backward, `Ctrl-S` forward,
  `Enter` accepts, `Esc` cancels.
- **Prefix (up-arrow with partial input).** Bash's
  `history-search-backward` behavior.
- **Fuzzy (`Ctrl-T` default).** fzf-style overlay with score-based
  ranking; uses `fuzzy-matcher` crate or an in-house equivalent
  (decided at implementation time).

### 8.6. The `history` and `fc` builtins

PLAN_14 owns the history store. The two bash builtins that read
or mutate it — `history` and `fc` — therefore live in
`fredshell-core::builtins::history` and `::fc` but are
implemented by calling into the PLAN_14 history API rather than
duplicating storage. PLAN_06 (builtin dispatch) is responsible
only for routing the builtin name to the entry point; the
semantics are owned here.

**`history` builtin.** Bash-compatible subset for v1:

- `history` — print the in-memory ring with line numbers; honour
  `HISTTIMEFORMAT`.
- `history N` — print only the last N entries.
- `history -c` — clear the in-memory ring (does not touch
  `HISTFILE`).
- `history -d offset[-end]` — delete entry or range.
- `history -a` — append entries added in this session to
  `HISTFILE` (the §8.3 sync path).
- `history -r` — read `HISTFILE` and append to the in-memory
  ring.
- `history -w` — overwrite `HISTFILE` with the in-memory ring.
- `history -n` — read entries from `HISTFILE` that have not been
  read yet.
- `history -p arg ...` — perform history expansion on each `arg`
  without storing the result; print expansions to stdout.
- `history -s arg ...` — push `arg` onto the in-memory ring
  without executing it.

Deferred to v1.1: `-S`/`-L` (synonyms for `-w`/`-r` in some
distributions), bash's undocumented buffer-flushing modes.

**`fc` builtin.** POSIX `fc` semantics:

- `fc [-r] [-e editor] [first [last]]` — open the range
  `[first, last]` in `$EDITOR` (or `-e editor`); on save, the
  edited content is executed as if typed at the prompt. `-r`
  reverses the order.
- `fc -l [-nr] [first [last]]` — list the range (no editor); `-n`
  suppresses line numbers, `-r` reverses.
- `fc -s [pat=rep] [cmd]` — re-execute `cmd` (or the most recent
  command) optionally with a single `pat=rep` substitution. This
  is the "quick re-run" mode.

Both builtins refuse cleanly (PLAN_05 refusal contract, PLAN_07
spec sheet) when invoked from a non-interactive context with no
`HISTFILE`. They are not Tier-2 — they are first-class builtins.

The implementation seam is a single trait in `fredshell-core`:

```rust
pub trait HistoryStore {
    fn entries(&self) -> &[HistoryEntry];
    fn append(&mut self, entry: HistoryEntry);
    fn clear(&mut self);
    fn delete(&mut self, range: RangeInclusive<usize>);
    fn sync(&mut self, mode: SyncMode) -> Result<(), HistoryError>;
    fn read(&mut self) -> Result<(), HistoryError>;
    fn write(&mut self) -> Result<(), HistoryError>;
}
```

PLAN_14 provides the concrete implementation; the builtin code
holds a `&mut dyn HistoryStore`. In the cooked-mode scaffold
(§11.1) the store is a stub that records entries to an in-memory
`Vec` and never touches disk.

## 9. Highlight, hints, and the redraw loop

### 9.1. Highlighter

```rust
pub trait Highlighter {
    /// Given a buffer, produce a sequence of styled spans.
    /// Spans must cover the buffer exactly; no gaps, no overlaps.
    fn highlight(&self, buffer: &str) -> Vec<StyledSpan>;
}

pub struct StyledSpan {
    pub byte_range: Range<usize>,
    pub style: Sgr,    // from PLAN_03
}
```

The default highlighter is parser-driven (PLAN_06). It accepts
incomplete input and produces "best-effort" spans — unterminated
strings are still styled as strings, unmatched brackets are
flagged.

The parser must be incremental enough that re-highlighting a line
on every keystroke fits the latency budget. Implementation note:
the parser caches token boundaries; insertions invalidate from
the affected token forward, not the entire line.

### 9.2. Hinter

```rust
pub trait Hinter {
    /// Given a buffer and cursor, return ghost text to display
    /// after the cursor. Empty string for no hint.
    fn hint(&self, buffer: &str, cursor: usize) -> SmallString;
}
```

The default hinter searches recent history for the most recent
entry that starts with the current buffer; the suffix is the hint.
`Alt-F` accepts one word; `Right` at end-of-line accepts the
whole hint.

Hints are recomputed on every keystroke. The history-based hinter
caches the last query result; a single-character insertion only
checks whether the cached hint still extends the buffer.

### 9.3. Frame model, wrap, and diff

The frame is a 2D grid built fresh per redraw from the buffer
(§5), the prompt (PLAN_15), the hint (§9.2), and the completion
menu (§7). It is never persisted between redraws as the source
of truth — the buffer is.

```rust
pub struct Frame {
    pub rows: Vec<FrameRow>,
    pub cursor: Option<(u16, u16)>,    // (row, col) in frame coords
    pub cursor_visible: bool,
}

pub struct FrameRow {
    pub cells: Vec<FrameCell>,
}

pub struct FrameCell {
    /// The cluster occupying this cell, by reference into the
    /// buffer or the prompt. Empty for the trailing half of a
    /// width-2 cluster (the renderer skips it; the diff treats
    /// it as a co-occupant of the head cell).
    pub cluster: FrameClusterRef,
    /// Style applied at this cell. Computed per-redraw from the
    /// highlighter, cursor, selection, hint, and menu state.
    /// Never persisted in the buffer.
    pub style: Sgr,
}
```

#### Wrap module

The wrap module owns the math that translates `LogicalRow`s into
visual rows for a given width and first-line indent. It is the
single concentrated place where "soft-wrap" exists; everything
else in the editor sees only logical positions.

```rust
pub struct WrapContext {
    pub width: u16,
    /// Columns occupied by the prompt on the first visual row of
    /// the buffer. Subsequent visual rows of the same logical
    /// row get a continuation indent (often zero).
    pub first_line_indent: u16,
    pub continuation_indent: u16,
}

pub struct RowLayout {
    /// One entry per visual line of this logical row. Empty rows
    /// have one entry covering [0, 0) to keep the cursor placeable.
    pub slices: Vec<VisualSlice>,
}

pub struct VisualSlice {
    /// TChar index range within the logical row.
    pub start_cell: usize,
    pub end_cell: usize,
    /// Sum of cell widths in the slice. May be < width when the
    /// next cluster is width 2 and would have straddled the wrap
    /// boundary; the boundary cell is left visually empty.
    pub visual_width: u16,
    /// Indent applied to this visual slice (first_line_indent for
    /// the first slice of row 0, continuation_indent thereafter).
    pub indent: u16,
}

/// Pure function: row + context → layout. No state, no caching.
pub fn wrap_row(row: &LogicalRow, ctx: &WrapContext, is_first_row: bool)
    -> RowLayout;
```

Wrap rules:

- Widths are summed per cluster, not per cell. A width-2 cluster
  that would cross the wrap boundary is placed entirely on the
  next visual line; the boundary cell on the previous line is
  marked visually empty (matches kitty / xterm convention).
- Width-0 clusters (combining, variation selectors) attach to
  the preceding non-zero cluster's slice; they never start a new
  visual slice.
- A logical row with no clusters produces one visual slice of
  width 0 so the cursor has somewhere to sit.

#### Visual cursor cache

```rust
pub struct VisualCursorCache {
    /// `Some` when the cache is valid for the current
    /// (cursor, wrap context) pair.
    pos: Option<VisualPos>,
}

pub struct VisualPos {
    /// Visual row offset from the start of the buffer's draw
    /// region (zero = the first visual row of the prompt).
    pub vis_row: u16,
    /// Column within that visual row, including any indent.
    pub vis_col: u16,
}
```

Invalidation:

- Any edit invalidates the cache.
- SIGWINCH invalidates the cache (and any cached `RowLayout`s).
- Prompt-width change invalidates the cache (and the row-0
  layout).
- Cursor motion that does not edit the buffer also invalidates
  the cache, because the cursor moved; the cache is then
  recomputed on the next read.

Recomputing the cache walks rows 0 through `cursor.row` calling
`wrap_row` for each. For shell-sized buffers this is microseconds.
Per-row layouts may be memoized inside `Buffer` keyed by row
identity + wrap context if profiling demands it; v1 does not
memoize.

#### Vertical motion (Up / Down)

```rust
fn move_up(buffer: &mut Buffer, ctx: &WrapContext) {
    let cur_vis = buffer.visual_cursor(ctx);    // logical → visual
    if cur_vis.vis_row == 0 { return; }
    let target_vis = VisualPos {
        vis_row: cur_vis.vis_row - 1,
        vis_col: buffer.preferred_column.unwrap_or(cur_vis.vis_col),
    };
    buffer.cursor = visual_to_logical(buffer, ctx, target_vis);
    // preferred_column is preserved across repeated MoveUp/Down.
}
```

`visual_to_logical` walks rows from 0 accumulating visual-row
counts via `wrap_row` until it finds the row containing the
target visual row, then locates the cluster at the target visual
column (clamping to end-of-visual-line if the column is past the
last cluster). Pure function over `(buffer, ctx, target_vis)`;
fully unit-testable without a terminal.

The same translation pair (`logical_to_visual`,
`visual_to_logical`) is used by mouse-click positioning
(if/when added), by `MoveToColumn` semantics, and by any future
visual-line-based motion.

#### Frame construction

Each redraw:

1. Build the prompt's `FrameRow`s (PLAN_15).
2. For each logical row of the buffer, call `wrap_row` against
   the current `WrapContext`. Materialize each `VisualSlice` as
   a `FrameRow`, applying highlighter spans, selection style,
   hint style for any trailing ghost text, and indent.
3. Append menu rows if a completion menu is open.
4. Compute the cursor's frame coords from the visual cursor cache.
5. Diff against the previous `Frame` row-by-row.
6. For each changed row, emit cursor positioning + the
   minimum-length sequence of SGR + text.
7. Wrap the whole emission in synchronized-output begin/end if
   `Capabilities::synchronized_output` is true.

The diff is the hot path. It must be allocation-free for the
common case (cursor moved, no buffer change → empty diff except
for the cursor-position emit). Benchmarks (Criterion, per
PLAN_05) cover: single-character insertion, full-line
replacement, paste of 4 KiB, vertical motion within a wrapped
row, window resize.

### 9.4. Resize

SIGWINCH is delivered to PLAN_04, which updates `WindowSize` and
wakes the main loop. The editor receives a synthetic
`Resize(new_size)` event and:

1. Updates the `WrapContext` width.
2. Invalidates the visual cursor cache and any cached
   `RowLayout`s.
3. Invalidates the previous `Frame` (geometry changed; row-by-row
   diff is meaningless).
4. Performs a full redraw against the new `WrapContext`.

The buffer itself is untouched. No reflow, no rejoin, no
re-segmentation. The only state changing is the wrap context and
the derived caches.

Prompt-width changes (right-prompt updating, git-branch refresh)
take the same path scoped to row 0's layout.

### 9.5. Terminal yield primitive

PLAN_13 (traps and jobs) needs a way to hand the controlling
terminal to a foreground external process — for example, when
the user runs `vim` from the prompt, or when a `trap '...' DEBUG`
handler shells out — without the editor's raw-mode state, kitty
keyboard protocol negotiation, or partial frame state being
visible to that child.

The primitive is a single method on the editor:

```rust
impl Editor {
    /// Yield the controlling terminal to a child process for the
    /// duration of `f`. The editor:
    ///
    /// 1. Flushes any pending output.
    /// 2. Saves cursor position and disables raw mode.
    /// 3. Disables kitty keyboard protocol (level 0).
    /// 4. Disables bracketed paste.
    /// 5. Calls `f`; the child sees a vanilla cooked TTY.
    /// 6. On return, re-enables (4), (3), (2), in that order.
    /// 7. Invalidates the frame cache; the next redraw is full.
    ///
    /// `f` must not retain references to the editor; the editor
    /// is logically suspended for its duration.
    pub fn yield_terminal<R>(
        &mut self,
        f: impl FnOnce() -> R,
    ) -> Result<R, EditorError>;
}
```

The name `yield_terminal` is deliberately not
`yield_for_one_line`; the primitive is more general than the
PLAN_13 use case. PLAN_13 calls it with a closure that spawns a
single child and waits; external-editor integration (§1) calls
it with a closure that invokes `$EDITOR`; future job-control
work will call it with a closure that does `tcsetpgrp` /
`waitpid` directly.

Three invariants the primitive guarantees and the tests in §12
must lock down:

1. **State restoration is total.** After `yield_terminal`
   returns, the editor is in exactly the same observable state
   it was in before, except for buffer contents that `f`
   intentionally modified (e.g., `$EDITOR` re-import).
2. **No keystrokes are lost.** Any bytes that arrived on stdin
   between (2) and (3) of the suspend path, or between the
   inverse steps of the resume path, are queued and re-decoded
   on resume. The PLAN_04 input ring owns the queue.
3. **`SIGWINCH` during yield is honoured.** PLAN_04 still
   delivers `WindowSize` updates while `f` runs. On resume the
   editor reads the current window size and recomputes layout
   before redrawing.

Failure modes:

- If `f` panics, the editor's `Drop` impl runs the resume path
  unconditionally; the panic propagates afterwards. The editor
  must not leave the terminal in raw mode if the shell process
  is about to die.
- If the resume path itself fails (e.g., terminal disappeared),
  `yield_terminal` returns `EditorError::TerminalLost` and the
  caller is expected to exit the shell.

This primitive is referenced by PLAN_13 §11 subtask 10.6
(trap handler execution) and answers PLAN_13 open question
Q10.5.

## 10. Why not reedline

`reedline` is a fine line editor for nushell. It is the wrong
foundation for fredshell. Recording the reasons here so we do not
relitigate them.

1. **Crossterm conflict.** Reedline owns termios and signal
   handling through `crossterm`. PLAN_04 owns the same resources
   directly. The two cannot share an `/dev/tty` cleanly; making
   them coexist requires a shim that intercepts crossterm's
   syscalls.
2. **Redraw opacity.** Reedline owns the screen between prompt
   prints. Our <1 ms keystroke budget cannot be enforced through
   its abstraction; if a particular terminal triggers a slow
   path in reedline's redraw, we cannot fix it without forking.
3. **Highlighter contract mismatch.** Reedline calls
   `Highlighter::highlight(&str) -> StyledText` on every keystroke
   over the _entire_ buffer. fredshell's parser is built to be
   incremental; the contract throws that away.
4. **History semantics gap.** Reedline's history is its own
   abstraction; bash semantics (`HISTCONTROL`, `HISTIGNORE`,
   `histappend`, history expansion) sit awkwardly on top of it.
5. **Keymap closure.** Reedline's `EditCommand` enum is closed at
   the reedline crate. Adding fredshell-specific primitives
   (shell-aware word boundaries, history-expansion preview)
   requires either patching reedline or working around its
   abstraction.
6. **Completion menu rigidity.** fzf-style completion is not a
   first-class menu in reedline. Building it inside reedline means
   bypassing reedline's menu system; at that point we are doing
   reedline's job ourselves.
7. **Kitty keyboard ceiling.** Reedline depends on crossterm for
   key decoding; crossterm's kitty-protocol support trails the
   spec by a wide margin. We want full level-4 support from day
   one.
8. **Upstream cadence.** Reedline tracks nushell's release cycle.
   We do not control it. Breaking changes in reedline become
   maintenance work for fredshell.

The middle path — adopt reedline behind a trait, replace later —
was considered and rejected. Building an adapter is real work; the
cut-over to a custom editor is never as clean as the plan
suggests; and the time saved up front (4–6 months) is paid back
later in adapter maintenance and migration. We sign up for the
full editor now and stop pretending the shortcut exists.

### 10.1. Considered and rejected: `freminal-buffer` crate dependency

Freminal's `freminal-buffer` crate is a mature, battle-tested
terminal-grid model, MIT-licensed, with the same author as
fredshell. The natural question: depend on it for the line-editor
buffer.

Reviewed and rejected. The crate is shaped for a terminal
emulator, which has different responsibilities than a shell line
editor:

- Scrollback storage and pruning. Not applicable; the shell line
  editor stores one command in flight.
- Alt-screen / primary-screen switching. Not applicable.
- Scroll regions, DECOM, DECLRMM, and the rest of the
  terminal-grid escape menagerie. Not applicable.
- Image storage and placement. Not applicable.
- `(head, continuation)` cell pairs for placing wide characters
  on a fixed-width grid. Not applicable; the shell buffer is not
  a grid.
- `flatten` and other operations that exist to translate the
  internal grid to a renderer-friendly form. Not applicable; the
  shell editor's renderer reads directly from logical rows.

What does transfer is the `TChar` type and its invariants. Those
are lifted by-copy (§5.1), MIT-to-MIT, with attribution in the
module header. No crate dependency.

### 10.2. Considered and rejected: flat `Vec<TChar>` storage

A flat buffer with a single integer cursor was considered. It is
simpler — one index type, slice operations work directly, no
nested iteration.

Rejected on the multi-line-paste case. A 500-line paste of
~100 chars per line followed by `Ctrl-A` and one insertion shifts
~50 000 cells. A row-shaped buffer shifts only the cells of the
first row (~100). The asymmetry is fundamental: edits in early
rows of a multi-line buffer should not pay for the size of late
rows. See §5.0.

The cost of the row shape — `LogicalPos { row, col }` instead of
a single integer, and a nested iterator for full-buffer walks —
is bounded and concentrated. The cost of the flat shape is
unbounded in buffer size and hits a real user flow. Row shape
wins.

### 10.3. Considered and rejected: soft wrap stored in the buffer

A variant of the row-shaped storage that splits rows on _both_
hard `\n` and soft wrap was considered. It would make per-redraw
frame construction cheaper (the visual layout is already known)
and would make vertical motion a direct row-index decrement
instead of a wrap-math computation.

Rejected. Storing soft-wrap state means storing a function of:

- Buffer contents (which clusters, at which widths).
- Terminal width (changes on SIGWINCH).
- Prompt width on row 0 (changes whenever the prompt updates,
  which can happen asynchronously — git status, time, …).

Every change to any input invalidates stored state. The
invalidation surface bleeds into every consumer of the buffer:
parser spans, undo log, kill ring, selection anchor,
completion-trigger position, search-match positions. Every
consumer becomes "soft-wrap-aware" or has to be passed a joined
view that re-walks the buffer.

Freminal stores soft wrap because the application producing the
data writes in screen-coordinate terms (the PTY writes one row
at a time, naturally creating soft-wrap boundaries at write
time) and because the consumers of the buffer are all the
GUI-painter, which wants screen coordinates anyway. Both
conditions are inverted for a shell.

The render-only soft-wrap approach concentrates wrap math in one
place (the `wrap` module, §9.3) and pays a microsecond-scale
cost per redraw on a small buffer. The "store it" approach
trades that cost for a much larger and more error-prone
invalidation surface. Render-only wins.

## 11. Implementation phasing

The editor is a 6–9 month subsystem at the assumed staffing
level. The phasing below ensures fredshell has a _runnable_
interactive shell at every checkpoint, even when the editor is
incomplete, so the rest of the codebase can be developed in
parallel.

### 11.1. Phase 0 — Cooked-mode scaffold (week 1)

Before any of the real editor lands, `fredshell-core::edit::
scaffold` provides a stand-in: cooked-mode `read_line`-style input
with no editing, no history, no completion. Enter submits. Ctrl-C
cancels via the existing PLAN_04 cancellation token. Ctrl-D
sends EOF.

This is enough to type `ls | grep foo` and see the rest of the
shell run. It is explicitly not a line editor; it exists so the
parser, exec, builtins, and prompt can be exercised end-to-end
while the real editor is being built.

The scaffold is deleted at the end of Phase 4. It is not a
fallback; once the real editor ships, it is the editor.

### 11.2. Phase 1 — Minimum viable editor (weeks 2–8)

- Key decoding (legacy + bracketed paste; kitty protocol stubbed).
- Buffer model with grapheme correctness, single-line only.
- Emacs keymap with the 30 most common bindings.
- Diff-based redraw.
- History storage and `Ctrl-R` substring search.
- Stub `Highlighter` and `Hinter` (no-op).

At the end of Phase 1, the shell is usable for daily work for
emacs-keybinding users on a single-line basis. Multiline,
vi mode, hints, highlighting, and fzf are not yet present.

### 11.3. Phase 2 — Multiline, vi mode, highlight (weeks 9–18)

- Multiline buffer + intelligent Enter (requires PLAN_06 parser
  to report "complete" vs "continuation").
- Vi normal/insert/visual + operators + text objects + counts +
  registers + `.` repeat.
- Parser-driven highlighter.
- Continuation prompts (coordinated with PLAN_15).

At the end of Phase 2, the editor is competitive with bash + vi
mode.

### 11.4. Phase 3 — Hints, completion menu, fzf (weeks 19–26)

- History-based hinter.
- Completion menu (columnar + fzf overlay), wired to a stub
  provider until PLAN_06 lands.
- History expansion (`!!`, `!$`, etc.) with `histverify` preview.

At the end of Phase 3, the editor is competitive with fish for
hints and zsh-with-fzf for completion.

### 11.5. Phase 4 — Kitty protocol, polish, scaffold removal (weeks 27–36)

- Kitty keyboard protocol levels 1–4.
- External-editor integration.
- Kill ring, undo/redo ring, region.
- All remaining `EditCommand` variants.
- Performance pass against PLAN_05 benchmarks.
- Delete the cooked-mode scaffold.

At the end of Phase 4, the editor is feature-complete for v1.

### 11.6. Risk and slip

The phases above assume one engineer on the editor full-time. The
schedule will slip. The structure is designed so that slip in
later phases does not block earlier phases from shipping; users
can run fredshell with a Phase 2 editor for months while Phases
3 and 4 finish.

If implementation reveals that Unicode width or the redraw diff
is harder than estimated (the two most likely sources of slip),
the schedule reflows. The phases themselves are sequential
because each builds on the previous; there is no parallelism to
exploit within the editor itself.

## 12. Testing strategy

The buffer model and the logical/visual translation are the two
load-bearing pieces of the editor. They must be tested
exhaustively. "Comprehensive unit testing — every scenario
possible" is an explicit requirement, not a nice-to-have. A
cursor mismatch between fredshell's internal position and the
host terminal emulator's screen position is a correctness bug
that corrupts
every subsequent edit.

The testing matrix below is mandatory before the editor ships.
Per PLAN_05 the tests live alongside the modules; this section
enumerates the coverage classes.

### 12.1. Buffer-edit coverage

For each edit primitive (`insert_tchar`, `insert_str`,
`delete_range`, `replace_range`):

- Empty buffer.
- Single-row buffer; edit at start, mid, end.
- Multi-row buffer; edit within first row, within last row,
  within an interior row.
- Edit straddling row boundaries (forward and backward).
- Edit that introduces new `\n`s (single, multiple,
  consecutive).
- Edit that removes `\n`s (joining rows).
- Edit at the cursor; edit before the cursor; edit after the
  cursor; edit containing the cursor.
- Edit at the anchor (with a non-empty selection).
- Edit at every interesting position relative to a width-2
  cluster: before, after, inside the trailing visual half.
- Edit at every interesting position relative to a combining
  sequence: before the base, between base and combining mark,
  after the combining mark.
- Edit at every interesting position relative to a ZWJ emoji
  cluster.

Each case asserts: row count, per-row cell contents, cursor
position, anchor position, undo-record contents.

### 12.2. Cursor-motion coverage

For each motion command (`MoveCharLeft/Right`,
`MoveWordLeft/Right`, `MoveBigWordLeft/Right`,
`MoveLineStart/End`, `MoveBufferStart/End`, `MoveUp`, `MoveDown`,
`MoveToColumn`, vi text objects `iw`/`aw`/`ip`/`ap`/`i"`/`a"`/
`i(`/`a(`/`i{`/`a{`/`i[`/`a[`):

- Empty buffer.
- Single-character buffer.
- At every boundary: row start, row end, buffer start, buffer
  end, before/after each cluster class transition.
- Across rows (where the motion crosses).
- Through soft-wrapped rows of every wrap width from 1 upward,
  with content of every width pattern (all width-1, all width-2,
  alternating, combining sequences).
- With a non-trivial `preferred_column` set by prior motion,
  consumed by `MoveUp`/`MoveDown`.
- Repeated motion until it becomes a no-op (proves the motion
  has a stable terminus).

### 12.3. Logical/visual translation coverage

The translation pair (`logical_to_visual`, `visual_to_logical`)
is the most error-prone code in the editor. Coverage:

- Every wrap width from 1 to 200, including pathological 1-col
  and 2-col cases where width-2 clusters cannot be placed.
- Buffers consisting entirely of width-1 clusters; entirely
  width-2; mixed.
- Buffers with width-0 clusters (combining marks, variation
  selectors).
- Buffers with empty rows (consecutive `\n`s).
- First-line indent of 0; non-zero (prompt occupies columns);
  full-width indent (degenerate case).
- Logical positions at: row start, row end, mid-row, mid-cluster
  (rejected at the type level — TChar is the unit), at the
  trailing one-past-end position.
- Visual positions: at start of visual line, end of visual line,
  past end of visual line (clamped), at the trailing visually
  empty cell of a wrap-boundary skip.

### 12.4. Round-trip properties (proptest)

Property 1 — logical-to-visual round trip:

```text
For any reachable LogicalPos pos and any WrapContext ctx,
visual_to_logical(ctx, logical_to_visual(buf, ctx, pos)) == pos
```

Property 2 — visual-to-logical round trip:

```text
For any reachable VisualPos vis and any WrapContext ctx,
logical_to_visual(buf, ctx, visual_to_logical(buf, ctx, vis)) == vis'
where vis' may differ from vis only in vis_col when the original
vis_col was past the end of its visual line (clamping is allowed
and explicit).
```

Property 3 — edit / redraw equivalence:

```text
For any sequence of edits applied to an empty buffer, the
resulting frame equals the frame produced by constructing the
final buffer state directly (no edit history).
```

Property 4 — buffer never observes window state:

```text
For any sequence of edits and any sequence of WrapContexts, the
final Buffer (rows, cursor, anchor) is identical regardless of
the WrapContexts encountered along the way. Only the
visual_cursor cache differs.
```

Property 5 — paste idempotence:

```text
Insert N characters one at a time at position P == insert N
characters in one paste at position P (modulo undo-record
shape).
```

Generators are biased toward Unicode-interesting inputs:
combining sequences, ZWJ emoji, CJK, RTL, surrogate pairs in
UTF-16 source (rejected at the type level), zero-width
characters.

### 12.5. Resize and prompt-change coverage

- SIGWINCH from every width to every other width (sample), with
  every cursor position.
- Prompt-width change while cursor is in row 0; while cursor is
  in row N>0 (must not affect anything visible).
- Repeated SIGWINCH events without intervening edits (must be
  idempotent).
- SIGWINCH during an active selection (selection endpoints are
  logical, must survive).
- SIGWINCH during an open completion menu (menu repositions but
  contents stable).

### 12.6. Other unit / snapshot / property tests

Per PLAN_05:

- **Unit tests** for the key decoder (byte sequences → key
  events), vi-mode dispatcher (operator + motion combinations),
  history expansion, frame diff (frame pairs → byte sequences),
  shell-aware word boundaries.
- **Snapshot tests** for the highlighter (parser input → styled
  spans) and the prompt+buffer frame model (input scenario →
  expected frame).
- **Property tests** for: the key decoder (no panic on arbitrary
  bytes); the frame diff (apply diff to old frame == new frame).
- **PTY-driven integration tests** for end-to-end behavior:
  typing produces the expected screen output; resize mid-edit
  reflows correctly; bracketed paste does not submit.
- **Benchmarks** (Criterion) for: keystroke insertion at
  buffer sizes 10/100/1000 chars; full redraw at 80×24 and
  200×60; frame diff for a 1-character change; vertical motion
  through a wrapped row; history search over 10k entries.

The performance regression threshold from AGENTS.md (>15%
triggers justification) applies; the editor is the most
performance-sensitive part of fredshell.

### 12.7. PTY harness (L4)

Most editor tests run against the in-memory `Frame`/`Buffer`
model — fast, hermetic, no kernel involvement. A subset of
behaviours can only be tested through a real PTY: the
`yield_terminal` primitive (§9.5), bracketed-paste interaction
with raw-mode toggling, kitty keyboard-protocol level
negotiation against a terminal that actually replies, and the
resume-after-yield invariants. These tests are PLAN_05's **L4**
tier (PTY-driven end-to-end), and PLAN_14 owns the harness.

The harness lives in `crates/fredshell-core/tests/pty/` and is
gated behind `#[cfg(target_family = "unix")]`. It is built on:

- `nix::pty::openpty` for the master/slave pair.
- A thin async-free wrapper in
  `fredshell-core::testing::pty_harness` (only compiled under
  `cfg(test)` or with the `test-support` feature).
- A reader thread that drains the master fd into a
  `Vec<u8>` byte log, normalised by the same filter PLAN_08 uses
  for the differential oracle (timestamps stripped, mode bits
  pinned to 022).

The public test surface is:

```rust
pub struct PtyHarness {
    pub master: OwnedFd,
    pub slave_path: PathBuf,
    pub child: Child,
    pub output: ByteLog,
}

impl PtyHarness {
    pub fn spawn(args: &[&str]) -> Result<Self, HarnessError>;
    pub fn send_keys(&mut self, bytes: &[u8]) -> Result<(), HarnessError>;
    pub fn expect_prompt(
        &mut self,
        timeout: Duration,
    ) -> Result<(), HarnessError>;
    pub fn snapshot_frame(&mut self) -> Result<FrameSnapshot, HarnessError>;
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), HarnessError>;
    pub fn shutdown(self) -> Result<ExitStatus, HarnessError>;
}
```

Mandatory scenarios at v1:

1. **Yield round-trip.** Spawn fredshell, type a command that
   invokes `yield_terminal` with a no-op closure, assert the
   prompt redraws identically afterwards.
2. **Yield with child output.** Closure runs `cat <<<hello`;
   assert "hello" appears in the byte log before the prompt
   redraw bytes.
3. **Yield + SIGWINCH.** Resize during the closure; assert the
   resumed prompt is laid out at the new width.
4. **Bracketed-paste end-to-end.** Send `ESC [ 200 ~ <data>
ESC [ 201 ~`; assert the buffer contains exactly `<data>`
   and no submission occurred even if `<data>` contains `\r`.
5. **Kitty L1–L4 negotiation.** Run against a faked terminal
   reply chain; assert the editor enables exactly the level
   reported as supported.
6. **Trap delivery during read-line.** PLAN_13 §11 subtask
   10.12 owns this case, but it executes against this harness.

Performance contract: the harness must spawn and reach the
first prompt in <100 ms on the CI runners; otherwise it
displaces real test cycles. Tests that exceed 500 ms are split
or moved to nightly.

The harness is **not** an integration test for the parser or
the executor — those run against the spec runner (PLAN_05,
PLAN_07). The harness exercises only the editor and PLAN_04
terminal session.

## 13. Open questions

- **Fuzzy matcher choice.** `fuzzy-matcher` (skim's algorithm) vs
  `nucleo` (helix's algorithm) vs in-house. Settled at
  implementation time against benchmark data.
- **Width-table generation.** Build a generator that consumes UCD
  and emits a static table, or vendor a snapshot? Vendoring is
  simpler; generation keeps us current. Lean toward vendoring
  with a documented update procedure.
- **`SmallString` choice.** `smallstr`, `compact_str`,
  `smartstring`, or in-house? Decided at implementation time;
  the public API uses `SmallString` as a type alias so the choice
  is reversible.
- **Hint colour.** Default colour for ghost text is configurable
  but needs a sensible default that works on light _and_ dark
  terminals. The `Capabilities` struct does not tell us which we
  are on (no probe is reliable). Lean toward "dim" attribute
  rather than a specific colour.
- **External-editor diff handling.** If `$EDITOR` returns an
  empty buffer or an unchanged buffer, do we submit, cancel, or
  drop back into editing? Bash submits the (possibly modified)
  buffer; we will match.
- **Vi mode and bracketed paste.** Pasting in vi-normal mode is
  ambiguous: treat as a stream of keystrokes (replay through
  keymap, dangerous) or as a single insertion at cursor
  (suppress mode dispatch)? We will suppress; this matches
  vim's `:set paste`.

## 14. Relationship to other plans

- **PLAN_03** provides the encoders the renderer emits through.
  Every redraw byte goes through `fredshell-ansi`.
- **PLAN_04** provides `TerminalSession::input()` /
  `output()`, the cancellation token, the window-size feed, and
  the raw-mode transition. The editor is the largest consumer
  of PLAN_04's API.
- **PLAN_05** owns the test infrastructure; PLAN_14 supplies the
  unit, snapshot, property, and benchmark cases.
- **PLAN_06** (parser) provides incremental highlighting input
  and the "is this line complete?" oracle for multiline Enter.
  PLAN_06 also dispatches the `history` and `fc` builtins to
  their entry points; the semantics of those builtins live in
  PLAN_14 §8.6.
- **PLAN_13** (traps and jobs) calls `yield_terminal` (§9.5)
  when a foreground child claims the controlling terminal and
  when trap handlers shell out. PLAN_13 §11 subtask 10.6
  consumes this primitive; PLAN_14 §12.7 owns the PTY harness
  that exercises trap delivery during read-line (PLAN_13
  subtask 10.12).
- **PLAN_15** (prompt) provides the leading frame content; the
  editor composes prompt + buffer + hint + menu into a single
  frame. Prompt rendering is **not** in PLAN_14; the editor
  knows only the prompt's column-zero offset on row 0 (and the
  continuation-prompt width on subsequent rows).
- **PLAN_06** (completion) implements `CompletionProvider`; the
  editor owns the menu and trigger semantics.
- **PLAN_13** (config) maps user keybinding configuration onto
  `EditCommand` variants.
