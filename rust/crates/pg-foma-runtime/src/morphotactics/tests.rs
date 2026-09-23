use super::*;
use pg_grammar::model::LexEntryId;

/// Five-slot template covering every slot-skippability shape in one fixture: mandatory+non-vacuous, optional, mandatory+vacuous, and a rule (`mrX`) shared by two slots plus one (`mrOrphan`) referenced by none.
const FIXTURE_SLOTS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>MtSlots</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cC"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cX"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cO"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrA" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>a</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subA">
                <MorphologicalInput><PhoneticSequence id="stemA"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments><CopyFromInput index="stemA" /></MorphologicalOutput>
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
          <MorphologicalRule id="mrC" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>c</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subC">
                <MorphologicalInput><PhoneticSequence id="stemC"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>c</PhoneticShape></InsertSegments><CopyFromInput index="stemC" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>C</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrV" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>vac</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subV">
                <MorphologicalInput><PhoneticSequence id="stemV"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemV" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>V</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrD" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>d</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subD">
                <MorphologicalInput><PhoneticSequence id="stemD"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>d</PhoneticShape></InsertSegments><CopyFromInput index="stemD" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>D</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrX" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>x</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subX">
                <MorphologicalInput><PhoneticSequence id="stemX"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments><CopyFromInput index="stemX" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>X</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrOrphan" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>orphan</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subOrphan">
                <MorphologicalInput><PhoneticSequence id="stemO"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>o</PhoneticShape></InsertSegments><CopyFromInput index="stemO" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>Orphan</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate requiredPartsOfSpeech="posV">
            <Name>T</Name>
            <Slot morphologicalRules="mrA"><Name>s0</Name></Slot>
            <Slot morphologicalRules="mrB"><Name>s1</Name></Slot>
            <Slot optional="true" morphologicalRules="mrC mrX"><Name>s2</Name></Slot>
            <Slot morphologicalRules="mrV"><Name>s3</Name></Slot>
            <Slot morphologicalRules="mrD mrX"><Name>s4</Name></Slot>
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
</HermitCrabInput>"#;

/// Two strata + two categories, covering what `FIXTURE_SLOTS`'s single stratum cannot: the free-floor monotone property, a `required_syn_fs` mismatch, and the partial-root gate.
const FIXTURE_STRATA: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>MtStrata</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
      <PartOfSpeech id="posN"><Name>n</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cL"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrL0">
        <Name>S0</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrL0" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>l0</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subL0">
                <MorphologicalInput><PhoneticSequence id="stemL0"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>l</PhoneticShape></InsertSegments><CopyFromInput index="stemL0" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>L0</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrP" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>p</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subP">
                <MorphologicalInput><PhoneticSequence id="stemP"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments><CopyFromInput index="stemP" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>P</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate>
            <Name>TP</Name>
            <Slot morphologicalRules="mrP"><Name>sp0</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
          <LexicalEntry id="eKP" partOfSpeech="posV" partial="true">
            <Allomorphs><Allomorph id="aKP"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>KP</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrL1">
        <Name>S1</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrL1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>l1</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subL1">
                <MorphologicalInput><PhoneticSequence id="stemL1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>m</PhoneticShape></InsertSegments><CopyFromInput index="stemL1" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>L1</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrG" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>g</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subG">
                <MorphologicalInput><PhoneticSequence id="stemG"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>g</PhoneticShape></InsertSegments><CopyFromInput index="stemG" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>G</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate requiredPartsOfSpeech="posN">
            <Name>TG</Name>
            <Slot morphologicalRules="mrG"><Name>sg0</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries></LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#;

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}"))
}

/// Finds a rule by the XML `id` its owning morpheme's `xml_key` recorded, the loader's convention for every morpheme-bearing element.
fn mrule_id_of(g: &Grammar, xml_key: &str) -> MRuleId {
    for (i, r) in g.mrules.iter().enumerate() {
        let m = match r {
            MorphRuleDef::AffixProcess(d) => d.morpheme,
            MorphRuleDef::Realizational(d) => d.morpheme,
            MorphRuleDef::Compounding(_) => continue,
        };
        if g.morphemes[m.0 as usize].xml_key == xml_key {
            return MRuleId(i as u32);
        }
    }
    panic!("no rule with xml id {xml_key:?}");
}

