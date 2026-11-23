//! The why-not search: a restricted traced reparse per morph sequence, then the first failure classified from the trace.

use pg_grammar_model::model::{AllomorphId, Grammar, MorphemeId};
use pg_rules::trace::{
    FailureContext, FailureReason, RejectionEvidence, TraceHandle, TraceNode, TraceObject,
    TraceType, TreeTraceSink,
};
use pg_rules::word::Word;

use super::{
    AcceptedAnalysis, EnvContext, ExpectedMorph, FailureEnvironment, FailureKind, FailureObject,
    FirstFailure, MorphRef, ObjectType, Progress, Provenance, Stop, StopReason, WhyNotOutcome,
};
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
        })
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
                    return WhyNotOutcome::Accepted(Box::new(AcceptedAnalysis {
                        analysis: analysis.clone(),
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

        if pinned.iter().any(|n| {
            n.failed_allomorph.is_some()
                && self.failure_position(n, root_index, &seq.pins).is_none()
        }) {
            return stop(
                StopReason::UnclassifiedFailure,
                "a pinned derivation fails an allomorph check this engine cannot place in the word, so it cannot say which failure is furthest",
            );
        }
        if let Some(best) = self.best_failure(&pinned, &seq.pins, root_index) {
            return self.classify(word, best, &seq.pins, pinned.len() - 1);
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
            context: None,
            environment: None,
            stem_name: None,
            features: Vec::new(),
            alternatives: None,
        }))
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
                    return stop(
                        StopReason::UnclassifiedFailure,
                        "a co-occurrence rule requires a partner that is missing; this engine names only exclusions",
                    );
                }
                self.co_occurrence_failure(word, reason, context, pins)
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
        let index = match alternatives.as_slice() {
            [only] => u32::try_from(only.index).ok(),
            _ => None,
        };
        let name = self.morpheme_name(morpheme)?;
        let synthesized =
            crate::surface::to_plain_string_range(table, &word.shape, false, 0..word.shape.len());
        let plain = if synthesized == input {
            format!(
                "allomorph {allomorph_id} of {name} is not allowed where it surfaces, between \"{left}\" and \"{right}\""
            )
        } else {
            format!(
                "allomorph {allomorph_id} of {name} is not allowed in the synthesized form \"{synthesized}\" (the word asked about is \"{input}\"), between \"{left}\" and \"{right}\""
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
                guid: None,
            }),
            morphs: vec![self.morph_ref(morpheme, Some(object))?],
            context: Some(EnvContext::derived(left, right)),
            environment: Some(FailureEnvironment {
                allomorph_id,
                index,
            }),
            stem_name: None,
            features: Vec::new(),
            alternatives: None,
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
        let (primary, other, position) = match *object {
            TraceObject::Allomorph(a) => {
                let TraceObject::Allomorph(b) = *partner()? else {
                    return None;
                };
                (
                    self.morph_ref(self.morpheme_of_allomorph(a)?, Some(a))?,
                    self.morph_ref(self.morpheme_of_allomorph(b)?, Some(b))?,
                    self.position_of_allomorph(a, pins, word),
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
                )
            }
            TraceObject::MorphRule(_) => return None,
        };
        let plain = format!("{} forbids {}", describe(&primary), describe(&other));
        Some(FirstFailure {
            kind: FailureKind::AdhocProhibition,
            plain,
            position: position.filter(|&p| p < pins.len()),
            hc_reason: Some(format!("{reason:?}")),
            provenance: Provenance::HcReported,
            object: None,
            morphs: vec![primary, other],
            context: None,
            environment: None,
            stem_name: None,
            features: Vec::new(),
            alternatives: None,
        })
    }

    fn morpheme_of_allomorph(&self, a: AllomorphId) -> Option<MorphemeId> {
        super::allomorph_morpheme(self.g, a)
    }
}

fn describe(m: &MorphRef) -> String {
    match &m.allomorph {
        Some(a) => format!("{} ({a})", m.morpheme),
        None => m.morpheme.clone(),
    }
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
    fn walk(sink: &TreeTraceSink, at: TraceHandle) -> Option<String> {
        let node = sink.node(at);
        if node.type_ == TraceType::Blocked {
            return Some("a derivation blocked by a family member".to_string());
        }
        if let Some(reason) = node.failure_reason {
            if !matches!(
                reason,
                FailureReason::Pattern
                    | FailureReason::HeadPattern
                    | FailureReason::NonHeadPattern
                    | FailureReason::AllomorphNotApplicable
            ) {
                return Some(format!("a derivation stopped with {reason:?}"));
            }
        } else if node.type_ == TraceType::Failed {
            return Some("a failed derivation".to_string());
        }
        node.children.iter().find_map(|&child| walk(sink, child))
    }
    walk(sink, sink.root()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_grammar_model::model::{LexEntryId, MRuleId, StratumId};
    use pg_rules::trace::TraceSink;
    use pg_shape::ShapeBuilder;

    fn word() -> Word {
        Word::new(ShapeBuilder::new().finish(), StratumId(0))
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
