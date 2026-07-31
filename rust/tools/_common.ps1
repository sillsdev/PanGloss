# Shared helpers for build.ps1 / test.ps1: worktree/path resolution, disk-aware
# target-dir redirection, sccache wiring, a cross-worktree build-concurrency gate, and
# worktree-scoped cleanup of orphaned processes and stale build caches.
#
# Dot-source from build.ps1/test.ps1: . "$PSScriptRoot\_common.ps1"

$ErrorActionPreference = 'Stop'

# Two physical drives, two different jobs. G: (`HddCacheRoot`) is a spinning disk with lots of
# capacity -- fine for sccache's cache (mostly cheap, sequential-ish "read one blob" hits) but
# bad for an active target-dir, where compiling/linking hammers many small .rlib/.rmeta/.d/object
# files with scattered random I/O (worse still with this workspace's lto=fat/codegen-units=1
# release profile) -- exactly the access pattern an HDD's seek time punishes and NVMe doesn't
# have. C: (`SsdCacheRoot`) is NVMe, so target-dir prefers it whenever there's real headroom.
$script:SsdCacheRoot = if ($env:PANGLOSS_SSD_CACHE_ROOT) { $env:PANGLOSS_SSD_CACHE_ROOT } else { 'C:\cargo-targets' }
$script:HddCacheRoot = if ($env:PANGLOSS_CARGO_CACHE_ROOT) { $env:PANGLOSS_CARGO_CACHE_ROOT } else { 'G:\cargo-build-cache' }
# Reserve kept free on C: before handing more of it to a target-dir -- 30+ worktrees each
# growing a multi-GB target/ is what drove C: down to 1.3GB free in the first place; this
# threshold is the guard against refilling that crisis, not just "is there any space at all".
$script:MinFreeGBOnSsd = if ($env:PANGLOSS_MIN_FREE_SSD_GB) { [double]$env:PANGLOSS_MIN_FREE_SSD_GB } else { 50 }
$script:BuildSemaphoreName = 'Global\PanGlossCargoBuild'

# Logical processors deliberately left unclaimed by compiler work, machine-wide. This is the
# companion to Enter-BuildSlot: the semaphore bounds how many cargo INVOCATIONS run at once, but
# says nothing about how wide each one fans out, and Cargo's default is one job per logical core.
# Two slots at the default width is 40 rustc processes on a 20-thread CPU -- 2x oversubscribed,
# with nothing held back for the latency-sensitive daemons this box actually depends on (sshd, and
# Chrome Remote Desktop's remoting_host video encoder). That combination is what froze remote
# sessions during otherwise-normal builds. 6 is sized for those daemons plus the shell/editor
# driving the build, not for a second workload.
$script:InteractiveReserveThreads = if ($env:PANGLOSS_INTERACTIVE_RESERVE) { [int]$env:PANGLOSS_INTERACTIVE_RESERVE } else { 6 }

# The memory analogue of the thread reserve above, and the one this machine actually died on twice.
# Threads were capped; bytes were not. A capped 7-wide build still fans out to test processes that
# each reach many GB of RSS (corpus/foma cases; CLAUDE.md's "Probe pathological grammars
# single-threaded" records one probe at 30+ GB), so a run that is polite about CPU can still take
# the box to zero available memory -- at which point Windows starts trimming working sets, the
# pagefile thrashes, and sshd / Chrome Remote Desktop's remoting_host stall exactly as hard as they
# did under CPU starvation. Below-normal priority does not help: an unrunnable daemon waiting on a
# page fault is not competing for CPU at all.
#
# Reserve kept unclaimed, machine-wide, for the OS plus the daemons this box is reached through.
#
# PROPORTIONAL, not a fixed number of gigabytes. A flat reserve cannot be right on two machines at
# once: 8GB is 12% of this 64GB box (reasonable) but 50% of a 16GB developer machine, where it would
# refuse to start a build unless half of RAM were free -- a gate that blocks ordinary work gets set
# to 0 and then protects nobody. The fraction is the policy; the floor and ceiling stop it becoming
# meaningless at the extremes (nothing worth reserving on a tiny box, no point hoarding 25GB on a
# huge one).
$script:InteractiveReserveFraction = if ($env:PANGLOSS_MEM_RESERVE_FRACTION) { [double]$env:PANGLOSS_MEM_RESERVE_FRACTION } else { 0.10 }
$script:InteractiveReserveFloorGB = 1.5
$script:InteractiveReserveCeilingGB = 6
# Room a build needs to make actual progress, on top of the daemon reserve. Separate because it
# answers a different question: the reserve protects the machine FROM the build, this protects the
# build from starting into a machine where it can only thrash. ~2GB is the caller's own estimate of
# what a build needs under good conditions, and it is consistent with the measured 4.03GB peak for
# an entire -Mode test fan-out.
$script:MinBuildRoomGB = if ($env:PANGLOSS_MIN_BUILD_ROOM_GB) { [double]$env:PANGLOSS_MIN_BUILD_ROOM_GB } else { 2 }

function Get-InteractiveReserveGB {
    param([Nullable[double]]$TotalGB = (Get-TotalMemoryGB))
    if ($env:PANGLOSS_MIN_FREE_MEM_GB) { return [double]$env:PANGLOSS_MIN_FREE_MEM_GB }
    # Unmeasurable machine: the floor, not the ceiling. Guessing high here would refuse builds on a
    # machine we know nothing about, and the job object bounds the damage either way.
    if ($null -eq $TotalGB) { return $script:InteractiveReserveFloorGB }
    $r = $TotalGB * $script:InteractiveReserveFraction
    if ($r -lt $script:InteractiveReserveFloorGB) { $r = $script:InteractiveReserveFloorGB }
    if ($r -gt $script:InteractiveReserveCeilingGB) { $r = $script:InteractiveReserveCeilingGB }
    return [math]::Round($r, 1)
}

function Get-SpawnFloorGB {
    # The "do not start a build" line: daemon headroom PLUS enough room for the build to progress.
    # 16GB machine -> 1.6 + 2 = 3.6GB; 64GB -> 6 + 2 = 8GB. Note the 64GB answer lands on the flat
    # 8GB this replaced, which is why that number looked right on the box it was chosen on.
    #
    # This threshold matters much less than it did before procgov: a job object caps each build's
    # commit regardless of how much was free at spawn, so the spawn gate's remaining job is to turn
    # a hopeless start into a clear message instead of a mid-build allocation failure. That is why it
    # is sized to be generous to the developer rather than maximally cautious.
    param([Nullable[double]]$TotalGB = (Get-TotalMemoryGB))
    return [math]::Round(((Get-InteractiveReserveGB -TotalGB $TotalGB) + $script:MinBuildRoomGB), 1)
}
# Working-set allowance assumed per concurrent process, used to convert "how much memory is free"
# into "how many of these may run at once". Three numbers, because the phases are not comparable:
#
#  - Codegen under thin/no LTO (the pg-test-opt and dev profiles) is the predictable case: a rustc
#    compiling one crate, around a GB.
#  - Codegen under FAT LTO ([profile.release]: lto = "fat", codegen-units = 1) is heavier per
#    process, because the whole-program optimization happens inside RUSTC -- it holds an entire
#    dependency graph's LLVM IR in one address space -- and `link.exe` merely consumes the object
#    rustc produced. Cargo has no lever for "how many crates may be in their LTO phase at once"
#    separately from -j, so N concurrent jobs can mean N overlapping LTO peaks; that is why this is
#    a per-job allowance rather than a separate link-concurrency knob.
#
#    MEASURED on the primary dev box (2026-07-30, i7-12700, 63.7GB, warm sccache, -j3, touching
#    pg-cli/src/main.rs to force a real recompile + fat-LTO relink of the pangloss binary): peak
#    rustc working set 0.71GB, peak SUM across the whole cargo/rustc/sccache fan-out 1.2GB,
#    available memory moved 55.0 -> 52.9GB across 87 samples. So fat-LTO codegen here costs well
#    under a gigabyte per job, and this workspace has only one bin target plus 4 cdylib/staticlib
#    crates, bounding how many such peaks can ever coexist.
#
#    2GB is that measurement with roughly 3x headroom for a cold full build and for the binary
#    growing. It is deliberately NOT sized to bind on an idle machine: an earlier draft assumed 8GB
#    and throttled every `-Mode build` from 7 jobs to 2, which is a large, permanent cost paid on a
#    guess the measurement then refuted. Note what this implies -- the compile/link path is NOT
#    where this machine's memory went, so do not reach for this knob first when diagnosing the next
#    exhaustion; see $script:MemoryPerTestProcessGB, which is the unmeasured one.
#  - A TEST PROCESS in this workspace can be an entire grammar compile and is bounded by nothing we
#    control -- CLAUDE.md records one `pangloss batch` probe that reached 30+ GB RSS and never
#    finished. This is the allowance with real evidence behind the risk and NO measurement behind
#    the number: 2.5GB is a placeholder chosen to be heavier than a compile job, not a peak anyone
#    has recorded. Measuring a corpus-test run is the obvious next calibration, and until someone
#    does, this gate's protection on the test path rests on the reserve and the spawn refusal rather
#    than on this figure being right.
# ENFORCEMENT is delegated to a Windows job object via procgov, rather than hand-rolled here.
# See Get-ProcGovPath / Get-ProcGovArgs below for why, and for what that replaced.
#
# The per-build job-object commit ceiling is derived (Get-JobMemoryCapGB), not a fixed fraction.
# An earlier flat 45%-of-RAM had the same defect as the flat 8GB reserve: on 64GB it gives a sane
# 28GB per build, but on a 16GB machine it gives 7.2GB x 2 slots = 14.4GB of 16GB, leaving the OS
# nothing. Deriving it from (installed - reserve) / slots makes the arithmetic hold at both sizes.
# PANGLOSS_JOB_MEM_GB overrides the result outright.

$script:MemoryPerCompileJobGB = if ($env:PANGLOSS_MEM_PER_JOB_GB) { [double]$env:PANGLOSS_MEM_PER_JOB_GB } else { 1.5 }
$script:MemoryPerLtoLinkJobGB = if ($env:PANGLOSS_MEM_PER_LTO_JOB_GB) { [double]$env:PANGLOSS_MEM_PER_LTO_JOB_GB } else { 2 }
$script:MemoryPerTestProcessGB = if ($env:PANGLOSS_MEM_PER_TEST_GB) { [double]$env:PANGLOSS_MEM_PER_TEST_GB } else { 2.5 }

function Get-PerJobMemoryGB {
    # Which of the two compile-side allowances applies, decided by whether the run's profile turns
    # on fat LTO rather than by mode name -- `build` and `release` both reach the fat-LTO profile,
    # and `-DebugProfile` takes `build` back off it, so a mode-name test would be wrong twice.
    param([switch]$FatLto)
    if ($FatLto) { return $script:MemoryPerLtoLinkJobGB }
    return $script:MemoryPerCompileJobGB
}

function Get-CargoJobBudget {
    # Per-invocation `-j` such that ALL concurrently-permitted builds together still leave
    # $script:InteractiveReserveThreads logical processors free. Divided by MaxConcurrent rather
    # than handed out whole, because the build-slot semaphore is machine-wide: if two worktrees can
    # each hold a slot, each one's job count has to be sized for the case where both do.
    param([int]$MaxConcurrent = 1)
    $logical = [Environment]::ProcessorCount
    if ($MaxConcurrent -lt 1) { $MaxConcurrent = 1 }
    $budget = $logical - $script:InteractiveReserveThreads
    # Floor of 2, not 1: a single-job cargo serializes codegen across the whole workspace and turns
    # a several-minute build into a very long one, which in practice gets the cap disabled entirely
    # rather than tuned. Only reachable on a machine far smaller than this one.
    if ($budget -lt 2) { $budget = 2 }
    return [Math]::Max(2, [Math]::Floor($budget / $MaxConcurrent))
}

function Get-AvailableMemoryGB {
    # "Available", NOT "free". Win32_OperatingSystem.FreePhysicalMemory counts only the free list
    # and omits the standby list -- cache pages Windows will hand to a new allocation on demand.
    # On a box that has been building for a while almost all reclaimable memory sits in standby, so
    # FreePhysicalMemory reads far lower than what a process can actually get, and gating on it
    # would refuse builds on a machine with tens of GB genuinely available. Win32_PerfRawData_PerfOS_Memory
    # exposes the same counter Task Manager labels "Available", and its property names are NOT
    # localized (unlike Get-Counter's '\Memory\Available MBytes' path, which is, and which would
    # throw on a non-English Windows).
    #
    # Returns $null rather than 0 if neither source answers: "could not look" must stay
    # distinguishable from "nothing available", for the same reason Test-DiskReserve takes a
    # [Nullable[double]] -- a failed query that reads as 0 blocks every build on the machine.
    try {
        $perf = Get-CimInstance Win32_PerfRawData_PerfOS_Memory -ErrorAction Stop
        if ($perf -and $null -ne $perf.AvailableBytes) {
            return [math]::Round(([double]$perf.AvailableBytes) / 1GB, 1)
        }
    } catch {}
    try {
        $os = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop
        if ($os -and $null -ne $os.FreePhysicalMemory) {
            # KB units in this class. Understates by the standby list, hence the fallback ordering.
            return [math]::Round(([double]$os.FreePhysicalMemory) * 1KB / 1GB, 1)
        }
    } catch {}
    return $null
}

