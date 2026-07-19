//! Ports `MetathesisRuleTests` (parse-opt: `tests/SIL.Machine.Morphology.HermitCrab.Tests/
//! PhonologicalRules/MetathesisRuleTests.cs`), workstream W4
//! (`rust/docs/phase2-completed/metathesis-w4.md`). All 3 C# tests land live (the feature is landing in
//! this same commit series, not just being probed) — see `rust/conformance/metathesis/*/README.md`
//! for the oracle-generated conformance fixtures these same 3 scenarios were frozen as before this
//! file was written (spec-first).
//!
//! **Two scope notes vs. the sub-plan's fixture list** (both discovered empirically while building
//! the conformance fixtures, not guessed):
//! - The sub-plan's "NEW: boundary node physically inside the switch span" test is **not
//!   authorable**: a `<Segments>`/`<OptionalSegmentSequence>`-tagged (i.e. multi-node) switch group
//!   is DTD-legal but fails to compile against the real C# oracle
//!   (`AnalysisMetathesisRuleSpec`/`SynthesisMetathesisRuleSpec` both assume a switch group's
//!   `Children` are `Constraint<Word,int>` objects directly, which is false for the `Group`/
//!   `Quantifier`-wrapped output those two element kinds produce) — see
//!   `rust/conformance/metathesis/complex_rule/README.md`'s finding. `complex_rule` below already
//!   exercises the only reachable "a boundary interacts with the reorder" shape: a boundary sitting
//!   *between* two single-node switches.
//! - The sub-plan's "NEW: confirm metathesis subrules cannot be MPR-gated" has no XML attribute
//!   surface to author either: the DTD's `<MetathesisRule>` has no `requiredMPRFeatures`/
//!   `excludedMPRFeatures`/`requiredPartsOfSpeech` attribute at all (unlike
//!   `<PhonologicalSubrule>`), so there is no grammar this port could write that would even attempt
//!   to gate one. `pg_grammar::model::MetathesisRuleDef`'s and `pg_rules::metathesis`'s module docs
//!   both record this (no MPR/POS fields, no `_with_mpr` sibling function) as the pin instead.

mod csharp_port_common;
use csharp_port_common::{assert_morphs_eq, build_grammar};
use pg_parse::Morpher;

/// Ports `MetathesisRuleTests.SimpleRule` (cs:10-28): adjacent i/u swap. `LeftSwitchName="2"` (u),
/// `RightSwitchName="1"` (i) — after synthesis, u ends up first, i second, matching entry `51` =
/// "miu"'s own underlying order reversed. See `rust/conformance/metathesis/simple_rule/README.md`.
#[test]
fn simple_rule() {
    let g = build_grammar(
        r#"<MetathesisRule id="mr1" leftSwitch="segU" rightSwitch="segI">
             <Name>metathesis1</Name>
             <StructuralDescription><PhoneticTemplate><PhoneticSequence>
               <SimpleContext id="segI" naturalClass="ncISeg" />
               <SimpleContext id="segU" naturalClass="ncUSeg" />
             </PhoneticSequence></PhoneticTemplate></StructuralDescription>
           </MetathesisRule>"#,
        "mr1",
        "",
        "",
        "",
    );
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("mui"), &["51"]);
}

/// Ports `MetathesisRuleTests.ComplexRule` (cs:30-62): a non-switch "middle" boundary node between
/// the two switch groups, plus a trailing anchor, interacting with a real suffix rule. See
/// `rust/conformance/metathesis/complex_rule/README.md` for the full worked trace (this fixture's
/// own confirmed surface is "mu+i", not the C# unit test's asserted-morphs-only "mui" — that test
/// never asserts a surface string, only the morph-gloss set asserted below).
#[test]
fn complex_rule() {
    let g = build_grammar(
        r#"<MetathesisRule id="mr1" leftSwitch="segU" rightSwitch="segI">
             <Name>metathesis1</Name>
             <StructuralDescription><PhoneticTemplate finalBoundaryCondition="true"><PhoneticSequence>
               <SimpleContext id="segI" naturalClass="ncISeg" />
               <BoundaryMarker boundary="cBnd" />
               <SimpleContext id="segU" naturalClass="ncUSeg" />
             </PhoneticSequence></PhoneticTemplate></StructuralDescription>
           </MetathesisRule>"#,
        "mr1",
        r#"<MorphologicalRule id="uSuffix">
             <Name>u_suffix</Name>
             <MorphologicalSubrules><MorphologicalSubrule id="uSuffixSub">
               <MorphologicalInput><PhoneticSequence id="p1">
                 <OptionalSegmentSequence min="1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>
               </PhoneticSequence></MorphologicalInput>
               <MorphologicalOutput>
                 <CopyFromInput index="p1" />
                 <InsertSegments><PhoneticShape>+u</PhoneticShape></InsertSegments>
               </MorphologicalOutput>
             </MorphologicalSubrule></MorphologicalSubrules>
             <MorphemeId>3SG</MorphemeId>
           </MorphologicalRule>"#,
        "uSuffix",
        "",
    );
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("mui"), &["53 3SG"]);
}

/// Ports `MetathesisRuleTests.SimpleRuleNotUnapplied` (cs:64-94): the negative case. Same
/// pattern/switch roles as `simple_rule`, but the suffixed word ("pu" + "i" = "pui") presents
/// u-then-i, never i-then-u, so the rule must not fire in either direction. See
/// `rust/conformance/metathesis/not_unapplied/README.md`.
#[test]
fn simple_rule_not_unapplied() {
    let g = build_grammar(
        r#"<MetathesisRule id="mr1" leftSwitch="segU" rightSwitch="segI">
             <Name>metathesis1</Name>
             <StructuralDescription><PhoneticTemplate><PhoneticSequence>
               <SimpleContext id="segI" naturalClass="ncISeg" />
               <SimpleContext id="segU" naturalClass="ncUSeg" />
             </PhoneticSequence></PhoneticTemplate></StructuralDescription>
           </MetathesisRule>"#,
        "mr1",
        r#"<MorphologicalRule id="iSuffix">
             <Name>i_suffix</Name>
             <MorphologicalSubrules><MorphologicalSubrule id="iSuffixSub">
               <MorphologicalInput><PhoneticSequence id="p1">
                 <OptionalSegmentSequence min="1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>
               </PhoneticSequence></MorphologicalInput>
               <MorphologicalOutput>
                 <CopyFromInput index="p1" />
                 <InsertSegments><PhoneticShape>i</PhoneticShape></InsertSegments>
               </MorphologicalOutput>
             </MorphologicalSubrule></MorphologicalSubrules>
             <MorphemeId>3SG</MorphemeId>
           </MorphologicalRule>"#,
        "iSuffix",
        "",
    );
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("pui"), &["52 3SG"]);
}
