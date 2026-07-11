//! Port of C# `RuleInverseCompilerTests.cs` (I1 of `FST_FULL_GRAMMAR_PLAN.md`), F7's "ported test
//! classes" gate. Two of C#'s nine methods are `[Explicit]` manual real-grammar diagnostics
//! (`TierReport_OnRealGrammar`, `TierGate_OnRealGrammar_MatchesRecordedCounts`) -- their runnable
//! substance (Indonesian/Amharic tier counts + reasons, byte-identical) is already covered by
//! `compiler.rs`'s own `indonesian_tier_report_matches_golden`/`amharic_tier_report_matches_golden`
//! inline tests, so those two are COVERED, not re-ported here.
//!
//! The remaining seven build each rule IN CODE against a shared 24-segment test table; this port
//! authors one XML fixture instead (`RuleInverseCompilerToyGrammar.xml`, see its own header) and a
//! `run_pinv` standalone interpreter mirroring C#'s own `RunPinv`/`Closure`/`Render` test-local
//! helpers exactly (same closure-over-epsilon-arcs algorithm, same "same lanes on both sides of an
//! arc -> render the concretely-matched segment" rule in place of C#'s `ReferenceEquals` check --
//! see `run_pinv`'s own doc for why value-equality is the correct substitute here).
//!
//! `CompileMetathesisRule_BroadSwitchClasses_ExceedsComboCap_DowngradesHonestly` is a DEFERRED gap,
//! not a full port: `compiler.rs`'s `compile_metathesis_stub` (documented at that function's own
//! site) treats every `MetathesisRuleDef` as an unconditional `IdentitySkip` regardless of its
//! switch-class breadth, so the combo-cap-trips-before-probing behavior C#'s test pins does not
//! exist in this port at all yet (metathesis probing itself is out of scope this milestone -- zero
//! observable impact on Indonesian/Sena/Amharic/the toy gate, none of which declares a
//! `MetathesisRule`). `metathesis_rule_is_the_documented_identityskip_stub` below asserts what
//! ACTUALLY happens (tier + reason + identity-only contract), not what C#'s finer-grained test
//! expects -- the gap between them is exactly the deferred scope, recorded here rather than
//! silently deleted.

use hc_featstruct::flat_unifiable;
use hc_grammar::chardef::{CharDefKind, CharDefTable};
use hc_grammar::model::Grammar;
use hc_hybrid::compiler::{self, RuleInverseTier};
use hc_hybrid::inverse::{InversePhonology, StateId};

fn load() -> Grammar {
    let xml = include_str!("fixtures/fst-advisor-toys/RuleInverseCompilerToyGrammar.xml");
    hc_grammar::load(xml).unwrap_or_else(|e| panic!("toy grammar failed to load: {e}"))
}

fn rule<'a>(g: &'a Grammar, compiled: &'a [compiler::CompiledRuleInverse], name: &str) -> &'a compiler::CompiledRuleInverse {
    let _ = g;
    compiled.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no compiled rule named {name:?}"))
}

