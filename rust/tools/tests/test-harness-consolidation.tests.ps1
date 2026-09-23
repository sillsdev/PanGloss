. "$PSScriptRoot\_test-harness.ps1"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path

function Get-TestTargets {
    param([string]$Text)
    $targets = @()
    foreach ($table in [regex]::Matches($Text, '(?ms)^\[\[test\]\]\s*(?<body>.*?)(?=^\[\[|\z)')) {
        $name = [regex]::Match($table.Groups['body'].Value, '(?m)^name\s*=\s*"([^"]+)"\r?$')
        $path = [regex]::Match($table.Groups['body'].Value, '(?m)^path\s*=\s*"([^"]+)"\r?$')
        if ($name.Success -and $path.Success) {
            $targets += [PSCustomObject]@{ Name = $name.Groups[1].Value; Path = $path.Groups[1].Value }
        }
    }
    return $targets
}

function Get-TargetModuleImports {
    param([string]$HarnessPath)
    $imports = @()
    $pattern = '(?ms)^\s*#\[path\s*=\s*"(?<path>[^"]+)"\]\s*\r?\n\s*mod\s+(?<module>[A-Za-z_][A-Za-z0-9_]*)\s*;'
    foreach ($match in [regex]::Matches((Get-Content -Raw -LiteralPath $HarnessPath), $pattern)) {
        $full = [System.IO.Path]::GetFullPath((Join-Path (Split-Path -Parent $HarnessPath) $match.Groups['path'].Value))
        $imports += [PSCustomObject]@{ Module = $match.Groups['module'].Value; Path = $full }
    }
    return $imports
}

