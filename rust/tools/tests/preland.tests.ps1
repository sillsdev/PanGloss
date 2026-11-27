<#
  .DESCRIPTION
  Covers the decisions inside rust/tools/preland.ps1 that decide whether a check runs and whether a
  conformance driver run passes: path gating, the scoreboard's crate closure, and driver grading.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\preland.ps1"

Test-Case 'a directory root matches a path under it and an exact root matches only itself' {
    Assert-Equal 'conformance-staging/edge-cases/x/words.yaml' (Test-PrelandPathTrigger -ChangedPaths @('README.md', 'conformance-staging/edge-cases/x/words.yaml') -Roots @('conformance-staging/'))
    Assert-Equal 'machine' (Test-PrelandPathTrigger -ChangedPaths @('machine') -Roots @('machine'))
    Assert-True ($null -eq (Test-PrelandPathTrigger -ChangedPaths @('machine-notes.md') -Roots @('machine'))) 'an exact root must not prefix-match'
    Assert-True ($null -eq (Test-PrelandPathTrigger -ChangedPaths @('rust/crates/pg-cli-extra/a.rs') -Roots @('rust/crates/pg-cli/'))) 'a directory root must not match a sibling with the same prefix'
}

Test-Case 'a gated check skips with the roots it watched, runs naming its trigger, and runs when forced' {
    $skip = Get-PrelandGateDecision -ChangedPaths @('docs/notes.md') -Roots @('conformance-staging/')
    Assert-False $skip.Run 'unrelated change must skip'
    Assert-True ($skip.Reason -match 'conformance-staging/') 'skip reason must name what was watched'
    $run = Get-PrelandGateDecision -ChangedPaths @('conformance-staging/a/words.yaml') -Roots @('conformance-staging/')
    Assert-True $run.Run
    Assert-True ($run.Reason -match 'words.yaml') 'run reason must name the triggering path'
    $forced = Get-PrelandGateDecision -ChangedPaths @() -Roots @('conformance-staging/') -ForceReason 'forced by -All'
    Assert-True $forced.Run 'a force reason must run the check with no changed path'
}

Test-Case 'the scoreboard closure reaches the crates the gate observes and not unrelated ones' {
    $rust = (Resolve-Path "$PSScriptRoot\..\..").Path
    $repo = (Resolve-Path "$rust\..").Path
    $closure = Get-WorkspaceCrateClosure -RustRoot $rust -RepoRoot $repo -Package 'pg-foma-backend'
    foreach ($crate in 'pg-foma-backend', 'pg-foma', 'pg-parse', 'pg-grammar', 'pg-conformance-fixtures') {
        Assert-Contains $closure "rust/crates/$crate/" "closure must contain $crate"
    }
    Assert-False ($closure -contains 'rust/crates/pg-cli/') 'pg-cli is not observed by the scoreboard gate'
}

Test-Case 'a change to any crate pg-cli depends on runs the pg-cli gates; an unrelated change does not (#351)' {
    $rust = (Resolve-Path "$PSScriptRoot\..\..").Path
    $repo = (Resolve-Path "$rust\..").Path
    $closure = @($script:PgCliGatePackages | ForEach-Object { Get-WorkspaceCrateClosure -RustRoot $rust -RepoRoot $repo -Package $_ } | Sort-Object -Unique)
    foreach ($crate in 'pg-cli', 'pg-foma', 'pg-foma-runtime', 'pg-foma-backend', 'pg-rules', 'pg-parse') {
        Assert-Contains $closure "rust/crates/$crate/" "pg-cli closure must contain $crate"
    }
    Assert-False ($closure -contains 'rust/crates/pg-wasm/') 'a crate pg-cli does not depend on must not trigger the gates'
    $dep = Get-PrelandGateDecision -ChangedPaths @('rust/crates/pg-foma/src/lib.rs') -Roots $closure
    Assert-True $dep.Run 'a pg-foma-only diff must run the pg-cli gates'
    foreach ($input in $script:PgCliGateInputs) {
        $probe = if ($input.EndsWith('/')) { "${input}x" } else { $input }
        Assert-True (Get-PrelandGateDecision -ChangedPaths @($probe) -Roots ($closure + $script:PgCliGateInputs)).Run "pg-cli reads $input, so changing it must run the gates"
        $onDisk = Join-Path $repo $input.TrimEnd('/')
        if ($input -ne 'samples/data/') { Assert-True (Test-Path $onDisk) "trigger input $input no longer exists; update PgCliGateInputs" }
    }
    $docs = Get-PrelandGateDecision -ChangedPaths @('docs/notes.md', 'CHANGELOG.md') -Roots $closure
    Assert-False $docs.Run 'a docs-only diff must stay cheap'
}

