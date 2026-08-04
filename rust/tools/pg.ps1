<#
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
    rust\tools\pg.ps1 -Mode doctor
    rust\tools\pg.ps1 -Mode gc            # dry run, reports only
    rust\tools\pg.ps1 -Mode gc -Apply     # actually deletes disposable targets
    rust\tools\pg.ps1 -Mode run -Example predict_census -- --grammar foo.xml
    rust\tools\pg.ps1 -Mode run -Bin pangloss -- batch --threads 1 --word-timeout-ms 5000
    rust\tools\pg.ps1 -Mode run -Exe C:\path\to\already-built.exe -- --some-flag
    rust\tools\pg.ps1 -Mode run -Exe .\predict_census.exe -RunMemoryGB 40   # deliberate large-mem experiment
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
    [ValidateSet('build', 'test', 'corpus-test', 'release', 'doctor', 'gc', 'run', 'new-worktree')]
    [string]$Mode,
    # new-worktree only: where to create it, which revision to base it on, and the branch name.
    [string]$Path = '',
    [string]$Base = '',
    [string]$Branch = '',
    [string]$Package = '',
    # `-Filter` narrows EXECUTION ONLY. It is appended as a bare positional to `cargo nextest run`
    # (or after `--` for libtest), where it matches TEST NAMES as a SUBSTRING -- never file names,
    # never test-target/binary names. Cargo still compiles and LINKS every test target in the
    # package regardless, which for pg-foma is ~78 separate binaries. If what you want is "run the
    # tests in <file>.rs", you want -TestTarget, not -Filter.
    #
    # This distinction is documented here because it was got wrong SEVEN times in one session (five
    # file names passed to -Filter, plus two subagents that concluded -TestTarget's underlying flag
    # was unreachable). A zero-match run fails loudly -- nextest prints "error: no tests to run" and
    # exits 4 -- but the PARTIAL match is silent: it runs some tests, exits 0, and omits the ones you
    # meant. That produced a confidently-filed bug report about a harness defect that did not exist.
    [string]$Filter = '',
    # `-TestTarget` narrows COMPILATION: it maps to cargo's `--test <name>`, building and linking ONE
    # test binary instead of every target in the package. This is the difference that matters for
    # loop speed -- measured: a warm tree running 13 tests via -TestTarget took 10.6s, while a cold
    # dependency graph for the same package took ~996s and exceeded the 595s agent tool cap. Three
    # agent batches produced ZERO measurements by paying the cold, all-targets cost repeatedly.
    #
    # Reachable before this parameter existed only as
    # `$env:PANGLOSS_EXTRA_ARGS = '--test <name>'`, which is why two agents concluded it did not
    # exist at all. Undiscoverable is the same as absent.
    [string]$TestTarget = '',
    # run only: exactly ONE of these three selects what to run -- see the header comment above and
    # the -Mode run validation block below for the full contract.
    [string]$Example = '',
    [string]$Bin = '',
    [string]$Exe = '',
    # run only: override the job object's committed-memory ceiling for one run. 0 = derive the same
    # machine-proportional cap a build gets (Get-JobMemoryCapGB -MaxConcurrent $MaxConcurrent) --
    # see that function's comment for the derivation. A positive value bypasses the derivation
    # entirely (same pattern as -Jobs/-TestThreads below), which is deliberately how a caller runs
    # the "what if this needs 40GB" experiment CLAUDE.md's background section calls for, without
    # touching PANGLOSS_JOB_MEM_GB (which would also change every ordinary build's cap for as long
    # as the env var stayed set).
    [int]$RunMemoryGB = 0,
    [switch]$DebugProfile,
    [switch]$NoNextest,
    [int]$MaxConcurrent = 2,
    # 0 = derive from the machine (Get-CargoJobBudget: logical cores, minus the interactive
    # reserve, split across the build slots). Pass a number to override for one run -- e.g. at the
    # console with no remote session to protect.
    [int]$Jobs = 0,
    # 0 = same derivation as -Jobs. Separate knob because it governs a DIFFERENT phase: -Jobs caps
    # compilation, this caps how many test processes execute at once. Capping only the first leaves
    # the run's second half uncapped.
    [int]$TestThreads = 0,
    # CPU priority for cargo and everything it spawns. BelowNormal by default so sshd and Chrome
    # Remote Desktop's encoder preempt compiler work; 'Normal' restores the old behavior.
    [ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = 'BelowNormal',
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

# FIRST thing after loading the library, before any mode dispatch, preflight, or Cargo argument is
# built: refuse if the script and the working directory disagree about which worktree this is. Placed
# here because every mode below (build/test/run/gc/doctor) resolves paths from the CWD-derived repo
# root, so a mismatch would mis-target all of them equally -- and because a refusal is only useful
# before work starts. See Assert-ScriptAndCwdAgreeOnWorktree for what this cost when it was absent.
Assert-ScriptAndCwdAgreeOnWorktree -ScriptRoot $PSScriptRoot

# Binder-proof passthrough channel. `pwsh -File pg.ps1 ... -- <cargo args>` CANNOT work: under -File
# the bare `--` reaches PowerShell's parameter binder, which rejects it as an empty parameter name.
# Worse, dropping the `--` silently misbinds any single-dash cargo argument that prefix-matches a
# parameter here (`-p foo` binds to -Package; cargo never sees it). Both verified -- see
# Split-ExtraArgsSpec for the full reproduction. Nothing inside this script can intercept either
# case, because binding fails or mis-resolves before the first line of the body runs.
#
# So callers that cannot use the call operator get an env var, which bypasses the binder entirely:
#   $env:PANGLOSS_EXTRA_ARGS = '-p pg-foma --no-capture'; pwsh -File rust\tools\pg.ps1 -Mode test
# Appended AFTER $ExtraArgs so an explicitly-typed argument still wins on cargo's own last-wins rule.
if ($env:PANGLOSS_EXTRA_ARGS) {
    $ExtraArgs = @($ExtraArgs) + @(Split-ExtraArgsSpec $env:PANGLOSS_EXTRA_ARGS)
}

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

    # Born ready: initialize the (public, non-gitignored) `machine` conformance submodule right now
    # rather than leaving every fresh worktree to fail `w91_affix_shapes_covered_by_upstream_fixtures`
    # with "machine submodule initialized?" the first time someone runs `-Mode test` here. Unlike the
    # private-corpus note above, this data is fetchable by this tool, so a failure here is something
    # to fail LOUDLY on rather than merely note -- same distinct exit code -Mode test/corpus-test use
    # for the identical failure, since the underlying problem (submodule unavailable) is the same one.
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
    # Fail fast, before any of the (comparatively expensive) preflight machinery below runs, on the
    # usage errors: exactly one of -Example/-Bin/-Exe, and (for -Exe specifically) a path that
    # actually exists. Resolve-RunTarget (_common.ps1) owns the selector/argument logic itself so
    # it stays unit-testable in rust/tools/tests/ without launching cargo or a probe binary; the
    # existence check stays here because it is the one part of this that touches the filesystem.
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
    # "Explicit, noisy, and incompatible with parallel managed builds" (design doc, error
    # handling): forcing MaxConcurrent to 1 means a caller can't accidentally combine no-cache
    # with the normal 2-way concurrency and double the uncached CPU/disk contention.
    Write-Host '[pg] NO-CACHE EMERGENCY MODE (-NoSccache): this build will not share compiled artifacts with any other worktree. Forcing MaxConcurrent=1.' -ForegroundColor Magenta
    $MaxConcurrent = 1
}

