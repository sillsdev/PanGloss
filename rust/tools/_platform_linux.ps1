# Linux implementation of the managed-build platform seam, loaded by Import-PanGlossPlatformAdapter; functions use global scope because a dot-source inside that importer function would discard the overrides on return.

function global:ConvertFrom-LinuxMountField {
    param([Parameter(Mandatory)][string]$Value)
    $builder = [System.Text.StringBuilder]::new()
    for ($i = 0; $i -lt $Value.Length; $i++) {
        $char = $Value[$i]
        if ($char -ne '\') {
            [void]$builder.Append($char)
            continue
        }
        if ($i + 3 -ge $Value.Length) { throw "malformed mountinfo escape in '$Value'" }
        $octal = $Value.Substring($i + 1, 3)
        if ($octal -notmatch '^[0-7]{3}$') { throw "malformed mountinfo escape in '$Value'" }
        $decoded = [Convert]::ToInt32($octal, 8)
        if ($decoded -eq 0) { throw 'NUL in mountinfo path' }
        [void]$builder.Append([char]$decoded)
        $i += 3
    }
    return $builder.ToString()
}

function global:ConvertTo-LinuxCanonicalPath {
    param([Parameter(Mandatory)][string]$Path)
    if ([string]::IsNullOrEmpty($Path) -or -not $Path.StartsWith('/') -or $Path.Contains([char]0)) {
        throw "invalid absolute cgroup path '$Path'"
    }
    if ($Path -eq '/') { return '/' }
    if ($Path -ne '/' -and ($Path.EndsWith('/') -or $Path.Contains('//'))) {
        throw "non-canonical cgroup path '$Path'"
    }
    foreach ($component in $Path.Trim('/').Split('/')) {
        if ($component -in @('.', '..') -or [string]::IsNullOrEmpty($component)) {
            throw "non-canonical cgroup path '$Path'"
        }
    }
    return $Path
}

function global:Read-LinuxPlatformText {
    param([Parameter(Mandatory)][string]$Path)
    return [System.IO.File]::ReadAllText($Path)
}

function global:Convert-LinuxKiBToBytes {
    param([Parameter(Mandatory)][string]$Value, [Parameter(Mandatory)][string]$Field)
    if ($Value -notmatch '^[0-9]+$') { throw "$Field is not an unsigned KiB value" }
    try { $kib = [System.Numerics.BigInteger]::Parse($Value, [Globalization.CultureInfo]::InvariantCulture) } catch { throw "$Field is not numeric" }
    $bytes = $kib * 1024
    if ($bytes -gt [long]::MaxValue) { throw "$Field overflows Int64 after KiB conversion" }
    return [long]$bytes
}

function global:Get-LinuxMemorySnapshot {
    param([string]$MeminfoPath = '/proc/meminfo')
    $text = Read-LinuxPlatformText -Path $MeminfoPath
    $values = @{}
    foreach ($line in ($text -split "`r?`n")) {
        if ($line -match '^([A-Za-z_]+):\s+([0-9]+)\s+kB\s*$') {
            if ($values.ContainsKey($Matches[1])) { throw "duplicate $($Matches[1]) in $MeminfoPath" }
            $values[$Matches[1]] = $Matches[2]
        }
    }
    $fields = @('MemTotal', 'MemAvailable', 'CommitLimit', 'Committed_AS')
    $bytes = @{}
    foreach ($field in $fields) {
        if (-not $values.ContainsKey($field)) { throw "missing $field in $MeminfoPath" }
        $bytes[$field] = Convert-LinuxKiBToBytes -Value ([string]$values[$field]) -Field $field
    }
    return [PSCustomObject]@{
        TotalBytes        = [long]$bytes.MemTotal
        AvailableBytes    = [long]$bytes.MemAvailable
        CommitLimitBytes  = [long]$bytes.CommitLimit
        CommittedBytes    = [long]$bytes.Committed_AS
    }
}

function global:Get-AvailableMemoryGB {
    $snapshot = Get-LinuxMemorySnapshot
    return [math]::Round(([double]$snapshot.AvailableBytes) / 1GB, 1)
}

function global:Get-TotalMemoryGB {
    $snapshot = Get-LinuxMemorySnapshot
    return [math]::Round(([double]$snapshot.TotalBytes) / 1GB, 1)
}

