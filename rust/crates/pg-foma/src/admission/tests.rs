use super::*;
use crate::replace::SegAlphabet;
use foma::constructions::fsm_symbol_occurs;
use foma::regex::fsm_parse_regex;
use foma::types::Sides;
use pg_parse::{Morpher, ParseOptions, ParseOutcome};
use pg_rules::stratum::RuleRef;

const ANY: &str = r#"<OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>"#;
const ANY_ONE: &str = r#"<OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>"#;

fn input(parts: &[&str]) -> String {
    let parts: String = parts
        .iter()
        .enumerate()
        .map(|(i, body)| format!(r#"<PhoneticSequence id="p{i}">{body}</PhoneticSequence>"#))
        .collect();
    format!("<MorphologicalInput>{parts}</MorphologicalInput>")
}

fn copy(i: usize) -> String {
    format!(r#"<CopyFromInput index="p{i}" />"#)
}

fn insert(text: &str) -> String {
    format!("<InsertSegments><PhoneticShape>{text}</PhoneticShape></InsertSegments>")
}

fn rule(id: usize, parts: &[&str], output: &[String]) -> String {
    format!(
        r#"<MorphologicalRule id="mr{id}" requiredPartsOfSpeech="posV"><Name>r{id}</Name><MorphemeId>m{id}</MorphemeId><MorphologicalSubrules>
        <MorphologicalSubrule id="sub{id}">{}<MorphologicalOutput>{}</MorphologicalOutput></MorphologicalSubrule>
        </MorphologicalSubrules></MorphologicalRule>"#,
        input(parts),
        output.concat()
    )
}

fn suffix(id: usize, text: &str) -> String {
    rule(id, &[ANY_ONE], &[copy(0), insert(text)])
}

fn prefix(id: usize, text: &str) -> String {
    rule(id, &[ANY_ONE], &[insert(text), copy(0)])
}

struct Spec<'a> {
    segments: &'a str,
    boundary: bool,
    rules: Vec<String>,
    roots: &'a [&'a str],
    prules: &'a str,
    prule_ids: &'a str,
    classes: &'a str,
}

fn grammar_xml(spec: &Spec) -> String {
    let featured = !spec.prules.is_empty();
    let segments: String = spec.segments.chars().map(|c| {
        let value = if featured { format!(r#"<FeatureValue feature="fId" symbolValues="v{c}" />"#) } else { String::new() };
        format!(r#"<SegmentDefinition id="c{c}"><Representations><Representation>{c}</Representation></Representations>{value}</SegmentDefinition>"#)
    }).collect();
    let features = if featured {
        let symbols: String = spec
            .segments
            .chars()
            .map(|c| format!(r#"<Symbol id="v{c}">{c}</Symbol>"#))
            .collect();
        format!(
            r#"<PhonologicalFeatureSystem><SymbolicFeature id="fId"><Name>identity</Name><Symbols>{symbols}</Symbols></SymbolicFeature></PhonologicalFeatureSystem>"#
        )
    } else {
        String::new()
    };
    let boundary = match spec.boundary {
        true => {
            r#"<BoundaryDefinitions><BoundaryDefinition id="bPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition></BoundaryDefinitions>"#
        }
        false => "",
    };
    let rule_ids: Vec<String> = (0..spec.rules.len()).map(|i| format!("mr{i}")).collect();
    let roots: String = spec.roots.iter().enumerate().map(|(i, shape)| {
        format!(r#"<LexicalEntry id="root{i}" partOfSpeech="posV"><MorphemeId>root{i}</MorphemeId><Allomorphs><Allomorph id="rootA{i}"><PhoneticShape>{shape}</PhoneticShape></Allomorph></Allomorphs><Gloss>root{i}</Gloss></LexicalEntry>"#)
    }).collect();
    let prule_attr = match spec.prule_ids.is_empty() {
        true => String::new(),
        false => format!(r#" phonologicalRules="{}""#, spec.prule_ids),
    };
    format!(
        r#"<HermitCrabInput><Language><Name>Admission</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      {features}<CharacterDefinitionTable id="t"><Name>Main</Name><SegmentDefinitions>{segments}</SegmentDefinitions>{boundary}</CharacterDefinitionTable>
      <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>{}</NaturalClasses>
      {}
      <Strata><Stratum characterDefinitionTable="t" morphologicalRuleOrder="unordered" morphologicalRules="{}"{prule_attr}><Name>Main</Name>
      <MorphologicalRuleDefinitions>{}</MorphologicalRuleDefinitions><LexicalEntries>{roots}</LexicalEntries></Stratum></Strata></Language></HermitCrabInput>"#,
        spec.classes,
        spec.prules,
        rule_ids.join(" "),
        spec.rules.concat()
    )
}

fn spec<'a>(segments: &'a str, rules: Vec<String>, roots: &'a [&'a str]) -> Spec<'a> {
    Spec {
        segments,
        boundary: false,
        rules,
        roots,
        prules: "",
        prule_ids: "",
        classes: "",
    }
}

fn load(spec: &Spec) -> Grammar {
    let xml = grammar_xml(spec);
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

fn plan_of(g: &Grammar) -> AdmissionPlan {
    plan_admission(g, &super::class_test(g)).expect("plan")
}

/// Token string for letters of the grammar's table, with `+` for the boundary.
fn tokens(g: &Grammar, letters: &str) -> String {
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    letters
        .chars()
        .map(|c| {
            let text = c.to_string();
            let id = table
                .iter()
                .find(|(_, def)| def.representations_nfd().contains(&text))
                .map(|(id, _)| id)
                .unwrap_or_else(|| panic!("no character {c}"));
            alphabet.token(id)
        })
        .collect()
}

/// True when the piece of rule `rule` accepts exactly `letters`.
fn piece_accepts(g: &Grammar, plan: &AdmissionPlan, rule: usize, letters: &str) -> bool {
    let opts = FomaOptions::default();
    let word = tokens(g, letters);
    plan.pieces
        .iter()
        .filter(|p| p.rule.0 as usize == rule)
        .any(|piece| {
            let net = fsm_parse_regex(&opts, &piece.regex, None, None).expect("piece compiles");
            let mut handle = apply_init(&net);
            let accepted = handle.up(&word).next().is_some();
            accepted
        })
}

fn pieces_of(plan: &AdmissionPlan, rule: usize) -> usize {
    plan.pieces
        .iter()
        .filter(|p| p.rule.0 as usize == rule)
        .count()
}

#[test]
fn prefix_piece_is_the_literal_and_nothing_else() {
    let g = load(&spec("kaqt", vec![prefix(0, "ka")], &["qt"]));
    let plan = plan_of(&g);
    assert!(!plan.free[0]);
    assert!(piece_accepts(&g, &plan, 0, "ka"));
    assert!(!piece_accepts(&g, &plan, 0, "k"));
    assert!(!piece_accepts(&g, &plan, 0, "kaq"));
}

#[test]
fn suffix_piece_is_the_literal_and_nothing_else() {
    let g = load(&spec("taqk", vec![suffix(0, "ta")], &["qk"]));
    let plan = plan_of(&g);
    assert!(!plan.free[0]);
    assert!(piece_accepts(&g, &plan, 0, "ta"));
    assert!(!piece_accepts(&g, &plan, 0, "t"));
}

#[test]
fn circumfix_has_two_literals_and_one_piece() {
    let rules = vec![rule(0, &[ANY_ONE], &[insert("ke"), copy(0), insert("en")])];
    let g = load(&spec("keqn", rules, &["qn"]));
    let plan = plan_of(&g);
    assert_eq!(plan.counts.literals, 2);
    assert_eq!(pieces_of(&plan, 0), 1);
    assert!(!plan.free[0]);
}

#[test]
fn infix_after_first_segment_opens_a_gap_inside_a_short_prefix() {
    let infix = rule(
        1,
        &[r#"<SimpleContext naturalClass="ncAny" />"#, ANY],
        &[copy(0), insert("um"), copy(1)],
    );
    let g = load(&spec("tauqmk", vec![prefix(0, "ta"), infix], &["qk"]));
    let plan = plan_of(&g);
    assert!(piece_accepts(&g, &plan, 0, "ta"));
    assert!(piece_accepts(&g, &plan, 0, "tuma"), "gap holds the infix");
    assert!(!piece_accepts(&g, &plan, 0, "utma"));
    assert!(piece_accepts(&g, &plan, 1, "um"));
}

#[test]
fn full_reduplication_is_free() {
    let g = load(&spec(
        "qk",
        vec![rule(0, &[ANY_ONE], &[copy(0), copy(0)])],
        &["qk"],
    ));
    let plan = plan_of(&g);
    assert!(plan.free[0]);
    assert!(plan.pieces.is_empty());
}

#[test]
fn truncation_makes_the_suffix_tail_optional() {
    let truncate = rule(
        1,
        &[ANY_ONE, r#"<SimpleContext naturalClass="ncAny" />"#],
        &[copy(0)],
    );
    let g = load(&spec("takqn", vec![suffix(0, "tak"), truncate], &["qn"]));
    let plan = plan_of(&g);
    assert_eq!(plan.counts.zones, 1);
    assert!(piece_accepts(&g, &plan, 0, "tak"));
    assert!(
        piece_accepts(&g, &plan, 0, "t"),
        "the last two tokens may be deleted"
    );
    assert!(!piece_accepts(&g, &plan, 0, "ak"), "the first token stays");
}

#[test]
fn short_suffix_under_truncation_is_free() {
    let truncate = rule(
        1,
        &[ANY_ONE, r#"<SimpleContext naturalClass="ncAny" />"#],
        &[copy(0)],
    );
    let g = load(&spec("taqn", vec![suffix(0, "ta"), truncate], &["qn"]));
    assert!(plan_of(&g).free[0], "no plain token is left");
}

const CLASS_X: &str =
    r#"<SegmentNaturalClass id="ncX"><Name>X</Name><Segment segment="cx" /></SegmentNaturalClass>"#;
const CTX_X: &str = r#"<SimpleContext naturalClass="ncX" />"#;
const CTX_ANY: &str = r#"<SimpleContext naturalClass="ncAny" />"#;

fn truncate_last(id: usize) -> String {
    rule(id, &[ANY_ONE, CTX_ANY], &[copy(0)])
}

fn truncate_first_x(id: usize) -> String {
    rule(id, &[CTX_X, ANY_ONE], &[copy(1)])
}

#[test]
fn a_truncation_of_another_class_leaves_the_suffix_usable() {
    let rules = vec![suffix(0, "s"), truncate_first_x(1)];
    let g = load(&Spec {
        classes: CLASS_X,
        ..spec("sxq", rules, &["q"])
    });
    let plan = plan_of(&g);
    assert!(!plan.free[0], "the zone only holds x");
    assert!(piece_accepts(&g, &plan, 0, "s"));
}

#[test]
fn an_unclear_class_still_frees_the_suffix() {
    let g = load(&Spec {
        classes: CLASS_X,
        ..spec("sxq", vec![suffix(0, "s"), truncate_first_x(1)], &["q"])
    });
    let plan = plan_admission(&g, &|_, _| true).expect("plan");
    assert!(plan.free[0], "{:?}", plan.counts);
}

#[test]
fn deletion_is_bounded_by_the_side_it_reaches() {
    let rules = vec![suffix(0, "tak"), truncate_last(1), truncate_first_x(2)];
    let g = load(&Spec {
        classes: CLASS_X,
        ..spec("takxq", rules, &["q"])
    });
    let plan = plan_of(&g);
    assert_eq!(plan.counts.deletion_left, Some(1));
    assert_eq!(plan.counts.deletion_right, Some(1));
    assert!(!plan.free[0]);
    assert!(piece_accepts(&g, &plan, 0, "t"), "the tail may be deleted");
    assert!(!piece_accepts(&g, &plan, 0, "ak"), "the head stays");
}

#[test]
fn a_literal_after_a_copied_part_gets_a_gap_at_the_junction() {
    let infix = rule(
        1,
        &[ANY, CTX_ANY, ANY],
        &[copy(0), copy(1), copy(1), insert("ta"), copy(2)],
    );
    let g = load(&spec("kuqta", vec![prefix(0, "kuu"), infix], &["q"]));
    let plan = plan_of(&g);
    assert!(piece_accepts(&g, &plan, 0, "kuu"));
    assert!(
        piece_accepts(&g, &plan, 0, "kqqtatauu"),
        "the material can land between its copied part and its literal"
    );
}

#[test]
fn a_copied_part_a_truncation_can_reach_is_not_bounded() {
    let tail = rule(
        1,
        &[ANY, CTX_ANY, CTX_ANY],
        &[copy(0), copy(1), insert("ta"), copy(1), copy(2)],
    );
    let truncate = rule(2, &[ANY_ONE, CTX_ANY, CTX_ANY, CTX_ANY], &[copy(0)]);
    let g = load(&spec(
        "kuqta",
        vec![prefix(0, "kuuuuuu"), tail, truncate],
        &["q"],
    ));
    let plan = plan_of(&g);
    assert!(piece_accepts(&g, &plan, 0, "kuuuuuu"));
    let regex = &plan.pieces[0].regex;
    let widened = format!("{} ( ? ) ?* ]*", tokens(&g, "t"));
    assert!(regex.contains(&widened), "{regex}");
}

#[test]
fn modify_only_rule_is_free() {
    let modify = rule(
        0,
        &[r#"<SimpleContext naturalClass="ncAny" />"#],
        &[r#"<ModifyFromInput index="p0"><SimpleContext naturalClass="ncAny" /></ModifyFromInput>"#.to_owned()],
    );
    let g = load(&spec("qk", vec![modify], &["q"]));
    let plan = plan_of(&g);
    assert!(plan.free[0]);
    assert_eq!(plan.counts.zones, 1);
}

#[test]
fn boundary_in_a_literal_is_optional_only_where_a_part_edge_reaches_it() {
    let rules = vec![suffix(0, "ta+"), suffix(1, "ta+k")];
    let mut s = spec("taqk", rules, &["qk"]);
    s.boundary = true;
    let g = load(&s);
    let plan = plan_of(&g);
    assert!(piece_accepts(&g, &plan, 0, "ta+"));
    assert!(
        piece_accepts(&g, &plan, 0, "ta"),
        "the final boundary touches a part edge"
    );
    assert!(piece_accepts(&g, &plan, 1, "ta+k"));
    assert!(
        !piece_accepts(&g, &plan, 1, "tak"),
        "a boundary inside the stem side stays"
    );
}

fn signature(outcome: &ParseOutcome) -> Vec<(Vec<u32>, i32)> {
    let mut keys: Vec<_> = outcome
        .structured
        .iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect();
    keys.sort();
    keys
}

fn restricted(morpher: &Morpher, word: &str, admitted: &[bool]) -> ParseOutcome {
    let rule = |rule: RuleRef| match rule {
        RuleRef::Stratum(_) | RuleRef::Template(_) => true,
        RuleRef::MRule(id) => admitted[id.0 as usize],
    };
    morpher.parse_word_selected(word, &ParseOptions::default(), None, Some(&rule))
}

/// Every word keeps its analyses; returns the words that have any.
fn assert_same_analyses(g: &Grammar, words: &[&str]) -> Vec<String> {
    let admission = build_rule_admission(&FomaOptions::default(), g).expect("admission builds");
    let morpher = Morpher::new(g, 2_000_000);
    let mut analysed = Vec::new();
    for word in words {
        let plain = morpher.parse_word_selected(word, &ParseOptions::default(), None, None);
        let admitted = admission.admit(word, 4096).expect("within allowance");
        let narrowed = restricted(&morpher, word, &admitted);
        assert_eq!(signature(&plain), signature(&narrowed), "word {word}");
        if !plain.structured.is_empty() {
            analysed.push((*word).to_owned());
        }
    }
    analysed
}

#[test]
fn affixes_end_to_end_keep_every_analysis() {
    let rules = vec![prefix(0, "ka"), suffix(1, "ta"), suffix(2, "nu")];
    let g = load(&spec("kaqtnu", rules, &["qt", "tq"]));
    let words = [
        "qt", "kaqt", "qtta", "kaqtta", "qtnu", "kaqtnu", "tqta", "qq", "kakaqt",
    ];
    let analysed = assert_same_analyses(&g, &words);
    assert!(analysed.len() >= 5, "{analysed:?}");
}

#[test]
fn an_absent_literal_is_not_admitted() {
    let rules = vec![prefix(0, "ka"), suffix(1, "ta"), suffix(2, "nu")];
    let g = load(&spec("kaqtnu", rules, &["qt"]));
    let admission = build_rule_admission(&FomaOptions::default(), &g).expect("builds");
    let admitted = admission.admit("kaqt", 4096).expect("allowance");
    assert_eq!(admitted, vec![true, false, false]);
    let admitted = admission.admit("qtnu", 4096).expect("allowance");
    assert_eq!(admitted, vec![false, false, true]);
}

#[test]
fn infix_circumfix_and_truncation_end_to_end() {
    let infix = rule(
        0,
        &[r#"<SimpleContext naturalClass="ncAny" />"#, ANY],
        &[copy(0), insert("um"), copy(1)],
    );
    let circumfix = rule(1, &[ANY_ONE], &[insert("ke"), copy(0), insert("en")]);
    let truncate = rule(
        2,
        &[ANY_ONE, r#"<SimpleContext naturalClass="ncAny" />"#],
        &[copy(0)],
    );
    let g = load(&spec(
        "kenumqt",
        vec![infix, circumfix, truncate, suffix(3, "tak")],
        &["qkt", "nqt"],
    ));
    let words = [
        "qkt",
        "qumkt",
        "keqkten",
        "kequmkten",
        "qk",
        "qktak",
        "qkta",
        "qkt",
        "nuqmt",
        "numqt",
    ];
    let analysed = assert_same_analyses(&g, &words);
    assert!(analysed.len() >= 4, "{analysed:?}");
}

const ASSIMILATION: &str = r#"<PhonologicalRuleDefinitions><PhonologicalRule id="pr1"><Name>n to m after p</Name>
  <PhoneticInput><PhoneticSequence><Segment segment="cn" /></PhoneticSequence></PhoneticInput>
  <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="cm" /></PhoneticSequence></PhoneticOutput>
  <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence><Segment segment="cp" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
  </PhonologicalSubrule></PhonologicalSubrules></PhonologicalRule></PhonologicalRuleDefinitions>"#;

#[test]
fn phonology_that_changes_an_affix_literal_keeps_the_analysis() {
    let mut s = spec(
        "pmnatku",
        vec![suffix(0, "na"), suffix(1, "ku")],
        &["tap", "tat"],
    );
    s.prules = ASSIMILATION;
    s.prule_ids = "pr1";
    let g = load(&s);
    let analysed = assert_same_analyses(
        &g,
        &["tapna", "tapma", "tatna", "tatma", "tapku", "tap", "tatku"],
    );
    assert!(analysed.contains(&"tapma".to_owned()), "{analysed:?}");
    let admission = build_rule_admission(&FomaOptions::default(), &g).expect("builds");
    let admitted = admission.admit("tapma", 4096).expect("allowance");
    assert!(admitted[0], "the changed literal still admits its rule");
    assert!(!admitted[1], "{admitted:?} {:?}", admission.plan().free);
}

#[test]
fn morphology_after_the_phonological_stratum_is_refused() {
    let xml = grammar_xml(&Spec {
        prules: ASSIMILATION,
        prule_ids: "pr1",
        ..spec("pmnat", vec![suffix(0, "na")], &["tap"])
    })
    .replace(
        "</Strata>",
        &format!(
            r#"<Stratum characterDefinitionTable="t" morphologicalRuleOrder="unordered" morphologicalRules="mr9"><Name>Late</Name><MorphologicalRuleDefinitions>{}</MorphologicalRuleDefinitions></Stratum></Strata>"#,
            suffix(9, "ta")
        ),
    );
    let g = pg_grammar::load(&xml).expect("loads");
    let refusal = build_rule_admission(&FomaOptions::default(), &g)
        .err()
        .expect("refused");
    assert!(
        matches!(refusal, AdmissionRefusal::StratumLayout(_)),
        "{refusal:?}"
    );
}

fn machine_net(g: &Grammar) -> foma::types::Fsm {
    let opts = FomaOptions::default();
    let plan = plan_of(g);
    let mut evidence = AdmissionEvidence::default();
    let mut meter = build::Meter::default();
    let machine = build::machine(&opts, g, &plan, &mut evidence, &mut meter).expect("machine");
    let (cascade, items) = build::cascade(&opts, g, &mut evidence, &mut meter).expect("cascade");
    build::finish(
        &opts,
        g,
        machine.net,
        cascade,
        &items,
        &mut evidence,
        &mut meter,
    )
    .expect("finish")
}

#[test]
fn upper_side_is_one_tag_and_lower_side_has_no_tag() {
    let g = load(&spec(
        "kaqtnu",
        vec![prefix(0, "ka"), suffix(1, "nu")],
        &["qt"],
    ));
    let net = machine_net(&g);
    for tag in ["<A:0>", "<A:1>"] {
        assert!(fsm_symbol_occurs(&net, tag, Sides::UPPER), "{tag} upper");
        assert!(!fsm_symbol_occurs(&net, tag, Sides::LOWER), "{tag} lower");
    }
    for letter in ["k", "a", "q", "t", "n", "u"] {
        assert!(
            fsm_symbol_occurs(&net, letter, Sides::LOWER),
            "{letter} lower"
        );
        assert!(
            !fsm_symbol_occurs(&net, letter, Sides::UPPER),
            "{letter} upper"
        );
    }
    let admission = build_rule_admission(&FomaOptions::default(), &g).expect("builds");
    let mut handle = admission.handle.lock().unwrap();
    for word in ["kaqt", "qtnu", "kaqtnu"] {
        let uppers: Vec<String> = handle.up(word).collect();
        assert!(!uppers.is_empty(), "{word}");
        assert!(
            uppers.iter().all(|u| admission.tags.contains_key(u)),
            "{uppers:?}"
        );
    }
}

#[test]
fn tokens_known_only_to_the_stem_pass_through_a_question_mark_position() {
    let truncate = rule(
        1,
        &[ANY_ONE, r#"<SimpleContext naturalClass="ncAny" />"#],
        &[copy(0)],
    );
    let g = load(&spec("takqn", vec![suffix(0, "tak"), truncate], &["qnq"]));
    let admission = build_rule_admission(&FomaOptions::default(), &g).expect("builds");
    assert!(admission.admit("qnqtn", 4096).expect("allowance")[0]);
    assert!(admission.admit("qnqtak", 4096).expect("allowance")[0]);
    assert!(!admission.admit("qnqq", 4096).expect("allowance")[0]);
}

#[test]
fn the_allowance_returns_none() {
    let g = load(&spec("kaqt", vec![prefix(0, "ka")], &["qt"]));
    let admission = build_rule_admission(&FomaOptions::default(), &g).expect("builds");
    let (admitted, raw) = admission.admit_counted("kaqt", 0);
    assert!(admitted.is_none());
    assert_eq!(raw, 1);
}

#[test]
fn a_decomposed_letter_in_the_word_reaches_the_suffix_rule() {
    let g = load(&spec("cafés", vec![suffix(0, "s")], &["café"]));
    let analysed = assert_same_analyses(&g, &["café", "cafés", "cafe"]);
    assert_eq!(analysed, vec!["café", "cafés"]);
    let admission = build_rule_admission(&FomaOptions::default(), &g).expect("builds");
    assert!(admission.admit("cafés", 4096).expect("allowance")[0]);
}

#[test]
fn a_mark_segment_after_a_letter_segment_reaches_the_suffix_rule() {
    let g = load(&spec("pan\u{301}s", vec![suffix(0, "s")], &["pa\u{301}n"]));
    let analysed = assert_same_analyses(&g, &["pa\u{301}n", "pa\u{301}ns"]);
    assert_eq!(analysed.len(), 2, "{analysed:?}");
    let admission = build_rule_admission(&FomaOptions::default(), &g).expect("builds");
    assert!(admission.admit("pa\u{301}ns", 4096).expect("allowance")[0]);
}
