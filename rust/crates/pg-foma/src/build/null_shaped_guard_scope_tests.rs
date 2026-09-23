//! What `reroute_null_shaped_affix_chains` does and does NOT cover, asserted rather than left
//! to its doc comment -- because the gap between the two is exactly how the epsilon-loop defect
//! regressed a second time (that function's own "This function is NAME-SCOPED" section).
//!
//! The claim pinned here: on a grammar whose bounded compound loop is genuinely emitted and
//! genuinely carries a null-shaped prefix allomorph, this rewriter is a **complete no-op on every
//! compound-loop lexicon** -- every `UCmp*` lexicon body comes out of it byte-identical -- while
//! the top-level `PrefixChain` body it DOES know by name is genuinely rewritten. Both halves
//! matter: the first is what makes `crate::uflexc`'s emission-time discipline the load-bearing
//! mechanism for the compound levels (so nobody "simplifies" it away believing this rewriter has
//! them covered), and the second is what proves the fixture reaches this rewriter at all rather
//! than the whole test passing because nothing matched anywhere.

use super::*;
use crate::replace::SegAlphabet;
use crate::uflexc::emit_underlying;

/// One unrestricted `CompoundingRule` (so the compound levels are really emitted), one ordinary prefix, one all-`Boundary` (`^0+`) prefix.
const COMPOUND_NULL_PREFIX_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>BuildCompoundNullShapedPrefixFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
        <BoundaryDefinition id="cNull"><Representations><Representation>^0</Representation><Representation>*0</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="cr1 mrRealPfx mrNullPfx">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <CompoundingRule id="cr1">
            <Name>Compound</Name>
            <CompoundingSubrules>
              <CompoundingSubrule>
                <HeadMorphologicalInput>
                  <PhoneticSequence id="h0"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                </HeadMorphologicalInput>
                <NonHeadMorphologicalInput>
                  <PhoneticSequence id="n0"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                </NonHeadMorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="h0" />
                  <CopyFromInput index="n0" />
                </MorphologicalOutput>
              </CompoundingSubrule>
            </CompoundingSubrules>
          </CompoundingRule>
          <MorphologicalRule id="mrRealPfx" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>RealPrefix</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="mrRealPfxS">
                <MorphologicalInput>
                  <PhoneticSequence id="stem1">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments>
                  <CopyFromInput index="stem1" />
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <Gloss>RPX</Gloss>
          </MorphologicalRule>
          <MorphologicalRule id="mrNullPfx" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>NullPrefix</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="mrNullPfxS">
                <MorphologicalInput>
                  <PhoneticSequence id="stem2">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <InsertSegments><PhoneticShape>^0+</PhoneticShape></InsertSegments>
                  <CopyFromInput index="stem2" />
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <Gloss>NPX</Gloss>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="root1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="root1a0"><PhoneticShape>s</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>rootS</Gloss>
          </LexicalEntry>
          <LexicalEntry id="root2" partOfSpeech="posV">
            <Allomorphs><Allomorph id="root2a0"><PhoneticShape>t</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>rootT</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// `lexc_source` split into `(lexicon name, body lines)` in emission order.
fn bodies(lexc_source: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for line in lexc_source.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("LEXICON ") {
            out.push((name.trim().to_string(), Vec::new()));
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if let Some(last) = out.last_mut() {
            last.1.push(trimmed.to_string());
        }
    }
    out
}

#[test]
fn reroute_is_a_no_op_on_the_compound_loop_lexicons() {
    let g = pg_grammar::load(COMPOUND_NULL_PREFIX_XML)
        .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let before = emit_underlying(&g, &alphabet)
        .expect("uflexc emission must succeed")
        .lexc_source;
    let after = reroute_null_shaped_affix_chains(&before, alphabet.table(), &alphabet);

    let before_bodies = bodies(&before);
    let after_bodies = bodies(&after);

    // Non-vacuity: the compound loop really is emitted here.
    let compound: Vec<&(String, Vec<String>)> = before_bodies
        .iter()
        .filter(|(name, _)| name.starts_with("UCmp"))
        .collect();
    assert!(
        !compound.is_empty(),
        "fixture emitted no UCmp* lexicon -- the compound loop did not run, so this test would \
         be vacuous:\n{before}"
    );

    // Half 1: every compound-loop lexicon body is byte-identical across the rewrite.
    for (name, body) in &before_bodies {
        if !name.starts_with("UCmp") {
            continue;
        }
        let after_body = after_bodies
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, b)| b.clone())
            .unwrap_or_else(|| {
                panic!("`reroute_null_shaped_affix_chains` dropped lexicon `{name}`")
            });
        assert_eq!(
            body, &after_body,
            "`reroute_null_shaped_affix_chains` changed compound-loop lexicon `{name}`. Its \
             `match` only names `PrefixChain`/`SuffixChain`, so if this ever becomes true the \
             guard's scope was widened -- and `crate::uflexc`'s emission-time discipline plus \
             this rewrite would then BOTH be acting on the same lines. Reconcile them; do not \
             just update this assertion."
        );
    }

    // Proves the fixture reaches this rewriter at all, rather than the check above passing vacuously.
    let prefix_chain_before = before_bodies
        .iter()
        .find(|(n, _)| n == "PrefixChain")
        .expect("uflexc always emits a PrefixChain lexicon");
    let prefix_chain_after = after_bodies
        .iter()
        .find(|(n, _)| n == "PrefixChain")
        .expect("the rewrite preserves the PrefixChain header");
    assert_ne!(
        prefix_chain_before.1, prefix_chain_after.1,
        "the top-level `PrefixChain` body was NOT rewritten, so this fixture is not reaching \
         `reroute_null_shaped_affix_chains`'s in-scope branch at all and half 1 above proves \
         nothing:\n{before}"
    );
    assert!(
        after_bodies
            .iter()
            .any(|(n, _)| n == "PrefixOrRootAfterNull"),
        "the rewrite did not append its `PrefixOrRootAfterNull` lexicon, so it did not treat \
         this fixture's `^0+` prefix as null-shaped:\n{after}"
    );
}
