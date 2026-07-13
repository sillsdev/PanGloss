//! Ports `MorpherTests`' two `GenerateWords_*` tests (parse-opt: `tests/SIL.Machine.Morphology.
//! HermitCrab.Tests/MorpherTests.cs:276-346`) — D-batch-7, the `WordAnalysis`/`GenerateWords` API
//! this file's sibling ports (`csharp_port_affix_process.rs`, `csharp_port_compounding.rs`) noted as
//! absent and out of scope. W7 lands the API; this file is its conformance evidence.
//!
//! Also ports the one `GenerateWords` assertion in `CompoundingRuleTests.MorphosyntacticRules`
//! (CompoundingRuleTests.cs:143-146) as [`direct_api_compounding_non_head`] — the ONLY C# test in the
//! whole suite that calls the direct 3-arg API with a bare `LexEntry` as an "other morpheme" (a
//! compounding non-head with no known `CompoundingRule`), which is what exercises the `mrule_apps`
//! `None`-wildcard engine support W7 added in `hc_rules::word`/`hc_rules::stratum`.
//!
//! **Update (W11 batch-7 remainder):** the `AffixProcessRuleTests.SuffixRules` direct-API assertions
//! noted as out-of-scope above are now ported at `csharp_port_affix_process.rs::suffix_rules`, using
//! the `lex_entry_id`/`mrule_id`/`morpheme_ordinal` lookups this file originally wrote, since
//! generalized into `csharp_port_common` so both files (and this one) share one copy.
//! `PrefixRules` has no `GenerateWords` calls in its C# body. Also ports the other half of the
//! D-batch-7 gap the coverage map flagged as needing `WordAnalysis` to exist first
//! (D-test-coverage-map.md:127): [`analyze_word_can_analyze_returns_correct_analysis`] below is the
//! true structured-return path for `MorpherTests.AnalyzeWord_CanAnalyze_ReturnsCorrectAnalysis`
//! (`ParseOutcome::structured`, not just a string-signature check like `csharp_port_morpher.rs`'s
//! existing `AnalyzeWord_CanAnalyzeLinear` port).

mod csharp_port_common;
use csharp_port_common::{build_grammar, lex_entry_id, morpheme_ordinal, mrule_id};
use hc_featstruct::FeatureStruct;
use hc_parse::{GenMorpheme, Morpher, WordAnalysis};
use std::collections::BTreeSet;

/// `si+` prefix (Gloss "3SG") + `+ɯd` suffix (Gloss "PAST"), both `requiredPartsOfSpeech="posV"` —
/// the exact two `AffixProcessRule`s `MorpherTests.GenerateWords_CanGenerate_ReturnsCorrectWord`
/// (MorpherTests.cs:281-311) builds inline via `AffixProcessRule`/`AffixProcessAllomorph` object
/// construction, ported here as XML (this port's grammar-construction idiom throughout).
const SI_ED_MRULES: &str = r#"
  <MorphologicalRule id="mrSi" requiredPartsOfSpeech="posV"><Name>si_prefix</Name><MorphemeId>3SG</MorphemeId>
    <MorphologicalSubrules><MorphologicalSubrule id="subSi">
      <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
      <MorphologicalOutput><InsertSegments><PhoneticShape>si+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
    </MorphologicalSubrule></MorphologicalSubrules>
  </MorphologicalRule>
  <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
    <MorphologicalSubrules><MorphologicalSubrule id="subEd">
      <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
      <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+ɯd</PhoneticShape></InsertSegments></MorphologicalOutput>
    </MorphologicalSubrule></MorphologicalSubrules>
  </MorphologicalRule>
"#;

