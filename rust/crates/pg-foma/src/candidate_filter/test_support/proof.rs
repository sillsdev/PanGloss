//! Re-derives a recorded rejection against the witness it names, independently of the pass.
//!
//! Nothing here runs while filtering. A run acts on a pass's rejection immediately and records the
//! proof; this re-derives that recorded evidence afterwards, which asserts the stronger property
//! that no invalid proof was ever produced rather than that the pipeline declined to act on one.

use std::collections::{BTreeMap, BTreeSet};

use pg_grammar::model::{AllomorphId, MorphemeId};

use crate::candidate_filter::decision::{
    AdmissibleProof, IdentityDefect, ProofClaim, ProofVerificationError, ProofWitness,
    RejectionProof, SpanDefect, StablePassId, TraceFactKind,
};
use crate::candidate_filter::model::{
    CandidateWitness, LocalEvent, NonEmpty, PartnerClassId, SurfaceSpan, TraceFact, TraceRole,
    TraceSlotId, TraceStratumId, TraceUnit,
};
use crate::candidate_filter::passes::CandidateFilterPass;
use crate::tags::Candidate;

/// One rejection as a run recorded it, together with the witness it was emitted for.
///
/// `emitting_pass` is the pass the pipeline ran, which is not necessarily the pass the proof names
/// itself after. Keeping the two apart is what catches a proof stamped with another pass's ID: the
/// ledger's own attribution is the trustworthy half of that comparison.
pub struct RecordedRejection<'a> {
    pub identity: &'a Candidate,
    pub witness: &'a CandidateWitness,
    pub emitting_pass: StablePassId,
    pub proof: &'a RejectionProof,
}

/// Re-derives recorded rejections, holding each to the rule population its pass declared.
///
/// Built from the passes themselves, so a proof cannot also supply the rules that admit it.
pub struct RejectionProofVerifier {
    admissible: BTreeMap<StablePassId, BTreeSet<AdmissibleProof>>,
}

impl RejectionProofVerifier {
    /// Collects what each pass says it may decide, and holds every one of them to it.
    pub fn of_passes(passes: &[Box<dyn CandidateFilterPass>]) -> Self {
        let mut admissible: BTreeMap<StablePassId, BTreeSet<AdmissibleProof>> = BTreeMap::new();
        for pass in passes {
            admissible
                .entry(pass.id())
                .or_default()
                .extend(pass.admissible_proofs());
        }
        Self { admissible }
    }

