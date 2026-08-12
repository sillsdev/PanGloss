//! The two passes that decide a candidate's structure: who owns its morphemes, and whether the
//! sites its trace claims exist.
//!
//! Both are anchored on something outside themselves. Ownership rejects only what the confirmer's
//! own pin resolution has already turned away, so a rejection here removes a candidate whose
//! confirmation bucket would have come back empty anyway — pinned by
//! `a_designated_root_owned_by_a_rule_has_a_verified_proof`. Transitions read the site tables of
//! [`MorphotacticIndex`](crate::morphotactics::MorphotacticIndex), the same authority the composite
//! builders prune against, rather than a second reading of the grammar's templates — pinned by
//! `a_slot_that_does_not_list_the_rule_has_a_verified_proof`.
//!
//! What the transition pass decides is deliberately narrow. A witness lists its units in morph
//! order, which is not the order the engine applied its rules in — a prefix rule that fired last
//! lands leftmost — so an ordering claim about two adjacent units is not one this witness model
//! establishes. What it does establish is where each unit claims to have been produced, and a slot
//! whose own rule list never names that unit's rule is a site the engine could not have used in
//! any order. Slot ordering itself belongs to a pass that has an ordering fact to read.

use std::sync::Arc;

use pg_grammar::model::{MRuleId, MorphemeId};

use crate::candidate_filter::decision::{
    AdmissibleProof, DeferReason, IdentityDefect, PassDecision, ProofCategory, ProofClaim,
    ProofWitness, RejectionProof, StablePassId, StableRuleId, TraceFactKind,
};
use crate::candidate_filter::index::{FilterIndex, RuleShape, SiteVerdict};
use crate::candidate_filter::model::{
    CandidateWitness, TraceRole, TraceSlotId, TraceStratumId, TraceUnit,
};
use crate::candidate_filter::passes::CandidateFilterPass;
use crate::candidate_filter::pipeline::FilterContext;
use crate::confirm::MorphemeOwner;
use crate::tags::Candidate;

/// The root position a producer uses to say the candidate has no root at all.
const NO_ROOT: i32 = -1;

const OWNERSHIP_PASS: StablePassId = StablePassId("structural.ownership.v1");

/// The candidate identity itself cannot be an analysis.
const IDENTITY_RULE: StableRuleId = StableRuleId {
    family: "structural.ownership",
    ordinal: 1,
};

/// A morpheme is owned by something that cannot place it where the identity does.
const OWNER_RULE: StableRuleId = StableRuleId {
    family: "structural.ownership",
    ordinal: 2,
};

const TRANSITION_PASS: StablePassId = StablePassId("structural.transition.v1");

/// A unit claims a site its own rule never occupies.
const SITE_RULE: StableRuleId = StableRuleId {
    family: "structural.transition",
    ordinal: 1,
};

/// Rejects identities HC's pin resolution already refuses.
pub struct OwnershipPass {
    index: Arc<FilterIndex>,
}

impl OwnershipPass {
    pub fn new(index: Arc<FilterIndex>) -> Self {
        Self { index }
    }
}

impl CandidateFilterPass for OwnershipPass {
    fn id(&self) -> StablePassId {
        OWNERSHIP_PASS
    }

    fn admissible_proofs(&self) -> Vec<AdmissibleProof> {
        vec![
            AdmissibleProof {
                rule_id: IDENTITY_RULE,
                category: ProofCategory::MalformedIdentity,
            },
            AdmissibleProof {
                rule_id: OWNER_RULE,
                category: ProofCategory::ImpossibleOwnership,
            },
        ]
    }

