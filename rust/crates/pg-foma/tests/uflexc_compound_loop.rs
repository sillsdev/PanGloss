//! `crate::uflexc`'s bounded compound loop: the non-head root set must be grammar-wide, not `allowed_entries`, since emission runs once per gate-partition group and unions the results afterward.

use std::collections::HashSet;

use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;

use pg_foma::gate::{find_gated_subrules, partition_entries};
use pg_foma::replace::SegAlphabet;
use pg_foma::tags;
use pg_foma::uflexc::emit_underlying_filtered;
use pg_grammar::model::{Grammar, LexEntryId, PhonRuleDef};

/// Head root `fasu` (posH) and non-head root `bel` (posN) fall in different gate-partition groups; the inert `x -> y` rewrite keeps the test about the partition alone, never about whether the rule fires.
const PARTITIONED_COMPOUNDING_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>UflexcPartitionedCompounding</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posH"><Name>head</Name></PartOfSpeech>
      <PartOfSpeech id="posN"><Name>nonhead</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cf"><Representations><Representation>f</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cl"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cs"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cu"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cy"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prule1">
        <Name>gated-inert</Name>
        <PhoneticInput>
          <PhoneticSequence><Segment segment="cx" /></PhoneticSequence>
        </PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule requiredPartsOfSpeech="posH">
            <PhoneticOutput>
              <PhoneticSequence><Segment segment="cy" /></PhoneticSequence>
            </PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="linear" morphologicalRules="cr1" phonologicalRules="prule1">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <CompoundingRule id="cr1" nonHeadPartsOfSpeech="posN">
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
        <LexicalEntries>
          <LexicalEntry id="eHead" partOfSpeech="posH">
            <Allomorphs><Allomorph id="aHead"><PhoneticShape>fasu</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>HEAD</MorphemeId>
            <Gloss>head</Gloss>
          </LexicalEntry>
          <LexicalEntry id="eNon" partOfSpeech="posN">
            <Allomorphs><Allomorph id="aNon"><PhoneticShape>bel</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>NON</MorphemeId>
            <Gloss>nonhead</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// The no-compounding control: same grammar with `CompoundingRule` removed, so no `AfterRoot`/`UCmp*` should be emitted at all.
fn no_compounding_xml() -> String {
    let start = PARTITIONED_COMPOUNDING_XML
        .find("        <MorphologicalRuleDefinitions>")
        .expect("fixture must contain the morphological-rule block");
    let end = PARTITIONED_COMPOUNDING_XML
        .find("        <LexicalEntries>")
        .expect("fixture must contain the lexical-entry block");
    let mut xml = PARTITIONED_COMPOUNDING_XML.to_string();
    xml.replace_range(start..end, "");
    xml.replace(" morphologicalRules=\"cr1\"", "")
}

fn rules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
    g.strata
        .iter()
        .flat_map(|st| st.prules.iter().map(|id| &g.prules[id.0 as usize]))
        .collect()
}

fn entry_id_of(g: &Grammar, xml_id: &str) -> LexEntryId {
    let idx = g
        .entries
        .iter()
        .position(|e| e.authored_id == xml_id)
        .unwrap_or_else(|| panic!("no lexical entry {xml_id:?} in the fixture"));
    LexEntryId(idx as u32)
}

fn root_tag(g: &Grammar, xml_id: &str) -> String {
    let width = tags::tag_width(g.morphemes.len());
    let entry = &g.entries[entry_id_of(g, xml_id).0 as usize];
    tags::root_tag_lexc(entry.morpheme, width)
}

/// The lines of `lexc` belonging to `LEXICON name`, up to the next `LEXICON` header.
fn lexicon_body<'a>(lexc: &'a str, name: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in lexc.lines() {
        if let Some(rest) = line.strip_prefix("LEXICON ") {
            inside = rest.trim() == name;
            continue;
        }
        if inside && !line.trim().is_empty() {
            out.push(line);
        }
    }
    out
}

