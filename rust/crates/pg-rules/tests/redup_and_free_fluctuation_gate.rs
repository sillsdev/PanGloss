//! Hand-built regression gate for reduplication morph attribution and the free-fluctuation allomorph break: neither fix's effect surfaces in the reference-grammar corpora, so these pin the mechanism directly against a controlled shape.

mod common;

use common::load_alpha_grammar;
use pg_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AllomorphId, EnvironmentDef, Grammar, MorphRuleDef,
    MorphemeId, MprSet, OutputAction, PartRef, Pattern, PatternNode, ReduplicationHint,
    SegmentedText, SimpleContext, StratumId, TableId, VarTable,
};
use pg_rules::morph::synthesize;
use pg_rules::{MorphRecord, Word};
use pg_shape::{NodeKind, Shape, ShapeBuilder};

// Shared builders duplicated from `morph_gate.rs`: integration test binaries can't share private helpers across files without growing `common`.

fn shape_with_lanes(g: &Grammar, text: &str) -> Shape {
    let t = &g.char_tables[0];
    let seg = pg_grammar::segment::segment(t, text).expect("segments");
    let w = g.phon_features.len() as u32;
    let mut b = ShapeBuilder::with_features_capacity(w, seg.len());
    for (_, kind, cd, _) in seg.interior() {
        let mut lanes = vec![u64::MAX; w as usize];
        for (i, &l) in t
            .get(pg_grammar::chardef::CharDefId(cd))
            .feature_lanes()
            .iter()
            .enumerate()
        {
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

fn root_word(g: &Grammar, text: &str, morpheme: u32) -> Word {
    let mut w = Word::new(shape_with_lanes(g, text), StratumId(0));
    w.morphs.push(MorphRecord::new(
        AllomorphId(morpheme),
        MorphemeId(morpheme),
        0,
    ));
    w
}

fn ctx(nc: &str, g: &Grammar) -> SimpleContext {
    common::ctx(common::nat_class(g, nc))
}

fn one_or_more(nc: &str, g: &Grammar) -> Pattern {
    Pattern {
        nodes: vec![PatternNode::Quantifier {
            min: 1,
            max: None,
            children: vec![PatternNode::Context(ctx(nc, g))],
        }],
    }
}

fn single(nc: &str, g: &Grammar) -> Pattern {
    Pattern {
        nodes: vec![PatternNode::Context(ctx(nc, g))],
    }
}

fn insert_segments(g: &Grammar, text: &str) -> OutputAction {
    let shape = pg_grammar::segment::segment(&g.char_tables[0], text).expect("segments");
    OutputAction::InsertSegments {
        table: TableId(0),
        shape: SegmentedText {
            text: text.to_string(),
            shape,
        },
    }
}

fn allomorph(
    id: u32,
    lhs: Vec<Pattern>,
    rhs: Vec<OutputAction>,
    redup_hint: ReduplicationHint,
) -> AffixAllomorphDef {
    AffixAllomorphDef {
        id: AllomorphId(id),
        environments: vec![],
        co_occurrence: vec![],
        required_syn_fs: pg_featstruct::FsId(0),
        vars: VarTable::default(),
        required_mpr: MprSet::EMPTY,
        excluded_mpr: MprSet::EMPTY,
        out_mpr: MprSet::EMPTY,
        redup_hint,
        lhs,
        rhs,
        properties: vec![],
    }
}

fn affix_rule(morpheme: u32, allomorphs: Vec<AffixAllomorphDef>) -> MorphRuleDef {
    MorphRuleDef::AffixProcess(AffixProcessRuleDef {
        morpheme: MorphemeId(morpheme),
        name: None,
        blockable: false,
        partial: false,
        max_apps: 1,
        required_syn_fs: pg_featstruct::FsId(0),
        out_syn_fs: pg_featstruct::FsId(0),
        obligatory_features: vec![],
        required_stem_name: None,
        is_template_rule: false,
        allomorphs,
    })
}

// Reduplication morph attribution: a repeated `CopyFromInput` of the same LHS part is not uniformly "existing input material"; exactly one occurrence (selected by `ReduplicationHint`) stays attributed to the existing morph, the rest become new material on the affix's morpheme.

/// Mirrors Indonesian `msubrule13` ("REDUP-meN", `redupMorphType="prefix"`): the new leading stem echo must be attributed to the affix, not folded into the root's existing morph.
#[test]
fn prefix_hint_reduplication_attributes_the_new_leading_copy_to_the_affix_not_the_root() {
    let g = load_alpha_grammar();
    // Input: one existing morph (id 100) spanning "kapa" ("k" = attached material, "apa" = stem).
    let input = root_word(&g, "kapa", 100);

    let attached_part = single("nc_cons", &g); // matches "k"
    let stem_part = one_or_more("nc_any", &g); // matches "apa"
    let rule = affix_rule(
        200,
        vec![allomorph(
            200,
            vec![attached_part, stem_part],
            vec![
                OutputAction::Copy(PartRef::Input(1)), // stem echo #1 (new / leading)
                insert_segments(&g, "n"),              // filler glue (always new)
                OutputAction::Copy(PartRef::Input(0)), // attached-material echo (singleton/existing)
                OutputAction::Copy(PartRef::Input(1)), // stem echo #2 (base / existing)
            ],
            ReduplicationHint::Prefix,
        )],
    );

    let out = synthesize(&g, &input, &rule);
    assert_eq!(out.len(), 1, "one synthesis output");
    let w = &out[0];

    // Exactly 2 morphs: the pre-existing root (100) and the new redup rule's own morpheme (200).
    let mut morphs = w.morphs.clone();
    morphs.sort_by_key(|m| m.order);
    let seq: Vec<u32> = morphs.iter().map(|m| m.morpheme.0).collect();
    assert_eq!(
        seq,
        vec![200, 100],
        "the new (leading) stem echo must sort the redup morpheme FIRST, ahead of the root — \
         mirroring C#'s own `order`-by-leftmost-position semantics once the leading copy is \
         correctly tagged as new material; got {seq:?} (order values: {:?})",
        morphs.iter().map(|m| m.order).collect::<Vec<_>>()
    );
    // The root's own order must be strictly after the redup morpheme's.
    assert!(
        morphs[0].order < morphs[1].order,
        "redup morpheme (order {}) must precede the root (order {})",
        morphs[0].order,
        morphs[1].order
    );
}

/// Mirrors Indonesian `msubrule5`/`msubrule11` ("-Cont"/"-Pl", `redupMorphType="suffix"`): the FIRST occurrence stays existing (root), the second becomes new (affix) — the opposite selection from the `Prefix` case above.
#[test]
fn suffix_hint_reduplication_keeps_the_first_copy_as_the_root() {
    let g = load_alpha_grammar();
    let input = root_word(&g, "apa", 100);
    let stem_part = one_or_more("nc_any", &g);
    let rule = affix_rule(
        200,
        vec![allomorph(
            200,
            vec![stem_part],
            vec![
                OutputAction::Copy(PartRef::Input(0)), // base (existing)
                insert_segments(&g, "n"),              // glue (new)
                OutputAction::Copy(PartRef::Input(0)), // redup echo (new)
            ],
            ReduplicationHint::Suffix,
        )],
    );

    let out = synthesize(&g, &input, &rule);
    assert_eq!(out.len(), 1);
    let mut morphs = out[0].morphs.clone();
    morphs.sort_by_key(|m| m.order);
    let seq: Vec<u32> = morphs.iter().map(|m| m.morpheme.0).collect();
    // Both the root and the new affix morpheme are present, count stays exactly 2, and root sorts first.
    assert_eq!(
        seq,
        vec![100, 200],
        "root first, redup rule's own morpheme second; got {seq:?}"
    );
}

// Free-fluctuation allomorph break: two allomorphs with identical LHS/environments/MPR-sets/required-syntactic-FS are in free variation, so synthesis must not stop at the first -- both must surface.

#[test]
fn constraint_equal_adjacent_allomorphs_both_synthesize() {
    let g = load_alpha_grammar();
    let stem = root_word(&g, "apa", 100);
    // Two allomorphs, same LHS/environments/MPR-sets (literally constraint-equal), different RHS suffixes so their outputs are distinguishable.
    let rule = affix_rule(
        200,
        vec![
            allomorph(
                200,
                vec![one_or_more("nc_any", &g)],
                vec![
                    OutputAction::Copy(PartRef::Input(0)),
                    insert_segments(&g, "n"),
                ],
                ReduplicationHint::Implicit,
            ),
            allomorph(
                201,
                vec![one_or_more("nc_any", &g)],
                vec![
                    OutputAction::Copy(PartRef::Input(0)),
                    insert_segments(&g, "g"),
                ],
                ReduplicationHint::Implicit,
            ),
        ],
    );

    let out = synthesize(&g, &stem, &rule);
    let suffixes: Vec<char> = out
        .iter()
        .map(|w| {
            let last = w.shape.interior().last().unwrap();
            let cd = pg_grammar::chardef::CharDefId(last.2);
            g.char_tables[0].get(cd).representations()[0]
                .chars()
                .next()
                .unwrap()
        })
        .collect();
    assert_eq!(
        out.len(),
        2,
        "both constraint-equal (free-fluctuating) allomorphs must synthesize their own word, \
         got {} output(s) with suffixes {suffixes:?}",
        out.len()
    );
    assert!(
        suffixes.contains(&'n') && suffixes.contains(&'g'),
        "got suffixes {suffixes:?}"
    );
}

#[test]
fn constraint_unequal_adjacent_allomorphs_still_break_after_the_first() {
    let g = load_alpha_grammar();
    let stem = root_word(&g, "apa", 100);
    // Same shape as above, but the second allomorph has a non-empty environment set, so it is not constraint-equal to the first: only the first (unconstrained) allomorph's word is produced.
    let rule = affix_rule(
        200,
        vec![
            allomorph(
                200,
                vec![one_or_more("nc_any", &g)],
                vec![
                    OutputAction::Copy(PartRef::Input(0)),
                    insert_segments(&g, "n"),
                ],
                ReduplicationHint::Implicit,
            ),
            {
                let mut a = allomorph(
                    201,
                    vec![one_or_more("nc_any", &g)],
                    vec![
                        OutputAction::Copy(PartRef::Input(0)),
                        insert_segments(&g, "g"),
                    ],
                    ReduplicationHint::Implicit,
                );
                a.environments = vec![EnvironmentDef {
                    require: true,
                    left: None,
                    right: None,
                }];
                a
            },
        ],
    );

    let out = synthesize(&g, &stem, &rule);
    assert_eq!(
        out.len(),
        1,
        "allomorphs whose constraints differ (here: environment sets) are NOT free-fluctuating — \
         C#'s original break-after-first must still fire, got {} output(s)",
        out.len()
    );
}
