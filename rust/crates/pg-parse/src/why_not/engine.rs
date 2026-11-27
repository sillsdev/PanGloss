//! The why-not search: a restricted traced reparse per morph sequence, then the first failure classified from the trace.

use pg_featstruct::{FeatureStruct, FeatureValue};
use pg_grammar_model::model::{
    AllomorphId, AllomorphOwner, CoOccurrenceAdjacency, EnvironmentSource, Grammar, MRuleId,
    MorphRuleDef, MorphemeId, SynFeatureKind,
};
use pg_rules::trace::{
    FailureContext, FailureReason, PartialParseCause, RejectionEvidence, TraceHandle, TraceNode,
    TraceObject, TraceSource, TraceType, TreeTraceSink,
};
use pg_rules::word::Word;

use super::{
    AcceptedAnalysis, AuthoredAnalysis, EnvContext, ExpectedMorph, FailureEnvironment, FailureKind,
    FailureObject, FailureSlot, FeatureDiff, FirstFailure, MorphRef, ObjectType, Progress,
    Provenance, Stop, StopReason, WhyNotOutcome,
};
use crate::identity::AnalysisIdentity;
use crate::pins::{build_morpheme_owners, parse_word_pinned_traced, resolve_pins, MorphemeOwner};
use crate::{Morpher, WordAnalysis};

/// The most concrete sequences one candidate may expand to before the engine stops asking.
pub(super) const MAX_SEQUENCES: usize = 16;

/// One position of a concrete sequence: the morpheme and, when named, its allomorph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pin {
    pub morpheme: MorphemeId,
    pub allomorph: Option<AllomorphId>,
}

/// A candidate's positions after resolving every choice, in surface order.
pub(super) struct ResolvedSequence {
    pub pins: Vec<Pin>,
    pub choices: Vec<ExpectedMorph>,
}

pub(super) struct Searcher<'a> {
    g: &'a Grammar,
    morpher: &'a Morpher<'a>,
    owners: Vec<Option<MorphemeOwner>>,
}

impl<'a> Searcher<'a> {
    pub(super) fn new(g: &'a Grammar, morpher: &'a Morpher<'a>) -> Self {
        Self {
            g,
            morpher,
            owners: build_morpheme_owners(g),
        }
    }

    pub(super) fn morpheme_name(&self, m: MorphemeId) -> Option<String> {
        let key = &self.g.morphemes.get(m.0 as usize)?.xml_key;
        (!key.is_empty()).then(|| key.clone())
    }

    fn morph_ref(&self, morpheme: MorphemeId, allomorph: Option<AllomorphId>) -> Option<MorphRef> {
        Some(MorphRef {
            morpheme: self.morpheme_name(morpheme)?,
            allomorph: allomorph.and_then(|a| self.g.allomorph_xml_key(a).map(str::to_string)),
            guid: super::morpheme_guid(self.g, morpheme),
            form_guid: allomorph.and_then(|a| self.form_guid(a)),
        })
    }

    /// The FieldWorks `MoForm` guid of an allomorph; `None` for HC XML and for an allomorph without a source form.
    fn form_guid(&self, a: AllomorphId) -> Option<String> {
        self.g
            .allomorph_sources
            .get(a.0 as usize)?
            .form_guids
            .iter()
            .flatten()
            .next()
            .cloned()
    }

    /// The authored source of the `index`th environment of an allomorph.
    fn environment_source(&self, a: AllomorphId, index: Option<u32>) -> Option<&EnvironmentSource> {
        let envs = match *self.g.allomorph_owners.get(a.0 as usize)? {
            AllomorphOwner::Root(le, i) => {
                &self
                    .g
                    .entries
                    .get(le.0 as usize)?
                    .allomorphs
                    .get(i as usize)?
                    .environments
            }
            AllomorphOwner::Affix(rule, i) => {
                &self
                    .g
                    .mrules
                    .get(rule.0 as usize)?
                    .affix_allomorphs()?
                    .get(i as usize)?
                    .environments
            }
        };
        envs.get(index? as usize)?.source.as_ref()
    }

    /// Whether a morpheme was built from a FieldWorks model, whose ids are opaque guids that must not reach a sentence.
    fn is_snapshot_morpheme(&self, m: MorphemeId) -> bool {
        self.g
            .morphemes
            .get(m.0 as usize)
            .is_some_and(|i| i.source_msa_guid.is_some() || i.source_infl_type_guid.is_some())
    }

    /// The authored form text of an allomorph (halves of a circumfix joined), else of the morpheme's first allomorph.
    fn form_text(&self, m: MorphemeId, allomorph: Option<AllomorphId>) -> Option<String> {
        let first = || match self.owners.get(m.0 as usize)?.as_ref()? {
            MorphemeOwner::LexEntry(le) => {
                Some(self.g.entries.get(le.0 as usize)?.allomorphs.first()?.id)
            }
            MorphemeOwner::MRule(r) => Some(
                self.g
                    .mrules
                    .get(r.0 as usize)?
                    .affix_allomorphs()?
                    .first()?
                    .id,
            ),
        };
        let a = allomorph.or_else(first)?;
        let texts: Vec<&str> = self
            .g
            .allomorph_sources
            .get(a.0 as usize)?
            .form_texts
            .iter()
            .flatten()
            .map(String::as_str)
            .collect();
        (!texts.is_empty()).then(|| texts.join(" ... "))
    }

    /// A snapshot-route morph for `plain`: its form text and gloss (`-ak 'PL'`), the form alone without a gloss.
    fn snapshot_label(&self, m: MorphemeId, allomorph: Option<AllomorphId>) -> String {
        let gloss = self
            .g
            .morphemes
            .get(m.0 as usize)
            .and_then(|i| i.gloss.as_deref())
            .filter(|g| !g.is_empty())
            .map(|g| format!("'{}'", g.replace('\\', "\\\\").replace('\'', "\\'")));
        match (self.form_text(m, allomorph), gloss) {
            (Some(form), Some(gloss)) => format!("{form} {gloss}"),
            (Some(form), None) => form,
            (None, Some(gloss)) => gloss,
            (None, None) => "a morph with no form text or gloss".to_string(),
        }
    }

    /// A morpheme for `plain`: on a snapshot its form text and gloss; else its grammar.xml id, then its gloss (else its `MorphemeId` text) when that adds something.
    fn label(&self, m: MorphemeId) -> String {
        if self.is_snapshot_morpheme(m) {
            return self.snapshot_label(m, None);
        }
        let Some(info) = self.g.morphemes.get(m.0 as usize) else {
            return String::new();
        };
        let name = [info.gloss.as_deref(), info.morph_id.as_deref()]
            .into_iter()
            .flatten()
            .find(|n| !n.is_empty() && *n != info.xml_key);
        match name {
            Some(n) => format!(
                "{} (\"{}\")",
                info.xml_key,
                n.replace('\\', "\\\\").replace('"', "\\\"")
            ),
            None => info.xml_key.clone(),
        }
    }

    fn label_with(&self, m: MorphemeId, allomorph: Option<AllomorphId>) -> String {
        if self.is_snapshot_morpheme(m) {
            return self.snapshot_label(m, allomorph);
        }
        match allomorph.and_then(|a| self.g.allomorph_xml_key(a)) {
            Some(a) => format!("{} [{a}]", self.label(m)),
            None => self.label(m),
        }
    }

