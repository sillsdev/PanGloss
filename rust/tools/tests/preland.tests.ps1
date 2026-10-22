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

$known = @('edge-cases/simultaneous-epenthesis-cascade')
$passing = @("[PASS] edge-cases/a (1ms)", "[FAIL] edge-cases/simultaneous-epenthesis-cascade (3ms) expected crash", 'totals: 1 passed, 1 failed, 0 skipped (of 2 attempted)')

Test-Case 'a driver run whose only failures are known divergences passes' {
    Assert-True (Get-ConformanceDriverVerdict -Lines $passing -ExitCode 1 -Known $known).Ok
}

Test-Case 'an unknown driver failure fails and is named' {
    $lines = @('[FAIL] edge-cases/a (1ms) adapter output malformed', "[FAIL] edge-cases/simultaneous-epenthesis-cascade (3ms) x", 'totals: 0 passed, 2 failed, 0 skipped (of 2 attempted)')
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

Test-Case 'slot queue time is summed from every slot line and nothing else' {
    $lines = @('[build-env] took build slot 1 after 12.5s', 'noise after 99s', '[build-env] took run slot 0 after 0.5s')
    Assert-Equal 13 (Get-SlotWaitSeconds -Lines $lines)
    Assert-Equal 0 (Get-SlotWaitSeconds -Lines @('[pg] cargo build'))
}

Write-TestSummary
