//! Replays discovered upstream and staged conformance fixtures against HC-Rust.

use pg_conformance_fixtures::{
    all_staged_fixtures, assert_matches_oracle, discover, graduation_guard_violations,
    producibility_census, replay_against_oracle, require_fixture, OracleProvenance,
};
use pg_parse::Morpher;

#[test]
fn strrep_rewrite_tracks_membership_when_a_provisional_literal_changes_identity() {
    use pg_grammar_model::model::PhonRuleDef;
    use pg_rules::{rewrite, shape_feat::segment_with_features};

    let fixture = require_fixture("edge-cases", "strrep-rewrite-unapplication");
    let mut grammar = pg_grammar::load(&fixture.load_grammar_xml()).unwrap();
    let m = grammar.char_tables[0].lookup_nfd("m").unwrap();
    grammar.char_tables[0].mark_provisional(m);
    let segment = |text| segment_with_features(&grammar, &grammar.char_tables[0], text).unwrap();
    let PhonRuleDef::Rewrite(rule) = &grammar.prules[0] else {
        panic!("fixture must contain a rewrite")
    };
    let underlying = segment("xmuma");
    let surface = segment("xpuma");
    let analyzed = rewrite::analyze(&grammar, rule, &surface, None);
    assert_eq!(analyzed.len(), 1);
    assert!(
        pg_featstruct::flat_unifiable(underlying.node_lanes(2), analyzed[0].node_lanes(2)),
        "unapplication must retain the provisional input as an eligible literal"
    );
    assert_eq!(
        rewrite::synthesize(&grammar, rule, &underlying),
        vec![surface],
        "the authored output must carry its own definition's membership"
    );
}

#[test]
fn strrep_rewrite_preserves_provisional_named_class_exclusion() {
    use pg_grammar_model::model::{
        NatClassId, NaturalClass, NaturalClassKind, PatternNode, PhonRuleDef, SimpleContext,
    };
    use pg_rules::{rewrite, shape_feat::segment_with_features};

    let fixture = require_fixture("edge-cases", "strrep-rewrite-unapplication");
    let mut grammar = pg_grammar::load(&fixture.load_grammar_xml()).unwrap();
    let x = grammar.char_tables[0].lookup_nfd("x").unwrap();
    let class = NatClassId(grammar.natural_classes.len() as u32);
    grammar.natural_classes.push(NaturalClass {
        xml_id: "named-x".into(),
        name: Some("Named x".into()),
        kind: NaturalClassKind::Segments(vec![x]),
    });
    let PhonRuleDef::Rewrite(rule) = &mut grammar.prules[0] else {
        panic!("fixture must contain a rewrite")
    };
    rule.subrules[0].left_env.as_mut().unwrap().nodes = vec![PatternNode::Context(SimpleContext {
        nat_class: class,
        vars: vec![],
    })];
    let check = |grammar: &pg_grammar_model::model::Grammar| {
        let PhonRuleDef::Rewrite(rule) = &grammar.prules[0] else {
            unreachable!()
        };
        let segment = |text| segment_with_features(grammar, &grammar.char_tables[0], text).unwrap();
        (
            rewrite::synthesize(grammar, rule, &segment("xmuma")),
            rewrite::analyze(grammar, rule, &segment("xpuma"), None),
            segment("xpuma"),
        )
    };
    let (synthesized, analyzed, surface) = check(&grammar);
    assert_eq!(synthesized, vec![surface]);
    assert_eq!(analyzed.len(), 1);

    grammar.char_tables[0].mark_provisional(x);
    let (synthesized, analyzed, _) = check(&grammar);
    assert!(
        synthesized.is_empty(),
        "a named class cannot match provisional x"
    );
    assert!(
        analyzed.is_empty(),
        "the same exclusion applies during unapplication"
    );
}

