//! Ports `RewriteRuleTests` (parse-opt: `tests/SIL.Machine.Morphology.HermitCrab.Tests/
//! PhonologicalRules/RewriteRuleTests.cs`) bucket-B rows per
//! `rust/parity-out/audit/phase2/D-test-coverage-map.md` §3. `MergeRules`/`MultipleMergeRules`/
//! `ExpandRules` (analysis-side depends on W8's narrowing landing -- another agent's workstream) are
//! out of scope here.
//!
//! **Update (W11 batch-5):** `LongDistanceRules`/`QuantifierRules`/`DisjunctiveRules` (was bucket D,
//! "needs the W9.2 probe") port live below with zero engine changes -- the probe confirmed the
//! general rewrite-environment machinery (multi-segment `LeftEnvironment`/`RightEnvironment`,
//! `{min,max}`-repeated groups via `OptionalSegmentSequence`, anchors, ordered disjunctive subrules)
//! already matches C#. `MultipleSegmentRules` is a partial exception: its first reconfiguration
//! ports live, but composing a second (untriggered deletion) rule into the same stratum surfaces a
//! genuine finding -- [`multiple_segment_rules_deletion_composition_finding`].
//!
//! Deliberate scope reductions (each noted at its test, not silently):
//! - `BoundaryRules`: ports the boundary+feature-environment reconfigurations and the MPR-feature
//!   reconfigurations (both confirmed supported: MPR sets ARE threaded through
//!   `RewriteSubruleDef.required_mpr`/`excluded_mpr`); the `RequiredSyntacticFeatureStruct`-on-a-
//!   subrule reconfiguration's own dedicated test (`boundary_rules_required_pos_on_subrule_finding`)
//!   is STILL a documented finding, but no longer the POS gate itself (plan item 2 / wave-3 ported
//!   `requiredPartsOfSpeech` for real, verified via a dedicated fixture) -- it's now confounded by a
//!   separate bare-root-epenthesis synthesis-confirm gap (see that test's updated doc).
//! - `EpenthesisRules`: omits the `RightToLeft` + left-anchor infinite-loop-detection negative case
//!   (no loop/budget detection exists in `hc_rules::rewrite`, so this would hang or diverge --
//!   already flagged as absent by the coverage map, not attempted here) and the alpha-variable
//!   agreement reconfiguration (already bucket A via `hc-rules/tests/alpha_gate.rs`).
//! - `DeletionRules`: omits the `Morpher.DeletionReapplications = 1` reconfiguration (no equivalent
//!   knob on `hc_parse::Morpher`'s public API).
//! - `MultipleApplicationRules`: **FIXED (P13)** -- both sub-cases now pass; see the test's own doc.
//!
//! **Major finding, FIXED (plan item 1 / wave-3)**: `hc_rules::rewrite::ana_feature`
//! (`hc-rules/src/rewrite.rs`, the mutation loop around `ms.nodes[node].lanes[f] = full_mask(g, f);`)
//! correctly widens a changed feature's LANES to full-mask on analysis-unapply (the documented
//! "underspecify on unapply" behavior, matching `hc-rules/tests/rewrite_gate.rs`'s
//! `feature_change_analysis_underspecifies_voice`) but never touched the node's `char_def`/`cd_set`.
//! Confirmed via direct calls: unapplying a rule that changes "v" (labiodental fricative) back toward
//! "p" produced a node whose lanes were correctly widened (`poa`/`vd`/etc. now span multiple values)
//! but whose `char_def` was still literally "v"'s. Root-allomorph lookup
//! (`hc_parse::root_trie::RootAllomorphIndex::search`) keys off that literal `char_def` (confirmed
//! empirically: it returned zero matches for the widened-but-still-"v" node, even though a
//! "p"-rooted lexical entry exists and the node's lanes are lane-compatible with "p") -- so an
//! analysis-side reconstruction could never find a lexical root whose underlying segment differs from
//! the word's own surface segment at that position, exactly the scenario every test below needs
//! (a phonological rule creating an analysis-side choice between two different underlying segments).
//!
//! **Fix applied**: `ana_feature` now clears the changed node's `char_def` to `NO_CHAR_DEF` after
//! widening its lanes (mirroring `syn_feature`'s pre-existing, already-`PV` identical clearing), so
//! root lookup falls back to pure lane unification -- matching C#'s own always-lane-based
//! `CharacterDefinitionTable.GetMatchingStrReps`. This crate's `MutNode` (`rewrite.rs`) carries no
//! separate `cd_set` column -- `freeze_to_shape` always pushes a `NO_CHAR_DEF` segment via the plain
//! `Unrestricted` path, exactly like `syn_feature`'s proven fix, so no additional `cd_set` machinery
//! was needed here (contrast `hc-rules/src/morph.rs`'s `OutNode`, which DOES carry an explicit
//! `cd_set` and needed the fuller `ctx_cd_set`-based fix -- see that file's `copy_part`/
//! `generate_shape` and this file's sibling `csharp_port_affix_process.rs`'s `ModifyFromInput`
//! findings, now also fixed). `ana_narrow` needed no change: its natural-class-only reinsertion path
//! (`new_seg_node`'s `_ => u32::MAX` fallback) already produced `NO_CHAR_DEF` nodes correctly; only a
//! LITERAL-char_def LHS pattern gets a concrete `char_def` there, which is faithful (that segment
//! really is the literal char). `ana_epenthesis` needed no change either: it only ever flips a node's
//! `optional` flag, never rewrites lanes/`char_def`.
//!
//! **Outcome**: `common_feature_rules`, `simulfix_rules`/`modify_from_input_rules` (this file's
//! sibling `csharp_port_affix_process.rs`) are now fully green and un-ignored. `anchor_rules`
//! **is now also fully green and un-ignored** (P5, `docs/p5-crosstable-featurestruct-design.md`
//! -- see the test's own doc: the residual finding was a narrower over-extended-identity-model bug,
//! not the cross-table redesign originally suspected). `boundary_rules` and
//! `deletion_rules_multi_position_reinsertion` each improved (documented per-test) but surfaced
//! DIFFERENT, deeper, genuinely separate residual findings this fix does not reach (a bare-root
//! epenthesis synthesis-confirm gap; and what was then believed to be an `ana_narrow`
//! per-subset-reinsertion gap, since RESOLVED as a stale status note -- see that test's doc:
//! all-sites-at-once OPTIONAL insertion IS the C# mechanism, the subset choice happens at root
//! lookup) -- each now documented at its own test rather than attributed to this root cause.
//! `epenthesis_rules`/`multiple_application_
//! rules` were NEVER blocked by this root cause at all (a load-time `RewriteMode::Simultaneous`
//! rejection, W1 item 4) -- the original blanket attribution above was imprecise; corrected at
//! `epenthesis_rules`'s own doc. `multiple_deletion_rules` needed only reconstruction of a segment
//! that's IDENTICAL in every candidate root (no literal-char_def mismatch), so it passed even before
//! this fix.