function global:Get-CommitChargeGB {
    $snapshot = Get-LinuxMemorySnapshot
    $limit = [math]::Round(([double]$snapshot.CommitLimitBytes) / 1GB, 1)
    $committed = [math]::Round(([double]$snapshot.CommittedBytes) / 1GB, 1)
    $free = [math]::Round(($limit - $committed), 1)
    return [PSCustomObject]@{
        LimitGB     = $limit
        FreeGB      = $free
        CommittedGB = $committed
        PercentUsed = if ($limit -gt 0) { [int][math]::Round(($committed / $limit) * 100) } else { $null }
    }
}

function global:Read-LinuxUnifiedMembership {
    param([Parameter(Mandatory)][string]$Text)
    $found = $null
    foreach ($line in ($Text -split "`r?`n" | Where-Object { $_.Trim() })) {
        $firstColon = $line.IndexOf(':')
        $secondColon = if ($firstColon -ge 0) { $line.IndexOf(':', $firstColon + 1) } else { -1 }
        if ($firstColon -le 0 -or $secondColon -lt 0) { throw 'malformed /proc/self/cgroup record' }
        $hierarchyId = $line.Substring(0, $firstColon)
        $cgroupPath = $line.Substring($secondColon + 1)
        if ($hierarchyId -eq '0') {
            if ($null -ne $found) { throw 'duplicate unified /proc/self/cgroup record' }
            $found = ConvertTo-LinuxCanonicalPath -Path $cgroupPath
        }
    }
    if ($null -eq $found) { throw 'no unified /proc/self/cgroup membership' }
    return $found
}

function global:Get-LinuxCgroupMounts {
    param([Parameter(Mandatory)][string]$Text)
    $mounts = @()
    foreach ($line in ($Text -split "`r?`n" | Where-Object { $_.Trim() })) {
        $separator = $line.IndexOf(' - ')
        if ($separator -le 0) { throw 'malformed /proc/self/mountinfo record' }
        $left = @($line.Substring(0, $separator) -split '\s+')
        $right = @($line.Substring($separator + 3) -split '\s+')
        if ($left.Count -lt 6 -or $right.Count -lt 3) { throw 'malformed /proc/self/mountinfo fields' }
        if ($right[0] -ne 'cgroup2') { continue }
        $root = ConvertTo-LinuxCanonicalPath -Path (ConvertFrom-LinuxMountField -Value $left[3])
        $mountPoint = ConvertTo-LinuxCanonicalPath -Path (ConvertFrom-LinuxMountField -Value $left[4])
        $mounts += [PSCustomObject]@{ Root = $root; MountPoint = $mountPoint }
    }
    return @($mounts)
}

function global:Test-LinuxPathContains {
    param([Parameter(Mandatory)][string]$Ancestor, [Parameter(Mandatory)][string]$Path)
    return $Path -eq $Ancestor -or ($Ancestor -eq '/' -and $Path.StartsWith('/')) -or
        ($Ancestor -ne '/' -and $Path.StartsWith($Ancestor + '/'))
}

function global:Join-LinuxMappedCgroupPath {
    param([Parameter(Mandatory)][string]$MountPoint, [Parameter(Mandatory)][string]$MountRoot, [Parameter(Mandatory)][string]$CgroupPath)
    if (-not (Test-LinuxPathContains -Ancestor $MountRoot -Path $CgroupPath)) { throw 'cgroup path is outside selected mount root' }
    $suffix = if ($CgroupPath -eq $MountRoot) { '' } elseif ($MountRoot -eq '/') { $CgroupPath } else { $CgroupPath.Substring($MountRoot.Length) }
    if ($suffix -eq '') { return $MountPoint }
    return ($MountPoint.TrimEnd('/') + '/' + $suffix.TrimStart('/'))
}

function global:Read-LinuxCgroupMemoryCap {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][scriptblock]$ReadFile)
    try { $value = (& $ReadFile $Path).Trim() } catch { throw "could not read $Path : $($_.Exception.Message)" }
    if ($value -eq 'max') { return $null }
    if ($value -notmatch '^[0-9]+$') { throw "malformed memory.max at $Path" }
    try { $number = [System.Numerics.BigInteger]::Parse($value, [Globalization.CultureInfo]::InvariantCulture) } catch { throw "malformed memory.max at $Path" }
    if ($number -le 0 -or $number -gt [long]::MaxValue) { throw "invalid finite memory.max at $Path" }
    return [long]$number
}

