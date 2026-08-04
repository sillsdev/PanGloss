<#
  Per-word, per-construct timing over the conformance suite IN BOTH ENGINES -- the complete Rust
  HermitCrab (`pg_parse::Morpher`) and the compiled propose+confirm path
  (`pg_foma::composite::FomaAnalyzer`) -- writing `typology-speedup.csv` (canonical data) and
  `typology-speedup.md` (a rendered view of it).

  THIN FRONT END, same relationship `test.ps1` has to `pg.ps1`: this sets the two environment
  variables the harness reads and then calls the managed entry point. It contains no policy.

  ## Why this exists, given `typology-speedup.sh` already did

  The harness (`rust/crates/pg-foma/tests/typology_speedup.rs`) has been complete since 2026-07-31
  and its ONLY driver was a bash script whose payload is `cargo test --release -p pg-foma --test
  typology_speedup -- --ignored --nocapture full_corpus_report`. On this repo's own machine that
  command cannot run at all:

    - it is `.sh` on a Windows/PowerShell box, and
    - it is BARE CARGO, which `.claude/hooks/block-bare-cargo.py` refuses as a PreToolUse hook.

  So a finished measurement was unreachable by construction. The cost of that was not theoretical:
  `docs/fst-plan/grammar-optimization-techniques.md:521` lists per-candidate apply cost as "the one
  dimension with no measurement gap" and names this harness, while the project's own working notes
  simultaneously recorded that no per-word cost baseline existed for any compiler and that the
  harness still had to be BUILT. Both statements were written about the same code. An unreachable
  tool reads exactly like an absent one -- the same lesson `-TestTarget` earned two days earlier
  ("undiscoverable is the same as absent"), one level up: there, a flag existed and nobody could
  find it; here, a whole harness exists and nobody can start it.

  This matters beyond convenience. The endgame bar -- strip the hand-spun path once the recipe path
  is as good or better on completeness AND arcs AND raw-HC-word-verified -- plus the standing
  invariant "we should never be slower than rust HC itself" are BOTH questions this harness answers
  and nothing else does: it is the only thing in the tree that times the two engines over the same
  words. A bar nobody can evaluate is not a bar.

  ## What the managed path fixes for free

  Routing through `pg.ps1 -Mode test` is not merely "the allowed spelling of the same command" --
  it is strictly better than what the bash script did:

    - `--run-ignored all` is already the default there, so the `--ignored` passthrough is unneeded.
    - `-TestTarget` compiles ONE test binary instead of every target in the package (measured
      2026-08-03: 10.6s warm versus ~996s cold for the package).
    - the conformance submodule is auto-initialized in preflight, so the bash script's "warning:
      machine/conformance is empty, only staging fixtures will be measured" degraded run -- a
      SILENTLY narrower corpus -- cannot happen here. That warning was the measurement-integrity
      hazard in the old driver: fewer fixtures still produces a confident-looking CSV.
    - the run takes a build slot, is priority-capped, and is bounded by a procgov job object. This
      one matters more than usual for THIS script: it times every word repeatedly in two engines
      over the whole suite, which is exactly the long, heavy, memory-hungry shape that produced
      three separate 90-118GB direct-invocation incidents when run outside the managed path.

  Profile note: `-Mode test` builds with `pg-test-opt` (release-derived, thin LTO) rather than the
  bash script's `--release` (fat LTO). Deliberate, and the timings remain comparable across a run
  because BOTH engines are measured inside that same binary -- the harness reports a ratio between
  two engines built identically, not an absolute figure to be compared against some other build.
  If you need fat-LTO absolute numbers for an external comparison, that is a different question and
  should say so out loud rather than being silently assumed here.

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

# Saved and restored rather than just set: this script's whole point is that it can be called
# repeatedly from one long-lived agent session, and a leaked PG_TYPOLOGY_* would silently retarget
# the NEXT run's output -- the same "a Set-Location many calls earlier retargets everything after
# it" failure `Assert-ScriptAndCwdAgreeOnWorktree` exists to refuse, in environment-variable form.
$priorOutDir = $env:PG_TYPOLOGY_OUT_DIR
$priorRepeats = $env:PG_TYPOLOGY_REPEATS

try {
    $env:PG_TYPOLOGY_OUT_DIR = $OutDir
    if ($Repeats -gt 0) { $env:PG_TYPOLOGY_REPEATS = "$Repeats" }

    Write-Host "[typology-speedup] out-dir: $OutDir"
    Write-Host "[typology-speedup] timing BOTH engines over every conformance fixture -- this is a long run."

    # -TestTarget selects the binary (compilation), -Filter selects the test (execution). Passing
    # the file name to -Filter instead is the mistake this repo made seven times in one session; see
    # pg.ps1's own parameter comments. Both are needed here and they are NOT interchangeable.
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
