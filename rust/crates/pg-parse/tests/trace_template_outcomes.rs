use pg_conformance_fixtures::{discover_filter_passes, require_fixture};
use pg_grammar::load;
use pg_parse::{Morpher, ParseOptions, ParseOutcome};
use pg_rules::trace::{
    FailureReason, PartialParseCause, TemplateSlotOutcome, TemplateSlotStatus, TraceHandle,
    TraceNode, TraceType, TreeTraceSink,
};

fn optional_template_xml() -> String {
    require_fixture("edge-cases", "optional-template-composite").load_grammar_xml()
}

fn exact_span_xml() -> String {
    discover_filter_passes()
        .into_iter()
        .find(|fixture| fixture.name == "exact-span")
        .expect("filter-passes/exact-span fixture must exist")
        .load_grammar_xml()
}

fn identity_multiset(outcome: &ParseOutcome) -> Vec<(String, String, String)> {
    assert_eq!(outcome.analyses.len(), outcome.structured.len());
    let mut identities: Vec<_> = outcome
        .analyses
        .iter()
        .zip(&outcome.structured)
        .map(|((morphemes, surface), structured)| {
            (
                morphemes.clone(),
                surface.clone(),
                serde_json::to_string(structured).expect("analysis identity serializes"),
            )
        })
        .collect();
    identities.sort();
    identities
}

fn assert_search_completed(outcome: &ParseOutcome) {
    assert!(!outcome.capped, "the parse must not hit its step cap");
    assert!(!outcome.timed_out, "the parse must not time out");
    assert!(!outcome.invalid_shape, "the input must have a valid shape");
}

fn collect_nodes(sink: &TreeTraceSink, handle: TraceHandle, out: &mut Vec<TraceNode>) {
    let node = sink.node(handle);
    out.push(node.clone());
    for child in node.children {
        collect_nodes(sink, child, out);
    }
}

fn traced_nodes(sink: &TreeTraceSink) -> Vec<TraceNode> {
    let mut nodes = Vec::new();
    collect_nodes(
        sink,
        sink.root().expect("traced parse has a root"),
        &mut nodes,
    );
    nodes
}

fn slots(node: &TraceNode) -> &[TemplateSlotOutcome] {
    node.template_slots
        .as_deref()
        .expect("rich tracing records the template slot evaluation")
}

#[test]
fn sipu_reports_retained_template_slot_outcomes_without_changing_parse_identities() {
    let fixture_xml = optional_template_xml();
    let grammar = load(&fixture_xml).expect("optional-template fixture loads");
    let morpher = Morpher::new(&grammar, usize::MAX);
    let options = ParseOptions::default();

    let plain = morpher.parse_word_opts("sipu", &options);
    let ordinary_sink = TreeTraceSink::new();
    let ordinary = morpher.parse_word_traced("sipu", &options, &ordinary_sink);
    let rich_sink = TreeTraceSink::with_failure_context();
    let rich = morpher.parse_word_traced("sipu", &options, &rich_sink);

    for outcome in [&plain, &ordinary, &rich] {
        assert_search_completed(outcome);
    }
    let expected = identity_multiset(&plain);
    assert_eq!(identity_multiset(&ordinary), expected);
    assert_eq!(identity_multiset(&rich), expected);
    assert_eq!(plain.signature(), "SIPU+VAC|sipu;SIPU|sipu");

    let template2 = grammar
        .templates
        .iter()
        .position(|template| template.name.as_deref() == Some("template2"))
        .expect("template2 exists");
    let vacuous_rule = grammar.templates[template2].slots[1].rules[0];
    let analysis_nodes = traced_nodes(&rich_sink);

    // Check only the first retained representative to preserve local surface deduplication.
    let retained_applied = analysis_nodes
        .iter()
        .find(|node| {
            node.type_ == TraceType::TemplateAnalysisOutput
                && node.source
                    == pg_rules::trace::TraceSource::Template(pg_grammar_model::model::TemplateId(
                        template2 as u32,
                    ))
                && node.output.is_some()
        })
        .expect("template2 retains a successful surface representative");
    assert_eq!(
        slots(retained_applied),
        &[
            TemplateSlotOutcome {
                slot_index: 0,
                status: TemplateSlotStatus::OptionalSkipped,
                selected_rule: None,
            },
            TemplateSlotOutcome {
                slot_index: 1,
                status: TemplateSlotStatus::Applied,
                selected_rule: Some(vacuous_rule),
            },
            TemplateSlotOutcome {
                slot_index: 2,
                status: TemplateSlotStatus::OptionalSkipped,
                selected_rule: None,
            },
        ],
        "the first retained representative applies the mandatory silent rule only"
    );

    assert!(
        analysis_nodes.iter().any(|node| {
            node.type_ == TraceType::TemplateAnalysisOutput
                && node.source == pg_rules::trace::TraceSource::Template(
                    pg_grammar_model::model::TemplateId(template2 as u32),
                )
                && slots(node).iter().any(|slot| {
                    slot.slot_index == 1 && slot.status == TemplateSlotStatus::RequiredUnfilled
                })
                && slots(node).iter().any(|slot| {
                    slot.slot_index == 0 && slot.status == TemplateSlotStatus::NotReached
                })
        }),
        "the residual path records that the required slot failed and the earlier slot was not reached"
    );

    assert!(
        analysis_nodes.iter().any(|node| {
            node.type_ == TraceType::TemplateSynthesisOutput
                && node.source
                    == pg_rules::trace::TraceSource::Template(pg_grammar_model::model::TemplateId(
                        template2 as u32,
                    ))
                && slots(node).iter().any(|slot| {
                    slot.slot_index == 1
                        && slot.status == TemplateSlotStatus::Applied
                        && slot.selected_rule == Some(vacuous_rule)
                })
                && slots(node)
                    .iter()
                    .filter(|slot| slot.status == TemplateSlotStatus::OptionalSkipped)
                    .count()
                    == 2
        }),
        "successful synthesis records the selected mandatory rule and both optional skips"
    );
}