function global:Get-LinuxHostCgroupPreflight {
    param(
        [string]$SelfCgroupText = $null,
        [string]$MountInfoText = $null,
        [scriptblock]$ReadFile = $null
    )
    try {
        if ($null -eq $ReadFile) { $ReadFile = { param([string]$Path) Read-LinuxPlatformText -Path $Path } }
        if ($null -eq $SelfCgroupText) { $SelfCgroupText = Read-LinuxPlatformText -Path '/proc/self/cgroup' }
        if ($null -eq $MountInfoText) { $MountInfoText = Read-LinuxPlatformText -Path '/proc/self/mountinfo' }
        $membership = Read-LinuxUnifiedMembership -Text $SelfCgroupText
        $mounts = @(Get-LinuxCgroupMounts -Text $MountInfoText)
        $covering = @($mounts | Where-Object { Test-LinuxPathContains -Ancestor $_.Root -Path $membership })
        if ($covering.Count -eq 0) { throw 'no visible cgroup2 mount covers current membership' }
        $maxDepth = ($covering | ForEach-Object { if ($_.Root -eq '/') { 0 } else { @($_.Root.Trim('/').Split('/')).Count } } | Measure-Object -Maximum).Maximum
        $selected = @($covering | Where-Object { $depth = if ($_.Root -eq '/') { 0 } else { @($_.Root.Trim('/').Split('/')).Count }; $depth -eq $maxDepth })
        if ($selected.Count -ne 1) { throw 'ambiguous most-specific cgroup2 mount mapping' }
        $mapped = Join-LinuxMappedCgroupPath -MountPoint $selected[0].MountPoint -MountRoot $selected[0].Root -CgroupPath $membership
        $rootMapped = $selected[0].MountPoint
        $caps = @()
        $current = $membership
        while ($true) {
            $mappedPath = Join-LinuxMappedCgroupPath -MountPoint $selected[0].MountPoint -MountRoot $selected[0].Root -CgroupPath $current
            # Linux virtual paths, not host filesystem paths: Join-Path would emit '\\' when the fixture suite runs under Windows PowerShell.
            $cap = Read-LinuxCgroupMemoryCap -Path ($mappedPath.TrimEnd('/') + '/memory.max') -ReadFile $ReadFile
            if ($null -ne $cap) { $caps += $cap }
            if ($current -eq $selected[0].Root) { break }
            $slash = $current.LastIndexOf('/')
            $current = if ($slash -le 0) { '/' } else { $current.Substring(0, $slash) }
        }
        if ($caps.Count -eq 0) { throw 'no finite memory.max cap in visible current cgroup ancestry' }
        $effective = ($caps | Measure-Object -Minimum).Minimum
        return [PSCustomObject]@{ Ok = $true; EffectiveMemoryCapBytes = [long]$effective; Detail = "host cgroup cap: $effective bytes (mapped root $rootMapped, current $mapped)" }
    } catch {
        return [PSCustomObject]@{ Ok = $false; EffectiveMemoryCapBytes = $null; Detail = $_.Exception.Message }
    }
}

function global:Get-LinuxBuildSlotRoot {
    param([string]$LockRoot = '')
    if ($LockRoot) {
        if (-not [System.IO.Path]::IsPathRooted($LockRoot)) { throw "LockRoot must be an absolute path: $LockRoot" }
        return $LockRoot
    }
    if ($env:PANGLOSS_STATE_ROOT) {
        if (-not [System.IO.Path]::IsPathRooted($env:PANGLOSS_STATE_ROOT)) { throw "PANGLOSS_STATE_ROOT must be an absolute path: $($env:PANGLOSS_STATE_ROOT)" }
        return (Join-Path $env:PANGLOSS_STATE_ROOT 'build-slots-linux')
    }
    return (Join-Path ([System.IO.Path]::GetTempPath()) 'PanGloss-build-slots-linux')
}

function global:Get-LinuxFreeSpaceGB {
    param([string]$Path)
    if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path)) { return $null }
    try {
        $line = & df -Pk -- $Path 2>$null | Select-Object -Last 1
        if ($line -notmatch '^\S+\s+(\d+)\s+(\d+)\s+(\d+)\s+') { return $null }
        return [math]::Round(([double]$Matches[3] * 1KB) / 1GB, 1)
    } catch { return $null }
}

