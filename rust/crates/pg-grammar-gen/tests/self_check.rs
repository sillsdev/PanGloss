//! Gate 0 (design doc §2's "the cheap gate-0 self-check"): every stage-1 builder's recipe renders
//! XML `pg_grammar::load` accepts, and `render(recipe)` is deterministic -- the SAME recipe
//! rendered twice produces byte-identical XML. Not `#[ignore]`d: everything here is generated,
//! no external fixture needed, and every check is fast (well under a second).

use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

fn load(xml: &str) -> pg_grammar::model::Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("generated XML failed to load: {e}\n\n{xml}"))
}

fn assert_deterministic(recipe: &Recipe) -> String {
    let a = pg_grammar_gen::render(recipe);
    let b = pg_grammar_gen::render(recipe);
    assert_eq!(a, b, "render({:?}) is not deterministic", recipe.name);
    a
}

// --- Single-table, no construct knobs at all: the minimal grammar every recipe degenerates to. ---

#[test]
fn minimal_single_table_recipe_loads_and_is_deterministic() {
    let recipe = Recipe {
        name: "self-check-minimal",
        seed: 1,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs::default(),
    };
    let xml = assert_deterministic(&recipe);
    let g = load(&xml);
    assert_eq!(g.char_tables.len(), 1);
    assert_eq!(g.strata.len(), 1);
    assert_eq!(g.entries.len(), recipe.scale.entries_per_stratum);
}

// --- build::tables: multi-table (GATE 1's shape). ---

#[test]
fn multi_table_recipe_loads_and_is_deterministic() {
    let recipe = Recipe {
        name: "self-check-multi-table",
        seed: 2,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 2,
            ..Default::default()
        },
    };
    let xml = assert_deterministic(&recipe);
    let g = load(&xml);
    assert_eq!(g.char_tables.len(), 2);
    assert_eq!(g.strata.len(), 2);
    // Table 0 and table 1 must never share a character (build::tables's own doc/tests already
    // pin this at the builder level; re-verified here post-load as a true end-to-end check).
    let mut seen = std::collections::HashSet::new();
    for t in &g.char_tables {
        for (_, cd) in t.iter() {
            for rep in cd.representations() {
                assert!(
                    seen.insert(rep.to_string()),
                    "representation {rep:?} reused across tables"
                );
            }
        }
    }
    // The demo devoicing rule must actually be present (design doc §5: "compile succeeds").
    assert_eq!(g.prules.len(), 1);
}

#[test]
fn table_count_three_still_loads() {
    let recipe = Recipe {
        name: "self-check-three-tables",
        seed: 3,
        scale: ScaleKnobs {
            entries_per_stratum: 2,
            segment_inventory: 2,
            ..ScaleKnobs::default()
        },
        construct: ConstructKnobs {
            table_count: 3,
            ..Default::default()
        },
    };
    let xml = assert_deterministic(&recipe);
    let g = load(&xml);
    assert_eq!(g.char_tables.len(), 3);
    assert_eq!(g.strata.len(), 3);
}

// --- build::circumfix + build::template (GATE 2's shape). ---

