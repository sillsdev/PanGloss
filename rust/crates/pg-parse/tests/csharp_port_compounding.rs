//! Ports `CompoundingRuleTests` (parse-opt: `tests/SIL.Machine.Morphology.HermitCrab.Tests/
//! MorphologicalRules/CompoundingRuleTests.cs`) bucket-B rows per
//! `rust/parity-out/audit/phase2/D-test-coverage-map.md` §3. `ProdRestrictRule` (D-batch-3,
//! formerly scope-cut) is now ported too — see [`prod_restrict_rule`]; the
//! `HeadProdRestrictionsMprFeatures`/`NonHeadProdRestrictionsMprFeatures`/
//! `OutputProdRestrictionsMprFeatures` gates it exercises turned out to already be implemented
//! (`pg-rules/src/morph.rs`'s `compound_match` sites), so the port was pure test-writing (its
//! steps 1/3 previously inherited the homophone-collapse finding documented below, now fixed --
//! not a new divergence).
//!
//! `SimpleRules` used to omit its final reconfiguration (`rule1.MaxApplicationCount = 2` +
//! `Morpher.MaxStemCount = 3`, three-root compounding "pʰutdatpip"): `pg_parse::Morpher` hardcoded
//! `max_stem_count: 2` in its `AnalyzerConfig` with no constructor knob to raise it, so this
//! sub-case had no way to reach 3 roots through the public API -- confirmed the coverage map's own
//! note ("`MaxStemCount` itself untested") rather than a new finding.
//!
//! **G11 (2026-07-25): closed.** C#'s `Morpher.MaxStemCount` (Morpher.cs:72) is a settable
//! per-INSTANCE property, ctor default `2` (Morpher.cs:56) — `2` was always a faithful *default*,
//! but hardcoding it in `pg_parse::Morpher` also dropped C#'s configurability, which is what
//! actually blocked a genuine 3-stem compound (a real, supported C# construct, not a design gap).
//! `Morpher` now carries a `max_stem_count` field (default `2`, unchanged) plus a builder,
//! [`Morpher::with_max_stem_count`], mirroring C#'s `new Morpher(...) { MaxStemCount = 3 }` usage
//! exactly (see `pg-parse/src/morpher.rs`'s field doc on `max_stem_count` for the full citation
//! trail and the "never explode" argument -- the existing per-`parse_word` step budget/timeout
//! already bounds every candidate regardless of this gate's value). The two below,
//! [`simple_rules_4_three_root_compound_single_rule`] and
//! [`simple_rules_5_three_root_compound_two_rules`], port `SimpleRules`' previously-omitted final
//! reconfiguration (cs:76-108) now that the knob exists.
//!
//! Two further findings surfaced while porting the remaining `SimpleRules` reconfigurations:
//! `simple_rules_1_homophone_disjunction_finding` (P4, 2026-07-09: FIXED -- see that test's doc
//! comment for the root cause and repair; retained its original name/doc-comment history rather
//! than renaming, to keep the finding's paper trail intact) and
//! `simple_rules_3_prefix_commutes_with_compounding` (formerly `#[ignore]`d as a "compounding
//! analysis never recurses into the non-head" engine gap -- resolved 2026-07-09 as a PORT bug, not
//! an engine gap: the C# reconfiguration keeps reconfiguration 2's nonHead+head output order, so
//! the affixed span is the HEAD, and the live C# oracle confirms both engines behave identically;
//! see that test's doc comment).

mod csharp_port_common;
use csharp_port_common::{assert_empty, assert_morphs_eq, build_grammar};
use pg_parse::Morpher;
use std::collections::BTreeSet;

