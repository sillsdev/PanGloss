use super::*;
use foma::apply::{apply_down, apply_init};

fn truncate_morphotactic_grammar() -> Grammar {
    let fixture = pg_conformance_fixtures::discover()
        .into_iter()
        .find(|f| {
            f.root == pg_conformance_fixtures::Root::Machine
                && f.category == "edge-cases"
                && f.name == "truncate-morphotactic"
        })
        .expect("machine:edge-cases/truncate-morphotactic must be discoverable");
    pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture grammar must load")
}

fn affix_process(g: &Grammar, index: usize) -> &AffixAllomorphDef {
    match &g.mrules[index] {
        MorphRuleDef::AffixProcess(def) => &def.allomorphs[0],
        other => panic!("mrule[{index}] must be affix-process, got {other:?}"),
    }
}

/// `[InsertSegments, Copy(Input(1))]` over an optional leading part -- the shape `recipe_for` used to miss.
#[test]
fn recipe_for_matches_insert_then_leading_drop() {
    let g = truncate_morphotactic_grammar();
    let allomorph = affix_process(&g, 2);
    assert_eq!(
        crate::emit::classify_affix(&allomorph.rhs),
        crate::emit::Role::Prefix
    );
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let recipe = recipe_for(&g, allomorph, table)
        .expect("insert-then-leading-drop shape must be recognized");
    assert!(recipe.leading);
    assert_eq!(
        recipe.inserted,
        alphabet.encode_query("g").expect("'g' must tokenize")
    );
    assert_eq!(recipe.tail_members.len(), 1);
}

/// Control: the two shapes `recipe_for` already matched before this fix must still match.
#[test]
fn recipe_for_still_matches_the_pre_existing_shapes() {
    let g = truncate_morphotactic_grammar();
    let table = &g.char_tables[0];
    let trail = recipe_for(&g, affix_process(&g, 0), table).expect("trailing-drop recipe");
    assert!(!trail.leading);
    assert!(trail.inserted.is_empty());
    let lead = recipe_for(&g, affix_process(&g, 1), table).expect("leading-drop recipe");
    assert!(lead.leading);
    assert!(lead.inserted.is_empty());
}

/// The deletion must be anchored to the marker, not `.#.`, once material (`ba`) precedes it.
#[test]
fn compile_layer_leading_insert_drop_anchors_to_marker_not_word_edge() {
    let g = truncate_morphotactic_grammar();
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let net = compile_layer(&opts, &g, &alphabet).expect("structural layer must compile");
    let allomorph = affix_process(&g, 2);
    // The real call site's `prefix_zone` for this rule's one reachable call is `false` (`role != None`).
    let marker = structural_marker_for_zone(&g, allomorph, table, false)
        .expect("a Role::Prefix-only rule's marker must not be suppressed");

    let encode = |word: &str| alphabet.encode_query(word).expect("word must tokenize");

    let mut handle = apply_init(&net);
    assert_eq!(
        apply_down(&mut handle, Some(&format!("{marker}{}", encode("sas")))),
        Some(encode("gas")),
        "marker + leading-tail root must truncate and insert, matching the direct oracle analysis"
    );
    let mut handle = apply_init(&net);
    assert_eq!(
        apply_down(
            &mut handle,
            Some(&format!("{}{marker}{}", encode("ba"), encode("sas")))
        ),
        Some(encode("bagas")),
        "material preceding the marker must survive untouched -- proves the deletion is \
         anchored to the marker, not to `.#.`"
    );
}
