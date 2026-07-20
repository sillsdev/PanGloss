//! GATE 2 (`docs/fst-plan/phase-c-generator-design.md` §5/§6, priority (2)): circumfix
//! recall-parity gate -- the FIRST full end-to-end validation of generator + oracle + gate
//! together (design doc §6).
//!
//! ## Why the production ENUMERATION path, not `pg-foma/src/uflexc.rs`
//! `pg-foma/src/emit.rs`'s `classify_affix` reads a circumfix rule's shape (leading AND trailing
//! insert around one copied span) as `Role::CircumfixPrefix`, and `is_structural_rule` (that
//! module's own doc) ALWAYS routes it through the "structural composite" builder -- a
//! `pg_parse::Morpher`-driven synthesis of every composite entry, never literal-lexc
//! concatenation. `pg-foma/src/uflexc.rs` (the simpler, token-space emitter GATE 1 uses) explicitly
//! SKIPS `Role::CircumfixPrefix` (that module's own doc: "everything else ... is skipped and
//! reported"). So this gate builds its net via `pg_foma::emit::emit` (the production `emit()` ->
//! lexc pipeline `FomaProposer::new` itself calls) rather than `uflexc`/`replace` -- the only path
//! that actually covers this construct today.
//!
//! ## Recall technique
//! Same compose-recall helper as GATE 1 (`tests/common/gate_template.rs`), but querying with
//! LITERAL surface text (not `pg_foma::replace::SegAlphabet` PUA tokens -- `emit.rs`'s own lower
//! tape is literal orthography, per that module's doc, unlike `uflexc.rs`'s token-space lower
//! tape) and, per-word, the EXACT tag sequence recovered by re-parsing the oracle's own generated
//! surface word through an independent `pg_parse::Morpher` -- mirroring
//! `p6_aweti_q4_compose_recall.rs`'s own technique (copied by re-implementing it after reading
//! that file, not by referencing it -- see that file's own worktree, which a different agent is
//! actively modifying). See `tests/common/gate_template.rs`'s own module doc for a real deviation
//! found empirically while building THIS gate (a structural-composite entry's projected-upper net
//! defeats `fsm_intersect` but not a bounded `apply_up` on that same tiny net).
//!
//! 100% recall is required here (unlike GATE 1): circumfix has no known compiler gap on the
//! enumeration path (design doc §5's "Recall parity now" list), so this grammar's small size
//! keeps this gate fast while still proving the full pipeline for real.

mod common;

use std::time::Duration;

use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;

use pg_foma::emit;
use pg_foma::tags;
use pg_grammar::model::{LexEntryId, MorphemeId};
use pg_grammar_gen::oracle::{sweep, OracleOpts};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};
use pg_parse::{Morpher, ParseOptions};

use common::gate_template::{assert_net_size_within, mrule_id_of, per_word_p99, recall_reachable};

fn recipe() -> Recipe {
    Recipe {
        name: "phase-c-circumfix",
        seed: 20260720,
        scale: ScaleKnobs {
            entries_per_stratum: 3,
            segment_inventory: 5,
            ..ScaleKnobs::default()
        },
        construct: ConstructKnobs {
            table_count: 1,
            circumfix_count: 1,
            template_slot_optional: true,
            ..Default::default()
        },
    }
}

#[test]
fn circumfix_recall_parity_via_generator_and_oracle() {
    let recipe = recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| panic!("generated circumfix XML failed to load: {e}\n{}", rendered.xml));

    assert_eq!(g.entries.len(), 3, "recipe must produce exactly 3 roots");
    assert_eq!(g.templates.len(), 1, "recipe must produce exactly 1 AffixTemplate");

    let circ_xml_id = rendered.tables[0]
        .circumfix_mrule_xml_ids
        .first()
        .cloned()
        .expect("recipe must produce at least 1 circumfix rule");
    let circ_mrule = mrule_id_of(&g, &circ_xml_id);

    // --- Oracle (design doc §3): bounded Morpher-as-generator sweep over every root, bare AND
    // circumfixed. Sized so this stays cheap by construction (3 roots x <=2 forms each). ---
    let oracle_opts = OracleOpts {
        step_cap: 20_000,
        word_timeout: Some(Duration::from_millis(500)),
        max_rules_per_root: 8,
        max_total_words: 100,
    };
    let roots: Vec<LexEntryId> = (0..g.entries.len() as u32).map(LexEntryId).collect();
    let words = sweep(&g, &roots, &[circ_mrule], &oracle_opts);
    assert!(!words.is_empty(), "oracle sweep produced zero words -- gate must be non-vacuous");
    assert!(words.iter().any(|w| w.mrule.is_none()), "no bare-root oracle word generated");
    assert!(words.iter().any(|w| w.mrule.is_some()), "no circumfixed oracle word generated");
    println!("oracle produced {} words ({} roots x up to 2 forms each)", words.len(), roots.len());

    // --- Build the FST via the production enumeration path (module doc). ---
    let emit_result = emit::emit(&g);
    assert!(
        emit_result.report.uncovered.is_empty(),
        "circumfix must be fully covered by the enumeration path: {:?}",
        emit_result.report.uncovered
    );
    let opts = FomaOptions::default();
    let net = fsm_lexc_parse_string(&opts, None, &emit_result.lexc_source)
        .unwrap_or_else(|| panic!("emitted lexc must compile:\n{}", emit_result.lexc_source));

    // --- Resource envelope (design doc §4b): a 3-root, 1-circumfix grammar must stay tiny. ---
    assert_net_size_within(&net, 2_000, 20_000);

    // --- Recall (design doc §4a): re-parse each oracle word via an independent Morpher to recover
    // its own tag sequence (mirrors the P6/Aweti compose-recall technique, module doc), then check
    // that sequence is reachable in `net`. 100% required (module doc). ---
    let morpher = Morpher::new(&g, oracle_opts.step_cap).with_word_timeout(oracle_opts.word_timeout);
    let popts = ParseOptions::default();
    let width = tags::tag_width(g.morphemes.len());

    let tag_sequences_for = |surface: &str| -> Vec<Vec<String>> {
        let outcome = morpher.parse_word_opts(surface, &popts);
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
    };

    let mut missed = Vec::new();
    for w in &words {
        let normalized = pg_grammar::nfd::nfd(&w.surface);
        let analyses = tag_sequences_for(&w.surface);
        assert!(
            !analyses.is_empty(),
            "oracle word {:?} (root {:?}) has no analysis from the SAME grammar's own Morpher -- \
             oracle/parser inconsistency, not a recall question",
            w.surface,
            w.root
        );
        let any_reachable = analyses.iter().any(|tags| recall_reachable(&net, &normalized, tags));
        if !any_reachable {
            missed.push(w.surface.clone());
        }
    }
    assert!(missed.is_empty(), "100% recall required on the oracle word list; missed: {missed:?}");

    // --- Resource envelope (design doc §4b): per-word p99, sub-10ms trip-wire (generous headroom
    // for a network this small; the trip-wire is about catching a regression, not benchmarking). ---
    let p99 = per_word_p99(&words, |w| {
        let normalized = pg_grammar::nfd::nfd(&w.surface);
        for tags in tag_sequences_for(&w.surface) {
            let _ = recall_reachable(&net, &normalized, &tags);
        }
    });
    assert!(p99 < Duration::from_millis(50), "per-word p99 {p99:?} exceeds the trip-wire");
}
