//! Pinned-oracle recall and confirmation at the recursive spelling proof's supported depths.

use std::collections::BTreeMap;

use pg_conformance_fixtures::{discover, replay_against_oracle, FixtureRef};
use pg_foma::analyzer::compile_proposer;
use pg_foma::compose_budget::{ApplyBudget, ApplyDimension, ApplyOutcome};
use pg_foma::composite::{compile_analyzer, FomaAnalyzer, FomaApplyOutcome};
use pg_foma::differential::{measure_word, DifferentialRow, IdentityMultiplicity};
use pg_foma::emit::{emit_tuned_surface, FomaTier};
use pg_health::capability::{CapabilityRefusal, ProposalRecallGap};
use pg_parse::Morpher;

fn fixture(suffix: &str) -> FixtureRef {
    let name = format!("edge-cases/spelling-coverage-compound-{suffix}");
    discover()
        .into_iter()
        .find(|f| {
            f.label()
                .split_once(':')
                .is_some_and(|(_, path)| path == name)
        })
        .unwrap_or_else(|| panic!("missing {name}"))
}

#[test]
fn recursive_routes_recover_every_pinned_identity_at_each_level() {
    for (stems, receipt) in [
        (3, include_str!("fixtures/recursive-compound-depth3.json")),
        (4, include_str!("fixtures/recursive-compound-depth4.json")),
    ] {
        let fixture = fixture(&format!("depth{stems}"));
        let g = fixture.load_grammar();
        let oracle = fixture.load_words_yaml();
        let mut identities: BTreeMap<String, Vec<IdentityMultiplicity>> =
            serde_json::from_str(receipt).unwrap();
        assert_eq!(identities.len(), oracle.words.len());
        let morpher = Morpher::new(&g, usize::MAX);
        let replay = replay_against_oracle(&oracle, &morpher);
        assert_eq!(replay.checked, oracle.words.len(), "S={stems}");
        assert!(
            replay.mismatches.is_empty(),
            "S={stems}: {:?}",
            replay.mismatches
        );

        let emitted = emit_tuned_surface(&g);
        assert!(
            matches!(emitted.report.tier, FomaTier::Full),
            "S={stems}: {:?}",
            emitted.report
        );
        let counts = &emitted.report.counts;
        assert_eq!(counts.compound_stem_bound, stems);
        assert_eq!(counts.compound_certificate_subrules, 2);
        assert!(counts.compound_certificate_uses > 0);

        // Require a pinned receipt for every two-letter neighbor through S+1.
        let mut neighbors = vec![String::new()];
        for _ in 0..=stems {
            neighbors = neighbors
                .into_iter()
                .flat_map(|word| ["a", "p"].map(|token| format!("{word}{token}")))
                .collect();
            for word in &neighbors {
                assert!(
                    oracle.words.iter().any(|entry| entry.word == *word),
                    "S={stems}: missing receipt for {word}"
                );
            }
        }

        let mut analyzer = compile_analyzer(&g).unwrap();
        for (index, entry) in oracle.words.iter().enumerate() {
            let row = measure_word(
                &g,
                &morpher,
                Some(&mut analyzer),
                index,
                &entry.word,
                &ApplyBudget::unbounded(),
                None,
            )
            .unwrap();
            let mut expected = identities
                .remove(&entry.word)
                .expect("missing C# identity receipt");
            assert_pinned_identity(&row, &mut expected);
            assert_eq!(
                row.confirmed.iter().map(|item| item.count).sum::<usize>(),
                entry.parses.len(),
                "{row:?}"
            );
        }

        let mut proposer = compile_proposer(&g).unwrap();
        for level in 1..=stems {
            let word = "a".repeat(level);
            assert!(
                oracle
                    .words
                    .iter()
                    .any(|entry| entry.word == word && !entry.parses.is_empty()),
                "missing positive C# witness at S={stems}, level={level}"
            );
            for boundary in ["+", "-", "="] {
                assert!(
                    proposer
                        .propose(&vec!["a"; level].join(boundary))
                        .iter()
                        .any(|candidate| candidate.morphemes.len() == level),
                    "missing path at S={stems}, level={level}, boundary={boundary}"
                );
            }
        }
        let beyond = "a".repeat(stems + 1);
        assert!(oracle
            .words
            .iter()
            .any(|entry| entry.word == beyond && entry.expect_fail));
        assert!(
            proposer.propose(&vec!["a"; stems + 1].join("-")).is_empty(),
            "path exceeds S={stems}"
        );
    }
}