fn root_gloss_set(outcome: &pg_parse::ParseOutcome) -> BTreeSet<String> {
    // `AssertRootAllomorphsEquals` (CompoundingRuleTests.cs:240-243): the DISTINCT set of each
    // surviving analysis' ROOT morpheme gloss (the first morpheme in the morph-order join).
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

/// Ports `CompoundingRuleTests.SimpleRules` reconfiguration 1 (CompoundingRuleTests.cs:13-29):
/// head+"+"+nonHead order. The negative checks (a nonhead/head that doesn't structurally match) are
/// live; the positive homophone-disjunction assertion is in
/// [`simple_rules_1_homophone_disjunction_finding`].
#[test]
fn simple_rules_1_negative_cases() {
    let g1 = build_grammar("", "", SIMPLE_RULES_MRULES_1, "mrC", "");
    let m1 = Morpher::new(&g1, usize::MAX);
    assert_empty(&m1.parse_word("pʰutdas"));
    assert_empty(&m1.parse_word("pʰusdat"));
}

/// **FIXED** (P4, 2026-07-09; name/doc history kept as-is rather than renaming, so the finding's
/// paper trail stays intact). Entries "8" (dat, N) and "9" (dat, V) are literal homophones
/// (identical surface, different category); compounding "pʰut" (5) with EITHER as non-head
/// produces the byte-identical final shape "pʰutdat". C# keeps both as distinct analyses
/// (`AssertMorphsEqual` compares gloss STRINGS, one per `Word` object, pre-deduplication); Rust's
/// `Morpher::parse_word` used to fold them into ONE.
///
/// Root cause (traced against the live C# source, not assumed): it was **not**
/// `Word::dedup_key()` -- that function is already a faithful, narrow port of C#
/// `Word.ValueEquals`/`FreezeImpl` (Word.cs:508-546), and already recurses into `non_heads`, whose
/// nested `root_allomorph` field *would* distinguish "compounded with entry 8" from "compounded
/// with entry 9" (they're different `LexEntry`/`AllomorphId`s) -- if `non_heads` still held that
/// child `Word` by the time the key was taken. It didn't: `pg-rules/src/morph.rs`'s
/// `synth_compound_subrule` (the synthesis-side compounding-rule applier) called
/// `w.non_heads.pop()` after folding the non-head's material into the compound's `shape`, on the
/// theory that the non-head was "consumed". Cross-checking C#'s own
/// `SynthesisCompoundingRule.ApplySubrule` (cs:248-291) and `Word`'s copy constructor (Word.cs:105)
/// shows C# does the opposite: `_nonHeadApps` is cloned forward and **never has an entry removed** --
/// `MorphologicalRuleApplied` (Word.cs:411-429, decrement at 417-418) only moves the separate
/// `_nonHeadAppIndex` pointer backward on confirmation (already faithfully ported as
/// `non_head_app_index -= 1` in `pg-rules/src/stratum.rs`'s `guided_synth`). So the consumed
/// non-head was meant to remain in the list as permanent history for exactly this kind of dedup
/// disambiguation; the `pop()` erased it. Fix: delete the `pop()` (see `morph.rs`'s comment at that
/// site for the full trace). `dedup_key()` itself, and every one of its ~20 call sites in
/// `stratum.rs`/`morpher.rs`, is unchanged.
///
/// A second, related bug: `Word::current_non_head()` read
/// `non_heads.last()` (the physically last element) instead of C#'s index-based
/// `_nonHeadApps[_nonHeadAppIndex]` (Word.cs:453-461). Those only agree while `non_heads` never
/// holds more than one un-consumed entry beyond the confirmed ones -- true for every grammar this
/// file's tests build, but NOT guaranteed by the public API: `pg_parse::Morpher::generate_words`
/// pushes one non-head per `GenMorpheme::NonHead` with no stem-count gate (that gate is
/// analysis-side only), so two non-heads in one generation call reach `non_heads.len() == 2` today.
/// After the first compound confirms, `.last()` would incorrectly re-read the same
/// already-consumed non-head instead of the next one down the index. Fixed alongside this test by
/// making `current_non_head()` index-based; see
/// `csharp_port_generation.rs::direct_api_compounding_two_non_heads_resolve_distinct_slots` and
/// `word.rs`'s doc comment on the method.
#[test]
fn simple_rules_1_homophone_disjunction_finding() {
    let g1 = build_grammar("", "", SIMPLE_RULES_MRULES_1, "mrC", "");
    let m1 = Morpher::new(&g1, usize::MAX);
    let out1 = m1.parse_word("pʰutdat");
    assert_morphs_eq(&out1, &["5 8", "5 9"]);
    assert_eq!(root_gloss_set(&out1), BTreeSet::from(["5".to_string()]));
}

/// Ports `CompoundingRuleTests.SimpleRules` reconfiguration 2 (CompoundingRuleTests.cs:31-46):
/// nonHead+"+"+head order (reversed `Rhs`) -> the root becomes the SECOND/non-head position. C#
/// doesn't add a distinct positive homophone-disjunction case for this reconfiguration (it reuses
/// reconfiguration 1's), so there is no separate positive test here either — just the negative
/// checks, which are live.
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

/// Ports `CompoundingRuleTests.SimpleRules` reconfiguration 3 (CompoundingRuleTests.cs:48-71): a
/// V-requiring PAST prefix ("di+") commutes with compounding.
///
/// **CORRECTED DIAGNOSIS** (P3, 2026-07-09; supersedes the prior "recursive non-head analysis"
/// finding this test used to document while `#[ignore]`d): the original port MIS-READ the C#
/// reconfiguration. `CompoundingRuleTests.cs:48-71` inserts the prefix WITHOUT resetting
/// `rule1.Subrules`, so rule1 still carries reconfiguration 2's `Rhs = { CopyFromInput("nonHead"),
/// "+", CopyFromInput("head") }` (cs:31-39) -- the NON-HEAD is "pʰut" (a literal root) and the
/// affixed span "didat" is the HEAD, which simply stays as the word's shape after compounding
/// unapplication and keeps flowing through the stratum's ordinary rule cascade, where the prefix
/// rule then unapplies ("didat" -> "dat" -> root "9"). C# has NO recursive re-entry into the rule
/// cascade for a non-head: `AnalysisCompoundingRule.Apply` (cs:61-62) explicitly discards any
/// split whose non-head is not already a bare root ("for computational complexity reasons, we
/// ensure that the non-head is a root, otherwise we assume it is not a valid analysis and throw it
/// away") -- structurally the SAME direct lexicon search Rust's
/// `pg_rules::morph::resolve_non_head_roots` performs. Verified against the live C# oracle
/// (`.worktrees/parse-opt` @ `ccf750e6`, `hc.dll` batch): the old head+nonHead grammar returns `-`
/// (empty) for "pʰutdidat" in C# too, and the faithful nonHead+head grammar returns
/// `5+PAST+9|(pʰ)ut+?di+?dat` -- pinned in `rust/conformance/compounding/prefix-commute/`.
///
/// The root assertion is C#'s `AssertRootAllomorphsEquals(output, "9")` -- the HEAD root, which
/// here is the LAST morpheme of the surface-ordered join, so [`root_gloss_set`] (first-morpheme
/// heuristic, correct only for head-first compounds) cannot express it; asserted via
/// `WordAnalysis::root_morpheme_index` instead.
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

    // PARITY PIN for the OLD (mis-ported) head+nonHead grammar: the non-head span would be "didat",
    // which is not a bare root, and NEITHER engine recurses into the rule cascade to resolve it --
    // live C# oracle returns `-` (empty) for "pʰutdidat" here, and so does Rust.
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

/// Ports `CompoundingRuleTests.MorphosyntacticRules` (CompoundingRuleTests.cs:110-172), the first 2 of
/// 3 reconfigurations (unrestricted-vs-V-output category) plus the final percolation reconfiguration
/// (non-head unrestricted, head requires `pers=2`, over the `Perc0`/`Perc3` homophones).
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
    // "ssagabba": head must be V + pers=2 -> Perc0 (unspecified pers, unifies) and Perc3 (pers in
    // {2,3}, overlaps 2); non-head unrestricted -> both "39"/"ab+ba" and "40"/"abba" (V, homophones).
    let out3 = m3.parse_word("ssagabba");
    assert_morphs_eq(&out3, &["Perc0 39", "Perc0 40", "Perc3 39", "Perc3 40"]);
    assert_eq!(
        root_gloss_set(&out3),
        BTreeSet::from(["Perc0".to_string(), "Perc3".to_string()])
    );
}

