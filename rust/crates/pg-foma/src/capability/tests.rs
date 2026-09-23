//! Synthetic, delanguaged fixtures only (no natural-language names) -- built via
//! `pg_grammar::load` from hand-authored XML, mirroring `gate.rs`'s own test-module style
//! rather than hand-constructing a `Grammar` (which would require standing up every interner
//! field by hand; `load` is this workspace's own supported entry point for exactly this).

use pg_grammar::model::{MorphRuleDef, MprGroupOutput, PRuleId, PhonRuleDef};

use super::*;
use crate::enumerate::enumerate_default;
use crate::junctions::PhonologyProbe;
use crate::plan::{FragmentSpec, PlanNodeKind, Provenance};

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// `g`'s phonological rules in cascade order, as literal borrows, for `enumerate_default`'s pointer-identity `PRuleId` recovery.
fn prules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
    g.strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|&id| &g.prules[id.0 as usize])
        .collect()
}

/// Builds `g`'s `Plan` via the real `enumerate_default` seam, so these tests exercise the full pipeline end to end, not a hand-built `Plan`.
fn enumerated_plan(g: &Grammar) -> Plan {
    let ro = prules_in_order(g);
    let phon = PhonologyProbe::new(g);
    enumerate_default(g, &ro, phon.as_ref())
}

// ---- characterize(): ConfigPredicate triggers ----

/// A single, isolated, `multipleApplication`-default(1) `CompoundingRule` characterizes as `compounding.non-recursive` at the `ConfigPredicate` landing spot.
#[test]
fn characterize_marks_compounding_config_predicate_and_non_recursive() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1">
                  <Name>Compound</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    assert!(matches!(g.mrules[0], MorphRuleDef::Compounding(_)));

    let profile = characterize(&g);
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::Compounding
                && o.disposition == Disposition::ConfigPredicate),
        "Compounding must characterize at the ConfigPredicate landing spot: {:?}",
        profile.observations()
    );
    let details: Vec<_> = profile.compounding_details().collect();
    assert_eq!(details.len(), 1);
    assert!(
        !details[0].recursive,
        "a single isolated CompoundingRule must characterize non-recursive: {details:?}"
    );
    // The depth bound for the ordinary head+non-head shape is exactly 2 stems.
    assert_eq!(
        details[0].max_depth, 2,
        "an isolated multipleApplication-default(1) CompoundingRule must bound at exactly 2 \
         stems: {details:?}"
    );
}

/// A `CompoundingRule` with `multipleApplication > 1` self-feeds and must characterize `compounding.recursive`.
#[test]
fn characterize_marks_compounding_recursive_via_multiple_application() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1" multipleApplication="2">
                  <Name>Compound</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let profile = characterize(&g);
    let details: Vec<_> = profile.compounding_details().collect();
    assert_eq!(details.len(), 1);
    assert!(
        details[0].recursive,
        "multipleApplication > 1 must characterize compounding.recursive: {details:?}"
    );
    // max_depth = 1 (base) + max_apps(2) = 3 stems for this isolated self-feeding rule.
    assert_eq!(
        details[0].max_depth, 3,
        "multipleApplication=2 on an otherwise-isolated rule must bound at exactly 3 stems: \
         {details:?}"
    );
}

/// The depth bound must scale with `multipleApplication`, not just cross the recursive threshold; also a "never a hang" witness for the self-loop in the "feeds" graph.
#[test]
fn compounding_max_depth_scales_with_multiple_application() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1" multipleApplication="5">
                  <Name>Compound</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let profile = characterize(&g);
    let details: Vec<_> = profile.compounding_details().collect();
    assert_eq!(details.len(), 1);
    assert!(details[0].recursive);
    assert_eq!(
        details[0].max_depth, 6,
        "multipleApplication=5 on an otherwise-isolated rule must bound at exactly 6 stems \
         (1 base + 5 applications): {details:?}"
    );
}

/// Two `CompoundingRule`s sharing one stratum must both characterize recursive: either's output could feed the other's search.
#[test]
fn characterize_marks_compounding_recursive_via_distinct_rule_same_stratum() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr1 cr2">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1">
                  <Name>Compound1</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
                <CompoundingRule id="cr2">
                  <Name>Compound2</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h1"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n1"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n1" />
                        <CopyFromInput index="h1" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let profile = characterize(&g);
    let details: Vec<_> = profile.compounding_details().collect();
    assert_eq!(details.len(), 2);
    assert!(
        details.iter().all(|d| d.recursive),
        "two co-located CompoundingRules must both characterize recursive: {details:?}"
    );
    // max_depth(cr1) = 1 + max_apps(cr1) + max_apps(cr2) = 3, symmetrically for cr2.
    assert!(
        details.iter().all(|d| d.max_depth == 3),
        "two co-located CompoundingRules (max_apps=1 each) must both bound at exactly 3 \
         stems: {details:?}"
    );
}

/// Three co-located `CompoundingRule`s scale the bound to `1+1+1+1=4` and must all agree, a genuine mutual cycle of size 3.
#[test]
fn compounding_max_depth_scales_with_co_located_rule_count() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr1 cr2 cr3">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1">
                  <Name>Compound1</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
                <CompoundingRule id="cr2">
                  <Name>Compound2</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h1"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n1"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n1" />
                        <CopyFromInput index="h1" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
                <CompoundingRule id="cr3">
                  <Name>Compound3</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h2"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n2"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n2" />
                        <CopyFromInput index="h2" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let profile = characterize(&g);
    let details: Vec<_> = profile.compounding_details().collect();
    assert_eq!(details.len(), 3);
    assert!(details.iter().all(|d| d.recursive));
    assert!(
        details.iter().all(|d| d.max_depth == 4),
        "three co-located CompoundingRules (max_apps=1 each) must all bound at exactly 4 \
         stems (1 base + 1 + 1 + 1): {details:?}"
    );
}

/// The bound is not always symmetric: an earlier-stratum rule feeding a later one gives the earlier rule an isolated depth-2 bound while the later rule reflects being fed (depth-3).
#[test]
fn compounding_max_depth_is_asymmetric_across_strata() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
              <Name>Earlier</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1">
                  <Name>Compound1</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr2">
              <Name>Later</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr2">
                  <Name>Compound2</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h1"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n1"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n1" />
                        <CopyFromInput index="h1" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let profile = characterize(&g);
    let details: Vec<_> = profile.compounding_details().collect();
    assert_eq!(details.len(), 2);
    let earlier = details.iter().find(|d| d.rule.0 == 0).unwrap();
    let later = details.iter().find(|d| d.rule.0 == 1).unwrap();
    assert!(
        !earlier.recursive,
        "the earlier stratum's rule is never fed by anything -- must stay non-recursive: \
         {details:?}"
    );
    assert_eq!(
        earlier.max_depth, 2,
        "earlier rule's own isolated bound: {details:?}"
    );
    assert!(
        later.recursive,
        "the later stratum's rule IS fed by the earlier one -- must characterize recursive: \
         {details:?}"
    );
    assert_eq!(
        later.max_depth, 3,
        "later rule's bound must include the earlier rule's own max_apps contribution: \
         {details:?}"
    );
}

/// A direct, from-scratch proof that `detail.recursive == (detail.max_depth > 2)` holds across every shape the tests above exercise.
#[test]
fn compounding_max_depth_matches_compounding_recursive_boolean_exactly() {
    fn one_rule_xml(multiple_application: &str) -> String {
        format!(
            r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1"{multiple_application}>
                  <Name>Compound</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#
        )
    }

    for (xml, label) in [
        (one_rule_xml(""), "isolated, default multipleApplication=1"),
        (
            one_rule_xml(" multipleApplication=\"2\""),
            "isolated, self-feeding",
        ),
        (
            one_rule_xml(" multipleApplication=\"7\""),
            "isolated, self-feeding, larger bound",
        ),
    ] {
        let g = load(&xml);
        let recursive_set = compounding_recursive(&g);
        let depth_map = compounding_max_depth(&g);
        for mid in recursive_set
            .iter()
            .copied()
            .chain(depth_map.keys().copied())
            .collect::<HashSet<_>>()
        {
            let is_recursive = recursive_set.contains(&mid);
            let depth = depth_map[&mid];
            assert_eq!(
                is_recursive,
                depth > 2,
                "{label}: recursive={is_recursive} but max_depth={depth} for rule {mid:?} -- \
                 the equivalence compounding_max_depth's own doc claims must hold exactly"
            );
        }
    }
}

/// `MorphRuleOrder::Unordered` resolves to `ConfirmOnly`, never `Refuse`.
#[test]
fn characterize_marks_unordered_morph_rule_order_config_predicate() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
              <Name>S</Name>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    assert_eq!(g.strata[0].mrule_order, MorphRuleOrder::Unordered);

    let profile = characterize(&g);
    assert!(
        profile.observations().iter().any(|o| o.kind
            == CharacteristicKind::UnorderedMorphRuleApplication
            && o.disposition == Disposition::ConfigPredicate),
        "Unordered stratum must characterize ConfigPredicate: {:?}",
        profile.observations()
    );
    let details: Vec<_> = profile.unordered_stratum_details().collect();
    assert_eq!(details.len(), 1);
    assert_eq!(details[0].rule_count, 0);
}

/// `MprGroupOutput::Append` -> ConfirmOnly, `MprGroupOutput::Overwrite` -> ConfigPredicate.
#[test]
fn characterize_marks_append_confirm_only_and_overwrite_config_predicate() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <MorphologicalPhonologicalRuleFeatures>
            <MorphologicalPhonologicalRuleFeature id="mprA">A</MorphologicalPhonologicalRuleFeature>
            <MorphologicalPhonologicalRuleFeature id="mprB">B</MorphologicalPhonologicalRuleFeature>
            <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="append" features="mprA"><Name>GAppend</Name></MorphologicalPhonologicalRuleFeatureGroup>
            <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="overwrite" features="mprB"><Name>GOverwrite</Name></MorphologicalPhonologicalRuleFeatureGroup>
          </MorphologicalPhonologicalRuleFeatures>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    assert_eq!(g.mpr_groups.len(), 2);

    let profile = characterize(&g);
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::MprGroupAppend
                && o.disposition == Disposition::ConfirmOnly),
        "Append MPR group must characterize ConfirmOnly: {:?}",
        profile.observations()
    );
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::MprGroupOverwrite
                && o.disposition == Disposition::ConfigPredicate),
        "Overwrite MPR group must characterize ConfigPredicate: {:?}",
        profile.observations()
    );
}

/// `MorphRuleDef::Realizational` characterizes `ConfirmOnly` unconditionally: no shape has, or could have, a proven no-false-negative admission filter, since `IsBlocked` depends on the word's accumulated FS.
#[test]
fn characterize_marks_realizational_rule_confirm_only() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <RealizationalRule id="rr1">
                  <Name>Realiz</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="sub1">
                      <MorphologicalInput><PhoneticSequence id="s0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
                      <MorphologicalOutput><CopyFromInput index="s0" /></MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </RealizationalRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    assert!(matches!(g.mrules[0], MorphRuleDef::Realizational(_)));

    let profile = characterize(&g);
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::RealizationalMorphology
                && o.disposition == Disposition::ConfirmOnly),
        "RealizationalRule must characterize ConfirmOnly: {:?}",
        profile.observations()
    );
}

/// A `<MorphemeCoOccurrenceRule>` characterizes `ConfirmOnly` unconditionally: the co-occurrence check depends on other morphemes in the same final derivation, an unbounded-window fact no per-transition FST filter can see.
#[test]
fn characterize_marks_morpheme_co_occurrence_confirm_only() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <MorphologicalRule id="mrA">
                  <Name>A</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="subA">
                      <MorphologicalInput><PhoneticSequence id="s0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
                      <MorphologicalOutput><CopyFromInput index="s0" /></MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </MorphologicalRule>
                <MorphologicalRule id="mrB">
                  <Name>B</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="subB">
                      <MorphologicalInput><PhoneticSequence id="s1"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
                      <MorphologicalOutput><CopyFromInput index="s1" /></MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </MorphologicalRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
          <MorphemeCoOccurrenceRules>
            <MorphemeCoOccurrenceRule type="exclude" primaryMorpheme="mrA" otherMorphemes="mrB" adjacency="anywhere" />
          </MorphemeCoOccurrenceRules>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    assert!(
        g.morphemes.iter().any(|m| !m.co_occurrence.is_empty()),
        "fixture must attach at least one MorphemeCoOccurrenceRule to a morpheme"
    );

    let profile = characterize(&g);
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::CoOccurrenceConstraint
                && o.disposition == Disposition::ConfirmOnly),
        "MorphemeCoOccurrenceRule must characterize ConfirmOnly: {:?}",
        profile.observations()
    );
}

/// An `<AllomorphCoOccurrenceRule>` on a root allomorph characterizes the same `ConfirmOnly`.
#[test]
fn characterize_marks_allomorph_co_occurrence_confirm_only() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
                <LexicalEntry id="e2">
                  <Allomorphs><Allomorph id="a2"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
          <AllomorphCoOccurrenceRules>
            <AllomorphCoOccurrenceRule type="exclude" primaryAllomorph="a1" otherAllomorphs="a2" adjacency="anywhere" />
          </AllomorphCoOccurrenceRules>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    assert!(
        g.entries
            .iter()
            .any(|e| e.allomorphs.iter().any(|a| !a.co_occurrence.is_empty())),
        "fixture must attach at least one AllomorphCoOccurrenceRule to a root allomorph"
    );

    let profile = characterize(&g);
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::CoOccurrenceConstraint
                && o.disposition == Disposition::ConfirmOnly),
        "AllomorphCoOccurrenceRule must characterize ConfirmOnly: {:?}",
        profile.observations()
    );
}

/// Two tables with disjoint representations characterize `MultiTable`/`ConfigPredicate`.
#[test]
fn characterize_marks_disjoint_multi_table_config_predicate() {
    let g = load(TWO_TABLE_DISJOINT_XML);
    assert_eq!(g.char_tables.len(), 2);

    let profile = characterize(&g);
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::MultiTable
                && o.disposition == Disposition::ConfigPredicate),
        "multi-table (disjoint) must characterize ConfigPredicate: {:?}",
        profile.observations()
    );
    let detail = profile
        .multi_table_detail()
        .expect("MultiTable must carry a MultiTableDetail");
    assert_eq!(detail.table_count, 2);
    assert!(detail.representations_pairwise_disjoint);
    assert!(detail.shared_representation_witness.is_none());
}

