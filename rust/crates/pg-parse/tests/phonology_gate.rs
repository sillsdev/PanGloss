//! Resolved phonology measurement: context comes from the named entry, never from the shape.

use pg_featstruct::SymbolBits;
use pg_grammar_model::chardef::CharDefId;
use pg_grammar_model::model::{
    Dir, Grammar, MorphRuleOrder, MorphemeOwner, MprId, MprSet, PRuleId, Pattern, PatternNode,
    PhonRuleDef, RewriteMode, RewriteRuleDef, RewriteSubruleDef, StratumDef, StratumId, VarTable,
};
use pg_parse::{
    measure_phonology, Morpher, PhonologyCompletion, PhonologyContext, PhonologyMeasurement,
    PhonologyRequest, PhonologyResult, PhonologyStopReason, PhonologyUnavailableReason,
};
use pg_snapshot::FwClass;
use std::time::Duration;

fn seg(id: &str, rep: &str, feats: &str) -> String {
    format!(
        r#"<SegmentDefinition id="{id}"><Representations><Representation>{rep}</Representation></Representations>{feats}</SegmentDefinition>"#
    )
}

fn grammar_xml(entries: &str) -> String {
    let cons = r#"<FeatureValue feature="f_cons" symbolValues="s_cp"/>"#;
    let a = seg(
        "c_a",
        "a",
        r#"<FeatureValue feature="f_cons" symbolValues="s_cm"/>"#,
    );
    let p = seg(
        "c_p",
        "p",
        &format!(
            r#"{cons}<FeatureValue feature="f_voi" symbolValues="s_vm"/><FeatureValue feature="f_poa" symbolValues="s_lab"/>"#
        ),
    );
    let b = seg(
        "c_b",
        "b",
        &format!(
            r#"{cons}<FeatureValue feature="f_voi" symbolValues="s_vp"/><FeatureValue feature="f_poa" symbolValues="s_lab"/>"#
        ),
    );
    let k = seg(
        "c_k",
        "k",
        &format!(
            r#"{cons}<FeatureValue feature="f_voi" symbolValues="s_vm"/><FeatureValue feature="f_poa" symbolValues="s_vel"/>"#
        ),
    );
    let g = seg(
        "c_g",
        "g",
        &format!(
            r#"{cons}<FeatureValue feature="f_voi" symbolValues="s_vp"/><FeatureValue feature="f_poa" symbolValues="s_vel"/>"#
        ),
    );
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>Phon</Name>
<PartsOfSpeech><PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech><PartOfSpeech id="posA"><Name>A</Name></PartOfSpeech></PartsOfSpeech>
<MorphologicalPhonologicalRuleFeatures><MorphologicalPhonologicalRuleFeature id="mprX">x</MorphologicalPhonologicalRuleFeature></MorphologicalPhonologicalRuleFeatures>
<PhonologicalFeatureSystem>
<SymbolicFeature id="f_cons"><Name>cons</Name><Symbols><Symbol id="s_cp">+</Symbol><Symbol id="s_cm">-</Symbol></Symbols></SymbolicFeature>
<SymbolicFeature id="f_voi"><Name>voi</Name><Symbols><Symbol id="s_vp">+</Symbol><Symbol id="s_vm">-</Symbol></Symbols></SymbolicFeature>
<SymbolicFeature id="f_poa"><Name>poa</Name><Symbols><Symbol id="s_lab">lab</Symbol><Symbol id="s_vel">vel</Symbol></Symbols></SymbolicFeature>
</PhonologicalFeatureSystem>
<CharacterDefinitionTable id="t1"><Name>Main</Name><SegmentDefinitions>{a}{p}{b}{k}{g}</SegmentDefinitions></CharacterDefinitionTable>
<Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="linear"><Name>S</Name>{entries}</Stratum></Strata>
</Language></HermitCrabInput>"#
    )
}

