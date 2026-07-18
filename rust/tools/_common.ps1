# Shared helpers for build.ps1 / test.ps1: worktree/path resolution, disk-aware
# target-dir redirection, sccache wiring, a cross-worktree build-concurrency gate, and
# worktree-scoped cleanup of orphaned processes and stale build caches.
#
# Dot-source from build.ps1/test.ps1: . "$PSScriptRoot\_common.ps1"

$ErrorActionPreference = 'Stop'

$script:CargoCacheRoot = if ($env:PANGLOSS_CARGO_CACHE_ROOT) { $env:PANGLOSS_CARGO_CACHE_ROOT } else { 'G:\cargo-build-cache' }
$script:BuildSemaphoreName = 'Global\PanGlossCargoBuild'

function Get-RustRoot {
    # `git rev-parse --show-toplevel` always answers for whichever worktree the caller is
    # standing in, so this resolves correctly whether run from the main checkout or any
    # .claude/worktrees/* checkout -- no hardcoded paths.
    $top = git rev-parse --show-toplevel 2>$null
    if (-not $top) { throw "Not inside a git repo (run from within a PanGloss checkout)." }
    $rustDir = Join-Path $top 'rust'
    if (-not (Test-Path $rustDir)) { throw "No rust/ dir under $top" }
    return $rustDir
}

function Get-WorktreeSlug {
    param([string]$RustRoot)
    # Leaf directory name of the checkout root (e.g. "agent-a30b043e9e8bc26b2", or
    # "PanGloss" for the primary checkout) -- stable, unique, matches `git worktree list`.
    $repoRoot = Split-Path $RustRoot -Parent
    return (Split-Path $repoRoot -Leaf)
}

function Resolve-TargetDir {
    param([string]$RustRoot)
    # Always keep build output off C: by default -- 30+ worktrees each growing their own
    # multi-GB target/ dir is exactly what drove C: down to 1.3GB free. Reactive
    # (redirect only once a threshold is crossed) isn't good enough: it lets the drive
    # fill back up between crises. So redirect unconditionally, UNLESS a choice was
    # already made on purpose: an explicit CARGO_TARGET_DIR env var, or an existing
    # worktree-local .cargo/config.toml target-dir (never fight either one).
    if ($env:CARGO_TARGET_DIR) { return $env:CARGO_TARGET_DIR }
    $cfg = Join-Path $RustRoot '.cargo\config.toml'
    if (Test-Path $cfg) {
        $text = Get-Content $cfg -Raw
        if ($text -match 'target-dir\s*=') { return $null }  # let cargo read its own config
    }
    # Defensive: these scripts are checked out into every worktree, including on machines
    # that don't have the cache drive (default G:) at all. Degrade to cargo's normal local
    # target/ instead of crashing on New-Item if the drive is missing.
    $driveRoot = [System.IO.Path]::GetPathRoot($script:CargoCacheRoot)
    if ($driveRoot -and -not (Test-Path $driveRoot)) {
        Write-Host "[build-env] cache drive '$driveRoot' not found on this machine -- falling back to local target/ (not redirecting CARGO_TARGET_DIR)" -ForegroundColor Yellow
        return $null
    }
    $slug = Get-WorktreeSlug -RustRoot $RustRoot
    $target = Join-Path $script:CargoCacheRoot $slug
    New-Item -ItemType Directory -Force -Path $target | Out-Null
    return $target
}

function Use-Sccache {
    if (-not (Get-Command sccache -ErrorAction SilentlyContinue)) { return $false }
    $env:RUSTC_WRAPPER = 'sccache'
    if (-not $env:SCCACHE_DIR) { $env:SCCACHE_DIR = Join-Path $script:CargoCacheRoot 'sccache' }
    New-Item -ItemType Directory -Force -Path $env:SCCACHE_DIR | Out-Null
    return $true
}

