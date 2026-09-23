use super::*;

fn realizer() -> TableRealizer {
    TableRealizer::new().unwrap_or_else(|e| panic!("embedded assets failed to load: {e}"))
}

fn ir(concept: Concept, case: CaseRole, poss: Poss, num: Num) -> GlossIr {
    GlossIr {
        concept,
        num,
        poss,
        case,
        extras: Vec::new(),
    }
}

// --- Load-time validation: the whole point of doing this in a test, not a runtime panic ---

#[test]
fn assets_load_and_cover_all_108_cells() {
    let r = realizer();
    assert_eq!(r.cells.len(), 108, "4 CaseRole x 9 Poss x 3 Num");
    for &case in &CaseRole::ALL {
        for &poss in &Poss::ALL {
            for &num in &Num::ALL {
                let template = r
                    .cells
                    .get(&(case, poss, num))
                    .unwrap_or_else(|| panic!("missing cell {case:?}.{poss:?}.{num:?}"));
                let has_sg = template.contains("{n:sg}");
                let has_pl = template.contains("{n:pl}");
                assert!(
                    has_sg != has_pl,
                    "{case:?}.{poss:?}.{num:?} template {template:?} must have exactly one slot"
                );
            }
        }
    }
}

#[test]
fn lexicon_exceptions_load() {
    let r = realizer();
    assert_eq!(
        r.plural_exceptions.get("child").map(String::as_str),
        Some("children")
    );
    assert_eq!(
        r.plural_exceptions.get("man").map(String::as_str),
        Some("men")
    );
    assert_eq!(
        r.plural_exceptions.get("sheep").map(String::as_str),
        Some("sheep")
    );
    assert_eq!(
        r.plural_exceptions.get("house"),
        None,
        "house is a regular plural"
    );
}

// --- The flagship assertion (task brief's own words) ---------------------------------------

#[test]
fn flagship_loc_p1sg_pl_house_renders_in_my_houses() {
    let r = realizer();
    let g = ir(
        Concept::Lex("house".to_string()),
        CaseRole::Loc,
        Poss::P1Sg,
        Num::Pl,
    );
    let out = r.realize(&g);
    assert_eq!(out.text, "in my houses");
    assert!(out.complete);
    assert!(out.residue.is_empty());
}

// --- Irregular plural via the exceptions table ----------------------------------------------

#[test]
fn irregular_child_becomes_his_children_via_p3sgm_pl() {
    let r = realizer();
    let g = ir(
        Concept::Lex("child".to_string()),
        CaseRole::None,
        Poss::P3SgM,
        Num::Pl,
    );
    let out = r.realize(&g);
    assert_eq!(out.text, "his children");
    assert!(out.complete);
}

// --- Regular -es/-ies rules ------------------------------------------------------------------

#[test]
fn regular_es_after_sibilant() {
    let r = realizer();
    for (word, expected_plural) in [
        ("box", "boxes"),
        ("buzz", "buzzes"),
        ("church", "churches"),
        ("bush", "bushes"),
    ] {
        let g = ir(
            Concept::Lex(word.to_string()),
            CaseRole::None,
            Poss::None,
            Num::Pl,
        );
        let out = r.realize(&g);
        assert_eq!(out.text, expected_plural, "{word}");
        assert!(out.complete, "{word}");
    }
}

#[test]
fn regular_ies_after_consonant_plus_y() {
    let r = realizer();
    let g = ir(
        Concept::Lex("city".to_string()),
        CaseRole::None,
        Poss::None,
        Num::Pl,
    );
    let out = r.realize(&g);
    assert_eq!(out.text, "cities");
    assert!(out.complete);
}

#[test]
fn vowel_plus_y_is_not_ies() {
    let r = realizer();
    let g = ir(
        Concept::Lex("toy".to_string()),
        CaseRole::None,
        Poss::None,
        Num::Pl,
    );
    let out = r.realize(&g);
    assert_eq!(out.text, "toys");
    assert!(out.complete);
}

// --- Multi-word / dotted glosses: only the final token is pluralized -----------------------

