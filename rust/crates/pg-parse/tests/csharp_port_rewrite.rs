//! Ports `RewriteRuleTests` (`RewriteRuleTests.cs`). `MergeRules`/`MultipleMergeRules`/`ExpandRules` are out of scope; each other scope reduction is noted at its own test.
//! Divergences found while porting: docs/research/csharp-port-rewrite-divergences.md.

mod csharp_port_common;
use csharp_port_common::{assert_empty, assert_morphs_eq, build_grammar};
use pg_parse::Morpher;

/// Ports `RewriteRuleTests.AnchorRules` (cs:165-244): anchors in environments, standalone and combined with segments.
/// Fixes a root-lookup gap on feature-bearing char tables (needed unification, not identity); full trace: docs/research/csharp-port-rewrite-divergences.md.
#[test]
fn anchor_rules() {
    // (1) RightEnvironment = [RightSideAnchor] only (absolute word-final devoicing/deaspiration).
    let g1 = build_grammar(
        r#"<PhonologicalRule id="pr3"><Name>rule3</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncC" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVlUnasp" /></PhoneticSequence></PhoneticOutput>
               <Environment><RightEnvironment><PhoneticTemplate finalBoundaryCondition="true" /></RightEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr3",
        "",
        "",
        "",
    );
    let m1 = Morpher::new(&g1, usize::MAX);
    assert_morphs_eq(&m1.parse_word("gap"), &["10", "11", "12"]);

    // (2) RightEnvironment = [vowel, cons, RightSideAnchor].
    let g2 = build_grammar(
        r#"<PhonologicalRule id="pr3"><Name>rule3</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncC" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVlUnasp" /></PhoneticSequence></PhoneticOutput>
               <Environment><RightEnvironment><PhoneticTemplate finalBoundaryCondition="true">
                 <PhoneticSequence><SimpleContext naturalClass="ncV" /><SimpleContext naturalClass="ncC" /></PhoneticSequence>
               </PhoneticTemplate></RightEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr3",
        "",
        "",
        "",
    );
    let m2 = Morpher::new(&g2, usize::MAX);
    assert_morphs_eq(&m2.parse_word("kab"), &["11", "12"]);

    // (3) LeftEnvironment = [LeftSideAnchor] only.
    let g3 = build_grammar(
        r#"<PhonologicalRule id="pr3"><Name>rule3</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncC" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVlUnasp" /></PhoneticSequence></PhoneticOutput>
               <Environment><LeftEnvironment><PhoneticTemplate initialBoundaryCondition="true" /></LeftEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr3",
        "",
        "",
        "",
    );
    let m3 = Morpher::new(&g3, usize::MAX);
    assert_morphs_eq(&m3.parse_word("kab"), &["11", "12"]);

    // (4) LeftEnvironment = [LeftSideAnchor, cons, vowel].
    let g4 = build_grammar(
        r#"<PhonologicalRule id="pr3"><Name>rule3</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncC" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVlUnasp" /></PhoneticSequence></PhoneticOutput>
               <Environment><LeftEnvironment><PhoneticTemplate initialBoundaryCondition="true">
                 <PhoneticSequence><SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncV" /></PhoneticSequence>
               </PhoneticTemplate></LeftEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr3",
        "",
        "",
        "",
    );
    let m4 = Morpher::new(&g4, usize::MAX);
    assert_morphs_eq(&m4.parse_word("gap"), &["10", "11", "12"]);
}

/// Ports `RewriteRuleTests.MultipleDeletionRules` (cs:412-442): a two-segment deletion gated on a preceding back-round vowel.
#[test]
fn multiple_deletion_rules() {
    let g = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>rule1</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighV" /><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBackRndV" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("bubu"), &["27", "19"]);
}