/// Positive witness: the predicate admits `ConfirmOnly`, never `Refuse`, for disjoint tables.
#[test]
fn multi_table_predicate_confirm_only_when_tables_disjoint() {
    let g = load(TWO_TABLE_DISJOINT_XML);
    let profile = characterize(&g);
    let predicate = MultiTableFaithfulThreadingPredicate;
    // Node-agnostic (module doc) -- any PlanNodeKind works; reuse `leaf_for` for convenience.
    let verdict = predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0)));
    assert_eq!(
        verdict,
        PredicateVerdict::ConfirmOnly,
        "disjoint multi-table must be ConfirmOnly, not Refuse or Admit"
    );
}

/// Positive witness: two tables sharing a literal representation must `ConfirmOnly`, not `Refuse` — a false-negative risk closed at render time, not a false-positive one.
#[test]
fn multi_table_predicate_confirm_only_when_tables_share_a_representation() {
    let g = load(TWO_TABLE_OVERLAPPING_XML);
    assert_eq!(g.char_tables.len(), 2);
    let profile = characterize(&g);
    let detail = profile
        .multi_table_detail()
        .expect("MultiTable must carry a MultiTableDetail");
    assert!(!detail.representations_pairwise_disjoint);
    assert!(detail
        .shared_representation_witness
        .as_deref()
        .unwrap_or_default()
        .contains("\"p\""));

    let predicate = MultiTableFaithfulThreadingPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::ConfirmOnly,
        "overlapping-representation tables must ConfirmOnly, never Refuse, after the \
         cross-table aliasing fix"
    );
}

/// A single-table grammar never observes `MultiTable`, and the predicate vacuously `Admit`s.
#[test]
fn multi_table_predicate_admits_vacuously_for_single_table_grammar() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>SingleTable</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    assert_eq!(g.char_tables.len(), 1);
    let profile = characterize(&g);
    assert!(
        !profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::MultiTable),
        "a single-table grammar must never observe MultiTable at all"
    );
    let predicate = MultiTableFaithfulThreadingPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::Admit
    );
}

// ---- RightToLeftRewrite ----

const RTL_PLAIN_XML: &str = r#"<HermitCrabInput><Language><Name>RtlPlain</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <PhonologicalRuleDefinitions>
        <PhonologicalRule id="prRtl" multipleApplicationOrder="rightToLeftIterative">
          <Name>rtlDemo</Name>
          <PhoneticInput><PhoneticSequence><Segment segment="ca" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><Segment segment="cb" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
      </PhonologicalRuleDefinitions>
      <Strata><Stratum characterDefinitionTable="t1" phonologicalRules="prRtl"><Name>S</Name></Stratum></Strata>
    </Language></HermitCrabInput>"#;

/// A plain, in-shape `Dir::RightToLeft` rule characterizes `ConfigPredicate` with `reversal_construction_attempted == true`.
#[test]
fn characterize_marks_right_to_left_rewrite_config_predicate_when_shape_supported() {
    let g = load(RTL_PLAIN_XML);
    let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(r.dir, Dir::RightToLeft);

    let profile = characterize(&g);
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::RightToLeftRewrite
                && o.disposition == Disposition::ConfigPredicate),
        "Dir::RightToLeft must characterize ConfigPredicate: {:?}",
        profile.observations()
    );
    let detail = profile
        .right_to_left_detail(PRuleId(0))
        .expect("RightToLeftRewrite must carry a RightToLeftRewriteDetail");
    assert!(
        detail.reversal_construction_attempted,
        "a plain fixed-segment, no-environment rule is exactly the shape the reversal \
         construction supports"
    );
}

/// Positive witness: the predicate returns `ConfirmOnly`, never `Admit`, for an in-shape rule.
#[test]
fn right_to_left_predicate_confirm_only_for_supported_shape() {
    let g = load(RTL_PLAIN_XML);
    let profile = characterize(&g);
    let predicate = RightToLeftRewriteFaithfulReversalPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::ConfirmOnly,
        "an in-shape RTL rule must be ConfirmOnly, never Refuse or Admit"
    );
}

/// A plain `Dir::LeftToRight` rule never observes `RightToLeftRewrite`, and the predicate vacuously `Admit`s.
#[test]
fn right_to_left_predicate_admits_vacuously_for_left_to_right_rule() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>LtrPlain</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="prLtr">
              <Name>ltrDemo</Name>
              <PhoneticInput><PhoneticSequence><Segment segment="ca" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><Segment segment="cb" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata><Stratum characterDefinitionTable="t1" phonologicalRules="prLtr"><Name>S</Name></Stratum></Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(r.dir, Dir::LeftToRight);
    let profile = characterize(&g);
    assert!(
        !profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::RightToLeftRewrite),
        "a LeftToRight rule must never observe RightToLeftRewrite at all"
    );
    let predicate = RightToLeftRewriteFaithfulReversalPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::Admit
    );
}

/// A positive `ConfirmOnly` witness: a `Dir::RightToLeft` rule whose LHS is a genuinely unbounded `Quantifier`, which `pattern_slots` accepts, so the predicate must `ConfirmOnly`, never silently `Admit`.
#[test]
fn right_to_left_predicate_confirm_only_for_unbounded_quantifier_shaped_rule() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>RtlQuantifier</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cb" /></SegmentNaturalClass></NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="prRtlQ" multipleApplicationOrder="rightToLeftIterative">
              <Name>rtlQuantifierDemo</Name>
              <PhoneticInput><PhoneticSequence>
                <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
              </PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><Segment segment="cb" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata><Stratum characterDefinitionTable="t1" phonologicalRules="prRtlQ"><Name>S</Name></Stratum></Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(r.dir, Dir::RightToLeft);

    let profile = characterize(&g);
    let detail = profile
        .right_to_left_detail(PRuleId(0))
        .expect("RightToLeftRewrite must carry a RightToLeftRewriteDetail");
    assert!(
        detail.reversal_construction_attempted,
        "a well-formed unbounded Quantifier-shaped LHS is now within \
         crate::replace::pattern_slots' own supported shape"
    );

    let predicate = RightToLeftRewriteFaithfulReversalPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::ConfirmOnly,
        "an unbounded Quantifier-shaped RTL rule must be ConfirmOnly, never Refuse or Admit"
    );
}

// ---- `Anchor`/same-table `Segments` do not disqualify; an unambiguous disagree-polarity alpha var no longer does either ----

/// Positive witness: an `Anchor`-shaped `Dir::RightToLeft` rule characterizes `reversal_construction_attempted == true`, and the predicate `ConfirmOnly`s it.
#[test]
fn right_to_left_predicate_confirm_only_for_anchor_shaped_rule() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>RtlAnchor</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="prRtlAnchor" multipleApplicationOrder="rightToLeftIterative">
              <Name>rtlAnchorDemo</Name>
              <PhoneticInput><PhoneticSequence><Segment segment="ca" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><Segment segment="cb" /></PhoneticSequence></PhoneticOutput>
                  <Environment><RightEnvironment><PhoneticTemplate finalBoundaryCondition="true"><PhoneticSequence /></PhoneticTemplate></RightEnvironment></Environment>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata><Stratum characterDefinitionTable="t1" phonologicalRules="prRtlAnchor"><Name>S</Name></Stratum></Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(r.dir, Dir::RightToLeft);
    assert!(
        matches!(
            r.subrules[0].right_env.as_ref().unwrap().nodes.as_slice(),
            [pg_grammar::model::PatternNode::Anchor(
                pg_grammar::model::AnchorSide::Right
            )]
        ),
        "fixture must lower to a right_env containing JUST a trailing Anchor(Right) node: {:?}",
        r.subrules[0].right_env
    );

    let profile = characterize(&g);
    let detail = profile
        .right_to_left_detail(PRuleId(0))
        .expect("RightToLeftRewrite must carry a RightToLeftRewriteDetail");
    assert!(
        detail.reversal_construction_attempted,
        "an Anchor-shaped right environment is now within crate::replace::pattern_slots' own \
         supported shape (task 4.2)"
    );
    assert_eq!(
        detail.unsupported_reason, None,
        "nothing to diagnose once the construction is attempted"
    );

    let predicate = RightToLeftRewriteFaithfulReversalPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::ConfirmOnly,
        "an Anchor-shaped RTL rule must be ConfirmOnly, never Refuse or Admit"
    );
}

/// Positive witness: a same-table `Segments` node in a `Dir::RightToLeft` rule's right environment characterizes `reversal_construction_attempted == true`, and the predicate `ConfirmOnly`s it.
#[test]
fn right_to_left_predicate_confirm_only_for_same_table_segments_shaped_rule() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>RtlSegments</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="prRtlSeg" multipleApplicationOrder="rightToLeftIterative">
              <Name>rtlSegmentsDemo</Name>
              <PhoneticInput><PhoneticSequence><Segment segment="ca" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><Segment segment="cb" /></PhoneticSequence></PhoneticOutput>
                  <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence>
                    <Segments><PhoneticShape>a</PhoneticShape></Segments>
                  </PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata><Stratum characterDefinitionTable="t1" phonologicalRules="prRtlSeg"><Name>S</Name></Stratum></Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(r.dir, Dir::RightToLeft);
    assert!(
        matches!(
            r.subrules[0].right_env.as_ref().unwrap().nodes.as_slice(),
            [pg_grammar::model::PatternNode::Segments { .. }]
        ),
        "fixture must lower to a right_env containing a Segments node: {:?}",
        r.subrules[0].right_env
    );

    let profile = characterize(&g);
    let detail = profile
        .right_to_left_detail(PRuleId(0))
        .expect("RightToLeftRewrite must carry a RightToLeftRewriteDetail");
    assert!(
        detail.reversal_construction_attempted,
        "a same-table Segments node is now within crate::replace::pattern_slots' own \
         supported shape (task 4.2)"
    );
    assert_eq!(detail.unsupported_reason, None);

    let predicate = RightToLeftRewriteFaithfulReversalPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::ConfirmOnly,
        "a same-table-Segments-shaped RTL rule must be ConfirmOnly, never Refuse or Admit"
    );
}

/// Positive witness: a cross-table `Segments` node is admitted as a table-qualified feature constraint, staying `ConfirmOnly` since confirmation prunes the recall-safe token union.
#[test]
fn right_to_left_predicate_accepts_cross_table_segments_for_confirmation() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>RtlCrossTableSegments</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <CharacterDefinitionTable id="t2"><Name>Other</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="prRtlCrossSeg" multipleApplicationOrder="rightToLeftIterative">
              <Name>rtlCrossTableSegmentsDemo</Name>
              <PhoneticInput><PhoneticSequence><Segment segment="ca" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><Segment segment="cb" /></PhoneticSequence></PhoneticOutput>
                  <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence>
                    <Segments characterDefinitionTable="t2"><PhoneticShape>x</PhoneticShape></Segments>
                  </PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata><Stratum characterDefinitionTable="t1" phonologicalRules="prRtlCrossSeg"><Name>S</Name></Stratum></Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(r.dir, Dir::RightToLeft);
    assert_eq!(
        g.char_tables.len(),
        2,
        "fixture must declare two distinct tables"
    );

    let profile = characterize(&g);
    let detail = profile
        .right_to_left_detail(PRuleId(0))
        .expect("RightToLeftRewrite must carry a RightToLeftRewriteDetail");
    assert!(
        detail.reversal_construction_attempted,
        "cross-table Segments must retain table identity and reach reversal construction"
    );
    assert_eq!(detail.unsupported_reason, None);

    let predicate = RightToLeftRewriteFaithfulReversalPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::ConfirmOnly,
        "cross-table Segments must be recall-safe candidate generation, never Refuse or Admit"
    );
}

/// Positive witness: a disagree-polarity `AlphaVariable` now lowers and resolves like any other alpha occurrence, so an otherwise-in-scope `Dir::RightToLeft` rule carrying one characterizes `reversal_construction_attempted == true` and the predicate `ConfirmOnly`s it — this was never reversal-specific (`resolve_alpha_tuples`' own joint-polarity filter, shared by every `Dir`).
#[test]
fn right_to_left_predicate_confirm_only_for_disagree_polarity_alpha_var_shaped_rule() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>RtlDisagree</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <PhonologicalFeatureSystem>
            <SymbolicFeature id="featA"><Name>a</Name><Symbols><Symbol id="symX">x</Symbol><Symbol id="symY">y</Symbol></Symbols></SymbolicFeature>
          </PhonologicalFeatureSystem>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations><FeatureValue feature="featA" symbolValues="symX" /></SegmentDefinition>
              <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featA" symbolValues="symY" /></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="prRtlDisagree" multipleApplicationOrder="rightToLeftIterative">
              <Name>rtlDisagreeDemo</Name>
              <VariableFeatures><VariableFeature id="var1" name="a" phonologicalFeature="featA" /></VariableFeatures>
              <PhoneticInput><PhoneticSequence>
                <SimpleContext naturalClass="ncAll"><AlphaVariables><AlphaVariable variableFeature="var1" polarity="minus" /></AlphaVariables></SimpleContext>
              </PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><Segment segment="cb" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata><Stratum characterDefinitionTable="t1" phonologicalRules="prRtlDisagree"><Name>S</Name></Stratum></Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(r.dir, Dir::RightToLeft);
    let pg_grammar::model::PatternNode::Context(sc) = &r.lhs.nodes[0] else {
        panic!("expected a Context node at lhs.nodes[0]: {:?}", r.lhs.nodes);
    };
    assert!(
        sc.vars.iter().any(|v| !v.plus),
        "fixture must actually carry a disagree-polarity (plus == false) AlphaVar: {sc:?}"
    );

    let profile = characterize(&g);
    let detail = profile
        .right_to_left_detail(PRuleId(0))
        .expect("RightToLeftRewrite must carry a RightToLeftRewriteDetail");
    assert!(
        detail.reversal_construction_attempted,
        "a disagree-polarity alpha var must now be admitted -- resolve_alpha_tuples' \
         joint-polarity filter implements both agree (bitwise overlap) and disagree (bitwise \
         non-overlap)"
    );
    assert_eq!(
        detail.unsupported_reason, None,
        "nothing left to diagnose once reversal_construction_attempted is true"
    );

    let predicate = RightToLeftRewriteFaithfulReversalPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::ConfirmOnly,
        "a disagree-polarity-shaped RTL rule must be ConfirmOnly, never Refuse or Admit"
    );
}

// ---- Metathesis ----