function global:Resolve-LinuxTargetDir {
    param([string]$RustRoot, [scriptblock]$DirectoryCreator = { param([string]$Path) New-Item -ItemType Directory -Force -Path $Path | Out-Null })
    $root = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } elseif ($env:PANGLOSS_SSD_CACHE_ROOT) { $env:PANGLOSS_SSD_CACHE_ROOT } elseif ($env:PANGLOSS_TARGET_ROOT) { $env:PANGLOSS_TARGET_ROOT } elseif ($env:PANGLOSS_CARGO_CACHE_ROOT) { $env:PANGLOSS_CARGO_CACHE_ROOT } else { $null }
    if (-not $root) { return $null }
    if (-not [System.IO.Path]::IsPathRooted($root)) { throw "Linux target root must be an absolute path: $root" }
    if ($env:CARGO_TARGET_DIR) { return $root }
    $target = $root.TrimEnd('/') + '/' + (Get-WorktreeSlug -RustRoot $RustRoot)
    & $DirectoryCreator $target
    return $target
}

function global:Use-LinuxSccache {
    param(
        [scriptblock]$CommandResolver = { Get-Command sccache -ErrorAction SilentlyContinue },
        [scriptblock]$DirectoryCreator = { param([string]$Path) New-Item -ItemType Directory -Force -Path $Path | Out-Null }
    )
    $command = & $CommandResolver
    if (-not $command) { return $false }
    $oldWrapper = $env:RUSTC_WRAPPER
    $oldDir = $env:SCCACHE_DIR
    $oldSize = $env:SCCACHE_CACHE_SIZE
    try {
        $env:RUSTC_WRAPPER = 'sccache'
        if (-not $env:SCCACHE_DIR -and $env:PANGLOSS_CARGO_CACHE_ROOT) {
            if (-not [System.IO.Path]::IsPathRooted($env:PANGLOSS_CARGO_CACHE_ROOT)) { throw "PANGLOSS_CARGO_CACHE_ROOT must be an absolute path: $($env:PANGLOSS_CARGO_CACHE_ROOT)" }
            $env:SCCACHE_DIR = $env:PANGLOSS_CARGO_CACHE_ROOT.TrimEnd('/') + '/sccache'
        }
        if ($env:SCCACHE_DIR) {
            if (-not [System.IO.Path]::IsPathRooted($env:SCCACHE_DIR)) { throw "SCCACHE_DIR must be an absolute path: $($env:SCCACHE_DIR)" }
            & $DirectoryCreator $env:SCCACHE_DIR
        }
        if (-not $env:SCCACHE_CACHE_SIZE -and $env:SCCACHE_DIR) {
            $freeGB = Get-LinuxFreeSpaceGB -Path $env:SCCACHE_DIR
            $sizeGB = if ($null -ne $freeGB) { [Math]::Min(150, [Math]::Max(20, [Math]::Floor($freeGB / 10))) } else { 20 }
            $env:SCCACHE_CACHE_SIZE = "${sizeGB}G"
        }
        return $true
    } catch {
        $env:RUSTC_WRAPPER = $oldWrapper
        $env:SCCACHE_DIR = $oldDir
        $env:SCCACHE_CACHE_SIZE = $oldSize
        throw
    }
}

function global:Get-BuildSlotHolders { return @() }

