//! `generate_words_traced`'s per-morpheme surface spans, on conformance fixtures.

use pg_conformance_fixtures::require_fixture;
use pg_featstruct::FeatureStruct;
use pg_grammar_model::model::{Grammar, LexEntryId, MRuleId};
use pg_parse::morpher::{
    SeedSurface, SegmentSpan, SurfaceSpans, TracedGenerateLimits, TracedGenerateOutcome,
};
use pg_parse::{GenMorpheme, Morpher};

/// The root entry and the rules a parse signature names, or `None` when a token is neither (a compound, say).
fn seed_of(g: &Grammar, signature: &str) -> Option<(LexEntryId, Vec<GenMorpheme>)> {
    let tokens: Vec<&str> = signature.split('|').next()?.split('+').collect();
    let mut root = None;
    let mut others = Vec::new();
    for token in tokens {
        let entry = g
            .entries
            .iter()
            .position(|e| g.morphemes[e.morpheme.0 as usize].morph_id.as_deref() == Some(token));
        let rule = g.mrules.iter().position(|r| {
            r.morpheme()
                .is_some_and(|m| g.morphemes[m.0 as usize].morph_id.as_deref() == Some(token))
        });
        match (entry, rule) {
            (Some(e), None) if root.is_none() => root = Some(LexEntryId(e as u32)),
            (None, Some(r)) => others.push(GenMorpheme::Rule(MRuleId(r as u32))),
            _ => return None,
        }
    }
    Some((root?, others))
}

fn generate(g: &Grammar, signature: &str) -> Option<TracedGenerateOutcome> {
    let (root, others) = seed_of(g, signature)?;
    let morpher = Morpher::new(g, usize::MAX);
    Some(morpher.generate_words_traced(
        root,
        &others,
        FeatureStruct::EMPTY,
        &TracedGenerateLimits {
            work_limit: 200_000,
            timeout: None,
            max_seeds: 200,
            max_trace_nodes: Some(200_000),
        },
    ))
}

/// What one morpheme got: its runs, or the typed null (`code` and the rule's name when it has one).
#[derive(Debug, PartialEq, Eq)]
enum Got {
    Runs(Vec<(usize, usize)>),
    Null(&'static str, Option<String>),
}

fn runs(list: &[(usize, usize)]) -> Got {
    Got::Runs(list.to_vec())
}

/// The surface's one span answer, with `morph_id`'s morpheme looked up in `seed.items`.
struct Case {
    spans: SurfaceSpans,
    morph_ids: Vec<String>,
}

impl Case {
    fn of(fixture: (&str, &str), word: &str, signature: &str) -> Case {
        let fixture = require_fixture(fixture.0, fixture.1);
        let g = fixture.load_grammar();
        let outcome =
            generate(&g, signature).unwrap_or_else(|| panic!("{signature} is not a plain seed"));
        let mut found = Vec::new();
        for seed in &outcome.seeds {
            for surface in seed.surfaces.iter().filter(|s| s.surface == word) {
                check_tiling(surface, &surface.morph_spans[0].spans);
                check_cuts(&g, &surface.surface, &surface.morph_spans[0].spans);
                assert_eq!(surface.morph_spans.len(), 1, "{word}: one answer expected");
                let morph_ids = seed
                    .items
                    .iter()
                    .map(|item| {
                        g.morphemes[item.morpheme.unwrap().0 as usize]
                            .morph_id
                            .clone()
                            .unwrap()
                    })
                    .collect();
                found.push(Case {
                    spans: surface.morph_spans[0].spans.clone(),
                    morph_ids,
                });
            }
        }
        assert_eq!(
            found.len(),
            1,
            "{signature} must reach {word} exactly once, got {:?}",
            outcome
                .seeds
                .iter()
                .flat_map(|s| s.surfaces.iter().map(|x| x.surface.clone()))
                .collect::<Vec<_>>()
        );
        found.pop().unwrap()
    }

