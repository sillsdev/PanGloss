//! Tests the representation-alias map and token-rendering contract.
use super::*;

fn two_table_shared_repr_grammar() -> Grammar {
    // Table A: "x" at index 0. Table B: "z"(0, decoy - misaligned index), "x"(1, shared), "y"(2).
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>AliasMapUnitProbe</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t0">
      <Name>TableA</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c0x"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <CharacterDefinitionTable id="t1">
      <Name>TableB</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1z"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c1x"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c1y"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
  </Language>
</HermitCrabInput>
"#;
    pg_grammar::load(XML).unwrap_or_else(|e| panic!("alias-map unit probe failed to load: {e}"))
}

/// "x" (shared) maps to both tables' pairs; "z"/"y" (unique to table B) map to their own single pair.
#[test]
fn build_maps_shared_representation_to_every_owning_table_and_cd() {
    let g = two_table_shared_repr_grammar();
    let map = RepresentationAliasMap::build(&g);

    let table_a = &g.char_tables[0];
    let table_b = &g.char_tables[1];
    let cd_a_x = table_a.lookup_nfd("x").expect("table A declares x");
    let cd_b_z = table_b.lookup_nfd("z").expect("table B declares z");
    let cd_b_x = table_b.lookup_nfd("x").expect("table B declares x");
    let cd_b_y = table_b.lookup_nfd("y").expect("table B declares y");
    // Sanity: "x" sits at different raw indices in the two tables -- why this fix is needed.
    assert_ne!(
        cd_a_x.0, cd_b_x.0,
        "the fixture's own misalignment must hold"
    );

    let aliases_x = map.aliases_for(table_b, TableId(1), cd_b_x);
    assert_eq!(
        aliases_x.len(),
        2,
        "\"x\" is shared by both tables -- aliases_for must return BOTH pairs: {aliases_x:?}"
    );
    assert!(aliases_x.contains(&(TableId(0), cd_a_x)));
    assert!(aliases_x.contains(&(TableId(1), cd_b_x)));

    // "z"/"y" are unique to table B: aliases_for degenerates to the singleton.
    assert_eq!(
        map.aliases_for(table_b, TableId(1), cd_b_z),
        vec![(TableId(1), cd_b_z)]
    );
    assert_eq!(
        map.aliases_for(table_b, TableId(1), cd_b_y),
        vec![(TableId(1), cd_b_y)]
    );
}

/// `SegAlphabet::new` never aliases, regardless of shared representations -- the byte-identical-to-today behavior existing parity tests depend on.
#[test]
fn render_tokens_never_aliases_without_a_table_id() {
    let g = two_table_shared_repr_grammar();
    let table_b = &g.char_tables[1];
    let cd_b_x = table_b.lookup_nfd("x").unwrap();
    let alphabet = SegAlphabet::new(table_b);
    assert_eq!(alphabet.render_tokens(cd_b_x), vec![alphabet.token(cd_b_x)]);
}

/// `with_table_id` renders the shared "x" atom as the union of both tables' tokens (deduplicated); unshared atoms degenerate to the single-token case.
#[test]
fn render_tokens_aliases_the_shared_atom_and_degenerates_for_the_unshared_ones() {
    let g = two_table_shared_repr_grammar();
    let table_a = &g.char_tables[0];
    let table_b = &g.char_tables[1];
    let cd_a_x = table_a.lookup_nfd("x").unwrap();
    let cd_b_z = table_b.lookup_nfd("z").unwrap();
    let cd_b_x = table_b.lookup_nfd("x").unwrap();
    let cd_b_y = table_b.lookup_nfd("y").unwrap();
    let map = RepresentationAliasMap::build(&g);
    let alphabet_b = SegAlphabet::with_table_id(table_b, TableId(1), &map);

    let alphabet_a_bare = SegAlphabet::new(table_a);
    let mut tokens_x = alphabet_b.render_tokens(cd_b_x);
    tokens_x.sort_unstable();
    let mut expected_x = vec![alphabet_b.token(cd_b_x), alphabet_a_bare.token(cd_a_x)];
    expected_x.sort_unstable();
    assert_eq!(
        tokens_x, expected_x,
        "the shared \"x\" atom must render as the union of BOTH tables' own tokens"
    );
    assert_eq!(
        tokens_x.len(),
        2,
        "must actually be two DISTINCT tokens, not a collapsed one"
    );

    assert_eq!(
        alphabet_b.render_tokens(cd_b_z),
        vec![alphabet_b.token(cd_b_z)]
    );
    assert_eq!(
        alphabet_b.render_tokens(cd_b_y),
        vec![alphabet_b.token(cd_b_y)]
    );
}

/// `encode_shape`/`encode_query` never alias, regardless of construction path -- a query word must stay single-token.
#[test]
fn encode_query_is_unaffected_by_table_id_aliasing() {
    let g = two_table_shared_repr_grammar();
    let table_b = &g.char_tables[1];
    let map = RepresentationAliasMap::build(&g);
    let bare = SegAlphabet::new(table_b);
    let aliased = SegAlphabet::with_table_id(table_b, TableId(1), &map);

    let bare_query = bare
        .encode_query("x")
        .expect("\"x\" must segment against table B");
    let aliased_query = aliased
        .encode_query("x")
        .expect("\"x\" must segment against table B");
    assert_eq!(
        bare_query, aliased_query,
        "encode_query must be byte-identical whether or not the alphabet carries a TableId"
    );
    assert_eq!(
        bare_query.chars().count(),
        1,
        "a single-segment query must encode to exactly one token, never a bracketed union"
    );
}