/// Two adjacent, distinct, singleton-class switch segments, no `multipleApplicationOrder` (defaults `Dir::LeftToRight`), the well-formed switch-tag convention.
const METATHESIS_PLAIN_XML: &str = r#"<HermitCrabInput><Language><Name>MetaPlain</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="cq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses>
        <SegmentNaturalClass id="ncQ"><Name>Q</Name><Segment segment="cq" /></SegmentNaturalClass>
        <SegmentNaturalClass id="ncP"><Name>P</Name><Segment segment="cp" /></SegmentNaturalClass>
      </NaturalClasses>
      <PhonologicalRuleDefinitions>
        <MetathesisRule id="mrPlain" leftSwitch="swP" rightSwitch="swQ">
          <Name>metaPlainDemo</Name>
          <StructuralDescription>
            <PhoneticTemplate>
              <PhoneticSequence>
                <SimpleContext id="swQ" naturalClass="ncQ" />
                <SimpleContext id="swP" naturalClass="ncP" />
              </PhoneticSequence>
            </PhoneticTemplate>
          </StructuralDescription>
        </MetathesisRule>
      </PhonologicalRuleDefinitions>
      <Strata><Stratum characterDefinitionTable="t1" phonologicalRules="mrPlain"><Name>S</Name></Stratum></Strata>
    </Language></HermitCrabInput>"#;

/// A plain, in-shape `Dir::LeftToRight` metathesis rule characterizes `ConfigPredicate` with `swap_construction_attempted == true`.
#[test]
fn characterize_marks_metathesis_config_predicate_when_shape_supported() {
    let g = load(METATHESIS_PLAIN_XML);
    let PhonRuleDef::Metathesis(m) = &g.prules[0] else {
        panic!("expected a Metathesis-kind rule");
    };
    assert_eq!(m.dir, Dir::LeftToRight);

    let profile = characterize(&g);
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::Metathesis
                && o.disposition == Disposition::ConfigPredicate),
        "PhonRuleDef::Metathesis must characterize ConfigPredicate: {:?}",
        profile.observations()
    );
    let detail = profile
        .metathesis_detail(PRuleId(0))
        .expect("Metathesis must carry a MetathesisDetail");
    assert!(
        detail.swap_construction_attempted,
        "a plain two-singleton-switch, no-environment rule is exactly the shape the swap \
         construction supports"
    );
}

/// Positive witness: the predicate returns `ConfirmOnly`, never `Admit`, for an in-shape rule.
#[test]
fn metathesis_predicate_confirm_only_for_supported_shape() {
    let g = load(METATHESIS_PLAIN_XML);
    let profile = characterize(&g);
    let predicate = MetathesisFaithfulSwapPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::ConfirmOnly,
        "an in-shape metathesis rule must be ConfirmOnly, never Refuse or Admit"
    );
}

/// A grammar with no `PhonRuleDef::Metathesis` never observes it, and the predicate vacuously `Admit`s.
#[test]
fn metathesis_predicate_admits_vacuously_for_rule_without_metathesis() {
    let g = load(RTL_PLAIN_XML);
    let profile = characterize(&g);
    assert!(
        !profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::Metathesis),
        "a grammar with no MetathesisRule must never observe Metathesis at all"
    );
    let predicate = MetathesisFaithfulSwapPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::Admit
    );
}

/// Positive witness: a `Dir::RightToLeft` metathesis rule, otherwise identical to `METATHESIS_PLAIN_XML`, characterizes `swap_construction_attempted == true` and `ConfirmOnly`, this test's job being only the capability-gate verdict.
#[test]
fn metathesis_predicate_confirm_only_for_right_to_left_rule() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>MetaRtl</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <SegmentNaturalClass id="ncQ"><Name>Q</Name><Segment segment="cq" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncP"><Name>P</Name><Segment segment="cp" /></SegmentNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <MetathesisRule id="mrRtl" leftSwitch="swP" rightSwitch="swQ" multipleApplicationOrder="rightToLeftIterative">
              <Name>metaRtlDemo</Name>
              <StructuralDescription>
                <PhoneticTemplate>
                  <PhoneticSequence>
                    <SimpleContext id="swQ" naturalClass="ncQ" />
                    <SimpleContext id="swP" naturalClass="ncP" />
                  </PhoneticSequence>
                </PhoneticTemplate>
              </StructuralDescription>
            </MetathesisRule>
          </PhonologicalRuleDefinitions>
          <Strata><Stratum characterDefinitionTable="t1" phonologicalRules="mrRtl"><Name>S</Name></Stratum></Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let PhonRuleDef::Metathesis(m) = &g.prules[0] else {
        panic!("expected a Metathesis-kind rule");
    };
    assert_eq!(m.dir, Dir::RightToLeft);

    let profile = characterize(&g);
    let detail = profile
        .metathesis_detail(PRuleId(0))
        .expect("Metathesis must carry a MetathesisDetail");
    assert!(
        detail.swap_construction_attempted,
        "Dir::RightToLeft is now IN scope (task 4.6): the structural admission floor is \
         Dir-agnostic, and this rule's own pattern shape (two singleton-class switches, no \
         environment) is exactly what it accepts"
    );

    let predicate = MetathesisFaithfulSwapPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::ConfirmOnly,
        "a Dir::RightToLeft metathesis rule with an otherwise-supported pattern shape must be \
         ConfirmOnly, never Refuse or Admit"
    );
}

/// A trailing `Slot::Anchor` is erased rather than enforced, so the swap construction is attempted and the verdict is `ConfirmOnly`, never `Refuse`.
#[test]
fn metathesis_predicate_confirm_only_for_anchor_shaped_pattern() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>MetaAnchor</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <SegmentNaturalClass id="ncQ"><Name>Q</Name><Segment segment="cq" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncP"><Name>P</Name><Segment segment="cp" /></SegmentNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <MetathesisRule id="mrAnchor" leftSwitch="swP" rightSwitch="swQ">
              <Name>metaAnchorDemo</Name>
              <StructuralDescription>
                <PhoneticTemplate finalBoundaryCondition="true">
                  <PhoneticSequence>
                    <SimpleContext id="swQ" naturalClass="ncQ" />
                    <SimpleContext id="swP" naturalClass="ncP" />
                  </PhoneticSequence>
                </PhoneticTemplate>
              </StructuralDescription>
            </MetathesisRule>
          </PhonologicalRuleDefinitions>
          <Strata><Stratum characterDefinitionTable="t1" phonologicalRules="mrAnchor"><Name>S</Name></Stratum></Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let profile = characterize(&g);
    let detail = profile
        .metathesis_detail(PRuleId(0))
        .expect("Metathesis must carry a MetathesisDetail");
    assert!(
        detail.swap_construction_attempted,
        "an edge Anchor must use the anchor-erased ConfirmOnly swap superset"
    );

    let predicate = MetathesisFaithfulSwapPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::ConfirmOnly
    );
}

// ---- CircumfixOutputAction ----

/// A 2-part LHS (`qA`, `qB`) whose RHS `CopyFromInput`s only `qA`: a null-role subtractive shape that `classify_affix` reads as `Role::None`, the in-scope case `is_structural_rule` admits.
const CIRCUMFIX_STRUCTURAL_XML: &str = r#"<HermitCrabInput><Language><Name>CircStruct</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRules="mrDropOk">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <MorphologicalRule id="mrDropOk">
              <Name>dropOk</Name>
              <MorphologicalSubrules>
                <MorphologicalSubrule id="subDropOk">
                  <MorphologicalInput>
                    <PhoneticSequence id="qA"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                    <PhoneticSequence id="qB"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                  </MorphologicalInput>
                  <MorphologicalOutput>
                    <CopyFromInput index="qA" />
                  </MorphologicalOutput>
                </MorphologicalSubrule>
              </MorphologicalSubrules>
            </MorphologicalRule>
          </MorphologicalRuleDefinitions>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

/// Same 2-part-LHS-drop shape, but the RHS uses `ModifyFromInput` instead of `CopyFromInput`: `classify_affix` reads this as `Role::Process`, which `is_structural_rule` admits unconditionally.
const CIRCUMFIX_PROCESS_XML: &str = r#"<HermitCrabInput><Language><Name>CircProcess</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRules="mrDropProcess">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <MorphologicalRule id="mrDropProcess">
              <Name>dropProcess</Name>
              <MorphologicalSubrules>
                <MorphologicalSubrule id="subDropProcess">
                  <MorphologicalInput>
                    <PhoneticSequence id="pA"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                    <PhoneticSequence id="pB"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                  </MorphologicalInput>
                  <MorphologicalOutput>
                    <ModifyFromInput index="pA"><SimpleContext naturalClass="ncAll" /></ModifyFromInput>
                  </MorphologicalOutput>
                </MorphologicalSubrule>
              </MorphologicalSubrules>
            </MorphologicalRule>
          </MorphologicalRuleDefinitions>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

/// A 3-part LHS whose RHS interleaves an `InsertSegments` between two `CopyFromInput`s and drops `qC`: `classify_affix` reads `Role::Infix`, which `is_structural_rule` admits since census C4 (the drop-aware arm).
const CIRCUMFIX_INFIX_DROP_XML: &str = r#"<HermitCrabInput><Language><Name>CircInfix</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRules="mrDropInfix">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <MorphologicalRule id="mrDropInfix">
              <Name>dropInfix</Name>
              <MorphologicalSubrules>
                <MorphologicalSubrule id="subDropInfix">
                  <MorphologicalInput>
                    <PhoneticSequence id="qA"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                    <PhoneticSequence id="qB"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                    <PhoneticSequence id="qC"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                  </MorphologicalInput>
                  <MorphologicalOutput>
                    <CopyFromInput index="qA" />
                    <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                    <CopyFromInput index="qB" />
                  </MorphologicalOutput>
                </MorphologicalSubrule>
              </MorphologicalSubrules>
            </MorphologicalRule>
          </MorphologicalRuleDefinitions>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

/// A 2-part LHS whose reduplicating RHS copies `qA` twice and drops `qB`.
const CIRCUMFIX_REDUPLICATION_DROP_XML: &str = r#"<HermitCrabInput><Language><Name>CircRedupDrop</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRules="mrDropRedup">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <MorphologicalRule id="mrDropRedup">
              <Name>dropRedup</Name>
              <MorphologicalSubrules>
                <MorphologicalSubrule id="subDropRedup">
                  <MorphologicalInput>
                    <PhoneticSequence id="qA"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                    <PhoneticSequence id="qB"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                  </MorphologicalInput>
                  <MorphologicalOutput redupMorphType="suffix">
                    <CopyFromInput index="qA" />
                    <CopyFromInput index="qA" />
                  </MorphologicalOutput>
                </MorphologicalSubrule>
              </MorphologicalSubrules>
            </MorphologicalRule>
          </MorphologicalRuleDefinitions>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

/// Allomorph 0 drops `qB` classifying `Role::Reduplication`; allomorph 1 drops `rB` classifying `Role::Prefix`.
/// See `docs/research/circumfix-composite-precedence-census.md`, C5.
const CIRCUMFIX_REDUP_FIRST_PREFIX_DROP_LATER_XML: &str = r#"<HermitCrabInput><Language><Name>CircRedupFirstPrefixDropLater</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRules="mrRedupThenPrefixDrop">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <MorphologicalRule id="mrRedupThenPrefixDrop">
              <Name>redupThenPrefixDrop</Name>
              <MorphologicalSubrules>
                <MorphologicalSubrule id="subRedup">
                  <MorphologicalInput>
                    <PhoneticSequence id="qA"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                    <PhoneticSequence id="qB"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                  </MorphologicalInput>
                  <MorphologicalOutput redupMorphType="suffix">
                    <CopyFromInput index="qA" />
                    <CopyFromInput index="qA" />
                  </MorphologicalOutput>
                </MorphologicalSubrule>
                <MorphologicalSubrule id="subPrefixDrop">
                  <MorphologicalInput>
                    <PhoneticSequence id="rA"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                    <PhoneticSequence id="rB"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                  </MorphologicalInput>
                  <MorphologicalOutput>
                    <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                    <CopyFromInput index="rA" />
                  </MorphologicalOutput>
                </MorphologicalSubrule>
              </MorphologicalSubrules>
            </MorphologicalRule>
          </MorphologicalRuleDefinitions>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

fn mrule_leaf(rule: MRuleId) -> PlanNodeKind {
    // Node-agnostic: `CircumfixStructuralCompositePredicate::evaluate` ignores `plan_node` entirely.
    PlanNodeKind::Leaf {
        fragment: FragmentSpec::LexiconFragment { entries: None },
        provenance: Provenance::MorphRule(rule),
    }
}

/// Pure ablaut: ONE input part, mutated in place, nothing copied and nothing dropped. `classify_affix` reads this as `Role::Process`; `allomorph_drops_lhs_material` cannot fire because the input has one part.
const ABLAUT_PROCESS_XML: &str = r#"<HermitCrabInput><Language><Name>Ablaut</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRules="mrAblaut">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <MorphologicalRule id="mrAblaut">
              <Name>ablaut</Name>
              <MorphologicalSubrules>
                <MorphologicalSubrule id="subAblaut">
                  <MorphologicalInput>
                    <PhoneticSequence id="pA"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                  </MorphologicalInput>
                  <MorphologicalOutput>
                    <ModifyFromInput index="pA"><SimpleContext naturalClass="ncAll" /></ModifyFromInput>
                  </MorphologicalOutput>
                </MorphologicalSubrule>
              </MorphologicalSubrules>
            </MorphologicalRule>
          </MorphologicalRuleDefinitions>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

/// A grammar whose morphology is ENTIRELY in-place mutation must not characterize as carrying nothing.
#[test]
fn characterize_marks_process_morphology_for_a_pure_ablaut_allomorph() {
    let g = load(ABLAUT_PROCESS_XML);
    let profile = characterize(&g);

    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::ProcessMorphology),
        "a Modify-only allomorph must be observed as ProcessMorphology -- without it the gate              reports a clean grammar it structurally cannot see: {:?}",
        profile.observations()
    );
}

/// `stemName` restricts one root allomorph; `characterize` must see it without any rule ever applying.
const STEM_NAME_ROOT_ALLOMORPH_XML: &str = r#"<HermitCrabInput><Language><Name>StemNameCap</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <StemNames>
        <StemName id="sn1" partsOfSpeech="posV"><Name>SN1</Name><Regions><Region/></Regions></StemName>
      </StemNames>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <Strata>
        <Stratum characterDefinitionTable="t1">
          <Name>S</Name>
          <LexicalEntries>
            <LexicalEntry id="eRoot" partOfSpeech="posV">
              <Allomorphs>
                <Allomorph id="aRoot" stemName="sn1"><PhoneticShape>a</PhoneticShape></Allomorph>
              </Allomorphs>
              <MorphemeId>ROOT</MorphemeId>
              <Gloss>root</Gloss>
            </LexicalEntry>
          </LexicalEntries>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

/// A `RootAllomorphDef::stem_name` occurrence must be observed as `StemName`, regardless of whether any rule ever applies to reach it.
#[test]
fn characterize_marks_stem_name_for_a_stem_name_restricted_root_allomorph() {
    let g = load(STEM_NAME_ROOT_ALLOMORPH_XML);
    let profile = characterize(&g);

    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::StemName),
        "a stemName-restricted root allomorph must be observed as StemName -- without it the gate reports a clean grammar it structurally cannot see: {:?}",
        profile.observations()
    );
}

