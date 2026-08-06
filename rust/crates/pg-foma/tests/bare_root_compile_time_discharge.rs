//! Pins the `BoundRoot` bare-root compile-time discharge and why omitting the arc is provably
//! safe: see `docs/research/pg-foma-bare-root-compile-time-discharge.md`.

use pg_foma::composite::FomaAnalyzer;
use pg_foma::emit;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

/// Synthetic fixture, invented CVC roots: `eBnd`/`bnd` is bound (the provably-dead bare-root case), `eFre`/`fre` is an ordinary free root (the contrast case).
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

/// The `LEXICON Root` block's own text, up to the next `LEXICON` header: slicing to just this block avoids a false match from `bnd`/`fre`'s other, non-bare occurrences elsewhere in the emitted lexc.
fn root_lexicon_block(lexc_source: &str) -> &str {
    let start = lexc_source
        .find("\nLEXICON Root\n")
        .expect("emitted lexc must declare LEXICON Root");
    let after_header = start + "\nLEXICON Root\n".len();
    let rest = &lexc_source[after_header..];
    let end = rest.find("\nLEXICON ").unwrap_or(rest.len());
    &rest[..end]
}

/// A line inside `block` that mentions `surface` and ends its lexc entry on the bare accept state (`# ;`).
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

    // The provably-dead case: `bnd`'s bare `"#"` line must be absent; fails if the discharge is reverted.
    assert!(
        !has_bare_accept_line_for(root_block, "bnd"),
        "bound single-allomorph root 'bnd' must NOT get a bare (\"#\"-continuation) accept arc -- \
         confirm's `distinct_count == 1 && is_bound` gate (FailureReason::BoundRoot) rejects any \
         word this arc could ever propose, unconditionally; found in Root lexicon:\n{root_block}"
    );

    // Contrast: an ordinary (unbound) root's bare arc must still be present, proving the omission is specific to `bnd`, not a blanket regression.
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

    // Positive control: the bound root with its suffix must still confirm identically to the oracle -- the fix removes only the bare arc, never the root's other continuations.
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

    // The dead case itself: bare 'bnd' must confirm zero analyses under the oracle, proving the removed arc was never a live analysis.
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

    // Contrast: bare 'fre' must still confirm exactly one analysis under both paths -- ordinary bare-root recall is untouched.
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
