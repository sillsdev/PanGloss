//! Compound non-head resolution on the real Sena grammar: `pg_rules::morph::resolve_non_head_roots` mirrors C#'s `AnalysisCompoundingRule.Apply`, replacing the non-head's shape/syntactic-FS/root-morph with the matched `LexEntry`'s own values, and this exercises `pg_rules::morph::{analyze_with_root_filter, synthesize}` directly (not full `Morpher::parse_word`) since Sena's boundary-inserting compounding rules make an unconstrained full-word search combinatorially explode.

use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{
    AllomorphId, CompoundingRuleDef, Grammar, LexEntryId, MorphRuleDef, StratumId,
};
use pg_rules::morph::{analyze_with_root_filter, synthesize};
use pg_rules::stratum::NonHeadRootFilter;
use pg_rules::{MorphRecord, Word};
use pg_shape::{NodeKind, Shape, ShapeBuilder};

fn sena_path() -> String {
    format!(
        "{}/../../../samples/data/sena-hc.xml",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn load_sena() -> Option<Grammar> {
    let path = sena_path();
    let xml = std::fs::read_to_string(&path).ok()?;
    Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("Sena grammar failed to load: {e}")))
}

/// A feature-bearing shape from `text`; table 0 is the one Sena's morphological rules resolve char-defs against.
fn shape_with_lanes(g: &Grammar, text: &str) -> Shape {
    let t = &g.char_tables[0];
    let seg = pg_grammar::segment::segment(t, text).expect("segments");
    let w = g.phon_features.len() as u32;
    let mut b = ShapeBuilder::with_features_capacity(w, seg.len());
    for (_, kind, cd, _) in seg.interior() {
        let mut lanes = vec![u64::MAX; w as usize];
        for (i, &l) in t.get(CharDefId(cd)).feature_lanes().iter().enumerate() {
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

fn char_defs(shape: &Shape) -> Vec<u32> {
    shape.interior().map(|(_, _, cd, _)| cd).collect()
}

/// Finds the real lexical entry whose allomorph's stored surface text is exactly `text`, looked up by surface rather than a hardcoded XML id so this test survives grammar-file edits.
fn find_entry_by_surface(g: &Grammar, text: &str) -> (LexEntryId, AllomorphId) {
    for (i, e) in g.entries.iter().enumerate() {
        if let Some(a) = e.allomorphs.iter().find(|a| a.shape.text == text) {
            return (LexEntryId(i as u32), a.id);
        }
    }
    panic!("no Sena lexical entry has an allomorph with surface {text:?}");
}

/// The first of Sena's 8 compounding rules, found by its authored `xml_id` rather than a `g.mrules` index, robust to unrelated grammar-file edits.
fn find_mrule1(g: &Grammar) -> (usize, &CompoundingRuleDef) {
    g.mrules
        .iter()
        .enumerate()
        .find_map(|(i, r)| match r {
            MorphRuleDef::Compounding(def) if def.xml_id == "mrule1" => Some((i, def)),
            _ => None,
        })
        .expect("Sena grammar has mrule1")
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn nonhead_resolution_replaces_shape_and_syntactic_fs() {
    let Some(g) = load_sena() else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let (_, rule_def) = find_mrule1(&g);

    let (sine_entry, sine_allo) = find_entry_by_surface(&g, "sine"); // pos69519 (non-head POS)
    let (ico_entry, ico_allo) = find_entry_by_surface(&g, "ico"); // pos97023 (head POS)
    let sine = &g.entries[sine_entry.0 as usize];

    // The raw analysis input: "sine" + boundary "+" + "ico", the literal `+` segmenting against `BoundaryDefinition id="char41"`.
    let input_shape = shape_with_lanes(&g, "sine+ico");
    let input = Word::new(input_shape, StratumId(0));

    // Stand-in for `pg-parse::RootAllomorphIndex::search`: a real surface-keyed lexicon lookup over just these two entries, matching `pg-parse::Morpher`'s production filter shape.
    let sine_cds = char_defs(&shape_with_lanes(&g, "sine"));
    let ico_cds = char_defs(&shape_with_lanes(&g, "ico"));
    let filter: NonHeadRootFilter = &|_st, shape: &Shape| {
        let cds = char_defs(shape);
        if cds == sine_cds {
            vec![pg_rules::word::ResolvedRoot::Grammar(sine_allo, sine_entry)]
        } else if cds == ico_cds {
            vec![pg_rules::word::ResolvedRoot::Grammar(ico_allo, ico_entry)]
        } else {
            Vec::new()
        }
    };

    let rule = MorphRuleDef::Compounding(clone_def(rule_def));
    let out = analyze_with_root_filter(&g, &input, &rule, filter);

    let want_head_cds = ico_cds.clone();
    let hit = out
        .iter()
        .find(|w| char_defs(&w.shape) == want_head_cds)
        .unwrap_or_else(|| {
            panic!(
                "no candidate split into head=ico; got {:?}",
                out.iter().map(|w| char_defs(&w.shape)).collect::<Vec<_>>()
            )
        });
    let nh = hit
        .current_non_head()
        .expect("the surviving candidate has a non-head");

    // (1) Shape replaced with the resolved root's own canonical shape.
    assert_eq!(
        char_defs(&nh.shape),
        sine_cds,
        "non-head shape must be the resolved root's own shape"
    );
    // Syntactic FS replaced with the entry's (no longer the empty FS `Word::new` starts from).
    assert_eq!(
        nh.syn_fs,
        *g.fs_interner.get(sine.syn_fs),
        "non-head syntactic FS must be the matched LexEntry's own FS"
    );
    // Root allomorph pinned.
    assert_eq!(
        nh.root_allomorph,
        Some(sine_allo),
        "non-head must carry the pinned root allomorph id"
    );
    // The ROOT morph is recorded on the non-head's own morph list (order 0, spans the whole shape).
    assert_eq!(
        nh.morphs,
        vec![MorphRecord::new(sine_allo, sine.morpheme, 0)],
        "non-head must carry a single order-0 ROOT MorphRecord"
    );

    // The head half of the split is untouched raw material; lexical lookup on the head is a separate step this rule-level test does not model.
    assert_eq!(char_defs(&hit.shape), ico_cds);
}

/// Clones a `CompoundingRuleDef` by rebuilding every field since neither it nor `CompoundingSubruleDef` implement `Clone` (they own `Pattern` trees).
fn clone_def(def: &CompoundingRuleDef) -> CompoundingRuleDef {
    CompoundingRuleDef {
        xml_id: def.xml_id.clone(),
        name: def.name.clone(),
        blockable: def.blockable,
        max_apps: def.max_apps,
        head_required_syn_fs: def.head_required_syn_fs,
        non_head_required_syn_fs: def.non_head_required_syn_fs,
        out_syn_fs: def.out_syn_fs,
        head_prod_restrictions_mpr: def.head_prod_restrictions_mpr,
        non_head_prod_restrictions_mpr: def.non_head_prod_restrictions_mpr,
        output_prod_restrictions_mpr: def.output_prod_restrictions_mpr,
        obligatory_features: def.obligatory_features.clone(),
        subrules: def
            .subrules
            .iter()
            .map(|sr| pg_grammar::model::CompoundingSubruleDef {
                vars: sr.vars.clone(),
                required_mpr: sr.required_mpr,
                excluded_mpr: sr.excluded_mpr,
                out_mpr: sr.out_mpr,
                head_lhs: sr.head_lhs.clone(),
                non_head_lhs: sr.non_head_lhs.clone(),
                rhs: sr.rhs.clone(),
            })
            .collect(),
    }
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn synthesis_records_non_head_root_morph_in_the_final_signature() {
    let Some(g) = load_sena() else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let (_, rule_def) = find_mrule1(&g);
    let rule = MorphRuleDef::Compounding(clone_def(rule_def));

    let (sine_entry, sine_allo) = find_entry_by_surface(&g, "sine");
    let (ico_entry, ico_allo) = find_entry_by_surface(&g, "ico");
    let sine = &g.entries[sine_entry.0 as usize];
    let ico = &g.entries[ico_entry.0 as usize];

    // Builds the post-analysis, post-(head)-lexical-lookup state directly: head = "ico" as a resolved root word, non-head = "sine" resolved exactly as `resolve_non_head_roots` does it.
    let mut head = Word::new(shape_with_lanes(&g, "ico"), StratumId(0));
    head.syn_fs = g.fs_interner.get(ico.syn_fs).clone();
    head.root_allomorph = Some(ico_allo);
    head.morphs = vec![MorphRecord::new(ico_allo, ico.morpheme, 0)];

    let mut nh = Word::new(shape_with_lanes(&g, "sine"), StratumId(0));
    nh.syn_fs = g.fs_interner.get(sine.syn_fs).clone();
    nh.root_allomorph = Some(sine_allo);
    nh.morphs = vec![MorphRecord::new(sine_allo, sine.morpheme, 0)];
    // `non_head_unapplied` (not a raw `.push`): pushes AND advances `non_head_app_index` in lock-step, required because `Word::current_non_head()` reads by that index rather than `.last()`.
    head.non_head_unapplied(nh);

    let out = synthesize(&g, &head, &rule);
    assert_eq!(
        out.len(),
        1,
        "the real POS-compatible split must synthesize exactly one word"
    );
    let w = &out[0];

    // The non-head's ROOT morph survives into the synthesized word: before the fix, the non-head's `morphs` list was always empty, so `sine.morpheme` would be missing here entirely.
    assert_eq!(
        w.morpheme_sequence(),
        vec![sine.morpheme, ico.morpheme],
        "compound signature must include BOTH the non-head and head morpheme ids, non-head first \
         (surface order: nonhead + '+' + head)"
    );
    // The non-head's material is consumed into the compound's `shape`/`morphs`, but `non_heads` itself is NOT cleared -- it stays as permanent history, which is what lets `Word::dedup_key()` distinguish two compounds built from surface-homophone but lexically distinct non-heads.
    assert_eq!(
        w.non_heads.len(),
        1,
        "the non-head stays in the list as history, not popped"
    );
    assert_eq!(
        char_defs(&w.non_heads[0].shape),
        char_defs(&shape_with_lanes(&g, "sine")),
        "the retained non-head is still the resolved \"sine\" root"
    );
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn synthesis_non_head_syntactic_fs_gate_rejects_a_mismatched_root() {
    let Some(g) = load_sena() else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let (_, rule_def) = find_mrule1(&g);
    let rule = MorphRuleDef::Compounding(clone_def(rule_def));

    let (ico_entry, ico_allo) = find_entry_by_surface(&g, "ico"); // pos97023 -- head POS
    let ico = &g.entries[ico_entry.0 as usize];

    // mrule1's `non_head_required_syn_fs` requires the non-head's own POS; plug in a non-head resolved from an entry of the head's POS instead -- a lexically real root, but the wrong part of speech for this rule's non-head slot, which `synth_compound`'s `is_unifiable` check must now reject.
    let mut head = Word::new(shape_with_lanes(&g, "ico"), StratumId(0));
    head.syn_fs = g.fs_interner.get(ico.syn_fs).clone();
    head.root_allomorph = Some(ico_allo);
    head.morphs = vec![MorphRecord::new(ico_allo, ico.morpheme, 0)];

    let mut wrong_pos_nh = Word::new(shape_with_lanes(&g, "ico"), StratumId(0));
    wrong_pos_nh.syn_fs = g.fs_interner.get(ico.syn_fs).clone(); // pos97023, not pos69519
    wrong_pos_nh.root_allomorph = Some(ico_allo);
    wrong_pos_nh.morphs = vec![MorphRecord::new(ico_allo, ico.morpheme, 0)];
    // `non_head_unapplied` (not a raw `.push`) -- see the sibling fix above.
    head.non_head_unapplied(wrong_pos_nh);

    let out = synthesize(&g, &head, &rule);
    assert!(
        out.is_empty(),
        "a non-head whose real syntactic FS conflicts with non_head_required_syn_fs must be \
         rejected by the gate, got {} candidate(s)",
        out.len()
    );
}