    /// The outcome for one concrete sequence.
    pub(super) fn search(&self, word: &str, seq: &ResolvedSequence) -> WhyNotOutcome {
        let morphemes: Vec<MorphemeId> = seq.pins.iter().map(|p| p.morpheme).collect();
        let roots: Vec<usize> = morphemes
            .iter()
            .enumerate()
            .filter(|(_, m)| matches!(self.owners[m.0 as usize], Some(MorphemeOwner::LexEntry(_))))
            .map(|(i, _)| i)
            .collect();
        let root_index =
            match roots[..] {
                [one] => one,
                [] => {
                    return stop(
                        StopReason::NoRoot,
                        "no position names a lexical entry, so there is no root to pin",
                    )
                }
                _ => return stop(
                    StopReason::CompoundingUnsupported,
                    "several positions name lexical entries (compounding); only one root is pinned",
                ),
            };
        let Some(pins) = resolve_pins(&self.owners, &morphemes, root_index as i32) else {
            return stop(
                StopReason::UnsupportedMorpheme,
                "a pinned morpheme is neither a lexical entry nor an affix rule",
            );
        };
        let sink = TreeTraceSink::with_failure_context();
        let outcome = parse_word_pinned_traced(self.g, self.morpher, &pins, word, &sink);
        if outcome.timed_out {
            return WhyNotOutcome::Unknown(StopReason::Deadline.into());
        }
        if outcome.capped || outcome.capture_capped || outcome.word_mem_capped {
            return WhyNotOutcome::Unknown(StopReason::WorkCap.into());
        }
        if outcome.invalid_shape {
            return stop(
                StopReason::WordNotSegmentable,
                "the word cannot be segmented with the grammar's character table",
            );
        }

        let mut same_morphemes_other_order = false;
        let mut same_order_other_allomorphs = false;
        for analysis in &outcome.structured {
            let found = analysis_sequence(analysis);
            if matches_pins(&found, &seq.pins, root_index, analysis.root_morpheme_index) {
                let used = found
                    .iter()
                    .filter_map(|&(m, a)| self.morph_ref(MorphemeId(m), Some(AllomorphId(a))))
                    .collect::<Vec<_>>();
                if used.len() == found.len() {
                    let Ok(identity) = AnalysisIdentity::project(analysis, self.g) else {
                        return stop(
                            StopReason::FailureNotExpressible,
                            "the accepted derivation uses a part of speech without a grammar id",
                        );
                    };
                    return WhyNotOutcome::Accepted(Box::new(AcceptedAnalysis {
                        analysis: AuthoredAnalysis {
                            morphs: used.clone(),
                            root_index: identity.root_index,
                            category: identity.category,
                            guessed: analysis.guessed,
                        },
                        used,
                    }));
                }
                return stop(
                    StopReason::FailureNotExpressible,
                    "the accepted derivation uses a morph without a grammar id",
                );
            }
            let in_order = found.len() == seq.pins.len()
                && analysis.root_morpheme_index == root_index as i32
                && found
                    .iter()
                    .zip(&seq.pins)
                    .all(|(&(m, _), p)| m == p.morpheme.0);
            if in_order {
                same_order_other_allomorphs = true;
                continue;
            }
            let mut a: Vec<u32> = found.iter().map(|p| p.0).collect();
            let mut b: Vec<u32> = morphemes.iter().map(|m| m.0).collect();
            a.sort_unstable();
            b.sort_unstable();
            same_morphemes_other_order |= a == b;
        }

        let mut failures = Vec::new();
        if let Some(root) = sink.root() {
            collect_failed(&sink, root, &mut failures);
        }

        let pinned: Vec<&TraceNode> = failures
            .iter()
            .filter(|n| {
                n.output.as_ref().is_some_and(|w| {
                    let seq_in_word: Vec<(u32, u32)> = self
                        .morpher
                        .allomorphs_in_morph_order(w)
                        .iter()
                        .map(|r| (r.morpheme.0, r.allomorph.0))
                        .collect();
                    matches_pins(&seq_in_word, &seq.pins, root_index, root_index as i32)
                })
            })
            .collect();

        if let Some(outcome) = self.classify_pinned(word, &pinned, &seq.pins, root_index) {
            return outcome;
        }
        if let Some(outcome) = self.classify_synthesis_block(&failures, &seq.pins) {
            return outcome;
        }
        if let Some(failure) = self.classify_derivation_stage(&sink, &seq.pins, root_index) {
            return WhyNotOutcome::Rejected(Box::new(failure));
        }
        let parses_otherwise = same_order_other_allomorphs || same_morphemes_other_order;
        if let Some(what) = other_stop(&sink).filter(|_| !parses_otherwise) {
            return stop(
                StopReason::UnclassifiedFailure,
                &format!("the trace shows {what}, which this engine does not name"),
            );
        }
        let plain = if same_order_other_allomorphs {
            "the word parses with these morphemes in this order, but with other allomorphs than the pinned ones"
        } else if same_morphemes_other_order {
            "the word parses with these morphemes, but not in this order"
        } else {
            "no derivation of the word uses these morphs in this order"
        };
        WhyNotOutcome::Rejected(Box::new(FirstFailure {
            kind: FailureKind::NoMorphMatch,
            plain: plain.to_string(),
            position: None,
            hc_reason: None,
            provenance: Provenance::Derived,
            object: None,
            morphs: seq
                .pins
                .iter()
                .filter_map(|p| self.morph_ref(p.morpheme, p.allomorph))
                .collect(),
            members: Vec::new(),
            environments: Vec::new(),
            context: None,
            environment: None,
            stem_name: None,
            features: Vec::new(),
            alternatives: None,
            gate: None,
            slot: None,
        }))
    }

    /// The answer drawn from the failing derivations that honour the pins; `None` when there is none.
    fn classify_pinned(
        &self,
        word: &str,
        pinned: &[&TraceNode],
        pins: &[Pin],
        root_index: usize,
    ) -> Option<WhyNotOutcome> {
        if pinned.iter().any(|n| {
            n.failed_allomorph.is_some() && self.failure_position(n, root_index, pins).is_none()
        }) {
            return Some(stop(
                StopReason::UnclassifiedFailure,
                "a pinned derivation fails an allomorph check this engine cannot place in the word, so it cannot say which failure is furthest",
            ));
        }
        let best = self.best_failure(pinned, pins, root_index)?;
        Some(self.classify(word, best, pins, pinned.len() - 1))
    }

    /// A blocked-allomorph stop from the synthesis loop, whose word lacks the pinned suffix and so is not among the pinned failures.
    fn classify_synthesis_block(
        &self,
        failures: &[TraceNode],
        pins: &[Pin],
    ) -> Option<WhyNotOutcome> {
        failures.iter().find_map(|n| {
            if n.failure_reason != Some(FailureReason::DisjunctiveAllomorph) {
                return None;
            }
            let Some(RejectionEvidence::AllomorphBlocked { blocked, blocker }) =
                n.failure_context.as_ref()?.evidence.as_ref()
            else {
                return None;
            };
            let morpheme = self.morpheme_of_allomorph(*blocked)?;
            pins.iter()
                .any(|p| p.morpheme == morpheme && p.allomorph.is_none_or(|a| a == *blocked))
                .then(|| {
                    match self.allomorph_blocked_failure(
                        *blocked,
                        *blocker,
                        pins,
                        Provenance::Derived,
                    ) {
                        Some(f) => WhyNotOutcome::Rejected(Box::new(f)),
                        None => stop(
                            StopReason::FailureNotExpressible,
                            "the failure names a morph or allomorph without a grammar id",
                        ),
                    }
                })
        })
    }

