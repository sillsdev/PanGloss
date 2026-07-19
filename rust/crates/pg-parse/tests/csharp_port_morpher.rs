//! Ports `MorpherTests` (parse-opt: `tests/SIL.Machine.Morphology.HermitCrab.Tests/MorpherTests.cs`)
//! bucket-B/C rows per `rust/parity-out/audit/phase2/D-test-coverage-map.md` §3.
//!
//! `AnalyzeWord_SingleThreaded_MatchesParallel` (bucket A, already `batch_determinism.rs`) and the
//! `WordAnalysis`/`GenerateWords` object-API tests (now ported at `csharp_port_generation.rs`, W7/
//! W11 batch-7) are out of scope here.
//!
//! **Bucket E scope notes (W11 doc task; also recorded in `D-test-coverage-map.md` §3/§6, which
//! predates this file's tracked existence and is itself gitignored/regenerable) -- never ported,
//! one line each:**
//! - `TestMatchNodesWithPattern` -- ported (P11 chunk 4) as `pg-parse/src/guess.rs`'s own unit
//!   tests, once the guesser API landed (see the next bullet); no longer "no Rust analog needed".
//! - `EnableLexicalGating_MatchesDisabled_SimpleAffixGrammar` -- FieldWorks-only `EnableLexicalGating`
//!   heuristic on/off equivalence; no "lexical gate" concept exists in the Rust engine.
//! - `IsEdgeStripperQualified_ReturnsFalse_ForReduplication` / `..._ForInfixation` -- same
//!   `GrammarAnalyzer` heuristic (disqualifies an edge-stripper optimization for these two rule
//!   shapes); no Rust analog.
//! - `XmlLanguageSerializationTests.RoundTripXml` -- C# `XmlLanguageLoader.Load` ->
//!   `XmlLanguageWriter.Save` byte-identity; Rust has no XML *writer* (load-only), so there is
//!   nothing to round-trip against. Not a 1:1 gap by design.
//! - `AnalyzeWord_CanGuess_ReturnsCorrectAnalysis` -- PORTED (P11 chunk 5, "PORT IT" decided
//!   2026-07-10) as `guesser_gate.rs::analyze_word_can_guess_returns_correct_analysis`, against a
//!   hand-transcribed XML fixture (no C# CLI `--guess` surface exists to oracle-generate a TSV --
//!   see `rust/docs/p11-guesser-api-design.md` §6's open question #2 -- so this is verified
//!   directly against the C# unit test's own literal expected values, the same pattern P9's
//!   Generation API used).
//!
//! **Architecture substitution for the 3 thread/memo tests** (`AnalyzeWord_ConcurrentRepeatedParsing_
//! IsDeterministic`, `ParseWord_SingleThreaded_MatchesParallel_With{Compounding,AffixTemplate}`): C#
//! compares `maxDegreeOfParallelism=1` against the default (`Environment.ProcessorCount`) parallel
//! cascade -- intra-word rule-cascade parallelism that phase 1 of this port explicitly cut (plan
//! rust-conversion.md §7: "intra-word parallelism: cut in phase 1, stays cut"). `pg_parse::Morpher`
//! has no parallel-cascade mode to compare against. The closest Rust analog carrying the same
//! *intent* -- "two different execution strategies over the same rule-cascade machinery must produce
//! identical results" -- is `Morpher::with_memo(bool)`: memo-on (the production default, replaying
//! memoized subtrees via `Word::ReplayOnto`) vs memo-off (recomputing every branch from scratch). This
//! is the exact property `pg-rules/tests/memo_gate.rs` already exercises generically; these 3 tests
//! port the C# tests' *specific grammars* (compounding-commutes-with-a-prefix; two-prefixes-commute-
//! with-a-template-slot) into that comparison, which is what each C# test's own doc comment says the
//! grammar is *for* (forcing a re-arrival at an equal cascade state so the replay path is genuinely
//! exercised, not vacuously skipped).

mod csharp_port_common;
use csharp_port_common::{
    assert_empty, assert_morphs_eq, build_grammar, build_grammar_cooccurrence, build_grammar_linear,
};
use pg_parse::Morpher;
use std::collections::BTreeSet;

