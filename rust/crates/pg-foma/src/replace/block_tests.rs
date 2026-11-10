//! Block relation and bundle-closure checks over synthetic featured grammars.
use super::*;
use crate::cascade_parts::CascadeParts;
use foma::apply::apply_init;
use std::collections::BTreeSet;

const HEAD: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput><Language><Name>Block</Name>
<PartsOfSpeech><PartOfSpeech id="posN"><Name>n</Name></PartOfSpeech></PartsOfSpeech>"#;

fn segment_xml(id: &str, rep: &str, values: &[(&str, &str)]) -> String {
    let values: String = values
        .iter()
        .map(|(feature, symbol)| {
            format!(r#"<FeatureValue feature="{feature}" symbolValues="{symbol}" />"#)
        })
        .collect();
    format!(
        r#"<SegmentDefinition id="{id}"><Representations><Representation>{rep}</Representation></Representations>{values}</SegmentDefinition>"#
    )
}

fn feature_xml(id: &str, symbols: &[&str]) -> String {
    let symbols: String = symbols
        .iter()
        .map(|symbol| format!(r#"<Symbol id="{symbol}">{symbol}</Symbol>"#))
        .collect();
    format!(
        r#"<SymbolicFeature id="{id}"><Name>{id}</Name><Symbols>{symbols}</Symbols></SymbolicFeature>"#
    )
}

fn class_xml(id: &str, values: &[(&str, &str)]) -> String {
    let values: String = values
        .iter()
        .map(|(feature, symbol)| {
            format!(r#"<FeatureValue feature="{feature}" symbolValues="{symbol}" />"#)
        })
        .collect();
    format!(r#"<FeatureNaturalClass id="{id}"><Name>{id}</Name>{values}</FeatureNaturalClass>"#)
}

fn context(class: &str, vars: &[&str]) -> String {
    let vars: String = vars
        .iter()
        .map(|var| format!(r#"<AlphaVariable variableFeature="{var}" />"#))
        .collect();
    let vars = if vars.is_empty() {
        String::new()
    } else {
        format!("<AlphaVariables>{vars}</AlphaVariables>")
    };
    format!(r#"<SimpleContext naturalClass="{class}">{vars}</SimpleContext>"#)
}

fn rule_xml(id: &str, lhs: &str, rhs: &str) -> String {
    format!(
        r#"<PhonologicalRule id="{id}" multipleApplicationOrder="leftToRightIterative"><Name>{id}</Name>
<VariableFeatures><VariableFeature id="vA" name="a" phonologicalFeature="fa" /><VariableFeature id="vB" name="b" phonologicalFeature="fb" /></VariableFeatures>
<PhoneticInput><PhoneticSequence>{lhs}</PhoneticSequence></PhoneticInput>
<PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence>{rhs}</PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
</PhonologicalRule>"#
    )
}

/// Consonants, vowels and glyphs over two binary features, with a fusion, an in-place and a boundary-focus rule.
fn fusion_grammar() -> Grammar {
    let kinds = [
        ("p", "t", "k", "s"),
        ("i", "e", "u", "o"),
        ("A", "B", "D", "E"),
    ];
    let mut segments = String::new();
    for (kind, reps) in ["kC", "kV", "kG"].iter().zip(kinds) {
        let reps = [reps.0, reps.1, reps.2, reps.3];
        for (n, rep) in reps.iter().enumerate() {
            let a = ["a1", "a1", "a2", "a2"][n];
            let b = ["b1", "b2", "b1", "b2"][n];
            segments += &segment_xml(
                &format!("c{rep}"),
                rep,
                &[("fk", kind), ("fa", a), ("fb", b)],
            );
        }
    }
    let rules = [
        rule_xml(
            "rFuse",
            &(context("ncC", &["vA"]) + &context("ncV", &["vB"])),
            &context("ncG", &["vA", "vB"]),
        ),
        rule_xml(
            "rFeat",
            &(context("ncC", &["vA"]) + &context("ncV", &[])),
            &(context("ncB2", &[]) + &context("ncV", &["vA"])),
        ),
        rule_xml(
            "rBound",
            &(context("ncC", &["vA"])
                + r#"<Segment segment="cplus" />"#
                + &context("ncV", &["vB"])),
            &context("ncG", &["vA", "vB"]),
        ),
        rule_xml("rPin", &context("ncC", &[]), &context("ncB2", &[])),
    ];
    let xml = format!(
        r#"{HEAD}<PhonologicalFeatureSystem>{}{}{}</PhonologicalFeatureSystem>
<CharacterDefinitionTable id="tbl"><Name>Main</Name><SegmentDefinitions>{segments}</SegmentDefinitions>
<BoundaryDefinitions><BoundaryDefinition id="cplus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition></BoundaryDefinitions></CharacterDefinitionTable>
<NaturalClasses>{}{}{}{}</NaturalClasses>
<PhonologicalRuleDefinitions>{}</PhonologicalRuleDefinitions>
<Strata><Stratum characterDefinitionTable="tbl" phonologicalRules="rFuse rFeat rBound rPin"><Name>Main</Name><LexicalEntries>
<LexicalEntry id="e1" partOfSpeech="posN"><Allomorphs><Allomorph id="a1"><PhoneticShape>pi</PhoneticShape></Allomorph></Allomorphs><MorphemeId>R</MorphemeId></LexicalEntry>
</LexicalEntries></Stratum></Strata></Language></HermitCrabInput>"#,
        feature_xml("fk", &["kC", "kV", "kG"]),
        feature_xml("fa", &["a1", "a2"]),
        feature_xml("fb", &["b1", "b2"]),
        class_xml("ncC", &[("fk", "kC")]),
        class_xml("ncV", &[("fk", "kV")]),
        class_xml("ncG", &[("fk", "kG")]),
        class_xml("ncB2", &[("fb", "b2")]),
        rules.concat(),
    );
    pg_grammar::load(&xml).expect("synthetic grammar loads")
}

fn cascade(g: &Grammar, positions: &[usize]) -> (Fsm, CascadeEvidence) {
    let rules: Vec<_> = positions.iter().map(|i| &g.prules[*i]).collect();
    compile_proposing_cascade(&FomaOptions::default(), g, &rules, &mut Vec::new())
        .expect("block cascade compiles")
}

fn encoded(g: &Grammar, text: &str) -> Option<String> {
    let table = &g.char_tables[0];
    let shape = pg_grammar::segment::segment(table, text).ok()?;
    Some(SegAlphabet::new(table).encode_shape(&shape))
}

/// The token string of a synthesized shape; lane-only nodes name the attested segment with the same feature lanes.
fn shape_tokens(g: &Grammar, shape: &pg_shape::Shape) -> String {
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let type_lane = g.phon_features.type_flat().0 as usize;
    shape
        .interior()
        .map(|(index, _, cd, _)| {
            if cd != pg_shape::NO_CHAR_DEF {
                return alphabet.token(CharDefId(cd));
            }
            let lanes = shape.node_lanes(index);
            let same = |candidate: &[u64]| {
                candidate
                    .iter()
                    .enumerate()
                    .all(|(lane, bits)| lane == type_lane || *bits == lanes[lane])
            };
            let (id, _) = table
                .iter()
                .find(|(_, definition)| same(definition.feature_lanes()))
                .expect("synthesized node is an attested bundle");
            alphabet.token(id)
        })
        .collect()
}

fn hc_output(g: &Grammar, rule: usize, input: &str) -> Option<String> {
    let PhonRuleDef::Rewrite(rule) = &g.prules[rule] else {
        panic!("rewrite")
    };
    let shape = pg_grammar::segment::segment(&g.char_tables[0], input).ok()?;
    Some(
        match pg_rules::rewrite::synthesize(g, rule, &shape).first() {
            Some(output) => shape_tokens(g, output),
            None => shape_tokens(g, &shape),
        },
    )
}

fn down(net: &Fsm, input: &str) -> BTreeSet<String> {
    apply_init(net).down(input).collect()
}

#[test]
fn block_relation_includes_every_synthesis_output_up_to_length_four() {
    let g = fusion_grammar();
    let alphabet = ["p", "k", "i", "o", "+"];
    let mut words = vec![String::new()];
    let mut layer = vec![String::new()];
    for _ in 0..4 {
        layer = layer
            .iter()
            .flat_map(|word| alphabet.iter().map(move |c| format!("{word}{c}")))
            .collect();
        words.extend(layer.iter().cloned());
    }
    for rule in 0..3 {
        let (net, evidence) = cascade(&g, &[rule]);
        assert_eq!(evidence.rules[0].construction, RuleConstruction::Block);
        let mut checked = 0;
        for word in words.iter().skip(1) {
            let (Some(expected), Some(input)) = (hc_output(&g, rule, word), encoded(&g, word))
            else {
                continue;
            };
            checked += 1;
            assert!(
                down(&net, &input).contains(&expected),
                "rule {rule}: synthesis output of {word:?} is missing"
            );
        }
        assert!(checked > 700, "rule {rule}: only {checked} words segmented");
    }
}

#[test]
fn fusion_maps_each_pair_only_to_the_row_synthesis_produces() {
    let g = fusion_grammar();
    let (net, evidence) = cascade(&g, &[0]);
    assert_eq!(evidence.rules[0].block_tuples, 16);
    assert_eq!(evidence.rules[0].blocks, 16);
    assert!(!evidence.rules[0].block_widened);
    let glyph = |a: usize, b: usize| ["A", "B", "D", "E"][a * 2 + b];
    for (c, a) in [("p", 0), ("t", 0), ("k", 1), ("s", 1)] {
        for (v, b) in [("i", 0), ("e", 1), ("u", 0), ("o", 1)] {
            let word = format!("{c}{v}");
            let input = encoded(&g, &word).unwrap();
            let expected = BTreeSet::from([input.clone(), encoded(&g, glyph(a, b)).unwrap()]);
            assert_eq!(down(&net, &input), expected, "{word}");
            assert!(expected.contains(&hc_output(&g, 0, &word).unwrap()));
        }
    }
}

#[test]
fn an_explicit_boundary_in_the_focus_is_consumed() {
    let g = fusion_grammar();
    let (net, evidence) = cascade(&g, &[2]);
    assert_eq!(evidence.rules[0].block_tuples, 16);
    let input = encoded(&g, "k+e").unwrap();
    let fused = encoded(&g, "D").unwrap();
    let _ = fused;
    let outputs = down(&net, &input);
    assert!(outputs.contains(&encoded(&g, "E").unwrap()), "{outputs:?}");
    assert!(outputs.contains(&input));
}

fn widening_grammar(lanes: usize) -> Grammar {
    let features: String = (0..lanes)
        .map(|n| feature_xml(&format!("f{n}"), &[&format!("m{n}"), &format!("p{n}")]))
        .collect();
    let mut segments = String::new();
    for mask in 0..1usize << lanes {
        let values: Vec<(String, String)> = (0..lanes)
            .map(|n| {
                let symbol = if mask >> n & 1 == 1 { "p" } else { "m" };
                (format!("f{n}"), format!("{symbol}{n}"))
            })
            .collect();
        let values: Vec<(&str, &str)> = values
            .iter()
            .map(|(f, s)| (f.as_str(), s.as_str()))
            .collect();
        let rep = char::from_u32('a' as u32 + mask as u32)
            .unwrap()
            .to_string();
        segments += &segment_xml(
            &format!("s{mask}"),
            &rep,
            &[&[("fk", "kX")], &values[..]].concat(),
        );
    }
    let kind = feature_xml("fk", &["kX", "kY"]);
    let variables: String = (0..lanes)
        .map(|n| format!(r#"<VariableFeature id="v{n}" name="v{n}" phonologicalFeature="f{n}" />"#))
        .collect();
    let vars: Vec<String> = (0..lanes).map(|n| format!("v{n}")).collect();
    let vars: Vec<&str> = vars.iter().map(String::as_str).collect();
    let xml = format!(
        r#"{HEAD}<PhonologicalFeatureSystem>{kind}{features}</PhonologicalFeatureSystem>
<CharacterDefinitionTable id="tbl"><Name>Main</Name><SegmentDefinitions>{segments}</SegmentDefinitions></CharacterDefinitionTable>
<NaturalClasses>{}{}</NaturalClasses>
<PhonologicalRuleDefinitions><PhonologicalRule id="rWide" multipleApplicationOrder="leftToRightIterative"><Name>w</Name>
<VariableFeatures>{variables}</VariableFeatures>
<PhoneticInput><PhoneticSequence>{}</PhoneticSequence></PhoneticInput>
<PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence>{}</PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
</PhonologicalRule></PhonologicalRuleDefinitions>
<Strata><Stratum characterDefinitionTable="tbl" phonologicalRules="rWide"><Name>Main</Name><LexicalEntries>
<LexicalEntry id="e1" partOfSpeech="posN"><Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs><MorphemeId>R</MorphemeId></LexicalEntry>
</LexicalEntries></Stratum></Strata></Language></HermitCrabInput>"#,
        class_xml("ncX", &[("fk", "kX")]),
        class_xml("ncY", &[("fk", "kY")]),
        context("ncX", &vars),
        context("ncY", &vars) + &context("ncY", &vars),
    );
    pg_grammar::load(&xml).expect("widening grammar loads")
}

#[test]
fn several_alpha_output_lanes_do_not_multiply_private_rows() {
    let private: Vec<usize> = (1..=4)
        .map(|lanes| {
            let g = widening_grammar(lanes);
            let (_, evidence) = cascade(&g, &[0]);
            assert_eq!(evidence.bundles.widened_alpha_outputs, 2 * lanes);
            evidence.bundles.private_rows
        })
        .collect();
    assert!(
        private.iter().all(|rows| *rows == private[0]),
        "{private:?}"
    );
    assert!(private[0] <= 2, "{private:?}");
}

#[test]
fn morphology_can_create_a_bundle_no_character_has() {
    let xml = format!(
        r#"{HEAD}<PhonologicalFeatureSystem>{}{}</PhonologicalFeatureSystem>
<CharacterDefinitionTable id="tbl"><Name>Main</Name><SegmentDefinitions>{}{}</SegmentDefinitions></CharacterDefinitionTable>
<NaturalClasses>{}{}</NaturalClasses>
<Strata><Stratum characterDefinitionTable="tbl" morphologicalRules="mr"><Name>Main</Name>
<MorphologicalRuleDefinitions><MorphologicalRule id="mr" requiredPartsOfSpeech="posN" outputPartOfSpeech="posN"><Name>mr</Name><MorphologicalSubrules>
<MorphologicalSubrule id="sub"><MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
<MorphologicalOutput><ModifyFromInput index="stem"><SimpleContext naturalClass="ncC2" /></ModifyFromInput></MorphologicalOutput></MorphologicalSubrule>
</MorphologicalSubrules><MorphemeId>M</MorphemeId></MorphologicalRule></MorphologicalRuleDefinitions>
<LexicalEntries><LexicalEntry id="e1" partOfSpeech="posN"><Allomorphs><Allomorph id="a1"><PhoneticShape>x</PhoneticShape></Allomorph></Allomorphs><MorphemeId>R</MorphemeId></LexicalEntry></LexicalEntries>
</Stratum></Strata></Language></HermitCrabInput>"#,
        feature_xml("fa", &["a1", "a2"]),
        feature_xml("fc", &["c1", "c2"]),
        segment_xml("cx", "x", &[("fa", "a1")]),
        segment_xml("cy", "y", &[("fa", "a2"), ("fc", "c1")]),
        class_xml("ncC2", &[("fc", "c2")]),
        class_xml("ncAny", &[]),
    );
    let g = pg_grammar::load(&xml).expect("morphology grammar loads");
    let parts = CascadeParts::build(&FomaOptions::default(), &g, &[]).unwrap();
    let table = &g.char_tables[0];
    let token_of = |text: &str| {
        let id = table.lookup_nfd(text).unwrap();
        crate::replace::chardef_token(id)
    };
    assert_eq!(parts.tokens.attested_segments.len(), 2);
    assert_eq!(parts.tokens.private_segments.len(), 2);
    let spelled: Vec<BTreeSet<String>> = parts
        .tokens
        .private_segments
        .iter()
        .map(|token| down(&parts.spelling, &token.to_string()))
        .collect();
    let x = BTreeSet::from([token_of("x").to_string()]);
    assert_eq!(
        spelled.iter().filter(|set| **set == x).count(),
        1,
        "{spelled:?}"
    );
    // The other private row matches no character and spells as nothing instead of failing the path.
    assert_eq!(
        spelled.iter().filter(|set| set.contains("")).count(),
        1,
        "{spelled:?}"
    );
    let (_, evidence) = parts.compose(&FomaOptions::default()).unwrap();
    assert_eq!(evidence.bundles.morphology_rows, 2);
    assert_eq!(evidence.bundles.unspelled_rows, 1);
}

#[test]
fn a_single_node_rule_maps_each_row_only_to_itself_or_its_synthesis_output() {
    let g = fusion_grammar();
    let (net, evidence) = cascade(&g, &[3]);
    assert_eq!(evidence.rules[0].construction, RuleConstruction::Site);
    for (word, other) in [("p", "t"), ("t", "t"), ("k", "s"), ("s", "s"), ("i", "i")] {
        let input = encoded(&g, word).unwrap();
        let expected = BTreeSet::from([input.clone(), encoded(&g, other).unwrap()]);
        assert_eq!(down(&net, &input), expected, "{word}");
        assert!(expected.contains(&hc_output(&g, 3, word).unwrap()));
    }
    for word in ["pi", "kk", "pip", "ksi"] {
        let input = encoded(&g, word).unwrap();
        let outputs = down(&net, &input);
        assert!(outputs.contains(&hc_output(&g, 3, word).unwrap()), "{word}");
        assert!(outputs.len() <= 1 << word.len(), "{word}: {outputs:?}");
    }
}

fn seq(contexts: &str) -> String {
    format!("<PhoneticTemplate><PhoneticSequence>{contexts}</PhoneticSequence></PhoneticTemplate>")
}

/// Segments without features, a syncope between consonants and a `t` to `k` change after a vowel.
fn featureless_grammar() -> Grammar {
    let segments: String = ["p", "t", "k", "i", "e"]
        .iter()
        .map(|rep| segment_xml(&format!("c{rep}"), rep, &[]))
        .collect();
    let class = |id: &str, members: &[&str]| {
        let members: String = members
            .iter()
            .map(|rep| format!(r#"<Segment segment="c{rep}" />"#))
            .collect();
        format!(
            r#"<SegmentNaturalClass id="{id}"><Name>{id}</Name>{members}</SegmentNaturalClass>"#
        )
    };
    let ctx = |class: &str| format!(r#"<SimpleContext naturalClass="{class}" />"#);
    let environment = |left: &str, right: &str| {
        let left = if left.is_empty() {
            String::new()
        } else {
            format!("<LeftEnvironment>{}</LeftEnvironment>", seq(left))
        };
        let right = if right.is_empty() {
            String::new()
        } else {
            format!("<RightEnvironment>{}</RightEnvironment>", seq(right))
        };
        format!("<Environment>{left}{right}</Environment>")
    };
    let rule = |id: &str, lhs: &str, rhs: &str, env: &str| {
        format!(
            r#"<PhonologicalRule id="{id}" multipleApplicationOrder="simultaneous"><Name>{id}</Name>
<PhoneticInput><PhoneticSequence>{lhs}</PhoneticSequence></PhoneticInput>
<PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence>{rhs}</PhoneticSequence></PhoneticOutput>{env}</PhonologicalSubrule></PhonologicalSubrules>
</PhonologicalRule>"#
        )
    };
    let rules = [
        rule(
            "rSyn",
            &ctx("ncV"),
            "",
            &environment(&ctx("ncC"), &ctx("ncC")),
        ),
        rule(
            "rTK",
            r#"<Segment segment="ct" />"#,
            r#"<Segment segment="ck" />"#,
            &environment(&ctx("ncV"), ""),
        ),
    ];
    let xml = format!(
        r#"{HEAD}<CharacterDefinitionTable id="tbl"><Name>Main</Name><SegmentDefinitions>{segments}</SegmentDefinitions></CharacterDefinitionTable>
<NaturalClasses>{}{}</NaturalClasses>
<PhonologicalRuleDefinitions>{}</PhonologicalRuleDefinitions>
<Strata><Stratum characterDefinitionTable="tbl" phonologicalRules="rSyn rTK"><Name>Main</Name><LexicalEntries>
<LexicalEntry id="e1" partOfSpeech="posN"><Allomorphs><Allomorph id="a1"><PhoneticShape>pi</PhoneticShape></Allomorph></Allomorphs><MorphemeId>R</MorphemeId></LexicalEntry>
</LexicalEntries></Stratum></Strata></Language></HermitCrabInput>"#,
        class("ncC", &["p", "t", "k"]),
        class("ncV", &["i", "e"]),
        rules.concat(),
    );
    pg_grammar::load(&xml).expect("featureless grammar loads")
}

#[test]
fn without_features_a_rule_relates_only_the_spellings_it_names() {
    let g = featureless_grammar();
    let outputs = |rule: usize, word: &str| {
        let (net, evidence) = cascade(&g, &[rule]);
        assert_eq!(evidence.rules[0].construction, RuleConstruction::Site);
        down(&net, &encoded(&g, word).unwrap())
    };
    let set = |words: &[&str]| -> BTreeSet<String> {
        words
            .iter()
            .map(|word| encoded(&g, word).unwrap())
            .collect()
    };
    assert_eq!(outputs(0, "pip"), set(&["pip", "pp"]));
    assert_eq!(outputs(0, "pie"), set(&["pie"]));
    assert_eq!(outputs(0, "kek"), set(&["kek", "kk"]));
    assert_eq!(outputs(1, "it"), set(&["it", "ik"]));
    assert_eq!(outputs(1, "pt"), set(&["pt"]));
    assert_eq!(outputs(1, "ek"), set(&["ek"]));
    for (rule, word) in [(0, "pip"), (0, "tet"), (1, "et"), (1, "it")] {
        assert!(
            outputs(rule, word).contains(&hc_output(&g, rule, word).unwrap()),
            "rule {rule}: {word}"
        );
    }
}