/// Two root allomorphs of one entry with identical (empty) `environments`/`is_bound` free-fluctuate; a differing pair (one environment-restricted) must not.
const FREE_FLUCTUATION_ROOT_ALLOMORPH_XML: &str = r#"<HermitCrabInput><Language><Name>FreeFluctCap</Name>
      <PartsOfSpeech><PartOfSpeech id="posRoot"><Name>root</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="co"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cl"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <Strata>
        <Stratum characterDefinitionTable="t1">
          <Name>S</Name>
          <LexicalEntries>
            <LexicalEntry id="eAlt" partOfSpeech="posRoot">
              <Allomorphs>
                <Allomorph id="aPol"><PhoneticShape>pol</PhoneticShape></Allomorph>
                <Allomorph id="aPel"><PhoneticShape>pel</PhoneticShape></Allomorph>
              </Allomorphs>
              <MorphemeId>ALT</MorphemeId>
              <Gloss>alt</Gloss>
            </LexicalEntry>
          </LexicalEntries>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

/// Two allomorphs comparing `root_constraints_equal` must be observed as `FreeFluctuation`.
#[test]
fn characterize_marks_free_fluctuation_for_two_equal_constraint_root_allomorphs() {
    let g = load(FREE_FLUCTUATION_ROOT_ALLOMORPH_XML);
    assert_eq!(g.entries[0].allomorphs.len(), 2);
    assert!(pg_rules::validity::root_constraints_equal(
        &g.entries[0].allomorphs[0],
        &g.entries[0].allomorphs[1]
    ));

    let profile = characterize(&g);
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::FreeFluctuation),
        "two root allomorphs with equal environments/is_bound must be observed as FreeFluctuation: {:?}",
        profile.observations()
    );
}

/// The in-scope `Role::None` drop shape characterizes `ConfigPredicate` with `structural_composite_attempted == true`.
#[test]
fn characterize_marks_circumfix_output_action_config_predicate_when_structural() {
    let g = load(CIRCUMFIX_STRUCTURAL_XML);
    assert!(matches!(g.mrules[0], MorphRuleDef::AffixProcess(_)));
    assert!(
        crate::emit::is_structural_rule(&g, MRuleId(0)),
        "the real compile path must route this rule through build_structural_composites"
    );

    let profile = characterize(&g);
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::CircumfixOutputAction
                && o.disposition == Disposition::ConfigPredicate),
        "a 2-part-LHS drop must characterize CircumfixOutputAction/ConfigPredicate: {:?}",
        profile.observations()
    );
    let detail = profile
        .circumfix_output_action_details()
        .find(|d| d.rule == MRuleId(0) && d.allomorph_index == 0)
        .expect("must carry a CircumfixOutputActionDetail for mrule 0 allomorph 0");
    assert!(
        detail.structural_composite_attempted,
        "Role::None with rhs_drops_lhs_material must reach build_structural_composites"
    );
}

/// The `Role::Process` drop shape is still observed as `CircumfixOutputAction`, independent of which `OutputAction` variant realizes it.
#[test]
fn characterize_marks_circumfix_output_action_structural_for_process_role() {
    let g = load(CIRCUMFIX_PROCESS_XML);
    assert!(
        crate::emit::is_structural_rule(&g, MRuleId(0)),
        "a process rule must reach the oracle-backed structural composite"
    );

    let profile = characterize(&g);
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::CircumfixOutputAction),
        "a Modify-only 2-part-LHS drop must still observe CircumfixOutputAction: {:?}",
        profile.observations()
    );
    let detail = profile
        .circumfix_output_action_details()
        .find(|d| d.rule == MRuleId(0) && d.allomorph_index == 0)
        .expect("must carry a CircumfixOutputActionDetail for mrule 0 allomorph 0");
    assert!(
        detail.structural_composite_attempted,
        "process rules must report the structural-composite route"
    );
}

/// Positive witness: the predicate returns `ConfirmOnly`, never `Admit`, for the in-scope shape.
#[test]
fn circumfix_output_action_predicate_confirm_only_for_structural_case() {
    let g = load(CIRCUMFIX_STRUCTURAL_XML);
    let profile = characterize(&g);
    let predicate = CircumfixStructuralCompositePredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &mrule_leaf(MRuleId(0))),
        PredicateVerdict::ConfirmOnly,
        "an in-scope structural circumfix/null-role drop must be ConfirmOnly, never Refuse \
         or Admit"
    );
}

/// `Role::Process` reaches `build_structural_composites` unconditionally, so its drop is `ConfirmOnly`, NOT the refusal branch.
#[test]
fn circumfix_output_action_predicate_confirm_only_for_process_role_drop() {
    let g = load(CIRCUMFIX_PROCESS_XML);
    let profile = characterize(&g);
    let predicate = CircumfixStructuralCompositePredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &mrule_leaf(MRuleId(0))),
        PredicateVerdict::ConfirmOnly
    );
}

/// Census C4's positive witness: an `Role::Infix` allomorph that drops LHS material now reaches `build_structural_composites`, so `evaluate` returns `ConfirmOnly`, not `Refuse` -- this test used to pin the opposite verdict; see `docs/research/circumfix-composite-precedence-census.md`, C4.
#[test]
fn circumfix_output_action_predicate_confirm_only_for_infix_role_drop() {
    let g = load(CIRCUMFIX_INFIX_DROP_XML);
    assert_eq!(
        crate::emit::rule_role(&g, MRuleId(0)),
        crate::emit::Role::Infix
    );
    assert!(
        crate::emit::is_structural_rule(&g, MRuleId(0)),
        "an Infix rule that drops LHS material must reach build_structural_composites since \
         census C4"
    );

    let profile = characterize(&g);
    let detail = profile
        .circumfix_output_action_details()
        .find(|d| d.rule == MRuleId(0) && d.allomorph_index == 0)
        .expect("an Infix allomorph dropping qC still observes CircumfixOutputAction");
    assert!(detail.structural_composite_attempted);

    let predicate = CircumfixStructuralCompositePredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &mrule_leaf(MRuleId(0))),
        PredicateVerdict::ConfirmOnly,
        "an Infix-with-drop allomorph must be ConfirmOnly, never Refuse, now that \
         is_structural_rule admits it"
    );
}

/// An unpeelable dropping reduplication is covered by structural synthesis.
#[test]
fn circumfix_output_action_predicate_accepts_structural_reduplication_role_drop() {
    let g = load(CIRCUMFIX_REDUPLICATION_DROP_XML);
    assert_eq!(
        crate::emit::rule_role(&g, MRuleId(0)),
        crate::emit::Role::Reduplication
    );
    assert!(
        crate::emit::is_structural_rule(&g, MRuleId(0)),
        "an unpeelable Reduplication rule must reach build_structural_composites"
    );

    let profile = characterize(&g);
    let detail = profile
        .circumfix_output_action_details()
        .find(|d| d.rule == MRuleId(0) && d.allomorph_index == 0)
        .expect("a Reduplication allomorph dropping qB still observes CircumfixOutputAction");
    assert!(detail.structural_composite_attempted);

    let predicate = CircumfixStructuralCompositePredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &mrule_leaf(MRuleId(0))),
        PredicateVerdict::ConfirmOnly
    );
}

/// Allomorph 1 must be admitted on its own dropping `Role::Prefix` shape regardless of allomorph 0's `Role::Reduplication`.
/// See `docs/research/circumfix-composite-precedence-census.md`, C5.
#[test]
fn circumfix_output_action_predicate_confirm_only_for_redup_first_then_prefix_drop_later() {
    let g = load(CIRCUMFIX_REDUP_FIRST_PREFIX_DROP_LATER_XML);
    assert_eq!(
        crate::emit::rule_role(&g, MRuleId(0)),
        crate::emit::Role::Reduplication,
        "allomorph 0 must be the Reduplication-shaped one -- the exact shape rule_role's \
         allomorph-0-only view would hide allomorph 1's drop behind"
    );
    assert!(
        crate::emit::is_structural_rule(&g, MRuleId(0)),
        "a later Prefix-shaped, LHS-material-dropping allomorph must reach \
         build_structural_composites even though allomorph 0 classifies Reduplication"
    );

    let profile = characterize(&g);
    let redup_detail = profile
        .circumfix_output_action_details()
        .find(|d| d.rule == MRuleId(0) && d.allomorph_index == 0)
        .expect("a Reduplication allomorph dropping qB still observes CircumfixOutputAction");
    assert!(
        redup_detail.structural_composite_attempted,
        "structural_composite_attempted is rule-wide, not per-allomorph, so allomorph 0's own \
         detail must also read true once the rule is admitted"
    );
    let prefix_detail = profile
        .circumfix_output_action_details()
        .find(|d| d.rule == MRuleId(0) && d.allomorph_index == 1)
        .expect("a Prefix allomorph dropping rB still observes CircumfixOutputAction");
    assert!(prefix_detail.structural_composite_attempted);

    let predicate = CircumfixStructuralCompositePredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &mrule_leaf(MRuleId(0))),
        PredicateVerdict::ConfirmOnly,
        "the later dropping Prefix-shaped allomorph must be ConfirmOnly, never Refuse, now \
         that is_structural_rule checks every allomorph rather than only the first"
    );
}

/// A grammar with no LHS-material-dropping allomorph never observes `CircumfixOutputAction`, and the predicate vacuously `Admit`s.
#[test]
fn circumfix_output_action_predicate_admits_vacuously_without_a_drop() {
    let g = load(RTL_PLAIN_XML);
    let profile = characterize(&g);
    assert!(
        !profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::CircumfixOutputAction),
        "a grammar with no LHS-material-dropping allomorph must never observe \
         CircumfixOutputAction at all"
    );
    let predicate = CircumfixStructuralCompositePredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &mrule_leaf(MRuleId(0))),
        PredicateVerdict::Admit
    );
}

// ---- Reduplication ----

/// An `AffixProcessRule` allomorph `CopyFromInput`s the same part twice, the in-scope peel-eligible case.
const REDUP_AFFIX_PROCESS_XML: &str = r#"<HermitCrabInput><Language><Name>RedupAffixProcess</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRules="mrRedupOk">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <MorphologicalRule id="mrRedupOk">
              <Name>redupOk</Name>
              <MorphologicalSubrules>
                <MorphologicalSubrule id="subRedupOk">
                  <MorphologicalInput>
                    <PhoneticSequence id="qA"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                  </MorphologicalInput>
                  <MorphologicalOutput redupMorphType="suffix">
                    <CopyFromInput index="qA" />
                    <CopyFromInput index="qA" />
                  </MorphologicalOutput>
                </MorphologicalSubrule>
              </MorphologicalSubrules>
              <MorphemeId>RED</MorphemeId>
            </MorphologicalRule>
          </MorphologicalRuleDefinitions>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

/// Same shape, but owned by a `RealizationalRule`: the out-of-scope case, a real C# quirk.
const REDUP_REALIZATIONAL_XML: &str = r#"<HermitCrabInput><Language><Name>RedupRealizational</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRules="rrRedupBad">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <RealizationalRule id="rrRedupBad">
              <Name>redupBad</Name>
              <MorphologicalSubrules>
                <MorphologicalSubrule id="subRedupBad">
                  <MorphologicalInput>
                    <PhoneticSequence id="qA"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                  </MorphologicalInput>
                  <MorphologicalOutput redupMorphType="suffix">
                    <CopyFromInput index="qA" />
                    <CopyFromInput index="qA" />
                  </MorphologicalOutput>
                </MorphologicalSubrule>
              </MorphologicalSubrules>
              <MorphemeId>REDBAD</MorphemeId>
            </RealizationalRule>
          </MorphologicalRuleDefinitions>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

/// The in-scope `AffixProcessRule`-owned shape characterizes `ConfigPredicate` with `peel_eligible_rule_kind == true`.
#[test]
fn characterize_marks_reduplication_config_predicate_for_affix_process_rule() {
    let g = load(REDUP_AFFIX_PROCESS_XML);
    assert!(matches!(g.mrules[0], MorphRuleDef::AffixProcess(_)));

    let profile = characterize(&g);
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::Reduplication
                && o.disposition == Disposition::ConfigPredicate),
        "a true-reduplicating AffixProcessRule allomorph must characterize \
         Reduplication/ConfigPredicate: {:?}",
        profile.observations()
    );
    let detail = profile
        .reduplication_details()
        .find(|d| d.rule == MRuleId(0) && d.allomorph_index == 0)
        .expect("must carry a ReduplicationDetail for mrule 0 allomorph 0");
    assert!(
        detail.peel_eligible_rule_kind,
        "an AffixProcessRule owner must be peel-eligible"
    );
    assert!(detail.peel_attempted);
    assert!(!detail.structural_composite_attempted);
}

/// The out-of-scope `RealizationalRule` shape still observes `Reduplication`, but reports `peel_eligible_rule_kind == false`.
#[test]
fn characterize_marks_reduplication_not_peel_eligible_for_realizational_rule() {
    let g = load(REDUP_REALIZATIONAL_XML);
    assert!(matches!(g.mrules[0], MorphRuleDef::Realizational(_)));

    let profile = characterize(&g);
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::Reduplication),
        "a true-reduplicating RealizationalRule allomorph must still observe Reduplication: {:?}",
        profile.observations()
    );
    let detail = profile
        .reduplication_details()
        .find(|d| d.rule == MRuleId(0) && d.allomorph_index == 0)
        .expect("must carry a ReduplicationDetail for mrule 0 allomorph 0");
    assert!(
        !detail.peel_eligible_rule_kind,
        "a RealizationalRule owner must never be reported peel-eligible"
    );
    assert!(!detail.peel_attempted);
    assert!(!detail.structural_composite_attempted);
}

#[test]
fn reduplication_predicate_accepts_structurally_owned_edge_insertion() {
    let xml = REDUP_AFFIX_PROCESS_XML.replacen(
        "<CopyFromInput index=\"qA\" />",
        "<InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>\n                     <CopyFromInput index=\"qA\" />",
        1,
    );
    let g = load(&xml);
    let profile = characterize(&g);
    let detail = profile
        .reduplication_details()
        .find(|d| d.rule == MRuleId(0) && d.allomorph_index == 0)
        .expect("edge-inserted reduplication must be characterized");
    assert!(detail.peel_eligible_rule_kind);
    assert!(!detail.peel_attempted);
    assert!(detail.structural_composite_attempted);
    assert_eq!(
        ReduplicationPeelSupportedPredicate.evaluate(&g, &profile, &mrule_leaf(MRuleId(0))),
        PredicateVerdict::ConfirmOnly
    );
}

