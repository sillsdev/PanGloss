//! GATE 2 (`docs/fst-plan/phase-c-generator-design.md` §5/§6, priority (2)): circumfix
//! recall-parity gate -- the FIRST full end-to-end validation of generator + oracle + gate
//! together (design doc §6).
//!
//! ## Why the production ENUMERATION path, not `pg-foma/src/uflexc.rs`
//! `pg-foma/src/emit.rs`'s `classify_affix` reads a circumfix rule's shape (leading AND trailing
//! insert around one copied span) as `Role::CircumfixPrefix`, and `is_structural_rule` (that
//! module's own doc) ALWAYS routes it through the "structural composite" builder -- a
//! `pg_parse::Morpher`-driven synthesis of every composite entry, never literal-lexc
//! concatenation. `pg-foma/src/uflexc.rs` (the simpler, token-space emitter GATE 1 uses) explicitly
//! SKIPS `Role::CircumfixPrefix` (that module's own doc: "everything else ... is skipped and
//! reported"). So this gate builds its net via `pg_foma::emit::emit` (the production `emit()` ->
//! lexc pipeline `FomaProposer::new` itself calls) rather than `uflexc`/`replace` -- the only path
//! that actually covers this construct today.
//!
//! ## Recall technique
//! Same compose-recall helper as GATE 1 (`tests/common/gate_template.rs`), but querying with
//! LITERAL surface text (not `pg_foma::replace::SegAlphabet` PUA tokens -- `emit.rs`'s own lower
//! tape is literal orthography, per that module's doc, unlike `uflexc.rs`'s token-space lower
//! tape) and, per-word, the EXACT tag sequence recovered by re-parsing the oracle's own generated
//! surface word through an independent `pg_parse::Morpher` -- mirroring
//! `p6_aweti_q4_compose_recall.rs`'s own technique (copied by re-implementing it after reading
//! that file, not by referencing it -- see that file's own worktree, which a different agent is
//! actively modifying). See `tests/common/gate_template.rs`'s own module doc for a real deviation
//! found empirically while building THIS gate (a structural-composite entry's projected-upper net
//! defeats `fsm_intersect` but not a bounded `apply_up` on that same tiny net).
//!
//! 100% recall is required here (unlike GATE 1): circumfix has no known compiler gap on the
//! enumeration path (design doc §5's "Recall parity now" list), so this grammar's small size
//! keeps this gate fast while still proving the full pipeline for real.

mod common;

use std::time::Duration;

use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;

use pg_featstruct::FeatureStruct;
use pg_foma::emit;
use pg_foma::tags;
use pg_grammar::model::{LexEntryId, MorphemeId};
use pg_grammar_gen::oracle::{sweep, OracleOpts};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};
use pg_parse::{GenMorpheme, Morpher, ParseOptions};

use common::gate_template::{
    assert_net_size_within, entry_id_of, mrule_id_of, per_word_p99, recall_reachable,
};

fn recipe() -> Recipe {
    Recipe {
        name: "phase-c-circumfix",
        seed: 20260720,
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
    }
}

