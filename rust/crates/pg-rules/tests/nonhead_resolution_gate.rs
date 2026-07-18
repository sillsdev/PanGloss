//! Plan Tier-2 #7 acceptance gate — first-class compound non-head resolution, on the **real Sena**
//! grammar (8 `CompoundingRule`s, `rust-conversion.md` §13.1.1's "sharpened" Tier-2 #7 row).
//!
//! C# resolves a compounding analysis split's non-head against the lexicon at analysis time
//! (`AnalysisCompoundingRule.Apply`'s `_morpher.SearchRootAllomorphs` loop, cs:63-124) and
//! **replaces** the non-head's shape/syntactic-FS with the matched `LexEntry`'s own canonical
//! values (`Word.RootAllomorph` setter → `SetRootAllomorph`, Word.cs:148-169) — mirrored here by
//! `pg_rules::morph::resolve_non_head_roots` (called from `ana_compound_subrule`, threaded via
//! `NonHeadRootFilter`, the same crate-boundary shape `pg-parse::Morpher::set_root_allomorph` uses
//! for the head root). Three coupled effects fall out of that one fix, all exercised below on the
//! same real lexical entries:
//! 1. the resolved non-head carries real shape/syn_fs/mpr/root_allomorph/morph data (no longer an
//!    empty `Word::new` placeholder);
//! 2. `SynthesisCompoundingRule`'s non-head syntactic-FS gate (`morph::synth_compound`'s
//!    `is_unifiable` check, cs:81-99) is no longer vacuously true;
//! 3. the non-head's ROOT morph is recorded and survives into the synthesized word's morph list
//!    (`SynthesisCompoundingRule.ApplySubrule`'s `output.MarkMorph(newMorphNodes,
//!    ...CurrentNonHead.RootAllomorph, Word.RootMorphID)`, cs:288 — `morph::attribute_morphs`'s
//!    `Origin::NonHead` branch here), so a compound signature includes the non-head morpheme id.
//!
//! Full-`Morpher::parse_word` coverage over real corpus words is not used here: the sample Sena
//! grammar's 8 compounding rules all insert a literal `+` **boundary** node at the juncture
//! (`InsertSegments><PhoneticShape>+</PhoneticShape>`, backed by `BoundaryDefinition id="char41"`)
//! that a real orthographic corpus word never contains (confirmed: zero `+` in
//! `samples/data/sena-words.txt`), and the rules' head/non-head patterns are POS-gated only
//! (`1+ of any segment`), so an unconstrained full-word search explores every split of every
//! substring — the documented pre-existing Sena tractability gap (§13.1.1's Tier-2 #14 / "compile-
//! once cache" items), independent of this fix. Calling `pg_rules::morph::{analyze_with_root_filter,
//! synthesize}` directly on real Sena lexical entries (the same pattern
//! `sena_affix_and_compounding_rules_compile_and_run_without_panic` in `morph_gate.rs` already
//! uses for structural coverage) exercises the real grammar's real rule + real lexicon data without
//! the combinatorial full-parse search.

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

/// A feature-bearing shape from `text` (mirrors `morph_gate.rs`'s `shape_with_lanes`; table 0 is
/// the one Sena's morphological rules resolve char-defs against, matching `pg_rules::morph`'s
/// `TABLE` constant).
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

/// Find the real lexical entry whose first allomorph's stored surface text is exactly `text`
/// (`entry575` = "sine"/pos69519, `entry621` = "ico"/pos97023 in the current sample file — looked
/// up by surface rather than hardcoded XML id/line number so this test survives grammar-file
/// edits). Returns `(LexEntryId, AllomorphId)`.
fn find_entry_by_surface(g: &Grammar, text: &str) -> (LexEntryId, AllomorphId) {
    for (i, e) in g.entries.iter().enumerate() {
        if let Some(a) = e.allomorphs.iter().find(|a| a.shape.text == text) {
            return (LexEntryId(i as u32), a.id);
        }
    }
    panic!("no Sena lexical entry has an allomorph with surface {text:?}");
}

