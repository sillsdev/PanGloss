//! Regression gate for `Allomorph.IsWordValid`'s sub-gates: environments, bound roots, and per-allomorph required syntactic FS.

use pg_grammar_model::model::{
    AllomorphCoOccurrenceRuleDef, AllomorphId, CoOccurrenceAdjacency, EnvironmentDef,
    EnvironmentSource, Grammar, MorphRuleDef, Pattern, PatternNode,
};
use pg_rules::cache::RuleCache;
use pg_rules::trace::{FailureReason, RejectionEvidence, TraceSink, TraceType, TreeTraceSink};
use pg_rules::validity::{allomorphs_valid, allomorphs_valid_cached_traced, environments_ok};
use pg_rules::word::MorphRecord;
use pg_rules::Word;
use pg_shape::{NodeKind, Shape, ShapeBuilder};

#[path = "common/mod.rs"]
mod common;
use common::{ctx, load_probe_grammar, nat_class};

// Part 1 -- `environments_ok`: the anchored left/right matching primitive, hand-built inputs.

fn probe_shape(g: &Grammar, text: &str) -> Shape {
    let t = &g.char_tables[0];
    let seg = pg_grammar_model::segment::segment(t, text).expect("segments");
    let w = g.phon_features.len() as u32;
    let mut b = ShapeBuilder::with_features_capacity(w, seg.len());
    for (_, kind, cd, _) in seg.interior() {
        let mut lanes = vec![u64::MAX; w as usize];
        for (i, &l) in t
            .get(pg_grammar_model::chardef::CharDefId(cd))
            .feature_lanes()
            .iter()
            .enumerate()
        {
            lanes[i] = l;
        }
        match kind {
            NodeKind::Segment => b.push_segment_with_lanes(cd, &lanes),
            NodeKind::Boundary => b.push_boundary_with_lanes(cd, &lanes),
            _ => {}
        }
    }
    b.finish()
}

fn single(g: &Grammar, nc: &str) -> Pattern {
    Pattern {
        nodes: vec![PatternNode::Context(ctx(nat_class(g, nc)))],
    }
}

fn env(require: bool, left: Option<Pattern>, right: Option<Pattern>) -> EnvironmentDef {
    EnvironmentDef::new(require, left, right)
}

#[test]
fn environment_source_metadata_does_not_change_semantic_equality() {
    let pattern = Some(single(&load_probe_grammar(), "nc_d"));
    let parsed = env(true, pattern.clone(), None);
    let authored = EnvironmentDef {
        source: Some(EnvironmentSource {
            id: Some("authored-guid".into()),
            text: Some("/d_".into()),
        }),
        ..env(true, pattern, None)
    };

    assert_eq!(parsed, authored);
    assert_eq!(format!("{parsed:?}"), format!("{authored:?}"));
}

/// A required environment with both sides declared must anchor each side to the correct edge of the morph span; every permutation but the true `d _ t` orientation on "dat" fails at least one side.
#[test]
fn environments_ok_anchors_left_and_right_to_the_correct_sides() {
    let g = load_probe_grammar();
    let e = env(true, Some(single(&g, "nc_d")), Some(single(&g, "nc_t")));
    let envs = vec![e];

    // "dat": preceded by d, followed by t -- both sides hold.
    let dat = probe_shape(&g, "dat");
    assert!(
        environments_ok(&g, &envs, &dat, 1, 1),
        "d _ t: both sides should hold"
    );

    // "tad": preceded by t (not d), followed by d (not t) -- both sides fail.
    let tad = probe_shape(&g, "tad");
    assert!(
        !environments_ok(&g, &envs, &tad, 1, 1),
        "t _ d: neither side should hold"
    );

    // "dad": preceded by d (holds), followed by d not t (fails) -- AND semantics, overall false.
    let dad = probe_shape(&g, "dad");
    assert!(
        !environments_ok(&g, &envs, &dad, 1, 1),
        "d _ d: right side fails, so overall false"
    );

    // "tat": preceded by t not d (fails), followed by t (holds) -- overall false.
    let tat = probe_shape(&g, "tat");
    assert!(
        !environments_ok(&g, &envs, &tat, 1, 1),
        "t _ t: left side fails, so overall false"
    );
}

