//! A production-shaped convenience entry point into [`crate::capability::compose_envelope`], for
//! callers (`pg-cli`) that just want "the
//! `CompileDecision` for this `Grammar`" without hand-assembling `characterize`'s `enumerate_default`
//! inputs themselves.
//!
//! **Check-only, non-blocking** — same discipline as `crate::capability`'s own
//! top-doc: [`evaluate_capability`] only ever COMPUTES a [`crate::capability::CompileDecision`], it
//! does not consult one, and nothing in this module (or its caller, `pg-cli`) alters what
//! `emit.rs`/`gate.rs`/`replace.rs`/`preexpand.rs` actually compile. See ADR 0001 (`docs/adr/
//! 0001-honest-capability-boundary.md`) for why a `Refuse` is reported rather than silently
//! papered over. Whether a `CompileDecision` actually blocks/stamps a real compile path is a fact
//! about the call graph, not this module — grep for callers of [`evaluate_capability`] to check.
//!
//! # Mirroring the real compile setup
//! [`crate::emit::emit_with_budget`]'s own SETUP (its first few lines, before any lexc text is
//! written) builds exactly three things this function also needs, the same way:
//! - `table`/the segment alphabet: `emit_with_budget` calls [`crate::emit::surface_table`] (the
//!   LAST stratum's char-def table — that function's own doc) and wraps it in a
//!   [`crate::replace::SegAlphabet`] wherever the compose/gate seams need one (`crate::gate`/
//!   `crate::replace`'s own production call sites, mirrored by `crate::enumerate`'s test-module
//!   helpers this function also mirrors). [`evaluate_capability`] does the identical
//!   `SegAlphabet::new(surface_table(g))`.
//! - `phon`: `emit_with_budget` calls `PhonologyProbe::new(g)` directly; so does this function.
//! - `prules_in_order`: not a literal local in `emit_with_budget` itself (that function's mainline
//!   lexc-emission path doesn't build a `Replace` cascade at all — see `crate::enumerate`'s own
//!   module doc, "Judgment calls," for why `crate::gate`/`crate::replace`'s compose-seam prototype
//!   and `emit.rs`'s mainline lexc path are two separate compile entry points today), but every
//!   OTHER real construction site for this exact slice in this crate (`crate::gate::
//!   compile_gated_grammar_with_budget`, `crate::enumerate`'s and `crate::capability`'s own test
//!   modules) builds it the same way: `g`'s strata, in order, flattened over each stratum's own
//!   `phonologicalRules` id list, as literal borrows of `g.prules` (required for
//!   [`crate::enumerate::rule_id_of`]'s pointer-identity `PRuleId` recovery). That construction is
//!   now the shared [`crate::enumerate::prules_in_order`], which [`evaluate_capability`] calls —
//!   this module used to carry its own private copy of the three-line body, one of four identical
//!   production copies.
//!
//! `evaluate_capability` then hands all three to [`crate::enumerate::enumerate_default`] (Step 2,
//! `reify-compilation-plans`) to get the reified [`crate::plan::Plan`], and folds that together with
//! [`crate::capability::characterize`]'s profile via [`crate::capability::compose_envelope`] against
//! [`crate::capability::default_registry`] — the same two spines Step 2 of
//! `add-capability-characteristics-check` already connects, just assembled from a bare `&Grammar` in
//! one call instead of by hand at every call site.

use pg_grammar::model::Grammar;

use crate::capability::{compose_envelope_with_semantics, default_registry, CompileDecision};
use crate::emit::surface_table;
use crate::enumerate::enumerate_default;
use crate::grammar_semantics::GrammarSemantics;
use crate::junctions::PhonologyProbe;
use crate::replace::SegAlphabet;

/// Computes the overall, whole-grammar [`CompileDecision`] for `g` — `characterize` + `enumerate_
/// default` + `compose_envelope`, assembled the way a real caller would, over
/// [`crate::capability::default_registry`] (today's shipped predicate set: the one real
/// `simultaneous.subrule-overlap` predicate plus a conservative `FailClosedPlaceholder` for every
/// other `FailClosed`/`ConfigPredicate` characteristic — see that function's own doc).
///
/// **Check-only.** Calling this function has no effect on `g` or on any other compile path; it does
/// not build a live `Fsm`, does not run foma, and does not touch `emit.rs`/`gate.rs`/`replace.rs`/
/// `preexpand.rs`. A caller (e.g. `pg-cli`) that wants to SURFACE this decision (as advisory output,
/// non-blocking) is responsible for printing it; nothing here does that itself, and nothing here
/// enforces it either — see this module's own top-doc.
pub fn evaluate_capability(g: &Grammar) -> CompileDecision {
    evaluate_capability_with_semantics(&GrammarSemantics::derive(g))
}

