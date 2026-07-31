//! Dual-root conformance fixture replay: every fixture discovered under BOTH
//! `machine/conformance/**` (the `sillsdev/machine` submodule) and `conformance-staging/**` (this
//! repo, committed) is loaded and every one of its `words.yaml` words is checked against
//! `pg_parse::Morpher`, the oracle for anything authored/verified against pangloss (staged fixtures'
//! `STAGING.md` says so explicitly per fixture) and the reference implementation being ported
//! wherever a fixture's ground truth instead traces back to the C# founding oracle (every
//! `machine/conformance` fixture). See `docs/conformance-staging-plan.md` for the design and
//! `machine/conformance/PROTOCOL.md` for the fixture format this replays.
//!
//! ## Resolves the `affix_shapes_conformance.rs` debt
//! That file's 4 permanently-`#[ignore]`d tests (`infix_matches_oracle`, `circumfix_matches_oracle`,
//! `noncontiguous_matches_oracle`, `truncate_matches_oracle`) pointed at a `rust/conformance/
//! affix-shapes/` directory that was never populated (see git blame — no commit ever created it).
//! Investigation (docs/conformance-staging-plan.md task) found all four already covered upstream,
//! under the SAME "W9.1 probe" provenance strings the dead tests' own doc comments cited:
//! - infix (`AffixProcessRuleTests.InfixRules`) — `languages/metathesis-phase-isolation`'s `sumulat` word.
//! - circumfix (`AffixProcessRuleTests.CircumfixRules`) — `languages/metathesis-phase-isolation`'s `keadilan`.
//! - noncontiguous (`AffixProcessRuleTests.NonContiguousRules`) — `languages/metathesis-phase-isolation`'s
//!   `katibɯd` (+ its `katabɯd` obligatory-rewrite negative control).
//! - truncate (`AffixProcessRuleTests.TruncateRules`) — `edge-cases/truncate-morphotactic`, whose
//!   own module doc says outright "Converted from conformance/affix-shapes/truncate (v1)", AND
//!   `languages/metathesis-phase-isolation`'s own `pur`/`pure` pair.
//!   `affix_shapes_conformance.rs` has been deleted; this file's generic replay exercises all four
//!   constructs for real (via the fixtures above) as part of the normal, non-ignored default suite —
//!   a strictly stronger gate than the dead placeholder ever ran, since it now runs unconditionally
//!   instead of being permanently skipped.

use pg_conformance_fixtures::{assert_matches_oracle, discover, graduation_guard_violations};
use pg_parse::Morpher;

/// The graduation guard (`docs/conformance-staging-plan.md`): FAILS if the same `(category, name)`
/// fixture identity exists under both `machine/conformance/` and `conformance-staging/` — the
/// enforcement behind "once a fixture is accepted upstream, its staged copy is deleted in the same
/// change". Runs unconditionally; tolerates either root being absent (an uninitialized `machine`
/// submodule degrades this to "staging-only", never panics).
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

/// Every discovered fixture's `words.yaml` replayed against `pg_parse::Morpher`. Pathological
/// (`budget_ms`) and crash-pinning (`expect_crash`) fixtures are skipped (see
/// `WordsYaml::skip_in_generic_replay`'s doc) — the default suite must stay small and fast, and a
/// crash fixture has no signature to diff against in the first place.
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

/// Named regression pin for the four W9.1 affix-shape constructs specifically (the debt this file
/// resolves), so their coverage doesn't silently disappear if `all_discovered_fixtures_match_oracle`
/// is ever narrowed. Directly replays the exact words the dead tests named.
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
    // truncate-morphotactic's own distinguishing row: "gas" has TWO distinct analyses (direct +
    // chained), per that fixture's words.yaml note — the unplanned-second-analysis pin.
    let gas = morpher2.parse_word("gas").signature();
    assert_eq!(
        gas, "++|gas;+|gas",
        "gas must yield both the direct and chained analyses"
    );
}