mod csharp_port_common;
use csharp_port_common::{assert_empty, assert_morphs_eq, build_grammar};
use hc_parse::Morpher;

/// Ports `RewriteRuleTests.AnchorRules` (RewriteRuleTests.cs:165-244): `RightSideAnchor`/
/// `LeftSideAnchor` in environments, standalone and combined with segments.
///
/// **FIXED (P5, `docs/p5-crosstable-featurestruct-design.md`).** Sub-case (1)'s
/// `assert_morphs_eq(&m1.parse_word("gap"), &["10","11","12"])` used to fail, missing root "10":
/// root "10"'s allomorph is `"ga̘p"` (ATR- "a̘", `cAUnderdot`) while surface "gap" segments its
/// vowel as plain "a" (`cA`) -- two DIFFERENT concrete `char_def`s that pr3 (a consonant-only rule)
/// never touches, so neither is ever `NO_CHAR_DEF`. The prior diagnosis attributed this to a needed
/// multi-table/cross-stratum redesign, but the real root cause (P5 §1.1) was narrower and did not
/// need per-table identity at all: C#'s root lookup is pure `FeatureStruct.IsUnifiable` with no
/// separate char-def-identity gate in the first place -- `CharacterDefinitionTable.Add` only attaches
/// a `StrRep` disjunction on the `fs == null` branch (zero authored phonological features, e.g.
/// Sena); a feature-bearing segment (this fixture's `cA`/`cAUnderdot`, real Indonesian/Amharic
/// segments) gets `Type + features` and NO `StrRep` at all. So two distinct concrete char-defs whose
/// feature structs unify legitimately cross-match root lookup in C# even within ONE table, and this
/// fixture's merged-table approximation can express the case once `cA` genuinely leaves ATR
/// unspecified (this fixture's earlier draft had `cA` pinned to `ATR+`, emulating Table3's "a" rather
/// than the Table1 "a" every ported test actually segments against -- see
/// `csharp_port_common::mod`'s `CHAR_TABLE_XML` comment for the correction).
///
/// Fix: `hc_grammar::chardef::CharDefTable` precomputes a build-time unifiability closure over a
/// feature-bearing table's segment char-defs (`unif_closure`/`unifiable_cds`, gated on
/// `!PhonFeatureSystem::is_empty()` so zero-feature grammars like Sena are untouched);
/// `root_trie::edge_matches`'s concrete×concrete arm and `surface::matching_reps_for_node`'s
/// concrete-identity gate both fall back to that closure on an equality miss, restoring C#'s
/// two-regime semantics (identity where C# has `StrRep`, unification where it doesn't) at both the
/// trie (analysis-side root lookup) and synthesis-confirm sites -- both needed the fix, per P5 §1.2.
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

