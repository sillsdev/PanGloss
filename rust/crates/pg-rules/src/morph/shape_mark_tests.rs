//! Morph order after a rewrite deletes or inserts nodes and a later affix rule rebuilds the shape; the keys passed to `remap_morph_orders` are exactly what `MutShape::anchor_keys` would give.

use super::*;
use pg_grammar_model::model::{AllomorphId, MorphemeId};

fn load_grammar(name: &str) -> Grammar {
    pg_conformance_fixtures::require_fixture("edge-cases", name).load_grammar()
}

fn null_prefix_grammar() -> Grammar {
    load_grammar("null-prefix-root-initial-deletion-later-suffix")
}

fn epenthesis_grammar() -> Grammar {
    load_grammar("epenthesis-beside-prefix-then-prefix-deletion")
}

const ROOT: u32 = 100;

fn shape(g: &Grammar, text: &str) -> Shape {
    crate::shape_feat::segment_with_features(g, &g.char_tables[0], text).unwrap()
}

fn root_word(g: &Grammar, text: &str) -> Word {
    let mut w = Word::new(shape(g, text), StratumId(0));
    w.morphs
        .push(MorphRecord::new(AllomorphId(ROOT), MorphemeId(ROOT), 0));
    w
}

/// The grammar's two affix rules: the one that inserts before its copy (a prefix) and the one that copies first (a suffix).
fn rules(g: &Grammar) -> (&MorphRuleDef, &MorphRuleDef) {
    let mut prefix = None;
    let mut suffix = None;
    for rule in &g.mrules {
        if let MorphRuleDef::AffixProcess(def) = rule {
            let first = &def.allomorphs[0].rhs[0];
            if matches!(first, OutputAction::InsertSegments { .. }) {
                prefix = Some(rule);
            } else {
                suffix = Some(rule);
            }
        }
    }
    (prefix.unwrap(), suffix.unwrap())
}

fn morpheme_of(rule: &MorphRuleDef) -> u32 {
    match rule {
        MorphRuleDef::AffixProcess(def) => def.morpheme.0,
        _ => unreachable!(),
    }
}

fn records(word: &Word) -> Vec<(u32, MorphStatus)> {
    word.morphs
        .iter()
        .map(|m| (m.morpheme.0, m.status))
        .collect()
}

/// The key list of a rewrite that kept every old node in `keep` and inserted nodes after the old node before each `(after)` entry.
fn keys(old_interior: u32, deleted: &[u32], inserted_after: &[u32]) -> Vec<u32> {
    let mut keys = Vec::new();
    for old in 0..old_interior {
        if !deleted.contains(&old) {
            keys.push(2 * (old + 1));
        }
        for _ in inserted_after.iter().filter(|&&after| after == old) {
            keys.push((2 * (old + 1)) | 1);
        }
    }
    keys
}

/// `+` prefix then `root`, with its first node deleted, so the shape reads `+ps`.
fn deleted_root_initial(text: &str, deleted: u32) -> (Grammar, Word) {
    let g = null_prefix_grammar();
    let (prefix, _) = rules(&g);
    let mut word = synthesize(&g, &root_word(&g, text), prefix).remove(0);
    let rest = synthesize(&g, &root_word(&g, "ps"), prefix).remove(0);
    let old_len = word.shape.len();
    word.remap_morph_orders(old_len, &keys(4, &[deleted], &[]));
    word.shape = rest.shape;
    (g, word)
}

#[test]
fn a_deleted_node_before_the_group_blocks_the_fold_and_the_prefix_is_subsumed() {
    let (g, word) = deleted_root_initial("tps", 1);
    assert_eq!(word.extras.deleted_gaps(), &[1]);
    let out = synthesize(&g, &word, rules(&g).1).remove(0);
    assert_eq!(
        records(&out),
        vec![
            (ROOT, MorphStatus::Real),
            (morpheme_of(rules(&g).0), MorphStatus::SubsumedChild),
            (morpheme_of(rules(&g).1), MorphStatus::Real),
        ]
    );
    assert!(out.extras.deleted_gaps().is_empty());
}

#[test]
fn a_deleted_node_inside_the_root_still_lets_the_fold_happen() {
    let (g, word) = deleted_root_initial("pts", 2);
    assert_eq!(word.extras.deleted_gaps(), &[2]);
    let out = synthesize(&g, &word, rules(&g).1).remove(0);
    assert_eq!(
        records(&out),
        vec![
            (morpheme_of(rules(&g).0), MorphStatus::Real),
            (ROOT, MorphStatus::Real),
            (morpheme_of(rules(&g).1), MorphStatus::Real),
        ],
        "the root's own deleted node lies after the prefix, so the prefix's boundary is still copied"
    );
}

#[test]
fn a_deleted_gap_follows_the_nodes_through_a_later_rewrite() {
    let g = null_prefix_grammar();
    let mut word = root_word(&g, "tps");
    word.remap_morph_orders(word.shape.len(), &keys(3, &[0], &[]));
    assert_eq!(word.extras.deleted_gaps(), &[0]);
    word.shape = shape(&g, "ps");
    word.remap_morph_orders(word.shape.len(), &[1, 2, 4]);
    assert_eq!(word.extras.deleted_gaps(), &[1]);
}