function Get-RelativeTestPath {
    param([string]$TestsRoot, [string]$Path)
    [System.IO.Path]::GetRelativePath($TestsRoot, $Path).Replace('\', '/')
}

$crateSpecs = @(
    @{
        Name = 'pg-foma'
        PackageRoot = Join-Path $repoRoot 'rust\crates\pg-foma'
        MinTargets = 11
        MaxTargets = 11
        ExpectedSourceNames = @(
            'all_fixtures_foma_analyzer_new_no_panic.rs', 'candidate_filter_passes.rs',
            'f0_viability.rs', 'f1_large_lexicon_gate.rs', 'f2_junction_gate.rs',
            'f3_interdigitation_gate.rs', 'f4_composite_gate.rs', 'mbugwe_corpus_smoke_gate.rs',
            'p6_gate_parity.rs', 'phase_c_alpha_scale.rs', 'phase_c_circumfix.rs', 'phase_c_compounding.rs',
            'phase_c_metathesis.rs', 'phase_c_multi_table.rs', 'phase_c_partition_k.rs',
            'phase_c_quantifier.rs', 'phase_c_right_to_left.rs', 'phase_c_simultaneous.rs',
            'phase_c_strata_depth.rs', 'pk1_precision_recall_invariance.rs'
        )
    }
    @{
        Name = 'pg-foma-backend'
        PackageRoot = Join-Path $repoRoot 'rust\crates\pg-foma-backend'
        MinTargets = 9
        MaxTargets = 9
        ExpectedSourceNames = @(
            'admission_single_owner_gate.rs', 'advice_catalog_contract.rs',
            'apply_path_refusal_gate.rs', 'atomic_template_slot_carrier_gate.rs',
            'backend_accuracy_gate.rs', 'backend_capability_cards_contract.rs',
            'backend_emission_strategy_gate.rs', 'backend_mechanism_graph.rs',
            'backend_optimizer_calibration.rs', 'backend_partition_refinement_gate.rs',
            'backend_promoted_fixtures.rs', 'backend_registry_census.rs',
            'backend_runtime_cache_gate.rs', 'backend_runtime_net_is_queryable_gate.rs',
            'backend_runtime_oracle_bound_gate.rs', 'backend_scoreboard_gate.rs',
            'backend_seam_gate.rs', 'backend_selection_contract.rs',
            'bare_root_compile_time_discharge.rs',
            'bistratal_overlapping_segment_representation_foma_analyzer_compiles.rs',
            'boundary_marker_epsilon_collapse_gate.rs', 'candidate_filter_contract.rs',
            'candidate_filter_fixture_weight.rs', 'candidate_filter_model_check.rs',
            'candidate_filter_shadow_gate.rs', 'circumfix_candidate_selection.rs',
            'circumfix_cross_product_and_infix_drop_candidate_selection.rs',
            'closure_unbounded_realizational.rs', 'conformance_coverage_gate.rs',
            'cover_bistratal_overlapping_segment_representation.rs', 'cover_compounding.rs',
            'cover_compounding_recursive_depth_bound.rs', 'cover_mpr_groups.rs',
            'cover_realizational_morphology_constraints.rs',
            'cover_recursive_endocentric_compounding.rs',
            'cover_right_to_left_bounded_quantifier_rewrite.rs',
            'cover_subrule_morphosyntactic_gating.rs', 'cover_unordered_morph_rules.rs',
            'coverage_citation_liveness.rs', 'cross_compiler_equivalence_gate.rs',
            'cross_table_root_respelling_gate.rs', 'deletion_reduplication_exception_fixture.rs',
            'deterministic_eligibility_gate.rs',
            'emit_underlying_templated_recursive_compound_chain.rs',
            'emit_underlying_templated_tag_reachability_gate.rs',
            'envelope_agrees_with_compiler_gate.rs', 'epenthesis_structural_route_containment.rs',
            'exercises_tag_liveness.rs', 'f3_parity.rs', 'f5_diacritics_gate.rs',
            'f6_reduplication_peel_chain_depth.rs', 'faithfulness_coverage_gate.rs',
            'five_language_backend_reports_gate.rs', 'flag_replace_scope.rs',
            'grammar_semantics_owner_gate.rs', 'health_finding_seam.rs',
            'late_structural_anchor_recall.rs', 'mechanism_provider_gate.rs',
            'mentanukam_multiplicity_recovered_by_confirm.rs', 'morphology_relation_plan_gate.rs',
            'morphotactics_boundary_cleanup_slice.rs', 'multi_table_metathesis_shared_representation.rs',
            'net_dedup_gate.rs', 'net_dedup_sizing_census.rs', 'net_shape_gate.rs',
            'oracle_step_determinism_gate.rs', 'orthogonal_basis_group_a.rs',
            'orthogonal_basis_group_b.rs', 'parity_divergence_census.rs',
            'partial_fst_production_admission_gate.rs', 'pattern_root_regex_route_gate.rs',
            'pattern_root_token_route_gate.rs', 'pk2_eliminate_flag_oracle.rs',
            'p6_templated_morphotactics_gate.rs', 'phase_c_chain_scale.rs',
            'plan_composed_marker_material_gate.rs', 'plan_interaction_coverage_gate.rs',
            'predicate_negative_witness_gate.rs', 'process_morphology_route_gate.rs',
            'realizational_pc_represents_gate.rs',
            'segment_natural_class_table_binding_discriminates.rs',
            'simultaneous_overlap_capability_refuses.rs', 'strategy_aware_capability_gate.rs',
            'strategy_coverage_join_gate.rs', 'structural_witness_gate.rs',
            'subrecipe_dossier_contract.rs', 'templated_circumfix_recall_parity.rs',
            'templated_conformance_proposal_pins.rs', 'templated_morphology_classifier_gate.rs',
            'templated_token_cascade_phonology_free_routing_gate.rs',
            'trusted_selected_build_gate.rs', 'two_table_shared_representation_recall.rs',
            'two_table_symbol_divergence.rs', 'typology_speedup.rs', 'uflexc_compound_loop.rs',
            'witnessed_strategy_coverage_gate.rs', 'worker_execution_limits_contract.rs'
        )
    }
    @{
        Name = 'pg-parse'
        PackageRoot = Join-Path $repoRoot 'rust\crates\pg-parse'
        MinTargets = 3
        MaxTargets = 3
        ExpectedSourceNames = @(
            'batch_determinism.rs', 'cd_set_gate.rs', 'conformance_fixtures_gate.rs',
            'cross_table_metathesis_surface_match_gate.rs', 'csharp_port_affix_process.rs',
            'csharp_port_affix_template.rs', 'csharp_port_compounding.rs',
            'csharp_port_generation.rs', 'csharp_port_lex_entry.rs', 'csharp_port_metathesis.rs',
            'csharp_port_morpher.rs', 'csharp_port_rewrite.rs', 'disjunctive_recheck_gate.rs',
            'exact_analysis_fs_recall.rs', 'free_fluctuation_gate.rs', 'guesser_gate.rs',
            'lexical_pattern_trie_exclusion_gate.rs', 'loader_n3_pattern_shapes_gate.rs',
            'merged_analyses_fs_generalization.rs', 'perf_cold_warm_probe.rs',
            'reduplication_gate.rs', 'root_trie_gate.rs', 'stats_collector_gate.rs',
            'supplied_overlay.rs', 'template_analysis_conformance.rs', 'trace_gate.rs',
            'trace_phon_gate.rs', 'trace_rule_sequence_gate.rs', 'word_timeout_gate.rs',
            'word_timeout_pathological_gate.rs', 'xample_migration_differential_gate.rs',
            'zero_width_morph_identity.rs'
        )
    }
    @{
        Name = 'pg-rules'
        PackageRoot = Join-Path $repoRoot 'rust\crates\pg-rules'
        MinTargets = 2
        MaxTargets = 2
        ExpectedSourceNames = @(
            'alpha_gate.rs', 'analysis_syn_fs_gate.rs', 'fst_probe.rs',
            'guessed_root_validity_gate.rs', 'max_apps_gate.rs', 'metathesis_gate.rs',
            'morph_gate.rs', 'non_head_root_filter_gate.rs', 'nonhead_resolution_gate.rs',
            'p7_segments_union_census.rs', 'part1_checkpoint.rs',
            'redup_and_free_fluctuation_gate.rs', 'rewrite_gate.rs', 'stats_gate.rs',
            'stratum_gate.rs', 'synth_gate_order_gate.rs', 'template_partial_gate.rs',
            'type_lane_gate.rs', 'unapplied_rule_counts_reader_gate.rs', 'validity_gate.rs'
        )
    }
    @{
        Name = 'pg-assess'
        PackageRoot = Join-Path $repoRoot 'rust\crates\pg-assess'
        MinTargets = 1
        MaxTargets = 1
        ExpectedSourceNames = @('certification_ledger.rs', 'duplicate_count_determinism.rs', 'identity_projection.rs', 'schema_conformance.rs')
    }
    @{
        Name = 'pg-cli'
        PackageRoot = Join-Path $repoRoot 'rust\crates\pg-cli'
        MinTargets = 2
        MaxTargets = 2
        ExpectedSourceNames = @('agent_docs_resolve_gate.rs', 'developer_flags_contract.rs', 'divergence_catalogue_gate.rs', 'fixture_pins_never_self_skip.rs', 'four_grammar_recipe_evidence.rs', 'fwdata_conformance_gate.rs', 'fwdata_grammar_equivalence_gate.rs', 'grammar_dump_diag.rs', 'guesser_conformance_gate.rs', 'inferred_segment_engine_parity_gate.rs', 'recipe_optimize_continuation.rs', 'recipe_optimize_timeout.rs', 'skills_never_instruct_bare_cargo.rs')
    }
    @{
        Name = 'pg-conformance-fixtures'
        PackageRoot = Join-Path $repoRoot 'rust\crates\pg-conformance-fixtures'
        MinTargets = 1
        MaxTargets = 1
        ExpectedSourceNames = @('build_command_contract.rs', 'case_set_schema.rs', 'producibility_marking_gate.rs', 'three_language_case_set_lock.rs')
    }
    @{
        Name = 'pg-ffi'
        PackageRoot = Join-Path $repoRoot 'rust\crates\pg-ffi'
        MinTargets = 2
        MaxTargets = 2
        ExpectedSourceNames = @('abort_safety.rs', 'ffi_transport_parity.rs', 'generate_round_trip.rs', 'header_abi.rs', 'json_api.rs', 'parse_opts_gate.rs')
    }
    @{
        Name = 'pg-fwdata'
        PackageRoot = Join-Path $repoRoot 'rust\crates\pg-fwdata'
        MinTargets = 1
        MaxTargets = 1
        ExpectedSourceNames = @('compile_real_projects_gate.rs', 'fixture_tests.rs', 'fwbackup_tests.rs', 'measured_import_parity.rs', 'real_projects.rs')
    }
    @{
        Name = 'pg-grammar'
        PackageRoot = Join-Path $repoRoot 'rust\crates\pg-grammar'
        MinTargets = 1
        MaxTargets = 1
        ExpectedSourceNames = @('circumfix_conditioning_parity.rs', 'compile_refusal_gate.rs', 'conversion_inventory_gate.rs', 'lossless_conversion_gate.rs', 'measure_only_confinement_gate.rs', 'p5_closure_property.rs')
    }
    @{
        Name = 'pg-lexicon'
        PackageRoot = Join-Path $repoRoot 'rust\crates\pg-lexicon'
        MinTargets = 1
        MaxTargets = 1
        ExpectedSourceNames = @('analysis_orchestration.rs', 'class_catalog.rs', 'classification.rs', 'persistence_runtime.rs', 'supplied_store.rs')
    }
    @{
        Name = 'pg-realize'
        PackageRoot = Join-Path $repoRoot 'rust\crates\pg-realize'
        MinTargets = 1
        MaxTargets = 1
        ExpectedSourceNames = @('n0_gloss_gate.rs', 'n1_ir_gate.rs', 'n2_realize_gate.rs')
    }
)

foreach ($spec in $crateSpecs) {
    $packageRoot = $spec.PackageRoot
    $testsRoot = Join-Path $packageRoot 'tests'
    $manifestPath = Join-Path $packageRoot 'Cargo.toml'
    $cargoToml = Get-Content -Raw -LiteralPath $manifestPath
    $targets = @(Get-TestTargets $cargoToml)

    Test-Case "$($spec.Name) declares its explicit integration-test target range" {
        Assert-True ($targets.Count -ge $spec.MinTargets -and $targets.Count -le $spec.MaxTargets) `
            "expected $($spec.MinTargets)–$($spec.MaxTargets) explicit [[test]] targets, got $($targets.Count)"
        Assert-True ($cargoToml -match '(?ms)^\[package\]\s*(?:(?!^\[).)*?^autotests\s*=\s*false\s*$') `
            'Cargo must disable implicit integration-test discovery'
    }

    Test-Case "$($spec.Name) has unique existing explicit target paths" {
        Assert-True ($targets.Count -gt 0) 'Cargo manifest has no explicit integration-test targets'
        $duplicateNames = @($targets | Group-Object Name | Where-Object Count -gt 1 | ForEach-Object Name)
        $duplicatePaths = @($targets | Group-Object Path | Where-Object Count -gt 1 | ForEach-Object Name)
        $missingPaths = @($targets | Where-Object {
            -not (Test-Path -LiteralPath (Join-Path $packageRoot $_.Path))
        } | ForEach-Object Path)
        Assert-Equal 0 $duplicateNames.Count "duplicate Cargo test target names: $($duplicateNames -join ', ')"
        Assert-Equal 0 $duplicatePaths.Count "duplicate Cargo test target paths: $($duplicatePaths -join ', ')"
        Assert-Equal 0 $missingPaths.Count "Cargo test targets point at missing files: $($missingPaths -join ', ')"
    }

    Test-Case "$($spec.Name) maps every original source exactly once" {
        $expectedSourceNames = @($spec.ExpectedSourceNames)
        $actualSourceNames = @(Get-ChildItem -LiteralPath $testsRoot -File -Filter '*.rs' |
            ForEach-Object Name | Sort-Object)
        $missingSources = @($expectedSourceNames | Where-Object { $actualSourceNames -notcontains $_ })
        $extraSources = @($actualSourceNames | Where-Object { $expectedSourceNames -notcontains $_ })
        Assert-Equal 0 $missingSources.Count "original source inventory lost files: $($missingSources -join ', ')"
        Assert-Equal 0 $extraSources.Count "unreviewed root-level test sources: $($extraSources -join ', ')"

        $expectedModules = @($expectedSourceNames | ForEach-Object {
            [System.IO.Path]::GetFullPath((Join-Path $testsRoot $_))
        })
        $imports = @()
        foreach ($target in $targets) {
            $targetPath = [System.IO.Path]::GetFullPath((Join-Path $packageRoot $target.Path))
            if ($expectedModules -contains $targetPath) {
                $imports += [PSCustomObject]@{ Target = $target.Name; Module = '<target>'; Path = $targetPath }
            } else {
                $imports += @(Get-TargetModuleImports $targetPath | ForEach-Object {
                    [PSCustomObject]@{ Target = $target.Name; Module = $_.Module; Path = $_.Path }
                })
            }
        }

        $expectedPaths = @($expectedModules | Sort-Object -Unique)
        $actualPaths = @($imports | ForEach-Object Path)
        $missing = @($expectedPaths | Where-Object { $actualPaths -notcontains $_ })
        $extra = @($imports | Where-Object {
            $expectedPaths -notcontains $_.Path -or -not (Test-Path -LiteralPath $_.Path)
        })
        $duplicates = @($imports | Group-Object Path | Where-Object Count -gt 1)
        $missingText = @($missing | ForEach-Object { Get-RelativeTestPath $testsRoot $_ })
        $extraText = @($extra | ForEach-Object {
            "$($_.Target):$($_.Module) -> $(Get-RelativeTestPath $testsRoot $_.Path)"
        })
        $duplicateText = @($duplicates | ForEach-Object {
            "$(Get-RelativeTestPath $testsRoot $_.Name) imported $($_.Count) times"
        })

        Assert-Equal 0 $missingText.Count `
            "missing source mappings: $($missingText -join ', '); duplicate/extra mappings: $($duplicateText + $extraText -join ', ')"
        Assert-Equal 0 ($duplicateText.Count + $extraText.Count) `
            "duplicate/extra source mappings: $($duplicateText + $extraText -join ', '); missing mappings: $($missingText -join ', ')"
    }
}

. "$PSScriptRoot\..\_common.ps1"
$backendTestsRoot = Join-Path $repoRoot 'rust\crates\pg-foma-backend\tests'
$fomaTestsRoot = Join-Path $repoRoot 'rust\crates\pg-foma\tests'
Test-Case 'an absorbed backend test-file stem resolves to its harness and module' {
    $r = Resolve-HarnessTestTarget -TestsDir $backendTestsRoot -TestTarget 'cover_subrule_morphosyntactic_gating'
    Assert-True ($null -ne $r) 'an absorbed stem must resolve'
    Assert-Equal 'cover_subrule_morphosyntactic_gating' $r.Module
    Assert-True (Test-Path (Join-Path $backendTestsRoot "harnesses\$($r.Target).rs")) 'the resolved target must be a harness'
}
Test-Case 'a standalone compiler target still resolves to itself' {
    Assert-Equal $null (Resolve-HarnessTestTarget -TestsDir $fomaTestsRoot -TestTarget 'f0_viability') 'a standalone target stays itself'
}

Write-TestSummary