// =================================================================================================
// CompoundingRuleTests.ProdRestrictRule (W5/D-batch-3 — formerly the bucket-D scope-cut in this
// file's module doc; the productivity-restriction MPR gates themselves were already ported
// (`head_prod_restrictions_mpr`/`non_head_prod_restrictions_mpr`/`output_prod_restrictions_mpr`,
// `pg-rules/src/morph.rs`), so this port is pure test-writing, unlocked by W5's plan row).
// =================================================================================================

/// One configuration of `ProdRestrictRule`'s grammar: C#'s `rule1` (an UNRESTRICTED compounding
/// rule — no head/non-head POS requirement at all, unlike this file's other tests) over just the
/// three entries the C# test touches (`5` = "pʰut" N, `8` = "dat" N, `9` = "dat" V,
/// `HermitCrabTestBase.cs:549-554`). The C# test's per-step `MprFeatures` mutations become
/// per-configuration `ruleFeatures`/`*ProdRestrictionsMprFeatures` attributes on a fresh grammar
/// (`build_grammar_custom_lexicon` replaces the shared lexicon so the SAME entries can vary).
/// `mprLatinate` stands in for C#'s test-local `excFeat` ("Allows compounding") — the feature's
/// identity/name is never asserted, only its presence/absence on each side of each gate.
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
    // Leak each configuration's grammar: `Morpher` borrows it, six configurations per test run,
    // process-lifetime leak is bounded and irrelevant for a test binary.
    let g = Box::leak(Box::new(csharp_port_common::build_grammar_custom_lexicon(
        &mrule, "mrC1", &lexicon,
    )));
    Morpher::new(g, usize::MAX)
}

