use super::*;
use crate::readiness_policy::policy_v1;

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// A tiny, ordinary synthetic affix grammar with none of the constructs that would keep the capability gate from reaching `Admit`.
const ADMIT_XML: &str = r#"<HermitCrabInput><Language><Name>Synthetic</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cb" /></SegmentNaturalClass></NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRules="mr1">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <MorphologicalRule id="mr1">
              <Name>-a</Name>
              <MorphologicalSubrules>
                <MorphologicalSubrule id="sub1">
                  <MorphologicalInput>
                    <PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence>
                  </MorphologicalInput>
                  <MorphologicalOutput>
                    <CopyFromInput index="stem" />
                    <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                  </MorphologicalOutput>
                </MorphologicalSubrule>
              </MorphologicalSubrules>
            </MorphologicalRule>
          </MorphologicalRuleDefinitions>
          <LexicalEntries>
            <LexicalEntry id="e1">
              <Allomorphs><Allomorph id="a1"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>
            </LexicalEntry>
          </LexicalEntries>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

/// A single, non-recursive `Compounding` fixture that evaluates to `ConfirmOnly`, giving this module's tests a second, distinct capability decision to exercise.
const CONFIRM_ONLY_XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <CompoundingRule id="cr1">
              <Name>Compound</Name>
              <CompoundingSubrules>
                <CompoundingSubrule>
                  <HeadMorphologicalInput>
                    <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                  </HeadMorphologicalInput>
                  <NonHeadMorphologicalInput>
                    <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                  </NonHeadMorphologicalInput>
                  <MorphologicalOutput>
                    <CopyFromInput index="n0" />
                    <CopyFromInput index="h0" />
                  </MorphologicalOutput>
                </CompoundingSubrule>
              </CompoundingSubrules>
            </CompoundingRule>
          </MorphologicalRuleDefinitions>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

fn passing_measurements(policy: &ThresholdPolicy) -> Measurements {
    Measurements {
        pack_size_bytes: policy.pack_size_max_bytes.value / 2,
        lexicon_entries: policy.lexicon_min_entries.value * 2,
        coverage: CoverageAssessment::Attested {
            attestor: "synthetic-test-attestor".to_string(),
            attested_on: "2026-07-27".to_string(),
            analysis_rate: (policy.coverage_min_analysis_rate.value + 1.0) / 2.0
                + policy.coverage_min_analysis_rate.value / 2.0,
        },
        latency_p50: LatencyMeasurement::Millis(policy.latency_p50_max_ms.value / 2.0),
        latency_p90: LatencyMeasurement::Millis(policy.latency_p90_max_ms.value / 2.0),
        latency_p99: LatencyMeasurement::Millis(policy.latency_p99_max_ms.value / 2.0),
    }
}

fn synthetic_override() -> OverrideRecord {
    OverrideRecord {
        authorized_by: "synthetic-test-operator".to_string(),
        reason: "synthetic field-trial override".to_string(),
        recorded_at: "2026-07-27T00:00:00Z".to_string(),
        overridden_configs: vec![OverriddenConfig {
            predicate: "synthetic.simultaneous.subrule-overlap".to_string(),
            construct: "mrule:synthetic-0001".to_string(),
            witness: "synthetic-witness-form".to_string(),
        }],
    }
}

// Basic tiering

#[test]
fn admit_grammar_with_passing_measurements_and_proven_trust_certifies() {
    let g = load(ADMIT_XML);
    let policy = policy_v1();
    let measurements = passing_measurements(&policy);
    let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);

    assert_eq!(report.tier, Tier::Certified);
    assert!(report.is_certified());
    assert_eq!(report.capability, CapabilitySummary::Admit);
    assert!(report.checks.iter().all(|c| c.outcome.is_pass()));
}

#[test]
fn a_failing_threshold_produces_not_yet_not_not_supported() {
    let g = load(ADMIT_XML);
    let policy = policy_v1();
    let mut measurements = passing_measurements(&policy);
    // Blow the pack-size budget way past the threshold.
    measurements.pack_size_bytes = policy.pack_size_max_bytes.value * 100;
    let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);

    assert_eq!(report.tier, Tier::NotYet);
    let pack_check = report
        .checks
        .iter()
        .find(|c| c.kind == CheckKind::PackSize)
        .expect("pack size check must be present");
    assert!(
        matches!(pack_check.outcome, CheckOutcome::Fail { .. }),
        "expected a Fail outcome, got {:?}",
        pack_check.outcome
    );
}

#[test]
fn confirm_only_grammar_can_still_certify_when_thresholds_pass() {
    // ConfirmOnly is first-class, not a failure: ConfirmOnly + Proven + all thresholds passing must reach Certified, exactly like Admit.
    let g = load(CONFIRM_ONLY_XML);
    let policy = policy_v1();
    let measurements = passing_measurements(&policy);
    let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);

    assert_eq!(report.capability, CapabilitySummary::ConfirmOnly);
    assert_eq!(report.tier, Tier::Certified);
}

