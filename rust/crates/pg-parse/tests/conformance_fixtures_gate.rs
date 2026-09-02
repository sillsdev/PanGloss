//! Dual-root conformance fixture replay: every fixture discovered under both `machine/conformance/**` and `conformance-staging/**` is loaded and every `words.yaml` word checked against `pg_parse::Morpher`.
//! See docs/conformance-staging-plan.md for the design and `machine/conformance/PROTOCOL.md` for the fixture format.

use pg_conformance_fixtures::{
    all_staged_fixtures, assert_matches_oracle, discover, graduation_guard_violations,
    OracleProvenance,
};
use pg_parse::Morpher;

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
        let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);
        let checked = assert_matches_oracle(&f.label(), &words_yaml, &morpher);
        assert!(
            checked > 0,
            "{}: replayed zero words (every word guess-only or the fixture is empty?)",
            f.label()
        );
        total_checked += checked;
    }
    eprintln!(
        "conformance_fixtures_gate: {total_checked} words checked across {} fixtures ({} skipped)",
        fixtures.len() - total_skipped_fixtures,
        total_skipped_fixtures
    );
}

/// Named regression pin for the four affix-shape constructs, so their coverage doesn't silently disappear if `all_discovered_fixtures_match_oracle` is ever narrowed.
#[test]
fn w91_affix_shapes_covered_by_upstream_fixtures() {
    let fixtures = discover();
    let austronesian = fixtures
        .iter()
        .find(|f| f.category == "languages" && f.name == "metathesis-phase-isolation")
        .expect(
            "languages/metathesis-phase-isolation must be discoverable (machine submodule initialized?)",
        );
    let truncate = fixtures
        .iter()
        .find(|f| f.category == "edge-cases" && f.name == "truncate-morphotactic")
        .expect("edge-cases/truncate-morphotactic must be discoverable");

    let g = pg_grammar::load(&austronesian.load_grammar_xml()).unwrap();
    let morpher = Morpher::new(&g, usize::MAX).with_memo(true);
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
    let morpher2 = Morpher::new(&g2, usize::MAX).with_memo(true);
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
