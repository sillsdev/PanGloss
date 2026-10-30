# Divergence catalogue: lookup by module

Two-way index. Find a C# source file or a Rust module below to get the entry ids that touch it.
See `README.md` for the full status table and the kind definitions.

## By C# file

| C# file | Entry ids |
|---|---|
| (none — no C# equivalent) | 028, 089 |
| `AnalysisAffixProcessRule.cs` | 117 |
| `AnalysisScope.cs` | 024, 045 |
| `AnalysisStateKey.cs` | 024, 045 |
| `FeatureValue.cs` / `SimpleFeatureValue.cs` (shared variable/negation machinery) | 026 |
| `HermitCrabExtensions.cs` | 027 |
| `Morpher.cs` (`MatchNodesWithPattern`, `LexicalGuess`) | 027 |
| FieldWorks `HCParser.cs` (`LoadParser`, `ProcessParseException`, `MaxAlternatives`); `Morpher.cs` (`SynthesizeSequential`/`SynthesizeParallel`); `AnalysisStratumRule.cs`, `CombinationRuleCascade.cs`, `LinearRuleCascade.cs`, `MemoizedCombinationRuleCascade.cs` | 097 |
| rule-spec constructors (LHS/RHS child type casts) | 020 |
| `TraversalMethodBase.cs` (`Advance`, `Initialize`), `Word.cs` (`ExpandAlternatives`), `Morpher.cs` (`MaxAlternatives`), FieldWorks `HCParser.cs` | 052 |

## By Rust module

| Rust file | Entry ids |
|---|---|
| _(was `pg-memo/src/lib.rs`, deleted)_ | 045 |
| `pg-cli/src/stored_analysis_health.rs`, `pg-cli/src/grammar_health.rs` | 086, 089 |
| `pg-featstruct/src/tree.rs`, `pg-featstruct/src/ops.rs` | 026 |
| `pg-fst/src/work.rs`, `traverse.rs`; `pg-rules/src/stratum.rs`, `cascade.rs`, `word.rs`, `morph.rs`, `rewrite.rs`, `validity.rs`; `pg-parse/src/morpher.rs`, `root_trie.rs`, `overlay.rs`, `guess.rs`, `surface.rs` | 052 |
| `pg-parse/src/guess.rs` | 027 |
| `pg-fwdata/src/parser_params.rs` (does not import `MaxAlternatives`); `pg-snapshot/src/morphology.rs` (`ParserParameters` has no field); `pg-parse/src/morpher.rs` (analysis/work caps, timeout, incomplete outcome) | 097 |
| `pg-rules/src/analysis_state_key.rs` | 024, 045 |
| `pg-rules/src/rewrite.rs` (`bind_or_check`, `resolve_bindings`) | 026 |
| `pg-rules/src/rewrite.rs` (`width_matches`) | 020 |
| `pg-rules/src/morph/edge_dispatch.rs`, `pg-rules/src/morph/edge_prefilter.rs`, `pg-rules/src/cache.rs`, `pg-rules/src/morph.rs` (`ana_affix_cached`), `pg-rules/src/stratum.rs` (`apply_one_mrule_uncounted`) | 117 |
| `pg-rules/src/stratum.rs` (`state_key`) | 024, 045 |
| `pg_foma::recipe_accuracy`, `pg_foma::parity::IdentityDivergence` | 028 |

## By fixture / test file

| Fixture or test | Entry ids |
|---|---|
| `pg-conformance-fixtures/src/tests.rs` (`a_morpher_block_the_harness_cannot_apply_is_refused`, `incomplete_replay_cannot_pass_as_an_expected_zero_analysis`) | 097 |
| `parity_divergence_census.rs` | 028 |
| `pg-cli/src/grammar_health/tests.rs` (`grammar_health_reports_a_capped_wordform_and_compares_the_rest`, `stored_analysis_comparison_stops_at_its_work_budget_identically_at_every_thread_count`, `stored_analysis_comparison_reports_every_capped_wordform_at_every_thread_count`, `grammar_health_stored_analyses_flag_selects_the_comparison`) | 089 |
| `pg-fst/tests/fst.rs`, `pg-fst/src/work/tests.rs`, `pg-rules/tests/stratum_gate.rs`, `pg-rules/src/word/tests.rs`, `pg-parse/src/root_trie/tests.rs`, `pg-parse/tests/step_cap_work_gate.rs` | 052 |
| `pg-grammar/src/compile/tests.rs` membership and unconstrained wildcard regressions; `machine/conformance/edge-cases/chained-output-feature-override-loss`; separate-lane `underdefined/07-featureless-phoneme`, `underdefined/12-featureless-rule-class` on `feat/xample-measure` | 076 |
| `pg-parse/src/guess.rs` unit tests | 027 |
| `pg-parse/src/surface/tests.rs::literal_surface_matching_preserves_provisional_and_featureless_wildcards`; `pg-cli/tests/inferred_segment_engine_parity_gate.rs` | 076 |
| `pg-rules/tests/unapplied_rule_counts_reader_gate.rs` | 024 |

## Provisional-definition seams

| C# site | Rust site | Entries |
|---|---|---|
| constrained-class subsumption / analysis-side unification | `pg-grammar-model/src/membership.rs::class_bits`, `segment.rs::nat_class_cd_set_with_constraints`; `pg-rules` matching and Foma owner calls | [076](076-underdefined-natural-class-membership-decision.md) |
| FieldWorks environment export / `HCLoader.InvalidEnvironment` | `pg-grammar/src/compile/environment.rs::nodes_from_spanned`, `mod.rs::Ctx::environment_resolution`; `pg-snapshot` warning metadata | [077](077-missing-natural-class-environment-info.md) |
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
| XAMPLE ignores authored phonology; FieldWorks/HC discard unreadable literal environments | authored rewrite rules and provisional literal environment; `pg-cli/tests/underdefined_stored_keys_gate.rs` | [078](078-xample-minimum-authored-phonology.md) |
| XAMPLE ignores authored phonology; FieldWorks stores analyses under `WfiWordform` | `pg-fwdata` stored-analysis extraction; `pg-parse` traced synthesis; `pg-cli` grammar health | [086](086-stored-analyses-lost-to-authored-phonology.md) |
