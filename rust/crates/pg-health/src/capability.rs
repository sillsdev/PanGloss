//! The two capability-envelope shapes the pack format reads: a predicate's stable identity, its
//! typed refusal payload, and the whole-plan compile decision they compose into.
//!
//! Plain data only — the envelope composition itself (`compose_envelope`, `meet`, the predicate
//! registry, `GrammarWideCheck`) stays in `pg_foma::capability`, which re-exports these three
//! items at their historical path.

/// A predicate's stable identity (e.g. `"simultaneous.subrule-overlap"`).
pub type PredicateId = &'static str;

/// A `Refuse` verdict's typed payload: which predicate refused, what construct/config, and a
/// human-readable witness: compilation fails with a typed diagnostic naming the construct and
/// configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "a capability verdict that is computed and dropped decides nothing"]
pub struct CapabilityDiagnostic {
    pub predicate: PredicateId,
    pub construct: String,
    pub witness: String,
}

/// The overall, whole-plan compile decision `pg_foma::capability::compose_envelope` returns: a
/// node verdict is the meet of its children's verdicts and its own predicate, with `Refuse`
/// dominating and any `ConfirmOnly` demoting the subtree. Distinct from `pg_foma::capability::
/// PredicateVerdict` (a PER-PREDICATE, single-node verdict, carrying at most one
/// `CapabilityDiagnostic`): composing a whole plan can collect refusals from many different
/// nodes/observations, and a caller should see all of them, not just whichever one `meet` folded
/// in first — this type widens the single diagnostic to a deduplicated `Vec` at exactly the point
/// those per-node/per-observation verdicts get folded together.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "a capability verdict that is computed and dropped decides nothing"]
pub enum CompileDecision {
    /// Every construct in the plan is `Proven`, or has a predicate-proven `Admit` verdict.
    /// Admission-filtering is licensed.
    Admit,
    /// At least one construct rests at (or was proven no better than) `ConfirmOnly`, and NONE is
    /// refused. Propose the superset, no admission-filtering — first-class, not a failure.
    ConfirmOnly,
    /// At least one construct is refused. Carries EVERY `CapabilityDiagnostic` collected while
    /// composing the plan (content-deduplicated — see `pg_foma::capability::meet`'s own doc), not
    /// just the first, so a caller sees every problem in one pass rather than one compile attempt
    /// at a time.
    Refuse(Vec<CapabilityDiagnostic>),
}
