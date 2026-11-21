use pg_grammar::model::{Grammar, PhonRuleDef};

/// A synthetic grammar over segments `a p s t` (two features each) with boundary `+`.
pub(crate) fn grammar(prules: &str, strata: &str) -> Grammar {
    let segment = |id: &str, f: &str, g: &str| {
        format!(
            r#"<SegmentDefinition id="c{id}"><Representations><Representation>{}</Representation></Representations><FeatureValue feature="f" symbolValues="f{f}" /><FeatureValue feature="g" symbolValues="g{g}" /></SegmentDefinition>"#,
            id.to_lowercase()
        )
    };
    let segments = [
        segment("A", "P", "P"),
        segment("P", "M", "M"),
        segment("T", "M", "P"),
        segment("S", "P", "M"),
    ]
    .concat();
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?><HermitCrabInput><Language><Name>L</Name><PartsOfSpeech><PartOfSpeech id="v"><Name>V</Name></PartOfSpeech></PartsOfSpeech><PhonologicalFeatureSystem><SymbolicFeature id="f"><Name>f</Name><Symbols><Symbol id="fP">+</Symbol><Symbol id="fM">-</Symbol></Symbols></SymbolicFeature><SymbolicFeature id="g"><Name>g</Name><Symbols><Symbol id="gP">+</Symbol><Symbol id="gM">-</Symbol></Symbols></SymbolicFeature></PhonologicalFeatureSystem><CharacterDefinitionTable id="t"><Name>Main</Name><SegmentDefinitions>{segments}</SegmentDefinitions><BoundaryDefinitions><BoundaryDefinition id="b"><Representations><Representation>+</Representation></Representations></BoundaryDefinition></BoundaryDefinitions></CharacterDefinitionTable><NaturalClasses><FeatureNaturalClass id="any"><Name>Any</Name></FeatureNaturalClass></NaturalClasses><PhonologicalRuleDefinitions>{prules}</PhonologicalRuleDefinitions><Strata>{strata}</Strata></Language></HermitCrabInput>"#
    );
    pg_grammar::load(&xml).expect("the synthetic grammar loads")
}

fn segment_nodes(id: &str, letters: &str) -> String {
    letters
        .chars()
        .enumerate()
        .map(|(i, c)| {
            format!(
                r#"<Segment id="{id}{i}" segment="c{}" />"#,
                c.to_ascii_uppercase()
            )
        })
        .collect()
}

/// A rule deleting `letter` anywhere.
pub(crate) fn deletion(id: &str, letter: &str) -> String {
    format!(
        r#"<PhonologicalRule id="{id}"><Name>{id}</Name><PhoneticInput><PhoneticSequence>{}</PhoneticSequence></PhoneticInput><PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules></PhonologicalRule>"#,
        segment_nodes(id, letter)
    )
}

/// A rule rewriting `from` as `to` anywhere.
pub(crate) fn change(id: &str, from: &str, to: &str) -> String {
    format!(
        r#"<PhonologicalRule id="{id}"><Name>{id}</Name><PhoneticInput><PhoneticSequence>{}</PhoneticSequence></PhoneticInput><PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence>{}</PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules></PhonologicalRule>"#,
        segment_nodes(id, from),
        segment_nodes(&format!("{id}o"), to)
    )
}

/// A metathesis rule over `letters` whose last two nodes are the switches, written as FieldWorks writes them.
pub(crate) fn metathesis(id: &str, letters: &str) -> String {
    let n = letters.len();
    format!(
        r#"<MetathesisRule id="{id}" leftSwitch="{id}{}" rightSwitch="{id}{}"><Name>{id}</Name><StructuralDescription><PhoneticTemplate><PhoneticSequence>{}</PhoneticSequence></PhoneticTemplate></StructuralDescription></MetathesisRule>"#,
        n - 1,
        n - 2,
        segment_nodes(id, letters)
    )
}

/// An affix rule with output `before` + stem + `after`; an empty side inserts nothing.
pub(crate) fn affix(id: &str, before: &str, after: &str) -> String {
    let insert = |shape: &str| {
        if shape.is_empty() {
            String::new()
        } else {
            format!("<InsertSegments><PhoneticShape>{shape}</PhoneticShape></InsertSegments>")
        }
    };
    format!(
        r#"<MorphologicalRule id="{id}" requiredPartsOfSpeech="v"><Name>{id}</Name><MorphologicalSubrules><MorphologicalSubrule id="{id}s"><MorphologicalInput><PhoneticSequence id="{id}S"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput>{}<CopyFromInput index="{id}S" />{}</MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>{id}</MorphemeId></MorphologicalRule>"#,
        insert(before),
        insert(after)
    )
}