/// With head/non-head in different partition groups, every group's lexc must still offer the grammar-wide non-head root in its compound level, while its bare-root lexicon stays restricted to its own entries.
#[test]
fn compound_levels_are_grammar_wide_under_a_real_gate_partition() {
    let g = pg_grammar::load(PARTITIONED_COMPOUNDING_XML).expect("fixture must load");
    let prules = rules_in_order(&g);
    let gated = find_gated_subrules(&g, &prules);
    assert_eq!(
        gated.len(),
        1,
        "the fixture must declare exactly one gated phonological subrule"
    );

    let head_id = entry_id_of(&g, "eHead");
    let non_head_id = entry_id_of(&g, "eNon");
    let groups = partition_entries(&g, &gated, &prules);
    assert_eq!(
        groups.len(),
        2,
        "the gated subrule must split this grammar into two partition groups, or this test is not \
         exercising the constraint it exists for"
    );
    let head_group = groups
        .iter()
        .position(|grp| grp.entries.contains(&head_id))
        .expect("some group must own the head entry");
    let non_head_group = groups
        .iter()
        .position(|grp| grp.entries.contains(&non_head_id))
        .expect("some group must own the non-head entry");
    assert_ne!(
        head_group, non_head_group,
        "head and non-head must land in DIFFERENT groups -- that is the whole point of this fixture"
    );

    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let head_tag = root_tag(&g, "eHead");
    let non_head_tag = root_tag(&g, "eNon");

    for (gi, group) in groups.iter().enumerate() {
        let allowed: HashSet<LexEntryId> = group.entries.clone();
        let report = emit_underlying_filtered(&g, &alphabet, Some(&allowed))
            .unwrap_or_else(|e| panic!("group {gi} must emit: {e}"));
        let lexc = &report.lexc_source;

        // The compound level is grammar-wide: the non-head root is offered in BOTH groups.
        let cmp_roots = lexicon_body(lexc, "UCmpRoots");
        assert!(
            cmp_roots.iter().any(|l| l.starts_with(&non_head_tag)),
            "group {gi}'s UCmpRoots must offer the grammar-wide licensed non-head root \
             {non_head_tag:?} even when that entry belongs to a DIFFERENT partition group; got \
             {cmp_roots:?}\n{lexc}"
        );
        // ...and its tag must be declared, or lexc reads the tag as individual characters.
        assert!(
            lexc.lines()
                .take_while(|l| !l.starts_with("LEXICON "))
                .any(|l| l.trim() == non_head_tag),
            "group {gi} must declare {non_head_tag:?} in Multichar_Symbols\n{lexc}"
        );

        // The bare-root lexicon stays PARTITIONED: only this group's own entries appear.
        let bare = lexicon_body(lexc, "RootBare");
        assert_eq!(
            bare.len(),
            group.entries.len(),
            "group {gi}'s RootBare must hold exactly its own entries' lines; got {bare:?}"
        );
        let expects_head = group.entries.contains(&head_id);
        assert_eq!(
            bare.iter().any(|l| l.starts_with(&head_tag)),
            expects_head,
            "group {gi}'s RootBare must contain the head root iff the group owns it: {bare:?}"
        );

        // A head-eligible root reaches the compound loop through `AfterRoot`, never straight to `SuffixOrEnd`; both entries are head-eligible here since `cr1` declares no head restriction.
        for line in &bare {
            assert!(
                line.ends_with("AfterRoot ;"),
                "group {gi}'s head-eligible root line must continue to AfterRoot: {line:?}"
            );
        }

        // Structural validity: an undefined continuation lexicon is a lexc compile failure, catching a compound section that references a lexicon it never wrote.
        let net = fsm_lexc_parse_string(&FomaOptions::default(), None, lexc)
            .unwrap_or_else(|| panic!("group {gi}'s emitted lexc must compile:\n{lexc}"));
        assert!(net.statecount > 0);
    }
}

/// A grammar with no `CompoundingRuleDef` must emit no compound machinery at all, every root continuing straight to `SuffixOrEnd`.
#[test]
fn a_grammar_without_compounding_emits_no_compound_machinery() {
    let xml = no_compounding_xml();
    let g =
        pg_grammar::load(&xml).unwrap_or_else(|e| panic!("control fixture must load: {e}\n{xml}"));
    assert!(
        !g.mrules
            .iter()
            .any(|r| matches!(r, pg_grammar::model::MorphRuleDef::Compounding(_))),
        "the control fixture must declare no compounding rule"
    );

    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let report = emit_underlying_filtered(&g, &alphabet, None).expect("control fixture must emit");
    let lexc = &report.lexc_source;
    assert!(
        !lexc.contains("AfterRoot") && !lexc.contains("UCmp"),
        "a no-compounding grammar must emit no compound lexicons at all:\n{lexc}"
    );
    for line in lexicon_body(lexc, "RootBare") {
        assert!(
            line.ends_with("SuffixOrEnd ;"),
            "every root must continue straight to SuffixOrEnd: {line:?}"
        );
    }
}

/// The bounded unroll must stay bounded: additive growth (one extra non-head level over the licensed root allomorphs), never multiplicative; ceilings below are generous but would catch a self-feeding loop.
#[test]
fn the_compound_unroll_stays_bounded_on_the_staged_fixture() {
    let fixture = pg_conformance_fixtures::discover()
        .into_iter()
        .find(|f| {
            f.root == pg_conformance_fixtures::Root::Staging
                && f.name == "compounding-non-recursive"
        })
        .expect("missing pinned synthetic fixture compounding-non-recursive");
    let g = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load");
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let report = emit_underlying_filtered(&g, &alphabet, None).expect("fixture must emit");

    let net = fsm_lexc_parse_string(&FomaOptions::default(), None, &report.lexc_source)
        .unwrap_or_else(|| panic!("emitted lexc must compile:\n{}", report.lexc_source));
    // Printed, not only asserted, so the "stays bounded" claim stays reproducible rather than becoming a number nobody can check.
    eprintln!(
        "[compound-unroll] compounding-non-recursive: {} states / {} arcs, {} lexc lines",
        net.statecount,
        net.arccount,
        report.lexc_source.lines().count()
    );

    // Exactly one extra non-head level for a non-recursive rule; `UCmp2` would mean the depth bound was computed wrong for this grammar.
    assert!(
        report.lexc_source.contains("LEXICON UCmpRoots"),
        "the compound loop must be emitted for this fixture:\n{}",
        report.lexc_source
    );
    assert!(
        !report.lexc_source.contains("LEXICON UCmp2"),
        "a NON-recursive compounding rule must unroll exactly one extra non-head level:\n{}",
        report.lexc_source
    );

    assert!(
        net.statecount <= 40 && net.arccount <= 120,
        "the bounded compound unroll must stay bounded: {} states / {} arcs",
        net.statecount,
        net.arccount
    );
}
