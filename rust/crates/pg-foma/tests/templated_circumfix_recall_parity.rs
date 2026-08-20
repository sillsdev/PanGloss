//! Recall-parity gate for the templated backend's circumfix shape, synthetic grammars only (none of the seven ratchet-pinned words) -- pins the mechanism via direct `FomaProposer::propose`, not `common::gate_template::recall_reachable` (never exercised against a templated-compiled network).

mod common;

use std::time::Duration;

use pg_featstruct::FeatureStruct;
use pg_foma::templated_compile::compile_templated_morphotactics;
use pg_grammar::model::{AllomorphOwner, Grammar, LexEntryId, MorphRuleDef};
use pg_grammar_gen::oracle::{sweep, OracleOpts};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};
use pg_parse::{GenMorpheme, Morpher, ParseOptions};

use common::gate_template::{entry_id_of, mrule_id_of};

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// Every `(morpheme id sequence, root index)` identity the oracle finds for `surface`, re-derived by re-parsing -- never a hand-derived guess at order.
fn oracle_identities(morpher: &Morpher, surface: &str) -> Vec<(Vec<u32>, i32)> {
    let outcome = morpher.parse_word_opts(surface, &ParseOptions::default());
    outcome
        .structured
        .iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect()
}

/// Asserts the templated backend's `propose(surface)` offers at least one oracle identity for `surface`, at the same `(morphemes, root_index)` key `word_proposal_containment` compares.
fn assert_templated_recall(g: &Grammar, morpher: &Morpher, surface: &str) {
    let identities = oracle_identities(morpher, surface);
    assert!(
        !identities.is_empty(),
        "oracle word {surface:?} must parse against its own grammar -- oracle/parser \
         inconsistency, not a recall question"
    );
    let compiled = compile_templated_morphotactics(g)
        .unwrap_or_else(|e| panic!("templated compile must succeed: {e}"));
    let mut proposer = compiled.proposer;
    let candidates = proposer.propose(surface);
    let matched = identities.iter().any(|(morphemes, root_index)| {
        candidates.iter().any(|c| {
            c.root_index == *root_index
                && c.morphemes
                    .iter()
                    .map(|m| m.0)
                    .eq(morphemes.iter().copied())
        })
    });
    assert!(
        matched,
        "{surface:?} must be reachable in the TEMPLATED backend's proposal set at one of the \
         oracle's own identities {identities:?}; proposer offered {candidates:?}"
    );
}

/// Generator-based: a template-slot circumfix, `circumfix-in-template-slot`'s shape but a DIFFERENT synthetic grammar, including the harder case of NO ordinary sibling rule in the slot.
#[test]
fn template_slot_circumfix_recall_parity_on_templated_backend() {
    let recipe = Recipe {
        name: "templated-circumfix-slot",
        seed: 20260810,
        scale: ScaleKnobs {
            entries_per_stratum: 3,
            segment_inventory: 5,
            ..ScaleKnobs::default()
        },
        construct: ConstructKnobs {
            table_count: 1,
            circumfix_count: 1,
            template_slot_optional: true,
            ..Default::default()
        },
    };
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = load(&rendered.xml);
    let circ_xml_id = rendered.tables[0]
        .circumfix_mrule_xml_ids
        .first()
        .cloned()
        .expect("recipe must produce at least 1 circumfix rule");
    let circ_mrule = mrule_id_of(&g, &circ_xml_id);

    let oracle_opts = OracleOpts {
        step_cap: 20_000,
        word_timeout: Some(Duration::from_millis(500)),
        max_rules_per_root: 8,
        max_total_words: 100,
    };
    let roots: Vec<LexEntryId> = (0..g.entries.len() as u32).map(LexEntryId).collect();
    let words = sweep(&g, &roots, &[circ_mrule], &oracle_opts);
    assert!(
        words.iter().any(|w| w.mrule.is_some()),
        "oracle sweep must produce at least one circumfixed word"
    );

    let morpher =
        Morpher::new(&g, oracle_opts.step_cap).with_word_timeout(oracle_opts.word_timeout);
    let mut missed = Vec::new();
    for w in &words {
        let identities = oracle_identities(&morpher, &w.surface);
        assert!(
            !identities.is_empty(),
            "oracle word {:?} must parse against its own grammar",
            w.surface
        );
        let compiled = compile_templated_morphotactics(&g)
            .unwrap_or_else(|e| panic!("templated compile must succeed: {e}"));
        let mut proposer = compiled.proposer;
        let candidates = proposer.propose(&w.surface);
        let matched = identities.iter().any(|(morphemes, root_index)| {
            candidates.iter().any(|c| {
                c.root_index == *root_index
                    && c.morphemes
                        .iter()
                        .map(|m| m.0)
                        .eq(morphemes.iter().copied())
            })
        });
        if !matched {
            missed.push(w.surface.clone());
        }
    }
    assert!(
        missed.is_empty(),
        "100% recall required on the oracle word list; missed: {missed:?}"
    );
}

