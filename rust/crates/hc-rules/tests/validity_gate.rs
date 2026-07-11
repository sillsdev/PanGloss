//! Regression gate for plan §13.1 Tier-1 #5 (`Allomorph.IsWordValid`'s previously-unenforced
//! sub-gates): environments, bound roots, and per-allomorph required syntactic FS.
//!
//! Two layers, mirroring this project's established style (`type_lane_gate.rs` drives the raw
//! `PatternBridge`→`hc_fst` primitive; `cd_set_gate.rs`/`morph_gate.rs` drive real loaded grammars
//! end-to-end):
//! - [`environments_ok`] tests drive `hc_rules::validity::environments_ok` directly against a
//!   hand-built `Shape`/`EnvironmentDef` (the probe grammar, phonology only) — the anchored
//!   left/right matching primitive this fix reuses verbatim from `rewrite.rs`.
//! - The XML-loaded tests build tiny real grammars (`hc_grammar::load`) with a lexicon and a
//!   morphological rule, so `Grammar::allomorph_owners`/`entries`/`mrules` are populated the same
//!   way a real grammar's are, then hand-build [`Word`]s (real `AllomorphId`/`MorphemeId` values
//!   looked up from the loaded grammar, not arbitrary literals — the load-bearing correction over
//!   `morph_gate.rs`'s style, which never registers its hand-built rules into a `Grammar`) and call
//!   `hc_rules::validity::allomorphs_valid` directly, the same function `hc-parse::Morpher::
//!   is_word_valid` calls in production.

use hc_grammar::model::{AllomorphId, EnvironmentDef, Grammar, MorphRuleDef, Pattern, PatternNode};
use hc_rules::validity::{allomorphs_valid, environments_ok};
use hc_rules::word::MorphRecord;
use hc_rules::Word;
use hc_shape::{NodeKind, Shape, ShapeBuilder};

mod common;
use common::{ctx, load_probe_grammar, nat_class};

// =================================================================================================
// Part 1 -- `environments_ok`: the anchored left/right matching primitive, hand-built inputs.
// =================================================================================================

fn probe_shape(g: &Grammar, text: &str) -> Shape {
    let t = &g.char_tables[0];
    let seg = hc_grammar::segment::segment(t, text).expect("segments");
    let w = g.phon_features.len() as u32;
    let mut b = ShapeBuilder::with_features_capacity(w, seg.len());
    for (_, kind, cd, _) in seg.interior() {
        let mut lanes = vec![u64::MAX; w as usize];
        for (i, &l) in t.get(hc_grammar::chardef::CharDefId(cd)).feature_lanes().iter().enumerate() {
            lanes[i] = l;
        }
        match kind {
            NodeKind::Segment => b.push_segment_with_lanes(cd, &lanes),
            NodeKind::Boundary => b.push_boundary_with_lanes(cd, &lanes),
            _ => {}
        }
    }
    b.finish()
}

fn single(g: &Grammar, nc: &str) -> Pattern {
    Pattern { nodes: vec![PatternNode::Context(ctx(nat_class(g, nc)))] }
}

fn env(require: bool, left: Option<Pattern>, right: Option<Pattern>) -> EnvironmentDef {
    EnvironmentDef { require, left, right }
}

/// A required environment with both sides declared must anchor each side to the *correct* edge of
/// the morph span -- the advisor-flagged risk of a left/right mix-up. Probe word "dat" (d-a-t):
/// morph = the middle "a" (interior index 1); env = "preceded by d (nc_d)" + "followed by t (nc_t)".
/// Only the true `d _ t` orientation satisfies both; every other permutation fails at least one
/// side, and a left/right swap in the caller would make even the true-positive case fail (nc_d and
/// nc_t are disjoint singleton classes, not symmetric).
#[test]
fn environments_ok_anchors_left_and_right_to_the_correct_sides() {
    let g = load_probe_grammar();
    let e = env(true, Some(single(&g, "nc_d")), Some(single(&g, "nc_t")));
    let envs = vec![e];

    // "dat": preceded by d, followed by t -- both sides hold.
    let dat = probe_shape(&g, "dat");
    assert!(environments_ok(&g, &envs, &dat, 1, 1), "d _ t: both sides should hold");

    // "tad": preceded by t (not d), followed by d (not t) -- both sides fail.
    let tad = probe_shape(&g, "tad");
    assert!(!environments_ok(&g, &envs, &tad, 1, 1), "t _ d: neither side should hold");

    // "dad": preceded by d (holds), followed by d not t (fails) -- AND semantics, overall false.
    let dad = probe_shape(&g, "dad");
    assert!(!environments_ok(&g, &envs, &dad, 1, 1), "d _ d: right side fails, so overall false");

    // "tat": preceded by t not d (fails), followed by t (holds) -- overall false.
    let tat = probe_shape(&g, "tat");
    assert!(!environments_ok(&g, &envs, &tat, 1, 1), "t _ t: left side fails, so overall false");
}