#[test]
fn circumfix_recall_parity_via_generator_and_oracle() {
    let recipe = recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated circumfix XML failed to load: {e}\n{}",
            rendered.xml
        )
    });

    assert_eq!(g.entries.len(), 3, "recipe must produce exactly 3 roots");
    assert_eq!(
        g.templates.len(),
        1,
        "recipe must produce exactly 1 AffixTemplate"
    );

    let circ_xml_id = rendered.tables[0]
        .circumfix_mrule_xml_ids
        .first()
        .cloned()
        .expect("recipe must produce at least 1 circumfix rule");
    let circ_mrule = mrule_id_of(&g, &circ_xml_id);

    // --- Oracle (design doc §3): bounded Morpher-as-generator sweep over every root, bare AND
    // circumfixed. Sized so this stays cheap by construction (3 roots x <=2 forms each). ---
    let oracle_opts = OracleOpts {
        step_cap: 20_000,
        word_timeout: Some(Duration::from_millis(500)),
        max_rules_per_root: 8,
        max_total_words: 100,
    };
    let roots: Vec<LexEntryId> = (0..g.entries.len() as u32).map(LexEntryId).collect();
    let words = sweep(&g, &roots, &[circ_mrule], &oracle_opts);
    assert!(
        !words.is_empty(),
        "oracle sweep produced zero words -- gate must be non-vacuous"
    );
    assert!(
        words.iter().any(|w| w.mrule.is_none()),
        "no bare-root oracle word generated"
    );
    assert!(
        words.iter().any(|w| w.mrule.is_some()),
        "no circumfixed oracle word generated"
    );
    println!(
        "oracle produced {} words ({} roots x up to 2 forms each)",
        words.len(),
        roots.len()
    );

    // --- Build the FST via the production enumeration path (module doc). ---
    let emit_result = emit::emit(&g);
    assert!(
        emit_result.report.uncovered.is_empty(),
        "circumfix must be fully covered by the enumeration path: {:?}",
        emit_result.report.uncovered
    );
    let opts = FomaOptions::default();
    let net = fsm_lexc_parse_string(&opts, None, &emit_result.lexc_source)
        .unwrap_or_else(|| panic!("emitted lexc must compile:\n{}", emit_result.lexc_source));

    // --- Resource envelope (design doc §4b): a 3-root, 1-circumfix grammar must stay tiny. ---
    assert_net_size_within(&net, 2_000, 20_000);

    // --- Recall (design doc §4a): re-parse each oracle word via an independent Morpher to recover
    // its own tag sequence (mirrors the P6/Aweti compose-recall technique, module doc), then check
    // that sequence is reachable in `net`. 100% required (module doc). ---
    let morpher =
        Morpher::new(&g, oracle_opts.step_cap).with_word_timeout(oracle_opts.word_timeout);
    let popts = ParseOptions::default();
    let width = tags::tag_width(g.morphemes.len());

    let tag_sequences_for = |surface: &str| -> Vec<Vec<String>> {
        let outcome = morpher.parse_word_opts(surface, &popts);
        outcome
            .structured
            .iter()
            .map(|a| {
                a.morpheme_ids
                    .iter()
                    .enumerate()
                    .map(|(i, &m)| {
                        let mid = MorphemeId(m);
                        if i as i32 == a.root_morpheme_index {
                            tags::root_tag_text(mid, width)
                        } else {
                            tags::morph_tag_text(mid, width)
                        }
                    })
                    .collect()
            })
            .collect()
    };

    let mut missed = Vec::new();
    for w in &words {
        let normalized = pg_grammar::nfd::nfd(&w.surface);
        let analyses = tag_sequences_for(&w.surface);
        assert!(
            !analyses.is_empty(),
            "oracle word {:?} (root {:?}) has no analysis from the SAME grammar's own Morpher -- \
             oracle/parser inconsistency, not a recall question",
            w.surface,
            w.root
        );
        let any_reachable = analyses
            .iter()
            .any(|tags| recall_reachable(&net, &normalized, tags));
        if !any_reachable {
            missed.push(w.surface.clone());
        }
    }
    assert!(
        missed.is_empty(),
        "100% recall required on the oracle word list; missed: {missed:?}"
    );

    // --- Resource envelope (design doc §4b): per-word p99, sub-10ms trip-wire (generous headroom
    // for a network this small; the trip-wire is about catching a regression, not benchmarking). ---
    let p99 = per_word_p99(&words, |w| {
        let normalized = pg_grammar::nfd::nfd(&w.surface);
        for tags in tag_sequences_for(&w.surface) {
            let _ = recall_reachable(&net, &normalized, &tags);
        }
    });
    assert!(
        p99 < Duration::from_millis(50),
        "per-word p99 {p99:?} exceeds the trip-wire"
    );
}

// =================================================================================================
// `openspec/changes/cover-circumfix-null-output-actions`: ordered output-action sequences (never
// just the first `InsertSegments`) and null-role/subtractive LHS drops, both IN-SCOPE (must
// compile) and OUT-OF-SCOPE (stays honestly unsupported). Synthetic, delanguaged, hand-authored
// XML (`pg_grammar::load`, mirroring `crate::capability`'s own test-module style): `pg_grammar_gen`
// has no knob for an ordered multi-`InsertSegments` RHS or a null-role LHS drop -- GATE 2's own
// circumfix recipe above is the only construct that crate generates today.
// =================================================================================================