#[test]
fn strrep_rewrite_preserves_literal_identity_and_rejects_vacuous_unapplication() {
    use pg_grammar_model::model::{Dir, PhonRuleDef, RewriteMode};
    use pg_rules::{rewrite, shape_feat::segment_with_features};

    let fixture = require_fixture("edge-cases", "strrep-rewrite-unapplication");
    let source = fixture.load_grammar_xml();
    for padding in [0, 62, 70] {
        let extra: String = (0..padding).map(|i| format!(
            "<SegmentDefinition id=\"pad{i}\"><Representations><Representation>z{i}</Representation></Representations></SegmentDefinition>"
        )).collect();
        let xml = source.replace(
            "<SegmentDefinitions>",
            &format!("<SegmentDefinitions>{extra}"),
        );
        let mut grammar = pg_grammar::load(&xml).unwrap();
        for mode in [RewriteMode::Iterative, RewriteMode::Simultaneous] {
            for dir in [Dir::LeftToRight, Dir::RightToLeft] {
                let PhonRuleDef::Rewrite(rule) = &mut grammar.prules[0] else {
                    panic!("fixture must contain a rewrite")
                };
                rule.mode = mode;
                rule.dir = dir;
                let PhonRuleDef::Rewrite(rule) = &grammar.prules[0] else {
                    unreachable!()
                };
                let segment =
                    |text| segment_with_features(&grammar, &grammar.char_tables[0], text).unwrap();
                let underlying = segment("xmuma");
                let surface = segment("xpuma");
                assert_eq!(
                    rewrite::synthesize(&grammar, rule, &underlying),
                    vec![surface.clone()]
                );
                let widened = rewrite::analyze(&grammar, rule, &surface, None);
                assert_eq!(widened.len(), 1);
                let table = &grammar.char_tables[0];
                for spelling in ["m", "p"] {
                    assert!(widened[0]
                        .node_cd_set(2)
                        .contains(table.lookup_nfd(spelling).unwrap().0));
                }
                for spelling in ["x", "u", "a"] {
                    assert!(!widened[0]
                        .node_cd_set(2)
                        .contains(table.lookup_nfd(spelling).unwrap().0));
                }
                assert!(rewrite::analyze(&grammar, rule, &widened[0], None).is_empty());
                assert!(rewrite::analyze(&grammar, rule, &segment("puma"), None).is_empty());
                assert_eq!(
                    Morpher::new(&grammar, usize::MAX)
                        .parse_word("xpuma")
                        .signature(),
                    "XMUMA|xpuma"
                );
            }
        }
    }
}

macro_rules! quantified_alpha_fixture {
    ($test:ident, $fixture:literal) => {
        quantified_alpha_fixture!($test, $fixture, 17);
    };
    ($test:ident, $fixture:literal, $rows:literal) => {
        #[test]
        fn $test() {
            let fixture = require_fixture("edge-cases", $fixture);
            let grammar = pg_grammar::load(&fixture.load_grammar_xml()).unwrap();
            let morpher = Morpher::new(&grammar, usize::MAX);
            assert_eq!(
                assert_matches_oracle(&fixture.label(), &fixture.load_words_yaml(), &morpher),
                $rows
            );
        }
    };
}

