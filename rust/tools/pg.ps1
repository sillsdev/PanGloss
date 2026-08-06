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
                  ignored with -Exe. `-RunMemoryGB` overrides the job's committed-memory ceiling
                  for one run (0 = derive the same machine-proportional cap a build gets); this is
                  the mechanism for a deliberate large-memory experiment (e.g. 40GB) without
                  changing the default for ordinary builds. `run` DOES take a build slot
                  (Enter-BuildSlot) -- see the `run` mode block below in this file for why that is
                  the deliberate choice, not an oversight.

  Examples:
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
    rust\tools\pg.ps1 -Mode run -Exe .\predict_census.exe -RunMemoryGB 40   # deliberate large-mem experiment

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

  -RunMemoryGB (run mode only): overrides the job object's committed-memory ceiling for one run; 0
  derives the same machine-proportional cap an ordinary build gets. This is the mechanism for a
  deliberate large-memory experiment (e.g. 40GB) without touching PANGLOSS_JOB_MEM_GB, which would
  also change every ordinary build's cap for as long as the env var stayed set.

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
    [ValidateSet('build', 'test', 'corpus-test', 'release', 'doc', 'doctor', 'gc', 'run', 'new-worktree')]
    [string]$Mode,
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
    [switch]$DebugProfile,
    [switch]$NoNextest,
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
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$ExtraArgs
)

. "$PSScriptRoot\_common.ps1"

# FIRST thing after loading the library: every mode below resolves paths from the CWD-derived repo root.
Assert-ScriptAndCwdAgreeOnWorktree -ScriptRoot $PSScriptRoot

# Binder-proof passthrough for callers that cannot use the call operator; appended AFTER $ExtraArgs so an explicit arg still wins.
if ($env:PANGLOSS_EXTRA_ARGS) {
    $ExtraArgs = @($ExtraArgs) + @(Split-ExtraArgsSpec $env:PANGLOSS_EXTRA_ARGS)
}

# The thin-LTO profile rust/Cargo.toml declares as `[profile.pg-test-opt]`; see that file's comment for why.
$script:TestOptProfile = 'pg-test-opt'