const ENTRIES: &str = r#"<LexicalEntries>
<LexicalEntry id="eNx" partOfSpeech="posN" ruleFeatures="mprX"><MorphemeId>m1</MorphemeId><Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>
<LexicalEntry id="eN" partOfSpeech="posN"><MorphemeId>m2</MorphemeId><Allomorphs><Allomorph id="a2"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>
<LexicalEntry id="eA" partOfSpeech="posA" ruleFeatures="mprX"><MorphemeId>m3</MorphemeId><Allomorphs><Allomorph id="a3"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>
</LexicalEntries>"#;

fn char_def(g: &Grammar, xml_id: &str) -> u32 {
    g.char_tables[0]
        .iter()
        .find(|(_, cd)| cd.xml_id() == xml_id)
        .map(|(id, _)| id.0)
        .unwrap_or_else(|| panic!("no char def {xml_id}"))
}

fn rewrite(id: &str, from: u32, to: u32, pos: Option<SymbolBits>, mpr: MprSet) -> RewriteRuleDef {
    let node = |c| Pattern {
        nodes: vec![PatternNode::CharDef(CharDefId(c))],
    };
    RewriteRuleDef {
        xml_id: id.to_string(),
        name: None,
        mode: RewriteMode::Iterative,
        dir: Dir::LeftToRight,
        vars: VarTable::default(),
        lhs: node(from),
        subrules: vec![RewriteSubruleDef {
            required_pos: pos,
            required_mpr: mpr,
            excluded_mpr: MprSet::EMPTY,
            rhs: node(to),
            left_env: None,
            right_env: None,
            self_opaquing: false,
        }],
    }
}

/// Which of `p>b` (N with mprX only) and `b>g` (any) are present, in the given stratum order.
fn grammar_with(order: &[&str], entries: &str) -> Grammar {
    let mut g = pg_grammar::load(&grammar_xml(entries)).expect("fixture loads");
    let mut mpr = MprSet::EMPTY;
    mpr.insert(MprId(0));
    let (p, b, gg) = (
        char_def(&g, "c_p"),
        char_def(&g, "c_b"),
        char_def(&g, "c_g"),
    );
    let mut ids = Vec::new();
    for name in order {
        let rule = match *name {
            "p>b" => rewrite("p>b", p, b, Some(SymbolBits::single(0)), mpr),
            "b>g" => rewrite("b>g", b, gg, None, MprSet::EMPTY),
            other => panic!("unknown rule {other}"),
        };
        ids.push(PRuleId(g.prules.len() as u32));
        g.prules.push(PhonRuleDef::Rewrite(rule));
    }
    g.strata[0].prules = ids;
    g
}

fn standard() -> Grammar {
    grammar_with(&["p>b", "b>g"], ENTRIES)
}

fn hc(id: &str) -> PhonologyContext {
    PhonologyContext::HcXmlEntry {
        entry_id: id.to_string(),
    }
}

fn request(context: PhonologyContext, shape: &str) -> PhonologyRequest {
    PhonologyRequest {
        context,
        underlying_shape: shape.to_string(),
        work_cap: 100_000,
        timeout: Duration::from_secs(30),
        attempt_cap: usize::MAX,
        max_ordered_rule_ids: usize::MAX,
        max_rule_id_chars: usize::MAX,
    }
}

fn measure(g: &Grammar, req: &PhonologyRequest) -> PhonologyMeasurement {
    measure_phonology(&Morpher::new(g, usize::MAX), req)
}

fn surface(g: &Grammar, id: &str, shape: &str) -> String {
    match measure(g, &request(hc(id), shape)).result {
        PhonologyResult::Complete { surface } => surface,
        other => panic!("expected a completed surface, got {other:?}"),
    }
}

fn unavailable(m: &PhonologyMeasurement) -> PhonologyUnavailableReason {
    assert_eq!(m.completion(), PhonologyCompletion::NotStarted);
    assert!(m.ordered_rule_ids.is_empty() && m.work_steps == 0);
    match m.result {
        PhonologyResult::Unavailable { reason } => reason,
        ref other => panic!("expected unavailable, got {other:?}"),
    }
}

