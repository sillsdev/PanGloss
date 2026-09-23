$script:BuildSlotMutexPrefix = 'Global\PanGlossBuildSlot'
$script:RunSlotMutexPrefix = 'Global\PanGlossRunSlot'
$script:MaxResourceSlotWidth = 64
$script:DefaultRunSlots = if ($env:PANGLOSS_RUN_SLOTS) { [int]$env:PANGLOSS_RUN_SLOTS } else { 4 }
$script:RunThreadsPerSlot = if ($env:PANGLOSS_RUN_THREADS_PER_SLOT) { [int]$env:PANGLOSS_RUN_THREADS_PER_SLOT } else { 1 }

function Get-SlotMutexPrefix {
    param([ValidateSet('build', 'run')][string]$Pool = 'build')
    if ($Pool -eq 'run') { return $script:RunSlotMutexPrefix }
    return $script:BuildSlotMutexPrefix
}

function Get-ResourceSlotContract {
    param([ValidateSet('build', 'run')][string]$Pool = 'build', [int]$RequestedWidth = 1)
    if ($RequestedWidth -lt 1 -or $RequestedWidth -gt $script:MaxResourceSlotWidth) {
        throw "resource slot width $RequestedWidth is outside the supported range 1..$($script:MaxResourceSlotWidth) for the $Pool pool"
    }
    return [PSCustomObject]@{
        Pool = $Pool
        Prefix = Get-SlotMutexPrefix -Pool $Pool
        RequestedWidth = $RequestedWidth
    }
}

