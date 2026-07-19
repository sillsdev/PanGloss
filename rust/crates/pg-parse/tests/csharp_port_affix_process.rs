//! Ports `AffixProcessRuleTests` (parse-opt: `tests/SIL.Machine.Morphology.HermitCrab.Tests/
//! MorphologicalRules/AffixProcessRuleTests.cs`) bucket-B/C rows per
//! `rust/parity-out/audit/phase2/D-test-coverage-map.md` §3.
//!
//! `RequiredEnvironments`/`RequiredSyntacticFeatureStruct`/`FreeFluctuation` (already bucket A via
//! `pg-rules/tests/validity_gate.rs`/`redup_and_free_fluctuation_gate.rs`) are out of scope.
//!
//! **Update (W11 batch-4):** `InfixRules`/`CircumfixRules`/`TruncateRules`/`NonContiguousRules` (was
//! bucket D, "needs the W9.1 probe") are ported below (`infix_rules`, `circumfix_rules`,
//! `truncate_rules`, `non_contiguous_rules`) -- the probe found the general affix machinery already
//! matches C# for all four (verbatim ports, zero engine changes), needing only a missing fixture
//! lexical entry ("49"="ktb") and a handful of natural classes `csharp_port_common` didn't happen to
//! need yet.
//!
//! Deliberate scope reductions from the full C# test bodies (each noted at its test, not silently):
//! - `MorphosyntacticRules`: ports the 4 non-`LexFamily` reconfigurations only. The 5th reconfiguration
//!   (`sSuffix` applied to root "si", part of the `SEE` `LexFamily`) needs `LexFamily`/
//!   `ChooseInflectionalStem`, confirmed absent (D-batch-3), so it is not portable.
//! - `PercolationRules`: ports the first 2 of 7 reconfigurations (L effort; these two already exercise
//!   the percolation-with-disjunctive-pers-values mechanism the test is about).
//! - `SuffixRules`/`PrefixRules`: port the phonologically-conditioned disjunctive-allomorph scenario;
//!   the alpha-variable-allomorph reconfiguration (needs alpha variables scoped to a *morphological*
//!   rule, not attempted here) is omitted. **Update (W11 batch-7 remainder):** `SuffixRules`'s 5
//!   `GenerateWords` round-trip assertions (AffixProcessRuleTests.cs:418-437) are now ported too --
//!   `Morpher::generate_words`/`WordAnalysis` (D-batch-7/W7) landed since the note above was written,
//!   via the shared `lex_entry_id`/`mrule_id` lookups in `csharp_port_common` (originally written for
//!   `csharp_port_generation.rs`). `PrefixRules` has no `GenerateWords` calls in its C# body, so
//!   nothing further to port there.
//! - `ReduplicationRules`: ports the first 2 of 6 reconfigurations (M effort; basic CV-reduplication +
//!   its interaction with an intervening voicing rule, the specific gap the coverage map names).
//!
//! Two engine findings surfaced while porting. **FIXED (plan item 1 / wave-3)**:
//! `simulfix_rules`/`modify_from_input_rules` -- a `ModifyFromInput` output node kept its
//! pre-modification `char_def`/`cd_set` (both the synthesis side, `pg-rules/src/morph.rs::copy_part`,
//! and the analysis-unapply side, `::generate_shape`), so a surface-changing modification never
//! rendered/matched end-to-end. Both now clear to `NO_CHAR_DEF` (synthesis additionally sets an
//! explicit `cd_set` via `ctx_cd_set`, mirroring `OutputAction::InsertContext`'s existing handling;
//! analysis leaves `cd_set` at its already-`Unrestricted` default). Both tests now green and
//! un-ignored. The wave-3 "still open" residual (`subsumed_affix_findings`) is **FIXED (wave-4)**:
//! both halves were morph-ATTRIBUTION drops in `pg_rules::morph::attribute_morphs` (the C#
//! `MarkSubsumedMorph`/`MarkMorph(Shape.First/Last)` fallback branches, ported as
//! `MorphStatus::{Floating, SubsumedChild, SubsumedFirst}`), not an analysis-cascade gap — see that
//! test's doc; now green and un-ignored.

mod csharp_port_common;
use csharp_port_common::{assert_empty, assert_morphs_eq, build_grammar, lex_entry_id, mrule_id};
use pg_featstruct::{FeatureStruct, FeatureStructBuilder, FeatureValue, SymbolBits};
use pg_grammar::model::{Grammar, MorphRuleDef};
use pg_parse::{GenMorpheme, Morpher};
use pg_rules::word::MorphRecord;
use pg_rules::Word;

/// `{pos: symbol}` for the given POS xml id -- the syntactic FS a bare category symbol produces
/// (`FeatureStruct.New(syn).Symbol("N").Value` in C#), built the same way `pg_grammar::load`'s
/// `intern_syn_fs` would.
fn pos_fs(g: &Grammar, xml_id: &str) -> FeatureStruct {
    let idx = g
        .syn_features
        .symbol_index(g.syn_features.pos, xml_id)
        .unwrap();
    let mut b = FeatureStructBuilder::new();
    b.add(
        g.syn_features.pos,
        FeatureValue::Symbolic(SymbolBits::single(idx)),
    );
    b.build()
}

/// Build a bare (feature-less shape, but real `MorphemeId`/`AllomorphId`/`syn_fs`) root [`Word`] for
/// the lexical entry whose `<MorphemeId>` text is `gloss`, the same style
/// `pg-rules/tests/validity_gate.rs` uses to drive `pg_rules::morph::synthesize` directly against a
/// real loaded grammar without needing the full `Morpher` pipeline -- appropriate here because
/// `MorphosyntacticRules`/`PercolationRules` are about the RULE's own syntactic-FS gating/percolation
/// math, not surface-string round-tripping.
fn root_word(g: &pg_grammar::model::Grammar, gloss: &str) -> Word {
    let (entry_idx, entry) = g
        .entries
        .iter()
        .enumerate()
        .find(|(_, e)| g.morphemes[e.morpheme.0 as usize].morph_id.as_deref() == Some(gloss))
        .unwrap_or_else(|| panic!("no entry with MorphemeId {gloss:?}"));
    let allo = &entry.allomorphs[0];
    let shape =
        pg_grammar::segment::segment(&g.char_tables[0], &allo.shape.text).expect("segments");
    let mut w = Word::new(shape, pg_grammar::model::StratumId(0));
    w.syn_fs = g.fs_interner.get(entry.syn_fs).clone();
    w.root_allomorph = Some(allo.id);
    w.morphs = vec![MorphRecord::new(allo.id, entry.morpheme, 0)];
    let _ = entry_idx;
    w
}

