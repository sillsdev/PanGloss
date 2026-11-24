//! Per-morpheme surface spans for a traced generation, read from the engine's own morph ranges on the final shape.

use pg_grammar_model::model::PRuleId;
use pg_rules::word::Word;
use unicode_normalization::UnicodeNormalization;

use super::generate_trace::{
    step_rule, ItemSpans, SeedItem, SeedItemRole, SegmentSpan, SpanReason, StepKind, SurfaceSpans,
    UnattributedSpan,
};
use super::Morpher;
use crate::surface;
use pg_rules::word::SegmentOwner;

/// Who one segment belongs to, before the segments are grouped into runs.
#[derive(Clone, PartialEq, Eq)]
enum Claim {
    Item(usize),
    Reason(&'static str, Option<PRuleId>),
}

/// A parse analysis's morph spans: item `i` of `spans` is the analysis's `i`th projected morph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSpans {
    /// The surface in NFD; `offsets` index it.
    pub surface: String,
    pub spans: SurfaceSpans,
}

impl Morpher<'_> {
    /// The synthesized `word`'s spans for each morph `analysis` projects to, or `None` when the analysis does not project.
    pub(super) fn parse_spans(
        &self,
        word: &Word,
        analysis: &crate::WordAnalysis,
    ) -> Option<ParseSpans> {
        let projected =
            crate::parse_morph::project_parse_analysis_with_morphemes(analysis, self.g).ok()?;
        let items: Vec<SeedItem> = projected
            .morphemes
            .iter()
            .map(|&morpheme| SeedItem {
                role: SeedItemRole::Root,
                entry_guid: None,
                msa_guid: None,
                allomorph_id: None,
                form_guid: None,
                shape: None,
                rule_name: None,
                morpheme: Some(pg_grammar_model::model::MorphemeId(morpheme)),
            })
            .collect();
        // `morph_occurrences` is the stable `order` sort of `word.morphs`; map each occurrence back to its record.
        let mut by_order: Vec<usize> = (0..word.morphs.len()).collect();
        by_order.sort_by_key(|&record| word.morphs[record].order);
        let record_sets: Vec<Vec<usize>> = projected
            .occurrences
            .iter()
            .map(|set| {
                set.iter()
                    .filter_map(|&o| by_order.get(o).copied())
                    .collect()
            })
            .collect();
        let surface = self.generated_surface_of(word);
        let spans = self.word_spans_with(word, &surface, &items, Some(&record_sets));
        Some(ParseSpans {
            surface: surface.nfd().collect(),
            spans,
        })
    }

    /// Where each segment of `word`'s `surface` came from; every segment is in exactly one morpheme's spans or one unattributed run, and a morpheme the engine cannot place soundly has typed-null spans.
    pub(super) fn word_spans(
        &self,
        word: &Word,
        surface: &str,
        items: &[SeedItem],
    ) -> SurfaceSpans {
        self.word_spans_with(word, surface, items, None)
    }

    /// `word_spans`, with each item's records given explicitly (a parse knows which record is which half of a circumfix) instead of found by morpheme.
    pub(super) fn word_spans_with(
        &self,
        word: &Word,
        surface: &str,
        items: &[SeedItem],
        record_sets: Option<&[Vec<usize>]>,
    ) -> SurfaceSpans {
        let g = self.g;
        let table = &g.char_tables[g.strata[word.stratum.0 as usize].table.0 as usize];
        let interior = word.shape.len().saturating_sub(2);
        // Each segment's interior node and rendering, as `generated_surface_of` makes them.
        let mut segments: Vec<(usize, String)> = Vec::new();
        for node in 0..interior {
            let index = node + 1;
            if word.shape.kind(index) == pg_shape::NodeKind::Segment {
                let text =
                    surface::to_plain_string_range(table, &word.shape, false, index..index + 1);
                segments.push((node, text));
            }
        }
        // The indexes only agree with `match` when its greedy segmenter cuts the same places.
        let aligned = segments.iter().all(|(_, text)| !text.is_empty())
            && segments
                .iter()
                .map(|(_, text)| text.as_str())
                .collect::<String>()
                == surface
            && pg_rules::shape_feat::segment_with_features_load(g, table, surface).is_ok_and(
                |cut| {
                    let cut: Vec<String> = (0..cut.len())
                        .filter(|&i| cut.kind(i) == pg_shape::NodeKind::Segment)
                        .map(|i| surface::to_plain_string_range(table, &cut, false, i..i + 1))
                        .collect();
                    cut.iter()
                        .map(String::as_str)
                        .eq(segments.iter().map(|(_, t)| t.as_str()))
                },
            );
        let rule_reason = |code: &'static str, rule: Option<PRuleId>| SpanReason {
            code,
            rule: rule.map(|id| step_rule(g, StepKind::Phon(id))),
        };
        let all_null =
            |code: &'static str, rule: Option<PRuleId>, segment_count: usize| SurfaceSpans {
                segment_count,
                morphemes: (0..items.len())
                    .map(|item| ItemSpans {
                        item,
                        spans: None,
                        reason: Some(rule_reason(code, rule)),
                    })
                    .collect(),
                unattributed: if segment_count == 0 {
                    Vec::new()
                } else {
                    vec![UnattributedSpan {
                        span: SegmentSpan {
                            segments: (0, segment_count),
                            offsets: None,
                        },
                        reason: rule_reason(code, rule),
                    }]
                },
            };
        if !aligned {
            return all_null("segments_not_aligned", None, segments.len());
        }
        let attribution = word.span_attribution();
        if attribution.compound || items.iter().any(|item| item.role == SeedItemRole::NonHead) {
            return all_null("compound_unsupported", None, segments.len());
        }
        if let Some(rule) = attribution.poisoned {
            return all_null("span_lost_by_rule", Some(rule), segments.len());
        }
        if attribution.records.iter().any(|r| r.ambiguous_copy) {
            return all_null("span_ambiguous_copy", None, segments.len());
        }
        if attribution.positional {
            return all_null("span_unknown_truncation", None, segments.len());
        }
        if attribution.owners.len() != interior {
            return all_null("segments_not_aligned", None, segments.len());
        }

        // Match each requested morpheme to the records the engine kept for it.
        let mut reasons: Vec<Option<(&'static str, Option<PRuleId>)>> = vec![None; items.len()];
        let mut records_of: Vec<Vec<usize>> = vec![Vec::new(); items.len()];
        for (index, item) in items.iter().enumerate() {
            let Some(morpheme) = item.morpheme else {
                reasons[index] = Some(("morph_not_recorded", None));
                continue;
            };
            let same_morpheme = items
                .iter()
                .filter(|other| other.morpheme == Some(morpheme))
                .count();
            match record_sets {
                Some(sets) => {
                    records_of[index] = sets[index].clone();
                    // Two items of one morpheme are the halves of one circumfix; more cannot be paired soundly.
                    let own_records = records_of[index].iter().all(|&r| {
                        attribution
                            .records
                            .get(r)
                            .is_some_and(|record| record.morpheme == morpheme)
                    });
                    if same_morpheme > 2 || !own_records {
                        reasons[index] = Some(("repeated_morpheme", None));
                    }
                }
                None => {
                    if same_morpheme > 1 {
                        reasons[index] = Some(("repeated_morpheme", None));
                    }
                    records_of[index] = attribution
                        .records
                        .iter()
                        .enumerate()
                        .filter(|(_, record)| record.morpheme == morpheme)
                        .map(|(record, _)| record)
                        .collect();
                }
            }
            if reasons[index].is_some() {
                continue;
            }
            if records_of[index].is_empty() {
                reasons[index] = Some(("morph_not_recorded", None));
            } else if records_of[index]
                .iter()
                .any(|&r| attribution.records[r].nested)
            {
                reasons[index] = Some(("nested_morph", None));
            } else if let Some(rule) = records_of[index]
                .iter()
                .find_map(|&r| attribution.records[r].lost)
            {
                reasons[index] = Some(("span_lost_by_rule", Some(rule)));
            }
        }

        // Claim every segment once: by the item that owns its record, or by a reason it has no item.
        let claims: Vec<Claim> = segments
            .iter()
            .map(|(node, _)| match attribution.owners[*node] {
                SegmentOwner::Inserted(rule) => Claim::Reason("inserted_by_rule", Some(rule)),
                SegmentOwner::Unowned => Claim::Reason("no_morph_owner", None),
                SegmentOwner::Record(record) => {
                    match records_of
                        .iter()
                        .position(|records| records.contains(&record))
                    {
                        Some(item) => match &reasons[item] {
                            None => Claim::Item(item),
                            Some((code, rule)) => Claim::Reason(code, *rule),
                        },
                        None => Claim::Reason("unrequested_morpheme", None),
                    }
                }
            })
            .collect();

        let nfd_parts: Vec<String> = segments
            .iter()
            .map(|(_, text)| text.nfd().collect::<String>())
            .collect();
        let nfd_ok = nfd_parts.concat() == surface.nfd().collect::<String>();
        let mut starts = Vec::with_capacity(segments.len() + 1);
        let mut at = 0;
        for part in &nfd_parts {
            starts.push(at);
            at += part.len();
        }
        starts.push(at);
        let span = |start: usize, end: usize| SegmentSpan {
            segments: (start, end),
            offsets: nfd_ok.then(|| (starts[start], starts[end])),
        };
        // Maximal runs of equal claims.
        let mut runs: Vec<(Claim, usize, usize)> = Vec::new();
        for (index, claim) in claims.iter().enumerate() {
            match runs.last_mut() {
                Some((last, _, end)) if last == claim && *end == index => *end = index + 1,
                _ => runs.push((claim.clone(), index, index + 1)),
            }
        }
        let morphemes = (0..items.len())
            .map(|item| match &reasons[item] {
                Some((code, rule)) => ItemSpans {
                    item,
                    spans: None,
                    reason: Some(rule_reason(code, *rule)),
                },
                None => ItemSpans {
                    item,
                    spans: Some(
                        runs.iter()
                            .filter(|(claim, _, _)| *claim == Claim::Item(item))
                            .map(|&(_, start, end)| span(start, end))
                            .collect(),
                    ),
                    reason: None,
                },
            })
            .collect();
        let unattributed = runs
            .iter()
            .filter_map(|(claim, start, end)| match claim {
                Claim::Item(_) => None,
                Claim::Reason(code, rule) => Some(UnattributedSpan {
                    span: span(*start, *end),
                    reason: rule_reason(code, *rule),
                }),
            })
            .collect();
        SurfaceSpans {
            segment_count: segments.len(),
            morphemes,
            unattributed,
        }
    }
}