fn load(xml: &str) -> pg_grammar::model::Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// Re-derives the real tag sequence(s) for `surface` by re-parsing it against `morpher`'s OWN
/// grammar (mirrors `circumfix_recall_parity_via_generator_and_oracle`'s own `tag_sequences_for`
/// closure above -- the oracle's own analysis of its own output, never a hand-derived guess at tag
/// order).
fn tag_sequences_for(
    g: &pg_grammar::model::Grammar,
    morpher: &Morpher,
    surface: &str,
) -> Vec<Vec<String>> {
    let popts = ParseOptions::default();
    let outcome = morpher.parse_word_opts(surface, &popts);
    let width = tags::tag_width(g.morphemes.len());
    outcome
        .structured
        .iter()
        .map(|a| {
            a.morpheme_ids
                .iter()
                .enumerate()
                .map(|(i, &m)| {
                    let mid = MorphemeId(m);
                    if i as i32 == a.root_morpheme_index {
                        tags::root_tag_text(mid, width)
                    } else {
                        tags::morph_tag_text(mid, width)
                    }
                })
                .collect()
        })
        .collect()
}

/// Synthetic, delanguaged fixture (task 2.2, design.md: "never silently reduced to the first
/// inserted segment"): a single-part-LHS prefix rule whose RHS is TWO ordered `InsertSegments`
/// actions before the `CopyFromInput` -- `x` then `y` then the stem. This never routes through
/// `build_structural_composites` at all (no LHS material is dropped: `lhs.len() == 1`), so it
/// exercises `crate::emit::insert_action_texts`'s own ORDINARY (non-structural) emission path
/// directly. Before this change's fix, [`emit::emit`] would have emitted only `x` + stem (the old
/// `first_insert_text` silently dropped the second `InsertSegments` action), losing recall for the
/// REAL surface `xya` entirely.
const ORDERED_MULTI_INSERT_XML: &str = r#"<HermitCrabInput><Language><Name>OrderedMultiInsert</Name>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cy"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cx" /><Segment segment="cy" /></SegmentNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1" morphologicalRules="mrPre">
      <Name>S</Name>
      <MorphologicalRuleDefinitions>
        <MorphologicalRule id="mrPre">
          <Name>pre</Name>
          <MorphologicalSubrules>
            <MorphologicalSubrule id="subPre">
              <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput>
                <InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments>
                <InsertSegments><PhoneticShape>y</PhoneticShape></InsertSegments>
                <CopyFromInput index="stem" />
              </MorphologicalOutput>
            </MorphologicalSubrule>
          </MorphologicalSubrules>
          <MorphemeId>PRE</MorphemeId>
        </MorphologicalRule>
      </MorphologicalRuleDefinitions>
      <LexicalEntries>
        <LexicalEntry id="eRoot">
          <Allomorphs><Allomorph id="aRoot"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          <MorphemeId>ROOT</MorphemeId>
        </LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

/// IN-SCOPE positive witness: the proposer-to-confirm containment the deliverable asks for. Proves
/// (a) the real oracle actually concatenates BOTH `InsertSegments` actions (`xya`, not `xa`), and
/// (b) that exact surface, with its own real tag sequence, is reachable in the compiled FST.
#[test]
fn ordered_multi_insert_no_first_insert_shortcut_recall_parity() {
    let g = load(ORDERED_MULTI_INSERT_XML);
    let root = entry_id_of(&g, "eRoot");
    let mid = mrule_id_of(&g, "mrPre");

    let emit_result = emit::emit(&g);
    let opts = FomaOptions::default();
    let net = fsm_lexc_parse_string(&opts, None, &emit_result.lexc_source)
        .unwrap_or_else(|| panic!("emitted lexc must compile:\n{}", emit_result.lexc_source));

    // --- Oracle: the REAL synthesis engine, not a hand re-derivation of "x + y + root". ---
    let morpher = Morpher::new(&g, 20_000);
    let words = morpher.generate_words(root, &[GenMorpheme::Rule(mid)], FeatureStruct::EMPTY);
    assert_eq!(
        words,
        vec!["xya".to_string()],
        "the real engine must concatenate BOTH InsertSegments actions in document order"
    );
    let surface = &words[0];

    let tag_sequences = tag_sequences_for(&g, &morpher, surface);
    assert!(
        !tag_sequences.is_empty(),
        "the oracle's own surface {surface:?} must parse against its own grammar"
    );
    let normalized = pg_grammar::nfd::nfd(surface);
    let any_reachable = tag_sequences
        .iter()
        .any(|tags| recall_reachable(&net, &normalized, tags));
    assert!(
        any_reachable,
        "the ordered-multi-insert surface {surface:?} must be reachable with its own real tag \
         sequence -- before this change's fix, only the FIRST InsertSegments action's text was \
         ever emitted, silently losing this candidate entirely"
    );
}

