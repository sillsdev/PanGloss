//! Ports `MetathesisRuleTests` (`MetathesisRuleTests.cs`); a multi-node (`<Segments>`/`<OptionalSegmentSequence>`) switch group is DTD-legal but not authorable against the real C# oracle, and `<MetathesisRule>` has no MPR/POS gating attribute at all, so neither scope gap has a fixture — see `rust/conformance/metathesis/*/README.md` for the oracle-generated fixtures this file's scenarios were frozen as.

mod csharp_port_common;
use csharp_port_common::{assert_morphs_eq, build_grammar};
use pg_parse::Morpher;

/// Ports `MetathesisRuleTests.SimpleRule`: adjacent i/u swap; after synthesis u ends up first, i second, matching entry `51` = "miu"'s underlying order reversed.
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

/// Ports `MetathesisRuleTests.ComplexRule`: a non-switch "middle" boundary node between the two switch groups, plus a trailing anchor, interacting with a real suffix rule; this fixture's confirmed surface is "mu+i", asserted here only as the morph-gloss set.
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

/// Ports `MetathesisRuleTests.SimpleRuleNotUnapplied`: same pattern/switch roles as `simple_rule`, but the suffixed word presents u-then-i, never i-then-u, so the rule must not fire in either direction.
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