#[test]
fn entry_context_gates_decide_the_surface() {
    let g = standard();
    assert_eq!(
        surface(&g, "eNx", "p"),
        "g",
        "POS N with mprX feeds p>b then b>g"
    );
    assert_eq!(
        surface(&g, "eN", "p"),
        "p",
        "same POS without the rule feature"
    );
    assert_eq!(
        surface(&g, "eA", "p"),
        "p",
        "the rule feature without POS N"
    );
}

#[test]
fn unchanged_control_and_ordered_rule_identities() {
    let g = standard();
    let m = measure(&g, &request(hc("eNx"), "a"));
    assert_eq!(
        m.result,
        PhonologyResult::Complete {
            surface: "a".into()
        }
    );
    assert_eq!(m.ordered_rule_ids, ["p>b", "b>g"]);
    assert_eq!(m.resolved_context, Some(hc("eNx")));
    assert!(!m.capped && !m.timed_out && m.work_steps > 0);
    assert!(m.steps > 0);
    assert_eq!(surface(&g, "eNx", "k"), "k");
}

#[test]
fn rule_order_and_removal_change_the_result() {
    let fed = grammar_with(&["p>b", "b>g"], ENTRIES);
    let reversed = grammar_with(&["b>g", "p>b"], ENTRIES);
    let without_second = grammar_with(&["p>b"], ENTRIES);
    let without_first = grammar_with(&["b>g"], ENTRIES);
    assert_eq!(surface(&fed, "eNx", "p"), "g");
    assert_eq!(
        surface(&reversed, "eNx", "p"),
        "b",
        "declared order, not feeding, decides"
    );
    assert_eq!(surface(&without_second, "eNx", "p"), "b");
    assert_eq!(surface(&without_first, "eNx", "p"), "p");
    let m = measure(&reversed, &request(hc("eNx"), "p"));
    assert_eq!(m.ordered_rule_ids, ["b>g", "p>b"]);
}

#[test]
fn multi_stratum_fold_shares_budgets_and_reports_declared_visited_and_applied_order() {
    let mut g = grammar_with(&["p>b", "b>g"], ENTRIES);
    let table = g.strata[0].table;
    g.strata[0].prules = vec![PRuleId(0)];
    g.strata.push(StratumDef {
        name: Some("S2".into()),
        table,
        mrule_order: MorphRuleOrder::Linear,
        prules: vec![PRuleId(1)],
        mrules: Vec::new(),
        templates: Vec::new(),
        entries: Vec::new(),
    });

    let complete = measure(&g, &request(hc("eNx"), "p"));
    assert_eq!(
        complete.result,
        PhonologyResult::Complete {
            surface: "g".into()
        }
    );
    assert_eq!(
        complete.steps, 2,
        "one shared step budget spans both strata"
    );
    assert_eq!(complete.ordered_rule_ids, ["p>b", "b>g"]);
    assert_eq!(complete.visited_rule_ids, ["p>b", "b>g"]);
    assert_eq!(complete.applied_rule_ids, ["p>b", "b>g"]);

    let mut step_limited = request(hc("eNx"), "p");
    step_limited.attempt_cap = 1;
    let stopped = measure(&g, &step_limited);
    assert_eq!(stopped.completion(), PhonologyCompletion::Incomplete);
    assert_eq!(stopped.steps, 1);
    assert!(stopped.capped && !stopped.timed_out);
    assert_eq!(stopped.ordered_rule_ids, ["p>b", "b>g"]);
    assert_eq!(stopped.visited_rule_ids, ["p>b"]);
    assert_eq!(stopped.applied_rule_ids, ["p>b"]);

    let first_only = measure(&grammar_with(&["p>b"], ENTRIES), &request(hc("eNx"), "p"));
    let mut work_limited = request(hc("eNx"), "p");
    work_limited.work_cap = first_only.work_steps + 1;
    let stopped = measure(&g, &work_limited);
    assert_eq!(stopped.completion(), PhonologyCompletion::Incomplete);
    assert!(stopped.capped && stopped.work_steps <= work_limited.work_cap);
    assert_eq!(stopped.ordered_rule_ids, ["p>b", "b>g"]);
    assert_eq!(stopped.visited_rule_ids, ["p>b", "b>g"]);
    assert_eq!(stopped.applied_rule_ids, ["p>b"]);
}