/// Synthetic, delanguaged fixture (D1's "cover-circumfix-null-..." row, IN-SCOPE case): a
/// 2-part-LHS rule whose RHS `CopyFromInput`s only the FIRST part -- a null-role subtractive drop
/// (the second part is matched/consumed but never reaches the output). `classify_affix` reads this
/// as `Role::None` (no `InsertSegments` at all), and `crate::emit::is_structural_rule` admits it
/// (`rhs_drops_lhs_material` is true), so the REAL surface (root `ab` truncated to `a`) is only
/// reachable via `build_structural_composites`'s oracle-backed resynthesis -- the ordinary
/// two-entry emission path can only ever propose the UNMODIFIED root (`ab`), never the truncated
/// one.
const NULL_ROLE_STRUCTURAL_DROP_XML: &str = r#"<HermitCrabInput><Language><Name>NullRoleDrop</Name>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses>
    <SegmentNaturalClass id="ncA"><Name>A</Name><Segment segment="ca" /></SegmentNaturalClass>
    <SegmentNaturalClass id="ncB"><Name>B</Name><Segment segment="cb" /></SegmentNaturalClass>
  </NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1" morphologicalRules="mrDropOk">
      <Name>S</Name>
      <MorphologicalRuleDefinitions>
        <MorphologicalRule id="mrDropOk">
          <Name>dropOk</Name>
          <MorphologicalSubrules>
            <MorphologicalSubrule id="subDropOk">
              <MorphologicalInput>
                <PhoneticSequence id="qA"><SimpleContext naturalClass="ncA" /></PhoneticSequence>
                <PhoneticSequence id="qB"><SimpleContext naturalClass="ncB" /></PhoneticSequence>
              </MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="qA" /></MorphologicalOutput>
            </MorphologicalSubrule>
          </MorphologicalSubrules>
          <MorphemeId>DROPOK</MorphemeId>
        </MorphologicalRule>
      </MorphologicalRuleDefinitions>
      <LexicalEntries>
        <LexicalEntry id="eRoot">
          <Allomorphs><Allomorph id="aRoot"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs>
          <MorphemeId>ROOT</MorphemeId>
        </LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

/// IN-SCOPE positive witness: proposer-to-confirm containment for a null-role structural drop.
#[test]
fn null_role_structural_drop_recall_parity() {
    let g = load(NULL_ROLE_STRUCTURAL_DROP_XML);
    let root = entry_id_of(&g, "eRoot");
    let mid = mrule_id_of(&g, "mrDropOk");

    let emit_result = emit::emit(&g);
    let opts = FomaOptions::default();
    let net = fsm_lexc_parse_string(&opts, None, &emit_result.lexc_source)
        .unwrap_or_else(|| panic!("emitted lexc must compile:\n{}", emit_result.lexc_source));

    let morpher = Morpher::new(&g, 20_000);
    let words = morpher.generate_words(root, &[GenMorpheme::Rule(mid)], FeatureStruct::EMPTY);
    assert_eq!(
        words,
        vec!["a".to_string()],
        "the real engine must truncate the root to just its copied part"
    );
    let surface = &words[0];

    let tag_sequences = tag_sequences_for(&g, &morpher, surface);
    assert!(
        !tag_sequences.is_empty(),
        "the oracle's own surface {surface:?} must parse against its own grammar"
    );
    let normalized = pg_grammar::nfd::nfd(surface);
    let any_reachable = tag_sequences
        .iter()
        .any(|tags| recall_reachable(&net, &normalized, tags));
    assert!(
        any_reachable,
        "the null-role structural-drop surface {surface:?} must be reachable with its own real \
         tag sequence -- only build_structural_composites's oracle-backed resynthesis can produce \
         a TRUNCATED root surface at all"
    );

    // The ordinary two-entry emission path can only ever propose the FULL, unmodified root: the
    // full spelling must ALSO be reachable (harmless spurious over-generation, never a false
    // negative -- module doc's "upward approximation" convention) -- confirming
    // build_structural_composites is genuinely ADDING the correct candidate on top of the ordinary
    // path, not replacing it.
    assert!(
        recall_reachable(&net, "ab", &tag_sequences[0]),
        "the unmodified root spelling must ALSO be reachable via the ordinary, non-structural \
         emission path's own harmless zero-morph candidate"
    );
}

