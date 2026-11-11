//! Iterative empty-focus insertion against the synthesis oracle, including insertions fed into the same gap.
use super::block_tests::{
    cascade, class_xml, encoded, feature_xml, hc_output, segment_xml, seq, HEAD,
};
use super::*;
use foma::constructions::fsm_compose;
use foma::structures::fsm_isempty;

/// A replacement or environment node: `C`/`V` are classes, anything else a literal segment.
fn node(name: &str) -> String {
    match name {
        "C" | "V" => format!(r#"<SimpleContext naturalClass="nc{name}" />"#),
        literal => format!(r#"<Segment segment="c{literal}" />"#),
    }
}

fn nodes(names: &[&str]) -> String {
    names.iter().map(|name| node(name)).collect()
}

/// One iterative rule with an empty focus; each subrule is (replacement, left, right).
fn rule_xml(id: &str, dir: &str, subrules: &[Subrule]) -> String {
    let subrules: String = subrules
        .iter()
        .map(|(rhs, left, right)| {
            let side = |tag: &str, names: &[&str]| {
                if names.is_empty() {
                    String::new()
                } else {
                    format!("<{tag}>{}</{tag}>", seq(&nodes(names)))
                }
            };
            format!(
                "<PhonologicalSubrule><PhoneticOutput><PhoneticSequence>{}</PhoneticSequence></PhoneticOutput><Environment>{}{}</Environment></PhonologicalSubrule>",
                nodes(rhs),
                side("LeftEnvironment", left),
                side("RightEnvironment", right),
            )
        })
        .collect();
    format!(
        r#"<PhonologicalRule id="{id}" multipleApplicationOrder="{dir}"><Name>{id}</Name><PhoneticInput /><PhonologicalSubrules>{subrules}</PhonologicalSubrules></PhonologicalRule>"#
    )
}

type Subrule = (
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
);

const RULES: &[(&str, &[Subrule])] = &[
    // Each insertion before a consonant recreates the environment of the next.
    ("rSelf", &[(&["i"], &[], &["C"])]),
    // An inserted consonant is the left environment of the next insertion.
    ("rLeft", &[(&["t"], &["C"], &[])]),
    // A two-node replacement fed into the middle of itself.
    ("rPair", &[(&["t", "i"], &["C"], &[])]),
    // Never fed: the inserted vowel breaks the environment.
    ("rPlain", &[(&["i"], &["C"], &["C"])]),
    // Two subrules, the second feeding the first.
    ("rTwo", &[(&["i"], &["C"], &[]), (&["t"], &["V"], &["C"])]),
    // The environment is a literal and a class the replacement can satisfy.
    ("rLiteral", &[(&["t"], &["p"], &["V"])]),
];

fn insertion_grammar(dir: &str) -> Grammar {
    let segments: String = [
        ("p", "kC"),
        ("t", "kC"),
        ("k", "kC"),
        ("i", "kV"),
        ("e", "kV"),
    ]
    .iter()
    .map(|(rep, kind)| segment_xml(&format!("c{rep}"), rep, &[("fk", kind)]))
    .collect();
    let rules: String = RULES
        .iter()
        .map(|(id, subrules)| rule_xml(id, dir, subrules))
        .collect();
    let names: Vec<&str> = RULES.iter().map(|(id, _)| *id).collect();
    let xml = format!(
        r#"{HEAD}<PhonologicalFeatureSystem>{}</PhonologicalFeatureSystem>
<CharacterDefinitionTable id="tbl"><Name>Main</Name><SegmentDefinitions>{segments}</SegmentDefinitions>
<BoundaryDefinitions><BoundaryDefinition id="cplus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition></BoundaryDefinitions></CharacterDefinitionTable>
<NaturalClasses>{}{}</NaturalClasses>
<PhonologicalRuleDefinitions>{rules}</PhonologicalRuleDefinitions>
<Strata><Stratum characterDefinitionTable="tbl" phonologicalRules="{}"><Name>Main</Name><LexicalEntries>
<LexicalEntry id="e1" partOfSpeech="posN"><Allomorphs><Allomorph id="a1"><PhoneticShape>pi</PhoneticShape></Allomorph></Allomorphs><MorphemeId>R</MorphemeId></LexicalEntry>
</LexicalEntries></Stratum></Strata></Language></HermitCrabInput>"#,
        feature_xml("fk", &["kC", "kV"]),
        class_xml("ncC", &[("fk", "kC")]),
        class_xml("ncV", &[("fk", "kV")]),
        names.join(" "),
    );
    pg_grammar::load(&xml).expect("insertion grammar loads")
}

fn acceptor(tokens: &str) -> Fsm {
    let spaced: Vec<String> = tokens.chars().map(String::from).collect();
    let regex = if spaced.is_empty() {
        "0".to_owned()
    } else {
        format!("[{}]", spaced.join(" "))
    };
    fsm_parse_regex(&FomaOptions::default(), &regex, None, None).expect("string acceptor")
}

/// Whether `net` relates `input` to `output`; the relation may be infinite, so it is composed rather than enumerated.
pub(super) fn relates(net: &Fsm, input: &str, output: &str) -> bool {
    let opts = FomaOptions::default();
    let from_input = fsm_compose(&opts, acceptor(input), net.clone());
    let mut pair = fsm_compose(&opts, from_input, acceptor(output));
    !fsm_isempty(&opts, &mut pair)
}

fn words() -> Vec<String> {
    let alphabet = ["p", "t", "i", "+"];
    let mut all = Vec::new();
    let mut layer = vec![String::new()];
    for _ in 0..3 {
        layer = layer
            .iter()
            .flat_map(|word| alphabet.iter().map(move |c| format!("{word}{c}")))
            .collect();
        all.extend(layer.iter().cloned());
    }
    all
}

#[test]
fn every_synthesis_output_of_an_iterative_insertion_is_in_the_net() {
    for dir in ["leftToRightIterative", "rightToLeftIterative"] {
        let g = insertion_grammar(dir);
        let words = words();
        for (rule, (id, _)) in RULES.iter().enumerate() {
            let (net, evidence) = cascade(&g, &[rule]);
            assert_eq!(evidence.rules[0].construction, RuleConstruction::Site);
            let (mut checked, mut changed) = (0, 0);
            for word in &words {
                let (Some(expected), Some(input)) = (hc_output(&g, rule, word), encoded(&g, word))
                else {
                    continue;
                };
                checked += 1;
                changed += usize::from(expected != input);
                assert!(
                    relates(&net, &input, &expected),
                    "{dir} {id}: synthesis output of {word:?} is missing"
                );
            }
            assert!(checked > 60, "{dir} {id}: only {checked} words segmented");
            assert!(changed > 0, "{dir} {id}: synthesis never inserted");
        }
    }
}

#[test]
fn an_insertion_fed_into_its_own_gap_is_in_the_net() {
    let g = insertion_grammar("leftToRightIterative");
    let at = |name: &str| RULES.iter().position(|(id, _)| *id == name).unwrap();
    // `p` -> `pt` -> `ptt`: each inserted `t` is the consonant the next insertion follows.
    let (net, _) = cascade(&g, &[at("rLeft")]);
    let fed = hc_output(&g, at("rLeft"), "p").unwrap();
    assert!(fed.chars().count() > 2, "synthesis feeds the insertion");
    assert!(relates(&net, &encoded(&g, "p").unwrap(), &fed));
    // A fed pair is inserted inside itself: `p` -> `pti` -> `ptiti`... with synthesis's own nesting.
    let (net, _) = cascade(&g, &[at("rPair")]);
    let nested = hc_output(&g, at("rPair"), "p").unwrap();
    assert!(nested.chars().count() > 3);
    assert!(relates(&net, &encoded(&g, "p").unwrap(), &nested));
}

#[test]
fn an_insertion_the_environment_forbids_is_not_in_the_net() {
    let g = insertion_grammar("leftToRightIterative");
    let at = |name: &str| RULES.iter().position(|(id, _)| *id == name).unwrap();
    // rPlain inserts only between two consonants: no vowel is created anywhere else.
    let (net, _) = cascade(&g, &[at("rPlain")]);
    let input = encoded(&g, "pi").unwrap();
    assert!(relates(&net, &input, &input));
    assert!(!relates(&net, &input, &encoded(&g, "pii").unwrap()));
    assert!(!relates(&net, &input, &encoded(&g, "ipi").unwrap()));
    assert!(relates(
        &net,
        &encoded(&g, "pp").unwrap(),
        &encoded(&g, "pip").unwrap()
    ));
}

#[test]
fn the_construction_adds_a_bounded_number_of_states_per_rule() {
    let g = insertion_grammar("leftToRightIterative");
    for (rule, (id, _)) in RULES.iter().enumerate() {
        let (_, evidence) = cascade(&g, &[rule]);
        let part = &evidence.rules[0];
        assert!(part.states < 40 && part.arcs < 400, "{id}: {part:?}");
    }
}