/// A left-only environment anchored at the word start: on "at" the target morph (index 0, the "a")
/// has no left context at all, so a real (non-anchor) left pattern must fail to match -- there is
/// nothing to satisfy it. On "dat" targeting index 1, the left context ("d") does satisfy it.
#[test]
fn environments_ok_left_only_requires_real_preceding_context() {
    let g = load_probe_grammar();
    let envs = vec![env(true, Some(single(&g, "nc_d")), None)];

    let at = probe_shape(&g, "at");
    assert!(!environments_ok(&g, &envs, &at, 0, 0), "no left context at all -- cannot match nc_d");

    let dat = probe_shape(&g, "dat");
    assert!(environments_ok(&g, &envs, &dat, 1, 1), "preceded by d");
}

/// `ConstraintType.Exclude` (`EnvironmentDef.require == false`) inverts the match: the environment
/// is satisfied when the pattern does *not* hold, matching `AllomorphEnvironment.IsWordValid`
/// (`_type == Exclude ? !IsMatch : IsMatch`, Allomorph.cs:83-86 / AllomorphEnvironment.cs).
#[test]
fn environments_ok_exclude_type_inverts_the_match() {
    let g = load_probe_grammar();
    // Excluded: "must NOT be followed by t".
    let envs = vec![env(false, None, Some(single(&g, "nc_t")))];

    let dat = probe_shape(&g, "dat"); // "a" (index 1) IS followed by t -- excluded pattern matches,
    assert!(!environments_ok(&g, &envs, &dat, 1, 1), "excluded env matched -> word invalid");

    let dad = probe_shape(&g, "dad"); // "a" (index 1) is followed by d, not t -- exclusion holds.
    assert!(environments_ok(&g, &envs, &dad, 1, 1), "excluded env did not match -> word valid");
}

/// No declared environments is vacuously valid (`Environments.Count > 0` guard, Allomorph.cs:112).
#[test]
fn environments_ok_vacuously_true_when_no_environments_declared() {
    let g = load_probe_grammar();
    let dat = probe_shape(&g, "dat");
    assert!(environments_ok(&g, &[], &dat, 1, 1));
}

/// W3.4 / history row `2469021f` ("Add unit test for multiple environments"): a morph carrying
/// TWO environment entries validates when **at least one** is satisfied — the commit flipped C#'s
/// `Allomorph.IsWordValid` environment clause from conjunctive (`Where(!valid).Any()` ⇒ all must
/// hold) to disjunctive (`!Environments.Any(valid)` ⇒ one suffices). Rust's `environments_ok` was
/// written on the post-fix OR side from day one, but until this test every `environments_ok_*`
/// case used a single-entry list (the left+right sides *within* one entry — a different, AND-ed
/// axis — are covered by `environments_ok_anchors_left_and_right_to_the_correct_sides`), so the
/// OR-across-entries semantics was genuinely unpinned: a regression to AND would have passed the
/// whole pre-existing suite.
#[test]
fn environments_ok_two_entries_use_or_semantics() {
    let g = load_probe_grammar();
    // Two REQUIRED environments on one allomorph: "preceded by d" OR "preceded by t".
    let envs = vec![
        env(true, Some(single(&g, "nc_d")), None),
        env(true, Some(single(&g, "nc_t")), None),
    ];

    // "dat", morph = the middle "a": preceded by d — entry 1 holds, entry 2 fails. Exactly one
    // satisfied ⇒ must PASS (an AND regression fails here).
    let dat = probe_shape(&g, "dat");
    assert!(environments_ok(&g, &envs, &dat, 1, 1), "one of two environments holds -> valid");

    // "tad": preceded by t — exactly the OTHER entry holds ⇒ must also pass (pins that the OR is
    // over the whole list, not a lucky first-entry short-circuit).
    let tad = probe_shape(&g, "tad");
    assert!(environments_ok(&g, &envs, &tad, 1, 1), "the other of two environments holds -> valid");

    // "aat": preceded by a (cons−, featurally disjoint from both d and t — n would NOT do here,
    // it is featurally identical to d in the probe grammar) — neither entry holds ⇒ must FAIL.
    let aat = probe_shape(&g, "aat");
    assert!(!environments_ok(&g, &envs, &aat, 1, 1), "neither environment holds -> invalid");
}