#[test]
fn recursive_left_growth_preserves_prefixes_at_later_roots() {
    assert_pinned_compound_route(
        "depth-prefix-hop",
        4,
        include_str!("fixtures/recursive-compound-depth-prefix-hop.json"),
        6,
        &["aasp", "aaasp"],
    );
}

#[test]
fn singleton_segments_use_the_spelling_proof_at_three_stems() {
    assert_pinned_compound_route(
        "singleton-depth3",
        3,
        include_str!("fixtures/recursive-compound-singleton-depth3.json"),
        4,
        &["apt", "aaa"],
    );
}

#[test]
fn singleton_segments_recover_written_boundaries_at_two_stems() {
    assert_pinned_compound_route(
        "singleton-boundary",
        2,
        include_str!("fixtures/recursive-compound-singleton-boundary.json"),
        0,
        &["a+a", "a+sa"],
    );
}

#[test]
fn singleton_compounds_preserve_literal_quotes_in_boundaries() {
    assert_pinned_compound_route(
        "singleton-quote-boundary",
        2,
        include_str!("fixtures/recursive-compound-singleton-quote-boundary.json"),
        0,
        &["a", "a\"p", "a\"sp"],
    );
}

#[test]
fn singleton_compounds_preserve_literal_quotes_in_roots_and_affixes() {
    assert_pinned_compound_route(
        "singleton-quote-segment",
        2,
        include_str!("fixtures/recursive-compound-singleton-quote-segment.json"),
        0,
        &["a\"", "s\"a", "as\"p"],
    );
}

#[test]
fn two_stem_compounds_preserve_thai_marks_that_foma_tokenizes_separately() {
    assert_pinned_compound_route(
        "singleton-thai-mark",
        2,
        include_str!("fixtures/recursive-compound-singleton-thai-mark.json"),
        0,
        &["่p", "a่p", "a+่p"],
    );
}

#[test]
fn two_stem_compounds_refuse_marks_that_reorder_across_a_junction() {
    let xml = singleton_thai_xml()
        .replace(
            "<Representation>a</Representation>",
            "<Representation>a\u{5c4}</Representation>",
        )
        .replace(
            "<PhoneticShape>a</PhoneticShape>",
            "<PhoneticShape>a\u{5c4}</PhoneticShape>",
        );
    let mut g = pg_grammar::load(&xml).unwrap();
    g.max_stem_count = 2;
    assert_refusal(
        &g,
        "marks reorder when the compound junction is unmarked",
        ProposalRecallGap::CompoundJunctionTokenization,
    );
}

#[test]
fn singleton_compounds_refuse_linker_segments() {
    let xml = fixture("singleton-boundary").load_grammar_xml();
    let g = singleton_compound_with_linker(&xml);
    assert_eq!(
        Morpher::new(&g, usize::MAX).parse_word("atp").signature(),
        "X+Y|atp",
    );
    assert_refusal(
        &g,
        "linker segment",
        ProposalRecallGap::CompoundBoundarySpelling,
    );

    let g = singleton_compound_with_linker(&singleton_thai_xml());
    assert_refusal(
        &g,
        "linker with Thai mark",
        ProposalRecallGap::CompoundBoundarySpelling,
    );
}

fn singleton_compound_with_linker(xml: &str) -> pg_grammar::model::Grammar {
    let linker = xml.replacen(
        "<PhoneticShape>+</PhoneticShape>",
        "<PhoneticShape>t</PhoneticShape>",
        1,
    );
    assert_ne!(linker, xml);
    let mut g = pg_grammar::load(&linker).unwrap();
    g.max_stem_count = 2;
    g
}

#[test]
fn singleton_compounds_require_a_shared_table_for_written_boundaries() {
    let mut g = pg_grammar::load(&singleton_thai_xml()).unwrap();
    let mut second = fixture("singleton-boundary").load_grammar();
    let mut stratum = second.strata.pop().unwrap();
    stratum.table = pg_grammar::model::TableId(1);
    stratum.entries.clear();
    stratum.mrules.clear();
    g.char_tables.push(second.char_tables.pop().unwrap());
    g.strata.push(stratum);
    g.max_stem_count = 2;
    assert_refusal(
        &g,
        "written boundary with distinct tables",
        ProposalRecallGap::CompoundBoundarySpelling,
    );
}

