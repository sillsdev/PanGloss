use super::*;

// The `&Grammar` front ends, used only by the golden-render test below; the live command drives the `_with_semantics` forms off its one shared owner.
use crate::readiness_verdict::certify;
use pg_foma_backend::plan_diagram::build_plan_document;
// Test-only: hoisting these to the module head made the production build warn on every compile.
use crate::readiness_verdict::{CoverageAssessment, LatencyMeasurement};

/// An ordinary `Admit`-verdict grammar: one bare root, no MPR groups, no `Compounding`.
const ADMIT_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MakeReportAdmitFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="e1">
            <Allomorphs><Allomorph id="e1-1"><PhoneticShape>kat</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>kat</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn backend_assessment_renderer_includes_shapes_and_cost_evidence() {
    let assessment = pg_pack::BackendAssessment {
        backend: "synthetic-backend".to_string(),
        decision: "admit".to_string(),
        status: "accepted".to_string(),
        findings: Vec::new(),
        failed_predicates: Vec::new(),
        shapes: vec!["synthetic-shape".to_string()],
        cost_evidence: vec![pg_pack::BackendCostEvidence {
            metric: pg_foma_backend::health::Metric::CompositeRulePairCount,
            value: pg_foma_backend::health::MetricValue::Count(42),
            threshold: Some(pg_foma_backend::health::MetricValue::Count(10)),
            provenance: pg_foma_backend::health::ValueProvenance::ProvenBound,
        }],
        advice_references: Vec::new(),
        status_detail: None,
    };

    let rendered = render_backend_assessments(Some(std::slice::from_ref(&assessment)))
        .expect("assessment rendering must produce markdown");
    assert!(rendered.contains("Shapes: synthetic-shape"), "{rendered}");
    assert!(rendered.contains("Cost evidence:"), "{rendered}");
    assert!(
        rendered.contains("metric=CompositeRulePairCount"),
        "{rendered}"
    );
    assert!(rendered.contains("value=Count(42)"), "{rendered}");
    assert!(rendered.contains("threshold=Some(Count(10))"), "{rendered}");
    assert!(rendered.contains("provenance=ProvenBound"), "{rendered}");
}

// A golden report over fixed, hand-picked inputs (never a live timer), since a live end-to-end run's real wall-clock timing would make a byte-for-byte golden inherently flaky.

fn golden_report_markdown() -> String {
    let g = pg_grammar::load(ADMIT_GRAMMAR_XML).expect("golden fixture must load");
    let policy = policy_v1();
    let measurements = Measurements {
        pack_size_bytes: 12_345,
        lexicon_entries: 2_000,
        coverage: CoverageAssessment::Attested {
            attestor: "synthetic-golden-attestor".to_string(),
            attested_on: "2026-07-27".to_string(),
            analysis_rate: 0.95,
        },
        latency_p50: LatencyMeasurement::Millis(0.5),
        latency_p90: LatencyMeasurement::Millis(2.0),
        latency_p99: LatencyMeasurement::Millis(10.0),
    };
    let verdict = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);

    // Real, deterministic composition, never a live timer: the same functions the live command calls, over the same fixed fixture.
    let plan_doc = build_plan_document(&g);
    let render = render_mermaid(&plan_doc, RenderMode::default());
    let mermaid_summary_line = render_mermaid_summary_line(&render);

    render_markdown(
        "MakeReportAdmitFixture",
        "synthetic-golden-fixture.xml",
        &sha256_hex(ADMIT_GRAMMAR_XML.as_bytes()),
        "report.md",
        &policy,
        &verdict,
        None,
        "0.500 ms (compiling the propose+confirm analyzer this report's latency numbers were \
             measured against; informational only -- no threshold in the declared policy gates \
             this figure).",
        "Measured in-process via nanosecond `Instant`/`Duration` timing over a real \
             `FomaAnalyzer`: 1 word(s), 7 timed samples/word after 1 discarded warmup call, this \
             run's calibrated timer floor is 100ns. Word source: fixed synthetic golden fixture. \
             p50/p90/p99 are the nearest-rank percentile over each word's own median duration.",
        "ATTESTED -- attestor=`synthetic-golden-attestor`, attested_on=`2026-07-27`, \
             corpus=`synthetic-golden-corpus.txt` (sha256=`0000000000000000000000000000000000000000000000000000000000000000`), \
             analysis_rate=0.9500. UNVERIFIED beyond the named attestor's own claim (nothing in \
             the artifact records what its author read while authoring, and PanGloss does not \
             train).",
        &render.mermaid,
        &mermaid_summary_line,
        "built in-process for this report, not persisted to disk (sha256=`fixed-golden-sha`, \
             package_fingerprint=`fixed-golden-fingerprint`)",
        "`synthetic-golden-corpus.txt` (sha256=`0000000000000000000000000000000000000000000000000000000000000000`)",
        "1 machine (heads/main)",
        "0000000000000000000000000000000000000000",
        &[
            "correctness: NOT CERTIFIED HERE -- coverage (when assessed) is a token-level \
                 analysis RATE, never accuracy; correctness evidence comes from the synthetic \
                 conformance suite, not from this report."
                .to_string(),
        ],
    )
}

#[track_caller]
fn assert_make_report_golden(actual: &str, expected: &str) {
    crate::test_support::assert_rendered_text_eq(actual, expected);
}

#[test]
fn make_report_raw_golden_boundary_would_reject_crlf_materialized_fixture() {
    let actual = "# Report\n";
    let expected = "# Report\r\n";
    assert_ne!(actual, expected);
    assert_make_report_golden(actual, expected);
}

#[test]
fn make_report_golden_rejects_whitespace_and_unicode_drift() {
    let whitespace = std::panic::catch_unwind(|| {
        assert_make_report_golden("# Report\nvalue\tA\n", "# Report\nvalue A\n");
    });
    assert!(whitespace.is_err());

    let unicode = std::panic::catch_unwind(|| {
        assert_make_report_golden("# Report\nnaïve\n", "# Report\nnaive\n");
    });
    assert!(unicode.is_err());
}

#[test]
#[ignore = "regeneration helper, not a gate: run with --ignored to rewrite the golden from \
                this test's own computation after a reviewed, deliberate change to this module's \
                report shape or the golden fixture's inputs"]
fn regenerate_make_report_golden_md() {
    let md = golden_report_markdown();
    fs::write(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/make_report_golden.md"),
        md,
    )
    .expect("golden must be writable");
}

#[test]
fn make_report_golden_md() {
    let md = golden_report_markdown();
    assert_make_report_golden(&md, GOLDEN_MD);
}

const GOLDEN_MD: &str = include_str!("../make_report_golden.md");