/// Ports `CompoundingRuleTests.ProdRestrictRule` (CompoundingRuleTests.cs:174-238). The C# test's
/// six sequential reconfigurations of one in-memory grammar become six grammars, in the C# step
/// order (each step's entry-side `MprFeatures` state carries over exactly as the C# mutations
/// leave it — e.g. step 4 still has the head feature on entry `5`, because C# only removes it in
/// step 5):
/// 1. no restrictions — parses as C# does: both dat homophones, {"5 8", "5 9"} (P4, 2026-07-09:
///    previously pinned at the known-collapsed {"5 8"} only, tracking the
///    [`simple_rules_1_homophone_disjunction_finding`] engine finding this step shared — now fixed,
///    see that test's doc comment for the root cause and repair).
/// 2. `headProdRestrictionsMprFeatures` set, no entry carries the feature — no parse.
/// 3. + entry `5` (the head root) carries it — parses again, both homophones: {"5 8", "5 9"}.
/// 4. restriction moved to `nonHeadProdRestrictionsMprFeatures` (entry `5` still carries the
///    now-irrelevant feature) — no parse (neither dat entry carries it).
/// 5. feature moved from entry `5` to entry `8` — parses as {"5 8"} ONLY, matching C# exactly
///    (the "5 9" split dies: entry `9` doesn't carry the feature, pinning that the gate is
///    per-ENTRY, not per-shape).
/// 6. also `outputProdRestrictionsMprFeatures` — still parses {"5 8"} (the output feature is added
///    to the produced word's MPR set, never a parse-blocking input gate; C# additionally asserts
///    the output set contents on the in-memory rule object, which has no parse surface here).
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

// =================================================================================================
// G11 gate: `Morpher::with_max_stem_count` (previously a hardcoded `2` with no public knob to raise
// it). Ports `CompoundingRuleTests.SimpleRules`' final reconfiguration (cs:76-108), the one this
// file's module doc used to note as omitted for exactly this reason.
// =================================================================================================

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

/// Ports `CompoundingRuleTests.SimpleRules` cs:76-90: ONE self-recursive compounding rule
/// (`rule1.MaxApplicationCount = 2`, head+"+"+nonHead order — the same shape as
/// [`SIMPLE_RULES_MRULES_1`]) over a genuine 3-root word, `morpher = new Morpher(...) { MaxStemCount
/// = 3 }`. Analysis: outer split head="pʰutdat"/nonHead="pip"(41, a bare root), then rule1
/// self-applies a SECOND time on the head (`NonHeadCount+1 == 2 < MaxStemCount(3)`, and rule1's own
/// `MaxApplicationCount == 2` is not yet exceeded) splitting head="pʰut"(5)/nonHead="dat"(8 or 9) --
/// exactly C#'s `MaxStemCount` depth gate letting a 2nd compounding-rule unapplication through.
/// Root stays "5" (the ultimate head chain), so [`root_gloss_set`]'s first-morpheme heuristic
/// applies here (head-first order throughout).
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

/// Ports `CompoundingRuleTests.SimpleRules` cs:92-108: TWO compounding rules, each with the DTD
/// default `MaxApplicationCount == 1` (`rule1.MaxApplicationCount` reset back to 1 at cs:92; `rule2`
/// never sets it) -- `rule1` head+"+"+nonHead order, `rule2` nonHead+"+"+head order, both active in
/// one (unordered) stratum. Same 3-root word, same `MaxStemCount = 3`: this time the 2 splits come
/// from two DIFFERENT rules (each individually still capped at 1 application), rather than one rule
/// re-entering itself. `AssertRootAllomorphsEquals(output, "8", "9")` (cs:108): the root is now
/// `rule1`'s head position at the INNER split ("dat"), not the outer word's first morpheme --
/// [`root_gloss_set`]'s first-morpheme heuristic does not apply here (same reason
/// [`simple_rules_3_prefix_commutes_with_compounding`] uses `root_morpheme_index` instead), so this
/// asserts via that index directly, exactly as that test does.
///
/// Note: `AssertMorphsEqual`/`AssertRootAllomorphsEquals` are both defined over a C# `HashSet`/
/// `.Distinct()` (`HermitCrabTestBase.cs:869-887`, `CompoundingRuleTests.cs:241-244`) -- set
/// membership only, not raw analysis *count* -- matching [`assert_morphs_eq`]/this test's own
/// root-set check, both of which dedupe the same way. Rust may (and empirically does) surface the
/// same final compound via more than one derivation history when two distinct rules can each supply
/// either split point in an unordered cascade; that duplication is exactly as C#-faithful as the
/// count-blind assertions it is checked against.
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
