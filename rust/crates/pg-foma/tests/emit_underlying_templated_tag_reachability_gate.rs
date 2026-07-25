//! Gate for `pg_foma::emit::verify_tags_reachable` (`emit.rs`'s own doc on that function): the
//! post-emission detection added to close a SILENT recall-loss class in
//! [`pg_foma::emit::emit_underlying_templated`].
//!
//! ## Background (2026-07-25 regression investigation)
//! `tests/p6_templated_morphotactics_gate.rs`'s own `BASELINE_MISSES` doc found that Aweti's
//! `mrule105` (a standalone `AffixProcess` rule on a stratum ABOVE the root/template stratum) was
//! correctly classified, declared, and had its lexicon entries written -- yet its tag was entirely
//! absent from the compiled lexc network's own sigma, and NOTHING reported this: pure silent recall
//! loss, undetectable from `EmitReport` alone.
//!
//! ## Why the trigger here is "many stratum-attached standalone rules", not literally "stratum > 0"
//! A synthetic reproduction (this file) shows the underlying mechanism is NOT a graph-reachability
//! bug in this crate's own `lexc_source` construction (every `LEXICON` block here really is wired
//! in from `Root` by construction -- `build_deriv_chain`'s every level has an unconditional bare
//! skip arc to the next). It reproduces identically on BOTH `emit_underlying_templated`'s
//! `TextMode::UnderlyingTokens` strategy and `pg_foma::emit::emit`'s legacy `TextMode::
//! SurfaceProbed` strategy, even though the two write completely different per-level content --
//! and the affected tag set is a scattered, non-monotonic subset (not "every level past some
//! depth"). This is consistent with a state-deduplication quirk in the vendored `foma` crate's own
//! lexc reader (`foma::lexcread`'s `lexc_merge_states`/`lexc_suffix_hash`) at scale, not a defect
//! in this crate's own structural wiring -- see `emit.rs`'s `verify_tags_reachable` doc for the
//! full reasoning. Whatever the exact mechanism, the OUTCOME (a declared, fully-wired tag silently
//! missing from the compiled net) is real and reproducible, which is what this gate pins.
//!
//! Kept as a small, fast, delanguaged synthetic fixture (via `pg_grammar_gen`, this crate's own
//! stress-grammar generator) specifically so this gate runs in ordinary CI -- unlike
//! `p6_templated_morphotactics_gate.rs`, it needs no gitignored corpus data at all.

use pg_foma::emit::emit_underlying_templated;
use pg_foma::replace::SegAlphabet;
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

const UNREACHABLE_KIND: &str = "unreachable-after-lexc-compile";

/// Many (24) independent stratum-attached standalone prefix rules (one per extra stratum, mirroring
/// `pg_foma::emit::emit`'s own `phase_c_strata_depth.rs` gate's construct, but at a scale that
/// gate's own `extra_strata: 3` never reaches) layered OVER a templated stratum 0 (one circumfix
/// rule wrapped in a single-slot `AffixTemplate`, `ConstructKnobs::circumfix_count`) -- the exact
/// "standalone rule on a stratum above a templated root stratum" shape the module doc's background
/// section describes, built at Aweti's own real per-zone rule-count scale (`docs/fst-plan/
/// p6-deep-truncation-chain-report.md`'s "11-rule prefix / 24-rule suffix" figure) so the vendored
/// foma lexc-compiler quirk this gate pins actually fires (verified directly: it does not fire at
/// `extra_strata: 1`, only once there are enough structurally-similar chained levels).
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

/// (1) Detection FIRES: a synthetic, delanguaged multi-stratum grammar where a stratum-N (N > 0)
/// standalone rule's tag is declared but genuinely unreachable in the compiled net must now surface
/// through `EmitReport::uncovered` under `UNREACHABLE_KIND`, never silently.
#[test]
fn detection_fires_on_stratum_scale_gap() {
    let recipe = stratum_scale_recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml)
        .unwrap_or_else(|e| panic!("generated XML failed to load: {e}\n{}", rendered.xml));
    assert_eq!(g.strata.len(), 25, "1 base stratum + 24 extra");
    assert_eq!(rendered.extra_strata.len(), 24);

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let result = emit_underlying_templated(&g, &alphabet, None);

    // Sanity: this recipe's lexc source must still compile (the detection itself depends on a
    // successful compile; a compile failure would be a different, already-loud problem).
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
        !unreachable.is_empty(),
        "expected at least one {UNREACHABLE_KIND:?} finding for this stratum-scale recipe (a real, \
         reproducible gap this gate exists to catch); full uncovered list: {:?}",
        result.report.uncovered
    );
    for u in &unreachable {
        assert!(
            u.reason.contains("Root") || u.reason.to_lowercase().contains("unreachable"),
            "unreachable-after-lexc-compile reason should name the reachability problem plainly: {:?}",
            u.reason
        );
        println!("detected: [{}] {} -- {}", u.kind, u.id, u.reason);
    }
}

/// (2) No false positive: an ORDINARY single-stratum templated grammar (this crate's own GATE 2
/// circumfix recipe, `tests/phase_c_circumfix.rs`'s exact recipe shape -- small, no extra strata)
/// must report ZERO `UNREACHABLE_KIND` findings. Every declared tag in a grammar this small is
/// genuinely reachable (verified independently by that gate's own 100%-recall assertion), so the
/// new detection must stay silent here -- proving it does not fire on ordinary, healthy grammars.
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
