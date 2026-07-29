<#
  Managed entry point for the PanGloss Rust workspace
  (docs/superpowers/specs/2026-07-29-categorical-build-hardening-design.md). Run from any
  worktree -- it resolves its own paths (Get-RepoRoot/Get-RustRoot), same as build.ps1/test.ps1
  always have.

  This is the ONE place that decides target-dir redirection, sccache wiring, the worktree
  base-commit check, disk/build-slot gates, and (for corpus-test) the fail-closed corpus
  contract. rust/tools/build.ps1 and rust/tools/test.ps1 are thin front ends that translate their
  existing parameters into a call here, so there is exactly one place that policy is decided
  rather than two copies that can drift.

  Modes:
    build        cargo build. --release unless -DebugProfile (matches build.ps1's existing
                  default exactly -- this mode exists for backward compatibility with that
                  script's long-standing behavior, not because "build" is meant to imply a dev
                  profile).
    test          the fast suite, built with the pg-test-opt profile (release-derived, thin/no
                  LTO -- see rust/Cargo.toml's comment) instead of full release, unless
                  -DebugProfile. Prefers cargo-nextest when installed, like test.ps1 always has.
    corpus-test   like test, but refuses BEFORE cargo starts if any required corpus-manifest file
                  is absent, sets PANGLOSS_CORPUS_REQUIRED=1 so pg_conformance_fixtures::corpus
                  panics rather than skips on a missing input, and fails afterward if cargo
                  exited 0 having recorded zero executed corpus cases.
    release       cargo build --release -- the actual fat-LTO deliverable profile, for optimized
                  binaries and production-equivalent perf measurements. Marks the target dir's
                  ownership marker `preserved` on success so a dry-run gc reports it rather than
                  offering to delete it.
    doctor        prints the preflight record and exits non-zero on an unsafe/incomplete
                  environment. Runs no cargo command at all.
    gc            reports (dry-run, the default) or removes (-Apply) managed target directories
                  this repository owns and no longer needs. Never touches an unmarked, preserved,
                  or still-live directory -- see Get-TargetClassification/Invoke-TargetGc in
                  _common.ps1.

  Examples:
    rust\tools\pg.ps1 -Mode build -Package pg-foma
    rust\tools\pg.ps1 -Mode test
    rust\tools\pg.ps1 -Mode corpus-test -Package pg-foma -Filter f1_sena
    rust\tools\pg.ps1 -Mode release
    rust\tools\pg.ps1 -Mode doctor
    rust\tools\pg.ps1 -Mode gc            # dry run, reports only
    rust\tools\pg.ps1 -Mode gc -Apply     # actually deletes disposable targets
#>
param(
    [Parameter(Mandatory)]
    [ValidateSet('build', 'test', 'corpus-test', 'release', 'doctor', 'gc', 'new-worktree')]
    [string]$Mode,
    # new-worktree only: where to create it, which revision to base it on, and the branch name.
    [string]$Path = '',
    [string]$Base = '',
    [string]$Branch = '',
    [string]$Package = '',
    [string]$Filter = '',
    [switch]$DebugProfile,
    [switch]$NoNextest,
    [int]$MaxConcurrent = 2,
    # 30 minutes: long enough that a normal queued build never trips this, short enough that a
    # genuinely wedged holder (crashed mid-build without releasing) is reported rather than
    # blocking every other worktree's build silently forever.
    [int]$BuildSlotTimeoutSeconds = 1800,
    [switch]$NoSccache,
    [ValidateSet('strict', 'development', 'off')][string]$BaseMode = 'development',
    [switch]$Apply,
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$ExtraArgs
)

. "$PSScriptRoot\_common.ps1"

# The optimized-but-not-fat-LTO profile rust/Cargo.toml declares as `[profile.pg-test-opt]`
# (inherits = "release", lto = "thin", codegen-units = 16, reduced debug info) -- see that file's
# comment for why `test`/`corpus-test` must not default to release's fat LTO/codegen-units=1.
$script:TestOptProfile = 'pg-test-opt'

$repoRoot = Get-RepoRoot
$rustRoot = Get-RustRoot

if ($Mode -eq 'new-worktree') {
    # The bootstrap half of the exact-base contract. Without it, Write-WorktreeMeta is never called,
    # no worktree has metadata, and Test-WorktreeBase can only ever report "unverified" -- the check
    # exists but has nothing to check against. This is the observed failure it prevents: a worktree
    # requested at one commit materialized at a DIFFERENT one (an older session's tip), and nothing
    # compared the two before minutes of building measured the wrong tree.
    if (-not $Path) { Write-Host '[pg] new-worktree requires -Path' -ForegroundColor Red; exit 2 }
    if (-not $Base) { Write-Host '[pg] new-worktree requires -Base (a revision: branch, tag, or SHA)' -ForegroundColor Red; exit 2 }

    # Resolve to a full object ID BEFORE creating anything, and peel to a commit so an annotated tag
    # records the commit it points at rather than the tag object. A branch name resolved later (or
    # left unresolved) is exactly how a worktree ends up on a different commit than intended: the
    # branch can move between the request and the creation.
    $resolved = (git rev-parse --verify "$Base^{commit}" 2>$null)
    if ($LASTEXITCODE -ne 0 -or -not $resolved) {
        Write-Host "[pg] cannot resolve -Base '$Base' to a commit in this repository." -ForegroundColor Red
        exit $script:ExitCodeWrongBase
    }
    $resolved = $resolved.Trim()
    if (Test-Path $Path) {
        Write-Host "[pg] refusing to create a worktree at an existing path: $Path" -ForegroundColor Red
        exit 2
    }
    if (-not $Branch) { $Branch = "worktree-" + (Split-Path $Path -Leaf) }

    Write-Host "[pg] creating worktree at $Path from $Base -> $resolved (branch $Branch)" -ForegroundColor Cyan
    & git worktree add -b $Branch $Path $resolved
    if ($LASTEXITCODE -ne 0) {
        Write-Host '[pg] git worktree add failed; nothing was recorded.' -ForegroundColor Red
        exit $LASTEXITCODE
    }

    $newRoot = (Resolve-Path -LiteralPath $Path).ProviderPath
    $meta = Write-WorktreeMeta -RepoRoot $newRoot -RequestedRevision $Base -ResolvedObjectId $resolved -Branch $Branch
    Write-Host "[pg] recorded base in $(Get-WorktreeMetaPath -RepoRoot $newRoot)" -ForegroundColor Green
    Write-Host "[pg] requested '$($meta.requested_revision)' -> resolved $($meta.resolved_object_id)" -ForegroundColor DarkGray
    Write-Host '[pg] NOTE: private corpora under samples/data/ are gitignored and are NOT copied into a new worktree. Corpus-backed suites there will refuse under -Mode corpus-test until you populate it or set PANGLOSS_CORPUS_ROOT.' -ForegroundColor Yellow
    exit 0
}

if ($NoSccache) {
    # "Explicit, noisy, and incompatible with parallel managed builds" (design doc, error
    # handling): forcing MaxConcurrent to 1 means a caller can't accidentally combine no-cache
    # with the normal 2-way concurrency and double the uncached CPU/disk contention.
    Write-Host '[pg] NO-CACHE EMERGENCY MODE (-NoSccache): this build will not share compiled artifacts with any other worktree. Forcing MaxConcurrent=1.' -ForegroundColor Magenta
    $MaxConcurrent = 1
}

$targetDir = Resolve-TargetDir -RustRoot $rustRoot
if ($targetDir) { $env:CARGO_TARGET_DIR = $targetDir }

$repoId = Get-RepoIdentity -RepoRoot $repoRoot

if ($targetDir) {
    $ownership = Write-TargetOwnership -TargetDir $targetDir -RepositoryId $repoId -WorktreePath $repoRoot
    if (-not $ownership.Ok) {
        Write-Host "[pg] $($ownership.Detail)" -ForegroundColor Red
        Write-Host '[pg] recovery: point CARGO_TARGET_DIR (or PANGLOSS_SSD_CACHE_ROOT/PANGLOSS_CARGO_CACHE_ROOT) at a target dir this repository actually owns, or remove the stale marker only if you are certain it is safe to reclaim.' -ForegroundColor Yellow
        exit $script:ExitCodeBadTargetOwnership
    }
}

$usedSccache = if (-not $NoSccache) { Use-Sccache } else { $false }
$sccacheHealth = if ($usedSccache) {
    Test-SccacheHealth
} elseif ($NoSccache) {
    [PSCustomObject]@{ State = 'disabled'; Ok = $true; Detail = 'disabled via -NoSccache' }
} else {
    [PSCustomObject]@{ State = 'not-installed'; Ok = $true; Detail = 'sccache not found on PATH' }
}
if ($usedSccache -and -not $sccacheHealth.Ok) {
    Write-Host "[pg] sccache is installed but unhealthy: $($sccacheHealth.Detail)" -ForegroundColor Red
    Write-Host '[pg] recovery: fix sccache (check SCCACHE_DIR permissions / disk space), or pass -NoSccache for the documented no-cache emergency mode.' -ForegroundColor Yellow
    exit $script:ExitCodeCacheUnavailable
}

$baseCheck = Test-WorktreeBase -Mode $BaseMode -RepoRoot $repoRoot

$freeGB = Get-FreeSpaceGB -Path $(if ($targetDir) { $targetDir } else { $rustRoot })
$diskCheck = Test-DiskReserve -FreeGB $freeGB

$corpusManifest = $null
$corpusState = $null
if ($Mode -eq 'corpus-test') {
    $corpusManifest = Get-CorpusManifest -RepoRoot $repoRoot
    $corpusState = Test-CorpusPresent -RepoRoot $repoRoot -Manifest $corpusManifest
}

$profileLabel = switch ($Mode) {
    'release' { 'release (fat LTO)' }
    'test' { if ($DebugProfile) { 'dev' } else { $script:TestOptProfile } }
    'corpus-test' { if ($DebugProfile) { 'dev' } else { $script:TestOptProfile } }
    'doctor' { '<none -- doctor runs no cargo command>' }
    'gc' { '<none -- gc runs no cargo command>' }
    default { if ($DebugProfile) { 'dev' } else { 'release (fat LTO)' } }
}

Write-Preflight -Mode $Mode -Profile $profileLabel -RepoRoot $repoRoot -TargetDir $targetDir `
    -BaseCheck $baseCheck -SccacheHealth $sccacheHealth -FreeGB $freeGB -DiskCheck $diskCheck `
    -CorpusState $corpusState -MaxConcurrent $MaxConcurrent

if ($BaseMode -ne 'off' -and $baseCheck.Checked -and -not $baseCheck.Ok) {
    Write-Host "[pg] worktree base check FAILED ($BaseMode mode): $($baseCheck.Detail)" -ForegroundColor Red
    Write-Host '[pg] recovery: this worktree is not at the commit it was created from. Re-create the worktree at the intended base rather than rebasing/checking out automatically -- this tool never does that for you (it can discard context or invalidate a build cache you were relying on).' -ForegroundColor Yellow
    exit $script:ExitCodeWrongBase
}

if (-not $diskCheck.Ok) {
    Write-Host "[pg] $($diskCheck.Detail)" -ForegroundColor Red
    exit $script:ExitCodeLowDisk
}

if ($Mode -eq 'corpus-test' -and -not $corpusState.Ok) {
    Write-Host '[pg] corpus-test refused BEFORE starting cargo -- required corpus file(s) missing:' -ForegroundColor Red
    foreach ($m in $corpusState.Missing) { Write-Host "  $m" -ForegroundColor Red }
    Write-Host "[pg] recovery: populate $($corpusState.CorpusRoot), or point PANGLOSS_CORPUS_ROOT at a populated corpus root." -ForegroundColor Yellow
    exit $script:ExitCodeMissingCorpus
}

if ($Mode -eq 'doctor') {
    $unsafe = ($baseCheck.Checked -and -not $baseCheck.Ok) -or (-not $diskCheck.Ok) -or ($usedSccache -and -not $sccacheHealth.Ok)
    if ($unsafe) {
        Write-Host '[pg] doctor: environment is UNSAFE for a managed build (see failures above).' -ForegroundColor Red
        exit 1
    }
    Write-Host '[pg] doctor: environment looks safe for a managed build.' -ForegroundColor Green
    exit 0
}

if ($Mode -eq 'gc') {
    # Reap dead-parent orphans first (cheap, always safe) regardless of -Apply -- an orphaned
    # rustc/link process holding file locks is exactly what would make a real deletion below fail
    # partway through.
    Remove-OrphanedCargoProcesses -WhatIfOnly:(-not $Apply)

    $classification = Get-TargetClassification -RepositoryId $repoId
    foreach ($c in ($classification | Sort-Object Class, Path)) {
        $color = switch ($c.Class) {
            'disposable' { 'Yellow' }
            'unknown' { 'DarkGray' }
            default { 'Gray' }
        }
        Write-Host "[gc] $($c.Class): $($c.Path) ($($c.SizeGB)GB) -- $($c.Detail)" -ForegroundColor $color
    }

    $busy = if ($Apply) { @(Get-LiveBuildProcesses) } else { @() }
    $gcResult = Invoke-TargetGc -Classification $classification -Apply:$Apply -BusyProcesses $busy
    $plural = if ($gcResult.Disposable.Count -eq 1) { 'y' } else { 'ies' }
    if ($gcResult.Skipped) {
        Write-Host "[gc] $($gcResult.SkipReason) ($($gcResult.Disposable.Count) disposable director$plural found)" -ForegroundColor $(if ($Apply) { 'Red' } else { 'Cyan' })
    } else {
        Write-Host "[gc] removed $($gcResult.Deleted.Count) director$plural." -ForegroundColor Yellow
    }
    exit 0
}

# build / test / corpus-test / release all run cargo from here.
$useNextest = ($Mode -eq 'test' -or $Mode -eq 'corpus-test') -and (-not $NoNextest) -and (Get-Command cargo-nextest -ErrorAction SilentlyContinue)

$cargoArgs = @()
switch ($Mode) {
    'build' {
        $cargoArgs += 'build'
        if (-not $DebugProfile) { $cargoArgs += '--release' }
    }
    'release' {
        $cargoArgs += @('build', '--release')
    }
    'test' {
        if ($useNextest) {
            $cargoArgs += @('nextest', 'run')
            if (-not $DebugProfile) { $cargoArgs += @('--cargo-profile', $script:TestOptProfile) }
        } else {
            $cargoArgs += 'test'
            if (-not $DebugProfile) { $cargoArgs += @('--profile', $script:TestOptProfile) }
        }
    }
    'corpus-test' {
        # MUST run ignored tests. Every corpus-backed suite in this repo is `#[ignore]`d precisely
        # BECAUSE it needs the private corpus ("needs local gitignored corpus data ...; run with
        # --include-ignored"), so a corpus-test that respects the default ignore filter runs ZERO
        # corpus tests -- measured: `Starting 0 tests across 58 binaries (660 tests skipped)`. That
        # failure is self-concealing: the run trips the zero-executed-cases guard, which looks like
        # the guard working rather than the mode never having executed a corpus test at all.
        if ($useNextest) {
            $cargoArgs += @('nextest', 'run', '--run-ignored', 'all')
            if (-not $DebugProfile) { $cargoArgs += @('--cargo-profile', $script:TestOptProfile) }
        } else {
            $cargoArgs += 'test'
            if (-not $DebugProfile) { $cargoArgs += @('--profile', $script:TestOptProfile) }
        }
    }
}
if ($Package) { $cargoArgs += @('-p', $Package) } else { $cargoArgs += '--workspace' }

if ($useNextest) {
    if ($Filter) { $cargoArgs += $Filter }
    # nextest's own flag name for "print stdout even for passing tests" -- without it,
    # PANGLOSS_CORPUS_CASES lines from PASSING tests would be swallowed, and a fully successful
    # corpus run would misreport as zero cases executed.
    if ($Mode -eq 'corpus-test') { $cargoArgs += '--no-capture' }
} else {
    $trailing = @()
    if ($Filter) { $trailing += $Filter }
    if ($Mode -eq 'corpus-test') {
        # plain `cargo test` hides passing tests' stdout the same way; same reason as above.
        $trailing += '--nocapture'
        # The `cargo test` spelling of nextest's `--run-ignored all` above -- same reason: without it
        # this mode cannot execute a single corpus-backed test, since they are all `#[ignore]`d.
        $trailing += '--include-ignored'
    }
    if ($trailing.Count -gt 0) { $cargoArgs += @('--') + $trailing }
}
if ($ExtraArgs) { $cargoArgs += $ExtraArgs }

if ($Mode -eq 'corpus-test') { $env:PANGLOSS_CORPUS_REQUIRED = '1' }

$sem = Enter-BuildSlot -MaxConcurrent $MaxConcurrent -TimeoutSeconds $BuildSlotTimeoutSeconds
if (-not $sem) {
    Write-Host "[pg] timed out after ${BuildSlotTimeoutSeconds}s waiting for a build slot (max $MaxConcurrent concurrent across all worktrees) -- another worktree's build is holding it." -ForegroundColor Red
    exit $script:ExitCodeBuildSlotTimeout
}
$code = 1
try {
    $runnerLabel = if ($useNextest) { 'nextest' } elseif ($Mode -eq 'build' -or $Mode -eq 'release') { 'cargo build' } else { 'cargo test' }
    Write-Host "[pg] cargo $($cargoArgs -join ' ')  (target-dir: $(if ($targetDir) { $targetDir } else { '<default>' }), runner: $runnerLabel)" -ForegroundColor Cyan

    if ($Mode -eq 'corpus-test') {
        $capturePath = Join-Path ([System.IO.Path]::GetTempPath()) "pg-corpus-test-$PID.log"
        $code = Invoke-CargoWithReaper -Exe 'cargo' -CmdArgs $cargoArgs -WorkingDirectory $rustRoot -CaptureStdoutPath $capturePath
        $lines = if (Test-Path $capturePath) { Get-Content $capturePath } else { @() }
        $lines | ForEach-Object { Write-Host $_ }
        $caseLines = @($lines | Where-Object { $_ -match '^PANGLOSS_CORPUS_CASES\s+(\S+)\s+(\d+)$' })
        $totalCases = 0
        foreach ($line in $caseLines) {
            $null = $line -match '^PANGLOSS_CORPUS_CASES\s+(\S+)\s+(\d+)$'
            $totalCases += [int]$Matches[2]
        }
        Remove-Item -Force $capturePath -ErrorAction SilentlyContinue
        if ($code -eq 0 -and $totalCases -eq 0) {
            Write-Host '[pg] corpus-test exited 0 but recorded ZERO executed corpus cases -- a suite that compiles, runs, and exercises nothing is a failure, not a pass. Check that the run actually reached a test calling pg_conformance_fixtures::corpus::record_cases (a too-narrow -Filter/-Package can also cause this).' -ForegroundColor Red
            exit $script:ExitCodeZeroCorpusCases
        }
        Write-Host "[pg] corpus-test executed $totalCases corpus case(s) across $($caseLines.Count) label(s)." -ForegroundColor Green
    } else {
        $code = Invoke-CargoWithReaper -Exe 'cargo' -CmdArgs $cargoArgs -WorkingDirectory $rustRoot
    }
} finally {
    Exit-BuildSlot -Semaphore $sem
}

if ($Mode -eq 'release' -and $code -eq 0 -and $targetDir) {
    # A failed build never registers a release deliverable (design doc, error handling) -- only
    # mark `preserved` after cargo itself reports success.
    Write-TargetOwnership -TargetDir $targetDir -RepositoryId $repoId -WorktreePath $repoRoot -Preserved | Out-Null
}

exit $code