/// `mrule1` (`headPartsOfSpeech="pos97023 pos125728" nonHeadPartsOfSpeech="pos69519"`,
/// `<Name>ndi+ipron</Name>`, output = `CopyFromInput(nonhead) + InsertSegments("+") +
/// CopyFromInput(head)`): the first of Sena's 8 compounding rules, found by its authored `xml_id`
/// rather than a `g.mrules` index (robust to unrelated grammar-file edits).
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
fn nonhead_resolution_replaces_shape_and_syntactic_fs() {
    let Some(g) = load_sena() else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let (_, rule_def) = find_mrule1(&g);

    let (sine_entry, sine_allo) = find_entry_by_surface(&g, "sine"); // pos69519 (non-head POS)
    let (ico_entry, ico_allo) = find_entry_by_surface(&g, "ico"); // pos97023 (head POS)
    let sine = &g.entries[sine_entry.0 as usize];

    // The raw analysis input: "sine" + boundary "+" + "ico" (the literal `+` segments against
    // `BoundaryDefinition id="char41"`, `<Representation>+</Representation>` — see module docs).
    let input_shape = shape_with_lanes(&g, "sine+ico");
    let input = Word::new(input_shape, StratumId(0));

    // Stand-in for `pg-parse::RootAllomorphIndex::search`: a real (surface-keyed) lexicon lookup
    // over just these two entries, exactly the shape `pg-parse::Morpher`'s production filter has
    // (`RootAllomorphFilter`'s doc in `pg_rules::stratum`).
    let sine_cds = char_defs(&shape_with_lanes(&g, "sine"));
    let ico_cds = char_defs(&shape_with_lanes(&g, "ico"));
    let filter: NonHeadRootFilter = &|_st, shape: &Shape| {
        let cds = char_defs(shape);
        if cds == sine_cds {
            vec![(sine_allo, sine_entry)]
        } else if cds == ico_cds {
            vec![(ico_allo, ico_entry)]
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

    // The head half of the split is untouched raw material (ico's own shape) -- lexical lookup on
    // the head is a separate M5 step this rule-level test does not model; only the non-head's
    // in-rule resolution is Tier-2 #7's concern.
    assert_eq!(char_defs(&hit.shape), ico_cds);
}

/// Clone a `CompoundingRuleDef` by rebuilding every field (subrules included) since neither
/// `CompoundingRuleDef` nor `CompoundingSubruleDef` implement `Clone` (they own `Pattern` trees --
/// see `pg_grammar::model`). Test-only: production code never needs to clone a loaded rule.
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

    // Build the post-analysis, post-(head)-lexical-lookup state directly (the state
    // `pg-parse::Morpher::synthesis_pipeline` would hand to `synth_compound`): head = "ico" as a
    // resolved root word (mirroring `pg-parse::Morpher::set_root_allomorph` on the head, out of
    // this rule-level test's scope to re-derive), non-head = "sine" resolved exactly as
    // `resolve_non_head_roots` (Tier-2 #7) now does it.
    let mut head = Word::new(shape_with_lanes(&g, "ico"), StratumId(0));
    head.syn_fs = g.fs_interner.get(ico.syn_fs).clone();
    head.root_allomorph = Some(ico_allo);
    head.morphs = vec![MorphRecord::new(ico_allo, ico.morpheme, 0)];

    let mut nh = Word::new(shape_with_lanes(&g, "sine"), StratumId(0));
    nh.syn_fs = g.fs_interner.get(sine.syn_fs).clone();
    nh.root_allomorph = Some(sine_allo);
    nh.morphs = vec![MorphRecord::new(sine_allo, sine.morpheme, 0)];
    // `non_head_unapplied` (not a raw `.push`): pushes AND advances `non_head_app_index` in
    // lock-step, matching C#'s `Word.NonHeadUnapplied` (Word.cs:477-482) -- required since P4
    // (2026-07-09) made `Word::current_non_head()` read by that index rather than `.last()`.
    head.non_head_unapplied(nh);

    let out = synthesize(&g, &head, &rule);
    assert_eq!(
        out.len(),
        1,
        "the real POS-compatible split must synthesize exactly one word"
    );
    let w = &out[0];

    // (2) The non-head's ROOT morph survives into the synthesized word (Tier-2 #7's third
    // sub-part): before the fix, the non-head's `morphs` list was always empty, so
    // `attribute_morphs`'s `Origin::NonHead` branch dropped it and `sine.morpheme` would be
    // missing here entirely.
    assert_eq!(
        w.morpheme_sequence(),
        vec![sine.morpheme, ico.morpheme],
        "compound signature must include BOTH the non-head and head morpheme ids, non-head first \
         (surface order: nonhead + '+' + head)"
    );
    // The non-head's material was consumed into the compound's `shape`/`morphs`, but (P4,
    // 2026-07-09) `non_heads` itself is NOT cleared: C#'s `SynthesisCompoundingRule` never removes
    // an entry from `_nonHeadApps` (only `_nonHeadAppIndex` moves, via the confirmation step in
    // `stratum.rs`'s `guided_synth`, not exercised by this raw `morph::synthesize` call) -- the
    // non-head stays as permanent history, which is exactly what lets `Word::dedup_key()`
    // distinguish two compounds built from surface-homophone but lexically distinct non-heads (see
    // `pg-parse/tests/csharp_port_compounding.rs`'s `simple_rules_1_homophone_disjunction_finding`).
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
fn synthesis_non_head_syntactic_fs_gate_rejects_a_mismatched_root() {
    let Some(g) = load_sena() else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let (_, rule_def) = find_mrule1(&g);
    let rule = MorphRuleDef::Compounding(clone_def(rule_def));

    let (ico_entry, ico_allo) = find_entry_by_surface(&g, "ico"); // pos97023 -- head POS
    let ico = &g.entries[ico_entry.0 as usize];

    // mrule1's `non_head_required_syn_fs` encodes `nonHeadPartsOfSpeech="pos69519"`. Plug in a
    // non-head resolved from an entry of the *head's* POS (pos97023) instead of a pos69519 entry
    // -- a lexically real root, but the wrong part of speech for this rule's non-head slot. Before
    // Tier-2 #7, `synth_compound`'s `is_unifiable(non_head_required_syn_fs, nh.syn_fs)` check was
    // vacuously true (`nh.syn_fs` was always the empty FS, which unifies with anything); with the
    // fix it must now see the real (and here, conflicting) POS and reject.
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