Test-Case 'the pg-cli gate runs with the features and nextest profile of CI FST job' {
    $workflow = Get-Content -Raw (Join-Path $PSScriptRoot '..\..\..\.github\workflows\rust-gates.yml')
    $fst = [regex]::Match($workflow, '(?s)FST nextest \(ci profile\).*?run: >-\s*(.*?)\s*-\s+name:').Groups[1].Value
    Assert-True ($fst -match '--profile ci' -and $fst -match '--cargo-profile pg-test-opt' -and $fst -match '--locked' -and $fst -match '-p pg-foma -p pg-foma-runtime -p pg-foma-backend -p pg-cli') 'CI FST job shape changed; update the preland pg-cli gate'
    Assert-True ($fst -match '--features pg-cli/foma-tools') 'CI FST job features changed; update the preland pg-cli gate'
    $a = $script:PgCliGateArgs -join ' '
    Assert-Equal '-Mode test --package=pg-foma --package=pg-foma-runtime --package=pg-foma-backend --package=pg-cli --features pg-cli/foma-tools --profile ci' $a
    Assert-True ($workflow -match '(?s)FST doctests.*?--locked.*?-p pg-foma -p pg-foma-runtime -p pg-foma-backend -p pg-cli --features pg-cli/foma-tools') 'CI FST doctest step changed; update the preland pg-cli gate'
    $pg = Get-Content -Raw (Join-Path $PSScriptRoot '..\pg.ps1')
    Assert-True ($pg.Contains("PANGLOSS_CONFORMANCE_SCOPE = $claimedScope") -and $pg.Contains("'--locked'") -or (Get-Content -Raw (Join-Path $PSScriptRoot '..\_common.ps1')).Contains("'--locked'")) 'pg.ps1 must still supply --locked and the conformance scope'
    $plan = @(Get-PrelandPlan -Gates @([PSCustomObject]@{ Check = 'pg-cli-gates'; Run = $true; Reason = 'changed: rust/crates/pg-foma/src/lib.rs'; PgArgs = $script:PgCliGateArgs }))
    Assert-True ($plan[0] -match 'RUN' -and $plan[0] -match '--features pg-cli/foma-tools') 'the plan must list the foma-tools gate'
}

$known = @('edge-cases/simultaneous-epenthesis-cascade')
$passing = @("discovered 2 fixture(s) under 'test'", "[PASS] edge-cases/a (1ms)", "[FAIL] edge-cases/simultaneous-epenthesis-cascade (3ms) expected crash", 'totals: 1 passed, 1 failed, 0 skipped (of 2 attempted)')

Test-Case 'a driver run whose only failures are known divergences passes' {
    Assert-True (Get-ConformanceDriverVerdict -Lines $passing -ExitCode 1 -Known $known).Ok
}

Test-Case 'run totals preserve complete variant IDs and do not inherit a base fixture waiver' {
    $lines = @(
        "discovered 2 fixture(s) under 'test'",
        '[PASS] edge-cases/a (1ms)',
        '[FAIL] edge-cases/a [variant strict] (2ms) signature mismatch',
        '[PASS] edge-cases/b (3ms)',
        'totals: 2 runs passed, 1 failed, 0 skipped (of 3 runs attempted across 2 fixtures)'
    )
    $result = Get-ConformanceDriverVerdict -Lines $lines -ExitCode 1 -Known @('edge-cases/a')
    Assert-False $result.Ok 'a named variant failure must not inherit the base fixture waiver'
    Assert-True ($result.Detail -match 'edge-cases/a \[variant strict\]') 'failure detail must retain the full run ID'
}

Test-Case 'run totals pass when every discovered fixture has a complete run row' {
    $lines = @(
        "discovered 2 fixture(s) under 'test'",
        '[PASS] edge-cases/a (1ms)',
        '[PASS] edge-cases/a [variant strict] (2ms)',
        '[PASS] edge-cases/b (3ms)',
        'totals: 3 runs passed, 0 failed, 0 skipped (of 3 runs attempted across 2 fixtures)'
    )
    Assert-True (Get-ConformanceDriverVerdict -Lines $lines -ExitCode 0 -Known @()).Ok
}