/// Ports `MorpherTests.AnalyzeWord_CannotAnalyze_ReturnsEmptyEnumerable` (MorpherTests.cs:101-125).
/// Bucket C: the negative "no analyses" path is exercised implicitly all over the suite but never
/// named to this exact scenario (a well-formed grammar, a word that simply doesn't parse).
#[test]
fn analyze_word_cannot_analyze_returns_empty_enumerable() {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrEd", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("sagd"), &["32 PAST"]); // sanity: the grammar does parse *something*
    assert_empty(&m.parse_word("sagt")); // C#'s actual negative case: "sagt" has no valid analysis
}

/// Ports `MorpherTests.AnalyzeWord_CanAnalyzeLinear_ReturnsCorrectAnalysis` (MorpherTests.cs:42-99).
/// `MorphologicalRuleOrder.Linear` (as opposed to every other ported test's `Unordered`) plus an
/// unconditional t->d phonological neutralization rule so unapplying it from surface "sagd" creates a
/// dead-end sibling candidate ("sagt", reverting the *d* back to *t* -- this candidate has no boundary
/// before its final consonant, so neither suffix's `+d`/`+t`-anchored pattern can unapply from it) --
/// the analysis must still recover the live candidate (root "32" + PAST) despite Linear order and the
/// irrelevant N-requiring `t_suffix` rule sharing the stratum ("shouldn't block" the V analysis).
#[test]
fn analyze_word_can_analyze_linear_returns_correct_analysis() {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrT" requiredPartsOfSpeech="posN"><Name>t_suffix</Name><MorphemeId>PLURAL</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subT">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+t</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let prules = r#"
      <PhonologicalRule id="pr1"><Name>rule1</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncTSeg" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule><PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncDSeg" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    "#;
    let g = build_grammar_linear(prules, "pr1", mrules, "mrEd mrT");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("sagd"), &["32 PAST"]);
}

/// Ports `MorpherTests.AnalyzeWord_ConcurrentRepeatedParsing_IsDeterministic` (MorpherTests.cs:496-555)
/// -- see the module doc for the memo-on/memo-off substitution rationale. Same grammar as the C# test
/// (one V-requiring `ed_suffix`); same word list; the property under test is "two different execution
/// strategies over the same rule machinery agree", checked once per word rather than 50x250 times
/// (Rust's cascade has no thread-interleaving nondeterminism to hunt for -- the memo/no-memo split is
/// a data-flow difference, not a race, so repetition adds no signal here).
#[test]
fn analyze_word_concurrent_repeated_parsing_is_deterministic() {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrEd", "");
    let memo_on = Morpher::new(&g, usize::MAX).with_memo(true);
    let memo_off = Morpher::new(&g, usize::MAX).with_memo(false);
    for word in ["sagd", "sag", "tag", "tagd", "gag", "xyzzy"] {
        let a: BTreeSet<String> = memo_on
            .parse_word(word)
            .analyses
            .into_iter()
            .map(|(m, s)| format!("{m}|{s}"))
            .collect();
        let b: BTreeSet<String> = memo_off
            .parse_word(word)
            .analyses
            .into_iter()
            .map(|(m, s)| format!("{m}|{s}"))
            .collect();
        assert_eq!(a, b, "memo-on vs memo-off disagree for {word:?}");
    }
}

/// Ports `MorpherTests.ParseWord_SingleThreaded_MatchesParallel_WithCompounding` (MorpherTests.cs:
/// 557-624) -- see module doc. A compounding rule commutes with a V->V "PAST"-tense prefix, both
/// ordinary (non-template) rules in the same Unordered cascade, forcing the memoized cascade to
/// revisit an equal analysis state via different arrival orders (di-then-compound vs
/// compound-then-di).
#[test]
fn parse_word_single_threaded_matches_parallel_with_compounding() {
    let mrules = r#"
      <CompoundingRule id="mrCompound">
        <Name>rule1</Name>
        <CompoundingSubrules>
          <CompoundingSubrule>
            <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
            <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
          </CompoundingSubrule>
        </CompoundingSubrules>
      </CompoundingRule>
      <MorphologicalRule id="mrPrefix" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>prefix</Name><MorphemeId>PAST</MorphemeId>
        <OutputHeadFeatures><FeatureValue feature="featTense" symbolValues="symPast" /></OutputHeadFeatures>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subPrefix">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>di+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let g = build_grammar("", "", mrules, "mrCompound mrPrefix", "");
    let memo_on = Morpher::new(&g, usize::MAX).with_memo(true);
    let memo_off = Morpher::new(&g, usize::MAX).with_memo(false);
    for word in ["pʰutdidat", "pʰutdat"] {
        let a: BTreeSet<String> = memo_on
            .parse_word(word)
            .analyses
            .into_iter()
            .map(|(m, s)| format!("{m}|{s}"))
            .collect();
        let b: BTreeSet<String> = memo_off
            .parse_word(word)
            .analyses
            .into_iter()
            .map(|(m, s)| format!("{m}|{s}"))
            .collect();
        assert_eq!(a, b, "memo-on vs memo-off disagree for {word:?}");
    }
}