# Computed AFTER the -NoSccache block above, because that block can lower MaxConcurrent and the
# job budget is per-slot: a run that just became the only permitted build should get the whole
# budget rather than half of it.
#
# Exported as CARGO_BUILD_JOBS rather than appended as `-j`, for two reasons. It reaches cargo
# subcommands that don't take `-j` in the same position (nextest's `--cargo-profile` form), and it
# beats rust/.cargo/config.toml's static `jobs` floor in Cargo's precedence order (CLI > env >
# config) without overriding an explicit `-j` a caller put in $ExtraArgs, which still wins.
#
# Both budgets below are then narrowed by AVAILABLE MEMORY, not just by cores. Threads were capped
# and bytes were not, and this machine was taken down twice by the uncapped half: a polite 7-wide
# build whose test processes each reached several GB still drove available memory to nothing, and a
# daemon blocked on a page fault stalls a remote session just as completely as one starved of CPU
# (BelowNormal priority buys nothing there -- it is not waiting for the scheduler).
$availableMemGB = Get-AvailableMemoryGB
$memCheck = Test-MemoryReserve -AvailableGB $availableMemGB

# `build` and `release` both land on [profile.release] (lto = "fat", codegen-units = 1) unless
# -DebugProfile takes `build` off it; test/corpus-test use pg-test-opt (thin LTO) instead. `run`
# with -Example/-Bin compiles through `cargo run`, which lands on the same fat-LTO release profile
# by the same rule -- but `run -Exe` compiles nothing at all, so it must NOT be counted here (this
# budget only estimates COMPILE-time job memory; the job object's own commit ceiling, computed
# separately below in the `run` branch, is what actually bounds the running probe). The distinction
# matters more than any other input to this budget -- see Get-PerJobMemoryGB.
$fatLto = ($Mode -eq 'release') -or (($Mode -eq 'build') -and (-not $DebugProfile)) -or (($Mode -eq 'run') -and (-not $Exe) -and (-not $DebugProfile))
$perJobMemGB = Get-PerJobMemoryGB -FatLto:$fatLto

