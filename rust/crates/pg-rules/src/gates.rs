//! Feature-gate observations keep admission in the predicate owner and aggregate by compact IDs.

use crate::observations::{Decision, MprGateOwner, Recorder};
use crate::stats::{ObjectKind, StatsCollector};
use crate::trace::FailureReason;
use crate::word::Word;
use pg_grammar_model::model::{Grammar, MprSet};

/// Identity of a declared predicate, including polarity and the comparison mechanism.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GateFeature {
    Mpr(u32, bool),
    CompoundMpr(u32),
    StemName(u32, bool),
    Syntactic(u32),
    InverseSyntactic(u32, u32),
    RealizationalUnification(u32),
    RealizationalSubsumption(u32),
    Pos(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateRow {
    pub kind: ObjectKind,
    pub object_index: u32,
    pub feature: GateFeature,
    pub passed: u64,
    pub blocked: u64,
}

/// Observe a boolean already computed by its owner. The return value preserves short circuits.
#[inline]
pub(crate) fn observe(
    stats: Option<&StatsCollector>,
    kind: ObjectKind,
    object: u32,
    feature: GateFeature,
    passed: bool,
) -> bool {
    if let Some(stats) = stats {
        stats.record_gate(kind, object, feature, passed);
    }
    passed
}

/// Gate the same required-then-excluded walk used by uninstrumented callers.
pub(crate) fn mpr_reason(
    g: &Grammar,
    required: &MprSet,
    excluded: &MprSet,
    have: &MprSet,
    owner: Option<(&StatsCollector, ObjectKind, u32)>,
) -> Option<crate::trace::FailureReason> {
    mpr_reason_with(|is_excluded| {
        let test = if is_excluded { excluded } else { required };
        mpr_check(g, test, have, is_excluded, owner)
    })
}

fn mpr_reason_with(mut check: impl FnMut(bool) -> bool) -> Option<FailureReason> {
    if !check(false) {
        Some(FailureReason::RequiredMprFeatures)
    } else if !check(true) {
        Some(FailureReason::ExcludedMprFeatures)
    } else {
        None
    }
}

/// Use the same short-circuit order, with the failed bucket reported at its evaluator.
pub(crate) fn mpr_reason_recorded(
    g: &Grammar,
    required: &MprSet,
    excluded: &MprSet,
    word: &Word,
    stats_owner: Option<(&StatsCollector, ObjectKind, u32)>,
    recorder: &dyn Recorder,
    owner: MprGateOwner,
) -> Option<FailureReason> {
    mpr_reason_with(|is_excluded| {
        let tested = if is_excluded { excluded } else { required };
        mpr_check_observed(
            g,
            tested,
            &word.mpr,
            is_excluded,
            stats_owner,
            |group, bucket| {
                recorder.record_at(
                    word,
                    Decision::MprGate {
                        owner,
                        tested: bucket,
                        actual: &word.mpr,
                        excluded: is_excluded,
                        group,
                    },
                );
            },
        )
    })
}

/// Evaluate the owning MPR predicate and observe its evaluated buckets, including lookup replay.
pub fn mpr_check(
    g: &Grammar,
    test: &MprSet,
    have: &MprSet,
    excluded: bool,
    owner: Option<(&StatsCollector, ObjectKind, u32)>,
) -> bool {
    if owner.is_none() {
        return if excluded {
            pg_grammar_model::model::mpr_excluded_ok(&g.mpr_groups, test, have)
        } else {
            pg_grammar_model::model::mpr_required_ok(&g.mpr_groups, test, have)
        };
    }
    mpr_check_observed(g, test, have, excluded, owner, |_, _| {})
}

/// Observe the failing bucket, if any, at the same short-circuit used for admission.
pub fn mpr_check_observed(
    g: &Grammar,
    test: &MprSet,
    have: &MprSet,
    excluded: bool,
    owner: Option<(&StatsCollector, ObjectKind, u32)>,
    mut observe: impl FnMut(Option<usize>, &MprSet),
) -> bool {
    pg_grammar_model::model::mpr_match_grouped_observed(
        &g.mpr_groups,
        test,
        have,
        excluded,
        |group, bucket, blocked| {
            if let Some((stats, kind, object)) = owner {
                for id in bucket.iter() {
                    stats.record_gate(
                        kind,
                        object,
                        GateFeature::Mpr(id.0, excluded),
                        !blocked.contains(id),
                    );
                }
            }
            if !blocked.is_empty() {
                observe(group, &bucket);
            }
        },
    )
}