/// Ports `MorpherTests.ParseWord_SingleThreaded_MatchesParallel_WithAffixTemplate` (MorpherTests.cs:
/// 626-736) -- see module doc. TWO commuting prefixes (`di-`/`gu-`) plus an optional-slot template
/// suffix: unapplying di-then-gu vs gu-then-di reaches the same state via different trail orders,
/// exercising the template-battery memo specifically (one commuting prefix alone is not enough, per
/// the C# comment -- a single rule can only unapply once, so no re-arrival is possible).
#[test]
fn parse_word_single_threaded_matches_parallel_with_affix_template() {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>template_ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrDi" requiredPartsOfSpeech="posV"><Name>template_di_prefix</Name><MorphemeId>DI</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subDi">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>di+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrGu" requiredPartsOfSpeech="posV"><Name>template_ku_prefix</Name><MorphemeId>KU</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subGu">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>gu+</PhoneticShape></InsertSegments><CopyFromInput index="1" /></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let templates = r#"
      <AffixTemplate requiredPartsOfSpeech="posV"><Name>verb_template</Name><Slot morphologicalRules="mrEd" optional="true"><Name>Sl1</Name></Slot></AffixTemplate>
    "#;
    let g = build_grammar("", "", mrules, "mrDi mrGu", templates);
    let memo_on = Morpher::new(&g, usize::MAX).with_memo(true);
    let memo_off = Morpher::new(&g, usize::MAX).with_memo(false);
    for word in ["digusagd", "disagd", "gusagd", "sagd", "sag"] {
        let a: BTreeSet<String> = memo_on
            .parse_word(word)
            .analyses
            .into_iter()
            .map(|(m, s)| format!("{m}|{s}"))
            .collect();
        let b: BTreeSet<String> = memo_off
            .parse_word(word)
            .analyses
            .into_iter()
            .map(|(m, s)| format!("{m}|{s}"))
            .collect();
        assert_eq!(a, b, "memo-on vs memo-off disagree for {word:?}");
    }
}

/// The shared `mrEd` PAST-suffix rule (`+d`) both W6 co-occurrence tests below attach their
/// `AllomorphCoOccurrenceRule`/`MorphemeCoOccurrenceRule` to -- identical shape to every other
/// ported test's `mrEd` (see e.g. `analyze_word_cannot_analyze_returns_empty_enumerable` above).
const ED_SUFFIX_MRULE: &str = r#"
  <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
    <MorphologicalSubrules>
      <MorphologicalSubrule id="subEd">
        <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
        <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
      </MorphologicalSubrule>
    </MorphologicalSubrules>
  </MorphologicalRule>
"#;

/// The C# test's extra `AddEntry("dEnclitic", ...)` root (MorpherTests.cs:176-178,232):
/// FLEx-emitted clitics occur as both a stem and an affix, so LT-22156's fix lets a co-occurrence
/// rule reference either form without the "other" morph's mere existence (never actually
/// co-occurring in the tested word) breaking anything -- this entry exists solely to be a legal,
/// always-absent `otherAllomorphs`/`otherMorphemes` reference, exactly like the `eOther`/`aOther`
/// decoy entry in `rust/conformance/cooccurrence/and-semantics-pin`.
const D_ENCLITIC_ENTRY: &str = r#"
  <LexicalEntry id="eDEnclitic" partOfSpeech="posV">
    <Allomorphs><Allomorph id="aDEnclitic"><PhoneticShape>d</PhoneticShape></Allomorph></Allomorphs>
    <MorphemeId>dEnclitic</MorphemeId>
  </LexicalEntry>
"#;

