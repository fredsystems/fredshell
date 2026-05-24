// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Case-status taxonomy (`PLAN_05` §12).
//!
//! Subtask 05.4 produced raw match / mismatch outcomes
//! ([`crate::CaseOutcome`]). 05.5 cross-references those outcomes with
//! the case's declared [`crate::CaseStatus`] and produces a
//! [`CaseVerdict`] — the value CI actually consumes.
//!
//! ## Taxonomy summary
//!
//! Per §12, the four declared statuses map to these CI behaviours:
//!
//! | Status             | Outcome matches | Outcome differs | CI behaviour on mismatch                     |
//! | ------------------ | --------------- | --------------- | -------------------------------------------- |
//! | `pass`             | [`CaseVerdict::ExpectedPass`] | [`CaseVerdict::Regression`] | Build fails                                  |
//! | `fail`             | [`CaseVerdict::Reclassify`]   | [`CaseVerdict::ExpectedFail`] | Match flags `RECLASSIFY`; mismatch is OK     |
//! | `wontfix`          | [`CaseVerdict::Reclassify`]   | [`CaseVerdict::WontfixHonored`] | Match flags `RECLASSIFY`; mismatch is OK     |
//! | `deferred:PLAN_XX` | [`CaseVerdict::Reclassify`]   | [`CaseVerdict::DeferredHonored`] | Match flags `RECLASSIFY`; mismatch is OK     |
//!
//! [`CaseVerdict::Regression`] is the only verdict that fails CI; the
//! others either pass cleanly or surface a `RECLASSIFY` advisory.
//!
//! ## "Outcome matches" / "Outcome differs"
//!
//! A case "matches" iff the runner reported [`CaseOutcome::Pass`]. A
//! [`CaseOutcome::Mismatch`] or [`CaseOutcome::ExecutorRefused`] both
//! count as "outcome differs":
//!
//! - For `pass` cases, a refusal IS a regression — the harness ran
//!   fredshell and got no output, so the case did not pass.
//! - For `deferred:PLAN_XX` cases, a refusal is the canonical
//!   "we got here" signal: strict-mode refusal proves the missing
//!   feature has not yet landed.
//! - For `fail` / `wontfix` cases the distinction is irrelevant —
//!   both forms of "differs" are honored equally.
//!
//! 05.6's `xtask compat` will preserve the underlying
//! [`CaseOutcome`] alongside the [`CaseVerdict`] in its JSON report
//! so consumers can drill down. The verdict is the *interpretation*;
//! the outcome is the *evidence*.

use std::collections::BTreeMap;
use std::fmt;

use crate::case::CaseStatus;
use crate::runner::CaseOutcome;

/// The §12 verdict for a single case.
///
/// Produced by [`classify`] from the case's declared status and the
/// runner's [`CaseOutcome`]. Treated as authoritative by CI:
/// [`CaseVerdict::Regression`] is the only verdict that fails the
/// build.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CaseVerdict {
    /// Status `pass`, outcome [`CaseOutcome::Pass`]. The healthy
    /// steady state. Counted toward the pass-rate.
    ExpectedPass,

    /// Status `pass`, outcome [`CaseOutcome::Mismatch`] or
    /// [`CaseOutcome::ExecutorRefused`]. The case used to pass and no
    /// longer does — CI must fail.
    Regression,

    /// Status `fail`, outcome [`CaseOutcome::Mismatch`] or
    /// [`CaseOutcome::ExecutorRefused`]. The case is documented as a
    /// parity gap and remains one. Excluded from the pass-rate.
    ExpectedFail,

    /// Status `wontfix`, outcome [`CaseOutcome::Mismatch`] or
    /// [`CaseOutcome::ExecutorRefused`]. The case documents an
    /// intentional non-goal and behaves accordingly. Excluded from
    /// the pass-rate.
    WontfixHonored,

    /// Status `deferred:PLAN_XX`, outcome [`CaseOutcome::Mismatch`]
    /// or [`CaseOutcome::ExecutorRefused`]. The case is waiting on
    /// the named plan. Counted in the per-plan "pending work" bucket
    /// (see [`VerdictTally::deferred_honored`]).
    DeferredHonored {
        /// The plan identifier from `deferred:<plan>` (e.g.
        /// `"PLAN_11"`). Used to group pending work by responsible
        /// document.
        plan: String,
    },

    /// The outcome disagrees with the declared status in a way the
    /// PR author should address: a `fail` / `wontfix` / `deferred`
    /// case now matches its fixtures, or some future status produced
    /// an outcome the taxonomy did not predict.
    ///
    /// CI emits a `RECLASSIFY` line but does NOT fail — the PR author
    /// updates the case's status in the same PR (or opens a
    /// follow-up).
    Reclassify {
        /// The status currently declared in the `.case.toml` file.
        from: CaseStatus,
        /// What [`classify`] suggests the new status should be.
        /// Almost always [`CaseStatus::Pass`]; preserved as a
        /// [`CaseStatus`] so callers can render advice cheaply.
        suggested: CaseStatus,
        /// Machine-readable reason for the recommendation.
        reason: ReclassifyReason,
    },
}

