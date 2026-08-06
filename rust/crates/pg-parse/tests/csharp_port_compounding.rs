//! Ports `CompoundingRuleTests` (`CompoundingRuleTests.cs`) plus `ProdRestrictRule`.
//! Divergences found while porting: docs/research/csharp-port-compounding-divergences.md.

mod csharp_port_common;
use csharp_port_common::{assert_empty, assert_morphs_eq, build_grammar};
use pg_parse::Morpher;
use std::collections::BTreeSet;

fn root_gloss_set(outcome: &pg_parse::ParseOutcome) -> BTreeSet<String> {
    // AssertRootAllomorphsEquals (CompoundingRuleTests.cs:240-243): the surviving analyses' distinct root-morpheme gloss set.
    outcome
        .analyses
        .iter()
        .map(|(m, _)| m.split('+').next().unwrap_or(m).to_string())
        .collect()
}

const SIMPLE_RULES_MRULES_1: &str = r#"
  <CompoundingRule id="mrC">
    <Name>rule1</Name>
    <CompoundingSubrules><CompoundingSubrule>
      <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
      <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
      <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
    </CompoundingSubrule></CompoundingSubrules>
  </CompoundingRule>
"#;

/// Ports `SimpleRules` reconfiguration 1 (cs:13-29), head+"+"+nonHead order, negative cases only; positive case is `simple_rules_1_homophone_disjunction_finding`.
#[test]
fn simple_rules_1_negative_cases() {
    let g1 = build_grammar("", "", SIMPLE_RULES_MRULES_1, "mrC", "");
    let m1 = Morpher::new(&g1, usize::MAX);
    assert_empty(&m1.parse_word("pʰutdas"));
    assert_empty(&m1.parse_word("pʰusdat"));
}

/// Fixes the homophone-disjunction collapse (`Word::non_heads.pop()` erased history a dedup key needed) and a related `current_non_head()` bug.
/// Full root-cause trace: docs/research/csharp-port-compounding-divergences.md.
#[test]
fn simple_rules_1_homophone_disjunction_finding() {
    let g1 = build_grammar("", "", SIMPLE_RULES_MRULES_1, "mrC", "");
    let m1 = Morpher::new(&g1, usize::MAX);
    let out1 = m1.parse_word("pʰutdat");
    assert_morphs_eq(&out1, &["5 8", "5 9"]);
    assert_eq!(root_gloss_set(&out1), BTreeSet::from(["5".to_string()]));
}

/// Ports `SimpleRules` reconfiguration 2 (cs:31-46): nonHead+"+"+head order; C# reuses reconfiguration 1's positive case, so only the negative checks are ported here.
#[test]
fn simple_rules_2_negative_cases() {
    let mrules2 = r#"
      <CompoundingRule id="mrC">
        <Name>rule1</Name>
        <CompoundingSubrules><CompoundingSubrule>
          <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
          <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="nonHead" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="head" /></MorphologicalOutput>
        </CompoundingSubrule></CompoundingSubrules>
      </CompoundingRule>
    "#;
    let g2 = build_grammar("", "", mrules2, "mrC", "");
    let m2 = Morpher::new(&g2, usize::MAX);
    assert_empty(&m2.parse_word("pʰutdas"));
    assert_empty(&m2.parse_word("pʰusdat"));
}

