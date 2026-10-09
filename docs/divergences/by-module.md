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
| `FeatureAnalysisRewriteRuleSpec.cs` (inverse StrRep, `IsUnapplicationNonvacuous`), `StringFeatureValue.cs` (`IsSupersetOf`) | 071 |
| `NaturalClass.cs` (ctor `Type` stamping) | 013 |
| `TraversalMethodBase.cs` (`Initialize`) | 013 (cited as not-a-bug) |
| `SimultaneousPhonologicalPatternRule.cs` (`Apply`) | 016 |
| `IterativePhonologicalPatternRule.cs` | 014 |
| `Word.cs` (`ExpandAlternatives`, realizational-FS diff) | 018 |
| `FeatureStruct.cs` (`Unify` out-param overload) | 018 |
| `Morpher.cs` (ctor, `_allomorphTries`, `IsPattern` partition) | 019 |
| rule-spec constructors (LHS/RHS child type casts) | 020 |
| `Morpher.cs` (`MaxStemCount`) | 022 |
| `AnalysisStateKey.cs` | 023, 024, 045 |
| `AnalysisScope.cs` | 023, 024, 025, 045 |
| `FeatureValue.cs` / `SimpleFeatureValue.cs` (shared variable/negation machinery) | 026 |
| `Morpher.cs` (`MatchNodesWithPattern`, `LexicalGuess`) | 027 |
| `HermitCrabExtensions.cs` | 027 |
| `Allomorph.cs` (`FreeFluctuatesWith`, disjunctive recheck) | 030 |
| FieldWorks `HCLoader.cs` (`LoadCircumfixAffixProcessAllomorph`) | 039 |
| FieldWorks `HCLoader.cs` (adhoc repositories, lines 340-350) | 053 |
| FieldWorks `HCLoader.cs` (`LoadRootAllomorph`, `GetValidEnvironments`, `IsValidRuleForm`) | 051 |
| `Stratum.cs` (`CharacterDefinitionTable` property) | 041 |
| `SynthesisStratumRule.cs`/`AnalysisStratumRule.cs` (`Apply`, `Word.Stratum` asymmetry) | 043 |
| `AnalysisMorphologicalTransform.cs` (`HasDisagreeingCopies`), `CopyAgreementPatternRule.cs` (PR #519) | 049 |
| `AnalysisRewriteRule.cs` (deletion unapplication), `AnalysisMetathesisRule.cs` | 050 |
| `TraversalMethodBase.cs` (`Advance`, `Initialize`), `Word.cs` (`ExpandAlternatives`), `Morpher.cs` (`MaxAlternatives`), FieldWorks `HCParser.cs` | 052 |
| (none — no C# equivalent) | 021, 028, 029, 040, 042, 044, 048 |

## By Rust module

| Rust file | Entry ids |
|---|---|
| `pg-grammar/src/compile/environment.rs`, `affixes.rs`, `warnings.rs`; `pg-cli/src/compile_failure.rs` | 051 |
| `pg-fwdata/src/xml.rs` (`ALLOWED_CLASSES`), `pg-fwdata/src/extract/inventory.rs` (`class_role`), `pg-fwdata/src/extract/morphology.rs` (`extract_adhoc_prohibitions`) | 053 |
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
| `pg-rules/src/rewrite.rs` (`ana_feature`) | 009, 015, 071 |
| `pg-rules/src/rewrite.rs` (`MutShape`, literal synthesis and inverse targets); `pg-rules/src/bridge.rs` (`StrRepMatcher`, `PatternBridge`) | 071 |
| `pg-parse/src/root_trie.rs` (`RootAllomorphIndex::search`, `RootAllomorphTrie::build`) | 009, 019 |
| `pg-grammar/src/chardef.rs` (`unif_closure`/`unifiable_cds`) | 010 |
| `pg-rules/src/rewrite.rs` (`syn_epenthesis`, `ana_epenthesis`) | 011, 013, 014 |
| `pg-rules/src/rewrite.rs` (`compile_lane_fst`) | 012 |
| `pg-rules/src/rewrite.rs` (`compile_lane_fst_grouped`) | 015 |
| `pg-rules/src/rewrite.rs` (`sim_feature`) | 016 |
| `pg-rules/src/rewrite.rs` (`width_matches`) | 020 |
| `pg-rules/src/bridge.rs` (`PatternBridge::nat_class_lanes`) | 013 |
| `pg-fst` (`Transduce::initialize`) | 013 (cited as not-a-bug) |
| `pg-foma/src/replace.rs` (`reversed_slots`, `compile_rtl_branch_net`) | 017 |
| `pg_lexicon::analysis`, FFI `hc_parse_word`/`hc_parse_batch` | 021 |
| `pg-parse/src/morpher.rs` (`Morpher::with_max_stem_count`) | 022 |
| _(was `pg-memo/src/lib.rs`, deleted)_ | 023, 025, 045 |
| `pg-rules/src/analysis_state_key.rs` | 024, 045 |
| `pg-rules/src/stratum.rs` (`state_key`) | 024, 045 |
| `pg-featstruct/src/tree.rs`, `pg-featstruct/src/ops.rs` | 026 |
| `pg-rules/src/rewrite.rs` (`bind_or_check`, `resolve_bindings`) | 026 |
| `pg-parse/src/guess.rs` | 019, 027 |
| `pg_foma::recipe_accuracy`, `pg_foma::parity::IdentityDivergence` | 028 |
| `pg_foma::emit` (`verify_tags_reachable`) | 029 |
| `pg-grammar/src/compile/affixes.rs` (`build_circumfix_allomorphs`) | 039 |
| `pg-foma/src/replace.rs` (`SegAlphabet::render_tokens`, `RepresentationAliasMap`, `compile_rewrite_rule_subset`, `compile_metathesis_swap_net`) | 040 |
| `pg-foma/src/replace.rs` (`pattern_slots`, `compile_rtl_branch_net`) | 017, 044 |
| `pg-foma/src/capability.rs` (`RightToLeftRewriteFaithfulReversalPredicate`) | 044 |
| `pg-rules/src/cache.rs` (`owning_table_for_prule`/`_metathesis_rule`/`_morpheme`/`_allomorph`/`_mrule`/`_compounding_rule`) | 041 |
| `pg-rules/src/metathesis.rs` (`synthesize`/`analyze` table resolution) | 041 |
| `pg-rules/src/metathesis.rs` (`synthesis_reorder`) | 042 |
| `pg-rules/src/stratum.rs` (`synthesize_stratum_traced`) | 043 |
| `pg-parse/src/morpher.rs` (`surface_of`, `is_match_traced`) | 042, 043 |
| `pg-cli/src/main.rs` (`run_batch`, `parse_batch_with_stats`) | 048 |
| `pg-cli/src/stats_cmd.rs` (`prepare_batch_stats_hc`, `finish_batch_stats_hc`) | 048 |
| `pg-rules/src/morph.rs` (`copy_agreement_refuses_match`, `ana_allomorph_matches`), `pg-rules/src/stratum.rs` (`AnalyzerConfig`) | 049 |
| `pg-rules/src/rewrite.rs` (deletion unapplication), `pg-rules/src/metathesis.rs` (analysis) | 050 |

| `pg-fst/src/work.rs`, `traverse.rs`; `pg-rules/src/stratum.rs`, `cascade.rs`, `word.rs`, `morph.rs`, `rewrite.rs`, `validity.rs`; `pg-parse/src/morpher.rs`, `root_trie.rs`, `overlay.rs`, `guess.rs`, `surface.rs` | 052 |

## By fixture / test file

| Fixture or test | Entry ids |
|---|---|
| `pg-rules/tests/analysis_syn_fs_gate.rs` | 001 |
| `pg-parse/tests/exact_analysis_fs_recall.rs` | 002 |
| `csharp_port_compounding.rs` | 003, 004, 005 |
| `machine/conformance/edge-cases/discontinuous-morph-environment/` | 006 |
| `csharp_port_affix_process.rs` | 007, 008 |
| `csharp_port_rewrite.rs` | 009, 010, 013, 014, 015 |
| `machine/conformance/edge-cases/iterative-epenthesis-cascade/` | 014 |
| `pg-rules/tests/rewrite_gate.rs` | 011, 012 |
| `machine/conformance/edge-cases/simultaneous-feeding/`, `simultaneous-feeding-control-iterative/` | 016 |
| `pg-rules/tests/unapplied_rule_counts_reader_gate.rs` | 024 |
| `conformance-staging/edge-cases/strrep-rewrite-unapplication/`, `pg-parse/tests/conformance_fixtures_gate.rs`, `pg-cli/src/tests.rs` (`analyses_sidecar_projects_source_guids_from_fwdata`), Machine `StrRepRewriteRuleTests.cs`, `pg-foma-backend/tests/backend_scoreboard_gate.rs` | 071 |

| `pg-parse/src/guess.rs` unit tests | 027 |
| `parity_divergence_census.rs` | 028 |
| `machine/conformance/edge-cases/disjunctive-recheck/`, `disjunctive_recheck_gate.rs` | 030 |
| `conformance-staging/edge-cases/circumfix-conditioned-halves/` (HCLoader shape; fwdata path unpinned) | 039 |
| `conformance-staging/edge-cases/two-table-shared-representation-recall/`, `pg-foma-backend/tests/two_table_shared_representation_recall.rs` | 040, 043 |
| `conformance-staging/edge-cases/multi-table-metathesis-shared-representation/`, `pg-foma-backend/tests/multi_table_metathesis_shared_representation.rs` | 040, 041, 042 |
| `conformance-staging/edge-cases/segment-natural-class-table-binding/`, `pg-foma-backend/tests/segment_natural_class_table_binding_discriminates.rs` | 010, 041 |
| `conformance-staging/edge-cases/right-to-left-segments-environment/`, `pg-foma/src/capability.rs` unit tests | 044 |
| `conformance-staging/edge-cases/right-to-left-cross-table-segments-environment/` | 044 |
| `pg-cli/src/stats_cmd.rs` (`batch_stats_parses_each_uncached_word_once`, `batch_stats_preserves_legacy_cache_and_tsv_across_thread_counts_and_options`) | 048 |
| `pg-parse/tests/csharp_port_affix_process.rs` (copy-agreement tests), `pg-rules/tests/stratum_gate.rs` (`copy_agreement_pruning_is_on_by_default`), `machine/conformance` `metathesis-phase-isolation` / `suffixing-extension-slot-ordering` reduplication words | 049 |
| `pg-fst/tests/fst.rs`, `pg-fst/src/work/tests.rs`, `pg-rules/tests/stratum_gate.rs`, `pg-rules/src/word/tests.rs`, `pg-parse/src/root_trie/tests.rs`, `pg-parse/tests/step_cap_work_gate.rs` | 052 |
| `pg-cli/tests/fwdata_conformance_gate.rs` (`grouped_fwdata_adhoc_rules_match_flat_hcloader_semantics`) | 053 |
| `pg-grammar/src/compile/tests.rs` provisional-definition snapshot regressions | 072, 073, 074, 075 |
| `pg-foma/src/lower/tests.rs::provisional_class_membership_reuses_the_model_decision` | 073, 074 |
| `pg-grammar/src/compile/tests.rs` membership and unconstrained wildcard regressions; `machine/conformance/edge-cases/chained-output-feature-override-loss`; separate-lane `underdefined/07-featureless-phoneme`, `underdefined/12-featureless-rule-class` on `feat/xample-measure` | 076 |
| `pg-parse/src/surface/tests.rs::literal_surface_matching_preserves_provisional_and_featureless_wildcards`; `pg-cli/tests/inferred_segment_engine_parity_gate.rs` | 076 |

## Provisional-definition seams

| C# site | Rust site | Entries |
|---|---|---|
| `CharacterDefinitionTable.GetShapeNodes`, `Segment` | `pg-grammar/src/compile/substrate.rs`, `mod.rs`, `options.rs` | [072](072-provisional-letter-unit.md), [075](075-provisional-definitions-always-on.md) |
| `CharacterDefinitionTable.Add`, feature unification | `pg-grammar-model/src/chardef.rs`, `membership.rs`, `segment.rs`; `pg-rules/src/bridge.rs`, `morph.rs`, `rewrite.rs` | [073](073-provisional-letter-no-natural-class.md), [074](074-featureless-phoneme-no-feature-class.md) |
| `CharacterDefinitionTable.Add`, feature unification | `pg-foma/src/lower.rs::class_members`, `structural_allomorph.rs::context_members` | [073](073-provisional-letter-no-natural-class.md), [074](074-featureless-phoneme-no-feature-class.md) |
| constrained-class subsumption / analysis-side unification | `pg-grammar-model/src/membership.rs::class_bits`, `segment.rs::nat_class_cd_set_with_constraints`; `pg-rules` matching and Foma owner calls | [076](076-underdefined-natural-class-membership-decision.md) |
| literal representation unification | `pg-parse/src/surface.rs::matching_reps_for_node`; `pg-grammar-model/src/membership.rs::literal_lanes`, `chardef.rs::literal_constraint_lanes` | [076](076-underdefined-natural-class-membership-decision.md) |

XAMPLE/C# measurements and staged fixtures for these entries come from a separate lane.

## FieldWorks authorability

| FieldWorks / Machine site | Rust site | Entry |
|---|---|---|
| `HCLoader.cs` `LoadMetathesisRule`, affix-process loading; `AnalysisMetathesisRuleSpec.cs` | `pg-foma/src/capability/variants.rs::permanent_refusal_reason`; `pg-foma-backend/tests/conformance_coverage_gate.rs` | [059](059-fieldworks-unauthorable-variants.md) |

## XAMPLE comparisons

| XAMPLE / FieldWorks export site | Rust site | Entry |
|---|---|---|
| `FxtM3ParserToXAmpleADCtl.xsl`, `NatClassStringToHvo` (feature class exported as `[]`) | `pg-grammar/src/compile/environment.rs::load_environment_pattern` | [070](070-xample-feature-class-environment.md) |

## Optimization and shared-correctness follow-up

| C# / Rust seam | Entries | Shared fixtures |
|---|---|---|
| Analysis cascade _(memo removed, 045)_ | 031, 045 | Cache-hit fixture never existed; now a C#-only coverage question |
| Template battery / `run_template_batch` _(memo removed, 045)_ | 032, 045 | `template-category-sharing` checks exclusivity; replay was never validated |
| Final-template state / `stratum.rs` policy | 033 | `final-template-partial-discriminators` |
| Stratum equivalence / `analyze_template` | 034 | Dedicated cross-engine collision fixture still missing |
| Template and slot merge / `run_template_batch_raw`, `apply_slot_batch` | 035 | `template-category-sharing` is not a collision discriminator |
| `ApplyRhs` / `attribute_morphs` | 036 | Stable shared zero-width fixture still missing |
| C# tied-node ordering / oracle comparison | 037 | Fresh-process identity pin still missing |
| Edge-segment matching / unported prefilter | 038 | Candidate only; no dedicated fixture |
| Reduplication copy agreement / `copy_agreement_refuses_match` | 049 | `metathesis-phase-isolation`, `suffixing-extension-slot-ordering` (Machine `f412c252`) |
| Deletion unapplication x metathesis across strata | 050 | Missing; repro in Machine #520 |
| Literal rewrite StrRep nonvacuity and inverse identity | 071 | `strrep-rewrite-unapplication`; synthetic kad/kat comparison; Machine proposal on local `fix/hc-strrep-unapplication`, PR pending lead publication |