#[test]
fn missing_or_wrong_context_is_unavailable_never_complete() {
    use PhonologyUnavailableReason as Why;
    let g = standard();
    assert_eq!(
        unavailable(&measure(&g, &request(hc("nope"), "p"))),
        Why::UnknownEntry
    );
    assert_eq!(
        unavailable(&measure(&g, &request(hc("eNx"), "zzz"))),
        Why::InvalidShape
    );
    assert_eq!(
        unavailable(&measure(&g, &request(hc("eNx"), ""))),
        Why::InvalidShape
    );

    let empty = grammar_with(&["p>b"], "");
    assert!(empty.entries.is_empty());
    assert_eq!(
        unavailable(&measure(&empty, &request(hc("eNx"), "p"))),
        Why::EmptyLexicon
    );

    let mut dup = standard();
    dup.entries[1].authored_id = "eNx".into();
    assert_eq!(
        unavailable(&measure(&dup, &request(hc("eNx"), "p"))),
        Why::AmbiguousContext
    );

    let mut bad = standard();
    let m = bad.entries[0].morpheme;
    bad.morphemes[m.0 as usize].stratum = StratumId(9);
    assert_eq!(
        unavailable(&measure(&bad, &request(hc("eNx"), "p"))),
        Why::InvalidStratum
    );
}

fn fwdata(entry: &str, msa: &str, class: FwClass) -> PhonologyContext {
    PhonologyContext::FwdataEntry {
        entry_guid: entry.into(),
        msa_guid: msa.into(),
        msa_class: class,
    }
}

fn grammar_with_source_identity() -> Grammar {
    let mut g = standard();
    for (i, (entry, msa)) in [("E1", "M1"), ("E2", "M2"), ("E3", "M3")]
        .into_iter()
        .enumerate()
    {
        g.entries[i].source_guid = Some(entry.into());
        let info = &mut g.morphemes[g.entries[i].morpheme.0 as usize];
        info.source_msa_guid = Some(msa.into());
        info.source_msa_class = Some(FwClass::MoStemMsa);
        info.owner = Some(MorphemeOwner {
            entry_guid: entry.into(),
            keys_by_infl_type: false,
        });
    }
    g
}

