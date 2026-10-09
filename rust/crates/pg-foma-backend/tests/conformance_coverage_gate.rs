//! Gates generic kind evidence and concrete variant obligations against passing fixtures.

use std::collections::HashSet;

use pg_conformance_fixtures::discover;
use pg_foma_backend::conformance_coverage::{supported_coverage_report, CoverageStatus};
use pg_foma_backend::fixture_coverage::passing_covered_constructs;

/// Retains the generic kind contract independently of the concrete variant ratchet.
#[test]
fn supported_construct_conformance_coverage_has_no_gaps() {
    let replay = passing_covered_constructs(&discover());
    assert!(
        replay.load_failures.is_empty(),
        "fixture load failures: {:?}",
        replay.load_failures
    );
    assert!(
        replay.invalid_variant_tags.is_empty(),
        "invalid variant tags: {:?}",
        replay.invalid_variant_tags
    );
    let covered = replay.passing_constructs;
    let covered_refs: HashSet<&str> = covered.iter().map(String::as_str).collect();
    let report = supported_coverage_report(&covered_refs);

    // Non-vacuity first: a report that enumerated nothing would make every assertion below pass trivially.
    assert_eq!(
        report.len(),
        pg_foma::capability::CharacteristicKind::ALL.len(),
        "the coverage report must enumerate EVERY CharacteristicKind ({} expected, {} reported) -- \
         a short report would make the build-breaking assertions below vacuous",
        pg_foma::capability::CharacteristicKind::ALL.len(),
        report.len()
    );

    let mut covered_n = 0usize;
    let mut uncovered = Vec::new();
    let mut unmappable = Vec::new();
    // Split the non-Covered set by disposition: Proven is a hard error, ConfigPredicate/ConfirmOnly are also required but reported separately.
    let mut proven_gaps = Vec::new();
    let mut config_or_confirm_gaps = Vec::new();
    for row in &report {
        match row.status {
            CoverageStatus::Covered => covered_n += 1,
            CoverageStatus::Uncovered => uncovered.push(row.kind),
            CoverageStatus::Unmappable => unmappable.push(row.kind),
        }
        if row.status != CoverageStatus::Covered {
            match row.disposition {
                pg_foma::capability::Disposition::Proven => proven_gaps.push(row.kind),
                pg_foma::capability::Disposition::ConfigPredicate
                | pg_foma::capability::Disposition::ConfirmOnly => {
                    config_or_confirm_gaps.push(row.kind)
                }
            }
        }
    }

    eprintln!(
        "=== conformance-coverage cross-check (ADR 0001; generic kind mapping) \
         BUILD-BREAKING ===\n\
         CharacteristicKinds: {} total | {covered_n} covered | {} uncovered | {} unmappable",
        report.len(),
        uncovered.len(),
        unmappable.len(),
    );
    for row in &report {
        eprintln!(
            "  {:?}: disposition={:?} status={:?} (mapped construct ids: {:?})",
            row.kind, row.disposition, row.status, row.construct_ids
        );
    }
    // The gate. Reported by disposition, not one undifferentiated count, so a failure says what KIND of evidence is missing.
    assert!(
        unmappable.is_empty(),
        "MAPPING-CONTRACT REGRESSION: {} CharacteristicKind(s) have no constructs.txt row at all: \
         {unmappable:?}\n\
         This is a vocabulary gap, not a fixture gap -- a row must be added upstream (see \
         sillsdev/machine#465 for the precedent) and mapped in \
         `conformance_coverage::construct_ids_for`. Full report above.",
        unmappable.len()
    );
    assert!(
        proven_gaps.is_empty(),
        "COVERAGE REGRESSION (Proven): {proven_gaps:?} are admission-filtered unconditionally yet \
         have no passing conformance fixture tagging their construct id.\n\
         A Proven construct with no covering fixture is the strongest form of this gap: the \
         compiler admits it with no evidence. Either a fixture regressed (check whether its words \
         still pass -- a FAILING word's exercises: tags do not count), or a tag is not a literal \
         constructs.txt row id (tests/exercises_tag_liveness.rs catches that specifically). Full \
         report above."
    );
    assert!(
        config_or_confirm_gaps.is_empty(),
        "COVERAGE REGRESSION (ConfigPredicate/ConfirmOnly): {config_or_confirm_gaps:?} are \
         compiled and relied upon, and ADR 0001 requires them evidenced too -- ConfirmOnly means \
         'the oracle prunes over-generation', never 'no fixture needed'. Same two likely causes as \
         the Proven case above. Full report above."
    );
    assert_eq!(
        covered_n,
        report.len(),
        "internal inconsistency: {covered_n} of {} rows are Covered yet every gap list above is \
         empty -- the status/disposition split in this test has drifted from CoverageStatus",
        report.len()
    );
}