Test-Case 'run totals reconcile default-only pathological exclusions' {
    $lines = @(
        "discovered 3 fixture(s) under 'test'",
        '1 pathological run(s) (budget_ms) excluded by default (--include-pathological to run)',
        '[PASS] edge-cases/a (1ms)',
        '[PASS] edge-cases/a [variant strict] (2ms)',
        '[PASS] edge-cases/b (3ms)',
        'totals: 3 runs passed, 0 failed, 0 skipped (of 3 runs attempted across 3 fixtures)'
    )
    Assert-True (Get-ConformanceDriverVerdict -Lines $lines -ExitCode 0 -Known @()).Ok
}

Test-Case 'run totals refuse exclusions that cannot reconcile to absent base fixtures' {
    $lines = @(
        "discovered 2 fixture(s) under 'test'",
        '2 pathological run(s) (budget_ms) excluded by default (--include-pathological to run)',
        '[PASS] edge-cases/a (1ms)',
        '[PASS] edge-cases/a [variant strict] (2ms)',
        'totals: 2 runs passed, 0 failed, 0 skipped (of 2 runs attempted across 2 fixtures)'
    )
    $result = Get-ConformanceDriverVerdict -Lines $lines -ExitCode 0 -Known @()
    Assert-False $result.Ok 'excluded runs must map one-for-one to absent default fixture IDs'
    Assert-True ($result.Detail -match 'observed 1 base fixture IDs and 2 excluded runs') 'the ambiguous exclusion must be named'
}

Test-Case 'an unknown driver failure fails and is named' {
    $lines = @("discovered 2 fixture(s) under 'test'", '[FAIL] edge-cases/a (1ms) adapter output malformed', "[FAIL] edge-cases/simultaneous-epenthesis-cascade (3ms) x", 'totals: 0 passed, 2 failed, 0 skipped (of 2 attempted)')
    $v = Get-ConformanceDriverVerdict -Lines $lines -ExitCode 1 -Known $known
    Assert-False $v.Ok
    Assert-True ($v.Detail -match 'edge-cases/a') 'the unexpected fixture must be named'
}

Test-Case 'a driver run that cannot be graded fails instead of passing' {
    Assert-False (Get-ConformanceDriverVerdict -Lines $passing -ExitCode 2 -Known $known).Ok 'exit 2 is a harness error'
    Assert-False (Get-ConformanceDriverVerdict -Lines @('[PASS] edge-cases/a (1ms)') -ExitCode 0 -Known $known).Ok 'no totals line'
    Assert-False (Get-ConformanceDriverVerdict -Lines @('totals: 0 passed, 0 failed, 0 skipped (of 0 attempted)') -ExitCode 0 -Known $known).Ok 'zero attempted'
    Assert-False (Get-ConformanceDriverVerdict -Lines @('totals: 1 passed, 0 failed, 0 skipped (of 1 attempted)') -ExitCode 1 -Known $known).Ok 'exit 1 without a FAIL row'
}

Test-Case 'pangloss is located from the target dir the build child named, or not at all' {
    $exe = if ($IsWindows) { 'pangloss.exe' } else { 'pangloss' }
    $lines = @('[pg] lint gate: cargo clippy', '[pg] cargo build --profile pg-test-opt -p pg-cli --bin pangloss  (target-dir: D:\t\wt, runner: cargo build)')
    Assert-Equal ([IO.Path]::Combine('D:\t\wt', 'pg-test-opt', $exe)) (Get-PanglossFromBuildOutput -Lines $lines -RustRoot 'R:\rust')
    $default = @('[pg] cargo build --profile pg-test-opt -p pg-cli  (target-dir: <default>, runner: cargo build)')
    Assert-Equal ([IO.Path]::Combine('R:\rust', 'target', 'pg-test-opt', $exe)) (Get-PanglossFromBuildOutput -Lines $default -RustRoot 'R:\rust')
    Assert-True ($null -eq (Get-PanglossFromBuildOutput -Lines @('[pg] lint gate: cargo clippy') -RustRoot 'R:\rust')) 'no build line means no binary'
}

Test-Case 'default Machine checkouts carry no drive letters off Windows and the miss names platform paths' {
    $previousCheckouts = $env:PANGLOSS_MACHINE_CHECKOUTS
    try {
        $env:PANGLOSS_MACHINE_CHECKOUTS = $null
        $defaults = @(Get-DefaultMachineCheckouts)
        Assert-True ($defaults.Count -ge 1) 'at least one default checkout'
        if (-not $IsWindows) {
            Assert-True (-not ($defaults -match '^[A-Za-z]:\\')) "no drive-letter default on this platform: $defaults"
            $miss = Find-PinMatchedConformanceDriver -PinCommit ('0' * 40)
            Assert-False $miss.Ok 'no driver for an unknown pin'
            Assert-True ($miss.Detail -notmatch '\\') "the miss detail uses no backslashes: $($miss.Detail)"
        }
    } finally { $env:PANGLOSS_MACHINE_CHECKOUTS = $previousCheckouts }
}