/// Ports `RewriteRuleTests.MultipleDeletionRules` (RewriteRuleTests.cs:412-442): a two-segment
/// deletion (`Lhs` = [highVowel, highVowel]) gated on a preceding back-round vowel.
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

/// Ports `RewriteRuleTests.BoundaryRules` (RewriteRuleTests.cs:562-844), the boundary+feature-
/// environment reconfigurations (1-4) and the MPR-feature reconfigurations (last 2); the
/// `RequiredSyntacticFeatureStruct`-on-a-subrule reconfiguration is a documented finding (see file
/// doc + [`boundary_rules_required_pos_on_subrule_finding`]).
///
/// PARTIALLY FIXED (plan item 1 / wave-3): sub-cases (1)-(4) (the boundary+feature-environment
/// reconfigurations needing the char_def-staleness fix) now pass. Sub-cases (5)/(6) (the MPR-gated
/// epenthesis reconfigurations) surface a DIFFERENT finding, confirmed independent of both
/// char_def staleness and MPR gating specifically: `m5.parse_word("taba")` returns EMPTY (want
/// `{"pos1"}`) even with `requiredMPRFeatures`/`excludedMPRFeatures` stripped from the grammar
/// entirely.
///
/// **ROOT CAUSE ISOLATED (wave-4), fix deferred to the W8 rewrite.rs owner** (wave-4's brief
/// hard-bounds `rewrite.rs` to the concurrent W8 narrowing/budget rewrite): the break is NOT in
/// the stratum synthesis pipeline (wave-3's suspicion) — the trailing prule loop at
/// `stratum.rs::synthesize_stratum` runs fine and the bare-root passthrough is correct. It is
/// `hc_rules::rewrite::syn_epenthesis`'s site enumeration (rewrite.rs:1466-1495 at 6c2a05be): the
/// loop `for (site, &node) in node_of.iter().enumerate()` with `left_end = right_start = site + 1`
/// only ever considers the gap AFTER each existing segment — the **word-initial gap before segment
/// 0** (`left_end = right_start = 0`) is never a candidate site, so an epenthesis whose
/// environments hold only at position 0 can never fire during synthesis. Probe matrix (direct
/// `rewrite::synthesize` on the feature-bearing root shape "ba", each env variant loaded correctly
/// per a full `RewriteRuleDef` dump):
/// - left `initialBoundaryCondition` + right `[C V]`+final anchor (this test's rule): 0 outputs;
/// - right `[C V]`+final anchor only (no left env): 0 outputs;
/// - right `[C V]` only (no anchors anywhere): 0 outputs (kills the "anchors are the problem"
///   hypothesis — every variant needing the position-0 site fails identically);
/// - left `[C]` (a real segment, insertion after "b"): 1 output (fires) — the only variant whose
///   site the loop can reach.
///
/// The C# contrast: `SynthesisRewriteRuleSpec`'s pattern walk starts BEFORE the first segment
/// annotation, so position 0 is an ordinary application site.
///
/// **FIXED (P1, 2026-07-09) — un-ignored; green.** TWO distinct bugs blocked sub-cases (5)/(6),
/// and wave-4's probe matrix (direct `rewrite::synthesize` only) could only see the first:
/// 1. `syn_epenthesis`'s missing word-initial site, exactly as diagnosed above — verified against
///    C# `SynthesisRewriteRuleSpec.cs:23-30` (empty-LHS pattern = one `Segment|Anchor` constraint,
///    so the left anchor IS a match site) + `RewriteRuleSpec.cs:58-73` + `AddAfter(range.Start)`.
///    Fixed by adding the site-0 gap (splice after `ms.nodes[0]`, the left anchor), the synthesis
///    twin of `ana_narrow_deletion`'s already-landed fix. Unit gate:
///    `hc-rules/tests/rewrite_gate.rs::epenthesis_synthesis_word_initial_site`.
/// 2. A separate ANALYSIS-side inversion the synthesis-only probe never reached: this rule's RHS
///    is 2 nodes (`ta`), and `compile_lane_fst` compiled multi-node analysis targets in document
///    order for a `RightToLeft` traversal — under `hc_fst`'s frozen "nodes follow traversal order"
///    convention that matched the physically REVERSED sequence (`at`), so `ana_epenthesis` never
///    marked `taba`'s `t,a` optional and no candidate root ever reached synthesis-confirm. C#'s
///    `PatternNode.GenerateNfa` (PatternNode.cs:55) enumerates children in `fsa.Direction` order,
///    i.e. an RtL matcher matches the SAME physical substring as LtR. Fixed by reordering
///    document→traversal inside `compile_lane_fst`; invisible on all three reference grammars'
///    single-node analysis targets. Unit gate: `rewrite_gate.rs::
///    epenthesis_analysis_multi_node_target_matches_document_order`.
///
/// Word-medial double-firing re-verified: `rewrite_gate.rs`' epenthesis suite + this file's
/// medial/final cases are unchanged-green (site 0 is a new site, not a re-enumeration of existing
/// ones). Oracle fixture: `rust/conformance/rewrite/word-initial-epenthesis/`.
#[test]
fn boundary_rules() {
    // (1) LeftEnvironment = [backRndVowel, "+"] -- epenthesis of a back-round vowel after a
    // back-round-vowel-then-boundary context.
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

    // (5)+(6): pos1 ("ba", V, Latinate) / pos2 ("ba", N, Germanic) disjunctive-epenthesis-of-"ta"
    // gated by MPR feature (RequiredMprFeatures / ExcludedMprFeatures on the subrule).
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

/// **FINDING, UPDATED (plan item 2 / wave-3): the POS gate itself is now REAL and CORRECT, but this
/// specific test remains blocked by a DIFFERENT, separate, already-documented finding.**
/// `RewriteSubruleDef.required_pos` is no longer unconditionally inapplicable during synthesis (W1
/// item 4's stopgap is gone -- `hc_rules::rewrite::subrule_applicable`/`required_pos_ok` now
/// implement C#'s real `SynthesisRewriteSubruleSpec.IsApplicable` POS check, verified byte-identical
/// against the oracle on a dedicated feature-change fixture, `rust/conformance/rewrite/
/// required-pos-subrule/`). This test's own rule is an EPENTHESIS subrule, though, and
/// `rewrite/boundary_rules`'s own (separately fixed by plan item 1) sub-cases (5)/(6) already
/// established that a bare-root (no morphological rule) epenthesis-only phonological rule never gets
/// re-applied on synthesis-confirm AT ALL, independent of any gating condition (confirmed by an
/// isolated probe with `requiredMPRFeatures`/`excludedMPRFeatures` stripped entirely -- still fails)
/// -- see that test's doc comment for the fuller diagnosis. This test's own grammar hits that exact
/// same confound: `taba`/`ba` still return the wrong results (confirmed still `{}`/won't-assert-past-
/// first-line below), but NOT because of the POS gate anymore -- because epenthesis subrules never
/// fire for a bare root regardless of any applicability gate. A future owner fixing the bare-root-
/// epenthesis gap should re-run this exact fixture as a regression check for the POS gate combined
/// with a working epenthesis path.
///
/// **FIXED (P1, 2026-07-09) — un-ignored; green.** The bare-root epenthesis path now works (see
/// [`boundary_rules`]' updated doc: word-initial synthesis site + the multi-node analysis-target
/// direction fix), and as predicted the separately-landed POS gate composes correctly with it:
/// `taba` resolves to `pos2` only (posN fires the epenthesis; `pos1`/posV's confirm can't produce
/// the `ta`) and `ba` to `pos1` only (posV never epenthesizes; `pos2`'s confirm would).
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

/// Ports `RewriteRuleTests.CommonFeatureRules` (RewriteRuleTests.cs:846-894): a feature change
/// expressed via one common natural class (a single voiced labiodental fricative target reachable
/// either as a feature bundle or as the literal segment "v").
///
/// FIXED (plan item 1 / wave-3): was `#[ignore]`d on the module doc's "major finding" (`ana_feature`
/// widened lanes but left `char_def` stale on analysis-unapply). `hc_rules::rewrite::ana_feature`
/// now clears the changed node's `char_def` to `NO_CHAR_DEF` after widening, mirroring
/// `syn_feature`'s existing (already-`PV`) identical fix — root-trie lookup falls back to lane
/// unification instead of a stale literal-identity gate. Un-ignored; green.
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

/// Ports `RewriteRuleTests.EpenthesisRules` (RewriteRuleTests.cs:1144-1342), the non-alpha-variable,
/// non-infinite-loop reconfigurations (1,2,3,4,5,7 of the C# test's 9; see file doc for the two
/// omitted).
///
/// CORRECTED FINDING (plan item 1 / wave-3): this test's actual blocker was mis-attributed to the
/// char_def-staleness "major finding" -- it is really the SAME `RewriteMode::Simultaneous` load-time
/// lint `multiple_application_rules` hits (W1 item 4): sub-case (1)'s grammar uses
/// `multipleApplicationOrder="simultaneous"` and fails at `build_grammar` itself (`GrammarError
/// ::Unsupported`), before any of item 1's `ana_feature`/root-lookup code ever runs. Item 1's
/// char_def fix does not and cannot touch this -- it's a grammar-load-time rejection, not an
/// analysis-time miss. Unchanged/still fully blocked; reclassified to the correct root cause.
///
/// **P13 update (`rust/docs/p13-simultaneous-design.md`): the load-time lint is gone, so this test
/// now runs past sub-case (1) for the first time ever -- and, per this plan's own explicit warning
/// ("un-ignoring is not automatic, re-verify each sub-case"), it surfaces TWO separate, genuinely
/// new findings rather than going straight green:**
///
/// - **Sub-case (7) (`"biiibuii" -> "18"`) was a fixture bug, now FIXED.** The shared
///   `csharp_port_common::LEXICON_XML`'s root `"18"` entry stored the WRONG shape ("bibabi" -- a
///   known, deliberately-left mislabeling from an earlier wave's `e18`/`e24`/`e26` cleanup, whose own
///   comment predicted exactly this: "`e18` is left as-is (still unreferenced by any live test, so no
///   collision forced a fix)"). This sub-case IS that collision. Confirmed against
///   `HermitCrabTestBase.cs:565` (`AddEntry("18", ..., Allophonic, "bibu")`) and cross-checked against
///   the OTHER live C# use of root 18 (`RewriteRuleTests.cs:1256-1289`'s alpha-variable
///   reconfiguration, `"biibuu"` -- not ported here, see below, but independently consistent with the
///   corrected "bibu" shape): root 18 is "bibu", not "bibabi". Fixed at the fixture, not the engine.
/// - **Sub-cases (2) and (5) reveal a genuine, separate, PRE-EXISTING Rust bug -- confirmed against
///   the real oracle, not just hypothesized, and confirmed NOT specific to `RewriteMode::Simultaneous`
///   (sub-case (5) is plain Iterative and fails identically).** Both insert an epenthetic segment
///   adjacent to root `"19"`'s own internal morpheme boundary (`"b+ubu"`,
///   `deletion_rules_multi_position_reinsertion`'s doc / the `simultaneous-epenthesis` conformance
///   fixture's README document this same root's real shape). Direct diagnosis (`hc_rules::rewrite
///   ::synthesize` on the bare root, plus a `TreeTraceSink` trace of the full `parse_word`) found
///   `syn_epenthesis`'s `RightEnvironment` check at the gap immediately BEFORE the boundary (between
///   the first "b" and "+") spuriously succeeds, inserting a THIRD "i" that should not exist -- giving
///   `"bi+iubiu"` instead of `"b+iubiu"` (8 rendered segments instead of 7), a surface that no longer
///   matches the input word, so the real root never round-trips (`SurfaceFormMismatch`). The real C#
///   test (`RewriteRuleTests.cs:1201-1254`) asserts BOTH sub-cases succeed, so this is not a case where
///   Rust's extra site might be the "more correct" answer -- it is a confirmed divergence. Root cause
///   (not yet fixed, and not fully disambiguated between two candidate mechanisms observed while
///   diagnosing this -- narrowing further needs closer inspection of `compile_env`'s generated FST,
///   which was not done in this pass):
///   (a) this port's environment patterns compiled from a bare phonological `FeatureNaturalClass`
///   (e.g. "highVowel") carry no `Type=Segment` constraint bit (this module's own top-of-file doc
///   already flags `Type` as a symbolic feature the frozen `hc_shape`/`hc_fst` contracts do not encode
///   as a lane, handled today only by WHICH node kinds are fed into the matcher stream), so a
///   `Boundary` node's all-unconstrained phonological lanes could satisfy a bare natural-class
///   environment constraint that ought to require a real Segment; or
///   (b) the boundary is being transparently skipped over by the same Optional-skip matching
///   mechanism `hc-rules/tests/rewrite_gate.rs`'s
///   `feature_change_synthesis_rejects_an_over_wide_optional_skip_span` gate test exercises for real
///   Optional segments, landing the environment check one position further out than it should reach.
///   Either way this is a real, separate, non-trivial fix (touches the same environment-compilation/
///   matching machinery every other rewrite/allomorph environment check shares) -- deliberately NOT
///   attempted inside this P13 pass given the blast radius; flagged here as a new, confirmed
///   divergence (mechanism not yet pinned down to one of the two candidates above) for a future
///   dedicated pass, not silently patched or hidden. Both sub-cases stay
///   part of THIS test (matching `anchor_rules`'s established convention of keeping a partially-
///   passing C# port together with a doc note, rather than splitting it), so the whole function stays
///   `#[ignore]`d -- (1), (3), (4), (7), (9) all verified passing individually; only (2)/(5) block a
///   clean run.
#[test]
#[ignore = "P13: the Simultaneous load-time lint is gone (multiple_application_rules is now fully \
            green) and sub-case (7)'s root-18 fixture shape is fixed, but sub-cases (2)/(5) surface \
            a separate, confirmed, pre-existing (not Simultaneous-specific) Rust bug: syn_epenthesis's \
            environment check spuriously matches root 19's internal morpheme boundary ('b+ubu'), \
            inserting an extra epenthetic segment the real C# oracle does not (RewriteRuleTests.cs: \
            1201-1254 both pass in C#). Mechanism not fully pinned down between two candidates (a \
            missing Type=Segment gate on natural-class environments, or an Optional-skip matcher \
            reaching one position too far) -- see doc comment. Deliberately not fixed here (real, \
            separate root-cause fix, out of P13's scope)."]
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

    // (9, the last reconfiguration): a NEW rule2 devoicing/vowel epenthesis composition.
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