    /// The `other_gate` answer for an earlier allomorph that blocks the one the analysis recorded.
    fn allomorph_blocked_failure(
        &self,
        blocked: AllomorphId,
        blocker: AllomorphId,
        pins: &[Pin],
        provenance: Provenance,
    ) -> Option<FirstFailure> {
        let morpheme = self.morpheme_of_allomorph(blocked)?;
        let blocked_id = self.g.allomorph_xml_key(blocked)?.to_string();
        let plain = format!(
            "{} is blocked: the earlier {} also fits here, and HermitCrab uses only the first allomorph that fits",
            self.label_with(morpheme, Some(blocked)),
            self.label_with(morpheme, Some(blocker)),
        );
        Some(FirstFailure {
            kind: FailureKind::OtherGate,
            plain,
            position: pins.iter().position(|p| p.morpheme == morpheme),
            hc_reason: (provenance == Provenance::HcReported)
                .then(|| "DisjunctiveAllomorph".to_string()),
            provenance,
            object: Some(FailureObject {
                kind: ObjectType::Allomorph,
                id: Some(blocked_id),
                guid: self.form_guid(blocked),
                name: None,
            }),
            morphs: vec![
                self.morph_ref(morpheme, Some(blocked))?,
                self.morph_ref(morpheme, Some(blocker))?,
            ],
            members: Vec::new(),
            environments: Vec::new(),
            context: None,
            environment: None,
            stem_name: None,
            features: Vec::new(),
            alternatives: None,
            gate: Some("disjunctive_allomorph".to_string()),
            slot: None,
        })
    }

    fn classify(
        &self,
        input: &str,
        node: &TraceNode,
        pins: &[Pin],
        others: usize,
    ) -> WhyNotOutcome {
        let reason = node
            .failure_reason
            .expect("collected failures carry a reason");
        let word = node
            .output
            .as_ref()
            .expect("collected failures carry a word");
        let context = node.failure_context.as_ref();
        let outcome = match reason {
            FailureReason::Environments => self.environment_failure(input, word, context, pins),
            FailureReason::AllomorphCoOccurrenceRules
            | FailureReason::MorphemeCoOccurrenceRules => {
                if let Some(RejectionEvidence::CoOccurrence { require: true, .. }) =
                    context.and_then(|c| c.evidence.as_ref())
                {
                    self.required_co_occurrence_failure(word, reason, context, pins)
                } else {
                    self.co_occurrence_failure(word, reason, context, pins)
                }
            }
            FailureReason::DisjunctiveAllomorph => match context.and_then(|c| c.evidence.as_ref()) {
                Some(RejectionEvidence::AllomorphBlocked { blocked, blocker }) => {
                    self.allomorph_blocked_failure(*blocked, *blocker, pins, Provenance::HcReported)
                }
                _ => {
                    return stop(
                        StopReason::UnclassifiedFailure,
                        "the furthest derivation fails the validity check DisjunctiveAllomorph without naming the blocking allomorph",
                    )
                }
            },
            FailureReason::SurfaceFormMismatch => {
                let table =
                    &self.g.char_tables[self.g.strata[word.stratum.0 as usize].table.0 as usize];
                let synthesized = crate::surface::to_plain_string_range(
                    table,
                    &word.shape,
                    false,
                    0..word.shape.len(),
                );
                return stop(
                    StopReason::UnclassifiedFailure,
                    &format!("the pinned morphs synthesize to \"{synthesized}\", not to the word; the surface form comes from every allomorph shape and phonological rule together, so no single grammar object is named"),
                );
            }
            other => {
                return stop(
                    StopReason::UnclassifiedFailure,
                    &format!("the furthest derivation fails the validity check {other:?}, which this engine does not name"),
                )
            }
        };
        match outcome {
            Some(mut f) => {
                f.alternatives = (others > 0).then_some(others as u32);
                WhyNotOutcome::Rejected(Box::new(f))
            }
            None => stop(
                StopReason::FailureNotExpressible,
                "the failure names a morph or allomorph without a grammar id",
            ),
        }
    }

    /// Index in morph order of the allomorph a failure is about, when the evidence names one.
    fn failure_position(&self, node: &TraceNode, root_index: usize, pins: &[Pin]) -> Option<usize> {
        let word = node.output.as_ref()?;
        match node.failure_reason? {
            FailureReason::BoundRoot
            | FailureReason::RequiredStemName
            | FailureReason::ExcludedStemName => Some(root_index),
            _ => {
                let from_evidence = node
                    .failure_context
                    .as_ref()
                    .and_then(|c| c.evidence.as_ref())
                    .and_then(|evidence| match evidence {
                        RejectionEvidence::AllomorphBlocked { blocked, .. } => {
                            self.position_of_allomorph(*blocked, pins, word)
                        }
                        RejectionEvidence::Environments { object, .. }
                        | RejectionEvidence::CoOccurrence { object, .. } => match *object {
                            TraceObject::Allomorph(a) => self.position_of_allomorph(a, pins, word),
                            TraceObject::Morpheme(m) => self
                                .morpher
                                .allomorphs_in_morph_order(word)
                                .iter()
                                .position(|r| r.morpheme == m),
                            TraceObject::MorphRule(_) => None,
                        },
                        _ => None,
                    });
                from_evidence.or_else(|| {
                    node.failed_allomorph
                        .and_then(|a| self.position_of_allomorph(a, pins, word))
                })
            }
        }
    }

