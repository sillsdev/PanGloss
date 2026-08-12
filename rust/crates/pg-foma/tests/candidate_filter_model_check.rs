//! Small-domain enumeration over the structural passes, compared against a reference predicate.

use std::sync::Arc;
use std::time::Instant;

use pg_foma::candidate_filter::decision::{PassDecision, ProofVerificationError};
use pg_foma::candidate_filter::index::FilterIndex;
use pg_foma::candidate_filter::model::{
    CandidateWitness, DeferredFactReason, FeatureSet, LexicalOrigin, ProposalProducer,
    ProposalProvenance, TraceFact, TraceRole, TraceSlotId, TraceStratumId, TraceUnit, WitnessId,
};
use pg_foma::candidate_filter::passes::structural::{OwnershipPass, StructuralTransitionPass};
use pg_foma::candidate_filter::pipeline::{FilterContext, FilterMode};
use pg_foma::candidate_filter::test_support::{RecordedRejection, RejectionProofVerifier};
use pg_foma::candidate_filter::CandidateFilterPass;
use pg_foma::tags::Candidate;
use pg_grammar::model::{Grammar, MorphemeId};

#[path = "common/filter_fixture.rs"]
mod fixture;

const MAX_TRACE_LENGTH: usize = 3;
const NO_ROOT: i32 = -1;

/// How each pass decided over the whole enumeration.
#[derive(Default)]
struct Tally {
    keeps: u64,
    defers: u64,
    rejects: u64,
    evaluations: u64,
}

impl Tally {
    fn record(&mut self, decision: &PassDecision) {
        self.evaluations += 1;
        match decision {
            PassDecision::Keep => self.keeps += 1,
            PassDecision::Defer(_) => self.defers += 1,
            PassDecision::Reject(_) => self.rejects += 1,
        }
    }

    fn report(&self) -> String {
        format!(
            "{} evaluations: {} kept, {} deferred, {} rejected",
            self.evaluations, self.keeps, self.defers, self.rejects
        )
    }
}

fn witness_of(units: Vec<TraceUnit>) -> CandidateWitness {
    CandidateWitness {
        witness_id: WitnessId(1),
        lexical_origin: LexicalOrigin::StaticGrammar,
        lexicon_revision: 0,
        units,
        deferred: FeatureSet::empty(),
        provenance: ProposalProvenance {
            producer: ProposalProducer::SyntheticFixture,
            grammar_revision: 0,
        },
    }
}

fn opaque_unit(morpheme: MorphemeId) -> TraceUnit {
    TraceUnit {
        morpheme,
        role: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
        allomorphs: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
        slot: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
        stratum: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
        surface_span: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
        local_events: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
    }
}

/// Every trace of length `0..=MAX_TRACE_LENGTH` over `alphabet`, in a fixed order.
fn traces<T: Clone>(alphabet: &[T]) -> Vec<Vec<T>> {
    let mut out: Vec<Vec<T>> = vec![Vec::new()];
    let mut frontier: Vec<Vec<T>> = vec![Vec::new()];
    for _ in 0..MAX_TRACE_LENGTH {
        let mut next = Vec::new();
        for trace in &frontier {
            for symbol in alphabet {
                let mut extended = trace.clone();
                extended.push(symbol.clone());
                next.push(extended);
            }
        }
        out.extend(next.iter().cloned());
        frontier = next;
    }
    out
}

fn verify(
    pass: &dyn CandidateFilterPass,
    verifier: &RejectionProofVerifier,
    identity: &Candidate,
    witness: &CandidateWitness,
    decision: &PassDecision,
) -> Result<(), Vec<ProofVerificationError>> {
    let PassDecision::Reject(proof) = decision else {
        return Ok(());
    };
    verifier.verify_recorded(&[RecordedRejection {
        identity,
        witness,
        emitting_pass: pass.id(),
        proof,
    }])
}

// --- ownership -----------------------------------------------------------------------------

/// One enumerated trace position: which morpheme sits there and what role it claims.
#[derive(Clone)]
struct OwnershipSymbol {
    morpheme: MorphemeId,
    role: Option<TraceRole>,
}

fn ownership_alphabet(g: &Grammar) -> Vec<OwnershipSymbol> {
    let morphemes = [
        fixture::morpheme_of(g, "eRoot"),
        fixture::morpheme_of(g, "mrP0"),
        fixture::unowned_morpheme(g),
    ];
    let roles = [
        Some(TraceRole::Root),
        Some(TraceRole::Prefix),
        Some(TraceRole::Suffix),
        None,
    ];
    morphemes
        .iter()
        .flat_map(|&morpheme| {
            roles
                .iter()
                .map(move |&role| OwnershipSymbol { morpheme, role })
        })
        .collect()
}

