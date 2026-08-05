//! Regression test for the `BoundRoot` bare-root compile-time discharge: omitting the bare-root
//! (`"#"`-continuation) lexc arc for a root lexical entry that has EXACTLY ONE allomorph and that
//! allomorph is `isBound="true"`.
//!
//! # Why this is provably safe (not a heuristic)
//! This crate's root validity check treats a word as invalid whenever a bound root allomorph is
//! the word's only allomorph (`FailureReason::BoundRoot` — C#
//! `RootAllomorph.CheckAllomorphConstraints`: "cannot be the word's only allomorph"; pinned by
//! `pg_rules`'s own `bound_root_alone_is_rejected`). A bare-root
//! candidate (the `"#"`-continuation `Root`/P6 `LEXICON` this test inspects) is BY CONSTRUCTION a
//! word consisting of exactly that one root morph and nothing else, so `distinct_count` is
//! trivially `1` for every such candidate. The gate therefore reduces, on this arc only, to
//! `def.is_bound` alone — a fact readable straight off `RootAllomorphDef::is_bound`
//! (`pg_grammar::model`, no live `Morpher` needed) whenever the owning entry has exactly one
//! allomorph (so there is no cross-allomorph free-fluctuation/disjunctive-candidate reasoning to
//! get wrong — see `RootRec::never_valid_bare`'s own doc in `pg_foma::emit`).
//!
//! # What this file proves
//! 1. The bound root's OWN bare (`"#"`) lexc line is ABSENT from the emitted `Root` lexicon.
//! 2. A FREE root's bare line is still PRESENT (the omission is per-entry, not a blanket bug).
//! 3. Recall is unchanged: the bound root's suffixed word (`bndes`) still confirms exactly like
//!    the oracle; the bare bound word (`bnd`) confirms ZERO analyses under BOTH the oracle and the
//!    FST propose-confirm pipeline, before and after this change is possible (the arc removed was
//!    always dead weight, never a live analysis) — the free root's bare word (`fre`) still
//!    confirms exactly one analysis, proving ordinary bare-root recall is untouched.
//!
//! Assertion 1 is the one that fails with the fix reverted (the old code emits `bnd`'s bare line
//! unconditionally) and passes with it applied — a regression test that actually fails without
//! the fix, not just one that happens to pass with it.

use pg_foma::composite::FomaAnalyzer;
use pg_foma::emit;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

/// Synthetic, delanguaged fixture (invented CVC roots `bnd`/`fre`, no natural-language lexemes):
/// one `posV` part of speech, one `linear` stratum, one ordinary suffix rule (`mrSuf`, "+es",
/// no `AffixTemplate` needed — same "no template wrapper" shape
/// `cover_realizational_morphology_constraints.rs` already established), two single-allomorph
/// root entries differing ONLY in `isBound`:
///  - `eBnd` / allomorph `bnd`, `isBound="true"` — the provably-dead bare-root case.
///  - `eFre` / allomorph `fre`, ordinary (unbound) — the contrast case: bare-root admission must
///    still work normally for a free root.
fn fixture_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>BareRootCompileTimeDischargeFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cd"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cf"><Representations><Representation>f</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cn"><Representations><Representation>n</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cr"><Representations><Representation>r</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cs"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1" morphologicalRuleOrder="linear" morphologicalRules="mrSuf">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrSuf" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>suf</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subSuf">
                <MorphologicalInput><PhoneticSequence id="stemSuf"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemSuf" /><InsertSegments><PhoneticShape>es</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>SUF</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eBnd" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aBnd" isBound="true"><PhoneticShape>bnd</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>BND</MorphemeId>
          </LexicalEntry>
          <LexicalEntry id="eFre" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aFre"><PhoneticShape>fre</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>FRE</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#
}

