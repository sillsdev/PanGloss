function Get-LinuxProcLinkTarget {
    param([Parameter(Mandatory)][string]$Path)
    try {
        return [string](Get-Item -LiteralPath $Path -Force -ErrorAction Stop).LinkTarget
    } catch {
        return ''
    }
}

function Get-LinuxProcessRecord {
    param([Parameter(Mandatory)][int]$ProcessId, [string]$ProcRoot = '/proc')
    $path = Join-Path $ProcRoot "$ProcessId"
    $errorText = ''
    try {
        $stat = [System.IO.File]::ReadAllText((Join-Path $path 'stat'))
        $close = $stat.LastIndexOf(')')
        $open = $stat.IndexOf('(')
        if ($open -lt 0 -or $close -le $open) { throw 'malformed stat record' }
        $name = $stat.Substring($open + 1, $close - $open - 1)
        $fields = @($stat.Substring($close + 1).Trim() -split '\s+')
        if ($fields.Count -lt 20) { throw 'incomplete stat record' }
        $parentProcessId = [int]$fields[1]
        $startTicks = [long]$fields[19]
    } catch {
        return [PSCustomObject]@{
            ProcessId = $ProcessId; ParentProcessId = -1; Name = ''; Argv0 = ''; CommandLine = ''
            Arguments = @(); Cwd = ''; Exe = ''; Uid = $null; StartTicks = $null; IdentityError = $_.Exception.Message
        }
    }

    $uid = $null
    $arguments = @()
    try {
        $status = [System.IO.File]::ReadAllText((Join-Path $path 'status'))
        $uidMatch = [regex]::Match($status, '(?m)^Uid:\s+\d+\s+(\d+)')
        if (-not $uidMatch.Success) { throw 'effective UID is absent from status' }
        $uid = [int]$uidMatch.Groups[1].Value
    } catch {
        $errorText = "effective UID unavailable: $($_.Exception.Message)"
    }

    try {
        $bytes = [System.IO.File]::ReadAllBytes((Join-Path $path 'cmdline'))
        if ($bytes.Length -gt 0) {
            $arguments = @(([System.Text.Encoding]::UTF8.GetString($bytes) -split "`0") | Where-Object { $_.Length -gt 0 })
        }
    } catch {
        $detail = "command line unavailable: $($_.Exception.Message)"
        $errorText = if ($errorText) { "$errorText; $detail" } else { $detail }
    }

    $cwd = Get-LinuxProcLinkTarget -Path (Join-Path $path 'cwd')
    $exe = Get-LinuxProcLinkTarget -Path (Join-Path $path 'exe')
    $argv0 = if ($arguments.Count -gt 0) { [string]$arguments[0] } else { '' }
    return [PSCustomObject]@{
        ProcessId = $ProcessId; ParentProcessId = $parentProcessId; Name = $name; Argv0 = $argv0
        CommandLine = $arguments -join ' '; Arguments = $arguments; Cwd = $cwd; Exe = $exe
        Uid = $uid; StartTicks = $startTicks; IdentityError = $errorText
    }
}

function Get-LinuxEffectiveUid {
    param([string]$ProcRoot = '/proc')
    $status = [System.IO.File]::ReadAllText((Join-Path $ProcRoot 'self/status'))
    $match = [regex]::Match($status, '(?m)^Uid:\s+\d+\s+(\d+)')
    if (-not $match.Success) { throw "cannot read effective UID from $ProcRoot/self/status" }
    return [int]$match.Groups[1].Value
}