/// Ports `AffixProcessRuleTests.MorphosyntacticRules` (AffixProcessRuleTests.cs:12-98), the 4
/// non-`LexFamily` reconfigurations of `s_suffix` applied to root "32" ("sag"). Tests
/// `RequiredSyntacticFeatureStruct`/`OutSyntacticFeatureStruct` toggling directly via
/// `pg_rules::morph::synthesize`, asserting on the output word's `syn_fs`.
#[test]
fn morphosyntactic_rules() {
    let mrule_xml = |gloss: &str, required: &str, output: &str| {
        format!(
            r#"<MorphologicalRule id="mrS" {required} {output}><Name>s_suffix</Name><MorphemeId>{gloss}</MorphemeId>
            <MorphologicalSubrules><MorphologicalSubrule id="subS">
              <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>"#
        )
    };

    // (1) required=V, output=N -> output category N.
    let g1 = build_grammar(
        "",
        "",
        &mrule_xml(
            "NMLZ",
            r#"requiredPartsOfSpeech="posV""#,
            r#"outputPartOfSpeech="posN""#,
        ),
        "mrS",
        "",
    );
    let stem1 = root_word(&g1, "32");
    let MorphRuleDef::AffixProcess(_) = &g1.mrules[0] else {
        panic!()
    };
    let out1 = pg_rules::morph::synthesize(&g1, &stem1, &g1.mrules[0]);
    assert_eq!(out1.len(), 1);
    assert_eq!(
        out1[0].syn_fs,
        pos_fs(&g1, "posN"),
        "required=V/output=N: output category must be N"
    );

    // (2) required=V, output=<empty> -> output category stays V (unchanged).
    let g2 = build_grammar(
        "",
        "",
        &mrule_xml("3.SG", r#"requiredPartsOfSpeech="posV""#, ""),
        "mrS",
        "",
    );
    let stem2 = root_word(&g2, "32");
    let out2 = pg_rules::morph::synthesize(&g2, &stem2, &g2.mrules[0]);
    assert_eq!(out2.len(), 1);
    assert_eq!(
        out2[0].syn_fs,
        pos_fs(&g2, "posV"),
        "required=V/output=<empty>: output category stays V"
    );

    // (3) required=<empty>, output=N -> output category N (root's own V is not required).
    let g3 = build_grammar(
        "",
        "",
        &mrule_xml("NMLZ", "", r#"outputPartOfSpeech="posN""#),
        "mrS",
        "",
    );
    let stem3 = root_word(&g3, "32");
    let out3 = pg_rules::morph::synthesize(&g3, &stem3, &g3.mrules[0]);
    assert_eq!(out3.len(), 1);
    assert_eq!(out3[0].syn_fs, pos_fs(&g3, "posN"));

    // (4) required=<empty>, output=<empty> -> unrestricted rule, category stays V.
    let g4 = build_grammar("", "", &mrule_xml("3.SG", "", ""), "mrS", "");
    let stem4 = root_word(&g4, "32");
    let out4 = pg_rules::morph::synthesize(&g4, &stem4, &g4.mrules[0]);
    assert_eq!(out4.len(), 1);
    assert_eq!(out4[0].syn_fs, pos_fs(&g4, "posV"));
}

/// Ports `AffixProcessRuleTests.PercolationRules` (AffixProcessRuleTests.cs:100-270), the first 2 of 7
/// reconfigurations. `rule1` (gloss "3SG") requires/outputs `pers=2`, applied to all 5 `Perc*` roots
/// (which vary `pers`: unspecified/1/3/{2,3}/{1,3}, all `num=pl`). Only roots whose `pers` overlaps the
/// rule's requirement produce output; the accumulated `syn_fs` percolates `num=pl` from the root and
/// `pers` from the rule's `OutputHeadFeatures`.
#[test]
fn percolation_rules() {
    let mrule_xml = |required_pers: &str| -> String {
        format!(
            r#"<MorphologicalRule id="mrR" requiredPartsOfSpeech="posV"><Name>rule1</Name><MorphemeId>3SG</MorphemeId>
            <RequiredHeadFeatures><FeatureValue feature="featPers" symbolValues="{required_pers}" /></RequiredHeadFeatures>
            <OutputHeadFeatures><FeatureValue feature="featPers" symbolValues="{required_pers}" /></OutputHeadFeatures>
            <MorphologicalSubrules><MorphologicalSubrule id="subR">
              <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>z</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>"#
        )
    };

    // Reconfiguration 1: required/output pers=2. Only Perc0 (unspecified pers, unifies with anything)
    // and Perc3 (pers in {2,3}, overlaps 2) can carry the rule.
    let g1 = build_grammar("", "", &mrule_xml("symP2"), "mrR", "");
    let mut surviving: Vec<&str> = Vec::new();
    for gloss in ["Perc0", "Perc1", "Perc2", "Perc3", "Perc4"] {
        let stem = root_word(&g1, gloss);
        if !pg_rules::morph::synthesize(&g1, &stem, &g1.mrules[0]).is_empty() {
            surviving.push(gloss);
        }
    }
    assert_eq!(
        surviving,
        vec!["Perc0", "Perc3"],
        "reconfig 1 (pers=2): only Perc0/Perc3 survive"
    );

    // Reconfiguration 2: required pers in {2,3} (a disjunctive requirement). Perc0 (unspecified),
    // Perc2 (pers=3), Perc3 (pers in {2,3}), and Perc4 (pers in {1,3}, overlaps 3) all survive;
    // Perc1 (pers=1 only) does not.
    let g2 = build_grammar("", "", &mrule_xml("symP2 symP3"), "mrR", "");
    let mut surviving2: Vec<&str> = Vec::new();
    for gloss in ["Perc0", "Perc1", "Perc2", "Perc3", "Perc4"] {
        let stem = root_word(&g2, gloss);
        if !pg_rules::morph::synthesize(&g2, &stem, &g2.mrules[0]).is_empty() {
            surviving2.push(gloss);
        }
    }
    assert_eq!(
        surviving2,
        vec!["Perc0", "Perc2", "Perc3", "Perc4"],
        "reconfig 2 (pers in {{2,3}}): Perc0/Perc2/Perc3/Perc4 survive, Perc1 does not"
    );
}

/// Ports `AffixProcessRuleTests.SuffixRules` (AffixProcessRuleTests.cs:272-467), the phonologically-
/// disjunctive allomorph-selection scenario plus its 5 `GenerateWords` round-trip assertions
/// (alpha-variable sub-case still excluded -- see file doc). `s_suffix` (3SG) picks `ɯz` after a
/// strident, `s` after a voiceless consonant, else `z`; `ed_suffix` (PAST) picks `+ɯd` after an
/// alveolar stop, `+t` after a voiceless consonant, else `+d` (voiced-alveolar, via
/// `InsertSimpleContext`) -- plus a trailing devoicing/deaspiration rule.
#[test]
fn suffix_rules() {
    let mrules = r#"
      <MorphologicalRule id="mrS" requiredPartsOfSpeech="posV"><Name>s_suffix</Name><MorphemeId>3SG</MorphemeId>
        <OutputHeadFeatures><FeatureValue feature="featPers" symbolValues="symP3" /></OutputHeadFeatures>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subS1">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence><SimpleContext naturalClass="ncStrident" /></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>ɯz</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subS2">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence><SimpleContext naturalClass="ncVlCons" /></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subS3">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>z</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <OutputHeadFeatures><FeatureValue feature="featTense" symbolValues="symPast" /></OutputHeadFeatures>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd1">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
              <PhoneticSequence id="2"><SimpleContext naturalClass="ncAlvStop" /></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><CopyFromInput index="2" /><InsertSegments><PhoneticShape>+ɯd</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subEd2">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence><SimpleContext naturalClass="ncVlCons" /></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+t</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subEd3">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><InsertSimpleContext><SimpleContext naturalClass="ncDLike" /></InsertSimpleContext></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let prules = r#"
      <PhonologicalRule id="pr1"><Name>rule1</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncTSeg" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncUnasp" /></PhoneticSequence></PhoneticOutput>
            <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncC" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    "#;
    let g = build_grammar(prules, "pr1", mrules, "mrS mrEd", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("sagz"), &["32 3SG"]);
    assert_morphs_eq(&m.parse_word("sagd"), &["32 PAST"]);
    assert_morphs_eq(&m.parse_word("sasɯz"), &["33 3SG"]);
    assert_morphs_eq(&m.parse_word("sast"), &["33 PAST"]);
    assert_morphs_eq(&m.parse_word("sazd"), &["34 PAST"]);
    assert_empty(&m.parse_word("sagɯs"));
    assert_empty(&m.parse_word("sags"));
    assert_empty(&m.parse_word("sasz"));
    assert_empty(&m.parse_word("sass"));
    assert_empty(&m.parse_word("satɯs"));
    assert_empty(&m.parse_word("satz"));

    // The 5 `GenerateWords` round-trip assertions (AffixProcessRuleTests.cs:418-437): direct-API
    // synthesis of root + single rule must reproduce exactly the surface forms the `ParseWord`
    // assertions above already established are this rule/root combination's analysis.
    let s_suffix = mrule_id(&g, "3SG");
    let ed_suffix = mrule_id(&g, "PAST");
    assert_eq!(
        m.generate_words(
            lex_entry_id(&g, "32"),
            &[GenMorpheme::Rule(s_suffix)],
            FeatureStruct::EMPTY
        ),
        vec!["sagz".to_string()]
    );
    assert_eq!(
        m.generate_words(
            lex_entry_id(&g, "32"),
            &[GenMorpheme::Rule(ed_suffix)],
            FeatureStruct::EMPTY
        ),
        vec!["sagd".to_string()]
    );
    assert_eq!(
        m.generate_words(
            lex_entry_id(&g, "33"),
            &[GenMorpheme::Rule(s_suffix)],
            FeatureStruct::EMPTY
        ),
        vec!["sasɯz".to_string()]
    );
    assert_eq!(
        m.generate_words(
            lex_entry_id(&g, "33"),
            &[GenMorpheme::Rule(ed_suffix)],
            FeatureStruct::EMPTY
        ),
        vec!["sast".to_string()]
    );
    assert_eq!(
        m.generate_words(
            lex_entry_id(&g, "34"),
            &[GenMorpheme::Rule(ed_suffix)],
            FeatureStruct::EMPTY
        ),
        vec!["sazd".to_string()]
    );
}

/// Ports `AffixProcessRuleTests.PrefixRules` (AffixProcessRuleTests.cs:469-692), the phonologically-
/// disjunctive prefix-allomorph scenario (alpha-variable/poa-variable reconfigurations excluded -- see
/// file doc). `s_prefix` (3SG) picks `zi` before a strident, `s` before a voiceless consonant, else
/// `z`; `ed_prefix` (PAST) picks `di+` before an alveolar stop, `t+` before a voiceless consonant,
/// else `d+`; plus an aspiration-neutralization rule for voiceless stops.
#[test]
fn prefix_rules() {
    let mrules = r#"
      <MorphologicalRule id="mrS" requiredPartsOfSpeech="posV"><Name>s_prefix</Name><MorphemeId>3SG</MorphemeId>
        <OutputHeadFeatures><FeatureValue feature="featPers" symbolValues="symP3" /></OutputHeadFeatures>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subS1">
            <MorphologicalInput><PhoneticSequence id="1"><SimpleContext naturalClass="ncStrident" /><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>zi</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subS2">
            <MorphologicalInput><PhoneticSequence id="1"><SimpleContext naturalClass="ncVlCons" /><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subS3">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>z</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_prefix</Name><MorphemeId>PAST</MorphemeId>
        <OutputHeadFeatures><FeatureValue feature="featTense" symbolValues="symPast" /></OutputHeadFeatures>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd1">
            <MorphologicalInput><PhoneticSequence id="1"><SimpleContext naturalClass="ncAlvStop2" /><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>di+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subEd2">
            <MorphologicalInput><PhoneticSequence id="1"><SimpleContext naturalClass="ncVlCons" /><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>t+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subEd3">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>d+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let prules = r#"
      <PhonologicalRule id="pr_asp"><Name>aspiration</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncVlStop" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule><PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncUnasp" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    "#;
    let g = build_grammar(prules, "pr_asp", mrules, "mrS mrEd", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("zisag"), &["3SG 32"]);
    assert_morphs_eq(&m.parse_word("stag"), &["3SG 47"]);
    assert_morphs_eq(&m.parse_word("zabba"), &["3SG 39", "3SG 40"]);
    assert_morphs_eq(&m.parse_word("ditag"), &["PAST 47"]);
    assert_morphs_eq(&m.parse_word("tpag"), &["PAST 48"]);
    assert_morphs_eq(&m.parse_word("dabba"), &["PAST 39", "PAST 40"]);
    assert_empty(&m.parse_word("zitag"));
    assert_empty(&m.parse_word("sabba"));
    assert_empty(&m.parse_word("ztag"));
    assert_empty(&m.parse_word("disag"));
    assert_empty(&m.parse_word("tabba"));
    assert_empty(&m.parse_word("dtag"));
}

/// Ports `AffixProcessRuleTests.SimulfixRules` (AffixProcessRuleTests.cs:856-965): `ModifyFromInput`
/// combined with `Optional`/`Range` quantifiers on the targeted part.
///
/// **FIXED (plan item 1 / wave-3)**: every sub-case here needs `ModifyFromInput` to change a segment
/// to a *different* character (voicing/rounding a specific target so it renders as a different
/// letter, e.g. "p" -> "b"); before this fix every one produced an EMPTY `Morpher::parse_word`
/// result instead of the expected morphs, even though the underlying *lane*-level modification was
/// already correct (`pg-rules/tests/morph_gate.rs::simulfix_synthesis_voices_target_segment`, which
/// asserts on `node_lanes` directly, never on the rendered surface string). Root cause, traced
/// directly: a `Modify`-produced `OutNode` (`pg-rules/src/morph.rs::copy_part`, the `char_def: src.
/// shape.char_def(p)` / `cd_set: cd_set_of(src.shape, p)` lines) kept the SOURCE node's own
/// `char_def` (e.g. "p") unchanged, and `Shape::node_cd_set` (`pg-shape/src/lib.rs`) treats any node
/// whose `char_def != NO_CHAR_DEF` as an implicit *singleton* of that original char-def, ignoring the
/// stored `cd_set` entirely. `pg_parse::surface::matching_str_reps` therefore restricted a modified
/// segment's renderable representations to its PRE-modification character forever, regardless of how
/// its lanes changed -- so a modified "p" always printed/matched as "p", never "b", and
/// `Morpher::parse_word`'s `IsMatch`-equivalent filter rejected every synthesis candidate whose
/// surface depended on the modification being visible. Fixed exactly as predicted: `Modify`'s
/// `OutNode` now gets `char_def: NO_CHAR_DEF` + a context-derived `cd_set` (`ctx_cd_set`), mirroring
/// `OutputAction::InsertContext`'s handling immediately below it in `synth_affix_allomorph`'s match
/// arm. Un-ignored; green.
#[test]
fn simulfix_rules() {
    // (1) suffix simulfix: copy(any+) + modify(bilabial-nasal-voiceless -> voiced). "pib" -> "41 SIMUL".
    let mrules1 = r#"
      <MorphologicalRule id="mrSim" requiredPartsOfSpeech="posV"><Name>simulfix</Name><MorphemeId>SIMUL</MorphemeId>
        <OutputHeadFeatures><FeatureValue feature="featPers" symbolValues="symP3" /></OutputHeadFeatures>
        <MorphologicalSubrules><MorphologicalSubrule id="sub1">
          <MorphologicalInput>
            <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
            <PhoneticSequence id="2"><SimpleContext naturalClass="ncPLike" /></PhoneticSequence>
          </MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><ModifyFromInput index="2"><SimpleContext naturalClass="ncVoiced" /></ModifyFromInput></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>
    "#;
    let g1 = build_grammar("", "", mrules1, "mrSim", "");
    let m1 = Morpher::new(&g1, usize::MAX);
    assert_morphs_eq(&m1.parse_word("pib"), &["41 SIMUL"]);

    // (2) prefix simulfix: modify(2) + copy(any+). "bip" -> "SIMUL 41".
    let mrules2 = r#"
      <MorphologicalRule id="mrSim" requiredPartsOfSpeech="posV"><Name>simulfix</Name><MorphemeId>SIMUL</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="sub1">
          <MorphologicalInput>
            <PhoneticSequence id="1"><SimpleContext naturalClass="ncPLike" /></PhoneticSequence>
            <PhoneticSequence id="2"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
          </MorphologicalInput>
          <MorphologicalOutput><ModifyFromInput index="1"><SimpleContext naturalClass="ncVoiced" /></ModifyFromInput><CopyFromInput index="2" /></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>
    "#;
    let g2 = build_grammar("", "", mrules2, "mrSim", "");
    let m2 = Morpher::new(&g2, usize::MAX);
    assert_morphs_eq(&m2.parse_word("bip"), &["SIMUL 41"]);

    // (3) optional leading consonant + modify a vowel to nonround. "bɯpu" -> "46 SIMUL".
    let mrules3 = r#"
      <MorphologicalRule id="mrSim" requiredPartsOfSpeech="posN"><Name>simulfix</Name><MorphemeId>SIMUL</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="sub1">
          <MorphologicalInput>
            <PhoneticSequence id="1"><OptionalSegmentSequence min="0" max="1"><SimpleContext naturalClass="ncC" /></OptionalSegmentSequence></PhoneticSequence>
            <PhoneticSequence id="2"><SimpleContext naturalClass="ncV" /></PhoneticSequence>
            <PhoneticSequence id="3"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
          </MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><ModifyFromInput index="2"><SimpleContext naturalClass="ncNonround" /></ModifyFromInput><CopyFromInput index="3" /></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>
    "#;
    let g3 = build_grammar("", "", mrules3, "mrSim", "");
    let m3 = Morpher::new(&g3, usize::MAX);
    assert_morphs_eq(&m3.parse_word("bɯpu"), &["46 SIMUL"]);

    // (4) same, but the vowel part is a Range(1,2) quantifier. "sɯɯpu" -> "50 SIMUL".
    let mrules4 = r#"
      <MorphologicalRule id="mrSim" requiredPartsOfSpeech="posN"><Name>simulfix</Name><MorphemeId>SIMUL</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="sub1">
          <MorphologicalInput>
            <PhoneticSequence id="1"><OptionalSegmentSequence min="0" max="1"><SimpleContext naturalClass="ncC" /></OptionalSegmentSequence></PhoneticSequence>
            <PhoneticSequence id="2"><OptionalSegmentSequence min="1" max="2"><SimpleContext naturalClass="ncV" /></OptionalSegmentSequence></PhoneticSequence>
            <PhoneticSequence id="3"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
          </MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><ModifyFromInput index="2"><SimpleContext naturalClass="ncNonround" /></ModifyFromInput><CopyFromInput index="3" /></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>
    "#;
    let g4 = build_grammar("", "", mrules4, "mrSim", "");
    let m4 = Morpher::new(&g4, usize::MAX);
    assert_morphs_eq(&m4.parse_word("sɯɯpu"), &["50 SIMUL"]);
}

/// `simulfix_rules`' underlying mechanism (voicing a modified segment), asserted at the LANE level
/// only -- this is the part that already works and is the live, non-ignored half of the port,
/// matching `morph_gate.rs::simulfix_synthesis_voices_target_segment`'s own assertion style (which
/// predates this file and never exercised the surface string). Confirms the `ModifyFromInput`
/// mechanism from `SimulfixRules`' first sub-case is not a total loss even though the full
/// `Morpher::parse_word` round-trip is currently broken (see `simulfix_rules`'s finding above).
#[test]
fn simulfix_rules_lane_level_modification_still_works() {
    let mrules1 = r#"
      <MorphologicalRule id="mrSim" requiredPartsOfSpeech="posV"><Name>simulfix</Name><MorphemeId>SIMUL</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="sub1">
          <MorphologicalInput>
            <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
            <PhoneticSequence id="2"><SimpleContext naturalClass="ncPLike" /></PhoneticSequence>
          </MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><ModifyFromInput index="2"><SimpleContext naturalClass="ncVoiced" /></ModifyFromInput></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>
    "#;
    let g1 = build_grammar("", "", mrules1, "mrSim", "");
    let stem = root_word(&g1, "41"); // "pip"
    let out = pg_rules::morph::synthesize(&g1, &stem, &g1.mrules[0]);
    assert_eq!(out.len(), 1, "one synthesis output");
    let voi = g1.phon_features.flat_index("fVd").expect("fVd declared");
    let vp = g1.phon_features.symbol_index(voi, "fVd_p").unwrap();
    let voiced_bits = 1u64 << vp;
    // 5 nodes total (2 boundaries + 3 interior: p,i,p); interior index 3 is the modified final "p".
    assert_eq!(
        out[0].shape.node_lanes(3)[voi.0 as usize],
        voiced_bits,
        "target segment's voi lane is voiced"
    );
}

/// Ports `AffixProcessRuleTests.ReduplicationRules` (AffixProcessRuleTests.cs:967-1156), the first 2
/// of 6 reconfigurations: basic CV-reduplication ("sasag" -> "RED 32"), then its interaction with an
/// intervening voicing rule ("sazag" -> "RED 32", the *underlying* "sasag" resurfaces as "sazag" via
/// intervocalic voicing after reduplication).
#[test]
fn reduplication_rules() {
    let mrules = r#"
      <MorphologicalRule id="mrRed" requiredPartsOfSpeech="posV"><Name>redup</Name><MorphemeId>RED</MorphemeId>
        <OutputHeadFeatures><FeatureValue feature="featPers" symbolValues="symP3" /></OutputHeadFeatures>
        <MorphologicalSubrules><MorphologicalSubrule id="sub1">
          <MorphologicalInput>
            <PhoneticSequence id="1"><SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncV" /></PhoneticSequence>
            <PhoneticSequence id="2"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
          </MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><CopyFromInput index="1" /><CopyFromInput index="2" /></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>
    "#;
    let g1 = build_grammar("", "", mrules, "mrRed", "");
    let m1 = Morpher::new(&g1, usize::MAX);
    assert_morphs_eq(&m1.parse_word("sasag"), &["RED 32"]);

    let prules = r#"
      <PhonologicalRule id="pr_voi"><Name>voicing</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncSSeg" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
            <Environment>
              <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
              <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
            </Environment>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    "#;
    let g2 = build_grammar(prules, "pr_voi", mrules, "mrRed", "");
    let m2 = Morpher::new(&g2, usize::MAX);
    assert_morphs_eq(&m2.parse_word("sazag"), &["RED 32"]);
}

/// Ports `AffixProcessRuleTests.BoundaryRules` (AffixProcessRuleTests.cs:1475-1527): an affix-process
/// `Lhs` explicitly matching a boundary marker (`+`) followed by a consonant then a vowel.
#[test]
fn boundary_rules() {
    let mrules = r#"
      <MorphologicalRule id="mrS" requiredPartsOfSpeech="posV"><Name>s_suffix</Name><MorphemeId>3SG</MorphemeId>
        <OutputHeadFeatures><FeatureValue feature="featPers" symbolValues="symP3" /></OutputHeadFeatures>
        <MorphologicalSubrules><MorphologicalSubrule id="sub1">
          <MorphologicalInput>
            <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
            <PhoneticSequence id="2"><BoundaryMarker boundary="cBnd" /><SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncV" /></PhoneticSequence>
          </MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><CopyFromInput index="2" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrS", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("abbas"), &["39 3SG"]);
}

/// Ports `AffixProcessRuleTests.WordSynthesisWithBoundaryAtBeginning` (AffixProcessRuleTests.cs:
/// 1530-1597), bucket C: a `ht_suffix` inserting `+pa` (a boundary at the very start of the inserted
/// material, then re-copied stem material) followed by `ed_suffix`; asserts the synthesized word's
/// FIRST shape node is a boundary.
#[test]
fn word_synthesis_with_boundary_at_beginning() {
    let mrules = r#"
      <MorphologicalRule id="mrHt" requiredPartsOfSpeech="posV"><Name>ht_suffix</Name><MorphemeId>prefix</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="sub1">
          <MorphologicalInput>
            <PhoneticSequence id="1"><OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
            <PhoneticSequence id="2"><SimpleContext naturalClass="ncC" /></PhoneticSequence>
            <PhoneticSequence id="3"><SimpleContext naturalClass="ncV" /></PhoneticSequence>
            <PhoneticSequence id="4"><SimpleContext naturalClass="ncC" /></PhoneticSequence>
          </MorphologicalInput>
          <MorphologicalOutput>
            <CopyFromInput index="1" /><InsertSegments><PhoneticShape>+pa</PhoneticShape></InsertSegments>
            <CopyFromInput index="2" /><InsertSegments><PhoneticShape>t</PhoneticShape></InsertSegments>
            <CopyFromInput index="3" /><CopyFromInput index="4" />
          </MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PST</MorphemeId>
        <OutputHeadFeatures><FeatureValue feature="featTense" symbolValues="symPast" /></OutputHeadFeatures>
        <MorphologicalSubrules><MorphologicalSubrule id="sub2">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+ɯd</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrHt mrEd", "");
    let m = Morpher::new(&g, usize::MAX);
    let out = m.parse_word("pastagɯd");
    assert_morphs_eq(&out, &["prefix 32 PST"]);
    assert_eq!(out.structured.len(), 1);
}

/// Ports `AffixProcessRuleTests.PartialRule` (AffixProcessRuleTests.cs:1600-1741): `IsPartial` rules
/// interacting with two optional-slot templates (verb/noun), gated by Tier-2 #13's 3 gates.
#[test]
fn partial_rule() {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>TEMP_VERB</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd1">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
              <PhoneticSequence id="2"><SimpleContext naturalClass="ncAlvStop" /></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><CopyFromInput index="2" /><InsertSegments><PhoneticShape>ɯd</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subEd2">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence><SimpleContext naturalClass="ncVlCons" /></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>t</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="subEd3">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>d</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrS" requiredPartsOfSpeech="posV" partial="true"><Name>s_suffix</Name><MorphemeId>PART_VERB</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subS">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrNom" requiredPartsOfSpeech="posV" outputPartOfSpeech="posN"><Name>nominalizer</Name><MorphemeId>DERIV</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subNom">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>v</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrU" requiredPartsOfSpeech="posN" partial="true"><Name>u_suffix</Name><MorphemeId>PART_NOUN</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subU">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>u</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrP" requiredPartsOfSpeech="posN"><Name>p_suffix</Name><MorphemeId>TEMP_NOUN</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subP">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let templates_final = r#"
      <AffixTemplate requiredPartsOfSpeech="posV"><Name>verb</Name><Slot morphologicalRules="mrEd" optional="true"><Name>Sl1</Name></Slot></AffixTemplate>
      <AffixTemplate requiredPartsOfSpeech="posN"><Name>noun</Name><Slot morphologicalRules="mrP" optional="true"><Name>Sl2</Name></Slot></AffixTemplate>
    "#;
    let g = build_grammar("", "", mrules, "mrS mrNom mrU", templates_final);
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("sagd"), &["32 TEMP_VERB"]);
    assert_morphs_eq(&m.parse_word("sagds"), &["32 TEMP_VERB PART_VERB"]);
    assert_morphs_eq(&m.parse_word("sagst"), &["32 PART_VERB TEMP_VERB"]);
    assert_morphs_eq(&m.parse_word("sags"), &["32 PART_VERB"]);
    assert_morphs_eq(&m.parse_word("sagsv"), &["32 PART_VERB DERIV"]);
    assert_morphs_eq(&m.parse_word("sagstv"), &["32 PART_VERB TEMP_VERB DERIV"]);
    assert_empty(&m.parse_word("sagdst"));
    assert_morphs_eq(&m.parse_word("sagvu"), &["32 DERIV PART_NOUN"]);
    assert_morphs_eq(&m.parse_word("sagvup"), &["32 DERIV PART_NOUN TEMP_NOUN"]);

    let templates_nonfinal = r#"
      <AffixTemplate requiredPartsOfSpeech="posV" final="false"><Name>verb</Name><Slot morphologicalRules="mrEd" optional="true"><Name>Sl1</Name></Slot></AffixTemplate>
      <AffixTemplate requiredPartsOfSpeech="posN"><Name>noun</Name><Slot morphologicalRules="mrP" optional="true"><Name>Sl2</Name></Slot></AffixTemplate>
    "#;
    let g2 = build_grammar("", "", mrules, "mrS mrNom mrU", templates_nonfinal);
    let m2 = Morpher::new(&g2, usize::MAX);
    assert_empty(&m2.parse_word("sagds"));
}

/// Ports `AffixProcessRuleTests.DisjunctiveAllomorphs` (AffixProcessRuleTests.cs:1744-1811): affix
/// allomorph selection by phonological context of the STEM (vowel-final vs consonant-final), combined
/// with a second, environment-gated affix.
#[test]
fn disjunctive_allomorphs() {
    let mrules = r#"
      <MorphologicalRule id="mrEs" requiredPartsOfSpeech="posN"><Name>s_suffix</Name><MorphemeId>PL</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="sub1">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="sub2">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>ɯs</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrGu" requiredPartsOfSpeech="posN"><Name>gu_suffix</Name><MorphemeId>3SG</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="sub3">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>gun</PhoneticShape></InsertSegments></MorphologicalOutput>
            <RequiredEnvironments><Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment></RequiredEnvironments>
          </MorphologicalSubrule>
          <MorphologicalSubrule id="sub4">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>gu</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrEs mrGu", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("pugunɯs"), &["52 3SG PL"]);
    assert_morphs_eq(&m.parse_word("pugus"), &["52 3SG PL"]);
    assert_empty(&m.parse_word("puguɯs"));
    assert_morphs_eq(&m.parse_word("pus"), &["52 PL"]);
    assert_empty(&m.parse_word("puɯs"));
}

/// Ports `AffixProcessRuleTests.SubsumedAffix` (AffixProcessRuleTests.cs:1814-1900): allomorph
/// pattern subsumption ordering (a more specific "vowel-final" `Lhs` for `s_suffix`/`delete_vowel_
/// suffix` vs the general "any+" pattern other rules use).
const SUBSUMED_AFFIX_MRULES: &str = r#"
  <MorphologicalRule id="mrU" requiredPartsOfSpeech="posV"><Name>u_suffix</Name><MorphemeId>3SG</MorphemeId>
    <MorphologicalSubrules><MorphologicalSubrule id="sub1">
      <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
      <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>u</PhoneticShape></InsertSegments></MorphologicalOutput>
    </MorphologicalSubrule></MorphologicalSubrules>
  </MorphologicalRule>
  <MorphologicalRule id="mrS" requiredPartsOfSpeech="posV"><Name>s_suffix</Name><MorphemeId>PAST</MorphemeId>
    <MorphologicalSubrules><MorphologicalSubrule id="sub2">
      <MorphologicalInput>
        <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
        <PhoneticSequence id="2"><SimpleContext naturalClass="ncV" /></PhoneticSequence>
      </MorphologicalInput>
      <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
    </MorphologicalSubrule></MorphologicalSubrules>
  </MorphologicalRule>
  <MorphologicalRule id="mrNom" requiredPartsOfSpeech="posV" outputPartOfSpeech="posN"><Name>nominalizer</Name><MorphemeId>NOM</MorphemeId>
    <MorphologicalSubrules><MorphologicalSubrule id="sub3">
      <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
      <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>v</PhoneticShape></InsertSegments></MorphologicalOutput>
    </MorphologicalSubrule></MorphologicalSubrules>
  </MorphologicalRule>
  <MorphologicalRule id="mrDel" requiredPartsOfSpeech="posV"><Name>delete_vowel_suffix</Name><MorphemeId>PRES</MorphemeId>
    <MorphologicalSubrules><MorphologicalSubrule id="sub4">
      <MorphologicalInput>
        <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
        <PhoneticSequence id="2"><SimpleContext naturalClass="ncV" /></PhoneticSequence>
      </MorphologicalInput>
      <MorphologicalOutput><CopyFromInput index="1" /></MorphologicalOutput>
    </MorphologicalSubrule></MorphologicalSubrules>
  </MorphologicalRule>
"#;

#[test]
fn subsumed_affix() {
    let g = build_grammar("", "", SUBSUMED_AFFIX_MRULES, "mrU mrS mrNom mrDel", "");
    let m = Morpher::new(&g, usize::MAX);
    // The one single-rule, non-zero-material analysis: live.
    assert_morphs_eq(&m.parse_word("tagu"), &["47 3SG"]);
}

/// **FIXED (wave-4).** The wave-3 finding text (kept for history) diagnosed two residuals after
/// item 1's char_def fix: (a) "tags"/"tagsv"/"tag" recovered only `{"47 PAST"}`-style sets with the
/// `u_suffix`-chained "3SG" component missing, and (b) "bubib" dropped the pure-deletion rule's own
/// "PRES" morph. Both turned out to be the SAME root cause family in
/// `pg_rules::morph::attribute_morphs`, not an analysis-cascade gap at all:
/// - (b) was W9.1/`dfbb754b` (a pure-truncation rule's own allomorph never recorded) — fixed by the
///   wave-4 floating-marker port (`MorphStatus::Floating`, fixture
///   `rust/conformance/affix-shapes/truncate/`).
/// - (a) was the **input-morph subsumption** half of the same C# branch
///   (`SynthesisAffixProcessAllomorphRuleSpec.ApplyRhs`, cs:185-205): on the synthesis-confirm of
///   tag+u+s, `s_suffix` captures the "u" (3SG's entire realization) as part "2" and never copies
///   it, so the 3SG record contributed zero output positions and was silently dropped — the
///   analysis chain itself was fine. C# marks such a morph via `MarkSubsumedMorph` (child of the
///   new "s" morph — rendering "47 3SG PAST" with 3SG *before* its host, postorder) or
///   `MarkMorph(Shape.First)` for pure truncation ("tag" → "47 3SG PRES"). Ported as
///   `MorphStatus::SubsumedChild`/`SubsumedFirst` (see `attribute_morphs`' doc).
///
/// Red-on-revert: dropping the `Real`-with-no-runs fallback arm in `attribute_morphs` returns
/// "tags" to `{"47 PAST"}` and "tag" to `{"47 PRES", "47"}`.
#[test]
fn subsumed_affix_findings() {
    let g = build_grammar("", "", SUBSUMED_AFFIX_MRULES, "mrU mrS mrNom mrDel", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("tags"), &["47 3SG PAST"]);
    assert_morphs_eq(&m.parse_word("tagsv"), &["47 3SG PAST NOM"]);
    assert_morphs_eq(&m.parse_word("tag"), &["47 3SG PRES", "47"]);
    assert_morphs_eq(&m.parse_word("bubib"), &["42 PRES", "43 PRES"]);
}

/// Ports `AffixProcessRuleTests.ModifyFromInputRules` (AffixProcessRuleTests.cs:1903-1945):
/// `ModifyFromInput` combined with `InsertSegments` in the same `Rhs`, after a captured vowel.
///
/// FIXED (plan item 1 / wave-3): see `simulfix_rules`'s doc comment for the full root-cause trace and
/// fix. Un-ignored; green.
#[test]
fn modify_from_input_rules() {
    let mrules = r#"
      <MorphologicalRule id="mrS" requiredPartsOfSpeech="posN"><Name>s_suffix</Name><MorphemeId>PL</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="sub1">
          <MorphologicalInput>
            <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
            <PhoneticSequence id="2"><SimpleContext naturalClass="ncV" /></PhoneticSequence>
          </MorphologicalInput>
          <MorphologicalOutput>
            <CopyFromInput index="1" /><CopyFromInput index="2" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments>
            <ModifyFromInput index="2"><SimpleContext naturalClass="ncLowRound" /></ModifyFromInput>
          </MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrS", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("puso"), &["52 PL"]);
}

/// Ports `AffixProcessRuleTests.InfixRules` (AffixProcessRuleTests.cs:695-853) -- the W9.1 probe
/// (batch-4) found the general affix machinery already matches C# here (verbatim port, no engine
/// changes needed), which is why this and the other 3 batch-4 tests below only needed the missing
/// lexical entry "49" ("ktb", added to `csharp_port_common`) plus already-present natural classes
/// (`cons`=`ncC`, `voicelessStop`=`ncVlStop`, `unasp`=`ncUnasp`). Four Arabic-templatic-style
/// `AffixProcessRule`s discontiguously interleave the consonantal root "ktb" with vowels: `perf_act`
/// (root's 3 consonants each as their own `MorphologicalInput` group, `1a2a3`), `perf_pass`
/// (`1u2i3`), `imperf_act` (root's first 2 consonants as ONE two-node group, `a12u3` -- exercising a
/// multi-segment single group, not 3 singletons), `imperf_pass` (`u123a`, 3 singleton groups again).
/// Plus a trailing aspiration-neutralization `RewriteRule` (voiceless stop -> unaspirated), identical
/// in shape to `prefix_rules`'s own aspiration rule above.
#[test]
fn infix_rules() {
    let mrules = r#"
      <MorphologicalRule id="mrPerfAct" requiredPartsOfSpeech="posV"><Name>perf_act</Name><MorphemeId>PER.ACT</MorphemeId>
        <RequiredHeadFeatures>
          <FeatureValue feature="featAspect" symbolValues="symPerf" /><FeatureValue feature="featMood" symbolValues="symActive" />
        </RequiredHeadFeatures>
        <MorphologicalSubrules><MorphologicalSubrule id="subPerfAct">
          <MorphologicalInput>
            <PhoneticSequence id="1"><SimpleContext naturalClass="ncC" /></PhoneticSequence>
            <PhoneticSequence id="2"><SimpleContext naturalClass="ncC" /></PhoneticSequence>
            <PhoneticSequence id="3"><SimpleContext naturalClass="ncC" /></PhoneticSequence>
          </MorphologicalInput>
          <MorphologicalOutput>
            <CopyFromInput index="1" /><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
            <CopyFromInput index="2" /><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
            <CopyFromInput index="3" />
          </MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrPerfPass" requiredPartsOfSpeech="posV"><Name>pref_pass</Name><MorphemeId>PER.PSV</MorphemeId>
        <RequiredHeadFeatures>
          <FeatureValue feature="featAspect" symbolValues="symPerf" /><FeatureValue feature="featMood" symbolValues="symPassive" />
        </RequiredHeadFeatures>
        <MorphologicalSubrules><MorphologicalSubrule id="subPerfPass">
          <MorphologicalInput>
            <PhoneticSequence id="1"><SimpleContext naturalClass="ncC" /></PhoneticSequence>
            <PhoneticSequence id="2"><SimpleContext naturalClass="ncC" /></PhoneticSequence>
            <PhoneticSequence id="3"><SimpleContext naturalClass="ncC" /></PhoneticSequence>
          </MorphologicalInput>
          <MorphologicalOutput>
            <CopyFromInput index="1" /><InsertSegments><PhoneticShape>u</PhoneticShape></InsertSegments>
            <CopyFromInput index="2" /><InsertSegments><PhoneticShape>i</PhoneticShape></InsertSegments>
            <CopyFromInput index="3" />
          </MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrImperfAct" requiredPartsOfSpeech="posV"><Name>imperf_act</Name><MorphemeId>IMPF.ACT</MorphemeId>
        <RequiredHeadFeatures>
          <FeatureValue feature="featAspect" symbolValues="symImpf" /><FeatureValue feature="featMood" symbolValues="symActive" />
        </RequiredHeadFeatures>
        <MorphologicalSubrules><MorphologicalSubrule id="subImperfAct">
          <MorphologicalInput>
            <PhoneticSequence id="1"><SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncC" /></PhoneticSequence>
            <PhoneticSequence id="2"><SimpleContext naturalClass="ncC" /></PhoneticSequence>
          </MorphologicalInput>
          <MorphologicalOutput>
            <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments><CopyFromInput index="1" />
            <InsertSegments><PhoneticShape>u</PhoneticShape></InsertSegments><CopyFromInput index="2" />
          </MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrImperfPass" requiredPartsOfSpeech="posV"><Name>imperf_pass</Name><MorphemeId>IMPF.PSV</MorphemeId>
        <RequiredHeadFeatures>
          <FeatureValue feature="featAspect" symbolValues="symImpf" /><FeatureValue feature="featMood" symbolValues="symPassive" />
        </RequiredHeadFeatures>
        <MorphologicalSubrules><MorphologicalSubrule id="subImperfPass">
          <MorphologicalInput>
            <PhoneticSequence id="1"><SimpleContext naturalClass="ncC" /></PhoneticSequence>
            <PhoneticSequence id="2"><SimpleContext naturalClass="ncC" /></PhoneticSequence>
            <PhoneticSequence id="3"><SimpleContext naturalClass="ncC" /></PhoneticSequence>
          </MorphologicalInput>
          <MorphologicalOutput>
            <InsertSegments><PhoneticShape>u</PhoneticShape></InsertSegments>
            <CopyFromInput index="1" /><CopyFromInput index="2" />
            <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
            <CopyFromInput index="3" />
          </MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let prules = r#"
      <PhonologicalRule id="pr_asp"><Name>aspiration</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncVlStop" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule><PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncUnasp" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    "#;
    let g = build_grammar(
        prules,
        "pr_asp",
        mrules,
        "mrPerfAct mrPerfPass mrImperfAct mrImperfPass",
        "",
    );
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("katab"), &["49 PER.ACT"]);
    assert_morphs_eq(&m.parse_word("kutib"), &["49 PER.PSV"]);
    assert_morphs_eq(&m.parse_word("aktub"), &["IMPF.ACT 49"]);
    assert_morphs_eq(&m.parse_word("uktab"), &["IMPF.PSV 49"]);
}