quantified_alpha_fixture!(
    quantified_alpha_bounded_ltr_left,
    "quantified-alpha-bounded-ltr-left"
);
quantified_alpha_fixture!(
    quantified_alpha_bounded_ltr_right,
    "quantified-alpha-bounded-ltr-right"
);
quantified_alpha_fixture!(
    quantified_alpha_bounded_rtl_left,
    "quantified-alpha-bounded-rtl-left"
);
quantified_alpha_fixture!(
    quantified_alpha_bounded_rtl_right,
    "quantified-alpha-bounded-rtl-right"
);
quantified_alpha_fixture!(
    quantified_alpha_unbounded_ltr_left,
    "quantified-alpha-unbounded-ltr-left"
);
quantified_alpha_fixture!(
    quantified_alpha_unbounded_ltr_right,
    "quantified-alpha-unbounded-ltr-right"
);
quantified_alpha_fixture!(
    quantified_alpha_unbounded_rtl_left,
    "quantified-alpha-unbounded-rtl-left"
);
quantified_alpha_fixture!(
    quantified_alpha_unbounded_rtl_right,
    "quantified-alpha-unbounded-rtl-right"
);
quantified_alpha_fixture!(
    quantified_alpha_bounded_zero_left,
    "quantified-alpha-bounded-zero-left",
    16
);
quantified_alpha_fixture!(
    quantified_alpha_bounded_zero_right,
    "quantified-alpha-bounded-zero-right",
    16
);
quantified_alpha_fixture!(
    quantified_alpha_unbounded_zero_left,
    "quantified-alpha-unbounded-zero-left",
    16
);
quantified_alpha_fixture!(
    quantified_alpha_unbounded_zero_right,
    "quantified-alpha-unbounded-zero-right",
    16
);

/// Fails if the same `(category, name)` fixture identity exists under both roots, enforcing that a fixture accepted upstream has its staged copy deleted in the same change.
#[test]
fn graduation_guard_no_duplicate_fixture_names() {
    let fixtures = discover();
    let violations = graduation_guard_violations(&fixtures);
    assert!(
        violations.is_empty(),
        "fixture(s) accepted upstream but still present in conformance-staging/ — delete the \
         staged copy in the same change: {violations:?}"
    );
}