#[test]
fn reachable_variant_coverage_does_not_regress() {
    use pg_foma_backend::coverage_ledger::{build_ledger, obligation_met};
    let replay = passing_covered_constructs(&discover());
    assert!(
        replay.load_failures.is_empty(),
        "load failures: {:?}",
        replay.load_failures
    );
    assert!(
        replay.invalid_variant_tags.is_empty(),
        "invalid tags: {:?}",
        replay.invalid_variant_tags
    );
    let refs = replay
        .passing_constructs
        .iter()
        .map(String::as_str)
        .collect();
    let ledger = build_ledger(&pg_foma::capability::default_registry(), &refs);
    let mut missing_fixture = 0;
    let mut missing_containment = 0;
    let mut unmet = 0;
    for row in &ledger.rows {
        if !obligation_met(row) {
            unmet += 1;
            eprintln!(
                "unmet {:?} {:?}: fixture={:?} containment={:?}",
                row.kind,
                row.variant,
                row.conformance_status,
                row.containment.as_ref().map(|c| &c.citation)
            );
        }

        missing_fixture += usize::from(row.conformance_status != CoverageStatus::Covered);
        missing_containment += usize::from(row.containment.is_none());
    }
    eprintln!("variant obligations: {} total; {unmet} unmet; {missing_fixture} without fixture; {missing_containment} without containment", ledger.rows.len());
    assert_eq!(
        ledger.rows.len(),
        37,
        "variant inventory changed; review the new obligations"
    );
    assert!(
        missing_fixture <= 6,
        "{missing_fixture} obligations lack passing fixtures, exceeding 6"
    );
    assert!(
        missing_containment <= 11,
        "{missing_containment} obligations lack containment citations, exceeding 11"
    );
    assert!(unmet <= 9, "{unmet} variant obligations unmet, exceeding 9");
}

fn assert_hc_xml_permanent_refusal_fixture(
    name: &str,
    variant: pg_foma::capability::ConstructVariant,
    predicate: &str,
    refusal: &str,
    loader_citation: &str,
) {
    use pg_conformance_fixtures::{
        assert_matches_oracle, require_fixture, FieldworksProducibility,
    };
    use pg_foma::capability::{
        compose_envelope, default_registry, observed_variants, CompileDecision,
    };
    use pg_foma::enumerate::{enumerate_default, prules_in_order};
    use pg_foma::junctions::PhonologyProbe;
    use pg_foma_backend::coverage_ledger::{build_ledger, obligation_met};
    use pg_foma_backend::fixture_coverage::coverage_for_grammar;
    use std::collections::HashSet;

    let fixture = require_fixture("edge-cases", name);
    let words = fixture.load_words_yaml();
    let FieldworksProducibility::EngineOnly { notes } = &words.fieldworks_producible else {
        panic!(
            "{} must identify its HCLoader-only authoring gap",
            fixture.label()
        );
    };
    assert!(notes.contains(loader_citation), "{notes}");

    let grammar = pg_grammar::load(&fixture.load_grammar_xml())
        .unwrap_or_else(|error| panic!("{}: {error}", fixture.label()));
    let registry = default_registry();
    assert_eq!(
        observed_variants(&grammar, &registry),
        HashSet::from([variant]),
        "{} must exercise exactly {}",
        fixture.label(),
        variant.id()
    );

    let rewrite_rules = prules_in_order(&grammar);
    let phonology = PhonologyProbe::new(&grammar);
    let plan = enumerate_default(&grammar, &rewrite_rules, phonology.as_ref());
    let CompileDecision::Refuse(diagnostics) = compose_envelope(&grammar, &plan, &registry) else {
        panic!("{} must be refused by FST capability", fixture.label());
    };
    assert_eq!(diagnostics.len(), 1, "{}: {diagnostics:?}", fixture.label());
    assert_eq!(diagnostics[0].predicate, predicate, "{diagnostics:?}");

    let coverage = coverage_for_grammar(&fixture.label(), &grammar, &words);
    assert!(coverage.invalid_variant_tags.is_empty(), "{coverage:?}");
    assert!(
        coverage.passing_constructs.contains(variant.id()),
        "{} does not pass with its exact variant tag",
        fixture.label()
    );

    let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
    assert_eq!(
        assert_matches_oracle(&fixture.label(), &words, &morpher),
        words.words.len(),
        "every oracle word must be exercised"
    );

    let covered: HashSet<&str> = coverage
        .passing_constructs
        .iter()
        .map(String::as_str)
        .collect();
    let ledger = build_ledger(&registry, &covered);
    let row = ledger
        .row(variant.kind(), Some(variant.id()))
        .expect("the variant must remain in the ledger");
    assert_eq!(row.conformance_status, CoverageStatus::Covered);
    assert_eq!(row.permanent_refusal.as_deref(), Some(refusal));
    assert!(obligation_met(row), "{row:?}");
}

#[test]
fn realizational_reduplication_has_a_permanent_hc_xml_only_refusal_fixture() {
    assert_hc_xml_permanent_refusal_fixture(
        "realizational-reduplication-no-proposal-route",
        pg_foma::capability::ConstructVariant::ReduplicationUnrouted,
        "reduplication.peel-eligible-rule-kind",
        "not authorable in FieldWorks; HC-XML only (HCLoader.cs:976-979)",
        "HCLoader.cs:976-979",
    );
}

#[test]
fn declared_supported_variants_are_observed_by_the_fixture_inventory() {
    use pg_foma::capability::{default_registry, observed_variants};
    let registry = default_registry();
    let declared: HashSet<_> = registry
        .predicates()
        .iter()
        .flat_map(|p| p.variants().iter().copied())
        .collect();
    assert_eq!(declared.len(), 24);
    let mut all_observed = HashSet::new();
    for fixture in discover() {
        let words = fixture.load_words_yaml();
        if words.skip_in_generic_replay().is_some() {
            continue;
        }
        let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load");
        let observed = observed_variants(&grammar, &registry);
        assert!(observed.is_subset(&declared));
        all_observed.extend(observed.iter().copied());
        let mut variants: Vec<_> = observed.into_iter().map(|v| v.id()).collect();
        variants.sort_unstable();
        if !variants.is_empty() {
            eprintln!("VARIANTS {}: {variants:?}", fixture.label());
        }
    }
    for variant in declared
        .iter()
        .filter(|v| v.disposition() != pg_foma::capability::VariantDisposition::Refuse)
    {
        assert!(
            all_observed.contains(variant),
            "declared supported variant {} has no structural witness",
            variant.id()
        );
    }
}
