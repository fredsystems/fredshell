# ADR 0002 — ANSI Encoding Crate Strategy

- Status: accepted
- Date: 2026-05-20
- Supersedes: —
- Superseded by: —

## Context

fredshell must emit a non-trivial volume of ANSI/VT escape sequences:

- SGR (colors, bold, italic, underline) for the prompt, syntax highlighting,
  and tier-2 builtin output.
- CSI cursor movement and erase commands for the line editor's redraw loop.
- OSC 8 hyperlinks for clickable file paths in `ls` output and error messages.
- OSC 52 clipboard set/get for kill-ring integration with terminal-mediated
  clipboards.
- OSC 7 (current working directory) and OSC 133 (semantic prompts) for
  terminal integration with tmux, kitty, WezTerm, and similar.
- Kitty keyboard protocol query/response and DA1/DSR queries for capability
  detection.
- Bracketed paste mode enable/disable.

These needs are predominantly **encoder-oriented**: fredshell writes
well-formed escape sequences to its output and only minimally reads structured
responses back (DA1, DSR, kitty keyboard protocol response, OSC clipboard
read response).

A neighboring project, freminal, contains a substantial body of
escape-sequence handling code:

- `freminal-terminal-emulator/src/ansi.rs` and `ansi_components/` —
  ~6,300 lines of streaming **decoder** for CSI, OSC, DCS, APC sequences.
  Decoder-oriented because freminal is a terminal emulator: it consumes
  whatever bytes a child PTY produces.
- `freminal-common/src/sgr.rs` (~370 lines) and `colors.rs` (~380 lines) —
  decoded data types for SGR parameters and terminal colors.

Two questions arise: should fredshell share these types with freminal, and if
so, how?

## Investigation

`freminal-common`'s contents (16 modules, ~9.8k lines) were audited for
shell-reusable material:

- **Potentially shareable (~10% of the crate):** `sgr.rs` (taxonomy of SGR
  parameters), `colors.rs` (terminal color types and 256-palette), `base64.rs`
  (generic encoder), `terminal_size.rs` (trivial).
- **Freminal-specific (~90% of the crate):** `config.rs` (2,805 lines —
  freminal's TOML schema), `keybindings.rs` (2,410 lines — freminal's
  binding system), `layout.rs` (1,348 lines — multi-pane terminal layout),
  `themes.rs` (1,262 lines — freminal color themes), `window_state.rs`
  (egui window geometry), `buffer_states/` (terminal cell grid),
  `cursor.rs` (terminal-emulator cursor model), `terminfo.rs` (embedded
  terminfo blob), `pty_write.rs` (PTY command channel), `app_state.rs`
  (freminal app state).

Coupling within the "shareable" portion is also non-trivial:

- `sgr.rs` imports `crate::buffer_states::fonts::UnderlineStyle`.
- `colors.rs` imports `crate::themes::ThemePalette`.

Both couplings would have to be broken before `freminal-common` could be
consumed by fredshell without dragging in the bulk of the crate.

Additionally, the data model in freminal was designed for the decoding side:
parameters are stored as they were parsed, including catch-all "unknown" and
"raw bytes" variants. Encoder ergonomics are different — an encoder wants
strongly-typed inputs that are guaranteed valid.

## Decision

fredshell will **not** depend on `freminal-common`. fredshell will ship its
own crate, `fredshell-ansi`, scoped initially to encoding only.

Scope of `fredshell-ansi` v1:

- SGR encoder: bold, italic, underline (with style variants), reverse,
  strikethrough, 16-color/256-color/truecolor foreground and background,
  reset variants.
- Basic CSI: cursor up/down/left/right by N, absolute positioning, erase in
  line, erase in display, save/restore cursor.
- OSC 8 hyperlinks: `set` and `clear`.
- OSC 52 clipboard: `set` (write to terminal clipboard).
- OSC 7 current working directory notification.
- OSC 133 semantic prompt markers (A/B/C/D).
- Bracketed paste mode enable/disable sequences.
- DECSET/DECRST encoder for the small set of modes a shell needs to toggle.
- Minimal decode surface: DA1 response parser, DSR cursor position parser,
  kitty keyboard protocol response parser, OSC 52 read response parser.
  These are the only structured replies a shell needs to interpret, and they
  are small enough that a tiny hand-written parser is preferable to importing
  a general decoder.