/// [`evaluate_capability`] over an already-derived [`GrammarSemantics`] (task 7.11,
/// `openspec/changes/cleanup-and-recipe-parity`). A caller that needs BOTH the profile and the
/// verdict -- [`crate::preflight::preflight_findings`] is exactly that, and `pangloss make-report`
/// needs the verdict three times in one process -- derives the owner once and calls this, instead of
/// paying for a second (and third) full [`crate::capability::characterize`] walk.
///
/// This is not "accept a caller-supplied verdict": a [`GrammarSemantics`] is a pure, deterministic
/// function of the grammar, and this function still computes the [`CompileDecision`] itself through
/// the same `enumerate_default` + `compose_envelope` spine `evaluate_capability` always used.
pub fn evaluate_capability_with_semantics(semantics: &GrammarSemantics<'_>) -> CompileDecision {
    let g = semantics.grammar();
    let alphabet = SegAlphabet::new(surface_table(g));
    let phon = PhonologyProbe::new_with_semantics(semantics);

    let plan = enumerate_default(g, &alphabet, semantics.prules_in_order(), phon.as_ref());
    let registry = default_registry();
    compose_envelope_with_semantics(semantics, &plan, &registry)
}

#[cfg(test)]
mod tests {
    //! Synthetic, delanguaged fixtures only, reused verbatim from `crate::capability`'s own
    //! `compose_envelope` test module (same XML, same intent: this module's job is purely to prove
    //! the convenience wrapper reaches the SAME verdict `compose_envelope` itself already proves for
    //! these two shapes when assembled by hand — not to re-litigate `compose_envelope`'s own
    //! correctness, which `capability.rs`'s much larger test module already covers).

    use super::*;

    fn load(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    /// An ordinary affix + iterative-rewrite grammar (no Compounding, no Unordered strata, no MPR
    /// groups, no Simultaneous/RightToLeft/Metathesis rules, no true reduplication/circumfix) must
    /// evaluate to `Admit` through this entry point too, not just through `compose_envelope` called
    /// directly.
    #[test]
    fn evaluate_capability_admits_ordinary_affix_and_iterative_rewrite_grammar() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>Ordinary</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cb" /></SegmentNaturalClass></NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata>
            <Stratum characterDefinitionTable="t1" phonologicalRules="pr1" morphologicalRules="mr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <MorphologicalRule id="mr1">
                  <Name>-a</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="sub1">
                      <MorphologicalInput>
                        <PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence>
                      </MorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="stem" />
                        <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                      </MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </MorphologicalRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let g = load(XML);

        assert_eq!(evaluate_capability(&g), CompileDecision::Admit);
    }

    /// `openspec/changes/cover-compounding`: a grammar with a single, non-recursive `Compounding`
    /// rule must evaluate to `ConfirmOnly` through this entry point too (no longer bare `Refuse`) —
    /// the same fixture/assertion shape
    /// `capability::tests::compose_envelope_confirm_only_for_non_recursive_compounding_grammar`
    /// already proves against `compose_envelope` called directly.
    #[test]
    fn evaluate_capability_confirm_only_for_non_recursive_compounding_grammar() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1">
                  <Name>Compound</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let g = load(XML);

        assert_eq!(evaluate_capability(&g), CompileDecision::ConfirmOnly);
    }

    /// A self-feeding (`multipleApplication="2"`) `Compounding` rule evaluates to `ConfirmOnly`
    /// through this entry point too, not just through `compose_envelope` called directly (mirrors
    /// `evaluate_capability_admits_ordinary_affix_and_iterative_rewrite_grammar`'s own "same verdict
    /// through the convenience wrapper" job) -- the same verdict `crate::capability::
    /// compose_envelope_confirm_only_for_recursive_compounding_grammar` also documents.
    #[test]
    fn evaluate_capability_confirm_only_for_recursive_compounding_grammar() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1" multipleApplication="2">
                  <Name>Compound</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let g = load(XML);

        assert_eq!(
            evaluate_capability(&g),
            CompileDecision::ConfirmOnly,
            "a self-feeding Compounding rule must now evaluate to ConfirmOnly through this entry \
             point too -- task 4.1 closed the construction gap"
        );
    }
}