fn entry_id_of(g: &Grammar, xml_key: &str) -> LexEntryId {
    LexEntryId(
        g.entries
            .iter()
            .position(|e| g.morphemes[e.morpheme.0 as usize].xml_key == xml_key)
            .unwrap_or_else(|| panic!("no entry with xml id {xml_key:?}")) as u32,
    )
}

fn entry_fs<'g>(g: &'g Grammar, xml_key: &str) -> &'g FeatureStruct {
    let e = &g.entries[entry_id_of(g, xml_key).0 as usize];
    g.fs_interner.get(e.syn_fs)
}

#[test]
fn slot_order_is_enforced() {
    let g = load(FIXTURE_SLOTS);
    let mt = MorphotacticIndex::build(&g);
    let fs = entry_fs(&g, "eK");
    let seed = ChainState::seed(&g, 0, false);

    // B (slot 1) is not first-reachable: slot 0 is mandatory/non-vacuous, and `seed.mid` is empty.
    let b = mrule_id_of(&g, "mrB");
    assert!(
        mt.next_state(&seed, b, fs, &g.fs_interner).is_none(),
        "slot 1's rule must not be reachable before slot 0 fires"
    );

    // A (slot 0) IS first-reachable.
    let a = mrule_id_of(&g, "mrA");
    let after_a = mt
        .next_state(&seed, a, fs, &g.fs_interner)
        .expect("slot 0's rule must be reachable from the seed state");
    assert_eq!(after_a.mid, vec![(0, 0)]);
    assert_eq!(
        after_a.free, None,
        "slot 0 alone does not complete the template"
    );
}

#[test]
fn fs_insensitive_transition_keeps_template_site_order() {
    let g = load(FIXTURE_SLOTS);
    let mt = MorphotacticIndex::build(&g);
    let seed = ChainState::seed(&g, 0, false);
    let b = mrule_id_of(&g, "mrB");
    assert!(
        mt.next_state_fs_insensitive(&seed, b).is_none(),
        "the reachability projection may ignore FS compatibility, but not mandatory slots"
    );

    let a = mrule_id_of(&g, "mrA");
    let after_a = mt
        .next_state_fs_insensitive(&seed, a)
        .expect("the first slot remains reachable in the FS-insensitive projection");
    assert_eq!(after_a.mid, vec![(0, 0)]);
}

#[test]
fn shared_sibling_rule_still_obeys_its_application_bound() {
    let g = load(FIXTURE_SLOTS);
    let mt = MorphotacticIndex::build(&g);
    let x = mrule_id_of(&g, "mrX");
    let seed = ChainState::seed(&g, 0, false);
    let a = mrule_id_of(&g, "mrA");
    let b = mrule_id_of(&g, "mrB");
    let after_a = mt
        .next_state_fs_insensitive(&seed, a)
        .expect("the first mandatory slot is reachable");
    let after_b = mt
        .next_state_fs_insensitive(&after_a, b)
        .expect("the second mandatory slot is reachable");
    let once = mt
        .next_state_fs_insensitive(&after_b, x)
        .expect("the sibling rule has at least one site reachable from the seed");
    assert!(
        mt.next_state_fs_insensitive(&once, x).is_none(),
        "a rule listed in sibling slots must not become an unbounded epsilon loop"
    );
}

#[test]
fn mandatory_non_vacuous_slot_blocks_jump() {
    let g = load(FIXTURE_SLOTS);
    let mt = MorphotacticIndex::build(&g);
    let fs = entry_fs(&g, "eK");
    let after_a = ChainState {
        free: None,
        mid: vec![(0, 0)],
        template_entry_disabled: false,
        applications: vec![0; g.mrules.len()],
    };
    // C lives in slot 2; slot 1 (mandatory, non-vacuous) sits strictly between and blocks it.
    let c = mrule_id_of(&g, "mrC");
    assert!(
        mt.next_state(&after_a, c, fs, &g.fs_interner).is_none(),
        "must not be able to jump the mandatory non-vacuous slot 1"
    );
}

