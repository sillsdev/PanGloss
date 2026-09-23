//! Measurement-only census of Aweti's composite enumeration and where entries come from
//! (builder, chain depth, rule, root). Both tests are `#[ignore]`d corpus measurements, run
//! via `pg.ps1 -Mode corpus-test -Package pg-foma -Filter aweti_enum_census`.

use super::*;
use crate::morphotactics::{ExploreMode, MorphotacticIndex};
use pg_conformance_fixtures::corpus;
use pg_grammar::model::Grammar;

/// Same 1 GiB dedicated stack `pg-cli`'s own `main()` uses; Aweti's compile-path recursion has overflowed default test-thread stacks before.
fn on_big_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(f)
        .expect("spawn census thread")
        .join()
        .expect("census thread panicked");
}

/// Resolves via `corpus::path` so `PANGLOSS_CORPUS_ROOT` is honored (unlike this crate's older gates, which hardcode the relative path).
fn load_aweti() -> Option<Grammar> {
    let Some(path) = corpus::path("aweti.json") else {
        assert!(
            !corpus::required(),
            "PANGLOSS_CORPUS_REQUIRED is set but aweti.json is not present in the corpus root"
        );
        eprintln!("skipping: aweti.json not present (set PANGLOSS_CORPUS_ROOT)");
        return None;
    };
    let json =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let snapshot = pg_snapshot::Snapshot::from_json(&json)
        .unwrap_or_else(|e| panic!("parse aweti.json snapshot: {e}"));
    let (g, _warnings) = pg_grammar::compile_project(&snapshot)
        .unwrap_or_else(|e| panic!("compile aweti project: {e}"));
    Some(g)
}

/// Runs both composite builders to completion unbounded (builders only -- no lexc/foma/apply_up hazards) and attributes the entry count.
#[test]
#[ignore = "measurement-only: needs private corpus data (PANGLOSS_CORPUS_ROOT); run via -Mode corpus-test"]
fn aweti_enum_census_uncapped_totals() {
    on_big_stack(|| {
        let Some(g) = load_aweti() else { return };
        let width = tags::tag_width(g.morphemes.len());
        let phon = PhonologyProbe::new(&g);
        let mt = MorphotacticIndex::build(&g);
        let t0 = Instant::now();
        let (recs, report) = crate::preexpand::build_composites_with_mode(
            &g,
            width,
            phon.as_ref(),
            &mt,
            ExploreMode::Pruned,
            None,
        );
        println!(
            "[census] preexpand done in {:?}: pairs_probed={} by_depth={:?} synth_successes={} interdigitation={} fusion={}",
            t0.elapsed(),
            report.pairs_probed,
            report.pairs_probed_by_depth,
            report.synth_successes,
            report.interdigitation_entries,
            report.fusion_entries
        );

        // Attribution: entries by chain length (extra rules beyond the root).
        let mut by_len: std::collections::BTreeMap<usize, usize> =
            std::collections::BTreeMap::new();
        let mut by_root: rustc_hash::FxHashMap<u32, usize> = rustc_hash::FxHashMap::default();
        let mut by_rule: rustc_hash::FxHashMap<u32, usize> = rustc_hash::FxHashMap::default();
        let mut total_variants = 0usize;
        let mut surface_tags: rustc_hash::FxHashMap<String, usize> =
            rustc_hash::FxHashMap::default();
        for r in &recs {
            *by_len
                .entry(r.chain_morphemes.len().saturating_sub(1))
                .or_insert(0) += 1;
            if let Some((_, root)) = r.chain_morphemes.first() {
                *by_root.entry(root.0).or_insert(0) += 1;
            }
            for (is_root, m) in &r.chain_morphemes {
                if !*is_root {
                    *by_rule.entry(m.0).or_insert(0) += 1;
                }
            }
            total_variants += r.variants.len();
            for v in &r.variants {
                *surface_tags.entry(v.clone()).or_insert(0) += 1;
            }
        }
        println!("[census] fusion recs by extra-rule count: {by_len:?}");
        println!(
            "[census] total variant lines={} distinct surfaces={} roots contributing={}",
            total_variants,
            surface_tags.len(),
            by_root.len()
        );
        let mut top_roots: Vec<_> = by_root.into_iter().collect();
        top_roots.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
        println!(
            "[census] top 10 roots by entry count: {:?}",
            &top_roots[..top_roots.len().min(10)]
        );
        let mut top_rules: Vec<_> = by_rule.into_iter().collect();
        top_rules.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
        println!(
            "[census] top 15 rules by chain membership: {:?}",
            &top_rules[..top_rules.len().min(15)]
        );
        let dup_surfaces = surface_tags.values().filter(|&&n| n > 1).count();
        println!(
            "[census] surfaces appearing under >1 (tag,surface) record: {dup_surfaces} (these carry DIFFERENT tag chains -- distinct analyses, not dedupable)"
        );
        drop(surface_tags);

        let fusion_total = report.interdigitation_entries + report.fusion_entries;
        println!("[census] preexpand fusion+interdigitation total = {fusion_total}");
        corpus::record_cases("aweti-enum-census-uncapped", 1);
    });
}

/// The structural half of the uncapped census, its own test so each half clears the workspace nextest hang ceiling with margin.
#[test]
#[ignore = "measurement-only: needs private corpus data (PANGLOSS_CORPUS_ROOT); run via -Mode corpus-test"]
fn aweti_enum_census_uncapped_structural() {
    on_big_stack(|| {
        let Some(g) = load_aweti() else { return };
        let width = tags::tag_width(g.morphemes.len());
        let mt = MorphotacticIndex::build(&g);
        let t1 = Instant::now();
        let struct_rules = structural_candidate_rules(&g);
        let cache = RuleCache::build(&g);
        let morpher = Morpher::new(&g, usize::MAX);
        let (srecs, _covered, _pending_successors, _pending_rules, _candidate_pairs_pruned) =
            build_structural_composites(
                &g,
                width,
                &struct_rules,
                &cache,
                &morpher,
                &mt,
                ExploreMode::Pruned,
                None,
                None,
            );
        let mut s_by_len: std::collections::BTreeMap<usize, usize> =
            std::collections::BTreeMap::new();
        for r in &srecs {
            *s_by_len
                .entry(r.chain_morphemes.len().saturating_sub(1))
                .or_insert(0) += 1;
        }
        println!(
            "[census] structural done in {:?}: candidates={} probe_would_refuse={} entries={} by extra-rule count {s_by_len:?}",
            t1.elapsed(),
            struct_rules.len(),
            probe_would_refuse(&g),
            srecs.len()
        );
        println!("[census] structural entries = {}", srecs.len());
        corpus::record_cases("aweti-enum-census-uncapped-structural", 1);
    });
}