// An override-trusted artifact never certifies, proven non-vacuous by sabotage.

/// Sabotage proof: asserts the report certifies cleanly under `Proven` first, then flips only the trust field to `Overridden` with every other input held identical, showing the verdict flips to `NotSupported` with every check `Blocked`.
#[test]
fn override_forces_not_supported_and_blocks_every_check_even_when_everything_else_would_pass() {
    let g = load(ADMIT_XML);
    let policy = policy_v1();
    let measurements = passing_measurements(&policy);

    // Premise: with Proven trust this combination certifies cleanly, or the sabotage below would be vacuous.
    let proven_report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);
    assert_eq!(
        proven_report.tier,
        Tier::Certified,
        "premise failed -- this sabotage test requires a genuinely certifying baseline"
    );
    assert!(proven_report.checks.iter().all(|c| c.outcome.is_pass()));

    // Sabotage: flip ONLY the trust status.
    let overridden_report = certify(
        &g,
        &TrustStatus::Overridden(synthetic_override()),
        Some(&measurements),
        &policy,
    );

    assert_eq!(
        overridden_report.tier,
        Tier::NotSupported,
        "an override-trusted artifact must never certify, even when every threshold passes"
    );
    assert!(
        !overridden_report.is_certified(),
        "is_certified() must be false for an overridden artifact"
    );
    assert!(
        overridden_report
            .checks
            .iter()
            .all(|c| matches!(c.outcome, CheckOutcome::Blocked { .. })),
        "every check must be Blocked under an override, never Pass, Fail, or NotAssessed: {:?}",
        overridden_report.checks
    );
    assert!(
        !overridden_report.checks.iter().any(|c| c.outcome.is_pass()),
        "no threshold result may be presented as passing when the artifact is overridden"
    );
    assert!(
        overridden_report
            .notes
            .iter()
            .any(|n| n.contains("ADR-0005")),
        "the report must state the override is why: {:?}",
        overridden_report.notes
    );
}

#[test]
fn override_blocks_even_a_capability_admit_grammar_under_any_configuration() {
    // "Under any configuration": exercise both capability decisions this module has fixtures for, not just one.
    for xml in [ADMIT_XML, CONFIRM_ONLY_XML] {
        let g = load(xml);
        let policy = policy_v1();
        let measurements = passing_measurements(&policy);
        let report = certify(
            &g,
            &TrustStatus::Overridden(synthetic_override()),
            Some(&measurements),
            &policy,
        );
        assert_eq!(report.tier, Tier::NotSupported);
    }
}

// Not-assessed coverage never renders as passed.

#[test]
fn not_assessed_coverage_blocks_certified_even_when_every_other_check_passes() {
    let g = load(ADMIT_XML);
    let policy = policy_v1();
    let mut measurements = passing_measurements(&policy);
    measurements.coverage = CoverageAssessment::NotAssessed;
    let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);

    let coverage_check = report
        .checks
        .iter()
        .find(|c| c.kind == CheckKind::CoverageAnalysisRate)
        .expect("coverage check must be present");
    assert!(
        matches!(coverage_check.outcome, CheckOutcome::NotAssessed { .. }),
        "expected NotAssessed, got {:?}",
        coverage_check.outcome
    );
    assert!(
        !coverage_check.outcome.is_pass(),
        "not-assessed coverage must never render as passed"
    );
    assert_eq!(
        report.tier,
        Tier::NotYet,
        "an unassessed required check must deny Certified even when every other check passes"
    );
}

#[test]
fn no_measurements_at_all_reports_every_check_not_assessed_never_passed() {
    let g = load(ADMIT_XML);
    let policy = policy_v1();
    let report = certify(&g, &TrustStatus::Proven, None, &policy);

    assert!(
        report
            .checks
            .iter()
            .all(|c| matches!(c.outcome, CheckOutcome::NotAssessed { .. })),
        "every check must be NotAssessed with no measurements supplied: {:?}",
        report.checks
    );
    assert_eq!(report.tier, Tier::NotYet);
}

#[test]
fn attested_coverage_carries_both_fixed_honesty_statements() {
    let g = load(ADMIT_XML);
    let policy = policy_v1();
    let measurements = passing_measurements(&policy);
    let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);

    let coverage_check = report
        .checks
        .iter()
        .find(|c| c.kind == CheckKind::CoverageAnalysisRate)
        .unwrap();
    assert!(coverage_check
        .statements
        .iter()
        .any(|s| s == COVERAGE_RATE_STATEMENT));
    assert!(coverage_check
        .statements
        .iter()
        .any(|s| s == COVERAGE_UNVERIFIED_STATEMENT));
}