The crate will be designed encoder-first:

- A `Write`-based API: `fn write_to<W: Write>(&self, w: &mut W) -> io::Result<()>`
  is the primary surface.
- Strongly-typed builders for sequences (no "unknown parameter" variants).
- No allocation on the hot path. The line-editor redraw loop and prompt
  renderer must be able to emit sequences without heap traffic.

The data model will be designed independently. Inspiration from freminal's
`SelectGraphicRendition` enum is fine; copying is not. Where the two
projects might eventually converge, the encoder-side design takes precedence
in fredshell because that is the dominant use case here.

## Consequences

### Positive

- fredshell can ship independently of any freminal refactor.
- The data model is designed for the dominant use case (encoding) rather
  than retrofitted from a decoder design.
- Crate scope stays small and focused.
- No cross-repo dependency or version-skew problem.
- The `Write`-based, allocation-light design is a hard requirement for the
  line-editor performance budget (see `PLAN_07_line_editor.md` once it
  exists).

### Negative

- Two related projects in the same author's ecosystem maintain two SGR
  taxonomies. Bug fixes and new SGR parameter additions must be applied to
  both.
- If fredshell ever needs full decode capability (e.g., for an embedded
  PTY view, or for a future "explain what this script's output is doing"
  feature), it will either grow its own decoder or take a dependency on a
  third-party crate, neither of which reuses freminal's work.

### Risks and mitigations

- **Risk: taxonomy drift.** The two SGR enums diverge in meaningful ways
  (different variant names, different category boundaries) and a future
  convergence becomes prohibitively expensive.
  **Mitigation:** when fredshell's `fredshell-ansi::sgr` module stabilizes
  (post-v1), revisit this decision. A future ADR may carve out a shared
  upstream crate.

- **Risk: subtle encode/decode asymmetry.** fredshell encodes a sequence
  that some terminal decodes differently than freminal would, and the bug
  is hard to find because the two codebases have no shared ground truth.
  **Mitigation:** end-to-end tests that round-trip representative sequences
  through fredshell's encoder and a public reference (e.g., `vte` crate, or
  a snapshot of expected bytes). These tests live in the `fredshell-ansi`
  crate and are independent of freminal.

## Alternatives considered

- **Depend on `freminal-common` directly.** Rejected: 90% of the crate is
  freminal-specific and would pollute fredshell's dependency graph. The
  "shareable" 10% is also coupled to the non-shareable 90% via internal
  imports.
- **Refactor `freminal-common` into `freminal-common-ansi` and `freminal-
common-app` sub-crates, publish, and depend on the ANSI sub-crate.**
  Rejected for now: it is a real week of freminal-side work, blocks
  fredshell's start, and the result would still be decoder-shaped. May be
  revisited if and when freminal does this refactor for its own reasons.
- **Adopt a third-party encoder crate (e.g., `anstyle`, `crossterm::style`,
  `termwiz`).** `anstyle` covers SGR competently but not the OSC 8 / 52 /
  7 / 133 / kitty-keyboard surface; `crossterm` couples encoding with a
  cross-platform terminal-control abstraction that we may not want to adopt
  wholesale; `termwiz` is heavyweight. The OSC + DECSET surface is small
  enough that the cost of writing it ourselves is lower than the cost of
  adapting to a third-party API we may outgrow. This decision will be
  revisited if a clearly suitable upstream crate emerges.

## Convergence note

If freminal is ever refactored to expose a decoder-and-types crate that is
genuinely decoupled from the freminal application — and if fredshell ever
needs full decode capability — a future ADR should evaluate convergence.
The current ADR records the deferral, not a permanent fork.

## References

- `freminal-common/src/sgr.rs`, `colors.rs` — source of the shareable
  taxonomy.
- `freminal-terminal-emulator/src/ansi.rs`, `ansi_components/` — freminal's
  decoder.
- `PLAN_02_architecture.md` — where `fredshell-ansi` is registered as a
  workspace crate.
- `PLAN_04_terminal_io.md` — the consumer of `fredshell-ansi`'s capabilities.