/// Ports `SimpleRules` reconfiguration 3 (cs:48-71), a V-requiring PAST prefix commuting with compounding; corrects an earlier "recursive non-head analysis" misdiagnosis (a mis-ported reconfiguration, not an engine gap) and asserts via `root_morpheme_index` since the head is the LAST morph here.
/// Full trace: docs/research/csharp-port-compounding-divergences.md.
#[test]
fn simple_rules_3_prefix_commutes_with_compounding() {
    let mrules3 = r#"
      <CompoundingRule id="mrC">
        <Name>rule1</Name>
        <CompoundingSubrules><CompoundingSubrule>
          <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
          <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="nonHead" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="head" /></MorphologicalOutput>
        </CompoundingSubrule></CompoundingSubrules>
      </CompoundingRule>
      <MorphologicalRule id="mrPrefix" requiredPartsOfSpeech="posV"><Name>prefix</Name><MorphemeId>PAST</MorphemeId>
        <OutputHeadFeatures><FeatureValue feature="featTense" symbolValues="symPast" /></OutputHeadFeatures>
        <MorphologicalSubrules><MorphologicalSubrule id="subPrefix">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><InsertSegments><PhoneticShape>di+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    // C# rule order: prefix inserted at index 0 (cs:59), rule1 already present -> "mrPrefix mrC".
    let g3 = build_grammar("", "", mrules3, "mrPrefix mrC", "");
    let m3 = Morpher::new(&g3, usize::MAX);
    let out3 = m3.parse_word("pʰutdidat");
    assert_morphs_eq(&out3, &["5 PAST 9"]);
    // AssertRootAllomorphsEquals(output, "9"): every analysis' root morpheme is entry "9" (dat, V).
    let root9 = csharp_port_common::morpheme_ordinal(&g3, "9");
    for wa in &out3.structured {
        assert_eq!(
            wa.morpheme_ids[wa.root_morpheme_index as usize], root9,
            "root must be entry 9"
        );
    }

    // Parity pin for the OLD (mis-ported) head+nonHead grammar: neither engine recurses into the cascade, so both return empty for "pʰutdidat".
    let mrules3_head_first = r#"
      <CompoundingRule id="mrC">
        <Name>rule1</Name>
        <CompoundingSubrules><CompoundingSubrule>
          <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
          <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
        </CompoundingSubrule></CompoundingSubrules>
      </CompoundingRule>
      <MorphologicalRule id="mrPrefix" requiredPartsOfSpeech="posV"><Name>prefix</Name><MorphemeId>PAST</MorphemeId>
        <OutputHeadFeatures><FeatureValue feature="featTense" symbolValues="symPast" /></OutputHeadFeatures>
        <MorphologicalSubrules><MorphologicalSubrule id="subPrefix">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><InsertSegments><PhoneticShape>di+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g_head_first = build_grammar("", "", mrules3_head_first, "mrPrefix mrC", "");
    let m_head_first = Morpher::new(&g_head_first, usize::MAX);
    assert_empty(&m_head_first.parse_word("pʰutdidat"));
}

/// Ports `MorphosyntacticRules` (cs:110-172): the first 2 of 3 reconfigurations plus the final percolation reconfiguration over the `Perc0`/`Perc3` homophones.
#[test]
fn morphosyntactic_rules() {
    let mrules1 = r#"
      <CompoundingRule id="mrC" nonHeadPartsOfSpeech="posV">
        <Name>rule1</Name>
        <CompoundingSubrules><CompoundingSubrule>
          <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
          <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
        </CompoundingSubrule></CompoundingSubrules>
      </CompoundingRule>
    "#;
    let g1 = build_grammar("", "", mrules1, "mrC", "");
    let m1 = Morpher::new(&g1, usize::MAX);
    let out1 = m1.parse_word("pʰutdat");
    assert_morphs_eq(&out1, &["5 9"]); // only entry "9" (dat, V) satisfies nonHead=V; "8" (dat, N) doesn't
    assert_eq!(root_gloss_set(&out1), BTreeSet::from(["5".to_string()]));
    assert_empty(&m1.parse_word("pʰutbupu")); // "bupu" (46, N) is not a V nonhead

    let g3 = build_grammar(
        "",
        "",
        r#"<CompoundingRule id="mrC" headPartsOfSpeech="posV">
             <Name>rule1</Name>
             <HeadRequiredHeadFeatures><FeatureValue feature="featPers" symbolValues="symP2" /></HeadRequiredHeadFeatures>
             <CompoundingSubrules><CompoundingSubrule>
               <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
               <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
               <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
             </CompoundingSubrule></CompoundingSubrules>
           </CompoundingRule>"#,
        "mrC",
        "",
    );
    let m3 = Morpher::new(&g3, usize::MAX);
    // "ssagabba": head V+pers=2 unifies with both Perc0 (unspecified) and Perc3 ({2,3}); non-head unrestricted admits both dat/V homophones.
    let out3 = m3.parse_word("ssagabba");
    assert_morphs_eq(&out3, &["Perc0 39", "Perc0 40", "Perc3 39", "Perc3 40"]);
    assert_eq!(
        root_gloss_set(&out3),
        BTreeSet::from(["Perc0".to_string(), "Perc3".to_string()])
    );
}