/// A left-only environment anchored at the word start: on "at" the target morph has no left context at all, so a real left pattern must fail; on "dat" the left context ("d") satisfies it.
#[test]
fn environments_ok_left_only_requires_real_preceding_context() {
    let g = load_probe_grammar();
    let envs = vec![env(true, Some(single(&g, "nc_d")), None)];

    let at = probe_shape(&g, "at");
    assert!(
        !environments_ok(&g, &envs, &at, 0, 0),
        "no left context at all -- cannot match nc_d"
    );

    let dat = probe_shape(&g, "dat");
    assert!(environments_ok(&g, &envs, &dat, 1, 1), "preceded by d");
}

/// `EnvironmentDef.require == false` inverts the match: the environment is satisfied when the pattern does *not* hold.
#[test]
fn environments_ok_exclude_type_inverts_the_match() {
    let g = load_probe_grammar();
    // Excluded: "must NOT be followed by t".
    let envs = vec![env(false, None, Some(single(&g, "nc_t")))];

    let dat = probe_shape(&g, "dat"); // "a" (index 1) IS followed by t -- excluded pattern matches,
    assert!(
        !environments_ok(&g, &envs, &dat, 1, 1),
        "excluded env matched -> word invalid"
    );

    let dad = probe_shape(&g, "dad"); // "a" (index 1) is followed by d, not t -- exclusion holds.
    assert!(
        environments_ok(&g, &envs, &dad, 1, 1),
        "excluded env did not match -> word valid"
    );
}

/// No declared environments is vacuously valid (`Environments.Count > 0` guard, Allomorph.cs:112).
#[test]
fn environments_ok_vacuously_true_when_no_environments_declared() {
    let g = load_probe_grammar();
    let dat = probe_shape(&g, "dat");
    assert!(environments_ok(&g, &[], &dat, 1, 1));
}

/// A morph carrying two environment entries validates when at least one is satisfied (OR across entries, not AND).
#[test]
fn environments_ok_two_entries_use_or_semantics() {
    let g = load_probe_grammar();
    // Two REQUIRED environments on one allomorph: "preceded by d" OR "preceded by t".
    let envs = vec![
        env(true, Some(single(&g, "nc_d")), None),
        env(true, Some(single(&g, "nc_t")), None),
    ];

    // "dat": entry 1 holds, entry 2 fails; exactly one satisfied must still pass (an AND regression fails here).
    let dat = probe_shape(&g, "dat");
    assert!(
        environments_ok(&g, &envs, &dat, 1, 1),
        "one of two environments holds -> valid"
    );

    // "tad": exactly the other entry holds, pinning that the OR is over the whole list, not a first-entry short-circuit.
    let tad = probe_shape(&g, "tad");
    assert!(
        environments_ok(&g, &envs, &tad, 1, 1),
        "the other of two environments holds -> valid"
    );

    // "aat": featurally disjoint from both d and t, so neither entry holds.
    let aat = probe_shape(&g, "aat");
    assert!(
        !environments_ok(&g, &envs, &aat, 1, 1),
        "neither environment holds -> invalid"
    );
}

// Part 2 -- `allomorphs_valid`: bound roots + required syntactic FS, over a real loaded grammar.

