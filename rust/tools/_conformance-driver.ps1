# Durable driver artifacts live outside worktree bin/obj cleanup; source exports are disposable.
function Get-ConformanceDriverCacheRoot {
    if ($env:PANGLOSS_CONFORMANCE_DRIVER_CACHE) { return $env:PANGLOSS_CONFORMANCE_DRIVER_CACHE }
    if ($IsWindows) { return (Join-Path $env:LOCALAPPDATA 'PanGloss/hc-conformance') }
    $root = if ($env:XDG_CACHE_HOME) { $env:XDG_CACHE_HOME } else { Join-Path $HOME '.cache' }
    return (Join-Path $root 'pangloss/hc-conformance')
}

function Get-MachineSourceTree {
    param([string]$Source, [string]$PinCommit)
    if (-not (Test-Path -LiteralPath $Source)) { return $null }
    $tree = @(& git -C $Source rev-parse --verify "$PinCommit`^{tree}" 2>$null)
    if ($LASTEXITCODE -eq 0 -and $tree.Count -eq 1 -and $tree[0] -match '^[0-9a-f]{40}$') { return $tree[0] }
    return $null
}

function Get-CachedConformanceDriver {
    param([string]$PinCommit, [string[]]$Sources)
    $cache = Join-Path (Get-ConformanceDriverCacheRoot) $PinCommit
    try {
        $manifest = Get-Content -LiteralPath (Join-Path $cache 'source.json') -Raw -ErrorAction Stop | ConvertFrom-Json
        if ($manifest.Pin -ne $PinCommit -or $manifest.Platform -ne [Runtime.InteropServices.RuntimeInformation]::RuntimeIdentifier) { return $null }
        $tree = $null
        foreach ($source in $Sources) {
            $tree = Get-MachineSourceTree -Source $source -PinCommit $PinCommit
            if ($tree) { break }
        }
        if (-not $tree -or $manifest.SourceTree -ne $tree) { return $null }
        foreach ($name in Get-ConformanceDriverFileNames) {
            $exe = Join-Path $cache $name
            if (Test-Path -LiteralPath $exe -PathType Leaf) {
                return [PSCustomObject]@{ Ok = $true; ExePath = $exe; Detail = "pin-matched cached driver at $exe (source tree $tree)" }
            }
        }
    } catch { return $null } # Incomplete or stale cache entries are rebuilt, never admitted.
    return $null
}

function Invoke-ConformanceDriverPublish {
    param([string]$Project, [string]$Output, [int]$Jobs)
    $slot = Enter-ResourceSlot -Pool build -MaxConcurrent 2 -TimeoutSeconds 300
    if (-not $slot) { throw 'timed out acquiring a build slot for the C# driver' }
    $previousSuite = $env:HC_CONFORMANCE_SUITE
    try {
        $memory = Test-MemoryReserve -AvailableGB (Get-AvailableMemoryGB)
        if (-not $memory.Ok) { throw "cannot build C# driver: $($memory.Detail)" }
        # Fixtures are runtime input; suppress the build-time clone (pinned by the driver publish tool test).
        if (-not $env:HC_CONFORMANCE_SUITE) { $env:HC_CONFORMANCE_SUITE = Get-ConformanceSuiteDir }
        $lines = @(& dotnet publish $Project -c Release -o $Output "-maxcpucount:$Jobs" --nologo 2>&1)
        if ($LASTEXITCODE -ne 0) { throw "dotnet publish failed: $($lines -join [Environment]::NewLine)" }
    } finally {
        $env:HC_CONFORMANCE_SUITE = $previousSuite
        Exit-ResourceSlot -Slot $slot
    }
}

function New-CachedConformanceDriver {
    param([string]$PinCommit, [string[]]$Sources, [int]$Jobs = 4)
    $cache = Join-Path (Get-ConformanceDriverCacheRoot) $PinCommit
    $temp = $null
    $stage = $null
    $lock = $null
    try {
        # Git archive reads the pin without changing or building in a registered checkout.
        $source = $null
        foreach ($candidate in ($Sources | Select-Object -Unique)) {
            $tree = Get-MachineSourceTree -Source $candidate -PinCommit $PinCommit
            if (-not $tree) { continue }
            $dirty = @(& git -C $candidate status --porcelain --untracked-files=no 2>$null)
            if ($LASTEXITCODE -ne 0 -or $dirty.Count -gt 0) { continue }
            $project = @(& git -C $candidate ls-tree -r --name-only $PinCommit -- src/SIL.Machine.Morphology.HermitCrab.Conformance 2>$null) |
                Where-Object { $_ -like '*.csproj' } | Select-Object -First 1
            if ($LASTEXITCODE -eq 0 -and $project) { $source = $candidate; break }
        }
        if (-not $source) { throw "no clean copyable Machine source contains pin $PinCommit and the conformance project; checked $($Sources -join '; ')" }
        $root = Get-ConformanceDriverCacheRoot
        New-Item -ItemType Directory -Force -Path $root | Out-Null
        # Serialize publication across lanes. A failed publisher leaves no valid manifest behind.
        $deadline = [DateTime]::UtcNow.AddMinutes(5)
        while (-not $lock) {
            try { $lock = [IO.File]::Open((Join-Path $root "$PinCommit.lock"), 'OpenOrCreate', 'ReadWrite', 'None') }
            catch [IO.IOException] {
                if ([DateTime]::UtcNow -ge $deadline) { throw "timed out locking driver cache $cache" }
                Start-Sleep -Milliseconds 250
            }
        }
        $hit = Get-CachedConformanceDriver -PinCommit $PinCommit -Sources $Sources
        if ($hit) { return $hit }
        $temp = Join-Path ([IO.Path]::GetTempPath()) ("pg-driver-" + [guid]::NewGuid().ToString('N'))
        $stage = Join-Path $root ("$PinCommit-" + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $temp, $stage | Out-Null
        $archive = Join-Path $temp 'source.zip'
        & git -C $source archive --format=zip "--output=$archive" $PinCommit 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "git archive failed for $source at $PinCommit" }
        [IO.Compression.ZipFile]::ExtractToDirectory($archive, $temp)
        Invoke-ConformanceDriverPublish -Project (Join-Path $temp $project) -Output $stage -Jobs $Jobs
        $driver = @(Get-ConformanceDriverFileNames | Where-Object { Test-Path -LiteralPath (Join-Path $stage $_) -PathType Leaf })
        if ($driver.Count -eq 0) { throw "dotnet publish produced no conformance apphost in $stage" }
        @{ Pin = $PinCommit; SourceTree = $tree; Platform = [Runtime.InteropServices.RuntimeInformation]::RuntimeIdentifier } |
            ConvertTo-Json | Set-Content -LiteralPath (Join-Path $stage 'source.json')
        if (Test-Path -LiteralPath $cache) { Remove-Item -LiteralPath $cache -Recurse -Force }
        Move-Item -LiteralPath $stage -Destination $cache
        $result = Get-CachedConformanceDriver -PinCommit $PinCommit -Sources $Sources
        if (-not $result) { throw "published driver cache failed validation at $cache" }
        return $result
    } catch {
        return [PSCustomObject]@{ Ok = $false; ExePath = $null; Detail = "could not build driver cache $cache`: $($_.Exception.Message)" }
    } finally {
        if ($lock) { $lock.Dispose() }
        foreach ($path in @($temp, $stage)) {
            if ($path -and (Test-Path -LiteralPath $path)) { Remove-Item -LiteralPath $path -Recurse -Force }
        }
    }
}
