use pg_grammar::grammar_health::{GrammarHealthCode, GrammarHealthReport};

#[test]
fn retained_reduplication_refusal_uses_grammar_health() {
    let fixture = pg_conformance_fixtures::require_fixture(
        "edge-cases",
        "realizational-reduplication-no-proposal-route",
    );
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).unwrap();
    let diagnostics = crate::grammar_health::check_fst_diagnostics(&grammar).unwrap();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code.wire(), "fst.reduplication-no-route");
    assert!(diagnostics[0].blocks_fst);
    assert!(diagnostics[0].fst_reason.is_some());
    let report = GrammarHealthReport::new(diagnostics)
        .unwrap()
        .with_fst_checked();
    assert!(report.fst_checked());
    assert_eq!(report.fst_blocking_count(), 1);
}

#[test]
fn alpha_compiler_refusal_has_no_blocking_label() {
    let grammar = pg_grammar::load(include_str!(
        "../../pg-parse/tests/data/alpha-optional-environment.xml"
    ))
    .unwrap();
    let mut diagnostics = Vec::new();
    let incomplete = crate::grammar_health::extend_diagnostics(&grammar, &mut diagnostics).unwrap();
    assert!(incomplete.unwrap().contains("#177"));
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(!GrammarHealthCode::ALL
        .iter()
        .any(|code| code.wire() == "fst.alpha-environment"));
}

fn fixture_grammar(name: &str) -> pg_grammar::model::Grammar {
    let fixture = pg_conformance_fixtures::require_fixture("edge-cases", name);
    pg_grammar::load(&fixture.load_grammar_xml()).unwrap()
}

fn realizational_grammar(
    outputs: &[&str],
    extra_table: &str,
    extra_stratum: &str,
) -> pg_grammar::model::Grammar {
    let fixture = pg_conformance_fixtures::require_fixture(
        "edge-cases",
        "realizational-reduplication-no-proposal-route",
    );
    let xml = fixture.load_grammar_xml();
    let start = xml.find("<MorphologicalSubrule id=\"sub\">").unwrap();
    let end = xml[start..].find("</MorphologicalSubrule>").unwrap()
        + start
        + "</MorphologicalSubrule>".len();
    let subrule = &xml[start..end];
    let output_start = subrule.find("<MorphologicalOutput>").unwrap();
    let output_end = subrule.find("</MorphologicalOutput>").unwrap();
    let subrules = outputs
        .iter()
        .enumerate()
        .map(|(index, output)| {
            format!(
                "{}<MorphologicalOutput>{output}{}",
                &subrule[..output_start],
                &subrule[output_end..]
            )
            .replace("id=\"sub\"", &format!("id=\"sub{index}\""))
        })
        .collect::<String>();
    let xml = format!("{}{subrules}{}", &xml[..start], &xml[end..])
        .replace(
            "<NaturalClasses>",
            &format!("{extra_table}<NaturalClasses>"),
        )
        .replace("</Strata>", &format!("{extra_stratum}</Strata>"));
    pg_grammar::load(&xml).unwrap()
}

fn assert_emitter_label(grammar: &pg_grammar::model::Grammar, expected: &str) {
    let emitted = crate::emit::emit_tuned_surface(grammar);
    let mut diagnostics = Vec::new();
    assert_eq!(
        crate::grammar_health::extend_emission_diagnostics(
            grammar,
            emitted.report,
            &mut diagnostics
        )
        .unwrap(),
        None
    );
    let report = GrammarHealthReport::new(diagnostics)
        .unwrap()
        .with_fst_checked();
    assert_eq!(report.len(), 1, "{report:?}");
    assert_eq!(report.diagnostics()[0].code.wire(), expected);
    assert!(report.diagnostics()[0].blocks_fst);
    assert!(report.diagnostics()[0].fst_reason.is_some());
    assert!(report.diagnostics()[0].subjects[0].internal_id.is_some());
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(value["summary"][0]["code"], expected);
    assert_eq!(value["summary"][0]["blocks_fst"], true);
    let diagnostics = crate::grammar_health::check_fst_diagnostics(grammar).unwrap();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code.wire(), expected);
}

#[test]
fn pattern_root_emission_produces_grammar_health_label() {
    assert_emitter_label(
        &fixture_grammar("pattern-root-required-environment"),
        "fst.pattern-root-shape",
    );
}

#[test]
fn circumfix_emission_produces_grammar_health_label() {
    let grammar = realizational_grammar(
        &[
            "<InsertSegments><PhoneticShape>t</PhoneticShape></InsertSegments><CopyFromInput index=\"stem\"/>",
            "<CopyFromInput index=\"stem\"/><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>",
        ], "", "",
    );
    assert!(crate::emit::eager_route_refuses_mixed_circumfix_zone(
        &grammar
    ));
    assert_emitter_label(&grammar, "fst.circumfix-no-route");
}

#[test]
fn process_emission_produces_grammar_health_label() {
    let grammar = realizational_grammar(
        &["<CopyFromInput index=\"stem\"/><InsertSimpleContext><SimpleContext naturalClass=\"ncAny\"/></InsertSimpleContext>"], "", "",
    );
    assert_emitter_label(&grammar, "fst.process-not-emittable");
}

#[test]
fn missing_surface_spelling_produces_grammar_health_label() {
    let grammar = realizational_grammar(
        &["<CopyFromInput index=\"stem\"/><InsertSegments><PhoneticShape>t</PhoneticShape></InsertSegments>"],
        "<CharacterDefinitionTable id=\"surface\"><Name>Surface</Name><SegmentDefinitions><SegmentDefinition id=\"surfaceA\"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions></CharacterDefinitionTable>",
        "<Stratum characterDefinitionTable=\"surface\"><Name>Surface</Name></Stratum>",
    );
    assert_emitter_label(&grammar, "fst.shape-not-in-surface-table");
}

#[test]
fn invalid_emission_subjects_return_typed_errors() {
    use crate::emit::{EmissionHealth, EmissionSubject};
    use pg_grammar::model::{LexEntryId, MRuleId};
    for (mut report, grammar, invalid_subject) in [
        {
            let grammar = fixture_grammar("pattern-root-required-environment");
            let report = crate::emit::emit_tuned_surface(&grammar).report;
            (
                report,
                grammar,
                EmissionSubject::LexicalEntry(LexEntryId(u32::MAX)),
            )
        },
        {
            let grammar = realizational_grammar(&["<CopyFromInput index=\"stem\"/><InsertSimpleContext><SimpleContext naturalClass=\"ncAny\"/></InsertSimpleContext>"], "", "");
            let report = crate::emit::emit_tuned_surface(&grammar).report;
            (
                report,
                grammar,
                EmissionSubject::MorphRule(MRuleId(u32::MAX)),
            )
        },
    ] {
        let EmissionHealth::Grammar(finding) = &mut report.uncovered[0].grammar_health else {
            panic!("fixture must produce a grammar finding");
        };
        finding.subject = invalid_subject;
        let error =
            crate::grammar_health::extend_emission_diagnostics(&grammar, report, &mut Vec::new())
                .unwrap_err();
        assert!(
            matches!(error, pg_grammar::GrammarError::Semantic(_)),
            "{error:?}"
        );
    }
}