/// Ports `RewriteRuleTests.BoundaryRules` (cs:562-844): boundary+feature-environment reconfigurations 1-4 and MPR-feature reconfigurations 5-6 (the POS-on-subrule reconfiguration is `boundary_rules_required_pos_on_subrule_finding`).
/// Fixes two bugs in word-initial epenthesis (missing synthesis site, analysis-side direction inversion); full trace: docs/research/csharp-port-rewrite-divergences.md.
#[test]
fn boundary_rules() {
    // (1) Epenthesis of a back-round vowel after a back-round-vowel-then-boundary context.
    let g1 = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>rule1</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncBackRnd" /></PhoneticSequence></PhoneticOutput>
               <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence>
                 <SimpleContext naturalClass="ncBackRndV" /><BoundaryMarker boundary="cBnd" />
               </PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    let m1 = Morpher::new(&g1, usize::MAX);
    assert_morphs_eq(&m1.parse_word("buub"), &["30"]);

    // (2) RightEnvironment = ["+", unbackUnrndVowel].
    let g2 = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>rule1</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncUnbackUnrnd" /></PhoneticSequence></PhoneticOutput>
               <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence>
                 <BoundaryMarker boundary="cBnd" /><SimpleContext naturalClass="ncUnbackUnrndV" />
               </PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    let m2 = Morpher::new(&g2, usize::MAX);
    assert_morphs_eq(&m2.parse_word("biib"), &["30"]);

    // (3) LeftEnvironment = [backRndVowel] (no boundary) -- both "30" (bu+ib) and "31" (buib) survive.
    let g3 = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>rule1</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncBackRnd" /></PhoneticSequence></PhoneticOutput>
               <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBackRndV" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    let m3 = Morpher::new(&g3, usize::MAX);
    assert_morphs_eq(&m3.parse_word("buub"), &["30", "31"]);

    // (4) RightEnvironment = [unbackUnrndVowel] (no boundary).
    let g4 = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>rule1</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncUnbackUnrnd" /></PhoneticSequence></PhoneticOutput>
               <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncUnbackUnrndV" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    let m4 = Morpher::new(&g4, usize::MAX);
    assert_morphs_eq(&m4.parse_word("biib"), &["30", "31"]);

    // (5)+(6): disjunctive epenthesis of "ta" gated by MPR feature on the subrule.
    let g5 = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>rule1</Name>
             <PhonologicalSubrules><PhonologicalSubrule requiredMPRFeatures="mprLatinate">
               <PhoneticOutput><PhoneticSequence>
                 <SimpleContext naturalClass="ncTSeg" /><SimpleContext naturalClass="ncASeg" />
               </PhoneticSequence></PhoneticOutput>
               <Environment>
                 <LeftEnvironment><PhoneticTemplate initialBoundaryCondition="true" /></LeftEnvironment>
                 <RightEnvironment><PhoneticTemplate finalBoundaryCondition="true"><PhoneticSequence>
                   <SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncV" />
                 </PhoneticSequence></PhoneticTemplate></RightEnvironment>
               </Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    let m5 = Morpher::new(&g5, usize::MAX);
    assert_morphs_eq(&m5.parse_word("taba"), &["pos1"]);

    let g6 = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>rule1</Name>
             <PhonologicalSubrules><PhonologicalSubrule excludedMPRFeatures="mprLatinate">
               <PhoneticOutput><PhoneticSequence>
                 <SimpleContext naturalClass="ncTSeg" /><SimpleContext naturalClass="ncASeg" />
               </PhoneticSequence></PhoneticOutput>
               <Environment>
                 <LeftEnvironment><PhoneticTemplate initialBoundaryCondition="true" /></LeftEnvironment>
                 <RightEnvironment><PhoneticTemplate finalBoundaryCondition="true"><PhoneticSequence>
                   <SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncV" />
                 </PhoneticSequence></PhoneticTemplate></RightEnvironment>
               </Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    let m6 = Morpher::new(&g6, usize::MAX);
    assert_morphs_eq(&m6.parse_word("taba"), &["pos2"]);
}