/// Ports `RewriteRuleTests.DeletionRules` (RewriteRuleTests.cs:1345-1559) reconfigurations 5-7 (the
/// two-Morphophonemic-rules negative case), which does not need multiple analysis-side alternatives
/// and so is unaffected by the module doc's "major finding". Reconfigurations 1-4 (and the omitted
/// `DeletionReapplications` one) are [`deletion_rules_multi_position_reinsertion`].
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

/// RESOLVED (P2, 2026-07-09) -- the former **FINDING** here (and rust-optimizations-phase2.md P2's
/// "C# explores the power-set of reinsertion sites via iterative reapplication" framing) was based
/// on a wrong mental model of the C# mechanism, and its "now 2/4" status note was stale. What C#
/// actually does (all verified against the oracle source, `.worktrees/parse-opt`):
///
/// - `AnalysisRewriteRule.Apply`'s Deletion branch runs exactly `1 + Morpher.DeletionReapplications`
///   passes (`AnalysisRewriteRule.cs:143-157`), and `DeletionReapplications` is a bare auto-property
///   (`Morpher.cs:122`) that defaults to **0** -- `RewriteRuleTests.DeletionRules` uses a default
///   `Morpher` for every reconfiguration ported here, and the one reconfiguration that sets
///   `DeletionReapplications = 1` (RewriteRuleTests.cs:1414, adding 8-segment entry "27" to
///   sub-case (1)'s expectations) is exactly the one this port omits. So the gold expectations
///   below come from a SINGLE analysis pass -- no iterative site search exists at this layer.
/// - Within that single pass, `SimultaneousPhonologicalPatternRule.Apply` collects ALL matches and
///   applies every one to the same input, and `NarrowAnalysisRewriteRuleSpec.Unapply` re-inserts
///   the deleted segment(s) at each site as **optional** nodes (`Shape.AddAfter(curNode, fs, true)`
///   -> `Annotation.Optional = true`, `NarrowAnalysisRewriteRuleSpec.cs:49`/`Shape.cs:695-698`).
/// - The "power set of reinsertion subsets" is therefore realized DOWNSTREAM, at root lookup: the
///   FST traversal may consume or skip each optional annotation independently
///   (`TraversalMethodBase.cs:295/390`), so one optional-decorated shape `b u (i) b u (i)` reaches
///   "19" `bubu` (skip both), "25" `buibu` (consume first), "24" `bubui` (consume second), and
///   "26" `buibui` (consume both) -- no rule-level enumeration at all.
///
/// Rust's `ana_narrow_deletion` (`hc-rules/src/rewrite.rs`) implements exactly this shape:
/// all-sites-in-one-pass optional inserts (`new_seg_node(.., true)`), with the skip-or-consume
/// branching in `hc_parse::root_trie`'s `search_segs_opt`. All 4 sub-cases pass, and pass even at
/// pre-P1 `92f2e166~1` (forced rebuild) -- the ignore note's "2/4" reflected an intermediate dev
/// state inside the `27b7a7a4` squash (before `ana_narrow_deletion`'s word-initial-site fix and the
/// optional-insert completion landed there) and was never re-checked. Oracle conformance fixture:
/// `rust/conformance/rewrite/deletion-reinsertion/` (live C# oracle, byte-identical).
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

