//! Discriminating-power proof: this fixture's `SegmentNaturalClass` is genuinely table-dependent, unlike every other multi-table fixture's `FeatureNaturalClass`-only classes.
//! See docs/research/pg-foma-segment-natural-class-table-binding-design-notes.md.

use std::fs;
use std::path::Path;

use pg_featstruct::flat_unifiable;
use pg_fst::CompileNode;
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{Grammar, NaturalClassKind, PhonRuleDef, TableId};
use pg_rules::bridge::PatternBridge;

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../../conformance-staging/edge-cases/segment-natural-class-table-binding/grammar.xml",
    )
}

fn load() -> Grammar {
    let path = fixture_path();
    let xml = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

fn devoice_rule(g: &Grammar) -> &pg_grammar::model::RewriteRuleDef {
    g.prules
        .iter()
        .find_map(|p| match p {
            PhonRuleDef::Rewrite(r) if r.xml_id == "prKtoG" => Some(r),
            _ => None,
        })
        .expect("prKtoG must be present in g.prules")
}

fn nat_class_id_by_xml_id(g: &Grammar, xml_id: &str) -> pg_grammar::model::NatClassId {
    let idx = g
        .natural_classes
        .iter()
        .position(|nc| nc.xml_id == xml_id)
        .unwrap_or_else(|| panic!("no natural class with xml id {xml_id:?}"));
    pg_grammar::model::NatClassId(idx as u32)
}

/// Structural sanity: `ncK` deliberately reuses table 0's raw index but with an opposite feature value.
/// See docs/research/pg-foma-segment-natural-class-table-binding-design-notes.md.
#[test]
fn fixture_shape_is_the_deliberately_misaligned_two_table_probe_it_claims_to_be() {
    let g = load();
    assert_eq!(
        g.char_tables.len(),
        2,
        "fixture must declare exactly 2 tables"
    );
    assert_eq!(g.strata.len(), 2, "fixture must declare exactly 2 strata");
    assert_eq!(
        g.strata[1].table,
        TableId(1),
        "S1 (\"Outer\", non-first) must own table 1"
    );
    assert!(
        g.strata[1].prules.iter().any(|&pid| matches!(
            &g.prules[pid.0 as usize],
            PhonRuleDef::Rewrite(r) if r.xml_id == "prKtoG"
        )),
        "prKtoG must be wired into S1's own phonologicalRules cascade, not S0's"
    );

    let nc_k = nat_class_id_by_xml_id(&g, "ncK");
    let NaturalClassKind::Segments(members) = &g.natural_classes[nc_k.0 as usize].kind else {
        panic!("ncK must load as a SegmentNaturalClass (NaturalClassKind::Segments)");
    };
    assert_eq!(
        members,
        &[CharDefId(0)],
        "ncK's one member must be table 1's raw index 0 (\"k\")"
    );

    let t0_z_lanes = g.char_tables[0].get(CharDefId(0)).feature_lanes();
    let t1_k_lanes = g.char_tables[1].get(CharDefId(0)).feature_lanes();
    assert_ne!(
        t0_z_lanes, t1_k_lanes,
        "table 0's raw index 0 (\"z\") and table 1's raw index 0 (\"k\") must carry OPPOSITE \
         feature values -- the deliberate misalignment that makes a wrong-table resolution \
         observably wrong rather than accidentally correct"
    );
}

/// The deliverable: `ncK`'s compiled constraint is genuinely table-dependent, so resolving against the wrong table breaks the fixture's ground truth match.
/// See docs/research/pg-foma-segment-natural-class-table-binding-design-notes.md.
#[test]
fn nat_class_k_resolved_against_the_wrong_table_stops_matching_a_real_table_1_k_segment() {
    let g = load();
    let rule = devoice_rule(&g);
    let real_k_lanes = g.char_tables[1].get(CharDefId(0)).feature_lanes().to_vec();

    // Correct: resolved against table 1, matching `RuleCache::build`'s production resolution.
    let correct = PatternBridge::new(&g)
        .with_table(TableId(1))
        .compile_pattern(&rule.lhs)
        .expect("ncK must compile against its own table 1");
    let [CompileNode::Constraint(correct_lanes)] = correct.input.nodes.as_slice() else {
        panic!(
            "rule.lhs must compile to exactly one Constraint node: {:?}",
            correct.input.nodes
        );
    };

    // Wrong: resolved against table 0, `PatternBridge::new`'s own default, the bug's own resolution.
    let wrong = PatternBridge::new(&g).compile_pattern(&rule.lhs).expect(
        "ncK must still compile against table 0 (table 0 has a raw index 0 to resolve to -- \
                 the fixture was deliberately built so the wrong-table lookup doesn't even panic, \
                 mirroring cache.rs's own probe)",
    );
    let [CompileNode::Constraint(wrong_lanes)] = wrong.input.nodes.as_slice() else {
        panic!(
            "rule.lhs must compile to exactly one Constraint node: {:?}",
            wrong.input.nodes
        );
    };

    eprintln!(
        "OUTPUT 1 (correct table, TableId(1)): ncK lanes = {correct_lanes:?}, real \"k\" lanes = \
         {real_k_lanes:?}, flat_unifiable = {}",
        flat_unifiable(&real_k_lanes, correct_lanes)
    );
    eprintln!(
        "OUTPUT 2 (wrong table, TableId(0), PatternBridge::new's own default): ncK lanes = \
         {wrong_lanes:?}, real \"k\" lanes = {real_k_lanes:?}, flat_unifiable = {}",
        flat_unifiable(&real_k_lanes, wrong_lanes)
    );

    // OUTPUT 1 (correct resolution): the constraint equals table 1's own "k" and matches a real one.
    assert_eq!(
        correct_lanes, &real_k_lanes,
        "resolved against its own table (1), ncK's compiled constraint must equal table 1's own \
         \"k\" lanes exactly: got {correct_lanes:?}, expected {real_k_lanes:?}"
    );
    assert!(
        flat_unifiable(&real_k_lanes, correct_lanes),
        "OUTPUT 1 (correct-table resolution): a real table-1 \"k\" segment MUST match ncK when \
         ncK is resolved against its own table -- this is what makes words.yaml's \"g\" -> \
         \"ROOT2|g\" reachable at all"
    );

    // OUTPUT 2 (the bug): resolved against table 0, the constraint becomes "z" and no longer matches.
    let t0_z_lanes = g.char_tables[0].get(CharDefId(0)).feature_lanes();
    assert_eq!(
        wrong_lanes, t0_z_lanes,
        "resolved against table 0 (the bug), ncK's compiled constraint must equal table 0's own \
         \"z\" lanes (raw index 0 there) -- exactly what an implicit table-zero default would grab"
    );
    assert!(
        !flat_unifiable(&real_k_lanes, wrong_lanes),
        "OUTPUT 2 (wrong-table resolution -- THE BUG): a real table-1 \"k\" segment must NOT match \
         ncK when ncK is (wrongly) resolved against table 0 -- this is the exact, concrete way the \
         eleven-site table-zero-default defect class would have made this fixture's own \"g\" -> \
         \"ROOT2|g\" ground truth UNREACHABLE (the obligatory prKtoG rule could never fire on a \
         real \"k\", so ROOT2 could never surface as \"g\", and the conformance replay would fail) \
         had it existed in the confirm engine's SegmentNaturalClass resolution path"
    );

    // Sanity: the two resolutions really do disagree (not simply both matching or both failing).
    assert_ne!(
        correct_lanes, wrong_lanes,
        "correct-table and wrong-table resolutions of the SAME natural class must differ -- if \
         they didn't, this fixture would have exactly the same discriminating-power problem as \
         every FeatureNaturalClass-only fixture in the suite"
    );
}
