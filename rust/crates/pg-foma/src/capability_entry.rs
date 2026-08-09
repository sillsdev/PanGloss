//! A production-shaped convenience entry point into `crate::capability::compose_envelope`, for
//! callers (`pg-cli`) that just want "the
//! `CompileDecision` for this `Grammar`" without hand-assembling `characterize`'s `enumerate_default`
//! inputs themselves.
//!
//! `best_case_across_backends` only ever COMPUTES a `crate::capability::CompileDecision`, it
//! does not consult one itself: a `Refuse` is reported rather than silently
//! papered over so an unrepresentable construct never turns into a quietly wrong parse. Whether a
//! `CompileDecision` actually blocks/stamps a real compile path is a fact
//! about the call graph, not this module — grep for callers to check.
//!
//! # Mirroring the real compile setup
//! `crate::emit::emit_with_budget`'s own SETUP (its first few lines, before any lexc text is
//! written) builds exactly three things this function also needs, the same way:
//! - `table`/the segment alphabet: `emit_with_budget` calls `crate::emit::surface_table` (the
//!   LAST stratum's char-def table — that function's own doc) and wraps it in a
//!   `crate::replace::SegAlphabet` wherever the compose/gate seams need one (`crate::gate`/
//!   `crate::replace`'s own production call sites, mirrored by `crate::enumerate`'s test-module
//!   helpers this function also mirrors). `best_case_across_backends` does the identical
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
//!   `crate::enumerate::rule_id_of`'s pointer-identity `PRuleId` recovery). That construction is
//!   the shared `crate::enumerate::prules_in_order`, which `best_case_across_backends` calls.
//!
//! `best_case_across_backends` then hands all three to `crate::enumerate::enumerate_default`
//! to get the reified `crate::plan::Plan`, and folds that together with
//! `crate::capability::characterize`'s profile via `crate::capability::compose_envelope` against
//! `crate::capability::default_registry` — the same two spines
//! already connected, just assembled from a bare `&Grammar` in
//! one call instead of by hand at every call site.
//!
//! # One verdict, three compilers
//! The scalar `CompileDecision` this module returns is a DERIVED fact: the primary judgement is
//! per-`crate::enumerate::EmissionStrategy`, and the whole-grammar answer is the best any of them
//! offers (`crate::capability::StrategyEnvelope::global`). A caller that DECIDES anything wants `crate::backend_selection::select_backends`; one that
//! needs per-backend detail wants `evaluate_capability_across_strategies`. A non-`Refuse` here says
//! nothing about the backend the caller was actually about to run.

use pg_grammar::model::Grammar;

use crate::capability::{
    compose_envelope_across_strategies, compose_envelope_with_semantics, default_registry,
    CompileDecision, StrategyEnvelope,
};
use crate::emit::surface_table;
use crate::enumerate::enumerate_default;
use crate::grammar_semantics::GrammarSemantics;
use crate::junctions::PhonologyProbe;
use crate::replace::SegAlphabet;

/// Takes an already-derived `GrammarSemantics` so a caller needing both the profile and the verdict
/// characterizes once rather than paying for a second full `crate::capability::characterize` walk.
/// Check-only: it builds no live `Fsm`, runs no foma, and touches no compile path.
///
/// **ADVISORY ONLY — never gate on this.** The best verdict ANY backend offers, joined into one
/// scalar. It is the right answer for a whole-grammar summary and the wrong answer for every
/// decision: a non-`Refuse` here says nothing about the backend a caller is about to run, so
/// gating on it lets a grammar past that the selected backend cannot compile, which then fails
/// deep in the compiler with an internal message instead of at the gate with a named construct.
/// Enforcement wants `crate::backend_selection::select_backends`; a caller needing per-backend
/// detail wants [`evaluate_capability_across_strategies`].
pub fn best_case_across_backends(semantics: &GrammarSemantics<'_>) -> CompileDecision {
    let g = semantics.grammar();
    let alphabet = SegAlphabet::new(surface_table(g));
    let phon = PhonologyProbe::new_with_semantics(semantics);

    let plan = enumerate_default(g, &alphabet, semantics.prules_in_order(), phon.as_ref());
    let registry = default_registry();
    compose_envelope_with_semantics(semantics, &plan, &registry)
}

/// Every compiler's own verdict for `g`, assembled exactly as `best_case_across_backends` assembles the
/// derived scalar one — which is `crate::capability::StrategyEnvelope::global` over this.
///
/// **Check-only**, on the same terms as `best_case_across_backends`: nothing here builds an `Fsm` or
/// alters a compile path.
/// [`best_case_across_backends`] over a `&Grammar`, deriving the semantics itself. Advisory only,
/// exactly like the function it wraps -- read that doc before calling this from anything that
/// decides something.
pub fn best_case_across_backends_for_grammar(g: &Grammar) -> CompileDecision {
    best_case_across_backends(&GrammarSemantics::derive(g))
}

pub fn evaluate_capability_across_strategies(g: &Grammar) -> StrategyEnvelope {
    let semantics = GrammarSemantics::derive(g);
    let alphabet = SegAlphabet::new(surface_table(g));
    let phon = PhonologyProbe::new_with_semantics(&semantics);

    let plan = enumerate_default(g, &alphabet, semantics.prules_in_order(), phon.as_ref());
    compose_envelope_across_strategies(&semantics, &plan, &default_registry())
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

    /// An ordinary affix + iterative-rewrite grammar must evaluate to `Admit` through this entry point too, not just through `compose_envelope` called directly.
    #[test]
    fn evaluate_capability_admits_ordinary_affix_and_iterative_rewrite_grammar() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>Ordinary</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
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

        assert_eq!(
            best_case_across_backends(&GrammarSemantics::derive(&g)),
            CompileDecision::Admit
        );
    }

    /// A grammar with a single, non-recursive `Compounding` rule must evaluate to `ConfirmOnly` through this entry point too, not bare `Refuse`.
    #[test]
    fn evaluate_capability_confirm_only_for_non_recursive_compounding_grammar() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
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

        assert_eq!(
            best_case_across_backends(&GrammarSemantics::derive(&g)),
            CompileDecision::ConfirmOnly
        );
    }

    /// A self-feeding (`multipleApplication="2"`) `Compounding` rule evaluates to `ConfirmOnly` through this entry point too, not just through `compose_envelope` called directly.
    #[test]
    fn evaluate_capability_confirm_only_for_recursive_compounding_grammar() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
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
            best_case_across_backends(&GrammarSemantics::derive(&g)),
            CompileDecision::ConfirmOnly,
            "a self-feeding Compounding rule must now evaluate to ConfirmOnly through this entry \
             point too -- task 4.1 closed the construction gap"
        );
    }
}
