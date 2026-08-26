mod common;

use std::time::Duration;

use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;

use pg_foma::emit;
use pg_foma::tags;
use pg_grammar::model::MorphemeId;
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};
use pg_parse::morpher::GenMorpheme;
use pg_parse::{Morpher, ParseOptions};

use common::gate_template::{assert_net_size_within, entry_id_of, per_word_p99, recall_reachable};

fn recipe() -> Recipe {
    Recipe {
        name: "phase-c-compounding",
        seed: 20260720,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            compounding_rule_count: 1,
            ..Default::default()
        },
    }
}
#[test]
fn compounding_recall_parity_via_generator_and_oracle() {
    let recipe = recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated compounding XML failed to load: {e}\n{}",
            rendered.xml
        )
    });

    let compounding = rendered
        .compounding
        .as_ref()
        .expect("recipe declared compounding_rule_count > 0");
    assert_eq!(compounding.rule_xml_ids.len(), 1);
    assert_eq!(g.entries.len(), 2, "1 head root + 1 non-head root");
    let compounding_rules = g
        .mrules
        .iter()
        .filter(|r| matches!(r, pg_grammar::model::MorphRuleDef::Compounding(_)))
        .count();
    assert_eq!(compounding_rules, 1);

    // Oracle uses `GenMorpheme::NonHead`, since `sweep`'s `candidate_rules` excludes `Compounding` rules (they own no `MRuleId` a `GenMorpheme::Rule` could name).
    let head_id = entry_id_of(&g, &compounding.head_entry_xml_ids[0]);
    let nonhead_id = entry_id_of(&g, &compounding.nonhead_entry_xml_ids[0]);
    let morpher = Morpher::new(&g, 20_000).with_word_timeout(Some(Duration::from_millis(500)));
    let words = morpher.generate_words(
        head_id,
        &[GenMorpheme::NonHead(nonhead_id)],
        pg_featstruct::FeatureStruct::EMPTY,
    );
    assert!(
        !words.is_empty(),
        "oracle must produce at least 1 compound word"
    );

    // --- Build the FST via the production enumeration path (module doc). ---
    let emit_result = emit::emit(&g);
    assert!(
        emit_result.report.uncovered.is_empty(),
        "compounding must be fully covered by the enumeration path: {:?}",
        emit_result.report.uncovered
    );
    let opts = FomaOptions::default();
    let net = fsm_lexc_parse_string(&opts, None, &emit_result.lexc_source)
        .unwrap_or_else(|| panic!("emitted lexc must compile:\n{}", emit_result.lexc_source));

    // --- Resource envelope (design doc §4b): a 2-root, 1-compounding-rule grammar must stay tiny. ---
    assert_net_size_within(&net, 2_000, 20_000);

    // Re-parses each oracle word via an independent Morpher to recover its tag sequence, then checks reachability in `net`; 100% recall required.
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
                    .map(|&m| {
                        let mid = MorphemeId(m);
                        // A compound has two root morphemes but `root_morpheme_index` names only one; the compiled lexc tags every root entry (head and non-head) identically, so check root-ness directly instead of trusting `root_morpheme_index`.
                        if g.entries.iter().any(|e| e.morpheme == mid) {
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
    for surface in &words {
        let normalized = pg_grammar::nfd::nfd(surface);
        let analyses = tag_sequences_for(surface);
        assert!(!analyses.is_empty(), "oracle word {surface:?} has no analysis from the SAME grammar's own Morpher -- oracle/parser inconsistency");
        let any_reachable = analyses
            .iter()
            .any(|tags| recall_reachable(&net, &normalized, tags));
        if !any_reachable {
            missed.push(surface.clone());
        }
    }
    assert!(
        missed.is_empty(),
        "100% recall required on the oracle word list; missed: {missed:?}"
    );

    let p99 = per_word_p99(&words, |surface| {
        let normalized = pg_grammar::nfd::nfd(surface);
        for tags in tag_sequences_for(surface) {
            let _ = recall_reachable(&net, &normalized, &tags);
        }
    });
    assert!(
        p99 < Duration::from_millis(50),
        "per-word p99 {p99:?} exceeds the trip-wire"
    );
}
