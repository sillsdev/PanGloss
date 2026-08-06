//! Gate for `pg_foma::emit::verify_tags_reachable`: a tag absent from the compiled lexc network's sigma is NOT recall loss, but an upstream `foma`-crate tokenizer bug (filed as `divvun/foma-rs`) that mis-tokenizes a multichar symbol whose name contains a literal `0` digit; `verify_tags_reachable` was corrected to recognize that exact decomposition artifact rather than flag it, and is kept as a safety net for a genuinely different future defect.

use pg_foma::emit::emit_underlying_templated;
use pg_foma::replace::SegAlphabet;
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

const UNREACHABLE_KIND: &str = "unreachable-after-lexc-compile";

/// 24 independent stratum-attached standalone prefix rules layered over a templated stratum 0; kept as the historical recipe that used to trip the `%0`-escape sigma artifact, since 24 morphemes pushes `tags::tag_width` to 2 digits and zero-pads several tag ids.
fn stratum_scale_recipe() -> Recipe {
    Recipe {
        name: "reachability-gate-stratum-scale",
        seed: 1,
        scale: ScaleKnobs {
            entries_per_stratum: 2,
            segment_inventory: 26,
            ..ScaleKnobs::default()
        },
        construct: ConstructKnobs {
            table_count: 1,
            circumfix_count: 1,
            template_slot_optional: true,
            extra_strata: 24,
            ..Default::default()
        },
    }
}

/// No false positive on the historical trigger shape: this recipe used to make `verify_tags_reachable` report several tags as unreachable, but `apply_down` against the actual compiled net proved they're genuinely reachable, so this test pins zero findings now.
#[test]
fn detection_does_not_false_positive_on_the_historical_zero_escape_shape() {
    let recipe = stratum_scale_recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml)
        .unwrap_or_else(|e| panic!("generated XML failed to load: {e}\n{}", rendered.xml));
    assert_eq!(g.strata.len(), 25, "1 base stratum + 24 extra");
    assert_eq!(rendered.extra_strata.len(), 24);

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let result = emit_underlying_templated(&g, &alphabet, None);

    // Sanity: this recipe's lexc source must still compile.
    let opts = foma::options::FomaOptions::default();
    let _net = foma::lexcread::fsm_lexc_parse_string(&opts, None, &result.lexc_source)
        .unwrap_or_else(|| panic!("emitted lexc must compile:\n{}", result.lexc_source));

    let unreachable: Vec<&pg_foma::emit::UncoveredItem> = result
        .report
        .uncovered
        .iter()
        .filter(|u| u.kind == UNREACHABLE_KIND)
        .collect();
    assert!(
        unreachable.is_empty(),
        "this recipe's tags are all genuinely reachable (verified via apply_down -- see this \
         test's own doc); the known %0-escape sigma artifact must not be reported as a gap: {:?}",
        unreachable
    );
}

/// No false positive: an ordinary single-stratum templated grammar must report zero findings, proving the detection does not fire on ordinary, healthy grammars.
#[test]
fn detection_does_not_false_positive_on_ordinary_grammar() {
    let recipe = Recipe {
        name: "reachability-gate-ordinary",
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
    };
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml)
        .unwrap_or_else(|e| panic!("generated XML failed to load: {e}\n{}", rendered.xml));
    assert_eq!(g.strata.len(), 1, "ordinary recipe: exactly 1 stratum");

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let result = emit_underlying_templated(&g, &alphabet, None);

    let unreachable: Vec<&pg_foma::emit::UncoveredItem> = result
        .report
        .uncovered
        .iter()
        .filter(|u| u.kind == UNREACHABLE_KIND)
        .collect();
    assert!(
        unreachable.is_empty(),
        "an ordinary single-stratum grammar must report NO {UNREACHABLE_KIND:?} findings (false \
         positive); got: {unreachable:?}"
    );
}