    /// The furthest-progress failing derivation; the first of equals.
    fn best_failure<'n>(
        &self,
        pinned: &[&'n TraceNode],
        pins: &[Pin],
        root_index: usize,
    ) -> Option<&'n TraceNode> {
        let mut best: Option<(&TraceNode, Progress)> = None;
        for &n in pinned {
            let key = Progress::of_reason(
                self.failure_position(n, root_index, pins),
                n.failure_reason?,
            );
            if best.is_none_or(|(_, k)| key > k) {
                best = Some((n, key));
            }
        }
        best.map(|(n, _)| n)
    }

    fn position_of_allomorph(
        &self,
        allomorph: AllomorphId,
        pins: &[Pin],
        word: &Word,
    ) -> Option<usize> {
        let seq = self.morpher.allomorphs_in_morph_order(word);
        let at = seq.iter().position(|r| r.allomorph == allomorph)?;
        (at < pins.len()).then_some(at)
    }

    fn environment_failure(
        &self,
        input: &str,
        word: &Word,
        context: Option<&FailureContext>,
        pins: &[Pin],
    ) -> Option<FirstFailure> {
        let Some(RejectionEvidence::Environments {
            object,
            constraint_owner,
            start,
            end,
            alternatives,
        }) = context?.evidence.as_ref()
        else {
            return None;
        };
        let (TraceObject::Allomorph(object), TraceObject::Allomorph(owner)) =
            (*object, *constraint_owner)
        else {
            return None;
        };
        let allomorph_id = self.g.allomorph_xml_key(owner)?.to_string();
        let morpheme = self.morpheme_of_allomorph(object)?;
        let table = &self.g.char_tables[self.g.strata[word.stratum.0 as usize].table.0 as usize];
        // The span is in interior indices; the shape's left anchor shifts them by one.
        let (start, end) = (*start as usize + 1, *end as usize + 1);
        let left = crate::surface::to_plain_string_range(table, &word.shape, false, 0..start);
        let right = crate::surface::to_plain_string_range(
            table,
            &word.shape,
            false,
            end + 1..word.shape.len(),
        );
        let failed: Vec<FailureEnvironment> = alternatives
            .iter()
            .map(|alt| {
                let index = u32::try_from(alt.index).ok();
                let source = self
                    .environment_source(owner, index)
                    .filter(|s| s.guid.is_some());
                FailureEnvironment {
                    allomorph_id: allomorph_id.clone(),
                    guid: source.and_then(|s| s.guid.clone()),
                    text: source.and_then(|s| s.text.clone()),
                    index,
                }
            })
            .collect();
        self.morpheme_name(morpheme)?;
        let subject = if self.is_snapshot_morpheme(morpheme) {
            self.snapshot_label(morpheme, Some(object))
        } else {
            format!("allomorph {allomorph_id} of {}", self.label(morpheme))
        };
        let synthesized =
            crate::surface::to_plain_string_range(table, &word.shape, false, 0..word.shape.len());
        let texts: Vec<&str> = failed.iter().filter_map(|e| e.text.as_deref()).collect();
        let env_note = match texts.as_slice() {
            [] => String::new(),
            [one] => format!(" (environment {one})"),
            many => format!(" (none of the environments {} is met)", many.join(", ")),
        };
        let plain = if synthesized == input {
            format!(
                "{subject} is not allowed where it surfaces, between \"{left}\" and \"{right}\"{env_note}"
            )
        } else {
            format!(
                "{subject} is not allowed in the synthesized form \"{synthesized}\" (the word asked about is \"{input}\"), between \"{left}\" and \"{right}\"{env_note}"
            )
        };
        Some(FirstFailure {
            kind: FailureKind::EnvironmentUnmet,
            plain,
            position: self.position_of_allomorph(object, pins, word),
            hc_reason: Some("Environments".to_string()),
            provenance: Provenance::HcReported,
            object: Some(FailureObject {
                kind: ObjectType::Allomorph,
                id: Some(allomorph_id.clone()),
                guid: self.form_guid(owner),
                name: None,
            }),
            morphs: vec![self.morph_ref(morpheme, Some(object))?],
            members: Vec::new(),
            context: Some(EnvContext::derived(left, right)),
            environment: Some(match failed.as_slice() {
                [only] => only.clone(),
                _ => FailureEnvironment {
                    allomorph_id,
                    guid: None,
                    text: None,
                    index: None,
                },
            }),
            environments: if failed.len() > 1 { failed } else { Vec::new() },
            stem_name: None,
            features: Vec::new(),
            alternatives: None,
            gate: None,
            slot: None,
        })
    }

    fn co_occurrence_failure(
        &self,
        word: &Word,
        reason: FailureReason,
        context: Option<&FailureContext>,
        pins: &[Pin],
    ) -> Option<FirstFailure> {
        let Some(RejectionEvidence::CoOccurrence {
            object,
            others,
            actual,
            ..
        }) = context?.evidence.as_ref()
        else {
            return None;
        };
        let word_seq = self.morpher.allomorphs_in_morph_order(word);
        let partner = || others.iter().find(|o| actual.contains(o));
        let (primary, other, position, labels) = match *object {
            TraceObject::Allomorph(a) => {
                let TraceObject::Allomorph(b) = *partner()? else {
                    return None;
                };
                let (ma, mb) = (
                    self.morpheme_of_allomorph(a)?,
                    self.morpheme_of_allomorph(b)?,
                );
                (
                    self.morph_ref(ma, Some(a))?,
                    self.morph_ref(mb, Some(b))?,
                    self.position_of_allomorph(a, pins, word),
                    (self.label_with(ma, Some(a)), self.label_with(mb, Some(b))),
                )
            }
            TraceObject::Morpheme(m) => {
                let TraceObject::Morpheme(n) = *partner()? else {
                    return None;
                };
                let in_word = |x: MorphemeId| {
                    word_seq
                        .iter()
                        .find(|r| r.morpheme == x)
                        .map(|r| r.allomorph)
                };
                (
                    self.morph_ref(m, in_word(m))?,
                    self.morph_ref(n, in_word(n))?,
                    word_seq.iter().position(|r| r.morpheme == m),
                    (
                        self.label_with(m, in_word(m)),
                        self.label_with(n, in_word(n)),
                    ),
                )
            }
            TraceObject::MorphRule(_) => return None,
        };
        let plain = format!("{} forbids {}", labels.0, labels.1);
        let in_word = |x: MorphemeId| {
            word_seq
                .iter()
                .find(|r| r.morpheme == x)
                .map(|r| r.allomorph)
        };
        let member = |o: &TraceObject| match *o {
            TraceObject::Allomorph(a) => self.morph_ref(self.morpheme_of_allomorph(a)?, Some(a)),
            TraceObject::Morpheme(m) => self.morph_ref(m, in_word(m)),
            TraceObject::MorphRule(_) => None,
        };
        let members: Option<Vec<MorphRef>> = std::iter::once(object)
            .chain(others.iter())
            .map(member)
            .collect();
        Some(FirstFailure {
            kind: FailureKind::AdhocProhibition,
            plain,
            position: position.filter(|&p| p < pins.len()),
            hc_reason: Some(format!("{reason:?}")),
            provenance: Provenance::HcReported,
            // A co-occurrence rule has no id in grammar.xml, so the object is typed and its id absent; `morphs[0]` is the rule's owner.
            object: Some(FailureObject {
                kind: ObjectType::Rule,
                id: None,
                guid: self.prohibition_guid(*object, others, false),
                name: None,
            }),
            morphs: vec![primary, other],
            members: members?,
            environments: Vec::new(),
            context: None,
            environment: None,
            stem_name: None,
            features: Vec::new(),
            alternatives: None,
            gate: None,
            slot: None,
        })
    }

    /// The FieldWorks guid of the co-occurrence rule (exclusion, or requirement when `require`) whose primary is `primary` and whose members are `others`; `None` on HC XML.
    fn prohibition_guid(
        &self,
        primary: TraceObject,
        others: &[TraceObject],
        require: bool,
    ) -> Option<String> {
        match primary {
            TraceObject::Allomorph(a) => {
                let want: Vec<AllomorphId> = others
                    .iter()
                    .filter_map(|o| match o {
                        TraceObject::Allomorph(x) => Some(*x),
                        _ => None,
                    })
                    .collect();
                let rules = match *self.g.allomorph_owners.get(a.0 as usize)? {
                    AllomorphOwner::Root(le, i) => {
                        &self
                            .g
                            .entries
                            .get(le.0 as usize)?
                            .allomorphs
                            .get(i as usize)?
                            .co_occurrence
                    }
                    AllomorphOwner::Affix(rule, i) => {
                        &self
                            .g
                            .mrules
                            .get(rule.0 as usize)?
                            .affix_allomorphs()?
                            .get(i as usize)?
                            .co_occurrence
                    }
                };
                unique_guid(
                    rules
                        .iter()
                        .filter(|r| r.require == require && same_set(&r.others, &want))
                        .map(|r| r.source_guid.as_deref()),
                )
            }
            TraceObject::Morpheme(m) => {
                let want: Vec<MorphemeId> = others
                    .iter()
                    .filter_map(|o| match o {
                        TraceObject::Morpheme(x) => Some(*x),
                        _ => None,
                    })
                    .collect();
                unique_guid(
                    self.g
                        .morphemes
                        .get(m.0 as usize)?
                        .co_occurrence
                        .iter()
                        .filter(|r| r.require == require && same_set(&r.others, &want))
                        .map(|r| r.source_guid.as_deref()),
                )
            }
            TraceObject::MorphRule(_) => None,
        }
    }

    /// The failure of a `require` co-occurrence rule: the rule's owner is named, the partners the candidate does not provide are in `morphs` after it.
    fn required_co_occurrence_failure(
        &self,
        word: &Word,
        reason: FailureReason,
        context: Option<&FailureContext>,
        pins: &[Pin],
    ) -> Option<FirstFailure> {
        let Some(RejectionEvidence::CoOccurrence {
            object,
            others,
            actual,
            adjacency,
            ..
        }) = context?.evidence.as_ref()
        else {
            return None;
        };
        let word_seq = self.morpher.allomorphs_in_morph_order(word);
        let in_word = |x: MorphemeId| {
            word_seq
                .iter()
                .find(|r| r.morpheme == x)
                .map(|r| r.allomorph)
        };
        let member = |o: &TraceObject| -> Option<(MorphRef, String)> {
            match *o {
                TraceObject::Allomorph(a) => {
                    let m = self.morpheme_of_allomorph(a)?;
                    Some((self.morph_ref(m, Some(a))?, self.label_with(m, Some(a))))
                }
                TraceObject::Morpheme(m) => Some((
                    self.morph_ref(m, in_word(m))?,
                    self.label_with(m, in_word(m)),
                )),
                TraceObject::MorphRule(_) => None,
            }
        };
        let (primary, primary_label) = member(object)?;
        let position = match *object {
            TraceObject::Allomorph(a) => self.position_of_allomorph(a, pins, word),
            TraceObject::Morpheme(m) => word_seq.iter().position(|r| r.morpheme == m),
            TraceObject::MorphRule(_) => None,
        };
        let missing: Vec<&TraceObject> = others.iter().filter(|o| !actual.contains(o)).collect();
        let named: Vec<&TraceObject> = if missing.is_empty() {
            others.iter().collect()
        } else {
            missing.clone()
        };
        let partners: Vec<(MorphRef, String)> = named
            .iter()
            .map(|o| member(o))
            .collect::<Option<Vec<_>>>()?;
        let members: Vec<MorphRef> = std::iter::once(object)
            .chain(others.iter())
            .map(|o| member(o).map(|(m, _)| m))
            .collect::<Option<Vec<_>>>()?;
        let place = match adjacency {
            CoOccurrenceAdjacency::Anywhere => "",
            CoOccurrenceAdjacency::SomewhereToLeft => " somewhere to its left",
            CoOccurrenceAdjacency::AdjacentToLeft => " immediately to its left",
            CoOccurrenceAdjacency::SomewhereToRight => " somewhere to its right",
            CoOccurrenceAdjacency::AdjacentToRight => " immediately to its right",
        };
        let labels: Vec<&str> = partners.iter().map(|(_, l)| l.as_str()).collect();
        let tail = if missing.is_empty() {
            "which the word has, but not placed that way"
        } else {
            "which the candidate does not provide"
        };
        let plain = format!(
            "{primary_label} requires {}{place}, {tail}",
            labels.join(" and ")
        );
        let mut morphs = vec![primary];
        morphs.extend(partners.into_iter().map(|(m, _)| m));
        Some(FirstFailure {
            kind: FailureKind::RequiredCoOccurrence,
            plain,
            position: position.filter(|&p| p < pins.len()),
            hc_reason: Some(format!("{reason:?}")),
            provenance: Provenance::HcReported,
            // As for a prohibition, a co-occurrence rule has no id in grammar.xml: typed, id absent; `morphs[0]` is the rule's owner.
            object: Some(FailureObject {
                kind: ObjectType::Rule,
                id: None,
                guid: self.prohibition_guid(*object, others, true),
                name: None,
            }),
            morphs,
            members,
            environments: Vec::new(),
            context: None,
            environment: None,
            stem_name: None,
            features: Vec::new(),
            alternatives: None,
            gate: None,
            slot: None,
        })
    }

    /// The pinned morpheme a rule realizes; `None` for a rule no morpheme is built from.
    fn morpheme_of_rule(&self, rule: MRuleId) -> Option<MorphemeId> {
        self.owners
            .iter()
            .position(|o| matches!(o, Some(MorphemeOwner::MRule(r)) if *r == rule))
            .map(|i| MorphemeId(i as u32))
    }

    /// Whether `w` is, rule for rule, the derivation the candidate names: its root is the pinned root and the rules its analysis unapplied (the trail, which synthesis keeps) are exactly the pinned rules, each with its pinned allomorph. A word that fails any part of this is a different derivation and says nothing about the candidate.
    fn is_pinned_derivation(&self, w: &Word, pins: &[Pin], root_index: usize) -> bool {
        let Some(root) = w.root_allomorph.filter(|&r| r != AllomorphId::GUESSED) else {
            return false;
        };
        let Some(root_pin) = pins.get(root_index) else {
            return false;
        };
        if self.morpheme_of_allomorph(root) != Some(root_pin.morpheme)
            || root_pin.allomorph.is_some_and(|a| a != root)
        {
            return false;
        }
        let mut want: Vec<Pin> = pins
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != root_index)
            .map(|(_, p)| *p)
            .collect();
        let (rules, allomorphs) = (w.trail.rules(), w.trail.allomorphs());
        if rules.len() != want.len() || allomorphs.len() != rules.len() {
            return false;
        }
        for (rule, allomorph) in rules.into_iter().zip(allomorphs) {
            let Some(morpheme) = rule.and_then(|r| self.morpheme_of_rule(r)) else {
                return false;
            };
            let Some(at) = want.iter().position(|p| {
                p.morpheme == morpheme && p.allomorph.is_none_or(|a| Some(a) == allomorph)
            }) else {
                return false;
            };
            want.swap_remove(at);
        }
        true
    }

    /// A failure that stopped the pinned derivation before it was a word to validate: a required template slot the candidate leaves empty, a family member that replaces its output, or a stem the first affix cannot take. Each is named only when the node's own word is the pinned derivation and the grammar shows the object; the one furthest along wins.
    fn classify_derivation_stage(
        &self,
        sink: &TreeTraceSink,
        pins: &[Pin],
        root_index: usize,
    ) -> Option<FirstFailure> {
        let mut nodes = Vec::new();
        if let Some(root) = sink.root() {
            collect_nodes(sink, root, &mut nodes);
        }
        let mut block = None;
        let mut slot = None;
        let mut clash = None;
        for n in &nodes {
            if block.is_none() && n.type_ == TraceType::Blocked {
                block = self.family_block_failure(n, pins, root_index);
            } else if slot.is_none()
                && n.partial_parse_cause == Some(PartialParseCause::ApplicableTemplatesNotApplied)
            {
                slot = self.empty_slot_failure(n, pins, root_index);
            } else if clash.is_none()
                && n.failure_reason == Some(FailureReason::LexicalLookupFeatures)
            {
                clash = self.stem_clash_failure(n, &nodes, pins, root_index);
            }
        }
        block.or(slot).or(clash)
    }

    /// The family entry that replaced the pinned derivation's output.
    fn family_block_failure(
        &self,
        n: &TraceNode,
        pins: &[Pin],
        root_index: usize,
    ) -> Option<FirstFailure> {
        let original = n.blocked_original.as_ref()?;
        let blocker = n.blocked_by_entry?;
        let TraceSource::MorphRule(rule) = n.source else {
            return None;
        };
        if !self.is_pinned_derivation(original, pins, root_index) {
            return None;
        }
        let root = original.root_allomorph?;
        let AllomorphOwner::Root(root_entry, _) = *self.g.allomorph_owners.get(root.0 as usize)?
        else {
            return None;
        };
        let family = self.g.entries.get(root_entry.0 as usize)?.family?;
        if blocker == root_entry
            || !self
                .g
                .families
                .get(family.0 as usize)?
                .entries
                .contains(&blocker)
        {
            return None;
        }
        let blocker_entry = self.g.entries.get(blocker.0 as usize)?;
        let rule_morpheme = self.morpheme_of_rule(rule)?;
        let root_morpheme = pins.get(root_index)?.morpheme;
        let position = pins.iter().position(|p| p.morpheme == rule_morpheme);
        let plain = format!(
            "{} applied to {} is blocked: {} is in the same family, and HermitCrab replaces the output with it",
            self.label(rule_morpheme),
            self.label_with(root_morpheme, Some(root)),
            self.label(blocker_entry.morpheme),
        );
        Some(FirstFailure {
            kind: FailureKind::OtherGate,
            plain,
            position,
            hc_reason: None,
            provenance: Provenance::Derived,
            object: Some(FailureObject {
                kind: ObjectType::Entry,
                id: Some(self.morpheme_name(blocker_entry.morpheme)?),
                guid: blocker_entry.source_guid.clone(),
                name: None,
            }),
            morphs: vec![
                self.morph_ref(root_morpheme, Some(root))?,
                self.morph_ref(rule_morpheme, pins.get(position?).and_then(|p| p.allomorph))?,
                self.morph_ref(blocker_entry.morpheme, None)?,
            ],
            members: Vec::new(),
            environments: Vec::new(),
            context: None,
            environment: None,
            stem_name: None,
            features: Vec::new(),
            alternatives: None,
            gate: Some("family_block".to_string()),
            slot: None,
        })
    }

    /// A required slot of the one template that applied, which no pinned rule can fill.
    fn empty_slot_failure(
        &self,
        n: &TraceNode,
        pins: &[Pin],
        root_index: usize,
    ) -> Option<FirstFailure> {
        let input = n.input.as_ref()?;
        let template_id = n.applicable_template?;
        if !self.is_pinned_derivation(input, pins, root_index) {
            return None;
        }
        let template = self.g.templates.get(template_id.0 as usize)?;
        let pinned_rules: Vec<MRuleId> = pins
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != root_index)
            .filter_map(|(_, p)| match self.owners.get(p.morpheme.0 as usize)? {
                Some(MorphemeOwner::MRule(r)) => Some(*r),
                _ => None,
            })
            .collect();
        let index = template.slots.iter().position(|s| {
            !s.rules.is_empty() && !s.optional && !s.rules.iter().any(|r| pinned_rules.contains(r))
        })?;
        let slot = &template.slots[index];
        let slot_name = slot.name.as_deref().filter(|n| !n.is_empty());
        let template_name = template.name.as_deref().filter(|n| !n.is_empty());
        let plain = format!(
            "{} of {} must be filled, and no morph of the candidate belongs to it",
            slot_name.map_or_else(|| format!("slot {index}"), |n| format!("slot \"{n}\"")),
            template_name.map_or_else(
                || "the affix template".to_string(),
                |n| format!("template \"{n}\"")
            ),
        );
        Some(FirstFailure {
            kind: FailureKind::RequiredSlotEmpty,
            plain,
            position: None,
            hc_reason: Some("PartialParse".to_string()),
            provenance: Provenance::HcReported,
            object: Some(FailureObject {
                kind: ObjectType::Template,
                id: None,
                guid: template.source_guid.clone(),
                name: template_name.map(str::to_string),
            }),
            morphs: Vec::new(),
            members: Vec::new(),
            environments: Vec::new(),
            context: None,
            environment: None,
            stem_name: None,
            features: Vec::new(),
            alternatives: None,
            gate: None,
            slot: Some(FailureSlot {
                index: u32::try_from(index).ok()?,
                name: slot_name.map(str::to_string),
            }),
        })
    }

    /// A root entry whose features the one pinned affix's stem requirement refuses. Named only when that affix is the derivation's only unapplied rule and the lookup's input features are exactly its requirement, so the requirement is the whole of what the entry was tested against.
    fn stem_clash_failure(
        &self,
        n: &TraceNode,
        nodes: &[TraceNode],
        pins: &[Pin],
        root_index: usize,
    ) -> Option<FirstFailure> {
        let w = n.output.as_ref()?;
        if !self.is_pinned_derivation(w, pins, root_index) {
            return None;
        }
        let rules = w.trail.rules();
        let [Some(rule)] = rules[..] else {
            return None;
        };
        let required_id = match self.g.mrules.get(rule.0 as usize)? {
            MorphRuleDef::AffixProcess(def) => def.required_syn_fs,
            MorphRuleDef::Realizational(def) => def.required_syn_fs,
            MorphRuleDef::Compounding(_) => return None,
        };
        let required = self.g.fs_interner.get(required_id);
        let root = w.root_allomorph?;
        let AllomorphOwner::Root(root_entry, _) = *self.g.allomorph_owners.get(root.0 as usize)?
        else {
            return None;
        };
        let entry_fs = self
            .g
            .fs_interner
            .get(self.g.entries.get(root_entry.0 as usize)?.syn_fs);
        // The failed word carries the entry's features, so the lookup's own input is found among the lookup nodes: same single rule, features exactly its requirement.
        let looked_up = nodes.iter().any(|l| {
            l.type_ == TraceType::LexicalLookup
                && l.input
                    .as_ref()
                    .is_some_and(|i| i.syn_fs == *required && i.trail.rules() == rules)
        });
        if !looked_up || pg_featstruct::is_unifiable(entry_fs, required) {
            return None;
        }
        let mut features = Vec::new();
        self.feature_clashes(required, entry_fs, "", &mut features);
        let first = features.first()?.clone();
        let rule_morpheme = self.morpheme_of_rule(rule)?;
        let root_morpheme = pins.get(root_index)?.morpheme;
        let position = pins.iter().position(|p| p.morpheme == rule_morpheme)?;
        let plain = format!(
            "{} needs a stem whose {} is {}, but {} has {}",
            self.label(rule_morpheme),
            first.path,
            first.required,
            self.label_with(root_morpheme, Some(root)),
            first.actual,
        );
        Some(FirstFailure {
            kind: FailureKind::FeatureClash,
            plain,
            position: Some(position),
            hc_reason: Some("LexicalLookupFeatures".to_string()),
            provenance: Provenance::HcReported,
            object: Some(FailureObject {
                kind: ObjectType::Rule,
                id: Some(self.morpheme_name(rule_morpheme)?),
                guid: super::morpheme_guid(self.g, rule_morpheme).and_then(|g| g.msa_guid),
                name: None,
            }),
            morphs: vec![
                self.morph_ref(rule_morpheme, pins.get(position).and_then(|p| p.allomorph))?,
                self.morph_ref(root_morpheme, Some(root))?,
            ],
            members: Vec::new(),
            environments: Vec::new(),
            context: None,
            environment: None,
            stem_name: None,
            features,
            alternatives: None,
            gate: None,
            slot: None,
        })
    }

    /// The feature paths where `actual` cannot meet `required`, with each side's symbols by their grammar.xml ids.
    fn feature_clashes(
        &self,
        required: &FeatureStruct,
        actual: &FeatureStruct,
        prefix: &str,
        out: &mut Vec<FeatureDiff>,
    ) {
        let system = &self.g.syn_features;
        for (feat, want) in required.entries() {
            let Some(have) = actual.get(*feat) else {
                continue;
            };
            let Some(def) = system.features.get(feat.0 as usize) else {
                continue;
            };
            let path = if prefix.is_empty() {
                def.xml_id.clone()
            } else {
                format!("{prefix}.{}", def.xml_id)
            };
            match (want, have) {
                (FeatureValue::Symbolic(w), FeatureValue::Symbolic(h)) => {
                    if !w.overlaps(false, *h, false, 0) {
                        let SynFeatureKind::Symbolic { symbols, .. } = &def.kind else {
                            continue;
                        };
                        let names = |bits: pg_featstruct::SymbolBits| {
                            symbols
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| bits.get(*i as u32))
                                .map(|(_, (id, _))| id.as_str())
                                .collect::<Vec<_>>()
                                .join("|")
                        };
                        out.push(FeatureDiff {
                            path,
                            required: names(*w),
                            actual: names(*h),
                        });
                    }
                }
                (FeatureValue::Complex(w), FeatureValue::Complex(h)) => {
                    self.feature_clashes(w, h, &path, out);
                }
                _ => {}
            }
        }
    }

    fn morpheme_of_allomorph(&self, a: AllomorphId) -> Option<MorphemeId> {
        super::allomorph_morpheme(self.g, a)
    }
}

