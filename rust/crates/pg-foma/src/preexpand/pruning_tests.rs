use super::*;
use crate::morphotactics::ExploreMode;

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}"))
}

/// One stratum, one template `[slot0: mrA; slot1: mrB]`, plus a trivial phonological rule so `should_run` is true; `vacuous` selects slot0's rule shape, real surface material vs. a bare `CopyFromInput`.
fn slot_gate_fixture(vacuous: bool) -> String {
    let slot0_output = if vacuous {
        r#"<CopyFromInput index="stemA" />"#.to_string()
    } else {
        r#"<InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments><CopyFromInput index="stemA" />"#.to_string()
    };
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
<Name>PruningDepth0Gate</Name>
<PartsOfSpeech>
  <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
</PartsOfSpeech>
<CharacterDefinitionTable id="t1">
  <Name>Main</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<NaturalClasses>
  <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
</NaturalClasses>
<PhonologicalRuleDefinitions>
  <PhonologicalRule id="pr1">
    <Name>PR</Name>
    <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAny" /></PhoneticSequence></PhoneticInput>
    <PhonologicalSubrules>
      <PhonologicalSubrule>
        <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAny" /></PhoneticSequence></PhoneticOutput>
      </PhonologicalSubrule>
    </PhonologicalSubrules>
  </PhonologicalRule>
</PhonologicalRuleDefinitions>
<Strata>
  <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="pr1">
    <Name>Main</Name>
    <MorphologicalRuleDefinitions>
      <MorphologicalRule id="mrA" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
        <Name>a</Name>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subA">
            <MorphologicalInput><PhoneticSequence id="stemA"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput>{slot0_output}</MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
        <MorphemeId>A</MorphemeId>
      </MorphologicalRule>
      <MorphologicalRule id="mrB" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
        <Name>b</Name>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subB">
            <MorphologicalInput><PhoneticSequence id="stemB"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>b</PhoneticShape></InsertSegments><CopyFromInput index="stemB" /></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
        <MorphemeId>B</MorphemeId>
      </MorphologicalRule>
    </MorphologicalRuleDefinitions>
    <AffixTemplates>
      <AffixTemplate requiredPartsOfSpeech="posV">
        <Name>T</Name>
        <Slot morphologicalRules="mrA"><Name>s0</Name></Slot>
        <Slot morphologicalRules="mrB"><Name>s1</Name></Slot>
      </AffixTemplate>
    </AffixTemplates>
    <LexicalEntries>
      <LexicalEntry id="eK" partOfSpeech="posV">
        <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
        <MorphemeId>K</MorphemeId>
      </LexicalEntry>
    </LexicalEntries>
  </Stratum>
</Strata>
  </Language>
</HermitCrabInput>"#
    )
}

/// A slot-only rule not first-reachable is not probed at depth 0 under `Pruned`, but is under `Flat` (kept as the A/B baseline) -- proving `extend` actually consults the automaton, not just that it computes the right answer in isolation.
#[test]
fn mandatory_non_vacuous_slot0_blocks_slot1_probe_at_depth0() {
    let g = load(&slot_gate_fixture(false));
    assert!(
        should_run(&g, PhonologyProbe::new(&g).as_ref()),
        "fixture must exercise should_run"
    );
    let width = tags::tag_width(g.morphemes.len());
    let phon = PhonologyProbe::new(&g);
    let mt = MorphotacticIndex::build(&g);

    let (_, flat) =
        build_composites_with_mode(&g, width, phon.as_ref(), &mt, ExploreMode::Flat, None);
    let (_, pruned) =
        build_composites_with_mode(&g, width, phon.as_ref(), &mt, ExploreMode::Pruned, None);

    // Flat (ignores morphotactics) probes both mrA and mrB at depth 0; pruned must skip mrB, blocked by the mandatory non-vacuous slot 0.
    assert_eq!(
        flat.pairs_probed_by_depth[0], 2,
        "flat mode probes both candidates at depth 0"
    );
    assert_eq!(
        pruned.pairs_probed_by_depth[0], 1,
        "pruned mode must not probe slot 1's rule at depth 0 while slot 0 is mandatory/non-vacuous"
    );
}

/// The variant where slot0's rule is instead vacuous (bare `CopyFromInput`, no surface material): it classifies `Role::None` and is never a candidate rule itself, but slot 1's rule must still be probed at depth 0 under `Pruned` -- a skippable mandatory slot must never become a hard barrier.
#[test]
fn vacuous_slot0_lets_slot1_be_probed_at_depth0_under_pruning() {
    let g = load(&slot_gate_fixture(true));
    assert!(
        should_run(&g, PhonologyProbe::new(&g).as_ref()),
        "fixture must exercise should_run"
    );
    let width = tags::tag_width(g.morphemes.len());
    let phon = PhonologyProbe::new(&g);
    let mt = MorphotacticIndex::build(&g);

    let (_, flat) =
        build_composites_with_mode(&g, width, phon.as_ref(), &mt, ExploreMode::Flat, None);
    let (_, pruned) =
        build_composites_with_mode(&g, width, phon.as_ref(), &mt, ExploreMode::Pruned, None);

    // mrA (vacuous) is `Role::None`, so only mrB is ever attempted at depth 0; the point here is that pruning does not lose that attempt.
    assert_eq!(
        flat.pairs_probed_by_depth[0], 1,
        "only mrB is a candidate rule in this fixture"
    );
    assert_eq!(
        pruned.pairs_probed_by_depth[0], 1,
        "a vacuous mandatory slot 0 must not block slot 1's rule from depth-0 probing"
    );
}

