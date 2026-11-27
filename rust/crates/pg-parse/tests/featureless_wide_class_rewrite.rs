//! A featureless rewrite rule whose right-hand segment class straddles the 63-segment identity chunk boundary. Analysis can pin the surface node by one chunk only, so a class spanning two leaves the FST unconstrained and a separate membership check refuses a surface letter outside the class. The C# oracle gives the same verdicts on this grammar. The table is invented: a-z, A-Z, 0-9 (ids 0-61), then ä (62, chunk 0) and ö (63, chunk 1).

use pg_grammar::load;
use pg_grammar_model::model::Grammar;
use pg_parse::Morpher;

/// `[s z] -> [<rhs>] / a _ a` over the 64-segment table, with the single root `asa`.
fn grammar(rhs: &[&str]) -> Grammar {
    let letters: Vec<String> = ('a'..='z')
        .chain('A'..='Z')
        .chain('0'..='9')
        .chain(['ä', 'ö'])
        .map(String::from)
        .collect();
    assert_eq!(letters.len(), 64);
    let id = |l: &str| format!("c{l}");
    let segments: String = letters
        .iter()
        .map(|l| {
            format!(
                r#"<SegmentDefinition id="{}"><Representations><Representation>{l}</Representation></Representations></SegmentDefinition>"#,
                id(l)
            )
        })
        .collect();
    let members = |ls: &[&str]| -> String {
        ls.iter()
            .map(|l| format!(r#"<Segment segment="{}"/>"#, id(l)))
            .collect()
    };
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>FeaturelessWideClass</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="t1"><Name>Main</Name><SegmentDefinitions>{segments}</SegmentDefinitions></CharacterDefinitionTable>
<NaturalClasses>
  <SegmentNaturalClass id="ncA"><Name>A</Name>{a}</SegmentNaturalClass>
  <SegmentNaturalClass id="ncIn"><Name>In</Name>{input}</SegmentNaturalClass>
  <SegmentNaturalClass id="ncOut"><Name>Out</Name>{output}</SegmentNaturalClass>
</NaturalClasses>
<PhonologicalRuleDefinitions><PhonologicalRule id="prR"><Name>r</Name>
  <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncIn"/></PhoneticSequence></PhoneticInput>
  <PhonologicalSubrules><PhonologicalSubrule>
    <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncOut"/></PhoneticSequence></PhoneticOutput>
    <Environment>
      <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncA"/></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
      <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncA"/></PhoneticSequence></PhoneticTemplate></RightEnvironment>
    </Environment>
  </PhonologicalSubrule></PhonologicalSubrules>
</PhonologicalRule></PhonologicalRuleDefinitions>
<Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prR"><Name>S</Name>
<LexicalEntries>
  <LexicalEntry id="e1" partOfSpeech="posV"><MorphemeId>ASA</MorphemeId><Allomorphs><Allomorph id="a1"><PhoneticShape>asa</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>
</LexicalEntries>
</Stratum></Strata></Language></HermitCrabInput>"#,
        a = members(&["a"]),
        input = members(&["s", "z"]),
        output = members(rhs),
    );
    let g = load(&xml).unwrap_or_else(|e| panic!("wide-class grammar failed to load: {e}"));
    assert!(
        g.phon_features.is_empty(),
        "the grammar must be featureless"
    );
    g
}

fn parses(g: &Grammar, word: &str) -> usize {
    let outcome = Morpher::new(g, usize::MAX).parse_word(word);
    assert!(!outcome.capped && !outcome.invalid_shape, "{word}");
    outcome.analyses.len()
}

/// Members at ids 62 and 63 sit in different chunks: the FST is left unconstrained and the membership check decides.
#[test]
fn a_class_across_the_chunk_boundary_admits_exactly_its_members() {
    let g = grammar(&["ä", "ö"]);
    assert_eq!(parses(&g, "aäa"), 1, "ä is a member, chunk 0");
    assert_eq!(parses(&g, "aöa"), 1, "ö is a member, chunk 1");
    assert_eq!(parses(&g, "asa"), 0, "the rule is obligatory");
    assert_eq!(parses(&g, "aza"), 0, "z is no root");
    // b is in the table but outside the class; unapplying onto it would still unify with the root `asa` unless the class is checked.
    assert_eq!(parses(&g, "aba"), 0, "b is not in the right-hand class");
    assert_eq!(parses(&g, "aBa"), 0, "B is not in the right-hand class");
}

/// Both members in chunk 0: the FST itself is constrained to the class.
#[test]
fn a_class_inside_one_chunk_admits_exactly_its_members() {
    let g = grammar(&["p", "q"]);
    assert_eq!(parses(&g, "apa"), 1);
    assert_eq!(parses(&g, "aqa"), 1);
    assert_eq!(parses(&g, "asa"), 0);
    assert_eq!(parses(&g, "ara"), 0, "r is not in the right-hand class");
    assert_eq!(parses(&g, "aäa"), 0, "ä is not in the right-hand class");
}

/// Final synthesis would refuse a wrongly unapplied surface, so the parse verdicts above cannot tell whether analysis itself held the line; unapplying the rule directly does.
#[test]
fn unapplying_a_wide_class_refuses_a_letter_outside_it() {
    let g = grammar(&["ä", "ö"]);
    let pg_grammar_model::model::PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("the grammar's one rule is a rewrite");
    };
    let unapplied = |text: &str| {
        let shape = pg_rules::shape_feat::segment_with_features(&g, &g.char_tables[0], text)
            .unwrap_or_else(|_| panic!("{text:?} should segment"));
        pg_rules::rewrite::analyze(&g, rule, &shape, None).len()
    };
    assert_eq!(unapplied("aäa"), 1);
    assert_eq!(unapplied("aöa"), 1);
    assert_eq!(unapplied("aba"), 0, "b is outside the right-hand class");
    assert_eq!(unapplied("aBa"), 0, "B is outside the right-hand class");
}