/// Ports `MorpherTests.GenerateWords_CanGenerate_ReturnsCorrectWord` (MorpherTests.cs:276-319): a
/// `WordAnalysis` for entry "33" ("sas", V) with `si_prefix` to its left and `ed_suffix` to its
/// right must generate exactly `"sisasɯd"` (the `+` boundary markers stripped by
/// `Shape.ToString(table, includeBdry: false)`, Morpher.cs:222) via the `WordAnalysis`-consuming
/// overload (`Morpher::generate_words_from_analysis`).
#[test]
fn generate_words_can_generate_returns_correct_word() {
    let g = build_grammar("", "", SI_ED_MRULES, "mrSi mrEd", "");
    let m = Morpher::new(&g, usize::MAX);

    let wa = WordAnalysis {
        morpheme_ids: vec![
            morpheme_ordinal(&g, "3SG"),
            morpheme_ordinal(&g, "33"),
            morpheme_ordinal(&g, "PAST"),
        ],
        root_morpheme_index: 1,
        pos_id: None,
        guessed: false,
    };
    let words: BTreeSet<String> = m.generate_words_from_analysis(&wa).into_iter().collect();
    assert_eq!(words, BTreeSet::from(["sisasɯd".to_string()]));
}

/// Ports `MorpherTests.GenerateWords_CannotGenerate_ReturnsEmptyEnumerable` (MorpherTests.cs:321-346):
/// a `PL`-suffix requiring `posN` cannot generate from root "32" ("sag", V) — the
/// `RequiredSyntacticFeatureStruct` gate (`SynthesisAffixProcessRule.cs:122`, ported at
/// `hc_rules::morph::synth_syn_fs`) rejects it, so the whole `WordAnalysis` yields no words.
#[test]
fn generate_words_cannot_generate_returns_empty_enumerable() {
    let mrules = r#"
      <MorphologicalRule id="mrPl" requiredPartsOfSpeech="posN"><Name>ed_suffix</Name><MorphemeId>PL</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subPl">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+ɯd</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrPl", "");
    let m = Morpher::new(&g, usize::MAX);

    let wa = WordAnalysis {
        morpheme_ids: vec![morpheme_ordinal(&g, "32"), morpheme_ordinal(&g, "PL")],
        root_morpheme_index: 0,
        pos_id: None,
        guessed: false,
    };
    assert!(m.generate_words_from_analysis(&wa).is_empty());
}

/// Direct-API sanity check (no C# test citation of its own — see this file's module doc): the same
/// `ed_suffix` rule as above, called through `Morpher::generate_words` directly (root "33" + the
/// rule, skipping the `WordAnalysis` layer entirely) must reproduce the same root+affix
/// concatenation semantics: "sas" + "+ɯd" (boundary stripped) = "sasɯd".
#[test]
fn direct_api_single_allomorph_suffix() {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subEd">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+ɯd</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrEd", "");
    let m = Morpher::new(&g, usize::MAX);

    let root = lex_entry_id(&g, "33");
    let rule = mrule_id(&g, "PAST");
    let words = m.generate_words(root, &[GenMorpheme::Rule(rule)], FeatureStruct::EMPTY);
    assert_eq!(words, vec!["sasɯd".to_string()]);
}

/// Ports the `GenerateWords` assertion inside `CompoundingRuleTests.MorphosyntacticRules`
/// (CompoundingRuleTests.cs:143-146): `GenerateWords(Entries["5"], new Morpheme[] { Entries["9"] },
/// new FeatureStruct())` must produce `"pʰutdat"` — the ONE C# test that passes a bare `LexEntry`
/// as a direct-API "other morpheme" (a compounding non-head with the owning `CompoundingRule`
/// deliberately unspecified), exercising the `mrule_apps` `None`-wildcard support added to
/// `hc_rules::word`/`hc_rules::stratum` for this milestone (`guided_synth`'s `None => is_compound`
/// arm). Same `rule1`/entries "5"("pʰut",N)/"9"("dat",V) as `csharp_port_compounding.rs`'s own
/// `morphosyntactic_rules` port (`nonHeadPartsOfSpeech="posV"` — "9" qualifies, its homophone "8"
/// (dat, N) would not).
#[test]
fn direct_api_compounding_non_head() {
    let mrules = r#"
      <CompoundingRule id="mrC" nonHeadPartsOfSpeech="posV">
        <Name>rule1</Name>
        <CompoundingSubrules><CompoundingSubrule>
          <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
          <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
        </CompoundingSubrule></CompoundingSubrules>
      </CompoundingRule>
    "#;
    let g = build_grammar("", "", mrules, "mrC", "");
    let m = Morpher::new(&g, usize::MAX);

    let root = lex_entry_id(&g, "5");
    let non_head = lex_entry_id(&g, "9");
    let words = m.generate_words(
        root,
        &[GenMorpheme::NonHead(non_head)],
        FeatureStruct::EMPTY,
    );
    assert_eq!(words, vec!["pʰutdat".to_string()]);
}

