<#
  .DESCRIPTION
  Shared helpers for build.ps1 / test.ps1 / pg.ps1: worktree/path resolution, disk- and memory-aware
  target-dir redirection, sccache wiring, a cross-worktree build-concurrency gate, kernel-enforced
  (procgov) resource ceilings, worktree-scoped cleanup of orphaned processes and stale build caches,
  and the preflight surface (exit codes, worktree base-commit contract, target ownership, corpus
  manifest validation, conformance-submodule auto-init) that pg.ps1 gates a run on.

  Dot-source from build.ps1/test.ps1/pg.ps1: . "$PSScriptRoot\_common.ps1"

  Full design rationale for the resource-governance mechanisms below (target-dir SSD/HDD placement,
  the CPU/memory reserve model, per-job memory allowances, procgov integration, the build-slot mutex
  design, orphan reaping, the conformance-submodule sparse checkout) is consolidated in
  docs/research/build-resource-governance.md, alongside the measured incidents in the repo's own
  CLAUDE.md that motivated each one. Comments in this file point there rather than re-deriving the
  argument at every call site.

  Worktree base-commit contract (Test-WorktreeBase / Write-WorktreeMeta): every worktree records the
  commit it was created from in a gitignored `.pangloss-worktree.json` at its root. `-BaseMode`
  strict rejects ANY drift from that recorded commit, even a clean fast-forward -- for read-only
  assessment tasks where "this isn't the snapshot you asked about" must fail loudly.
  `development` (the default) accepts new commits on top of the recorded base -- ordinary work -- but
  rejects the base having been rewound or rebased out of history entirely (`git merge-base
  --is-ancestor`, not a HEAD-equality check, which would also reject normal forward progress). `off`
  is an explicit opt-out for a worktree nobody has bootstrapped. Absent metadata is always
  "unverified", never a failure: it predates this contract or the file is unreadable, and a preflight
  check must never crash a build over its own diagnostic.

  Target ownership markers (Write-TargetOwnership, `.pangloss-owner.json` inside a managed target
  dir): identify which repository and worktree a shared-cache target dir belongs to, keyed by
  worktree SLUG (leaf directory name) rather than an absolute path or repository identity, because
  cache roots are shared across independent clones. A marker naming a different repository_id is
  refused rather than silently adopted, since reusing it would mix build artifacts across repos.
  `preserved` is monotonic once set (an explicit release deliverable) -- an ordinary build/test call
  never clears it. Get-TargetClassification sorts every managed target dir under the configured
  roots into exactly one of five classes -- unknown (no marker), other-repo, preserved, live (marker's
  worktree still exists), disposable (this repo's, not preserved, worktree gone) -- and
  Invoke-TargetGc (`pg.ps1 -Mode gc -Apply`) ever deletes only the last one, and only when no
  cargo/rustc/link/sccache process is running anywhere on the machine.

  Preflight exit codes (10-19), one per distinct failure so a caller can branch without parsing text:
  10 wrong worktree base, 11 missing corpus file, 12 low disk, 13 sccache unavailable, 14 bad target
  ownership, 15 build-slot wait timeout, 16 zero corpus cases executed, 17 low memory, 18 conformance
  submodule missing and could not auto-init, 19 the invoked script and the caller's cwd resolve to
  different worktrees. Picked to avoid colliding with cargo's own exit codes (101 on build failure)
  and PowerShell's reserved low range.

  Exit code 19 (Assert-ScriptAndCwdAgreeOnWorktree) deserves special caution: `Get-RepoRoot` resolves
  via `git rev-parse --show-toplevel`, which answers for whichever worktree the CALLER IS STANDING IN,
  so `pwsh -File <worktreeA>\rust\tools\pg.ps1` run from worktreeB silently builds and tests B while
  every visible part of the command names A. This fails in the *reassuring* direction -- the run
  passes, the command text names the tree you meant -- so it reads as "I DID look" and can silently
  void a completed verification. Agents are especially exposed, since a shell's cwd persists across
  tool calls and a `Set-Location` many calls earlier retargets everything after it. The guard refuses
  rather than silently preferring `$PSScriptRoot`'s own tree: a caller who genuinely wants worktree B
  should invoke B's own copy of the script.
#>

$ErrorActionPreference = 'Stop'

# SSD preferred for an active target-dir (scattered small-file I/O); HDD for sccache's cache (blob reads).
# docs/research/build-resource-governance.md
$script:SsdCacheRoot = if ($env:PANGLOSS_SSD_CACHE_ROOT) { $env:PANGLOSS_SSD_CACHE_ROOT } else { 'C:\cargo-targets' }
$script:HddCacheRoot = if ($env:PANGLOSS_CARGO_CACHE_ROOT) { $env:PANGLOSS_CARGO_CACHE_ROOT } else { 'G:\cargo-build-cache' }
# The guard against refilling the disk-space crisis that motivated moving target-dirs off C: at all.
$script:MinFreeGBOnSsd = if ($env:PANGLOSS_MIN_FREE_SSD_GB) { [double]$env:PANGLOSS_MIN_FREE_SSD_GB } else { 50 }
$script:BuildSemaphoreName = 'Global\PanGlossCargoBuild'

function Import-PanGlossPlatformAdapter {
    # Installs the platform-native seam functions (global scope, since a dot-source inside a function would discard them on return); -Platform exists for fixture tests, and load-time dispatch only selects Linux on a real Linux host.
    param(
        [ValidateSet('Windows', 'Linux')][string]$Platform = $(if ($IsLinux) { 'Linux' } else { 'Windows' }),
        [string]$ToolRoot = $PSScriptRoot
    )
    if ($Platform -eq 'Windows') {
        $global:PanGlossPlatformAdapter = [PSCustomObject]@{ Platform = 'Windows'; Overrides = @() }
        return $global:PanGlossPlatformAdapter
    }
    $adapterPath = Join-Path $ToolRoot '_platform_linux.ps1'
    if (-not (Test-Path -LiteralPath $adapterPath -PathType Leaf)) {
        throw "Linux platform adapter not found: $adapterPath"
    }
    . $adapterPath
    $global:PanGlossPlatformAdapter = [PSCustomObject]@{
        Platform  = 'Linux'
        Overrides = @(
            'Get-AvailableMemoryGB', 'Get-TotalMemoryGB', 'Get-CommitChargeGB',
            'Get-FreeSpaceGB', 'Resolve-TargetDir', 'Use-Sccache', 'Set-SccacheServerPriority',
            'Get-BuildSlotHolders',
            'Enter-BuildSlot', 'Exit-BuildSlot', 'Invoke-CargoWithReaper', 'Invoke-ProcessInJobObject'
        )
    }
    return $global:PanGlossPlatformAdapter
}

# Logical processors left unclaimed by compiler work, machine-wide, so latency-sensitive daemons (sshd,
# Chrome Remote Desktop) keep headroom. docs/research/build-resource-governance.md
$script:InteractiveReserveThreads = if ($env:PANGLOSS_INTERACTIVE_RESERVE) { [int]$env:PANGLOSS_INTERACTIVE_RESERVE } else { 6 }

# The memory analogue of the thread reserve above, proportional to installed RAM rather than a flat
# figure. docs/research/build-resource-governance.md
$script:InteractiveReserveFraction = if ($env:PANGLOSS_MEM_RESERVE_FRACTION) { [double]$env:PANGLOSS_MEM_RESERVE_FRACTION } else { 0.10 }
$script:InteractiveReserveFloorGB = 1.5
$script:InteractiveReserveCeilingGB = 6
# Room a build needs to make actual progress, on top of the daemon reserve above -- a different question.
$script:MinBuildRoomGB = if ($env:PANGLOSS_MIN_BUILD_ROOM_GB) { [double]$env:PANGLOSS_MIN_BUILD_ROOM_GB } else { 2 }

function Get-InteractiveReserveGB {
    param([Nullable[double]]$TotalGB = (Get-TotalMemoryGB))
    if ($env:PANGLOSS_MIN_FREE_MEM_GB) { return [double]$env:PANGLOSS_MIN_FREE_MEM_GB }
    # Unmeasurable machine gets the floor, not the ceiling; the job object bounds the damage either way.
    if ($null -eq $TotalGB) { return $script:InteractiveReserveFloorGB }
    $r = $TotalGB * $script:InteractiveReserveFraction
    if ($r -lt $script:InteractiveReserveFloorGB) { $r = $script:InteractiveReserveFloorGB }
    if ($r -gt $script:InteractiveReserveCeilingGB) { $r = $script:InteractiveReserveCeilingGB }
    return [math]::Round($r, 1)
}

function Get-SpawnFloorGB {
    <#
      .DESCRIPTION
      The "do not start a build" line: daemon headroom (Get-InteractiveReserveGB) plus enough room
      for the build itself to make progress ($script:MinBuildRoomGB).

      This threshold matters less than it did before procgov: a job object caps each build's commit
      regardless of how much was free at spawn, so this gate's remaining job is to turn a hopeless
      start into a clear message instead of a mid-build allocation failure. That is why it is sized
      to be generous to the developer rather than maximally cautious.
    #>
    param([Nullable[double]]$TotalGB = (Get-TotalMemoryGB))
    return [math]::Round(((Get-InteractiveReserveGB -TotalGB $TotalGB) + $script:MinBuildRoomGB), 1)
}
# Working-set allowance per concurrent process (compile / fat-LTO link / test), enforced via procgov's job object.
# docs/research/build-resource-governance.md

$script:MemoryPerCompileJobGB = if ($env:PANGLOSS_MEM_PER_JOB_GB) { [double]$env:PANGLOSS_MEM_PER_JOB_GB } else { 1.5 }
$script:MemoryPerLtoLinkJobGB = if ($env:PANGLOSS_MEM_PER_LTO_JOB_GB) { [double]$env:PANGLOSS_MEM_PER_LTO_JOB_GB } else { 2 }
$script:MemoryPerTestProcessGB = if ($env:PANGLOSS_MEM_PER_TEST_GB) { [double]$env:PANGLOSS_MEM_PER_TEST_GB } else { 2.5 }

function Get-PerJobMemoryGB {
    <#
      .DESCRIPTION
      Which of the two compile-side allowances applies, decided by whether the run's PROFILE turns on
      fat LTO rather than by mode name: `build` and `release` both reach the fat-LTO profile, and
      `-DebugProfile` takes `build` back off it, so a mode-name test would be wrong twice.
    #>
    param([switch]$FatLto)
    if ($FatLto) { return $script:MemoryPerLtoLinkJobGB }
    return $script:MemoryPerCompileJobGB
}

function Get-CargoJobBudget {
    <#
      .DESCRIPTION
      Per-invocation `-j` such that ALL concurrently-permitted builds AND runs together still leave
      $script:InteractiveReserveThreads logical processors free. Divided by MaxConcurrent rather than
      handed out whole, because the build-slot mutex is machine-wide: if two worktrees can each hold a
      slot, each one's job count has to be sized for the case where both do. The run pool's allotment
      comes off the top for exactly the same reason -- it can be fully occupied while both builds run.
    #>
    param(
        [int]$MaxConcurrent = 1,
        # -1 means "the machine's configured run pool"; an explicit 0 models a machine with no run pool.
        [int]$RunSlots = -1,
        [int]$RunThreadsPerSlot = -1
    )
    $logical = [Environment]::ProcessorCount
    if ($MaxConcurrent -lt 1) { $MaxConcurrent = 1 }
    if ($RunSlots -lt 0) { $RunSlots = $script:DefaultRunSlots }
    if ($RunThreadsPerSlot -lt 0) { $RunThreadsPerSlot = $script:RunThreadsPerSlot }
    $budget = $logical - $script:InteractiveReserveThreads - ($RunSlots * $RunThreadsPerSlot)
    # Floor of 2: a single-job cargo serializes codegen workspace-wide, which in practice gets the cap disabled rather than tuned.
    if ($budget -lt 2) { $budget = 2 }
    return [Math]::Max(2, [Math]::Floor($budget / $MaxConcurrent))
}

function Get-AvailableMemoryGB {
    <#
      .DESCRIPTION
      "Available", not "free": Win32_PerfRawData_PerfOS_Memory's AvailableBytes (the counter Task
      Manager itself labels "Available") includes the standby list, unlike
      Win32_OperatingSystem.FreePhysicalMemory, which counts only the free list and understates by
      whatever sits in standby -- often most of a box that has been building for a while. Both CIM
      classes are used, not Get-Counter's '\Memory\Available MBytes' path, because CIM property names
      are not localized and Get-Counter's path throws on a non-English Windows.

      Returns $null, never 0, if neither source answers -- see docs/research/build-resource-governance.md.
    #>
    try {
        $perf = Get-CimInstance Win32_PerfRawData_PerfOS_Memory -ErrorAction Stop
        if ($perf -and $null -ne $perf.AvailableBytes) {
            return [math]::Round(([double]$perf.AvailableBytes) / 1GB, 1)
        }
    } catch {}
    try {
        $os = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop
        if ($os -and $null -ne $os.FreePhysicalMemory) {
            # KB units in this class; understates by the standby list, hence the fallback ordering.
            return [math]::Round(([double]$os.FreePhysicalMemory) * 1KB / 1GB, 1)
        }
    } catch {}
    return $null
}