/// Would pin resolution accept this identity? Read off the grammar tables directly.
fn reference_pins_resolve(g: &Grammar, morphemes: &[MorphemeId], root_index: i32) -> bool {
    if root_index < 0 || root_index as usize >= morphemes.len() {
        return false;
    }
    let is_entry = |m: MorphemeId| g.entries.iter().any(|entry| entry.morpheme == m);
    let is_rule = |m: MorphemeId| {
        g.mrules.iter().any(|rule| match rule {
            pg_grammar::model::MorphRuleDef::AffixProcess(def) => def.morpheme == m,
            pg_grammar::model::MorphRuleDef::Realizational(def) => def.morpheme == m,
            pg_grammar::model::MorphRuleDef::Compounding(_) => false,
        })
    };
    if !is_entry(morphemes[root_index as usize]) {
        return false;
    }
    morphemes
        .iter()
        .enumerate()
        .all(|(index, &m)| index == root_index as usize || is_entry(m) || is_rule(m))
}

#[test]
fn ownership_never_rejects_a_trace_the_reference_accepts() {
    let g = fixture::grammar();
    let index = Arc::new(FilterIndex::build(&g));
    let passes: Vec<Box<dyn CandidateFilterPass>> = vec![Box::new(OwnershipPass::new(index))];
    let verifier = RejectionProofVerifier::of_passes(&passes);
    let pass = passes[0].as_ref();

    let mut tally = Tally::default();
    let started = Instant::now();
    for trace in traces(&ownership_alphabet(&g)) {
        let units: Vec<TraceUnit> = trace
            .iter()
            .map(|symbol| match symbol.role {
                Some(role) => TraceUnit {
                    role: TraceFact::Known(role),
                    ..opaque_unit(symbol.morpheme)
                },
                None => opaque_unit(symbol.morpheme),
            })
            .collect();
        let morphemes: Vec<MorphemeId> = trace.iter().map(|symbol| symbol.morpheme).collect();
        let witness = witness_of(units);

        for root_index in root_positions(morphemes.len()) {
            let identity = Candidate {
                morphemes: morphemes.clone(),
                root_index,
            };
            let context = FilterContext::new(&identity, 0, FilterMode::Enforce);
            let decision = pass.evaluate(&context, &witness);
            tally.record(&decision);

            match decision {
                PassDecision::Reject(_) => assert!(
                    !reference_pins_resolve(&g, &morphemes, root_index),
                    "rejected a trace the reference accepts: {morphemes:?} root {root_index}"
                ),
                PassDecision::Keep => assert!(
                    reference_pins_resolve(&g, &morphemes, root_index),
                    "kept a trace the reference refuses: {morphemes:?} root {root_index}"
                ),
                PassDecision::Defer(_) => {}
            }
            assert_eq!(
                verify(pass, &verifier, &identity, &witness, &decision),
                Ok(()),
                "a rejection failed to re-derive: {morphemes:?} root {root_index}"
            );
        }
    }
    let elapsed = started.elapsed();

    assert!(
        tally.rejects > 0,
        "ownership never fired: {}",
        tally.report()
    );
    assert!(tally.keeps > 0, "ownership never kept: {}", tally.report());
    assert!(
        tally.defers > 0,
        "ownership never deferred: {}",
        tally.report()
    );
    println!(
        "ownership {} in {:?} ({:?} per evaluation)",
        tally.report(),
        elapsed,
        elapsed / u32::try_from(tally.evaluations).unwrap_or(u32::MAX)
    );
}

/// Every root position the enumeration tries: absent, each real index, and one past the end.
fn root_positions(morphemes: usize) -> Vec<i32> {
    let mut out = vec![NO_ROOT];
    out.extend((0..=morphemes).map(|index| index as i32));
    out
}

// --- transitions ---------------------------------------------------------------------------

/// One enumerated trace position: which rule morpheme sits there, and the site it claims.
#[derive(Clone)]
struct TransitionSymbol {
    morpheme: MorphemeId,
    slot: Option<Option<TraceSlotId>>,
    stratum: Option<TraceStratumId>,
}

struct Sites {
    slots: Vec<(TraceSlotId, u16, u8)>,
    strata: Vec<(TraceStratumId, u8)>,
}

fn sites(g: &Grammar, index: &FilterIndex) -> Sites {
    let mut slots = Vec::new();
    let mut strata = Vec::new();
    for (template, def) in g.templates.iter().enumerate() {
        let template = template as u16;
        for slot in 0..def.slots.len() {
            let slot = slot as u8;
            if let Some(id) = index.slot_id(template, slot) {
                slots.push((id, template, slot));
            }
        }
        let stratum = fixture::stratum_of_template(g, template);
        if let Some(id) = index.stratum_id(stratum) {
            if !strata.iter().any(|&(_, s)| s == stratum) {
                strata.push((id, stratum));
            }
        }
    }
    Sites { slots, strata }
}