/// Ports `RewriteRuleTests.MultipleApplicationRules` (RewriteRuleTests.cs:1809-1862).
///
/// **FIXED (P13, 2026-07-10) -- un-ignored; green.** This test's entire point is that
/// `RewriteApplicationMode.Simultaneous` and `.Iterative` produce DIFFERENT results on the same rule
/// over overlapping-match input ("gigugu" needs Simultaneous semantics; "gigugi" needs Iterative).
/// W1 item 4 originally left `RewriteMode::Simultaneous` parsed but silently executed identically to
/// `Iterative`; a later hardening pass made it hard-fail at grammar-load time instead. Both gaps are
/// now closed: `RewriteMode::Simultaneous` loads AND has real synthesis semantics
/// (`hc_rules::rewrite::sim_feature`, `rust/docs/p13-simultaneous-design.md` §4.1/§4.2) --
/// `synthesize_with_mpr`/`synthesize_with_mpr_cached` dispatch on `(classify(rule, sr), rule.mode)`,
/// collecting every accepted match against one pristine snapshot before applying any of them
/// (mirroring C#'s `SimultaneousPhonologicalPatternRule.Apply` exactly), instead of `syn_feature`'s
/// find-one-then-rescan Iterative shape. Both sub-cases now pass: `m1` (Simultaneous) parses
/// "gigugu"; `m2` (the same rule with `multipleApplicationOrder` omitted, i.e. Iterative) parses
/// "gigugi" -- the same divergence this whole test exists to pin down, oracle-verified independently
/// via the `rewrite/simultaneous-feeding`/`simultaneous-feeding-control-iterative` conformance
/// fixtures (wired in `rewrite_conformance.rs`).
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