// CompoundingRuleTests.ProdRestrictRule: its productivity-restriction MPR gates were already implemented (pg-rules/src/morph.rs), so this port is pure test-writing.

/// One `ProdRestrictRule` configuration: C#'s per-step `MprFeatures` mutations become per-configuration attributes on a fresh grammar over the same three entries (`5`/`8`/`9`), since `mprLatinate`'s identity is never asserted, only its presence per gate.
fn prod_restrict_grammar(rule_mpr_attrs: &str, e5_attrs: &str, e8_attrs: &str) -> Morpher<'static> {
    let mrule = format!(
        r#"<CompoundingRule id="mrC1" {rule_mpr_attrs}><Name>rule1</Name>
             <CompoundingSubrules><CompoundingSubrule>
               <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
               <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
               <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
             </CompoundingSubrule></CompoundingSubrules>
           </CompoundingRule>"#
    );
    let lexicon = format!(
        r#"
      <LexicalEntry id="e5" partOfSpeech="posN" {e5_attrs}><MorphemeId>5</MorphemeId>
        <Allomorphs><Allomorph id="a5"><PhoneticShape>pʰut</PhoneticShape></Allomorph></Allomorphs>
      </LexicalEntry>
      <LexicalEntry id="e8" partOfSpeech="posN" {e8_attrs}><MorphemeId>8</MorphemeId>
        <Allomorphs><Allomorph id="a8"><PhoneticShape>dat</PhoneticShape></Allomorph></Allomorphs>
      </LexicalEntry>
      <LexicalEntry id="e9" partOfSpeech="posV"><MorphemeId>9</MorphemeId>
        <Allomorphs><Allomorph id="a9"><PhoneticShape>dat</PhoneticShape></Allomorph></Allomorphs>
      </LexicalEntry>
    "#
    );
    // Leaked because `Morpher` borrows it; bounded (six configurations per run) and irrelevant for a test binary.
    let g = Box::leak(Box::new(csharp_port_common::build_grammar_custom_lexicon(
        &mrule, "mrC1", &lexicon,
    )));
    Morpher::new(g, usize::MAX)
}