#[test]
fn optional_slot_is_jumped() {
    let g = load(FIXTURE_SLOTS);
    let mt = MorphotacticIndex::build(&g);
    let fs = entry_fs(&g, "eK");
    let after_b = ChainState {
        free: None,
        mid: vec![(0, 1)],
        template_entry_disabled: false,
        applications: vec![0; g.mrules.len()],
    };
    // V lives in slot 3; slot 2 (optional) sits strictly between and must be jumpable.
    let v = mrule_id_of(&g, "mrV");
    let next = mt
        .next_state(&after_b, v, fs, &g.fs_interner)
        .expect("must be able to jump the optional slot 2");
    assert_eq!(next.mid, vec![(0, 3)]);
    assert_eq!(
        next.free, None,
        "slot 4 remains mandatory/non-vacuous -- not yet completable"
    );
}

#[test]
fn mandatory_but_vacuous_slot_is_jumped() {
    let g = load(FIXTURE_SLOTS);
    let mt = MorphotacticIndex::build(&g);
    let fs = entry_fs(&g, "eK");
    let after_c = ChainState {
        free: None,
        mid: vec![(0, 2)],
        template_entry_disabled: false,
        applications: vec![0; g.mrules.len()],
    };
    // D lives in slot 4; slot 3 is mandatory but vacuous (bare CopyFromInput) and must be jumpable.
    let d = mrule_id_of(&g, "mrD");
    let next = mt
        .next_state(&after_c, d, fs, &g.fs_interner)
        .expect("must be able to jump the mandatory-but-vacuous slot 3");
    assert_eq!(next.mid, vec![(0, 4)]);
}

#[test]
fn completion_grants_loose() {
    let g = load(FIXTURE_SLOTS);
    let mt = MorphotacticIndex::build(&g);
    let fs = entry_fs(&g, "eK");
    let after_c = ChainState {
        free: None,
        mid: vec![(0, 2)],
        template_entry_disabled: false,
        applications: vec![0; g.mrules.len()],
    };
    let d = mrule_id_of(&g, "mrD");
    let next = mt.next_state(&after_c, d, fs, &g.fs_interner).unwrap();
    // Slot 4 is the template's last slot, so firing D grants a fresh `free` floor at its owning stratum.
    assert_eq!(next.free, Some(0));
}

#[test]
fn free_floor_is_monotone_non_decreasing() {
    let g = load(FIXTURE_STRATA);
    let mt = MorphotacticIndex::build(&g);
    let fs = entry_fs(&g, "eK");
    let l0 = mrule_id_of(&g, "mrL0");
    let l1 = mrule_id_of(&g, "mrL1");

    let seed = ChainState::seed(&g, 0, false);
    let after_l0 = mt.next_state(&seed, l0, fs, &g.fs_interner).unwrap();
    assert_eq!(after_l0.free, Some(0));

    let after_l1 = mt.next_state(&after_l0, l1, fs, &g.fs_interner).unwrap();
    assert_eq!(
        after_l1.free,
        Some(1),
        "stratum 1's loose rule advances the floor"
    );

    // The free floor can only move forward, never back.
    assert!(
        mt.next_state(&after_l1, l0, fs, &g.fs_interner).is_none(),
        "the free floor must never decrease"
    );
}

#[test]
fn default_application_bound_allows_one_use() {
    let g = load(FIXTURE_STRATA);
    let mt = MorphotacticIndex::build(&g);
    let fs = entry_fs(&g, "eK");
    let rule = mrule_id_of(&g, "mrL0");
    let seed = ChainState::seed(&g, 0, false);

    let once = mt
        .next_state(&seed, rule, fs, &g.fs_interner)
        .expect("default multipleApplication=1 permits the first use");
    assert_eq!(once.applications[rule.0 as usize], 1);
    assert!(
        mt.next_state(&once, rule, fs, &g.fs_interner).is_none(),
        "default multipleApplication=1 must reject a second use"
    );
}

#[test]
fn authored_application_bound_allows_exact_count() {
    let xml = FIXTURE_STRATA.replacen(
        r#"<MorphologicalRule id="mrL0""#,
        r#"<MorphologicalRule id="mrL0" multipleApplication="2""#,
        1,
    );
    let g = load(&xml);
    let mt = MorphotacticIndex::build(&g);
    let fs = entry_fs(&g, "eK");
    let rule = mrule_id_of(&g, "mrL0");
    let seed = ChainState::seed(&g, 0, false);

    let once = mt.next_state(&seed, rule, fs, &g.fs_interner).unwrap();
    let twice = mt.next_state(&once, rule, fs, &g.fs_interner).unwrap();
    assert_eq!(twice.applications[rule.0 as usize], 2);
    assert!(
        mt.next_state(&twice, rule, fs, &g.fs_interner).is_none(),
        "multipleApplication=2 must reject a third use"
    );
}