Test-Case 'default Machine checkout is the main clone sibling from a worktree, including a nested branch dir' {
    if ($IsWindows) { return }  # the Windows list is fixed paths
    $temp = New-TestTempDir -Prefix 'pg-machine-default'
    try {
        $clone = Join-Path $temp 'PanGloss'
        New-Item -ItemType Directory -Force -Path $clone | Out-Null
        & git -C $clone init --quiet
        & git -C $clone -c user.name=t -c user.email=t@t commit --quiet --allow-empty -m x
        $expected = Join-Path $temp 'machine'
        Assert-Equal $expected @(Get-DefaultMachineCheckouts -RepoRoot $clone)[0] 'main clone'
        $flat = Join-Path (Join-Path $temp 'PanGloss.worktrees') 'wt'
        $nested = Join-Path (Join-Path (Join-Path $temp 'PanGloss.worktrees') 'fix') 'foo'
        & git -C $clone worktree add --quiet -b wt1 $flat
        & git -C $clone worktree add --quiet -b fix/foo $nested
        Assert-Equal $expected @(Get-DefaultMachineCheckouts -RepoRoot $flat)[0] 'flat worktree'
        Assert-Equal $expected @(Get-DefaultMachineCheckouts -RepoRoot $nested)[0] 'nested worktree'
    } finally { Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue }
}