$repoRoot = Get-RepoRoot
$rustRoot = Get-RustRoot

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
if ($Mode -eq 'test' -or $Mode -eq 'corpus-test' -or $Mode -eq 'doctor') {
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
    'test' { if ($DebugProfile) { 'dev' } else { $script:TestOptProfile } }
    'corpus-test' { if ($DebugProfile) { 'dev' } else { $script:TestOptProfile } }
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
    -TestThreads $(if ($Mode -eq 'test' -or $Mode -eq 'corpus-test') { $TestThreads } else { 0 }) `
    -TestThreadsBudget $testThreadsBudget -Priority $Priority

if ($BaseMode -ne 'off' -and $baseCheck.Checked -and -not $baseCheck.Ok) {
    Write-Host "[pg] worktree base check FAILED ($BaseMode mode): $($baseCheck.Detail)" -ForegroundColor Red
    Write-Host '[pg] recovery: this worktree is not at the commit it was created from. Re-create the worktree at the intended base rather than rebasing/checking out automatically -- this tool never does that for you (it can discard context or invalidate a build cache you were relying on).' -ForegroundColor Yellow
    exit $script:ExitCodeWrongBase
}

if (-not $diskCheck.Ok) {
    Write-Host "[pg] $($diskCheck.Detail)" -ForegroundColor Red
    exit $script:ExitCodeLowDisk
}

# The spawn gate; gc/doctor are exempt by construction -- gc is the recovery action, doctor only reports.
if (-not $memCheck.Ok) {
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
if ($Mode -ne 'doctor') { Invoke-CommentHygieneReport -ToolRoot $PSScriptRoot }

# Only for modes that actually compile: `gc`/`run` must not rewrite source, and `doctor` is read-only.
if ($Mode -in @('build', 'test', 'corpus-test', 'release', 'doc')) { Invoke-RustFmt -RustRoot $rustRoot }

if ($Mode -eq 'corpus-test' -and -not $corpusState.Ok) {
    Write-Host '[pg] corpus-test refused BEFORE starting cargo -- required corpus file(s) missing:' -ForegroundColor Red
    foreach ($m in $corpusState.Missing) { Write-Host "  $m" -ForegroundColor Red }
    Write-Host "[pg] recovery: populate $($corpusState.CorpusRoot), or point PANGLOSS_CORPUS_ROOT at a populated corpus root." -ForegroundColor Yellow
    exit $script:ExitCodeMissingCorpus
}

# Same fail-closed shape as the corpus-missing gate above: conformance_fixtures_gate is part of the ordinary suite.
if (($Mode -eq 'test' -or $Mode -eq 'corpus-test') -and $conformanceCheck -and -not $conformanceCheck.Ok) {
    Write-Host "[pg] conformance submodule unavailable BEFORE starting cargo: $($conformanceCheck.Detail)" -ForegroundColor Red
    if ($conformanceCheck.RecoveryCommand) {
        Write-Host "[pg] recovery: $($conformanceCheck.RecoveryCommand)  (or: pwsh -File rust\tools\conformance.ps1)" -ForegroundColor Yellow
    }
    exit $script:ExitCodeConformanceSubmoduleMissing
}

if ($Mode -eq 'doctor') {
    # Conformance IS folded into $unsafe: it describes the environment RIGHT NOW, same as disk/memory/base/sccache.
    # docs/research/build-resource-governance.md
    $unsafe = ($baseCheck.Checked -and -not $baseCheck.Ok) -or (-not $diskCheck.Ok) -or (-not $memCheck.Ok) -or ($usedSccache -and -not $sccacheHealth.Ok) -or ($conformanceCheck -and -not $conformanceCheck.Ok)

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
    if ($Filter) { $cargoArgs += $Filter }
    # Without this, PANGLOSS_CORPUS_CASES lines from PASSING tests are swallowed and misreport as zero cases.
    if ($Mode -eq 'corpus-test') { $cargoArgs += '--no-capture' }
} else {
    $trailing = @()
    if ($Filter) { $trailing += $Filter }
    if ($Mode -eq 'test' -or $Mode -eq 'corpus-test') { $trailing += @('--test-threads', "$TestThreads") }
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
        # Same derivation an ordinary build gets by default; -RunMemoryGB bypasses it for one invocation. See this script's own header.
        $runMemGB = if ($RunMemoryGB -gt 0) { $RunMemoryGB } else { Get-JobMemoryCapGB -MaxConcurrent $MaxConcurrent }
        $runCpuRate = Get-JobCpuRatePercent
        Write-Host "[pg] run ($($runPlan.Label)): $($runPlan.LaunchExe) $($runPlan.LaunchArgs -join ' ')  (target-dir: $(if ($targetDir) { $targetDir } else { '<default>' }))" -ForegroundColor Cyan
        $code = Invoke-ProcessInJobObject -Exe $runPlan.LaunchExe -CmdArgs $runPlan.LaunchArgs -WorkingDirectory $rustRoot `
            -Priority $Priority -JobMemoryGB $runMemGB -CpuRatePercent $runCpuRate -Subject 'run'
    } elseif ($Mode -eq 'corpus-test') {
        $runnerLabel = if ($useNextest) { 'nextest' } elseif ($Mode -eq 'build' -or $Mode -eq 'release') { 'cargo build' } elseif ($Mode -eq 'doc') { 'rustdoc' } else { 'cargo test' }
        Write-Host "[pg] cargo $($cargoArgs -join ' ')  (target-dir: $(if ($targetDir) { $targetDir } else { '<default>' }), runner: $runnerLabel)" -ForegroundColor Cyan
        $capturePath = Join-Path ([System.IO.Path]::GetTempPath()) "pg-corpus-test-$PID.log"
        $code = Invoke-CargoWithReaper -Exe 'cargo' -CmdArgs $cargoArgs -WorkingDirectory $rustRoot -CaptureStdoutPath $capturePath -Priority $Priority -JobMaxConcurrent $MaxConcurrent
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
        $runnerLabel = if ($useNextest) { 'nextest' } elseif ($Mode -eq 'build' -or $Mode -eq 'release') { 'cargo build' } elseif ($Mode -eq 'doc') { 'rustdoc' } else { 'cargo test' }
        Write-Host "[pg] cargo $($cargoArgs -join ' ')  (target-dir: $(if ($targetDir) { $targetDir } else { '<default>' }), runner: $runnerLabel)" -ForegroundColor Cyan
        $code = Invoke-CargoWithReaper -Exe 'cargo' -CmdArgs $cargoArgs -WorkingDirectory $rustRoot -Priority $Priority -JobMaxConcurrent $MaxConcurrent
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