fn singleton_thai_xml() -> String {
    fixture("singleton-thai-mark")
        .load_grammar_xml()
        .replace(
            "<Symbol id=\"gM\">-</Symbol>",
            "<Symbol id=\"gM\">-</Symbol><Symbol id=\"gTone\">tone</Symbol>",
        )
        .replace(
            "<Representation>่</Representation>\n          </Representations>\n          <FeatureValue feature=\"f\" symbolValues=\"fP\" />\n          <FeatureValue feature=\"g\" symbolValues=\"gP\" />",
            "<Representation>่</Representation>\n          </Representations>\n          <FeatureValue feature=\"f\" symbolValues=\"fP\" />\n          <FeatureValue feature=\"g\" symbolValues=\"gTone\" />",
        )
}

#[test]
fn singleton_compounds_refuse_a_typed_boundary_inside_a_root() {
    assert_pinned_refusal(
        "singleton-root-boundary",
        "t+a",
        ProposalRecallGap::SingletonRootBoundary,
    );
}

#[test]
fn singleton_routes_refuse_a_typed_boundary_inside_a_root_without_compounds() {
    assert_pinned_refusal(
        "singleton-root-boundary-free",
        "t+a",
        ProposalRecallGap::SingletonRootBoundary,
    );
}

#[test]
fn singleton_compounds_refuse_a_feature_change_across_the_junction() {
    assert_pinned_refusal(
        "singleton-junction-featchange",
        "sp",
        ProposalRecallGap::CompoundJunctionPhonology,
    );
}

#[test]
fn singleton_compounds_refuse_epenthesis_across_the_junction() {
    assert_pinned_refusal(
        "singleton-junction-epenthesis",
        "tap",
        ProposalRecallGap::CompoundJunctionPhonology,
    );
}

#[test]
fn singleton_compounds_refuse_metathesis_across_the_junction() {
    assert_pinned_refusal(
        "singleton-junction-metathesis",
        "pa",
        ProposalRecallGap::CompoundJunctionPhonology,
    );
}

#[test]
fn singleton_compounds_refuse_deletion_across_the_junction() {
    assert_pinned_refusal(
        "singleton-junction-deletion",
        "p",
        ProposalRecallGap::CompoundJunctionPhonology,
    );
}

#[test]
fn singleton_phonology_is_not_refused_when_no_stratum_applies_a_compounding_rule() {
    let xml = fixture("singleton-junction-deletion")
        .load_grammar_xml()
        .replace(
            "morphologicalRules=\"cr1 cr2 PRE\"",
            "morphologicalRules=\"PRE\"",
        );
    let g = pg_grammar::load(&xml).unwrap();
    assert!(
        compile_proposer(&g).is_ok(),
        "defined but unstaged compounding rules cannot form a compound"
    );
}

#[test]
fn singleton_compounds_refuse_merging_marks_even_without_written_junctions() {
    assert_pinned_refusal(
        "singleton-merging-mark",
        "a\u{301}p",
        ProposalRecallGap::CompoundJunctionTokenization,
    );
}

fn assert_pinned_compound_route(
    name: &str,
    stems: usize,
    receipt: &str,
    sweep_length: usize,
    witnesses: &[&str],
) {
    let fixture = fixture(name);
    let g = fixture.load_grammar();
    let morpher = Morpher::new(&g, usize::MAX);
    let mut analyzer = compile_analyzer(&g).unwrap();
    let mut identities: BTreeMap<String, Vec<IdentityMultiplicity>> =
        serde_json::from_str(receipt).unwrap();
    for word in witnesses {
        assert!(!identities[*word].is_empty(), "missing witness: {word}");
    }
    let mut neighbors = vec![String::new()];
    for _ in 0..sweep_length {
        neighbors = neighbors
            .into_iter()
            .flat_map(|word| ["a", "p", "s"].map(|token| format!("{word}{token}")))
            .collect();
        for word in &neighbors {
            assert!(identities.contains_key(word), "{name}: missing {word}");
        }
    }
    for (index, (word, expected)) in identities.iter_mut().enumerate() {
        let row = measure_word(
            &g,
            &morpher,
            Some(&mut analyzer),
            index,
            word,
            &ApplyBudget::unbounded(),
            None,
        )
        .unwrap();
        assert_pinned_identity(&row, expected);
    }
    let emitted = emit_tuned_surface(&g);
    assert_eq!(emitted.report.counts.compound_stem_bound, stems);
    if stems > 2 {
        assert!(emitted.report.counts.compound_certificate_uses > 0);
    }
}

