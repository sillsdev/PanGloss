<#
  .DESCRIPTION
  Runs the C# founding oracle's self-check harness (`hc-conformance.exe`, built from
  sillsdev/machine's `conformance-framework` branch) over this repo's conformance fixtures and gates
  on it, per CLAUDE.md's "oracle hierarchy" section: HC-Rust (`pg_parse::Morpher`) is a port under
  test, not a source of truth, so a fixture whose `words.yaml` was only ever checked against HC-Rust
  proves nothing about correctness. This script is the missing other half of the transitivity
  argument -- the existing `conformance_fixtures_gate` (`rust/crates/pg-parse/tests/`) already gates
  HC-Rust == words.yaml; this gates C# == words.yaml (self-check mode). Both green means HC-Rust == C#
  exactly, because the signature comparison covers the complete analysis set (PROTOCOL.md section 3),
  catching both over- and under-generation.

  This is a THIN FRONT END in the same family as build.ps1/test.ps1/conformance.ps1: it takes no
  build slot and starts no Cargo, because it never touches the Rust workspace -- it only shells out to
  an already-built .NET executable.

  A ratchet, not an all-or-nothing gate: `oracle-conformance-known-divergences.json` (beside this
  script) lists fixtures known to currently mismatch, each tagged with WHY (a grammar the oracle
  cannot load at all vs. a fixture whose SIGNATURE already matches and only the incidental `rules:`
  attribution field is incomplete -- PROTOCOL.md section 4 does not compare `rules:` at all). A FAIL
  matching exact reviewed evidence in the baseline is reported but does not fail the gate; any changed FAIL
  is a NEW divergence and fails it. Removing a reconciled fixture from the baseline is how the
  backlog shrinks; nothing here grows it silently.

  A control that cannot act must say so (CLAUDE.md): if `dotnet` or `hc-conformance.exe` cannot be
  found, this exits nonzero naming exactly what is missing and how to get it -- never a silent skip
  that would read as "everything is fine".

  `conformance-staging/filter-passes/**` fixtures are invisible to `Fixture.DiscoverAll` (it only
  scans `languages`/`edge-cases`), so this script materializes a throwaway `edge-cases/<name>` mirror
  of them under a temp directory for the run and maps results back to their real `filter-passes/<name>`
  id -- see `Test-FilterPassesSelfCheckRun`. The real, committed fixtures are never moved.

  Usage:
    rust\tools\oracle-conformance.ps1                       # self-check over conformance-staging
    rust\tools\oracle-conformance.ps1 -Scope all             # + machine/conformance as a control
    rust\tools\oracle-conformance.ps1 -IncludePathological
    rust\tools\oracle-conformance.ps1 -Propose               # print reconciliation patches for any
                                                              # genuine signature mismatch found
    rust\tools\oracle-conformance.ps1 -ExePath <path-to-hc-conformance.exe>

  Exit codes: 0 = no new divergence (baselined ones may still be present, and are printed). 25
  ($script:ExitCodeOracleUnavailable) = dotnet or hc-conformance.exe not found, or the harness itself
  could not run (bad path, zero fixtures discovered). 26 ($script:ExitCodeOracleDivergence) = at
  least one FAIL (or, for -Scope all's machine/conformance control, ANY FAIL at all) outside the
  known-divergence baseline.
#>
[CmdletBinding()]
param(
    [ValidateSet('local', 'all')]
    [string]$Scope = 'local',
    [switch]$IncludePathological,
    [switch]$Propose,
    [string]$ExePath,
    [string]$PinExePath,
    [string]$BaselinePath = (Join-Path $PSScriptRoot 'oracle-conformance-known-divergences.json')
)

. "$PSScriptRoot\_common.ps1"

$repoRoot = Get-RepoRoot
# Run from another worktree's cwd, this script would grade THAT tree's fixtures under this tree's name; refusing (exit 19) is the same rule pg.ps1 applies.
Assert-ScriptAndCwdAgreeOnWorktree -ScriptRoot $PSScriptRoot

function Find-OracleExe {
    param([string]$Explicit)

    if ($Explicit) {
        if (Test-Path $Explicit) { return (Resolve-Path $Explicit).Path }
        Write-Host "[oracle-conformance] -ExePath '$Explicit' does not exist." -ForegroundColor Red
        return $null
    }

    # Documented default: the conformance-framework branch TIP worktree (Release then Debug), preferred over the main checkout's exe, whose build provenance cannot be pinned to a commit.
    $candidates = @(
        'C:\Users\johnm\Documents\repos\machine\.worktrees\conformance\src\SIL.Machine.Morphology.HermitCrab.Conformance\bin\Release\net10.0\hc-conformance.exe',
        'C:\Users\johnm\Documents\repos\machine\.worktrees\conformance\src\SIL.Machine.Morphology.HermitCrab.Conformance\bin\Debug\net10.0\hc-conformance.exe',
        'C:\Users\johnm\Documents\repos\machine\src\SIL.Machine.Morphology.HermitCrab.Conformance\bin\Release\net10.0\hc-conformance.exe',
        'C:\Users\johnm\Documents\repos\machine\src\SIL.Machine.Morphology.HermitCrab.Conformance\bin\Debug\net10.0\hc-conformance.exe'
    )
    foreach ($c in $candidates) {
        if (Test-Path $c) { return (Resolve-Path $c).Path }
    }
    Write-Host "[oracle-conformance] hc-conformance.exe not found. Probed:" -ForegroundColor Red
    foreach ($c in $candidates) { Write-Host "  $c" -ForegroundColor Red }
    Write-Host "[oracle-conformance] build it: dotnet build <machine checkout>\src\SIL.Machine.Morphology.HermitCrab.Conformance -c Release" -ForegroundColor Yellow
    return $null
}

function Find-PinOracleExe {
    <#
      .DESCRIPTION
      The machine/conformance control (-Scope all) needs an exe built from the EXACT commit
      PanGloss's `machine` submodule pins, not the conformance-framework tip: the tip's
      WordsYamlLoader rejects a key (`claimed_cells`) present in the pinned commit's own fixture
      data, because the pin is not an ancestor of the tip (the branch was rebased since PanGloss
      pinned) -- so the two commits' loader/fixture pairs are genuinely incompatible with each
      other's fixture format. A commit-matched exe has no such mismatch by construction.
    #>
    param([string]$Explicit, [string]$PinCommit)

    if ($Explicit) {
        if (Test-Path $Explicit) { return (Resolve-Path $Explicit).Path }
        Write-Host "[oracle-conformance] -PinExePath '$Explicit' does not exist." -ForegroundColor Red
        return $null
    }

    $candidates = @(
        "C:\Users\johnm\Documents\repos\machine\.worktrees\pin-$PinCommit\src\SIL.Machine.Morphology.HermitCrab.Conformance\bin\Release\net10.0\hc-conformance.exe",
        "C:\Users\johnm\Documents\repos\machine\.worktrees\pin-$PinCommit\src\SIL.Machine.Morphology.HermitCrab.Conformance\bin\Debug\net10.0\hc-conformance.exe"
    )
    foreach ($c in $candidates) {
        if (Test-Path $c) { return (Resolve-Path $c).Path }
    }
    Write-Host "[oracle-conformance] pin-matched hc-conformance.exe not found. Probed:" -ForegroundColor Red
    foreach ($c in $candidates) { Write-Host "  $c" -ForegroundColor Red }
    Write-Host "[oracle-conformance] build it: git -C C:\Users\johnm\Documents\repos\machine worktree add --detach .worktrees\pin-$PinCommit $PinCommit; dotnet build C:\Users\johnm\Documents\repos\machine\.worktrees\pin-$PinCommit\src\SIL.Machine.Morphology.HermitCrab.Conformance -c Release" -ForegroundColor Yellow
    return $null
}

function Get-DotnetCommand {
    return Get-Command dotnet -ErrorAction SilentlyContinue
}

function Invoke-OracleSelfCheck {
    param(
        [string]$ExePath,
        [string]$FixturesRoot,
        [switch]$IncludePathological,
        [switch]$Propose
    )
    $args = @('--fixtures', $FixturesRoot)
    if ($IncludePathological) { $args += '--include-pathological' }
    if ($Propose) { $args += '--propose' }

    $output = & $ExePath @args 2>&1 | Out-String
    $exitCode = $LASTEXITCODE
    return [PSCustomObject]@{
        Output   = $output
        ExitCode = $exitCode
    }
}

function ConvertFrom-SelfCheckOutput {
    <#
      .DESCRIPTION
      Parses hc-conformance.exe's human-readable report (there is no machine-readable output mode)
      into per-fixture outcomes. Pure function of the text, so it's testable without invoking the
      exe. Line shape: "[PASS|FAIL|SKIP] <fixture-id> (<n>ms) <reason>".
    #>
    param([string]$Text)

    $results = @()
    foreach ($line in ($Text -split "`r?`n")) {
        if ($line -match '^\[(PASS|FAIL|SKIP)\]\s+(\S+)\s+\((\d+)ms\)\s*(.*)$') {
            $results += [PSCustomObject]@{
                Fixture = $Matches[2]
                Status  = $Matches[1]
                Reason  = $Matches[4].Trim()
            }
        }
    }
    return $results
}

function Get-FilterPassesOracleRoot {
    <#
      .DESCRIPTION
      A deterministic, worktree-scoped scratch path for the materialized filter-passes root -- a
      hash of $RepoRoot rather than a GUID, so re-running the script targets the same path (the
      task's reproducibility requirement) while still not colliding with another worktree's run.
    #>
    param([string]$RepoRoot)
    $hasher = [System.Security.Cryptography.MD5]::Create()
    try {
        $bytes = $hasher.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($RepoRoot))
    } finally {
        $hasher.Dispose()
    }
    $hash = ([System.BitConverter]::ToString($bytes) -replace '-', '').ToLowerInvariant().Substring(0, 12)
    return Join-Path ([System.IO.Path]::GetTempPath()) "pangloss-oracle-filter-passes-$hash"
}

function New-FilterPassesOracleRoot {
    <#
      .DESCRIPTION
      hc-conformance.exe's `Fixture.DiscoverAll` (machine's Fixture.cs) only ever scans a fixtures
      root's `languages` and `edge-cases` subdirectories -- `filter-passes` is a THIRD staging-only
      category (see pg_conformance_fixtures::discover_filter_passes) that neither this repo's own
      dual-root discovery nor the founding oracle's harness walks. Rather than moving the real,
      committed fixtures (explicitly forbidden -- see candidate_filter_fixture_weight.rs), this
      rebuilds a disposable root from scratch on every call, mirroring each filter-passes fixture's
      `grammar.xml`/`words.yaml` under `edge-cases/<name>` so the harness can discover and self-check
      it. Returns the mirrored fixture names.
    #>
    param([string]$FilterPassesRoot, [string]$DestRoot)

    if (Test-Path $DestRoot) { Remove-Item $DestRoot -Recurse -Force }
    $destEdgeCases = Join-Path $DestRoot 'edge-cases'
    New-Item -ItemType Directory -Force -Path $destEdgeCases | Out-Null

    $fixtureDirs = @(Get-ChildItem $FilterPassesRoot -Directory | Where-Object {
        (Test-Path (Join-Path $_.FullName 'grammar.xml')) -and (Test-Path (Join-Path $_.FullName 'words.yaml'))
    })
    foreach ($d in $fixtureDirs) {
        $dest = Join-Path $destEdgeCases $d.Name
        New-Item -ItemType Directory -Force -Path $dest | Out-Null
        Copy-Item (Join-Path $d.FullName 'grammar.xml') (Join-Path $dest 'grammar.xml') -Force
        Copy-Item (Join-Path $d.FullName 'words.yaml') (Join-Path $dest 'words.yaml') -Force
    }
    return $fixtureDirs.Name
}