/// Positive witness: the predicate returns `ConfirmOnly`, never `Admit`, for the in-scope shape.
#[test]
fn reduplication_predicate_confirm_only_for_affix_process_rule() {
    let g = load(REDUP_AFFIX_PROCESS_XML);
    let profile = characterize(&g);
    let predicate = ReduplicationPeelSupportedPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &mrule_leaf(MRuleId(0))),
        PredicateVerdict::ConfirmOnly,
        "an in-scope, peel-eligible true reduplication must be ConfirmOnly, never Refuse or \
         Admit"
    );
}

/// Negative witness: the predicate `Refuse`s the out-of-scope `RealizationalRule`-owned shape.
#[test]
fn reduplication_predicate_refuses_realizational_rule() {
    let g = load(REDUP_REALIZATIONAL_XML);
    let profile = characterize(&g);
    let predicate = ReduplicationPeelSupportedPredicate;
    match predicate.evaluate(&g, &profile, &mrule_leaf(MRuleId(0))) {
        PredicateVerdict::Refuse(diag) => {
            assert_eq!(diag.predicate, "reduplication.peel-eligible-rule-kind");
        }
        other => panic!(
            "expected Refuse for the RealizationalRule-owned out-of-scope shape, got {other:?}"
        ),
    }
}

/// A grammar with no true reduplication never observes it, and the predicate vacuously `Admit`s.
#[test]
fn reduplication_predicate_admits_vacuously_without_true_reduplication() {
    let g = load(RTL_PLAIN_XML);
    let profile = characterize(&g);
    assert!(
        !profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::Reduplication),
        "a grammar with no true-reduplicating allomorph must never observe Reduplication at \
         all"
    );
    let predicate = ReduplicationPeelSupportedPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &mrule_leaf(MRuleId(0))),
        PredicateVerdict::Admit
    );
}

// ---- QuantifierPattern ----

/// An ordinary fixed-segment rewrite gated by a bounded (`min="1" max="2"`) quantifier in its right environment.
const QUANT_BOUNDED_ENV_XML: &str = r#"<HermitCrabInput><Language><Name>QuantBoundedEnv</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncZ"><Name>Z</Name><Segment segment="cz" /></SegmentNaturalClass></NaturalClasses>
      <PhonologicalRuleDefinitions>
        <PhonologicalRule id="prQuantBounded">
          <Name>quantBoundedDemo</Name>
          <PhoneticInput><PhoneticSequence><Segment segment="ca" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><Segment segment="cb" /></PhoneticSequence></PhoneticOutput>
              <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence>
                <OptionalSegmentSequence min="1" max="2"><SimpleContext naturalClass="ncZ" /></OptionalSegmentSequence>
              </PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
      </PhonologicalRuleDefinitions>
      <Strata><Stratum characterDefinitionTable="t1" phonologicalRules="prQuantBounded"><Name>S</Name></Stratum></Strata>
    </Language></HermitCrabInput>"#;

/// Same shape, but the right-environment quantifier is genuinely unbounded (`max="-1"`).
const QUANT_UNBOUNDED_ENV_XML: &str = r#"<HermitCrabInput><Language><Name>QuantUnboundedEnv</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncZ"><Name>Z</Name><Segment segment="cz" /></SegmentNaturalClass></NaturalClasses>
      <PhonologicalRuleDefinitions>
        <PhonologicalRule id="prQuantUnbounded">
          <Name>quantUnboundedDemo</Name>
          <PhoneticInput><PhoneticSequence><Segment segment="ca" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><Segment segment="cb" /></PhoneticSequence></PhoneticOutput>
              <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence>
                <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncZ" /></OptionalSegmentSequence>
              </PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
      </PhonologicalRuleDefinitions>
      <Strata><Stratum characterDefinitionTable="t1" phonologicalRules="prQuantUnbounded"><Name>S</Name></Stratum></Strata>
    </Language></HermitCrabInput>"#;

/// A bounded environment quantifier characterizes `ConfigPredicate` with `all_bounded == true` and `compile_attempted == true`.
#[test]
fn characterize_marks_quantifier_pattern_config_predicate_when_bounded() {
    let g = load(QUANT_BOUNDED_ENV_XML);
    assert!(rule_has_quantifier(match &g.prules[0] {
        PhonRuleDef::Rewrite(r) => r,
        _ => panic!("expected a Rewrite-kind rule"),
    }));

    let profile = characterize(&g);
    assert!(
        profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::QuantifierPattern
                && o.disposition == Disposition::ConfigPredicate),
        "a bounded environment quantifier must characterize ConfigPredicate: {:?}",
        profile.observations()
    );
    let detail = profile
        .quantifier_detail(PRuleId(0))
        .expect("QuantifierPattern must carry a QuantifierPatternDetail");
    assert!(detail.all_bounded, "min=1/max=2 is finitely bounded");
    assert!(
        detail.compile_attempted,
        "a bounded environment quantifier alongside an ordinary fixed-segment LHS/RHS is \
         exactly the shape crate::replace::pattern_slots accepts"
    );
}

/// Positive witness: the predicate returns `ConfirmOnly`, never `Admit`/`Refuse`, for a bounded rule.
#[test]
fn quantifier_predicate_confirm_only_for_bounded_shape() {
    let g = load(QUANT_BOUNDED_ENV_XML);
    let profile = characterize(&g);
    let predicate = QuantifierBoundedExpansionPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::ConfirmOnly,
        "a bounded, compile-attempted quantifier rule must be ConfirmOnly, never Admit or Refuse"
    );
}

/// An unbounded environment quantifier characterizes `all_bounded == false`, still accurate but no longer disposition-driving.
#[test]
fn characterize_marks_quantifier_unbounded_as_not_all_bounded() {
    let g = load(QUANT_UNBOUNDED_ENV_XML);
    let profile = characterize(&g);
    let detail = profile
        .quantifier_detail(PRuleId(0))
        .expect("QuantifierPattern must carry a QuantifierPatternDetail");
    assert!(
        !detail.all_bounded,
        "max=-1 is the DTD's own unbounded Kleene sentinel"
    );
}

/// A positive `ConfirmOnly` witness: the predicate does not `Refuse` merely for an unbounded quantifier, once `pattern_slots` accepts the whole pattern shape.
#[test]
fn quantifier_predicate_confirm_only_for_unbounded_shape() {
    let g = load(QUANT_UNBOUNDED_ENV_XML);
    let profile = characterize(&g);
    let detail = profile
        .quantifier_detail(PRuleId(0))
        .expect("QuantifierPattern must carry a QuantifierPatternDetail");
    assert!(
        detail.compile_attempted,
        "an unbounded quantifier used in a well-formed right-environment is exactly the shape \
         crate::replace::pattern_slots now accepts"
    );
    let predicate = QuantifierBoundedExpansionPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::ConfirmOnly,
        "an unbounded, compile-attempted quantifier rule must be ConfirmOnly, never Admit or \
         Refuse"
    );
}

/// A rule that never uses `Quantifier` never observes `QuantifierPattern`, and the predicate vacuously `Admit`s.
#[test]
fn quantifier_predicate_admits_vacuously_for_rule_without_quantifier() {
    let g = load(RTL_PLAIN_XML);
    let profile = characterize(&g);
    assert!(
        !profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::QuantifierPattern),
        "a quantifier-free rule must never observe QuantifierPattern at all"
    );
    let predicate = QuantifierBoundedExpansionPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::Admit
    );
}

const TWO_TABLE_DISJOINT_XML: &str = r#"<HermitCrabInput><Language><Name>TwoTableDisjoint</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t0"><Name>T0</Name>
        <SegmentDefinitions><SegmentDefinition id="c0a"><Representations><Representation>p</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <CharacterDefinitionTable id="t1"><Name>T1</Name>
        <SegmentDefinitions><SegmentDefinition id="c1a"><Representations><Representation>k</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <Strata>
        <Stratum characterDefinitionTable="t0"><Name>S0</Name></Stratum>
        <Stratum characterDefinitionTable="t1"><Name>S1</Name></Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

const TWO_TABLE_OVERLAPPING_XML: &str = r#"<HermitCrabInput><Language><Name>TwoTableOverlap</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t0"><Name>T0</Name>
        <SegmentDefinitions><SegmentDefinition id="c0a"><Representations><Representation>p</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <CharacterDefinitionTable id="t1"><Name>T1</Name>
        <SegmentDefinitions><SegmentDefinition id="c1a"><Representations><Representation>p</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <Strata>
        <Stratum characterDefinitionTable="t0"><Name>S0</Name></Stratum>
        <Stratum characterDefinitionTable="t1"><Name>S1</Name></Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

/// An ordinary affix + iterative-rewrite grammar must characterize with no `ConfigPredicate` observations.
#[test]
fn ordinary_affix_and_iterative_rewrite_grammar_characterizes_proven() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>Ordinary</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cb" /></SegmentNaturalClass></NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata>
            <Stratum characterDefinitionTable="t1" phonologicalRules="pr1" morphologicalRules="mr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <MorphologicalRule id="mr1">
                  <Name>-a</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="sub1">
                      <MorphologicalInput>
                        <PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence>
                      </MorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="stem" />
                        <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                      </MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </MorphologicalRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);

    let profile = characterize(&g);
    assert!(
        !profile.has_disposition(Disposition::ConfigPredicate),
        "ordinary grammar must have NO ConfigPredicate observations: {:?}",
        profile.observations()
    );
    // Sanity: it DOES characterize the expected Proven/ConfirmOnly-free constructs.
    assert!(profile
        .observations()
        .iter()
        .any(|o| o.kind == CharacteristicKind::Affixation));
    assert!(profile
        .observations()
        .iter()
        .any(|o| o.kind == CharacteristicKind::IterativeRewrite));
    assert!(profile
        .observations()
        .iter()
        .any(|o| o.kind == CharacteristicKind::LeftToRightRewrite));
    assert!(profile
        .observations()
        .iter()
        .any(|o| o.kind == CharacteristicKind::OrderedMorphRuleApplication));
}

// ---- simultaneous.subrule-overlap ----

const SIMULTANEOUS_PROBE_XML: &str = r#"<HermitCrabInput><Language><Name>SimultaneousOverlapProbe</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <MorphologicalPhonologicalRuleFeatures>
        <MorphologicalPhonologicalRuleFeature id="mprA">Alpha</MorphologicalPhonologicalRuleFeature>
      </MorphologicalPhonologicalRuleFeatures>
      <PhonologicalFeatureSystem>
        <SymbolicFeature id="featCons"><Name>cons</Name>
          <Symbols><Symbol id="symConsP">+</Symbol><Symbol id="symConsM">-</Symbol></Symbols>
        </SymbolicFeature>
        <SymbolicFeature id="featVoi"><Name>voi</Name>
          <Symbols><Symbol id="symVoiP">+</Symbol><Symbol id="symVoiM">-</Symbol></Symbols>
        </SymbolicFeature>
      </PhonologicalFeatureSystem>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations>
            <FeatureValue feature="featCons" symbolValues="symConsP" />
            <FeatureValue feature="featVoi" symbolValues="symVoiM" />
          </SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses>
        <FeatureNaturalClass id="ncStop"><Name>Stop</Name>
          <FeatureValue feature="featCons" symbolValues="symConsP" />
        </FeatureNaturalClass>
        <FeatureNaturalClass id="ncVoiced"><Name>Voiced</Name>
          <FeatureValue feature="featVoi" symbolValues="symVoiP" />
        </FeatureNaturalClass>
        <FeatureNaturalClass id="ncVoiceless"><Name>Voiceless</Name>
          <FeatureValue feature="featVoi" symbolValues="symVoiM" />
        </FeatureNaturalClass>
      </NaturalClasses>
      <PhonologicalRuleDefinitions>
        <PhonologicalRule id="prAdmit" multipleApplicationOrder="simultaneous"><Name>admit</Name>
          <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule requiredMPRFeatures="mprA">
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
            <PhonologicalSubrule excludedMPRFeatures="mprA">
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
        <PhonologicalRule id="prRefuseOverlap" multipleApplicationOrder="simultaneous"><Name>refuseOverlap</Name>
          <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
        <PhonologicalRule id="prRefuseSelfOpaquing" multipleApplicationOrder="simultaneous"><Name>refuseSelfOpaquing</Name>
          <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
              <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncVoiceless" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
            </PhonologicalSubrule>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
      </PhonologicalRuleDefinitions>
    </Language></HermitCrabInput>"#;

fn leaf_for(rule: PRuleId) -> PlanNodeKind {
    PlanNodeKind::Leaf {
        fragment: FragmentSpec::RewriteRule { rule },
        provenance: Provenance::RewriteRule(rule),
    }
}

/// Admit when the two subrules' MPR gates are provably disjoint and neither is self_opaquing.
#[test]
fn simultaneous_predicate_admits_mpr_disjoint_subrules() {
    let g = load(SIMULTANEOUS_PROBE_XML);
    let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
        panic!("expected rewrite rule at 0 (prAdmit)")
    };
    assert!(!r.subrules[0].self_opaquing && !r.subrules[1].self_opaquing);

    let profile = characterize(&g);
    let predicate = SimultaneousSubruleOverlapPredicate;
    let verdict = predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0)));
    assert_eq!(
        verdict,
        PredicateVerdict::Admit,
        "mpr-disjoint, non-self-opaquing subrules must Admit"
    );
}

/// Refuse when neither subrule declares an MPR gate, so overlap cannot be ruled out.
#[test]
fn simultaneous_predicate_refuses_when_overlap_cannot_be_ruled_out() {
    let g = load(SIMULTANEOUS_PROBE_XML);
    let PhonRuleDef::Rewrite(r) = &g.prules[1] else {
        panic!("expected rewrite rule at 1 (prRefuseOverlap)")
    };
    assert!(!r.subrules[0].self_opaquing && !r.subrules[1].self_opaquing);
    assert!(r.subrules[0].required_mpr.is_empty() && r.subrules[0].excluded_mpr.is_empty());

    let profile = characterize(&g);
    let predicate = SimultaneousSubruleOverlapPredicate;
    let verdict = predicate.evaluate(&g, &profile, &leaf_for(PRuleId(1)));
    match verdict {
        PredicateVerdict::Refuse(diag) => {
            assert_eq!(diag.predicate, "simultaneous.subrule-overlap");
        }
        other => panic!("expected Refuse, got {other:?}"),
    }
}