/// Why [`classify`] suggested a status change.
///
/// Kept narrow on purpose: 05.5 only knows about §12; richer drilling
/// (e.g. "matched only the exit code, not stdout") is left to the
/// diff renderer that 05.6 owns.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReclassifyReason {
    /// A case declared `fail`, `wontfix`, or `deferred:PLAN_XX`
    /// matched all of its fixtures. The most common reclassification
    /// trigger and the entire point of the §12.1 signal.
    OutcomeMatchedDespiteNonPassStatus,
}

impl fmt::Display for ReclassifyReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutcomeMatchedDespiteNonPassStatus => {
                f.write_str("case matched its fixtures despite a non-`pass` status")
            }
        }
    }
}

impl CaseVerdict {
    /// `true` iff CI must treat this verdict as a build-breaking
    /// failure. Only [`CaseVerdict::Regression`] returns `true`.
    #[must_use]
    pub const fn is_ci_failure(&self) -> bool {
        matches!(self, Self::Regression)
    }

    /// `true` iff the verdict should emit a `RECLASSIFY` line in the
    /// harness report.
    #[must_use]
    pub const fn is_reclassify(&self) -> bool {
        matches!(self, Self::Reclassify { .. })
    }

    /// `true` iff this verdict counts toward the headline pass-rate
    /// numerator. `wontfix` and `deferred` cases are intentionally
    /// excluded per §12.
    #[must_use]
    pub const fn counts_toward_pass_rate(&self) -> bool {
        matches!(self, Self::ExpectedPass)
    }
}

/// Apply the §12 taxonomy to a single case's outcome.
///
/// The function is total: every combination of [`CaseStatus`] and
/// [`CaseOutcome`] produces exactly one [`CaseVerdict`]. No I/O, no
/// allocation beyond cloning the status string when constructing a
/// `Reclassify` or `DeferredHonored`.
#[must_use]
pub fn classify(status: &CaseStatus, outcome: &CaseOutcome) -> CaseVerdict {
    let outcome_matched = matches!(outcome, CaseOutcome::Pass);

    match (status, outcome_matched) {
        // status=pass
        (CaseStatus::Pass, true) => CaseVerdict::ExpectedPass,
        (CaseStatus::Pass, false) => CaseVerdict::Regression,

        // status=fail
        (CaseStatus::Fail, true) => CaseVerdict::Reclassify {
            from: CaseStatus::Fail,
            suggested: CaseStatus::Pass,
            reason: ReclassifyReason::OutcomeMatchedDespiteNonPassStatus,
        },
        (CaseStatus::Fail, false) => CaseVerdict::ExpectedFail,

        // status=wontfix
        (CaseStatus::Wontfix, true) => CaseVerdict::Reclassify {
            from: CaseStatus::Wontfix,
            suggested: CaseStatus::Pass,
            reason: ReclassifyReason::OutcomeMatchedDespiteNonPassStatus,
        },
        (CaseStatus::Wontfix, false) => CaseVerdict::WontfixHonored,

        // status=deferred:PLAN_XX
        (CaseStatus::Deferred(plan), true) => CaseVerdict::Reclassify {
            from: CaseStatus::Deferred(plan.clone()),
            suggested: CaseStatus::Pass,
            reason: ReclassifyReason::OutcomeMatchedDespiteNonPassStatus,
        },
        (CaseStatus::Deferred(plan), false) => CaseVerdict::DeferredHonored { plan: plan.clone() },
    }
}

