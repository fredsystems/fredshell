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
