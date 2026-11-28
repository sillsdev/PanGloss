//! An affix rule that follows another in analysis is skipped when the outer rule's required inflection-class or exception features can never be met by what the inner rule outputs. Pruning must never change an analysis; it must only be applied when the pair provably fails synthesis.

#[path = "csharp_port_common/mod.rs"]
mod csharp_port_common;

use csharp_port_common::build_grammar_mpr;
use pg_grammar_model::model::{Grammar, MRuleId, MorphRuleDef};
use pg_parse::Morpher;
use pg_rules::pair_gate::MprDeadPairs;

const MPR: &str = r#"
<MorphologicalPhonologicalRuleFeatures>
  <MorphologicalPhonologicalRuleFeature id="clsA">A</MorphologicalPhonologicalRuleFeature>
  <MorphologicalPhonologicalRuleFeature id="clsB">B</MorphologicalPhonologicalRuleFeature>
  <MorphologicalPhonologicalRuleFeature id="clsC">C</MorphologicalPhonologicalRuleFeature>
  <MorphologicalPhonologicalRuleFeature id="exX">X</MorphologicalPhonologicalRuleFeature>
  <MorphologicalPhonologicalRuleFeature id="exY">Y</MorphologicalPhonologicalRuleFeature>
  <MorphologicalPhonologicalRuleFeature id="ungrouped">U</MorphologicalPhonologicalRuleFeature>
  <MorphologicalPhonologicalRuleFeatureGroup matchType="any" outputType="overwrite" features="clsA clsB clsC"><Name>inflClasses</Name></MorphologicalPhonologicalRuleFeatureGroup>
  <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="overwrite" features="exX exY"><Name>exceptionFeatures</Name></MorphologicalPhonologicalRuleFeatureGroup>
</MorphologicalPhonologicalRuleFeatures>
"#;