/// Declaration order does not hide a later circumfix allomorph's suffix half.
#[test]
fn template_slot_later_circumfix_allomorph_keeps_its_suffix_half() {
    let mut g = load(include_str!(
        "../../../../conformance-staging/edge-cases/circumfix-in-template-slot/grammar.xml"
    ));
    let circ = mrule_id_of(&g, "mrCircum");
    let ordinary = mrule_id_of(&g, "mrOrdPfx");
    let ordinary_allomorph = match &mut g.mrules[ordinary.0 as usize] {
        MorphRuleDef::AffixProcess(def) => def.allomorphs.remove(0),
        other => panic!("mrOrdPfx must be affix-process, got {other:?}"),
    };
    let allomorph_ids = match &mut g.mrules[circ.0 as usize] {
        MorphRuleDef::AffixProcess(def) => {
            def.allomorphs.insert(0, ordinary_allomorph);
            def.allomorphs.iter().map(|a| a.id).collect::<Vec<_>>()
        }
        other => panic!("mrCircum must be affix-process, got {other:?}"),
    };
    for (index, allomorph) in allomorph_ids.into_iter().enumerate() {
        g.allomorph_owners[allomorph.0 as usize] = AllomorphOwner::Affix(circ, index as u16);
    }

    let root = entry_id_of(&g, "eRoot");
    let morpher = Morpher::new(&g, 20_000);
    let words = morpher.generate_words(root, &[GenMorpheme::Rule(circ)], FeatureStruct::EMPTY);
    assert!(words.contains(&"talodien".to_string()));
    assert_templated_recall(&g, &morpher, "talodien");
}

const CIRCUMFIX_CROSS_PRODUCT_XML: &str = r#"<HermitCrabInput><Language><Name>SynCircCross</Name>
  <PartsOfSpeech><PartOfSpeech id="posR"><Name>R</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cy"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cw"><Representations><Representation>w</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrCross">
      <Name>S</Name>
      <MorphologicalRuleDefinitions>
        <MorphologicalRule id="mrCross" requiredPartsOfSpeech="posR" outputPartOfSpeech="posR">
          <Name>cross</Name>
          <MorphologicalSubrules>
            <MorphologicalSubrule id="subAA">
              <MorphologicalInput><PhoneticSequence id="s0"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput>
                <InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments>
                <CopyFromInput index="s0" />
                <InsertSegments><PhoneticShape>z</PhoneticShape></InsertSegments>
              </MorphologicalOutput>
            </MorphologicalSubrule>
            <MorphologicalSubrule id="subAB">
              <MorphologicalInput><PhoneticSequence id="s1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput>
                <InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments>
                <CopyFromInput index="s1" />
                <InsertSegments><PhoneticShape>w</PhoneticShape></InsertSegments>
              </MorphologicalOutput>
            </MorphologicalSubrule>
            <MorphologicalSubrule id="subBA">
              <MorphologicalInput><PhoneticSequence id="s2"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput>
                <InsertSegments><PhoneticShape>y</PhoneticShape></InsertSegments>
                <CopyFromInput index="s2" />
                <InsertSegments><PhoneticShape>z</PhoneticShape></InsertSegments>
              </MorphologicalOutput>
            </MorphologicalSubrule>
            <MorphologicalSubrule id="subBB">
              <MorphologicalInput><PhoneticSequence id="s3"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput>
                <InsertSegments><PhoneticShape>y</PhoneticShape></InsertSegments>
                <CopyFromInput index="s3" />
                <InsertSegments><PhoneticShape>w</PhoneticShape></InsertSegments>
              </MorphologicalOutput>
            </MorphologicalSubrule>
          </MorphologicalSubrules>
          <MorphemeId>CROSS</MorphemeId>
        </MorphologicalRule>
      </MorphologicalRuleDefinitions>
      <LexicalEntries>
        <LexicalEntry id="eRoot" partOfSpeech="posR">
          <Allomorphs><Allomorph id="aRoot"><PhoneticShape>q</PhoneticShape></Allomorph></Allomorphs>
          <MorphemeId>ROOT</MorphemeId>
        </LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