/// Three lexical entries -- plain `sg` root "cat", plain `pl` root "dog", bound `pl` root "bat" -- and a suffix rule requiring `num=pl`, isolating the bound-root check from the required-FS check.
const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>ValidityGate</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <HeadFeatures>
      <SymbolicFeature id="featNum">
        <Name>num</Name>
        <Symbols>
          <Symbol id="symSg">sg</Symbol>
          <Symbol id="symPl">pl</Symbol>
        </Symbols>
      </SymbolicFeature>
    </HeadFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cC"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cO"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cX"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll">
        <Name>All</Name>
        <Segment segment="cA" /><Segment segment="cB" /><Segment segment="cC" /><Segment segment="cD" />
        <Segment segment="cG" /><Segment segment="cO" /><Segment segment="cT" /><Segment segment="cX" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRules="mrSuffix">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrSuffix" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>-x</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subX">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
                <RequiredHeadFeatures>
                  <FeatureValue feature="featNum" symbolValues="symPl" />
                </RequiredHeadFeatures>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eCat" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aCat"><PhoneticShape>cat</PhoneticShape></Allomorph></Allomorphs>
            <AssignedHeadFeatures><FeatureValue feature="featNum" symbolValues="symSg" /></AssignedHeadFeatures>
          </LexicalEntry>
          <LexicalEntry id="eDog" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aDog"><PhoneticShape>dog</PhoneticShape></Allomorph></Allomorphs>
            <AssignedHeadFeatures><FeatureValue feature="featNum" symbolValues="symPl" /></AssignedHeadFeatures>
          </LexicalEntry>
          <LexicalEntry id="eBat" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aBat" isBound="true"><PhoneticShape>bat</PhoneticShape></Allomorph></Allomorphs>
            <AssignedHeadFeatures><FeatureValue feature="featNum" symbolValues="symPl" /></AssignedHeadFeatures>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn load_gate_grammar() -> Grammar {
    pg_grammar::load(XML).expect("validity-gate grammar loads")
}

fn entry_shape(g: &Grammar, text: &str) -> Shape {
    let t = &g.char_tables[0];
    let seg = pg_grammar_model::segment::segment(t, text).expect("segments");
    let mut b = ShapeBuilder::with_features_capacity(0, seg.len());
    for (_, kind, cd, _) in seg.interior() {
        match kind {
            NodeKind::Segment => b.push_segment_with_lanes(cd, &[]),
            NodeKind::Boundary => b.push_boundary_with_lanes(cd, &[]),
            _ => {}
        }
    }
    b.finish()
}

/// Find the lexical entry whose first allomorph's authored surface text is `text`.
fn find_entry<'g>(g: &'g Grammar, text: &str) -> &'g pg_grammar_model::model::LexEntryDef {
    g.entries
        .iter()
        .find(|e| e.allomorphs.first().map(|a| a.shape.text.as_str()) == Some(text))
        .unwrap_or_else(|| panic!("no entry with surface {text:?}"))
}

fn suffix_allomorph(g: &Grammar) -> (AllomorphId, pg_grammar_model::model::MorphemeId) {
    let MorphRuleDef::AffixProcess(def) = &g.mrules[0] else {
        panic!("expected affix rule")
    };
    (def.allomorphs[0].id, def.morpheme)
}

/// Bare bound root ("bat" alone, no affix): must be rejected.
#[test]
fn bound_root_alone_is_rejected() {
    let g = load_gate_grammar();
    let bat = find_entry(&g, "bat");
    let root_allo = bat.allomorphs[0].id;
    assert!(
        bat.allomorphs[0].is_bound,
        "sanity: eBat's allomorph is isBound=\"true\""
    );

    let mut w = Word::new(
        entry_shape(&g, "bat"),
        pg_grammar_model::model::StratumId(0),
    );
    w.syn_fs = g.fs_interner.get(bat.syn_fs).clone();
    w.morphs = vec![MorphRecord::new(root_allo, bat.morpheme, 0)];

    assert!(
        !allomorphs_valid(&g, &w),
        "a bound root as the word's only allomorph must be rejected"
    );
}

/// The same bound root combined with the suffix is no longer rejected by the bound-root gate.
#[test]
fn bound_root_with_an_affix_is_not_rejected_by_the_bound_gate() {
    let g = load_gate_grammar();
    let bat = find_entry(&g, "bat");
    let root_allo = bat.allomorphs[0].id;
    let (affix_allo, affix_morpheme) = suffix_allomorph(&g);

    let mut w = Word::new(
        entry_shape(&g, "batx"),
        pg_grammar_model::model::StratumId(0),
    );
    w.syn_fs = g.fs_interner.get(bat.syn_fs).clone();
    w.morphs = vec![
        MorphRecord::new(root_allo, bat.morpheme, 0),
        MorphRecord::new(affix_allo, affix_morpheme, 3),
    ];

    assert!(
        allomorphs_valid(&g, &w),
        "bound root + affix (distinct_count=2) must not be rejected"
    );
}

