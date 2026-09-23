. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

function Get-LegacyCargoTestInvocation {
    param(
        [string]$Mode,
        [bool]$UseNextest,
        [bool]$DebugProfile,
        [int]$TestThreads,
        [string]$Package = '',
        [string]$TestTarget = '',
        [string]$Filter = '',
        [bool]$FailFast = $false,
        [string]$HarnessModule = '',
        [string[]]$ExtraArgs = @()
    )

    $args = @()
    switch ($Mode) {
        'quick' {
            if ($UseNextest) {
                $args += @('nextest', 'run', '--lib', '--bins', '--test-threads', "$TestThreads")
                if (-not $DebugProfile) { $args += @('--cargo-profile', 'pg-test-opt') }
            } else {
                $args += @('test', '--lib', '--bins')
                if (-not $DebugProfile) { $args += @('--profile', 'pg-test-opt') }
            }
        }
        'test' {
            if ($UseNextest) {
                $args += @('nextest', 'run', '--test-threads', "$TestThreads")
                if (-not $DebugProfile) { $args += @('--cargo-profile', 'pg-test-opt') }
            } else {
                $args += 'test'
                if (-not $DebugProfile) { $args += @('--profile', 'pg-test-opt') }
            }
        }
        'corpus-test' {
            if ($UseNextest) {
                $args += @('nextest', 'run', '--run-ignored', 'all', '--test-threads', "$TestThreads")
                if (-not $DebugProfile) { $args += @('--cargo-profile', 'pg-test-opt') }
            } else {
                $args += 'test'
                if (-not $DebugProfile) { $args += @('--profile', 'pg-test-opt') }
            }
        }
        'conformance-test' {
            if ($UseNextest) {
                $args += @('nextest', 'run', '--test-threads', "$TestThreads")
                if (-not $DebugProfile) { $args += @('--cargo-profile', 'pg-test-opt') }
            } else {
                $args += 'test'
                if (-not $DebugProfile) { $args += @('--profile', 'pg-test-opt') }
            }
        }
    }

    if ($Package) { $args += @('-p', $Package) } else { $args += '--workspace' }
    if ($TestTarget) { $args += @('--test', $TestTarget) }

    if ($UseNextest) {
        if ((-not $FailFast) -and ($ExtraArgs -notcontains '--no-fail-fast')) { $args += '--no-fail-fast' }
        if ($HarnessModule) {
            $expr = "test(/^$HarnessModule`::/)"
            if ($Filter) { $expr += " & test($Filter)" }
            $args += @('-E', $expr)
        } elseif ($Filter) {
            $args += $Filter
        }
        if (($Mode -eq 'corpus-test') -and ($ExtraArgs -notcontains '--no-capture')) { $args += '--no-capture' }
    } else {
        $trailing = @()
        if ($Filter) { $trailing += $Filter } elseif ($HarnessModule) { $trailing += "$HarnessModule`::" }
        if ($Mode -in @('quick', 'test', 'corpus-test', 'conformance-test')) { $trailing += @('--test-threads', "$TestThreads") }
        if ($Mode -eq 'corpus-test') { $trailing += @('--nocapture', '--include-ignored') }
        if ($trailing.Count -gt 0) { $args += @('--') + $trailing }
    }
    if ($ExtraArgs) { $args += $ExtraArgs }

    [PSCustomObject]@{
        CargoArgs   = @($args)
        RunnerLabel = if ($UseNextest) { 'nextest' } else { 'cargo test' }
    }
}

Test-Case 'shared cargo test invocation preserves the current argument arrays across the mode matrix' {
    $modes = @('quick', 'test', 'corpus-test', 'conformance-test')
    $cases = 0
    foreach ($mode in $modes) {
        foreach ($useNextest in @($false, $true)) {
            foreach ($debugProfile in @($false, $true)) {
                foreach ($filter in @('', 'needle')) {
                    foreach ($testTarget in @('', 'target')) {
                        foreach ($harnessModule in @('', 'module')) {
                            $expected = Get-LegacyCargoTestInvocation -Mode $mode -UseNextest $useNextest `
                                -DebugProfile $debugProfile -TestThreads 3 -Package 'pg-foma' -TestTarget $testTarget `
                                -Filter $filter -HarnessModule $harnessModule
                            $actual = Get-CargoTestInvocation -Mode $mode -UseNextest $useNextest `
                                -DebugProfile $debugProfile -TestThreads 3 -Package 'pg-foma' -TestTarget $testTarget `
                                -Filter $filter -HarnessModule $harnessModule
                            Assert-Equal ($expected.CargoArgs -join "`0") ($actual.CargoArgs -join "`0") "$mode nextest=$useNextest debug=$debugProfile filter=[$filter] target=[$testTarget] harness=[$harnessModule]"
                            Assert-Equal $expected.RunnerLabel $actual.RunnerLabel "$mode runner label"
                            $cases++
                        }
                    }
                }
            }
        }
    }

    foreach ($case in @(
        @{ Mode = 'test'; UseNextest = $true; DebugProfile = $false; FailFast = $true; ExtraArgs = @('--no-fail-fast') }
        @{ Mode = 'corpus-test'; UseNextest = $true; DebugProfile = $true; ExtraArgs = @('--no-capture') }
        @{ Mode = 'corpus-test'; UseNextest = $false; DebugProfile = $false; Filter = 'needle'; ExtraArgs = @('--custom') }
        @{ Mode = 'conformance-test'; UseNextest = $false; DebugProfile = $true; Package = ''; TestTarget = 'target'; HarnessModule = 'module' }
    )) {
        $expected = Get-LegacyCargoTestInvocation @case -TestThreads 1
        $actual = Get-CargoTestInvocation @case -TestThreads 1
        Assert-Equal ($expected.CargoArgs -join "`0") ($actual.CargoArgs -join "`0") "edge case $($case.Mode)"
        Assert-Equal $expected.RunnerLabel $actual.RunnerLabel "edge runner label $($case.Mode)"
    }
    Assert-Equal 128 $cases 'the mode × runner × profile × filter × target × harness matrix must be complete'
}

Write-TestSummary