/// Ports `ProdRestrictRule` (cs:174-238): six sequential C# reconfigurations of one grammar become six grammars, each step's entry-side `MprFeatures` carried over exactly as the C# mutations leave it.
/// Step-by-step rationale: docs/research/csharp-port-compounding-divergences.md.
#[test]
fn prod_restrict_rule() {
    let m1 = prod_restrict_grammar("", "", "");
    let out1 = m1.parse_word("pʰutdat");
    assert_morphs_eq(&out1, &["5 8", "5 9"]);
    assert_eq!(root_gloss_set(&out1), BTreeSet::from(["5".to_string()]));

    let m2 = prod_restrict_grammar(r#"headProdRestrictionsMprFeatures="mprLatinate""#, "", "");
    assert_empty(&m2.parse_word("pʰutdat"));

    let m3 = prod_restrict_grammar(
        r#"headProdRestrictionsMprFeatures="mprLatinate""#,
        r#"ruleFeatures="mprLatinate""#,
        "",
    );
    let out3 = m3.parse_word("pʰutdat");
    assert_morphs_eq(&out3, &["5 8", "5 9"]);
    assert_eq!(root_gloss_set(&out3), BTreeSet::from(["5".to_string()]));

    let m4 = prod_restrict_grammar(
        r#"nonHeadProdRestrictionsMprFeatures="mprLatinate""#,
        r#"ruleFeatures="mprLatinate""#,
        "",
    );
    assert_empty(&m4.parse_word("pʰutdat"));

    let m5 = prod_restrict_grammar(
        r#"nonHeadProdRestrictionsMprFeatures="mprLatinate""#,
        "",
        r#"ruleFeatures="mprLatinate""#,
    );
    let out5 = m5.parse_word("pʰutdat");
    assert_morphs_eq(&out5, &["5 8"]);
    assert_eq!(root_gloss_set(&out5), BTreeSet::from(["5".to_string()]));

    let m6 = prod_restrict_grammar(
        r#"nonHeadProdRestrictionsMprFeatures="mprLatinate" outputProdRestrictionsMprFeatures="mprLatinate""#,
        "",
        r#"ruleFeatures="mprLatinate""#,
    );
    let out6 = m6.parse_word("pʰutdat");
    assert_morphs_eq(&out6, &["5 8"]);
    assert_eq!(root_gloss_set(&out6), BTreeSet::from(["5".to_string()]));
}

// `Morpher::with_max_stem_count` (previously a hardcoded `2`, no public knob) enables `SimpleRules`' final reconfiguration (cs:76-108); see docs/research/csharp-port-compounding-divergences.md.

const RULE1_HEAD_NONHEAD_MAX_APP_2: &str = r#"
  <CompoundingRule id="mrC" multipleApplication="2">
    <Name>rule1</Name>
    <CompoundingSubrules><CompoundingSubrule>
      <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
      <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
      <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
    </CompoundingSubrule></CompoundingSubrules>
  </CompoundingRule>
"#;

/// Ports `SimpleRules` cs:76-90: a self-recursive rule (`MaxApplicationCount = 2`) over a 3-root word, exercising the `MaxStemCount(3)` depth gate that lets a second unapplication through.
#[test]
fn simple_rules_4_three_root_compound_single_rule() {
    let g = build_grammar("", "", RULE1_HEAD_NONHEAD_MAX_APP_2, "mrC", "");
    let m = Morpher::new(&g, usize::MAX).with_max_stem_count(3);
    assert_empty(&Morpher::new(&g, usize::MAX).parse_word("pʰutdatpip")); // default (2) still refuses -- unaffected
    let out = m.parse_word("pʰutdatpip");
    assert_morphs_eq(&out, &["5 8 41", "5 9 41"]);
    assert_eq!(root_gloss_set(&out), BTreeSet::from(["5".to_string()]));
}

const TWO_RULES_HEAD_NONHEAD_AND_NONHEAD_HEAD: &str = r#"
  <CompoundingRule id="mrC">
    <Name>rule1</Name>
    <CompoundingSubrules><CompoundingSubrule>
      <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
      <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
      <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
    </CompoundingSubrule></CompoundingSubrules>
  </CompoundingRule>
  <CompoundingRule id="mrC2">
    <Name>rule2</Name>
    <CompoundingSubrules><CompoundingSubrule>
      <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
      <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
      <MorphologicalOutput><CopyFromInput index="nonHead" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="head" /></MorphologicalOutput>
    </CompoundingSubrule></CompoundingSubrules>
  </CompoundingRule>
"#;

/// Ports `SimpleRules` cs:92-108: two capped rules supply the two splits instead of one rule re-entering itself; asserts via `root_morpheme_index` since the root is the inner split.
/// Assertion semantics (set-membership, not count): docs/research/csharp-port-compounding-divergences.md.
#[test]
fn simple_rules_5_three_root_compound_two_rules() {
    let g = build_grammar(
        "",
        "",
        TWO_RULES_HEAD_NONHEAD_AND_NONHEAD_HEAD,
        "mrC mrC2",
        "",
    );
    let m = Morpher::new(&g, usize::MAX).with_max_stem_count(3);
    let out = m.parse_word("pʰutdatpip");
    assert_morphs_eq(&out, &["5 8 41", "5 9 41"]);

    let root8 = csharp_port_common::morpheme_ordinal(&g, "8");
    let root9 = csharp_port_common::morpheme_ordinal(&g, "9");
    assert!(
        !out.structured.is_empty(),
        "expected at least one surviving analysis"
    );
    for wa in &out.structured {
        let root = wa.morpheme_ids[wa.root_morpheme_index as usize];
        assert!(
            root == root8 || root == root9,
            "root must be entry 8 or 9 (dat), got ordinal {root}"
        );
    }
}