/// `requiredPartsOfSpeech` on a subrule, confounded by (and fixed alongside) `boundary_rules`' bare-root epenthesis gap.
/// Full trace: docs/research/csharp-port-rewrite-divergences.md.
#[test]
fn boundary_rules_required_pos_on_subrule_finding() {
    let g = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>rule1</Name>
             <PhonologicalSubrules><PhonologicalSubrule requiredPartsOfSpeech="posN">
               <PhoneticOutput><PhoneticSequence>
                 <SimpleContext naturalClass="ncTSeg" /><SimpleContext naturalClass="ncASeg" />
               </PhoneticSequence></PhoneticOutput>
               <Environment>
                 <LeftEnvironment><PhoneticTemplate initialBoundaryCondition="true" /></LeftEnvironment>
                 <RightEnvironment><PhoneticTemplate finalBoundaryCondition="true"><PhoneticSequence>
                   <SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncV" />
                 </PhoneticSequence></PhoneticTemplate></RightEnvironment>
               </Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("taba"), &["pos2"]);
    assert_morphs_eq(&m.parse_word("ba"), &["pos1"]);
}

/// Ports `RewriteRuleTests.CommonFeatureRules` (cs:846-894): a feature change reachable either as a feature bundle or as the literal segment "v".
/// Exercises the char_def-staleness fix; see docs/research/csharp-port-rewrite-divergences.md.
#[test]
fn common_feature_rules() {
    let g1 = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>rule1</Name>
             <PhoneticInput><PhoneticSequence><Segment segment="cP" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVdLabFric" /></PhoneticSequence></PhoneticOutput>
               <Environment>
                 <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
                 <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
               </Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    let m1 = Morpher::new(&g1, usize::MAX);
    assert_morphs_eq(&m1.parse_word("buvu"), &["46"]);

    let g2 = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>rule1</Name>
             <PhoneticInput><PhoneticSequence><Segment segment="cP" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><Segment segment="cV" /></PhoneticSequence></PhoneticOutput>
               <Environment>
                 <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
                 <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
               </Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    let m2 = Morpher::new(&g2, usize::MAX);
    assert_morphs_eq(&m2.parse_word("buvu"), &["46"]);
}