// =================================================================================================
// Part 2 -- `allomorphs_valid`: bound roots + required syntactic FS, over a real loaded grammar.
// =================================================================================================

/// A grammar with three lexical entries -- a plain `sg` root ("cat"), a plain `pl` root ("dog"), and
/// a **bound** `pl` root ("bat", `isBound="true"`) -- and one suffix rule (`-x`) whose allomorph
/// declares `RequiredHeadFeatures` `num=pl`. The `num` mismatch (cat=sg) and the bound-root
/// constraint are independent, orthogonal knobs: giving "bat" `num=pl` too isolates the bound-root
/// check from the required-FS check in the tests that follow.
const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>ValidityGate</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <HeadFeatures>
      <SymbolicFeature id="featNum">
        <Name>num</Name>
        <Symbols>
          <Symbol id="symSg">sg</Symbol>
          <Symbol id="symPl">pl</Symbol>
        </Symbols>
      </SymbolicFeature>
    </HeadFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cC"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cO"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cX"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll">
        <Name>All</Name>
        <Segment segment="cA" /><Segment segment="cB" /><Segment segment="cC" /><Segment segment="cD" />
        <Segment segment="cG" /><Segment segment="cO" /><Segment segment="cT" /><Segment segment="cX" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRules="mrSuffix">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrSuffix" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>-x</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subX">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
                <RequiredHeadFeatures>
                  <FeatureValue feature="featNum" symbolValues="symPl" />
                </RequiredHeadFeatures>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eCat" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aCat"><PhoneticShape>cat</PhoneticShape></Allomorph></Allomorphs>
            <AssignedHeadFeatures><FeatureValue feature="featNum" symbolValues="symSg" /></AssignedHeadFeatures>
          </LexicalEntry>
          <LexicalEntry id="eDog" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aDog"><PhoneticShape>dog</PhoneticShape></Allomorph></Allomorphs>
            <AssignedHeadFeatures><FeatureValue feature="featNum" symbolValues="symPl" /></AssignedHeadFeatures>
          </LexicalEntry>
          <LexicalEntry id="eBat" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aBat" isBound="true"><PhoneticShape>bat</PhoneticShape></Allomorph></Allomorphs>
            <AssignedHeadFeatures><FeatureValue feature="featNum" symbolValues="symPl" /></AssignedHeadFeatures>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn load_gate_grammar() -> Grammar {
    hc_grammar::load(XML).expect("validity-gate grammar loads")
}

fn entry_shape(g: &Grammar, text: &str) -> Shape {
    let t = &g.char_tables[0];
    let seg = hc_grammar::segment::segment(t, text).expect("segments");
    let mut b = ShapeBuilder::with_features_capacity(0, seg.len());
    for (_, kind, cd, _) in seg.interior() {
        match kind {
            NodeKind::Segment => b.push_segment_with_lanes(cd, &[]),
            NodeKind::Boundary => b.push_boundary_with_lanes(cd, &[]),
            _ => {}
        }
    }
    b.finish()
}