/// Standalone interpreter mirroring C#'s test-local `RunPinv` (no lexicon/trie involved -- I1's own
/// tests predate the I2 walker). Segments `surface` against `table`, walks `pinv`'s arcs closing
/// over epsilon (structural + ε-input restoration) at every step, and renders every accepted
/// underlying reading back to a string; returns `[]` for a surface the table can't even segment.
fn run_pinv(g: &Grammar, table: &CharDefTable, pinv: &InversePhonology, surface: &str) -> Vec<String> {
    let Ok(shape) = hc_rules::shape_feat::segment_with_features(g, table, surface) else {
        return Vec::new();
    };
    let segs: Vec<Vec<u64>> = shape
        .interior()
        .filter(|&(_, kind, _, _)| kind == hc_shape::NodeKind::Segment)
        .map(|(i, _, _, _)| shape.node_lanes(i).to_vec())
        .collect();

    let mut current: Vec<(StateId, Vec<Vec<u64>>)> = closure(pinv, vec![(pinv.start_state, Vec::new())]);
    for seg in &segs {
        let mut next: Vec<(StateId, Vec<Vec<u64>>)> = Vec::new();
        for (state, underlying) in &current {
            for arc in pinv.arcs_from(*state) {
                if arc.is_epsilon_input() || !arc.surface_unifiable(seg) {
                    continue; // ε-input arcs are taken in the closure; this arc must consume a segment
                }
                let mut u2 = underlying.clone();
                if let Some(u) = &arc.underlying {
                    // C# checks ReferenceEquals(SurfaceInput, UnderlyingOutput) to detect an
                    // identity/class-constraint pass-through arc (env_nfa's own identity arcs push
                    // the SAME FeatureStruct instance on both sides, possibly underspecified) vs a
                    // genuine restoration/substitution arc (different lanes on each side). This
                    // port has no object-identity tracking for `Vec<u64>` lane rows, but VALUE
                    // equality is behavior-identical here: an identity arc's surface/underlying
                    // lanes are literally the same vector (built from the same source node in
                    // env_nfa.rs), and a real substitution arc's lanes differ by construction
                    // (that's what makes it a substitution). Render the concretely-matched
                    // segment for an identity arc (the underspecified constraint unifies with it,
                    // same as C#'s walk-time unification against the lexicon); render the arc's
                    // own (possibly more specific) underlying lanes otherwise.
                    u2.push(if arc.surface.as_ref() == Some(u) { seg.clone() } else { u.clone() });
                }
                next.push((arc.target, u2));
            }
        }
        current = closure(pinv, next);
    }

    let mut results: Vec<String> = current
        .into_iter()
        .filter(|(state, _)| pinv.is_accepting(*state))
        .map(|(_, underlying)| render(table, &underlying))
        .collect();
    results.sort();
    results.dedup();
    results
}

/// Closure over ε-input (restoration) and structural-epsilon arcs, matching C#'s own bounded
/// (state, rendered-so-far) visited set exactly (same rationale: two distinct readings can land at
/// the same state with the same segment count and must not collapse).
fn closure(pinv: &InversePhonology, configs: Vec<(StateId, Vec<Vec<u64>>)>) -> Vec<(StateId, Vec<Vec<u64>>)> {
    let mut visited: rustc_hash::FxHashSet<(StateId, Vec<Vec<u64>>)> = rustc_hash::FxHashSet::default();
    let mut result = Vec::new();
    let mut stack = Vec::new();
    for (state, underlying) in configs {
        if visited.insert((state, underlying.clone())) {
            result.push((state, underlying.clone()));
            stack.push((state, underlying));
        }
    }
    while let Some((state, underlying)) = stack.pop() {
        for arc in pinv.arcs_from(state) {
            if !arc.is_epsilon_input() {
                continue; // consumes a real surface segment -- not part of closure
            }
            let mut u2 = underlying.clone();
            if let Some(u) = &arc.underlying {
                u2.push(u.clone());
            }
            if visited.insert((arc.target, u2.clone())) {
                result.push((arc.target, u2.clone()));
                stack.push((arc.target, u2));
            }
        }
    }
    result
}

fn render(table: &CharDefTable, segments: &[Vec<u64>]) -> String {
    let mut s = String::new();
    for lanes in segments {
        match table.iter().find(|(_, cd)| cd.kind() == CharDefKind::Segment && flat_unifiable(cd.feature_lanes(), lanes) && cd.feature_lanes() == lanes.as_slice()) {
            Some((_, cd)) => s.push_str(&cd.representations()[0]),
            None => s.push('?'),
        }
    }
    s
}

#[test]
fn compile_plain_substitution_no_environment_is_exact() {
    let g = load();
    let table = &g.char_tables[0];
    let compiled = compiler::compile_default(&g);
    let result = rule(&g, &compiled, "t_to_d");
    assert_eq!(result.tier, RuleInverseTier::Exact, "{:?}", result.reasons);
    assert!(result.reasons.is_empty());

    assert!(run_pinv(&g, table, &result.pinv, "d").contains(&"t".to_string()));
    assert!(run_pinv(&g, table, &result.pinv, "g").contains(&"g".to_string()), "unrelated segment: identity only");
}

#[test]
fn compile_left_and_right_environment_is_exact() {
    let g = load();
    let table = &g.char_tables[0];
    let compiled = compiler::compile_default(&g);
    let result = rule(&g, &compiled, "t_to_d_env");
    assert_eq!(result.tier, RuleInverseTier::Exact, "{:?}", result.reasons);

    assert!(run_pinv(&g, table, &result.pinv, "gdd").contains(&"gtd".to_string()));
    assert_eq!(
        run_pinv(&g, table, &result.pinv, "gdg"),
        vec!["gdg".to_string()],
        "soundness: no right-env 'd' follows, so no restoration branch may fire -- identity only"
    );
}

