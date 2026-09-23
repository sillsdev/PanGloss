use super::*;
use crate::GlossToken;

fn tok(gloss: Option<&str>, is_root: bool) -> GlossToken {
    GlossToken {
        gloss: gloss.map(str::to_string),
        properties: Vec::new(),
        is_root,
    }
}

fn tok_with_property(gloss: Option<&str>, prop_value: &str) -> GlossToken {
    GlossToken {
        gloss: gloss.map(str::to_string),
        properties: vec![("realize".to_string(), prop_value.to_string())],
        is_root: false,
    }
}

fn bundle(tokens: Vec<GlossToken>, root_index: Option<usize>, guessed: bool) -> GlossBundle {
    GlossBundle {
        tokens,
        root_index,
        pos_id: None,
        guessed,
    }
}

fn sidecar() -> RealizeMap {
    RealizeMap::parse(
        "[features]\n\"pl\" = \"Num:Pl\"\n\"poss.1s\" = \"Poss:P1Sg\"\n\"at\" = \"Case:Loc\"\n",
    )
    .expect("valid sidecar")
}

#[test]
fn property_takes_priority_over_sidecar() {
    // gloss "pl" is in the sidecar as Num:Pl, but the property says Num:Sg -- property wins.
    let t = GlossToken {
        gloss: Some("pl".to_string()),
        properties: vec![("realize".to_string(), "Num:Sg".to_string())],
        is_root: false,
    };
    let b = bundle(vec![tok(Some("house"), true), t], Some(0), false);
    let ir = to_ir(&b, &sidecar(), "house");
    assert_eq!(ir.num, Num::Sg);
}

#[test]
fn malformed_property_degrades_to_sidecar_lookup() {
    let t = GlossToken {
        gloss: Some("pl".to_string()),
        properties: vec![("realize".to_string(), "not valid".to_string())],
        is_root: false,
    };
    let b = bundle(vec![tok(Some("house"), true), t], Some(0), false);
    let ir = to_ir(&b, &sidecar(), "house");
    assert_eq!(
        ir.num,
        Num::Pl,
        "bad property should fall through to sidecar"
    );
}

#[test]
fn conflict_first_wins_rest_to_extras() {
    let t1 = tok_with_property(Some("pl"), "Num:Pl");
    let t2 = tok_with_property(Some("dup"), "Num:Sg");
    let b = bundle(vec![tok(Some("house"), true), t1, t2], Some(0), false);
    let ir = to_ir(&b, &RealizeMap::empty(), "house");
    assert_eq!(ir.num, Num::Pl, "first assignment wins");
    assert_eq!(ir.extras, vec!["dup".to_string()]);
}

#[test]
fn ignore_drops_token_from_extras() {
    let t = tok_with_property(Some("epenthetic"), "Ignore");
    let b = bundle(vec![tok(Some("house"), true), t], Some(0), false);
    let ir = to_ir(&b, &RealizeMap::empty(), "house");
    assert!(ir.extras.is_empty());
}

#[test]
fn unmapped_token_goes_to_extras_verbatim() {
    let t = tok(Some("obj"), false);
    let b = bundle(vec![tok(Some("house"), true), t], Some(0), false);
    let ir = to_ir(&b, &RealizeMap::empty(), "house");
    assert_eq!(ir.extras, vec!["obj".to_string()]);
}

#[test]
fn unmapped_ungossed_token_goes_to_extras_as_bracket_question() {
    let t = tok(None, false);
    let b = bundle(vec![tok(Some("house"), true), t], Some(0), false);
    let ir = to_ir(&b, &RealizeMap::empty(), "house");
    assert_eq!(ir.extras, vec!["[?]".to_string()]);
}

#[test]
fn glossed_root_becomes_lex_concept() {
    let b = bundle(vec![tok(Some("house"), true)], Some(0), false);
    let ir = to_ir(&b, &RealizeMap::empty(), "house");
    assert_eq!(ir.concept, Concept::Lex("house".to_string()));
}

#[test]
fn unglossed_real_root_becomes_bracket_question_lex_concept() {
    let b = bundle(vec![tok(None, true)], Some(0), false);
    let ir = to_ir(&b, &RealizeMap::empty(), "mystery");
    assert_eq!(ir.concept, Concept::Lex("[?]".to_string()));
}

#[test]
fn guessed_root_becomes_guessed_concept_with_surface_word() {
    let b = bundle(vec![tok(None, true)], Some(0), true);
    let ir = to_ir(&b, &RealizeMap::empty(), "gag");
    assert_eq!(ir.concept, Concept::Guessed("gag".to_string()));
}

#[test]
fn no_root_index_yields_bracket_question_and_all_tokens_through_chain() {
    let t = tok_with_property(Some("pl"), "Num:Pl");
    let b = bundle(vec![tok(Some("house"), false), t], None, false);
    let ir = to_ir(&b, &RealizeMap::empty(), "house");
    assert_eq!(ir.concept, Concept::Lex("[?]".to_string()));
    // "house" itself, with no root to consume it, is unmapped -> extras.
    assert_eq!(ir.extras, vec!["house".to_string()]);
    assert_eq!(ir.num, Num::Pl);
}

#[test]
fn empty_bundle_never_panics() {
    let b = bundle(vec![], None, false);
    let ir = to_ir(&b, &RealizeMap::empty(), "");
    assert_eq!(ir.concept, Concept::Lex("[?]".to_string()));
    assert_eq!(ir.num, Num::Unspec);
    assert_eq!(ir.poss, Poss::None);
    assert_eq!(ir.case, CaseRole::None);
    assert!(ir.extras.is_empty());
}

#[test]
fn full_priority_and_multiple_features_together() {
    // root "house" + Num:Pl (property) + Poss:P1Sg (sidecar) + Case:Loc (sidecar) + one unmapped extra token ("obj").
    let root = tok(Some("house"), true);
    let pl = tok_with_property(Some("pl-ish"), "Num:Pl");
    let poss = tok(Some("poss.1s"), false);
    let case = tok(Some("at"), false);
    let extra = tok(Some("obj"), false);
    let b = bundle(vec![root, pl, poss, case, extra], Some(0), false);
    let ir = to_ir(&b, &sidecar(), "house");
    assert_eq!(ir.concept, Concept::Lex("house".to_string()));
    assert_eq!(ir.num, Num::Pl);
    assert_eq!(ir.poss, Poss::P1Sg);
    assert_eq!(ir.case, CaseRole::Loc);
    assert_eq!(ir.extras, vec!["obj".to_string()]);
}