#[test]
fn circumfix_recipe_loads_and_is_deterministic() {
    let recipe = Recipe {
        name: "self-check-circumfix",
        seed: 4,
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
    let xml = assert_deterministic(&recipe);
    let g = load(&xml);
    assert_eq!(g.entries.len(), 3);
    assert_eq!(g.templates.len(), 1);
    // Exactly one AffixProcess-kind circumfix rule, no phonological rules at all (single table,
    // no devoice demo).
    assert_eq!(g.prules.len(), 0);
    let affix_process_rules = g
        .mrules
        .iter()
        .filter(|r| matches!(r, pg_grammar::model::MorphRuleDef::AffixProcess(_)))
        .count();
    assert_eq!(affix_process_rules, 1);
}

#[test]
fn circumfix_recipe_with_multiple_rules_loads() {
    let recipe = Recipe {
        name: "self-check-circumfix-multi",
        seed: 5,
        scale: ScaleKnobs {
            entries_per_stratum: 2,
            segment_inventory: 6,
            ..ScaleKnobs::default()
        },
        construct: ConstructKnobs {
            table_count: 1,
            circumfix_count: 2,
            template_slot_optional: true,
            ..Default::default()
        },
    };
    let xml = assert_deterministic(&recipe);
    let g = load(&xml);
    let affix_process_rules = g
        .mrules
        .iter()
        .filter(|r| matches!(r, pg_grammar::model::MorphRuleDef::AffixProcess(_)))
        .count();
    assert_eq!(affix_process_rules, 2);
}

// --- Different seeds/names must not collide on ids or accidentally produce identical output when
// the knobs themselves differ; same name+seed+knobs must always match (already covered above,
// re-asserted here across two independently-constructed recipes for extra confidence). ---

#[test]
fn same_recipe_fields_reproduce_identical_xml_across_independent_calls() {
    let make = || Recipe {
        name: "self-check-repro",
        seed: 99,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 2,
            ..Default::default()
        },
    };
    let xml_a = pg_grammar_gen::render(&make());
    let xml_b = pg_grammar_gen::render(&make());
    assert_eq!(xml_a, xml_b);
}

// --- Stage 2: build::gating (partition-k). ---

#[test]
fn gating_recipe_loads_and_is_deterministic() {
    let recipe = Recipe {
        name: "self-check-gating",
        seed: 10,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            gated_subrule_count: 2,
            ..Default::default()
        },
    };
    let xml = assert_deterministic(&recipe);
    let g = load(&xml);
    assert_eq!(
        g.entries.len(),
        4,
        "2 gated rules must realize 2^2 = 4 entries"
    );
    assert_eq!(g.prules.len(), 2);
    assert_eq!(
        g.mpr_groups.len(),
        0,
        "no MprGroup declared -- every bit is ungrouped"
    );
}

// --- Stage 2: build::alpha (alpha-variable scale). ---

#[test]
fn alpha_recipe_loads_and_is_deterministic() {
    let recipe = Recipe {
        name: "self-check-alpha",
        seed: 11,
        scale: ScaleKnobs {
            segment_inventory: 3,
            ..ScaleKnobs::default()
        },
        construct: ConstructKnobs {
            table_count: 1,
            alpha_var_count: 2,
            alpha_class_size: 3,
            ..Default::default()
        },
    };
    let xml = assert_deterministic(&recipe);
    let g = load(&xml);
    assert_eq!(g.entries.len(), 1, "alpha recipe has exactly 1 root");
    assert_eq!(g.prules.len(), 2, "alpha_var_count independent rules");
}

// --- Stage 2: build::strata (stratum-depth scale). ---

#[test]
fn strata_recipe_loads_and_is_deterministic() {
    let recipe = Recipe {
        name: "self-check-strata",
        seed: 12,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            extra_strata: 2,
            ..Default::default()
        },
    };
    let xml = assert_deterministic(&recipe);
    let g = load(&xml);
    assert_eq!(g.strata.len(), 3, "1 base stratum + 2 extra");
}

// --- Stage 2: build::compounding (compounding-rule scale). ---

#[test]
fn compounding_recipe_loads_and_is_deterministic() {
    let recipe = Recipe {
        name: "self-check-compounding",
        seed: 13,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            compounding_rule_count: 1,
            ..Default::default()
        },
    };
    let xml = assert_deterministic(&recipe);
    let g = load(&xml);
    assert_eq!(g.entries.len(), 2, "1 head root + 1 non-head root");
    let compounding_rules = g
        .mrules
        .iter()
        .filter(|r| matches!(r, pg_grammar::model::MorphRuleDef::Compounding(_)))
        .count();
    assert_eq!(compounding_rules, 1);
}

// --- Stage 2: build::quantifier (HONEST-SKIP bail gate). ---

#[test]
fn quantifier_recipe_loads_and_is_deterministic() {
    let recipe = Recipe {
        name: "self-check-quantifier",
        seed: 14,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            quantifier_bound: Some((1, 3)),
            ..Default::default()
        },
    };
    let xml = assert_deterministic(&recipe);
    let g = load(&xml);
    assert_eq!(g.entries.len(), 1);
    assert_eq!(g.prules.len(), 1);
}