#[test]
fn nonfinal_template_mutation_reports_a_typed_partial_parse_cause() {
    let fixture_xml = optional_template_xml();
    let mutated = fixture_xml.replace(
        "<AffixTemplate requiredPartsOfSpeech=\"posAweti\">",
        "<AffixTemplate requiredPartsOfSpeech=\"posAweti\" final=\"false\">",
    );
    assert_ne!(mutated, fixture_xml, "fixture mutation must take effect");
    let grammar = load(&mutated).expect("mutated optional-template fixture loads");
    let morpher = Morpher::new(&grammar, usize::MAX);
    let sink = TreeTraceSink::with_failure_context();
    let outcome = morpher.parse_word_traced("kasiputa", &ParseOptions::default(), &sink);
    assert_search_completed(&outcome);
    assert!(outcome.analyses.is_empty());

    let partial_nodes: Vec<_> = traced_nodes(&sink)
        .into_iter()
        .filter(|node| node.failure_reason == Some(FailureReason::PartialParse))
        .collect();
    assert!(
        partial_nodes.iter().any(|node| {
            node.partial_parse_cause == Some(PartialParseCause::NonFinalTemplateAppliedLast)
        }),
        "the nonfinal-template rejection should carry its typed cause: {partial_nodes:?}"
    );
}

#[test]
fn exact_span_parse_records_remaining_stratum_rules_without_changing_parse_identities() {
    let fixture_xml = exact_span_xml();
    let grammar = load(&fixture_xml).expect("exact-span fixture loads");
    let morpher = Morpher::new(&grammar, usize::MAX);
    let plain = morpher.parse_word("matinlu");
    let sink = TreeTraceSink::with_failure_context();
    let traced = morpher.parse_word_traced("matinlu", &ParseOptions::default(), &sink);

    assert_search_completed(&plain);
    assert_search_completed(&traced);
    assert_eq!(identity_multiset(&traced), identity_multiset(&plain));
    assert!(
        traced_nodes(&sink).iter().any(|node| {
            node.failure_reason == Some(FailureReason::PartialParse)
                && node.partial_parse_cause == Some(PartialParseCause::RemainingRulesInStratum)
        }),
        "matinlu should exercise the stratum gate that rejects candidates with a pending rule"
    );
}

#[test]
fn applicable_but_unapplied_templates_record_their_partial_parse_cause() {
    assert_applicable_but_unapplied_template_cause(&optional_template_xml());
}

#[test]
fn applicable_but_unapplied_templates_record_their_partial_parse_cause_from_crlf_fixture() {
    let fixture_xml = optional_template_xml().replace("\r\n", "\n");
    assert_applicable_but_unapplied_template_cause(&fixture_xml.replace('\n', "\r\n"));
}

fn assert_applicable_but_unapplied_template_cause(fixture_xml: &str) {
    // Upstream fixtures can use CRLF even when staged fixtures are checked out with LF.
    let fixture_xml = fixture_xml.replace("\r\n", "\n");
    // Require absent material so every applicable template fails on `toʼa`.
    let mutated = fixture_xml
        .replace(
            "<PhoneticSequence id=\"stemVac\"><OptionalSegmentSequence min=\"1\" max=\"-1\"><SimpleContext naturalClass=\"ncAny\" /></OptionalSegmentSequence></PhoneticSequence>",
            "<PhoneticSequence id=\"stemVac\"><OptionalSegmentSequence min=\"1\" max=\"-1\"><SimpleContext naturalClass=\"ncAny\" /></OptionalSegmentSequence><Segment segment=\"cU\" /></PhoneticSequence>",
        )
        .replace(
            "<Name>template1</Name>\n            <Slot optional=\"true\" morphologicalRules=\"mrOpt1\"><Name>t1pfx</Name></Slot>",
            "<Name>template1</Name>\n            <Slot morphologicalRules=\"mrVacuous\"><Name>t1pfx</Name></Slot>",
        )
        .replace(
            "<Slot optional=\"true\" morphologicalRules=\"mrTrunc\"><Name>t3trunc</Name></Slot>",
            "<Slot morphologicalRules=\"mrTrunc\"><Name>t3trunc</Name></Slot>",
        );
    assert_ne!(mutated, fixture_xml, "fixture mutations must take effect");
    assert!(
        mutated.contains("<Segment segment=\"cU\" />")
            && mutated.contains("<Slot morphologicalRules=\"mrVacuous\"><Name>t1pfx</Name></Slot>")
            && mutated.contains("<Slot morphologicalRules=\"mrTrunc\"><Name>t3trunc</Name></Slot>"),
        "each intended template failure must be represented in the mutated XML"
    );

    let grammar = load(&mutated).expect("mutated optional-template fixture loads");
    let morpher = Morpher::new(&grammar, usize::MAX);
    let plain = morpher.parse_word("toʼa");
    let sink = TreeTraceSink::with_failure_context();
    let traced = morpher.parse_word_traced("toʼa", &ParseOptions::default(), &sink);

    assert_search_completed(&plain);
    assert_search_completed(&traced);
    assert_eq!(identity_multiset(&traced), identity_multiset(&plain));
    assert!(
        traced_nodes(&sink).iter().any(|node| {
            node.failure_reason == Some(FailureReason::PartialParse)
                && node.partial_parse_cause
                    == Some(PartialParseCause::ApplicableTemplatesNotApplied)
        }),
        "the applicable-template gate should report that all required template applications failed"
    );
}
