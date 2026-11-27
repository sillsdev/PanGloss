//! Public projection of bounded, same-pass rejection summaries.

use pg_rules::partials::{FailureKind, RejectingObject, Rejection};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct PartialAnalysis {
    pub morpheme_ids: Vec<String>,
    pub residue: String,
    pub failure_kind: &'static str,
    /// Typed dense grammar id, e.g. `allomorph:2`; `stratum:N` identifies lookup/surface gates.
    pub rejecting_object_id: String,
    pub work_steps: usize,
}

impl PartialAnalysis {
    pub(crate) fn from_rejection(r: Rejection) -> Self {
        let failure_kind = match r.kind {
            FailureKind::NoRoot => "no-root",
            FailureKind::RootGate => "root-gate",
            FailureKind::AffixGate => "affix-gate",
            FailureKind::SurfaceMismatch => "surface-mismatch",
        };
        let rejecting_object_id = match r.object {
            RejectingObject::Root(id) => format!("entry:{}", id.0),
            RejectingObject::Allomorph(id) => format!("allomorph:{}", id.0),
            RejectingObject::Rule(id) => format!("rule:{}", id.0),
            RejectingObject::Stratum(id) => format!("stratum:{}", id.0),
        };
        Self {
            morpheme_ids: r.seed.morpheme_ids.clone(),
            residue: r.seed.residue.clone(),
            failure_kind,
            rejecting_object_id,
            work_steps: r.work_steps,
        }
    }
}