#[test]
fn fwdata_context_binds_entry_and_exact_msa() {
    use PhonologyUnavailableReason as Why;
    let g = grammar_with_source_identity();
    let run = |ctx| measure(&g, &request(ctx, "p"));
    let ok = run(fwdata("E1", "M1", FwClass::MoStemMsa));
    assert_eq!(
        ok.result,
        PhonologyResult::Complete {
            surface: "g".into()
        }
    );
    assert_eq!(
        ok.resolved_context,
        Some(fwdata("E1", "M1", FwClass::MoStemMsa))
    );
    let plain = run(fwdata("E2", "M2", FwClass::MoStemMsa));
    assert_eq!(
        plain.result,
        PhonologyResult::Complete {
            surface: "p".into()
        }
    );
    assert_eq!(
        unavailable(&run(fwdata("E1", "M2", FwClass::MoStemMsa))),
        Why::MsaNotOwnedByEntry
    );
    assert_eq!(
        unavailable(&run(fwdata("E1", "M1", FwClass::MoInflAffMsa))),
        Why::MsaNotOwnedByEntry
    );
    assert_eq!(
        unavailable(&run(fwdata("E1", "gone", FwClass::MoStemMsa))),
        Why::MsaNotOwnedByEntry
    );
    assert_eq!(
        unavailable(&run(fwdata("EX", "M1", FwClass::MoStemMsa))),
        Why::UnknownEntry
    );

    let mut shared = grammar_with_source_identity();
    let m = shared.entries[0].morpheme.0 as usize;
    shared.morphemes[m].owner = Some(MorphemeOwner {
        entry_guid: "someone-else".into(),
        keys_by_infl_type: false,
    });
    let got = measure(
        &shared,
        &request(fwdata("E1", "M1", FwClass::MoStemMsa), "p"),
    );
    assert_eq!(unavailable(&got), Why::MsaNotOwnedByEntry);

    let mut twice = grammar_with_source_identity();
    twice.entries[2].source_guid = Some("E1".into());
    let m = twice.entries[2].morpheme.0 as usize;
    twice.morphemes[m].source_msa_guid = Some("M1".into());
    twice.morphemes[m].owner = Some(MorphemeOwner {
        entry_guid: "E1".into(),
        keys_by_infl_type: false,
    });
    let got = measure(
        &twice,
        &request(fwdata("E1", "M1", FwClass::MoStemMsa), "p"),
    );
    assert_eq!(unavailable(&got), Why::AmbiguousContext);
}

#[test]
fn ordered_rule_result_limits_refuse_before_start_and_keep_the_resolved_context() {
    use PhonologyUnavailableReason as Why;
    let g = standard();
    let mut req = request(hc("eNx"), "p");
    req.max_ordered_rule_ids = 1;
    let too_many = measure(&g, &req);
    assert_eq!(
        too_many.result,
        PhonologyResult::Unavailable {
            reason: Why::ResultLimit
        }
    );
    assert_eq!(too_many.completion(), PhonologyCompletion::NotStarted);
    assert_eq!(too_many.resolved_context, Some(hc("eNx")));
    assert!(too_many.ordered_rule_ids.is_empty());
    assert_eq!(too_many.steps, 0);
    assert_eq!(too_many.work_steps, 0);

    req.max_ordered_rule_ids = usize::MAX;
    req.max_rule_id_chars = 1;
    assert_eq!(unavailable(&measure(&g, &req)), Why::ResultLimit);
}

#[test]
fn forced_stops_are_incomplete_never_a_completed_absence() {
    let g = standard();
    let mut req = request(hc("eNx"), "p");
    req.work_cap = 0;
    let m = measure(&g, &req);
    assert_eq!(
        m.result,
        PhonologyResult::Incomplete {
            reason: PhonologyStopReason::WorkCap
        }
    );
    assert!(m.capped && !m.timed_out);
    assert_eq!(m.completion(), PhonologyCompletion::Incomplete);

    let mut req = request(hc("eNx"), "p");
    req.timeout = Duration::ZERO;
    let m = measure(&g, &req);
    assert_eq!(
        m.result,
        PhonologyResult::Incomplete {
            reason: PhonologyStopReason::Timeout
        }
    );
    assert!(m.timed_out);

    let mut req = request(hc("eNx"), "p");
    req.attempt_cap = 0;
    let m = measure(&g, &req);
    assert_eq!(
        m.result,
        PhonologyResult::Incomplete {
            reason: PhonologyStopReason::AttemptCap
        }
    );

    let mut stopped = 0;
    for work_cap in 0..200 {
        let mut req = request(hc("eNx"), "p");
        req.work_cap = work_cap;
        match measure(&g, &req).result {
            PhonologyResult::Complete { surface } => assert_eq!(surface, "g", "cap {work_cap}"),
            PhonologyResult::Incomplete { .. } => stopped += 1,
            other => panic!("cap {work_cap}: {other:?}"),
        }
    }
    assert!(stopped > 1);
}
