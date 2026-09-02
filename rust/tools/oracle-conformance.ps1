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
  already in the baseline is reported but does not fail the gate; a FAIL that is NOT in the baseline
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
    <#
      .DESCRIPTION
      Runs hc-conformance.exe self-check over a materialized mirror of conformance-staging/
      filter-passes/** (see New-FilterPassesOracleRoot) and reports against the baseline, keyed by
      the fixtures' REAL id (`filter-passes/<name>`, never the materialized `edge-cases/<name>`).
      Mirrors Test-SelfCheckRun's shape and return contract ($null = harness could not run, $false =
      new divergence, $true = clean or fully baselined) so the caller's exit-code logic is unchanged.
    #>
    param(
        [string]$ExePath,
        [string]$FilterPassesRoot,
        [string]$TempRoot,
        [hashtable]$Baseline,
        [switch]$IncludePathological,
        [switch]$Propose
    )

    Write-Host ""
    Write-Host "[oracle-conformance] self-check: conformance-staging/filter-passes (materialized under $TempRoot as edge-cases/<name> for discovery; real fixtures are never moved)" -ForegroundColor Cyan

    $fixtureNames = @(New-FilterPassesOracleRoot -FilterPassesRoot $FilterPassesRoot -DestRoot $TempRoot)
    if ($fixtureNames.Count -eq 0) {
        Write-Host "[oracle-conformance] no filter-passes fixtures with both grammar.xml and words.yaml found under $FilterPassesRoot -- nothing to self-check." -ForegroundColor Yellow
        return $true
    }

    $run = Invoke-OracleSelfCheck -ExePath $ExePath -FixturesRoot $TempRoot -IncludePathological:$IncludePathological -Propose:$Propose
    Write-Host $run.Output

    if ($run.ExitCode -ne 0 -and $run.ExitCode -ne 1) {
        Write-Host "[oracle-conformance] hc-conformance.exe could not run against the materialized filter-passes root (exit $($run.ExitCode)) -- see output above." -ForegroundColor Red
        return $null
    }

    $results = ConvertFrom-SelfCheckOutput -Text $run.Output
    foreach ($r in $results) {
        if ($r.Fixture -like 'edge-cases/*') {
            $r.Fixture = 'filter-passes/' + $r.Fixture.Substring('edge-cases/'.Length)
        }
    }

    $fails = $results | Where-Object { $_.Status -eq 'FAIL' }
    $newDivergences = @()
    $baselined = @()
    foreach ($f in $fails) {
        if ($Baseline.ContainsKey($f.Fixture)) { $baselined += $f } else { $newDivergences += $f }
    }

    if ($baselined.Count -gt 0) {
        Write-Host ""
        Write-Host "[oracle-conformance] $($baselined.Count) known (baselined) divergence(s) under filter-passes -- tolerated, not gating:" -ForegroundColor Yellow
        foreach ($f in $baselined) {
            $entry = $Baseline[$f.Fixture]
            Write-Host "  $($f.Fixture): [$($entry.kind)] $($f.Reason)" -ForegroundColor Yellow
        }
    }

    if ($newDivergences.Count -gt 0) {
        Write-Host ""
        Write-Host "[oracle-conformance] *** $($newDivergences.Count) NEW divergence(s) under filter-passes (not in the known-divergence baseline) ***" -ForegroundColor Red
        foreach ($f in $newDivergences) { Write-Host "  $($f.Fixture): $($f.Reason)" -ForegroundColor Red }
        return $false
    }

    Write-Host "[oracle-conformance] filter-passes: no new divergence ($($results.Count) fixture(s) attempted, $($baselined.Count) known-baselined)." -ForegroundColor Green
    return $true
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

function Test-SelfCheckRun {
    <#
      .DESCRIPTION
      Runs self-check over one fixtures root and reports against the baseline. Returns $true if this
      root introduced no NEW divergence (baselined FAILs are printed but do not count).
    #>
    param(
        [string]$ExePath,
        [string]$FixturesRoot,
        [string]$RootLabel,
        [hashtable]$Baseline,
        [switch]$IncludePathological,
        [switch]$Propose,
        [switch]$ExpectCleanBaseline
    )

    Write-Host ""
    Write-Host "[oracle-conformance] self-check: $RootLabel ($FixturesRoot)" -ForegroundColor Cyan
    $run = Invoke-OracleSelfCheck -ExePath $ExePath -FixturesRoot $FixturesRoot -IncludePathological:$IncludePathological -Propose:$Propose
    Write-Host $run.Output

    if ($run.ExitCode -ne 0 -and $run.ExitCode -ne 1) {
        Write-Host "[oracle-conformance] hc-conformance.exe could not run against $RootLabel (exit $($run.ExitCode)) -- bad --fixtures path or zero fixtures discovered. See output above." -ForegroundColor Red
        return $null
    }

    $results = ConvertFrom-SelfCheckOutput -Text $run.Output
    $fails = $results | Where-Object { $_.Status -eq 'FAIL' }
    $newDivergences = @()
    $baselined = @()

    foreach ($f in $fails) {
        if ($ExpectCleanBaseline) {
            # machine/conformance is already C#-authored ground truth, so no fail there is ever baselined.
            $newDivergences += $f
            continue
        }
        if ($Baseline.ContainsKey($f.Fixture)) {
            $baselined += $f
        } else {
            $newDivergences += $f
        }
    }

    if ($baselined.Count -gt 0) {
        Write-Host ""
        Write-Host "[oracle-conformance] $($baselined.Count) known (baselined) divergence(s) under $RootLabel -- tolerated, not gating:" -ForegroundColor Yellow
        foreach ($f in $baselined) {
            $entry = $Baseline[$f.Fixture]
            Write-Host "  $($f.Fixture): [$($entry.kind)] $($f.Reason)" -ForegroundColor Yellow
        }
    }

    if ($newDivergences.Count -gt 0) {
        Write-Host ""
        Write-Host "[oracle-conformance] *** $($newDivergences.Count) NEW divergence(s) under $RootLabel (not in the known-divergence baseline) ***" -ForegroundColor Red
        foreach ($f in $newDivergences) {
            Write-Host "  $($f.Fixture): $($f.Reason)" -ForegroundColor Red
        }
        return $false
    }

    Write-Host "[oracle-conformance] ${RootLabel}: no new divergence ($($results.Count) fixture(s) attempted, $($baselined.Count) known-baselined)." -ForegroundColor Green
    return $true
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
    if (-not $pinExe) { exit $script:ExitCodeOracleUnavailable }
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
