//! `emit_underlying_templated`'s bounded compound loop now shares `build_compound_chain` with the `SurfaceProbed`/`FomaProposer` path instead of hardcoding one extra root level.
//! See docs/research/pg-foma-emit-underlying-templated-recursive-compound-chain-design-notes.md.

use std::time::{Duration, Instant};

use foma::apply::apply_init;
use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;

use pg_foma::emit::{emit_underlying_templated, FomaTier};
use pg_foma::replace::SegAlphabet;
use pg_foma::tags::{self, Candidate};
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

/// One isolated `CompoundingRule` at `multipleApplication = max_apps`, so `max_depth = 1 + max_apps`; deliberately not wrapped in any `<AffixTemplate>` to exercise the template-less `TLCmp` chain.
fn small_bound_grammar_xml(max_apps: u32, roots: &[&str]) -> String {
    let mut entries = String::new();
    for (i, root) in roots.iter().enumerate() {
        entries.push_str(&format!(
            r#"<LexicalEntry id="eRoot{i}" partOfSpeech="posRoot">
                 <Allomorphs><Allomorph id="aRoot{i}"><PhoneticShape>{root}</PhoneticShape></Allomorph></Allomorphs>
                 <MorphemeId>ROOT{i}</MorphemeId>
               </LexicalEntry>"#
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>TemplatedCompoundChainFixture</Name>
  <PartsOfSpeech><PartOfSpeech id="posRoot"><Name>root</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ci"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="co"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cu"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cd"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cf"><Representations><Representation>f</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cg"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cl"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cm"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cn"><Representations><Representation>n</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cs"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ct"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="linear" morphologicalRules="cr1">
      <Name>Main</Name>
      <MorphologicalRuleDefinitions>
        <CompoundingRule id="cr1" multipleApplication="{max_apps}" headPartsOfSpeech="posRoot" nonHeadPartsOfSpeech="posRoot" outputPartOfSpeech="posRoot">
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
      </MorphologicalRuleDefinitions>
      <LexicalEntries>{entries}</LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#
    )
}

/// Emits `g` via the templated path, foma-compiles the lexc, and returns an `apply_init` handle plus the `SegAlphabet` needed to encode query words — factored out since both tests below need it.
fn compile_templated(g: &Grammar) -> (SegAlphabet<'_>, foma::types::Fsm) {
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let result = emit_underlying_templated(g, &alphabet, None);
    assert!(
        !matches!(result.report.tier, FomaTier::Unsupported { .. }),
        "fixture must compile via the templated path: {:?}",
        result.report.tier
    );
    let opts = FomaOptions::default();
    let net = fsm_lexc_parse_string(&opts, None, &result.lexc_source).unwrap_or_else(|| {
        panic!(
            "templated lexc failed to foma-compile:\n{}",
            result.lexc_source
        )
    });
    (alphabet, net)
}

/// Every candidate `apply_up` decodes for `word`: the templated path's own analogue of `FomaProposer::propose`, bounded by a raw-result cap and wall-clock ceiling.
fn propose_templated(
    alphabet: &SegAlphabet<'_>,
    net: &foma::types::Fsm,
    word: &str,
) -> Vec<Candidate> {
    const RAW_CAP: usize = 20_000;
    let Some(query) = alphabet.encode_query(word) else {
        return Vec::new();
    };
    let mut handle = apply_init(net);
    let mut out = Vec::new();
    let t0 = Instant::now();
    let mut raw_n = 0usize;
    for s in handle.up(&query) {
        raw_n += 1;
        if let Some(path) = tags::decode_path(&s) {
            out.extend(tags::to_candidates(&path));
        }
        if raw_n >= RAW_CAP || t0.elapsed() > Duration::from_secs(30) {
            break;
        }
    }
    out
}

/// The load-bearing recursion-recall proof: the templated path must now propose a genuine 3-stem self-feeding compound, with a non-vacuous raised-cap containment check.
/// See docs/research/pg-foma-emit-underlying-templated-recursive-compound-chain-design-notes.md.
#[test]
fn templated_path_proposes_a_bounded_recursive_compound() {
    let roots = ["fasa", "kelu", "tibo"];
    let xml = small_bound_grammar_xml(2, &roots);
    let g = pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"));

    let max_depth = pg_foma::capability::characterize(&g)
        .compounding_details()
        .map(|d| d.max_depth)
        .max()
        .unwrap_or(0);
    assert_eq!(
        max_depth, 3,
        "isolated multipleApplication=\"2\" rule must bound at 1 + 2 = 3 stems"
    );

    // Non-vacuity precondition: default max_stem_count must still confirm zero for the 3-root word.
    let default_morpher = Morpher::new(&g, usize::MAX);
    let word = "fasakelutibo"; // 3 roots concatenated
    let default_outcome = default_morpher.parse_word_opts(word, &ParseOptions::default());
    assert_eq!(
        default_outcome.analyses.len(),
        0,
        "at the default max_stem_count (2), {word:?} must still confirm zero analyses -- a \
         containment check against this default would be vacuously true"
    );

    let raised_morpher = Morpher::new(&g, usize::MAX).with_max_stem_count(3);
    let raised_outcome = raised_morpher.parse_word_opts(word, &ParseOptions::default());
    assert_eq!(
        raised_outcome.structured.len(),
        1,
        "with max_stem_count raised to 3, {word:?} (a genuine ROOT0+ROOT1+ROOT2 self-feeding \
         compound) must confirm exactly one analysis"
    );
    let oracle_morphemes = raised_outcome.structured[0].morpheme_ids.clone();

    let (alphabet, net) = compile_templated(&g);

    // Sanity: the same network still proposes the ordinary depth-1 compounds/bare roots fine.
    for w in ["fasa", "kelu", "tibo", "fasakelu", "kelutibo"] {
        assert!(
            !propose_templated(&alphabet, &net, w).is_empty(),
            "sanity: {w:?} (bare root / depth-1 compound) must still propose at least one candidate"
        );
    }

    let candidates = propose_templated(&alphabet, &net, word);
    assert!(
        !candidates.is_empty(),
        "the templated path's depth-budgeted compound chain (task #44) must now propose at least \
         one candidate for the genuine 3-stem self-feeding compound {word:?}"
    );

    // Containment: propose must offer the EXACT morpheme sequence the raised-cap oracle confirms.
    let contained = candidates.iter().any(|c| {
        c.morphemes
            .iter()
            .map(|m| m.0)
            .eq(oracle_morphemes.iter().copied())
    });
    assert!(
        contained,
        "the templated path's proposed candidate set must CONTAIN the oracle's raised-cap analysis \
         (exact morpheme-id sequence match) -- proposed: {candidates:?}, oracle: {oracle_morphemes:?}"
    );
}

/// The depth-bound-respected gate: over-approximation is licensed up to the computed bound, never past it.
/// See docs/research/pg-foma-emit-underlying-templated-recursive-compound-chain-design-notes.md.
#[test]
fn templated_path_respects_the_depth_bound_never_proposing_k_plus_one_stems() {
    let roots = ["fasa", "kelu", "tibo"];
    let xml = small_bound_grammar_xml(1, &roots);
    let g = pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"));

    let max_depth = pg_foma::capability::characterize(&g)
        .compounding_details()
        .map(|d| d.max_depth)
        .max()
        .unwrap_or(0);
    assert_eq!(
        max_depth, 2,
        "isolated multipleApplication=\"1\" rule must bound at 1 + 1 = 2 stems"
    );

    let (alphabet, net) = compile_templated(&g);

    let word_k = "fasakelu"; // 2 roots -- within the k=2 bound.
    assert!(
        !propose_templated(&alphabet, &net, word_k).is_empty(),
        "a {k}-stem word (the computed bound itself) must be proposable: {word_k:?}",
        k = 2
    );

    let word_k_plus_one = "fasakelutibo"; // 3 roots -- one past the k=2 bound.
    let over_candidates = propose_templated(&alphabet, &net, word_k_plus_one);
    assert!(
        over_candidates.is_empty(),
        "a (k+1)-stem word must NEVER be proposed once the templated path's own compound chain \
         depth bound is k=2 -- got {over_candidates:?}"
    );
}
