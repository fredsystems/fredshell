# Questions for review — autonomous PLAN drafting session

> This file collects clarifying questions that arose while drafting
> PLAN_07, PLAN_08, PLAN_09, PLAN_10, and the PLAN_06 Phase B
> expansion in a single autonomous session. None of them were
> impactful enough to block the drafting (each was answered in the
> doc with a defensible default), but each deserves a review pass
> before implementation begins.
>
> Format: one heading per question, with the doc and section that
> raised it, the default I picked, and the alternative(s).

## From PLAN_10

### Q-10-A — `disown -h` storage

**Doc:** PLAN_10 §6.6.
**Default:** added a `nohup_on_exit: bool` field to `Job`.
**Alternative:** store the disowned-with-nohup set in a separate
`HashSet<JobId>` on `JobTable` so `Job` itself stays pure status.
**Why it matters:** purely an API-ergonomics choice; affects how
`disown -h` and the shell-exit SIGHUP loop are written.

### Q-10-B — Pseudo-signal storage key

**Doc:** PLAN_10 §5.1.
**Default:** single `HashMap<TrapKind, TrapDisposition>` with
`TrapKind` an enum spanning real and pseudo signals.
**Alternative:** split into two tables (`signal_traps`,
`pseudo_traps`) to keep dispatch sites visibly separate.
**Why it matters:** the unified table is simpler but slightly
hides the very different dispatch logic for EXIT vs SIGINT.

### Q-10-C — `set -b` notification at raw-mode prompt time

**Doc:** PLAN_10 §12 Q10.5.
**Default:** punted to a future "yield terminal for one line"
primitive owned by PLAN_07.
**Alternative:** PLAN_10 itself owns the terminal-yield primitive,
calling into PLAN_04 directly for raw-mode toggle.
**Why it matters:** ownership boundary between PLAN_07 and
PLAN_10. If PLAN_07 isn't going to own a redraw API, PLAN_10
needs to.

### Q-10-D — `coproc` placement

**Doc:** PLAN_10 §12 Q10.3.
**Default:** deferred entirely from v1; tracked as a future
PLAN_10 extension plus a PLAN_06 Phase B parser feature.
**Alternative:** assign `coproc` to a brand-new PLAN doc (e.g.
PLAN_10.5 or a future PLAN_16) because it cuts across grammar
and runtime.
**Why it matters:** affects whether the eventual implementer has
a single home or two.

## From PLAN_08

### Q-08-A — Single template or two

**Doc:** PLAN_08 §11 Q08.1.
**Default:** single template for builtins and features; features
leave the `Tier` line blank.
**Alternative:** two distinct templates.
**Why it matters:** small DX choice; affects how `xtask
check-specs` validates the section list.

### Q-08-B — Are `defer:N` workarounds contractually binding?

**Doc:** PLAN_08 §11 Q08.2.
**Default:** best-effort guidance, not contract.
**Alternative:** treat workarounds as binding promises and test
them in the corpus.
**Why it matters:** if binding, every defer row gains an
executable test obligation, materially expanding the v1 corpus.

### Q-08-C — `set -o` granularity

**Doc:** PLAN_08 §11 Q08.3.
**Default:** one row per `-o` longopt; accept ~80-row `set` sheet.
**Alternative:** group `-o` options by category in the sheet,
classify at the category level.
**Why it matters:** affects sheet readability vs.
classification precision.

### Q-08-D — Sheet ID embedded in `refuse!`

**Doc:** PLAN_08 §8.2.
**Default:** `refuse!` macro reads the sheet at compile time via
`include_str!` and validates row classification matches.
**Alternative:** `refuse!` just takes a string; no compile-time
checking; rely on `xtask check-specs` instead.
**Why it matters:** compile-time check is stronger but couples
compilation to sheet contents (every sheet edit triggers
rebuilds of every builtin that refuses something).

## PLAN_09 — Grammar-aware fuzzer + differential oracle

### Q-09-1 — Fixed `umask` for fuzz processes

**Doc:** PLAN_09 §11 Q09.1.
**Default:** yes, `umask 022` for both fredshell and reference
bash, documented in §2.3.
**Alternative:** leave umask uncontrolled and add an output-mode
redaction filter to the normaliser.
**Why it matters:** mode bits in `ls -l` / `stat` output diverge
under different umasks; fixing it is a one-line guarantee versus
ongoing normaliser maintenance.