/// Per-status breakdown across many cases.
///
/// Built by feeding [`CaseVerdict`]s through [`VerdictTally::record`].
/// `xtask compat` (05.6) consumes this for both the JSON report and
/// the human-readable summary.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct VerdictTally {
    /// Number of `pass` cases that matched.
    pub expected_pass: usize,
    /// Number of `pass` cases that no longer match. CI fails iff
    /// this is non-zero.
    pub regression: usize,
    /// Number of `fail` cases that continued to differ.
    pub expected_fail: usize,
    /// Number of `wontfix` cases honored.
    pub wontfix_honored: usize,
    /// Per-plan count of `deferred:PLAN_XX` cases honored.
    /// Keyed by the bare plan identifier (e.g. `"PLAN_11"`).
    pub deferred_honored: BTreeMap<String, usize>,
    /// Number of `RECLASSIFY` signals emitted.
    pub reclassify: usize,
}

impl VerdictTally {
    /// Construct an empty tally.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one verdict.
    pub fn record(&mut self, verdict: &CaseVerdict) {
        // `CaseVerdict` is `#[non_exhaustive]`. The wildcard arm
        // covers future variants for external callers; inside this
        // crate the compiler can prove it unreachable, hence the
        // scoped allow.
        #[allow(unreachable_patterns)]
        match verdict {
            CaseVerdict::ExpectedPass => self.expected_pass += 1,
            CaseVerdict::Regression => self.regression += 1,
            CaseVerdict::ExpectedFail => self.expected_fail += 1,
            CaseVerdict::WontfixHonored => self.wontfix_honored += 1,
            CaseVerdict::DeferredHonored { plan } => {
                *self.deferred_honored.entry(plan.clone()).or_insert(0) += 1;
            }
            CaseVerdict::Reclassify { .. } => self.reclassify += 1,
            // Future variants are silently ignored here until they
            // get explicit counters. This keeps the tally
            // compile-stable as the taxonomy grows.
            _ => {}
        }
    }

    /// Total number of cases recorded.
    #[must_use]
    pub fn total(&self) -> usize {
        let deferred: usize = self.deferred_honored.values().sum();
        self.expected_pass
            + self.regression
            + self.expected_fail
            + self.wontfix_honored
            + deferred
            + self.reclassify
    }

    /// Number of cases that count toward the pass-rate numerator
    /// (`expected_pass`). Denominator excludes `wontfix` cases per
    /// §12.
    #[must_use]
    pub const fn pass_rate_numerator(&self) -> usize {
        self.expected_pass
    }

    /// Pass-rate denominator: every recorded case EXCEPT `wontfix`
    /// honors (`wontfix` is documented as intentionally excluded in
    /// §12) and `wontfix`-triggered `Reclassify` rows. 05.5 keeps it
    /// simple: every non-wontfix verdict counts.
    ///
    /// In practice 05.6 will compute this from the same tally; the
    /// helper exists so the harness binary can emit a quick
    /// human-readable summary without re-deriving the rule.
    #[must_use]
    pub fn pass_rate_denominator(&self) -> usize {
        self.total().saturating_sub(self.wontfix_honored)
    }

