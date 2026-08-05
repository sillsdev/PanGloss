//! Gate for `pg_foma::emit::verify_tags_reachable` (`emit.rs`'s own doc on that function): the
//! post-emission detection originally added to close what was BELIEVED to be a silent recall-loss
//! class in [`pg_foma::emit::emit_underlying_templated`].
//!
//! ## Background
//! `tests/p6_templated_morphotactics_gate.rs`'s own `BASELINE_MISSES` doc found that Aweti's
//! `mrule105` (a standalone `AffixProcess` rule on a stratum ABOVE the root/template stratum) was
//! correctly classified, declared, and had its lexicon entries written -- yet its tag was entirely
//! absent from the compiled lexc network's own sigma, and NOTHING reported this.
//!
//! ## Root cause: NOT `lexc_merge_states`, and NOT recall loss
//! A follow-up investigation built a minimal reproduction depending on nothing but the `foma`
//! crate (no PanGloss code at all) and found the true mechanism: `foma::lexcread`'s own
//! `lexc_string_to_tokens` (the ENTRY-text tokenizer) fails to recognize a declared multichar
//! symbol as one atomic token whenever its name contains a literal `0` digit (spelled `%0` in lexc
//! source, since a bare `0` is the alignment epsilon) -- a declaration-vs-tokenization
//! `@ZERO@`-marker normalization-order bug, filed upstream as `divvun/foma-rs` (see that issue for
//! the exact repro and a comparison showing the original C foma reader does not have this defect).
//! This reproduces with a SINGLE `Multichar_Symbols` declaration and a SINGLE entry -- no chaining,
//! no scale, and no synthetic states for `lexc_merge_states` to act on at all, so the "many
//! stratum-attached standalone rules" shape below is NOT what triggers it; what actually explains
//! the previously-observed "scattered, non-monotonic" tag set is simpler: it is exactly the set of
//! tags whose zero-padded numeral text happens to contain a `0` digit (`tags::tag_width` only
//! zero-pads once the grammar has more than 10 morphemes, so this affects most real grammars, not
//! specifically deep/chained ones).
//!
//! Verified directly via `foma::apply::apply_down` (not assumed): every tag this gate used to
//! flag is still reachable at the LANGUAGE level -- the network's actual recognized strings are
//! unaffected, only `sigma`'s bookkeeping is incomplete for these symbols. `emit.rs`'s
//! `verify_tags_reachable` was corrected accordingly (it no longer flags a tag absent from `sigma`
//! when the tag text contains `0` and every one of its individual characters IS present in
//! `sigma` -- the signature of this exact decomposition artifact), so this recipe below now
//! correctly reports ZERO `unreachable-after-lexc-compile` findings; see
//! `detection_does_not_false_positive_on_the_historical_zero_escape_shape` below. The detector
//! itself is KEPT (not deleted) as a safety net for a genuinely different future defect -- see
//! `emit.rs`'s doc on `verify_tags_reachable` for the full reasoning.
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
/// section describes, built at Aweti's own real per-zone rule-count scale ("11-rule prefix /
/// 24-rule suffix"). This shape is
/// kept as the HISTORICAL recipe that used to trip the `%0`-escape sigma artifact (module doc's
/// correction) -- 24 morphemes pushes `tags::tag_width` to 2 digits, so ids 0-10 and 20 all get a
/// zero-padded `0` in their tag text -- not because chaining/scale is actually load-bearing for the
/// artifact (it isn't; the minimal upstream repro needs neither).
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

/// (1) No false positive on the HISTORICAL trigger shape: this recipe used to make
/// `verify_tags_reachable` report several tags (any whose zero-padded numeral text contains a `0`
/// digit) as `UNREACHABLE_KIND`, but a follow-up investigation proved (via
/// `foma::apply::apply_down` against the actual compiled net, both for this recipe and for a
/// foma-crate-only minimal reproduction at the same "many chained levels" scale) that every one of
/// those tags is genuinely reachable at the LANGUAGE level -- only the vendored `foma` crate's
/// `sigma` bookkeeping was incomplete for them (a narrow upstream bug, filed as `divvun/foma-rs`;
/// see `emit.rs`'s `verify_tags_reachable` doc). `verify_tags_reachable` was corrected to recognize
/// this exact artifact (tag text contains `0` AND every individual character of it IS present in
/// `sigma`) and no longer flag it, so this recipe must now report ZERO `UNREACHABLE_KIND` findings.
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
