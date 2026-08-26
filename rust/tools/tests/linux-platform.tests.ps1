<#
  .DESCRIPTION
  Contract tests for the Linux managed-build adapter. These tests deliberately use the adapter's
  injected readers and process callback: they are runnable on Windows, never inspect this machine's
  cgroup or cache, and never start Cargo. The functions named below are the platform seam that
  _common.ps1 must provide when it dispatches at load time on Linux.

  This file is intentionally red until the Linux adapter exists. A missing function is a useful
  failure here; adding a Windows fallback would make the contract pass without proving Linux.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

# _common.ps1 dispatches the native adapter from the real platform. This test runs on Windows too,
# so it uses the same importer with an explicit Linux platform for pure fixture tests. It never
# dot-sources the adapter or lets the Windows implementation stand in for a missing Linux one.
$toolRoot = Split-Path $PSScriptRoot -Parent
$commonPath = Join-Path $toolRoot '_common.ps1'
$linuxImporter = Get-Command Import-PanGlossPlatformAdapter -CommandType Function -ErrorAction SilentlyContinue
$linuxAdapterImportResult = $null
$linuxAdapterImportError = ''
if ($linuxImporter) {
    try {
        $linuxAdapterImportResult = Import-PanGlossPlatformAdapter -Platform Linux -ToolRoot $toolRoot
    } catch {
        $linuxAdapterImportError = $_.Exception.Message
    }
}

function Assert-Throws {
    param(
        [Parameter(Mandatory)][scriptblock]$Body,
        [string]$Message = 'expected the operation to throw',
        [string]$CommandName = ''
    )
    if ($CommandName -and $null -eq (Get-Command $CommandName -CommandType Function -ErrorAction SilentlyContinue)) {
        throw "missing contract function: $CommandName"
    }
    $threw = $false
    try { & $Body } catch { $threw = $true }
    if (-not $threw) { throw $Message }
}

function New-FixtureFile {
    param([Parameter(Mandatory)][string]$Root, [Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][string]$Text)
    $path = Join-Path $Root $Name
    Set-Content -LiteralPath $path -Value $Text -NoNewline -Encoding utf8
    return $path
}

function Assert-LinuxAdapterReady {
    if (-not $linuxImporter) {
        throw 'missing contract importer: _common.ps1 must expose Import-PanGlossPlatformAdapter'
    }
    if ($linuxAdapterImportError) {
        throw "Linux adapter import failed: $linuxAdapterImportError"
    }
    if ($null -eq $linuxAdapterImportResult) {
        throw 'Linux adapter importer returned no marker'
    }
    Assert-Equal 'Linux' $linuxAdapterImportResult.Platform 'the importer must return the Linux adapter marker'
    foreach ($name in @('Get-AvailableMemoryGB', 'Get-TotalMemoryGB', 'Get-CommitChargeGB', 'Enter-BuildSlot', 'Exit-BuildSlot', 'Invoke-CargoWithReaper', 'Invoke-ProcessInJobObject')) {
        Assert-Contains -Haystack @($linuxAdapterImportResult.Overrides) -Needle $name `
            "the Linux adapter must override the actual shared seam $name"
        Assert-True ($null -ne (Get-Command $name -CommandType Function -ErrorAction SilentlyContinue)) `
            "the imported Linux adapter must expose $name"
    }
}

$fixtureRoot = New-TestTempDir -Prefix 'pg-linux-platform'