/// Every discovered fixture's `words.yaml` replayed against `pg_parse::Morpher`; pathological/crash-pinning fixtures are skipped since a crash fixture has no signature to diff against.
#[test]
fn all_discovered_fixtures_match_oracle() {
    let fixtures = discover();
    assert!(
        !fixtures.is_empty(),
        "no conformance fixtures discovered at all — check the `machine` submodule is \
         initialized (`git submodule update --init machine`) and conformance-staging/ exists"
    );

    let mut total_checked = 0usize;
    let mut total_skipped_fixtures = 0usize;
    let mut mismatches = Vec::new();
    for f in &fixtures {
        let words_yaml = f.load_words_yaml();
        if let Some(reason) = words_yaml.skip_in_generic_replay() {
            eprintln!("skipping {}: {reason}", f.label());
            total_skipped_fixtures += 1;
            continue;
        }
        let xml = f.load_grammar_xml();
        let grammar = pg_grammar::load(&xml)
            .unwrap_or_else(|e| panic!("{}: grammar failed to load: {e}", f.label()));
        let morpher = Morpher::new(&grammar, usize::MAX);
        let label = f.label();
        let replay = replay_against_oracle(&words_yaml, &morpher);
        for mismatch in &replay.mismatches {
            mismatches.push(format!(
                "{label}: word {:?} {}\n  left (HC-Rust): {}\n right (oracle): {}",
                mismatch.word, mismatch.what, mismatch.got, mismatch.expected
            ));
        }
        let checked = replay.checked;
        assert!(
            checked > 0,
            "{label}: replayed zero words (every word guess-only or the fixture is empty?)"
        );
        total_checked += checked;
    }
    eprintln!(
        "conformance_fixtures_gate: {total_checked} words checked across {} fixtures ({} skipped)",
        fixtures.len() - total_skipped_fixtures,
        total_skipped_fixtures
    );
    // "fixtures covered" is not one FieldWorks-facing population; report the three separately.
    let census = producibility_census(&fixtures);
    eprintln!(
        "conformance_fixtures_gate: fieldworks_producible -- {}",
        census.summary_line()
    );
    assert!(
        mismatches.is_empty(),
        "{} conformance mismatches across the complete replay:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

/// Named regression pin for the four affix-shape constructs, so their coverage doesn't silently disappear if `all_discovered_fixtures_match_oracle` is ever narrowed.
#[test]
fn w91_affix_shapes_covered_by_upstream_fixtures() {
    // Named pins search both roots whatever the run's claimed scope: docs/design/fixture-pins.md
    let austronesian = require_fixture("languages", "metathesis-phase-isolation");
    let truncate = require_fixture("edge-cases", "truncate-morphotactic");

    let g = pg_grammar::load(&austronesian.load_grammar_xml()).unwrap();
    let morpher = Morpher::new(&g, usize::MAX);
    // infix: sumulat = SULAT + AV (-um- infixed after the first consonant).
    assert_eq!(
        morpher.parse_word("sumulat").signature(),
        "SULAT+AV|sumulat"
    );
    // circumfix: keadilan = ADIL + NMLZ (ke-...-an wraps the stem).
    assert_eq!(
        morpher.parse_word("keadilan").signature(),
        "NMLZ+ADIL|keadilan"
    );
    // noncontiguous: katibɯd = KTB + PERF, plus its obligatory-rewrite negative control.
    assert_eq!(
        morpher.parse_word("katibɯd").signature(),
        "KTB+PERF|katibɯd"
    );
    assert_eq!(morpher.parse_word("katabɯd").signature(), "-");
    // truncate (also present in metathesis-phase-isolation itself): pur = PURE + INCP.
    assert_eq!(morpher.parse_word("pur").signature(), "PURE+INCP|pur");

    let g2 = pg_grammar::load(&truncate.load_grammar_xml()).unwrap();
    let morpher2 = Morpher::new(&g2, usize::MAX);
    // "gas" has two distinct analyses (direct + chained), per that fixture's words.yaml note.
    let gas = morpher2.parse_word("gas").signature();
    assert_eq!(
        gas, "++|gas;+|gas",
        "gas must yield both the direct and chained analyses"
    );
}

/// Every staged fixture must declare a recognized `# oracle-provenance:` marker in `words.yaml` (CLAUDE.md's "oracle hierarchy"): silence reads as "verified against the C# founding oracle" and is the bug.
#[test]
fn staged_fixtures_carry_recognized_oracle_provenance() {
    let fixtures = all_staged_fixtures();
    assert!(
        !fixtures.is_empty(),
        "no staged fixtures discovered — check conformance-staging/ exists"
    );
    let missing: Vec<String> = fixtures
        .iter()
        .filter(|f| f.oracle_provenance().is_none())
        .map(|f| f.label())
        .collect();
    assert!(
        missing.is_empty(),
        "fixture(s) missing a recognized `# oracle-provenance: founding-oracle|rust-only` marker \
         in words.yaml (see CLAUDE.md's oracle hierarchy / machine/conformance/PROTOCOL.md): {missing:?}"
    );
}

/// Ratchet, not a target: 21 -> 1 once six unloadable grammars were made valid HC XML and the filter-passes root was mirrored into the C# harness; the one left is guesser-pattern-root-fallback, whose guessed words hc.dll exposes no CLI surface for. May only shrink via `rust/tools/oracle-conformance.ps1` reconciliation, never grow with a newly staged, unverified fixture.
const RUST_ONLY_ORACLE_PROVENANCE_BACKLOG: usize = 1;

#[test]
fn rust_only_oracle_provenance_backlog_does_not_grow() {
    let fixtures = all_staged_fixtures();
    let rust_only = fixtures
        .iter()
        .filter(|f| f.oracle_provenance() == Some(OracleProvenance::RustOnly))
        .count();
    assert!(
        rust_only <= RUST_ONLY_ORACLE_PROVENANCE_BACKLOG,
        "rust-only oracle-provenance backlog grew from {RUST_ONLY_ORACLE_PROVENANCE_BACKLOG} to \
         {rust_only} — a newly staged fixture must be verified against the C# founding oracle \
         (rust/tools/oracle-conformance.ps1) before being marked rust-only"
    );
}