/// Refuse when a subrule is self_opaquing; do not attempt Admit regardless of mpr gating.
#[test]
fn simultaneous_predicate_refuses_self_opaquing_subrule() {
    let g = load(SIMULTANEOUS_PROBE_XML);
    let PhonRuleDef::Rewrite(r) = &g.prules[2] else {
        panic!("expected rewrite rule at 2 (prRefuseSelfOpaquing)")
    };
    assert!(
        r.subrules[0].self_opaquing,
        "prRefuseSelfOpaquing's first subrule must be self_opaquing (RHS Voiced vs \
         RightEnvironment Voiceless, disjoint voi bits)"
    );

    let profile = characterize(&g);
    let predicate = SimultaneousSubruleOverlapPredicate;
    let verdict = predicate.evaluate(&g, &profile, &leaf_for(PRuleId(2)));
    match verdict {
        PredicateVerdict::Refuse(diag) => {
            assert!(diag.witness.contains("self_opaquing"));
        }
        other => panic!("expected Refuse, got {other:?}"),
    }
}

/// A non-Simultaneous (Iterative) rule is always Admit.
#[test]
fn simultaneous_predicate_admits_iterative_rule_unconditionally() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let profile = characterize(&g);
    let predicate = SimultaneousSubruleOverlapPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::Admit
    );
}

// ---- The real automaton intersection, isolated from the self-opaquing/mpr-gate early-outs ----
// See docs/research/pg-foma-capability-design-notes.md.

/// Two subrules whose right environments are mutually exclusive `SegmentNaturalClass`es genuinely cannot overlap; the real automaton intersection proves their spans disjoint and `Admit`s.
#[test]
fn simultaneous_predicate_admits_genuinely_non_overlapping_subrules_via_lowered_span() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>SimLowerAdmit</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cStop"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cFront"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cBack"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <SegmentNaturalClass id="ncStop"><Name>Stop</Name><Segment segment="cStop" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncFront"><Name>Front</Name><Segment segment="cFront" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncBack"><Name>Back</Name><Segment segment="cBack" /></SegmentNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1" multipleApplicationOrder="simultaneous"><Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
                  <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncFront" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
                </PhonologicalSubrule>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
                  <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBack" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
        panic!("expected rewrite rule at 0")
    };
    assert_eq!(r.mode, RewriteMode::Simultaneous);
    assert!(!r.subrules[0].self_opaquing && !r.subrules[1].self_opaquing);
    // No MPR features declared, so `mpr_gates_disjoint` cannot short-circuit; decided purely by the lowered-span intersection.
    assert!(r.subrules[0].required_mpr.is_empty() && r.subrules[0].excluded_mpr.is_empty());
    assert!(r.subrules[1].required_mpr.is_empty() && r.subrules[1].excluded_mpr.is_empty());

    let profile = characterize(&g);
    let predicate = SimultaneousSubruleOverlapPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::Admit,
        "Front/Back-flanked, non-mpr-disjoint, non-self-opaquing subrules must now Admit \
         via the real lowered-span intersection (previously Refuse under the conservative \
         fallback)"
    );
}

/// Two subrules whose right environments genuinely overlap (a shared member, not identical automata) must still `Refuse`, with a witness naming the real intersection.
#[test]
fn simultaneous_predicate_refuses_genuinely_overlapping_subrules_via_lowered_span() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>SimLowerRefuse</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cStop"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cFront"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cBack"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <SegmentNaturalClass id="ncStop"><Name>Stop</Name><Segment segment="cStop" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncFrontOrBack"><Name>FrontOrBack</Name><Segment segment="cFront" /><Segment segment="cBack" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncBack"><Name>Back</Name><Segment segment="cBack" /></SegmentNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1" multipleApplicationOrder="simultaneous"><Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
                  <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncFrontOrBack" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
                </PhonologicalSubrule>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
                  <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBack" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
        panic!("expected rewrite rule at 0")
    };
    assert!(!r.subrules[0].self_opaquing && !r.subrules[1].self_opaquing);

    let profile = characterize(&g);
    let predicate = SimultaneousSubruleOverlapPredicate;
    match predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))) {
        PredicateVerdict::Refuse(diag) => {
            assert_eq!(diag.predicate, "simultaneous.subrule-overlap");
            assert!(
                diag.witness.contains("genuinely intersect"),
                "witness should name the real intersection, not the old conservative \
                 wording: {diag:?}"
            );
        }
        other => {
            panic!("expected Refuse (genuine overlap via shared Back member), got {other:?}")
        }
    }
}

/// A right environment using an `Anchor` node, which `lower_span` does not represent, must conservatively `Refuse`, naming the unhandled kind.
#[test]
fn simultaneous_predicate_refuses_unsupported_pattern_node_conservatively() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>SimLowerUnsupported</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cStop"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cFront"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cBack"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <SegmentNaturalClass id="ncStop"><Name>Stop</Name><Segment segment="cStop" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncFront"><Name>Front</Name><Segment segment="cFront" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncBack"><Name>Back</Name><Segment segment="cBack" /></SegmentNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1" multipleApplicationOrder="simultaneous"><Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
                  <Environment><RightEnvironment><PhoneticTemplate finalBoundaryCondition="true"><PhoneticSequence><SimpleContext naturalClass="ncFront" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
                </PhonologicalSubrule>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
                  <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBack" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
        panic!("expected rewrite rule at 0")
    };
    assert!(!r.subrules[0].self_opaquing && !r.subrules[1].self_opaquing);
    assert!(
        matches!(
            r.subrules[0].right_env.as_ref().unwrap().nodes.last(),
            Some(pg_grammar::model::PatternNode::Anchor(
                pg_grammar::model::AnchorSide::Right
            ))
        ),
        "fixture must actually carry an Anchor node in subrule 0's right_env: {:?}",
        r.subrules[0].right_env
    );

    let profile = characterize(&g);
    let predicate = SimultaneousSubruleOverlapPredicate;
    match predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))) {
        PredicateVerdict::Refuse(diag) => {
            assert_eq!(diag.predicate, "simultaneous.subrule-overlap");
            assert!(
                diag.witness.contains("Anchor"),
                "witness must name the unhandled Anchor node kind: {diag:?}"
            );
        }
        other => panic!("expected conservative Refuse naming Anchor, got {other:?}"),
    }
}

// ---- The `owning_table` fix to `lower_subrule_span`, and its compile-facing consumer chain ----
// See docs/research/pg-foma-capability-design-notes.md.

/// Two tables; the Simultaneous rule is wired into the second stratum's table, `t1`. Table `t0` is deliberately tiny and unrelated, so a `g.char_tables.first()` default would fail this rule's span lowering.
const TWO_TABLE_SIMULTANEOUS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>TwoTableSimultaneous</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featVoice"><Name>voice</Name><Symbols>
        <Symbol id="symVless">vless</Symbol><Symbol id="symVd1">vd1</Symbol><Symbol id="symVd2">vd2</Symbol>
      </Symbols></SymbolicFeature>
      <SymbolicFeature id="featPlace"><Name>place</Name><Symbols>
        <Symbol id="symFront">front</Symbol><Symbol id="symBack">back</Symbol><Symbol id="symNeutral">neutral</Symbol>
      </Symbols></SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t0"><Name>T0</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c0z"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <CharacterDefinitionTable id="t1"><Name>T1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVless" /><FeatureValue feature="featPlace" symbolValues="symNeutral" /></SegmentDefinition>
        <SegmentDefinition id="ci"><Representations><Representation>i</Representation></Representations><FeatureValue feature="featPlace" symbolValues="symFront" /><FeatureValue feature="featVoice" symbolValues="symVless" /></SegmentDefinition>
        <SegmentDefinition id="cu"><Representations><Representation>u</Representation></Representations><FeatureValue feature="featPlace" symbolValues="symBack" /><FeatureValue feature="featVoice" symbolValues="symVless" /></SegmentDefinition>
        <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVd1" /><FeatureValue feature="featPlace" symbolValues="symNeutral" /></SegmentDefinition>
        <SegmentDefinition id="cd"><Representations><Representation>d</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVd2" /><FeatureValue feature="featPlace" symbolValues="symNeutral" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncStop"><Name>Stop</Name><FeatureValue feature="featVoice" symbolValues="symVless" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncFront"><Name>Front</Name><FeatureValue feature="featPlace" symbolValues="symFront" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncBack"><Name>Back</Name><FeatureValue feature="featPlace" symbolValues="symBack" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncB"><Name>B</Name><FeatureValue feature="featVoice" symbolValues="symVd1" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncD"><Name>D</Name><FeatureValue feature="featVoice" symbolValues="symVd2" /></FeatureNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prSimT1" multipleApplicationOrder="simultaneous">
        <Name>simT1Demo</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncB" /></PhoneticSequence></PhoneticOutput>
            <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncFront" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
          </PhonologicalSubrule>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncD" /></PhoneticSequence></PhoneticOutput>
            <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBack" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t0"><Name>S0</Name></Stratum>
      <Stratum characterDefinitionTable="t1" phonologicalRules="prSimT1"><Name>S1</Name></Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// Positive witness: `lower_subrule_span` must resolve this rule's span against its own owning table, never table 0.
#[test]
fn lower_subrule_span_uses_the_rules_owning_table_not_table_zero() {
    let g = load(TWO_TABLE_SIMULTANEOUS_XML);
    assert_eq!(
        g.char_tables.len(),
        2,
        "fixture must declare exactly 2 tables"
    );
    assert_eq!(
        g.char_tables[0].len(),
        1,
        "table 0 must be the tiny, unrelated 1-segment table"
    );
    assert_eq!(
        g.char_tables[1].len(),
        5,
        "table 1 must be the rule's own 5-segment inventory"
    );

    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule at prules[0]");
    };
    assert_eq!(rule.mode, RewriteMode::Simultaneous);
    assert!(!rule.subrules[0].self_opaquing && !rule.subrules[1].self_opaquing);

    let table = crate::replace::owning_table(&g, rule)
        .expect("prSimT1 is wired into stratum S1's own phonologicalRules cascade");
    assert_eq!(
        table.len(),
        5,
        "owning_table must resolve to table 1 (5 segments) -- NOT table 0's 1-segment table"
    );

    // The real end-to-end proof: this can only succeed if owning-table threading is actually wired in.
    assert_eq!(
        simultaneous_rule_admitted_for_compile(&g, rule),
        Ok(()),
        "the real per-owning-table lowering must Admit this genuinely non-overlapping rule"
    );

    // Cross-check: the gate and the compiler must agree, proving they share one proof, never two.
    let profile = characterize(&g);
    let predicate = SimultaneousSubruleOverlapPredicate;
    assert_eq!(
        predicate.evaluate(&g, &profile, &leaf_for(PRuleId(0))),
        PredicateVerdict::Admit,
        "the registered predicate must also Admit, using the SAME owning-table-lowered spans \
         `characterize` computed"
    );
}

/// Negative witness: a rule with no owning stratum in a 2+-table grammar is genuinely ambiguous, so `lower_subrule_span` must return `Unsupported` rather than guess table 0.
#[test]
fn lower_subrule_span_refuses_conservatively_when_owning_table_is_ambiguous() {
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>TwoTableUnwiredSimultaneous</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t0"><Name>T0</Name>
      <SegmentDefinitions><SegmentDefinition id="c0z"><Representations><Representation>z</Representation></Representations></SegmentDefinition></SegmentDefinitions>
    </CharacterDefinitionTable>
    <CharacterDefinitionTable id="t1"><Name>T1</Name>
      <SegmentDefinitions><SegmentDefinition id="c1p"><Representations><Representation>p</Representation></Representations></SegmentDefinition></SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncStop"><Name>Stop</Name><Segment segment="c1p" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prSimUnwired" multipleApplicationOrder="simultaneous">
        <Name>simUnwiredDemo</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t0"><Name>S0</Name></Stratum>
      <Stratum characterDefinitionTable="t1"><Name>S1</Name></Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    let g = load(XML);
    assert_eq!(
        g.char_tables.len(),
        2,
        "fixture must declare exactly 2 tables"
    );
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule at prules[0]");
    };
    assert!(
        crate::replace::owning_table(&g, rule).is_none(),
        "prSimUnwired must NOT be wired into any stratum's phonologicalRules cascade"
    );

    match simultaneous_rule_admitted_for_compile(&g, rule) {
        Err(reason) => assert!(
            reason.contains("no owning stratum") || reason.contains("CharacterDefinitionTable"),
            "witness should name the ambiguous-table-selection cause: {reason}"
        ),
        Ok(()) => panic!(
            "must NOT wrongly Admit an unresolvable-table rule in a genuinely multi-table \
             grammar -- that would be exactly the silent wrong-Admit this fix exists to \
             prevent"
        ),
    }
}

// ---- Registry coverage ----

/// Every `ConfigPredicate` characteristic must be discharged by at least one registered predicate.
#[test]
fn default_registry_discharges_every_config_predicate_kind() {
    let registry = default_registry();
    let missing = undischarged_kinds(&registry);
    assert!(
        missing.is_empty(),
        "undischarged ConfigPredicate characteristics: {missing:?}"
    );
}

/// No registered predicate may discharge only `Proven` kinds, which `compose_envelope` never consults.
#[test]
fn no_registered_predicate_is_inert() {
    let inert = inert_predicates(&default_registry());
    assert!(
        inert.is_empty(),
        "these predicates discharge only Disposition::Proven kinds, so compose_envelope never \
         evaluates them -- they compile, run, change no test and no behaviour, and simply do \
         nothing: {inert:?}"
    );
}

/// The guard must be able to see its own target, or it passes for every registry and gates nothing.
#[test]
fn the_inert_guard_detects_an_inert_registration() {
    struct ProvenOnlyPredicate;
    impl CapabilityPredicate for ProvenOnlyPredicate {
        fn id(&self) -> PredicateId {
            "test.proven-only"
        }
        fn shape_key(&self) -> &'static str {
            "nonregular-process-morphology"
        }
        fn discharges(&self) -> &[CharacteristicKind] {
            &[CharacteristicKind::Affixation]
        }
        fn provenance(&self) -> EvidenceProvenance {
            EvidenceProvenance::Structural
        }
        fn evaluate(
            &self,
            _grammar: &Grammar,
            _profile: &CharacteristicsProfile,
            _plan_node: &PlanNodeKind,
        ) -> PredicateVerdict {
            PredicateVerdict::Admit
        }
    }
    assert_eq!(
        CharacteristicKind::Affixation.default_disposition(),
        Disposition::Proven,
        "this guard's own fixture assumes Affixation is Proven"
    );
    let mut registry = default_registry();
    registry.register(Box::new(ProvenOnlyPredicate));
    assert!(inert_predicates(&registry).contains(&"test.proven-only"));
}