function Get-CommitChargeGB {
    # Committed bytes and the commit LIMIT, which are a different resource from available physical
    # memory and are the ones that actually matter here.
    #
    # Reported because the two diverge exactly when it counts. During the 2026-07-31 incident window
    # a `git` fork failed with "MEM_COMMIT failed / Resource temporarily unavailable" while doctor
    # cheerfully showed 52GB of available PHYSICAL memory -- because the commit charge was near its
    # limit even though RAM was free. Both things this repo now relies on are commit-denominated:
    # Resource-Exhaustion-Detector event 2004 reports committed memory per process, and procgov's
    # --maxjobmem caps committed bytes. Reporting only available physical while enforcing on commit
    # is how "there is plenty of memory" and "allocation failed" end up true at the same time.
    #
    # Win32_OperatingSystem's TotalVirtualMemorySize/FreeVirtualMemory are the commit limit and its
    # free remainder, in KB. Non-localized property names, same reason Get-AvailableMemoryGB prefers
    # the CIM classes over Get-Counter's localized paths.
    try {
        $os = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop
        if ($os -and $null -ne $os.TotalVirtualMemorySize -and $null -ne $os.FreeVirtualMemory) {
            $limit = [math]::Round(([double]$os.TotalVirtualMemorySize) * 1KB / 1GB, 1)
            $free = [math]::Round(([double]$os.FreeVirtualMemory) * 1KB / 1GB, 1)
            return [PSCustomObject]@{
                LimitGB     = $limit
                FreeGB      = $free
                CommittedGB = [math]::Round(($limit - $free), 1)
                PercentUsed = if ($limit -gt 0) { [int][math]::Round((($limit - $free) / $limit) * 100) } else { $null }
            }
        }
    } catch {}
    return $null
}

function Get-TotalMemoryGB {
    try {
        $os = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop
        if ($os -and $null -ne $os.TotalVisibleMemorySize) {
            return [math]::Round(([double]$os.TotalVisibleMemorySize) * 1KB / 1GB, 1)
        }
    } catch {}
    return $null
}

function Test-MemoryReserve {
    # The spawn gate: is there enough headroom to start this run at all. Pure decision logic taking
    # a number, like Test-DiskReserve, so it is unit-testable without a real machine state.
    #
    # This is the hard floor, distinct from Get-MemoryProcessBudget's narrowing below: under the
    # floor there is no concurrency low enough to be safe, because even ONE test process here can
    # be a multi-GB grammar compile. Refusing outright is the conservative direction, and it is the
    # direction that leaves a machine you can still SSH into.
    param(
        [Nullable[double]]$AvailableGB,
        [double]$MinFreeGB = (Get-SpawnFloorGB)
    )
    if ($null -eq $AvailableGB) {
        return [PSCustomObject]@{ Ok = $true; Detail = 'available memory unknown (not queryable) -- not blocking on it'; AvailableGB = $null }
    }
    $ok = $AvailableGB -ge $MinFreeGB
    return [PSCustomObject]@{
        Ok          = $ok
        Detail      = if ($ok) {
            "${AvailableGB}GB available (>= ${MinFreeGB}GB reserve)"
        } else {
            "${AvailableGB}GB available (< ${MinFreeGB}GB reserve) -- refusing to start a build that could take the machine to zero memory"
        }
        AvailableGB = $AvailableGB
    }
}

function Get-MemoryProcessBudget {
    # How many concurrent processes of a given weight the CURRENTLY available memory supports,
    # after setting the interactive reserve aside. Pure; the caller supplies the measurement.
    #
    # Returns $null for "no opinion" when memory is unqueryable, so a caller combining this with the
    # CPU budget can tell "memory says 3" from "memory has nothing to say" instead of silently
    # clamping every build to a fabricated number.
    #
    # Divided by MaxConcurrent for the same reason Get-CargoJobBudget is: the build-slot semaphore
    # is machine-wide, so each permitted build has to be sized for the case where all of them run.
    # That is deliberately conservative even though the measurement is live -- a build that started
    # one second ago has allocated almost nothing yet, so a live reading cannot see the peak that
    # the other slot is about to reach.
    param(
        [Nullable[double]]$AvailableGB,
        [double]$PerProcessGB,
        [double]$ReserveGB = (Get-InteractiveReserveGB),
        [int]$MaxConcurrent = 1
    )
    if ($null -eq $AvailableGB) { return $null }
    if ($PerProcessGB -le 0) { return $null }
    if ($MaxConcurrent -lt 1) { $MaxConcurrent = 1 }
    $usable = $AvailableGB - $ReserveGB
    if ($usable -lt 0) { $usable = 0 }
    $n = [Math]::Floor($usable / $PerProcessGB / $MaxConcurrent)
    # Floor of 1, never 0: reaching here means Test-MemoryReserve already passed the hard floor, so
    # the answer to "how many" is at worst "one at a time" -- 0 would mean a build that can never
    # run and would be reported as a concurrency setting rather than as the refusal it really is.
    return [int][Math]::Max(1, $n)
}

function Resolve-ConcurrencyBudget {
    # Combine the CPU-derived and memory-derived caps, keeping WHICH ONE bound the result, so the
    # preflight record can state the real reason a run is 3-wide instead of 7. Write-Preflight is
    # already careful not to print a derivation that did not produce the number beside it; the same
    # rule has to hold once there are two competing derivations.
    param(
        [int]$CpuBudget,
        [Nullable[int]]$MemoryBudget,
        [switch]$Explicit
    )
    if ($Explicit) {
        return [PSCustomObject]@{ Value = $CpuBudget; Bound = 'explicit'; Detail = 'explicit override' }
    }
    if ($null -eq $MemoryBudget) {
        return [PSCustomObject]@{ Value = $CpuBudget; Bound = 'cpu'; Detail = 'cpu budget (available memory not queryable)' }
    }
    if ($MemoryBudget -lt $CpuBudget) {
        return [PSCustomObject]@{ Value = [int]$MemoryBudget; Bound = 'memory'; Detail = "memory-bound (cpu budget would allow $CpuBudget)" }
    }
    return [PSCustomObject]@{ Value = $CpuBudget; Bound = 'cpu'; Detail = "cpu-bound (memory would allow $MemoryBudget)" }
}

# ---------------------------------------------------------------------------------------------
# Resource enforcement via a Windows job object (procgov)
#
# This replaces three hand-rolled mechanisms with one prefabricated tool, deliberately, because the
# hand-rolled versions were worse AND had to be maintained here:
#
#   * a 2-second polling loop that sampled available memory and ran `taskkill /T` -- a kernel job
#     object enforces a commit limit at ALLOCATION time, so there is no sampling interval to lose a
#     spike in, and the failure mode is rustc dying with an out-of-memory error rather than the whole
#     machine going unreachable while everything fights over the last gigabyte;
#   * a machine-wide reservation ledger with a mutex, invented to stop several waiting builds from
#     all seeing "memory is free!" at once and starting together. With a HARD per-build cap plus
#     Enter-BuildSlot's max of 2, the worst case is bounded by construction, so the race stops
#     mattering and ~200 lines of ledger, expiry and liveness bookkeeping go away;
#   * `-j`-based CPU limiting, which provably cannot bound rustc's total thread count
#     (rust-lang/rust#81957: -j caps codegen workers WITHIN an instance, not threads across
#     instances). Measured here: -j7 produced 112 threads and 17.7 of 20 cores busy. --cpurate is a
#     kernel-enforced ceiling that does not care how many threads exist.
#
# Cargo has no built-in answer to any of this -- rust-lang/cargo#12912 (limit parallelism
# automatically) is open and S-needs-design, and #9157 (restrict parallel linker invocations) and
# #11707 / #9735 (OOM linking many binaries) describe this exact workspace shape. There is no cargo
# plugin that solves it, so the choice is a job object or nothing.
#
# procgov is OPTIONAL. A machine without it still builds, with every pre-spawn gate intact and a
# loud warning -- an absent tool must degrade the protection, never break the build.
# ---------------------------------------------------------------------------------------------

