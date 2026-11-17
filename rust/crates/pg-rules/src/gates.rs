//! Feature-gate observations keep admission in the predicate owner and aggregate by compact IDs.

use crate::stats::{ObjectKind, StatsCollector};
use pg_grammar_model::model::{Grammar, MprSet};

/// Identity of a declared predicate, including polarity and the comparison mechanism.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GateFeature {
    Mpr(u8, bool),
    CompoundMpr(u8),
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
    required: MprSet,
    excluded: MprSet,
    have: MprSet,
    owner: Option<(&StatsCollector, ObjectKind, u32)>,
) -> Option<crate::trace::FailureReason> {
    use crate::trace::FailureReason;
    if !mpr_check(g, required, have, false, owner) {
        Some(FailureReason::RequiredMprFeatures)
    } else if !mpr_check(g, excluded, have, true, owner) {
        Some(FailureReason::ExcludedMprFeatures)
    } else {
        None
    }
}

/// Evaluate the owning MPR predicate and observe its evaluated buckets, including lookup replay.
pub fn mpr_check(
    g: &Grammar,
    test: MprSet,
    have: MprSet,
    excluded: bool,
    owner: Option<(&StatsCollector, ObjectKind, u32)>,
) -> bool {
    let Some((stats, kind, object)) = owner else {
        return if excluded {
            pg_grammar_model::model::mpr_excluded_ok(&g.mpr_groups, test, have)
        } else {
            pg_grammar_model::model::mpr_required_ok(&g.mpr_groups, test, have)
        };
    };
    pg_grammar_model::model::mpr_match_observed(
        &g.mpr_groups,
        test,
        have,
        excluded,
        |bucket, blocked| {
            let mut bits = bucket.0;
            while bits != 0 {
                let bit = bits.trailing_zeros();
                bits &= bits - 1;
                stats.record_gate(
                    kind,
                    object,
                    GateFeature::Mpr(bit as u8, excluded),
                    blocked.0 & (1 << bit) == 0,
                );
            }
        },
    )
}