### Q-09-2 — Oracle against `dash` and `mksh`?

**Doc:** PLAN_09 §11 Q09.2.
**Default:** not in v1; oracle API is shaped to support multiple
references later.
**Alternative:** add `dash` and `mksh` as additional reference
shells from day one to surface bash-specific non-portability.
**Why it matters:** POSIX-only divergences are informative but
expand the divergence triage surface 3×. v1 is bash-only.

### Q-09-3 — Functions and aliases in grammar

**Doc:** PLAN_09 §11 Q09.3.
**Default:** functions are in grammar; aliases are not (aliases
interact poorly with our parser stage gating).
**Alternative:** include aliases gated behind a separate weight
profile.
**Why it matters:** aliases double effective grammar depth and
require special-casing in the minimiser; functions are
self-contained.

### Q-09-4 — Fuzz corpus subtree layout

**Doc:** PLAN_09 §11 Q09.4.
**Default:** separate `tests/spec/fuzz/<category>/` subtree;
`[meta]` block records fuzz provenance.
**Alternative:** interleave fuzz-derived cases with hand-written
cases under the natural category (e.g.,
`tests/spec/parameter_expansion/`).
**Why it matters:** separation makes provenance grep-able and
keeps hand-curated corpus density visible; interleaving better
reflects "one category, one folder" mental model.

### Q-09-5 — UTF-8 locale fuzz tier

**Doc:** PLAN_09 §11 Q09.5.
**Default:** `LC_ALL=C` only in v1; UTF-8 tier tracked as future
work.
**Alternative:** add a UTF-8 tier (e.g., F2-utf8) from v1 to
catch `$'...'` and pattern-matching quirks early.
**Why it matters:** locale-dependent behaviour is a known bash
quirk surface; postponing it leaves a class of bugs uncovered
until a later milestone.

## PLAN_06 — Phase B execution semantics

### Q-06B-1 — Parser implementation strategy

**Doc:** PLAN_06 §13.2 + §13.8 Q06B.1.
**Default:** write our own recursive-descent parser; lands as
ADR 0005 (subtask 06b.1) before lexer/parser implementation.
**Alternative:** adopt `brush-parser` upstream, or fork it.
**Why it matters:** diagnostic quality and incremental parsing
(PLAN_07 highlighter needs partial-line tolerance) argue for
in-house; ecosystem reuse + maintenance burden argue for adopting
`brush-parser`. Decision blocks 06b.2 and downstream.

### Q-06B-2 — `coproc` support

**Doc:** PLAN_06 §13.8 Q06B.2.
**Default:** recognise and refuse cleanly in v1; defer real
implementation to v1.1.
**Alternative:** implement in Phase B if the real-world corpus
reveals frequent use (current evidence: none).
**Why it matters:** `coproc` is a parser-level construct with
non-trivial semantics; deferring keeps Phase B tractable.

### Q-06B-3 — Here-doc temp-file threshold

**Doc:** PLAN_06 §13.8 Q06B.3.
**Default:** 64 KiB here-doc body → tempfile; smaller → pipe.
**Alternative:** always pipe (simpler, atomic) or always
tempfile (matches bash on macOS).
**Why it matters:** affects observable behaviour under
`$BASH_SUBSHELL` introspection and under FD-table inspection;
spec sheets must agree with whichever choice ships.

### Q-06B-4 — `$RANDOM` and `$SECONDS` determinism

**Doc:** PLAN_06 §13.8 Q06B.4.
**Default:** spec harness pins both to deterministic values per
case; the PLAN_08 sheet records the pin.
**Alternative:** leave them stochastic and normalise output in
the harness.
**Why it matters:** pinning is one config-line per case;
normalisation is per-output-pattern. Pinning scales better.

### Q-06B-5 — Locale-translated strings (`$"..."`)

**Doc:** PLAN_06 §13.8 Q06B.5.
**Default:** refuse cleanly in v1; document as deferred.
**Alternative:** no-op (treat as `"..."`), matching some POSIX
shells.
**Why it matters:** real i18n support requires a message catalog
loader (out of scope for v1); the no-op alternative silently
drops translations, which is a footgun for users who do use them.