function Get-CommitChargeGB {
    <#
      .DESCRIPTION
      Committed bytes and the commit LIMIT -- a different resource from available physical memory,
      and the one that actually matters here: a `git` fork can fail on MEM_COMMIT while available
      PHYSICAL memory reads generously high, because the commit charge was near its limit even though
      RAM was free. Both Resource-Exhaustion-Detector event 2004 and procgov's --maxjobmem are
      commit-denominated, not physical-memory-denominated -- see
      docs/research/build-resource-governance.md.

      Win32_OperatingSystem's TotalVirtualMemorySize/FreeVirtualMemory are the commit limit and its
      free remainder, in KB; non-localized property names, same reason as Get-AvailableMemoryGB.
    #>
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
    <#
      .DESCRIPTION
      The spawn gate: is there enough headroom to start this run at all. Pure decision logic taking a
      number, like Test-DiskReserve, so it is unit-testable without a real machine state.

      This is the hard floor, distinct from Get-MemoryProcessBudget's narrowing below: under the
      floor there is no concurrency low enough to be safe, because even ONE test process here can be a
      multi-GB grammar compile. Refusing outright is the conservative direction, and it is the
      direction that leaves a machine you can still SSH into.
    #>
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
    <#
      .DESCRIPTION
      How many concurrent processes of a given weight the CURRENTLY available memory supports, after
      setting the interactive reserve aside. Pure; the caller supplies the measurement.

      Returns $null for "no opinion" when memory is unqueryable, so a caller combining this with the
      CPU budget can tell "memory says 3" from "memory has nothing to say" instead of silently
      clamping every build to a fabricated number.

      Divided by MaxConcurrent for the same reason Get-CargoJobBudget is: the build-slot mutex is
      machine-wide, so each permitted build has to be sized for the case where all of them run. That
      is deliberately conservative even though the measurement is live -- a build that started one
      second ago has allocated almost nothing yet, so a live reading cannot see the peak the other slot
      is about to reach.
    #>
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
    # Floor of 1, never 0: 0 would report as a concurrency setting rather than as the refusal it really is.
    return [int][Math]::Max(1, $n)
}