/// Every `CharacteristicKind` variant has an explicit default disposition, and doubles as a canary that `ALL` hasn't drifted out of sync with the enum.
#[test]
fn all_kinds_have_a_default_disposition() {
    for kind in CharacteristicKind::ALL {
        let _ = kind.default_disposition();
    }
    assert_eq!(
        CharacteristicKind::ALL.len(),
        24,
        "20 -> 22 by research report 13 (StemName/FreeFluctuation); 22 -> 23 by \
         ProcessMorphology, which NOTHING observed -- a pure-ablaut grammar reported \
         Affixation/Ordered/NaturalClass all Proven and nothing about the mutation; 23 -> 24 \
         by CrossTableRespelling, which two fixtures exhibited for months under MultiTable's \
         inherited coverage while every backend missed the respelled analysis"
    );
}

// ---- compose_envelope: meet lattice unit checks ----

fn diag(predicate: PredicateId, construct: &str) -> CapabilityDiagnostic {
    CapabilityDiagnostic {
        predicate,
        construct: construct.to_string(),
        witness: "unit-test witness".to_string(),
    }
}

/// The lattice spelled out directly on `meet`: `Refuse` dominates `ConfirmOnly` dominates `Admit`.
#[test]
fn meet_lattice_lines_up_with_d4() {
    assert_eq!(
        meet(CompileDecision::Admit, CompileDecision::Admit),
        CompileDecision::Admit
    );
    assert_eq!(
        meet(CompileDecision::Admit, CompileDecision::ConfirmOnly),
        CompileDecision::ConfirmOnly
    );
    assert_eq!(
        meet(CompileDecision::ConfirmOnly, CompileDecision::Admit),
        CompileDecision::ConfirmOnly
    );
    assert_eq!(
        meet(CompileDecision::ConfirmOnly, CompileDecision::ConfirmOnly),
        CompileDecision::ConfirmOnly
    );
    let d1 = diag("p1", "c1");
    assert_eq!(
        meet(
            CompileDecision::Admit,
            CompileDecision::Refuse(vec![d1.clone()])
        ),
        CompileDecision::Refuse(vec![d1.clone()])
    );
    assert_eq!(
        meet(
            CompileDecision::Refuse(vec![d1.clone()]),
            CompileDecision::ConfirmOnly
        ),
        CompileDecision::Refuse(vec![d1.clone()])
    );
}

/// Two `Refuse`s meet to carry both diagnostics, not just one side's.
#[test]
fn meet_of_two_refuses_unions_diagnostics() {
    let d1 = diag("p1", "c1");
    let d2 = diag("p2", "c2");
    let merged = meet(
        CompileDecision::Refuse(vec![d1.clone()]),
        CompileDecision::Refuse(vec![d2.clone()]),
    );
    assert_eq!(merged, CompileDecision::Refuse(vec![d1, d2]));
}

/// Meeting a `Refuse` with an equal diagnostic, from two DAG paths to a shared node, does not duplicate it.
#[test]
fn meet_of_two_refuses_deduplicates_identical_diagnostics() {
    let d1 = diag("p1", "c1");
    let merged = meet(
        CompileDecision::Refuse(vec![d1.clone()]),
        CompileDecision::Refuse(vec![d1.clone()]),
    );
    assert_eq!(merged, CompileDecision::Refuse(vec![d1]));
}

// ---- compose_envelope: end-to-end over characterize() + enumerate_default() ----

/// An ordinary affix + iterative-rewrite grammar must compose to `Admit`.
#[test]
fn compose_envelope_admits_ordinary_affix_and_iterative_rewrite_grammar() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>Ordinary</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cb" /></SegmentNaturalClass></NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata>
            <Stratum characterDefinitionTable="t1" phonologicalRules="pr1" morphologicalRules="mr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <MorphologicalRule id="mr1">
                  <Name>-a</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="sub1">
                      <MorphologicalInput>
                        <PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence>
                      </MorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="stem" />
                        <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                      </MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </MorphologicalRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let plan = enumerated_plan(&g);
    let registry = default_registry();

    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::Admit
    );
}

/// A grammar with a single, non-recursive `Compounding` rule must compose to `ConfirmOnly`, not `Refuse`.
#[test]
fn compose_envelope_confirm_only_for_non_recursive_compounding_grammar() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1">
                  <Name>Compound</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let plan = enumerated_plan(&g);
    let registry = default_registry();

    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::ConfirmOnly,
        "a single non-recursive Compounding rule must compose to ConfirmOnly"
    );
}

/// A self-feeding `Compounding` rule composes to `ConfirmOnly`, same as the non-recursive case, since `build_compound_chain` unrolls enough levels to realize its computed `max_depth`.
#[test]
fn compose_envelope_confirm_only_for_recursive_compounding_grammar() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1" multipleApplication="2">
                  <Name>Compound</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let plan = enumerated_plan(&g);
    let registry = default_registry();

    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::ConfirmOnly,
        "a self-feeding (multipleApplication > 1) Compounding rule must now compose to \
         ConfirmOnly, same as the non-recursive case -- task 4.1 closed the construction gap"
    );
}

/// A chain-depth-bounded, zero-rule `Unordered` grammar must compose to `ConfirmOnly`, never `Refuse`.
#[test]
fn compose_envelope_confirm_only_for_unordered_stratum() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
              <Name>S</Name>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let plan = enumerated_plan(&g);
    let registry = default_registry();

    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::ConfirmOnly,
        "a chain-depth-bounded Unordered grammar must compose to ConfirmOnly, never Refuse"
    );
}

/// An `Unordered` stratum with `rule_count` trivial suffix rules, mirroring `crate::unordered`'s own test-only helper (duplicated, not shared, across the module boundary).
fn unordered_stratum_xml(rule_count: u32) -> String {
    let mut rules = String::new();
    let mut segs = String::new();
    for i in 0..rule_count {
        segs.push_str(&format!(
            r#"<SegmentDefinition id="cx{i}"><Representations><Representation>x{i}</Representation></Representations></SegmentDefinition>"#
        ));
        rules.push_str(&format!(
            r#"<MorphologicalRule id="mr{i}" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
                     <Name>r{i}</Name>
                     <MorphologicalSubrules>
                       <MorphologicalSubrule id="sub{i}">
                         <MorphologicalInput><PhoneticSequence id="stem{i}"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                         <MorphologicalOutput><InsertSegments><PhoneticShape>x{i}</PhoneticShape></InsertSegments><CopyFromInput index="stem{i}" /></MorphologicalOutput>
                       </MorphologicalSubrule>
                     </MorphologicalSubrules>
                     <MorphemeId>R{i}</MorphemeId>
                   </MorphologicalRule>"#
        ));
    }
    let rule_ids: Vec<String> = (0..rule_count).map(|i| format!("mr{i}")).collect();
    format!(
        r#"<HermitCrabInput><Language><Name>UnorderedUnbounded</Name>
              <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
              <CharacterDefinitionTable id="t1"><Name>Main</Name>
                <SegmentDefinitions>
                  <SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
                  {segs}
                </SegmentDefinitions>
              </CharacterDefinitionTable>
              <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
              <Strata>
                <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="{rule_ids}">
                  <Name>S</Name>
                  <MorphologicalRuleDefinitions>{rules}</MorphologicalRuleDefinitions>
                  <LexicalEntries>
                    <LexicalEntry id="eK" partOfSpeech="posV">
                      <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
                      <MorphemeId>K</MorphemeId>
                    </LexicalEntry>
                  </LexicalEntries>
                </Stratum>
              </Strata>
            </Language></HermitCrabInput>"#,
        rule_ids = rule_ids.join(" "),
    )
}

/// A grammar with an `MprGroupOutput::Append` group and nothing worse must compose to `ConfirmOnly`, not `Admit` or `Refuse`.
#[test]
fn compose_envelope_confirm_only_for_append_group_alone() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>AppendOnly</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <MorphologicalPhonologicalRuleFeatures>
            <MorphologicalPhonologicalRuleFeature id="mprA">A</MorphologicalPhonologicalRuleFeature>
            <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="append" features="mprA"><Name>GAppend</Name></MorphologicalPhonologicalRuleFeatureGroup>
          </MorphologicalPhonologicalRuleFeatures>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <LexicalEntries>
                <LexicalEntry id="e1" partOfSpeech="posV">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
                  <Gloss>e1</Gloss>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    assert!(
        !g.mpr_groups.is_empty(),
        "fixture must declare an MprGroup at all"
    );
    let plan = enumerated_plan(&g);
    let registry = default_registry();

    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::ConfirmOnly
    );
}

/// The mirror image: an `MprGroupOutput::Overwrite` group alone also composes to `ConfirmOnly`, never `Admit`.
#[test]
fn compose_envelope_confirms_overwrite_group_alone() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>OverwriteOnly</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <MorphologicalPhonologicalRuleFeatures>
            <MorphologicalPhonologicalRuleFeature id="mprA">A</MorphologicalPhonologicalRuleFeature>
            <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="overwrite" features="mprA"><Name>GOverwrite</Name></MorphologicalPhonologicalRuleFeatureGroup>
          </MorphologicalPhonologicalRuleFeatures>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <LexicalEntries>
                <LexicalEntry id="e1" partOfSpeech="posV">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
                  <Gloss>e1</Gloss>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    assert!(
        !g.mpr_groups.is_empty(),
        "fixture must declare an MprGroup at all"
    );
    assert_eq!(g.mpr_groups[0].output, MprGroupOutput::Overwrite);
    let plan = enumerated_plan(&g);
    let registry = default_registry();

    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::ConfirmOnly
    );
}

/// A grammar with an `Epenthesis` occurrence (empty-LHS rule) and nothing worse must compose to `ConfirmOnly`, not `Admit` or `Refuse`.
#[test]
fn compose_envelope_confirm_only_for_epenthesis_alone() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>EpenthesisAlone</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cy"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <SegmentNaturalClass id="ncE"><Name>Epenthetic</Name><Segment segment="ce" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncX"><Name>X</Name><Segment segment="cx" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncY"><Name>Y</Name><Segment segment="cy" /></SegmentNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="prEpenthesis">
              <Name>epenthesisAlone</Name>
              <PhoneticInput><PhoneticSequence /></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncE" /></PhoneticSequence></PhoneticOutput>
                  <Environment>
                    <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncX" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
                    <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncY" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
                  </Environment>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata>
            <Stratum characterDefinitionTable="t1" phonologicalRules="prEpenthesis">
              <Name>S</Name>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>xy</PhoneticShape></Allomorph></Allomorphs>
                  <Gloss>e1</Gloss>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    assert!(
        g.prules.iter().any(|pr| matches!(pr,
            PhonRuleDef::Rewrite(r) if r.lhs.nodes.is_empty())),
        "fixture must declare an empty-LHS (epenthesis) rewrite rule"
    );
    let plan = enumerated_plan(&g);
    let registry = default_registry();

    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::ConfirmOnly,
        "an epenthesis-only fixture must compose to ConfirmOnly, never Refuse"
    );
}

/// A grammar with a `RealizationalRule` and nothing worse must compose to `ConfirmOnly`, not `Admit` or `Refuse`.
#[test]
fn compose_envelope_confirm_only_for_realizational_rule_alone() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>RealizAlone</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <RealizationalRule id="rr1">
                  <Name>Realiz</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="sub1">
                      <MorphologicalInput><PhoneticSequence id="s0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
                      <MorphologicalOutput><CopyFromInput index="s0" /></MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </RealizationalRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    assert!(matches!(g.mrules[0], MorphRuleDef::Realizational(_)));
    let plan = enumerated_plan(&g);
    let registry = default_registry();

    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::ConfirmOnly
    );
}

/// A grammar with a `MorphemeCoOccurrenceRule` and nothing worse must compose to `ConfirmOnly` for the same reason.
#[test]
fn compose_envelope_confirm_only_for_co_occurrence_rule_alone() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>CoOccurAlone</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <MorphologicalRule id="mrA">
                  <Name>A</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="subA">
                      <MorphologicalInput><PhoneticSequence id="s0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
                      <MorphologicalOutput><CopyFromInput index="s0" /></MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </MorphologicalRule>
                <MorphologicalRule id="mrB">
                  <Name>B</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="subB">
                      <MorphologicalInput><PhoneticSequence id="s1"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
                      <MorphologicalOutput><CopyFromInput index="s1" /></MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </MorphologicalRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
          <MorphemeCoOccurrenceRules>
            <MorphemeCoOccurrenceRule type="exclude" primaryMorpheme="mrA" otherMorphemes="mrB" adjacency="anywhere" />
          </MorphemeCoOccurrenceRules>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    assert!(g.morphemes.iter().any(|m| !m.co_occurrence.is_empty()));
    let plan = enumerated_plan(&g);
    let registry = default_registry();

    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::ConfirmOnly
    );
}

/// A `Simultaneous` rule whose subrules are provably mpr-disjoint and not self-opaquing must compose to `Admit`.
#[test]
fn compose_envelope_admits_simultaneous_rule_with_mpr_disjoint_subrules() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>SimAdmit</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <MorphologicalPhonologicalRuleFeatures>
            <MorphologicalPhonologicalRuleFeature id="mprA">Alpha</MorphologicalPhonologicalRuleFeature>
          </MorphologicalPhonologicalRuleFeatures>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="c2"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="c1" /></SegmentNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1" multipleApplicationOrder="simultaneous">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule requiredMPRFeatures="mprA">
                  <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
                <PhonologicalSubrule excludedMPRFeatures="mprA">
                  <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata>
            <Stratum characterDefinitionTable="t1" phonologicalRules="pr1">
              <Name>S</Name>
              <LexicalEntries>
                <LexicalEntry id="e1" partOfSpeech="posV">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
                  <Gloss>e1</Gloss>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
        panic!("expected rewrite rule at 0")
    };
    assert_eq!(r.mode, RewriteMode::Simultaneous);
    assert!(!r.subrules[0].self_opaquing && !r.subrules[1].self_opaquing);

    let plan = enumerated_plan(&g);
    let registry = default_registry();

    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::Admit
    );
}

