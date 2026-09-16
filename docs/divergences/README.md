# HC-Rust / hc.dll divergence catalogue

This folder is the standing ledger of every place `pg-parse`/`pg-rules`/`pg-featstruct`/`pg-memo`/
`pg-grammar` (the Rust port, "HC-Rust") is known or suspected to differ from the founding oracle:
`SIL.Machine.Morphology.HermitCrab`'s C# implementation (`hc.dll`) in
`C:\Users\johnm\Documents\repos\machine`. Per the repo rule, HC-Rust must never produce a different
parse than C# without that difference being documented here (and, if Rust believes its behaviour is
an improvement, reported in a Machine issue, with a fix PR when available, rather than silently kept).

## What counts as an entry

One entry per distinct mechanism-level difference between the two engines, traceable to a specific
C# file/method and a specific Rust file/function. A regression test that merely re-confirms an
already-fixed divergence stays green is not a new entry; a test that is the ONLY thing standing
between "fixed" and "silently reverted" is exactly the evidence an entry should cite.

## The five kinds

- **`behavioural`** — the two implementations can produce different parses/analyses for some input.
  The serious kind. Every entry of this kind states which side is believed correct, whether a
  fixture pins it, and whether an upstream PR exists or is needed.
- **`efficiency`** — same observable parses, different work or memory (memoization, caches, indexes,
  gates that only prune provably-dead branches). Allowed by repo policy, but each entry must explain
  *why* the shortcut cannot change a parse, not just assert it.
- **`representational`** — same behaviour, different data model or API shape (an `Option<FeatureStruct>`
  where C# has a null, an interned id where C# has object identity, a `BTreeMap` where C# has a
  `Dictionary` plus a hand-rolled XOR hash). Each entry flags whether the representation could become
  behavioural under a future change.
- **`unported`** — a C# feature/branch Rust does not implement at all, or vice versa. Each entry says
  what happens today if that path is reached: panic, refusal, or (worst case) silent under-handling.
- **`stale-claim`** — a documented divergence that no longer exists, or a comment whose claim could
  not be verified against the current source of either engine.

## File naming

One file per entry: `docs/divergences/NNN-short-slug.md`, `NNN` a zero-padded three-digit number
assigned in discovery order (never reused, never renumbered — a retired entry keeps its number and
gets `Status: reverted-in-rust` or similar rather than being deleted).

## Finding the entry for a file

- **From a C# file:** grep this folder's `## C# site` lines for the filename, or use
  `by-module.md`'s C#-side table.
- **From a Rust module:** grep this folder's `## Rust site` lines for the module/function name, or
  use `by-module.md`'s Rust-side table.
- **From this README's status table below:** every entry is listed with both sites, kind, status,
  and pinning fixture in one row.

## Entry lifecycle

An entry's `Status` is one of:

- **`open`** — unresolved behavior, compatibility research, or missing discriminating evidence.
  State separately what is implemented in Rust and C#, and whether a bug is reproduced or suspected.
- **`superseded`** — a later algorithm replaced this Rust implementation; preserve its history and link the successor.
- **`proposed-upstream`** — Rust's behaviour is believed better than hc.dll's, and a PR against
  `sillsdev/machine` proposing hc.dll adopt it has been filed (name the PR number).
- **`accepted-upstream`** — that PR merged; hc.dll now matches Rust, so the divergence is closed from
  the C# side.