// Below-floor latency never renders as a bare zero or a guessed call.

#[test]
fn below_floor_latency_within_threshold_passes_conservatively() {
    let g = load(ADMIT_XML);
    let policy = policy_v1();
    let mut measurements = passing_measurements(&policy);
    measurements.latency_p50 = LatencyMeasurement::BelowFloor { floor_ms: 0.001 };
    let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);
    let p50 = report
        .checks
        .iter()
        .find(|c| c.kind == CheckKind::LatencyP50)
        .unwrap();
    assert!(matches!(p50.outcome, CheckOutcome::Pass { .. }));
}

#[test]
fn below_floor_latency_coarser_than_threshold_is_not_assessed_not_guessed() {
    let g = load(ADMIT_XML);
    let policy = policy_v1();
    let mut measurements = passing_measurements(&policy);
    // A pathologically coarse floor, well above even the loosest threshold.
    measurements.latency_p50 = LatencyMeasurement::BelowFloor {
        floor_ms: 1_000_000.0,
    };
    let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);
    let p50 = report
        .checks
        .iter()
        .find(|c| c.kind == CheckKind::LatencyP50)
        .unwrap();
    assert!(
        matches!(p50.outcome, CheckOutcome::NotAssessed { .. }),
        "a floor coarser than the threshold must be NotAssessed, never a guessed Pass/Fail: \
             {:?}",
        p50.outcome
    );
}

// Report always records the policy version + device class.

#[test]
fn report_records_policy_id_and_device_class() {
    let g = load(ADMIT_XML);
    let policy = policy_v1();
    let report = certify(&g, &TrustStatus::Proven, None, &policy);
    assert_eq!(report.policy_id, policy.policy_id);
    assert_eq!(report.device_class, policy.device_class);
}

// Canonical JSON round trip.

#[test]
fn report_round_trips_through_canonical_json() {
    let g = load(ADMIT_XML);
    let policy = policy_v1();
    let measurements = passing_measurements(&policy);
    let report = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);
    let json = report.to_canonical_json();
    let parsed = ReadinessReport::from_json(&json).expect("valid report JSON must parse");
    assert_eq!(parsed, report);
}

// Golden certificate for one small synthetic fixture, regenerated from the generator's own output, never hand-edited.

/// A deterministic report over the `ADMIT_XML` fixture with fixed, hand-picked measurements, independent of any live grammar/pack state elsewhere in the repo.
fn golden_report() -> ReadinessReport {
    let g = load(ADMIT_XML);
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
    certify(&g, &TrustStatus::Proven, Some(&measurements), &policy)
}

#[track_caller]
fn assert_readiness_verdict_golden(actual: &str, expected: &str) {
    crate::test_support::assert_canonical_lf_text_eq(actual, expected);
}

#[test]
fn readiness_verdict_golden_boundary_accepts_lf_actual_against_crlf_expected() {
    let actual = "{\n  \"report_schema_version\": 1\n}\n";
    let expected = actual.replace('\n', "\r\n");
    assert_ne!(actual, expected);
    assert_readiness_verdict_golden(actual, &expected);
}

#[test]
fn readiness_verdict_golden_boundary_rejects_crlf_actual() {
    let actual = "{\n  \"report_schema_version\": 1\n}\n";
    let expected = "{\n  \"report_schema_version\": 1\n}\n";
    let crlf_actual = actual.replace('\n', "\r\n");
    assert_ne!(crlf_actual, expected);
    let panic = std::panic::catch_unwind(|| {
        assert_readiness_verdict_golden(&crlf_actual, expected);
    });
    assert!(panic.is_err());
}

#[test]
fn readiness_verdict_golden_boundary_rejects_ordering_and_trailing_newline_drift() {
    let ordering = std::panic::catch_unwind(|| {
        assert_readiness_verdict_golden(
            "{\n  \"a\": 1,\n  \"b\": 2\n}\n",
            "{\n  \"b\": 2,\n  \"a\": 1\n}\n",
        );
    });
    assert!(ordering.is_err());

    let trailing_newline = std::panic::catch_unwind(|| {
        assert_readiness_verdict_golden(
            "{\n  \"report_schema_version\": 1\n}",
            "{\n  \"report_schema_version\": 1\n}\n",
        );
    });
    assert!(trailing_newline.is_err());
}

#[test]
#[ignore = "regeneration helper, not a gate: run with --ignored to rewrite the golden from \
                this test's own computation after a reviewed, deliberate change to this module's \
                schema or the golden fixture's inputs"]
fn regenerate_readiness_verdict_golden_json() {
    let json = golden_report().to_canonical_json();
    std::fs::write(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/readiness_verdict_golden.json"
        ),
        json,
    )
    .expect("golden must be writable");
}