/// Ports `RewriteRuleTests.LongDistanceRules` (RewriteRuleTests.cs:66-162) -- the W9.2 probe (batch-5)
/// found the general rewrite-environment machinery already matches C# here (all 4 root causes wave-3
/// fixed applied), so this ports live with zero engine changes. 3 reconfigurations of `rule3`
/// (highVowel -> backRnd) with progressively longer/more-optional environments spanning multiple
/// segments, unapplied against "bubabu"/"mimuu": (1) a 4-segment LeftEnvironment
/// (rndVowel,cons,lowVowel,cons); (2) the mirror-image RightEnvironment; (3) a LeftEnvironment mixing
/// mandatory (highVowel, a literal "+" boundary) and optional (cons, cons, vowel) nodes, exercising
/// discontinuous morph reconstruction across `entry 55`'s own `+` boundary.
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

    // (3) LeftEnvironment = highVowel, cons?, "+", cons?, vowel? -- mandatory highVowel+boundary,
    // optional cons/cons/vowel around them.
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

/// Ports `RewriteRuleTests.QuantifierRules` (RewriteRuleTests.cs:247-347) -- same probe outcome as
/// [`long_distance_rules`] (batch-5, live, zero engine changes). `(cons lowVowel){1,2}` as a
/// REPEATED GROUP -- the DTD's only group-authoring mechanism outside `<MetathesisRule>`, an
/// `<OptionalSegmentSequence min max>` whose children are matched together as one repeated unit
/// (`hc-grammar/src/load.rs:890-906`'s `PatternNode::Quantifier { children, .. }`) -- is exactly how
/// C#'s `.Group(g => ...).LazyRange(lo,hi)` is represented here. `rule3`/`rule4` (mirror-image
/// Right/LeftEnvironment `{1,2}`-repeated groups) run together against 4 words; `rule1` (a `{0,2}`
/// group with NO trailing anchor segment) runs alone against a 5th.
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

    // rule1: LeftEnvironment = backRndVowel, (highVowel){0,2} -- no trailing anchor segment after the
    // repeated group.
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

