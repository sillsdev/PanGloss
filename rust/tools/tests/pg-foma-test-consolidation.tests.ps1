. "$PSScriptRoot\_test-harness.ps1"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$packageRoot = Join-Path $repoRoot 'rust\crates\pg-foma'
$testsRoot = Join-Path $repoRoot 'rust\crates\pg-foma\tests'
$manifestPath = Join-Path $repoRoot 'rust\crates\pg-foma\Cargo.toml'

function Get-RelativeTestPath {
    param([string]$Path)
    [System.IO.Path]::GetRelativePath($testsRoot, $Path).Replace('\', '/')
}

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
    $text = Get-Content -Raw -LiteralPath $HarnessPath
    $imports = @()
    $pattern = '(?ms)^\s*#\[path\s*=\s*"(?<path>[^"]+)"\]\s*\r?\n\s*mod\s+(?<module>[A-Za-z_][A-Za-z0-9_]*)\s*;'
    foreach ($match in [regex]::Matches($text, $pattern)) {
        $full = [System.IO.Path]::GetFullPath((Join-Path (Split-Path -Parent $HarnessPath) $match.Groups['path'].Value))
        $imports += [PSCustomObject]@{ Module = $match.Groups['module'].Value; Path = $full }
    }
    return $imports
}

$cargoToml = Get-Content -Raw -LiteralPath $manifestPath
$targets = @(Get-TestTargets $cargoToml)

Test-Case 'pg-foma declares between five and twenty explicit integration harnesses' {
    Assert-True ($targets.Count -ge 5 -and $targets.Count -le 20) `
        "expected 5–20 explicit [[test]] harnesses, got $($targets.Count)"
    Assert-True ($cargoToml -match '(?ms)^\[package\]\s*(?:(?!^\[).)*?^autotests\s*=\s*false\s*$') `
        'Cargo must disable implicit integration-test discovery so source modules do not become extra executables'
}

Test-Case 'every explicit harness path exists and target paths are unique' {
    Assert-True ($targets.Count -gt 0) 'Cargo manifest has no explicit integration-test targets'
    $duplicateNames = @($targets | Group-Object Name | Where-Object Count -gt 1 | ForEach-Object Name)
    $duplicatePaths = @($targets | Group-Object Path | Where-Object Count -gt 1 | ForEach-Object Name)
    # Cargo resolves [[test]].path relative to the package root, not the workspace root.
    $missingPaths = @($targets | Where-Object { -not (Test-Path -LiteralPath (Join-Path $packageRoot $_.Path)) } | ForEach-Object Path)
    Assert-Equal 0 $duplicateNames.Count "duplicate Cargo test target names: $($duplicateNames -join ', ')"
    Assert-Equal 0 $duplicatePaths.Count "duplicate Cargo test target paths: $($duplicatePaths -join ', ')"
    Assert-Equal 0 $missingPaths.Count "Cargo test targets point at missing files: $($missingPaths -join ', ')"
}