/// The required syntactic FS is re-checked against the word's accumulated syn FS: "cat" (sg) + "-x" must be rejected, "dog" (pl) + "-x" must pass.
#[test]
fn required_syntactic_fs_gates_on_the_words_accumulated_syn_fs() {
    let g = load_gate_grammar();
    let (affix_allo, affix_morpheme) = suffix_allomorph(&g);

    let cat = find_entry(&g, "cat");
    let mut w_cat = Word::new(
        entry_shape(&g, "catx"),
        pg_grammar_model::model::StratumId(0),
    );
    w_cat.syn_fs = g.fs_interner.get(cat.syn_fs).clone();
    w_cat.morphs = vec![
        MorphRecord::new(cat.allomorphs[0].id, cat.morpheme, 0),
        MorphRecord::new(affix_allo, affix_morpheme, 3),
    ];
    assert!(
        !allomorphs_valid(&g, &w_cat),
        "cat is num=sg; the suffix requires num=pl -- must reject"
    );
    let cache = RuleCache::build(&g);
    let sink = TreeTraceSink::with_failure_context();
    let root = sink.analyze_word(&w_cat);
    assert!(!allomorphs_valid_cached_traced(
        &g, &w_cat, &cache, &sink, root
    ));
    let failure = sink
        .node(root)
        .children
        .into_iter()
        .map(|handle| sink.node(handle))
        .find(|node| node.failure_reason == Some(FailureReason::RequiredSyntacticFeatureStruct))
        .unwrap();
    let Some(RejectionEvidence::SyntacticFeatures {
        object,
        required,
        actual,
    }) = failure.failure_context.unwrap().evidence
    else {
        panic!("the rejecting allomorph must publish its actual feature operands");
    };
    assert_eq!(object, pg_rules::trace::TraceObject::Allomorph(affix_allo));
    assert_eq!(actual, w_cat.syn_fs);
    assert_ne!(required, actual);

    let dog = find_entry(&g, "dog");
    let mut w_dog = Word::new(
        entry_shape(&g, "dogx"),
        pg_grammar_model::model::StratumId(0),
    );
    w_dog.syn_fs = g.fs_interner.get(dog.syn_fs).clone();
    w_dog.morphs = vec![
        MorphRecord::new(dog.allomorphs[0].id, dog.morpheme, 0),
        MorphRecord::new(affix_allo, affix_morpheme, 3),
    ];
    assert!(
        allomorphs_valid(&g, &w_dog),
        "dog is num=pl; the suffix's requirement is satisfied"
    );
}

