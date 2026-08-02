//! `crate::uflexc`'s bounded compound loop (`uflexc.rs`'s own "Bounded compound loop" section),
//! and specifically the constraint that silently breaks it: **the non-head root set must be
//! GRAMMAR-WIDE, not `allowed_entries`.**
//!
//! `uflexc::emit_underlying_filtered_with_budget` is called ONCE PER GATE-PARTITION GROUP
//! (`crate::gate`'s static-partition design; `crate::build::build_controllable`) and the per-group
//! networks are then `fsm_union`ed. A compound loop restricted to the calling group's own
//! `allowed_entries` therefore still cannot propose a compound whose head and non-head fall in
//! DIFFERENT partition groups: neither group's network contains such a path, so neither does the
//! union. A fix that passes a single-group fixture (the `compounding-non-recursive` fixture RED-1
//! uses declares no phonological rules at all, so it partitions into exactly one group) and fails
//! on a partitioned grammar is the likely failure mode, which is what
//! [`compound_levels_are_grammar_wide_under_a_real_gate_partition`] exists to catch.
//!
//! The other half of the same constraint is that the BARE-ROOT lexicon must stay partitioned --
//! `crate::gate`'s "groups are lexically disjoint by construction" argument for why unioning the
//! per-group nets is safe rests on it -- so every assertion below checks both directions, never
//! just "the compound section got bigger".

use std::collections::HashSet;

use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;

use pg_foma::compose_budget::ComposeBudget;
use pg_foma::gate::{find_gated_subrules, partition_entries};
use pg_foma::replace::SegAlphabet;
use pg_foma::tags;
use pg_foma::uflexc::emit_underlying_filtered_with_budget;
use pg_grammar::model::{Grammar, LexEntryId, PhonRuleDef};

/// Head root `fasu` is `posH`; non-head root `bel` is `posN`. `prule1`'s only subrule is gated
/// `requiredPartsOfSpeech="posH"`, so `gate::entry_gate_key` gives the two entries DIFFERENT keys
/// and `gate::partition_entries` puts them in different groups -- exactly the shape the module doc
/// above describes. The rewrite itself (`x -> y`) is deliberately inert on both roots' spellings:
/// the partition is decided statically from each entry's own POS, never from whether the rule would
/// actually fire, so an inert rule keeps the test about the PARTITION and nothing else.
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

/// The same grammar with the `CompoundingRule` (and therefore `morphologicalRules="cr1"`) removed:
/// the no-compounding control, which must emit byte-for-byte what this module emitted before the
/// compound loop existed -- no `AfterRoot`, no `UCmp*`, every root continuing to `SuffixOrEnd`.
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

fn unbounded_budget() -> ComposeBudget {
    ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    )
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

/// THE constraint (module doc): with a genuinely gated grammar whose head and non-head land in
/// DIFFERENT partition groups, every group's emitted lexc must still offer the grammar-wide
/// non-head root in its compound level, while its BARE-ROOT lexicon stays restricted to that
/// group's own entries.
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
    let budget = unbounded_budget();
    let head_tag = root_tag(&g, "eHead");
    let non_head_tag = root_tag(&g, "eNon");

    for (gi, group) in groups.iter().enumerate() {
        let allowed: HashSet<LexEntryId> = group.entries.clone();
        let report = emit_underlying_filtered_with_budget(&g, &alphabet, Some(&allowed), &budget)
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

        // A head-eligible root reaches the compound loop through `AfterRoot`, never straight to
        // `SuffixOrEnd` (the head/no-compound split `crate::emit`'s TLPost/TLPostNoCmp makes).
        // Both entries are head-eligible here (`cr1` declares no head restriction at all).
        for line in &bare {
            assert!(
                line.ends_with("AfterRoot ;"),
                "group {gi}'s head-eligible root line must continue to AfterRoot: {line:?}"
            );
        }

        // Structural validity: an undefined continuation lexicon is a lexc compile failure, so this
        // catches a compound section that references a lexicon it never wrote.
        let net = fsm_lexc_parse_string(&FomaOptions::default(), None, lexc)
            .unwrap_or_else(|| panic!("group {gi}'s emitted lexc must compile:\n{lexc}"));
        assert!(net.statecount > 0);
    }
}

/// The no-compounding control: a grammar with no `CompoundingRuleDef` must emit no compound
/// machinery at all, with every root continuing straight to `SuffixOrEnd` -- the pre-compound-loop
/// emission, unchanged.
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
    let report = emit_underlying_filtered_with_budget(&g, &alphabet, None, &unbounded_budget())
        .expect("control fixture must emit");
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

/// Acceptance item 2: the bounded unroll must stay BOUNDED. Emits the same staged
/// `compounding-non-recursive` fixture RED-1 uses, compiles it, and pins the compiled network's
/// size. MEASURED on this fixture (5 entries, 1 non-recursive `CompoundingRuleDef`, so exactly one
/// extra non-head level), by running this test with the compound loop force-disabled and then
/// enabled:
///
/// | | states | arcs | lexc lines |
/// |---|---|---|---|
/// | before (no compound loop) | 14 | 17 | 28 |
/// | after (bounded compound loop) | 18 | 25 | 45 |
///
/// An additive +4 states / +8 arcs for one extra non-head level over 5 licensed root allomorphs --
/// bounded, not multiplicative. The ceilings below are generous relative to that (a self-feeding
/// compound loop, or a compound level accidentally re-emitting a whole partitioned lexicon per
/// group, would blow straight through them) while leaving room for incidental emission changes.
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
    let report = emit_underlying_filtered_with_budget(&g, &alphabet, None, &unbounded_budget())
        .expect("fixture must emit");

    let net = fsm_lexc_parse_string(&FomaOptions::default(), None, &report.lexc_source)
        .unwrap_or_else(|| panic!("emitted lexc must compile:\n{}", report.lexc_source));
    // Printed, not only asserted: the before/after figures in this test's own doc were read off
    // this line (run with `--no-capture`), and re-reading them after a change is how the "stays
    // bounded" claim stays honest rather than becoming a number nobody can reproduce.
    eprintln!(
        "[compound-unroll] compounding-non-recursive: {} states / {} arcs, {} lexc lines",
        net.statecount,
        net.arccount,
        report.lexc_source.lines().count()
    );

    // Exactly one extra non-head level for a non-recursive rule: `UCmp2` would mean the depth bound
    // `crate::emit::compound_extra_levels_checked` computed was wrong for this grammar.
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