    /// Re-derives every recorded rejection, reporting one error per record that fails.
    pub fn verify_recorded(
        &self,
        records: &[RecordedRejection<'_>],
    ) -> Result<(), Vec<ProofVerificationError>> {
        self.verify_all(records, true)
    }

    /// The same, minus re-deriving each claim from its witness.
    ///
    /// Stopping at the envelope is what measures the claim re-derivation's worth: a forged payload
    /// passes this and fails `verify_recorded`.
    pub fn verify_recorded_envelopes(
        &self,
        records: &[RecordedRejection<'_>],
    ) -> Result<(), Vec<ProofVerificationError>> {
        self.verify_all(records, false)
    }

    fn verify_all(
        &self,
        records: &[RecordedRejection<'_>],
        re_derive_claim: bool,
    ) -> Result<(), Vec<ProofVerificationError>> {
        let errors: Vec<ProofVerificationError> = records
            .iter()
            .filter_map(|record| self.verify_one(record, re_derive_claim).err())
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn verify_one(
        &self,
        record: &RecordedRejection<'_>,
        re_derive_claim: bool,
    ) -> Result<(), ProofVerificationError> {
        let proof = record.proof;
        if proof.pass_id != record.emitting_pass {
            return Err(ProofVerificationError::PassIdMismatch {
                declared: record.emitting_pass,
                claimed: proof.pass_id,
            });
        }
        check_envelope(record.identity, record.witness, &proof.witness)?;
        check_declared_category(proof)?;
        self.check_rule(record.emitting_pass, proof)?;
        if re_derive_claim {
            check_claim(record.witness, &proof.witness)?;
        }
        Ok(())
    }

    /// Keyed by the pass that ran, so a proof cannot borrow another pass's declared rules.
    fn check_rule(
        &self,
        emitting_pass: StablePassId,
        proof: &RejectionProof,
    ) -> Result<(), ProofVerificationError> {
        let declared = self
            .admissible
            .get(&emitting_pass)
            .ok_or(ProofVerificationError::UnrecognizedRule(proof.rule_id))?;
        let wanted = AdmissibleProof {
            rule_id: proof.rule_id,
            category: proof.category,
        };
        if declared.contains(&wanted) {
            return Ok(());
        }
        if declared.iter().any(|entry| entry.rule_id == proof.rule_id) {
            return Err(ProofVerificationError::CategoryNotSupported(proof.category));
        }
        Err(ProofVerificationError::UnrecognizedRule(proof.rule_id))
    }
}

fn check_envelope(
    identity: &Candidate,
    witness: &CandidateWitness,
    proof: &ProofWitness,
) -> Result<(), ProofVerificationError> {
    if &proof.candidate_identity != identity {
        return Err(ProofVerificationError::CandidateIdentityMismatch);
    }
    if proof.witness_id != witness.witness_id {
        return Err(ProofVerificationError::WitnessIdMismatch);
    }
    if proof.grammar_revision != witness.provenance.grammar_revision {
        return Err(ProofVerificationError::GrammarRevisionMismatch);
    }
    if proof.lexicon_revision != witness.lexicon_revision {
        return Err(ProofVerificationError::LexiconRevisionMismatch);
    }
    if proof.lexical_origin != witness.lexical_origin {
        return Err(ProofVerificationError::LexicalOriginMismatch);
    }
    for &index in &proof.unit_indices {
        if index >= witness.units.len() {
            return Err(ProofVerificationError::UnitIndexOutOfRange {
                index,
                units: witness.units.len(),
            });
        }
    }
    Ok(())
}

fn check_declared_category(proof: &RejectionProof) -> Result<(), ProofVerificationError> {
    let claimed = proof.witness.claim.category();
    if claimed == proof.category {
        return Ok(());
    }
    Err(ProofVerificationError::CategoryClaimMismatch {
        declared: proof.category,
        claimed,
    })
}

fn check_claim(
    witness: &CandidateWitness,
    proof: &ProofWitness,
) -> Result<(), ProofVerificationError> {
    let cited: BTreeSet<usize> = proof.unit_indices.iter().copied().collect();
    for index in proof.claim.cited_units() {
        if !cited.contains(&index) {
            return Err(ProofVerificationError::UnitNotCited { index });
        }
    }

    match &proof.claim {
        ProofClaim::MalformedIdentity(defect) => check_identity(&proof.candidate_identity, *defect),
        ProofClaim::ImpossibleOwnership {
            unit_index,
            morpheme,
            role,
        } => {
            let unit = &witness.units[*unit_index];
            check_morpheme(unit, *unit_index, *morpheme)?;
            check_role(unit, *unit_index, *role)
        }
        ProofClaim::ForbiddenTransition {
            from_unit_index,
            to_unit_index,
            from_slot,
            to_slot,
            stratum,
        } => check_transition(
            witness,
            (*from_unit_index, *to_unit_index),
            (*from_slot, *to_slot),
            *stratum,
        ),
        ProofClaim::MissingRequiredPartner { opened_at, class } => {
            check_missing_partner(witness, *opened_at, *class)
        }
        ProofClaim::StaticCoOccurrenceViolation {
            left_unit_index,
            right_unit_index,
            left_morpheme,
            right_morpheme,
            eliminated_pairs,
        } => {
            let left = &witness.units[*left_unit_index];
            let right = &witness.units[*right_unit_index];
            check_morpheme(left, *left_unit_index, *left_morpheme)?;
            check_morpheme(right, *right_unit_index, *right_morpheme)?;
            let left_set = established_allomorphs(left, *left_unit_index)?;
            let right_set = established_allomorphs(right, *right_unit_index)?;
            check_pairs_exhausted(left_set, right_set, eliminated_pairs, *left_unit_index)
        }
        ProofClaim::NoCompatibleAllomorph {
            unit_index,
            morpheme,
            eliminated,
        } => {
            let unit = &witness.units[*unit_index];
            check_morpheme(unit, *unit_index, *morpheme)?;
            let known = established_allomorphs(unit, *unit_index)?;
            check_allomorphs_exhausted(known, eliminated, *unit_index)
        }
        ProofClaim::StaticSignatureConflict {
            unit_index,
            morpheme,
            eliminated,
            conflicting_unit_index,
            conflicting_morpheme,
            conflicting_eliminated,
        } => {
            if !witness.deferred.is_empty() {
                return Err(ProofVerificationError::DeferredFeaturesUnresolved);
            }
            let unit = &witness.units[*unit_index];
            let other = &witness.units[*conflicting_unit_index];
            check_morpheme(unit, *unit_index, *morpheme)?;
            check_morpheme(other, *conflicting_unit_index, *conflicting_morpheme)?;
            check_allomorphs_exhausted(
                established_allomorphs(unit, *unit_index)?,
                eliminated,
                *unit_index,
            )?;
            check_allomorphs_exhausted(
                established_allomorphs(other, *conflicting_unit_index)?,
                conflicting_eliminated,
                *conflicting_unit_index,
            )
        }
        ProofClaim::ImpossibleSurfaceSpan {
            unit_index,
            span,
            defect,
        } => check_span(witness, *unit_index, *span, *defect),
        ProofClaim::ImpossibleLocalEnvironment {
            unit_index,
            events,
            neighbor_unit_index,
            neighbor_events,
        } => {
            check_adjacent(*unit_index, *neighbor_unit_index)?;
            check_events(witness, *unit_index, events)?;
            check_events(witness, *neighbor_unit_index, neighbor_events)
        }
    }
}

fn check_identity(
    identity: &Candidate,
    defect: IdentityDefect,
) -> Result<(), ProofVerificationError> {
    let established = match defect {
        IdentityDefect::EmptyMorphemeSequence => identity.morphemes.is_empty(),
        IdentityDefect::RootIndexOutOfRange {
            root_index,
            morphemes,
        } => {
            root_index == identity.root_index
                && morphemes == identity.morphemes.len()
                && !root_position_is_possible(identity)
        }
    };
    if established {
        Ok(())
    } else {
        Err(ProofVerificationError::IdentityDefectNotEstablished)
    }
}

/// `-1` is the established "no root at all", so only it and a real index are possible.
fn root_position_is_possible(identity: &Candidate) -> bool {
    identity.root_index == -1
        || (identity.root_index >= 0 && (identity.root_index as usize) < identity.morphemes.len())
}

fn check_morpheme(
    unit: &TraceUnit,
    unit_index: usize,
    morpheme: MorphemeId,
) -> Result<(), ProofVerificationError> {
    if unit.morpheme == morpheme {
        Ok(())
    } else {
        Err(ProofVerificationError::MorphemeMismatch { unit_index })
    }
}

fn check_role(
    unit: &TraceUnit,
    unit_index: usize,
    role: TraceRole,
) -> Result<(), ProofVerificationError> {
    if *established(&unit.role, unit_index, TraceFactKind::Role)? == role {
        Ok(())
    } else {
        Err(ProofVerificationError::FactMismatch {
            unit_index,
            fact: TraceFactKind::Role,
        })
    }
}

fn check_transition(
    witness: &CandidateWitness,
    units: (usize, usize),
    slots: (TraceSlotId, TraceSlotId),
    stratum: TraceStratumId,
) -> Result<(), ProofVerificationError> {
    let (from_index, to_index) = units;
    check_adjacent(from_index, to_index)?;
    check_slot(&witness.units[from_index], from_index, slots.0)?;
    check_slot(&witness.units[to_index], to_index, slots.1)?;
    check_stratum(&witness.units[from_index], from_index, stratum)?;
    check_stratum(&witness.units[to_index], to_index, stratum)
}

/// A transition claim reads a step, so the two units have to be one step apart.
fn check_adjacent(from: usize, to: usize) -> Result<(), ProofVerificationError> {
    if to.abs_diff(from) == 1 {
        Ok(())
    } else {
        Err(ProofVerificationError::UnitsNotAdjacent { from, to })
    }
}

fn check_slot(
    unit: &TraceUnit,
    unit_index: usize,
    slot: TraceSlotId,
) -> Result<(), ProofVerificationError> {
    match established(&unit.slot, unit_index, TraceFactKind::Slot)? {
        Some(established) if *established == slot => Ok(()),
        _ => Err(ProofVerificationError::FactMismatch {
            unit_index,
            fact: TraceFactKind::Slot,
        }),
    }
}

fn check_stratum(
    unit: &TraceUnit,
    unit_index: usize,
    stratum: TraceStratumId,
) -> Result<(), ProofVerificationError> {
    match established(&unit.stratum, unit_index, TraceFactKind::Stratum)? {
        Some(established) if *established == stratum => Ok(()),
        _ => Err(ProofVerificationError::FactMismatch {
            unit_index,
            fact: TraceFactKind::Stratum,
        }),
    }
}

fn check_missing_partner(
    witness: &CandidateWitness,
    opened_at: usize,
    class: PartnerClassId,
) -> Result<(), ProofVerificationError> {
    let opening = established(
        &witness.units[opened_at].local_events,
        opened_at,
        TraceFactKind::LocalEvents,
    )?;
    if !opening.contains(&LocalEvent::PartnerOpen(class)) {
        return Err(ProofVerificationError::FactMismatch {
            unit_index: opened_at,
            fact: TraceFactKind::LocalEvents,
        });
    }
    for (unit_index, unit) in witness.units.iter().enumerate() {
        let events = established(&unit.local_events, unit_index, TraceFactKind::LocalEvents)?;
        if events.contains(&LocalEvent::PartnerClose(class)) {
            return Err(ProofVerificationError::PartnerAlreadyClosed { unit_index });
        }
    }
    Ok(())
}

fn check_span(
    witness: &CandidateWitness,
    unit_index: usize,
    span: SurfaceSpan,
    defect: SpanDefect,
) -> Result<(), ProofVerificationError> {
    let claimed = established_span(witness, unit_index)?;
    if claimed != span {
        return Err(ProofVerificationError::FactMismatch {
            unit_index,
            fact: TraceFactKind::SurfaceSpan,
        });
    }
    let established = match defect {
        SpanDefect::EndBeforeStart => span.end < span.start,
        SpanDefect::OverlapsUnit { other_unit_index } => {
            let other = established_span(witness, other_unit_index)?;
            !span.is_empty()
                && !other.is_empty()
                && span.start < other.end
                && other.start < span.end
        }
    };
    if established {
        Ok(())
    } else {
        Err(ProofVerificationError::SpanDefectNotEstablished { unit_index })
    }
}

fn established_span(
    witness: &CandidateWitness,
    unit_index: usize,
) -> Result<SurfaceSpan, ProofVerificationError> {
    match established(
        &witness.units[unit_index].surface_span,
        unit_index,
        TraceFactKind::SurfaceSpan,
    )? {
        Some(span) => Ok(*span),
        None => Err(ProofVerificationError::FactMismatch {
            unit_index,
            fact: TraceFactKind::SurfaceSpan,
        }),
    }
}

fn check_events(
    witness: &CandidateWitness,
    unit_index: usize,
    events: &[LocalEvent],
) -> Result<(), ProofVerificationError> {
    let known = established(
        &witness.units[unit_index].local_events,
        unit_index,
        TraceFactKind::LocalEvents,
    )?;
    if known.as_slice() == events {
        Ok(())
    } else {
        Err(ProofVerificationError::FactMismatch {
            unit_index,
            fact: TraceFactKind::LocalEvents,
        })
    }
}

fn established_allomorphs<'a>(
    unit: &'a TraceUnit,
    unit_index: usize,
) -> Result<&'a NonEmpty<AllomorphId>, ProofVerificationError> {
    established(&unit.allomorphs, unit_index, TraceFactKind::Allomorphs)
}

fn check_allomorphs_exhausted(
    known: &NonEmpty<AllomorphId>,
    eliminated: &[AllomorphId],
    unit_index: usize,
) -> Result<(), ProofVerificationError> {
    let known: BTreeSet<AllomorphId> = known.iter().copied().collect();
    let eliminated: BTreeSet<AllomorphId> = eliminated.iter().copied().collect();
    if known == eliminated {
        Ok(())
    } else {
        Err(ProofVerificationError::AlternativesNotExhausted { unit_index })
    }
}

fn check_pairs_exhausted(
    left: &NonEmpty<AllomorphId>,
    right: &NonEmpty<AllomorphId>,
    eliminated: &[(AllomorphId, AllomorphId)],
    unit_index: usize,
) -> Result<(), ProofVerificationError> {
    let mut required: BTreeSet<(AllomorphId, AllomorphId)> = BTreeSet::new();
    for &left_allomorph in left.iter() {
        for &right_allomorph in right.iter() {
            required.insert((left_allomorph, right_allomorph));
        }
    }
    let eliminated: BTreeSet<(AllomorphId, AllomorphId)> = eliminated.iter().copied().collect();
    if required == eliminated {
        Ok(())
    } else {
        Err(ProofVerificationError::AlternativesNotExhausted { unit_index })
    }
}

/// A deferred fact is the absence of a claim, so nothing may be proved from it.
fn established<'a, T>(
    fact: &'a TraceFact<T>,
    unit_index: usize,
    kind: TraceFactKind,
) -> Result<&'a T, ProofVerificationError> {
    fact.known()
        .ok_or(ProofVerificationError::FactNotEstablished {
            unit_index,
            fact: kind,
        })
}