/// Ports `RewriteRuleTests.EpenthesisRules` (cs:1144-1342), reconfigurations 1,2,3,4,5,7 of 9 (see the module doc for the two omitted, and `epenthesis_rules_iterative_cascade_finding` for the 9th, a separate open finding).
/// Fixes a fixture bug (sub-case 7) and a real natural-class/site-enumeration bug pair (sub-cases 2,5); full trace: docs/research/csharp-port-rewrite-divergences.md.
#[test]
fn epenthesis_rules() {
    // (1) Simultaneous epenthesis: insert a high-front-unrounded vowel after any high vowel.
    let g1 = build_grammar(
        r#"<PhonologicalRule id="pr4" multipleApplicationOrder="simultaneous"><Name>rule4</Name>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncHfuV" /></PhoneticSequence></PhoneticOutput>
               <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr4",
        "",
        "",
        "",
    );
    let m1 = Morpher::new(&g1, usize::MAX);
    assert_morphs_eq(&m1.parse_word("buibui"), &["19"]);

    // (2) Insert literal "i" before any high vowel.
    let g2 = build_grammar(
        r#"<PhonologicalRule id="pr4" multipleApplicationOrder="simultaneous"><Name>rule4</Name>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncISeg" /></PhoneticSequence></PhoneticOutput>
               <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr4",
        "",
        "",
        "",
    );
    let m2 = Morpher::new(&g2, usize::MAX);
    assert_morphs_eq(&m2.parse_word("biubiu"), &["19"]);

    // (3) Iterative, word-initial epenthesis: LeftSideAnchor + cons on the right.
    let g3 = build_grammar(
        r#"<PhonologicalRule id="pr4"><Name>rule4</Name>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncHfuV" /></PhoneticSequence></PhoneticOutput>
               <Environment>
                 <LeftEnvironment><PhoneticTemplate initialBoundaryCondition="true" /></LeftEnvironment>
                 <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncC" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
               </Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr4",
        "",
        "",
        "",
    );
    let m3 = Morpher::new(&g3, usize::MAX);
    assert_morphs_eq(&m3.parse_word("ipʰit"), &["1"]);

    // (4) Iterative, word-final epenthesis: cons on the left + RightSideAnchor.
    let g4 = build_grammar(
        r#"<PhonologicalRule id="pr4"><Name>rule4</Name>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncHfuV" /></PhoneticSequence></PhoneticOutput>
               <Environment>
                 <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncC" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
                 <RightEnvironment><PhoneticTemplate finalBoundaryCondition="true" /></RightEnvironment>
               </Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr4",
        "",
        "",
        "",
    );
    let m4 = Morpher::new(&g4, usize::MAX);
    assert_morphs_eq(&m4.parse_word("pʰiti"), &["1"]);

    // (5) cons on the left + a high-back-round vowel on the right.
    let g5 = build_grammar(
        r#"<PhonologicalRule id="pr4"><Name>rule4</Name>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncHfuV" /></PhoneticSequence></PhoneticOutput>
               <Environment>
                 <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncC" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
                 <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncHbrV" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
               </Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr4",
        "",
        "",
        "",
    );
    let m5 = Morpher::new(&g5, usize::MAX);
    assert_morphs_eq(&m5.parse_word("biubiu"), &["19"]);

    // (7, skipping the alpha-variable (6)): double-epenthesis after a high vowel, two segments at once.
    let g7 = build_grammar(
        r#"<PhonologicalRule id="pr4" multipleApplicationOrder="simultaneous"><Name>rule4</Name>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncHfuV" /><SimpleContext naturalClass="ncHfuV" /></PhoneticSequence></PhoneticOutput>
               <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr4",
        "",
        "",
        "",
    );
    let m7 = Morpher::new(&g7, usize::MAX);
    assert_morphs_eq(&m7.parse_word("biiibuii"), &["18"]);
}

/// Ports `RewriteRuleTests.EpenthesisRules`' last reconfiguration (cs:1370-1394): two `Iterative`-mode rules composed in one stratum, root "25" expected surface "butubu".
/// Open: `syn_epenthesis` is structurally Simultaneous-shaped regardless of declared mode, so composing two Iterative rules over-fires; full trace docs/research/csharp-port-rewrite-divergences.md.
#[test]
#[ignore = "syn_epenthesis is structurally Simultaneous-shaped regardless of a rule's declared \
            Iterative mode, so composing two Iterative epenthesis rules over-fires relative to \
            C#'s true iterative cursor walk; see docs/research/csharp-port-rewrite-divergences.md."]
fn epenthesis_rules_iterative_cascade_finding() {
    let g9 = build_grammar(
        r#"<PhonologicalRule id="pr4"><Name>rule4</Name>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncBackRnd" /></PhoneticSequence></PhoneticOutput>
               <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBackRndV" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>
           <PhonologicalRule id="pr5"><Name>rule2</Name>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncTSeg" /></PhoneticSequence></PhoneticOutput>
               <Environment>
                 <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
                 <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
               </Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr4 pr5",
        "",
        "",
        "",
    );
    let m9 = Morpher::new(&g9, usize::MAX);
    assert_morphs_eq(&m9.parse_word("butubu"), &["25"]);
}