    fn evaluate(&self, context: &FilterContext<'_>, witness: &CandidateWitness) -> PassDecision {
        let identity = context.identity();
        if self.index.pins_resolve(identity) {
            return PassDecision::Keep;
        }

        if identity.morphemes.is_empty() {
            return reject_identity(identity, witness, IdentityDefect::EmptyMorphemeSequence);
        }
        if identity.root_index != NO_ROOT && !root_index_in_range(identity) {
            return reject_identity(
                identity,
                witness,
                IdentityDefect::RootIndexOutOfRange {
                    root_index: identity.root_index,
                    morphemes: identity.morphemes.len(),
                },
            );
        }
        if identity.root_index == NO_ROOT {
            return PassDecision::Defer(DeferReason::UnsupportedConstruct);
        }
        if !units_match_identity(identity, witness) {
            return PassDecision::Defer(DeferReason::UnsupportedConstruct);
        }

        let root = identity.root_index as usize;
        for (unit_index, unit) in witness.units.iter().enumerate() {
            if !self.placement_is_impossible(unit.morpheme, unit_index == root) {
                continue;
            }
            let Some(&role) = unit.role.known() else {
                return PassDecision::Defer(DeferReason::MissingTraceFact(TraceFactKind::Role));
            };
            return reject_ownership(identity, witness, unit_index, unit.morpheme, role);
        }

        PassDecision::Defer(DeferReason::UnsupportedConstruct)
    }
}

impl OwnershipPass {
    /// A morpheme nothing owns can appear nowhere, and a designated root must be a lexical entry.
    fn placement_is_impossible(&self, morpheme: MorphemeId, is_designated_root: bool) -> bool {
        match self.index.morpheme_owner(morpheme) {
            None => true,
            Some(MorphemeOwner::MRule(_)) => is_designated_root,
            Some(MorphemeOwner::LexEntry(_)) => false,
        }
    }
}

/// Rejects a step whose claimed sites the grammar's own slot tables do not have.
pub struct StructuralTransitionPass {
    index: Arc<FilterIndex>,
}

impl StructuralTransitionPass {
    pub fn new(index: Arc<FilterIndex>) -> Self {
        Self { index }
    }
}

/// What one adjacent pair came to.
enum Step {
    Legal,
    Undecidable(DeferReason),
    Impossible(SitedStep),
}

/// The established facts a forbidden-transition claim restates.
struct SitedStep {
    from_unit_index: usize,
    to_unit_index: usize,
    from_slot: TraceSlotId,
    to_slot: TraceSlotId,
    stratum: TraceStratumId,
}

impl CandidateFilterPass for StructuralTransitionPass {
    fn id(&self) -> StablePassId {
        TRANSITION_PASS
    }

    fn admissible_proofs(&self) -> Vec<AdmissibleProof> {
        vec![AdmissibleProof {
            rule_id: SITE_RULE,
            category: ProofCategory::ForbiddenTransition,
        }]
    }

    fn evaluate(&self, context: &FilterContext<'_>, witness: &CandidateWitness) -> PassDecision {
        let mut deferred: Option<DeferReason> = None;
        for (index, pair) in witness.units.windows(2).enumerate() {
            match self.step(index, &pair[0], &pair[1]) {
                Step::Legal => {}
                Step::Undecidable(reason) => deferred = deferred.or(Some(reason)),
                Step::Impossible(step) => {
                    return reject_transition(context.identity(), witness, &step)
                }
            }
        }
        match deferred {
            Some(reason) => PassDecision::Defer(reason),
            None => PassDecision::Keep,
        }
    }
}

impl StructuralTransitionPass {
    fn step(&self, from_unit_index: usize, from: &TraceUnit, to: &TraceUnit) -> Step {
        let (Some(from_slot), Some(to_slot)) = (from.slot.known(), to.slot.known()) else {
            return Step::Undecidable(DeferReason::MissingTraceFact(TraceFactKind::Slot));
        };
        let (Some(from_stratum), Some(to_stratum)) = (from.stratum.known(), to.stratum.known())
        else {
            return Step::Undecidable(DeferReason::MissingTraceFact(TraceFactKind::Stratum));
        };
        let (Some(&from_slot), Some(&to_slot)) = (from_slot.as_ref(), to_slot.as_ref()) else {
            return Step::Undecidable(DeferReason::UnsupportedConstruct);
        };
        let (Some(&stratum), Some(&to_stratum)) = (from_stratum.as_ref(), to_stratum.as_ref())
        else {
            return Step::Undecidable(DeferReason::UnsupportedConstruct);
        };
        if stratum != to_stratum {
            return Step::Undecidable(DeferReason::UnsupportedConstruct);
        }

        let (Some(from_rule), Some(to_rule)) = (
            self.rule_behind(from.morpheme),
            self.rule_behind(to.morpheme),
        ) else {
            return Step::Undecidable(DeferReason::UnsupportedConstruct);
        };

        let ends = [(from_slot, from_rule), (to_slot, to_rule)];
        let mut impossible = false;
        for (slot, rule) in ends {
            match self.site_is_established(slot, rule, stratum) {
                SiteVerdict::Admits => {}
                SiteVerdict::Refuses => impossible = true,
                SiteVerdict::Unknown => {
                    return Step::Undecidable(DeferReason::UnsupportedConstruct)
                }
            }
        }
        if !impossible {
            return Step::Legal;
        }

        Step::Impossible(SitedStep {
            from_unit_index,
            to_unit_index: from_unit_index + 1,
            from_slot,
            to_slot,
            stratum,
        })
    }