/// Ports `MorpherTests.AnalyzeWord_CannotAnalyzeDueToAllomorphCooccurenceFailure_
/// ReturnsEmptyEnumerable` (MorpherTests.cs:127-184). An `AllomorphCoOccurrenceRule` on sag's root
/// allomorph (`a32`) excluding the PAST suffix's allomorph (`subEd`) blocks "sagd" from analyzing
/// at all -- pins the W6 evaluation gate (`pg-rules/src/validity.rs::allomorph_co_occurrence_ok`)
/// at the granularity the C# test exercises: one specific allomorph excluding one specific other
/// allomorph, `adjacency="anywhere"`, `type="exclude"`.
#[test]
fn analyze_word_cannot_analyze_due_to_allomorph_cooccurence_failure_returns_empty_enumerable() {
    // Step 1 (MorpherTests.cs:128-161): the single exclusion rule alone already rejects "sagd".
    let coo1 = r#"
      <AllomorphCoOccurrenceRules>
        <AllomorphCoOccurrenceRule type="exclude" primaryAllomorph="a32" otherAllomorphs="subEd" adjacency="anywhere" />
      </AllomorphCoOccurrenceRules>
    "#;
    let g1 = build_grammar_cooccurrence(ED_SUFFIX_MRULE, "mrEd", "", coo1);
    assert_empty(&Morpher::new(&g1, usize::MAX).parse_word("sagd"));

    // Step 2 (MorpherTests.cs:163-183, the LT-22156/#311 pin): adding a SECOND rule that excludes
    // `dEnclitic` -- which never actually co-occurs with sag in this grammar, so its own
    // `IsWordValid` is trivially satisfied -- must NOT rescue "sagd". Pre-90dcee64 C# used
    // `.Any(IsWordValid)`, under which this second (satisfied) rule alone would have made the
    // whole check pass; post-fix, every attached rule must pass, so rule1's failure still rejects
    // the word. Still empty.
    let coo2 = r#"
      <AllomorphCoOccurrenceRules>
        <AllomorphCoOccurrenceRule type="exclude" primaryAllomorph="a32" otherAllomorphs="subEd" adjacency="anywhere" />
        <AllomorphCoOccurrenceRule type="exclude" primaryAllomorph="a32" otherAllomorphs="aDEnclitic" adjacency="anywhere" />
      </AllomorphCoOccurrenceRules>
    "#;
    let g2 = build_grammar_cooccurrence(ED_SUFFIX_MRULE, "mrEd", D_ENCLITIC_ENTRY, coo2);
    assert_empty(&Morpher::new(&g2, usize::MAX).parse_word("sagd"));
}

/// Ports `MorpherTests.AnalyzeWord_CannotAnalyzeDueToMorphemeCooccurenceFailure_
/// ReturnsEmptyEnumerable` (MorpherTests.cs:186-239). Identical shape to the allomorph-level test
/// above, but the exclusion is a `MorphemeCoOccurrenceRule` on sag's MORPHEME (`e32`) excluding the
/// PAST rule's morpheme (`mrEd`'s own xml id, not its subrule) -- pins
/// `pg-rules/src/validity.rs::morpheme_co_occurrence_ok` at the morpheme granularity.
#[test]
fn analyze_word_cannot_analyze_due_to_morpheme_cooccurence_failure_returns_empty_enumerable() {
    let coo1 = r#"
      <MorphemeCoOccurrenceRules>
        <MorphemeCoOccurrenceRule type="exclude" primaryMorpheme="e32" otherMorphemes="mrEd" adjacency="anywhere" />
      </MorphemeCoOccurrenceRules>
    "#;
    let g1 = build_grammar_cooccurrence(ED_SUFFIX_MRULE, "mrEd", "", coo1);
    assert_empty(&Morpher::new(&g1, usize::MAX).parse_word("sagd"));

    // The same LT-22156/#311 AND-semantics re-check as the allomorph-level test, at morpheme
    // granularity: a second, trivially-satisfied rule excluding `dEnclitic`'s morpheme must not
    // rescue "sagd".
    let coo2 = r#"
      <MorphemeCoOccurrenceRules>
        <MorphemeCoOccurrenceRule type="exclude" primaryMorpheme="e32" otherMorphemes="mrEd" adjacency="anywhere" />
        <MorphemeCoOccurrenceRule type="exclude" primaryMorpheme="e32" otherMorphemes="eDEnclitic" adjacency="anywhere" />
      </MorphemeCoOccurrenceRules>
    "#;
    let g2 = build_grammar_cooccurrence(ED_SUFFIX_MRULE, "mrEd", D_ENCLITIC_ENTRY, coo2);
    assert_empty(&Morpher::new(&g2, usize::MAX).parse_word("sagd"));
}