/// Ports `RewriteRuleTests.DeletionRules` (cs:1345-1559) reconfigurations 5-7 (the two-rules negative case); reconfigurations 1-4 are `deletion_rules_multi_position_reinsertion`.
#[test]
fn deletion_rules_negative_case() {
    let g5 = build_grammar(
        r#"<PhonologicalRule id="pr4"><Name>rule4</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncBSeg" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <Environment>
                 <LeftEnvironment><PhoneticTemplate initialBoundaryCondition="true" /></LeftEnvironment>
                 <RightEnvironment><PhoneticTemplate><PhoneticSequence><BoundaryMarker boundary="cBnd" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
               </Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>
           <PhonologicalRule id="pr5"><Name>rule5</Name>
             <PhoneticInput><PhoneticSequence>
               <SimpleContext naturalClass="ncUSeg" /><SimpleContext naturalClass="ncBSeg" /><SimpleContext naturalClass="ncUSeg" />
             </PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <Environment>
                 <LeftEnvironment><PhoneticTemplate><PhoneticSequence><BoundaryMarker boundary="cBnd" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
                 <RightEnvironment><PhoneticTemplate finalBoundaryCondition="true" /></RightEnvironment>
               </Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr4 pr5",
        "",
        "",
        "",
    );
    let m5 = Morpher::new(&g5, usize::MAX);
    assert_empty(&m5.parse_word("b"));
}

/// C# reinsertion is a single analysis pass with optional-insert annotations expanded combinatorially downstream at root lookup, not an iterative power-set search; `ana_narrow_deletion` implements the same shape.
/// Full derivation and citations: docs/research/csharp-port-rewrite-divergences.md.
#[test]
fn deletion_rules_multi_position_reinsertion() {
    // (1) delete a high-front-unrounded vowel after a high vowel.
    let g1 = build_grammar(
        r#"<PhonologicalRule id="pr4"><Name>rule4</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHfuV" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr4",
        "",
        "",
        "",
    );
    let m1 = Morpher::new(&g1, usize::MAX);
    assert_morphs_eq(&m1.parse_word("bubu"), &["24", "25", "26", "19"]);

    // (2) delete before a consonant (RightEnvironment only).
    let g2 = build_grammar(
        r#"<PhonologicalRule id="pr4"><Name>rule4</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHfuV" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncC" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr4",
        "",
        "",
        "",
    );
    let m2 = Morpher::new(&g2, usize::MAX);
    assert_morphs_eq(&m2.parse_word("bubu"), &["25", "19"]);

    // (3) two-segment deletion target (same RightEnvironment).
    let g3 = build_grammar(
        r#"<PhonologicalRule id="pr4"><Name>rule4</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHfuV" /><SimpleContext naturalClass="ncHfuV" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncC" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr4",
        "",
        "",
        "",
    );
    let m3 = Morpher::new(&g3, usize::MAX);
    assert_morphs_eq(&m3.parse_word("bubu"), &["29", "19"]);

    // (4) two-segment deletion target gated on a preceding back-round vowel.
    let g4 = build_grammar(
        r#"<PhonologicalRule id="pr4"><Name>rule4</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHfuV" /><SimpleContext naturalClass="ncHfuV" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBackRndV" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr4",
        "",
        "",
        "",
    );
    let m4 = Morpher::new(&g4, usize::MAX);
    assert_morphs_eq(&m4.parse_word("bubu"), &["27", "19"]);
}