$jobsExplicit = ($Jobs -gt 0)
$jobsBudget = Resolve-ConcurrencyBudget -CpuBudget (Get-CargoJobBudget -MaxConcurrent $MaxConcurrent) `
    -MemoryBudget (Get-MemoryProcessBudget -AvailableGB $availableMemGB -PerProcessGB $perJobMemGB -MaxConcurrent $MaxConcurrent) `
    -Explicit:$jobsExplicit
if (-not $jobsExplicit) { $Jobs = $jobsBudget.Value }
$env:CARGO_BUILD_JOBS = "$Jobs"

# The EXECUTION half of the same problem. CARGO_BUILD_JOBS bounds compilation only; once cargo
# finishes building, nextest (and libtest) fan out test processes at their own default of one per
# logical core -- 20 here, with no nextest.toml in this repo to say otherwise. So a capped build
# was followed immediately by an uncapped 20-wide test run, which is the heavier half: these suites
# spawn real processes (pangloss.exe, worker_test_child.exe, a C and C++ toolchain for
# pg-ffi::header_abi), and corpus/foma cases can reach many GB of RSS each
# (CLAUDE.md, "Probe pathological grammars single-threaded"). 20 of those at once is a memory
# storm as much as a CPU one, and memory pressure is what actually freezes a remote session.
#
# Sized against a HEAVIER per-process memory allowance than compilation gets: a rustc is
# predictable, whereas a test process in this workspace can be an entire grammar compile and is not
# bounded by anything we control.
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

# Must come after the health check above (that's what starts the daemon) and before cargo runs
# (priority is inherited at spawn). Covers the rustc processes the sccache SERVER spawns, which are
# NOT cargo's children and so do not pick up the priority set in Invoke-CargoWithReaper -- see
# Set-SccacheServerPriority for the measurement that showed most of the fan-out was escaping.
#
# Deliberately NOT guarded on `$Priority -ne 'Normal'`. The sccache server is long-lived and shared
# across every build on the machine, so whatever priority one run leaves on it persists into the
# next. With a `Normal` early-out, a single `-Priority Idle` run would strand the daemon at Idle
# indefinitely and a later `-Priority Normal` -- someone explicitly asking for full speed -- would
# silently keep compiling at Idle. Setting it unconditionally makes the daemon track the priority
# actually requested, in both directions.
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