try {
    Test-Case 'Linux platform adapter exposes every injected contract seam' {
        Assert-LinuxAdapterReady
        foreach ($name in @(
            'Get-LinuxMemorySnapshot',
            'Get-LinuxHostCgroupPreflight',
            'Enter-BuildSlot',
            'Exit-BuildSlot',
            'Invoke-CargoWithReaper'
        )) {
            Assert-True ($null -ne (Get-Command $name -CommandType Function -ErrorAction SilentlyContinue)) `
                "_common.ps1 must expose $name before the Linux tests can run"
        }
    }

    # --- /proc/meminfo: checked KiB-to-byte conversion, with no live host query. ---

    $meminfoPath = New-FixtureFile -Root $fixtureRoot -Name 'meminfo' -Text @'
MemTotal:       4096 kB
MemAvailable:   1024 kB
CommitLimit:    8192 kB
Committed_AS:   2048 kB
'@

    Test-Case 'Linux memory snapshot converts total, available, and commit values from checked KiB' {
        Assert-LinuxAdapterReady
        $r = Get-LinuxMemorySnapshot -MeminfoPath $meminfoPath
        Assert-Equal ([long]4194304) $r.TotalBytes 'MemTotal must be converted from KiB to bytes'
        Assert-Equal ([long]1048576) $r.AvailableBytes 'MemAvailable must be converted from KiB to bytes'
        Assert-Equal ([long]8388608) $r.CommitLimitBytes 'CommitLimit must be converted from KiB to bytes'
        Assert-Equal ([long]2097152) $r.CommittedBytes 'Committed_AS must be converted from KiB to bytes'
    }

    Test-Case 'Linux memory snapshot rejects malformed or overflowing KiB values' {
        Assert-LinuxAdapterReady
        $required = @(
            @{ Name = 'MemTotal'; Value = 'not-a-number kB' },
            @{ Name = 'MemAvailable'; Value = 'not-a-number kB' },
            @{ Name = 'CommitLimit'; Value = 'not-a-number kB' },
            @{ Name = 'Committed_AS'; Value = 'not-a-number kB' }
        )
        foreach ($badField in $required) {
            $lines = @(
                "MemTotal:       $(if ($badField.Name -eq 'MemTotal') { $badField.Value } else { '4096 kB' })",
                "MemAvailable:   $(if ($badField.Name -eq 'MemAvailable') { $badField.Value } else { '1024 kB' })",
                "CommitLimit:    $(if ($badField.Name -eq 'CommitLimit') { $badField.Value } else { '8192 kB' })",
                "Committed_AS:   $(if ($badField.Name -eq 'Committed_AS') { $badField.Value } else { '2048 kB' })"
            )
            $bad = New-FixtureFile -Root $fixtureRoot -Name "meminfo-bad-$($badField.Name)" -Text ($lines -join "`n")
            Assert-Throws { Get-LinuxMemorySnapshot -MeminfoPath $bad } -CommandName 'Get-LinuxMemorySnapshot' `
                -Message "$($badField.Name) malformed value must fail closed"
        }

        $overflow = New-FixtureFile -Root $fixtureRoot -Name 'meminfo-overflow' -Text @"
MemTotal:       $([long]::MaxValue) kB
MemAvailable:   1024 kB
CommitLimit:    8192 kB
Committed_AS:   2048 kB
"@
        Assert-Throws { Get-LinuxMemorySnapshot -MeminfoPath $overflow } -CommandName 'Get-LinuxMemorySnapshot' `
            -Message 'a KiB-to-byte multiplication overflow must fail closed'

        $duplicate = New-FixtureFile -Root $fixtureRoot -Name 'meminfo-duplicate' -Text @'
MemTotal:       4096 kB
MemTotal:       8192 kB
MemAvailable:   1024 kB
CommitLimit:    8192 kB
Committed_AS:   2048 kB
'@
        Assert-Throws { Get-LinuxMemorySnapshot -MeminfoPath $duplicate } -CommandName 'Get-LinuxMemorySnapshot' `
            -Message 'duplicate required meminfo fields must fail closed'
    }

    # --- Host cgroup proof: all input comes through an injected file reader. ---

    $selfCgroup = "0::/delegated/supervisor`n"
    # The second record is the most-specific visible mount. A correct mapper must use its
    # mountpoint plus the suffix below /delegated, not concatenate the raw hierarchy path.
    $mountInfo = @'
36 29 0:32 / /a/very/long/cgroup/mountpoint rw,nosuid,nodev,noexec,relatime - cgroup2 cgroup rw
37 29 0:33 /delegated /cg\040d rw,nosuid,nodev,noexec,relatime - cgroup2 cgroup rw
'@

    function New-Reader {
        param([hashtable]$Files)
        return {
            param([string]$Path)
            if (-not $Files.ContainsKey($Path)) { throw "fixture has no file: $Path" }
            if ($Files[$Path] -is [System.Exception]) { throw $Files[$Path] }
            return [string]$Files[$Path]
        }.GetNewClosure()
    }

    $validCgroupFiles = @{
        '/cg d/memory.max' = "8388608`n"
        '/cg d/supervisor/memory.max' = "4194304`n"
        '/cg d/supervisor/worker/memory.max' = "max`n"
    }

    Test-Case 'Linux cgroup preflight maps the most-specific mount and reports the finite ancestor cap' {
        Assert-LinuxAdapterReady
        $r = Get-LinuxHostCgroupPreflight -SelfCgroupText "0::/delegated/supervisor/worker`n" -MountInfoText $mountInfo `
            -ReadFile (New-Reader -Files $validCgroupFiles)
        Assert-True $r.Ok $r.Detail
        Assert-Equal ([long]4194304) $r.EffectiveMemoryCapBytes `
            'the wrapper must report the minimum finite cap from mapped visible ancestors'
    }

    Test-Case 'Linux cgroup preflight accepts an unbounded leaf when an ancestor is finite' {
        Assert-LinuxAdapterReady
        $files = @{} + $validCgroupFiles
        $files['/cg d/supervisor/memory.max'] = "max`n"
        $r = Get-LinuxHostCgroupPreflight -SelfCgroupText "0::/delegated/supervisor/worker`n" -MountInfoText $mountInfo `
            -ReadFile (New-Reader -Files $files)
        Assert-True $r.Ok $r.Detail
        Assert-Equal ([long]8388608) $r.EffectiveMemoryCapBytes `
            'a finite parent cap bounds a leaf whose memory.max is max'
    }

    Test-Case 'Linux memory seams use the checked snapshot and preserve shared return shapes' {
        Assert-LinuxAdapterReady
        $snapshotFn = (Get-Command Get-LinuxMemorySnapshot -CommandType Function).ScriptBlock
        try {
            Set-Item Function:\global:Get-LinuxMemorySnapshot -Value {
                [PSCustomObject]@{
                    TotalBytes = [long]8GB; AvailableBytes = [long]3GB
                    CommitLimitBytes = [long]10GB; CommittedBytes = [long]4GB
                }
            }
            Assert-Equal 3.0 (Get-AvailableMemoryGB) 'Linux available memory must be reported in the shared GB shape'
            Assert-Equal 8.0 (Get-TotalMemoryGB) 'Linux total memory must be reported in the shared GB shape'
            $commit = Get-CommitChargeGB
            Assert-Equal 10.0 $commit.LimitGB
            Assert-Equal 6.0 $commit.FreeGB
            Assert-Equal 4.0 $commit.CommittedGB
            Assert-Equal 40 $commit.PercentUsed
        } finally {
            Set-Item Function:\global:Get-LinuxMemorySnapshot -Value $snapshotFn
        }
    }

    Test-Case 'Linux pg preflight is ordered before rustfmt and has a distinct refusal code' {
        $pgText = Get-Content -LiteralPath (Join-Path $toolRoot 'pg.ps1') -Raw
        $commonText = Get-Content -LiteralPath $commonPath -Raw
        $proofAt = $pgText.IndexOf('$linuxHostProof = Get-LinuxHostCgroupPreflight', [StringComparison]::Ordinal)
        $fmtAt = $pgText.IndexOf('Invoke-RustFmt -RustRoot $rustRoot', [StringComparison]::Ordinal)
        Assert-True ($proofAt -ge 0 -and $fmtAt -gt $proofAt) 'Linux host proof must be established before any rustfmt invocation'
        Assert-True $commonText.Contains('ExitCodeLinuxHostContainment') 'Linux host containment must have its own exit code'
        Assert-True $pgText.Contains('-HostCgroupProof $linuxHostProof') `
            'the validated proof must be passed through the preflight/report seam'
    }

    Test-Case 'Linux process seam is the actual Invoke-ProcessInJobObject path and preflights before launch' {
        Assert-LinuxAdapterReady
        $calls = [System.Collections.Generic.List[object]]::new()
        $runner = {
            param([string]$Executable, [string[]]$Arguments, [string]$WorkingDirectory)
            [void]$calls.Add([PSCustomObject]@{ Executable = $Executable; Arguments = @($Arguments); WorkingDirectory = $WorkingDirectory })
            return 23
        }.GetNewClosure()
        $code = Invoke-ProcessInJobObject -Exe 'cargo' -CmdArgs @('build') -WorkingDirectory $fixtureRoot `
            -SelfCgroupText "0::/delegated/supervisor/worker`n" -MountInfoText $mountInfo `
            -ReadFile (New-Reader -Files $validCgroupFiles) -ProcessInvoker $runner
        Assert-Equal 23 $code 'Linux direct process invocation must preserve the injected exit code'
        Assert-Equal 1 $calls.Count 'Linux direct process invocation must launch exactly once'
        Assert-Equal 'cargo' $calls[0].Executable
        Assert-Equal $fixtureRoot $calls[0].WorkingDirectory
    }

    Test-Case 'Linux process seam accepts derived cap arguments while pg rejects explicit run overrides' {
        Assert-LinuxAdapterReady
        $calls = [System.Collections.Generic.List[object]]::new()
        $runner = {
            param([string]$Executable, [string[]]$Arguments, [string]$WorkingDirectory)
            [void]$calls.Add($Executable)
            return 29
        }.GetNewClosure()
        $code = Invoke-ProcessInJobObject -Exe 'cargo' -CmdArgs @('run') -WorkingDirectory $fixtureRoot `
            -JobMemoryGB 10 -CpuRatePercent 50 -SelfCgroupText "0::/delegated/supervisor/worker`n" `
            -MountInfoText $mountInfo -ReadFile (New-Reader -Files $validCgroupFiles) -ProcessInvoker $runner
        Assert-Equal 29 $code 'derived Windows-shaped cap arguments must not block ordinary Linux execution'
        Assert-Equal 1 $calls.Count

        $pgText = Get-Content -LiteralPath (Join-Path $toolRoot 'pg.ps1') -Raw
        $overrideCondition = 'if ($IsLinux -and $Mode -eq ''run'' -and $RunMemoryGB -gt 0)'
        $overrideAt = $pgText.IndexOf($overrideCondition, [StringComparison]::Ordinal)
        $fmtAt = $pgText.IndexOf('Invoke-RustFmt -RustRoot $rustRoot', [StringComparison]::Ordinal)
        Assert-True ($overrideAt -ge 0 -and $fmtAt -gt $overrideAt) `
            'the exact Linux -RunMemoryGB refusal must precede rustfmt/Cargo'
        Assert-True $pgText.Contains('Linux -RunMemoryGB is not supported: the host cgroup owns the cap.') `
            'the Linux refusal must explain that the host cgroup owns the cap'
    }

    Test-Case 'Linux cgroup preflight chooses a finite leaf cap below its ancestors' {
        Assert-LinuxAdapterReady
        $files = @{} + $validCgroupFiles
        $files['/cg d/supervisor/worker/memory.max'] = "2097152`n"
        $r = Get-LinuxHostCgroupPreflight -SelfCgroupText "0::/delegated/supervisor/worker`n" -MountInfoText $mountInfo `
            -ReadFile (New-Reader -Files $files)
        Assert-True $r.Ok $r.Detail
        Assert-Equal ([long]2097152) $r.EffectiveMemoryCapBytes `
            'the lowest finite cap must win even when the leaf is lower than its ancestors'
    }

    Test-Case 'Linux cgroup preflight rejects a missing leaf memory.max' {
        Assert-LinuxAdapterReady
        $files = @{} + $validCgroupFiles
        $files.Remove('/cg d/supervisor/worker/memory.max')
        $r = Get-LinuxHostCgroupPreflight -SelfCgroupText "0::/delegated/supervisor/worker`n" -MountInfoText $mountInfo `
            -ReadFile (New-Reader -Files $files)
        Assert-False $r.Ok 'a missing current cgroup memory.max must fail closed'
    }

    Test-Case 'Linux cgroup preflight rejects a completely unbounded visible ancestor chain' {
        Assert-LinuxAdapterReady
        $files = @{} + $validCgroupFiles
        $files['/cg d/memory.max'] = "max`n"
        $files['/cg d/supervisor/memory.max'] = "max`n"
        $r = Get-LinuxHostCgroupPreflight -SelfCgroupText "0::/delegated/supervisor/worker`n" -MountInfoText $mountInfo `
            -ReadFile (New-Reader -Files $files)
        Assert-False $r.Ok 'the complete visible ancestor chain must contain a finite cap'
    }

    Test-Case 'Linux cgroup preflight rejects malformed or unreadable cgroup data' {
        Assert-LinuxAdapterReady
        $malformed = Get-LinuxHostCgroupPreflight -SelfCgroupText 'not a cgroup record' `
            -MountInfoText $mountInfo -ReadFile (New-Reader -Files $validCgroupFiles)
        Assert-False $malformed.Ok 'malformed /proc/self/cgroup must fail closed'

        $unreadable = @{} + $validCgroupFiles
        $unreadable['/cg d/supervisor/memory.max'] = [System.IO.IOException]::new('permission denied')
        $r = Get-LinuxHostCgroupPreflight -SelfCgroupText "0::/delegated/supervisor/worker`n" -MountInfoText $mountInfo `
            -ReadFile (New-Reader -Files $unreadable)
        Assert-False $r.Ok 'an unreadable ancestor memory.max must fail closed'

        $malformedCap = @{} + $validCgroupFiles
        $malformedCap['/cg d/supervisor/memory.max'] = "not-a-number`n"
        $r = Get-LinuxHostCgroupPreflight -SelfCgroupText "0::/delegated/supervisor/worker`n" -MountInfoText $mountInfo `
            -ReadFile (New-Reader -Files $malformedCap)
        Assert-False $r.Ok 'a malformed ancestor memory.max must fail closed'
    }

    Test-Case 'Linux cgroup preflight rejects ambiguous or absent cgroup2 mount mapping' {
        Assert-LinuxAdapterReady
        $ambiguous = @'
36 29 0:32 / /sys/fs/cgroup rw - cgroup2 cgroup rw
37 29 0:33 / /sys/fs/cgroup2 rw - cgroup2 cgroup rw
'@
        $r = Get-LinuxHostCgroupPreflight -SelfCgroupText $selfCgroup -MountInfoText $ambiguous `
            -ReadFile (New-Reader -Files $validCgroupFiles)
        Assert-False $r.Ok 'tied cgroup2 mount mappings must fail closed'

        $absent = Get-LinuxHostCgroupPreflight -SelfCgroupText $selfCgroup `
            -MountInfoText '35 29 0:31 / /sys/fs/cgroup rw - tmpfs tmpfs rw' `
            -ReadFile (New-Reader -Files $validCgroupFiles)
        Assert-False $absent.Ok 'without a cgroup2 mount the host is not proven bounded'
    }

    # --- Linux build slot: exclusive FileStreams at injected paths through the shared seam. ---

    Test-Case 'Linux build slots honor MaxConcurrent and release owned files through the shared seam' {
        Assert-LinuxAdapterReady
        $lockRoot = Join-Path $fixtureRoot 'build-slots'
        New-Item -ItemType Directory -Force -Path $lockRoot | Out-Null
        $slotA = Enter-BuildSlot -MaxConcurrent 2 -TimeoutSeconds 1 -LockRoot $lockRoot
        Assert-True ($null -ne $slotA) 'the first Linux slot must be acquired'
        Assert-True ($slotA.Stream -is [System.IO.FileStream]) `
            'the returned Linux token must own its FileStream, not only a path or name'
        $probe = $null
        try {
            $probe = [System.IO.File]::Open($slotA.Stream.Name, [System.IO.FileMode]::OpenOrCreate, `
                [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::ReadWrite)
            Assert-Throws { $probe.Lock(0, 1) } `
                'a second stream must not lock the byte range owned by the token'
        } finally {
            if ($probe) { $probe.Dispose() }
        }
        $slotB = $null
        try {
            $slotB = Enter-BuildSlot -MaxConcurrent 2 -TimeoutSeconds 1 -LockRoot $lockRoot
            Assert-True ($null -ne $slotB) 'MaxConcurrent=2 must permit two independently held tokens'
            $slotC = Enter-BuildSlot -MaxConcurrent 2 -TimeoutSeconds 1 -LockRoot $lockRoot
            Assert-Equal $null $slotC 'a third token must time out while both slots are held'

            Exit-BuildSlot -Semaphore $slotA
            $slotC = Enter-BuildSlot -MaxConcurrent 2 -TimeoutSeconds 1 -LockRoot $lockRoot
            Assert-True ($null -ne $slotC) 'releasing one token must permit reacquisition'
            Exit-BuildSlot -Semaphore $slotC
        } finally {
            Exit-BuildSlot -Semaphore $slotA
            Exit-BuildSlot -Semaphore $slotB
        }
    }

    Test-Case 'Exit-BuildSlot tolerates null and repeated release on the Linux token' {
        Assert-LinuxAdapterReady
        Exit-BuildSlot -Semaphore $null
        $lockRoot = Join-Path $fixtureRoot 'build-slots-repeat'
        $slot = Enter-BuildSlot -MaxConcurrent 1 -TimeoutSeconds 1 -LockRoot $lockRoot
        Assert-True ($null -ne $slot)
        Exit-BuildSlot -Semaphore $slot
        Exit-BuildSlot -Semaphore $slot
    }

    Test-Case 'Linux build slots refuse a relative state or lock root' {
        Assert-LinuxAdapterReady
        $oldStateRoot = $env:PANGLOSS_STATE_ROOT
        $oldLocation = Get-Location
        $slot = $null
        try {
            Set-Location -LiteralPath $fixtureRoot
            $env:PANGLOSS_STATE_ROOT = 'relative-state-root'
            Assert-Throws { $slot = Enter-BuildSlot -MaxConcurrent 1 -TimeoutSeconds 1 -LockRoot 'relative-lock-root' } 'a relative LockRoot must fail closed rather than create a separate pool'
            Assert-False (Test-Path -LiteralPath (Join-Path $fixtureRoot 'relative-lock-root')) 'relative LockRoot refusal must not create a pool under the current directory'
            Assert-Throws { $slot = Enter-BuildSlot -MaxConcurrent 1 -TimeoutSeconds 1 } 'a relative PANGLOSS_STATE_ROOT must fail closed when LockRoot is omitted'
            Assert-False (Test-Path -LiteralPath (Join-Path $fixtureRoot 'relative-state-root')) 'relative PANGLOSS_STATE_ROOT refusal must not create a pool under the current directory'
        } finally {
            Exit-BuildSlot -Semaphore $slot
            Set-Location -LiteralPath $oldLocation
            $env:PANGLOSS_STATE_ROOT = $oldStateRoot
        }
    }

    Test-Case 'Linux adapter provides safe path and cache seams without Windows drive defaults' {
        Assert-LinuxAdapterReady
        foreach ($name in @('Get-FreeSpaceGB', 'Resolve-TargetDir', 'Use-Sccache')) {
            Assert-True ($null -ne (Get-Command $name -CommandType Function -ErrorAction SilentlyContinue)) "Linux adapter must expose $name"
            Assert-Contains -Haystack @($linuxAdapterImportResult.Overrides) -Needle $name "Linux importer must register the $name override before it is exercised"
        }
        $linuxTarget = '/var/tmp/pangloss-fixture-target'
        $oldTarget = $env:CARGO_TARGET_DIR
        $oldSccache = $env:SCCACHE_DIR
        $oldTargetRoot = $env:PANGLOSS_TARGET_ROOT
        $oldCacheRoot = $env:PANGLOSS_CARGO_CACHE_ROOT
        $oldSsdRoot = $env:PANGLOSS_SSD_CACHE_ROOT
        $oldWrapper = $env:RUSTC_WRAPPER
        $oldCacheSize = $env:SCCACHE_CACHE_SIZE
        try {
            $env:CARGO_TARGET_DIR = $linuxTarget
            $resolved = Resolve-TargetDir -RustRoot '/srv/pangloss/rust'
            Assert-Equal $linuxTarget $resolved 'explicit Linux target root must be preserved'
            Assert-False ($resolved -match '^[A-Za-z]:[\\/]') 'Linux target must not acquire a Windows drive literal'
            $env:SCCACHE_DIR = '/var/tmp/pangloss-fixture-cache'
            $space = Get-FreeSpaceGB -Path $fixtureRoot
            Assert-True ($null -eq $space -or $space -is [double] -or $space -is [decimal] -or $space -is [int]) 'Linux free-space seam must return a number or unavailable result'
            $env:RUSTC_WRAPPER = ''
            [void](Use-Sccache -CommandResolver { 'fake-sccache' } -DirectoryCreator { param([string]$Path) })
            Assert-False ($env:SCCACHE_DIR -match '^[A-Za-z]:[\\/]') 'Linux cache root must not default to a Windows drive literal'
            $env:CARGO_TARGET_DIR = $null
            $env:PANGLOSS_TARGET_ROOT = $null
            $env:PANGLOSS_SSD_CACHE_ROOT = $null
            $env:PANGLOSS_CARGO_CACHE_ROOT = $null
            $env:SCCACHE_DIR = $null
            $defaultTarget = Resolve-TargetDir -RustRoot '/srv/pangloss/rust'
            Assert-True ($null -eq $defaultTarget -or ($defaultTarget.StartsWith('/') -and $defaultTarget -notmatch '^[A-Za-z]:[\\/]')) 'Linux default target must be null or a canonical non-Windows absolute path'
            $useParams = (Get-Command Use-Sccache -CommandType Function).Parameters
            Assert-True ($useParams.ContainsKey('CommandResolver') -and $useParams.ContainsKey('DirectoryCreator')) 'Use-Sccache must expose injected command and directory seams for pure tests'
            $resolveParams = (Get-Command Resolve-TargetDir -CommandType Function).Parameters
            Assert-True $resolveParams.ContainsKey('DirectoryCreator') 'Resolve-TargetDir must expose an injected directory seam for pure tests'
            $created = [System.Collections.Generic.List[string]]::new()
            $env:PANGLOSS_CARGO_CACHE_ROOT = '/var/tmp/pangloss-explicit-cache'
            $env:SCCACHE_DIR = $null
            $targetCreated = [System.Collections.Generic.List[string]]::new()
            $fallback = Resolve-TargetDir -RustRoot '/srv/pangloss/rust' -DirectoryCreator { param([string]$Path) [void]$targetCreated.Add($Path) }
            Assert-True ($fallback.StartsWith('/var/tmp/pangloss-explicit-cache/') -and $fallback -notmatch '^[A-Za-z]:[\\/]') 'explicit Linux cargo cache root must provide a non-Windows target fallback'
            Assert-Equal 1 $targetCreated.Count 'target fallback must use the injected creator exactly once'
            [void](Use-Sccache -CommandResolver { 'fake-sccache' } -DirectoryCreator { param([string]$Path) [void]$created.Add($Path) })
            Assert-True ($created.Count -eq 1 -and $created[0].StartsWith('/') -and $created[0] -notmatch '^[A-Za-z]:[\\/]') 'explicit Linux cache root must produce a non-Windows sccache directory'
            Assert-True ($created.Count -eq 0 -or ($created[0].StartsWith('/') -and $created[0] -notmatch '^[A-Za-z]:[\\/]')) 'Linux default sccache root must not be a Windows drive path'
            $env:RUSTC_WRAPPER = 'fixture-wrapper'
            $env:SCCACHE_DIR = '/var/tmp/original-sccache'
            $env:SCCACHE_CACHE_SIZE = '17G'
            Assert-Throws { Use-Sccache -CommandResolver { 'fake-sccache' } -DirectoryCreator { throw 'creator fixture failure' } } 'sccache directory creation failure must fail closed'
            Assert-Equal 'fixture-wrapper' $env:RUSTC_WRAPPER 'failed sccache setup must restore RUSTC_WRAPPER'
            Assert-Equal '/var/tmp/original-sccache' $env:SCCACHE_DIR 'failed sccache setup must restore SCCACHE_DIR'
            Assert-Equal '17G' $env:SCCACHE_CACHE_SIZE 'failed sccache setup must restore SCCACHE_CACHE_SIZE'
        } finally {
            $env:CARGO_TARGET_DIR = $oldTarget
            $env:SCCACHE_DIR = $oldSccache
            $env:PANGLOSS_TARGET_ROOT = $oldTargetRoot
            $env:PANGLOSS_CARGO_CACHE_ROOT = $oldCacheRoot
            $env:PANGLOSS_SSD_CACHE_ROOT = $oldSsdRoot
            $env:RUSTC_WRAPPER = $oldWrapper
            $env:SCCACHE_CACHE_SIZE = $oldCacheSize
        }
    }

    Test-Case 'Host cgroup proof report names host-owned scheduling without Windows priority claims' {
        Assert-LinuxAdapterReady
        $proof = [PSCustomObject]@{ Ok = $true; EffectiveMemoryCapBytes = 4194304; Detail = 'fixture host cgroup' }
        $base = [PSCustomObject]@{ Checked = $true; Ok = $true; Detail = 'fixture'; Expected = ''; Actual = '' }
        $sccache = [PSCustomObject]@{ Ok = $true; Detail = 'fixture' }
        $disk = [PSCustomObject]@{ Ok = $true; Detail = 'fixture' }
        Assert-Contains -Haystack @($linuxAdapterImportResult.Overrides) -Needle 'Get-BuildSlotHolders' 'Linux importer must override build-holder census without Windows CIM'
        function global:Get-BuildSlotHolders { @() }
        $memory = [PSCustomObject]@{ Ok = $true; Detail = 'fixture memory' }
        $repoForReport = Split-Path (Split-Path $toolRoot -Parent) -Parent
        $text = (Write-Preflight -Mode build -Profile debug -RepoRoot $repoForReport -TargetDir '/var/tmp/target' -BaseCheck $base -SccacheHealth $sccache -FreeGB 1 -DiskCheck $disk -MemoryCheck $memory -MaxConcurrent 2 -Priority BelowNormal -HostCgroupProof $proof *>&1 | Out-String)
        Assert-True ($text -match 'host-service-owned|unapplied') 'Linux host proof report must say scheduling priority is host-service-owned/unapplied'
        Assert-False ($text -match 'procgov|event-2004') 'Linux host proof report must not claim Windows procgov/event-2004 enforcement anywhere'
    }

    Test-Case 'Unsupported platforms and Linux gc refuse before platform-specific work' {
        $common = Get-Content (Join-Path $toolRoot '_common.ps1') -Raw
        $pg = Get-Content (Join-Path $toolRoot 'pg.ps1') -Raw
        $unsupportedAt = $pg.IndexOf('if (-not $IsWindows -and -not $IsLinux)', [StringComparison]::Ordinal)
        $repoAt = $pg.IndexOf('$repoRoot = Get-RepoRoot', [StringComparison]::Ordinal)
        Assert-True ($unsupportedAt -ge 0 -and $unsupportedAt -lt $repoAt) 'unsupported platforms must refuse before repository/path work'
        $resolveAt = $pg.IndexOf('Resolve-TargetDir', [StringComparison]::Ordinal)
        $preflightAt = $pg.IndexOf('Write-Preflight', [StringComparison]::Ordinal)
        Assert-True ($unsupportedAt -lt $resolveAt -and $unsupportedAt -lt $preflightAt) 'unsupported refusal must precede target resolution and preflight'
        Assert-True ($pg.IndexOf('ExitCodeUnsupportedPlatform', $unsupportedAt, [StringComparison]::Ordinal) -ge 0) 'unsupported platforms need a distinct refusal exit code'
        Assert-True ($pg.IndexOf('unsupported platform', $unsupportedAt, [StringComparison]::Ordinal) -ge 0) 'unsupported platform refusal must provide an actionable message'
        $gcGuardAt = $pg.LastIndexOf('if ($IsLinux -and $Mode -eq ''gc'')', [StringComparison]::Ordinal)
        $gcAt = $pg.LastIndexOf('if ($Mode -eq ''gc'')', [StringComparison]::Ordinal)
        $linuxProofAt = $pg.LastIndexOf('if ($IsLinux -and $Mode -notin @(''gc'', ''new-worktree'', ''remove-worktree''))', [StringComparison]::Ordinal)
        Assert-True ($gcGuardAt -ge 0 -and $gcGuardAt -lt $gcAt) 'Linux gc must have an early refusal guard before the Windows process-snapshot branch'
        Assert-True ($gcGuardAt -lt $repoAt -and $gcGuardAt -lt $resolveAt -and $gcGuardAt -lt $preflightAt) 'Linux gc refusal must precede repo, target, and preflight work'
        Assert-True ($pg.IndexOf('ExitCodeLinuxGcUnsupported', $gcGuardAt, [StringComparison]::Ordinal) -ge 0) 'Linux gc needs a distinct refusal exit code'
        Assert-True ($pg.IndexOf('Linux gc', $gcGuardAt, [StringComparison]::Ordinal) -ge 0) 'Linux gc refusal must provide an actionable message'
        Assert-True ($gcAt -ge 0 -and $linuxProofAt -ge 0 -and $gcAt -gt $linuxProofAt) 'Linux gc must be ordered after its explicit preflight exclusion'
        Assert-True ($gcAt -lt $pg.IndexOf('Get-ProcessSnapshot', [StringComparison]::Ordinal)) 'Linux gc must enter its early branch before process-specific work'
    }

    Test-Case 'Linux direct process branch captures child cwd stdout and exit code' {
        Assert-LinuxAdapterReady
        $workingDirectory = Join-Path $fixtureRoot 'direct-process'
        $capture = Join-Path $fixtureRoot 'direct-process.out'
        New-Item -ItemType Directory -Force -Path $workingDirectory | Out-Null
        $code = Invoke-ProcessInJobObject -Exe 'pwsh' -CmdArgs @('-NoProfile', '-Command', "Write-Output (Get-Location).Path; Write-Output 'linux-fixture'; exit 23") -WorkingDirectory $workingDirectory -CaptureStdoutPath $capture -SelfCgroupText "0::/delegated/supervisor/worker`n" -MountInfoText $mountInfo -ReadFile (New-Reader -Files $validCgroupFiles)
        Assert-Equal 23 $code 'direct Linux process must preserve child exit code'
        $output = Get-Content -LiteralPath $capture -Raw
        Assert-True ($output -match [regex]::Escape($workingDirectory)) 'direct process must run in the requested cwd'
        Assert-True ($output -match 'linux-fixture') 'direct process stdout must be captured'
    }

    Test-Case 'Linux build-slot exclusion is shared with an independent pwsh process' {
        Assert-LinuxAdapterReady
        $lockRoot = Join-Path $fixtureRoot 'build-slots-cross-process'
        $holderScript = Join-Path $fixtureRoot 'linux-slot-holder.ps1'
        $probeScript = Join-Path $fixtureRoot 'linux-slot-probe.ps1'
        $readyPath = Join-Path $fixtureRoot 'linux-slot-holder.ready'
        $holderOut = Join-Path $fixtureRoot 'linux-slot-holder.out'
        $probeOut = Join-Path $fixtureRoot 'linux-slot-probe.out'
        Set-Content -LiteralPath $holderScript -Encoding utf8 -Value @'
param([string]$Common, [string]$ToolRoot, [string]$LockRoot, [string]$ReadyPath)
. $Common
$import = Import-PanGlossPlatformAdapter -Platform Linux -ToolRoot $ToolRoot
$slot = Enter-BuildSlot -MaxConcurrent 1 -TimeoutSeconds 5 -LockRoot $LockRoot
if ($null -eq $slot) { exit 2 }
Set-Content -LiteralPath $ReadyPath -Value 'held' -NoNewline
Start-Sleep -Seconds 30
Exit-BuildSlot -Semaphore $slot
'@
        Set-Content -LiteralPath $probeScript -Encoding utf8 -Value @'
param([string]$Common, [string]$ToolRoot, [string]$LockRoot)
. $Common
$import = Import-PanGlossPlatformAdapter -Platform Linux -ToolRoot $ToolRoot
$slot = Enter-BuildSlot -MaxConcurrent 1 -TimeoutSeconds 1 -LockRoot $LockRoot
if ($null -eq $slot) { 'DENIED'; exit 0 }
Exit-BuildSlot -Semaphore $slot
'ACQUIRED'
exit 1
'@
        $holder = $null
        try {
            $holder = Start-Process -FilePath 'pwsh' -PassThru -NoNewWindow -RedirectStandardOutput $holderOut `
                -ArgumentList @('-NoProfile', '-File', $holderScript, $commonPath, $toolRoot, $lockRoot, $readyPath)
            foreach ($attempt in 1..100) {
                Start-Sleep -Milliseconds 50
                if (Test-Path -LiteralPath $readyPath) { break }
                if ($holder.HasExited) { throw "slot holder exited early: $(Get-Content -LiteralPath $holderOut -Raw)" }
            }
            Assert-True (Test-Path -LiteralPath $readyPath) 'child process never reported its held slot'

            $probe = Start-Process -FilePath 'pwsh' -PassThru -NoNewWindow -RedirectStandardOutput $probeOut `
                -ArgumentList @('-NoProfile', '-File', $probeScript, $commonPath, $toolRoot, $lockRoot)
            $probe.WaitForExit(10000) | Out-Null
            Assert-Equal 0 $probe.ExitCode 'an independent process must be denied while the holder owns the slot'
            Assert-Equal 'DENIED' (Get-Content -LiteralPath $probeOut -Raw).Trim()
        } finally {
            if ($holder -and -not $holder.HasExited) { Stop-Process -Id $holder.Id -Force -ErrorAction SilentlyContinue }
            if ($holder) { $holder.WaitForExit(10000) | Out-Null }
        }

        $probe = Start-Process -FilePath 'pwsh' -PassThru -NoNewWindow -RedirectStandardOutput $probeOut `
            -ArgumentList @('-NoProfile', '-File', $probeScript, $commonPath, $toolRoot, $lockRoot)
        $probe.WaitForExit(10000) | Out-Null
        Assert-Equal 1 $probe.ExitCode 'a released cross-process slot must be acquirable'
        Assert-Equal 'ACQUIRED' (Get-Content -LiteralPath $probeOut -Raw).Trim()
    }

    # --- Process seam: valid preflight gates the injected launch; no real Cargo is reachable. ---

    Test-Case 'Linux Cargo process seam preserves working directory and exit code' {
        Assert-LinuxAdapterReady
        $calls = [System.Collections.Generic.List[object]]::new()
        $workingDirectory = Join-Path $fixtureRoot 'working-directory'
        New-Item -ItemType Directory -Force -Path $workingDirectory | Out-Null
        $runner = {
            param([string]$Executable, [string[]]$Arguments, [string]$WorkingDirectory)
            [void]$calls.Add([PSCustomObject]@{
                Executable = $Executable
                Arguments = @($Arguments)
                WorkingDirectory = $WorkingDirectory
            })
            return 37
        }.GetNewClosure()
        $code = Invoke-CargoWithReaper -Exe 'cargo' -CmdArgs @('test', '--package', 'fixture') `
            -WorkingDirectory $workingDirectory -SelfCgroupText "0::/delegated/supervisor/worker`n" `
            -MountInfoText $mountInfo -ReadFile (New-Reader -Files $validCgroupFiles) -ProcessInvoker $runner
        Assert-Equal 37 $code 'the adapter must return the injected process exit code unchanged'
        Assert-Equal 1 $calls.Count 'exactly one process must be launched'
        Assert-Equal 'cargo' $calls[0].Executable
        Assert-Equal $workingDirectory $calls[0].WorkingDirectory
        Assert-Equal 'test' $calls[0].Arguments[0]
    }

    Test-Case 'Linux Cargo refuses before the process seam when host containment is unproven' {
        Assert-LinuxAdapterReady
        Assert-True ($null -ne (Get-Command Invoke-CargoWithReaper -CommandType Function -ErrorAction SilentlyContinue)) `
            'missing contract function: Invoke-CargoWithReaper'
        $called = @{ Value = $false }
        $runner = {
            param([string]$Executable, [string[]]$Arguments, [string]$WorkingDirectory)
            $called.Value = $true
            return 0
        }.GetNewClosure()
        $invalidFiles = @{} + $validCgroupFiles
        $invalidFiles['/cg d/memory.max'] = "max`n"
        $invalidFiles['/cg d/supervisor/memory.max'] = "max`n"
        $invalidFiles['/cg d/supervisor/worker/memory.max'] = "max`n"
        $threw = $false
        try {
            Invoke-CargoWithReaper -Exe 'cargo' -CmdArgs @('build') -WorkingDirectory $fixtureRoot `
                -SelfCgroupText "0::/delegated/supervisor/worker`n" -MountInfoText $mountInfo `
                -ReadFile (New-Reader -Files $invalidFiles) -ProcessInvoker $runner
        } catch { $threw = $true }
        Assert-True $threw 'an unproven host cgroup must refuse before process launch'
        Assert-False $called.Value 'Cargo must never be called after a failed host proof'
    }
} finally {
    Remove-Item -LiteralPath $fixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-TestSummary
