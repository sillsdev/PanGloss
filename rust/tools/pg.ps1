<#
  .DESCRIPTION
  Managed entry point for the PanGloss Rust workspace. Run from any
  worktree -- it resolves its own paths (Get-RepoRoot/Get-RustRoot), same as build.ps1/test.ps1
  always have.

  This is the ONE place that decides target-dir redirection, sccache wiring, the worktree
  base-commit check, disk/build-slot gates, the fail-closed corpus contract (corpus-test), and the
  fail-closed `machine` conformance-submodule contract (test/corpus-test). rust/tools/build.ps1 and
  rust/tools/test.ps1 are thin front ends that translate their existing parameters into a call
  here, so there is exactly one place that policy is decided rather than two copies that can drift.

  Modes:
    check         cargo check --all-targets. The fast inner loop: it type-checks TEST and EXAMPLE
                  code, which `build` never touches, and stops before codegen and linking. That
                  matters because linking is where the time is -- pg-foma alone has 105 integration
                  test targets and 32 examples, and a green `build` has twice hidden broken test
                  code that only a full `test` round-trip revealed. Skips comment hygiene: that is a
                  prose check, and this mode is asked exactly one question. Uses the same profile as
                  `test` so its fingerprints are the ones a later test run reuses.
    quick         check's question plus unit tests: `nextest run --lib --bins`. Deliberately does
                  NOT build the integration targets -- that is the expensive half and what `test`
                  is for. Hygiene off, same profile, so a warm loop stays inside a few minutes.
                  A green `quick` is not a green suite; it is a fast way to be wrong less often.
    build        cargo build. --release unless -DebugProfile (matches build.ps1's existing
                  default exactly -- this mode exists for backward compatibility with that
                  script's long-standing behavior, not because "build" is meant to imply a dev
                  profile).
    test          the fast suite, built with the pg-test-opt profile (release-derived, thin/no
                  LTO -- see rust/Cargo.toml's comment) instead of full release, unless
                  -DebugProfile. Prefers cargo-nextest when installed, like test.ps1 always has.
                  Refuses BEFORE cargo starts (exit $script:ExitCodeConformanceSubmoduleMissing,
                  18) if the `machine` conformance submodule can't be auto-initialized --
                  conformance_fixtures_gate is part of this ordinary suite, not #[ignore]d, so a
                  fresh worktree that skipped this would fail minutes into a build with a
                  confusing panic instead. See Initialize-ConformanceSubmodule in _common.ps1 (or
                  run rust/tools/conformance.ps1 standalone) for what "auto-initialize" means: a
                  sparse, path-scoped checkout of machine/conformance ONLY (~1MB), never the
                  ~415MB full `machine` checkout, because that's the only subtree this suite reads.
    corpus-test   like test, but ALSO refuses BEFORE cargo starts if any required corpus-manifest
                  file is absent, sets PANGLOSS_CORPUS_REQUIRED=1 so pg_conformance_fixtures::corpus
                  panics rather than skips on a missing input, and fails afterward if cargo
                  exited 0 having recorded zero executed corpus cases.
    conformance-test
                  the same suite `test` runs, but it MUST be told which fixtures it covers:
                  -Scope local (conformance-staging/** only, this repo's own fixtures) or
                  -Scope all (those plus machine/conformance/**). There is NO default -- an
                  unclaimed run exits $script:ExitCodeConformanceScopeUnclaimed (20) before
                  taking a build slot or starting cargo, because "green" over the staged
                  fixtures alone and "green" over those plus every upstream fixture are
                  different claims and nothing here should guess which one you meant. The
                  claim reaches the fixture walker as PANGLOSS_CONFORMANCE_SCOPE and is
                  printed. -Scope local needs no submodule, so it skips that init entirely.
                  `test` and `corpus-test` claim `all` explicitly and print it too.
    release       cargo build --release -- the actual fat-LTO deliverable profile, for optimized
                  binaries and production-equivalent perf measurements. Marks the target dir's
                  ownership marker `preserved` on success so a dry-run gc reports it rather than
                  offering to delete it.
    doctor        prints the preflight record and exits non-zero on an unsafe/incomplete
                  environment -- including the `machine` conformance-submodule state (attempts the
                  same auto-init test/corpus-test do, since it's cheap and idempotent once already
                  present; folded into the unsafe/exit-code decision, unlike the exhaustion history
                  below, because it describes the environment RIGHT NOW rather than a past,
                  already-recovered-from event). Runs no cargo command at all. Also reports
                  (without failing on) any Resource-Exhaustion-Detector history from the last 7
                  days -- see Get-ResourceExhaustionEvents in _common.ps1.
    gc            reports (dry-run, the default) or removes (-Apply) managed target directories
                  this repository owns and no longer needs. Never touches an unmarked, preserved,
                  or still-live directory -- see Get-TargetClassification/Invoke-TargetGc in
                  _common.ps1.
    run           runs an arbitrary PanGloss binary -- an example, a workspace bin, or an
                  already-built .exe -- inside the SAME kernel-enforced job object a managed build
                  gets (Invoke-ProcessInJobObject in _common.ps1), instead of the unmanaged direct
                  invocation that took this machine to a frozen state three times (predict_census.exe
                  118GB, pangloss.exe 90GB, hc-rs.exe 97GB -- all invoked directly, none through
                  pg.ps1; see CLAUDE.md). Exactly ONE of -Example / -Bin / -Exe is required:
                    -Example <name>   `cargo run --example <name>` (builds first, then runs the
                                      result as a job-object CHILD of cargo -- procgov's `-r` flag
                                      recurses the ceiling onto it same as rustc/link.exe).
                    -Bin <name>       `cargo run --bin <name>`, same as above.
                    -Exe <path>       runs an already-built executable directly, no cargo involved.
                  Args after a literal `--` are passed through to the binary. `-Package` selects
                  which workspace crate's example/bin to build when the name is ambiguous; it is
                   ignored with -Exe. On Windows, `-RunMemoryGB` overrides the job's committed-memory
                   ceiling for one run (0 = derive the same machine-proportional cap a build gets).
                   On Linux, an explicit value is refused because the host cgroup owns the cap;
                   configure the finite host service cgroup instead. `run` DOES take a build slot
                  (Enter-BuildSlot) -- see the `run` mode block below in this file for why that is
                  the deliberate choice, not an oversight.

  Examples:
    rust\tools\pg.ps1 -Mode check                  # does everything, including test code, compile?
    rust\tools\pg.ps1 -Mode quick -Package pg-foma # + that package's unit tests
    rust\tools\pg.ps1 -Mode test -FailFast         # stop at the first failure (default: report all)
    rust\tools\pg.ps1 -Mode build -Package pg-foma
    rust\tools\pg.ps1 -Mode test
    rust\tools\pg.ps1 -Mode corpus-test -Package pg-foma -Filter f1_sena
    rust\tools\pg.ps1 -Mode release
    rust\tools\pg.ps1 -Mode doc            # rustdoc; the only thing that enforces the doc-link deny
    rust\tools\pg.ps1 -Mode doctor
    rust\tools\pg.ps1 -Mode gc            # dry run, reports only
    rust\tools\pg.ps1 -Mode gc -Apply     # actually deletes disposable targets
    rust\tools\pg.ps1 -Mode run -Example predict_census -- --grammar foo.xml
    rust\tools\pg.ps1 -Mode run -Bin pangloss -- batch --threads 1 --word-timeout-ms 5000
    rust\tools\pg.ps1 -Mode run -Exe C:\path\to\already-built.exe -- --some-flag
    rust\tools\pg.ps1 -Mode run -Exe .\predict_census.exe -RunMemoryGB 40   # Windows-only deliberate large-mem experiment

  -Filter vs -TestTarget (documented here because getting this backwards cost seven wrong invocations
  in one session): -Filter narrows EXECUTION ONLY -- appended as a bare positional to the test runner,
  it matches TEST NAMES as a substring, never file names or test-target names, and cargo still
  compiles and links every test target in the package regardless. -TestTarget narrows COMPILATION --
  it maps to cargo's `--test <name>`, building and linking ONE test binary instead of every target in
  the package, which for pg-foma is ~78 separate binaries and the difference between a ~10s warm run
  and a ~996s cold one. A zero-match -Filter fails loudly ("no tests to run", exit 4); a PARTIAL match
  is silent -- it runs some tests, exits 0, and omits the ones you meant.

  -Jobs / -TestThreads: 0 means "derive from the machine" (Get-CargoJobBudget: logical cores minus
  the interactive reserve, narrowed further by available memory, split across build slots) -- see
  docs/research/build-resource-governance.md. A positive value overrides the derivation outright for
  one run, e.g. at the console with no remote session to protect. They are separate knobs because they
  bound different phases: -Jobs caps compilation, -TestThreads caps how many test processes execute.

  -RunMemoryGB (run mode only): on Windows, overrides the job object's committed-memory ceiling for
  one run; 0 derives the same machine-proportional cap an ordinary build gets. On Linux, an explicit
  value is refused because the host cgroup owns the cap; configure the finite host service cgroup
  instead.

  -BuildSlotTimeoutSeconds (default 1800 = 30 minutes): long enough that a normal queued build never
  trips it, short enough that a genuinely wedged holder (crashed mid-build without releasing) is
  reported rather than blocking every other worktree's build silently forever.
#>
# PositionalBinding = $false is a CORRECTNESS gate, not style. Without it every string parameter
# below is implicitly positional, so a stray or misplaced cargo flag is silently absorbed as the
# VALUE of whichever positional slot happens to be free instead of reaching $ExtraArgs. Measured:
# `pg.ps1 -Mode test -Package pg-foma --no-capture` bound "--no-capture" to -Filter, so nextest ran
# with a filter no test name can match -- "0 tests run" while looking like a successful filtered run.
# `pg.ps1 -Mode build -Package pg-foma --example foo` bound "--example" to -Path and "foo" to -Base.
# That is the self-concealing class of failure this script exists to prevent: an argument that
# changes what runs, absorbed without a word. Unknown tokens now flow to $ExtraArgs (and on to
# cargo) or fail loudly. -Mode keeps Position = 0 so `pg.ps1 build` still works rather than blocking
# on a Mandatory-parameter prompt, which in an agent context is an unattended hang.
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory, Position = 0)]
    [ValidateSet('check', 'quick', 'build', 'test', 'corpus-test', 'conformance-test', 'release', 'doc', 'doctor', 'gc', 'run', 'new-worktree', 'remove-worktree')]
    [string]$Mode,
    # conformance-test only and MANDATORY there; no default by design (see CLAUDE.md).
    [ValidateSet('local', 'all')][string]$Scope = '',
    # new-worktree only: where to create it, which revision to base it on, and the branch name.
    [string]$Path = '',
    [string]$Base = '',
    [string]$Branch = '',
    [string]$Package = '',
    # Narrows EXECUTION only, by test NAME substring -- never file/target names. See this script's own header.
    [string]$Filter = '',
    # Narrows COMPILATION: maps to cargo's `--test <name>`. See this script's own header.
    [string]$TestTarget = '',
    # run only: exactly ONE of these three selects what to run -- see this script's own header.
    [string]$Example = '',
    [string]$Bin = '',
    [string]$Exe = '',
    # run only: overrides the job's committed-memory ceiling for one run; 0 derives the machine-proportional default.
    [int]$RunMemoryGB = 0,

    # `run` only. Without it the child's stdout goes to the inherited console, where an outer PowerShell `*>` captures NOTHING -- two long censuses lost their entire output that way. Live console output is what you give up by passing it.
    [string]$RunCaptureStdout = '',
    [switch]$DebugProfile,
    [switch]$NoNextest,
    # Stop at the first failing test. Off by default -- see the header block.
    [switch]$FailFast,
    [int]$MaxConcurrent = 2,
    # 0 = derive from the machine; see this script's own header.
    [int]$Jobs = 0,
    # 0 = same derivation as -Jobs, but for a different phase: this caps test-process execution, not compilation.
    [int]$TestThreads = 0,
    # BelowNormal by default so sshd and Chrome Remote Desktop's encoder preempt compiler work.
    [ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = 'BelowNormal',
    # 30 minutes: long enough not to trip on a normal queue, short enough to report a genuinely wedged holder.
    [int]$BuildSlotTimeoutSeconds = 1800,
    [switch]$NoSccache,
    [ValidateSet('strict', 'development', 'off')][string]$BaseMode = 'development',
    [switch]$Apply,
    # gc only: also reclaim fully-committed worktrees idle this many days; 0 (default) leaves them alone.
    [int]$StaleWorktreeDays = 0,
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$ExtraArgs
)

. "$PSScriptRoot\_common.ps1"

if (-not $IsWindows -and -not $IsLinux) {
    Write-Host '[pg] unsupported platform: this tool supports Windows and Linux only.' -ForegroundColor Red
    exit $script:ExitCodeUnsupportedPlatform
}
if ($IsLinux -and $Mode -eq 'gc') {
    Write-Host '[pg] Linux gc is unsupported: a safe native process census/reaper is not implemented.' -ForegroundColor Red
    exit $script:ExitCodeLinuxGcUnsupported
}

# FIRST thing after loading the library: every mode below resolves paths from the CWD-derived repo root.
Assert-ScriptAndCwdAgreeOnWorktree -ScriptRoot $PSScriptRoot

# Binder-proof passthrough for callers that cannot use the call operator; appended AFTER $ExtraArgs so an explicit arg still wins.
if ($env:PANGLOSS_EXTRA_ARGS) {
    $ExtraArgs = @($ExtraArgs) + @(Split-ExtraArgsSpec $env:PANGLOSS_EXTRA_ARGS)
}

# Refuse an unclaimed scope before ANY work -- no build slot, no cargo, no submodule fetch.
if ($Mode -eq 'conformance-test' -and [string]::IsNullOrWhiteSpace($Scope)) {
    Write-Host "[pg] conformance-test requires -Scope, and has no default." -ForegroundColor Red
    Write-Host "     -Scope local  conformance-staging/** only (this repo's own fixtures)"
    Write-Host "     -Scope all    those plus machine/conformance/** (every upstream fixture)"
    Write-Host "     A green conformance run has to say what it covered, so this will not guess."
    exit $script:ExitCodeConformanceScopeUnclaimed
}
# -Scope on a mode that cannot honour it would read as scoping while scoping nothing.
if ($Scope -and $Mode -ne 'conformance-test') {
    Write-Host "[pg] -Scope applies to -Mode conformance-test only; '$Mode' would ignore it." -ForegroundColor Red
    Write-Host "     -Mode test and -Mode corpus-test always cover every fixture, and say so."
    exit $script:ExitCodeConformanceScopeUnclaimed
}

# The thin-LTO profile rust/Cargo.toml declares as `[profile.pg-test-opt]`; see that file's comment for why.
$script:TestOptProfile = 'pg-test-opt'

$repoRoot = Get-RepoRoot
$rustRoot = Get-RustRoot

# Linux has no Windows job-object/procgov equivalent in this wrapper.  Establish the host's finite
# cgroup bound before any formatting or Cargo path can run, then pass the proof through to the actual
# process seam so the launch does not rediscover a mutable host state.
$linuxHostProof = $null
if ($IsLinux -and $Mode -notin @('gc', 'new-worktree', 'remove-worktree')) {
    $linuxHostProof = Get-LinuxHostCgroupPreflight
    if (-not $linuxHostProof.Ok -and $Mode -ne 'doctor') {
        Write-Host "[pg] Linux host containment preflight failed BEFORE Cargo: $($linuxHostProof.Detail)" -ForegroundColor Red
        Write-Host '[pg] recovery: run inside the configured finite cgroup-v2 service hierarchy.' -ForegroundColor Yellow
        exit $script:ExitCodeLinuxHostContainment
    }
}

if ($IsLinux -and $Mode -eq 'run' -and $RunMemoryGB -gt 0) {
    Write-Host '[pg] Linux -RunMemoryGB is not supported: the host cgroup owns the cap. Configure the finite host service cgroup instead.' -ForegroundColor Red
    exit $script:ExitCodeLinuxRunMemoryOverride
}

if ($Mode -eq 'new-worktree') {
    # The bootstrap half of the exact-base contract: without it, no worktree has metadata and Test-WorktreeBase can only report "unverified".
    if (-not $Path) { Write-Host '[pg] new-worktree requires -Path' -ForegroundColor Red; exit 2 }
    if (-not $Base) { Write-Host '[pg] new-worktree requires -Base (a revision: branch, tag, or SHA)' -ForegroundColor Red; exit 2 }

    # Resolve to a full object ID before creating anything: a branch name resolved later is exactly how a worktree ends up on the wrong commit.
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

    # Born ready: initializes the machine conformance submodule now rather than leaving every fresh worktree to fail it later.
    $conformanceResult = Initialize-ConformanceSubmodule -RepoRoot $newRoot
    if ($conformanceResult.Ok) {
        Write-Host "[pg] conformance submodule ($($conformanceResult.Mode)): $($conformanceResult.Detail)" -ForegroundColor Green
    } else {
        Write-Host "[pg] conformance submodule initialization FAILED: $($conformanceResult.Detail)" -ForegroundColor Red
        if ($conformanceResult.RecoveryCommand) {
            Write-Host "[pg] the worktree was created; run this by hand once fixed: $($conformanceResult.RecoveryCommand)" -ForegroundColor Yellow
        }
        exit $script:ExitCodeConformanceSubmoduleMissing
    }
    exit 0
}

# Before the disk/memory gates deliberately: this mode IS the disk recovery, so gating it would refuse the one command that creates some.
if ($Mode -eq 'remove-worktree') {
    if (-not $Path) { Write-Host '[pg] remove-worktree requires -Path' -ForegroundColor Red; exit 2 }
    $removal = Remove-ManagedWorktree -RepoRoot $repoRoot -Path $Path -Apply:$Apply
    if (-not $removal.Ok) {
        Write-Host "[pg] refusing to remove this worktree ($($removal.Refusal)): $($removal.Detail)" -ForegroundColor Red
        if ($removal.Refusal -eq 'dirty') {
            Write-Host '[pg] committed work would survive removal because the branch outlives the worktree; uncommitted work would not, so this one is yours to decide.' -ForegroundColor Yellow
            foreach ($p in ($removal.DirtyPaths | Select-Object -First 10)) { Write-Host "  $p" -ForegroundColor Yellow }
            if ($removal.DirtyPaths.Count -gt 10) { Write-Host "  ... and $($removal.DirtyPaths.Count - 10) more" -ForegroundColor Yellow }
        }
        exit 2
    }
    if (-not $Apply) {
        Write-Host "[pg] $($removal.Detail)" -ForegroundColor Cyan
        foreach ($t in $removal.Targets) { Write-Host "  would free $($t.SizeGB)GB  $($t.Path)" -ForegroundColor Cyan }
        Write-Host '[pg] dry run -- pass -Apply to actually remove it.' -ForegroundColor DarkGray
        exit 0
    }
    Write-Host "[pg] removed worktree $($removal.Path) (branch $($removal.Branch) is untouched and still checkoutable)" -ForegroundColor Green
    foreach ($t in $removal.TargetsRemoved) { Write-Host "  freed $($t.SizeGB)GB  $($t.Path)" -ForegroundColor Green }
    if ($removal.TargetsRemoved.Count -lt $removal.Targets.Count) {
        Write-Host "[pg] $($removal.Targets.Count - $removal.TargetsRemoved.Count) target dir(s) were left in place: $($removal.TargetSkipReason)" -ForegroundColor Yellow
    }
    if (-not $removal.Pruned) { Write-Host '[pg] `git worktree prune` did not report success -- run it by hand.' -ForegroundColor Yellow }
    exit 0
}

if ($Mode -eq 'run') {
    # Fail fast on usage errors before the expensive preflight machinery below runs; the -Exe existence check stays here since it touches the filesystem.
    $runTargetCheck = Resolve-RunTarget -Example $Example -Bin $Bin -Exe $Exe
    if (-not $runTargetCheck.Ok) {
        Write-Host "[pg] $($runTargetCheck.Detail)" -ForegroundColor Red
        exit 2
    }
    if ($Exe -and -not (Test-Path -LiteralPath $Exe -PathType Leaf)) {
        Write-Host "[pg] -Mode run: -Exe path not found: $Exe" -ForegroundColor Red
        exit 2
    }
}

if ($NoSccache) {
    # Explicit and noisy: forcing MaxConcurrent to 1 stops no-cache from also doubling uncached CPU/disk contention.
    Write-Host '[pg] NO-CACHE EMERGENCY MODE (-NoSccache): this build will not share compiled artifacts with any other worktree. Forcing MaxConcurrent=1.' -ForegroundColor Magenta
    $MaxConcurrent = 1
}

# Computed AFTER -NoSccache (which can lower MaxConcurrent) since the job budget is per-slot, and narrowed by
# available memory as well as cores. docs/research/build-resource-governance.md
$availableMemGB = Get-AvailableMemoryGB
$memCheck = Test-MemoryReserve -AvailableGB $availableMemGB

# `run -Exe` compiles nothing, so it must not be counted in this COMPILE-time job memory estimate -- see Get-PerJobMemoryGB.
$fatLto = ($Mode -eq 'release') -or (($Mode -eq 'build') -and (-not $DebugProfile)) -or (($Mode -eq 'run') -and (-not $Exe) -and (-not $DebugProfile))
$perJobMemGB = Get-PerJobMemoryGB -FatLto:$fatLto

$jobsExplicit = ($Jobs -gt 0)
$jobsBudget = Resolve-ConcurrencyBudget -CpuBudget (Get-CargoJobBudget -MaxConcurrent $MaxConcurrent) `
    -MemoryBudget (Get-MemoryProcessBudget -AvailableGB $availableMemGB -PerProcessGB $perJobMemGB -MaxConcurrent $MaxConcurrent) `
    -Explicit:$jobsExplicit
if (-not $jobsExplicit) { $Jobs = $jobsBudget.Value }
$env:CARGO_BUILD_JOBS = "$Jobs"

# The EXECUTION half: CARGO_BUILD_JOBS bounds compilation only, and nextest/libtest fan out test processes
# at their own uncapped default. Sized against a heavier per-process allowance. docs/research/build-resource-governance.md
$testThreadsExplicit = ($TestThreads -gt 0)
$testThreadsBudget = Resolve-ConcurrencyBudget -CpuBudget (Get-CargoJobBudget -MaxConcurrent $MaxConcurrent) `
    -MemoryBudget (Get-MemoryProcessBudget -AvailableGB $availableMemGB -PerProcessGB $script:MemoryPerTestProcessGB -MaxConcurrent $MaxConcurrent) `
    -Explicit:$testThreadsExplicit
if (-not $testThreadsExplicit) { $TestThreads = $testThreadsBudget.Value }

$targetDir = Resolve-TargetDir -RustRoot $rustRoot
if ($targetDir) { $env:CARGO_TARGET_DIR = $targetDir }

$repoId = Get-RepoIdentity -RepoRoot $repoRoot

function Test-BackendCardRegenerationScope {
    param(
        [string]$BuildMode,
        [string]$BuildPackage
    )
    return ($BuildMode -in @('build', 'release')) -and
        ([string]::IsNullOrWhiteSpace($BuildPackage) -or $BuildPackage -eq 'pg-foma')
}

function Invoke-BackendCardRegeneration {
    param(
        [string]$RustRoot,
        [bool]$ReleaseBuild,
        [ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$BuildPriority,
        [int]$BuildMaxConcurrent,
        $HostCgroupProof = $null
    )
    $generatorArgs = @('run', '-p', 'pg-foma', '--example', 'regenerate_backend_cards')
    if ($ReleaseBuild) { $generatorArgs += '--release' }
    Write-Host "[pg] regenerating backend capability cards ($($generatorArgs -join ' '))" -ForegroundColor Cyan
    $invokeArgs = @{
        Exe = 'cargo'; CmdArgs = $generatorArgs; WorkingDirectory = $RustRoot
        Priority = $BuildPriority; JobMaxConcurrent = $BuildMaxConcurrent
    }
    if ($null -ne $HostCgroupProof) { $invokeArgs['HostCgroupProof'] = $HostCgroupProof }
    $generatorCode = Invoke-CargoWithReaper @invokeArgs
    if ($generatorCode -ne 0) {
        Write-Host "[pg] backend capability card regeneration failed with exit code $generatorCode" -ForegroundColor Red
        return $generatorCode
    }
    Write-Host 'backend capability cards regenerated' -ForegroundColor Green
    return 0
}

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

# Must come after the health check (starts the daemon) and before cargo runs (priority is inherited at spawn).
# Called unconditionally, even for 'Normal' -- see docs/research/build-resource-governance.md.
if ($usedSccache) {
    $null = Set-SccacheServerPriority -Priority $Priority
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

# Not computed for build/release/gc/run: those modes never reach conformance_fixtures_gate, so even a fast-path Test-Path is an avoidable tax.
$conformanceCheck = $null
if ($Mode -eq 'test' -or $Mode -eq 'corpus-test' -or $Mode -eq 'doctor' -or ($Mode -eq 'conformance-test' -and $Scope -ne 'local')) {
    # -Scope local reads conformance-staging/** only, so it does not need the submodule at all.
    $conformanceCheck = Initialize-ConformanceSubmodule -RepoRoot $repoRoot
}

function Invoke-CommentHygieneReport {
    <#
      .DESCRIPTION
      Reported on EVERY managed build, not only in doctor -- doctor is the mode nobody runs before an
      ordinary build, so a comment regression there could survive indefinitely.

      Deliberately NOT folded into the unsafe/exit-code decision: a documentation finding that blocks
      every managed build is the gate shape this repo has already watched get switched off and then
      protect nothing. Loud, never fatal.

      Prints its own timing (costs a few seconds per invocation against several hundred files): if
      that cost ever becomes the reason someone reaches for bare cargo, it has outgrown the benefit
      and should move to changed-files-only rather than be quietly dropped.
    #>
    param([Parameter(Mandatory)][string]$ToolRoot)
    $hygiene = Join-Path $ToolRoot 'comment-hygiene.ps1'
    if (-not (Test-Path $hygiene)) { return }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $hygieneOut = & pwsh -NoProfile -File $hygiene 2>&1
    $sw.Stop()
    $secs = [math]::Round($sw.Elapsed.TotalSeconds, 1)
    if ($LASTEXITCODE -eq 0) {
        Write-Host "[pg] comment hygiene: clean (${secs}s)." -ForegroundColor Green
    } else {
        # Warning here, fatal in CI: blocking every local build on documentation is how a gate gets switched off.
        Write-Host "[pg] comment hygiene: violations present -- warning here, fatal in CI (${secs}s)." -ForegroundColor Yellow
        $hygieneOut | ForEach-Object { Write-Host "  $_" -ForegroundColor Yellow }
    }
}

function Invoke-RustFmt {
    <#
      .DESCRIPTION
      Formatting is APPLIED, not merely checked, before any mode that is about to compile: it is the
      one cleanup that is provably semantics-preserving, and removes a whole class of diff churn.
      Safe specifically because agents in this repo are instructed not to build -- the compile modes
      are the coordinator's path, so this never rewrites a file out from under an agent mid-edit. If
      that ever changes, this must become a check instead.

      No rustfmt.toml on purpose: stock defaults already match this repo's conventions, and a config
      file is an invitation to relitigate settings nothing depends on. `wrap_comments` stays off, so
      this never rewrites comment TEXT -- that is the comment checker's job, and the two must not
      fight over the same lines.
    #>
    param([Parameter(Mandatory)][string]$RustRoot)
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { return }
    $before = & cargo fmt --all --manifest-path (Join-Path $RustRoot 'Cargo.toml') -- --check 2>&1
    $hunks = @($before | Where-Object { $_ -match '^Diff in ' }).Count
    if ($hunks -eq 0) { Write-Host '[pg] rustfmt: already formatted.' -ForegroundColor Green; return }
    & cargo fmt --all --manifest-path (Join-Path $RustRoot 'Cargo.toml') 2>&1 | Out-Null
    if ($LASTEXITCODE -eq 0) {
        Write-Host "[pg] rustfmt: applied ($hunks hunk(s) reformatted -- they are in your working tree now)." -ForegroundColor Yellow
    } else {
        # A parse error is the usual cause and will fail the build a moment later with a better message.
        Write-Host '[pg] rustfmt: could not run (syntax error?) -- continuing to the build for a real diagnostic.' -ForegroundColor Yellow
    }
}

$profileLabel = switch ($Mode) {
    'release' { 'release (fat LTO)' }
    'check' { if ($DebugProfile) { 'dev (check; no codegen, no linking)' } else { "$($script:TestOptProfile) (check; no codegen, no linking)" } }
    'quick' { if ($DebugProfile) { 'dev' } else { $script:TestOptProfile } }
    'test' { if ($DebugProfile) { 'dev' } else { $script:TestOptProfile } }
    'corpus-test' { if ($DebugProfile) { 'dev' } else { $script:TestOptProfile } }
    'conformance-test' { if ($DebugProfile) { 'dev' } else { $script:TestOptProfile } }
    'doc' { 'dev (rustdoc; no codegen)' }
    'doctor' { '<none -- doctor runs no cargo command>' }
    'gc' { '<none -- gc runs no cargo command>' }
    'run' {
        if ($Exe) { '<none -- running an already-built exe directly>' }
        elseif ($DebugProfile) { 'dev' }
        else { 'release (fat LTO)' }
    }
    default { if ($DebugProfile) { 'dev' } else { 'release (fat LTO)' } }
}

Write-Preflight -Mode $Mode -Profile $profileLabel -RepoRoot $repoRoot -TargetDir $targetDir `
    -BaseCheck $baseCheck -SccacheHealth $sccacheHealth -FreeGB $freeGB -DiskCheck $diskCheck `
    -MemoryCheck $memCheck `
    -CorpusState $corpusState -ConformanceCheck $conformanceCheck `
    -MaxConcurrent $MaxConcurrent -Jobs $Jobs -JobsExplicit:$jobsExplicit `
    -JobsBudget $jobsBudget -PerJobMemoryGB $perJobMemGB `
    -TestThreads $(if ($Mode -in @('quick', 'test', 'corpus-test', 'conformance-test')) { $TestThreads } else { 0 }) `
    -TestThreadsBudget $testThreadsBudget -Priority $Priority -HostCgroupProof $linuxHostProof

if ($BaseMode -ne 'off' -and $baseCheck.Checked -and -not $baseCheck.Ok) {
    Write-Host "[pg] worktree base check FAILED ($BaseMode mode): $($baseCheck.Detail)" -ForegroundColor Red
    Write-Host '[pg] recovery: this worktree is not at the commit it was created from. Re-create the worktree at the intended base rather than rebasing/checking out automatically -- this tool never does that for you (it can discard context or invalidate a build cache you were relying on).' -ForegroundColor Yellow
    exit $script:ExitCodeWrongBase
}

if (-not $diskCheck.Ok) {
    Write-Host "[pg] $($diskCheck.Detail)" -ForegroundColor Red
    $stale = @(Get-StaleWorktreeCandidates -RepoRoot $repoRoot)
    if ($stale.Count -gt 0) {
        Write-Host "[pg] $($stale.Count) worktree(s) look reclaimable -- fully committed, and idle 3+ days:" -ForegroundColor Yellow
        foreach ($w in ($stale | Select-Object -First 8)) {
            Write-Host "  $($w.IdleDays)d idle  $($w.Name)  [$($w.Branch)]" -ForegroundColor Yellow
        }
        if ($stale.Count -gt 8) { Write-Host "  ... and $($stale.Count - 8) more" -ForegroundColor Yellow }
        Write-Host '[pg] remove one with: pg.ps1 -Mode remove-worktree -Path <path> -- it also reclaims that worktree''s target dirs, which hold far more than the checkout does.' -ForegroundColor Yellow
        Write-Host '[pg] NOT `git worktree remove`: it refuses outright on a worktree holding a submodule, and new-worktree initializes `machine` in every one it creates.' -ForegroundColor Yellow
        Write-Host '[pg] the branch outlives the worktree either way, so committed work stays recoverable by checkout.' -ForegroundColor Yellow
        Write-Host '[pg] a worktree holding ANY uncommitted or untracked file is never listed above; decide those by hand.' -ForegroundColor Yellow
    }
    Write-Host '[pg] also: pg.ps1 -Mode gc -Apply reclaims stale managed target directories this repository owns.' -ForegroundColor Yellow
    Write-Host '[pg] sibling repositories keep their own worktrees; this list covers only this one.' -ForegroundColor DarkGray
    exit $script:ExitCodeLowDisk
}

# The spawn gate. gc and doctor are exempt: neither spawns a build, gc IS the recovery, and doctor folds this check into its own exit code below.
if (-not $memCheck.Ok -and $Mode -notin @('gc', 'doctor')) {
    Write-Host "[pg] $($memCheck.Detail)" -ForegroundColor Red
    $top = @(Get-TopMemoryConsumers -Top 5)
    if ($top.Count -gt 0) {
        Write-Host '[pg] largest working sets right now:' -ForegroundColor Yellow
        foreach ($p in $top) { Write-Host "  $($p.WorkingSetGB)GB  $($p.ProcessName) (pid $($p.Id))" -ForegroundColor Yellow }
    }
    Write-Host '[pg] recovery: wait for whatever is holding memory to finish, or reap orphans with pg.ps1 -Mode gc -Apply (it kills only dead-parent processes and never a live build). Do NOT kill a large rustc/cargo blindly -- it may belong to another worktree that is building normally.' -ForegroundColor Yellow
    Write-Host "[pg] override for one run with PANGLOSS_MIN_FREE_MEM_GB=<gb> if you know the reading is stale, but understand that this gate exists because the machine was taken to zero memory twice." -ForegroundColor DarkGray
    exit $script:ExitCodeLowMemory
}

# Not in doctor: it reports this in its own findings section further up, so an unguarded call would scan twice there.
if ($Mode -notin @('doctor', 'check', 'quick')) { Invoke-CommentHygieneReport -ToolRoot $PSScriptRoot }

# Only for modes that actually compile: `gc`/`run` must not rewrite source, and `doctor` is read-only.
if ($Mode -in @('check', 'quick', 'build', 'test', 'corpus-test', 'conformance-test', 'release', 'doc')) { Invoke-RustFmt -RustRoot $rustRoot }

if ($Mode -eq 'corpus-test' -and -not $corpusState.Ok) {
    Write-Host '[pg] corpus-test refused BEFORE starting cargo -- required corpus file(s) missing:' -ForegroundColor Red
    foreach ($m in $corpusState.Missing) { Write-Host "  $m" -ForegroundColor Red }
    Write-Host "[pg] recovery: populate $($corpusState.CorpusRoot), or point PANGLOSS_CORPUS_ROOT at a populated corpus root." -ForegroundColor Yellow
    exit $script:ExitCodeMissingCorpus
}

# Same fail-closed shape as the corpus-missing gate above: conformance_fixtures_gate is part of the ordinary suite.
if (($Mode -in @('test', 'corpus-test', 'conformance-test')) -and $conformanceCheck -and -not $conformanceCheck.Ok) {
    Write-Host "[pg] conformance submodule unavailable BEFORE starting cargo: $($conformanceCheck.Detail)" -ForegroundColor Red
    if ($conformanceCheck.RecoveryCommand) {
        Write-Host "[pg] recovery: $($conformanceCheck.RecoveryCommand)  (or: pwsh -File rust\tools\conformance.ps1)" -ForegroundColor Yellow
    }
    exit $script:ExitCodeConformanceSubmoduleMissing
}

if ($Mode -eq 'doctor') {
    # Conformance IS folded into $unsafe: it describes the environment RIGHT NOW, same as disk/memory/base/sccache.
    # docs/research/build-resource-governance.md
    $linuxContainmentUnsafe = $IsLinux -and ($null -eq $linuxHostProof -or -not $linuxHostProof.Ok)
    $unsafe = ($baseCheck.Checked -and -not $baseCheck.Ok) -or (-not $diskCheck.Ok) -or (-not $memCheck.Ok) -or ($usedSccache -and -not $sccacheHealth.Ok) -or ($conformanceCheck -and -not $conformanceCheck.Ok) -or $linuxContainmentUnsafe

    # Exhaustion HISTORY is deliberately NOT folded into $unsafe: it describes something already recovered from.
    # docs/research/build-resource-governance.md
    $exhaustion = Get-ResourceExhaustionEvents -Since ((Get-Date).AddDays(-7))
    if (-not $exhaustion.Queryable) {
        Write-Host "[pg] resource-exhaustion history: $($exhaustion.Detail)" -ForegroundColor DarkGray
    } elseif ($exhaustion.Events.Count -eq 0) {
        Write-Host '[pg] resource-exhaustion history: none in the last 7 days.' -ForegroundColor Green
    } else {
        Write-Host "[pg] resource-exhaustion history: $($exhaustion.Events.Count) event(s) in the last 7 days -- THIS MACHINE HIT ITS COMMIT LIMIT RECENTLY." -ForegroundColor Red
        $latest = $exhaustion.Events | Sort-Object TimeCreated -Descending | Select-Object -First 1
        Write-Host "  most recent: $($latest.TimeCreated)" -ForegroundColor Red
        if ($latest.Consumers.Count -gt 0) {
            foreach ($c in $latest.Consumers) { Write-Host "    $($c.ProcessName) (pid $($c.Pid)): $($c.GB)GB" -ForegroundColor Red }
        } else {
            Write-Host "    (could not parse consumer names from the event message -- raw text: $($latest.RawMessage))" -ForegroundColor Red
        }
        Write-Host "  if this keeps happening: wrap the offending binary with 'pg.ps1 -Mode run' instead of invoking it directly -- that puts it under the same kernel-enforced --maxjobmem ceiling a managed build already gets." -ForegroundColor Yellow
    }

    Invoke-CommentHygieneReport -ToolRoot $PSScriptRoot

    if ($unsafe) {
        Write-Host '[pg] doctor: environment is UNSAFE for a managed build (see failures above).' -ForegroundColor Red
        exit 1
    }
    Write-Host '[pg] doctor: environment looks safe for a managed build.' -ForegroundColor Green
    exit 0
}

if ($Mode -eq 'gc') {
    # Reap dead-parent orphans first, regardless of -Apply: an orphaned rustc/link holding file locks would fail a real deletion below.
    $procSnapshot = Get-ProcessSnapshot
    Remove-OrphanedCargoProcesses -WhatIfOnly:(-not $Apply) -Snapshot $procSnapshot
    # Separate sweep: reaping a compiler can destroy work another worktree awaits; reaping a scanner cannot.
    Remove-OrphanedScanProcesses -WhatIfOnly:(-not $Apply) -Snapshot $procSnapshot
    # A live-but-stuck build-slot holder (see Test-BuildSlotHolderStale) blocks every other worktree's builds until reaped.
    Remove-StaleBuildSlotHolders -WhatIfOnly:(-not $Apply)

    # Worktrees first: a target dir stays `live` while its worktree is registered, so the reverse order would report the dirs this frees as untouchable.
    if ($StaleWorktreeDays -gt 0) {
        $stale = @(Get-StaleWorktreeCandidates -RepoRoot $repoRoot -IdleDays $StaleWorktreeDays)
        Write-Host "[gc] $($stale.Count) worktree(s) fully committed and idle $StaleWorktreeDays+ days" -ForegroundColor Cyan
        foreach ($w in $stale) {
            $r = Remove-ManagedWorktree -RepoRoot $repoRoot -Path $w.Path -Apply:$Apply -RepositoryId $repoId -BusyProcesses @(Get-LiveBuildProcesses)
            if (-not $r.Ok) {
                Write-Host "[gc] skipped $($w.Name) ($($r.Refusal)): $($r.Detail)" -ForegroundColor Yellow
                continue
            }
            $verb = if ($Apply) { 'removed' } else { 'would remove' }
            Write-Host "[gc] $verb worktree $($w.Name) [$($w.Branch)], $($w.IdleDays)d idle -- $($r.TargetsFreedGB)GB of target dirs" -ForegroundColor Yellow
        }
    }

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

# `run` builds its own, much simpler launch command below rather than falling through this cargo-invocation-shaping block.
if ($Mode -ne 'run') {

# build / test / corpus-test / release all run cargo from here.
$useNextest = ($Mode -in @('quick', 'test', 'corpus-test', 'conformance-test')) -and (-not $NoNextest) -and (Get-Command cargo-nextest -ErrorAction SilentlyContinue)

$cargoArgs = @()
switch ($Mode) {
    'check' {
        # --all-targets reaches test and example code; check stops before codegen and linking.
        $cargoArgs += @('check', '--all-targets')
        if (-not $DebugProfile) { $cargoArgs += @('--profile', $script:TestOptProfile) }
    }
    'quick' {
        # Unit tests only; the integration targets are the link cost and `test` runs them.
        if ($useNextest) {
            $cargoArgs += @('nextest', 'run', '--lib', '--bins', '--test-threads', "$TestThreads")
            if (-not $DebugProfile) { $cargoArgs += @('--cargo-profile', $script:TestOptProfile) }
        } else {
            $cargoArgs += @('test', '--lib', '--bins')
            if (-not $DebugProfile) { $cargoArgs += @('--profile', $script:TestOptProfile) }
        }
    }
    'build' {
        $cargoArgs += 'build'
        if (-not $DebugProfile) { $cargoArgs += '--release' }
    }
    'release' {
        $cargoArgs += @('build', '--release')
    }
    'doc' {
        # The only thing here running rustdoc; see this repo's own CLAUDE.md for why each flag below is required.
        $cargoArgs += @('doc', '--no-deps', '--document-private-items', '--keep-going')
    }
    'test' {
        if ($useNextest) {
            # nextest's own flag goes BEFORE `--`; libtest's identically-named one goes after -- see $trailing below.
            $cargoArgs += @('nextest', 'run', '--test-threads', "$TestThreads")
            if (-not $DebugProfile) { $cargoArgs += @('--cargo-profile', $script:TestOptProfile) }
        } else {
            $cargoArgs += 'test'
            if (-not $DebugProfile) { $cargoArgs += @('--profile', $script:TestOptProfile) }
        }
    }
    'conformance-test' {
        # Same runner as 'test'; the MANDATORY -Scope claim is what makes it a separate mode.
        if ($useNextest) {
            $cargoArgs += @('nextest', 'run', '--test-threads', "$TestThreads")
            if (-not $DebugProfile) { $cargoArgs += @('--cargo-profile', $script:TestOptProfile) }
        } else {
            $cargoArgs += 'test'
            if (-not $DebugProfile) { $cargoArgs += @('--profile', $script:TestOptProfile) }
        }
    }
    'corpus-test' {
        # MUST run ignored tests: every corpus-backed suite is #[ignore]d precisely because it needs the private corpus.
        if ($useNextest) {
            $cargoArgs += @('nextest', 'run', '--run-ignored', 'all', '--test-threads', "$TestThreads")
            if (-not $DebugProfile) { $cargoArgs += @('--cargo-profile', $script:TestOptProfile) }
        } else {
            $cargoArgs += 'test'
            if (-not $DebugProfile) { $cargoArgs += @('--profile', $script:TestOptProfile) }
        }
    }
}
if ($Package) { $cargoArgs += @('-p', $Package) } else { $cargoArgs += '--workspace' }

# Before the runner-specific branches: `--test` is a CARGO argument, valid for both runners, and must not land after `--`.
if ($TestTarget) { $cargoArgs += @('--test', $TestTarget) }

if ($useNextest) {
    # Skipped when the caller already passed it; nextest refuses a repeated flag.
    if ((-not $FailFast) -and ($ExtraArgs -notcontains '--no-fail-fast')) { $cargoArgs += '--no-fail-fast' }
    if ($Filter) { $cargoArgs += $Filter }
    # Without this, PANGLOSS_CORPUS_CASES lines from PASSING tests are swallowed and misreport as zero cases.
    if (($Mode -eq 'corpus-test') -and ($ExtraArgs -notcontains '--no-capture')) { $cargoArgs += '--no-capture' }
} else {
    $trailing = @()
    if ($Filter) { $trailing += $Filter }
    if ($Mode -in @('quick', 'test', 'corpus-test', 'conformance-test')) { $trailing += @('--test-threads', "$TestThreads") }
    if ($Mode -eq 'corpus-test') {
        $trailing += '--nocapture'
        # libtest's spelling of nextest's --run-ignored all: without it, every corpus test (all #[ignore]d) is skipped.
        $trailing += '--include-ignored'
    }
    if ($trailing.Count -gt 0) { $cargoArgs += @('--') + $trailing }
}
if ($ExtraArgs) { $cargoArgs += $ExtraArgs }

} # end: if ($Mode -ne 'run')

if ($Mode -eq 'corpus-test') { $env:PANGLOSS_CORPUS_REQUIRED = '1' }

# 'test'/'corpus-test' claim 'all' HERE, not as a library default, so the claim is visible and printed.
if ($Mode -in @('quick', 'test', 'corpus-test', 'conformance-test')) {
    $claimedScope = if ($Mode -eq 'conformance-test') { $Scope } else { 'all' }
    $env:PANGLOSS_CONFORMANCE_SCOPE = $claimedScope
    Write-Host "[pg] conformance scope: $claimedScope" -ForegroundColor DarkCyan
}

$runPlan = $null
if ($Mode -eq 'run') {
    # Resolves -Exe to an absolute path against the CALLER's cwd now, before WorkingDirectory switches to $rustRoot below.
    $exeResolved = if ($Exe) { (Resolve-Path -LiteralPath $Exe).ProviderPath } else { '' }
    $runPlan = Resolve-RunTarget -Example $Example -Bin $Bin -Exe $exeResolved -Package $Package -DebugProfile:$DebugProfile -ExtraArgs $ExtraArgs
    if (-not $runPlan.Ok) {
        # Unreachable in practice, but Resolve-RunTarget is general-purpose and must not assume its caller already validated.
        Write-Host "[pg] $($runPlan.Detail)" -ForegroundColor Red
        exit 2
    }
}

# DELIBERATE CHOICE, weighed both ways, not an oversight: `run` takes a build slot too, so the machine-wide
# headroom bound stays true by construction. docs/research/build-resource-governance.md
$sem = Enter-BuildSlot -MaxConcurrent $MaxConcurrent -TimeoutSeconds $BuildSlotTimeoutSeconds
if (-not $sem) {
    Write-Host "[pg] timed out after ${BuildSlotTimeoutSeconds}s waiting for a build slot (max $MaxConcurrent concurrent across all worktrees) -- another worktree's build (or a long-running 'pg.ps1 -Mode run') is holding it." -ForegroundColor Red
    exit $script:ExitCodeBuildSlotTimeout
}

# Re-checks memory after the slot wait (up to 30 min): a courtesy message, not what bounds the machine -- the job object does.
$memCheckNow = Test-MemoryReserve -AvailableGB (Get-AvailableMemoryGB)
if (-not $memCheckNow.Ok) {
    Exit-BuildSlot -Semaphore $sem
    Write-Host "[pg] memory dropped below the reserve while waiting for a build slot: $($memCheckNow.Detail)" -ForegroundColor Red
    Write-Host '[pg] nothing was started. Re-run when the build that was ahead of this one has finished.' -ForegroundColor Yellow
    exit $script:ExitCodeLowMemory
}

$code = 1
try {
    if ($Mode -eq 'run') {
        # Same derivation an ordinary build gets by default; Linux rejects an explicit -RunMemoryGB above.
        $runMemGB = if ($RunMemoryGB -gt 0) { $RunMemoryGB } else { Get-JobMemoryCapGB -MaxConcurrent $MaxConcurrent }
        Write-Host "[pg] run ($($runPlan.Label)): $($runPlan.LaunchExe) $($runPlan.LaunchArgs -join ' ')  (target-dir: $(if ($targetDir) { $targetDir } else { '<default>' }))" -ForegroundColor Cyan
        $invokeArgs = @{
            Exe = $runPlan.LaunchExe; CmdArgs = $runPlan.LaunchArgs; WorkingDirectory = $rustRoot
            Priority = $Priority; Subject = 'run'
        }
        if ($RunCaptureStdout) {
            $invokeArgs['CaptureStdoutPath'] = $RunCaptureStdout
            Write-Host "[pg] run: stdout -> $RunCaptureStdout (console stays quiet until it exits; tail the file to watch it)" -ForegroundColor Cyan
        }
        if (-not $IsLinux) {
            $invokeArgs['JobMemoryGB'] = $runMemGB
            $invokeArgs['CpuRatePercent'] = Get-JobCpuRatePercent
        }
        if ($null -ne $linuxHostProof) { $invokeArgs['HostCgroupProof'] = $linuxHostProof }
        $code = Invoke-ProcessInJobObject @invokeArgs
        if ($RunCaptureStdout -and (Test-Path $RunCaptureStdout)) {
            Write-Host "[pg] run: captured $((Get-Item $RunCaptureStdout).Length) byte(s) to $RunCaptureStdout" -ForegroundColor Cyan
        }
    } elseif ($Mode -eq 'corpus-test') {
        $runnerLabel = if ($useNextest) { 'nextest' } elseif ($Mode -eq 'check') { 'cargo check' } elseif ($Mode -eq 'build' -or $Mode -eq 'release') { 'cargo build' } elseif ($Mode -eq 'doc') { 'rustdoc' } else { 'cargo test' }
        Write-Host "[pg] cargo $($cargoArgs -join ' ')  (target-dir: $(if ($targetDir) { $targetDir } else { '<default>' }), runner: $runnerLabel)" -ForegroundColor Cyan
        $capturePath = Join-Path ([System.IO.Path]::GetTempPath()) "pg-corpus-test-$PID.log"
        $invokeArgs = @{
            Exe = 'cargo'; CmdArgs = $cargoArgs; WorkingDirectory = $rustRoot; CaptureStdoutPath = $capturePath
            Priority = $Priority; JobMaxConcurrent = $MaxConcurrent
        }
        if ($null -ne $linuxHostProof) { $invokeArgs['HostCgroupProof'] = $linuxHostProof }
        $code = Invoke-CargoWithReaper @invokeArgs
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
        $runnerLabel = if ($useNextest) { 'nextest' } elseif ($Mode -eq 'check') { 'cargo check' } elseif ($Mode -eq 'build' -or $Mode -eq 'release') { 'cargo build' } elseif ($Mode -eq 'doc') { 'rustdoc' } else { 'cargo test' }
        Write-Host "[pg] cargo $($cargoArgs -join ' ')  (target-dir: $(if ($targetDir) { $targetDir } else { '<default>' }), runner: $runnerLabel)" -ForegroundColor Cyan
        $invokeArgs = @{
            Exe = 'cargo'; CmdArgs = $cargoArgs; WorkingDirectory = $rustRoot
            Priority = $Priority; JobMaxConcurrent = $MaxConcurrent
        }
        if ($null -ne $linuxHostProof) { $invokeArgs['HostCgroupProof'] = $linuxHostProof }
        $code = Invoke-CargoWithReaper @invokeArgs
        if ($code -eq 0 -and (Test-BackendCardRegenerationScope -BuildMode $Mode -BuildPackage $Package)) {
            $releaseBuild = ($Mode -eq 'release') -or (($Mode -eq 'build') -and (-not $DebugProfile))
            $code = Invoke-BackendCardRegeneration -RustRoot $rustRoot -ReleaseBuild:$releaseBuild `
                -BuildPriority $Priority -BuildMaxConcurrent $MaxConcurrent -HostCgroupProof $linuxHostProof
        }
    }
} finally {
    Exit-BuildSlot -Semaphore $sem
    # Post-run disk check: preflight runs BEFORE cargo and cannot see space consumed during the build itself.
    $freeAfter = if ($targetDir) { Get-FreeSpaceGB $targetDir } else { $null }
    if ($null -ne $freeAfter -and $freeAfter -lt 15) {
        Write-Host "[pg] WARNING: only ${freeAfter}GB free on the target drive after this run." -ForegroundColor Red
        Write-Host '[pg] Recover with: pg.ps1 -Mode gc (dry run, then -Apply). It only removes target dirs this repository owns and never touches an unmarked, preserved, or still-live one.' -ForegroundColor Yellow
        Write-Host '[pg] If that frees little, the space is likely a LOCAL rust/target from a bare-cargo run, which sits on the system drive because it bypassed target-dir redirection.' -ForegroundColor Yellow
    }
}

# nextest exits 4 for "no tests to run" but doesn't say WHY; the most common cause is a test TARGET name passed to -Filter.
if ($code -eq 4 -and $Filter -and -not $TestTarget) {
    $targets = @()
    try {
        $pkgGlob = if ($Package) { $Package } else { '*' }
        $testsDir = Join-Path $rustRoot (Join-Path 'crates' (Join-Path $pkgGlob 'tests'))
        $targets = @(Get-ChildItem -Path $testsDir -Filter '*.rs' -File -ErrorAction SilentlyContinue |
            ForEach-Object { [IO.Path]::GetFileNameWithoutExtension($_.Name) })
    } catch { $targets = @() }

    foreach ($line in (Get-FilterZeroMatchHint -Filter $Filter -TestTargets $targets)) {
        Write-Host $line.Text -ForegroundColor $line.Color
    }
}

if ($Mode -eq 'release' -and $code -eq 0 -and $targetDir) {
    # A failed build never registers a release deliverable -- mark `preserved` only after cargo itself reports success.
    Write-TargetOwnership -TargetDir $targetDir -RepositoryId $repoId -WorktreePath $repoRoot -Preserved | Out-Null
}

exit $code