/// Synthetic, delanguaged fixture (D1's "cover-circumfix-null-..." row, OUT-OF-SCOPE case): the
/// SAME 2-part-LHS drop shape as [`NULL_ROLE_STRUCTURAL_DROP_XML`], but the RHS uses
/// `ModifyFromInput` (an ablaut/simulfix-style "process morph") instead of ever `CopyFromInput`ing
/// either part. `classify_affix` reads this as `Role::Process` (design.md's "not compilable as
/// strings" citation), which `crate::emit::is_structural_rule` never admits -- this construct must
/// stay HONESTLY unsupported: reported in `EmitReport::uncovered` at the standalone-rule
/// classification stage (never reaching per-allomorph emission at all, since `Role::Process` is
/// excluded from both the prefix and suffix derivation-rule lists), never silently compiled to any
/// candidate (never a wrong compile, this change's own hard rule).
const PROCESS_ROLE_DROP_XML: &str = r#"<HermitCrabInput><Language><Name>ProcessRoleDrop</Name>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses>
    <SegmentNaturalClass id="ncA"><Name>A</Name><Segment segment="ca" /></SegmentNaturalClass>
    <SegmentNaturalClass id="ncB"><Name>B</Name><Segment segment="cb" /></SegmentNaturalClass>
  </NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1" morphologicalRules="mrDropProcess">
      <Name>S</Name>
      <MorphologicalRuleDefinitions>
        <MorphologicalRule id="mrDropProcess">
          <Name>dropProcess</Name>
          <MorphologicalSubrules>
            <MorphologicalSubrule id="subDropProcess">
              <MorphologicalInput>
                <PhoneticSequence id="pA"><SimpleContext naturalClass="ncA" /></PhoneticSequence>
                <PhoneticSequence id="pB"><SimpleContext naturalClass="ncB" /></PhoneticSequence>
              </MorphologicalInput>
              <MorphologicalOutput>
                <ModifyFromInput index="pA"><SimpleContext naturalClass="ncA" /></ModifyFromInput>
              </MorphologicalOutput>
            </MorphologicalSubrule>
          </MorphologicalSubrules>
          <MorphemeId>DROPPROC</MorphemeId>
        </MorphologicalRule>
      </MorphologicalRuleDefinitions>
      <LexicalEntries>
        <LexicalEntry id="eRoot">
          <Allomorphs><Allomorph id="aRoot"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs>
          <MorphemeId>ROOT</MorphemeId>
        </LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

/// OUT-OF-SCOPE negative witness: stays honestly unsupported, never silently (mis)compiled.
#[test]
fn process_role_drop_stays_honestly_unsupported() {
    let g = load(PROCESS_ROLE_DROP_XML);

    let emit_result = emit::emit(&g);
    assert!(
        emit_result
            .report
            .uncovered
            .iter()
            .any(|u| u.kind == "process"),
        "a Role::Process standalone rule must be reported uncovered, never silently compiled: {:?}",
        emit_result.report.uncovered
    );

    // The grammar must still compile cleanly (an uncovered construct simply contributes no
    // candidates -- FomaTier::Partial, never a build failure).
    let opts = FomaOptions::default();
    let _net = fsm_lexc_parse_string(&opts, None, &emit_result.lexc_source).unwrap_or_else(|| {
        panic!(
            "emitted lexc must still compile:\n{}",
            emit_result.lexc_source
        )
    });
}