function Resolve-ConcurrencyBudget {
    <#
      .DESCRIPTION
      Combine the CPU-derived and memory-derived caps, keeping WHICH ONE bound the result, so the
      preflight record can state the real reason a run is narrower than the core count. Never print a
      derivation that did not produce the number shown beside it.
    #>
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

# Resource enforcement via a Windows job object (procgov), replacing three hand-rolled mechanisms.
# docs/research/build-resource-governance.md

function Get-ProcGovPath {
    <#
      .DESCRIPTION
      PATH first, then winget's own shim and package directories: winget only adds its Links
      directory to PATH for shells started AFTER the install, so the shell that just installed it (or
      a long-lived agent session) will not see it there yet.
    #>
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
    <#
      .DESCRIPTION
      Per-build commit ceiling, derived from INSTALLED memory, not available memory: this is a
      runaway backstop, and a cap that shrank because another build was already running would make
      the second build fail spuriously at a size the first was allowed.
    #>
    param([int]$MaxConcurrent = 2, [Nullable[double]]$TotalGB = (Get-TotalMemoryGB))
    if ($env:PANGLOSS_JOB_MEM_GB) { return [int]$env:PANGLOSS_JOB_MEM_GB }
    if ($null -eq $TotalGB) { return $null }
    if ($MaxConcurrent -lt 1) { $MaxConcurrent = 1 }
    # Split across MaxConcurrent + 1, not MaxConcurrent: a correctness fix for over-admission, not padding.
    # docs/research/build-resource-governance.md
    $cap = [math]::Floor((($TotalGB - (Get-InteractiveReserveGB -TotalGB $TotalGB)) / ($MaxConcurrent + 1)))
    # Floor of 4GB: a limit that breaks every build (by failing ordinary linking) gets removed rather than tuned.
    return [int][Math]::Max(4, $cap)
}

# The light-run ceiling: flat and small, NOT machine-proportional; measured against a full Sena corpus.
# docs/research/build-resource-governance.md
$script:RunSlotMemoryGB = if ($env:PANGLOSS_RUN_MEM_GB) { [int]$env:PANGLOSS_RUN_MEM_GB } else { 2 }

function Get-RunJobMemoryCapGB {
    <#
      .DESCRIPTION
      Deliberately not Get-JobMemoryCapGB's derivation: this cap exists to catch a runaway, and a
      runaway is recognizable by absolute size, not by a share of whichever box it is on. A run that
      legitimately needs more is not a light run -- `-Heavy` gives it a build slot and a build's cap.
    #>
    return $script:RunSlotMemoryGB
}

function Get-JobCpuRatePercent {
    <#
      .DESCRIPTION
      Kernel-enforced ceiling sized from the same interactive reserve as the job budget, so the
      daemons this machine is administered through keep headroom no matter how many threads rustc
      decides to spawn. Returns $null when the reserve leaves nothing meaningful to cap.

      -Threads sizes the ceiling from ONE slot's own width instead of the whole machine's usable
      width. Without it every concurrent job requests the entire machine-wide figure and the requests
      sum past 100%; with it they sum back to it. docs/research/build-resource-governance.md
    #>
    param(
        [int]$ReserveThreads = $script:InteractiveReserveThreads,
        [Nullable[int]]$Threads
    )
    $logical = [Environment]::ProcessorCount
    if ($logical -le 0) { return $null }
    if ($null -ne $Threads) {
        $usable = [Math]::Max(1, $Threads)
        # One core's worth: the whole-machine floor below would hand a single-threaded run several cores.
        $floor = [int][math]::Ceiling(100 / $logical)
    } else {
        $usable = $logical - $ReserveThreads
        if ($usable -lt 1) { $usable = 1 }
        $floor = 10
    }
    $pct = [int][math]::Floor(($usable / $logical) * 100)
    if ($pct -lt $floor) { $pct = $floor }
    if ($pct -ge 100) { return $null }   # nothing to enforce
    return $pct
}

function Ensure-ProcGovNative {
    <#
      .DESCRIPTION
      JIT-defines the P/Invoke surface Terminate-ProcGovJob needs (OpenJobObject/TerminateJobObject/
      CloseHandle). Split out so a caller that never hits the kill path never pays Add-Type's cost,
      and so the type is defined at most once per process.
    #>
    if (-not ([System.Management.Automation.PSTypeName]'PanGlossProcGov.Native').Type) {
        Add-Type @'
using System;
using System.Runtime.InteropServices;
namespace PanGlossProcGov {
    public static class Native {
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern IntPtr OpenJobObject(uint access, bool inheritHandle, string name);
        [DllImport("kernel32.dll", SetLastError = true)]
        public static extern bool TerminateJobObject(IntPtr job, uint exitCode);
        [DllImport("kernel32.dll", SetLastError = true)]
        public static extern bool CloseHandle(IntPtr handle);
    }
}
'@
    }
}

function Terminate-ProcGovJob {
    <#
      .DESCRIPTION
      Kills every process procgov's named job object still contains, by asking the KERNEL for job
      membership rather than walking a PID tree. `taskkill /T` (Invoke-ProcessInJobObject's own
      Ctrl+C cleanup) only sees processes still parented under the PID it started with; anything
      re-parented after a crash or an early procgov exit is invisible to that walk but still a member
      of the job object procgov created, which is exactly the gap a real job handle doesn't have.
      Belt-and-braces, not a replacement: called in ADDITION to the existing taskkill, never instead
      of it, since a job name procgov didn't actually create (or already tore down cleanly) just
      means OpenJobObject returns a null handle here -- a harmless no-op, not an error.
    #>
    param(
        [Parameter(Mandatory)][string]$JobName,
        [int]$ExitCode = 130
    )
    Ensure-ProcGovNative
    $job = [PanGlossProcGov.Native]::OpenJobObject([uint32]0x0008, $false, $JobName)
    if ($job -eq [IntPtr]::Zero) { return $false }
    try {
        return [PanGlossProcGov.Native]::TerminateJobObject($job, [uint32]$ExitCode)
    } finally {
        [void][PanGlossProcGov.Native]::CloseHandle($job)
    }
}

function Get-ProcGovArgs {
    <#
      .DESCRIPTION
      Pure argument construction, split out so the limits actually applied are assertable in a test
      without launching procgov or a build.

      -CpuCores and -EfficiencyMode are both OPT-IN alternatives to the default --cpurate ceiling,
      unmeasured against it -- see docs/research/build-resource-governance.md for why each might help
      and why neither has replaced the default on a hunch.
    #>
    param(
        [Nullable[int]]$JobMemoryGB,
        [Nullable[int]]$CpuRatePercent,
        [string]$Priority = '',
        [Nullable[int]]$CpuCores,
        [switch]$EfficiencyMode,
        # Names the job so Terminate-ProcGovJob can find it later; omitted = procgov names it itself.
        [string]$JobName = '',
        [Parameter(Mandatory)][string]$Exe,
        [string[]]$CmdArgs = @()
    )
    $a = @()
    if ($null -ne $JobMemoryGB) { $a += "--maxjobmem=${JobMemoryGB}G" }
    if ($null -ne $CpuCores) {
        # Mutually exclusive with --cpurate, not additive: procgov applies the rate only to the selected cores.
        $a += "--cpu=$CpuCores"
    } elseif ($null -ne $CpuRatePercent) {
        $a += "--cpurate=$CpuRatePercent"
    }
    if ($EfficiencyMode) { $a += '--efficiency-mode=on' }
    if ($Priority) { $a += "--priority=$Priority" }
    if ($JobName) { $a += "--job-name=$JobName" }
    # -r is required: without it the limits apply to the launched process alone and every rustc/link.exe escapes the job.
    $a += '-r'
    $a += '--terminate-job-on-exit'
    $a += '--'
    $a += $Exe
    $a += $CmdArgs
    return $a
}

function Split-ExtraArgsSpec {
    <#
      .DESCRIPTION
      Tokenizes ONE string into an argv array, honoring double quotes so a value containing spaces (a
      path, a filter expression) survives as a single argument.

      This exists because `pwsh -File script.ps1 -Mode test -- --nocapture` fails at parameter-binding
      time ("the parameter name '' is ambiguous"): under -File the bare `--` reaches the binder, which
      reads it as a parameter with an empty name, rather than being consumed by PowerShell's own parser
      the way it is via the call operator (`& .\pg.ps1 ... -- --nocapture`).

      Worse, omitting the separator SILENTLY MISBINDS any single-dash argument meant for the wrapped
      tool that happens to prefix-match a script parameter (`-p foo` intended for cargo binding to
      -Package instead, so cargo never receives it) -- see
      docs/research/build-resource-governance.md. An environment variable is immune because it never
      passes through the parameter binder at all, matching how this repo already handles other
      binder-proof escape hatches (PANGLOSS_ALLOW_BARE_CARGO).
    #>
    param([string]$Spec)
    if (-not $Spec) { return @() }
    $out = @()
    # Quoted run first so it wins over the bare-token alternative; the unquoted branch then takes any run of non-whitespace.
    foreach ($m in [regex]::Matches($Spec, '"([^"]*)"|(\S+)')) {
        $out += if ($m.Groups[1].Success) { $m.Groups[1].Value } else { $m.Groups[2].Value }
    }
    return $out
}

function Get-TopMemoryConsumers {
    <#
      .DESCRIPTION
      Only ever used to make a refusal actionable: "8GB available, under the reserve" is a dead end
      unless it also says what ate the memory. Read-only -- never kills anything, because the process
      holding the memory may well belong to another worktree's healthy build.
    #>
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
    <#
      .DESCRIPTION
      Pure argument-construction logic for `pg.ps1 -Mode run`: the "exactly one selector",
      "'--' stripping", and "cargo run vs. a bare .exe" decisions are unit-testable without launching
      cargo, a probe binary, or touching the filesystem -- existence of -Exe is the CALLER's job via
      Test-Path, precisely so this function never needs a real file to be tested.

      Returns Ok=$false with a Detail message for a usage error (wrong number of selectors); callers
      print Detail and exit rather than this function throwing, matching every other preflight-style
      function in this file (Test-DiskReserve, Test-MemoryReserve, ...).
    #>
    param(
        [string]$Example = '',
        [string]$Bin = '',
        [string]$Exe = '',
        [string]$Package = '',
        [switch]$DebugProfile,
        [string[]]$ExtraArgs = @()
    )
    # The outer @(...) is load-bearing: a lone Where-Object match unwraps to a Hashtable, whose .Count is its key count.
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

    # Strip at most one leading '--' (cargo's own "rest is for the child" convention) so it never reaches argv[1].
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

    # Example/Bin go through `cargo run` (builds first, so a stale binary never runs) and exec as a child of cargo.
    $launchArgs = @('run')
    if (-not $DebugProfile) { $launchArgs += '--release' }
    if ($Package) { $launchArgs += @('-p', $Package) }
    if ($Example) { $launchArgs += @('--example', $Example) }
    if ($Bin) { $launchArgs += @('--bin', $Bin) }
    # cargo's OWN '--' separator: without it, args meant for the binary are parsed by cargo as unrecognized flags.
    if ($passthrough.Count -gt 0) { $launchArgs += @('--') + $passthrough }
    return [PSCustomObject]@{
        Ok         = $true
        LaunchExe  = 'cargo'
        LaunchArgs = $launchArgs
        Label      = if ($Example) { "example: $Example" } else { "bin: $Bin" }
    }
}

function Get-ExhaustionConsumersFromMessage {
    <#
      .DESCRIPTION
      Pure parsing, split out of Get-ResourceExhaustionEvents below so it is unit-testable against
      sample message text (rust/tools/tests/run-mode.tests.ps1) without ever calling Get-WinEvent.

      Message shape, verified against real Microsoft-Windows-Resource-Exhaustion-Detector (event ID
      2004) events: "... consumed the most virtual memory: predict_census.exe (30004) consumed
      118387073024 bytes, vmmemCmZygote (9984) consumed 853762048 bytes, and MsMpEng.exe (5320)
      consumed 529256448 bytes." -- always "<name> (<pid>) consumed <N> bytes", comma-joined, "and"
      before the last one.

      Parsing is best-effort ONLY, never throwing: Microsoft publishes no stable grammar for this
      text, so a message shape this regex does not recognize degrades to an EMPTY list, and the
      caller keeps RawMessage intact for a human regardless of whether this parses it.
    #>
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
    <#
      .DESCRIPTION
      Windows already diagnoses an approaching commit-limit condition and logs it:
      Microsoft-Windows-Resource-Exhaustion-Detector fires event ID 2004 into the System log naming
      the top few processes by committed bytes. This is what lets `pg.ps1 -Mode doctor` surface that
      history instead of it sitting undiscovered in the System log -- see
      docs/research/build-resource-governance.md for the incidents that motivated reading it.

      Message parsing itself lives in Get-ExhaustionConsumersFromMessage above; this function is just
      the live Get-WinEvent query plus the "no data vs. genuinely none" distinction below.
    #>
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
        # Get-WinEvent throws for both "genuinely nothing" and "could not query at all"; only its message text tells them apart.
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
    <#
      .DESCRIPTION
      `git rev-parse --show-toplevel` always answers for whichever worktree the caller is standing
      in, so this resolves correctly from the main checkout or any .claude/worktrees/* checkout with
      no hardcoded paths. Split out from Get-RustRoot because worktree metadata/ownership/base-check
      plumbing needs the repo root itself, not the rust/ subdirectory under it.
    #>
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
    <#
      .DESCRIPTION
      A stable identity for "which repository is this" that survives everything a path can't: cloning
      to a new location, renaming the leaf directory, or a linked worktree with a completely different
      directory name from the primary checkout. The root commit is the one thing every clone/worktree
      of the same repo shares and nothing else does, which target-ownership markers rely on to detect
      "this target dir belongs to a DIFFERENT repo" across machines/clones.
    #>
    param([string]$RepoRoot = (Get-RepoRoot))
    $roots = git -C $RepoRoot rev-list --max-parents=0 HEAD 2>$null
    if (-not $roots) { throw "Could not determine repository root commit (git rev-list --max-parents=0 HEAD) under $RepoRoot" }
    # Sorted so multiple root commits (histories stitched together) still yield one deterministic identity.
    return (($roots | Sort-Object) -join ',')
}

function Get-WorktreeSlug {
    param([string]$RustRoot)
    # Leaf directory name of the checkout root; stable, unique, matches `git worktree list`.
    $repoRoot = Split-Path $RustRoot -Parent
    return (Split-Path $repoRoot -Leaf)
}

function Get-FreeSpaceGB {
    param([string]$Path)
    if ($global:PanGlossPlatformAdapter.Platform -eq 'Linux') { return Get-LinuxFreeSpaceGB -Path $Path }
    $driveRoot = [System.IO.Path]::GetPathRoot($Path)
    if (-not $driveRoot) { return $null }
    $driveLetter = $driveRoot.TrimEnd('\').TrimEnd(':')
    $d = Get-PSDrive -Name $driveLetter -PSProvider FileSystem -ErrorAction SilentlyContinue
    if (-not $d) { return $null }
    return [math]::Round($d.Free / 1GB, 1)
}

function Resolve-TargetDir {
    param([string]$RustRoot, [scriptblock]$DirectoryCreator)
    if ($global:PanGlossPlatformAdapter.Platform -eq 'Linux') {
        $args = @{ RustRoot = $RustRoot }
        if ($PSBoundParameters.ContainsKey('DirectoryCreator')) { $args.DirectoryCreator = $DirectoryCreator }
        return Resolve-LinuxTargetDir @args
    }
    # Never fight a choice already made on purpose: an explicit CARGO_TARGET_DIR or worktree-local config wins outright.
    if ($env:CARGO_TARGET_DIR) { return $env:CARGO_TARGET_DIR }
    $cfg = Join-Path $RustRoot '.cargo\config.toml'
    if (Test-Path $cfg) {
        $text = Get-Content $cfg -Raw
        if ($text -match 'target-dir\s*=') { return $null }  # let cargo read its own config
    }
    $slug = Get-WorktreeSlug -RustRoot $RustRoot

    # SSD preferred while it has headroom; HDD fallback so many worktrees building at once can't refill the crisis.
    # docs/research/build-resource-governance.md
    $ssdFree = Get-FreeSpaceGB $script:SsdCacheRoot
    if ($null -ne $ssdFree -and $ssdFree -ge $script:MinFreeGBOnSsd) {
        $target = Join-Path $script:SsdCacheRoot $slug
        New-Item -ItemType Directory -Force -Path $target | Out-Null
        return $target
    }
    if ($null -ne $ssdFree) {
        Write-Host "[build-env] $($script:SsdCacheRoot)'s drive has ${ssdFree}GB free (< $($script:MinFreeGBOnSsd)GB reserve) -- using HDD cache root instead" -ForegroundColor Yellow
    }

    # Defensive: a machine without the HDD cache drive at all must degrade to a local target/, not crash on New-Item.
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
    param([scriptblock]$CommandResolver, [scriptblock]$DirectoryCreator)
    if ($global:PanGlossPlatformAdapter.Platform -eq 'Linux') {
        $args = @{}
        if ($PSBoundParameters.ContainsKey('CommandResolver')) { $args.CommandResolver = $CommandResolver }
        if ($PSBoundParameters.ContainsKey('DirectoryCreator')) { $args.DirectoryCreator = $DirectoryCreator }
        return Use-LinuxSccache @args
    }
    if (-not (Get-Command sccache -ErrorAction SilentlyContinue)) { return $false }
    $env:RUSTC_WRAPPER = 'sccache'
    # Deliberately on the HDD root: a cache hit is one blob read, so capacity matters more than seek time here.
    if (-not $env:SCCACHE_DIR) { $env:SCCACHE_DIR = Join-Path $script:HddCacheRoot 'sccache' }
    New-Item -ItemType Directory -Force -Path $env:SCCACHE_DIR | Out-Null
    if (-not $env:SCCACHE_CACHE_SIZE) {
        # Proportional to free space, not sccache's flat 10GiB default -- many worktrees share one server.
        $freeGB = Get-FreeSpaceGB $script:HddCacheRoot
        $sizeGB = if ($null -ne $freeGB) { [Math]::Min(150, [Math]::Max(20, [Math]::Floor($freeGB / 10))) } else { 20 }
        $env:SCCACHE_CACHE_SIZE = "${sizeGB}G"
    }
    return $true
}

function Set-SccacheServerPriority {
    <#
      .DESCRIPTION
      Load-bearing, not cosmetic: dropping cargo to BelowNormal alone leaves most rustc work at
      Normal, because RUSTC_WRAPPER=sccache means cargo never execs rustc itself -- it invokes a
      short-lived sccache client, which hands the compile to the long-lived sccache SERVER daemon,
      which spawns rustc. Those rustc processes inherit the DAEMON's priority class, not cargo's, and
      the daemon outlives any one build and normally starts at Normal. See
      docs/research/build-resource-governance.md.

      Call AFTER Test-SccacheHealth (its `--show-stats` is what starts the server) and BEFORE cargo
      starts: priority is inherited at spawn time, so an already-running rustc keeps the class it was
      born with.
    #>
    param([ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = 'BelowNormal')
    $changed = 0
    foreach ($p in @(Get-Process -Name sccache -ErrorAction SilentlyContinue)) {
        try {
            if ($p.PriorityClass -ne $Priority) { $p.PriorityClass = $Priority; $changed++ }
        } catch {
            # Non-fatal by design: a build at the wrong priority is a performance problem, not one worth refusing to start over.
            Write-Host "[pg] note: could not set $Priority priority on sccache server (pid $($p.Id)): $($_.Exception.Message)" -ForegroundColor DarkGray
        }
    }
    return $changed
}

# Build slots: N named mutexes, not one counted semaphore -- the kernel reclaims a dead holder's slot.
# docs/research/build-resource-governance.md

$script:BuildSlotMutexPrefix = 'Global\PanGlossBuildSlot'

# A SECOND, independent pool -- deliberately not just more build slots; builds and runs bind on different resources.
# docs/research/build-resource-governance.md
$script:RunSlotMutexPrefix = 'Global\PanGlossRunSlot'
$script:DefaultRunSlots = if ($env:PANGLOSS_RUN_SLOTS) { [int]$env:PANGLOSS_RUN_SLOTS } else { 4 }
# One core per run slot: the light-run shape (`pangloss parse`, `batch --threads 1`) is single-threaded.
$script:RunThreadsPerSlot = if ($env:PANGLOSS_RUN_THREADS_PER_SLOT) { [int]$env:PANGLOSS_RUN_THREADS_PER_SLOT } else { 1 }

function Get-SlotMutexPrefix {
    <#
      .DESCRIPTION
      Read through a function rather than inlined so the pool prefixes stay overridable at script scope
      (rust/tools/tests/build-slot.tests.ps1 substitutes its own to isolate from the live machine).
    #>
    param([ValidateSet('build', 'run')][string]$Pool = 'build')
    if ($Pool -eq 'run') { return $script:RunSlotMutexPrefix }
    return $script:BuildSlotMutexPrefix
}

function New-BuildSlotMutex {
    param([Parameter(Mandatory)][string]$Name)
    try {
        return New-Object System.Threading.Mutex($false, $Name)
    } catch [System.UnauthorizedAccessException] {
        # Same Global\ -> Local\ fallback the semaphore had, for a session that cannot create global kernel objects.
        return New-Object System.Threading.Mutex($false, ($Name -replace '^Global\\', 'Local\'))
    }
}

function Enter-ResourceSlot {
    <#
      .DESCRIPTION
      Waits for one slot in $Pool and returns a handle to hand back to Exit-ResourceSlot, or $null on
      timeout. The two pools are separate kernel objects, so a run never queues behind a build.

      -TimeoutSeconds <= 0 waits indefinitely (kept for direct callers); pg.ps1 passes a real timeout
      so a genuinely long queue is reported rather than hung on forever.
    #>
    param(
        [ValidateSet('build', 'run')][string]$Pool = 'build',
        [int]$MaxConcurrent = 2,
        [int]$TimeoutSeconds = 0
    )
    if ($MaxConcurrent -lt 1) { $MaxConcurrent = 1 }
    $prefix = Get-SlotMutexPrefix -Pool $Pool

    $mutexes = @()
    for ($i = 0; $i -lt $MaxConcurrent; $i++) {
        $mutexes += New-BuildSlotMutex -Name "$prefix$i"
    }

    # Report WHO holds the slots before blocking: a 20-minute anonymous wait is indistinguishable from a deadlock.
    Write-Host "[build-env] waiting for a $Pool slot ($MaxConcurrent concurrent across all worktrees)..." -ForegroundColor DarkGray
    try {
        $holders = @(Get-SlotHolders -Pool $Pool)
        foreach ($h in $holders) {
            $state = if ($h.Alive) { "alive since $($h.AcquiredAt)" } else { 'NOT ALIVE (kernel will hand this slot over)' }
            Write-Host "[build-env]   $Pool slot $($h.Slot): pid $($h.Pid) ($($h.Mode) in $($h.Worktree)) -- $state" -ForegroundColor DarkGray
        }
    } catch {}

    $timeoutMs = if ($TimeoutSeconds -le 0) { [System.Threading.Timeout]::Infinite } else { $TimeoutSeconds * 1000 }
    $index = -1
    try {
        $index = [System.Threading.WaitHandle]::WaitAny($mutexes, $timeoutMs)
    } catch [System.Threading.AbandonedMutexException] {
        # The previous holder died without releasing; we now OWN that mutex -- the kernel's recovery, not ours.
        $index = $_.Exception.MutexIndex
        Write-Host "[build-env] recovered an abandoned $Pool slot ($index) -- its previous holder exited without releasing it." -ForegroundColor Yellow
    }

    if ($index -eq [System.Threading.WaitHandle]::WaitTimeout -or $index -lt 0) {
        foreach ($m in $mutexes) { $m.Dispose() }
        return $null
    }

    $slot = [PSCustomObject]@{ Mutexes = $mutexes; Index = $index; Pool = $Pool }
    try { Write-SlotHolder -Pool $Pool -Slot $index } catch {}
    return $slot
}

function Enter-BuildSlot {
    <# .DESCRIPTION Build-pool front end onto Enter-ResourceSlot, kept because most callers want that pool. #>
    param([int]$MaxConcurrent = 2, [int]$TimeoutSeconds = 0)
    return Enter-ResourceSlot -Pool 'build' -MaxConcurrent $MaxConcurrent -TimeoutSeconds $TimeoutSeconds
}

function Exit-ResourceSlot {
    <#
      .DESCRIPTION
      Accepts the object Enter-ResourceSlot returned. Releasing only the mutex actually acquired matters:
      ReleaseMutex on one we do not own throws ApplicationException.
    #>
    param($Slot)
    if (-not $Slot) { return }
    try {
        if ($null -ne $Slot.Index -and $Slot.Mutexes) {
            try { $Slot.Mutexes[$Slot.Index].ReleaseMutex() } catch {}
            # A handle from before the pools split carries no Pool; it can only have been a build slot.
            $pool = if ($Slot.Pool) { $Slot.Pool } else { 'build' }
            try { Clear-SlotHolder -Pool $pool -Slot $Slot.Index } catch {}
            foreach ($m in $Slot.Mutexes) { $m.Dispose() }
            return
        }
        # Back-compat: a caller still holding a raw semaphore/mutex handle from older code.
        $Slot.Release() | Out-Null
        $Slot.Dispose()
    } catch {}
}

function Exit-BuildSlot {
    <# .DESCRIPTION Back-compat front end; the pool travels on the handle, so this works for either. #>
    param($Semaphore)
    Exit-ResourceSlot -Slot $Semaphore
}

# Slot holder ledger -- DIAGNOSTIC ONLY; never consulted to decide whether a slot is free.
# docs/research/build-resource-governance.md

function Get-SlotLedgerPath {
    <#
      .DESCRIPTION
      One directory per pool, rather than one directory with prefixed filenames, so the build pool's
      on-disk layout is exactly what it was before the run pool existed.
    #>
    param([ValidateSet('build', 'run')][string]$Pool = 'build')
    $root = if ($env:PANGLOSS_STATE_ROOT) { $env:PANGLOSS_STATE_ROOT } elseif ($env:ProgramData) { Join-Path $env:ProgramData 'PanGloss' } else { Join-Path ([System.IO.Path]::GetTempPath()) 'PanGloss' }
    if (-not (Test-Path $root)) { New-Item -ItemType Directory -Force -Path $root | Out-Null }
    return (Join-Path $root "$Pool-slots")
}

function Get-BuildSlotLedgerPath { return (Get-SlotLedgerPath -Pool 'build') }

function Write-SlotHolder {
    param(
        [ValidateSet('build', 'run')][string]$Pool = 'build',
        [Parameter(Mandatory)][int]$Slot,
        [string]$Mode = '',
        [string]$Worktree = ''
    )
    $dir = Get-SlotLedgerPath -Pool $Pool
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    if (-not $Mode) { $Mode = if ($script:CurrentPgMode) { $script:CurrentPgMode } else { 'build' } }
    if (-not $Worktree) { $Worktree = try { Split-Path (Get-RepoRoot) -Leaf } catch { 'unknown' } }
    [PSCustomObject]@{ Pid = $PID; Mode = $Mode; Worktree = $Worktree; AcquiredAt = (Get-Date).ToString('HH:mm:ss') } |
        ConvertTo-Json -Compress | Set-Content -Path (Join-Path $dir "slot$Slot.json") -Encoding UTF8
}

function Write-BuildSlotHolder {
    param([Parameter(Mandatory)][int]$Slot, [string]$Mode = '', [string]$Worktree = '')
    Write-SlotHolder -Pool 'build' -Slot $Slot -Mode $Mode -Worktree $Worktree
}

function Clear-SlotHolder {
    param([ValidateSet('build', 'run')][string]$Pool = 'build', [Parameter(Mandatory)][int]$Slot)
    Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path (Get-SlotLedgerPath -Pool $Pool) "slot$Slot.json")
}

function Clear-BuildSlotHolder {
    param([Parameter(Mandatory)][int]$Slot)
    Clear-SlotHolder -Pool 'build' -Slot $Slot
}

function Get-BuildSlotHolders { return @(Get-SlotHolders -Pool 'build') }

function Get-SlotHolders {
    param([ValidateSet('build', 'run')][string]$Pool = 'build')
    $dir = Get-SlotLedgerPath -Pool $Pool
    if (-not (Test-Path $dir)) { return @() }
    $out = @()
    foreach ($f in @(Get-ChildItem -Path $dir -Filter 'slot*.json' -ErrorAction SilentlyContinue)) {
        try {
            $e = Get-Content $f.FullName -Raw | ConvertFrom-Json
            $alive = $false
            try { $alive = $null -ne (Get-Process -Id $e.Pid -ErrorAction Stop) } catch { $alive = $false }
            $out += [PSCustomObject]@{
                Pool       = $Pool
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

# Signs of real work under a build-slot holder; procgov.exe is deliberately excluded -- an idle wrapper IS the stuck shape this checks for.
$script:LiveBuildActivityNames = @('rustc.exe', 'cargo.exe', 'link.exe', 'cc1.exe', 'cc1plus.exe', 'sccache.exe', 'cargo-nextest.exe', 'pangloss.exe')

function Get-ProcessDescendants {
    <#
      .DESCRIPTION
      Every process transitively parented under $RootPid, PID-reuse-safe (a candidate child is only
      accepted when created AFTER the parent it claims, same guard as Test-ParentAlive) -- a build
      wrapper's job nests procgov one level deep, so a direct-children-only walk would miss the
      compiler processes underneath it.
    #>
    param([Parameter(Mandatory)][int]$RootPid, [Parameter(Mandatory)]$Snapshot)
    $byParent = @{}
    foreach ($p in $Snapshot) {
        $key = [string]$p.ParentProcessId
        if (-not $byParent.ContainsKey($key)) { $byParent[$key] = @() }
        $byParent[$key] += $p
    }
    $root = $Snapshot | Where-Object { $_.ProcessId -eq $RootPid } | Select-Object -First 1
    $out = @()
    # Visited set: the snapshot is not a tree (PID reuse, self-parented/null-field system rows -- System Idle is pid 0 with parent 0), and a BFS without one loops forever (measured live: preflight's staleness check spun a full core indefinitely walking a holder's tree).
    $visited = @{ ([string]$RootPid) = $true }
    $frontier = @([PSCustomObject]@{ ProcessId = $RootPid; CreationDate = $(if ($root) { $root.CreationDate } else { $null }) })
    while ($frontier.Count -gt 0) {
        $next = @()
        foreach ($node in $frontier) {
            foreach ($child in @($byParent[[string]$node.ProcessId])) {
                if ($node.CreationDate -and $child.CreationDate -and $child.CreationDate -lt $node.CreationDate) { continue }
                $key = [string]$child.ProcessId
                if (-not $key -or $visited.ContainsKey($key)) { continue }
                $visited[$key] = $true
                $out += $child
                $next += $child
            }
        }
        $frontier = $next
    }
    return $out
}

function Test-ManagedProcessTreeIdle {
    <#
      .DESCRIPTION
      True when $RootPid is alive but neither it nor any descendant (Get-ProcessDescendants) matches
      $script:LiveBuildActivityNames plus $ExtraLiveNames -- "alive, but empty of real work". Shared by
      Test-BuildSlotHolderStale below and Wait-ManagedProcessTree (this file's wedged-procgov
      detector) so the two never re-derive the same question differently. $ExtraLiveNames exists for
      `-Mode run`: the launched payload (predict_census.exe, hc-rs.exe, ...) is not a build tool, so
      without naming it here a legitimate hours-long probe would read as idle and be killed.
    #>
    param([Parameter(Mandatory)][int]$RootPid, [Parameter(Mandatory)]$Snapshot, [string[]]$ExtraLiveNames = @())
    $root = $Snapshot | Where-Object { $_.ProcessId -eq $RootPid } | Select-Object -First 1
    if (-not $root) { return $false }
    $tree = @($root) + @(Get-ProcessDescendants -RootPid $RootPid -Snapshot $Snapshot)
    $liveNames = @($script:LiveBuildActivityNames) + @($ExtraLiveNames)
    $active = @($tree | Where-Object { $_.Name -in $liveNames })
    return $active.Count -eq 0
}

function Test-BuildSlotHolderStale {
    <#
      .DESCRIPTION
      A held slot is stale when the holder is alive but doing nothing: past a generous minimum age
      (a real `-Scope all` conformance run legitimately runs tens of minutes, so this must never fire
      mid-build) AND Test-ManagedProcessTreeIdle says its tree matches nothing in
      $script:LiveBuildActivityNames. Root cause this exists for: `Invoke-ProcessInJobObject`'s wait
      used to be a bare `Wait-Process -Id $psi.Id` with no timeout, so a procgov process that never
      exits after its own job empties out was invisible to that function's own `finally` cleanup,
      which only runs once the wait returns -- Wait-ManagedProcessTree below now closes that gap
      directly, at the source, rather than leaving this ledger-only sweep as the sole recovery.
    #>
    param(
        $Holder, $Snapshot,
        [int]$MinAgeMinutes = 20,
        [datetime]$Now = (Get-Date)
    )
    if (-not $Holder.Alive) { return $false }
    $proc = $Snapshot | Where-Object { $_.ProcessId -eq $Holder.Pid } | Select-Object -First 1
    if (-not $proc -or -not $proc.CreationDate) { return $false }
    if (($Now - $proc.CreationDate).TotalMinutes -lt $MinAgeMinutes) { return $false }
    return Test-ManagedProcessTreeIdle -RootPid $Holder.Pid -Snapshot $Snapshot
}

function Remove-StaleBuildSlotHolders {
    <#
      .DESCRIPTION
      Reaps build-slot holders Test-BuildSlotHolderStale flags. Kills the ledger PID itself (not its
      descendants directly) -- `taskkill /T` takes the whole tree, and once the ledger PID dies the
      OS marks its build-slot mutex abandoned, which Enter-BuildSlot's existing AbandonedMutexException
      path already hands to the next waiter automatically; this function only ever supplies the "the
      holder dies" half that path was designed around, never a second way to free a slot.
    #>
    param([switch]$WhatIfOnly = $true, [int]$MinAgeMinutes = 20)
    $snapshot = Get-ProcessSnapshot
    $now = Get-Date
    foreach ($h in @(Get-BuildSlotHolders)) {
        if (-not (Test-BuildSlotHolderStale -Holder $h -Snapshot $snapshot -MinAgeMinutes $MinAgeMinutes -Now $now)) { continue }
        $ageMin = [int](($now - ($snapshot | Where-Object { $_.ProcessId -eq $h.Pid } | Select-Object -First 1).CreationDate).TotalMinutes)
        if ($WhatIfOnly) {
            Write-Host "[gc] would kill stale build-slot holder PID $($h.Pid) (slot $($h.Slot), $($h.Mode) in $($h.Worktree), alive ${ageMin}min with no compiler activity)" -ForegroundColor Yellow
        } else {
            Write-Host "[gc] killing stale build-slot holder PID $($h.Pid) (slot $($h.Slot), $($h.Mode) in $($h.Worktree), ${ageMin}min idle)" -ForegroundColor Yellow
            & taskkill /T /F /PID $h.Pid 2>$null | Out-Null
            try { Clear-BuildSlotHolder -Slot $h.Slot } catch {}
        }
    }
}

function Wait-ManagedProcessTree {
    <#
      .DESCRIPTION
      Bounded replacement for a bare `Wait-Process -Id $Process.Id`. Observed live: nextest printed
      its full summary, every cargo/rustc/link/test process on the machine had exited, yet the outer
      AND inner procgov.exe stayed alive with a completely empty job tree -- a bare wait on that PID
      hangs forever, and every caller (pg.ps1, then release.ps1's test gate) hangs with it.

      Liveness of the TREE is the discriminator, never a wall clock alone: this repo's own CLAUDE.md
      already documents why a fixed deadline is the wrong instrument here (the 30-minute build-slot
      timeout is arithmetically unreachable under load for exactly that reason, and a real -Scope all
      run or a fat-LTO relink can legitimately run far longer). A real build keeps at least one
      $script:LiveBuildActivityNames process alive continuously -- cargo.exe itself spans the whole
      invocation -- so Test-ManagedProcessTreeIdle only starts reading "idle" the moment the real work
      has already finished; $MaxIdleMinutes then bounds how long $Process may sit idle after that
      before it is declared wedged, and is deliberately short (minutes, not the 20-minute ledger
      threshold above) because nothing legitimate happens between "cargo returned" and "the wrapper
      also returns".

      Returns Wedged=$true rather than killing anything itself -- the caller already owns $Process's
      cleanup (Invoke-ProcessInJobObject's existing `finally`) and must not gain a second copy of it.

      $SnapshotProvider/$SleepAction/$NowProvider are injection seams so the polling loop is testable
      against synthetic ticks with no real process, no real sleep, and no real clock -- see
      rust/tools/tests/managed-process-wait.tests.ps1 -- never something production overrides.
    #>
    param(
        [Parameter(Mandatory)]$Process,
        [int]$PollSeconds = 10,
        # double, not int: a real-process test needs a sub-minute threshold to stay fast without ever mocking the OS process tree.
        [double]$MaxIdleMinutes = 3,
        # Names beyond $script:LiveBuildActivityNames that count as real work in THIS tree -- the launched payload itself, for `-Mode run`.
        [string[]]$ExtraLiveNames = @(),
        [scriptblock]$SnapshotProvider = { Get-ProcessSnapshot },
        [scriptblock]$SleepAction = { param($Seconds) Start-Sleep -Seconds $Seconds },
        [scriptblock]$NowProvider = { Get-Date }
    )
    $idleSince = $null
    while (-not $Process.HasExited) {
        $snapshot = & $SnapshotProvider
        $now = & $NowProvider
        if (Test-ManagedProcessTreeIdle -RootPid $Process.Id -Snapshot $snapshot -ExtraLiveNames $ExtraLiveNames) {
            if (-not $idleSince) { $idleSince = $now }
            elseif (($now - $idleSince).TotalMinutes -ge $MaxIdleMinutes) {
                return [PSCustomObject]@{ Wedged = $true; IdleSince = $idleSince; ExitCode = $null }
            }
        } else {
            $idleSince = $null
        }
        & $SleepAction $PollSeconds
    }
    return [PSCustomObject]@{ Wedged = $false; IdleSince = $null; ExitCode = $Process.ExitCode }
}

function Invoke-ProcessInJobObject {
    <#
      .DESCRIPTION
      The procgov-wrapping core, extracted out of what used to be the entire body of
      Invoke-CargoWithReaper so a cargo build and an arbitrary long-running PanGloss binary
      (`pg.ps1 -Mode run` -- predict_census, `pangloss batch`, ...) get the SAME kernel-enforced
      ceiling from ONE implementation instead of two copies that can drift. See
      docs/research/build-resource-governance.md for the incidents this closes.

      Callers resolve JobMemoryGB/CpuRatePercent themselves (Get-JobMemoryCapGB/
      Get-JobCpuRatePercent) rather than this function deriving them, because different callers derive
      them differently: a build divides the machine's headroom by how many build SLOTS are permitted
      at once; `run` sizes its cap the same way but a caller wanting a deliberate experiment overrides
      the number outright rather than the derivation.
    #>
    param(
        [Parameter(Mandatory)][string]$Exe,
        # NOT named $Args: that's PowerShell's automatic variable, and a parameter of that name silently fails to bind.
        [string[]]$CmdArgs = @(),
        [string]$WorkingDirectory,
        # corpus-test needs cargo's raw stdout after the run regardless of pass/fail, to sum recorded case counts.
        [string]$CaptureStdoutPath = '',
        [ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = 'BelowNormal',
        [Nullable[int]]$JobMemoryGB,
        [Nullable[int]]$CpuRatePercent,
        # Purely cosmetic word choice for the "no procgov" warning so it stays accurate for whichever pg.ps1 mode called in.
        [string]$Subject = 'build',
        # Wait-ManagedProcessTree tuning; overridable so a caller (or a test) never has to wait on the production default.
        [int]$WaitPollSeconds = 10,
        [double]$WaitMaxIdleMinutes = 3,
        # $null means "derive from $Exe" (the payload counts as live work); a test passes @() to simulate the payload having already exited, the incident's exact shape.
        [string[]]$WaitExtraLiveNames = $null
    )
    # Wrap the whole process tree in a Windows job object (via procgov) so the kernel enforces the ceilings.
    # docs/research/build-resource-governance.md
    $procgov = Get-ProcGovPath
    $launchExe = $Exe
    $launchArgs = $CmdArgs
    $jobName = $null
    if ($procgov) {
        $jobName = "PanGloss-$PID-$([guid]::NewGuid().ToString('N'))"
        $launchArgs = Get-ProcGovArgs -JobMemoryGB $JobMemoryGB -CpuRatePercent $CpuRatePercent -Priority $Priority -JobName $jobName -Exe $Exe -CmdArgs $CmdArgs
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
    # Start-Process so we hold a real PID to reap: only `taskkill /T` reliably kills rustc/link.exe descendants on Windows.
    $psi = Start-Process @psiArgs

    # Set on the PARENT, not each descendant: Windows propagates BelowNormal to children for free, keeping
    # interactive daemons (sshd, Chrome Remote Desktop) ahead of the whole fan-out. docs/research/build-resource-governance.md
    try {
        if (-not $psi.HasExited) { $psi.PriorityClass = $Priority }
    } catch {
        Write-Host "[pg] note: could not set $Priority priority on $Exe (pid $($psi.Id)): $($_.Exception.Message)" -ForegroundColor DarkGray
    }

    # Bounded, tree-liveness-aware wait -- see Wait-ManagedProcessTree's own doc for the wedge this replaced.
    try {
        if ($null -eq $WaitExtraLiveNames) {
            # The payload IS the work for `-Mode run -Exe`: without this, an hours-long predict_census.exe would read as idle and be killed at the bound.
            $payloadName = [System.IO.Path]::GetFileName($Exe)
            if ($payloadName -and -not [System.IO.Path]::GetExtension($payloadName)) { $payloadName = "$payloadName.exe" }
            $WaitExtraLiveNames = @($payloadName)
        }
        $wait = Wait-ManagedProcessTree -Process $psi -PollSeconds $WaitPollSeconds -MaxIdleMinutes $WaitMaxIdleMinutes -ExtraLiveNames $WaitExtraLiveNames
        if ($wait.Wedged) {
            $liveNames = @($script:LiveBuildActivityNames) + @($WaitExtraLiveNames)
            Write-Host "[pg] REFUSING to wait any longer: pid $($psi.Id) ($launchExe) is alive but its process tree has matched none of {$($liveNames -join ', ')} for ${WaitMaxIdleMinutes}+ minute(s) (idle since $($wait.IdleSince))." -ForegroundColor Red
            if ($jobName) { Write-Host "[pg]   job object: $jobName -- terminating it now, along with pid $($psi.Id)." -ForegroundColor Red }
            Write-Host "[pg] exit $script:ExitCodeManagedProcessWedged means exactly this: the wrapper's own work already finished and it never returned on its own." -ForegroundColor Red
            exit $script:ExitCodeManagedProcessWedged
        }
        return $wait.ExitCode
    } finally {
        if (-not $psi.HasExited) {
            & taskkill /T /F /PID $psi.Id 2>$null | Out-Null
        }
        # Belt-and-braces: catches anything taskkill's PID-tree walk missed after a re-parent.
        if ($jobName) {
            [void](Terminate-ProcGovJob -JobName $jobName)
        }
    }
}

function Invoke-CargoWithReaper {
    <#
      .DESCRIPTION
      Cargo-specific front end onto Invoke-ProcessInJobObject, kept as its own function rather than
      inlining the derivation at every call site, so the four existing modes (build/test/corpus-test/
      release) don't each have to know how to derive the job-object ceilings.
    #>
    param(
        [string]$Exe,
        [string[]]$CmdArgs,
        [string]$WorkingDirectory,
        [string]$CaptureStdoutPath = '',
        [ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = 'BelowNormal',
        # Only sizes the job object's memory ceiling; the build-slot mutex is what actually bounds concurrency.
        [int]$JobMaxConcurrent = 2,
        # This invocation's own width, so its CPU ceiling is its share rather than the whole machine's; 0 keeps the pre-split behavior.
        [int]$Threads = 0
    )
    $jobMemGB = Get-JobMemoryCapGB -MaxConcurrent $JobMaxConcurrent
    $cpuRate = if ($Threads -gt 0) { Get-JobCpuRatePercent -Threads $Threads } else { Get-JobCpuRatePercent }
    return Invoke-ProcessInJobObject -Exe $Exe -CmdArgs $CmdArgs -WorkingDirectory $WorkingDirectory `
        -CaptureStdoutPath $CaptureStdoutPath -Priority $Priority -JobMemoryGB $jobMemGB -CpuRatePercent $cpuRate -Subject 'build'
}

function Get-LiveWorktreeSlugs {
    <#
      .DESCRIPTION
      Slugs (leaf dir names) of every worktree `git worktree list` currently knows about; anything
      under a cache root not in this set belongs to a worktree that's been deleted. -RepoRoot exists
      so a caller holding a repo path can ask about THAT repository rather than whichever one the
      process happens to be standing in.
    #>
    param([string]$RepoRoot = (Get-RepoRoot))
    (git -C $RepoRoot worktree list --porcelain | Select-String '^worktree (.+)$').Matches |
        ForEach-Object { Split-Path $_.Groups[1].Value -Leaf }
}

function Remove-StaleTargetCaches {
    param([switch]$WhatIfOnly = $true)
    # Both roots need sweeping: a target-dir can live on either, depending on headroom at build time.
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
    <#
      .DESCRIPTION
      ONE CIM query, reused for every liveness decision below. A snapshot for correctness, not speed:
      querying processes one at a time lets the picture change underneath a loop, so a build starting
      mid-sweep could be judged against a parent list that predates it.
    #>
    Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name, CommandLine, CreationDate
}

function Test-ParentAlive {
    <#
      .DESCRIPTION
      Is $Proc's parent genuinely still running? Two ways to get this wrong, and killing another
      worktree's live build is the unacceptable one, so both are guarded -- see
      docs/research/build-resource-governance.md:
      1. PID reuse: a candidate parent is only accepted when created BEFORE the child.
      2. `Get-Process -Id` reports failure both for "gone" and for "access denied", and "I could not
         look" must never read as "it is dead". The CIM snapshot answers existence uniformly.
    #>
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
    # Machine-wide sweep: liveness is decided by Test-ParentAlive, never by name/age/CPU, so another worktree's
    # healthy build stays untouchable. docs/research/build-resource-governance.md
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

# The ONLY process names this sweep may ever consider -- a named constant so the safety argument stays checkable in one place.
$script:ReapableScanNames = @('find.exe', 'rg.exe', 'grep.exe', 'findstr.exe')

function Test-ReapableScanProcess {
    <#
      .DESCRIPTION
      Pure decision, split out from the killing so the safety properties are testable without
      spawning or terminating anything real. Returns $true only when ALL of:
        - the name is in $script:ReapableScanNames, so a cargo/rustc/link can never be selected;
        - the parent is genuinely gone (PID-reuse-safe, see Test-ParentAlive);
        - it has burned real CPU and existed long enough that a just-launched scan is never caught.
    #>
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
        # Both thresholds must be crossed, to make a false positive practically impossible.
        [int]$MinCpuSeconds = 60,
        [int]$MinAgeMinutes = 2
    )
    # An orphaned scanner has produced nothing but a closed pipe, so there is no salvageable output to weigh against killing it.
    # docs/research/build-resource-governance.md
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
    <#
      .DESCRIPTION
      gc's process check before it deletes anything: cargo/rustc/link currently running, orphaned or
      not -- broader than Remove-OrphanedCargoProcesses on purpose, since a live, healthy build in
      another worktree is exactly what gc must not race against.

      sccache is deliberately NOT in this list, and used to be. It is a long-lived shared DAEMON,
      not a build: this script starts it, keeps it alive, and reports it healthy in every preflight
      record. Counting it as a busy process made `gc -Apply` refuse unconditionally on any machine
      where sccache works -- measured with 32GB of disposable target directories present, zero
      compilers running, and the single reported "live build process" being the sccache server.
      A reclaimer that can never reclaim is the same defect as a gate that never gates. sccache also
      writes only its own cache directory, never a managed target dir, so it cannot be raced with.
    #>
    Get-CimInstance Win32_Process -Filter "Name='rustc.exe' or Name='cargo.exe' or Name='link.exe'"
}

# Preflight and build-hardening surface, consumed by pg.ps1; exit code taxonomy is in this file's own header.
$script:ExitCodeWrongBase = 10
$script:ExitCodeMissingCorpus = 11
$script:ExitCodeLowDisk = 12
$script:ExitCodeCacheUnavailable = 13
$script:ExitCodeBadTargetOwnership = 14
$script:ExitCodeBuildSlotTimeout = 15
$script:ExitCodeZeroCorpusCases = 16
$script:ExitCodeLowMemory = 17
$script:ExitCodeConformanceSubmoduleMissing = 18
# The invoked script and the CWD it is run from resolve to different worktrees -- nothing is wrong with either tree alone.
$script:ExitCodeWorktreeMismatch = 19
# conformance-test invoked without -Scope; recovery is a caller decision, not an environment repair.
$script:ExitCodeConformanceScopeUnclaimed = 20
# Linux managed spawning requires proof that this wrapper is already under a finite host cgroup cap.
$script:ExitCodeLinuxHostContainment = 21
# An explicit per-run memory override cannot be honored on Linux; the service cgroup owns the cap.
$script:ExitCodeLinuxRunMemoryOverride = 22
$script:ExitCodeUnsupportedPlatform = 23
$script:ExitCodeLinuxGcUnsupported = 24
# oracle-conformance.ps1: dotnet or hc-conformance.exe not found -- "I could not look" must exit loud.
$script:ExitCodeOracleUnavailable = 25
# oracle-conformance.ps1: a signature/load-failure mismatch outside the known-divergence baseline.
$script:ExitCodeOracleDivergence = 26
# Wait-ManagedProcessTree declared $Process (procgov, or the bare exe without it) wedged: alive, tree idle past the bound.
$script:ExitCodeManagedProcessWedged = 27

function Get-FilterZeroMatchHint {
    <#
      .DESCRIPTION
      Pure function: given the -Filter a caller used and the test-target names available, return the
      lines to print when the runner reported "no tests to run". Extracted from pg.ps1's tail rather
      than left inline SO THAT IT IS TESTABLE WITHOUT A BUILD -- the condition it explains only arises
      after cargo runs, and on this repo a cold cargo run is ~996s. A guard whose only exercise costs
      sixteen minutes is a guard that never gets exercised.

      Returns an array of [pscustomobject]@{ Text; Color }. Empty array means "no hint applies".
    #>
    param(
        [string]$Filter,
        [string[]]$TestTargets = @()
    )

    if (-not $Filter) { return @() }
    $out = @([pscustomobject]@{ Text = "[pg] no test NAME matched -Filter '$Filter' (runner reported no tests to run)."; Color = 'Yellow' })

    if ($TestTargets -contains $Filter) {
        # A test TARGET (a file stem under tests/) handed to a TEST-NAME filter -- the recurring mistake this guards against.
        $out += [pscustomobject]@{ Text = "[pg] '$Filter' is a test TARGET (a file in tests/), not a test name. -Filter matches TEST NAMES as a substring."; Color = 'Yellow' }
        $out += [pscustomobject]@{ Text = "[pg] Use:  -TestTarget $Filter    (compiles and links ONLY that binary -- much faster than the whole package)"; Color = 'Green' }
        return $out
    }

    $out += [pscustomobject]@{ Text = '[pg] -Filter matches TEST NAMES as a substring -- never file names, never test-target names.'; Color = 'Yellow' }
    $out += [pscustomobject]@{ Text = '[pg] To run one test FILE use -TestTarget <file-stem>; to run tests by name keep -Filter and check the spelling.'; Color = 'Yellow' }
    $near = @($TestTargets | Where-Object { $_ -like "*$Filter*" -or $Filter -like "*$_*" } | Select-Object -First 5)
    if ($near.Count -gt 0) {
        $out += [pscustomobject]@{ Text = "[pg] Did you mean -TestTarget one of: $($near -join ', ')"; Color = 'Green' }
    }
    return $out
}

function Assert-ScriptAndCwdAgreeOnWorktree {
    <#
      .DESCRIPTION
      Refuse when the invoked script's worktree and the CWD-resolved worktree differ. Returns
      silently when they agree, when either cannot be resolved (never convert "I could not look" into
      a refusal -- that is the same error class in the other direction), or when
      PANGLOSS_ALLOW_WORKTREE_MISMATCH=1 is set deliberately.
    #>
    param([Parameter(Mandatory)][string]$ScriptRoot)

    $scriptRepo = try { (Resolve-Path (Join-Path $ScriptRoot '..\..')).Path } catch { $null }
    $cwdRepo = try { (Resolve-Path (Get-RepoRoot)).Path } catch { $null }
    if (-not $scriptRepo -or -not $cwdRepo) { return }
    if ($scriptRepo.TrimEnd('\', '/') -ieq $cwdRepo.TrimEnd('\', '/')) { return }
    if ($env:PANGLOSS_ALLOW_WORKTREE_MISMATCH -eq '1') {
        Write-Host "[build-env] WARNING: script worktree '$scriptRepo' != cwd worktree '$cwdRepo' -- allowed by PANGLOSS_ALLOW_WORKTREE_MISMATCH=1"
        return
    }

    Write-Host "[pg] REFUSING: the script and the current directory belong to different worktrees."
    Write-Host "[pg]   script:  $scriptRepo"
    Write-Host "[pg]   cwd:     $cwdRepo"
    Write-Host "[pg] The build would have used the CWD tree ('$(Split-Path $cwdRepo -Leaf)'), not the one you named."
    Write-Host "[pg] Fix: Set-Location '$scriptRepo' first, or invoke that worktree's own rust\tools\pg.ps1."
    Write-Host "[pg] exit $script:ExitCodeWorktreeMismatch means exactly this -- nothing is wrong with the machine or either tree."
    exit $script:ExitCodeWorktreeMismatch
}

# --- Worktree metadata: the exact-base contract. See this file's own header. ---

function Get-WorktreeMetaPath {
    # Gitignored: per-worktree, machine-local record of what commit this worktree was built from.
    param([string]$RepoRoot = (Get-RepoRoot))
    return Join-Path $RepoRoot '.pangloss-worktree.json'
}

function Write-WorktreeMeta {
    <#
      .DESCRIPTION
      Called by the worktree bootstrap command at creation time, once, with the revision it was asked
      to create from -- both as typed ($RequestedRevision, e.g. a branch name) and as resolved to a
      full object ID. Recording BOTH is what lets a later mismatch report be useful ("you asked for
      main, main has since moved, you're still on <object id>"); recording only one loses half of that.
    #>
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
    <#
      .DESCRIPTION
      Absence is the COMMON case and must not be an error -- callers (Test-WorktreeBase) treat $null
      as "unverified", never as a failure. A corrupt/partially-written file folds into the same $null
      return, for the same reason: a preflight check must never crash the build over its own diagnostic.
    #>
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
    <#
      .DESCRIPTION
      strict/development/off semantics are documented in this file's own header. Never checks out or
      rebases anything itself: either action can silently discard context or invalidate a build cache
      the caller was relying on.
    #>
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

# --- Target ownership. See this file's own header for the marker schema and gc classification. ---

function Get-TargetOwnershipPath {
    param([Parameter(Mandatory)][string]$TargetDir)
    return Join-Path $TargetDir '.pangloss-owner.json'
}

function Write-TargetOwnership {
    # Refuses to silently adopt a target dir whose marker names a different repository_id -- see this file's own header.
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
        # created_utc survives every rewrite: it is the target dir's age, not this invocation's start time.
        if ($existing -and $existing.created_utc) { $createdUtc = $existing.created_utc }
    }
    # $isPreserved, not $preserved: PowerShell variable names are case-insensitive, colliding with the -Preserved switch above.
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

# --- sccache health ---

function Test-SccacheHealth {
    <#
      .DESCRIPTION
      Three states, not two: "not installed" is normal local-dev (falls back to an uncached build);
      "installed but --show-stats fails" means something IS on PATH named sccache but can't actually
      talk to its cache (bad SCCACHE_DIR permissions, a stale/corrupt cache, a wrapped compiler
      mismatch) -- that state must FAIL the build, since silently proceeding uncached is exactly how
      "sccache active" claims in a build log stop being trustworthy.
    #>
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
    # Summarize, don't dump: every field below is OPTIONAL, since a freshly started server reports none of them.
    $text = ($stats | Out-String)
    $hitRate = ([regex]::Match($text, 'Cache hits rate\s+([\d.]+\s*%)')).Groups[1].Value
    $size = ([regex]::Match($text, 'Cache size\s+(.+)')).Groups[1].Value.Trim()
    $maxSize = ([regex]::Match($text, 'Max cache size\s+(.+)')).Groups[1].Value.Trim()
    $summary = 'responding'
    if ($hitRate) { $summary += ", hit rate $hitRate" }
    if ($size -and $maxSize) { $summary += ", cache $size / $maxSize" }
    return [PSCustomObject]@{ State = 'healthy'; Ok = $true; Detail = $summary; RawStats = $text }
}

# --- Corpus manifest: mirrors pg_conformance_fixtures::corpus's Rust-side reader, same "present"/"required" semantics. ---

function Get-CorpusManifest {
    param([string]$RepoRoot = (Get-RepoRoot))
    $path = Join-Path $RepoRoot 'rust\tools\corpus-manifest.json'
    if (-not (Test-Path $path)) { throw "corpus manifest not found: $path" }
    return Get-Content $path -Raw | ConvertFrom-Json
}

function Get-CorpusRoot {
    <#
      .DESCRIPTION
      PANGLOSS_CORPUS_ROOT overrides the manifest's own corpus_root, exactly like
      pg_conformance_fixtures::corpus::corpus_root() on the Rust side -- a linked worktree can point
      this at an external corpus location instead of copying gigabytes of private data per worktree.
    #>
    param([string]$RepoRoot = (Get-RepoRoot), $Manifest)
    if ($env:PANGLOSS_CORPUS_ROOT) { return $env:PANGLOSS_CORPUS_ROOT }
    if (-not $Manifest) { $Manifest = Get-CorpusManifest -RepoRoot $RepoRoot }
    return Join-Path $RepoRoot ($Manifest.corpus_root -replace '/', '\')
}

function Test-CorpusPresent {
    <#
      .DESCRIPTION
      Validates every REQUIRED manifest file before cargo starts. Digests are truncated to the first
      12 hex chars of SHA-256 -- enough to catch "this isn't the file you think it is" across
      machines/runs without printing a full 64-char hash into every build log.
    #>
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

# Conformance submodule (machine/conformance) -- sparse, path-scoped auto-init.
# docs/research/build-resource-governance.md

function Get-ConformanceSubmoduleSentinel {
    # Proof the working tree is actually usable, not just that `machine/` exists: `clone --no-checkout` leaves only a gitlink.
    param([string]$RepoRoot = (Get-RepoRoot))
    return (Join-Path $RepoRoot 'machine\conformance\constructs.txt')
}

function Test-ConformanceSubmodulePresent {
    # The fast, idempotent check every caller does FIRST: the common case must cost exactly one Test-Path call.
    param([string]$RepoRoot = (Get-RepoRoot))
    return (Test-Path (Get-ConformanceSubmoduleSentinel -RepoRoot $RepoRoot) -PathType Leaf)
}

function Get-ConformancePinnedCommit {
    <#
      .DESCRIPTION
      The gitlink SHA the SUPERPROJECT's own tree records for `machine` -- read from `git ls-tree`,
      never `.gitmodules`' branch name, since a branch can move but the tree entry cannot. Returns
      $null (never throws) on any failure; the caller folds an unresolved pin into its own actionable
      message.
    #>
    param([string]$RepoRoot = (Get-RepoRoot))
    $out = & git -C $RepoRoot ls-tree HEAD -- machine 2>$null
    if (-not $out) { return $null }
    # Format: "160000 commit <40-hex-sha>\tmachine" (160000 = gitlink mode).
    if ($out -match '^\d+\s+commit\s+([0-9a-f]{40})\b') { return $Matches[1] }
    return $null
}

function Get-ConformanceSubmoduleSizeMB {
    # Reported alongside a successful init so the record states what happened (sparse ~3.6MB vs. fallback ~41MB), not just "ok".
    param([string]$RepoRoot = (Get-RepoRoot))
    $dir = Join-Path $RepoRoot 'machine'
    if (-not (Test-Path $dir)) { return 0 }
    $bytes = (Get-ChildItem $dir -Recurse -Force -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum
    if (-not $bytes) { return 0 }
    return [math]::Round($bytes / 1MB, 1)
}

function Initialize-ConformanceSubmodule {
    <#
      .DESCRIPTION
      Makes `machine/conformance` show up without anyone running `git submodule update` by hand.
      Callers: pg.ps1 preflight (fail-CLOSED with $script:ExitCodeConformanceSubmoduleMissing before
      Cargo starts if this returns Ok=$false), `pg.ps1 -Mode new-worktree` (best-effort, so a fresh
      worktree is born ready), `-Mode doctor` (reports the state), and rust/tools/conformance.ps1 (a
      standalone front end onto this same function).

      Returns a PSCustomObject with:
        Ok               bool -- $true means machine/conformance/constructs.txt exists NOW.
        AlreadyPresent   bool -- $true means the fast path fired; nothing was invoked.
        Mode             'already-present' | 'sparse' | 'fallback-full' | 'failed'
        Detail           human-readable summary, always safe to print as-is.
        RecoveryCommand  the exact command to run by hand; '' when Ok (nothing to recover).
    #>
    param([string]$RepoRoot = (Get-RepoRoot))

    # 1. Fast idempotent path FIRST, before touching git at all.
    if (Test-ConformanceSubmodulePresent -RepoRoot $RepoRoot) {
        return [PSCustomObject]@{
            Ok = $true; AlreadyPresent = $true; Mode = 'already-present'
            Detail          = 'machine/conformance/constructs.txt present -- submodule already initialized (no git invoked)'
            RecoveryCommand = ''
        }
    }

    $machineDir = Join-Path $RepoRoot 'machine'
    # Named once so every failure branch below quotes the identical recovery command.
    $fullFallbackCmd = "git -C `"$RepoRoot`" submodule update --init -- machine"

    $pinned = Get-ConformancePinnedCommit -RepoRoot $RepoRoot
    if (-not $pinned) {
        return [PSCustomObject]@{
            Ok = $false; AlreadyPresent = $false; Mode = 'failed'
            Detail          = "could not resolve the machine submodule's pinned commit (git -C `"$RepoRoot`" ls-tree HEAD -- machine returned nothing) -- is .gitmodules / the machine gitlink present at all? Run by hand: $fullFallbackCmd"
            RecoveryCommand = $fullFallbackCmd
        }
    }

    Write-Host "[conformance] machine/conformance not found -- initializing the machine submodule (sparse: conformance/ only, ~3.6MB, not the ~41MB full checkout)..." -ForegroundColor Cyan

    # 2. Sparse, path-scoped init -- skip cloning if a machine/.git gitlink shows an earlier attempt got that far.
    $alreadyCloned = Test-Path (Join-Path $machineDir '.git')
    if (-not $alreadyCloned) {
        # Harmless no-op if already registered; keeps `git submodule status`/`sync`/`foreach` working normally.
        & git -C $RepoRoot submodule init -- machine 2>&1 | Out-Null

        $gitmodulesPath = Join-Path $RepoRoot '.gitmodules'
        $url = (& git config -f $gitmodulesPath --get submodule.machine.url 2>$null)
        $branch = (& git config -f $gitmodulesPath --get submodule.machine.branch 2>$null)
        if (-not $url) {
            return [PSCustomObject]@{
                Ok = $false; AlreadyPresent = $false; Mode = 'failed'
                Detail          = "could not read submodule.machine.url from $gitmodulesPath -- run by hand: $fullFallbackCmd"
                RecoveryCommand = $fullFallbackCmd
            }
        }

        # Worktree-scoped modules/ location, matching `git submodule update`'s own layout, so two worktrees never contend for one gitdir.
        $absoluteGitDir = (& git -C $RepoRoot rev-parse --absolute-git-dir 2>$null)
        if (-not $absoluteGitDir) {
            return [PSCustomObject]@{
                Ok = $false; AlreadyPresent = $false; Mode = 'failed'
                Detail          = "could not resolve this worktree's git-dir (git rev-parse --absolute-git-dir) -- run by hand: $fullFallbackCmd"
                RecoveryCommand = $fullFallbackCmd
            }
        }
        $modulesDir = Join-Path $absoluteGitDir.Trim() 'modules'
        New-Item -ItemType Directory -Force -Path $modulesDir | Out-Null
        $targetGitDir = Join-Path $modulesDir 'machine'

        $cloneArgs = @('clone', '--no-checkout', '--separate-git-dir', $targetGitDir)
        if ($branch) { $cloneArgs += @('--branch', $branch) }
        $cloneArgs += @($url, $machineDir)
        $cloneOut = & git @cloneArgs 2>&1
        if ($LASTEXITCODE -ne 0) {
            # Likely no network reachable; name the exact recovery command so offline reads as legible, not "fine".
            return [PSCustomObject]@{
                Ok = $false; AlreadyPresent = $false; Mode = 'failed'
                Detail          = "git clone of the machine submodule failed (exit $LASTEXITCODE): $($cloneOut -join ' | ') -- if this is a network error, initialization cannot happen offline; run once connectivity is available: $fullFallbackCmd"
                RecoveryCommand = $fullFallbackCmd
            }
        }
    }

    # 3. Cone-mode sparse-checkout, then materialize just the pinned commit's conformance/ subtree.
    $sparseOk = $true
    $sparseErr = ''
    $o1 = & git -C $machineDir sparse-checkout init --cone 2>&1
    if ($LASTEXITCODE -ne 0) { $sparseOk = $false; $sparseErr = "sparse-checkout init --cone (exit $LASTEXITCODE): $($o1 -join ' | ')" }
    if ($sparseOk) {
        $o2 = & git -C $machineDir sparse-checkout set conformance 2>&1
        if ($LASTEXITCODE -ne 0) { $sparseOk = $false; $sparseErr = "sparse-checkout set conformance (exit $LASTEXITCODE): $($o2 -join ' | ')" }
    }
    if ($sparseOk) {
        # `clone --branch` fetches only that branch, and a pinned commit the branch has since moved past is then absent -- the same hazard this recipe already avoids by not using `--depth`. Fetching the SHA by name is cheap when it is already present.
        $o3 = & git -C $machineDir fetch --no-tags origin $pinned 2>&1
        if ($LASTEXITCODE -ne 0) { $sparseOk = $false; $sparseErr = "fetch $($pinned.Substring(0, 12)) (exit $LASTEXITCODE): $($o3 -join ' | ')" }
    }
    if ($sparseOk) {
        $o4 = & git -C $machineDir checkout $pinned 2>&1
        if ($LASTEXITCODE -ne 0) { $sparseOk = $false; $sparseErr = "checkout $($pinned.Substring(0, 12)) (exit $LASTEXITCODE): $($o4 -join ' | ')" }
    }

    if (-not $sparseOk) {
        # A working checkout beats a broken clever one, rather than leaving the submodule half-initialized.
        Write-Host "[conformance] sparse checkout failed ($sparseErr) -- falling back to `git submodule update --init`." -ForegroundColor Yellow
        $fbOut = & git -C $RepoRoot submodule update --init -- machine 2>&1
        if ($LASTEXITCODE -ne 0 -or -not (Test-ConformanceSubmodulePresent -RepoRoot $RepoRoot)) {
            return [PSCustomObject]@{
                Ok = $false; AlreadyPresent = $false; Mode = 'failed'
                Detail          = "sparse checkout failed ($sparseErr) AND the full-checkout fallback also failed (exit $LASTEXITCODE): $($fbOut -join ' | ') -- run by hand: $fullFallbackCmd"
                RecoveryCommand = $fullFallbackCmd
            }
        }
        $sizeMB = Get-ConformanceSubmoduleSizeMB -RepoRoot $RepoRoot
        return [PSCustomObject]@{
            Ok = $true; AlreadyPresent = $false; Mode = 'fallback-full'
            Detail          = "sparse checkout failed ($sparseErr) -- fell back to a full submodule checkout (~${sizeMB}MB). Sparse cone mode may not work in this git/environment; investigate before assuming every worktree gets the cheap path."
            RecoveryCommand = ''
        }
    }

    if (-not (Test-ConformanceSubmodulePresent -RepoRoot $RepoRoot)) {
        return [PSCustomObject]@{
            Ok = $false; AlreadyPresent = $false; Mode = 'failed'
            Detail          = "sparse submodule init reported success but $(Get-ConformanceSubmoduleSentinel -RepoRoot $RepoRoot) is still missing -- something is wrong beyond a network failure; run by hand: $fullFallbackCmd"
            RecoveryCommand = $fullFallbackCmd
        }
    }

    $sizeMB = Get-ConformanceSubmoduleSizeMB -RepoRoot $RepoRoot
    return [PSCustomObject]@{
        Ok = $true; AlreadyPresent = $false; Mode = 'sparse'
        Detail          = "sparse checkout of machine/conformance complete (~${sizeMB}MB, cone mode -- not the ~41MB full checkout)"
        RecoveryCommand = ''
    }
}

# --- Disk reserve: pure decision logic, unit-testable without touching a real disk. ---

function Test-DiskReserve {
    <#
      .DESCRIPTION
      A separate, lower bar than Resolve-TargetDir's SSD/HDD placement preference: that one picks
      where to build, this one is the last-resort "the chosen drive is nearly full" safety gate that
      must reject the build outright. [Nullable[double]], not [double]: a plain [double] parameter
      would coerce a passed $null to 0.0, making "unknown" indistinguishable from "0GB free".
    #>
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

function Get-StaleWorktreeCandidates {
    <#
      .DESCRIPTION
      Worktrees whose disk is reclaimable: every change committed, and no commit for -IdleDays.

      Removing a worktree does not delete its branch, so committed work survives and is recoverable
      with a plain checkout. The only thing a removal can destroy is uncommitted or untracked
      content -- which is exactly why a worktree with ANY dirty or untracked path is never listed
      here, however old it is. A judgement call on one of those belongs to a human.

      Deliberately does not measure directory sizes: this runs on a refusal path, and walking every
      file of dozens of checkouts would add minutes to a message whose whole purpose is to be read
      quickly. Size varies mostly with leftover build output, so the oldest are usually the largest.
    #>
    param(
        [Parameter(Mandatory)][string]$RepoRoot,
        [int]$IdleDays = 3
    )
    $found = @()
    $listing = & git -C $RepoRoot worktree list --porcelain 2>$null
    if ($LASTEXITCODE -ne 0) { return $found }
    $cutoff = (Get-Date).AddDays(-$IdleDays)
    foreach ($line in $listing) {
        if ($line -notmatch '^worktree (.+)$') { continue }
        $path = $Matches[1]
        if ((Resolve-Path -LiteralPath $path -ErrorAction SilentlyContinue).ProviderPath -eq (Resolve-Path -LiteralPath $RepoRoot).ProviderPath) { continue }
        $dirty = & git -C $path status --porcelain --untracked-files=all 2>$null
        if ($LASTEXITCODE -ne 0 -or $dirty) { continue }
        $stamp = & git -C $path log -1 --format=%cI 2>$null
        if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($stamp)) { continue }
        $when = $null
        try { $when = [datetime]::Parse($stamp) } catch { continue }
        if ($when -ge $cutoff) { continue }
        $found += [PSCustomObject]@{
            Path     = $path
            Name     = Split-Path $path -Leaf
            Branch   = (& git -C $path rev-parse --abbrev-ref HEAD 2>$null)
            IdleDays = [int]((Get-Date) - $when).TotalDays
        }
    }
    return @($found | Sort-Object IdleDays -Descending)
}

function Get-RegisteredWorktreePaths {
    <#
      .DESCRIPTION
      Every path `git worktree list` knows about for this repository, main checkout FIRST (git's own
      documented ordering), so a caller can both test registration and identify the main checkout
      without a second git invocation.
    #>
    param([Parameter(Mandatory)][string]$RepoRoot)
    $paths = @()
    $listing = & git -C $RepoRoot worktree list --porcelain 2>$null
    if ($LASTEXITCODE -ne 0) { return $paths }
    foreach ($line in $listing) {
        if ($line -match '^worktree (.+)$') { $paths += $Matches[1] }
    }
    return $paths
}

function Resolve-ComparablePath {
    # git prints worktree paths with forward slashes on Windows, so every path comparison here has to normalize first.
    param([string]$Path)
    $resolved = (Resolve-Path -LiteralPath $Path -ErrorAction SilentlyContinue)
    if (-not $resolved) { return $null }
    return $resolved.ProviderPath.TrimEnd([System.IO.Path]::DirectorySeparatorChar)
}

function Remove-ManagedWorktree {
    <#
      .DESCRIPTION
      Removes ONE worktree and reclaims the target directories that removal unlocks. The two halves
      belong together: Get-TargetClassification calls a target dir `live` for exactly as long as its
      worktree is registered, so the checkout -- usually a couple of GB -- is what stands between
      several times that much build output and reclamation.

      Refuses on ANY uncommitted or untracked path, with no override parameter. Removing a worktree
      does not delete its branch, so committed work stays recoverable by checkout; uncommitted work
      is not recoverable at all, and that judgement belongs to a human rather than to a disk-pressure
      sweep. The dirty check is character-for-character the one Get-StaleWorktreeCandidates uses, so
      a worktree that sweep offers can never be one this refuses.

      Deletes the directory and prunes rather than calling `git worktree remove`, which refuses
      outright on a worktree containing a submodule ("working trees containing submodules cannot be
      moved or removed") -- and `new-worktree` initializes `machine` in every worktree it creates.
      `git submodule deinit -f` first does not change that answer.

      Dry run is the default: without -Apply nothing is deleted and nothing is pruned, and the
      reported target dirs are what removal WOULD free -- classified against the live-slug list this
      worktree has already been subtracted from, since otherwise a preview of its own targets could
      only ever say `live`.
    #>
    param(
        [Parameter(Mandatory)][string]$RepoRoot,
        [Parameter(Mandatory)][string]$Path,
        [switch]$Apply,
        [string]$RepositoryId,
        [string[]]$Roots = @($script:SsdCacheRoot, $script:HddCacheRoot),
        # Bound explicitly by tests; otherwise sampled here, matching what gc passes Invoke-TargetGc.
        [object[]]$BusyProcesses
    )
    $result = [ordered]@{
        Ok               = $false
        Refusal          = ''
        Detail           = ''
        Path             = $Path
        Slug             = ''
        Branch           = ''
        Applied          = [bool]$Apply
        DirectoryRemoved = $false
        Pruned           = $false
        DirtyPaths       = @()
        Targets          = @()
        TargetsRemoved   = @()
        TargetsFreedGB   = 0.0
        TargetSkipReason = ''
    }

    $full = Resolve-ComparablePath -Path $Path
    if (-not $full) {
        $result.Refusal = 'missing-path'
        $result.Detail = "no such path: $Path"
        return [PSCustomObject]$result
    }
    $result.Path = $full
    $result.Slug = Split-Path $full -Leaf

    $registered = @(Get-RegisteredWorktreePaths -RepoRoot $RepoRoot | ForEach-Object { Resolve-ComparablePath -Path $_ } | Where-Object { $_ })
    if ($registered.Count -eq 0) {
        $result.Refusal = 'unregistered'
        $result.Detail = "could not read `git worktree list` for $RepoRoot -- refusing to delete a directory this repository cannot confirm it owns"
        return [PSCustomObject]$result
    }
    if ($full -eq $registered[0]) {
        $result.Refusal = 'main-checkout'
        $result.Detail = "$full is this repository's main checkout, not a worktree"
        return [PSCustomObject]$result
    }
    # Removing the tree the caller is standing in would delete the running script out from under itself.
    $runningRoot = Resolve-ComparablePath -Path $RepoRoot
    if ($runningRoot -and $full -eq $runningRoot) {
        $result.Refusal = 'running-checkout'
        $result.Detail = "$full is the worktree this command is running from"
        return [PSCustomObject]$result
    }
    if ($registered -notcontains $full) {
        $result.Refusal = 'unregistered'
        $result.Detail = "$full is not registered in `git worktree list` for this repository"
        return [PSCustomObject]$result
    }

    $dirty = @(& git -C $full status --porcelain --untracked-files=all 2>$null)
    if ($LASTEXITCODE -ne 0) {
        $result.Refusal = 'status-unreadable'
        $result.Detail = "`git status` failed in $full -- 'I could not look' is not 'there is nothing to lose'"
        return [PSCustomObject]$result
    }
    if ($dirty.Count -gt 0) {
        $result.Refusal = 'dirty'
        $result.DirtyPaths = $dirty
        $result.Detail = "$($dirty.Count) uncommitted or untracked path(s) in $full"
        return [PSCustomObject]$result
    }
    $result.Branch = (& git -C $full rev-parse --abbrev-ref HEAD 2>$null)

    if (-not $RepositoryId) { $RepositoryId = Get-RepoIdentity -RepoRoot $RepoRoot }
    $liveAfter = @(Get-LiveWorktreeSlugs -RepoRoot $RepoRoot | Where-Object { $_ -ne $result.Slug })
    # Scoped to this slug alone: a targeted removal must never become a machine-wide sweep of every disposable dir.
    $scoped = @(Get-TargetClassification -RepositoryId $RepositoryId -Roots $Roots -LiveSlugs $liveAfter |
        Where-Object { $_.Class -eq 'disposable' -and (Split-Path $_.Path -Leaf) -eq $result.Slug })
    $result.Targets = $scoped
    $sum = ($scoped | Measure-Object SizeGB -Sum).Sum
    $result.TargetsFreedGB = if ($sum) { [math]::Round($sum, 2) } else { 0.0 }

    if (-not $Apply) {
        $result.Ok = $true
        $result.Detail = "dry run -- would remove $full and $($scoped.Count) target dir(s) totalling $($result.TargetsFreedGB)GB"
        return [PSCustomObject]$result
    }

    try {
        Remove-Item -Recurse -Force -LiteralPath $full -ErrorAction Stop
    } catch {
        $result.Refusal = 'delete-failed'
        $result.Detail = "could not delete $full : $($_.Exception.Message)"
        return [PSCustomObject]$result
    }
    $result.DirectoryRemoved = $true

    & git -C $RepoRoot worktree prune 2>&1 | Out-Null
    $result.Pruned = ($LASTEXITCODE -eq 0)

    # Invoke-TargetGc's Mandatory array param rejects an empty $scoped outright, so skip the call.
    if ($scoped.Count -gt 0) {
        $busy = if ($PSBoundParameters.ContainsKey('BusyProcesses')) { @($BusyProcesses) } else { @(Get-LiveBuildProcesses) }
        $gc = Invoke-TargetGc -Classification $scoped -Apply -BusyProcesses $busy -Roots $Roots
        $result.TargetsRemoved = @($gc.Deleted)
        $result.TargetSkipReason = $gc.SkipReason
    }
    $result.Ok = $true
    $result.Detail = "removed $full; reclaimed $($result.TargetsRemoved.Count) of $($scoped.Count) target dir(s)"
    return [PSCustomObject]$result
}

# --- Preflight record ---

function Write-Preflight {
    <#
      .DESCRIPTION
      One record, printed before cargo starts, naming everything an agent or a human would otherwise
      have to reconstruct after the fact from a build log: worktree, commit, target dir, cache state,
      corpus state, disk state, and build slot.
    #>
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
        $ConformanceCheck = $null,
        [int]$MaxConcurrent,
        [int]$RunSlots = 0,
        [int]$Jobs = 0,
        [switch]$JobsExplicit,
        $JobsBudget = $null,
        [double]$PerJobMemoryGB = 0,
        [int]$TestThreads = 0,
        $TestThreadsBudget = $null,
        [string]$Priority = '',
        $HostCgroupProof = $null
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
        $total = if ($HostCgroupProof) { $null } else { Get-TotalMemoryGB }
        $ofTotal = if ($null -ne $total) { " of ${total}GB total" } else { '' }
        Write-Host "free memory: $($MemoryCheck.Detail)$ofTotal" -ForegroundColor $(if ($MemoryCheck.Ok) { 'Gray' } else { 'Red' })
        # Commit charge alongside it, never instead of it -- see Get-CommitChargeGB for why the two diverge.
        $commit = if ($HostCgroupProof) { $null } else { Get-CommitChargeGB }
        if ($commit) {
            $commitColor = if ($commit.PercentUsed -ge 90) { 'Red' } elseif ($commit.PercentUsed -ge 75) { 'Yellow' } else { 'Gray' }
            $commitDetail = if ($HostCgroupProof) {
                'host service owns the cgroup cap; pg does not apply per-process enforcement'
            } else {
                "procgov's --maxjobmem and event-2004 both measure THIS, not available physical"
            }
            Write-Host "commit charge: $($commit.CommittedGB)GB of $($commit.LimitGB)GB limit ($($commit.PercentUsed)% used, $($commit.FreeGB)GB uncommitted) -- $commitDetail" -ForegroundColor $commitColor
        }
    }
    if ($HostCgroupProof) {
        if ($HostCgroupProof.Ok) {
            Write-Host "host cgroup: bounded ($($HostCgroupProof.EffectiveMemoryCapBytes) bytes effective cap) -- $($HostCgroupProof.Detail)" -ForegroundColor Gray
        } else {
            Write-Host "host cgroup: UNAVAILABLE -- $($HostCgroupProof.Detail)" -ForegroundColor Red
        }
    }
    # Diagnostic only (the mutexes are the real exclusion); printed because an anonymous wait looks like a deadlock.
    $slotSnapshot = $null
    foreach ($pool in @('build', 'run')) {
        $slotHolders = @(Get-SlotHolders -Pool $pool)
        if ($slotHolders.Count -eq 0) { continue }
        # Lazy: a process snapshot is not cheap, and the common preflight has no holders to describe.
        if ($null -eq $slotSnapshot) { $slotSnapshot = Get-ProcessSnapshot }
        Write-Host "$pool slots in use:"
        foreach ($h in $slotHolders) {
            # Reported for both pools, but only the build pool is ever reaped -- Remove-StaleBuildSlotHolders walks it alone.
            $stale = $h.Alive -and (Test-BuildSlotHolderStale -Holder $h -Snapshot $slotSnapshot)
            $state = if (-not $h.Alive) { 'NOT ALIVE -- stale ledger entry; the kernel hands this slot to the next waiter' }
                elseif ($stale -and $pool -eq 'build') { "alive since $($h.AcquiredAt) -- STALE: no compiler activity for 20+ min; 'pg.ps1 -Mode gc -Apply' will reap it" }
                elseif ($stale) { "alive since $($h.AcquiredAt) -- no build-shaped activity for 20+ min (not reaped: run slots are not swept)" }
                else { "alive since $($h.AcquiredAt)" }
            Write-Host "  $pool slot $($h.Slot): pid $($h.Pid) ($($h.Mode) in $($h.Worktree)) -- $state" -ForegroundColor $(if (-not $h.Alive -or $stale) { 'Yellow' } else { 'Gray' })
        }
    }
    Write-Host "sccache: $($SccacheHealth.State) -- $($SccacheHealth.Detail)" -ForegroundColor $(if ($SccacheHealth.Ok -or $SccacheHealth.State -eq 'disabled') { 'Gray' } else { 'Red' })
    if ($ConformanceCheck) {
        Write-Host "conformance submodule ($($ConformanceCheck.Mode)): $($ConformanceCheck.Detail)" -ForegroundColor $(if ($ConformanceCheck.Ok) { 'Gray' } else { 'Red' })
    }
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
    $runSlotsShown = if ($RunSlots -gt 0) { $RunSlots } else { $script:DefaultRunSlots }
    Write-Host "slot limits: $MaxConcurrent build, $runSlotsShown run (machine-wide convention -- see Enter-ResourceSlot)"
    if ($Jobs -gt 0) {
        # Printed with its provenance: a derivation is only shown when the number actually came FROM it.
        $why = if ($JobsExplicit) {
            'explicit -Jobs override'
        } elseif ($JobsBudget -and $JobsBudget.Bound -eq 'memory') {
            # Memory can bind the number instead of CPU; the CPU derivation is still true arithmetic but no longer the reason.
            $perJob = if ($PerJobMemoryGB -gt 0) { $PerJobMemoryGB } else { $script:MemoryPerCompileJobGB }
            $ltoNote = if ($perJob -eq $script:MemoryPerLtoLinkJobGB) { ' (fat-LTO link peak)' } else { '' }
            "$($JobsBudget.Detail); ${perJob}GB/job assumed${ltoNote} over a $(Get-InteractiveReserveGB)GB reserve, split across $MaxConcurrent slot(s)"
        } else {
            "$([Environment]::ProcessorCount) logical - $script:InteractiveReserveThreads reserved for SSH/remote-desktop daemons - $($runSlotsShown * $script:RunThreadsPerSlot) reserved for the run pool, split across $MaxConcurrent slot(s)"
        }
        Write-Host "cargo jobs: $Jobs per build ($why)"
    }
    if ($TestThreads -gt 0) {
        # Reported separately from jobs: they bound different phases, and a compile-capped run can still spawn 20-wide.
        $testWhy = if ($TestThreadsBudget -and $TestThreadsBudget.Bound -eq 'memory') {
            " -- $($TestThreadsBudget.Detail), ${script:MemoryPerTestProcessGB}GB/process assumed"
        } else {
            ''
        }
        Write-Host "test threads: $TestThreads concurrent test processes (default would be $([Environment]::ProcessorCount))$testWhy"
    }
    if ($Priority) {
        if ($HostCgroupProof) {
            Write-Host "build priority: $Priority (host-service-owned; unapplied by pg)"
        } else {
            Write-Host "build priority: $Priority (inherited by rustc/link.exe -- keeps interactive daemons ahead of compiler work)"
        }
    }
    Write-Host '-------------------------' -ForegroundColor Cyan
}

# --- gc: marker-aware classification + the actual (side-effecting) deletion step, kept separate. ---

function Get-ManagedTargetDirs {
    # -Roots is a parameter so tests can point this at a temp dir instead of the real cache roots.
    param([Parameter(Mandatory)][string[]]$Roots)
    foreach ($root in $Roots) {
        if (-not (Test-Path $root)) { continue }
        Get-ChildItem $root -Directory -ErrorAction SilentlyContinue | Where-Object { $_.Name -ne 'sccache' }
    }
}

function Get-TargetClassification {
    <#
      .DESCRIPTION
      Five classes -- unknown, other-repo, preserved, live, disposable -- documented in this file's
      own header; only `disposable` is ever a candidate for deletion. -LiveSlugs is a parameter
      (default calls the real Get-LiveWorktreeSlugs) so tests can inject a fixed slug list instead of
      depending on this checkout's actual `git worktree list` output.
    #>
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
    <#
      .DESCRIPTION
      The only function in this file allowed to delete a managed target directory. Dry-run (-Apply
      not passed) is the default and NEVER deletes anything -- $Apply defaults to $false here on
      purpose, not just at the pg.ps1 call site, so a caller that forgets to pass it explicitly fails
      safe.
    #>
    param(
        [Parameter(Mandatory)][object[]]$Classification,
        [switch]$Apply,
        [object[]]$BusyProcesses = @(),
        # Defaults to the same two roots the classifier enumerates, so the containment re-check below matches.
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
        # A live build anywhere is reason enough to abstain entirely: deleting a target it's mid-write to is a race.
        $result.Skipped = $true
        $result.SkipReason = "refusing to delete: $($BusyProcesses.Count) live cargo/rustc/link/sccache process(es) running"
        return [PSCustomObject]$result
    }
    foreach ($d in $disposable) {
        # Re-validate containment at deletion time, guarding a future caller that hand-builds a classification list.
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
            # Compare against "<root>\" so a sibling root sharing a name prefix can never be mistaken for being inside this one.
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

# Load the native implementation only on the host that executes it: Windows never loads the Linux adapter implicitly, and Linux uses the same importer fixture callers do so the production and contract seams cannot drift.
if ($IsLinux) {
    $script:PanGlossPlatformAdapter = Import-PanGlossPlatformAdapter -Platform Linux -ToolRoot $PSScriptRoot
} elseif ($IsWindows) {
    $script:PanGlossPlatformAdapter = Import-PanGlossPlatformAdapter -Platform Windows -ToolRoot $PSScriptRoot
} else {
    $script:PanGlossPlatformAdapter = [PSCustomObject]@{ Platform = 'Unknown'; Overrides = @() }
}