/// Synthetic (NOT `circumfix-cross-product-and-infix-drop`): a 2x2 cross-product circumfix, 4 alternative subrules of one rule; pins that every cell is independently reachable.
#[test]
fn circumfix_cross_product_recall_parity_on_templated_backend() {
    let g = load(CIRCUMFIX_CROSS_PRODUCT_XML);
    let root = entry_id_of(&g, "eRoot");
    let mid = mrule_id_of(&g, "mrCross");
    let morpher = Morpher::new(&g, 20_000);

    let words = morpher.generate_words(root, &[GenMorpheme::Rule(mid)], FeatureStruct::EMPTY);
    assert_eq!(
        words,
        vec![
            "xqw".to_string(),
            "xqz".to_string(),
            "yqw".to_string(),
            "yqz".to_string()
        ],
        "the real engine must produce all 4 cross-product cells"
    );

    for surface in &words {
        assert_templated_recall(&g, &morpher, surface);
    }
}

const CIRCUMFIX_PLUS_TRAILING_SUFFIX_XML: &str = r#"<HermitCrabInput><Language><Name>SynCircTrail</Name>
  <PartsOfSpeech><PartOfSpeech id="posR"><Name>R</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ch"><Representations><Representation>h</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrCirc mrSuf">
      <Name>S</Name>
      <MorphologicalRuleDefinitions>
        <MorphologicalRule id="mrCirc" requiredPartsOfSpeech="posR" outputPartOfSpeech="posR">
          <Name>circ</Name>
          <MorphologicalSubrules>
            <MorphologicalSubrule id="subCirc">
              <MorphologicalInput><PhoneticSequence id="s0"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput>
                <InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments>
                <CopyFromInput index="s0" />
                <InsertSegments><PhoneticShape>z</PhoneticShape></InsertSegments>
              </MorphologicalOutput>
            </MorphologicalSubrule>
          </MorphologicalSubrules>
          <MorphemeId>CIRC</MorphemeId>
        </MorphologicalRule>
        <MorphologicalRule id="mrSuf" requiredPartsOfSpeech="posR" outputPartOfSpeech="posR">
          <Name>suf</Name>
          <MorphologicalSubrules>
            <MorphologicalSubrule id="subSuf">
              <MorphologicalInput><PhoneticSequence id="s1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput>
                <CopyFromInput index="s1" />
                <InsertSegments><PhoneticShape>h</PhoneticShape></InsertSegments>
              </MorphologicalOutput>
            </MorphologicalSubrule>
          </MorphologicalSubrules>
          <MorphemeId>SUF</MorphemeId>
        </MorphologicalRule>
      </MorphologicalRuleDefinitions>
      <LexicalEntries>
        <LexicalEntry id="eRoot" partOfSpeech="posR">
          <Allomorphs><Allomorph id="aRoot"><PhoneticShape>q</PhoneticShape></Allomorph></Allomorphs>
          <MorphemeId>ROOT</MorphemeId>
        </LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

/// Synthetic (NOT `fusional-realizational-morphology`): a circumfix plus an independent later ordinary suffix; pins the circumfix's suffix half lands root-adjacent, not at the word's absolute edge.
#[test]
fn circumfix_composes_with_a_later_ordinary_suffix_on_templated_backend() {
    let g = load(CIRCUMFIX_PLUS_TRAILING_SUFFIX_XML);
    let root = entry_id_of(&g, "eRoot");
    let circ_mid = mrule_id_of(&g, "mrCirc");
    let suf_mid = mrule_id_of(&g, "mrSuf");
    let morpher = Morpher::new(&g, 20_000);

    let bare = morpher.generate_words(root, &[GenMorpheme::Rule(circ_mid)], FeatureStruct::EMPTY);
    assert_eq!(bare, vec!["xqz".to_string()]);
    assert_templated_recall(&g, &morpher, "xqz");

    // generate_words's list order is OUTER-to-INNER (unapply direction), so suf-then-circ makes the circumfix the INNER, root-adjacent wrap -- matching the templated backend's own declaration-order chain.
    let combined = morpher.generate_words(
        root,
        &[GenMorpheme::Rule(suf_mid), GenMorpheme::Rule(circ_mid)],
        FeatureStruct::EMPTY,
    );
    assert_eq!(
        combined,
        vec!["xqzh".to_string()],
        "the circumfix's own suffix half ('z') must stay adjacent to the root, before the later \
         ordinary suffix ('h'), not at the word's absolute right edge"
    );
    assert_templated_recall(&g, &morpher, "xqzh");
}
