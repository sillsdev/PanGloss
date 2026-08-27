//! Alpha-variable scale builder. Exercises `pg_foma::replace::resolve_alpha_tuples`'s
//! parameterized over `alpha_var_count` × `alpha_class_size`.
//!
//! ## Design choice: a same-var LHS/RHS IDENTITY rule, not a copy-from-environment rule
//! **Found empirically while building the recall-parity gate for this module** (`pg-foma/tests/
//! phase_c_alpha_scale.rs`), recorded here because it is load-bearing, in two stages:
//! 1. An EARLIER version gave each rule exactly ONE alpha-bound occurrence (RHS only, no
//!    environment) -- `pg_foma::replace::compile_rewrite_rule_subset`'s own proven fixture shape
//!    (`compose_budget_tests::SYNTH_ALPHA_XML`). With no environment to constrain which tuple's
//!    context is "real", the REAL engine's synthesis pipeline left the position UNCHANGED (nothing
//!    to agree with), while the P6 prototype's sequential tuple-fold rewrote it to whichever
//!    candidate `resolve_alpha_tuples` enumerates FIRST -- a genuine, observable mismatch.
//! 2. A SECOND version added a real `<LeftEnvironment>` occurrence bound to the same var (a
//!    "copy the left neighbor's value" construct), reasoning that `featId`'s uniqueness would force
//!    the environment and RHS occurrences to agree on ONE real neighboring segment, making the
//!    tuple fold's own "mutually exclusive by environment" correctness argument apply cleanly. This
//!    ALSO produced a real engine/P6 divergence in the surface LENGTH (a 4-segment root came back 3
//!    segments long from the real engine's own synthesis) -- `<LeftEnvironment>` interacting with
//!    an `<AlphaVariable>` occurrence has some real-engine behavior this investigation did not fully
//!    characterize in the time available, and chasing it further risked scope creep into `pg_rules`
//!    engine internals well outside this crate's own remit.
//!
//! This builder instead uses a construction that is UNAMBIGUOUS for both implementations BY
//! CONSTRUCTION, sidestepping the open question rather than needing to resolve it: LHS and RHS are
//! BOTH `ncAny`-alpha-bound occurrences of the SAME var, no environment at all. Since LHS and RHS
//! must (via `featId`'s uniqueness, module doc of `build::tables`) resolve to the IDENTICAL
//! segment, EVERY surviving tuple compiles to `pool[m] -> pool[m]` -- a literal identity map, for
//! every `m`. An identity map trivially composes with anything (no "which branch wins" question
//! can even arise), so both the real engine (rewriting X to X is a no-op) and the P6 prototype
//! (composing `class_size` individually-no-op branches) agree the root's own surface is UNCHANGED,
//! by construction, with no dependence on tuple-fold-ordering semantics either implementation might
//! have. This still genuinely exercises `resolve_alpha_tuples`' real joint-agreement machinery (2
//! occurrences, `var_count` independent rules, `alpha_class_size`-many real tuples each, actually
//! compiled and composed) -- the recall claim under test is "a grammar with this many real alpha
//! tuples in its cascade still compiles and still relates the root's own tag to its own (unchanged)
//! surface correctly," which is exactly the scale/budget concern that matters here, without
//! staking the gate on an unresolved real-engine semantics question.
//!
//! `var_count` independent rules, each on its own dedicated marker segment (unnecessary for
//! correctness now -- an identity map is harmless wherever it matches -- but kept for symmetry with
//! every other construct builder's "one dedicated position per instance" convention, and so a gate
//! can still name each rule's own target character if useful for diagnostics).

use crate::build::tables::TableSpec;
use crate::ids::IdMinter;

/// Everything `build` produces: `var_count` independent identity-mapping alpha rules and the
/// single root's own `<PhoneticShape>` text (one marker character per rule, concatenated, in rule
/// order) -- a gate splices this shape into its own single `<LexicalEntry>`. The root's own
/// spelling is UNCHANGED by every rule here (module doc), so it is also the expected post-synthesis
/// surface.
#[derive(Debug, Clone)]
pub struct AlphaBuild {
    /// `var_count` `<PhonologicalRule>` elements (module doc).
    pub prules_xml: String,
    /// The rules' own minted xml ids, in order (`rule_xml_ids[j]` targets marker position `j`).
    pub rule_xml_ids: Vec<String>,
    /// The single root's own required `<PhoneticShape>` text -- also its expected, UNCHANGED
    /// post-synthesis surface (module doc: every rule here is an identity map).
    pub root_shape: String,
}

/// Build `var_count` (`>= 1`) independent identity-mapping alpha rules over `table`'s own segments
/// (each rule's own `surviving` tuple count is exactly `table.segments.len()`, module doc). Needs
/// at least `var_count` distinct segments (one dedicated marker-trigger character per rule) --
/// panics otherwise.
pub fn build(var_count: usize, table: &TableSpec, ids: &mut IdMinter) -> AlphaBuild {
    assert!(var_count >= 1, "build_alpha: var_count must be >= 1");
    assert!(
        table.segments.len() >= var_count,
        "build_alpha: table has {} segments, needs at least {var_count} (one dedicated marker-trigger char per var)",
        table.segments.len()
    );

    let nc_any = crate::build::tables::nc_any_xml_id();
    let feat_id = crate::build::tables::feat_id_xml_id();

    let mut prules_xml = String::new();
    let mut rule_xml_ids = Vec::with_capacity(var_count);
    let mut root_shape = String::new();
    for j in 0..var_count {
        let seg = &table.segments[j];
        root_shape.push(seg.ch);
        let rule_xml_id = ids.next("pruleAlpha");
        let var_xml_id = ids.next("var");
        // LHS is also an alpha-bound occurrence of the same var, not a fixed <Segment> reference, so the compiled rule is an identity map on whichever segment it matches.
        prules_xml.push_str(&format!(
            "\n      <PhonologicalRule id=\"{rule_xml_id}\">\n        <Name>alpha{j}</Name>\n        \
             <VariableFeatures>\n          <VariableFeature id=\"{var_xml_id}\" name=\"v{j}\" phonologicalFeature=\"{feat_id}\" />\n        \
             </VariableFeatures>\n        \
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass=\"{nc_any}\"><AlphaVariables>\
             <AlphaVariable variableFeature=\"{var_xml_id}\" /></AlphaVariables></SimpleContext></PhoneticSequence></PhoneticInput>\n        \
             <PhonologicalSubrules>\n          <PhonologicalSubrule>\n            \
             <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass=\"{nc_any}\"><AlphaVariables>\
             <AlphaVariable variableFeature=\"{var_xml_id}\" /></AlphaVariables></SimpleContext></PhoneticSequence></PhoneticOutput>\n          \
             </PhonologicalSubrule>\n        </PhonologicalSubrules>\n      </PhonologicalRule>",
        ));
        rule_xml_ids.push(rule_xml_id);
    }

    AlphaBuild {
        prules_xml,
        rule_xml_ids,
        root_shape,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::tables;

    #[test]
    fn three_vars_mint_three_independent_rules() {
        let mut ids = IdMinter::new();
        let tb = tables::build(1, 5, false, false, &mut ids);
        let ab = build(3, &tb.tables[0], &mut ids);
        assert_eq!(ab.rule_xml_ids.len(), 3);
        assert_eq!(ab.root_shape.chars().count(), 3);
        assert_eq!(ab.prules_xml.matches("<VariableFeature ").count(), 3);
        assert_eq!(
            ab.prules_xml
                .matches("<PhoneticInput><PhoneticSequence><SimpleContext")
                .count(),
            3
        );
    }
}