Test-Case 'pin-matched driver is found through Join-Path paths in a pin-named worktree' {
    $temp = New-TestTempDir -Prefix 'pg-pin-driver-neutral'
    $previousCheckouts = $env:PANGLOSS_MACHINE_CHECKOUTS
    try {
        $checkout = Join-Path $temp 'machine'
        New-Item -ItemType Directory -Force -Path $checkout | Out-Null
        & git -C $checkout init --quiet
        & git -C $checkout -c user.name=t -c user.email=t@t commit --quiet --allow-empty -m pin
        $pin = (& git -C $checkout rev-parse HEAD).Trim()
        $wt = Join-Path (Join-Path $checkout '.worktrees') "pin-$pin"
        & git -C $checkout worktree add --quiet --detach $wt $pin
        $exe = [IO.Path]::Combine($wt, 'src', 'SIL.Machine.Morphology.HermitCrab.Conformance', 'bin', 'Release', 'net10.0', $(if ($IsWindows) { 'hc-conformance.exe' } else { 'hc-conformance' }))
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $exe) | Out-Null
        Set-Content -LiteralPath $exe -Value 'driver'
        [IO.File]::SetLastWriteTimeUtc($exe, [DateTime]::UtcNow.AddMinutes(1))
        $env:PANGLOSS_MACHINE_CHECKOUTS = $checkout
        $found = Find-PinMatchedConformanceDriver -PinCommit $pin
        Assert-True $found.Ok $found.Detail
        Assert-Equal ([IO.Path]::GetFullPath($exe)) $found.ExePath
    } finally {
        $env:PANGLOSS_MACHINE_CHECKOUTS = $previousCheckouts
        Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Test-Case 'pin-matched driver rejects a canonical wrong-HEAD candidate and accepts a matching worktree' {
    $temp = New-TestTempDir -Prefix 'pg-pin-driver-test'
    $previousCheckouts = $env:PANGLOSS_MACHINE_CHECKOUTS
    try {
        $checkout = Join-Path $temp 'machine'
        New-Item -ItemType Directory -Force -Path $checkout | Out-Null
        & git init --quiet $checkout
        Assert-Equal 0 $LASTEXITCODE 'initialize the temporary Machine checkout'
        & git -C $checkout config user.name 'PanGloss Test'
        Assert-Equal 0 $LASTEXITCODE 'configure the temporary Git identity'
        & git -C $checkout config user.email 'pangloss-test@example.invalid'
        Assert-Equal 0 $LASTEXITCODE 'configure the temporary Git identity'
        New-Item -ItemType Directory -Force -Path (Join-Path $checkout 'src') | Out-Null
        Set-Content -LiteralPath (Join-Path $checkout 'src\source.cs') -Value 'class Source { }'
        & git -C $checkout add src/source.cs
        Assert-Equal 0 $LASTEXITCODE 'stage the temporary source'
        & git -C $checkout commit --quiet -m 'pin commit'
        Assert-Equal 0 $LASTEXITCODE 'commit the requested pin'
        $pinCommit = (& git -C $checkout rev-parse HEAD).Trim()
        Assert-Equal 0 $LASTEXITCODE 'read the requested pin'

        Set-Content -LiteralPath (Join-Path $checkout 'src\source.cs') -Value 'class Source { int Changed; }'
        & git -C $checkout commit --all --quiet -m 'different driver commit'
        Assert-Equal 0 $LASTEXITCODE 'commit a different driver revision'
        $wrongCommit = (& git -C $checkout rev-parse HEAD).Trim()
        Assert-Equal 0 $LASTEXITCODE 'read the different revision'

        $canonical = Join-Path $checkout ".worktrees\pin-$pinCommit"
        & git -C $checkout worktree add --quiet --detach $canonical $wrongCommit
        Assert-Equal 0 $LASTEXITCODE 'create the misleading canonical candidate'
        $canonicalExe = Join-Path $canonical 'src\SIL.Machine.Morphology.HermitCrab.Conformance\bin\Release\net10.0\hc-conformance.exe'
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $canonicalExe) | Out-Null
        Set-Content -LiteralPath $canonicalExe -Value 'wrong-head driver'
        [IO.File]::SetLastWriteTimeUtc($canonicalExe, [DateTime]::UtcNow.AddMinutes(1))

        $env:PANGLOSS_MACHINE_CHECKOUTS = $checkout
        $wrongOnly = Find-PinMatchedConformanceDriver -PinCommit $pinCommit
        Assert-False $wrongOnly.Ok 'the pin-named directory must not admit a binary from another HEAD'

        $matching = Join-Path $temp 'matching-worktree'
        & git -C $checkout worktree add --quiet --detach $matching $pinCommit
        Assert-Equal 0 $LASTEXITCODE 'create a matching registered worktree'
        $matchingExe = Join-Path $matching 'src\SIL.Machine.Morphology.HermitCrab.Conformance\bin\Release\net10.0\hc-conformance.exe'
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $matchingExe) | Out-Null
        Set-Content -LiteralPath $matchingExe -Value 'matching driver'
        [IO.File]::SetLastWriteTimeUtc($matchingExe, [DateTime]::UtcNow.AddMinutes(1))

        $withMatching = Find-PinMatchedConformanceDriver -PinCommit $pinCommit
        Assert-True $withMatching.Ok 'a registered worktree at the requested pin must remain usable'
        Assert-Equal ([IO.Path]::GetFullPath($matchingExe)) $withMatching.ExePath 'the wrong-HEAD candidate must be skipped'

        & git -C $checkout worktree remove --force $canonical
        Assert-Equal 0 $LASTEXITCODE 'remove the mismatched worktree'
        & git -C $checkout worktree remove --force $matching
        Assert-Equal 0 $LASTEXITCODE 'remove the matching worktree'
        & git -C $checkout checkout --quiet --detach $pinCommit
        Assert-Equal 0 $LASTEXITCODE 'move the checkout itself to the requested pin'
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $canonicalExe) | Out-Null
        Set-Content -LiteralPath $canonicalExe -Value 'not a worktree driver'
        [IO.File]::SetLastWriteTimeUtc($canonicalExe, [DateTime]::UtcNow.AddMinutes(1))
        $notWorktree = Find-PinMatchedConformanceDriver -PinCommit $pinCommit
        Assert-False $notWorktree.Ok 'a plain pin-named directory must not inherit its parent checkout HEAD'
    } finally {
        $env:PANGLOSS_MACHINE_CHECKOUTS = $previousCheckouts
        Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Test-Case 'pin-matched driver accepts the platform binary name and a miss names every name it searched' {
    $temp = New-TestTempDir -Prefix 'pg-pin-driver-name-test'
    $previousCheckouts = $env:PANGLOSS_MACHINE_CHECKOUTS
    try {
        $checkout = Join-Path $temp 'machine'
        New-Item -ItemType Directory -Force -Path (Join-Path $checkout 'src') | Out-Null
        & git init --quiet $checkout
        Assert-Equal 0 $LASTEXITCODE 'initialize the temporary Machine checkout'
        & git -C $checkout config user.name 'PanGloss Test'
        & git -C $checkout config user.email 'pangloss-test@example.invalid'
        Set-Content -LiteralPath (Join-Path $checkout 'src\source.cs') -Value 'class Source { }'
        & git -C $checkout add src/source.cs
        & git -C $checkout commit --quiet -m 'pin commit'
        Assert-Equal 0 $LASTEXITCODE 'commit the requested pin'
        $pinCommit = (& git -C $checkout rev-parse HEAD).Trim()
        $env:PANGLOSS_MACHINE_CHECKOUTS = $checkout
        $binDir = Join-Path $checkout 'src\SIL.Machine.Morphology.HermitCrab.Conformance\bin\Release\net10.0'

        $missing = Find-PinMatchedConformanceDriver -PinCommit $pinCommit
        Assert-False $missing.Ok 'no stub exists yet'
        Assert-True ($missing.Detail -match [regex]::Escape('hc-conformance.exe')) 'the miss names the .exe it searched for'
        Assert-True ($missing.Detail -match [regex]::Escape($checkout)) 'the build failure names the whole single checkout, not its first character'
        if ($IsLinux -or $IsMacOS) {
            Assert-True ($missing.Detail -match 'hc-conformance or hc-conformance\.exe') 'the miss names the native name it searched for'
            Assert-True ($missing.Detail -match [regex]::Escape((Join-Path $binDir 'hc-conformance'))) 'the miss lists the native-name path it probed'
        }

        New-Item -ItemType Directory -Force -Path $binDir | Out-Null
        $nativeName = if ($IsLinux -or $IsMacOS) { 'hc-conformance' } else { 'hc-conformance.exe' }
        $stub = Join-Path $binDir $nativeName
        Set-Content -LiteralPath $stub -Value 'platform-named driver'
        [IO.File]::SetLastWriteTimeUtc($stub, [DateTime]::UtcNow.AddMinutes(1))
        $found = Find-PinMatchedConformanceDriver -PinCommit $pinCommit
        Assert-True $found.Ok "a driver named $nativeName at the pin must be found"
        Assert-Equal ([IO.Path]::GetFullPath($stub)) $found.ExePath 'the platform-named stub is the one returned'
    } finally {
        $env:PANGLOSS_MACHINE_CHECKOUTS = $previousCheckouts
        Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Test-Case 'pin-matched driver is found after an earlier candidate directory failed its git probe' {
    $temp = New-TestTempDir -Prefix 'pg-pin-driver-stale-exit-test'
    $previousCheckouts = $env:PANGLOSS_MACHINE_CHECKOUTS
    try {
        $checkout = Join-Path $temp 'machine'
        New-Item -ItemType Directory -Force -Path (Join-Path $checkout 'src') | Out-Null
        & git init --quiet $checkout
        & git -C $checkout config user.name 'PanGloss Test'
        & git -C $checkout config user.email 'pangloss-test@example.invalid'
        Set-Content -LiteralPath (Join-Path $checkout 'src\source.cs') -Value 'class Source { }'
        & git -C $checkout add src/source.cs
        & git -C $checkout commit --quiet -m 'pin commit'
        Assert-Equal 0 $LASTEXITCODE 'commit the requested pin'
        $pinCommit = (& git -C $checkout rev-parse HEAD).Trim()
        $nativeName = if ($IsLinux -or $IsMacOS) { 'hc-conformance' } else { 'hc-conformance.exe' }
        $relative = 'src\SIL.Machine.Morphology.HermitCrab.Conformance\bin\Release\net10.0'

        # The first checkout is not a git repository, so git exits non-zero for its pin-named stub before the real one is probed.
        $plain = Join-Path $temp 'plain'
        $plainBin = Join-Path (Join-Path $plain ".worktrees\pin-$pinCommit") $relative
        New-Item -ItemType Directory -Force -Path $plainBin | Out-Null
        $realBin = Join-Path $checkout $relative
        New-Item -ItemType Directory -Force -Path $realBin | Out-Null
        foreach ($stub in (Join-Path $plainBin $nativeName), (Join-Path $realBin $nativeName)) {
            Set-Content -LiteralPath $stub -Value 'driver'
            [IO.File]::SetLastWriteTimeUtc($stub, [DateTime]::UtcNow.AddMinutes(1))
        }

        $env:PANGLOSS_MACHINE_CHECKOUTS = "$plain;$checkout"
        $found = Find-PinMatchedConformanceDriver -PinCommit $pinCommit
        Assert-True $found.Ok 'a failed git probe of an earlier candidate must not reject the matching worktree'
        Assert-Equal ([IO.Path]::GetFullPath((Join-Path $realBin $nativeName))) $found.ExePath 'the registered checkout at the pin is the one returned'
    } finally {
        $env:PANGLOSS_MACHINE_CHECKOUTS = $previousCheckouts
        Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Test-Case 'slot queue time is summed from every slot line and nothing else' {
    $lines = @('[build-env] took build slot 1 after 12.5s', 'noise after 99s', '[build-env] took run slot 0 after 0.5s')
    Assert-Equal 13 (Get-SlotWaitSeconds -Lines $lines)
    Assert-Equal 0 (Get-SlotWaitSeconds -Lines @('[pg] cargo build'))
}

Test-Case 'driver cache builds from a submodule export, survives bin cleanup, and rebuilds stale metadata' {
    $temp = New-TestTempDir -Prefix 'pg-driver-cache'
    $previousCheckouts = $env:PANGLOSS_MACHINE_CHECKOUTS
    $previousCache = $env:PANGLOSS_CONFORMANCE_DRIVER_CACHE
    $publisher = ${function:Invoke-ConformanceDriverPublish}
    try {
        $upstream = Join-Path $temp 'upstream'
        $repo = Join-Path $temp 'super'
        $project = 'src/SIL.Machine.Morphology.HermitCrab.Conformance/driver.csproj'
        New-Item -ItemType Directory -Force -Path (Split-Path (Join-Path $upstream $project)), $repo | Out-Null
        Set-Content (Join-Path $upstream $project) '<Project />'
        & git -C $upstream init --quiet
        & git -C $upstream add .
        & git -C $upstream -c user.name=t -c user.email=t@t commit --quiet -m pin
        $pin = (& git -C $upstream rev-parse HEAD).Trim()
        & git -C $repo init --quiet
        & git -C $repo -c protocol.file.allow=always submodule add --quiet $upstream machine
        Assert-Equal 0 $LASTEXITCODE 'create a real submodule source'
        $env:PANGLOSS_MACHINE_CHECKOUTS = Join-Path $temp 'absent'
        $env:PANGLOSS_CONFORMANCE_DRIVER_CACHE = Join-Path $temp 'cache'
        $script:publishCount = 0
        function Invoke-ConformanceDriverPublish {
            param($Project, $Output, $Jobs)
            Assert-Equal 4 $Jobs
            Assert-True (Test-Path $Project) 'export includes the project'
            Assert-False ($Project.StartsWith($repo)) 'never build inside a registered source'
            $script:publishCount++
            Set-Content (Join-Path $Output @(Get-ConformanceDriverFileNames)[0]) 'published driver'
            Set-Content (Join-Path $Output 'dependency.dll') 'dependency'
        }
        $first = Find-PinMatchedConformanceDriver -PinCommit $pin -RepoRoot $repo
        Assert-True $first.Ok $first.Detail
        Assert-Equal 1 $script:publishCount 'cache miss publishes once'
        Assert-True (Test-Path (Join-Path (Split-Path $first.ExePath) 'dependency.dll')) 'whole publish output retained'
        Assert-False (Test-Path (Join-Path $repo 'machine/src/SIL.Machine.Morphology.HermitCrab.Conformance/bin')) 'no source bin created'
        $second = Find-PinMatchedConformanceDriver -PinCommit $pin -RepoRoot $repo
        Assert-True $second.Ok $second.Detail
        Assert-Equal 1 $script:publishCount 'cache hit does not publish'
        foreach ($field in 'Pin', 'SourceTree', 'Platform') {
            $manifestPath = Join-Path (Split-Path $first.ExePath) 'source.json'
            $manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
            $manifest.$field = 'stale'
            $manifest | ConvertTo-Json | Set-Content $manifestPath
            $rebuilt = Find-PinMatchedConformanceDriver -PinCommit $pin -RepoRoot $repo
            Assert-True $rebuilt.Ok $rebuilt.Detail
        }
        Assert-Equal 4 $script:publishCount 'wrong pin, source hash and platform each rebuild'
        Remove-Item $first.ExePath
        Assert-True (Find-PinMatchedConformanceDriver -PinCommit $pin -RepoRoot $repo).Ok 'missing apphost rebuilds'
        Assert-Equal 5 $script:publishCount
        Remove-Item $env:PANGLOSS_CONFORMANCE_DRIVER_CACHE -Recurse -Force
        Set-Content (Join-Path $repo "machine/$project") 'dirty'
        $dirty = Find-PinMatchedConformanceDriver -PinCommit $pin -RepoRoot $repo
        Assert-False $dirty.Ok 'dirty submodule cannot supply a build'
        Assert-Equal 5 $script:publishCount 'dirty source never invokes publisher'
        & git -C (Join-Path $repo 'machine') checkout -- $project
        function Invoke-ConformanceDriverPublish { param($Project, $Output, $Jobs) throw 'synthetic build error' }
        $failed = Find-PinMatchedConformanceDriver -PinCommit $pin -RepoRoot $repo
        Assert-False $failed.Ok 'failed publish is never admitted'
        Assert-True ($failed.Detail -match 'synthetic build error') 'failure explains the publish error'
        Assert-False (Test-Path (Join-Path $env:PANGLOSS_CONFORMANCE_DRIVER_CACHE "$pin/source.json")) 'failure publishes no manifest'
        function Invoke-ConformanceDriverPublish { param($Project, $Output, $Jobs) }
        $empty = Find-PinMatchedConformanceDriver -PinCommit $pin -RepoRoot $repo
        Assert-False $empty.Ok 'zero-exit publish without an apphost cannot succeed'
        Assert-True ($empty.Detail -match 'no conformance apphost') 'missing output is named'
        $bin = Join-Path $repo 'machine/src/SIL.Machine.Morphology.HermitCrab.Conformance/bin/Release/net10.0'
        New-Item -ItemType Directory -Force -Path $bin | Out-Null
        $exe = Join-Path $bin @(Get-ConformanceDriverFileNames)[0]
        Set-Content $exe 'prebuilt submodule driver'
        [IO.File]::SetLastWriteTimeUtc($exe, [DateTime]::UtcNow.AddMinutes(1))
        $prebuilt = Find-PinMatchedConformanceDriver -PinCommit $pin -RepoRoot $repo
        Assert-True $prebuilt.Ok $prebuilt.Detail
        Assert-Equal ([IO.Path]::GetFullPath($exe)) $prebuilt.ExePath 'a prebuilt submodule driver is discoverable'
        [IO.File]::SetLastWriteTimeUtc($exe, [DateTime]::UnixEpoch)
        $stale = Find-PinMatchedConformanceDriver -PinCommit $pin -RepoRoot $repo
        Assert-False $stale.Ok 'an older-than-HEAD submodule binary is refused'
        Assert-True ($stale.Detail -match 'older than its HEAD') 'stale binary is identified'
        [IO.File]::SetLastWriteTimeUtc($exe, [DateTime]::UtcNow.AddMinutes(1))
        Set-Content (Join-Path $repo "machine/$project") 'dirty'
        $dirtyBinary = Find-PinMatchedConformanceDriver -PinCommit $pin -RepoRoot $repo
        Assert-False $dirtyBinary.Ok 'a dirty submodule binary is refused'
    } finally {
        ${function:Invoke-ConformanceDriverPublish} = $publisher
        $env:PANGLOSS_MACHINE_CHECKOUTS = $previousCheckouts
        $env:PANGLOSS_CONFORMANCE_DRIVER_CACHE = $previousCache
        Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Test-Case 'driver publish limits jobs, suppresses build-time suite fetch, and restores its environment on failure' {
    $savedEnter = ${function:Enter-ResourceSlot}
    $savedExit = ${function:Exit-ResourceSlot}
    $savedMemory = ${function:Test-MemoryReserve}
    $savedSuite = $env:HC_CONFORMANCE_SUITE
    try {
        function Enter-ResourceSlot { param($Pool, $MaxConcurrent, $TimeoutSeconds) return 'test-slot' }
        function Exit-ResourceSlot { param($Slot) $script:releasedDriverSlot = $Slot }
        function Test-MemoryReserve { param($AvailableGB) return @{ Ok = $true } }
        function dotnet {
            Assert-Contains $args '-maxcpucount:4' 'dotnet receives the job limit'
            Assert-True ([bool]$env:HC_CONFORMANCE_SUITE) 'build never tries to clone the fixture suite'
            $global:LASTEXITCODE = 1
            'synthetic publish failure'
        }
        $env:HC_CONFORMANCE_SUITE = $null
        $script:releasedDriverSlot = $null
        $message = ''
        try { Invoke-ConformanceDriverPublish -Project 'driver.csproj' -Output 'unused' -Jobs 4 }
        catch { $message = $_.Exception.Message }
        Assert-True ($message -match 'synthetic publish failure') 'failed dotnet invocation is reported'
        Assert-Equal 'test-slot' $script:releasedDriverSlot 'failed publish releases its build slot'
        Assert-False ([bool]$env:HC_CONFORMANCE_SUITE) 'temporary suite override is removed'
    } finally {
        ${function:Enter-ResourceSlot} = $savedEnter
        ${function:Exit-ResourceSlot} = $savedExit
        ${function:Test-MemoryReserve} = $savedMemory
        $env:HC_CONFORMANCE_SUITE = $savedSuite
        Remove-Item function:dotnet -ErrorAction SilentlyContinue
    }
}

Write-TestSummary
