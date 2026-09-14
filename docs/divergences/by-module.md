# Divergence catalogue: lookup by module

Two-way index. Find a C# source file or a Rust module below to get the entry ids that touch it.
See `README.md` for the full status table and the kind/status/lifecycle definitions.

## By C# file

| C# file | Entry ids |
|---|---|
| `AnalysisAffixProcessRule.cs` | 001 |
| `AnalysisCompoundingRule.cs` | 001 |
| `AnalysisSyntacticFeatureMerge.cs` (research-only, `pr494` worktree) | 002 |
| `SynthesisCompoundingRule.cs` (`ApplySubrule`) | 003 |
| `Word.cs` (copy ctor, `_nonHeadApps`) | 003 |
| `Word.cs` (`CurrentNonHead`) | 004 |
| `AnalysisCompoundingRule.cs` (`Apply`, non-head-must-be-bare-root gate) | 005 |
| `CompoundingRuleTests.cs` | 003, 004, 005 |
| `Word.cs` (`GetMorphs`/`MarkMorphs`) | 006 |
| `Allomorph.cs` (`IsWordValid` environment clause) | 006, 030 |
| `SynthesisAffixProcessAllomorphRuleSpec.cs` (`ApplyRhs`) | 007, 008 |
| `CharacterDefinitionTable.cs` (`GetMatchingStrReps`, `Add`) | 007, 009, 010 |
| `CharacterDefinitionTable.cs` (`Add` / `FeatureStruct.IsUnifiable`) | 010 |
| `SynthesisRewriteRuleSpec.cs` (empty-LHS pattern walk) | 011, 013 |
| `PatternNode.GenerateNfa` | 012 |
| `FeatureAnalysisRewriteRuleSpec.cs` (`Group`) | 015 |
| `NaturalClass.cs` (ctor `Type` stamping) | 013 |
| `TraversalMethodBase.cs` (`Initialize`) | 013 (cited as not-a-bug) |
| `SimultaneousPhonologicalPatternRule.cs` (`Apply`) | 016 |
| `IterativePhonologicalPatternRule.cs` | 014 |
| `Word.cs` (`ExpandAlternatives`, realizational-FS diff) | 018 |
| `FeatureStruct.cs` (`Unify` out-param overload) | 018 |
| `Morpher.cs` (ctor, `_allomorphTries`, `IsPattern` partition) | 019 |
| rule-spec constructors (LHS/RHS child type casts) | 020 |
| `Morpher.cs` (`MaxStemCount`) | 022 |
| `AnalysisStateKey.cs` | 023, 024 |
| `AnalysisScope.cs` | 023, 024, 025 |
| `FeatureValue.cs` / `SimpleFeatureValue.cs` (shared variable/negation machinery) | 026 |
| `Morpher.cs` (`MatchNodesWithPattern`, `LexicalGuess`) | 027 |
| `HermitCrabExtensions.cs` | 027 |
| `Allomorph.cs` (`FreeFluctuatesWith`, disjunctive recheck) | 030 |
| (none — no C# equivalent) | 021, 028, 029 |

## By Rust module

| Rust file | Entry ids |
|---|---|
| `pg-rules/src/morph.rs` (`ana_syn_fs`) | 001, 002 |
| `pg-rules/src/morph.rs` (`synth_compound_subrule`) | 003 |
| `pg-rules/src/word.rs` (`current_non_head`) | 004 |
| `pg-rules/src/word.rs` (`expand_alternatives`) | 018 |
| `pg-rules/src/morph.rs` (`resolve_non_head_roots`) | 005 |
| `pg-rules/src/morph.rs` (`attribute_morphs`) | 006, 008 |
| `pg-rules/src/validity.rs` | 006, 030 |
| `pg-rules/src/morph.rs` (`copy_part`) | 007 |
| `pg-shape/src/lib.rs` (`Shape::node_cd_set`) | 007 |
| `pg-parse/src/surface.rs` (`matching_str_reps`, `matching_reps_for_node`) | 007, 010 |
| `pg-rules/src/rewrite.rs` (`ana_feature`) | 009, 015 |
| `pg-parse/src/root_trie.rs` (`RootAllomorphIndex::search`, `RootAllomorphTrie::build`) | 009, 019 |
| `pg-grammar/src/chardef.rs` (`unif_closure`/`unifiable_cds`) | 010 |
| `pg-rules/src/rewrite.rs` (`syn_epenthesis`) | 011, 013, 014 |
| `pg-rules/src/rewrite.rs` (`compile_lane_fst`) | 012 |
| `pg-rules/src/rewrite.rs` (`compile_lane_fst_grouped`) | 015 |
| `pg-rules/src/rewrite.rs` (`sim_feature`) | 016 |
| `pg-rules/src/rewrite.rs` (`width_matches`) | 020 |
| `pg-rules/src/bridge.rs` (`PatternBridge::nat_class_lanes`) | 013 |
| `pg-fst` (`Transduce::initialize`) | 013 (cited as not-a-bug) |
| `pg-foma/src/replace.rs` (`reversed_slots`, `compile_rtl_branch_net`) | 017 |
| `pg_lexicon::analysis`, FFI `hc_parse_word`/`hc_parse_batch` | 021 |
| `pg-parse/src/morpher.rs` (`Morpher::with_max_stem_count`) | 022 |
| `pg-memo/src/lib.rs` | 023, 024, 025 |
| `pg-rules/src/stratum.rs` (`state_key`) | 024 |
| `pg-featstruct/src/tree.rs`, `pg-featstruct/src/ops.rs` | 026 |
| `pg-rules/src/rewrite.rs` (`bind_or_check`, `resolve_bindings`) | 026 |
| `pg-parse/src/guess.rs` | 019, 027 |
| `pg_foma::recipe_accuracy`, `pg_foma::parity::IdentityDivergence` | 028 |
| `pg_foma::emit` (`verify_tags_reachable`) | 029 |

## By fixture / test file

| Fixture or test | Entry ids |
|---|---|
| `pg-rules/tests/analysis_syn_fs_gate.rs` | 001 |
| `pg-parse/tests/exact_analysis_fs_recall.rs` | 002 |
| `csharp_port_compounding.rs` | 003, 004, 005 |
| `rust/conformance/allomorphy/discontinuous-env/`, `discontinuous_env_gate.rs` | 006 |
| `csharp_port_affix_process.rs` | 007, 008 |
| `csharp_port_rewrite.rs` | 009, 010, 013, 014, 015 |
| `pg-rules/tests/rewrite_gate.rs` | 011, 012 |
| `rust/conformance/rewrite/simultaneous-feeding*` | 016 |
| `pg-rules/tests/memo_gate.rs`, `unapplied_rule_counts_reader_gate.rs` | 024 |
| `pg-parse/src/guess.rs` unit tests | 027 |
| `parity_divergence_census.rs` | 028 |
| `rust/conformance/allomorphy/disjunctive-recheck/`, `disjunctive_recheck_gate.rs` | 030 |
