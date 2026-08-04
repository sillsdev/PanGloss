<#
  Per-word, per-construct timing over the conformance suite IN BOTH ENGINES -- the complete Rust
  HermitCrab (`pg_parse::Morpher`) and the compiled propose+confirm path
  (`pg_foma::composite::FomaAnalyzer`) -- writing `typology-speedup.csv` (canonical data) and
  `typology-speedup.md` (a rendered view of it).

  Thin front end: sets the two environment variables the harness reads, then calls the managed
  entry point. No policy lives here.

  Preferred over `typology-speedup.sh`, which drives the same harness with bare cargo and so is
  unusable on Windows and refused by the bare-cargo hook.

  Routing through `pg.ps1 -Mode test` also buys three things the bash driver could not:
  `--run-ignored all` is already the default, `-TestTarget` compiles one test binary rather than
  every target in the package, and preflight initializes the conformance submodule -- without which
  the run silently measures only the staging fixtures and still produces a confident-looking CSV.
  The run is also slot-gated and memory-capped, which matters here because timing every word in two
  engines over the whole suite is long and memory-hungry.

  Profile: `-Mode test` builds with `pg-test-opt` (thin LTO), not `--release` (fat LTO). The harness
  reports a RATIO between two engines built inside the same binary, so cross-engine comparison is
  unaffected; only absolute figures compared against some other build would be.

  Usage:
    rust\tools\typology-speedup.ps1
    rust\tools\typology-speedup.ps1 -OutDir C:\tmp\speedup -Repeats 11
    rust\tools\typology-speedup.ps1 -NoNextest        # libtest, for live per-test progress output
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    # Default matches the bash script's, so a CSV produced either way lands in the same place.
    [string]$OutDir = '',
    # Timed samples per word per engine, after one discarded warmup. The harness's own default is 7;
    # 0 here means "say nothing and let the harness decide" rather than restating its default in a
    # second place, which is the same convention test.ps1/build.ps1 use for -Jobs/-TestThreads.
    [int]$Repeats = 0,
    # nextest gives no live output for a single long-running test. libtest with --nocapture does,
    # which is worth having when the run is measured in minutes and you want to see it progressing.
    [switch]$NoNextest,
    [int]$MaxConcurrent = 2,
    [int]$Jobs = 0,
    [int]$TestThreads = 0,
    [ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = ''
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if (-not $OutDir) { $OutDir = Join-Path $repoRoot 'rust\target\typology-speedup' }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

# Saved and restored: a leaked PG_TYPOLOGY_* would silently retarget a later run's output.
$priorOutDir = $env:PG_TYPOLOGY_OUT_DIR
$priorRepeats = $env:PG_TYPOLOGY_REPEATS

try {
    $env:PG_TYPOLOGY_OUT_DIR = $OutDir
    if ($Repeats -gt 0) { $env:PG_TYPOLOGY_REPEATS = "$Repeats" }

    Write-Host "[typology-speedup] out-dir: $OutDir"
    Write-Host "[typology-speedup] timing BOTH engines over every conformance fixture -- this is a long run."

    # -TestTarget selects the binary (compilation), -Filter selects the test (execution). Both are
    # needed and they are not interchangeable.
    $pgArgs = @{
        Mode       = 'test'
        Package    = 'pg-foma'
        TestTarget = 'typology_speedup'
        Filter     = 'full_corpus_report'
    }
    if ($NoNextest) { $pgArgs.NoNextest = $true }
    if ($MaxConcurrent) { $pgArgs.MaxConcurrent = $MaxConcurrent }
    if ($Jobs -gt 0) { $pgArgs.Jobs = $Jobs }
    if ($TestThreads -gt 0) { $pgArgs.TestThreads = $TestThreads }
    if ($Priority) { $pgArgs.Priority = $Priority }

    & "$PSScriptRoot\pg.ps1" @pgArgs
    $code = $LASTEXITCODE
}
finally {
    $env:PG_TYPOLOGY_OUT_DIR = $priorOutDir
    $env:PG_TYPOLOGY_REPEATS = $priorRepeats
}

$csv = Join-Path $OutDir 'typology-speedup.csv'
$md = Join-Path $OutDir 'typology-speedup.md'

if ($code -ne 0) {
    Write-Host "[typology-speedup] FAILED (exit $code). See pg.ps1's exit-code table for what the code means." -ForegroundColor Red
    exit $code
}

# "I could not look" must never read as "everything is fine": a green test run that wrote no CSV is
# a harness that did not measure, not a measurement of nothing. The suite passing and the artifact
# existing are two different facts, so check the second one explicitly.
if (-not (Test-Path $csv)) {
    Write-Host "[typology-speedup] run reported success but produced no CSV at $csv" -ForegroundColor Red
    Write-Host "[typology-speedup] the filter matched no test, or the harness wrote elsewhere -- treat this as NO measurement." -ForegroundColor Yellow
    exit 1
}

$rows = (Get-Content $csv | Measure-Object -Line).Lines - 1
Write-Host "[typology-speedup] CSV:      $csv  ($rows data row(s))" -ForegroundColor Green
if (Test-Path $md) { Write-Host "[typology-speedup] Markdown: $md" -ForegroundColor Green }
exit 0