    /// The rule that introduces this morpheme, when a rule shape the pass can decide about does.
    fn rule_behind(&self, morpheme: MorphemeId) -> Option<MRuleId> {
        let rule = match self.index.morpheme_owner(morpheme) {
            Some(MorphemeOwner::MRule(rule)) => rule,
            Some(MorphemeOwner::LexEntry(_)) | None => return None,
        };
        match self.index.rule_shape(rule) {
            Some(RuleShape::AffixProcess) | Some(RuleShape::Realizational) => Some(rule),
            Some(RuleShape::Compounding) | None => None,
        }
    }

    /// Whether the slot lists the rule, and belongs to the stratum the trace claims for it.
    fn site_is_established(
        &self,
        slot: TraceSlotId,
        rule: MRuleId,
        stratum: TraceStratumId,
    ) -> SiteVerdict {
        match self.index.slot_stratum(slot) {
            None => SiteVerdict::Unknown,
            Some(owning) if owning != stratum => SiteVerdict::Refuses,
            Some(_) => self.index.slot_admits(slot, rule),
        }
    }
}

fn root_index_in_range(identity: &Candidate) -> bool {
    identity.root_index >= 0 && (identity.root_index as usize) < identity.morphemes.len()
}

/// Does the witness describe the very morphemes the identity names, in order?
fn units_match_identity(identity: &Candidate, witness: &CandidateWitness) -> bool {
    witness.units.len() == identity.morphemes.len()
        && witness
            .units
            .iter()
            .zip(identity.morphemes.iter())
            .all(|(unit, &morpheme)| unit.morpheme == morpheme)
}

fn proof_of(
    pass_id: StablePassId,
    rule_id: StableRuleId,
    identity: &Candidate,
    witness: &CandidateWitness,
    unit_indices: Vec<usize>,
    claim: ProofClaim,
) -> PassDecision {
    PassDecision::Reject(RejectionProof {
        pass_id,
        rule_id,
        category: claim.category(),
        witness: ProofWitness {
            candidate_identity: identity.clone(),
            witness_id: witness.witness_id,
            grammar_revision: witness.provenance.grammar_revision,
            lexicon_revision: witness.lexicon_revision,
            lexical_origin: witness.lexical_origin,
            unit_indices,
            claim,
        },
    })
}

fn reject_identity(
    identity: &Candidate,
    witness: &CandidateWitness,
    defect: IdentityDefect,
) -> PassDecision {
    proof_of(
        OWNERSHIP_PASS,
        IDENTITY_RULE,
        identity,
        witness,
        Vec::new(),
        ProofClaim::MalformedIdentity(defect),
    )
}

fn reject_ownership(
    identity: &Candidate,
    witness: &CandidateWitness,
    unit_index: usize,
    morpheme: MorphemeId,
    role: TraceRole,
) -> PassDecision {
    proof_of(
        OWNERSHIP_PASS,
        OWNER_RULE,
        identity,
        witness,
        vec![unit_index],
        ProofClaim::ImpossibleOwnership {
            unit_index,
            morpheme,
            role,
        },
    )
}

fn reject_transition(
    identity: &Candidate,
    witness: &CandidateWitness,
    step: &SitedStep,
) -> PassDecision {
    proof_of(
        TRANSITION_PASS,
        SITE_RULE,
        identity,
        witness,
        vec![step.from_unit_index, step.to_unit_index],
        ProofClaim::ForbiddenTransition {
            from_unit_index: step.from_unit_index,
            to_unit_index: step.to_unit_index,
            from_slot: step.from_slot,
            to_slot: step.to_slot,
            stratum: step.stratum,
        },
    )
}