- **`reverted-in-rust`** — Rust changed to match hc.dll instead of pursuing an upstream change.
- **`fixed-in-rust`** — Rust had a genuine bug (its output differed from hc.dll's), and it has been
  fixed to match hc.dll, verified against the oracle or a fixture. No proposal to hc.dll was needed
  because hc.dll was already correct.
- **`not-a-bug`** — investigated as a suspected divergence and found, on reading both sources
  directly, not to be one (a misdiagnosis, a stale suspicion, or a difference that provably cannot
  affect any parse).

A `stale-claim` entry uses whichever of the above best describes the current truth, plus a note on
what the original claim got wrong.

## Status table

| id | title | kind | status | C# site | Rust site | pinning fixture/test |
|---|---|---|---|---|---|---|
| 001 | [Analysis syntactic-FS fold: Add vs PriorityUnion](001-ana-syn-fs-add-vs-priority-union.md) | behavioural | superseded | `AnalysisAffixProcessRule.cs`/`AnalysisCompoundingRule.cs` (`.Add`) | `pg-rules/src/morph.rs::ana_syn_fs` | `pg-rules/tests/analysis_syn_fs_gate.rs` |
| 002 | [Analysis syntactic-FS fold: exact inverse of synthesis](002-ana-syn-fs-exact-inverse.md) | behavioural | open | `AnalysisSyntacticFeatureMerge.cs` (research-only `Exact` mode, unmerged) | `pg-rules/src/morph.rs::ana_syn_fs` (main, `149f88df`) | `pg-parse/tests/exact_analysis_fs_recall.rs` |
| 003 | [Compounding homophone-disjunction collapse](003-compounding-homophone-collapse.md) | behavioural | fixed-in-rust | `SynthesisCompoundingRule.ApplySubrule` / `Word` copy ctor | `pg-rules/src/morph.rs::synth_compound_subrule` | `csharp_port_compounding.rs::simple_rules_1_homophone_disjunction_finding` |
| 004 | [`Word::current_non_head()` index vs last-element](004-current-non-head-index.md) | behavioural | fixed-in-rust | `Word.CurrentNonHead` (Word.cs) | `pg-rules/src/word.rs::current_non_head` | `csharp_port_generation.rs::direct_api_compounding_two_non_heads_resolve_distinct_slots` |
| 005 | [Prefix-commutes-with-compounding misdiagnosis](005-prefix-compounding-misdiagnosis.md) | stale-claim | not-a-bug | `AnalysisCompoundingRule.Apply` | `pg_rules::morph::resolve_non_head_roots` | `csharp_port_compounding.rs::simple_rules_3_prefix_commutes_with_compounding` |
| 006 | [Discontinuous-morph environment anchoring (W3.3)](006-discontinuous-morph-env-anchor.md) | behavioural | fixed-in-rust | `Word.GetMorphs`/`MarkMorphs` (Word.cs) | `pg-rules/src/validity.rs`, `pg-rules/src/morph.rs::attribute_morphs` | **UNPINNED** — both former pins died with the v1 fixture layout; see the entry |
| 007 | [`ModifyFromInput` char-def staleness](007-modify-from-input-char-def-staleness.md) | behavioural | fixed-in-rust | `SynthesisAffixProcessAllomorphRuleSpec.ApplyRhs` | `pg-rules/src/morph.rs::copy_part` | `csharp_port_affix_process.rs::simulfix_rules`/`modify_from_input_rules` |
| 008 | [Morph-attribution drop on input-morph subsumption](008-morph-attribution-subsumption.md) | behavioural | fixed-in-rust | `SynthesisAffixProcessAllomorphRuleSpec.ApplyRhs`, `MarkSubsumedMorph`/`MarkMorph` | `pg-rules/src/morph.rs::attribute_morphs` | `csharp_port_affix_process.rs::subsumed_affix_findings` |
| 009 | [Analysis-side `char_def` staleness in rewrite rules](009-ana-feature-char-def-staleness.md) | behavioural | fixed-in-rust | `CharacterDefinitionTable.GetMatchingStrReps` | `pg-rules/src/rewrite.rs::ana_feature` | `csharp_port_rewrite.rs::common_feature_rules`/`boundary_rules` |
| 010 | [Root lookup needs unification, not char-def identity, on feature-bearing tables](010-anchor-rules-root-unification.md) | behavioural | fixed-in-rust | `CharacterDefinitionTable.Add`/`FeatureStruct.IsUnifiable` | `pg-grammar/src/chardef.rs` (`unif_closure`), `pg-parse/src/root_trie.rs`, `pg-parse/src/surface.rs` (`matching_reps_for_node`) | `csharp_port_rewrite.rs::anchor_rules`; `conformance-staging/edge-cases/segment-natural-class-table-binding` (2026-09-02 update) |
| 011 | [Missing word-initial epenthesis synthesis site](011-epenthesis-word-initial-site.md) | behavioural | fixed-in-rust | `SynthesisRewriteRuleSpec` pattern walk | `pg-rules/src/rewrite.rs::syn_epenthesis` | `pg-rules/tests/rewrite_gate.rs::epenthesis_synthesis_word_initial_site` |
| 012 | [RTL analysis direction inversion for multi-node targets](012-epenthesis-rtl-direction-inversion.md) | behavioural | fixed-in-rust | `PatternNode.GenerateNfa` | `pg-rules/src/rewrite.rs::compile_lane_fst` | `pg-rules/tests/rewrite_gate.rs::epenthesis_analysis_multi_node_target_matches_document_order` |
| 013 | [Boundary node counted as a spurious epenthesis site](013-epenthesis-boundary-spurious-site.md) | behavioural | fixed-in-rust | `SynthesisRewriteRuleSpec.cs:26-29` (empty-LHS pattern) | `pg-rules/src/rewrite.rs::syn_epenthesis` | `csharp_port_rewrite.rs::epenthesis_rules` |
| 014 | [Iterative epenthesis cascading is unimplemented](014-iterative-epenthesis-cascading.md) | behavioural | open | `IterativePhonologicalPatternRule` | `pg-rules/src/rewrite.rs::syn_epenthesis` | `csharp_port_rewrite.rs::epenthesis_rules_iterative_cascade_finding` (`#[ignore]`d) |
| 015 | [Deletion-composition loses candidates behind an interposed Optional](015-deletion-composition-optional-interposed.md) | behavioural | fixed-in-rust | `FeatureAnalysisRewriteRuleSpec.cs:48,68-71` (`Group`) | `pg-rules/src/rewrite.rs::ana_feature` (`compile_lane_fst_grouped`) | `csharp_port_rewrite.rs::multiple_segment_rules_deletion_composition_finding` |
| 016 | [`RewriteMode::Simultaneous` real semantics](016-simultaneous-rewrite-mode.md) | behavioural | fixed-in-rust | `SimultaneousPhonologicalPatternRule.Apply` | `pg-rules/src/rewrite.rs::sim_feature` | **UNPINNED** — the `simultaneous-feeding` fixtures did not survive the v1 -> v2 migration |
| 017 | [RTL + bounded quantifier: shallow-reverse mirror risk](017-rtl-bounded-quantifier-shallow-reverse.md) | behavioural | open | (no C# equivalent path; structural risk in the Rust FST builder) | `pg-foma/src/replace.rs::reversed_slots`, `compile_rtl_branch_net` | none — unreproduced by design |
| 018 | [Realizational-FS diff: Rust avoids a latent C# null-crash](018-realizational-fs-diff-null-avoidance.md) | behavioural | open | `Word.ExpandAlternatives`/`FeatureStruct.Unify` (Word.cs:515-524) | `pg-rules/src/word.rs::expand_alternatives` | none — not exercised by any oracle-verified fixture |
| 019 | [Root-allomorph trie previously mis-indexed pattern allomorphs](019-root-trie-pattern-indexing.md) | behavioural | fixed-in-rust | `Morpher.cs:39-47` (partition of pattern vs. non-pattern allomorphs) | `pg-parse/src/root_trie.rs::RootAllomorphTrie::build` | (see module doc; no dedicated named test found — unverified beyond the doc comment) |
| 020 | [Bounded `Quantifier` spanning a whole LHS/RHS: silently inert, not refused](020-bounded-quantifier-whole-pattern-unsupported.md) | unported | open | rule-spec constructors (cast every LHS/RHS child to a non-quantifier type; throws on load) | `pg-rules/src/rewrite.rs::width_matches` and callers | none |
| 021 | [Guesser FFI symbols could return an unmarked guessed analysis](021-guesser-ffi-unmarked-overclaim.md) | behavioural | fixed-in-rust | (no C# equivalent — FFI-boundary-only defect) | `pg_lexicon::analysis`, `hc_parse_word`/`hc_parse_batch` | `docs/hermitcrab-rust-port-audit.md` §3a |
| 022 | [`Morpher.MaxStemCount`: hardcoded then made configurable](022-max-stem-count-configurable.md) | representational | fixed-in-rust | `Morpher.cs:56,72` | `pg-parse/src/morpher.rs::Morpher::with_max_stem_count` | `csharp_port_compounding.rs` (`SimpleRules` final reconfiguration) |
| 023 | [`AnalysisStateKey` representation: interned ids vs live references](023-analysis-state-key-representation.md) | representational | open | `AnalysisStateKey.cs:26-34` | `pg-memo/src/lib.rs` | none — structural, not test-pinned |
| 024 | [Analysis memo key saturates unapplication counts; C# does not](024-memo-key-count-saturation.md) | representational | open | `AnalysisStateKey.cs:14-34` | `pg-rules/src/stratum.rs::state_key` | `pg-rules/tests/memo_gate.rs::state_key_saturates_unapplication_counts_past_max_apps`, `pg-rules/tests/unapplied_rule_counts_reader_gate.rs` |
| 025 | [Two in-flight re-entrancy guards where C# has one](025-two-in-flight-guards.md) | representational | open | `AnalysisScope.cs:56-60` (`InProgress`) | `pg-memo/src/lib.rs` (`in_flight`, `template_in_progress`) | none — claimed correctness-neutral by construction |
| 026 | [Syntactic-domain feature structures are provably variable-free; phonological variables live in a separate mechanism](026-featstruct-variable-domain-split.md) | representational | open | `FeatureValue`/`SimpleFeatureValue` (shared variable machinery for both domains) | `pg-featstruct/src/{tree,ops}.rs` vs `pg-rules/src/rewrite.rs` (`bind_or_check`) | none — argued sound by construction in `ops.rs`'s module doc |
| 027 | [Guesser matcher bypasses the FST engine, and is bug-compatible with C#'s single-owning-entry fabrication](027-guesser-bypasses-fst.md) | representational | open | `Morpher.MatchNodesWithPattern`/`Morpher.LexicalGuess` (Morpher.cs:522-625) | `pg-parse/src/guess.rs` | `pg-parse/src/guess.rs` unit tests (ported from `MorpherTests.TestMatchNodesWithPattern`) |
| 028 | [Confirmation-free accuracy screen: an unmeasured-until-now soundness hazard](028-confirmation-free-accuracy-hazard.md) | efficiency | open | (no C# equivalent — Rust-only evaluation harness) | `pg_foma::recipe_accuracy`, `pg_foma::parity::IdentityDivergence` | `parity_divergence_census.rs`; `candidate_only_identities` measured 0 on current fixtures |
| 029 | [Declared morpheme tags can vanish from the compiled lexc alphabet at depth](029-lexc-tag-alphabet-vanishing.md) | unported | open | (no C# equivalent — Rust-only foma compilation path) | `pg_foma::emit` (`verify_tags_reachable`) | `EmitReport::uncovered` (`kind: "unreachable-after-lexc-compile"`) |
| 030 | [Disjunctive-allomorph / free-fluctuation re-check (W3.2), formerly deferred](030-disjunctive-recheck-w32.md) | behavioural | fixed-in-rust | `Allomorph.IsWordValid`'s second loop (Allomorph.cs:127-152) | `pg-rules/src/validity.rs` (`allomorphs_valid_impl`, `MorphRecord::passed_over`) | `machine/conformance/edge-cases/disjunctive-recheck/`, `pg-parse/tests/disjunctive_recheck_gate.rs` |
| 031 | [Analysis cascade memoization](031-analysis-cascade-memo.md) | efficiency | open | `AnalysisStratumRule / AnalysisScope` | `pg-rules/src/stratum.rs::memo_apply_rules` | See entry: implementation, fixture and evidence status are separate |
| 032 | [Template-battery memoization](032-template-battery-memo.md) | efficiency | open | `AnalysisAffixTemplatesRule / AnalysisScope` | `pg-rules/src/stratum.rs::run_template_batch` | See entry: implementation, fixture and evidence status are separate |
| 033 | [Final-template analysis pruning](033-final-template-pruning.md) | efficiency | open | `AnalysisStratumRule / Word.FinalTemplateState` | `pg-rules/src/stratum.rs final-template policy` | See entry: implementation, fixture and evidence status are separate |
| 034 | [Stratum analysis-state merging](034-stratum-merge-equivalence.md) | behavioural | open | `AnalysisStratumRule.MergeEquivalentAnalyses / AnalysisAffixTemplateRule` | `pg-rules/src/stratum.rs analysis merge and analyze_template` | See entry: implementation, fixture and evidence status are separate |
| 035 | [Template/slot feature collisions and widening](035-template-slot-fs-collisions.md) | behavioural | open | `AnalysisAffixTemplatesRule / AnalysisAffixTemplateRule` | `pg-rules/src/stratum.rs::run_template_batch_raw / apply_slot_batch` | See entry: implementation, fixture and evidence status are separate |
| 036 | [Zero-width morpheme identity preservation](036-zero-width-morpheme-identity.md) | behavioural | open | `SynthesisAffixProcessAllomorphRuleSpec.ApplyRhs / MarkMorph` | `pg-rules/src/morph.rs::attribute_morphs` | See entry: implementation, fixture and evidence status are separate |
| 037 | [Oracle annotation ordering instability](037-oracle-annotation-ordering.md) | behavioural | open | `BidirList / tied-node annotation ordering (suspected)` | `pg-parse oracle comparison dependency; no BidirList port claim` | See entry: implementation, fixture and evidence status are separate |
| 038 | [Edge-segment prefilter candidate](038-edge-segment-prefilter.md) | efficiency | open | `AnalysisAffixProcessAllomorphRuleSpec candidate matching` | `No current Rust port identified` | See entry: implementation, fixture and evidence status are separate |
| 039 | [fwdata circumfix conditioning encoding](039-fwdata-circumfix-conditioning-encoding.md) | behavioural | fixed-in-rust | `HCLoader.LoadCircumfixAffixProcessAllomorph` (FieldWorks) | `pg-grammar/src/compile/affixes.rs::build_circumfix_allomorphs` | `pg-grammar/tests/circumfix_conditioning_parity.rs::circumfix_cross_product_with_conditioned_halves_parses_all_four_cells`; `conformance-staging/edge-cases/circumfix-conditioned-halves` pins the HCLoader shape on the XML-authored path |
| 040 | [FST rewrite/metathesis compilation was blind to cross-table shared representations](040-fst-cross-table-representation-aliasing.md) | behavioural | fixed-in-rust | (no C# equivalent — no FST precompilation stage exists) | `pg-foma/src/replace.rs` (`SegAlphabet::render_tokens`, `RepresentationAliasMap`, `compile_rewrite_rule_subset`, `compile_metathesis_swap_net`) | `two-table-shared-representation-recall`, `multi-table-metathesis-shared-representation` |
| 041 | [Oracle-side (`pg-rules`) phonological/metathesis/allomorph resolution defaulted to table 0](041-oracle-table-zero-default.md) | behavioural | fixed-in-rust | `Stratum.CharacterDefinitionTable` (`Stratum.cs:95`) | `pg-rules/src/cache.rs` (`owning_table_for_*`), `pg-rules/src/metathesis.rs`, `pg-rules/src/morph.rs`, `pg-rules/src/rewrite.rs` | `segment-natural-class-table-binding`, `multi-table-metathesis-shared-representation`, `pg-rules/src/cache.rs::owning_table_tests` |
| 042 | [Metathesis relocation left a stale origin-table char-def identity at the surface-match gate](042-metathesis-relocation-stale-table-identity.md) | behavioural | fixed-in-rust | (no C# equivalent — HermitCrab has exactly one table per grammar) | `pg-rules/src/metathesis.rs::synthesis_reorder`, `pg-parse/src/morpher.rs::is_match_traced` | `multi-table-metathesis-shared-representation` |
| 043 | [Synthesis-direction stratum reassignment diverged from hc.dll's surface rendering](043-synthesis-stratum-reassignment-reverted.md) | behavioural | reverted-in-rust | `SynthesisStratumRule.Apply`/`AnalysisStratumRule.Apply` (`Stratum.cs`) | `pg-rules/src/stratum.rs::synthesize_stratum_traced`, `pg-parse/src/morpher.rs::surface_of` | `two-table-shared-representation-recall` |
| 044 | [RTL rewrite-rule FST construction refused `Segments`-shaped patterns entirely](044-rtl-segments-pattern-coverage.md) | unported | fixed-in-rust | (no C# equivalent — no FST-compilability gate exists) | `pg-foma/src/replace.rs` (`pattern_slots`, `compile_rtl_branch_net`), `pg-foma/src/capability.rs` (`RightToLeftRewriteFaithfulReversalPredicate`) | `right-to-left-segments-environment`, `right-to-left-cross-table-segments-environment` |

## Evidence and upstream reporting

For each active shared concern, record a Machine issue link, any fix PR or posted PR comment,
Rust implementation status, exact fixture location, and what was actually run at which commits.
An issue is not a fix PR; a posted comment is not a patch; a unit test is not a conformance grammar.
A green test with the fix disabled is not evidence that the fix is necessary. A grammar mutation
proves fixture sensitivity, not an engine fix-removed regression. Compare complete identity
multisets and statuses; skips, caps, missing rows and timeouts are not alignment passes.
Rust-only implementation bugs need not become Machine issues. Shared semantic concerns and
C# oracle dependencies do; classify hypotheses and coverage gaps without presenting them as bugs.

Audit: PanGloss `0006b938` (including Exact `149f88df`); Machine conformance base `8bad1934`;
Machine #493 merged at `52d069f8`. Live PR states checked 2026-09-15. Historical notes below
`history/` are dated evidence, not substitutes for checking the current head.
