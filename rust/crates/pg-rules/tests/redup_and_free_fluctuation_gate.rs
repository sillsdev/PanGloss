//! Tier-2 #8 (reduplication morph attribution) + R3 (free-fluctuation allomorph break) acceptance
//! gate (plan §13.1.1 / §13.2 step 10).
//!
//! Both hand-built tests below follow `morph_gate.rs`'s pattern (real `pg_grammar::load`-ed probe
//! grammar, hand-authored `MorphRuleDef`s, `pg_rules::morph::{synthesize, analyze}` as the only
//! entry points) rather than loading a full reference grammar, because — verified empirically
//! against all three reference grammars before writing this file — the *observable* effect of each
//! fix (a shift in `MorphRecord.order`, or a second synthesized word) never actually surfaces on
//! Indonesian/Amharic/Sena's own words:
//!
//! - Tier-2 #8: exactly 3 subrules across all three grammars ever repeat an `Input` part in one
//!   RHS (all Indonesian: `msubrule5`/`msubrule11`/`msubrule13` — `redupMorphType` is present on 5
//!   more Amharic subrules but every one of those references its `Input` part exactly once, so
//!   C#'s own `redupParts.Count > 0` gate never fires for them). Of the 3 real cases, the two
//!   `Suffix`-hint ones (`msubrule5`/`msubrule11`, single `Input` part) are *mathematically*
//!   `order`-invariant under this fix (the new/existing split only ever swaps which occurrence
//!   claims the tail, and the tail is never the record's minimum position either way); the one
//!   `Prefix`-hint case (`msubrule13`, 2 parts) IS order-sensitive, but direct probing
//!   (`Morpher::parse_word` on `memijit-mijit`/`menulis-nulis`/`menyewa-nyewa`) shows the winning
//!   analysis chain never actually applies it — those three words resynthesize through the
//!   `Suffix`-hint `-Cont` rule (`mrule7`) instead. Reference-grammar corpus measurement (full
//!   Indonesian 121/121, full Amharic 673, Sena first-100) confirms zero signature movement.
//! - R3: Indonesian/Amharic have no adjacent same-rule allomorph pair whose environments/MPR-
//!   sets/LHS/required-syntactic-FS are literally identical, so the gate never opens on either
//!   corpus (byte-identical full-corpus re-measurement). Sena's non-capped first-100 subset moved
//!   exactly one word (`ana`, index 49): before this fix, zero analyses (`-`); after, 3 of gold's 4
//!   sub-analyses recover (`++|a+?[mn]+?a;+|...;+|...`, all of them literal substrings of gold's
//!   own answer) — real evidence the mechanism does something, and in the correct (gold) direction,
//!   even though `ana`'s full signature isn't byte-exact either before or after (so this doesn't
//!   flip the coarse match/mismatch tally).
//!
//! Given the reference grammars can't demonstrate either fix end-to-end, these hand-built tests are
//! the actual regression gate: they pin the exact mechanism against a controlled shape mirroring
//! the real subrules, so a future change can't silently revert either fix without a red test here.

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

// ---- shared builders (duplicated from `morph_gate.rs` — integration test binaries can't share
// private helpers across files without growing `common`, and these are small enough to inline). ---

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

// =================================================================================================
// Tier-2 #8 — reduplication morph attribution.
//
// C#: `SynthesisAffixProcessAllomorphRuleSpec` ctor (`_nonAllomorphActions`,
// cs:23-124) + `ApplyRhs` (cs:137-207). A repeated `CopyFromInput` of the same LHS part is *not*
// uniformly "existing input material" the way a lone one is: exactly one occurrence (selected by
// `ReduplicationHint`) stays attributed to the word's existing morph; the rest become new material
// attributed to the affix's own morpheme.
// =================================================================================================