/// No C# citation (a discriminating regression test added during P4 review, not a port): TWO
/// `GenMorpheme::NonHead` items in one `generate_words` call, pinning that a nested (nonhead-of-a-
/// nonhead-of-the-root) compound resolves EACH non-head slot correctly rather than reusing the
/// same one twice.
///
/// `generate_words` pushes one `(mrule_apps[i] = None, non_heads[i])` pair per `NonHead` item, in
/// `others`' own order (`permute_rules` preserves input order, branching only over allomorph
/// choice — see that function's doc): entry "8" ("dat", N) lands at index 0, entry "46" ("bupu", N)
/// at index 1. Synthesis confirms trail slots from the END backward (`mrule_app_index`/
/// `non_head_app_index` both start at 1 and decrement on each compounding confirmation, exactly
/// mirroring C#'s `_mruleAppIndex`/`_nonHeadAppIndex`, Word.cs:411-429), so the FIRST compounding
/// confirmation must consume `non_heads[1]` ("bupu") and the SECOND (nested, now operating on the
/// first compound's own output shape as its new "head") must consume `non_heads[0]` ("dat").
///
/// This is exactly the case `Word::current_non_head()`'s old `non_heads.last()` got wrong: since
/// P4's fix to `synth_compound_subrule` (deliberately not popping a confirmed non-head off
/// `non_heads`, to preserve `WordKey` disambiguation — see `hc-rules/src/morph.rs`'s comment
/// there), `non_heads` stays `[dat, bupu]` (2 elements) for BOTH confirmations. `.last()` always
/// re-reads "bupu" for the second (nested) confirmation too, producing `pʰutbupubupu` (dat is never
/// used, bupu is used twice) instead of the correct `pʰutbupudat` (each entry used exactly once,
/// innermost non-head first per the compounding output template's `head+"+"+nonHead` order,
/// confirmed root-outward). `Word::current_non_head()` reading by `non_head_app_index` (matching
/// C#'s `_nonHeadApps[_nonHeadAppIndex]`, Word.cs:453-461) instead of `.last()` is what makes the
/// second confirmation see "dat" instead of "bupu" again.
#[test]
fn direct_api_compounding_two_non_heads_resolve_distinct_slots() {
    let mrules = r#"
      <CompoundingRule id="mrC">
        <Name>rule1</Name>
        <CompoundingSubrules><CompoundingSubrule>
          <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
          <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
        </CompoundingSubrule></CompoundingSubrules>
      </CompoundingRule>
    "#;
    // Unrestricted (no POS gates at all): the SAME compounding rule must be free to re-confirm on
    // its own output (the nested/outer compounding step), which a POS-gated rule (like the sibling
    // `direct_api_compounding_non_head` test's `mrC`) would block once the head's POS changes.
    let g = build_grammar("", "", mrules, "mrC", "");
    let m = Morpher::new(&g, usize::MAX);

    let root = lex_entry_id(&g, "5"); // "pʰut"
    let dat = lex_entry_id(&g, "8"); // "dat" -- pushed FIRST (non_heads[0]), confirmed SECOND
    let bupu = lex_entry_id(&g, "46"); // "bupu" -- pushed SECOND (non_heads[1]), confirmed FIRST
    let words = m.generate_words(
        root,
        &[GenMorpheme::NonHead(dat), GenMorpheme::NonHead(bupu)],
        FeatureStruct::EMPTY,
    );
    assert_eq!(
        words,
        vec!["pʰutbupudat".to_string()],
        "each non-head must be used exactly once (bupu innermost, dat outermost); got {words:?} \
         (\"pʰutbupubupu\" would mean the second confirmation re-read the wrong non-head)"
    );
}