fn assert_pinned_identity(row: &DifferentialRow, expected: &mut [IdentityMultiplicity]) {
    assert_eq!(
        (&*row.hc_status, &*row.fst_status),
        ("complete", "complete"),
        "{row:?}"
    );
    assert!(!row.incomplete, "{row:?}");
    assert_eq!(row.proposal_misses.as_deref(), Some(&[][..]), "{row:?}");
    assert_eq!(row.spurious_confirmed.as_deref(), Some(&[][..]), "{row:?}");
    assert_eq!(row.confirmed_multiset_equal, Some(true), "{row:?}");
    expected.sort_by(|a, b| a.identity.cmp(&b.identity));
    assert_eq!(row.confirmed, expected, "{}", row.word);
}

#[test]
fn a_four_root_run_counts_each_normalization_before_deduplication() {
    let xml = fixture("repeated-root").load_grammar_xml();
    let mut g = pg_grammar::load(&xml.replace(
        "<CompoundingRule id=\"cr1\">",
        "<CompoundingRule id=\"cr1\" multipleApplication=\"3\">",
    ))
    .unwrap();
    g.max_stem_count = 4;
    let mut proposer = compile_proposer(&g).unwrap();
    let word = "aaaa";
    // Each retained length k = 1, 2, 3, 4 offers k head positions. One is legacy.
    let total = 10;
    let recovered = total - 1;
    let budget = ApplyBudget::with_caps(None, Some(total));
    let (outcome, counts) = proposer.propose_budgeted_counted(word, &budget);
    assert!(matches!(outcome, ApplyOutcome::Complete(candidates) if candidates.len() == total));
    assert_eq!(
        (counts.raw_paths, counts.normalized_alternatives),
        (1, recovered)
    );
    let exact = budget.remaining(0, 0, 1);
    assert!(
        matches!(proposer.propose_budgeted(word, &exact), ApplyOutcome::Complete(candidates) if candidates.len() == total)
    );
    let one_less = budget.remaining(0, 0, 2);
    assert_eq!(
        proposer.propose_budgeted(word, &one_less),
        ApplyOutcome::Incomplete {
            dimension: ApplyDimension::NormalizedCandidates,
            value: recovered,
            limit: recovered - 1,
        }
    );
}

#[test]
fn a_parser_cap_past_four_stems_trips_the_proposer_guard() {
    let mut g = fixture("depth-limit").load_grammar();
    g.max_stem_count = 4;
    let analyzer = compile_analyzer(&g).unwrap();
    assert_eq!(analyzer.stem_bound_trip(), None);
    let (proposer, peeler, owners, _, filter) = analyzer.into_parts_with_morpher();
    let raised = Morpher::new(&g, usize::MAX).with_max_stem_count(5);
    assert!(!raised.parse_word("aaaaa").analyses.is_empty());
    let mut analyzer =
        FomaAnalyzer::from_cached_with_morpher(&g, proposer, peeler, owners, raised, filter);
    for word in ["a", "aaaa", "aaaaa"] {
        assert!(
            matches!(
                analyzer.analyze_word_budgeted(word, &ApplyBudget::unbounded()),
                FomaApplyOutcome::Incomplete {
                    dimension: ApplyDimension::StemBound,
                    value: 5,
                    limit: 4
                }
            ),
            "{word}"
        );
    }
}

#[test]
fn a_cached_singleton_artifact_trips_at_three_stems() {
    let mut g = fixture("singleton-medial-suffix").load_grammar();
    g.max_stem_count = 2;
    let analyzer = compile_analyzer(&g).unwrap();
    assert_eq!(analyzer.stem_bound_trip(), None);
    let (proposer, peeler, owners, _, filter) = analyzer.into_parts_with_morpher();
    let raised = Morpher::new(&g, usize::MAX).with_max_stem_count(3);
    assert_eq!(
        proposer
            .report
            .as_ref()
            .unwrap()
            .counts
            .compound_certificate_uses,
        0
    );
    let identities: BTreeMap<String, Vec<IdentityMultiplicity>> = serde_json::from_str(
        include_str!("fixtures/recursive-compound-singleton-medial-suffix.json"),
    )
    .unwrap();
    let row = measure_word(
        &g,
        &raised,
        None,
        0,
        "apst",
        &ApplyBudget::unbounded(),
        None,
    )
    .unwrap();
    assert!(!row.oracle.is_empty());
    assert_eq!(row.oracle, identities["apst"]);
    let mut analyzer =
        FomaAnalyzer::from_cached_with_morpher(&g, proposer, peeler, owners, raised, filter);
    assert_eq!(analyzer.stem_bound_trip(), Some((3, 2)));
    assert!(matches!(
        analyzer.analyze_word_budgeted("apst", &ApplyBudget::unbounded()),
        FomaApplyOutcome::Incomplete {
            dimension: ApplyDimension::StemBound,
            value: 3,
            limit: 2,
        }
    ));
}