/// Mirrors Indonesian `msubrule13` (`mrule15`, "REDUP-meN", `redupMorphType="prefix"`) exactly in
/// shape: LHS = [attached-material part, stem part], RHS = [Copy(stem), Insert(filler),
/// Copy(attached-material), Copy(stem)] — the stem is referenced twice (a real redup group), the
/// attached-material once (an ordinary singleton copy, unaffected by this fix).
///
/// Before this fix (`morph.rs` on `rust` at `3c36cbd3`): every `CopyFromInput` defaulted to
/// `Origin::Head`, so *both* stem copies — including the new, leading one — were folded into the
/// SAME existing-morph `MorphKey`, wrongly claiming the whole output's leftmost position (`order`
/// 0) for the root. The new affix morpheme's own record then only covered the filler insert,
/// sorting *after* the root. That is backwards versus C#: the first-position stem echo is new
/// material introduced by this rule, and C#'s own record for it necessarily starts at position 0
/// (`SynthesisAffixProcessAllomorphRuleSpec.cs:99-119`, `Prefix` branch marks the *last* occurrence
/// existing, not the first).
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
    // The root's own order must be strictly after the redup morpheme's (it starts only once the
    // leading echo + filler + attached-material copy have all been emitted).
    assert!(
        morphs[0].order < morphs[1].order,
        "redup morpheme (order {}) must precede the root (order {})",
        morphs[0].order,
        morphs[1].order
    );
}

/// Mirrors Indonesian `msubrule5`/`msubrule11` (`mrule7`/`mrule13`, "-Cont"/"-Pl",
/// `redupMorphType="suffix"`, single LHS part): RHS = [Copy(stem), Insert(glue), Copy(stem)]. Per
/// `SynthesisAffixProcessAllomorphRuleSpec.cs:99-119`'s `Suffix`/`Implicit` branch, the FIRST
/// occurrence stays existing (root) and the SECOND becomes new (affix) — the opposite selection
/// from the `Prefix` case above, confirming `classify_redup`'s hint-dependent branch is exercised
/// both ways. (This is the shape where the corpus measurement showed zero `order` movement — see
/// the module docs — so this test pins the *classification*, not an order shift.)
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
    // Both the root and the new affix morpheme are present (the glue insert alone would already
    // guarantee this pre-fix; the assertion that matters is the *count* stays exactly 2 — neither
    // occurrence is dropped or double-counted — and root sorts first, per the module docs' proof
    // that `order` cannot move for this single-part `Suffix` shape).
    assert_eq!(
        seq,
        vec![100, 200],
        "root first, redup rule's own morpheme second; got {seq:?}"
    );
}

// =================================================================================================
// R3 — free-fluctuation allomorph break.
//
// C#: `SynthesisAffixProcessRule.cs:235-242` breaks the disjunctive-allomorph loop after a
// successful application unless the allomorph is environment-/syn-FS-constrained OR it
// `FreeFluctuatesWith` the next allomorph (`Allomorph.cs:80-98`, `AffixProcessAllomorph.cs:75-85`
// `ConstraintsEqual`). Two allomorphs of the same rule with identical LHS/environments/MPR-
// sets/required-syntactic-FS are in free variation: C# does NOT stop at the first — both surface.
// =================================================================================================

#[test]
fn constraint_equal_adjacent_allomorphs_both_synthesize() {
    let g = load_alpha_grammar();
    let stem = root_word(&g, "apa", 100);
    // Two allomorphs, same LHS pattern, no environments, no required-syn-FS, same (empty) MPR sets
    // — literally `ConstraintsEqual` — but different RHS suffixes, so their outputs are
    // distinguishable. Before R3: only the first (200) is produced. After: both.
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
    // Same shape as above, but the second allomorph has a non-empty environment set — NOT
    // `ConstraintsEqual` to the first (`AffixProcessAllomorph.cs:80-84` compares environment sets
    // first), so C#'s original disjunctive-break behavior must still apply: only the first
    // (unconstrained) allomorph's word is produced. This also matches a real grammar shape:
    // allomorphs disambiguated by environment are exactly the ordinary, non-free-fluctuating case
    // the break condition's own `allo.Environments.Count == 0` guard already special-cases.
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
