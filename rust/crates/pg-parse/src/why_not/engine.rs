//! The why-not search: a restricted traced reparse per morph sequence, then the first failure classified from the trace.

use pg_grammar_model::model::{
    AllomorphId, AllomorphOwner, EnvironmentSource, Grammar, MorphemeId,
};
use pg_rules::trace::{
    FailureContext, FailureReason, RejectionEvidence, TraceHandle, TraceNode, TraceObject,
    TraceType, TreeTraceSink,
};
use pg_rules::word::Word;

use super::{
    AcceptedAnalysis, AuthoredAnalysis, EnvContext, ExpectedMorph, FailureEnvironment, FailureKind,
    FailureObject, FirstFailure, MorphRef, ObjectType, Progress, Provenance, Stop, StopReason,
    WhyNotOutcome,
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

    /// A morpheme for `plain`: its grammar.xml id, then its gloss (else its `MorphemeId` text) when that adds something.
    fn label(&self, m: MorphemeId) -> String {
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
        let name = self.label(morpheme);
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
                "allomorph {allomorph_id} of {name} is not allowed where it surfaces, between \"{left}\" and \"{right}\"{env_note}"
            )
        } else {
            format!(
                "allomorph {allomorph_id} of {name} is not allowed in the synthesized form \"{synthesized}\" (phonological rules later make it \"{input}\"), between \"{left}\" and \"{right}\"{env_note}"
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
                guid: self.prohibition_guid(*object, others),
            }),
            morphs: vec![primary, other],
            members: members?,
            environments: Vec::new(),
            context: None,
            environment: None,
            stem_name: None,
            features: Vec::new(),
            alternatives: None,
        })
    }

    /// The FieldWorks guid of the prohibition whose primary is `primary` and whose members are `others`; `None` on HC XML.
    fn prohibition_guid(&self, primary: TraceObject, others: &[TraceObject]) -> Option<String> {
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
                        .filter(|r| !r.require && same_set(&r.others, &want))
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
                        .filter(|r| !r.require && same_set(&r.others, &want))
                        .map(|r| r.source_guid.as_deref()),
                )
            }
            TraceObject::MorphRule(_) => None,
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