#[test]
fn compile_quantified_environment_span_long_distance_harmony_is_exact() {
    let g = load();
    let table = &g.char_tables[0];
    let compiled = compiler::compile_default(&g);
    let result = rule(&g, &compiled, "t_to_d_harmony");
    assert_eq!(result.tier, RuleInverseTier::Exact, "quantified env spans must NOT be relegated to Permissive: {:?}", result.reasons);

    assert!(run_pinv(&g, table, &result.pinv, "dggd").contains(&"tggd".to_string()), "2-segment span");
    assert!(run_pinv(&g, table, &result.pinv, "dgggd").contains(&"tgggd".to_string()), "3-segment span: genuinely unbounded");
    assert_eq!(
        run_pinv(&g, table, &result.pinv, "dggb"),
        vec!["dggb".to_string()],
        "soundness: no trailing 'd' anywhere -- no restoration, identity only"
    );
}

#[test]
fn compile_alpha_variable_assimilation_is_permissive_but_recovers_one_representative() {
    let g = load();
    let table = &g.char_tables[0];
    let compiled = compiler::compile_default(&g);
    let result = rule(&g, &compiled, "nasal_assim");
    assert_eq!(result.tier, RuleInverseTier::Permissive);
    assert!(result.reasons.contains(&"alpha-variable".to_string()));

    // "p" is the bilabial trigger; n (alveolar) and q ("velar" stand-in) assimilating to it both
    // become m -- the Pinv must recover both as alternate underlying readings of surface "mp",
    // alongside the trivial identity reading (m is already bilabial).
    let underlyings = run_pinv(&g, table, &result.pinv, "mp");
    assert!(underlyings.contains(&"np".to_string()), "{underlyings:?}");
    assert!(underlyings.contains(&"qp".to_string()), "{underlyings:?}");
    assert!(underlyings.contains(&"mp".to_string()), "{underlyings:?}");
}

#[test]
fn compile_two_segment_lhs_is_exact() {
    let g = load();
    let table = &g.char_tables[0];
    let compiled = compiler::compile_default(&g);
    let result = rule(&g, &compiled, "two_seg");
    assert_eq!(result.tier, RuleInverseTier::Exact, "{:?}", result.reasons);
    assert!(run_pinv(&g, table, &result.pinv, "db").contains(&"tg".to_string()));
}

#[test]
fn compile_right_to_left_rewrite_rule_is_permissive_reason_direction() {
    let g = load();
    let table = &g.char_tables[0];
    let compiled = compiler::compile_default(&g);
    let result = rule(&g, &compiled, "t_to_d_rtl");
    assert_eq!(result.tier, RuleInverseTier::Permissive);
    assert_eq!(result.reasons, vec!["direction".to_string()]);
    // The branch itself must still be a real, correct substitution -- "direction" only means the
    // chain's LTR-only walk order can't be trusted to match this rule's declared sweep direction,
    // not that the compiled arc is wrong.
    assert!(run_pinv(&g, table, &result.pinv, "d").contains(&"t".to_string()));
}

/// DEFERRED (see module doc): `compiler.rs`'s `compile_metathesis_stub` is an unconditional
/// `IdentitySkip`, so the combo-cap-trips-before-probing behavior C#'s
/// `CompileMetathesisRule_BroadSwitchClasses_ExceedsComboCap_DowngradesHonestly` pins does not
/// exist in this port. This asserts what the port ACTUALLY does instead: honest `IdentitySkip`,
/// reason `"metathesis-unported"` (not C#'s `"metathesis-too-many-combos"`), and the
/// `RuleInverseTier::IdentitySkip` contract itself (identity-only, not reject-all) still holds.
#[test]
fn metathesis_rule_is_the_documented_identityskip_stub_not_a_combo_cap_port() {
    let g = load();
    let table = &g.char_tables[0];
    let compiled = compiler::compile_default(&g);
    let result = rule(&g, &compiled, "broad_swap");
    assert_eq!(result.tier, RuleInverseTier::IdentitySkip);
    assert_eq!(result.reasons, vec!["metathesis-unported".to_string()]);
    assert_eq!(run_pinv(&g, table, &result.pinv, "tg"), vec!["tg".to_string()], "identity-only, not reject-all");
}