/// The same shape, but neither subrule declares an MPR gate, so overlap can't be ruled out and the cascade-composing compilers must `Refuse`.
#[test]
fn compose_envelope_refuses_simultaneous_rule_when_overlap_cannot_be_ruled_out() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>SimRefuse</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="c2"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="c1" /></SegmentNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1" multipleApplicationOrder="simultaneous">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata>
            <Stratum characterDefinitionTable="t1" phonologicalRules="pr1">
              <Name>S</Name>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
        panic!("expected rewrite rule at 0")
    };
    assert!(!r.subrules[0].self_opaquing && !r.subrules[1].self_opaquing);
    assert!(r.subrules[0].required_mpr.is_empty() && r.subrules[0].excluded_mpr.is_empty());

    let semantics = GrammarSemantics::derive(&g);
    let plan = enumerated_plan(&g);
    let registry = default_registry();

    for &strategy in CASCADE_COMPOSING_STRATEGIES {
        match compose_envelope_for_strategy(&semantics, &plan, strategy, &registry) {
            CompileDecision::Refuse(diags) => {
                assert!(
                    diags
                        .iter()
                        .any(|d| d.predicate == "simultaneous.subrule-overlap"),
                    "{strategy:?}: expected a simultaneous.subrule-overlap diagnostic: {diags:?}"
                );
            }
            other => {
                panic!("{strategy:?} composes replace's cascade, so it must Refuse; got {other:?}")
            }
        }
    }

    // The mainline emitter composes no cascade, so `replace`'s admission floor is not its limit.
    assert_eq!(
        compose_envelope_for_strategy(
            &semantics,
            &plan,
            EmissionStrategy::TunedSurfaceProbed,
            &registry
        ),
        CompileDecision::ConfirmOnly
    );
    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::ConfirmOnly
    );
}

/// Meet correctness: a grammar with both a self-feeding `Compounding` rule and an `Overwrite` `MprGroup` must compose deterministically over both constructs' verdicts, dropping neither.
#[test]
fn compose_envelope_meet_correctness_two_confirm_only_constructs() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>OverwritePlusCompound</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <MorphologicalPhonologicalRuleFeatures>
            <MorphologicalPhonologicalRuleFeature id="mprA">A</MorphologicalPhonologicalRuleFeature>
            <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="overwrite" features="mprA"><Name>GOverwrite</Name></MorphologicalPhonologicalRuleFeatureGroup>
          </MorphologicalPhonologicalRuleFeatures>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="c1" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1" multipleApplication="2">
                  <Name>Compound</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1" partOfSpeech="posV">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
                  <Gloss>e1</Gloss>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = load(XML);
    assert!(!g.mpr_groups.is_empty(), "fixture must declare an MprGroup");
    assert_eq!(
        g.mpr_groups[0].output,
        MprGroupOutput::Overwrite,
        "fixture must declare an Overwrite-output MprGroup (the Refuse-worthy half)"
    );
    assert!(
        g.mrules
            .iter()
            .any(|m| matches!(m, MorphRuleDef::Compounding(_))),
        "fixture must declare a Compounding rule (the ConfirmOnly-worthy half)"
    );

    let plan = enumerated_plan(&g);
    let registry = default_registry();

    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::ConfirmOnly
    );
}

// The pre-refactor whole-grammar composition verbatim: one walk, entire registry, no strategy.
fn compiler_blind_reference(
    semantics: &GrammarSemantics<'_>,
    plan: &Plan,
    registry: &PredicateRegistry,
) -> CompileDecision {
    let predicates: Vec<&dyn CapabilityPredicate> =
        registry.predicates().iter().map(|p| p.as_ref()).collect();
    compose_over_predicates(semantics, plan, &predicates)
}

// Narrowing moves a verdict toward ConfirmOnly from either end; it never crosses to the far side.
fn assert_narrowing_only_softens(
    label: &str,
    strategy: EmissionStrategy,
    blind_narrowed: &CompileDecision,
    composed: &CompileDecision,
) {
    let ok = match (blind_narrowed, composed) {
        (CompileDecision::ConfirmOnly, CompileDecision::ConfirmOnly) => true,
        (CompileDecision::ConfirmOnly, _) => false,
        (CompileDecision::Admit, CompileDecision::Admit | CompileDecision::ConfirmOnly) => true,
        (CompileDecision::Admit, _) => false,
        (CompileDecision::Refuse(_), CompileDecision::Refuse(_) | CompileDecision::ConfirmOnly) => {
            true
        }
        (CompileDecision::Refuse(_), _) => false,
    };
    assert!(
        ok,
        "{label}: narrowing moved {strategy:?} from {blind_narrowed:?} to {composed:?} -- a \
         dropped predicate lands its kind on disposition_floor (ConfirmOnly for every kind any \
         narrowed predicate discharges), so the only legal moves are Refuse->ConfirmOnly and \
         Admit->ConfirmOnly. Anything else means a narrowing manufactured a verdict."
    );
}

// Returns the whole-grammar verdict so a caller can tally which of the three the corpus reached.
fn assert_per_strategy_derivation_is_identical(label: &str, g: &Grammar) -> CompileDecision {
    let semantics = GrammarSemantics::derive(g);
    let phon = PhonologyProbe::new_with_semantics(&semantics);
    let plan = enumerate_default(g, semantics.prules_in_order(), phon.as_ref());
    let registry = default_registry();

    let blind = compiler_blind_reference(&semantics, &plan, &registry);
    let baseline = baseline_grammar_wide_checks();
    let envelope = compose_envelope_across_strategies(
        &semantics,
        &plan,
        &CapabilityContributions::new(&registry, &baseline),
    );

    assert_eq!(
        compose_envelope_with_semantics(&semantics, &plan, &registry),
        envelope.global(),
        "{label}: the public whole-grammar entry point is no longer StrategyEnvelope::global"
    );

    let whole_registry: Vec<usize> = (0..registry.predicates().len()).collect();
    for &strategy in ALL_STRATEGIES {
        let composed = compose_envelope_for_strategy(&semantics, &plan, strategy, &registry);
        assert_eq!(
            envelope.decision_for(strategy),
            Some(&composed),
            "{label}: {strategy:?}'s row in the envelope differs from asking for it directly"
        );

        // The OLD per-strategy form: the compiler-blind verdict, narrowed by the same coverage rows and baseline checks `compose_envelope_for_strategy` folds in.
        let blind_narrowed = fold_grammar_wide_checks(
            with_strategy_coverage(&semantics, strategy, blind.clone()),
            &semantics,
            &plan,
            strategy,
            &baseline,
        );
        if constraining_predicate_indices(&registry, strategy) == whole_registry {
            assert_eq!(
                composed, blind_narrowed,
                "{label}: {strategy:?} is still gated by every registered predicate, so its \
                 composed verdict must equal the narrowed-from-blind one"
            );
        } else {
            assert_narrowing_only_softens(label, strategy, &blind_narrowed, &composed);
        }
    }

    envelope.global()
}

fn decision_label(decision: &CompileDecision) -> &'static str {
    match decision {
        CompileDecision::Admit => "admit",
        CompileDecision::ConfirmOnly => "confirm-only",
        CompileDecision::Refuse(_) => "refuse",
    }
}

// Covers the derivation over both fixture roots; NOT the shared plan walk both sides call.
#[test]
fn per_strategy_derivation_is_identical_on_every_conformance_fixture() {
    let fixtures = pg_conformance_fixtures::discover();
    let machine = fixtures
        .iter()
        .filter(|f| f.root == pg_conformance_fixtures::Root::Machine)
        .count();
    let staging = fixtures
        .iter()
        .filter(|f| f.root == pg_conformance_fixtures::Root::Staging)
        .count();
    // Fail closed: an absent corpus must not read as a passing identity check.
    assert!(
        machine > 0 && staging > 0,
        "no conformance corpus to check against (machine={machine} staging={staging}) -- \
         `rust/tools/conformance.ps1` initializes the machine/conformance submodule"
    );

    let mut tally: HashMap<&'static str, usize> = HashMap::new();
    let mut load_failed = 0usize;
    for f in &fixtures {
        let Ok(g) = pg_grammar::load(&f.load_grammar_xml()) else {
            load_failed += 1;
            continue;
        };
        let decision = assert_per_strategy_derivation_is_identical(&f.label(), &g);
        *tally.entry(decision_label(&decision)).or_default() += 1;
    }

    let checked: usize = tally.values().sum();
    println!(
        "per_strategy_derivation_is_identical: checked={checked} load_failed={load_failed} \
         verdicts={tally:?}"
    );
    assert_eq!(
        checked,
        machine + staging - load_failed,
        "every loadable fixture must have been checked"
    );
    // A real floor, not `> 0`: a passing run must assert the corpus size it actually saw.
    assert!(
        checked >= 35,
        "only {checked} of {} fixtures were checked (machine={machine} staging={staging} \
         load_failed={load_failed}) -- too few for this to be an exhaustive claim",
        machine + staging
    );
}

// The corpus need not reach a `Refuse` or a compiler disagreement; these fixtures reach both.
#[test]
fn per_strategy_derivation_is_identical_across_all_three_verdicts() {
    const ORDINARY_XML: &str = r#"<HermitCrabInput><Language><Name>Ordinary</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;

    // Where the compilers disagree: PlanComposed emits no lexc line for a RealizationalRule.
    const REALIZATIONAL_XML: &str = r#"<HermitCrabInput><Language><Name>RealizAlone</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <RealizationalRule id="rr1">
                  <Name>Realiz</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="sub1">
                      <MorphologicalInput><PhoneticSequence id="s0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
                      <MorphologicalOutput><CopyFromInput index="s0" /></MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </RealizationalRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;

    // Refuses through a predicate no narrowing touches, so every compiler declines it alike.
    let refusing_xml = REDUP_REALIZATIONAL_XML.to_string();

    let mut seen: Vec<&'static str> = Vec::new();
    for (label, xml) in [
        ("ordinary", ORDINARY_XML.to_string()),
        ("realizational", REALIZATIONAL_XML.to_string()),
        ("unordered", unordered_stratum_xml(3)),
        ("reduplication-on-realizational-rule", refusing_xml),
    ] {
        let g = load(&xml);
        let decision = assert_per_strategy_derivation_is_identical(label, &g);
        let seen_label = decision_label(&decision);
        if !seen.contains(&seen_label) {
            seen.push(seen_label);
        }
    }

    seen.sort_unstable();
    assert_eq!(
        seen,
        vec!["admit", "confirm-only", "refuse"],
        "the synthetic set must exercise all three verdicts, or the identity claim is only \
         proven for whichever one it happened to hit"
    );
}

// The tripwire this replaced forbade narrowing outright; this one gates its QUALITY instead.
#[test]
fn every_narrowing_excuses_only_a_compiler_that_can_represent_the_construct() {
    let registry = default_registry();
    for predicate in registry.predicates() {
        let constrained = predicate.constrains_strategies();
        assert!(
            !constrained.is_empty(),
            "predicate {} constrains no compiler at all -- a predicate nothing is gated by is \
             dead weight, not a narrowing",
            predicate.id()
        );
        for &strategy in ALL_STRATEGIES {
            if constrained.contains(&strategy) {
                continue;
            }
            // Excusing a compiler that cannot even PROPOSE the construct is the inheritance trap run backwards.
            for &kind in predicate.discharges() {
                assert_ne!(
                    crate::strategy_coverage::representation_of(strategy, kind).representation,
                    crate::strategy_coverage::StrategyRepresentation::CannotRepresent,
                    "predicate {} does not constrain {strategy:?}, but strategy_coverage says \
                     that compiler emits NOTHING for {kind:?} -- letting it off this predicate \
                     hands it an admission it has not earned",
                    predicate.id()
                );
            }
        }
    }
}

// The property making every narrowing safe: the floor a dropped predicate lands on is ConfirmOnly.
#[test]
fn a_narrowed_predicate_can_never_land_its_kind_on_an_admit_floor() {
    let registry = default_registry();
    let mut narrowed_kinds = 0usize;
    for predicate in registry.predicates() {
        if predicate.constrains_strategies().len() == ALL_STRATEGIES.len() {
            continue;
        }
        for &kind in predicate.discharges() {
            narrowed_kinds += 1;
            assert_eq!(
                disposition_floor(kind.default_disposition()),
                CompileDecision::ConfirmOnly,
                "predicate {} is narrowed, so a compiler it no longer constrains falls back to \
                 disposition_floor for {kind:?} -- that floor must be ConfirmOnly, or narrowing \
                 could manufacture an Admit and license an admission filter nothing proved",
                predicate.id()
            );
        }
    }
    assert!(
        narrowed_kinds > 0,
        "no predicate is narrowed, so this property is vacuous -- it must be re-pointed at \
         whatever replaced the narrowing rather than left passing on an empty set"
    );
}

// Hand-built verdicts, so the join claim does not wait on finding a grammar of each shape.
#[test]
fn global_refuses_only_when_every_strategy_refuses() {
    fn diagnostic(id: PredicateId) -> CapabilityDiagnostic {
        CapabilityDiagnostic {
            predicate: id,
            construct: "X".to_string(),
            witness: "w".to_string(),
        }
    }
    fn envelope(decisions: [CompileDecision; 3]) -> StrategyEnvelope {
        StrategyEnvelope {
            verdicts: ALL_STRATEGIES
                .iter()
                .copied()
                .zip(decisions)
                .map(|(strategy, decision)| StrategyVerdict { strategy, decision })
                .collect(),
        }
    }
    let shared = diagnostic("shared");
    let only_a = diagnostic("only-a");
    let only_b = diagnostic("only-b");

    assert_eq!(
        envelope([
            CompileDecision::Refuse(vec![shared.clone()]),
            CompileDecision::ConfirmOnly,
            CompileDecision::Refuse(vec![shared.clone()]),
        ])
        .global(),
        CompileDecision::ConfirmOnly,
        "one non-refusing compiler is enough"
    );
    assert_eq!(
        envelope([
            CompileDecision::Refuse(vec![shared.clone()]),
            CompileDecision::Admit,
            CompileDecision::Refuse(vec![shared.clone()]),
        ])
        .global(),
        CompileDecision::Admit
    );
    // Unanimous for a shared reason: that reason alone, without the compiler-specific extras.
    assert_eq!(
        envelope([
            CompileDecision::Refuse(vec![shared.clone(), only_a.clone()]),
            CompileDecision::Refuse(vec![shared.clone()]),
            CompileDecision::Refuse(vec![shared.clone(), only_b.clone()]),
        ])
        .global(),
        CompileDecision::Refuse(vec![shared.clone()])
    );
    // Unanimous for disjoint reasons: the union, since an empty-diagnostic refusal is unactionable.
    assert_eq!(
        envelope([
            CompileDecision::Refuse(vec![only_a.clone()]),
            CompileDecision::Refuse(vec![only_b.clone()]),
            CompileDecision::Refuse(vec![only_a.clone()]),
        ])
        .global(),
        CompileDecision::Refuse(vec![only_a.clone(), only_b.clone()])
    );

    let declining = envelope([
        CompileDecision::Refuse(vec![only_a.clone()]),
        CompileDecision::ConfirmOnly,
        CompileDecision::Refuse(vec![only_b.clone()]),
    ]);
    let reported: Vec<EmissionStrategy> = declining.declining().iter().map(|(s, _)| *s).collect();
    assert_eq!(
        reported,
        vec![
            EmissionStrategy::PlanComposed,
            EmissionStrategy::TemplatedUnderlyingTokens
        ],
        "the envelope must name which compilers declined, which a scalar decision cannot"
    );
}