# The `machine` conformance submodule: needed by `test` (conformance_fixtures_gate is NOT
# #[ignore]d -- it runs in the ordinary suite) and `corpus-test` (same suite, plus corpus-gated
# ones), and reported (not just checked) by `doctor` since a fresh, never-tested worktree is
# exactly the state doctor exists to catch before someone burns a build finding out the hard way.
# Not computed for build/release/gc/run: those modes don't reach conformance_fixtures_gate at all,
# and a git invocation on every one of THOSE would be exactly the "tax on ordinary work" this
# design elsewhere refuses to add -- Initialize-ConformanceSubmodule's own fast path already makes
# the already-initialized case a single Test-Path, but there is no reason to pay even that outside
# the modes that need it.
$conformanceCheck = $null
if ($Mode -eq 'test' -or $Mode -eq 'corpus-test' -or $Mode -eq 'doctor') {
    $conformanceCheck = Initialize-ConformanceSubmodule -RepoRoot $repoRoot
}

$profileLabel = switch ($Mode) {
    'release' { 'release (fat LTO)' }
    'test' { if ($DebugProfile) { 'dev' } else { $script:TestOptProfile } }
    'corpus-test' { if ($DebugProfile) { 'dev' } else { $script:TestOptProfile } }
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

# The spawn gate. gc/doctor are exempt by construction: gc is the recovery action for exactly this
# state (refusing to run it when memory is low would leave no way out but a reboot), and doctor is
# reached above and reports rather than exits here.
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

if ($Mode -eq 'corpus-test' -and -not $corpusState.Ok) {
    Write-Host '[pg] corpus-test refused BEFORE starting cargo -- required corpus file(s) missing:' -ForegroundColor Red
    foreach ($m in $corpusState.Missing) { Write-Host "  $m" -ForegroundColor Red }
    Write-Host "[pg] recovery: populate $($corpusState.CorpusRoot), or point PANGLOSS_CORPUS_ROOT at a populated corpus root." -ForegroundColor Yellow
    exit $script:ExitCodeMissingCorpus
}

# ($Mode -eq 'test' -or 'corpus-test') refused BEFORE starting cargo -- the same fail-closed shape
# as the corpus-missing gate just above, for the same reason: conformance_fixtures_gate is part of
# the ordinary (non-#[ignore]d) suite, so letting cargo start without it would fail minutes later
# with a confusing panic instead of this actionable message. $conformanceCheck was already computed
# above (Initialize-ConformanceSubmodule attempted the auto-init as a side effect); this only acts
# on the result.
if (($Mode -eq 'test' -or $Mode -eq 'corpus-test') -and $conformanceCheck -and -not $conformanceCheck.Ok) {
    Write-Host "[pg] conformance submodule unavailable BEFORE starting cargo: $($conformanceCheck.Detail)" -ForegroundColor Red
    if ($conformanceCheck.RecoveryCommand) {
        Write-Host "[pg] recovery: $($conformanceCheck.RecoveryCommand)  (or: pwsh -File rust\tools\conformance.ps1)" -ForegroundColor Yellow
    }
    exit $script:ExitCodeConformanceSubmoduleMissing
}

if ($Mode -eq 'doctor') {
    # Conformance IS folded into $unsafe (unlike the exhaustion history below): a missing/failed
    # submodule describes the environment RIGHT NOW, exactly like disk/memory/base/sccache do --
    # not something that already happened and was already recovered from, which is the dividing
    # line this file draws for the exhaustion log. A doctor run that reports "safe" on a worktree
    # that would immediately fail conformance_fixtures_gate is the exact "I could not look read as
    # everything is fine" failure this repo's own rules elsewhere warn against.
    $unsafe = ($baseCheck.Checked -and -not $baseCheck.Ok) -or (-not $diskCheck.Ok) -or (-not $memCheck.Ok) -or ($usedSccache -and -not $sccacheHealth.Ok) -or ($conformanceCheck -and -not $conformanceCheck.Ok)

    # Exhaustion HISTORY, deliberately NOT folded into $unsafe above. The four checks that DO gate
    # $unsafe all describe the environment RIGHT NOW (disk/memory/base/sccache); an exhaustion event
    # describes something that already happened and the machine already recovered from on its own.
    # Failing doctor on old history would mean a single incident blocks every managed build for the
    # whole 7-day window for no actionable reason -- the actionable response to "predict_census.exe
    # hit 118GB three days ago" is "stop invoking it directly, use pg.ps1 -Mode run", not "doctor
    # exits 1 today". So this is reported prominently -- loud enough that it isn't scrollback -- but
    # never gates the exit code.
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

    # Deliberately NOT folded into $unsafe: a stale comment cannot make a build wrong, and a
    # documentation finding that blocks every managed build is the kind of gate that gets switched
    # off and then protects nothing. Reported loudly, never fatal -- as with the exhaustion history.
    #
    # The ratchet fails only on a category GROWING, so a red line means markers were added since the
    # baseline, not that a backlog still exists.
    $hygiene = Join-Path $PSScriptRoot 'comment-hygiene.ps1'
    if (Test-Path $hygiene) {
        $hygieneOut = & pwsh -NoProfile -File $hygiene 2>&1
        if ($LASTEXITCODE -eq 0) {
            Write-Host '[pg] comment hygiene: no category grew since the baseline.' -ForegroundColor Green
        } else {
            Write-Host '[pg] comment hygiene: REGRESSED -- project state is creeping back into comments.' -ForegroundColor Red
            $hygieneOut | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
            Write-Host '  rules: .claude/skills/code-comments/SKILL.md   offenders: rust\tools\comment-hygiene.ps1 -List' -ForegroundColor Yellow
        }
    }

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
    # One snapshot shared by both sweeps, so they cannot disagree about which processes were live
    # at the moment the decision was taken.
    $procSnapshot = Get-ProcessSnapshot
    Remove-OrphanedCargoProcesses -WhatIfOnly:(-not $Apply) -Snapshot $procSnapshot
    # Same dead-parent rule, applied to filesystem scanners. Kept as a separate sweep rather than
    # more names in the list above because the two have different risk profiles: reaping a compiler
    # can destroy work another worktree is waiting on, reaping an orphaned scanner cannot (its
    # output already has no reader), so only this one carries CPU/age thresholds.
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

# `run` builds its own launch command below (cargo run --example/--bin, or a bare .exe) rather than
# falling through this cargo-invocation-shaping block, which exists for build/test/corpus-test/
# release's very different needs (profile selection, nextest wiring, workspace-wide vs -p). Guarding
# the whole block on Mode rather than threading a dozen extra conditions through it keeps `run`'s
# much simpler command construction from being tangled into logic (nextest flags, --workspace) that
# does not apply to it and, for the -Exe case, involves no cargo at all.
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
    'test' {
        if ($useNextest) {
            # nextest's own flag, BEFORE `--` (it selects how many test processes run at once).
            # libtest's identically-named flag has to go after `--` instead -- see $trailing below.
            $cargoArgs += @('nextest', 'run', '--test-threads', "$TestThreads")
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
            $cargoArgs += @('nextest', 'run', '--run-ignored', 'all', '--test-threads', "$TestThreads")
            if (-not $DebugProfile) { $cargoArgs += @('--cargo-profile', $script:TestOptProfile) }
        } else {
            $cargoArgs += 'test'
            if (-not $DebugProfile) { $cargoArgs += @('--profile', $script:TestOptProfile) }
        }
    }
}
if ($Package) { $cargoArgs += @('-p', $Package) } else { $cargoArgs += '--workspace' }

# Placed BEFORE the runner-specific branches below because `--test` is a CARGO argument, not a
# nextest or libtest one: it selects which target cargo compiles, so it is valid for both runners and
# must not land after a `--` separator. Both `cargo nextest run` and `cargo test` accept it.
if ($TestTarget) { $cargoArgs += @('--test', $TestTarget) }

if ($useNextest) {
    if ($Filter) { $cargoArgs += $Filter }
    # nextest's own flag name for "print stdout even for passing tests" -- without it,
    # PANGLOSS_CORPUS_CASES lines from PASSING tests would be swallowed, and a fully successful
    # corpus run would misreport as zero cases executed.
    if ($Mode -eq 'corpus-test') { $cargoArgs += '--no-capture' }
} else {
    $trailing = @()
    if ($Filter) { $trailing += $Filter }
    # libtest's spelling of the nextest flag above. Only applies to the `test`/`corpus-test` modes;
    # `build`/`release` never reach this branch, and passing it to them would be an error.
    if ($Mode -eq 'test' -or $Mode -eq 'corpus-test') { $trailing += @('--test-threads', "$TestThreads") }
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

} # end: if ($Mode -ne 'run')

if ($Mode -eq 'corpus-test') { $env:PANGLOSS_CORPUS_REQUIRED = '1' }

$runPlan = $null
if ($Mode -eq 'run') {
    # Re-resolve (rather than caching the earlier check's result): the earlier call above only
    # existed to fail fast before spending time on preflight; this is the one whose LaunchExe/
    # LaunchArgs/Label actually get used below. Using -Exe's already-resolved absolute path here
    # (rather than the raw -Exe string) is deliberate -- WorkingDirectory for the launched process
    # is $rustRoot, not wherever the caller's shell happened to be, so a relative -Exe path must be
    # resolved against the CALLER's cwd now, before that context is gone.
    $exeResolved = if ($Exe) { (Resolve-Path -LiteralPath $Exe).ProviderPath } else { '' }
    $runPlan = Resolve-RunTarget -Example $Example -Bin $Bin -Exe $exeResolved -Package $Package -DebugProfile:$DebugProfile -ExtraArgs $ExtraArgs
    if (-not $runPlan.Ok) {
        # Unreachable in practice (the identical check above already exited on this), but Resolve-
        # RunTarget is a general-purpose function and must not assume its caller already validated.
        Write-Host "[pg] $($runPlan.Detail)" -ForegroundColor Red
        exit 2
    }
}

# DELIBERATE CHOICE: `run` takes a build slot too, exactly like build/test/corpus-test/release.
# This was weighed both ways and is not an oversight:
#
#   AGAINST taking the slot: a probe can run for hours (that is the whole point of `run` --
#   predict_census-shaped binaries are not a five-minute cargo build), and Enter-BuildSlot's
#   $BuildSlotTimeoutSeconds (default 1800) exists to report a WAITER stuck queuing behind a wedged
#   holder. If `run` occupied a slot for hours, a build queued behind it would eventually hit that
#   same timeout and exit $script:ExitCodeBuildSlotTimeout with a "held it" message that (correctly)
#   points at the run, not at anything broken.
#
#   FOR taking the slot: that timeout firing is exactly the graceful degradation this design wants.
#   The alternative -- `run` NOT counting against the semaphore -- breaks the property the rest of
#   this file relies on to avoid a reservation ledger: "at most $MaxConcurrent heavy operations
#   share the machine's headroom at once, so each one's job-object cap (Get-JobMemoryCapGB, divided
#   by $MaxConcurrent) is safe by construction." A `run` outside that count is an UNBOUNDED extra
#   consumer on top of up to $MaxConcurrent full-cap builds -- precisely the "several things assume
#   they have the whole machine's headroom, simultaneously" shape that took this box to zero memory
#   in the first place (CLAUDE.md's "Memory headroom" section). Costing a queued build a wait (which
#   fails loud and clear, is retryable, and points at the actual cause) is strictly preferable to
#   costing the machine an unaccounted-for 40-118GB job.
#
# So: a build stalled behind a long `run` is a KNOWN, documented, recoverable cost. An unbounded
# machine-wide worst case is what this whole file exists to rule out. Take the slot.
$sem = Enter-BuildSlot -MaxConcurrent $MaxConcurrent -TimeoutSeconds $BuildSlotTimeoutSeconds
if (-not $sem) {
    Write-Host "[pg] timed out after ${BuildSlotTimeoutSeconds}s waiting for a build slot (max $MaxConcurrent concurrent across all worktrees) -- another worktree's build (or a long-running 'pg.ps1 -Mode run') is holding it." -ForegroundColor Red
    exit $script:ExitCodeBuildSlotTimeout
}

# Re-check memory after the slot wait, which is allowed to last 30 minutes -- the reading the
# preflight gate approved is precisely the one most likely to be stale by the time a queued build
# actually starts. This is only a courtesy check that turns a hopeless start into a clear message;
# it is deliberately NOT the thing that bounds the machine. Several waiters can pass it in the same
# instant, and that is fine, because Enter-BuildSlot caps concurrent builds at 2 and each one then
# runs inside a job object with a hard commit ceiling. Two bounded builds cannot exhaust the box, so
# the race stops mattering without a reservation ledger to maintain.
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
        # Same derivation a build gets by default (Get-JobMemoryCapGB, divided across $MaxConcurrent
        # slots) -- see this file's header comment and the build-slot comment above for why `run`
        # is sized as one of those slots rather than given its own separate budget. -RunMemoryGB
        # bypasses the derivation outright for one invocation (e.g. the documented 40GB experiment)
        # without touching PANGLOSS_JOB_MEM_GB, which would also change every ordinary build's cap
        # for as long as the env var stayed set.
        $runMemGB = if ($RunMemoryGB -gt 0) { $RunMemoryGB } else { Get-JobMemoryCapGB -MaxConcurrent $MaxConcurrent }
        $runCpuRate = Get-JobCpuRatePercent
        Write-Host "[pg] run ($($runPlan.Label)): $($runPlan.LaunchExe) $($runPlan.LaunchArgs -join ' ')  (target-dir: $(if ($targetDir) { $targetDir } else { '<default>' }))" -ForegroundColor Cyan
        $code = Invoke-ProcessInJobObject -Exe $runPlan.LaunchExe -CmdArgs $runPlan.LaunchArgs -WorkingDirectory $rustRoot `
            -Priority $Priority -JobMemoryGB $runMemGB -CpuRatePercent $runCpuRate -Subject 'run'
    } elseif ($Mode -eq 'corpus-test') {
        $runnerLabel = if ($useNextest) { 'nextest' } elseif ($Mode -eq 'build' -or $Mode -eq 'release') { 'cargo build' } else { 'cargo test' }
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
        $runnerLabel = if ($useNextest) { 'nextest' } elseif ($Mode -eq 'build' -or $Mode -eq 'release') { 'cargo build' } else { 'cargo test' }
        Write-Host "[pg] cargo $($cargoArgs -join ' ')  (target-dir: $(if ($targetDir) { $targetDir } else { '<default>' }), runner: $runnerLabel)" -ForegroundColor Cyan
        $code = Invoke-CargoWithReaper -Exe 'cargo' -CmdArgs $cargoArgs -WorkingDirectory $rustRoot -Priority $Priority -JobMaxConcurrent $MaxConcurrent
    }
} finally {
    Exit-BuildSlot -Semaphore $sem
    # Post-run disk check, because the preflight one cannot catch this. Preflight runs BEFORE cargo,
    # so it happily passes at 46GB free and tells you nothing about the state cargo left behind --
    # and a fleet of parallel agents crosses the floor MIDWAY, which is exactly how this machine
    # reached 7GB free with no gate having fired. Reported after the fact rather than blocking (the
    # build already happened; failing it retroactively helps nobody), but reported LOUDLY, because
    # the next invocation is the one that dies and the cause is by then invisible.
    $freeAfter = if ($targetDir) { Get-FreeSpaceGB $targetDir } else { $null }
    if ($null -ne $freeAfter -and $freeAfter -lt 15) {
        Write-Host "[pg] WARNING: only ${freeAfter}GB free on the target drive after this run." -ForegroundColor Red
        Write-Host '[pg] Recover with: pg.ps1 -Mode gc (dry run, then -Apply). It only removes target dirs this repository owns and never touches an unmarked, preserved, or still-live one.' -ForegroundColor Yellow
        Write-Host '[pg] If that frees little, the space is likely a LOCAL rust/target from a bare-cargo run, which sits on the system drive because it bypassed target-dir redirection.' -ForegroundColor Yellow
    }
}

# nextest exits 4 for "no tests to run". That is already loud, but it does not say WHY, and the most
# common why on this repo is a TEST TARGET name passed to -Filter, which matches TEST NAMES. Naming the
# alternative here turns a wasted full-package compile into a one-line correction. Measured cause for
# this existing: seven such invocations in one session, five of them file/target names.
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
    # A failed build never registers a release deliverable (design doc, error handling) -- only
    # mark `preserved` after cargo itself reports success.
    Write-TargetOwnership -TargetDir $targetDir -RepositoryId $repoId -WorktreePath $repoRoot -Preserved | Out-Null
}

exit $code