/// Ports `RewriteRuleTests.MultipleApplicationRules` (cs:1809-1862): `Simultaneous` vs `Iterative` produce different results over overlapping-match input.
/// Required real `RewriteMode::Simultaneous` synthesis semantics, not just grammar-load acceptance; see docs/research/csharp-port-rewrite-divergences.md.
#[test]
fn multiple_application_rules() {
    let mrules = r#"
      <PhonologicalRule id="pr1" multipleApplicationOrder="simultaneous"><Name>rule1</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncBackRnd" /></PhoneticSequence></PhoneticOutput>
          <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence>
            <SimpleContext naturalClass="ncHfuV" /><SimpleContext naturalClass="ncC" />
          </PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
    "#;
    let g1 = build_grammar(mrules, "pr1", "", "", "");
    let m1 = Morpher::new(&g1, usize::MAX);
    assert_morphs_eq(&m1.parse_word("gigugu"), &["44"]);

    let mrules_iter = mrules.replace(r#" multipleApplicationOrder="simultaneous""#, "");
    let g2 = build_grammar(&mrules_iter, "pr1", "", "", "");
    let m2 = Morpher::new(&g2, usize::MAX);
    assert_morphs_eq(&m2.parse_word("gigugi"), &["44"]);
}

/// Ports `RewriteRuleTests.LongDistanceRules` (cs:66-162): 3 reconfigurations of a rule with progressively longer/more-optional environments spanning multiple segments, exercising discontinuous morph reconstruction across a morpheme boundary.
#[test]
fn long_distance_rules() {
    // (1) LeftEnvironment = rndVowel, cons, lowVowel, cons (4 mandatory segments).
    let g1 = build_grammar(
        r#"<PhonologicalRule id="pr3"><Name>rule3</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncBackRnd" /></PhoneticSequence></PhoneticOutput>
               <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence>
                 <SimpleContext naturalClass="ncRndV" /><SimpleContext naturalClass="ncC" />
                 <SimpleContext naturalClass="ncLowV" /><SimpleContext naturalClass="ncC" />
               </PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr3",
        "",
        "",
        "",
    );
    assert_morphs_eq(
        &Morpher::new(&g1, usize::MAX).parse_word("bubabu"),
        &["13", "14"],
    );

    // (2) RightEnvironment = cons, lowVowel, cons, rndVowel (the mirror image of (1)).
    let g2 = build_grammar(
        r#"<PhonologicalRule id="pr3"><Name>rule3</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncBackRnd" /></PhoneticSequence></PhoneticOutput>
               <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence>
                 <SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncLowV" />
                 <SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncRndV" />
               </PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr3",
        "",
        "",
        "",
    );
    assert_morphs_eq(
        &Morpher::new(&g2, usize::MAX).parse_word("bubabu"),
        &["13", "15"],
    );

    // (3) Mandatory highVowel+boundary with optional cons/cons/vowel around them.
    let g3 = build_grammar(
        r#"<PhonologicalRule id="pr3"><Name>rule3</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncBackRnd" /></PhoneticSequence></PhoneticOutput>
               <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence>
                 <SimpleContext naturalClass="ncHighV" />
                 <OptionalSegmentSequence min="0" max="1"><SimpleContext naturalClass="ncC" /></OptionalSegmentSequence>
                 <BoundaryMarker boundary="cBnd" />
                 <OptionalSegmentSequence min="0" max="1"><SimpleContext naturalClass="ncC" /></OptionalSegmentSequence>
                 <OptionalSegmentSequence min="0" max="1"><SimpleContext naturalClass="ncV" /></OptionalSegmentSequence>
               </PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr3",
        "",
        "",
        "",
    );
    assert_morphs_eq(&Morpher::new(&g3, usize::MAX).parse_word("mimuu"), &["55"]);
}

/// Ports `RewriteRuleTests.QuantifierRules` (cs:247-347): `<OptionalSegmentSequence min max>` as a repeated group of multiple children, matching C#'s `.Group(g => ...).LazyRange(lo,hi)`.
#[test]
fn quantifier_rules() {
    let mrules_rule3_4 = r#"
      <PhonologicalRule id="pr3"><Name>rule3</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncBackRnd" /></PhoneticSequence></PhoneticOutput>
          <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence>
            <OptionalSegmentSequence min="1" max="2"><SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncLowV" /></OptionalSegmentSequence>
            <SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncRndV" />
          </PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
      <PhonologicalRule id="pr4"><Name>rule4</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncBackRnd" /></PhoneticSequence></PhoneticOutput>
          <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence>
            <SimpleContext naturalClass="ncRndV" />
            <OptionalSegmentSequence min="1" max="2"><SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncLowV" /></OptionalSegmentSequence>
            <SimpleContext naturalClass="ncC" />
          </PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
    "#;
    let g34 = build_grammar(mrules_rule3_4, "pr3 pr4", "", "", "");
    let m34 = Morpher::new(&g34, usize::MAX);
    assert_morphs_eq(&m34.parse_word("bubu"), &["19"]);
    assert_morphs_eq(&m34.parse_word("bubabu"), &["13", "14", "15"]);
    assert_morphs_eq(&m34.parse_word("bubababu"), &["20", "21"]);
    assert_empty(&m34.parse_word("bubabababu"));

    // rule1: no trailing anchor segment after the repeated group.
    let g1 = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>rule1</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncBackRnd" /></PhoneticSequence></PhoneticOutput>
               <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence>
                 <SimpleContext naturalClass="ncBackRndV" />
                 <OptionalSegmentSequence min="0" max="2"><SimpleContext naturalClass="ncHighV" /></OptionalSegmentSequence>
               </PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    assert_morphs_eq(
        &Morpher::new(&g1, usize::MAX).parse_word("buuubuuu"),
        &["27"],
    );
}

/// Ports `RewriteRuleTests.MultipleSegmentRules`'s first reconfiguration (cs:384-399): a genuinely two-segment `PhoneticInput`/`PhoneticOutput` gated on a preceding vowel; the second reconfiguration is `multiple_segment_rules_deletion_composition_finding`.
#[test]
fn multiple_segment_rules() {
    let mrules = r#"
      <PhonologicalRule id="pr1"><Name>rule1</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighV" /><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncBackRnd" /><SimpleContext naturalClass="ncBackRnd" /></PhoneticSequence></PhoneticOutput>
          <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBackRndV" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
    "#;
    let g = build_grammar(mrules, "pr1", "", "", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("buuubuuu"), &["27"]);
}

/// Ports `RewriteRuleTests.MultipleSegmentRules`'s second reconfiguration (cs:401-408): adding an untriggered pure-deletion rule to the same stratum must not change the first rule's result.
/// Fixed a target-match span bug where an interposed Optional segment made `width_matches` discard every candidate; full trace: docs/research/csharp-port-rewrite-divergences.md.
#[test]
fn multiple_segment_rules_deletion_composition_finding() {
    let mrules = r#"
      <PhonologicalRule id="pr1"><Name>rule1</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighV" /><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncBackRnd" /><SimpleContext naturalClass="ncBackRnd" /></PhoneticSequence></PhoneticOutput>
          <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBackRndV" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
      <PhonologicalRule id="pr2"><Name>rule2</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncTSeg" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBackRndV" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
    "#;
    let g = build_grammar(mrules, "pr1 pr2", "", "", "");
    let m = Morpher::new(&g, usize::MAX);
    assert_morphs_eq(&m.parse_word("buuubuuu"), &["27"]);
}

/// Ports `RewriteRuleTests.DisjunctiveRules` (cs:1562-1806): 5 reconfigurations of a rule with 2+ ordered disjunctive subrules where the first matching `Environment` wins.
#[test]
fn disjunctive_rules() {
    // (1) `stop` target; subrule(a) word-initial -> asp; subrule(b) (else) -> unasp.
    let g1 = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>disrule1</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules>
               <PhonologicalSubrule>
                 <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAsp" /></PhoneticSequence></PhoneticOutput>
                 <Environment><LeftEnvironment><PhoneticTemplate initialBoundaryCondition="true" /></LeftEnvironment></Environment>
               </PhonologicalSubrule>
               <PhonologicalSubrule>
                 <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncUnasp" /></PhoneticSequence></PhoneticOutput>
               </PhonologicalSubrule>
             </PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    assert_morphs_eq(&Morpher::new(&g1, usize::MAX).parse_word("pʰip"), &["41"]);

    // (2) Unbounded stretch; 4 disjunctive subrules, one per backness/roundness combination.
    let g2 = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>disrule1</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncHighV" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules>
               <PhonologicalSubrule>
                 <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncBackRnd" /></PhoneticSequence></PhoneticOutput>
                 <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence>
                   <SimpleContext naturalClass="ncBackRndV" />
                   <OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncHFrontV" /></OptionalSegmentSequence>
                   <SimpleContext naturalClass="ncC" />
                 </PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
               </PhonologicalSubrule>
               <PhonologicalSubrule>
                 <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncFrontRnd" /></PhoneticSequence></PhoneticOutput>
                 <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence>
                   <SimpleContext naturalClass="ncFrontRndV" />
                   <OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncHFrontV" /></OptionalSegmentSequence>
                   <SimpleContext naturalClass="ncC" />
                 </PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
               </PhonologicalSubrule>
               <PhonologicalSubrule>
                 <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncBackUnrnd" /></PhoneticSequence></PhoneticOutput>
                 <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence>
                   <SimpleContext naturalClass="ncBackUnrndV" />
                   <OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncHFrontV" /></OptionalSegmentSequence>
                   <SimpleContext naturalClass="ncC" />
                 </PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
               </PhonologicalSubrule>
               <PhonologicalSubrule>
                 <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncUnbackUnrnd" /></PhoneticSequence></PhoneticOutput>
                 <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence>
                   <SimpleContext naturalClass="ncUnbackUnrndV" />
                   <OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncC" /><SimpleContext naturalClass="ncHFrontV" /></OptionalSegmentSequence>
                   <SimpleContext naturalClass="ncC" />
                 </PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
               </PhonologicalSubrule>
             </PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    assert_morphs_eq(
        &Morpher::new(&g2, usize::MAX).parse_word("bububu"),
        &["42", "43"],
    );

    // (3) Anchors instead of a bare environment-less "else" fallback.
    let g3 = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>disrule1</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules>
               <PhonologicalSubrule>
                 <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAsp" /></PhoneticSequence></PhoneticOutput>
                 <Environment><LeftEnvironment><PhoneticTemplate initialBoundaryCondition="true" /></LeftEnvironment></Environment>
               </PhonologicalSubrule>
               <PhonologicalSubrule>
                 <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncUnasp" /></PhoneticSequence></PhoneticOutput>
                 <Environment><RightEnvironment><PhoneticTemplate finalBoundaryCondition="true" /></RightEnvironment></Environment>
               </PhonologicalSubrule>
             </PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    assert_morphs_eq(&Morpher::new(&g3, usize::MAX).parse_word("pʰip"), &["41"]);

    // (4) Literal target; subrule(a) intervocalic voices it, subrule(b) word-final aspirates it.
    let g4 = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>disrule1</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncPSeg" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules>
               <PhonologicalSubrule>
                 <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
                 <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncV" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
               </PhonologicalSubrule>
               <PhonologicalSubrule>
                 <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAsp" /></PhoneticSequence></PhoneticOutput>
                 <Environment><RightEnvironment><PhoneticTemplate finalBoundaryCondition="true" /></RightEnvironment></Environment>
               </PhonologicalSubrule>
             </PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    assert_morphs_eq(
        &Morpher::new(&g4, usize::MAX).parse_word("bubu"),
        &["46", "19"],
    );

    // (5) subrule(a) fires after another voiceless stop; subrule(b) is the else fallback.
    let g5 = build_grammar(
        r#"<PhonologicalRule id="pr1"><Name>disrule1</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncVlStop" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules>
               <PhonologicalSubrule>
                 <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAsp" /></PhoneticSequence></PhoneticOutput>
                 <Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncVlStop" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment>
               </PhonologicalSubrule>
               <PhonologicalSubrule>
                 <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncUnasp" /></PhoneticSequence></PhoneticOutput>
               </PhonologicalSubrule>
             </PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr1",
        "",
        "",
        "",
    );
    assert_morphs_eq(&Morpher::new(&g5, usize::MAX).parse_word("ktʰb"), &["49"]);
}
