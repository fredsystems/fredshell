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
>
> **Status (2026-05-23):** all 18 questions resolved via the
> question-walk recorded below. New questions arising during
> implementation belong in the relevant PLAN doc's "Open questions"
> section, not this file.

## Resolved

- **Q-10-A** (2026-05-23) — Accepted default. `nohup_on_exit: bool`
  remains an inline field on `Job`. No doc change.
- **Q-10-B** (2026-05-23) — Accepted default + dispatch-asymmetry
  note. PLAN_10 §5.1 now documents that signal and pseudo traps
  share storage but dispatch from different paths; debug
  assertions guard the boundary.
- **Q-10-C** (2026-05-23) — Accepted default. PLAN_07 §9.5 owns
  the `yield_terminal` primitive; PLAN_10 §6 adds an explicit
  notification-dispatch subsection routing through it when an
  editor session is active, and writing direct to stderr
  otherwise. PLAN_10 §12 Q10.5 marked resolved.
- **Q-10-D** (2026-05-23) — Selected alternative (c). New stub
  `Documents/PLAN_16_coproc.md` created as the permanent owning
  doc for `coproc`. PLAN_10 §1 and §12 Q10.3 updated to point at
  it. **Q-06B-2 resolved transitively.**
- **Q-08-A** (2026-05-23) — Accepted default + canonical-marker
  refinement. PLAN_08 §4 documents a single template; feature
  sheets carry `Tier: feature` (linted by path); builtins must
  use `Tier: 1` or `Tier: 2`. `xtask check-specs` enforces the
  pairing.
- **Q-08-B** (2026-05-23) — Accepted default. PLAN_08 §5.3
  documents the policy: workarounds in `defer:N` rows are
  best-effort guidance, not contract. Rationale (avoiding 150–200
  brittle corpus cases) recorded in-line. Drafters wanting a
  stronger guarantee should propose promoting the row to
  `support`.
- **Q-08-C** (2026-05-23) — Accepted default + readability
  refinements. PLAN_08 §3 template permits optional
  `### 3.A`-style sub-headers in long sheets and allows
  multiple support rows to share a corpus case when verified
  together. §2.1 flags `set` (~80 rows) and `shopt` (~50 rows)
  as the two unusually long sheets.
- **Q-08-D** (2026-05-23) — Accepted default. `refuse!` validates
  sheet row references at compile time via `include_str!`.
  PLAN_08 §8.2 documents the rebuild-coupling cost as an
  accepted trade-off; PLAN_08 §11 Q08.4 added for audit trail.
- **Q-09-1** (2026-05-23) — Accepted default + refinement.
  PLAN_09 §2.3 pins `umask 022` for both fredshell and
  reference bash before each fuzz invocation. PLAN_09 §4.5
  (new) adds an excluded-builtins table covering `umask`,
  `exit`, `exec`, `cd`, `ulimit`, `trap`, and `kill`; their
  behaviours are owned by the PLAN_08 spec corpus rather than
  the fuzzer. PLAN_09 §11 Q09.1 marked resolved.
- **Q-09-2** (2026-05-23) — Accepted default. v1 oracle is
  bash-only; PLAN_09 §5.1 reference-shell trait already shaped
  to support `dash` / `mksh` as post-v1 extensions. PLAN_09
  §11 Q09.2 marked resolved.
- **Q-09-3** (2026-05-23) — Accepted default. Functions remain
  in the grammar (§4.2 `function_def`); aliases excluded in v1
  pending parser-strategy ADR 0005 (per PLAN_06 §13.8 Q06B.1).
  Alias behaviour stays owned by the PLAN_08 spec corpus.
  PLAN_09 §11 Q09.3 marked resolved.
- **Q-09-4** (2026-05-23) — Accepted default. Fuzz-derived
  cases live under `tests/spec/fuzz/<category>/`; `[meta]`
  records `seed`, `tier`, `original_input_hash`. Hand-curated
  density per PLAN_05 §3 category not diluted. PLAN_09 §11
  Q09.4 marked resolved.
- **Q-09-5** (2026-05-23) — Accepted option (a): v1
  spec-corpus coverage + post-v1 fuzzer tier. Shell UTF-8
  correctness is hand-curated in v1 via PLAN_08's new
  `utf8_locale` feature category (§2.2; ~23 feature sheets,
  ~80 total). A UTF-8 fuzz tier (`F2-utf8`) is scheduled in
  PLAN_15 as milestone M-15-utf8-fuzz between v1.0 and v1.1.
  PLAN_09 §11 Q09.5, PLAN_08 §2.2 / §2.3, and `plan.md` row
  15 updated.
- **Q-06B-1** (2026-05-23) — Accepted default. In-house
  recursive-descent parser. ADR 0005 (subtask 06b.1)
  ratifies. PLAN_06 §13.2 prose updated and §13.8 Q06B.1
  marked resolved; PLAN_07 partial-line tolerance, lossless
  CST for the future formatter, and alias / parse-stage
  gating (PLAN_09 Q09.3) all cited as drivers.
- **Q-06B-3** (2026-05-23) — Accepted default. Here-doc
  bodies ≤ 64 KiB via pipe, > 64 KiB via tempfile under
  `$TMPDIR` with `unlink`-on-open. Threshold is a named const
  `HEREDOC_PIPE_MAX` (not runtime-configurable). PLAN_08
  here-doc sheets must include boundary cases at
  `HEREDOC_PIPE_MAX - 1` and `HEREDOC_PIPE_MAX + 1`.
  FD-table introspection divergence from bash at the boundary
  is documented and expected. PLAN_06 §13.8 Q06B.3 marked
  resolved.
- **Q-06B-4** (2026-05-23) — Accepted default. `$RANDOM`
  and `$SECONDS` pinned per case via PLAN_08 sheet `[harness]`
  block (`random_seed`, `seconds_offset`); fredshell reads
  them from harness-only env vars; reference bash gets a
  matched pin via `RANDOM=<seed>` and a `faketime` clock
  shim. Workspace defaults (`0`, `0`) apply when fields are
  absent. PLAN_06 §13.8 Q06B.4 marked resolved.
- **Q-06B-5** (2026-05-23) — Accepted default. Parser accepts
  `$"..."` syntactically; executor refuses with
  `ExecError::Unsupported { feature: "locale_translation",
suggestion: ... }`. Refusal preserves the loud-failure
  contract; silently dropping translations is rejected as a
  footgun. Full `gettext` support is post-v1; refusal-corpus
  case lives at `tests/spec/refusals/locale_translation.case.toml`.
  PLAN_06 §13.8 Q06B.5 marked resolved.

## Open questions

_None. All 18 questions in the resolved log above were
walked and decided on 2026-05-23. Any new question arising
during implementation belongs in the relevant PLAN doc's
"Open questions" section._