/// Ports `AffixProcessRuleTests.CircumfixRules` (AffixProcessRuleTests.cs:1446-1472): an unrestricted
/// `circumfix` rule (`ta+root+od`) composes with an unrestricted `s_suffix` (`+s`) on the same
/// stratum -- "tasagods" = ta + "sag" (root "32") + od + s.
#[test]
fn circumfix_rules() {
    let mrules = r#"
      <MorphologicalRule id="mrCircum"><Name>circumfix</Name><MorphemeId>OBJ</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subCircum">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><InsertSegments><PhoneticShape>ta</PhoneticShape></InsertSegments><CopyFromInput index="1" /><InsertSegments><PhoneticShape>od</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrS"><Name>s_suffix</Name><MorphemeId>3SG</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subS">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrCircum mrS", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("tasagods"), &["OBJ 32 3SG"]);
}

/// Ports `AffixProcessRuleTests.TruncateRules` (AffixProcessRuleTests.cs:1159-1266): 5
/// reconfigurations of a single `truncate` rule (each `Allomorphs.Clear()`-then-re-add in C#, so a
/// fresh grammar per sub-case here too), each dropping (or, in the 5th, ambiguously restoring) part
/// of the matched input rather than copying it -- the defining trait of a truncation rule.
#[test]
fn truncate_rules() {
    // (1) drop a trailing literal "g": "sa" (root "32"="sag" minus "g") -> "32 3SG".
    let g1 = build_grammar(
        "",
        "",
        r#"<MorphologicalRule id="mrT" requiredPartsOfSpeech="posV"><Name>truncate</Name><MorphemeId>3SG</MorphemeId>
          <MorphologicalSubrules><MorphologicalSubrule id="sub1">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
              <PhoneticSequence id="2"><SimpleContext naturalClass="ncGSeg" /></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /></MorphologicalOutput>
          </MorphologicalSubrule></MorphologicalSubrules>
        </MorphologicalRule>"#,
        "mrT",
        "",
    );
    assert_morphs_eq(&Morpher::new(&g1, usize::MAX).parse_word("sa"), &["32 3SG"]);

    // (2) drop a leading literal "s": "ag" (root "32"="sag" minus leading "s") -> "32 3SG".
    let g2 = build_grammar(
        "",
        "",
        r#"<MorphologicalRule id="mrT" requiredPartsOfSpeech="posV"><Name>truncate</Name><MorphemeId>3SG</MorphemeId>
          <MorphologicalSubrules><MorphologicalSubrule id="sub1">
            <MorphologicalInput>
              <PhoneticSequence id="1"><SimpleContext naturalClass="ncSSeg" /></PhoneticSequence>
              <PhoneticSequence id="2"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="2" /></MorphologicalOutput>
          </MorphologicalSubrule></MorphologicalSubrules>
        </MorphologicalRule>"#,
        "mrT",
        "",
    );
    assert_morphs_eq(&Morpher::new(&g2, usize::MAX).parse_word("ag"), &["32 3SG"]);

    // (3) drop a leading FRICATIVE (natural class, not a literal char): same "ag" result, now via
    // `ncFric` matching "s" (cons+, cont+) rather than the segment-literal `ncSSeg` above.
    let g3 = build_grammar(
        "",
        "",
        r#"<MorphologicalRule id="mrT" requiredPartsOfSpeech="posV"><Name>truncate</Name><MorphemeId>3SG</MorphemeId>
          <MorphologicalSubrules><MorphologicalSubrule id="sub1">
            <MorphologicalInput>
              <PhoneticSequence id="1"><SimpleContext naturalClass="ncFric" /></PhoneticSequence>
              <PhoneticSequence id="2"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="2" /></MorphologicalOutput>
          </MorphologicalSubrule></MorphologicalSubrules>
        </MorphologicalRule>"#,
        "mrT",
        "",
    );
    assert_morphs_eq(&Morpher::new(&g3, usize::MAX).parse_word("ag"), &["32 3SG"]);

    // (4) drop a trailing VELAR STOP (natural class): "sa" again, now via `ncVelarC` matching "g"
    // (cons+, poa=velar) rather than the segment-literal `ncGSeg` in (1).
    let g4 = build_grammar(
        "",
        "",
        r#"<MorphologicalRule id="mrT" requiredPartsOfSpeech="posV"><Name>truncate</Name><MorphemeId>3SG</MorphemeId>
          <MorphologicalSubrules><MorphologicalSubrule id="sub1">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
              <PhoneticSequence id="2"><SimpleContext naturalClass="ncVelarC" /></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /></MorphologicalOutput>
          </MorphologicalSubrule></MorphologicalSubrules>
        </MorphologicalRule>"#,
        "mrT",
        "",
    );
    assert_morphs_eq(&Morpher::new(&g4, usize::MAX).parse_word("sa"), &["32 3SG"]);

    // (5) OPTIONAL leading "s" truncated, "g" always prepended: unapplying "gas" can restore the
    // truncated "s" (root "33"="sas") or not (in which case "as" alone would need its own root,
    // which doesn't exist) -- only the "sas" reconstruction survives, giving "3SG 33". Unapplying
    // "gbubibi" similarly tries restoring a leading "s" (no such root "sbubibi") and not restoring one
    // (root "42"="bubibi" -- survives), giving "3SG 42". Matches AffixProcessRuleTests.cs:1264-1265.
    let g5 = build_grammar(
        "",
        "",
        r#"<MorphologicalRule id="mrT" requiredPartsOfSpeech="posV"><Name>truncate</Name><MorphemeId>3SG</MorphemeId>
          <MorphologicalSubrules><MorphologicalSubrule id="sub1">
            <MorphologicalInput>
              <PhoneticSequence id="1"><OptionalSegmentSequence min="0" max="1"><SimpleContext naturalClass="ncSSeg" /></OptionalSegmentSequence></PhoneticSequence>
              <PhoneticSequence id="2"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>g</PhoneticShape></InsertSegments><CopyFromInput index="2" /></MorphologicalOutput>
          </MorphologicalSubrule></MorphologicalSubrules>
        </MorphologicalRule>"#,
        "mrT",
        "",
    );
    let m5 = Morpher::new(&g5, usize::MAX);
    assert_morphs_eq(&m5.parse_word("gas"), &["3SG 33"]);
    assert_morphs_eq(&m5.parse_word("gbubibi"), &["3SG 42"]);
}