// --- Stage 2: build::metathesis (HONEST-SKIP bail gate). ---

#[test]
fn metathesis_recipe_loads_and_is_deterministic() {
    let recipe = Recipe {
        name: "self-check-metathesis",
        seed: 15,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            metathesis_rule_count: 1,
            ..Default::default()
        },
    };
    let xml = assert_deterministic(&recipe);
    let g = load(&xml);
    assert_eq!(g.entries.len(), 1);
    let metathesis_rules = g
        .prules
        .iter()
        .filter(|p| matches!(p, pg_grammar::model::PhonRuleDef::Metathesis(_)))
        .count();
    assert_eq!(metathesis_rules, 1);
}

// --- Stage 2: build::simultaneous (HONEST-SKIP bail gate, needs detection wiring). ---

#[test]
fn simultaneous_recipe_loads_and_is_deterministic() {
    let recipe = Recipe {
        name: "self-check-simultaneous",
        seed: 16,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            simultaneous_rule_count: 1,
            ..Default::default()
        },
    };
    let xml = assert_deterministic(&recipe);
    let g = load(&xml);
    assert_eq!(g.entries.len(), 1);
    assert_eq!(g.prules.len(), 1);
}

// --- Stage 2: build::right_to_left (HONEST-SKIP bail gate, needs detection wiring). ---

#[test]
fn right_to_left_recipe_loads_and_is_deterministic() {
    let recipe = Recipe {
        name: "self-check-rtl",
        seed: 17,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            rtl_rule_count: 1,
            ..Default::default()
        },
    };
    let xml = assert_deterministic(&recipe);
    let g = load(&xml);
    assert_eq!(g.entries.len(), 1);
    assert_eq!(g.prules.len(), 1);
}

// --- Part C (delanguaging): build::chain (deep standalone-affix chain). ---

#[test]
fn chain_recipe_loads_and_is_deterministic() {
    let recipe = Recipe {
        name: "self-check-chain",
        seed: 18,
        scale: ScaleKnobs {
            segment_inventory: 6,
            ..ScaleKnobs::default()
        },
        construct: ConstructKnobs {
            table_count: 1,
            chain_rule_count: 5,
            ..Default::default()
        },
    };
    let xml = assert_deterministic(&recipe);
    let g = load(&xml);
    assert_eq!(g.entries.len(), 1, "chain recipe has exactly 1 root");
    let affix_process_rules = g
        .mrules
        .iter()
        .filter(|r| matches!(r, pg_grammar::model::MorphRuleDef::AffixProcess(_)))
        .count();
    assert_eq!(affix_process_rules, 5, "chain_rule_count standalone rules");
    // None of these rules are wrapped in a template -- they're stratum-attached standalone rules.
    assert_eq!(g.templates.len(), 0);
}

#[cfg(feature = "oracle")]
#[test]
fn oracle_sweep_on_circumfix_recipe_is_non_vacuous() {
    let recipe = Recipe {
        name: "self-check-circumfix-oracle",
        seed: 6,
        scale: ScaleKnobs {
            entries_per_stratum: 2,
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
    let g = load(&rendered.xml);
    let roots: Vec<pg_grammar::model::LexEntryId> = (0..g.entries.len() as u32)
        .map(pg_grammar::model::LexEntryId)
        .collect();
    let words = pg_grammar_gen::oracle::sweep_all_rules(
        &g,
        &roots,
        &pg_grammar_gen::oracle::OracleOpts::default(),
    );
    assert!(!words.is_empty(), "oracle sweep produced zero words");
    // Both the bare-root and the circumfixed form must show up for at least one root.
    assert!(
        words.iter().any(|w| w.mrule.is_none()),
        "no bare-root oracle word"
    );
    assert!(
        words.iter().any(|w| w.mrule.is_some()),
        "no circumfixed oracle word"
    );
}