/// A compounding rule that joins head and non-head with nothing between them.
pub(crate) fn compounding(id: &str) -> String {
    format!(
        r#"<CompoundingRule id="{id}"><Name>{id}</Name><CompoundingSubrules><CompoundingSubrule><HeadMorphologicalInput><PhoneticSequence id="{id}h"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput><NonHeadMorphologicalInput><PhoneticSequence id="{id}n"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput><MorphologicalOutput><CopyFromInput index="{id}h" /><CopyFromInput index="{id}n" /></MorphologicalOutput></CompoundingSubrule></CompoundingSubrules></CompoundingRule>"#
    )
}

/// A stratum listing `mrules` (defined by `defs`) and `prules`, with roots `roots`.
pub(crate) fn stratum(
    name: &str,
    mrules: &str,
    prules: &str,
    defs: &str,
    roots: &[&str],
) -> String {
    let entries: String = roots
        .iter()
        .enumerate()
        .map(|(i, shape)| {
            format!(
                r#"<LexicalEntry id="{name}r{i}" partOfSpeech="v"><Allomorphs><Allomorph id="{name}r{i}a"><PhoneticShape>{shape}</PhoneticShape></Allomorph></Allomorphs><MorphemeId>{name}R{i}</MorphemeId></LexicalEntry>"#
            )
        })
        .collect();
    format!(
        r#"<Stratum characterDefinitionTable="t" morphologicalRuleOrder="unordered" morphologicalRules="{mrules}" phonologicalRules="{prules}"><Name>{name}</Name><MorphologicalRuleDefinitions>{defs}</MorphologicalRuleDefinitions><LexicalEntries>{entries}</LexicalEntries></Stratum>"#
    )
}

/// Whether the one metathesis rule of `g` is refused as able to swap across morphs.
fn refused(g: &Grammar) -> bool {
    let (index, rule) = g
        .prules
        .iter()
        .enumerate()
        .find_map(|(i, rule)| match rule {
            PhonRuleDef::Metathesis(rule) => Some((i, rule)),
            PhonRuleDef::Rewrite(_) => None,
        })
        .expect("one metathesis rule");
    super::metathesis_can_swap_across_morphs(g, pg_grammar::model::PRuleId(index as _), rule)
}

/// Prefix `p` and suffix `p` meet only over root `a`, which can be deleted; the context pair `p p` then spans two morphs. Kills `(Prefix, Suffix) => false`.
#[test]
fn a_prefix_and_suffix_that_meet_over_a_vanishing_root_are_adjacent() {
    let prules = [deletion("d", "a"), metathesis("m", "ppst")].concat();
    let defs = [affix("PR", "p", ""), affix("SU", "", "p")].concat();
    let g = grammar(&prules, &stratum("S0", "PR SU", "d m", &defs, &["a"]));
    assert!(refused(&g));
    let kept = grammar(
        &[metathesis("m", "ppst")].concat(),
        &stratum("S0", "PR SU", "m", &defs, &["a"]),
    );
    assert!(
        !refused(&kept),
        "with no deletion the root keeps them apart"
    );
}

/// Root `a` can be deleted, so it is a morph with no segment beside root `st`, whose swap can move it. Kills dropping `segmentless |= root_can_vanish`.
#[test]
fn a_root_that_can_vanish_makes_the_grammar_segmentless() {
    let prules = [deletion("d", "a"), metathesis("m", "st")].concat();
    let defs = affix("PR", "p", "");
    let g = grammar(&prules, &stratum("S0", "PR", "d m", &defs, &["a", "st"]));
    assert!(refused(&g));
    let kept = grammar(
        &metathesis("m", "st"),
        &stratum("S0", "PR", "m", &defs, &["a", "st"]),
    );
    assert!(!refused(&kept), "with no deletion nothing is segmentless");
}

/// A compound has no segment of its own at the junction, so a swap inside root `s`'s run can move it. Kills `segmentless: false` for a compounding grammar.
#[test]
fn a_compounding_rule_makes_the_grammar_segmentless() {
    let g = grammar(
        &metathesis("m", "st"),
        &stratum("S0", "CR", "m", &compounding("CR"), &["s", "p"]),
    );
    assert!(refused(&g));
    let kept = grammar(
        &metathesis("m", "st"),
        &stratum("S0", "PR", "m", &affix("PR", "p", ""), &["s", "p"]),
    );
    assert!(
        !refused(&kept),
        "without the compound nothing is segmentless"
    );
}