/// Ports `AffixProcessRuleTests.NonContiguousRules` (AffixProcessRuleTests.cs:1948-2030): the same
/// `perf_act` infix shape as [`infix_rules`] (root "49"="ktb" -> "k"+a+"t"+a+"b"+"ɯd" = "katabɯd"),
/// but this time followed by an ITERATIVE `RewriteRule` raising a low vowel ("a") to `[i]`
/// (`ncISeg`) whenever the RIGHT environment is a voiced consonant (`ncVoicedCons`) -- discontiguous
/// from the affixation rule's own insertion sites, hence the test's name. Applied iteratively across
/// "katabɯd": the first "a" (before "t", voiceless) is unaffected; the second "a" (before "b",
/// voiced) raises to "i", giving the expected surface "katibɯd".
#[test]
fn non_contiguous_rules() {
    let mrules = r#"
      <MorphologicalRule id="mrPerfAct" requiredPartsOfSpeech="posV"><Name>perf_act</Name><MorphemeId>PER.ACT</MorphemeId>
        <RequiredHeadFeatures>
          <FeatureValue feature="featAspect" symbolValues="symPerf" /><FeatureValue feature="featMood" symbolValues="symActive" />
        </RequiredHeadFeatures>
        <MorphologicalSubrules><MorphologicalSubrule id="subPerfAct">
          <MorphologicalInput>
            <PhoneticSequence id="1"><SimpleContext naturalClass="ncC" /></PhoneticSequence>
            <PhoneticSequence id="2"><SimpleContext naturalClass="ncC" /></PhoneticSequence>
            <PhoneticSequence id="3"><SimpleContext naturalClass="ncC" /></PhoneticSequence>
          </MorphologicalInput>
          <MorphologicalOutput>
            <CopyFromInput index="1" /><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
            <CopyFromInput index="2" /><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
            <CopyFromInput index="3" /><InsertSegments><PhoneticShape>ɯd</PhoneticShape></InsertSegments>
          </MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let prules = r#"
      <PhonologicalRule id="pr1"><Name>rule1</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncLowV" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncISeg" /></PhoneticSequence></PhoneticOutput>
            <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncVoicedCons" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    "#;
    let g = build_grammar(prules, "pr1", mrules, "mrPerfAct", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("katibɯd"), &["49 PER.ACT"]);
}