#[test]
fn unpruned_transition_still_enforces_application_bound() {
    let g = load(FIXTURE_STRATA);
    let mt = MorphotacticIndex::build(&g);
    let rule = mrule_id_of(&g, "mrL0");
    let seed = ChainState::seed(&g, 0, false);

    let once = mt.next_state_unpruned(&seed, rule).unwrap();
    assert!(
        mt.next_state_unpruned(&once, rule).is_none(),
        "flat diagnostic exploration must not bypass authored application bounds"
    );
}

#[test]
fn partial_root_never_enters_template() {
    let g = load(FIXTURE_STRATA);
    let mt = MorphotacticIndex::build(&g);
    let fs = entry_fs(&g, "eK");
    let p = mrule_id_of(&g, "mrP"); // TP has no required_syn_fs -- always otherwise enterable.

    let seed_ok = ChainState::seed(&g, 0, false);
    assert!(
        mt.next_state(&seed_ok, p, fs, &g.fs_interner).is_some(),
        "a non-partial root must be able to enter TP"
    );

    let seed_partial = ChainState::seed(&g, 0, true);
    assert!(
        mt.next_state(&seed_partial, p, fs, &g.fs_interner)
            .is_none(),
        "a partial root must never enter any template"
    );
}

#[test]
fn template_required_syn_fs_gate_is_honored() {
    let g = load(FIXTURE_STRATA);
    let mt = MorphotacticIndex::build(&g);
    let fs = entry_fs(&g, "eK"); // posV
    let gr = mrule_id_of(&g, "mrG"); // TG requires posN -- must never unify with a posV root.
    let at_stratum1 = ChainState {
        free: Some(1),
        mid: Vec::new(),
        template_entry_disabled: false,
        applications: vec![0; g.mrules.len()],
    };
    assert!(
        mt.next_state(&at_stratum1, gr, fs, &g.fs_interner)
            .is_none(),
        "a posV word must not enter a template requiring posN"
    );
}

#[test]
fn rule_with_no_sites_returns_none() {
    let g = load(FIXTURE_SLOTS);
    let mt = MorphotacticIndex::build(&g);
    let fs = entry_fs(&g, "eK");
    let orphan = mrule_id_of(&g, "mrOrphan");
    let seed = ChainState::seed(&g, 0, false);
    assert!(mt.next_state(&seed, orphan, fs, &g.fs_interner).is_none());
    // Also true from an in-template state -- an orphan rule has no site anywhere, period.
    let mid_state = ChainState {
        free: None,
        mid: vec![(0, 1)],
        template_entry_disabled: false,
        applications: vec![0; g.mrules.len()],
    };
    assert!(mt
        .next_state(&mid_state, orphan, fs, &g.fs_interner)
        .is_none());
}

#[test]
fn state_normalization_is_deterministic() {
    let g = load(FIXTURE_SLOTS);
    let mt = MorphotacticIndex::build(&g);
    let fs = entry_fs(&g, "eK");
    let after_b = ChainState {
        free: None,
        mid: vec![(0, 1)],
        template_entry_disabled: false,
        applications: vec![0; g.mrules.len()],
    };
    // X sits in both slot 2 and slot 4's rule lists, both reachable from slot 1; one firing must merge both into a single, sorted/deduped `mid`.
    let x = mrule_id_of(&g, "mrX");
    let next = mt.next_state(&after_b, x, fs, &g.fs_interner).unwrap();
    assert_eq!(next.mid, vec![(0, 2), (0, 4)]);
    assert_eq!(
        next.free,
        Some(0),
        "slot 4's completion still grants a fresh floor"
    );

    // Calling again from the same input must be byte-for-byte identical (pure function).
    let next2 = mt.next_state(&after_b, x, fs, &g.fs_interner).unwrap();
    assert_eq!(next, next2);
}