/// A suffix rule `id` inserting `letter`; `req` and `out` are space-separated feature ids.
fn rule(id: &str, letter: &str, req: &str, out: &str) -> String {
    let req = if req.is_empty() {
        String::new()
    } else {
        format!(r#" requiredMPRFeatures="{req}""#)
    };
    let out = if out.is_empty() {
        String::new()
    } else {
        format!(r#" MPRFeatures="{out}""#)
    };
    format!(
        r#"<MorphologicalRule id="{id}" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>{id}</Name><MorphemeId>{id}M</MorphemeId>
        <MorphologicalSubrules><MorphologicalSubrule id="{id}Sub">
          <MorphologicalInput{req}><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput{out}><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+{letter}</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>"#
    )
}

fn entry(id: &str, shape: &str, features: &str) -> String {
    let features = if features.is_empty() {
        String::new()
    } else {
        format!(r#" ruleFeatures="{features}""#)
    };
    format!(
        r#"<LexicalEntry id="{id}" partOfSpeech="posV"{features}><MorphemeId>{id}</MorphemeId>
        <Allomorphs><Allomorph id="{id}A"><PhoneticShape>{shape}</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>"#
    )
}

const ROOTS: [(&str, &str, &str); 5] = [
    ("rA", "pat", "clsA"),
    ("rB", "kat", "clsB"),
    ("rC", "tat", "clsC"),
    ("rN", "dat", ""),
    ("rAX", "bat", "clsA exX"),
];

fn grammar(rules: &[String]) -> Grammar {
    let ids = rules
        .iter()
        .map(|r| {
            let start = r.find("id=\"").unwrap() + 4;
            r[start..start + r[start..].find('"').unwrap()].to_string()
        })
        .collect::<Vec<_>>()
        .join(" ");
    let lexicon: String = ROOTS.iter().map(|(i, s, f)| entry(i, s, f)).collect();
    let strata = format!(
        r#"<Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="{ids}">
        <Name>S</Name><MorphologicalRuleDefinitions>{}</MorphologicalRuleDefinitions>
        <LexicalEntries>{lexicon}</LexicalEntries></Stratum></Strata>"#,
        rules.concat()
    );
    build_grammar_mpr(MPR, &strata)
}

fn rule_id(g: &Grammar, name: &str) -> MRuleId {
    let i = g
        .mrules
        .iter()
        .position(|r| matches!(r, MorphRuleDef::AffixProcess(a) if a.name.as_deref() == Some(name)))
        .unwrap_or_else(|| panic!("no rule {name}"));
    MRuleId(i as u32)
}

fn dead(g: &Grammar, outer: &str, inner: &str) -> bool {
    MprDeadPairs::build(g).is_dead(rule_id(g, outer), rule_id(g, inner))
}

/// Every string of `0..=depth` suffix letters after each root.
fn words(letters: &[&str], depth: usize) -> Vec<String> {
    let mut out = Vec::new();
    for (_, shape, _) in ROOTS {
        let mut layer = vec![shape.to_string()];
        out.push(shape.to_string());
        for _ in 0..depth {
            layer = layer
                .iter()
                .flat_map(|w| letters.iter().map(move |l| format!("{w}{l}")))
                .collect();
            out.extend(layer.iter().cloned());
        }
    }
    out
}

fn sorted(mut v: Vec<(String, String)>) -> Vec<(String, String)> {
    v.sort();
    v
}

/// Parses every word with the gate on and off: analyses must agree, and the gate must not add work. Returns `(on, off)` work totals and the number of words that have an analysis.
fn identical(g: &Grammar, letters: &[&str], depth: usize) -> (u64, u64, usize) {
    let on = Morpher::new(g, usize::MAX).with_mpr_pair_gate(true);
    let off = Morpher::new(g, usize::MAX).with_mpr_pair_gate(false);
    let (mut son, mut soff, mut parsed) = (0, 0, 0);
    for w in words(letters, depth) {
        let a = on.parse_word(&w);
        let b = off.parse_word(&w);
        assert_eq!(
            sorted(a.analyses.clone()),
            sorted(b.analyses.clone()),
            "{w}"
        );
        assert!(!a.capped && !b.capped, "{w}");
        son += a.work_steps as u64;
        soff += b.work_steps as u64;
        parsed += usize::from(!a.analyses.is_empty());
    }
    (son, soff, parsed)
}

#[test]
fn an_inner_rule_that_overwrites_the_class_out_of_the_required_set_is_dead() {
    let g = grammar(&[
        rule("outA", "s", "clsA", "clsB"),
        rule("inB", "z", "clsA", "clsB"),
        rule("inA", "d", "clsA", "clsA"),
    ]);
    assert!(
        dead(&g, "outA", "inB"),
        "B cannot satisfy a requirement of A"
    );
    assert!(!dead(&g, "outA", "inA"), "A satisfies a requirement of A");
    let (on, off, parsed) = identical(&g, &["s", "z", "d"], 3);
    assert!(parsed > 5);
    assert!(on < off, "on={on} off={off}");
}

#[test]
fn an_inner_rule_without_class_output_is_never_dead() {
    let g = grammar(&[
        rule("outA", "s", "clsA", ""),
        rule("inNone", "z", "", ""),
        rule("inUngrouped", "d", "", "ungrouped"),
        rule("inExcept", "f", "", "exX"),
    ]);
    for inner in ["inNone", "inUngrouped", "inExcept"] {
        assert!(!dead(&g, "outA", inner), "{inner} leaves the root's class");
    }
    identical(&g, &["s", "z", "d", "f"], 3);
}

#[test]
fn an_inner_rule_adding_several_classes_satisfies_any_of_them() {
    let g = grammar(&[
        rule("needC", "s", "clsC", ""),
        rule("needA", "v", "clsA", ""),
        rule("needBorC", "g", "clsB clsC", ""),
        rule("both", "z", "", "clsB clsC"),
        rule("onlyA", "d", "", "clsA"),
    ]);
    assert!(!dead(&g, "needC", "both"));
    assert!(dead(&g, "needA", "both"));
    assert!(
        !dead(&g, "needBorC", "both"),
        "any-of: either member suffices"
    );
    assert!(dead(&g, "needBorC", "onlyA"));
    assert!(dead(&g, "needC", "onlyA"));
    assert!(!dead(&g, "needA", "onlyA"));
    identical(&g, &["s", "v", "g", "z", "d"], 3);
}

#[test]
fn all_of_exception_features_are_checked_against_what_the_inner_rule_writes() {
    let g = grammar(&[
        rule("needXY", "s", "exX exY", ""),
        rule("needX", "v", "exX", ""),
        rule("writesX", "z", "", "exX"),
        rule("writesXY", "d", "", "exX exY"),
        rule("writesClass", "f", "", "clsB"),
    ]);
    assert!(
        dead(&g, "needXY", "writesX"),
        "Y is missing after the overwrite"
    );
    assert!(!dead(&g, "needXY", "writesXY"));
    assert!(!dead(&g, "needX", "writesX"));
    assert!(
        !dead(&g, "needXY", "writesClass"),
        "a class output leaves the exception group to the root"
    );
    identical(&g, &["s", "v", "z", "d", "f"], 3);
}

#[test]
fn an_ungrouped_requirement_is_not_decided() {
    let g = grammar(&[
        rule("needU", "s", "ungrouped", ""),
        rule("inB", "z", "", "clsB"),
    ]);
    assert!(!dead(&g, "needU", "inB"));
    identical(&g, &["s", "z"], 3);
}

#[test]
fn the_pair_needs_every_allomorph_of_the_outer_rule_to_fail() {
    // One rule with two allomorphs: the first needs A, the second needs B.
    let two = r#"<MorphologicalRule id="outTwo" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>outTwo</Name><MorphemeId>outTwoM</MorphemeId>
        <MorphologicalSubrules>
        <MorphologicalSubrule id="outTwoSub1">
          <MorphologicalInput requiredMPRFeatures="clsA"><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+s</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule>
        <MorphologicalSubrule id="outTwoSub2">
          <MorphologicalInput requiredMPRFeatures="clsB"><PhoneticSequence id="2"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="2" /><InsertSegments><PhoneticShape>+s</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule>
        </MorphologicalSubrules></MorphologicalRule>"#
        .to_string();
    let g = grammar(&[
        two,
        rule("inB", "z", "", "clsB"),
        rule("inC", "d", "", "clsC"),
    ]);
    assert!(!dead(&g, "outTwo", "inB"), "the second allomorph accepts B");
    assert!(dead(&g, "outTwo", "inC"));
    identical(&g, &["s", "z", "d"], 3);
}

#[test]
fn a_group_that_appends_keeps_the_root_class_so_nothing_is_decided() {
    let append = MPR.replace(
        r#"matchType="any" outputType="overwrite" features="clsA clsB clsC""#,
        r#"matchType="any" outputType="append" features="clsA clsB clsC""#,
    );
    let rules = [rule("outA", "s", "clsA", ""), rule("inB", "z", "", "clsB")];
    let ids = "outA inB";
    let lexicon: String = ROOTS.iter().map(|(i, s, f)| entry(i, s, f)).collect();
    let strata = format!(
        r#"<Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="{ids}">
        <Name>S</Name><MorphologicalRuleDefinitions>{}</MorphologicalRuleDefinitions>
        <LexicalEntries>{lexicon}</LexicalEntries></Stratum></Strata>"#,
        rules.concat()
    );
    let g = build_grammar_mpr(&append, &strata);
    assert!(!dead(&g, "outA", "inB"));
    // The root keeps class A beneath, so the pair is a real analysis.
    let found = Morpher::new(&g, usize::MAX).parse_word("patzs");
    assert!(!found.analyses.is_empty());
}

#[test]
fn a_second_stratum_does_not_hide_a_class_written_in_between() {
    // Rules of two strata: synthesis applies the first stratum's rules, then the second's; the outer rule still sees the inner rule's output.
    let inner = rule("inB", "z", "", "clsB");
    let outer = rule("outA", "s", "clsA", "");
    let lexicon: String = ROOTS.iter().map(|(i, s, f)| entry(i, s, f)).collect();
    let strata = format!(
        r#"<Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="inB"><Name>S1</Name>
          <MorphologicalRuleDefinitions>{inner}</MorphologicalRuleDefinitions><LexicalEntries>{lexicon}</LexicalEntries></Stratum>
        <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="outA"><Name>S2</Name>
          <MorphologicalRuleDefinitions>{outer}</MorphologicalRuleDefinitions></Stratum>
        </Strata>"#
    );
    let g = build_grammar_mpr(MPR, &strata);
    let on = Morpher::new(&g, usize::MAX).with_mpr_pair_gate(true);
    let off = Morpher::new(&g, usize::MAX).with_mpr_pair_gate(false);
    for (_, shape, _) in ROOTS {
        for suffix in ["", "z", "s", "zs", "sz", "zz", "ss", "zss", "szs"] {
            let w = format!("{shape}{suffix}");
            assert_eq!(
                sorted(on.parse_word(&w).analyses),
                sorted(off.parse_word(&w).analyses),
                "{w}"
            );
        }
    }
}

/// The blowup shape: each of four affixes has three rules with different From and To classes, so a word of several affixes multiplies by three per affix until the root's class is checked.
fn blowup_rules() -> Vec<String> {
    let mut rules = Vec::new();
    for (letter, name) in [("s", "ca"), ("z", "ap"), ("d", "re"), ("f", "pa")] {
        for (n, (from, to)) in [("clsA", "clsB"), ("clsB", "clsC"), ("clsC", "clsA")]
            .into_iter()
            .enumerate()
        {
            rules.push(rule(&format!("{name}{n}"), letter, from, to));
        }
    }
    rules
}

#[test]
fn several_msas_per_affix_do_not_multiply_the_search() {
    let g = grammar(&blowup_rules());
    let on = Morpher::new(&g, usize::MAX).with_mpr_pair_gate(true);
    let off = Morpher::new(&g, usize::MAX).with_mpr_pair_gate(false);
    let word = "patszdf";
    let a = on.parse_word(word);
    let b = off.parse_word(word);
    assert_eq!(sorted(a.analyses.clone()), sorted(b.analyses.clone()));
    println!("work on={} off={}", a.work_steps, b.work_steps);
    assert!(a.work_steps < 600, "work bound: {}", a.work_steps);
    assert!(
        a.work_steps * 2 < b.work_steps,
        "on={} off={}",
        a.work_steps,
        b.work_steps
    );
    let (son, soff, parsed) = identical(&g, &["s", "z", "d", "f"], 3);
    assert!(parsed > 20, "{parsed}");
    assert!(son < soff, "on={son} off={soff}");
}

/// Parses each word with the gate on and off and asserts equal analyses; returns the number of words that have one.
fn same(g: &Grammar, words: &[String]) -> usize {
    let on = Morpher::new(g, usize::MAX).with_mpr_pair_gate(true);
    let off = Morpher::new(g, usize::MAX).with_mpr_pair_gate(false);
    let mut parsed = 0;
    for w in words {
        let (a, b) = (on.parse_word(w), off.parse_word(w));
        assert_eq!(sorted(a.analyses.clone()), sorted(b.analyses), "{w}");
        assert!(!a.capped, "{w}");
        parsed += usize::from(!a.analyses.is_empty());
    }
    parsed
}

fn all_words(prefixes: &[&str], suffixes: &[&str], roots: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for p in prefixes.iter().chain(std::iter::once(&"")) {
        for r in roots {
            out.push(format!("{p}{r}"));
            for a in suffixes {
                out.push(format!("{p}{r}{a}"));
                for b in suffixes {
                    out.push(format!("{p}{r}{a}{b}"));
                }
            }
        }
    }
    out
}

fn prefix_rule(id: &str, letter: &str, req: &str, out: &str) -> String {
    rule(id, letter, req, out).replace(
        &format!("<CopyFromInput index=\"1\" /><InsertSegments><PhoneticShape>+{letter}</PhoneticShape></InsertSegments>"),
        &format!("<InsertSegments><PhoneticShape>{letter}+</PhoneticShape></InsertSegments><CopyFromInput index=\"1\" />"),
    )
}

#[test]
fn a_clitic_rule_is_never_a_pair_and_its_parses_are_unchanged() {
    let clitic = rule("outA", "s", "clsA", "").replace(
        "requiredPartsOfSpeech=\"posV\"",
        "requiredPartsOfSpeech=\"posV\" isCliticRule=\"true\"",
    );
    let g = grammar(&[clitic, rule("inB", "z", "", "clsB")]);
    assert!(
        !dead(&g, "outA", "inB"),
        "a clitic outer rule is not decided"
    );
    assert!(
        !dead(&g, "inB", "outA"),
        "a clitic inner rule is not decided"
    );
    let words = all_words(&[], &["s", "z"], &["pat", "kat", "tat", "dat", "bat"]);
    assert!(same(&g, &words) > 5);
}

#[test]
fn a_compounding_rule_between_the_pair_changes_no_parse() {
    let compound = r#"<CompoundingRule id="cmp" blockable="false" headPartsOfSpeech="posV" nonHeadPartsOfSpeech="posV"><Name>cmp</Name>
      <CompoundingSubrules><CompoundingSubrule>
        <HeadMorphologicalInput><PhoneticSequence id="h"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
        <NonHeadMorphologicalInput><PhoneticSequence id="n"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
        <MorphologicalOutput><CopyFromInput index="h" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="n" /></MorphologicalOutput>
      </CompoundingSubrule></CompoundingSubrules></CompoundingRule>"#;
    let g = grammar(&[
        rule("outA", "s", "clsA", ""),
        rule("inB", "z", "", "clsB"),
        compound.to_string(),
    ]);
    // The pair is still decided between the two affix rules; compounding is only a trail slot that breaks adjacency.
    assert!(dead(&g, "outA", "inB"));
    let mut words = all_words(&[], &["s", "z"], &["pat", "kat", "tat"]);
    for (h, n) in [("pat", "kat"), ("kat", "tat"), ("tat", "pat")] {
        for h_suffix in ["", "z", "s", "zs"] {
            for n_suffix in ["", "z", "s", "zs"] {
                words.push(format!("{h}{h_suffix}{n}{n_suffix}"));
            }
        }
    }
    let plain = all_words(&[], &["s", "z"], &["pat", "kat", "tat"]).len();
    let parsed = same(&g, &words);
    println!(
        "compounding: parsed {parsed} of {} ({plain} plain)",
        words.len()
    );
    assert!(parsed > 5);
}

#[test]
fn a_prefix_and_suffix_in_an_unordered_stratum_fold_the_same_with_the_gate() {
    let g = grammar(&[
        rule("outA", "s", "clsA", ""),
        rule("inB", "z", "", "clsB"),
        prefix_rule("preB", "m", "", "clsB"),
        prefix_rule("preA", "n", "clsA", ""),
    ]);
    assert!(dead(&g, "outA", "inB"));
    assert!(dead(&g, "outA", "preB"));
    let words = all_words(
        &["m", "n", "mn", "nm"],
        &["s", "z"],
        &["pat", "kat", "tat", "dat", "bat"],
    );
    assert!(same(&g, &words) > 20);
}

#[test]
fn a_realizational_rule_between_the_pair_changes_no_parse() {
    let realizational = |id: &str, letter: &str, out: &str| {
        let out = if out.is_empty() {
            String::new()
        } else {
            format!(r#" MPRFeatures="{out}""#)
        };
        format!(
            r#"<RealizationalRule id="{id}"><Name>{id}</Name><MorphologicalSubrules><MorphologicalSubrule id="{id}Sub">
              <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput{out}><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+{letter}</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules><MorphemeId>{id}M</MorphemeId></RealizationalRule>"#
        )
    };
    let words = all_words(&[], &["s", "z", "f"], &["pat", "kat", "tat", "dat", "bat"]);
    // A realizational rule that writes no class leaves the pair decided.
    let quiet = grammar(&[
        rule("outA", "s", "clsA", ""),
        rule("inB", "z", "", "clsB"),
        realizational("rr", "f", ""),
    ]);
    assert!(dead(&quiet, "outA", "inB"));
    same(&quiet, &words);
    // One that writes the group makes the group undecidable, so nothing is pruned.
    let writes = grammar(&[
        rule("outA", "s", "clsA", ""),
        rule("inB", "z", "", "clsB"),
        realizational("rr", "f", "clsA"),
    ]);
    assert!(!dead(&writes, "outA", "inB"));
    same(&writes, &words);
}