/// Ports `RewriteRuleTests.MultipleSegmentRules`'s FIRST reconfiguration only (RewriteRuleTests.cs:
/// 384-399) -- same probe outcome as [`long_distance_rules`]/[`quantifier_rules`] for this half
/// (batch-5, live, zero engine changes). `rule1` has a genuinely TWO-SEGMENT `PhoneticInput`/
/// `PhoneticOutput` (highVowel,highVowel -> backRnd,backRnd), gated on a preceding backRndVowel. The
/// SECOND reconfiguration (adding `rule2`, a pure-deletion rule) surfaces a genuine finding --
/// [`multiple_segment_rules_deletion_composition_finding`].
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

/// Ports `RewriteRuleTests.MultipleSegmentRules`'s SECOND reconfiguration (RewriteRuleTests.cs:
/// 401-408): adding `rule2` (`t` -> nothing, gated on a following backRndVowel -- a pure-deletion
/// rule that never actually fires on "buuubuuu", no "t" is present) to the SAME stratum as
/// [`multiple_segment_rules`]'s `rule1` must still yield `{"27"}` per the C# oracle (cs:408).
///
/// **FIXED (P6).** The original triangulation (`Morpher::with_memo(false)` gives the same empty
/// result, ruling out the #451 analysis memo / `merge_equivalent` shape-fold; `rule2`/`rule1` each
/// analyze correctly alone; only the composition, analyzed in C#'s listed-order-then-reverse
/// convention, loses every candidate) was correct as far as it went, but its own HYPOTHESIS ("the
/// vacuous deletion candidate doesn't carry forward into the next rule's search") was refined by
/// direct tracing: `rule2`'s vacuous unapply is NOT vacuous in candidate-count terms at all --
/// `ana_narrow_deletion` legitimately inserts an OPTIONAL "t" node at every site where its
/// `RightEnvironment` (a following backRndV) holds, which on "buuubuuu" is 6 of the word's 8 gaps
/// (real deletion candidates C# is equally obligated to consider). That interposes an Optional
/// segment directly between the two real segments of each of `rule1`'s 2-segment target pairs.
///
/// The REAL root cause: `rule1`'s analysis target match (`hc_rules::rewrite::ana_feature`) recovered
/// each target-pattern row's matched segment via a positional `node_of[s..e]` slice of the overall
/// `ENTIRE_MATCH` span. `hc_fst::traverse::Transduce::advance`'s "skip the next Optional annotation"
/// mechanism (needed so a 2-segment target can transparently pass over the newly-interposed Optional
/// "t") reports every such match as a span WIDER than the pattern (it must span the skipped Optional
/// to reach the second real segment) -- and since, in this composition, NO alternative *tight*
/// (exactly-2-wide) match exists either (every candidate site has an Optional immediately inside the
/// pair), the pre-existing `width_matches` guard -- written on the assumption that a tight duplicate
/// always survives alongside a wide one -- discarded every candidate. `rule1` silently unapplied
/// nothing.
///
/// **Fix**: `ana_feature`'s target FST now compiles each target-pattern row in its own named
/// `CompileNode::Group` (`compile_lane_fst_grouped`, mirroring the real C#
/// `FeatureAnalysisRewriteRuleSpec.cs:48,68-71` `new Group("target"+i)` mechanism this port had not
/// yet used for its OWN target, only for its environments/`compile_parts`), and reads each row's real
/// matched segment from that group's own tag -- recovering the correct per-row position regardless of
/// interposed Optional segments, so `width_matches` is no longer needed at this call site.
/// Empirically probed (`hc-rules/src/rewrite.rs`'s `group_probe_diag` unit tests): WHICH tag half
/// (start vs end) is trustworthy is direction-dependent -- `LeftToRight` targets must read each row's
/// START (an entering tag is always freshly computed; only a row's END can be widened by a
/// *following* skip), while `RightToLeft` targets (the actual case here -- every reference-grammar
/// rule defaults to `Dir::LeftToRight`, and analysis always compiles the REVERSED direction) must
/// read each row's END instead, because the compiled node order is document-reversed AND
/// `Fst::get_offsets` swaps `(start,end)` back for that direction, which together flip which
/// physical tag half is corruption-free. `resolve_bindings`/`pattern_defaults_ok` were generalized
/// from an implicit `node_of[s+k]` contiguity assumption to an explicit `target_nodes: &[usize]`
/// parameter (the caller's own already-resolved node list) so they work for both the old
/// contiguous-slice callers (`syn_feature`/`syn_narrow`/`ana_narrow_general`, unchanged behavior) and
/// `ana_feature`'s new non-contiguous list.
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