/// Whether two member lists hold the same members, in any order.
fn same_set<T: Ord + Copy>(a: &[T], b: &[T]) -> bool {
    let sorted = |x: &[T]| {
        let mut v = x.to_vec();
        v.sort_unstable();
        v.dedup();
        v
    };
    sorted(a) == sorted(b)
}

/// The one guid every candidate rule carries; `None` when there is none or the candidates differ, because the trace cannot say which of several equal rules fired.
fn unique_guid<'a>(mut guids: impl Iterator<Item = Option<&'a str>>) -> Option<String> {
    let first = guids.next()??;
    guids.all(|g| g == Some(first)).then(|| first.to_string())
}

fn stop(reason: StopReason, note: &str) -> WhyNotOutcome {
    WhyNotOutcome::Unknown(Stop {
        reason,
        note: Some(note.to_string()),
    })
}

/// The `(morpheme, allomorph)` pairs of an analysis in surface order, each allomorph once.
fn analysis_sequence(a: &WordAnalysis) -> Vec<(u32, u32)> {
    let mut occurrences: Vec<_> = a.morph_occurrences.iter().collect();
    occurrences.sort_by_key(|o| o.order);
    let mut seen: Vec<u32> = Vec::new();
    let mut out = Vec::new();
    for o in occurrences {
        if !seen.contains(&o.allomorph_id) {
            seen.push(o.allomorph_id);
            out.push((o.morpheme_id, o.allomorph_id));
        }
    }
    out
}