/// Find the lexical entry whose first allomorph's authored surface text is `text`.
fn find_entry<'g>(g: &'g Grammar, text: &str) -> &'g hc_grammar::model::LexEntryDef {
    g.entries
        .iter()
        .find(|e| e.allomorphs.first().map(|a| a.shape.text.as_str()) == Some(text))
        .unwrap_or_else(|| panic!("no entry with surface {text:?}"))
}

fn suffix_allomorph(g: &Grammar) -> (AllomorphId, hc_grammar::model::MorphemeId) {
    let MorphRuleDef::AffixProcess(def) = &g.mrules[0] else { panic!("expected affix rule") };
    (def.allomorphs[0].id, def.morpheme)
}

/// Bare bound root ("bat" alone, no affix): `distinct_count == 1` and `is_bound` -- must be
/// rejected (`RootAllomorph.CheckAllomorphConstraints`, RootAllomorph.cs:56-63).
#[test]
fn bound_root_alone_is_rejected() {
    let g = load_gate_grammar();
    let bat = find_entry(&g, "bat");
    let root_allo = bat.allomorphs[0].id;
    assert!(bat.allomorphs[0].is_bound, "sanity: eBat's allomorph is isBound=\"true\"");

    let mut w = Word::new(entry_shape(&g, "bat"), hc_grammar::model::StratumId(0));
    w.syn_fs = g.fs_interner.get(bat.syn_fs).clone();
    w.morphs = vec![MorphRecord::new(root_allo, bat.morpheme, 0)];

    assert!(!allomorphs_valid(&g, &w), "a bound root as the word's only allomorph must be rejected");
}

/// The same bound root combined with the suffix (`distinct_count == 2`) is no longer rejected by
/// the bound-root gate (the suffix also requires `num=pl`, which "bat"/eBat satisfies).
#[test]
fn bound_root_with_an_affix_is_not_rejected_by_the_bound_gate() {
    let g = load_gate_grammar();
    let bat = find_entry(&g, "bat");
    let root_allo = bat.allomorphs[0].id;
    let (affix_allo, affix_morpheme) = suffix_allomorph(&g);

    let mut w = Word::new(entry_shape(&g, "batx"), hc_grammar::model::StratumId(0));
    w.syn_fs = g.fs_interner.get(bat.syn_fs).clone();
    w.morphs = vec![
        MorphRecord::new(root_allo, bat.morpheme, 0),
        MorphRecord::new(affix_allo, affix_morpheme, 3),
    ];

    assert!(allomorphs_valid(&g, &w), "bound root + affix (distinct_count=2) must not be rejected");
}

/// `AffixProcessAllomorph.RequiredSyntacticFeatureStruct` is re-checked at final-validity time
/// against the word's *accumulated* syntactic FS (AffixProcessAllomorph.cs:87-105): the "-x" suffix
/// requires `num=pl`. "cat" (sg) + "-x" must be rejected; "dog" (pl) + "-x" must pass.
#[test]
fn required_syntactic_fs_gates_on_the_words_accumulated_syn_fs() {
    let g = load_gate_grammar();
    let (affix_allo, affix_morpheme) = suffix_allomorph(&g);

    let cat = find_entry(&g, "cat");
    let mut w_cat = Word::new(entry_shape(&g, "catx"), hc_grammar::model::StratumId(0));
    w_cat.syn_fs = g.fs_interner.get(cat.syn_fs).clone();
    w_cat.morphs = vec![
        MorphRecord::new(cat.allomorphs[0].id, cat.morpheme, 0),
        MorphRecord::new(affix_allo, affix_morpheme, 3),
    ];
    assert!(!allomorphs_valid(&g, &w_cat), "cat is num=sg; the suffix requires num=pl -- must reject");

    let dog = find_entry(&g, "dog");
    let mut w_dog = Word::new(entry_shape(&g, "dogx"), hc_grammar::model::StratumId(0));
    w_dog.syn_fs = g.fs_interner.get(dog.syn_fs).clone();
    w_dog.morphs = vec![
        MorphRecord::new(dog.allomorphs[0].id, dog.morpheme, 0),
        MorphRecord::new(affix_allo, affix_morpheme, 3),
    ];
    assert!(allomorphs_valid(&g, &w_dog), "dog is num=pl; the suffix's requirement is satisfied");
}