function New-ResourceSlotMutex {
    param([Parameter(Mandatory)][string]$Name)
    try {
        return New-Object System.Threading.Mutex($false, $Name)
    } catch [System.UnauthorizedAccessException] {
        return New-Object System.Threading.Mutex($false, ($Name -replace '^Global\\', 'Local\'))
    }
}

function Enter-ResourceSlot {
    param(
        [ValidateSet('build', 'run')][string]$Pool = 'build',
        [int]$MaxConcurrent = 2,
        [int]$TimeoutSeconds = 0
    )
    $contract = Get-ResourceSlotContract -Pool $Pool -RequestedWidth $MaxConcurrent
    $mutexes = @()
    for ($i = 0; $i -lt $MaxConcurrent; $i++) {
        $mutexes += New-ResourceSlotMutex -Name "$($contract.Prefix)$i"
    }

    Write-Host "[build-env] waiting for a $Pool slot ($MaxConcurrent concurrent across all worktrees)..." -ForegroundColor DarkGray
    try {
        foreach ($h in @(Get-SlotHolders -Pool $Pool)) {
            $state = if ($h.Alive) { "alive since $($h.AcquiredAt)" } else { 'NOT ALIVE (kernel will hand this slot over)' }
            Write-Host "[build-env]   $Pool slot $($h.Slot): pid $($h.Pid) ($($h.Mode) in $($h.Worktree)) -- $state" -ForegroundColor DarkGray
        }
    } catch {}

    $timeoutMs = if ($TimeoutSeconds -le 0) { [System.Threading.Timeout]::Infinite } else { $TimeoutSeconds * 1000 }
    $index = -1
    try {
        $index = [System.Threading.WaitHandle]::WaitAny($mutexes, $timeoutMs)
    } catch [System.Threading.AbandonedMutexException] {
        $index = $_.Exception.MutexIndex
        Write-Host "[build-env] recovered an abandoned $Pool slot ($index) -- its previous holder exited without releasing it." -ForegroundColor Yellow
    }

    if ($index -eq [System.Threading.WaitHandle]::WaitTimeout -or $index -lt 0) {
        foreach ($m in $mutexes) { $m.Dispose() }
        return $null
    }

    $slot = [PSCustomObject]@{
        Mutexes = $mutexes
        Index = $index
        Pool = $Pool
        Prefix = $contract.Prefix
    }
    try { Write-SlotHolder -Pool $Pool -Slot $index } catch {}
    return $slot
}

function Exit-ResourceSlot {
    param($Slot)
    if (-not $Slot) { return }
    try {
        if ($null -ne $Slot.Index -and $Slot.Mutexes -and $Slot.Pool) {
            try { $Slot.Mutexes[$Slot.Index].ReleaseMutex() } catch {}
            try { Clear-SlotHolder -Pool $Slot.Pool -Slot $Slot.Index } catch {}
            foreach ($m in $Slot.Mutexes) { $m.Dispose() }
        }
    } catch {}
}

function Get-SlotLedgerPath {
    param([ValidateSet('build', 'run')][string]$Pool = 'build')
    $root = if ($env:PANGLOSS_STATE_ROOT) { $env:PANGLOSS_STATE_ROOT } elseif ($env:ProgramData) { Join-Path $env:ProgramData 'PanGloss' } else { Join-Path ([System.IO.Path]::GetTempPath()) 'PanGloss' }
    if (-not (Test-Path $root)) { New-Item -ItemType Directory -Force -Path $root | Out-Null }
    return (Join-Path $root "$Pool-slots")
}

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

function Clear-SlotHolder {
    param([ValidateSet('build', 'run')][string]$Pool = 'build', [Parameter(Mandatory)][int]$Slot)
    Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path (Get-SlotLedgerPath -Pool $Pool) "slot$Slot.json")
}

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

$script:RustBuildCoreProcessNames = @('rustc.exe', 'cargo.exe')
$script:RustBuildLinkerProcessNames = @('link.exe', 'lld-link.exe', 'rust-lld.exe')
$script:RustBuildProcessNames = @($script:RustBuildCoreProcessNames) + @($script:RustBuildLinkerProcessNames)
$script:LiveBuildActivityNames = @($script:RustBuildProcessNames) + @('cc1.exe', 'cc1plus.exe', 'sccache.exe', 'cargo-nextest.exe', 'pangloss.exe')
$script:OrphanedCargoProcessNames = @($script:RustBuildProcessNames) + @('cc1.exe')
$script:GcBusyBuildProcessNames = @($script:RustBuildProcessNames)

function Get-ProcessDescendants {
    param([Parameter(Mandatory)][int]$RootPid, [Parameter(Mandatory)]$Snapshot)
    $byParent = @{}
    foreach ($p in $Snapshot) {
        $key = [string]$p.ParentProcessId
        if (-not $byParent.ContainsKey($key)) { $byParent[$key] = @() }
        $byParent[$key] += $p
    }
    $root = $Snapshot | Where-Object { $_.ProcessId -eq $RootPid } | Select-Object -First 1
    $out = @()
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
    param([Parameter(Mandatory)][int]$RootPid, [Parameter(Mandatory)]$Snapshot, [string[]]$ExtraLiveNames = @())
    $root = $Snapshot | Where-Object { $_.ProcessId -eq $RootPid } | Select-Object -First 1
    if (-not $root) { return $false }
    $tree = @($root) + @(Get-ProcessDescendants -RootPid $RootPid -Snapshot $Snapshot)
    $active = @($tree | Where-Object { $_.Name -in @($script:LiveBuildActivityNames) + @($ExtraLiveNames) })
    return $active.Count -eq 0
}

function Test-BuildSlotHolderStale {
    param($Holder, $Snapshot, [int]$MinAgeMinutes = 20, [datetime]$Now = (Get-Date))
    if (-not $Holder.Alive) { return $false }
    $proc = $Snapshot | Where-Object { $_.ProcessId -eq $Holder.Pid } | Select-Object -First 1
    if (-not $proc -or -not $proc.CreationDate) { return $false }
    if (($Now - $proc.CreationDate).TotalMinutes -lt $MinAgeMinutes) { return $false }
    return Test-ManagedProcessTreeIdle -RootPid $Holder.Pid -Snapshot $Snapshot
}

function Remove-StaleBuildSlotHolders {
    param([switch]$WhatIfOnly = $true, [int]$MinAgeMinutes = 20)
    $snapshot = Get-ProcessSnapshot
    $now = Get-Date
    foreach ($h in @(Get-SlotHolders -Pool 'build')) {
        if (-not (Test-BuildSlotHolderStale -Holder $h -Snapshot $snapshot -MinAgeMinutes $MinAgeMinutes -Now $now)) { continue }
        $ageMin = [int](($now - ($snapshot | Where-Object { $_.ProcessId -eq $h.Pid } | Select-Object -First 1).CreationDate).TotalMinutes)
        if ($WhatIfOnly) {
            Write-Host "[gc] would kill stale build-slot holder PID $($h.Pid) (slot $($h.Slot), $($h.Mode) in $($h.Worktree), alive ${ageMin}min with no compiler activity)" -ForegroundColor Yellow
        } else {
            Write-Host "[gc] killing stale build-slot holder PID $($h.Pid) (slot $($h.Slot), $($h.Mode) in $($h.Worktree), ${ageMin}min idle)" -ForegroundColor Yellow
            & taskkill /T /F /PID $h.Pid 2>$null | Out-Null
            try { Clear-SlotHolder -Pool 'build' -Slot $h.Slot } catch {}
        }
    }
}