#[test]
fn multi_word_dotted_gloss_pluralizes_only_final_token() {
    let r = realizer();
    let g = ir(
        Concept::Lex("treat.someone".to_string()),
        CaseRole::None,
        Poss::None,
        Num::Sg,
    );
    let out = r.realize(&g);
    assert_eq!(out.text, "a treat someone");
    assert!(out.complete);

    let g_pl = ir(
        Concept::Lex("treat.someone".to_string()),
        CaseRole::None,
        Poss::None,
        Num::Pl,
    );
    let out_pl = r.realize(&g_pl);
    assert_eq!(
        out_pl.text, "treat someones",
        "only 'someone' inflects, not 'treat'"
    );
    assert!(out_pl.complete);
}

#[test]
fn already_spaced_multi_word_gloss_works_the_same_as_dotted() {
    let r = realizer();
    let g = ir(
        Concept::Lex("soar skyward".to_string()),
        CaseRole::None,
        Poss::None,
        Num::Pl,
    );
    let out = r.realize(&g);
    assert_eq!(out.text, "soar skywards");
    assert!(out.complete);
}

// --- Guessed concept: never inflected, always incomplete, template still filled --------------

#[test]
fn guessed_concept_uninflected_in_plural_slot_and_marked_incomplete() {
    let r = realizer();
    let g = ir(
        Concept::Guessed("mbal".to_string()),
        CaseRole::None,
        Poss::P1Sg,
        Num::Pl,
    );
    let out = r.realize(&g);
    assert_eq!(out.text, "my *mbal*", "uninflected -- NOT '*mbal*s'");
    assert!(!out.complete, "guessed concepts are always incomplete");
}

#[test]
fn guessed_concept_singular_cell_also_incomplete() {
    let r = realizer();
    let g = ir(
        Concept::Guessed("mbal".to_string()),
        CaseRole::None,
        Poss::None,
        Num::Sg,
    );
    let out = r.realize(&g);
    assert_eq!(out.text, "a *mbal*");
    assert!(!out.complete);
}

// --- Missing-plural incompleteness: a non-alphabetic final token can't be inflected ----------

#[test]
fn non_alphabetic_final_token_leaves_citation_form_unchanged_and_marks_incomplete() {
    let r = realizer();
    let g = ir(
        Concept::Lex("x1".to_string()),
        CaseRole::None,
        Poss::P3SgF,
        Num::Pl,
    );
    let out = r.realize(&g);
    assert_eq!(
        out.text, "her x1",
        "unpluralized citation form used verbatim in the slot"
    );
    assert!(!out.complete);
}

// --- Residue / partial flagging ---------------------------------------------------------------

#[test]
fn nonempty_residue_marks_incomplete_even_with_a_matched_cell_and_clean_plural() {
    let r = realizer();
    let mut g = ir(
        Concept::Lex("house".to_string()),
        CaseRole::Loc,
        Poss::P1Sg,
        Num::Pl,
    );
    g.extras = vec!["obj".to_string()];
    let out = r.realize(&g);
    assert_eq!(out.text, "in my houses", "text still renders normally");
    assert!(!out.complete, "nonempty residue forces incomplete");
    assert_eq!(out.residue, vec!["obj".to_string()]);
}

#[test]
fn residue_mirrors_ir_extras_verbatim_regardless_of_concept_kind() {
    let r = realizer();
    let mut g = ir(
        Concept::Guessed("mbal".to_string()),
        CaseRole::None,
        Poss::None,
        Num::Unspec,
    );
    g.extras = vec!["foo".to_string(), "bar".to_string()];
    let out = r.realize(&g);
    assert_eq!(out.residue, vec!["foo".to_string(), "bar".to_string()]);
}

// --- Sg vs Unspec both use the singular slot, with the Unspec/Sg article distinction ---------

#[test]
fn unspec_num_uses_singular_form_with_no_indefinite_article() {
    let r = realizer();
    let g = ir(
        Concept::Lex("house".to_string()),
        CaseRole::None,
        Poss::None,
        Num::Unspec,
    );
    let out = r.realize(&g);
    assert_eq!(out.text, "house");
    assert!(out.complete);
}

#[test]
fn sg_num_gets_indefinite_article_when_unpossessed() {
    let r = realizer();
    let g = ir(
        Concept::Lex("house".to_string()),
        CaseRole::None,
        Poss::None,
        Num::Sg,
    );
    let out = r.realize(&g);
    assert_eq!(out.text, "a house");
    assert!(out.complete);
}