function Get-ProcGovPath {
    # PATH first, then winget's shim and package directories, because winget only adds its Links
    # directory to PATH for shells started AFTER the install -- and the shell that just installed it
    # (or a long-lived agent session) will not see it there.
    if ($env:PANGLOSS_PROCGOV) { return (Test-Path $env:PANGLOSS_PROCGOV) ? $env:PANGLOSS_PROCGOV : $null }
    $cmd = Get-Command 'procgov' -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    foreach ($candidate in @(
            (Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Links\procgov.exe'),
            (Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages\LowLevelDesign.ProcessGovernor_Microsoft.Winget.Source_8wekyb3d8bbwe\procgov.exe')
        )) {
        if ($candidate -and (Test-Path $candidate)) { return $candidate }
    }
    return $null
}

function Get-JobMemoryCapGB {
    # Per-build commit ceiling. Derived from INSTALLED memory, not available memory: this is a
    # runaway backstop, and a cap that shrank because another build was already running would make
    # the second build fail spuriously at a size the first was allowed.
    param([int]$MaxConcurrent = 2, [Nullable[double]]$TotalGB = (Get-TotalMemoryGB))
    if ($env:PANGLOSS_JOB_MEM_GB) { return [int]$env:PANGLOSS_JOB_MEM_GB }
    if ($null -eq $TotalGB) { return $null }
    if ($MaxConcurrent -lt 1) { $MaxConcurrent = 1 }
    # (installed - daemon reserve) split across MaxConcurrent + 1, NOT MaxConcurrent.
    #
    # The +1 is a correctness fix, not padding. Sizing for exactly MaxConcurrent assumes the build
    # slot semaphore actually holds that many, and it does not: a named semaphore's maximum is fixed
    # by whichever process creates it FIRST and cannot be queried or changed afterwards, so one
    # caller passing a larger -MaxConcurrent silently raises the ceiling for every other worktree for
    # as long as the object lives. MEASURED 2026-07-31 on this machine: three procgov-wrapped builds
    # running concurrently under a nominal limit of 2, with the semaphore reporting zero free slots
    # (all three had genuine live parents -- not orphans, and not a Global\ vs Local\ namespace
    # split; both were ruled out). At 28GB each that permitted 84GB of commit on a 63.7GB box, so
    # CLAUDE.md's "bounded by construction" claim was simply false.
    #
    # Dividing by MaxConcurrent + 1 keeps the bound true through one slot of over-admission --
    # 64GB -> (63.7-6)/3 = 19GB, so even three concurrent builds stay inside the reserve. It costs
    # nothing real: the measured peak for an ENTIRE -Mode test fan-out is 4.03GB, so 19GB is still
    # ~4x headroom, and it deliberately stays generous enough for a corpus-test job whose test
    # processes (which run inside the same job object under procgov -r) can each be multi-GB.
    #
    # This is the cheap fix for the safety half of the problem. It does NOT fix the liveness half --
    # the queue is unfair and its 30-minute timeout is unreachable for a deep queue -- which is a
    # separate, larger change deliberately not made here.
    $cap = [math]::Floor((($TotalGB - (Get-InteractiveReserveGB -TotalGB $TotalGB)) / ($MaxConcurrent + 1)))
    # Floor of 4GB: a cap below this fails ordinary linking, and a limit that breaks every build is
    # worse than no limit, because it gets removed rather than tuned.
    return [int][Math]::Max(4, $cap)
}

function Get-JobCpuRatePercent {
    # Kernel-enforced ceiling sized from the same interactive reserve as the job budget, so the
    # daemons this machine is administered through keep headroom no matter how many threads rustc
    # decides to spawn. Returns $null when the reserve leaves nothing meaningful to cap.
    param([int]$ReserveThreads = $script:InteractiveReserveThreads)
    $logical = [Environment]::ProcessorCount
    if ($logical -le 0) { return $null }
    $usable = $logical - $ReserveThreads
    if ($usable -lt 1) { $usable = 1 }
    $pct = [int][math]::Floor(($usable / $logical) * 100)
    if ($pct -lt 10) { $pct = 10 }
    if ($pct -ge 100) { return $null }   # nothing to enforce
    return $pct
}

function Get-ProcGovArgs {
    # Pure argument construction, split out so the limits actually applied are assertable in a test
    # without launching procgov or a build.
    param(
        [Nullable[int]]$JobMemoryGB,
        [Nullable[int]]$CpuRatePercent,
        [string]$Priority = '',
        # OPT-IN alternative to the CPU rate: hand the job a fixed COUNT of logical processors and
        # leave the rest genuinely uncontended, instead of throttling its share of all of them. For a
        # latency-sensitive daemon (Chrome Remote Desktop's remoting_host encoder has a frame
        # deadline) dedicated cores should beat an aggregate percentage, because a rate-limited job
        # still schedules threads onto every core between throttle windows.
        #
        # Deliberately NOT the default, and this is the honest reason: --cpurate is measured working
        # and the affinity variant is not. On this hybrid i7-12700 (8 P-cores / 16 threads on CPU
        # 0-15, 4 E-cores on 16-19) a decimal count claims the FIRST N logical processors, so
        # -CpuCores 14 leaves 14-19 -- two P-core threads plus all four E-cores -- for the daemons,
        # which is a sensible split. But whether that actually beats a 70% rate for encoder latency
        # is unmeasured, and switching the default on an unmeasured hunch is what produced the
        # 8GB-per-job mistake earlier in this work.
        [Nullable[int]]$CpuCores,
        # OPT-IN EcoQoS (procgov --efficiency-mode, i.e. PROCESS_POWER_THROTTLING_EXECUTION_SPEED).
        # On a hybrid CPU Windows prefers E-cores for a throttled process, so this pushes compiler
        # work off the P-cores entirely. Costs build wall-clock, which is why it is opt-in.
        [switch]$EfficiencyMode,
        [Parameter(Mandatory)][string]$Exe,
        [string[]]$CmdArgs = @()
    )
    $a = @()
    if ($null -ne $JobMemoryGB) { $a += "--maxjobmem=${JobMemoryGB}G" }
    if ($null -ne $CpuCores) {
        # MUTUALLY EXCLUSIVE with --cpurate, not additive. procgov applies the rate only to the
        # selected cores when both are set, so --cpu=14 --cpurate=70 would mean 70% of 14 of 20
        # logical processors -- about 49% of the machine -- which is a much harsher cap than either
        # flag alone implies and would look like an unexplained slowdown.
        $a += "--cpu=$CpuCores"
    } elseif ($null -ne $CpuRatePercent) {
        $a += "--cpurate=$CpuRatePercent"
    }
    if ($EfficiencyMode) { $a += '--efficiency-mode=on' }
    if ($Priority) { $a += "--priority=$Priority" }
    # -r is REQUIRED, not optional: without it the limits apply to the cargo process alone, and every
    # rustc/link.exe -- which is where all the memory and CPU actually goes -- escapes the job.
    # It also makes procgov wait for the whole tree, so orphaned compilers cannot outlive the build.
    $a += '-r'
    $a += '--terminate-job-on-exit'
    $a += '--'
    $a += $Exe
    $a += $CmdArgs
    return $a
}

function Split-ExtraArgsSpec {
    # Tokenizes ONE string into an argv array, honoring double quotes so a value containing spaces
    # (a path, a filter expression) survives as a single argument.
    #
    # This exists because of a PowerShell limitation that cannot be worked around inside the scripts
    # it affects. `pwsh -File script.ps1 -Mode test -- --nocapture` FAILS at parameter-binding time
    # with "Parameter cannot be processed because the parameter name '' is ambiguous": under -File,
    # the bare `--` reaches the binder, which reads it as a parameter with an empty name. Verified
    # 2026-07-31: it fails identically with and without [CmdletBinding()], and quoting it ('--')
    # does not help, because -File passes raw strings that PowerShell then re-parses. The same
    # command works fine via the call operator (`& .\pg.ps1 -Mode test -- --nocapture`), because
    # there PowerShell's own parser consumes the `--` before binding ever sees it.
    #
    # That would be merely annoying if the failure were always loud. It is not. Omitting the
    # separator SILENTLY MISBINDS any single-dash cargo argument that prefix-matches a script
    # parameter -- measured: `-p foo` intended for cargo bound to -Package instead, so cargo never
    # received it and the run proceeded with a different meaning. That is precisely the
    # self-concealing class pg.ps1's PositionalBinding note exists to prevent.
    #
    # An environment variable is immune because it never passes through the parameter binder at all,
    # which is the whole point -- and it matches how this repo already handles binder-proof escape
    # hatches (PANGLOSS_ALLOW_BARE_CARGO).
    param([string]$Spec)
    if (-not $Spec) { return @() }
    $out = @()
    # Quoted run first so it wins over the bare-token alternative; the unquoted branch then takes
    # any run of non-whitespace.
    foreach ($m in [regex]::Matches($Spec, '"([^"]*)"|(\S+)')) {
        $out += if ($m.Groups[1].Success) { $m.Groups[1].Value } else { $m.Groups[2].Value }
    }
    return $out
}

function Get-TopMemoryConsumers {
    # Only ever used to make a refusal actionable: "8GB available, under the reserve" is a dead end
    # unless it also says what ate the memory. Read-only -- this never kills anything, because the
    # thing holding the memory may well belong to another worktree's healthy build (CLAUDE.md,
    # "Playing nicely with other worktrees").
    param([int]$Top = 5)
    try {
        Get-Process -ErrorAction Stop |
            Sort-Object -Property WorkingSet64 -Descending |
            Select-Object -First $Top -Property Id, ProcessName, @{ Name = 'WorkingSetGB'; Expression = { [math]::Round($_.WorkingSet64 / 1GB, 2) } }
    } catch {
        @()
    }
}

function Resolve-RunTarget {
    # Pure argument-construction logic for `pg.ps1 -Mode run`, split out of pg.ps1 itself so the
    # "exactly one selector", "'--' stripping", and "cargo run vs. a bare .exe" decisions are
    # unit-testable the same way Get-ProcGovArgs/Resolve-ConcurrencyBudget already are -- without
    # launching cargo, a probe binary, or touching the filesystem (existence of -Exe is the
    # CALLER's job, via Test-Path, precisely so this function never needs a real file to be tested).
    #
    # Returns Ok=$false with a Detail message for the usage error (wrong number of selectors);
    # callers are expected to print Detail and exit rather than this function throwing, matching
    # every other preflight-style function in this file (Test-DiskReserve, Test-MemoryReserve, ...).
    param(
        [string]$Example = '',
        [string]$Bin = '',
        [string]$Exe = '',
        [string]$Package = '',
        [switch]$DebugProfile,
        [string[]]$ExtraArgs = @()
    )
    # The outer @(...) around the whole pipeline is load-bearing, not decorative: PowerShell
    # unwraps a Where-Object result down to a bare (non-array) object whenever exactly one item
    # survives the filter -- the expected, common case here. Without it, .Count on that lone
    # surviving HASHTABLE silently returns its KEY count (2: "Name" and "Value") instead of "how
    # many selectors were passed", so the exactly-one-selector case (the one this check exists to
    # ACCEPT) was instead being rejected as "got 2". Caught by hand-testing this exact construct
    # standalone before trusting it in pg.ps1: `@{...} | Where-Object {...}` down to one match
    # returns a Hashtable, not a 1-element array, and Hashtable.Count means something else entirely.
    $selectors = @(@(
            @{ Name = 'Example'; Value = $Example }
            @{ Name = 'Bin'; Value = $Bin }
            @{ Name = 'Exe'; Value = $Exe }
        ) | Where-Object { $_.Value })
    if ($selectors.Count -ne 1) {
        $got = if ($selectors.Count -gt 0) { " ($(($selectors | ForEach-Object { $_.Name }) -join ', '))" } else { '' }
        return [PSCustomObject]@{
            Ok     = $false
            Detail = "-Mode run requires EXACTLY ONE of -Example <name> / -Bin <name> / -Exe <path>; got $($selectors.Count)$got."
        }
    }

    # A literal leading '--' is the natural way to type "everything after here is for the child
    # process" (cargo's own convention, which the rest of pg.ps1 already relies on for build/test
    # passthrough). It is not always special to PowerShell's own parameter binder (behavior differs
    # by invocation style -- see the `run` mode's own test/usage notes), so strip at most one
    # leading '--' defensively rather than forwarding it into the launch args, where it would show
    # up as a stray first argument to whatever gets run (a bare .exe would receive it as argv[1];
    # `cargo run` would get a DOUBLED '--' once the code below adds its own).
    $passthrough = @($ExtraArgs)
    if ($passthrough.Count -gt 0 -and $passthrough[0] -eq '--') {
        $passthrough = @($passthrough | Select-Object -Skip 1)
    }

    if ($Exe) {
        return [PSCustomObject]@{
            Ok         = $true
            LaunchExe  = $Exe
            LaunchArgs = $passthrough
            Label      = "exe: $Exe"
        }
    }

    # Example/Bin both go through `cargo run`, which builds first -- so a stale binary from an
    # earlier build is never what actually runs -- and then execs the result as a CHILD of cargo.
    # procgov's `-r` flag (Get-ProcGovArgs, `-r` = recurse the job object onto every descendant) is
    # what puts that child inside the ceiling, exactly like it already does for rustc/link.exe
    # under an ordinary build; nothing extra is needed here for that to work.
    $launchArgs = @('run')
    if (-not $DebugProfile) { $launchArgs += '--release' }
    if ($Package) { $launchArgs += @('-p', $Package) }
    if ($Example) { $launchArgs += @('--example', $Example) }
    if ($Bin) { $launchArgs += @('--bin', $Bin) }
    # cargo's OWN '--' separator, inserted unconditionally regardless of what the caller typed:
    # without it, args meant for the binary would be parsed by cargo itself as (unrecognized)
    # cargo flags instead of reaching the program.
    if ($passthrough.Count -gt 0) { $launchArgs += @('--') + $passthrough }
    return [PSCustomObject]@{
        Ok         = $true
        LaunchExe  = 'cargo'
        LaunchArgs = $launchArgs
        Label      = if ($Example) { "example: $Example" } else { "bin: $Bin" }
    }
}

function Get-ExhaustionConsumersFromMessage {
    # Pure parsing, split out of Get-ResourceExhaustionEvents below so it is unit-testable against
    # SAMPLE message text (rust/tools/tests/run-mode.tests.ps1) without ever calling Get-WinEvent --
    # matching this file's established pattern of keeping the parsing/decision logic pure and
    # pushing the live query to a thin wrapper (Get-AvailableMemoryGB vs. Test-MemoryReserve is the
    # same split).
    #
    # MESSAGE TEXT, verified against real Microsoft-Windows-Resource-Exhaustion-Detector (event ID
    # 2004) events on this machine (2026-07-31):
    #   "Windows successfully diagnosed a low virtual memory condition. The following programs
    #    consumed the most virtual memory: predict_census.exe (30004) consumed 118387073024 bytes,
    #    vmmemCmZygote (9984) consumed 853762048 bytes, and MsMpEng.exe (5320) consumed 529256448
    #    bytes."
    # Always exactly "<name> (<pid>) consumed <N> bytes", comma-joined, "and" before the last one.
    # Parsing this is inherently FRAGILE -- Microsoft does not publish a stable grammar for it, it
    # is not guaranteed to hold across Windows builds/locales, and a process name with an embedded
    # space (none observed) would break the regex below. So parsing is best-effort ONLY: a message
    # that fails to match returns an EMPTY list, never a thrown error -- the caller keeps RawMessage
    # intact for a human to read regardless of whether this parses it.
    param([string]$Message)
    $consumers = @()
    if (-not $Message) { return $consumers }
    foreach ($m in [regex]::Matches($Message, '(\S+)\s+\((\d+)\)\s+consumed\s+(\d+)\s+bytes')) {
        $bytes = [int64]$m.Groups[3].Value
        $consumers += [PSCustomObject]@{
            ProcessName = $m.Groups[1].Value
            Pid         = [int]$m.Groups[2].Value
            Bytes       = $bytes
            GB          = [math]::Round($bytes / 1GB, 1)
        }
    }
    return $consumers
}

function Get-ResourceExhaustionEvents {
    # Windows already diagnoses this and logs it: Microsoft-Windows-Resource-Exhaustion-Detector
    # fires event ID 2004 into the System log when the OS is approaching its commit limit, and the
    # message names the top-3 processes it picked plus how many bytes each had committed. This
    # machine had three such events BEFORE this function existed and nobody was reading the log:
    #   2026-07-04  hc-rs.exe         97 GB
    #   2026-07-26  pangloss.exe      90 GB
    #   2026-07-30  predict_census.exe  118 GB (repeated over ~45 minutes as it climbed)
    # All three were a single PanGloss binary invoked DIRECTLY -- never through cargo, so nothing
    # that only wrapped cargo (Invoke-CargoWithReaper, before this file's `run`-mode support) could
    # ever have bounded them. This function is what lets `pg.ps1 -Mode doctor` surface that history
    # instead of it sitting undiscovered in the System log until someone thinks to check manually.
    #
    # Message parsing itself lives in Get-ExhaustionConsumersFromMessage above; this function is
    # just the live Get-WinEvent query plus the "no data vs. genuinely none" distinction below.
    param(
        [datetime]$Since = (Get-Date).AddDays(-7),
        [int]$MaxEvents = 20
    )
    try {
        $events = Get-WinEvent -FilterHashtable @{
            LogName      = 'System'
            ProviderName = 'Microsoft-Windows-Resource-Exhaustion-Detector'
            Id           = 2004
            StartTime    = $Since
        } -MaxEvents $MaxEvents -ErrorAction Stop
    } catch {
        # Get-WinEvent throws (rather than returning an empty collection) both for "genuinely
        # nothing in this window" -- the NORMAL case on a machine with no exhaustion history, hit
        # on every doctor run that finds nothing wrong -- and for "could not look at all" (provider
        # absent on this Windows edition/locale, System log access denied). Those two are NOT the
        # same fact and must not be reported identically: the first is good news, the second is "I
        # don't know" and must never be silently upgraded to good news. Get-WinEvent's own message
        # text is the only signal available to tell them apart (there is no separate exception
        # type), so match on it explicitly rather than treating every failure as "no data".
        if ($_.Exception.Message -like 'No events were found*') {
            return [PSCustomObject]@{
                Ok        = $true
                Queryable = $true
                Detail    = "0 exhaustion event(s) since $($Since.ToString('u')) -- queried successfully, none found"
                Events    = @()
            }
        }
        return [PSCustomObject]@{
            Ok        = $true
            Queryable = $false
            Detail    = "could not query Resource-Exhaustion-Detector events: $($_.Exception.Message)"
            Events    = @()
        }
    }
    $parsed = @()
    foreach ($e in $events) {
        $parsed += [PSCustomObject]@{
            TimeCreated = $e.TimeCreated
            Consumers   = @(Get-ExhaustionConsumersFromMessage -Message $e.Message)
            RawMessage  = $e.Message
        }
    }
    return [PSCustomObject]@{
        Ok        = $true
        Queryable = $true
        Detail    = "$($parsed.Count) exhaustion event(s) since $($Since.ToString('u'))"
        Events    = $parsed
    }
}

function Get-RepoRoot {
    # `git rev-parse --show-toplevel` always answers for whichever worktree the caller is
    # standing in, so this resolves correctly whether run from the main checkout or any
    # .claude/worktrees/* checkout -- no hardcoded paths. Split out from Get-RustRoot because
    # worktree metadata/ownership/base-check plumbing below needs the repo root itself, not the
    # rust/ subdirectory under it.
    $top = git rev-parse --show-toplevel 2>$null
    if (-not $top) { throw "Not inside a git repo (run from within a PanGloss checkout)." }
    return $top
}

function Get-RustRoot {
    $top = Get-RepoRoot
    $rustDir = Join-Path $top 'rust'
    if (-not (Test-Path $rustDir)) { throw "No rust/ dir under $top" }
    return $rustDir
}

function Get-RepoIdentity {
    # A stable identity for "which repository is this" that survives everything a path can't:
    # cloning to a new location, renaming the leaf directory, or being a linked worktree with a
    # completely different directory name from the primary checkout. The root commit is the one
    # thing every clone/worktree of the same repo shares and nothing else does -- unlike a path
    # (worktree ownership markers need to detect "this target dir belongs to a DIFFERENT repo",
    # which a path comparison can't do across machines/clones).
    param([string]$RepoRoot = (Get-RepoRoot))
    $roots = git -C $RepoRoot rev-list --max-parents=0 HEAD 2>$null
    if (-not $roots) { throw "Could not determine repository root commit (git rev-list --max-parents=0 HEAD) under $RepoRoot" }
    # Sorted so the (unusual) case of multiple root commits -- e.g. history stitched together
    # from unrelated histories -- still yields one deterministic identity regardless of git's
    # traversal order, instead of an identity that could flip between runs.
    return (($roots | Sort-Object) -join ',')
}

function Get-WorktreeSlug {
    param([string]$RustRoot)
    # Leaf directory name of the checkout root (e.g. "agent-a30b043e9e8bc26b2", or
    # "PanGloss" for the primary checkout) -- stable, unique, matches `git worktree list`.
    $repoRoot = Split-Path $RustRoot -Parent
    return (Split-Path $repoRoot -Leaf)
}

function Get-FreeSpaceGB {
    param([string]$Path)
    $driveRoot = [System.IO.Path]::GetPathRoot($Path)
    if (-not $driveRoot) { return $null }
    $driveLetter = $driveRoot.TrimEnd('\').TrimEnd(':')
    $d = Get-PSDrive -Name $driveLetter -PSProvider FileSystem -ErrorAction SilentlyContinue
    if (-not $d) { return $null }
    return [math]::Round($d.Free / 1GB, 1)
}

function Resolve-TargetDir {
    param([string]$RustRoot)
    # Never fight a choice already made on purpose: an explicit CARGO_TARGET_DIR env var, or
    # an existing worktree-local .cargo/config.toml target-dir, wins outright.
    if ($env:CARGO_TARGET_DIR) { return $env:CARGO_TARGET_DIR }
    $cfg = Join-Path $RustRoot '.cargo\config.toml'
    if (Test-Path $cfg) {
        $text = Get-Content $cfg -Raw
        if ($text -match 'target-dir\s*=') { return $null }  # let cargo read its own config
    }
    $slug = Get-WorktreeSlug -RustRoot $RustRoot

    # Prefer the SSD (C:) for the active target-dir: compiling/linking is scattered random
    # I/O across many small files, and lto=fat/codegen-units=1 makes the link step especially
    # heavy per binary -- exactly what an HDD's seek time punishes and NVMe doesn't. Only fall
    # back to the HDD cache root (G:) once C: no longer has enough headroom, so many worktrees
    # building at once can't refill the disk-space crisis that motivated moving off C: at all.
    $ssdFree = Get-FreeSpaceGB $script:SsdCacheRoot
    if ($null -ne $ssdFree -and $ssdFree -ge $script:MinFreeGBOnSsd) {
        $target = Join-Path $script:SsdCacheRoot $slug
        New-Item -ItemType Directory -Force -Path $target | Out-Null
        return $target
    }
    if ($null -ne $ssdFree) {
        Write-Host "[build-env] $($script:SsdCacheRoot)'s drive has ${ssdFree}GB free (< $($script:MinFreeGBOnSsd)GB reserve) -- using HDD cache root instead" -ForegroundColor Yellow
    }

    # Defensive: these scripts are checked out into every worktree, including on machines
    # that don't have the HDD cache drive (default G:) at all. Degrade to cargo's normal
    # local target/ instead of crashing on New-Item if the drive is missing.
    $driveRoot = [System.IO.Path]::GetPathRoot($script:HddCacheRoot)
    if ($driveRoot -and -not (Test-Path $driveRoot)) {
        Write-Host "[build-env] cache drive '$driveRoot' not found on this machine -- falling back to local target/ (not redirecting CARGO_TARGET_DIR)" -ForegroundColor Yellow
        return $null
    }
    $target = Join-Path $script:HddCacheRoot $slug
    New-Item -ItemType Directory -Force -Path $target | Out-Null
    return $target
}

function Use-Sccache {
    if (-not (Get-Command sccache -ErrorAction SilentlyContinue)) { return $false }
    $env:RUSTC_WRAPPER = 'sccache'
    # Deliberately on the HDD root, not the SSD one: a cache hit is one blob read, not the
    # scattered-small-file churn a live target-dir produces, so the HDD's capacity matters
    # more here than its seek time -- and keeping it off C: means the shared cache growing
    # large over time can't itself contribute to a C: space crisis.
    if (-not $env:SCCACHE_DIR) { $env:SCCACHE_DIR = Join-Path $script:HddCacheRoot 'sccache' }
    New-Item -ItemType Directory -Force -Path $env:SCCACHE_DIR | Out-Null
    return $true
}

function Set-SccacheServerPriority {
    # Without this, dropping cargo to BelowNormal silently fails to cover most of the actual
    # compiler work on this machine. MEASURED during a workspace build with the priority drop
    # already in place on cargo: 7 concurrent rustc, of which only 2 were BelowNormal and 4 were
    # still Normal.
    #
    # The reason is RUSTC_WRAPPER=sccache. Cargo does not exec rustc itself; it invokes a short-lived
    # sccache client, which hands the compile to the long-lived sccache SERVER daemon, and the
    # server spawns rustc. Those rustc processes are children of the daemon, so Windows' inherit
    # rule gives them the DAEMON's priority class -- not cargo's. The daemon outlives any one build
    # and normally starts at Normal, so the bulk of compilation kept running at Normal no matter
    # what priority cargo held.
    #
    # Call AFTER Test-SccacheHealth: its `sccache --show-stats` is what starts the server if it
    # isn't already up, so by then there is a process to find. Priority is inherited at spawn time,
    # so this must also happen BEFORE cargo starts -- already-running rustc keep the class they
    # were born with.
    param([ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = 'BelowNormal')
    $changed = 0
    foreach ($p in @(Get-Process -Name sccache -ErrorAction SilentlyContinue)) {
        try {
            if ($p.PriorityClass -ne $Priority) { $p.PriorityClass = $Priority; $changed++ }
        } catch {
            # Non-fatal by design: the daemon may belong to another user, or may have exited between
            # the enumeration and the assignment. A build that runs at the wrong priority is a
            # performance problem; a build that refuses to start over one is a worse one.
            Write-Host "[pg] note: could not set $Priority priority on sccache server (pid $($p.Id)): $($_.Exception.Message)" -ForegroundColor DarkGray
        }
    }
    return $changed
}

# ---------------------------------------------------------------------------------------------
# Build slots: N named MUTEXES, not one counted semaphore.
#
# The semaphore this replaced deadlocked every worktree on this machine on 2026-07-31. A counted
# semaphore's count is NOT restored when its holder dies, and in agent workflows the holder dies
# constantly: a tool timeout, an agent stop/resume, or a detached invocation whose parent
# conversation has gone all kill pg.ps1 somewhere between Enter-BuildSlot and Exit-BuildSlot. Any
# critical section between acquire and release WILL be interrupted eventually here. Observed
# consequence: 4+ worktrees sitting at "waiting for a build slot" for 20+ minutes with ZERO
# cargo/rustc/link processes alive machine-wide, recoverable only by hand-releasing the semaphore
# until it threw SemaphoreFullException.
#
# A mutex cannot leak that way, because the KERNEL owns the cleanup: when a thread holding a mutex
# exits without releasing it, the mutex becomes ABANDONED and the next waiter is granted ownership
# (surfaced to .NET as AbandonedMutexException, which carries the index of the mutex it just gave
# us). Catching it and carrying on IS the recovery -- there is no ledger to reconcile, no sweep to
# schedule, and no hand-repair procedure to document. Same reasoning that replaced the hand-rolled
# memory watchdog with a job object: prefer the primitive whose cleanup the OS already guarantees.
#
# It also removes the frozen-maximum wart. A semaphore's max is fixed by whichever process creates
# it FIRST and cannot be queried or changed (measured: three builds ran concurrently under a nominal
# limit of 2). Here the slot count is simply how many mutex NAMES a caller waits on, so a caller
# asking for 1 waits on Slot0 alone and genuinely cannot take a second slot.
#
# TRANSITION HAZARD, read before deploying: a worktree still running the OLD semaphore code and one
# running this share NO mutual exclusion -- different primitives, different names. Every worktree
# must pick this up, or the machine's real concurrency is (old-code builds) + (new-code builds).
# ---------------------------------------------------------------------------------------------

$script:BuildSlotMutexPrefix = 'Global\PanGlossBuildSlot'

function New-BuildSlotMutex {
    param([Parameter(Mandatory)][string]$Name)
    try {
        return New-Object System.Threading.Mutex($false, $Name)
    } catch [System.UnauthorizedAccessException] {
        # Same Global\ -> Local\ fallback the semaphore had, for a session that may not create
        # kernel objects in the global namespace.
        return New-Object System.Threading.Mutex($false, ($Name -replace '^Global\\', 'Local\'))
    }
}

function Enter-BuildSlot {
    # -TimeoutSeconds <= 0 waits indefinitely (kept for direct callers); pg.ps1 passes a real
    # timeout so a genuinely long queue is reported rather than hung on forever.
    param([int]$MaxConcurrent = 2, [int]$TimeoutSeconds = 0)
    if ($MaxConcurrent -lt 1) { $MaxConcurrent = 1 }

    $mutexes = @()
    for ($i = 0; $i -lt $MaxConcurrent; $i++) {
        $mutexes += New-BuildSlotMutex -Name "$($script:BuildSlotMutexPrefix)$i"
    }

    # Report WHO holds the slots before blocking. A 20-minute anonymous wait is indistinguishable
    # from a deadlock, and that ambiguity is what actually burned time during the incident -- nobody
    # could tell a real queue from a dead one. Best-effort: never let diagnostics fail a build.
    Write-Host "[build-env] waiting for a build slot ($MaxConcurrent concurrent across all worktrees)..." -ForegroundColor DarkGray
    try {
        $holders = @(Get-BuildSlotHolders)
        foreach ($h in $holders) {
            $state = if ($h.Alive) { "alive since $($h.AcquiredAt)" } else { 'NOT ALIVE (kernel will hand this slot over)' }
            Write-Host "[build-env]   slot $($h.Slot): pid $($h.Pid) ($($h.Mode) in $($h.Worktree)) -- $state" -ForegroundColor DarkGray
        }
    } catch {}

    $timeoutMs = if ($TimeoutSeconds -le 0) { [System.Threading.Timeout]::Infinite } else { $TimeoutSeconds * 1000 }
    $index = -1
    try {
        $index = [System.Threading.WaitHandle]::WaitAny($mutexes, $timeoutMs)
    } catch [System.Threading.AbandonedMutexException] {
        # The previous holder died without releasing. We now OWN that mutex -- this is the kernel
        # performing the recovery that the semaphore design required a human to do by hand.
        $index = $_.Exception.MutexIndex
        Write-Host "[build-env] recovered an abandoned build slot ($index) -- its previous holder exited without releasing it." -ForegroundColor Yellow
    }

    if ($index -eq [System.Threading.WaitHandle]::WaitTimeout -or $index -lt 0) {
        foreach ($m in $mutexes) { $m.Dispose() }
        return $null
    }

    $slot = [PSCustomObject]@{ Mutexes = $mutexes; Index = $index }
    try { Write-BuildSlotHolder -Slot $index } catch {}
    return $slot
}

function Exit-BuildSlot {
    # Accepts the object Enter-BuildSlot returned. Releasing only the mutex we actually acquired
    # matters: ReleaseMutex on one we do not own throws ApplicationException.
    param($Semaphore)
    if (-not $Semaphore) { return }
    try {
        if ($null -ne $Semaphore.Index -and $Semaphore.Mutexes) {
            try { $Semaphore.Mutexes[$Semaphore.Index].ReleaseMutex() } catch {}
            try { Clear-BuildSlotHolder -Slot $Semaphore.Index } catch {}
            foreach ($m in $Semaphore.Mutexes) { $m.Dispose() }
            return
        }
        # Back-compat: a caller still holding a raw semaphore/mutex handle from older code.
        $Semaphore.Release() | Out-Null
        $Semaphore.Dispose()
    } catch {}
}

# ---------------------------------------------------------------------------------------------
# Slot holder ledger -- DIAGNOSTIC ONLY.
#
# Correctness does not depend on this: the mutexes above are the mutual exclusion, and the kernel
# reclaims them. This exists so a waiter can say "slot 0: pid 1234, corpus-test in crp-objective,
# alive since 10:42" instead of blocking anonymously, and so `doctor` can report the same. A stale
# entry (holder killed) is therefore expected and harmless, and is reported as NOT ALIVE rather
# than trusted -- it must never be used to decide whether a slot is free.
# ---------------------------------------------------------------------------------------------

function Get-BuildSlotLedgerPath {
    $root = if ($env:PANGLOSS_STATE_ROOT) { $env:PANGLOSS_STATE_ROOT } elseif ($env:ProgramData) { Join-Path $env:ProgramData 'PanGloss' } else { Join-Path ([System.IO.Path]::GetTempPath()) 'PanGloss' }
    if (-not (Test-Path $root)) { New-Item -ItemType Directory -Force -Path $root | Out-Null }
    return (Join-Path $root 'build-slots')
}

function Write-BuildSlotHolder {
    param([Parameter(Mandatory)][int]$Slot, [string]$Mode = '', [string]$Worktree = '')
    $dir = Get-BuildSlotLedgerPath
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    if (-not $Mode) { $Mode = if ($script:CurrentPgMode) { $script:CurrentPgMode } else { 'build' } }
    if (-not $Worktree) { $Worktree = try { Split-Path (Get-RepoRoot) -Leaf } catch { 'unknown' } }
    [PSCustomObject]@{ Pid = $PID; Mode = $Mode; Worktree = $Worktree; AcquiredAt = (Get-Date).ToString('HH:mm:ss') } |
        ConvertTo-Json -Compress | Set-Content -Path (Join-Path $dir "slot$Slot.json") -Encoding UTF8
}

function Clear-BuildSlotHolder {
    param([Parameter(Mandatory)][int]$Slot)
    Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path (Get-BuildSlotLedgerPath) "slot$Slot.json")
}

function Get-BuildSlotHolders {
    $dir = Get-BuildSlotLedgerPath
    if (-not (Test-Path $dir)) { return @() }
    $out = @()
    foreach ($f in @(Get-ChildItem -Path $dir -Filter 'slot*.json' -ErrorAction SilentlyContinue)) {
        try {
            $e = Get-Content $f.FullName -Raw | ConvertFrom-Json
            $alive = $false
            try { $alive = $null -ne (Get-Process -Id $e.Pid -ErrorAction Stop) } catch { $alive = $false }
            $out += [PSCustomObject]@{
                Slot       = ($f.BaseName -replace '^slot', '')
                Pid        = [int]$e.Pid
                Mode       = [string]$e.Mode
                Worktree   = [string]$e.Worktree
                AcquiredAt = [string]$e.AcquiredAt
                Alive      = $alive
            }
        } catch {}
    }
    return @($out | Sort-Object Slot)
}

function Invoke-ProcessInJobObject {
    # The procgov-wrapping core, extracted out of what used to be the entire body of
    # Invoke-CargoWithReaper so a cargo build and an arbitrary long-running PanGloss binary
    # (`pg.ps1 -Mode run` -- predict_census, `pangloss batch`, ...) get the SAME kernel-enforced
    # ceiling from ONE implementation instead of two copies that can drift. This is the change that
    # closes the gap named in the `run` mode's own design note: every incident that took the
    # machine to a frozen state (predict_census.exe 118GB, pangloss.exe 90GB, hc-rs.exe 97GB -- see
    # CLAUDE.md) was a binary invoked DIRECTLY, never through cargo, so nothing that wrapped ONLY
    # cargo could ever have caught it.
    #
    # Callers resolve JobMemoryGB/CpuRatePercent themselves (Get-JobMemoryCapGB/
    # Get-JobCpuRatePercent) rather than this function deriving them, because different callers
    # derive them differently -- a build divides the machine's headroom by how many build SLOTS
    # are permitted at once (Enter-BuildSlot's MaxConcurrent); `run` sizes its cap the same way
    # (see pg.ps1's `run` mode comment for why it also takes a build slot) but a caller wanting the
    # documented 40GB experiment overrides the number outright rather than the derivation.
    param(
        [Parameter(Mandatory)][string]$Exe,
        # NOT named $Args -- that's PowerShell's automatic variable inside a function scope,
        # and a formal parameter of that name silently fails to bind (the child would run with
        # zero arguments instead of erroring).
        [string[]]$CmdArgs = @(),
        [string]$WorkingDirectory,
        # corpus-test needs cargo's raw stdout AFTER the run to sum the PANGLOSS_CORPUS_CASES
        # lines pg_conformance_fixtures::corpus::record_cases emits, regardless of pass/fail --
        # a green exit code alone must not be trusted (a suite that compiles, runs, and asserts
        # nothing would otherwise still "pass"). Redirected to a file rather than piped so a
        # reaped/killed process's output up to that point isn't lost.
        [string]$CaptureStdoutPath = '',
        # CPU priority class for $Exe AND every process it spawns -- see the PriorityClass block
        # below for why this is inherited rather than set per-child.
        [ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = 'BelowNormal',
        [Nullable[int]]$JobMemoryGB,
        [Nullable[int]]$CpuRatePercent,
        # Purely cosmetic word choice for the "no procgov" warning ("this build runs..." vs
        # "this run runs...") so the message stays accurate for whichever pg.ps1 mode called in,
        # without a second near-duplicate warning string per caller.
        [string]$Subject = 'build'
    )
    # Wrap the whole process tree in a Windows job object (via procgov) when available, so the
    # memory and CPU ceilings are enforced by the KERNEL at allocation/scheduling time rather than
    # by this script noticing after the fact. -r puts every descendant process in the job, which is
    # where all the resource use actually is (rustc/link.exe for a build; whatever a probe forks for
    # a `run`). See the Get-ProcGovPath block above for what this replaced.
    $procgov = Get-ProcGovPath
    $launchExe = $Exe
    $launchArgs = $CmdArgs
    if ($procgov) {
        $launchArgs = Get-ProcGovArgs -JobMemoryGB $JobMemoryGB -CpuRatePercent $CpuRatePercent -Priority $Priority -Exe $Exe -CmdArgs $CmdArgs
        $launchExe = $procgov
        $capDesc = @()
        if ($null -ne $JobMemoryGB) { $capDesc += "${JobMemoryGB}GB committed memory" }
        if ($null -ne $CpuRatePercent) { $capDesc += "${CpuRatePercent}% CPU" }
        Write-Host "[pg] job object: $($capDesc -join ', ') (kernel-enforced across $Exe and every process it spawns)" -ForegroundColor DarkGray
    } else {
        Write-Host "[pg] WARNING: procgov not found -- this $Subject runs with NO kernel-enforced memory or CPU ceiling." -ForegroundColor Yellow
        Write-Host '[pg] The pre-spawn gates still apply, but nothing bounds a runaway once it starts. Install with: winget install LowLevelDesign.ProcessGovernor' -ForegroundColor Yellow
    }

    $psiArgs = @{
        FilePath         = $launchExe
        ArgumentList     = $launchArgs
        WorkingDirectory = $WorkingDirectory
        NoNewWindow      = $true
        PassThru         = $true
    }
    if ($CaptureStdoutPath) { $psiArgs['RedirectStandardOutput'] = $CaptureStdoutPath }
    # Start-Process (not `& cargo ...`) so we hold a real PID to reap. On Windows,
    # Stop-Process / Ctrl+C alone does NOT kill rustc/link.exe descendants -- only
    # `taskkill /T` (kill the whole tree) reliably does, which is what `finally` runs here
    # if the process is still alive (e.g. this script itself got Ctrl+C'd).
    $psi = Start-Process @psiArgs

    # Drop the whole process tree below the interactive daemons. A job cap alone is not enough:
    # capping jobs bounds how many runnable threads exist, but every one of them still sits at
    # Normal priority, which is exactly where sshd and Chrome Remote Desktop's remoting_host video
    # encoder sit. Equal priority means the scheduler round-robins them, so the encoder waits
    # behind compiler (or probe) work for its frame deadline and the remote session stalls.
    # BelowNormal means any daemon that becomes runnable preempts that work immediately; the run
    # gives up almost nothing in wall-clock, because it still owns every core no one else wants.
    #
    # Set on the PARENT rather than hunting down each descendant, because Windows propagates it for
    # free: CreateProcess gives a child NORMAL_PRIORITY_CLASS by default *unless* the creating
    # process is IDLE or BELOW_NORMAL, in which case the child inherits the parent's class. So the
    # fan-out below $Exe lands at BelowNormal without any per-child bookkeeping -- which also means
    # it keeps working for processes this script never sees (build scripts, proc-macro servers, the
    # linker's own children, or whatever a probe binary forks).
    #
    # Best-effort: a process that failed instantly (bad args, missing toolchain/file) can already
    # be gone, and losing the priority drop must not turn that into a different, more confusing
    # error. Set unconditionally, including for 'Normal': $Exe inherits its class from THIS
    # PowerShell host, so if the host is itself running below normal (a nested build, or a shell
    # someone de-prioritized), an early-out on 'Normal' would quietly fail to deliver the
    # full-speed run that was explicitly asked for.
    try {
        if (-not $psi.HasExited) { $psi.PriorityClass = $Priority }
    } catch {
        Write-Host "[pg] note: could not set $Priority priority on $Exe (pid $($psi.Id)): $($_.Exception.Message)" -ForegroundColor DarkGray
    }

    # A plain wait. There is no polling watchdog here any more: under procgov the ceilings are
    # enforced by the kernel continuously, with no sampling interval for a spike to hide in, and a
    # run that exceeds its commit limit fails its own allocation instead of taking the machine
    # down. The taskkill in `finally` stays, because it covers a case the job object does not: this
    # SCRIPT being interrupted (Ctrl+C) while the run is healthy and still running.
    try {
        Wait-Process -Id $psi.Id
        return $psi.ExitCode
    } finally {
        if (-not $psi.HasExited) {
            & taskkill /T /F /PID $psi.Id 2>$null | Out-Null
        }
    }
}

function Invoke-CargoWithReaper {
    # Cargo-specific front end onto Invoke-ProcessInJobObject, kept as its own function (rather than
    # inlining the two lines below at every call site) so the four existing modes (build/test/
    # corpus-test/release) don't each have to know how to derive the job-object ceilings. Behavior
    # is unchanged from before the refactor above: same parameters, same derivation
    # (Get-JobMemoryCapGB/Get-JobCpuRatePercent keyed on $JobMaxConcurrent build SLOTS), same
    # "build"-flavored wording on the no-procgov warning.
    param(
        [string]$Exe,
        [string[]]$CmdArgs,
        [string]$WorkingDirectory,
        [string]$CaptureStdoutPath = '',
        [ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = 'BelowNormal',
        # Only used to size the job object's memory ceiling; the build-slot semaphore is what
        # actually bounds concurrency.
        [int]$JobMaxConcurrent = 2
    )
    $jobMemGB = Get-JobMemoryCapGB -MaxConcurrent $JobMaxConcurrent
    $cpuRate = Get-JobCpuRatePercent
    return Invoke-ProcessInJobObject -Exe $Exe -CmdArgs $CmdArgs -WorkingDirectory $WorkingDirectory `
        -CaptureStdoutPath $CaptureStdoutPath -Priority $Priority -JobMemoryGB $jobMemGB -CpuRatePercent $cpuRate -Subject 'build'
}

function Get-LiveWorktreeSlugs {
    # Slugs (leaf dir names) of every worktree `git worktree list` currently knows about --
    # anything under the cache root NOT in this set belongs to a worktree that's been deleted.
    (git worktree list --porcelain | Select-String '^worktree (.+)$').Matches |
        ForEach-Object { Split-Path $_.Groups[1].Value -Leaf }
}

function Remove-StaleTargetCaches {
    param([switch]$WhatIfOnly = $true)
    # Target-dirs can now live on either root (SSD when it had headroom at build time, HDD
    # otherwise), so both need sweeping -- a worktree deleted after it built on C: would
    # otherwise leak there forever.
    foreach ($root in @($script:SsdCacheRoot, $script:HddCacheRoot)) {
        if (-not (Test-Path $root)) { continue }
        $live = @(Get-LiveWorktreeSlugs)
        Get-ChildItem $root -Directory | Where-Object { $_.Name -ne 'sccache' -and $live -notcontains $_.Name } |
            ForEach-Object {
                $sizeGB = [math]::Round(((Get-ChildItem $_.FullName -Recurse -Force -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum) / 1GB, 2)
                if ($WhatIfOnly) {
                    Write-Host "[gc] would remove stale cache: $($_.FullName) (${sizeGB}GB) -- worktree no longer exists" -ForegroundColor Yellow
                } else {
                    Write-Host "[gc] removing stale cache: $($_.FullName) (${sizeGB}GB)" -ForegroundColor Yellow
                    Remove-Item -Recurse -Force $_.FullName
                }
            }
    }
}

function Get-ProcessSnapshot {
    # ONE CIM query, reused for every liveness decision below. Taken as a snapshot rather than
    # re-queried per process for a correctness reason, not a speed one: asking about processes one
    # at a time means the picture can change underneath a loop, so a build that starts mid-sweep
    # can be judged against a parent list that predates it.
    Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name, CommandLine, CreationDate
}

function Test-ParentAlive {
    # Is $Proc's parent genuinely still running? Two ways to get this wrong, and killing another
    # worktree's live build is the unacceptable one, so both are guarded:
    #
    # 1. Windows RECYCLES PIDs. A dead parent's PID can be reused by an unrelated new process, and
    #    a bare "does this PID exist" check then reports the orphan as parented and skips it. That
    #    direction is merely a missed reap. The dangerous direction is the same mechanism seen from
    #    the other side: any liveness test that can answer "dead" for a process whose parent is
    #    actually alive will kill work someone is waiting on. So a candidate parent is only
    #    accepted when it was created BEFORE the child -- a process that started later cannot be
    #    the thing that spawned it.
    # 2. The old check used `Get-Process -Id`, which reports failure for reasons other than "the
    #    process is gone" (access denied on a process owned by another session or elevated
    #    differently). "I could not look" was being read as "it is dead" -- exactly the false
    #    positive that reaps a healthy build running in another worktree. The CIM snapshot answers
    #    existence uniformly for every process on the machine.
    param($Proc, $Snapshot)
    $parent = $Snapshot | Where-Object { $_.ProcessId -eq $Proc.ParentProcessId } | Select-Object -First 1
    if (-not $parent) { return $false }
    if ($parent.CreationDate -and $Proc.CreationDate -and $parent.CreationDate -gt $Proc.CreationDate) {
        return $false   # recycled PID: this is not our parent
    }
    return $true
}

function Remove-OrphanedCargoProcesses {
    param([switch]$WhatIfOnly = $true, $Snapshot = $null)
    # An orphan here is a rustc/cargo/link/cc1 process whose parent is no longer live -- e.g. a
    # backgrounded shell that was killed or timed out without taking its child tree with it (the
    # POSIX process-group cleanup you'd expect doesn't happen by default on Windows).
    #
    # This sweep is machine-wide, so it can see builds belonging to OTHER worktrees. That is the
    # whole reason liveness is decided by Test-ParentAlive and never by process name, age, or CPU:
    # a healthy build in another worktree must be indistinguishable from untouchable. Being wrong
    # in the conservative direction leaves a stray process for the next gc to catch; being wrong in
    # the other direction destroys work someone is waiting on.
    if (-not $Snapshot) { $Snapshot = Get-ProcessSnapshot }
    $procs = $Snapshot | Where-Object { $_.Name -in @('rustc.exe', 'cargo.exe', 'link.exe', 'cc1.exe') }
    foreach ($p in $procs) {
        if (-not (Test-ParentAlive -Proc $p -Snapshot $Snapshot)) {
            if ($WhatIfOnly) {
                Write-Host "[gc] would kill orphan PID $($p.ProcessId) ($($p.Name), parent $($p.ParentProcessId) is dead)" -ForegroundColor Yellow
            } else {
                Write-Host "[gc] killing orphan PID $($p.ProcessId) ($($p.Name))" -ForegroundColor Yellow
                & taskkill /T /F /PID $p.ProcessId 2>$null | Out-Null
            }
        }
    }
}

# The ONLY process names this sweep may ever consider. Deliberately a named constant and not an
# inline list: the safety argument for reaping scanners rests entirely on no Rust build process
# appearing here, and that is easier to keep true when there is one place to check.
$script:ReapableScanNames = @('find.exe', 'rg.exe', 'grep.exe', 'findstr.exe')

function Test-ReapableScanProcess {
    # Pure decision, split out from the killing so the safety properties are testable without
    # spawning or terminating anything real (same reason the gc classification is a separate
    # function from Invoke-TargetGc). Returns $true only when ALL of:
    #   - the name is in $script:ReapableScanNames -- so a cargo/rustc/link belonging to any
    #     worktree can never be selected, whatever its age, CPU, or parentage;
    #   - the parent is genuinely gone (PID-reuse-safe, see Test-ParentAlive), meaning the output
    #     pipe has no reader and the work cannot be delivered to anyone;
    #   - it has burned real CPU and existed long enough that a just-launched scan is never caught.
    param(
        $Proc, $Snapshot, [int]$CpuSeconds,
        [int]$MinCpuSeconds = 60, [int]$MinAgeMinutes = 2,
        [datetime]$Now = (Get-Date)
    )
    if ($Proc.Name -notin $script:ReapableScanNames) { return $false }
    if (Test-ParentAlive -Proc $Proc -Snapshot $Snapshot) { return $false }
    if ($CpuSeconds -lt $MinCpuSeconds) { return $false }
    $ageMin = if ($Proc.CreationDate) { ($Now - $Proc.CreationDate).TotalMinutes } else { 0 }
    if ($ageMin -lt $MinAgeMinutes) { return $false }
    return $true
}

function Remove-OrphanedScanProcesses {
    param(
        [switch]$WhatIfOnly = $true,
        $Snapshot = $null,
        # Both thresholds must be crossed. They exist to make a false positive practically
        # impossible rather than to decide what is "expensive": a scan that is genuinely orphaned
        # AND has been burning a core for a minute is not one anybody is still reading.
        [int]$MinCpuSeconds = 60,
        [int]$MinAgeMinutes = 2
    )
    # Why this exists: measured on this machine, a single orphaned
    # `find / -iname rewrite.rs -path *foma*` ran for 35 minutes at Normal priority and consumed
    # 2110 CPU-seconds -- a saturated core plus continuous random I/O -- writing to a pipe whose
    # reader had already exited, so not one byte of it could ever be read. It survived because
    # Remove-OrphanedCargoProcesses only knows about compiler binaries.
    #
    # Scanners are worth reaping precisely BECAUSE of the constraint that makes reaping compilers
    # delicate. An orphaned rustc has at least produced object files on disk; an orphaned `find`
    # has produced nothing but a closed pipe, so there is no salvageable output to weigh against
    # killing it. And none of these names is ever a Rust build process, so this sweep cannot touch
    # another worktree's cargo/rustc/link no matter how the thresholds are tuned.
    if (-not $Snapshot) { $Snapshot = Get-ProcessSnapshot }
    $now = Get-Date
    foreach ($p in ($Snapshot | Where-Object { $_.Name -in $script:ReapableScanNames })) {
        $proc = Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue
        if (-not $proc) { continue }
        $cpu = [int]$proc.CPU
        if (-not (Test-ReapableScanProcess -Proc $p -Snapshot $Snapshot -CpuSeconds $cpu `
                    -MinCpuSeconds $MinCpuSeconds -MinAgeMinutes $MinAgeMinutes -Now $now)) { continue }
        $ageMin = if ($p.CreationDate) { ($now - $p.CreationDate).TotalMinutes } else { 0 }
        $cmd = if ($p.CommandLine) { $p.CommandLine.Substring(0, [Math]::Min(100, $p.CommandLine.Length)) } else { $p.Name }
        if ($WhatIfOnly) {
            Write-Host "[gc] would kill orphaned scan PID $($p.ProcessId) ($($p.Name), $([int]$ageMin)min, ${cpu}s CPU, parent dead): $cmd" -ForegroundColor Yellow
        } else {
            Write-Host "[gc] killing orphaned scan PID $($p.ProcessId) ($($p.Name), ${cpu}s CPU wasted): $cmd" -ForegroundColor Yellow
            & taskkill /T /F /PID $p.ProcessId 2>$null | Out-Null
        }
    }
}

function Get-LiveBuildProcesses {
    # gc's process check before it deletes anything: cargo/rustc/link/sccache all currently
    # running, orphaned or not (Remove-OrphanedCargoProcesses only cares about orphans; this is
    # broader on purpose -- a live, perfectly healthy build in another worktree is exactly the
    # thing gc must not race against).
    Get-CimInstance Win32_Process -Filter "Name='rustc.exe' or Name='cargo.exe' or Name='link.exe' or Name='sccache.exe'"
}

# =================================================================================================
# docs/superpowers/specs/2026-07-29-categorical-build-hardening-design.md, parts 2-4:
# distinct preflight exit codes, worktree base-commit contract, target ownership, sccache health,
# corpus-manifest validation, the one-line preflight record, and marker-aware gc classification.
# Consumed by rust/tools/pg.ps1; also exercised directly by rust/tools/tests/*.tests.ps1 so the
# decision logic is testable without a real build, a real drive, or a real git worktree registry.
# =================================================================================================

# One code per distinct preflight failure (design doc, "Error handling"): a caller (or a human
# reading a CI log) can tell "wrong commit" from "disk full" from "corpus missing" without parsing
# text. Picked to avoid colliding with cargo's own exit codes (101 on build failure, etc.) and with
# PowerShell's own reserved low range.
$script:ExitCodeWrongBase = 10
$script:ExitCodeMissingCorpus = 11
$script:ExitCodeLowDisk = 12
$script:ExitCodeCacheUnavailable = 13
$script:ExitCodeBadTargetOwnership = 14
$script:ExitCodeBuildSlotTimeout = 15
$script:ExitCodeZeroCorpusCases = 16
# Deliberately distinct from ExitCodeLowDisk: both are "the machine cannot take this run", but the
# recovery is completely different (free bytes on a drive vs. wait for / kill what is holding RAM),
# and a caller that cannot tell them apart will run gc at a memory problem and conclude gc is broken.
$script:ExitCodeLowMemory = 17

# ---------------------------------------------------------------------------------------------
# Worktree metadata: the exact-base contract
# ---------------------------------------------------------------------------------------------

function Get-WorktreeMetaPath {
    # Gitignored (see rust/tools -- top-level .gitignore entry added alongside this function):
    # per-worktree, machine-local record of what commit this worktree was BUILT FROM, not
    # something to commit or share. Lives at the worktree root (not under rust/) so it is
    # unambiguously one file per worktree even for repos that grow a second Cargo workspace later.
    param([string]$RepoRoot = (Get-RepoRoot))
    return Join-Path $RepoRoot '.pangloss-worktree.json'
}

function Write-WorktreeMeta {
    # Called by the worktree bootstrap command at creation time, once, with the revision it was
    # asked to create from (both as typed -- $RequestedRevision, e.g. a branch name or short
    # SHA -- and as resolved to a full object ID). Recording BOTH is what lets a later mismatch
    # report be useful: "you asked for main, main has since moved, you're still on <object id>"
    # is a diagnosable message; recording only one of the two loses half of that.
    param(
        [Parameter(Mandatory)][string]$RepoRoot,
        [Parameter(Mandatory)][string]$RequestedRevision,
        [Parameter(Mandatory)][string]$ResolvedObjectId,
        [string]$Branch = '',
        [string]$CorpusPolicy = 'local-samples-data',
        [string]$ManagedTarget = ''
    )
    $meta = [ordered]@{
        schema_version     = 1
        repository_id      = Get-RepoIdentity -RepoRoot $RepoRoot
        requested_revision = $RequestedRevision
        resolved_object_id = $ResolvedObjectId
        created_utc        = (Get-Date).ToUniversalTime().ToString('o')
        worktree_path      = $RepoRoot
        branch             = $Branch
        corpus_policy      = $CorpusPolicy
        managed_target     = $ManagedTarget
    }
    $path = Get-WorktreeMetaPath -RepoRoot $RepoRoot
    ($meta | ConvertTo-Json -Depth 4) | Set-Content -Path $path -Encoding utf8
    return [PSCustomObject]$meta
}

function Read-WorktreeMeta {
    # Absence is the COMMON case (primary checkout, every worktree created before this change)
    # and must not be an error -- callers (Test-WorktreeBase) treat $null as "unverified", never
    # as a failure. A corrupt/partially-written file is folded into the same $null return rather
    # than thrown, for the same reason: a preflight check must not itself crash the build.
    param([string]$RepoRoot = (Get-RepoRoot))
    $path = Get-WorktreeMetaPath -RepoRoot $RepoRoot
    if (-not (Test-Path $path)) { return $null }
    try {
        return Get-Content $path -Raw | ConvertFrom-Json
    } catch {
        return $null
    }
}

function Test-WorktreeBase {
    # strict: for read-only assessment tasks where ANY drift from the recorded base -- even a
    # clean fast-forward -- means "this isn't the snapshot you asked about."
    # development: for ordinary work, where new commits on top of the recorded base are exactly
    # what's supposed to happen; the thing that must NOT happen is the recorded base being rewound
    # or rebased out of history entirely (git merge-base --is-ancestor catches that; a plain HEAD
    # equality check would not, since it would also reject perfectly normal forward progress).
    # off: explicit opt-out, e.g. `pg.ps1 doctor` runs against a worktree nobody has bootstrapped.
    #
    # Absent metadata is reported as Checked=$false, Ok=$true ("unverified"), never as a failure --
    # see Read-WorktreeMeta's doc for why. This function never checks out or rebases anything: the
    # design doc is explicit that either action can silently discard context or invalidate a build
    # cache the caller was relying on.
    param(
        [ValidateSet('strict', 'development', 'off')][string]$Mode = 'development',
        [string]$RepoRoot = (Get-RepoRoot)
    )
    $head = git -C $RepoRoot rev-parse HEAD 2>$null
    $result = [ordered]@{
        Checked  = $true
        Ok       = $true
        Detail   = ''
        Expected = $null
        Actual   = $head
    }
    if ($Mode -eq 'off') {
        $result.Checked = $false
        $result.Detail = 'base check disabled (-BaseMode off)'
        return [PSCustomObject]$result
    }
    $meta = Read-WorktreeMeta -RepoRoot $RepoRoot
    if (-not $meta -or -not $meta.resolved_object_id) {
        $result.Checked = $false
        $result.Detail = 'no recorded base (worktree predates metadata, or metadata unreadable) -- unverified'
        return [PSCustomObject]$result
    }
    $result.Expected = $meta.resolved_object_id
    if ($Mode -eq 'strict') {
        $result.Ok = ($head -eq $meta.resolved_object_id)
        $result.Detail = if ($result.Ok) {
            "HEAD matches recorded base exactly ($head)"
        } else {
            "HEAD ($head) != recorded base ($($meta.resolved_object_id))"
        }
        return [PSCustomObject]$result
    }
    & git -C $RepoRoot merge-base --is-ancestor $meta.resolved_object_id $head 2>$null
    $isAncestor = ($LASTEXITCODE -eq 0)
    $result.Ok = $isAncestor
    $result.Detail = if ($isAncestor) {
        "recorded base ($($meta.resolved_object_id)) is an ancestor of HEAD ($head)"
    } else {
        "recorded base ($($meta.resolved_object_id)) is NOT an ancestor of HEAD ($head) -- history diverged from the recorded base"
    }
    return [PSCustomObject]$result
}

# ---------------------------------------------------------------------------------------------
# Target ownership
# ---------------------------------------------------------------------------------------------

function Get-TargetOwnershipPath {
    param([Parameter(Mandatory)][string]$TargetDir)
    return Join-Path $TargetDir '.pangloss-owner.json'
}

function Write-TargetOwnership {
    # Target dirs are keyed by worktree SLUG (leaf directory name), not by an absolute path or the
    # repository identity -- so two independent clones of this repo (or, in principle, of a
    # DIFFERENT repo that happens to check out into a same-named leaf directory) can collide on
    # the same cache-root subdirectory. Refuse to silently adopt a target dir whose marker names a
    # different repository_id: reusing it would mix one repo's build artifacts under another's
    # ownership record, and a later gc run keyed on "matches this repository" would then be wrong
    # in either direction (deleting it, or refusing to delete it, for the wrong reason).
    param(
        [Parameter(Mandatory)][string]$TargetDir,
        [Parameter(Mandatory)][string]$RepositoryId,
        [Parameter(Mandatory)][string]$WorktreePath,
        [switch]$Preserved
    )
    $path = Get-TargetOwnershipPath -TargetDir $TargetDir
    $createdUtc = (Get-Date).ToUniversalTime().ToString('o')
    if (Test-Path $path) {
        try {
            $existing = Get-Content $path -Raw | ConvertFrom-Json
        } catch {
            $existing = $null
        }
        if ($existing -and $existing.repository_id -and $existing.repository_id -ne $RepositoryId) {
            return [PSCustomObject]@{
                Ok     = $false
                Detail = "target dir owner mismatch: marker names repository_id '$($existing.repository_id)', this build is repository_id '$RepositoryId' -- refusing to reuse $TargetDir"
                Path   = $path
            }
        }
        # created_utc survives every rewrite of an already-owned marker -- it is the target
        # dir's age, not this invocation's start time; only last_used_utc should move on a rewrite.
        if ($existing -and $existing.created_utc) { $createdUtc = $existing.created_utc }
    }
    # preserved is monotonic: an ordinary build/test call (no -Preserved) must not silently clear
    # a `preserved` flag a prior `-Mode release` run set on this same target dir. There is no
    # un-preserve path here on purpose -- nothing in this design calls for one.
    #
    # Local var deliberately named $isPreserved, NOT $preserved: PowerShell variable names are
    # CASE-INSENSITIVE, so a local `$preserved` would silently be the exact same variable as the
    # `-Preserved` switch parameter above it, and assigning `$preserved = $false` here would wipe
    # out the caller's switch value before the `if ($Preserved)` check below even ran. Caught by
    # hand-inspecting a marker this wrote for real, which had `preserved` serialized as
    # `{"IsPresent": ...}` (the raw SwitchParameter object) instead of a JSON boolean.
    $isPreserved = $false
    if ($Preserved) { $isPreserved = $true }
    if ($existing -and $existing.preserved) { $isPreserved = $true }
    $marker = [ordered]@{
        schema_version = 1
        repository_id  = $RepositoryId
        worktree_path  = $WorktreePath
        created_utc    = $createdUtc
        last_used_utc  = (Get-Date).ToUniversalTime().ToString('o')
        preserved      = $isPreserved
    }
    ($marker | ConvertTo-Json -Depth 4) | Set-Content -Path $path -Encoding utf8
    return [PSCustomObject]@{ Ok = $true; Detail = 'ownership marker written'; Path = $path }
}

# ---------------------------------------------------------------------------------------------
# sccache health
# ---------------------------------------------------------------------------------------------

function Test-SccacheHealth {
    # Three states, not two: "not installed" is a normal, expected local-dev situation (falls back
    # to an uncached build); "installed but --show-stats fails" means something IS on PATH named
    # sccache but can't actually talk to its cache (bad SCCACHE_DIR permissions, a stale/corrupt
    # cache, a wrapped compiler mismatch) -- that's the state the design doc says must FAIL the
    # build rather than silently proceed uncached, because a silent fallback there is exactly how
    # "sccache active" claims in a build log stop being trustworthy.
    if (-not (Get-Command sccache -ErrorAction SilentlyContinue)) {
        return [PSCustomObject]@{ State = 'not-installed'; Ok = $false; Detail = 'sccache not found on PATH' }
    }
    $stats = & sccache --show-stats 2>&1
    if ($LASTEXITCODE -ne 0) {
        return [PSCustomObject]@{
            State  = 'unhealthy'
            Ok     = $false
            Detail = "sccache --show-stats exited $($LASTEXITCODE): $($stats -join ' | ')"
        }
    }
    # Summarize, don't dump. `--show-stats` prints ~40 lines; splicing all of it into the preflight
    # record buried every other preflight field (base check, target dir, corpus digests) in stats
    # noise, which defeats the point of having a record you actually read. Keep the few numbers that
    # say the cache is working and leave `sccache --show-stats` for when you want the rest.
    # Out-String rather than -join so an ErrorRecord (some sccache builds write to stderr, which
    # `2>&1` captures as objects, not strings) still stringifies the way the patterns below expect.
    #
    # Every field below is OPTIONAL on purpose. A freshly started sccache server reports zero compile
    # requests, prints its hit rate as a placeholder rather than a number, and omits the cache-size
    # block entirely -- so "responding" with no numbers is the correct, expected output there, not a
    # parse failure. Checked: a warm server on the same machine yields
    # "responding, hit rate 0.71 %, cache 1 GiB / 10 GiB".
    $text = ($stats | Out-String)
    $hitRate = ([regex]::Match($text, 'Cache hits rate\s+([\d.]+\s*%)')).Groups[1].Value
    $size = ([regex]::Match($text, 'Cache size\s+(.+)')).Groups[1].Value.Trim()
    $maxSize = ([regex]::Match($text, 'Max cache size\s+(.+)')).Groups[1].Value.Trim()
    $summary = 'responding'
    if ($hitRate) { $summary += ", hit rate $hitRate" }
    if ($size -and $maxSize) { $summary += ", cache $size / $maxSize" }
    return [PSCustomObject]@{ State = 'healthy'; Ok = $true; Detail = $summary; RawStats = $text }
}

# ---------------------------------------------------------------------------------------------
# Corpus manifest (mirrors pg_conformance_fixtures::corpus's Rust-side reader so the PowerShell
# front end and the Rust test helpers agree on what "present" and "required" mean)
# ---------------------------------------------------------------------------------------------

function Get-CorpusManifest {
    param([string]$RepoRoot = (Get-RepoRoot))
    $path = Join-Path $RepoRoot 'rust\tools\corpus-manifest.json'
    if (-not (Test-Path $path)) { throw "corpus manifest not found: $path" }
    return Get-Content $path -Raw | ConvertFrom-Json
}

function Get-CorpusRoot {
    # PANGLOSS_CORPUS_ROOT overrides the manifest's own corpus_root, exactly like
    # pg_conformance_fixtures::corpus::corpus_root() on the Rust side -- a linked worktree can
    # point this at an external corpus location instead of copying gigabytes of private data per
    # worktree.
    param([string]$RepoRoot = (Get-RepoRoot), $Manifest)
    if ($env:PANGLOSS_CORPUS_ROOT) { return $env:PANGLOSS_CORPUS_ROOT }
    if (-not $Manifest) { $Manifest = Get-CorpusManifest -RepoRoot $RepoRoot }
    return Join-Path $RepoRoot ($Manifest.corpus_root -replace '/', '\')
}

function Test-CorpusPresent {
    # Validates every REQUIRED manifest file before cargo starts (design doc: "It validates every
    # requested file before Cargo starts"). Digests are truncated (first 12 hex chars of SHA-256)
    # -- enough to catch "this isn't the file you think it is" across machines/runs without
    # printing a full 64-char hash into every build log.
    param(
        [string]$RepoRoot = (Get-RepoRoot),
        $Manifest,
        [string]$CorpusRoot
    )
    if (-not $Manifest) { $Manifest = Get-CorpusManifest -RepoRoot $RepoRoot }
    if (-not $CorpusRoot) { $CorpusRoot = Get-CorpusRoot -RepoRoot $RepoRoot -Manifest $Manifest }
    $missing = @()
    $present = @()
    foreach ($corpus in $Manifest.corpora) {
        foreach ($file in $corpus.files) {
            $full = Join-Path $CorpusRoot $file.path
            if (Test-Path $full -PathType Leaf) {
                $bytes = (Get-Item $full).Length
                $hash = (Get-FileHash -Path $full -Algorithm SHA256).Hash.Substring(0, 12).ToLower()
                $present += [PSCustomObject]@{
                    Logical     = $corpus.logical_name
                    Path        = $file.path
                    Bytes       = $bytes
                    Sha256Short = $hash
                }
            } elseif ($file.required) {
                $missing += "$($corpus.logical_name):$($file.path)"
            }
        }
    }
    return [PSCustomObject]@{
        Ok         = ($missing.Count -eq 0)
        Missing    = $missing
        Present    = $present
        CorpusRoot = $CorpusRoot
    }
}

# ---------------------------------------------------------------------------------------------
# Disk reserve (pure decision logic -- takes a free-space number rather than querying a drive
# itself, so it's unit-testable without touching a real disk)
# ---------------------------------------------------------------------------------------------

function Test-DiskReserve {
    # This is deliberately a SEPARATE, lower bar from Resolve-TargetDir's SSD/HDD selection
    # reserve (default 50GB, "prefer NVMe while it has headroom"): that one is a placement
    # preference, not a safety gate, and a build should not hard-fail just because the SSD alone
    # dipped below its preference threshold when the HDD fallback is fine. This is the last-resort
    # "the chosen target dir's own drive is nearly full" check that must reject the build outright
    # -- the 1.3GB-free crisis the whole design doc opens with.
    # [Nullable[double]], NOT [double]: a plain [double] parameter silently coerces a passed $null
    # into 0.0 rather than keeping it null, which would make the "free space unknown" case
    # indistinguishable from "0GB free" and wrongly fail the build. Caught by a test asserting
    # $null -FreeGB is non-blocking, which failed until this type was fixed.
    param(
        [Nullable[double]]$FreeGB,
        [double]$MinFreeGB = 5
    )
    if ($null -eq $FreeGB) {
        return [PSCustomObject]@{ Ok = $true; Detail = 'free space unknown (drive not queryable) -- not blocking on it'; FreeGB = $null }
    }
    $ok = $FreeGB -ge $MinFreeGB
    return [PSCustomObject]@{
        Ok     = $ok
        Detail = if ($ok) {
            "${FreeGB}GB free (>= ${MinFreeGB}GB reserve)"
        } else {
            "${FreeGB}GB free (< ${MinFreeGB}GB reserve) -- refusing to start a build that could fill the drive"
        }
        FreeGB = $FreeGB
    }
}

# ---------------------------------------------------------------------------------------------
# Preflight record
# ---------------------------------------------------------------------------------------------

function Write-Preflight {
    # One record, printed before cargo starts, naming everything an agent or a human would
    # otherwise have to reconstruct after the fact from a build log: worktree, commit, target
    # dir, cache state, corpus state, disk state, and build slot (design doc goal 6).
    param(
        [string]$Mode,
        [string]$Profile,
        [string]$RepoRoot,
        [string]$TargetDir,
        [Parameter(Mandatory)]$BaseCheck,
        [Parameter(Mandatory)]$SccacheHealth,
        [Nullable[double]]$FreeGB,
        $DiskCheck,
        $MemoryCheck = $null,
        $CorpusState = $null,
        [int]$MaxConcurrent,
        [int]$Jobs = 0,
        [switch]$JobsExplicit,
        $JobsBudget = $null,
        [double]$PerJobMemoryGB = 0,
        [int]$TestThreads = 0,
        $TestThreadsBudget = $null,
        [string]$Priority = ''
    )
    $slug = Get-WorktreeSlug -RustRoot (Join-Path $RepoRoot 'rust')
    $head = git -C $RepoRoot rev-parse HEAD 2>$null
    $dirty = @(git -C $RepoRoot status --porcelain 2>$null).Count
    Write-Host '----- pg preflight -----' -ForegroundColor Cyan
    Write-Host "mode: $Mode  profile: $Profile"
    Write-Host "worktree: $RepoRoot  (slug: $slug)"
    Write-Host "HEAD: $head  dirty files: $dirty"
    $baseColor = if (-not $BaseCheck.Checked) { 'Yellow' } elseif ($BaseCheck.Ok) { 'Green' } else { 'Red' }
    Write-Host "base check ($($BaseCheck.Checked ? 'checked' : 'unverified')): $($BaseCheck.Detail)" -ForegroundColor $baseColor
    if ($BaseCheck.Checked -and -not $BaseCheck.Ok) {
        Write-Host "  expected: $($BaseCheck.Expected)" -ForegroundColor Red
        Write-Host "  actual:   $($BaseCheck.Actual)" -ForegroundColor Red
    }
    Write-Host "target dir: $(if ($TargetDir) { $TargetDir } else { '<cargo default>' })"
    if ($DiskCheck) {
        Write-Host "free space: $($DiskCheck.Detail)" -ForegroundColor $(if ($DiskCheck.Ok) { 'Gray' } else { 'Red' })
    }
    if ($MemoryCheck) {
        $total = Get-TotalMemoryGB
        $ofTotal = if ($null -ne $total) { " of ${total}GB total" } else { '' }
        Write-Host "free memory: $($MemoryCheck.Detail)$ofTotal" -ForegroundColor $(if ($MemoryCheck.Ok) { 'Gray' } else { 'Red' })
        # Commit charge alongside it, never instead of it -- see Get-CommitChargeGB for why the two
        # numbers diverge precisely when a failure is happening (a fork can fail on MEM_COMMIT while
        # tens of GB of physical memory read as available).
        $commit = Get-CommitChargeGB
        if ($commit) {
            $commitColor = if ($commit.PercentUsed -ge 90) { 'Red' } elseif ($commit.PercentUsed -ge 75) { 'Yellow' } else { 'Gray' }
            Write-Host "commit charge: $($commit.CommittedGB)GB of $($commit.LimitGB)GB limit ($($commit.PercentUsed)% used, $($commit.FreeGB)GB uncommitted) -- procgov's --maxjobmem and event-2004 both measure THIS, not available physical" -ForegroundColor $commitColor
        }
    }
    # Who holds the machine-wide build slots. Diagnostic only (the mutexes are the real exclusion);
    # printed because an anonymous wait is indistinguishable from a deadlock.
    $slotHolders = @(Get-BuildSlotHolders)
    if ($slotHolders.Count -gt 0) {
        Write-Host 'build slots in use:'
        foreach ($h in $slotHolders) {
            $state = if ($h.Alive) { "alive since $($h.AcquiredAt)" } else { 'NOT ALIVE -- stale ledger entry; the kernel hands this slot to the next waiter' }
            Write-Host "  slot $($h.Slot): pid $($h.Pid) ($($h.Mode) in $($h.Worktree)) -- $state" -ForegroundColor $(if ($h.Alive) { 'Gray' } else { 'Yellow' })
        }
    }
    Write-Host "sccache: $($SccacheHealth.State) -- $($SccacheHealth.Detail)" -ForegroundColor $(if ($SccacheHealth.Ok -or $SccacheHealth.State -eq 'disabled') { 'Gray' } else { 'Red' })
    if ($CorpusState) {
        if ($CorpusState.Ok) {
            Write-Host "corpus: present ($($CorpusState.Present.Count) file(s), root $($CorpusState.CorpusRoot))"
            foreach ($p in $CorpusState.Present) {
                Write-Host "  $($p.Logical):$($p.Path)  $($p.Bytes) bytes  sha256:$($p.Sha256Short)"
            }
        } else {
            Write-Host 'corpus: MISSING required file(s):' -ForegroundColor Red
            foreach ($m in $CorpusState.Missing) { Write-Host "  $m" -ForegroundColor Red }
        }
    }
    Write-Host "build slot limit: $MaxConcurrent (machine-wide convention -- see Enter-BuildSlot)"
    if ($Jobs -gt 0) {
        # Printed with its provenance, not just the number: the useful fact when a build feels slow
        # is WHY it is 7 and not 20, and that the reserve is what keeps SSH/remote desktop alive.
        # The derivation is only shown when the number actually came FROM it -- printing
        # "20 logical - 6 reserved, split across 2" next to an explicit `-Jobs 3` states arithmetic
        # that did not happen and cannot produce the value shown beside it.
        $why = if ($JobsExplicit) {
            'explicit -Jobs override'
        } elseif ($JobsBudget -and $JobsBudget.Bound -eq 'memory') {
            # The number that answers "why is this slower than I expected" is different once memory
            # can bind it: the CPU derivation is still true arithmetic but no longer the reason.
            $perJob = if ($PerJobMemoryGB -gt 0) { $PerJobMemoryGB } else { $script:MemoryPerCompileJobGB }
            $ltoNote = if ($perJob -eq $script:MemoryPerLtoLinkJobGB) { ' (fat-LTO link peak)' } else { '' }
            "$($JobsBudget.Detail); ${perJob}GB/job assumed${ltoNote} over a $(Get-InteractiveReserveGB)GB reserve, split across $MaxConcurrent slot(s)"
        } else {
            "$([Environment]::ProcessorCount) logical - $script:InteractiveReserveThreads reserved for SSH/remote-desktop daemons, split across $MaxConcurrent slot(s)"
        }
        Write-Host "cargo jobs: $Jobs per build ($why)"
    }
    if ($TestThreads -gt 0) {
        # Reported separately from jobs because they bound different phases, and a run capped for
        # compilation but not execution looks capped in the log while still going 20-wide in the
        # half that spawns real processes.
        $testWhy = if ($TestThreadsBudget -and $TestThreadsBudget.Bound -eq 'memory') {
            " -- $($TestThreadsBudget.Detail), ${script:MemoryPerTestProcessGB}GB/process assumed"
        } else {
            ''
        }
        Write-Host "test threads: $TestThreads concurrent test processes (default would be $([Environment]::ProcessorCount))$testWhy"
    }
    if ($Priority) {
        Write-Host "build priority: $Priority (inherited by rustc/link.exe -- keeps interactive daemons ahead of compiler work)"
    }
    Write-Host '-------------------------' -ForegroundColor Cyan
}

# ---------------------------------------------------------------------------------------------
# gc: marker-aware classification + the actual (side-effecting) deletion step, kept separate so
# the decision of WHAT to delete is unit-testable without ever calling Remove-Item.
# ---------------------------------------------------------------------------------------------

function Get-ManagedTargetDirs {
    # -Roots is a parameter (not always $script:SsdCacheRoot/$script:HddCacheRoot) so tests can
    # point this at a temp directory instead of the real cache roots -- gc's own tests must never
    # require C:\cargo-targets or G:\cargo-build-cache to exist, let alone touch them.
    param([Parameter(Mandatory)][string[]]$Roots)
    foreach ($root in $Roots) {
        if (-not (Test-Path $root)) { continue }
        Get-ChildItem $root -Directory -ErrorAction SilentlyContinue | Where-Object { $_.Name -ne 'sccache' }
    }
}

function Get-TargetClassification {
    # Five classes, only one of which gc may ever delete:
    #  - unknown:     no ownership marker at all. Never deleted -- an unmarked directory could be
    #                  anything (a manual experiment, a tool this design doesn't know about); the
    #                  design doc is explicit that gc must never guess here.
    #  - other-repo:   marker names a DIFFERENT repository_id. Not this repo's to touch.
    #  - preserved:    marker's `preserved` flag is set (an explicitly registered release
    #                  deliverable). Never deleted.
    #  - live:         owned by this repo, not preserved, but its slug still appears in
    #                  `git worktree list` -- the worktree that owns it still exists, so this is
    #                  someone's active target, not stale.
    #  - disposable:   owned by this repo, not preserved, and its worktree is gone. The only class
    #                  Invoke-TargetGc will ever remove.
    #
    # -LiveSlugs is likewise a parameter (default calls the real Get-LiveWorktreeSlugs) so tests
    # can inject a fixed slug list instead of depending on this checkout's actual
    # `git worktree list` output.
    param(
        [Parameter(Mandatory)][string]$RepositoryId,
        [string[]]$Roots = @($script:SsdCacheRoot, $script:HddCacheRoot),
        [string[]]$LiveSlugs
    )
    if ($null -eq $LiveSlugs) { $LiveSlugs = @(Get-LiveWorktreeSlugs) }
    $out = @()
    foreach ($d in (Get-ManagedTargetDirs -Roots $Roots)) {
        $sizeGB = [math]::Round(((Get-ChildItem $d.FullName -Recurse -Force -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum) / 1GB, 2)
        $markerPath = Get-TargetOwnershipPath -TargetDir $d.FullName
        if (-not (Test-Path $markerPath)) {
            $out += [PSCustomObject]@{ Path = $d.FullName; Class = 'unknown'; SizeGB = $sizeGB; Detail = 'no ownership marker -- gc never touches unmarked directories' }
            continue
        }
        try {
            $marker = Get-Content $markerPath -Raw | ConvertFrom-Json
        } catch {
            $out += [PSCustomObject]@{ Path = $d.FullName; Class = 'unknown'; SizeGB = $sizeGB; Detail = 'unreadable ownership marker -- treated as unmarked' }
            continue
        }
        if ($marker.repository_id -ne $RepositoryId) {
            $out += [PSCustomObject]@{ Path = $d.FullName; Class = 'other-repo'; SizeGB = $sizeGB; Detail = "owned by a different repository ($($marker.repository_id))" }
            continue
        }
        if ($marker.preserved) {
            $out += [PSCustomObject]@{ Path = $d.FullName; Class = 'preserved'; SizeGB = $sizeGB; Detail = 'explicitly preserved (release deliverable)' }
            continue
        }
        if ($LiveSlugs -contains $d.Name) {
            $out += [PSCustomObject]@{ Path = $d.FullName; Class = 'live'; SizeGB = $sizeGB; Detail = 'worktree still exists in `git worktree list`' }
            continue
        }
        $out += [PSCustomObject]@{ Path = $d.FullName; Class = 'disposable'; SizeGB = $sizeGB; Detail = 'owned by this repository, not preserved, worktree no longer exists' }
    }
    return $out
}

function Invoke-TargetGc {
    # The only function in this file allowed to delete a managed target directory. Dry-run
    # (-Apply not passed) is the default and NEVER deletes anything, matching the design doc's
    # "the first gc run is dry-run only" migration note -- $Apply defaults to $false here on
    # purpose, not just at the pg.ps1 call site, so a test (or a future caller) that forgets to
    # pass it explicitly fails safe.
    param(
        [Parameter(Mandatory)][object[]]$Classification,
        [switch]$Apply,
        [object[]]$BusyProcesses = @(),
        # The roots a deletion is allowed to touch. Defaults to the same two the classifier
        # enumerates, so the containment re-check below compares against the same boundary the
        # caller reasoned about; a test can narrow it to a temp dir.
        [string[]]$Roots = @($script:SsdCacheRoot, $script:HddCacheRoot)
    )
    $disposable = @($Classification | Where-Object { $_.Class -eq 'disposable' })
    $result = [ordered]@{
        Disposable = $disposable
        Deleted    = @()
        Skipped    = $false
        SkipReason = ''
    }
    if (-not $Apply) {
        $result.Skipped = $true
        $result.SkipReason = 'dry run (-Apply not passed) -- nothing deleted'
        return [PSCustomObject]$result
    }
    if ($BusyProcesses.Count -gt 0) {
        # A live cargo/rustc/link/sccache process anywhere on the machine is reason enough to
        # abstain entirely rather than try to reason about which specific target dir it's using --
        # gc runs rarely enough that "try again once nothing is building" costs nothing, whereas
        # deleting a target a live build is mid-write to is a build-breaking race.
        $result.Skipped = $true
        $result.SkipReason = "refusing to delete: $($BusyProcesses.Count) live cargo/rustc/link/sccache process(es) running"
        return [PSCustomObject]$result
    }
    foreach ($d in $disposable) {
        # Re-validate containment at the moment of deletion, not only at classification time (design
        # doc: "It resolves and validates each absolute target before deletion"). Today's callers all
        # build $Classification from Get-ManagedTargetDirs, which only ever enumerates under $Roots,
        # so this cannot currently fire -- which is exactly why it is cheap insurance rather than
        # redundant: this is the one function in the repo that recursively force-deletes directories,
        # and the next caller to hand it a hand-built or re-normalized list is where an escape would
        # otherwise happen silently. Resolve first so `..` or a symlink cannot smuggle a path out of a
        # root that a plain string-prefix test would accept.
        $resolved = (Resolve-Path -LiteralPath $d.Path -ErrorAction SilentlyContinue)
        if (-not $resolved) {
            $result.SkipReason = "skipped $($d.Path): no longer resolvable"
            continue
        }
        $full = $resolved.ProviderPath
        $contained = $false
        foreach ($root in $Roots) {
            $rootResolved = (Resolve-Path -LiteralPath $root -ErrorAction SilentlyContinue)
            if (-not $rootResolved) { continue }
            $rootFull = $rootResolved.ProviderPath.TrimEnd('\')
            # Compare against "<root>\" so a sibling root sharing a name prefix (C:\cargo-targets vs
            # C:\cargo-targets-old) can never be mistaken for being inside this one.
            if ($full.StartsWith($rootFull + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
                $contained = $true
                break
            }
        }
        if (-not $contained) {
            throw "refusing to delete '$full': not contained in any configured cache root ($($Roots -join ', ')). This guards against a caller handing Invoke-TargetGc a path it did not enumerate."
        }
        Remove-Item -Recurse -Force -LiteralPath $full
        $result.Deleted += $d.Path
    }
    return [PSCustomObject]$result
}