function Enter-BuildSlot {
    param([int]$MaxConcurrent = 2)
    # Caveat: a named Windows semaphore's maximum count is fixed by whichever process
    # creates it first and is immutable for the object's lifetime (which itself lasts only
    # as long as at least one process holds it open). A later caller passing a different
    # -MaxConcurrent just opens the existing object and gets ITS max count silently --
    # requesting 1 here does nothing if another worktree's build already created this
    # semaphore with max=2 and is still running. In practice this means -MaxConcurrent is a
    # machine-wide convention (everyone should pass the same value, or none, to get the
    # default), not a per-invocation guarantee. Keep the default consistent across
    # build.ps1/test.ps1 (both default to 2) so this rarely bites in practice.
    try {
        $sem = New-Object System.Threading.Semaphore($MaxConcurrent, $MaxConcurrent, $script:BuildSemaphoreName)
    } catch [System.UnauthorizedAccessException] {
        $localName = $script:BuildSemaphoreName -replace '^Global\\', 'Local\'
        $sem = New-Object System.Threading.Semaphore($MaxConcurrent, $MaxConcurrent, $localName)
    }
    Write-Host "[build-env] waiting for a build slot (max $MaxConcurrent concurrent across all worktrees)..." -ForegroundColor DarkGray
    $sem.WaitOne() | Out-Null
    return $sem
}

function Exit-BuildSlot {
    param($Semaphore)
    if ($Semaphore) { $Semaphore.Release() | Out-Null; $Semaphore.Dispose() }
}

function Invoke-CargoWithReaper {
    param(
        [string]$Exe,
        # NOT named $Args -- that's PowerShell's automatic variable inside a function scope,
        # and a formal parameter of that name silently fails to bind (cargo would run with
        # zero arguments instead of erroring).
        [string[]]$CmdArgs,
        [string]$WorkingDirectory
    )
    # Start-Process (not `& cargo ...`) so we hold a real PID to reap. On Windows,
    # Stop-Process / Ctrl+C alone does NOT kill rustc/link.exe descendants -- only
    # `taskkill /T` (kill the whole tree) reliably does, which is what `finally` runs here
    # if the process is still alive (e.g. this script itself got Ctrl+C'd).
    $psi = Start-Process -FilePath $Exe -ArgumentList $CmdArgs -WorkingDirectory $WorkingDirectory -NoNewWindow -PassThru
    try {
        Wait-Process -Id $psi.Id
        return $psi.ExitCode
    } finally {
        if (-not $psi.HasExited) {
            & taskkill /T /F /PID $psi.Id 2>$null | Out-Null
        }
    }
}

function Get-LiveWorktreeSlugs {
    # Slugs (leaf dir names) of every worktree `git worktree list` currently knows about --
    # anything under the cache root NOT in this set belongs to a worktree that's been deleted.
    (git worktree list --porcelain | Select-String '^worktree (.+)$').Matches |
        ForEach-Object { Split-Path $_.Groups[1].Value -Leaf }
}

function Remove-StaleTargetCaches {
    param([switch]$WhatIfOnly = $true)
    if (-not (Test-Path $script:CargoCacheRoot)) { return }
    $live = @(Get-LiveWorktreeSlugs)
    Get-ChildItem $script:CargoCacheRoot -Directory | Where-Object { $_.Name -ne 'sccache' -and $live -notcontains $_.Name } |
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

function Remove-OrphanedCargoProcesses {
    param([switch]$WhatIfOnly = $true)
    # An orphan here is a rustc/cargo/link/cc1 process whose parent PID no longer resolves
    # to a live process -- e.g. a backgrounded shell that was killed or timed out without
    # taking its child tree with it (the POSIX process-group cleanup you'd expect doesn't
    # happen by default on Windows).
    $procs = Get-CimInstance Win32_Process -Filter "Name='rustc.exe' or Name='cargo.exe' or Name='link.exe' or Name='cc1.exe'"
    foreach ($p in $procs) {
        $parentAlive = [bool](Get-Process -Id $p.ParentProcessId -ErrorAction SilentlyContinue)
        if (-not $parentAlive) {
            if ($WhatIfOnly) {
                Write-Host "[gc] would kill orphan PID $($p.ProcessId) ($($p.Name), parent $($p.ParentProcessId) is dead)" -ForegroundColor Yellow
            } else {
                Write-Host "[gc] killing orphan PID $($p.ProcessId) ($($p.Name))" -ForegroundColor Yellow
                & taskkill /T /F /PID $p.ProcessId 2>$null | Out-Null
            }
        }
    }
}