function Get-LinuxProcessSnapshot {
    param([string]$ProcRoot = '/proc', [int]$EffectiveUid = -1)
    if (-not (Test-Path -LiteralPath $ProcRoot -PathType Container)) {
        return [PSCustomObject]@{ Processes = @(); Failures = @("cannot enumerate proc root: $ProcRoot is unavailable"); EffectiveUid = $EffectiveUid }
    }
    if ($EffectiveUid -lt 0) {
        try { $EffectiveUid = Get-LinuxEffectiveUid -ProcRoot $ProcRoot } catch {
            return [PSCustomObject]@{ Processes = @(); Failures = @($_.Exception.Message); EffectiveUid = $null }
        }
    }
    $processes = @()
    $failures = @()
    try {
        $entries = @(Get-ChildItem -LiteralPath $ProcRoot -Directory -ErrorAction Stop | Where-Object { $_.Name -match '^\d+$' })
    } catch {
        return [PSCustomObject]@{ Processes = @(); Failures = @("cannot enumerate $ProcRoot`: $($_.Exception.Message)"); EffectiveUid = $EffectiveUid }
    }
    foreach ($entry in $entries) {
        $record = Get-LinuxProcessRecord -ProcessId ([int]$entry.Name) -ProcRoot $ProcRoot
        if ($record.IdentityError -and $record.Name -eq '') {
            if (Test-Path -LiteralPath $entry.FullName) { $failures += "PID $($entry.Name) could not be read: $($record.IdentityError)" }
            continue
        }
        $processes += $record
    }
    return [PSCustomObject]@{ Processes = $processes; Failures = $failures; EffectiveUid = $EffectiveUid }
}

function Get-LinuxGcProcessKind {
    param([Parameter(Mandatory)]$Process)
    $names = @([string]$Process.Name)
    if ($Process.Argv0) { $names += [System.IO.Path]::GetFileName([string]$Process.Argv0) }
    foreach ($name in $names) {
        switch -Regex ($name.ToLowerInvariant()) {
            '^(cargo|rustc|pangloss|cc|cc1|cc1plus|ld|ld\.lld|mold|rust-lld|link|lld)(-[0-9]+)?$' { return $name }
        }
    }
    $text = "$($Process.Argv0) $($Process.CommandLine)"
    if ($text -match '(?i)(?:^|/)target/(?:[^/]+/){1,2}deps/[^\s"'']+-[0-9a-f]{8,}(?:\s|$)' -and
        $text -match '(?i)(--test-threads|--nocapture|--ignored|--exact)') {
        return 'rust-test-binary'
    }
    return $null
}

function Get-LinuxBuildSlotRecords {
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Processes,
        [Parameter(Mandatory)][int]$EffectiveUid,
        [ValidateSet('build', 'run')][string]$Pool = 'build',
        [string]$LedgerPath = ''
    )
    if (-not $LedgerPath) {
        try { $LedgerPath = Get-SlotLedgerPath -Pool $Pool } catch {
            return [PSCustomObject]@{ Records = @(); Failures = @("cannot resolve $Pool-slot ledger path: $($_.Exception.Message)") }
        }
    }
    $records = @()
    $failures = @()
    if (-not (Test-Path -LiteralPath $LedgerPath -PathType Container)) {
        return [PSCustomObject]@{ Records = @(); Failures = @() }
    }
    try {
        $files = @(Get-ChildItem -LiteralPath $LedgerPath -Filter 'slot*.json' -File -ErrorAction Stop)
    } catch {
        return [PSCustomObject]@{ Records = @(); Failures = @("cannot enumerate $Pool-slot records at $LedgerPath`: $($_.Exception.Message)") }
    }
    foreach ($file in $files) {
        try {
            $entry = Get-Content -LiteralPath $file.FullName -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
            $pidValue = 0
            if (-not [int]::TryParse([string]$entry.Pid, [ref]$pidValue) -or $pidValue -le 1) { throw 'record has no valid PID' }
            $process = $Processes | Where-Object { $_.ProcessId -eq $pidValue } | Select-Object -First 1
            if (-not $process) { continue }
            if ($null -eq $process.Uid) {
                $failures += "$($file.Name) names PID $pidValue, but its UID is unreadable"
                continue
            }
            if ([int]$process.Uid -ne $EffectiveUid) { continue }
            $recordTicks = if ($null -ne $entry.ProcStartTicks) { [string]$entry.ProcStartTicks } else { '' }
            if ($recordTicks -and $recordTicks -ne [string]$process.StartTicks) { continue }
            $acquiredAtUtc = if ($entry.AcquiredAtUtc -is [datetime]) {
                $entry.AcquiredAtUtc.ToUniversalTime().ToString('o')
            } else {
                [string]$entry.AcquiredAtUtc
            }
            $records += [PSCustomObject]@{
                Pool = $Pool; Slot = [string]($file.BaseName -replace '^slot', '')
                Pid = $pidValue; Mode = [string]$entry.Mode; Worktree = [string]$entry.Worktree
                WorktreePath = [string]$entry.WorktreePath; TargetDir = [string]$entry.TargetDir
                ProcStartTicks = $recordTicks; AcquiredAtUtc = $acquiredAtUtc; Process = $process
            }
        } catch {
            $failures += "$Pool/$($file.Name) could not be trusted: $($_.Exception.Message)"
        }
    }
    return [PSCustomObject]@{ Records = $records; Failures = $failures }
}