#[test]
fn ordinary_preexpand_exhausts_a_four_rule_chain() {
    let phonology = r#"
<PhonologicalRuleDefinitions>
  <PhonologicalRule id="pr1">
    <Name>identity</Name>
    <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAny" /></PhoneticSequence></PhoneticInput>
    <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAny" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
  </PhonologicalRule>
</PhonologicalRuleDefinitions>
  <Strata>"#;
    let xml = include_str!(
        "../../tests/fixtures/pangloss/fst-completeness/late-structural-anchor-five-rule-chain/grammar.xml"
    )
    .replacen("<Strata>", phonology, 1)
    .replacen(
        "<Stratum characterDefinitionTable=\"t1\"",
        "<Stratum characterDefinitionTable=\"t1\" phonologicalRules=\"pr1\"",
        1,
    );
    let g = load(&xml);
    let width = tags::tag_width(g.morphemes.len());
    let phon = PhonologyProbe::new(&g);
    let mt = MorphotacticIndex::build(&g);

    let (_, report) =
        build_composites_with_mode(&g, width, phon.as_ref(), &mt, ExploreMode::Pruned, None);

    assert_eq!(report.pending_successors, 0);
    assert!(
        report.pairs_probed_by_depth.get(3).copied().unwrap_or(0) > 0,
        "the fourth ordinary rule must be explored rather than hidden behind a depth boundary"
    );
}

fn sample_path(name: &str) -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../samples/data")
        .join(name);
    path.exists().then_some(path)
}

fn load_amharic() -> Option<Grammar> {
    let path = sample_path("amharic-hc.xml")?;
    let xml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load amharic-hc.xml: {e}")))
}

/// The Amharic A/B subset gate: pruned exploration is a strict subset of flat exploration, recall-preserving by construction since pruning only removes adjacencies the morphotactics could never produce.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with \
            --include-ignored"]
fn amharic_pruned_composites_are_a_subset_of_flat() {
    let Some(g) = load_amharic() else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let width = tags::tag_width(g.morphemes.len());
    let phon = PhonologyProbe::new(&g);
    let mt = MorphotacticIndex::build(&g);

    let t_flat = std::time::Instant::now();
    let (flat_recs, flat_report) =
        build_composites_with_mode(&g, width, phon.as_ref(), &mt, ExploreMode::Flat, None);
    let flat_elapsed = t_flat.elapsed();

    let t_pruned = std::time::Instant::now();
    let (pruned_recs, pruned_report) =
        build_composites_with_mode(&g, width, phon.as_ref(), &mt, ExploreMode::Pruned, None);
    let pruned_elapsed = t_pruned.elapsed();

    let flat_set: rustc_hash::FxHashSet<(String, String)> = flat_recs
        .iter()
        .flat_map(|r| {
            r.variants
                .iter()
                .map(move |v| (r.tag_lexc.clone(), v.clone()))
        })
        .collect();
    let pruned_set: rustc_hash::FxHashSet<(String, String)> = pruned_recs
        .iter()
        .flat_map(|r| {
            r.variants
                .iter()
                .map(move |v| (r.tag_lexc.clone(), v.clone()))
        })
        .collect();

    let missing: Vec<&(String, String)> = pruned_set.difference(&flat_set).collect();
    assert!(
        missing.is_empty(),
        "pruned composites must be a SUBSET of flat -- {} pruned entries are NOT in the flat \
         set (pruning must only ever REMOVE candidates, never add): {:?}",
        missing.len(),
        missing.iter().take(5).collect::<Vec<_>>()
    );

    let shrink_ratio = if pruned_report.pairs_probed > 0 {
        flat_report.pairs_probed as f64 / pruned_report.pairs_probed as f64
    } else {
        f64::INFINITY
    };
    println!(
        "Amharic pruning A/B: flat pairs_probed={} ({:?}), pruned pairs_probed={} ({:?}), \
         shrink={shrink_ratio:.2}x; flat entries={}, pruned entries={} (subset={})",
        flat_report.pairs_probed,
        flat_elapsed,
        pruned_report.pairs_probed,
        pruned_elapsed,
        flat_set.len(),
        pruned_set.len(),
        pruned_set.len() <= flat_set.len(),
    );
}
