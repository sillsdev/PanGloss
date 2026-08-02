//! Recipe-candidate census: for each grammar named on the command line, report which seeded recipe
//! FAMILIES the registry offers it and how many DISTINCT candidate plans those families materialize
//! to (content-addressed plan roots, so semantics-preserving rewrites that collapse back onto the
//! baseline are counted once).
//!
//! This is the cheap half of `pangloss recipe-optimize`'s own preflight — `enumerate_default` +
//! `Registry::seeded` + `recipe_space::characterize` — and nothing else: no foma compile, no word
//! list, no runtime evaluation. It exists so "did a change to a grammar predicate move the
//! measurement space?" is answerable in one run rather than by reading two optimizer reports.
//!
//! ```text
//! pg.ps1 -Mode run -Example recipe_candidate_census -- [--budget N] <grammar> [<grammar> ...]
//! ```
//! `<grammar>` is any path `pangloss` itself accepts: `.xml` (HC), `.json` (snapshot), `.fwdata`.

use std::collections::BTreeSet;
use std::path::Path;

use pg_foma::capability::rhs_has_true_reduplication;
use pg_foma::enumerate::{enumerate_default, prules_in_order};
use pg_foma::gate::find_gated_subrules;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::recipe_registry::Registry;
use pg_foma::recipe_space::{characterize, FeasibleCount};
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::{Grammar, MorphRuleDef, PhonRuleDef, ReduplicationHint};

fn load(path: &str) -> Result<Grammar, String> {
    match Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
    {
        "json" => {
            let json = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
            let snapshot = pg_snapshot::Snapshot::from_json(&json)
                .map_err(|e| format!("parse snapshot {path}: {e}"))?;
            pg_grammar::compile_project(&snapshot)
                .map(|(g, _)| g)
                .map_err(|e| format!("compile {path}: {e:?}"))
        }
        "fwdata" => {
            let (snapshot, _) = pg_fwdata::import_file(Path::new(path))
                .map_err(|e| format!("import {path}: {e}"))?;
            pg_grammar::compile_project(&snapshot)
                .map(|(g, _)| g)
                .map_err(|e| format!("compile {path}: {e:?}"))
        }
        _ => {
            let xml = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
            pg_grammar::load(&xml).map_err(|e| format!("load {path}: {e:?}"))
        }
    }
}

/// Raw structural statistics, printed alongside the census so a change in the family set is
/// attributable to a grammar fact rather than to an opaque predicate. These are counts of what is
/// IN the grammar; none of them is a second derivation of an applicability predicate.
fn print_facts(g: &Grammar) {
    let prules = prules_in_order(g);
    let gated = find_gated_subrules(g, &prules);

    let subrules_with_any_restriction: usize = g
        .prules
        .iter()
        .map(|rule| match rule {
            PhonRuleDef::Rewrite(rewrite) => rewrite
                .subrules
                .iter()
                .filter(|sr| {
                    sr.required_pos.is_some()
                        || !sr.required_mpr.is_empty()
                        || !sr.excluded_mpr.is_empty()
                })
                .count(),
            PhonRuleDef::Metathesis(_) => 0,
        })
        .sum();

    let mut allomorphs = 0usize;
    let mut non_implicit_hint = 0usize;
    let mut copies_exceed_lhs = 0usize;
    let mut true_redup = 0usize;
    for rule in &g.mrules {
        let allos = match rule {
            MorphRuleDef::AffixProcess(def) => &def.allomorphs,
            MorphRuleDef::Realizational(def) => &def.allomorphs,
            MorphRuleDef::Compounding(_) => continue,
        };
        for allo in allos {
            allomorphs += 1;
            if !matches!(allo.redup_hint, ReduplicationHint::Implicit) {
                non_implicit_hint += 1;
            }
            let copies = allo
                .rhs
                .iter()
                .filter(|a| matches!(a, pg_grammar::model::OutputAction::Copy(_)))
                .count();
            if copies > allo.lhs.len() {
                copies_exceed_lhs += 1;
            }
            if rhs_has_true_reduplication(&allo.rhs) {
                true_redup += 1;
            }
        }
    }

    println!("  facts:");
    println!(
        "    entries={} mrules={} prules={} (in cascade: {}) templates={} strata={}",
        g.entries.len(),
        g.mrules.len(),
        g.prules.len(),
        prules.len(),
        g.templates.len(),
        g.strata.len()
    );
    println!(
        "    mpr_features={} gated_subrules(real mechanism)={} subrules_with_any_restriction={}",
        g.mpr_features.len(),
        gated.len(),
        subrules_with_any_restriction
    );
    println!(
        "    allomorphs={allomorphs} non_implicit_redup_hint={non_implicit_hint} \
         copies>lhs={copies_exceed_lhs} true_reduplication={true_redup}"
    );
}

fn census(path: &str, budget: u64) -> Result<(), String> {
    println!("== {path}");
    let g = load(path)?;
    print_facts(&g);

    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let prules = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let baseline = enumerate_default(&g, &alphabet, &prules, phon.as_ref());

    let registry = Registry::seeded();
    registry.validate_ready().map_err(|e| e.to_string())?;

    let offered = registry
        .instances_for_grammar(&g)
        .into_iter()
        .map(|instance| instance.family_id)
        .collect::<BTreeSet<_>>();

    let characterization =
        characterize(&g, &registry, &baseline, budget, 0).map_err(|e| e.to_string())?;

    println!("  census:");
    println!(
        "    families offered ({}): {}",
        offered.len(),
        offered.iter().cloned().collect::<Vec<_>>().join(", ")
    );
    println!(
        "    statically admissible instances: {}",
        characterization.counts.statically_admissible.value
    );
    println!(
        "    DISTINCT candidates materialized: {} ({})",
        characterization.distinct_roots.len(),
        match characterization.counts.feasible {
            FeasibleCount::Exact { .. } => "exact -- whole static space materialized".to_owned(),
            FeasibleCount::Estimate { sample_size, .. } =>
                format!("lower bound only -- budget {budget} sampled {sample_size}"),
        }
    );
    Ok(())
}

fn main() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut budget = u64::MAX;
    if let Some(pos) = args.iter().position(|a| a == "--budget") {
        budget = args
            .get(pos + 1)
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("--budget needs a number"));
        args.drain(pos..=pos + 1);
    }
    if args.is_empty() {
        eprintln!(
            "usage: recipe_candidate_census [--budget N] <grammar.(xml|json|fwdata)> [more ...]"
        );
        std::process::exit(2);
    }
    let mut failed = false;
    for path in &args {
        if let Err(error) = census(path, budget) {
            eprintln!("  ERROR {path}: {error}");
            failed = true;
        }
    }
    if failed {
        std::process::exit(1);
    }
}