function Test-LinuxPathWithin {
    param([string]$Path, [string]$Root)
    if (-not $Path -or -not $Root) { return $false }
    $base = $Root.TrimEnd('/')
    return $Path.Equals($base, [StringComparison]::Ordinal) -or $Path.StartsWith($base + '/', [StringComparison]::Ordinal)
}

function Test-LinuxHasScopedProcessPath {
    param([Parameter(Mandatory)]$Process)
    $cwd = [string]$Process.Cwd
    if ([System.IO.Path]::IsPathRooted($cwd) -and $cwd -ne [System.IO.Path]::GetPathRoot($cwd)) { return $true }
    $arguments = @($Process.Arguments | Select-Object -Skip 1)
    foreach ($argument in $arguments) {
        $pathToken = [string]$argument
        if ($pathToken -match '^[^=]+=(/.*)$') { $pathToken = $Matches[1] }
        if ([System.IO.Path]::IsPathRooted($pathToken) -and $pathToken -ne [System.IO.Path]::GetPathRoot($pathToken)) { return $true }
    }
    return $false
}

function Get-LinuxReaperDescription {
    param([Parameter(Mandatory)]$Process)
    if ([string]$Process.Name -match '^Relay\(\d+\)$') { return 'WSL init relay process name' }
    if ([string]$Process.Exe -eq '/init') { return 'WSL init relay executable /init' }
    if ([string]$Process.Argv0 -eq '/init') { return 'WSL init relay argv0 /init' }
    if ([string]$Process.Name -ieq 'systemd' -and @($Process.Arguments) -contains '--user') { return 'systemd --user' }
    return ''
}