fn load() -> Grammar {
    let xml = fixture_xml();
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// The `LEXICON Root` block's own text (up to the next `LEXICON` header) — the section
/// `emit::emit`'s bare-root path (module doc, "Bare-root paths") writes every root's
/// `"#"`-continuation entry into. Slicing to just this block (rather than grepping the whole
/// `lexc_source`) avoids a false negative/positive from `bnd`/`fre`'s OTHER, non-bare occurrences
/// (e.g. the template-less section's `TLRoots` entries, continuation `TLPost`, which this change
/// deliberately leaves untouched — see `bare_admissible_roots`'s own doc).
fn root_lexicon_block(lexc_source: &str) -> &str {
    let start = lexc_source
        .find("\nLEXICON Root\n")
        .expect("emitted lexc must declare LEXICON Root");
    let after_header = start + "\nLEXICON Root\n".len();
    let rest = &lexc_source[after_header..];
    let end = rest.find("\nLEXICON ").unwrap_or(rest.len());
    &rest[..end]
}

/// A line inside `block` that mentions `surface` and ends its lexc entry on the bare accept state
/// (`# ;`) -- the exact shape `emit::emit`'s bare-root `write_root_entries(.., "#", ..)` call
/// writes (module doc's tag-tape convention: upper = tag symbol, lower = literal surface text).
fn has_bare_accept_line_for(block: &str, surface: &str) -> bool {
    block
        .lines()
        .any(|line| line.contains(surface) && line.trim_end().ends_with("# ;"))
}

#[test]
fn bound_single_allomorph_root_has_no_bare_accept_arc() {
    let g = load();
    let result = emit::emit(&g);
    assert!(
        matches!(result.report.tier, pg_foma::emit::FomaTier::Full),
        "fixture must compile to the Full tier (plain affixation, no unsupported construct): {:?}",
        result.report.tier
    );
    let root_block = root_lexicon_block(&result.lexc_source);

    // The provably-dead case: `bnd` is bound AND its entry has exactly one allomorph, so
    // `RootRec::never_valid_bare` is true for it -- its bare `"#"` line must be ABSENT. This is
    // the assertion that FAILS if the bare-root discharge (`bare_admissible_roots` filtering the
    // `write_root_entries(.., "#", ..)` call) is reverted -- the old code emits this line
    // unconditionally for every root allomorph regardless of `is_bound`.
    assert!(
        !has_bare_accept_line_for(root_block, "bnd"),
        "bound single-allomorph root 'bnd' must NOT get a bare (\"#\"-continuation) accept arc -- \
         confirm's `distinct_count == 1 && is_bound` gate (FailureReason::BoundRoot) rejects any \
         word this arc could ever propose, unconditionally; found in Root lexicon:\n{root_block}"
    );

    // Contrast: an ordinary (unbound) root's bare arc must still be present -- proves the omission
    // is specific to `bnd`'s own `is_bound` allomorph, not a blanket regression that silently
    // dropped every root's bare path.
    assert!(
        has_bare_accept_line_for(root_block, "fre"),
        "free root 'fre' must still get its ordinary bare accept arc; found in Root lexicon:\n{root_block}"
    );
}

#[test]
fn bound_root_recall_is_unaffected_by_omitting_its_dead_bare_arc() {
    let g = load();
    let mut analyzer = FomaAnalyzer::new(&g).expect(
        "fixture must compile: plain affixation, one linear stratum, no templates/compounding",
    );
    let morpher = Morpher::new(&g, usize::MAX);

    // Positive control: the bound root WITH its suffix must still parse and confirm identically
    // to the oracle -- the fix only ever removes the BARE arc, never the root's OTHER
    // continuations (TLPfx/TLRoots/TLPost/TLSfx0), so ordinary derived-word recall is untouched.
    let bndes_oracle = morpher.parse_word_opts("bndes", &ParseOptions::default());
    let bndes_outcome = analyzer.analyze_word("bndes");
    assert!(
        !bndes_oracle.structured.is_empty(),
        "precondition: 'bndes' (bnd+SUF) must be a real oracle analysis"
    );
    assert_eq!(
        bndes_outcome.confirmed,
        bndes_oracle.structured.len(),
        "'bndes' confirmed count must equal the oracle's exact analysis count -- suffixed-word \
         recall for a bound root must be unaffected by omitting its dead bare arc"
    );

    // The dead case itself: bare 'bnd' must confirm ZERO analyses under the oracle (ground truth:
    // a bound root can never stand alone) -- proving the arc this change removes was NEVER a live
    // analysis to begin with, so removing it costs nothing.
    let bnd_oracle = morpher.parse_word_opts("bnd", &ParseOptions::default());
    assert!(
        bnd_oracle.structured.is_empty(),
        "precondition: bare 'bnd' must have NO valid oracle analysis at all (bound root)"
    );
    let bnd_outcome = analyzer.analyze_word("bnd");
    assert_eq!(
        bnd_outcome.confirmed, 0,
        "bare 'bnd' must confirm zero analyses under the FST propose-confirm pipeline too, \
         matching the oracle exactly"
    );

    // Contrast: bare 'fre' (the free, unbound root) must still confirm exactly one analysis under
    // both the oracle and the FST pipeline -- ordinary bare-root recall is untouched.
    let fre_oracle = morpher.parse_word_opts("fre", &ParseOptions::default());
    assert_eq!(
        fre_oracle.structured.len(),
        1,
        "precondition: bare 'fre' must have exactly one oracle analysis (ordinary free root)"
    );
    let fre_outcome = analyzer.analyze_word("fre");
    assert_eq!(
        fre_outcome.confirmed, 1,
        "bare 'fre' must still confirm exactly one analysis -- free-root bare recall must be \
         completely unaffected by this change"
    );
}