function Test-FilterPassesSelfCheckRun {
    # The mirror uses the same evidence validator and waiver owner as ordinary fixture roots.
    param([string]$ExePath, [string]$FilterPassesRoot, [string]$TempRoot, [hashtable]$Baseline,
          [switch]$IncludePathological, [switch]$Propose)
    $fixtureNames = @(New-FilterPassesOracleRoot -FilterPassesRoot $FilterPassesRoot -DestRoot $TempRoot)
    if ($fixtureNames.Count -eq 0) {
        Write-Host '[oracle-conformance] no filter-passes fixtures found -- no evidence available.' -ForegroundColor Red
        return $null
    }
    return Test-SelfCheckRun -ExePath $ExePath -FixturesRoot $TempRoot -RootLabel 'filter-passes (mirror)' `
        -Baseline $Baseline -IncludePathological:$IncludePathological -Propose:$Propose -MapFilterPasses
}

function Get-Baseline {
    param([string]$Path)
    if (-not (Test-Path $Path)) {
        Write-Host "[oracle-conformance] no baseline file at '$Path' -- treating the known-divergence set as empty (every FAIL will be reported as NEW)." -ForegroundColor Yellow
        return @{}
    }
    $json = Get-Content $Path -Raw | ConvertFrom-Json
    $map = @{}
    foreach ($d in $json.divergences) { $map[$d.fixture] = $d }
    return $map
}

function Test-DeclaredForwardSynthesis {
    # True when the fixture's words.yaml carries the PROTOCOL.md marker for expectations the oracle is known to lose.
    param([string]$FixturesRoot, [string]$Fixture)
    $words = Join-Path $FixturesRoot ($Fixture -replace '/', '\') | Join-Path -ChildPath 'words.yaml'
    if (-not (Test-Path $words)) { return $false }
    foreach ($line in (Get-Content $words -TotalCount 20)) {
        if ($line -match '^\s*#\s*oracle-provenance:\s*forward-synthesis\b') { return $true }
    }
    return $false
}

function Test-OracleEvidence {
    # Reconcile the oracle's own discovery, exclusions, result rows, totals and exit contract.
    param($Run, [array]$Results)
    if ($Run.ExitCode -notin @(0, 1)) { return $false }
    $summaries = [regex]::Matches($Run.Output, '(?m)^totals: (\d+) passed, (\d+) failed, (\d+) skipped \(of (\d+) attempted\)\s*$')
    $discoveries = [regex]::Matches($Run.Output, '(?m)^discovered (\d+) fixture\(s\) under .+$')
    if ($summaries.Count -ne 1 -or $discoveries.Count -ne 1) { return $false }
    $totals = $summaries[0].Groups
    $passed = [long]$totals[1].Value; $failed = [long]$totals[2].Value
    $skipped = [long]$totals[3].Value; $attempted = [long]$totals[4].Value
    if ($passed + $failed -eq 0 -or $Results.Count -ne $attempted) { return $false }
    if (@($Results | Select-Object -ExpandProperty Fixture -Unique).Count -ne $Results.Count) { return $false }
    if (@($Results | Where-Object Status -eq PASS).Count -ne $passed -or
        @($Results | Where-Object Status -eq FAIL).Count -ne $failed -or
        @($Results | Where-Object Status -eq SKIP).Count -ne $skipped) { return $false }
    $exclusions = [regex]::Matches($Run.Output, '(?m)^(\d+) pathological \(budget_ms\) fixture\(s\) excluded by default .+$')
    if ($exclusions.Count -gt 1) { return $false }
    $excluded = if ($exclusions.Count) { [long]$exclusions[0].Groups[1].Value } else { 0 }
    if ([long]$discoveries[0].Groups[1].Value -ne $attempted + $excluded) { return $false }
    return $Run.ExitCode -eq $(if ($failed -gt 0) { 1 } else { 0 })
}

function Test-OracleKnownFailure {
    # A fixture identifier or provenance marker cannot waive a newly changed failure.
    param($Failure, [hashtable]$Baseline, [string]$FixturesRoot, [switch]$ExpectCleanBaseline)
    if (-not $Baseline.ContainsKey($Failure.Fixture)) { return $false }
    $entry = $Baseline[$Failure.Fixture]
    if (-not $entry.PSObject.Properties['reason'] -or [string]$entry.reason -cne $Failure.Reason) { return $false }
    if ($entry.kind -eq 'forward-synthesis') {
        return Test-DeclaredForwardSynthesis -FixturesRoot $FixturesRoot -Fixture $Failure.Fixture
    }
    return -not $ExpectCleanBaseline
}

function Test-SelfCheckRun {
    # $null means unavailable/inconsistent evidence; $false means a new divergence.
    param([string]$ExePath, [string]$FixturesRoot, [string]$RootLabel, [hashtable]$Baseline,
          [switch]$IncludePathological, [switch]$Propose, [switch]$ExpectCleanBaseline, [switch]$MapFilterPasses)
    Write-Host "[oracle-conformance] self-check: $RootLabel ($FixturesRoot)" -ForegroundColor Cyan
    $run = Invoke-OracleSelfCheck -ExePath $ExePath -FixturesRoot $FixturesRoot -IncludePathological:$IncludePathological -Propose:$Propose
    Write-Host $run.Output
    $results = @(ConvertFrom-SelfCheckOutput -Text $run.Output)
    if (-not (Test-OracleEvidence -Run $run -Results $results)) {
        Write-Host "[oracle-conformance] incomplete or inconsistent oracle evidence under $RootLabel (exit $($run.ExitCode)); no passing claim can be made." -ForegroundColor Red
        return $null
    }
    if ($MapFilterPasses) {
        foreach ($r in $results) {
            if ($r.Fixture -like 'edge-cases/*') { $r.Fixture = 'filter-passes/' + $r.Fixture.Substring('edge-cases/'.Length) }
        }
    }
    $newDivergences = @(); $known = @()
    foreach ($failure in @($results | Where-Object Status -eq FAIL)) {
        if (Test-OracleKnownFailure -Failure $failure -Baseline $Baseline -FixturesRoot $FixturesRoot -ExpectCleanBaseline:$ExpectCleanBaseline) {
            $known += $failure
            Write-Host "  $($failure.Fixture): [$($Baseline[$failure.Fixture].kind), exact reviewed evidence] $($failure.Reason)" -ForegroundColor Yellow
        } else {
            $newDivergences += $failure
            Write-Host "  NEW $($failure.Fixture): $($failure.Reason)" -ForegroundColor Red
        }
    }
    $passCount = @($results | Where-Object Status -eq PASS).Count
    $skipCount = @($results | Where-Object Status -eq SKIP).Count
    Write-Host "[oracle-conformance] ${RootLabel}: $($results.Count) attempted; $passCount PASS, $($known.Count) exact known FAIL, $($newDivergences.Count) new FAIL, $skipCount SKIP."
    return $newDivergences.Count -eq 0
}

# ---- main ----

$exePath = Find-OracleExe -Explicit $ExePath
if (-not $exePath) { exit $script:ExitCodeOracleUnavailable }

$dotnetCmd = Get-DotnetCommand
if (-not $dotnetCmd) {
    Write-Host "[oracle-conformance] 'dotnet' is not on PATH. hc-conformance.exe is a .NET executable and needs the dotnet runtime to launch even though it is already built. Install the .NET 10 SDK/runtime and re-run." -ForegroundColor Red
    exit $script:ExitCodeOracleUnavailable
}

Write-Host "[oracle-conformance] using $exePath" -ForegroundColor Cyan
$baseline = Get-Baseline -Path $BaselinePath

$stagingRoot = Join-Path $repoRoot 'conformance-staging'
$ok = Test-SelfCheckRun -ExePath $exePath -FixturesRoot $stagingRoot -RootLabel 'conformance-staging (local)' `
    -Baseline $baseline -IncludePathological:$IncludePathological -Propose:$Propose
if ($null -eq $ok) { exit $script:ExitCodeOracleUnavailable }

$allOk = $ok
if ($Scope -eq 'all') {
    $machineConformanceRoot = Join-Path $repoRoot 'machine\conformance'
    if (-not (Test-Path (Join-Path $machineConformanceRoot 'constructs.txt'))) {
        Write-Host "[oracle-conformance] -Scope all requested but machine/conformance is not initialized. Run rust\tools\conformance.ps1 first." -ForegroundColor Red
        exit $script:ExitCodeOracleUnavailable
    }
    # The control needs an exe matching the submodule's OWN pinned commit; read it from git, never hardcode it.
    $pinCommit = git -C (Join-Path $repoRoot 'machine') rev-parse HEAD 2>$null
    if (-not $pinCommit) {
        Write-Host "[oracle-conformance] could not read the machine submodule's pinned commit (git rev-parse HEAD failed)." -ForegroundColor Red
        exit $script:ExitCodeOracleUnavailable
    }
    $pinExe = Find-PinOracleExe -Explicit $PinExePath -PinCommit $pinCommit
    if (-not $pinExe) {
        # A divergence already found outranks a missing control exe; exiting 25 here would let a caller read it as merely unavailable.
        if (-not $ok) { Write-Host '[oracle-conformance] FAILED: new divergence under conformance-staging (the control run was also unavailable).' -ForegroundColor Red; exit $script:ExitCodeOracleDivergence }
        exit $script:ExitCodeOracleUnavailable
    }
    Write-Host "[oracle-conformance] control exe (pin-matched, commit $pinCommit): $pinExe" -ForegroundColor Cyan
    $controlOk = Test-SelfCheckRun -ExePath $pinExe -FixturesRoot $machineConformanceRoot -RootLabel 'machine/conformance (control, upstream, pin-matched exe)' `
        -Baseline @{} -IncludePathological:$IncludePathological -Propose:$Propose -ExpectCleanBaseline
    if ($null -eq $controlOk) { exit $script:ExitCodeOracleUnavailable }
    $allOk = $allOk -and $controlOk
}

# Materialized under a throwaway edge-cases/<name> mirror; the real fixtures are never moved.
$filterPassesRoot = Join-Path $stagingRoot 'filter-passes'
if (Test-Path $filterPassesRoot) {
    $filterPassesTempRoot = Get-FilterPassesOracleRoot -RepoRoot $repoRoot
    $filterPassesOk = Test-FilterPassesSelfCheckRun -ExePath $exePath -FilterPassesRoot $filterPassesRoot `
        -TempRoot $filterPassesTempRoot -Baseline $baseline -IncludePathological:$IncludePathological -Propose:$Propose
    if ($null -eq $filterPassesOk) { exit $script:ExitCodeOracleUnavailable }
    $allOk = $allOk -and $filterPassesOk
}

if (-not $allOk) {
    Write-Host ""
    Write-Host "[oracle-conformance] FAILED: at least one new divergence against the C# founding oracle." -ForegroundColor Red
    exit $script:ExitCodeOracleDivergence
}

Write-Host ""
Write-Host "[oracle-conformance] PASSED: no divergence against the C# founding oracle outside the known baseline." -ForegroundColor Green
exit 0
