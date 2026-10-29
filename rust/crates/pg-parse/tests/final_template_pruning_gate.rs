//! Final-template analysis pruning on small grammars: it fires by default where the guard allows it, saves work, and changes no parse.

use pg_conformance_fixtures::require_fixture;
use pg_grammar::model::{Grammar, MorphRuleDef, PartialMorphemeReason, StratumId};
use pg_parse::{Morpher, ParseOptions};

const WORDS: [&str; 5] = ["m", "mt", "mo", "mto", "mot"];

fn rule(id: &str, tag: &str) -> String {
    format!(
        r#"<MorphologicalRule id="{id}" requiredPartsOfSpeech="p" outputPartOfSpeech="p"><Name>{id}</Name>
        <MorphologicalSubrules><MorphologicalSubrule><MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="nc" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>{tag}</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>"#
    )
}

/// One stratum: a final template over `mrT` (suffix t) and the ordinary rule `mrO` (suffix o); `overlap` also lists `mrT` on the stratum, so it plays both roles.
fn grammar(overlap: bool) -> Grammar {
    let listed = if overlap { "mrO mrT" } else { "mrO" };
    let xml = format!(
        r#"<HermitCrabInput><Language><Name>FinalTemplatePruning</Name>
      <PartsOfSpeech><PartOfSpeech id="p"><Name>n</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions>
        <SegmentDefinition id="cm"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ct"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="co"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions></CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="nc"><Name>Any</Name><Segment segment="cm" /><Segment segment="ct" /><Segment segment="co" /></SegmentNaturalClass></NaturalClasses>
      <Strata><Stratum characterDefinitionTable="t" morphologicalRuleOrder="unordered" morphologicalRules="{listed}">
        <Name>S</Name><MorphologicalRuleDefinitions>{}{}</MorphologicalRuleDefinitions>
        <AffixTemplates><AffixTemplate id="tpl" final="true"><Name>Tpl</Name><Slot morphologicalRules="mrT" /></AffixTemplate></AffixTemplates>
        <LexicalEntries><LexicalEntry id="e" partOfSpeech="p"><MorphemeId>E</MorphemeId><Allomorphs><Allomorph id="a"><PhoneticShape>m</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries>
      </Stratum></Strata></Language></HermitCrabInput>"#,
        rule("mrT", "t"),
        rule("mrO", "o"),
    );
    pg_grammar::load(&xml).expect("pruning grammar loads")
}

struct Run {
    analyses: Vec<Vec<(String, String)>>,
    work: usize,
    skipped: u64,
}

fn run(morpher: &Morpher) -> Run {
    let mut run = Run {
        analyses: Vec::new(),
        work: 0,
        skipped: 0,
    };
    for word in WORDS {
        let (outcome, _, prunes) =
            morpher.parse_word_with_stats_and_prunes(word, &ParseOptions::default());
        let mut analyses = outcome.analyses.clone();
        analyses.sort();
        run.analyses.push(analyses);
        run.work += outcome.work_steps;
        run.skipped += prunes
            .iter()
            .filter(|row| row.stratum == StratumId(0))
            .map(|row| {
                row.counters.final_templates_skipped + row.counters.template_batteries_skipped
            })
            .sum::<u64>();
    }
    run
}

#[test]
fn a_final_template_after_an_ordinary_rule_is_pruned_by_default_and_no_parse_changes() {
    let g = grammar(false);
    let on = run(&Morpher::new(&g, usize::MAX));
    let off = run(&Morpher::new(&g, usize::MAX).with_final_template_pruning(false));
    assert!(on.skipped > 0, "the default prunes the template after `o`");
    assert_eq!(off.skipped, 0);
    assert_eq!(on.analyses, off.analyses);
    assert!(
        !on.analyses[1].is_empty() && !on.analyses[4].is_empty(),
        "the template alone and the template after the ordinary rule still parse: {:?}",
        on.analyses
    );
    assert!(
        on.analyses[3].is_empty(),
        "mto has the template before the ordinary rule"
    );
    assert!(
        on.work < off.work,
        "pruning saves steps: on {}, off {}",
        on.work,
        off.work
    );
}

#[test]
fn a_partial_lexical_entry_alone_keeps_pruning_on() {
    let mut g = grammar(false);
    g.entries[0].partial_reason = Some(PartialMorphemeReason::StemWithoutCategory);
    let on = run(&Morpher::new(&g, usize::MAX));
    let off = run(&Morpher::new(&g, usize::MAX).with_final_template_pruning(false));
    assert!(
        on.skipped > 0,
        "only the entry is partial, so no rescue rule exists"
    );
    assert_eq!(on.analyses, off.analyses);
}

#[test]
fn a_rule_in_both_roles_turns_default_pruning_off_and_forcing_it_on_changes_no_parse() {
    let g = grammar(true);
    let default = run(&Morpher::new(&g, usize::MAX));
    let forced = run(&Morpher::new(&g, usize::MAX).with_always_enforce_final_templates(true));
    assert_eq!(
        default.skipped, 0,
        "a template rule that is also an ordinary rule is not pruned by default"
    );
    assert!(forced.skipped > 0);
    assert_eq!(default.analyses, forced.analyses);
}

#[test]
fn a_partial_affix_rule_turns_default_pruning_off_because_pruning_would_lose_a_parse() {
    let mut g = grammar(false);
    let MorphRuleDef::AffixProcess(def) = &mut g.mrules[1] else {
        panic!("mrO is an affix process rule");
    };
    def.partial_reason = Some(PartialMorphemeReason::InflectionalAffixWithoutTemplateSlot);
    let default = run(&Morpher::new(&g, usize::MAX));
    let forced = run(&Morpher::new(&g, usize::MAX).with_always_enforce_final_templates(true));
    assert_eq!(
        default.skipped, 0,
        "a partial rule may rescue what pruning would drop"
    );
    assert!(forced.skipped > 0);
    assert!(
        !default.analyses[3].is_empty(),
        "the partial rule rescues mto"
    );
    assert!(
        forced.analyses[3].is_empty(),
        "forcing pruning on loses it: the guard is load-bearing"
    );
}

/// The template state from `Clitics` crosses into `Main` (entry 102); pruning there removes no parse.
#[test]
fn a_lower_stratum_state_prunes_the_upper_strata_template_and_no_parse_changes() {
    let fixture = require_fixture("edge-cases", "clitic-rule-designation");
    let g = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture loads");
    let (mut on, mut off) = (0, 0);
    for word in ["sagdt", "sagds"] {
        let default = Morpher::new(&g, usize::MAX);
        let unpruned = Morpher::new(&g, usize::MAX).with_final_template_pruning(false);
        let (a, _, prunes) =
            default.parse_word_with_stats_and_prunes(word, &ParseOptions::default());
        let (b, _, _) = unpruned.parse_word_with_stats_and_prunes(word, &ParseOptions::default());
        assert_eq!(a.analyses, b.analyses, "{word}");
        on += a.work_steps;
        off += b.work_steps;
        assert!(
            word != "sagdt"
                || prunes
                    .iter()
                    .any(|row| row.counters.final_templates_skipped > 0)
        );
    }
    assert!(on < off, "pruning saves steps: on {on}, off {off}");
}