Test-Case 'every original source is mapped exactly once and no harness imports an extra source' {
    # Fixed inventory, not derived from Cargo, so a dropped [[test]] entry cannot hide an original test.
    $expectedSourceNames = @(
        'admission_single_owner_gate.rs', 'advice_catalog_contract.rs',
        'all_fixtures_foma_analyzer_new_no_panic.rs', 'apply_path_refusal_gate.rs',
        'atomic_template_slot_carrier_gate.rs', 'backend_accuracy_gate.rs',
        'backend_capability_cards_contract.rs', 'backend_emission_strategy_gate.rs',
        'backend_mechanism_graph.rs', 'backend_optimizer_calibration.rs',
        'backend_partition_refinement_gate.rs', 'backend_promoted_fixtures.rs',
        'backend_registry_census.rs', 'backend_runtime_cache_gate.rs',
        'backend_runtime_net_is_queryable_gate.rs', 'backend_runtime_oracle_bound_gate.rs',
        'backend_scoreboard_gate.rs', 'backend_seam_gate.rs', 'backend_selection_contract.rs',
        'bare_root_compile_time_discharge.rs',
        'bistratal_overlapping_segment_representation_foma_analyzer_compiles.rs',
        'boundary_marker_epsilon_collapse_gate.rs', 'candidate_filter_contract.rs',
        'candidate_filter_fixture_weight.rs', 'candidate_filter_model_check.rs',
        'candidate_filter_passes.rs', 'candidate_filter_shadow_gate.rs',
        'circumfix_candidate_selection.rs',
        'circumfix_cross_product_and_infix_drop_candidate_selection.rs',
        'closure_unbounded_realizational.rs', 'conformance_coverage_gate.rs',
        'cover_bistratal_overlapping_segment_representation.rs',
        'cover_compounding_recursive_depth_bound.rs', 'cover_compounding.rs',
        'cover_mpr_groups.rs', 'cover_realizational_morphology_constraints.rs',
        'cover_recursive_endocentric_compounding.rs',
        'cover_right_to_left_bounded_quantifier_rewrite.rs',
        'cover_subrule_morphosyntactic_gating.rs', 'cover_unordered_morph_rules.rs',
        'coverage_citation_liveness.rs', 'cross_compiler_equivalence_gate.rs',
        'cross_table_root_respelling_gate.rs', 'deletion_reduplication_exception_fixture.rs',
        'deterministic_eligibility_gate.rs',
        'emit_underlying_templated_recursive_compound_chain.rs',
        'emit_underlying_templated_tag_reachability_gate.rs',
        'envelope_agrees_with_compiler_gate.rs', 'epenthesis_structural_route_containment.rs',
        'exercises_tag_liveness.rs', 'f0_viability.rs', 'f1_large_lexicon_gate.rs',
        'f2_junction_gate.rs', 'f3_interdigitation_gate.rs', 'f3_parity.rs',
        'f4_composite_gate.rs', 'f5_diacritics_gate.rs', 'f6_reduplication_peel_chain_depth.rs',
        'faithfulness_coverage_gate.rs', 'five_language_backend_reports_gate.rs',
        'flag_replace_scope.rs', 'grammar_semantics_owner_gate.rs', 'health_finding_seam.rs',
        'late_structural_anchor_recall.rs', 'mbugwe_corpus_smoke_gate.rs',
        'mechanism_provider_gate.rs', 'mentanukam_multiplicity_recovered_by_confirm.rs',
        'morphology_relation_plan_gate.rs', 'morphotactics_boundary_cleanup_slice.rs',
        'multi_table_metathesis_shared_representation.rs', 'net_dedup_gate.rs',
        'net_dedup_sizing_census.rs', 'net_shape_gate.rs', 'oracle_step_determinism_gate.rs',
        'orthogonal_basis_group_a.rs', 'orthogonal_basis_group_b.rs', 'p6_gate_parity.rs',
        'p6_templated_morphotactics_gate.rs', 'parity_divergence_census.rs',
        'partial_fst_production_admission_gate.rs', 'pattern_root_regex_route_gate.rs',
        'pattern_root_token_route_gate.rs', 'phase_c_alpha_scale.rs', 'phase_c_chain_scale.rs',
        'phase_c_circumfix.rs', 'phase_c_compounding.rs', 'phase_c_metathesis.rs',
        'phase_c_multi_table.rs', 'phase_c_partition_k.rs', 'phase_c_quantifier.rs',
        'phase_c_right_to_left.rs', 'phase_c_simultaneous.rs', 'phase_c_strata_depth.rs',
        'pk1_precision_recall_invariance.rs', 'pk2_eliminate_flag_oracle.rs',
        'plan_composed_marker_material_gate.rs', 'plan_interaction_coverage_gate.rs',
        'predicate_negative_witness_gate.rs', 'process_morphology_route_gate.rs',
        'realizational_pc_represents_gate.rs',
        'segment_natural_class_table_binding_discriminates.rs',
        'simultaneous_overlap_capability_refuses.rs', 'strategy_aware_capability_gate.rs',
        'strategy_coverage_join_gate.rs', 'structural_witness_gate.rs',
        'subrecipe_dossier_contract.rs', 'templated_circumfix_recall_parity.rs',
        'templated_conformance_proposal_pins.rs', 'templated_morphology_classifier_gate.rs',
        'templated_token_cascade_phonology_free_routing_gate.rs', 'trusted_selected_build_gate.rs',
        'two_table_shared_representation_recall.rs', 'two_table_symbol_divergence.rs',
        'typology_speedup.rs', 'uflexc_compound_loop.rs', 'witnessed_strategy_coverage_gate.rs',
        'worker_execution_limits_contract.rs'
    )
    $actualSourceNames = @(Get-ChildItem -LiteralPath $testsRoot -File -Filter '*.rs' |
        ForEach-Object Name | Sort-Object)
    $missingSources = @($expectedSourceNames | Where-Object { $actualSourceNames -notcontains $_ })
    $extraSources = @($actualSourceNames | Where-Object { $expectedSourceNames -notcontains $_ })
    Assert-Equal 0 $missingSources.Count "original source inventory lost files: $($missingSources -join ', ')"
    Assert-Equal 0 $extraSources.Count "unreviewed root-level test sources: $($extraSources -join ', ')"

    $expectedModules = @($expectedSourceNames | ForEach-Object { Join-Path $testsRoot $_ })

    $imports = @()
    foreach ($target in $targets) {
        $harnessPath = [System.IO.Path]::GetFullPath((Join-Path $packageRoot $target.Path))
        $resolvedTargetPath = [System.IO.Path]::GetFullPath($harnessPath)
        if ($expectedModules -contains $resolvedTargetPath) {
            $imports += [PSCustomObject]@{ Target = $target.Name; Module = '<target>'; Path = $resolvedTargetPath }
        } else {
            $imports += @(Get-TargetModuleImports $harnessPath | ForEach-Object {
                [PSCustomObject]@{ Target = $target.Name; Module = $_.Module; Path = $_.Path }
            })
        }
    }

    $expectedPaths = @($expectedModules | Sort-Object -Unique)
    $actualPaths = @($imports | ForEach-Object Path)
    $missing = @($expectedPaths | Where-Object { $actualPaths -notcontains $_ })
    $extra = @($imports | Where-Object { $expectedPaths -notcontains $_.Path -or -not (Test-Path -LiteralPath $_.Path) })
    $duplicates = @($imports | Group-Object Path | Where-Object Count -gt 1)
    $missingText = @($missing | ForEach-Object { Get-RelativeTestPath $_ })
    $extraText = @($extra | ForEach-Object { "$($_.Target):$($_.Module) -> $(Get-RelativeTestPath $_.Path)" })
    $duplicateText = @($duplicates | ForEach-Object { "$(Get-RelativeTestPath $_.Name) imported $($_.Count) times" })

    Assert-Equal 0 $missingText.Count "missing source mappings: $($missingText -join ', '); duplicate/extra mappings: $($duplicateText + $extraText -join ', ')"
    Assert-Equal 0 ($duplicateText.Count + $extraText.Count) `
        "duplicate/extra source mappings: $($duplicateText + $extraText -join ', '); missing mappings: $($missingText -join ', ')"
}

. "$PSScriptRoot\..\_common.ps1"
Test-Case 'an absorbed test-file stem resolves to its harness and module; a standalone one does not' {
    $r = Resolve-HarnessTestTarget -TestsDir $testsRoot -TestTarget 'cover_subrule_morphosyntactic_gating'
    Assert-True ($null -ne $r) 'an absorbed stem must resolve'
    Assert-Equal 'cover_subrule_morphosyntactic_gating' $r.Module
    Assert-True (Test-Path (Join-Path $testsRoot "harnesses\$($r.Target).rs")) 'the resolved target must be a harness'
    Assert-Equal $null (Resolve-HarnessTestTarget -TestsDir $testsRoot -TestTarget 'f0_viability') 'a standalone target stays itself'
}

Write-TestSummary