#[test]
fn rich_environment_rejection_records_only_the_failed_alternatives() {
    let mut g = load_gate_grammar();
    let cat = find_entry(&g, "cat");
    let cat_index = g
        .entries
        .iter()
        .position(|entry| entry.authored_id == cat.authored_id)
        .expect("cat entry index");
    let cat_allo = cat.allomorphs[0].id;
    let cat_morpheme = cat.morpheme;
    let environments = vec![
        EnvironmentDef {
            source: Some(EnvironmentSource {
                id: Some("env-first".into()),
                text: Some("/[All]_".into()),
            }),
            ..env(true, Some(single(&g, "ncAll")), None)
        },
        EnvironmentDef {
            source: Some(EnvironmentSource {
                id: Some("env-second".into()),
                text: Some("/[All]_".into()),
            }),
            ..env(true, Some(single(&g, "ncAll")), None)
        },
    ];
    g.entries[cat_index].allomorphs[0].environments = environments;
    let mut w = Word::new(
        entry_shape(&g, "cat"),
        pg_grammar_model::model::StratumId(0),
    );
    w.syn_fs = g.fs_interner.get(g.entries[cat_index].syn_fs).clone();
    w.morphs = vec![MorphRecord::new(cat_allo, cat_morpheme, 0)];

    let expected = allomorphs_valid(&g, &w);
    let cache = RuleCache::build(&g);
    let sink = TreeTraceSink::with_failure_context();
    let root = sink.analyze_word(&w);
    let traced = allomorphs_valid_cached_traced(&g, &w, &cache, &sink, root);
    assert_eq!(traced, expected);
    assert!(!traced);

    let failure = sink
        .node(root)
        .children
        .into_iter()
        .map(|handle| sink.node(handle))
        .find(|node| {
            node.type_ == TraceType::Failed
                && node.failure_reason == Some(FailureReason::Environments)
        })
        .expect("environment gate emits its rejection");
    let context = failure.failure_context.expect("rich evidence is enabled");
    assert!(
        context.environment.is_some(),
        "legacy display field remains populated"
    );
    let Some(RejectionEvidence::Environments {
        object,
        constraint_owner,
        alternatives,
        ..
    }) = context.evidence
    else {
        panic!("expected typed environment evidence");
    };
    assert_eq!(object, pg_rules::trace::TraceObject::Allomorph(cat_allo));
    assert_eq!(constraint_owner, object);
    assert_eq!(alternatives.len(), 2);
    assert!(alternatives.iter().all(|result| !result.accepted));
    assert_eq!(alternatives[0].source_id.as_deref(), Some("env-first"));
    assert_eq!(alternatives[1].source_id.as_deref(), Some("env-second"));
}

#[test]
fn rich_co_occurrence_rejection_records_the_first_failed_rule_operands() {
    let mut g = load_gate_grammar();
    let cat_index = g
        .entries
        .iter()
        .position(|entry| {
            entry
                .allomorphs
                .first()
                .is_some_and(|a| a.shape.text == "cat")
        })
        .expect("cat entry index");
    let cat_allo = g.entries[cat_index].allomorphs[0].id;
    let cat_morpheme = g.entries[cat_index].morpheme;
    let absent_other = AllomorphId(123_456);
    g.entries[cat_index].allomorphs[0].co_occurrence = vec![AllomorphCoOccurrenceRuleDef {
        require: true,
        others: vec![absent_other],
        adjacency: CoOccurrenceAdjacency::Anywhere,
    }];
    let mut w = Word::new(
        entry_shape(&g, "cat"),
        pg_grammar_model::model::StratumId(0),
    );
    w.syn_fs = g.fs_interner.get(g.entries[cat_index].syn_fs).clone();
    w.morphs = vec![MorphRecord::new(cat_allo, cat_morpheme, 0)];

    let expected = allomorphs_valid(&g, &w);
    let cache = RuleCache::build(&g);
    let sink = TreeTraceSink::with_failure_context();
    let root = sink.analyze_word(&w);
    let traced = allomorphs_valid_cached_traced(&g, &w, &cache, &sink, root);
    assert_eq!(traced, expected);
    assert!(!traced);

    let failure = sink
        .node(root)
        .children
        .into_iter()
        .map(|handle| sink.node(handle))
        .find(|node| {
            node.type_ == TraceType::Failed
                && node.failure_reason == Some(FailureReason::AllomorphCoOccurrenceRules)
        })
        .expect("co-occurrence gate emits its rejection");
    let evidence = failure
        .failure_context
        .expect("rich evidence is enabled")
        .evidence
        .expect("co-occurrence operands are captured");
    let RejectionEvidence::CoOccurrence {
        object,
        constraint_owner,
        rule_index,
        require,
        others,
        actual,
        ..
    } = evidence
    else {
        panic!("expected typed co-occurrence evidence");
    };
    assert_eq!(object, pg_rules::trace::TraceObject::Allomorph(cat_allo));
    assert_eq!(constraint_owner, object);
    assert_eq!(rule_index, 0);
    assert!(require);
    assert_eq!(
        others,
        vec![pg_rules::trace::TraceObject::Allomorph(absent_other)]
    );
    assert_eq!(
        actual,
        vec![pg_rules::trace::TraceObject::Allomorph(cat_allo)]
    );
}