function global:Set-SccacheServerPriority {
    param([ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = 'BelowNormal')
    return 0
}

function global:Enter-BuildSlot {
    param([int]$MaxConcurrent = 2, [int]$TimeoutSeconds = 0, [string]$LockRoot = '')
    if ($MaxConcurrent -lt 1) { $MaxConcurrent = 1 }
    $root = Get-LinuxBuildSlotRoot -LockRoot $LockRoot
    New-Item -ItemType Directory -Force -Path $root | Out-Null
    $deadline = if ($TimeoutSeconds -le 0) { $null } else { [DateTime]::UtcNow.AddSeconds($TimeoutSeconds) }
    while ($true) {
        for ($i = 0; $i -lt $MaxConcurrent; $i++) {
            $path = Join-Path $root "slot$i.lock"
            $stream = $null
            try {
                $stream = [System.IO.File]::Open($path, [System.IO.FileMode]::OpenOrCreate, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::ReadWrite)
                $stream.Lock(0, 1)
                return [PSCustomObject]@{ Stream = $stream; Slot = $i; Released = $false }
            } catch [System.IO.IOException] {
                if ($stream) { $stream.Dispose() }
            } catch {
                if ($stream) { $stream.Dispose() }
                throw
            }
        }
        if ($deadline -and [DateTime]::UtcNow -ge $deadline) { return $null }
        Start-Sleep -Milliseconds 50
    }
}

function global:Exit-BuildSlot {
    param($Semaphore)
    if (-not $Semaphore -or $Semaphore.Released) { return }
    try {
        if ($Semaphore.Stream) {
            try { $Semaphore.Stream.Unlock(0, 1) } catch {}
            try { $Semaphore.Stream.Dispose() } catch {}
            $Semaphore.Released = $true
            return
        }
    } catch {}
    try { $Semaphore.Dispose() } catch {}
}

function global:Invoke-LinuxDirectProcess {
    param(
        [string]$Exe,
        [string[]]$CmdArgs,
        [string]$WorkingDirectory,
        [string]$CaptureStdoutPath = '',
        [ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = 'BelowNormal',
        [int]$JobMaxConcurrent = 2,
        [string]$SelfCgroupText = $null,
        [string]$MountInfoText = $null,
        [scriptblock]$ReadFile = $null,
        [scriptblock]$ProcessInvoker = $null,
        [object]$HostCgroupProof = $null
    )
    $proof = if ($null -ne $HostCgroupProof) { $HostCgroupProof } else {
        Get-LinuxHostCgroupPreflight -SelfCgroupText $SelfCgroupText -MountInfoText $MountInfoText -ReadFile $ReadFile
    }
    if (-not $proof.Ok) { throw "refusing to start managed process: $($proof.Detail)" }
    Write-Host "[pg] host cgroup preflight: $($proof.Detail)" -ForegroundColor DarkGray
    if ($ProcessInvoker) { return (& $ProcessInvoker $Exe @($CmdArgs) $WorkingDirectory) }
    $oldLocation = Get-Location
    try {
        Set-Location -LiteralPath $WorkingDirectory
        if ($CaptureStdoutPath) {
            & $Exe @($CmdArgs) > $CaptureStdoutPath
        } else {
            & $Exe @($CmdArgs)
        }
        return $LASTEXITCODE
    } finally {
        Set-Location -LiteralPath $oldLocation
    }
}

function global:Invoke-ProcessInJobObject {
    param(
        [Parameter(Mandatory)][string]$Exe,
        [string[]]$CmdArgs = @(),
        [string]$WorkingDirectory,
        [string]$CaptureStdoutPath = '',
        [ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = 'BelowNormal',
        [Nullable[int]]$JobMemoryGB,
        [Nullable[int]]$CpuRatePercent,
        [string]$Subject = 'build',
        [string]$SelfCgroupText = $null,
        [string]$MountInfoText = $null,
        [scriptblock]$ReadFile = $null,
        [scriptblock]$ProcessInvoker = $null,
        [object]$HostCgroupProof = $null
    )
    return Invoke-LinuxDirectProcess -Exe $Exe -CmdArgs $CmdArgs -WorkingDirectory $WorkingDirectory `
        -CaptureStdoutPath $CaptureStdoutPath -Priority $Priority -SelfCgroupText $SelfCgroupText `
        -MountInfoText $MountInfoText -ReadFile $ReadFile -ProcessInvoker $ProcessInvoker `
        -HostCgroupProof $HostCgroupProof
}

function global:Invoke-CargoWithReaper {
    param(
        [string]$Exe,
        [string[]]$CmdArgs,
        [string]$WorkingDirectory,
        [string]$CaptureStdoutPath = '',
        [ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = 'BelowNormal',
        [int]$JobMaxConcurrent = 2,
        [string]$SelfCgroupText = $null,
        [string]$MountInfoText = $null,
        [scriptblock]$ReadFile = $null,
        [scriptblock]$ProcessInvoker = $null,
        [object]$HostCgroupProof = $null
    )
    return Invoke-LinuxDirectProcess -Exe $Exe -CmdArgs $CmdArgs -WorkingDirectory $WorkingDirectory `
        -CaptureStdoutPath $CaptureStdoutPath -Priority $Priority -JobMaxConcurrent $JobMaxConcurrent `
        -SelfCgroupText $SelfCgroupText -MountInfoText $MountInfoText -ReadFile $ReadFile `
        -ProcessInvoker $ProcessInvoker -HostCgroupProof $HostCgroupProof
}