#[test]
fn recursive_compounds_refuse_mark_initial_root_spellings() {
    assert_pinned_refusal(
        "junction-mark",
        "ap\u{301}",
        ProposalRecallGap::CompoundJunctionTokenization,
    );
}

#[test]
fn recursive_compounds_refuse_unproved_combining_boundary_spellings() {
    assert_pinned_refusal(
        "junction-boundary-mark",
        "apét",
        ProposalRecallGap::CompoundJunctionTokenization,
    );
}

#[test]
fn recursive_prefixes_must_copy_the_whole_host_in_order() {
    assert_pinned_refusal(
        "depth-reordered-prefix",
        "spat",
        ProposalRecallGap::RecursiveCompoundAffixPlacement,
    );
}

#[test]
fn recursive_prefixes_must_be_in_a_standalone_prefix_zone() {
    let name = "depth-prefix-zone";
    let g = fixture(name).load_grammar();
    let mid = g.strata[0].mrules[2];
    assert_eq!(
        pg_foma::emit::rule_role(&g, mid),
        pg_foma::emit::Role::Suffix
    );
    assert_eq!(
        pg_foma::emit::classify_affix(&pg_foma::emit::allomorphs_of(&g, mid)[1].rhs),
        pg_foma::emit::Role::Prefix
    );
    assert_pinned_refusal(
        name,
        "aspt",
        ProposalRecallGap::RecursiveCompoundAffixPlacement,
    );
}

#[test]
fn recursive_singleton_medial_suffixes_require_a_proof() {
    assert_pinned_refusal(
        "singleton-medial-suffix",
        "apst",
        ProposalRecallGap::RecursiveCompoundAffixPlacement,
    );
}

#[test]
fn recursive_compound_refusals_identify_the_unproved_condition() {
    use ProposalRecallGap::*;
    for (fixture_name, gap, accepted) in [
        ("depth-limit", RecursiveCompoundDepth, "aaaaa"),
        (
            "depth-asymmetric",
            RecursiveCompoundAsymmetricLicense,
            "a-p-p",
        ),
        (
            "depth-mpr-restricted",
            RecursiveCompoundMprRestriction,
            "a=p-t",
        ),
        ("depth-mpr-output", RecursiveCompoundMprOutput, "a=p-t"),
        (
            "depth-medial-suffix",
            RecursiveCompoundAffixPlacement,
            "a=ps-t",
        ),
    ] {
        assert_pinned_refusal(fixture_name, accepted, gap);
    }
}

fn assert_pinned_refusal(name: &str, witness: &str, gap: ProposalRecallGap) {
    let fixture = fixture(name);
    let g = fixture.load_grammar();
    let oracle = fixture.load_words_yaml();
    assert!(oracle
        .words
        .iter()
        .any(|entry| entry.word == witness && !entry.parses.is_empty()));
    let replay = replay_against_oracle(&oracle, &Morpher::new(&g, usize::MAX));
    assert_eq!(replay.checked, oracle.words.len(), "{name}");
    assert!(
        replay.mismatches.is_empty(),
        "{name}: {:?}",
        replay.mismatches
    );
    assert_refusal(&g, name, gap);
}

#[test]
fn recursive_compound_junctions_accept_boundary_insertions_only() {
    let xml = fixture("depth3").load_grammar_xml();
    assert!(xml.contains("<PhoneticShape>+</PhoneticShape>"));
    let mut g = pg_grammar::load(&xml.replace(
        "<PhoneticShape>+</PhoneticShape>",
        "<PhoneticShape>s</PhoneticShape>",
    ))
    .unwrap();
    g.max_stem_count = 3;
    assert_refusal(
        &g,
        "literal inserted at a junction",
        ProposalRecallGap::RecursiveCompoundSpelling,
    );
}

fn assert_refusal(g: &pg_grammar::model::Grammar, control: &str, gap: ProposalRecallGap) {
    let emitted = emit_tuned_surface(g);
    assert!(
        emitted.lexc_source.is_empty(),
        "{control}: emitted without a proof"
    );
    let Err(pg_foma::analyzer::FomaError::CapabilityRefused(refusals)) = compile_proposer(g) else {
        panic!("{control}: must be refused by normal compilation");
    };
    assert!(
        refusals
            .iter()
            .any(|d| d.refusal == CapabilityRefusal::ProposalRecall(gap)),
        "{control}: expected {gap:?}, got {refusals:?}"
    );
}