/// A derivation honours the pins when morphemes, order and pinned allomorphs all agree.
fn matches_pins(found: &[(u32, u32)], pins: &[Pin], root_index: usize, found_root: i32) -> bool {
    found.len() == pins.len()
        && found_root == root_index as i32
        && found.iter().zip(pins).all(|(&(m, a), pin)| {
            m == pin.morpheme.0 && pin.allomorph.is_none_or(|want| want.0 == a)
        })
}

fn collect_nodes(sink: &TreeTraceSink, at: TraceHandle, out: &mut Vec<TraceNode>) {
    let node = sink.node(at);
    for &child in &node.children {
        collect_nodes(sink, child, out);
    }
    out.push(node);
}

fn collect_failed(sink: &TreeTraceSink, at: TraceHandle, out: &mut Vec<TraceNode>) {
    let node = sink.node(at);
    for &child in &node.children {
        collect_failed(sink, child, out);
    }
    if node.type_ == TraceType::Failed && node.failure_reason.is_some() && node.output.is_some() {
        out.push(node);
    }
}

/// What in the trace is more than a plain analysis-side miss; `no_morph_match` needs there to be none.
fn other_stop(sink: &TreeTraceSink) -> Option<String> {
    /// Whether the trace names a blocked allomorph, which explains the partial parse it leaves behind.
    fn names_blocked_allomorph(sink: &TreeTraceSink, at: TraceHandle) -> bool {
        let node = sink.node(at);
        node.failure_reason == Some(FailureReason::DisjunctiveAllomorph)
            || node
                .children
                .iter()
                .any(|&child| names_blocked_allomorph(sink, child))
    }
    fn walk(sink: &TreeTraceSink, at: TraceHandle, blocked: bool) -> Option<String> {
        let node = sink.node(at);
        if node.type_ == TraceType::Blocked {
            return Some("a derivation blocked by a family member".to_string());
        }
        if let Some(reason) = node.failure_reason {
            if !(blocked && reason == FailureReason::PartialParse)
                && !matches!(
                    reason,
                    FailureReason::Pattern
                        | FailureReason::HeadPattern
                        | FailureReason::NonHeadPattern
                        | FailureReason::AllomorphNotApplicable
                )
            {
                return Some(format!("a derivation stopped with {reason:?}"));
            }
        } else if node.type_ == TraceType::Failed {
            return Some("a failed derivation".to_string());
        }
        node.children
            .iter()
            .find_map(|&child| walk(sink, child, blocked))
    }
    let root = sink.root()?;
    walk(sink, root, names_blocked_allomorph(sink, root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::why_not::{why_not, Candidate, WhyNotRequest};
    use pg_grammar_model::model::{
        AllomorphOwner, LexEntryId, MRuleId, MorphemeOwner as Owner, StratumId,
    };
    use pg_rules::trace::TraceSink;
    use pg_rules::word::MorphRecord;
    use pg_shape::ShapeBuilder;

    fn word() -> Word {
        Word::new(ShapeBuilder::new().finish(), StratumId(0))
    }

    fn load(name: &str) -> Grammar {
        let path = format!("{}/../pg-cli/tests/data/{name}", env!("CARGO_MANIFEST_DIR"));
        pg_grammar::load(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn entry(g: &Grammar, key: &str) -> usize {
        g.entries
            .iter()
            .position(|e| g.morphemes[e.morpheme.0 as usize].xml_key == key)
            .unwrap()
    }

    fn rule_morpheme(g: &Grammar, key: &str) -> MorphemeId {
        MorphemeId(g.morphemes.iter().position(|m| m.xml_key == key).unwrap() as u32)
    }

    fn rule_allomorph(g: &Grammar, key: &str) -> AllomorphId {
        let want = rule_morpheme(g, key);
        (0..g.allomorph_owners.len() as u32)
            .map(AllomorphId)
            .find(|&a| {
                matches!(g.allomorph_owners[a.0 as usize], AllomorphOwner::Affix(..))
                    && super::super::allomorph_morpheme(g, a) == Some(want)
            })
            .unwrap()
    }

    fn failed_node(
        sink: &TreeTraceSink,
        w: &Word,
        reason: FailureReason,
        allomorph: Option<AllomorphId>,
    ) -> TraceNode {
        let root = sink.analyze_word(w);
        let h = sink.failed(root, w, reason);
        if let Some(a) = allomorph {
            sink.set_failed_allomorph(h, a);
        }
        sink.node(h)
    }

    /// A word whose morph order is the root entry `eSal` then the suffix rule `mrSufK`.
    struct Rig {
        g: Grammar,
        word: Word,
        pins: Vec<Pin>,
        suffix: AllomorphId,
        other_root: AllomorphId,
    }

    fn rig() -> Rig {
        let g = load("why-not-environments.xml");
        let sal = &g.entries[entry(&g, "eSal")];
        let (root_a, root_m) = (sal.allomorphs[0].id, sal.morpheme);
        let suffix = rule_allomorph(&g, "mrSufK");
        let suffix_m = rule_morpheme(&g, "mrSufK");
        let other_root = g.entries[entry(&g, "eLos")].allomorphs[0].id;
        let mut word = word();
        word.morphs.push(MorphRecord::new(root_a, root_m, 0));
        word.morphs.push(MorphRecord::new(suffix, suffix_m, 1));
        let pins = vec![
            Pin {
                morpheme: root_m,
                allomorph: None,
            },
            Pin {
                morpheme: suffix_m,
                allomorph: None,
            },
        ];
        Rig {
            g,
            word,
            pins,
            suffix,
            other_root,
        }
    }

    #[test]
    fn a_failed_allomorph_without_evidence_is_placed_by_its_slot() {
        let r = rig();
        let morpher = Morpher::new(&r.g, 1_000_000);
        let searcher = Searcher::new(&r.g, &morpher);
        let sink = TreeTraceSink::with_failure_context();
        let node = failed_node(
            &sink,
            &r.word,
            FailureReason::DisjunctiveAllomorph,
            Some(r.suffix),
        );
        assert!(node.failure_context.is_none());
        assert_eq!(searcher.failure_position(&node, 0, &r.pins), Some(1));
        let outcome = searcher
            .classify_pinned("salk", &[&node], &r.pins, 0)
            .unwrap();
        let WhyNotOutcome::Unknown(stop) = outcome else {
            panic!("a disjunctive failure is not named: {outcome:?}")
        };
        let note = stop.note.unwrap();
        assert!(note.contains("DisjunctiveAllomorph"), "{note}");
    }

    #[test]
    fn a_failure_that_cannot_be_placed_hides_a_failure_that_can() {
        let r = rig();
        let morpher = Morpher::new(&r.g, 1_000_000);
        let searcher = Searcher::new(&r.g, &morpher);
        let sink = TreeTraceSink::with_failure_context();
        let unplaced = failed_node(
            &sink,
            &r.word,
            FailureReason::DisjunctiveAllomorph,
            Some(r.other_root),
        );
        assert_eq!(searcher.failure_position(&unplaced, 0, &r.pins), None);
        let placed = failed_node(&sink, &r.word, FailureReason::BoundRoot, None);
        let outcome = searcher
            .classify_pinned("salk", &[&placed, &unplaced], &r.pins, 0)
            .unwrap();
        let WhyNotOutcome::Unknown(stop) = outcome else {
            panic!("an unplaced failure makes the answer unknown: {outcome:?}")
        };
        assert_eq!(stop.reason, StopReason::UnclassifiedFailure);
        assert!(stop.note.unwrap().contains("cannot place"));
    }

    fn request(word: &str, positions: &[&str]) -> WhyNotRequest<'static> {
        let word: &'static str = Box::leak(word.to_string().into_boxed_str());
        WhyNotRequest {
            word,
            candidates: vec![Candidate {
                positions: positions
                    .iter()
                    .map(|p| vec![ExpectedMorph::new(*p)])
                    .collect(),
            }],
            root_index: None,
        }
    }

    #[test]
    fn morphs_carry_the_fieldworks_guids_the_grammar_has() {
        let mut g = load("why-not-environments.xml");
        let sal = g.entries[entry(&g, "eSal")].morpheme.0 as usize;
        g.morphemes[sal].owner = Some(Owner {
            entry_guid: "11111111-1111-1111-1111-111111111111".into(),
            keys_by_infl_type: false,
        });
        g.morphemes[sal].source_msa_guid = Some("22222222-2222-2222-2222-222222222222".into());
        let morpher = Morpher::new(&g, 5_000_000);
        let report = why_not(&g, &morpher, &request("salk", &["eSal", "mrSufK"])).unwrap();
        let WhyNotOutcome::Rejected(f) = &report.results[0].outcome else {
            panic!("{:?}", report.results[0].outcome)
        };
        let guid = f.morphs[0].guid.as_ref().expect("the entry has guids");
        assert_eq!(
            guid.entry_guid.as_deref(),
            Some("11111111-1111-1111-1111-111111111111")
        );
        assert_eq!(
            guid.msa_guid.as_deref(),
            Some("22222222-2222-2222-2222-222222222222")
        );
        let ok = why_not(&g, &morpher, &request("salt", &["eSal", "mrSufT"])).unwrap();
        let WhyNotOutcome::Accepted(a) = &ok.results[0].outcome else {
            panic!("{:?}", ok.results[0].outcome)
        };
        assert_eq!(a.used[0].guid, a.analysis.morphs[0].guid);
        assert!(a.used[0].guid.is_some());
        assert!(a.used[1].guid.is_none(), "the suffix has no guids");
    }

    #[test]
    fn a_hand_written_grammar_has_no_guids_and_says_nothing_instead() {
        let g = load("why-not-environments.xml");
        let morpher = Morpher::new(&g, 5_000_000);
        let report = why_not(&g, &morpher, &request("salk", &["eSal", "mrSufK"])).unwrap();
        let WhyNotOutcome::Rejected(f) = &report.results[0].outcome else {
            panic!()
        };
        let morphs = serde_json::to_string(&f.morphs).unwrap();
        assert!(!morphs.contains("guid"), "{morphs}");
    }

    #[test]
    fn plain_names_a_morpheme_by_its_id_and_morpheme_id_text() {
        let g = load("why-not-environments.xml");
        let morpher = Morpher::new(&g, 5_000_000);
        let report = why_not(&g, &morpher, &request("salk", &["eSal", "mrSufK"])).unwrap();
        let WhyNotOutcome::Rejected(f) = &report.results[0].outcome else {
            panic!()
        };
        assert!(f.plain.contains("aSal of eSal (\"SAL\")"), "{}", f.plain);
    }

    #[test]
    fn quotes_in_a_gloss_are_escaped_in_plain() {
        let mut g = load("why-not-environments.xml");
        let sal = g.entries[entry(&g, "eSal")].morpheme.0 as usize;
        g.morphemes[sal].gloss = Some("say \"hi\" \\".into());
        let morpher = Morpher::new(&g, 5_000_000);
        let report = why_not(&g, &morpher, &request("salk", &["eSal", "mrSufK"])).unwrap();
        let WhyNotOutcome::Rejected(f) = &report.results[0].outcome else {
            panic!()
        };
        assert!(f.plain.contains(r#"eSal ("say \"hi\" \\")"#), "{}", f.plain);
    }

    fn snapshot_morpheme(forms: &[Option<&str>], gloss: Option<&str>) -> String {
        let mut g = load("why-not-environments.xml");
        let e = entry(&g, "eSal");
        let (m, a) = (g.entries[e].morpheme, g.entries[e].allomorphs[0].id);
        g.morphemes[m.0 as usize].source_msa_guid = Some("msa".into());
        g.morphemes[m.0 as usize].gloss = gloss.map(str::to_string);
        g.allomorph_sources[a.0 as usize].form_texts =
            forms.iter().map(|f| f.map(str::to_string)).collect();
        let morpher = Morpher::new(&g, 5_000_000);
        Searcher::new(&g, &morpher).label_with(m, Some(a))
    }

    #[test]
    fn a_circumfix_names_both_halves_in_order() {
        let both = snapshot_morpheme(&[Some("ka-"), Some("-ta")], Some("CIRC"));
        assert_eq!(both, "ka- ... -ta 'CIRC'");
    }

    #[test]
    fn a_snapshot_morph_falls_back_to_gloss_then_to_a_fixed_phrase() {
        assert_eq!(snapshot_morpheme(&[], Some("PL")), "'PL'");
        assert_eq!(snapshot_morpheme(&[Some("-ak")], None), "-ak");
        assert_eq!(
            snapshot_morpheme(&[], None),
            "a morph with no form text or gloss"
        );
    }

    #[test]
    fn a_blocked_node_alone_is_another_cause() {
        let sink = TreeTraceSink::new();
        let root = sink.analyze_word(&word());
        sink.blocked(root, MRuleId(0), &word(), LexEntryId(0));
        assert!(other_stop(&sink).is_some_and(|s| s.contains("blocked")));
    }

    #[test]
    fn plain_pattern_misses_are_not_another_cause() {
        let sink = TreeTraceSink::new();
        let root = sink.analyze_word(&word());
        sink.failed(root, &word(), FailureReason::Pattern);
        assert_eq!(other_stop(&sink), None);
    }
}
