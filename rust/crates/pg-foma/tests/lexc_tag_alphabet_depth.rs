//! Declared morpheme tags stay in the compiled lexc alphabet as stratum depth grows, through the mainline `emit` path (divergence 029).

use std::collections::HashSet;

use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;

use pg_foma::emit;
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

/// Every symbol the emitted source declares under `Multichar_Symbols`.
fn declared_multichar_symbols(lexc: &str) -> Vec<String> {
    lexc.lines()
        .skip_while(|line| line.trim() != "Multichar_Symbols")
        .skip(1)
        .take_while(|line| !line.trim().is_empty() && !line.starts_with("LEXICON"))
        .flat_map(|line| line.split_whitespace().map(str::to_owned))
        .collect()
}

fn missing_tags(extra_strata: usize, entries: usize) -> (usize, Vec<String>) {
    let recipe = Recipe {
        name: "lexc-tag-alphabet-depth",
        seed: 20261028,
        scale: ScaleKnobs {
            entries_per_stratum: entries,
            segment_inventory: 2,
            ..ScaleKnobs::default()
        },
        construct: ConstructKnobs {
            table_count: 1,
            extra_strata,
            ..Default::default()
        },
    };
    let g = pg_grammar::load(&pg_grammar_gen::render_indexed(&recipe).xml).expect("loads");
    let result = emit::emit(&g);
    let net = fsm_lexc_parse_string(&FomaOptions::default(), None, &result.lexc_source)
        .expect("emitted lexc compiles");
    let sigma: HashSet<&str> = net.sigma.iter().map(|s| s.symbol.as_str()).collect();
    let declared = declared_multichar_symbols(&result.lexc_source);
    let missing = declared
        .iter()
        .filter(|tag| !sigma.contains(tag.as_str()))
        .cloned()
        .collect();
    (declared.len(), missing)
}

#[test]
fn declared_tags_survive_in_the_compiled_alphabet_at_depth() {
    for extra_strata in [0, 3, 8, 16, 40] {
        let (declared, missing) = missing_tags(extra_strata, 2);
        assert!(declared > 0, "{extra_strata} extra strata: no tags declared");
        assert!(
            missing.is_empty(),
            "{extra_strata} extra strata: {} of {declared} declared tags missing from sigma: {missing:?}",
            missing.len()
        );
    }
}