function Get-LinuxProcessAncestry {
    param([Parameter(Mandatory)]$Process, [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Snapshot)
    $byPid = @{}
    foreach ($item in $Snapshot) { $byPid[[string]$item.ProcessId] = $item }
    $chain = @()
    $seen = @{}
    $parentPid = [int]$Process.ParentProcessId
    $directParent = $true
    while ($parentPid -gt 0) {
        if ($seen.ContainsKey([string]$parentPid)) {
            return [PSCustomObject]@{ Chain = $chain; Complete = $false; IsOrphan = $false; OrphanReason = ''; Error = 'parent chain contains a cycle' }
        }
        $seen[[string]$parentPid] = $true
        if ($parentPid -eq 1) {
            return [PSCustomObject]@{
                Chain = $chain; Complete = $true; IsOrphan = $directParent
                OrphanReason = $(if ($directParent) { 'parent PID 1 is the process reaper' } else { '' }); Error = ''
            }
        }
        if (-not $byPid.ContainsKey([string]$parentPid)) {
            return [PSCustomObject]@{ Chain = $chain; Complete = $false; IsOrphan = $false; OrphanReason = ''; Error = "parent PID $parentPid is absent from the process snapshot" }
        }
        $parent = $byPid[[string]$parentPid]
        if ($null -eq $parent.Uid) {
            return [PSCustomObject]@{ Chain = $chain + @($parent); Complete = $false; IsOrphan = $false; OrphanReason = ''; Error = "parent PID $parentPid has unreadable ownership" }
        }
        $chain += $parent
        if ($directParent) {
            $reaper = Get-LinuxReaperDescription -Process $parent
            if ($reaper) {
                return [PSCustomObject]@{
                    Chain = $chain; Complete = $true; IsOrphan = $true
                    OrphanReason = "parent PID $parentPid is $reaper"; Error = ''
                }
            }
            $directParent = $false
        }
        $parentPid = [int]$parent.ParentProcessId
    }
    return [PSCustomObject]@{ Chain = $chain; Complete = $false; IsOrphan = $false; OrphanReason = ''; Error = 'parent identity is unavailable' }
}

function Get-LinuxGcProcessDecision {
    param(
        [Parameter(Mandatory)]$Process,
        [Parameter(Mandatory)][object[]]$Snapshot,
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$SlotOwners,
        [string[]]$SlotFailures = @(),
        [int[]]$CallerPids = @(),
        [Parameter(Mandatory)][int]$EffectiveUid
    )
    $kind = Get-LinuxGcProcessKind -Process $Process
    if (-not $kind) { return [PSCustomObject]@{ Action = 'Ignore'; Kind = ''; Reason = '' } }
    if ($null -eq $Process.Uid) { return [PSCustomObject]@{ Action = 'Refuse'; Kind = $kind; Reason = 'effective UID is unreadable' } }
    if ([int]$Process.Uid -ne $EffectiveUid) { return [PSCustomObject]@{ Action = 'Ignore'; Kind = $kind; Reason = 'process belongs to another user' } }
    if ($null -eq $Process.StartTicks -or $Process.IdentityError) {
        return [PSCustomObject]@{ Action = 'Refuse'; Kind = $kind; Reason = "process identity is incomplete: $($Process.IdentityError)" }
    }
    if ($SlotFailures.Count -gt 0) {
        return [PSCustomObject]@{ Action = 'Refuse'; Kind = $kind; Reason = "managed-slot ownership is incomplete: $($SlotFailures -join '; ')" }
    }

    $ancestry = Get-LinuxProcessAncestry -Process $Process -Snapshot $Snapshot
    if (-not $ancestry.Complete) {
        return [PSCustomObject]@{ Action = 'Refuse'; Kind = $kind; Reason = "ownership cannot be determined: $($ancestry.Error)" }
    }
    foreach ($ancestor in $ancestry.Chain) {
        if ($CallerPids -contains [int]$ancestor.ProcessId) {
            return [PSCustomObject]@{ Action = 'Refuse'; Kind = $kind; Reason = "process is in the caller's process tree (PID $($ancestor.ProcessId))" }
        }
        $owner = $SlotOwners | Where-Object { $_.Pid -eq $ancestor.ProcessId } | Select-Object -First 1
        if ($owner) {
            return [PSCustomObject]@{ Action = 'Refuse'; Kind = $kind; Reason = "owned by live $($owner.Pool) slot $($owner.Slot) (PID $($owner.Pid), $($owner.Worktree))" }
        }
    }
    if ($CallerPids -contains [int]$Process.ProcessId) {
        return [PSCustomObject]@{ Action = 'Refuse'; Kind = $kind; Reason = "process is in the caller's process tree" }
    }

    $matchingOwner = $null
    $possibleOwner = $null
    foreach ($owner in $SlotOwners) {
        $ownerPaths = @($owner.WorktreePath, $owner.TargetDir | Where-Object { $_ })
        foreach ($ownerPath in $ownerPaths) {
            foreach ($path in @([string]$Process.Argv0, [string]$Process.Cwd)) {
                if (Test-LinuxPathWithin -Path $path -Root $ownerPath) {
                    $matchingOwner = $owner
                    break
                }
                if (-not $possibleOwner -and (Test-LinuxPathWithin -Path $ownerPath -Root $path)) {
                    $possibleOwner = [PSCustomObject]@{ Owner = $owner; Path = $path; Root = $ownerPath }
                }
            }
            if (-not $matchingOwner -and $ownerPath -and $Process.CommandLine.Contains([string]$ownerPath)) {
                $matchingOwner = $owner
            }
            if ($matchingOwner) { break }
        }
        if ($matchingOwner) { break }
    }
    if ($matchingOwner) {
        return [PSCustomObject]@{ Action = 'Refuse'; Kind = $kind; Reason = "owned by live $($matchingOwner.Pool) slot $($matchingOwner.Slot) (PID $($matchingOwner.Pid), $($matchingOwner.Worktree))" }
    }
    if ($possibleOwner) {
        return [PSCustomObject]@{
            Action = 'Refuse'; Kind = $kind
            Reason = "process path '$($possibleOwner.Path)' is too broad to separate it from live $($possibleOwner.Owner.Pool) slot $($possibleOwner.Owner.Slot) at '$($possibleOwner.Root)'"
        }
    }
    if ($SlotOwners.Count -gt 0) {
        $ownersWithoutAbsolutePaths = @($SlotOwners | Where-Object {
            (-not [System.IO.Path]::IsPathRooted([string]$_.WorktreePath)) -and
            (-not [System.IO.Path]::IsPathRooted([string]$_.TargetDir))
        }).Count
        $ownersHavePaths = $ownersWithoutAbsolutePaths -eq 0
        $processHasPath = Test-LinuxHasScopedProcessPath -Process $Process
        if (-not $ownersHavePaths -or -not $processHasPath) {
            return [PSCustomObject]@{ Action = 'Refuse'; Kind = $kind; Reason = 'a live managed slot exists, but this process cannot be matched to or separated from its recorded worktree and target directory' }
        }
    }
    if (-not $ancestry.IsOrphan) {
        $parentPid = if ($ancestry.Chain.Count -gt 0) { $ancestry.Chain[0].ProcessId } else { $Process.ParentProcessId }
        return [PSCustomObject]@{ Action = 'Refuse'; Kind = $kind; Reason = "parent PID $parentPid is still alive and this is not a verified orphan" }
    }
    return [PSCustomObject]@{ Action = 'Kill'; Kind = $kind; Reason = "$($ancestry.OrphanReason) with no live managed build owner" }
}

function Get-LinuxCallerProcessIds {
    param([int]$CurrentPid = $PID)
    return @($CurrentPid)
}

function Stop-LinuxProcessRecord {
    param([Parameter(Mandatory)]$Process, [Parameter(Mandatory)][int]$EffectiveUid, [string]$ProcRoot = '/proc')
    $current = Get-LinuxProcessRecord -ProcessId ([int]$Process.ProcessId) -ProcRoot $ProcRoot
    if ($null -eq $current.Uid -or [int]$current.Uid -ne $EffectiveUid) { return [PSCustomObject]@{ Ok = $false; Detail = 'effective UID changed or is unreadable before termination' } }
    if ([string]$current.StartTicks -ne [string]$Process.StartTicks) { return [PSCustomObject]@{ Ok = $false; Detail = 'PID was reused or process identity changed before termination' } }
    try {
        $handle = [System.Diagnostics.Process]::GetProcessById([int]$Process.ProcessId)
        $handle.Kill()
        [void]$handle.WaitForExit(5000)
        if (-not $handle.HasExited) { return [PSCustomObject]@{ Ok = $false; Detail = 'process did not exit after SIGKILL' } }
        return [PSCustomObject]@{ Ok = $true; Detail = 'process exited' }
    } catch {
        return [PSCustomObject]@{ Ok = $false; Detail = $_.Exception.Message }
    }
}

function Remove-LinuxOrphanedBuildProcesses {
    param(
        [Parameter(Mandatory)]$ProcessSnapshot,
        [switch]$Apply,
        [int[]]$CallerPids = @(),
        [string]$ProcRoot = '/proc',
        [string]$LedgerPath = '',
        [string]$RunLedgerPath = '',
        [scriptblock]$Stopper = $null
    )
    $result = [ordered]@{
        Killed = @(); WouldKill = @(); Refused = @(); OtherUsers = @()
        CensusFailures = @(); SlotFailures = @()
    }
    if ($ProcessSnapshot.Failures.Count -gt 0 -or $null -eq $ProcessSnapshot.EffectiveUid) {
        $result.CensusFailures = @($ProcessSnapshot.Failures)
        if ($null -eq $ProcessSnapshot.EffectiveUid -and $result.CensusFailures.Count -eq 0) { $result.CensusFailures = @('effective UID could not be established') }
        return [PSCustomObject]$result
    }
    $snapshot = @($ProcessSnapshot.Processes)
    if ($CallerPids.Count -eq 0) { $CallerPids = @(Get-LinuxCallerProcessIds) }
    $slots = @()
    foreach ($pool in @('build', 'run')) {
        $slotResult = Get-LinuxBuildSlotRecords -Processes $snapshot -EffectiveUid $ProcessSnapshot.EffectiveUid `
            -Pool $pool -LedgerPath $(if ($pool -eq 'build') { $LedgerPath } else { $RunLedgerPath })
        $slots += $slotResult
    }
    $slotOwners = @($slots | ForEach-Object { $_.Records })
    $slotFailures = @($slots | ForEach-Object { $_.Failures })
    $result.SlotFailures = $slotFailures
    foreach ($process in $snapshot) {
        $decision = Get-LinuxGcProcessDecision -Process $process -Snapshot $snapshot -SlotOwners $slotOwners `
            -SlotFailures $slotFailures -CallerPids $CallerPids -EffectiveUid $ProcessSnapshot.EffectiveUid
        if ($decision.Action -eq 'Ignore') {
            if ($decision.Kind -and $decision.Reason -eq 'process belongs to another user') {
                $result.OtherUsers += [PSCustomObject]@{
                    ProcessId = $process.ProcessId; Name = $decision.Kind; Reason = 'process belongs to another user'
                }
            }
            continue
        }
        if ($decision.Action -eq 'Refuse') {
            $result.Refused += [PSCustomObject]@{ ProcessId = $process.ProcessId; Name = $decision.Kind; Reason = $decision.Reason }
            continue
        }
        if (-not $Apply) {
            $result.WouldKill += [PSCustomObject]@{ ProcessId = $process.ProcessId; Name = $decision.Kind }
            continue
        }
        $stopped = if ($Stopper) {
            & $Stopper -Process $process -EffectiveUid $ProcessSnapshot.EffectiveUid -ProcRoot $ProcRoot
        } else {
            Stop-LinuxProcessRecord -Process $process -EffectiveUid $ProcessSnapshot.EffectiveUid -ProcRoot $ProcRoot
        }
        if ($null -eq $stopped -or $null -eq $stopped.Ok) {
            $result.Refused += [PSCustomObject]@{
                ProcessId = $process.ProcessId; Name = $decision.Kind
                Reason = 'termination did not return a confirmed process-exit result'
            }
            continue
        }
        if ($stopped.Ok) {
            $result.Killed += [PSCustomObject]@{ ProcessId = $process.ProcessId; Name = $decision.Kind }
        } else {
            $result.Refused += [PSCustomObject]@{ ProcessId = $process.ProcessId; Name = $decision.Kind; Reason = $stopped.Detail }
        }
    }
    return [PSCustomObject]$result
}

function Get-LinuxGcTargetRoots {
    param()
    $roots = @()
    foreach ($candidate in @($env:PANGLOSS_SSD_CACHE_ROOT, $env:PANGLOSS_CARGO_CACHE_ROOT, $env:PANGLOSS_TARGET_ROOT)) {
        if (-not $candidate) { continue }
        if (-not [System.IO.Path]::IsPathRooted($candidate)) { throw "Linux managed-target root must be absolute: $candidate" }
        $roots += $candidate
    }
    $seen = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($root in $roots) {
        try {
            $full = [System.IO.Path]::GetFullPath($root)
            if ($full -eq [System.IO.Path]::GetPathRoot($full)) { continue }
            if ($seen.Add($full)) { $full }
        } catch {}
    }
}

function Get-LinuxLiveBuildProcesses {
    param([object[]]$Snapshot = $null)
    if ($null -eq $Snapshot) {
        $census = Get-LinuxProcessSnapshot
        if ($census.Failures.Count -gt 0) { throw "cannot safely inspect Linux build processes: $($census.Failures -join '; ')" }
        $Snapshot = $census.Processes
    }
    return @($Snapshot | Where-Object {
        (Get-LinuxGcProcessKind -Process $_) -or $_.Name -in @('sccache', 'cargo-nextest')
    })
}

function Get-LinuxAgedBuildSlotRefusals {
    param(
        [Parameter(Mandatory)]$ProcessSnapshot,
        [int]$MinAgeMinutes = 20,
        [int[]]$CallerPids = @(),
        [string]$LedgerPath = '',
        [datetime]$Now = (Get-Date)
    )
    $result = [ordered]@{ Refused = @() }
    if ($ProcessSnapshot.Failures.Count -gt 0 -or $null -eq $ProcessSnapshot.EffectiveUid) {
        foreach ($failure in $ProcessSnapshot.Failures) {
            $result.Refused += [PSCustomObject]@{ ProcessId = 0; Reason = "process census incomplete: $failure" }
        }
        if ($null -eq $ProcessSnapshot.EffectiveUid -and $ProcessSnapshot.Failures.Count -eq 0) {
            $result.Refused += [PSCustomObject]@{ ProcessId = 0; Reason = 'effective UID could not be established' }
        }
        return [PSCustomObject]$result
    }
    $snapshot = @($ProcessSnapshot.Processes)
    if ($CallerPids.Count -eq 0) { $CallerPids = @(Get-LinuxCallerProcessIds) }
    $slots = Get-LinuxBuildSlotRecords -Processes $snapshot -EffectiveUid $ProcessSnapshot.EffectiveUid -LedgerPath $LedgerPath
    foreach ($failure in $slots.Failures) {
        $result.Refused += [PSCustomObject]@{ ProcessId = 0; Reason = "build-slot ownership incomplete: $failure" }
    }
    foreach ($slot in $slots.Records) {
        $root = $slot.Process
        $rootAncestry = Get-LinuxProcessAncestry -Process $root -Snapshot $snapshot
        if ($CallerPids -contains $slot.Pid -or
            @($rootAncestry.Chain | Where-Object { $CallerPids -contains [int]$_.ProcessId }).Count -gt 0) {
            $result.Refused += [PSCustomObject]@{ ProcessId = $slot.Pid; Reason = "slot holder is in the caller's process tree" }
            continue
        }
        if (-not $rootAncestry.Complete) {
            $result.Refused += [PSCustomObject]@{ ProcessId = $slot.Pid; Reason = "slot ownership cannot be determined: $($rootAncestry.Error)" }
            continue
        }
        if (-not $slot.ProcStartTicks -or $slot.ProcStartTicks -ne [string]$root.StartTicks) {
            $result.Refused += [PSCustomObject]@{ ProcessId = $slot.Pid; Reason = 'slot record lacks a matching process identity' }
            continue
        }
        $commandLine = [string]$root.CommandLine
        if ($commandLine -notmatch '(?i)(?:^|[/\\])pg\.ps1(?:\s|$)') {
            $result.Refused += [PSCustomObject]@{ ProcessId = $slot.Pid; Reason = 'slot PID does not identify a pg.ps1 process' }
            continue
        }
        $acquired = [datetimeoffset]::MinValue
        if (-not [datetimeoffset]::TryParse($slot.AcquiredAtUtc, [ref]$acquired)) {
            $result.Refused += [PSCustomObject]@{ ProcessId = $slot.Pid; Reason = 'slot acquisition time is missing or invalid' }
            continue
        }
        $ageMinutes = ($Now.ToUniversalTime() - $acquired.UtcDateTime).TotalMinutes
        if ($ageMinutes -lt $MinAgeMinutes) { continue }

        $result.Refused += [PSCustomObject]@{
            ProcessId = $slot.Pid
            Reason = 'slot record points to a live process; Linux cannot prove its complete work tree is idle, so it was left running'
        }
    }
    return [PSCustomObject]$result
}