    /// `true` iff at least one verdict was a [`CaseVerdict::Regression`].
    #[must_use]
    pub const fn has_ci_failures(&self) -> bool {
        self.regression > 0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use fredshell_core::NoExternalExecutorReason;

    fn pass_outcome() -> CaseOutcome {
        CaseOutcome::Pass
    }

    fn mismatch_outcome() -> CaseOutcome {
        CaseOutcome::Mismatch {
            observed_stdout: b"x".to_vec(),
            observed_stderr: Vec::new(),
            observed_exit: 1,
        }
    }

    fn refused_outcome() -> CaseOutcome {
        CaseOutcome::ExecutorRefused {
            command: "/bin/echo hi".to_owned(),
            reason: NoExternalExecutorReason::PolicyStrict,
        }
    }

    // ----- classify: status=pass --------------------------------------------

    #[test]
    fn classify_pass_match_is_expected_pass() {
        let v = classify(&CaseStatus::Pass, &pass_outcome());
        assert_eq!(v, CaseVerdict::ExpectedPass);
        assert!(!v.is_ci_failure());
        assert!(v.counts_toward_pass_rate());
    }

    #[test]
    fn classify_pass_mismatch_is_regression() {
        let v = classify(&CaseStatus::Pass, &mismatch_outcome());
        assert_eq!(v, CaseVerdict::Regression);
        assert!(v.is_ci_failure());
        assert!(!v.counts_toward_pass_rate());
    }

    #[test]
    fn classify_pass_refused_is_regression() {
        // Strict-mode refusal on a `pass` case IS a regression: the
        // harness ran the case and got no usable result.
        let v = classify(&CaseStatus::Pass, &refused_outcome());
        assert_eq!(v, CaseVerdict::Regression);
        assert!(v.is_ci_failure());
    }

    // ----- classify: status=fail --------------------------------------------

    #[test]
    fn classify_fail_match_is_reclassify_to_pass() {
        let v = classify(&CaseStatus::Fail, &pass_outcome());
        match v {
            CaseVerdict::Reclassify {
                from,
                suggested,
                reason,
            } => {
                assert_eq!(from, CaseStatus::Fail);
                assert_eq!(suggested, CaseStatus::Pass);
                assert_eq!(reason, ReclassifyReason::OutcomeMatchedDespiteNonPassStatus);
            }
            other => panic!("expected Reclassify, got {other:?}"),
        }
    }

    #[test]
    fn classify_fail_mismatch_is_expected_fail() {
        let v = classify(&CaseStatus::Fail, &mismatch_outcome());
        assert_eq!(v, CaseVerdict::ExpectedFail);
        assert!(!v.is_ci_failure());
    }

    #[test]
    fn classify_fail_refused_is_expected_fail() {
        let v = classify(&CaseStatus::Fail, &refused_outcome());
        assert_eq!(v, CaseVerdict::ExpectedFail);
    }

    // ----- classify: status=wontfix -----------------------------------------

    #[test]
    fn classify_wontfix_match_is_reclassify_to_pass() {
        let v = classify(&CaseStatus::Wontfix, &pass_outcome());
        match v {
            CaseVerdict::Reclassify {
                from, suggested, ..
            } => {
                assert_eq!(from, CaseStatus::Wontfix);
                assert_eq!(suggested, CaseStatus::Pass);
            }
            other => panic!("expected Reclassify, got {other:?}"),
        }
    }

    #[test]
    fn classify_wontfix_mismatch_is_wontfix_honored() {
        let v = classify(&CaseStatus::Wontfix, &mismatch_outcome());
        assert_eq!(v, CaseVerdict::WontfixHonored);
        assert!(!v.is_ci_failure());
        assert!(!v.counts_toward_pass_rate());
    }

    #[test]
    fn classify_wontfix_refused_is_wontfix_honored() {
        let v = classify(&CaseStatus::Wontfix, &refused_outcome());
        assert_eq!(v, CaseVerdict::WontfixHonored);
    }

    // ----- classify: status=deferred ---------------------------------------

    #[test]
    fn classify_deferred_match_is_reclassify_carrying_plan_in_from() {
        let v = classify(&CaseStatus::Deferred("PLAN_11".to_owned()), &pass_outcome());
        match v {
            CaseVerdict::Reclassify {
                from, suggested, ..
            } => {
                assert_eq!(from, CaseStatus::Deferred("PLAN_11".to_owned()));
                assert_eq!(suggested, CaseStatus::Pass);
            }
            other => panic!("expected Reclassify, got {other:?}"),
        }
    }

    #[test]
    fn classify_deferred_mismatch_is_deferred_honored_with_plan() {
        let v = classify(
            &CaseStatus::Deferred("PLAN_11".to_owned()),
            &mismatch_outcome(),
        );
        match v {
            CaseVerdict::DeferredHonored { plan } => assert_eq!(plan, "PLAN_11"),
            other => panic!("expected DeferredHonored, got {other:?}"),
        }
    }

    #[test]
    fn classify_deferred_refused_is_deferred_honored() {
        // The canonical "we got here" signal for deferred cases: the
        // executor refused because the feature has not landed yet.
        let v = classify(
            &CaseStatus::Deferred("PLAN_12".to_owned()),
            &refused_outcome(),
        );
        match v {
            CaseVerdict::DeferredHonored { plan } => assert_eq!(plan, "PLAN_12"),
            other => panic!("expected DeferredHonored, got {other:?}"),
        }
    }

    // ----- reclassify reason display ---------------------------------------

    #[test]
    fn reclassify_reason_display_is_descriptive() {
        let s = ReclassifyReason::OutcomeMatchedDespiteNonPassStatus.to_string();
        assert!(s.contains("matched"));
        assert!(s.contains("non-`pass`"));
    }

    // ----- tally -----------------------------------------------------------

    #[test]
    fn tally_starts_empty_and_counts_zero_totals() {
        let t = VerdictTally::new();
        assert_eq!(t.total(), 0);
        assert_eq!(t.pass_rate_numerator(), 0);
        assert_eq!(t.pass_rate_denominator(), 0);
        assert!(!t.has_ci_failures());
    }

    #[test]
    fn tally_records_each_verdict_variant_into_the_right_bucket() {
        let mut t = VerdictTally::new();
        t.record(&CaseVerdict::ExpectedPass);
        t.record(&CaseVerdict::ExpectedPass);
        t.record(&CaseVerdict::Regression);
        t.record(&CaseVerdict::ExpectedFail);
        t.record(&CaseVerdict::WontfixHonored);
        t.record(&CaseVerdict::DeferredHonored {
            plan: "PLAN_11".to_owned(),
        });
        t.record(&CaseVerdict::DeferredHonored {
            plan: "PLAN_11".to_owned(),
        });
        t.record(&CaseVerdict::DeferredHonored {
            plan: "PLAN_12".to_owned(),
        });
        t.record(&CaseVerdict::Reclassify {
            from: CaseStatus::Fail,
            suggested: CaseStatus::Pass,
            reason: ReclassifyReason::OutcomeMatchedDespiteNonPassStatus,
        });

        assert_eq!(t.expected_pass, 2);
        assert_eq!(t.regression, 1);
        assert_eq!(t.expected_fail, 1);
        assert_eq!(t.wontfix_honored, 1);
        assert_eq!(t.deferred_honored.get("PLAN_11").copied(), Some(2));
        assert_eq!(t.deferred_honored.get("PLAN_12").copied(), Some(1));
        assert_eq!(t.reclassify, 1);
        assert_eq!(t.total(), 9);
        assert!(t.has_ci_failures());
    }

    #[test]
    fn tally_pass_rate_excludes_wontfix_from_denominator() {
        let mut t = VerdictTally::new();
        t.record(&CaseVerdict::ExpectedPass);
        t.record(&CaseVerdict::ExpectedPass);
        t.record(&CaseVerdict::ExpectedFail);
        t.record(&CaseVerdict::WontfixHonored);
        t.record(&CaseVerdict::WontfixHonored);

        // total = 5, wontfix = 2, denominator = 3, numerator = 2.
        assert_eq!(t.total(), 5);
        assert_eq!(t.pass_rate_numerator(), 2);
        assert_eq!(t.pass_rate_denominator(), 3);
        assert!(!t.has_ci_failures());
    }

    #[test]
    fn tally_deferred_keyed_by_plan_so_xtask_can_filter() {
        // §12.2: `cargo xtask compat --status deferred:PLAN_11` lists
        // every PLAN_11 case. The tally must preserve enough
        // structure for that query.
        let mut t = VerdictTally::new();
        t.record(&CaseVerdict::DeferredHonored {
            plan: "PLAN_11".to_owned(),
        });
        t.record(&CaseVerdict::DeferredHonored {
            plan: "PLAN_12".to_owned(),
        });
        let keys: Vec<&String> = t.deferred_honored.keys().collect();
        assert_eq!(keys, vec!["PLAN_11", "PLAN_12"]);
    }

    #[test]
    fn tally_has_ci_failures_is_false_when_only_reclassify_fires() {
        // RECLASSIFY is advisory: CI warns but does not fail.
        let mut t = VerdictTally::new();
        t.record(&CaseVerdict::Reclassify {
            from: CaseStatus::Fail,
            suggested: CaseStatus::Pass,
            reason: ReclassifyReason::OutcomeMatchedDespiteNonPassStatus,
        });
        assert!(!t.has_ci_failures());
    }
}