#[test]
fn readiness_verdict_golden_json() {
    let report = golden_report();
    let json = report.to_canonical_json();
    assert_readiness_verdict_golden(&json, GOLDEN_JSON);
}

#[test]
fn golden_report_is_certified() {
    // Documents the golden fixture's tier directly, so a reader doesn't have to decode JSON to know it.
    assert_eq!(golden_report().tier, Tier::Certified);
}

const GOLDEN_JSON: &str = include_str!("../readiness_verdict_golden.json");

/// Pins that `not-supported` cites a real refusal on all three reference grammars; `#[ignore]`d and self-skipping since it needs gitignored `samples/data/`.
mod certification_gate {
    use std::path::{Path, PathBuf};

    use super::{certify, CapabilitySummary, CheckOutcome, Tier, TrustStatus};
    use crate::readiness_policy::policy_v1;
    use pg_grammar::model::Grammar;

    fn sample_path(name: &str) -> PathBuf {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.join("../../../samples/data").join(name)
    }

    /// Self-skip guard: gitignored real-corpus fixtures aren't present in a fresh clone or CI.
    fn have(name: &str) -> bool {
        sample_path(name).exists()
    }

    fn load_grammar(xml_name: &str) -> Grammar {
        let path = sample_path(xml_name);
        let xml = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {xml_name}: {e}"))
    }

    /// Runs the not-supported-cites-a-real-refusal assertion against one reference grammar.
    fn assert_not_supported_names_overwrite_output(xml_name: &str) {
        let g = load_grammar(xml_name);
        let policy = policy_v1();

        // No compiled artifact/measurements at all — this grammar is refused before anything would compile.
        let report = certify(&g, &TrustStatus::Proven, None, &policy);

        assert_eq!(
            report.tier,
            Tier::NotSupported,
            "{xml_name}: expected the not-supported tier for a permanently-refused grammar, got {:?}",
            report.tier
        );

        let refusals = match &report.capability {
            CapabilitySummary::Refuse { refusals } => refusals,
            other => panic!(
                "{xml_name}: expected CapabilitySummary::Refuse (per docs/benchmark-matrix.md's own \
                     finding that every reference grammar carries the permanent mpr-group.overwrite-output \
                     carve-out), got {other:?} -- if this construct's disposition has genuinely changed, \
                     pick a different verified-refused fixture"
            ),
        };
        assert!(
            !refusals.is_empty(),
            "{xml_name}: the not-supported tier must cite at least one real refusal"
        );
        assert!(
            refusals
                .iter()
                .any(|r| r.predicate == "mpr-group.overwrite-output"),
            "{xml_name}: expected mpr-group.overwrite-output among the real refusals, got {refusals:?}"
        );
        // Every cited refusal must name both a predicate and a construct — an empty string would be a citation in name only.
        for r in refusals {
            assert!(
                !r.predicate.is_empty(),
                "{xml_name}: refusal must name a predicate: {r:?}"
            );
            assert!(
                !r.construct.is_empty(),
                "{xml_name}: refusal must name a construct: {r:?}"
            );
        }

        // Every check must be forced to NotAssessed, never silently rendered as passed — there is no compiled artifact to measure at all.
        assert!(
            report
                .checks
                .iter()
                .all(|c| matches!(c.outcome, CheckOutcome::NotAssessed { .. })),
            "{xml_name}: every check must be NotAssessed with no compiled artifact: {:?}",
            report.checks
        );
        assert!(
            !report.is_certified(),
            "{xml_name}: a not-supported grammar must never certify"
        );

        // The report's notes must explain the not-supported tier in terms of the real capability evaluation — a bare "not passing" is useless.
        assert!(
            report.notes.iter().any(|n| n.contains("NOT SUPPORTED")),
            "{xml_name}: report notes must explain the not-supported tier: {:?}",
            report.notes
        );
    }

    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with \
                    --include-ignored"]
    fn indonesian_reference_grammar_is_not_supported_citing_overwrite_output() {
        if !have("indonesian-hc.xml") {
            eprintln!("skip: samples/data/indonesian-hc.xml not present locally");
            return;
        }
        assert_not_supported_names_overwrite_output("indonesian-hc.xml");
    }

    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with \
                    --include-ignored"]
    fn amharic_reference_grammar_is_not_supported_citing_overwrite_output() {
        if !have("amharic-hc.xml") {
            eprintln!("skip: samples/data/amharic-hc.xml not present locally");
            return;
        }
        assert_not_supported_names_overwrite_output("amharic-hc.xml");
    }

    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with \
                    --include-ignored"]
    fn sena_reference_grammar_is_not_supported_citing_overwrite_output() {
        if !have("sena-hc.xml") {
            eprintln!("skip: samples/data/sena-hc.xml not present locally");
            return;
        }
        assert_not_supported_names_overwrite_output("sena-hc.xml");
    }
}
