//! Candidate-selection coverage for the mrCross/mrInfixDrop constructs; see `docs/research/circumfix-composite-precedence-census.md`.

mod common;

use std::path::PathBuf;

use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;

use pg_foma::emit;
use pg_foma::tags;
use pg_grammar::model::{Grammar, MorphemeId};
use pg_parse::{Morpher, ParseOptions};

use common::gate_template::{mrule_id_of, recall_reachable};

const FIXTURE: &str = "circumfix-cross-product-and-infix-drop";

/// Repo root, from this crate's own `CARGO_MANIFEST_DIR` -- never a path relative to the process CWD.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Loads the staged fixture's `grammar.xml` directly off disk -- the same grammar the oracle replays, never a drifting inline copy.
fn load_fixture() -> Grammar {
    let path = repo_root()
        .join("conformance-staging/edge-cases")
        .join(FIXTURE)
        .join("grammar.xml");
    let xml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml)
        .unwrap_or_else(|e| panic!("{FIXTURE}: grammar failed to load: {e}\n{xml}"))
}

/// Re-derives the real tag sequence(s) for `surface` by re-parsing it against `morpher`'s own grammar, never a hand-derived guess.
fn tag_sequences_for(g: &Grammar, morpher: &Morpher, surface: &str) -> Vec<Vec<String>> {
    let popts = ParseOptions::default();
    let outcome = morpher.parse_word_opts(surface, &popts);
    let width = tags::tag_width(g.morphemes.len());
    outcome
        .structured
        .iter()
        .map(|a| {
            a.morpheme_ids
                .iter()
                .enumerate()
                .map(|(i, &m)| {
                    let mid = MorphemeId(m);
                    if i as i32 == a.root_morpheme_index {
                        tags::root_tag_text(mid, width)
                    } else {
                        tags::morph_tag_text(mid, width)
                    }
                })
                .collect()
        })
        .collect()
}

/// The standard containment shape: `emit::emit`'s compiled net must fully cover every analysis the real confirm engine finds for `surface`.
fn assert_full_containment(g: &Grammar, surface: &str) {
    let emit_result = emit::emit(g);
    assert!(
        emit_result.report.uncovered.is_empty(),
        "{surface:?}: grammar must be fully covered by the enumeration path: {:?}",
        emit_result.report.uncovered
    );
    let opts = FomaOptions::default();
    let net = fsm_lexc_parse_string(&opts, None, &emit_result.lexc_source)
        .unwrap_or_else(|| panic!("emitted lexc must compile:\n{}", emit_result.lexc_source));

    let morpher = Morpher::new(g, 20_000);
    let tag_sequences = tag_sequences_for(g, &morpher, surface);
    assert!(
        !tag_sequences.is_empty(),
        "oracle word {surface:?} must parse against its own grammar -- oracle/parser \
         inconsistency, not a recall question"
    );
    let normalized = pg_grammar::nfd::nfd(surface);
    let any_reachable = tag_sequences
        .iter()
        .any(|tags| recall_reachable(&net, &normalized, tags));
    assert!(
        any_reachable,
        "{surface:?} must be reachable with its own real tag sequence"
    );
}

/// All 4 `mrCross` subrule words, plus the bare-root control, reachable in the compiled net.
#[test]
fn mr_cross_all_four_subrule_words_reachable() {
    let g = load_fixture();
    assert_full_containment(&g, "batid");
    assert_full_containment(&g, "pabatidan"); // subCrossAA
    assert_full_containment(&g, "pabatidin"); // subCrossAB
    assert_full_containment(&g, "mabatidan"); // subCrossBA (non-first, echoes census C1)
    assert_full_containment(&g, "mabatidin"); // subCrossBB (both axes jointly)
}

/// Census C4: `mrCross` and `mrInfixDrop` now both make up the structural-composite candidate set.
#[test]
fn mr_cross_and_mr_infix_drop_are_the_structural_candidates() {
    let g = load_fixture();
    let cross_mid = mrule_id_of(&g, "mrCross");
    let infix_drop_mid = mrule_id_of(&g, "mrInfixDrop");
    let diag = emit::composite_candidate_rules(&g);
    assert_eq!(
        diag.structural_candidate_count, 2,
        "mrCross and mrInfixDrop must be exactly the two structural-composite candidates: {:?}",
        diag.structural_candidates
    );
    assert!(
        diag.structural_candidates.contains(&cross_mid.0),
        "mrCross must be a structural-composite candidate: {:?}",
        diag.structural_candidates
    );
    assert!(
        diag.structural_candidates.contains(&infix_drop_mid.0),
        "mrInfixDrop must be a structural-composite candidate since census C4: {:?}",
        diag.structural_candidates
    );
}

/// Census C4: `mrInfixDrop` leaves `crate::preexpand`'s candidate set once `build_structural_composites` claims it.
#[test]
fn mr_infix_drop_leaves_preexpand_candidates_after_the_structural_widening() {
    let g = load_fixture();
    let mid = mrule_id_of(&g, "mrInfixDrop");
    let diag = emit::composite_candidate_rules(&g);
    assert!(
        !diag.preexpand_candidates.iter().any(|&(id, _)| id == mid.0),
        "mrInfixDrop must NOT appear in crate::preexpand's own candidate set once \
         build_structural_composites claims it: {:?}",
        diag.preexpand_candidates
    );
}

/// UPDATED by census C4: `bumat` is now reachable via `build_structural_composites`, not `crate::preexpand` -- see `docs/research/circumfix-composite-precedence-census.md`, C4.
#[test]
fn mr_infix_drop_word_reachable_via_build_structural_composites_now() {
    let g = load_fixture();
    assert_full_containment(&g, "bumat");
}