fn transition_alphabet(g: &Grammar, sites: &Sites) -> Vec<TransitionSymbol> {
    let morphemes = [
        fixture::morpheme_of(g, "mrP0"),
        fixture::morpheme_of(g, "mrP1"),
    ];
    let mut slot_facts: Vec<Option<Option<TraceSlotId>>> = sites
        .slots
        .iter()
        .map(|&(id, _, _)| Some(Some(id)))
        .collect();
    slot_facts.push(Some(None));
    slot_facts.push(None);
    let mut stratum_facts: Vec<Option<TraceStratumId>> =
        sites.strata.iter().map(|&(id, _)| Some(id)).collect();
    stratum_facts.push(None);

    let mut out = Vec::new();
    for &morpheme in &morphemes {
        for slot in &slot_facts {
            for stratum in &stratum_facts {
                out.push(TransitionSymbol {
                    morpheme,
                    slot: *slot,
                    stratum: *stratum,
                });
            }
        }
    }
    out
}

/// Could this morpheme have been produced at the site it claims? Read off the grammar tables.
fn reference_site_possible(g: &Grammar, sites: &Sites, symbol: &TransitionSymbol) -> bool {
    let (Some(Some(slot)), Some(stratum)) = (symbol.slot, symbol.stratum) else {
        return true;
    };
    let Some(&(_, template, slot)) = sites.slots.iter().find(|&&(id, _, _)| id == slot) else {
        return true;
    };
    let Some(&(_, stratum)) = sites.strata.iter().find(|&&(id, _)| id == stratum) else {
        return true;
    };
    if fixture::stratum_of_template(g, template) != stratum {
        return false;
    }
    fixture::site_of(g, fixture::rule_of(g, symbol.morpheme)) == (template, slot)
}

/// A step is only refutable when both its ends claim a site in one shared stratum.
fn reference_step_possible(
    g: &Grammar,
    sites: &Sites,
    from: &TransitionSymbol,
    to: &TransitionSymbol,
) -> bool {
    if from.stratum.is_none() || from.stratum != to.stratum {
        return true;
    }
    if !matches!(from.slot, Some(Some(_))) || !matches!(to.slot, Some(Some(_))) {
        return true;
    }
    reference_site_possible(g, sites, from) && reference_site_possible(g, sites, to)
}

#[test]
fn transitions_never_reject_a_trace_the_reference_accepts() {
    let g = fixture::grammar();
    let index = Arc::new(FilterIndex::build(&g));
    let sites = sites(&g, &index);
    let passes: Vec<Box<dyn CandidateFilterPass>> =
        vec![Box::new(StructuralTransitionPass::new(index))];
    let verifier = RejectionProofVerifier::of_passes(&passes);
    let pass = passes[0].as_ref();

    let mut tally = Tally::default();
    let started = Instant::now();
    for trace in traces(&transition_alphabet(&g, &sites)) {
        let units: Vec<TraceUnit> = trace
            .iter()
            .map(|symbol| TraceUnit {
                slot: match symbol.slot {
                    Some(slot) => TraceFact::Known(slot),
                    None => TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
                },
                stratum: match symbol.stratum {
                    Some(stratum) => TraceFact::Known(Some(stratum)),
                    None => TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
                },
                ..opaque_unit(symbol.morpheme)
            })
            .collect();
        let identity = Candidate {
            morphemes: trace.iter().map(|symbol| symbol.morpheme).collect(),
            root_index: NO_ROOT,
        };
        let witness = witness_of(units);

        let context = FilterContext::new(&identity, 0, FilterMode::Enforce);
        let decision = pass.evaluate(&context, &witness);
        tally.record(&decision);

        let accepted = trace
            .windows(2)
            .all(|pair| reference_step_possible(&g, &sites, &pair[0], &pair[1]));
        match decision {
            PassDecision::Reject(_) => assert!(
                !accepted,
                "rejected a trace the reference accepts: {:?}",
                identity.morphemes
            ),
            PassDecision::Keep => assert!(
                accepted,
                "kept a trace the reference refuses: {:?}",
                identity.morphemes
            ),
            PassDecision::Defer(_) => {}
        }
        assert_eq!(
            verify(pass, &verifier, &identity, &witness, &decision),
            Ok(()),
            "a rejection failed to re-derive: {:?}",
            identity.morphemes
        );
    }
    let elapsed = started.elapsed();

    assert!(
        tally.rejects > 0,
        "transitions never fired: {}",
        tally.report()
    );
    assert!(
        tally.keeps > 0,
        "transitions never kept: {}",
        tally.report()
    );
    assert!(
        tally.defers > 0,
        "transitions never deferred: {}",
        tally.report()
    );
    println!(
        "transitions {} in {:?} ({:?} per evaluation)",
        tally.report(),
        elapsed,
        elapsed / u32::try_from(tally.evaluations).unwrap_or(u32::MAX)
    );
}