    fn got(&self, morph_id: &str) -> Got {
        let item = self.morph_ids.iter().position(|id| id == morph_id).unwrap();
        let m = &self.spans.morphemes[item];
        match (&m.spans, &m.reason) {
            (Some(list), None) => Got::Runs(list.iter().map(|s| s.segments).collect()),
            (None, Some(reason)) => Got::Null(
                reason.code,
                reason.rule.as_ref().and_then(|r| r.name.clone()),
            ),
            other => panic!("spans and reason must exclude each other: {other:?}"),
        }
    }

    /// Each unattributed run as `(start, end, code, rule name)`.
    fn unattributed(&self) -> Vec<(usize, usize, &'static str, Option<String>)> {
        self.spans
            .unattributed
            .iter()
            .map(|u| {
                (
                    u.span.segments.0,
                    u.span.segments.1,
                    u.reason.code,
                    u.reason.rule.as_ref().and_then(|r| r.name.clone()),
                )
            })
            .collect()
    }
}

/// The NFD byte offset where each segment starts under the greedy segmenter `match` uses, plus the end.
fn match_cut(g: &Grammar, surface: &str) -> Option<Vec<usize>> {
    use unicode_normalization::UnicodeNormalization;
    let table = &g.char_tables[g.strata.last()?.table.0 as usize];
    let cut = pg_rules::shape_feat::segment_with_features_load(g, table, surface).ok()?;
    let mut starts = vec![0];
    for i in (0..cut.len()).filter(|&i| cut.kind(i) == pg_shape::NodeKind::Segment) {
        let text = pg_parse::surface::to_plain_string_range(table, &cut, false, i..i + 1);
        starts.push(starts.last().unwrap() + text.nfd().collect::<String>().len());
    }
    Some(starts)
}

/// Every span edge is a cut `match` makes: its segment index and its offset name the same place.
fn check_cuts(g: &Grammar, surface: &str, spans: &SurfaceSpans) {
    let Some(cut) = match_cut(g, surface) else {
        return;
    };
    assert_eq!(
        cut.len() - 1,
        spans.segment_count,
        "{surface}: segment count"
    );
    let all = spans
        .morphemes
        .iter()
        .flat_map(|m| m.spans.iter().flatten())
        .chain(spans.unattributed.iter().map(|u| &u.span));
    for span in all {
        if let Some((start, end)) = span.offsets {
            assert_eq!(cut[span.segments.0], start, "{surface}: {span:?}");
            assert_eq!(cut[span.segments.1], end, "{surface}: {span:?}");
        }
    }
}

/// Every segment is in one morpheme's spans or one unattributed run, and the offsets tile the NFD surface.
fn check_tiling(surface: &SeedSurface, spans: &SurfaceSpans) {
    use unicode_normalization::UnicodeNormalization;
    let mut covered: Vec<SegmentSpan> = Vec::new();
    for m in &spans.morphemes {
        assert_eq!(
            m.spans.is_none(),
            m.reason.is_some(),
            "{}: {m:?}",
            surface.surface
        );
        covered.extend(m.spans.iter().flatten().copied());
    }
    covered.extend(spans.unattributed.iter().map(|u| u.span));
    covered.sort();
    let nfd: String = surface.surface.nfd().collect();
    let mut next = 0;
    let mut next_byte = 0;
    let mut pieces = String::new();
    for span in &covered {
        assert_eq!(
            span.segments.0, next,
            "{}: gap or overlap in {spans:?}",
            surface.surface
        );
        assert!(
            span.segments.0 < span.segments.1,
            "{}: empty run {span:?}",
            surface.surface
        );
        next = span.segments.1;
        if let Some((start, end)) = span.offsets {
            assert_eq!(
                start, next_byte,
                "{}: byte gap in {spans:?}",
                surface.surface
            );
            next_byte = end;
            pieces.push_str(&nfd[start..end]);
        } else {
            assert!(
                covered.iter().all(|s| s.offsets.is_none()),
                "{}: offsets are all or none",
                surface.surface
            );
        }
    }
    assert_eq!(next, spans.segment_count, "{}: {spans:?}", surface.surface);
    if covered.iter().all(|s| s.offsets.is_some()) {
        assert_eq!(
            pieces, nfd,
            "{}: offsets must tile the NFD surface",
            surface.surface
        );
    }
}

#[test]
fn prefix_root_and_suffix_each_get_their_own_run() {
    let case = Case::of(
        ("edge-cases", "mpr-gated-exception"),
        "mentanukam",
        "NPFX+TANUK+DELEXCLSRC|mentanukam",
    );
    assert_eq!(case.got("NPFX"), runs(&[(0, 3)]));
    assert_eq!(case.got("TANUK"), runs(&[(3, 8)]));
    assert_eq!(case.got("DELEXCLSRC"), runs(&[(8, 10)]));
    assert_eq!(case.unattributed(), []);
}

#[test]
fn an_infix_splits_the_root_into_two_runs() {
    let case = Case::of(
        ("edge-cases", "infix-interdigitation"),
        "kpfotab",
        "KTB+PFV|kpfotab",
    );
    assert_eq!(case.got("KTB"), runs(&[(0, 1), (3, 7)]));
    assert_eq!(case.got("PFV"), runs(&[(1, 3)]));
    assert_eq!(case.unattributed(), []);
}

#[test]
fn a_circumfix_and_an_infix_of_one_rule_are_three_runs_around_the_root() {
    let case = Case::of(
        ("edge-cases", "circumfix-infix-interior-action-precedence"),
        "kebzatan",
        "CIRCINFIX+ROOT2|kebzatan",
    );
    assert_eq!(case.got("CIRCINFIX"), runs(&[(0, 2), (3, 4), (6, 8)]));
    assert_eq!(case.got("ROOT2"), runs(&[(2, 3), (4, 6)]));
}

#[test]
fn reduplicated_material_belongs_to_the_reduplicating_affix() {
    let fixture = ("edge-cases", "bracket-pattern-reduplication-affix");
    let prefix = Case::of(fixture, "kukuma", "PFX+KUMA|kukuma");
    assert_eq!(prefix.got("PFX"), runs(&[(0, 2)]));
    assert_eq!(prefix.got("KUMA"), runs(&[(2, 6)]));
    let suffix = Case::of(fixture, "kumama", "KUMA+SFX|kumama");
    assert_eq!(suffix.got("KUMA"), runs(&[(0, 4)]));
    assert_eq!(suffix.got("SFX"), runs(&[(4, 6)]));
    let both = Case::of(fixture, "kukumama", "PFX+KUMA+SFX|kukumama");
    assert_eq!(both.got("PFX"), runs(&[(0, 2)]));
    assert_eq!(both.got("KUMA"), runs(&[(2, 6)]));
    assert_eq!(both.got("SFX"), runs(&[(6, 8)]));
}

#[test]
fn an_epenthetic_segment_is_unattributed_to_its_rule_and_splits_the_root() {
    let case = Case::of(
        ("edge-cases", "rtl-epenthesis-cursor-after-apply"),
        "uyu",
        "SUBRULE|uyu",
    );
    assert_eq!(case.got("SUBRULE"), runs(&[(0, 1), (2, 3)]));
    assert_eq!(
        case.unattributed(),
        [(1, 2, "inserted_by_rule", Some("prSubruleFeed".to_string()))]
    );
}

#[test]
fn a_prefix_a_rule_deletes_keeps_empty_spans_and_the_epenthesis_beside_it_is_unattributed() {
    let case = Case::of(
        (
            "edge-cases",
            "epenthesis-beside-prefix-then-prefix-deletion",
        ),
        "asa",
        "PR0+R0|asa",
    );
    assert_eq!(case.got("R0"), runs(&[(1, 3)]));
    assert_eq!(case.got("PR0"), runs(&[]));
    assert_eq!(
        case.unattributed(),
        [(0, 1, "inserted_by_rule", Some("pe0".to_string()))]
    );
}

#[test]
fn a_merger_across_an_edge_has_typed_null_spans_naming_the_rule() {
    let case = Case::of(
        ("edge-cases", "two-merges-cascade-env"),
        "teK",
        "TAK+SFX|teK",
    );
    assert_eq!(
        case.got("TAK"),
        Got::Null("span_lost_by_rule", Some("mergeCBoundaryA".to_string()))
    );
    assert_eq!(case.got("SFX"), runs(&[]));
    assert_eq!(
        case.unattributed(),
        [(
            0,
            3,
            "span_lost_by_rule",
            Some("mergeCBoundaryA".to_string())
        )]
    );
}

#[test]
fn a_metathesis_across_an_edge_nulls_both_morphemes_and_only_those() {
    let case = Case::of(
        ("edge-cases", "metathesis-boundary-context-right"),
        "tsa",
        "Z+SUF|t+?sa",
    );
    assert_eq!(
        case.got("Z"),
        Got::Null("span_lost_by_rule", Some("m".to_string()))
    );
    assert_eq!(
        case.got("SUF"),
        Got::Null("span_lost_by_rule", Some("m".to_string()))
    );
    assert_eq!(
        case.unattributed(),
        [(0, 3, "span_lost_by_rule", Some("m".to_string()))]
    );
    // A third morpheme the swap does not touch keeps its span.
    let case = Case::of(
        ("edge-cases", "metathesis-reversed-switch-order"),
        "asts",
        "PRE0+SUF0+R2|as+?t+?s",
    );
    assert_eq!(case.got("PRE0"), runs(&[(0, 2)]));
    assert_eq!(
        case.got("R2"),
        Got::Null("span_lost_by_rule", Some("pr1".to_string()))
    );
    assert_eq!(
        case.got("SUF0"),
        Got::Null("span_lost_by_rule", Some("pr1".to_string()))
    );
}

#[test]
fn a_truncation_leaves_a_typed_null_not_a_span_on_the_roots_segment() {
    let fixture = ("edge-cases", "truncate-morphotactic-identity");
    for (word, signature, morphemes) in [
        ("a", "ROOT+B+A|a", &["ROOT", "B", "A"][..]),
        ("a", "ROOT+A+B|a", &["ROOT", "A", "B"][..]),
        ("ga", "ROOT+C+A|ga", &["ROOT", "C", "A"][..]),
        ("ga", "ROOT+A+C+B|ga", &["ROOT", "A", "C", "B"][..]),
        ("ga", "C+ROOT+A|ga", &["C", "ROOT", "A"][..]),
    ] {
        let case = Case::of(fixture, word, signature);
        for morpheme in morphemes {
            assert_eq!(
                case.got(morpheme),
                Got::Null("span_unknown_truncation", None),
                "{signature} {morpheme}"
            );
        }
    }
}

#[test]
fn a_digraph_segment_is_one_index_and_cuts_where_match_cuts() {
    let case = Case::of(
        ("languages", "suffixing-evidential-adjacency-chain"),
        "walakchikni",
        "WALAK+1SG+PL|walaknichik",
    );
    assert_eq!(case.spans.segment_count, 10);
    assert_eq!(case.got("WALAK"), runs(&[(0, 5)]));
    assert_eq!(case.got("PL"), runs(&[(5, 8)]));
    assert_eq!(case.got("1SG"), runs(&[(8, 10)]));
    let offsets: Vec<_> = case
        .spans
        .morphemes
        .iter()
        .flat_map(|m| m.spans.iter().flatten())
        .map(|s| s.offsets)
        .collect();
    assert_eq!(offsets, [Some((0, 5)), Some((9, 11)), Some((5, 9))]);
}

#[test]
fn a_full_reduplication_without_a_direction_hint_does_not_pick_a_copy() {
    let case = Case::of(
        ("edge-cases", "syncope-redup-optional-vowel-lattice"),
        "kaakaa",
        "kaa+REDA|kaakaa",
    );
    assert_eq!(case.got("kaa"), Got::Null("span_ambiguous_copy", None));
    assert_eq!(case.got("REDA"), Got::Null("span_ambiguous_copy", None));
    assert_eq!(case.unattributed(), [(0, 6, "span_ambiguous_copy", None)]);
}

#[test]
fn an_affix_inside_an_ambiguous_reduplication_is_null_too() {
    let case = Case::of(
        ("edge-cases", "partial-rule-wrapped-material"),
        "loplopt",
        "LOP+T+RD|loplopt",
    );
    for morpheme in ["LOP", "T", "RD"] {
        assert_eq!(
            case.got(morpheme),
            Got::Null("span_ambiguous_copy", None),
            "{morpheme}"
        );
    }
    assert_eq!(case.unattributed(), [(0, 7, "span_ambiguous_copy", None)]);
}

#[test]
fn an_in_place_mutation_is_nested_so_it_is_null_not_guessed() {
    let case = Case::of(
        ("edge-cases", "process-morphology-in-place-mutation"),
        "a",
        "ROOT+ABLAUT|a",
    );
    assert_eq!(case.got("ROOT"), Got::Null("nested_morph", None));
    assert_eq!(case.got("ABLAUT"), Got::Null("nested_morph", None));
}

/// On every fixture word with a plain seed, every derivation's spans tile its surface exactly once.
#[test]
fn spans_tile_the_surface_on_every_fixture_with_expected_surfaces() {
    let mut surfaces = 0;
    let mut analyses = 0;
    for fixture in
        pg_conformance_fixtures::discover_scoped(pg_conformance_fixtures::ConformanceScope::All)
    {
        let yaml = fixture.load_words_yaml();
        if yaml.skip_in_generic_replay().is_some() {
            continue;
        }
        let g = fixture.load_grammar();
        for entry in &yaml.words {
            for parse in entry.parses.iter().filter(|p| !p.guess) {
                let Some(outcome) = generate(&g, &parse.signature) else {
                    continue;
                };
                for seed in &outcome.seeds {
                    for surface in seed.surfaces.iter().filter(|s| s.surface == entry.word) {
                        surfaces += 1;
                        assert_eq!(
                            surface
                                .morph_spans
                                .iter()
                                .map(|a| a.derivations)
                                .sum::<usize>(),
                            surface.derivations,
                            "{}: {}",
                            fixture.label(),
                            entry.word
                        );
                        for analysis in &surface.morph_spans {
                            analyses += 1;
                            let root = &analysis.spans.morphemes[0];
                            assert!(
                                root.spans.as_ref().is_none_or(|runs| !runs.is_empty())
                                    || surface.surface.is_empty(),
                                "{}: {}: a root with material on the surface reported empty spans",
                                fixture.label(),
                                entry.word
                            );
                            check_tiling(surface, &analysis.spans);
                            check_cuts(&g, &surface.surface, &analysis.spans);
                        }
                    }
                }
            }
        }
    }
    assert!(
        surfaces > 1000,
        "expected a large sweep, saw {surfaces} surfaces"
    );
    assert!(analyses >= surfaces);
}

/// A surface the trace did not ask for carries no span work, and a traced generation's surfaces match the untraced ones.
#[test]
fn spans_are_only_computed_for_a_traced_generation() {
    let fixture = require_fixture("edge-cases", "infix-interdigitation");
    let g = fixture.load_grammar();
    let (root, others) = seed_of(&g, "KTB+PFV|kpfotab").unwrap();
    let morpher = Morpher::new(&g, usize::MAX);
    let limits = |trace: bool| TracedGenerateLimits {
        work_limit: 200_000,
        timeout: None,
        max_seeds: 200,
        max_trace_nodes: trace.then_some(200_000),
    };
    let plain = morpher.generate_words_traced(root, &others, FeatureStruct::EMPTY, &limits(false));
    let traced = morpher.generate_words_traced(root, &others, FeatureStruct::EMPTY, &limits(true));
    let surfaces = |o: &TracedGenerateOutcome| -> Vec<String> {
        o.seeds
            .iter()
            .flat_map(|s| s.surfaces.iter().map(|x| x.surface.clone()))
            .collect()
    };
    assert_eq!(surfaces(&plain), surfaces(&traced));
    assert!(plain
        .seeds
        .iter()
        .flat_map(|s| &s.surfaces)
        .all(|s| s.morph_spans.is_empty()));
    assert!(traced
        .seeds
        .iter()
        .flat_map(|s| &s.surfaces)
        .all(|s| !s.morph_spans.is_empty()));
}