#[test]
fn an_optional_deleted_node_leaves_no_gap() {
    let g = null_prefix_grammar();
    let (prefix, _) = rules(&g);
    let mut word = synthesize(&g, &root_word(&g, "ps"), prefix).remove(0);
    word.remap_morph_orders(word.shape.len(), &keys(3, &[0], &[]));
    assert!(word.extras.deleted_gaps().is_empty());
}

/// `PR0` (inserts `t`) then root `sa`; an epenthetic `a` after `t`, then `t` deleted, then the shape reads `asa`.
fn epenthesis_then_prefix_deletion() -> (Grammar, Word) {
    let g = epenthesis_grammar();
    let (prefix, _) = rules(&g);
    let mut word = synthesize(&g, &root_word(&g, "sa"), prefix).remove(0);
    let inserted = synthesize(&g, &root_word(&g, "asa"), prefix).remove(0);
    word.remap_morph_orders(word.shape.len(), &keys(3, &[], &[0]));
    word.shape = inserted.shape;
    let old_len = word.shape.len();
    word.remap_morph_orders(old_len, &keys(4, &[0], &[]));
    word.shape = shape(&g, "asa");
    (g, word)
}

#[test]
fn an_epenthetic_node_at_a_morphs_edge_does_not_keep_the_morph() {
    let (g, word) = epenthesis_then_prefix_deletion();
    let prefix = morpheme_of(rules(&g).0);
    assert_eq!(
        records(&word),
        vec![(prefix, MorphStatus::Floating), (ROOT, MorphStatus::Real)],
        "asa with no suffix: the emptied prefix stays a singleton before the root"
    );
    let out = synthesize(&g, &word, rules(&g).1).remove(0);
    assert_eq!(
        records(&out),
        vec![
            (ROOT, MorphStatus::Real),
            (prefix, MorphStatus::SubsumedChild),
            (morpheme_of(rules(&g).1), MorphStatus::Real),
        ]
    );
}

#[test]
fn an_epenthetic_node_strictly_inside_a_morph_keeps_it() {
    let g = epenthesis_grammar();
    let mut word = root_word(&g, "sasa");
    word.remap_morph_orders(word.shape.len(), &keys(4, &[], &[1]));
    assert!(word.extras.bare_nodes().is_empty());
    let old_len = word.shape.len();
    word.remap_morph_orders(old_len, &keys(5, &[0, 1, 3, 4], &[]));
    assert_eq!(
        records(&word),
        vec![(ROOT, MorphStatus::Real)],
        "the interior insertion belongs to the root, so deleting its neighbours does not empty it"
    );
}

#[test]
fn an_insertion_at_the_edge_is_bare_and_follows_the_node_through_later_rewrites() {
    let g = epenthesis_grammar();
    let mut word = root_word(&g, "sa");
    word.remap_morph_orders(word.shape.len(), &keys(2, &[], &[1]));
    assert_eq!(word.extras.bare_nodes(), &[2]);
    word.shape = shape(&g, "saa");
    word.remap_morph_orders(word.shape.len(), &[1, 2, 4, 6]);
    assert_eq!(word.extras.bare_nodes(), &[0, 3]);
}

#[test]
fn rebuilding_the_shape_forgets_gaps_and_pinned_starts_but_keeps_bare_nodes_it_copies() {
    let (g, mut word) = deleted_root_initial("tps", 1);
    word.extras.pin_start((AllomorphId(ROOT), 0, 0));
    word.extras.set_shape_marks(vec![1], vec![2]);
    let out = synthesize(&g, &word, rules(&g).1).remove(0);
    assert!(out.extras.pinned_starts().is_empty());
    assert!(out.extras.deleted_gaps().is_empty());
    // The root's last node is copied, and the suffix's one segment follows it.
    assert_eq!(out.extras.bare_nodes(), &[out.shape.len() as u32 - 4]);
}

fn decoded_ranges(bare: &[u32]) -> Vec<(u32, u32)> {
    let morphs = vec![
        MorphRecord::new(AllomorphId(0), MorphemeId(0), 0),
        MorphRecord::new(AllomorphId(1), MorphemeId(1), 3),
    ];
    let mut extras = crate::extras::Extras::default();
    extras.set_shape_marks(Vec::new(), bare.to_vec());
    MorphLayout::decode(&morphs, 7, &extras).record_ranges
}

#[test]
fn a_morph_span_ends_before_a_bare_node_at_its_right_edge() {
    assert_eq!(decoded_ranges(&[]), vec![(0, 2), (3, 4)]);
    assert_eq!(decoded_ranges(&[2]), vec![(0, 1), (3, 4)]);
    assert_eq!(decoded_ranges(&[1, 2]), vec![(0, 0), (3, 4)]);
    assert_eq!(decoded_ranges(&[4]), vec![(0, 2), (3, 3)]);
}

#[test]
fn a_swap_moves_bare_nodes_but_leaves_deleted_gaps_in_place() {
    let g = epenthesis_grammar();
    let mut word = root_word(&g, "sapt");
    word.extras.set_shape_marks(vec![1, 4], vec![0, 3]);
    word.reorder_morphs(&[0, 2, 1, 3, 4, 5]);
    assert_eq!(word.extras.bare_nodes(), &[1, 3]);
    assert_eq!(word.extras.deleted_gaps(), &[1, 4]);
}