/// No C# citation (a discriminating fixture this port needed but the two ported MorpherTests don't
/// exercise, since both have exactly one morpheme per side of the root): pins
/// `Morpher::generate_words_from_analysis`'s left-side reversal against C#'s actual
/// `PermuteOtherMorphemes`/`PermuteRules` trail-construction order (Morpher.cs:239-280,681-711),
/// mechanically re-derived (not just hand-traced) while porting to settle a real, initially-missed
/// deviation: this port's `interleavings` helper preserves each side's array (left-to-right
/// position) order, but C#'s stack-based construction, run through BOTH `PermuteOtherMorphemes` and
/// `PermuteRules`, ends up requiring the OUTER (root-index-0, leftmost) prefix in a two-prefix chain
/// to be `mrule_apps`'s LAST-pushed (hence first-confirmed-during-synthesis) entry -- the opposite of
/// naively preserving position order. `Morpher::generate_words_from_analysis` reverses the resolved
/// `left` slice before calling `interleavings` specifically to reproduce this.
///
/// The grammar is built so ONLY the C#-correct order can succeed at all, making this genuinely
/// discriminating rather than incidentally passing either way: `outer` requires `posV` (satisfied by
/// the bare root, so it must be the rule confirmed FIRST) and outputs `posA`; `inner` requires `posA`
/// (satisfied only AFTER `outer` has run, so it must be confirmed SECOND) — a naive
/// position-order-preserving interleaving would try `inner` first against the still-`posV` root and
/// fail outright, synthesizing nothing.
#[test]
fn generate_words_from_analysis_two_prefixes_confirm_in_the_correct_relative_order() {
    let mrules = r#"
      <MorphologicalRule id="mrOuter" requiredPartsOfSpeech="posV" outputPartOfSpeech="posA"><Name>outer</Name><MorphemeId>OUTER</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subOuter">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><InsertSegments><PhoneticShape>o+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrInner" requiredPartsOfSpeech="posA" outputPartOfSpeech="posA"><Name>inner</Name><MorphemeId>INNER</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subInner">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><InsertSegments><PhoneticShape>i+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrOuter mrInner", "");
    let m = Morpher::new(&g, usize::MAX);

    // Word-position order: OUTER (leftmost) then INNER (adjacent to root) then the root "32" ("sag", V).
    let wa = WordAnalysis {
        morpheme_ids: vec![
            morpheme_ordinal(&g, "OUTER"),
            morpheme_ordinal(&g, "INNER"),
            morpheme_ordinal(&g, "32"),
        ],
        root_morpheme_index: 2,
        pos_id: None,
        guessed: false,
    };
    let words: BTreeSet<String> = m.generate_words_from_analysis(&wa).into_iter().collect();
    assert_eq!(words, BTreeSet::from(["iosag".to_string()]));
}

/// Ports `MorpherTests.AnalyzeWord_CanAnalyze_ReturnsCorrectAnalysis` (MorpherTests.cs:13-40): the
/// true structured-`WordAnalysis`-return path, as opposed to `csharp_port_morpher.rs`'s
/// `analyze_word_can_analyze_linear_returns_correct_analysis` (which checks the morph/signature
/// string form only, not the `WordAnalysis` object itself). `ParseOutcome::structured`
/// (`Morpher::structured_analysis`, morpher.rs:371-384) is the exact Rust value the C# assertion's
/// `new WordAnalysis(new IMorpheme[] { Entries["32"], edSuffix }, 0, "V")` mirrors: root "32"
/// ("sag", V) + `ed_suffix` (PAST) analyzing "sagd" must produce exactly one `WordAnalysis` =
/// (morpheme_ids `[32, PAST]`, root index 0, POS "V").
#[test]
fn analyze_word_can_analyze_returns_correct_analysis() {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="subEd">
          <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrEd", "");
    let m = Morpher::new(&g, usize::MAX);

    let outcome = m.parse_word("sagd");
    let want_pos = g
        .syn_features
        .symbol_index(g.syn_features.pos, "posV")
        .expect("posV symbol");
    let want = WordAnalysis {
        morpheme_ids: vec![morpheme_ordinal(&g, "32"), morpheme_ordinal(&g, "PAST")],
        root_morpheme_index: 0,
        pos_id: Some(want_pos),
        guessed: false,
    };
    assert_eq!(
        outcome.structured,
        vec![want],
        "structured WordAnalysis mismatch (got {:?})",
        outcome.structured
    );
}