/// Ports `RewriteRuleTests.DisjunctiveRules` (RewriteRuleTests.cs:1562-1806) -- same probe outcome as
/// [`long_distance_rules`]/[`quantifier_rules`] (batch-5, live, zero engine changes): 5
/// reconfigurations of a single rule (`disrule1`), each with 2+ DISJUNCTIVE `PhonologicalSubrule`s
/// (ordered alternatives -- the first whose own `Environment` matches wins, mirroring the earlier
/// `AffixProcessRuleTests.SuffixRules`-style ordered-subrule disjunction, now for phonology).
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

    // (2) `highFrontVowel` target across an unbounded `(cons highFrontVowel)*` stretch: 4 disjunctive
    // subrules, one per backness/roundness combination of the anchoring vowel.
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

    // (3) `stop` target again, via anchors instead of a bare environment-less "else": subrule(a)
    // word-initial -> asp; subrule(b) word-FINAL -> unasp (RightSideAnchor, not an "else" fallback).
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

    // (4) literal `p` target (bilabial voiceless unaspirated stop); subrule(a) intervocalic -> vd
    // (voice it); subrule(b) word-final -> asp.
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

    // (5) `voicelessStop` target; subrule(a) after another voicelessStop -> asp; subrule(b) (else)
    // -> unasp.
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
