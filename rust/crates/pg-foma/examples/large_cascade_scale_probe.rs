//! Part C (delanguaging) measurement tool, second required shape: a synthetic LARGE-CASCADE
//! grammar (many roots x several circumfix rules, all routed through `pg_foma::emit`'s
//! "structural composite" builder -- `build::circumfix`'s own module doc: circumfix ALWAYS routes
//! through the full `pg_parse::Morpher`-driven composite synthesis, `O(roots x rules)` probes,
//! never literal-lexc concatenation) -- the shape closest to the OTHER historical Aweti anchor
//! quoted directly in `pg_foma::analyzer::FomaError::EnumerationBudgetExceeded`'s own doc: "the
//! Aweti grammar -- 855 roots, 123 rules, 3 strata -- ... 2,833,559 fusion entries, a
//! 691MB/9.7M-line lexc, and an ~8.8GB `apply_up` allocation that killed the process outright."
//! That is Fix 1's OWN motivating case, and Fix 1 (the default-on `EnumerationBudget`) already
//! guards it -- this probe measures whether a synthetic roots x rules cascade (1) stays cheap at
//! moderate scale and (2) trips the SAME honest, typed `EnumerationBudgetExceeded` (never an OOM)
//! once pushed past default budget, using nothing but `pg_grammar_gen::build::circumfix` at
//! increasing `entries_per_stratum x circumfix_count`.
//!
//! Run with `cargo run -p pg-foma --release --example large_cascade_scale_probe`.

use std::time::Instant;

use pg_foma::analyzer::{FomaError, FomaProposer};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

fn recipe(entries: usize, circumfix_count: usize) -> Recipe {
    Recipe {
        name: "large-cascade-scale-probe",
        seed: 20260725,
        scale: ScaleKnobs {
            entries_per_stratum: entries,
            segment_inventory: entries + 2,
            ..ScaleKnobs::default()
        },
        construct: ConstructKnobs {
            table_count: 1,
            circumfix_count,
            template_slot_optional: true,
            ..Default::default()
        },
    }
}

fn main() {
    println!("=== large_cascade_scale_probe: roots x circumfix-rules composite-cascade scale ===\n");

    // (entries, circumfix_count) pairs -- both axes of the "roots x rules" product Fix 1's own
    // motivating case names. Capped at entries=24 (table_count=1 needs entries+2 <= 26 distinct
    // ASCII letters, `build::tables`' own ceiling).
    for &(entries, rules) in &[
        (3usize, 1usize),
        (8, 2),
        (16, 3),
        (24, 4),
        (24, 8),
        (24, 12),
        (24, 16),
        (24, 24),
    ] {
        let recipe = recipe(entries, rules);
        let rendered = pg_grammar_gen::render_indexed(&recipe);
        let g = pg_grammar::load(&rendered.xml)
            .unwrap_or_else(|e| panic!("entries={entries} rules={rules}: XML failed to load: {e}"));
        assert_eq!(g.entries.len(), entries);

        let t0 = Instant::now();
        let (result, profile) = FomaProposer::new_with_profile(&g);
        let elapsed = t0.elapsed();
        match result {
            Ok(_proposer) => {
                println!(
                    "entries={entries:>3} rules={rules:>2} (product={:>4}): Ok in {elapsed:?} \
                     — lexc_lines={:?} states={:?} arcs={:?}",
                    entries * rules,
                    profile.total_lexc_lines,
                    profile.final_state_count,
                    profile.final_arc_count,
                );
            }
            Err(FomaError::EnumerationBudgetExceeded {
                measure,
                value,
                limit,
            }) => {
                let product = entries * rules;
                println!(
                    "entries={entries:>3} rules={rules:>2} (product={product:>4}): honest \
                     EnumerationBudgetExceeded in {elapsed:?} — {measure}={value} (limit {limit}) \
                     — the Fix-1 guard fired, exactly as designed (no OOM, no hang)"
                );
            }
            Err(other) => {
                let product = entries * rules;
                println!(
                    "entries={entries:>3} rules={rules:>2} (product={product:>4}): OTHER error in \
                     {elapsed:?}: {other}"
                );
            }
        }
    }
}
