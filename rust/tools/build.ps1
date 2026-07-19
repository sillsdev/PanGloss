<#
  Build entry point for the PanGloss Rust workspace. Run from any worktree (the main
  checkout or any .claude/worktrees/* checkout) -- it resolves its own paths, so there's
  no per-worktree copy-and-edit needed.

  Handles: worktree/path resolution, full-workspace vs single-crate builds, target-dir
  redirection (prefers C:\cargo-targets -- NVMe, fast for the scattered small-file I/O a
  build/link does -- as long as C: keeps >=50GB free; falls back to G:\cargo-build-cache,
  a spinning HDD with much more capacity but worse random-I/O latency, once C: gets tight,
  so 30+ worktrees can't refill the space crisis that motivated moving off C: in the first
  place), sccache wiring if installed (its cache always lives on the HDD root -- cache hits
  are cheap single-blob reads, so capacity matters more there than seek time), a
  cross-worktree concurrency gate, and process-tree cleanup so a killed/timed-out build
  doesn't leave orphaned rustc/link processes behind.

  Examples:
    rust\tools\build.ps1                       # full workspace, release
    rust\tools\build.ps1 -Package pg-foma       # single crate
    rust\tools\build.ps1 -DebugProfile
    rust\tools\build.ps1 -Gc                    # reap orphans + stale caches first
    rust\tools\build.ps1 -- --features foo      # extra args passed through to cargo
#>
param(
    [string]$Package = '',
    [switch]$DebugProfile,
    [int]$MaxConcurrent = 2,
    [switch]$Gc,
    [switch]$NoSccache,
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$ExtraArgs
)

. "$PSScriptRoot\_common.ps1"

if ($Gc) {
    Remove-OrphanedCargoProcesses -WhatIfOnly:$false
    Remove-StaleTargetCaches -WhatIfOnly:$false
}

$rustRoot = Get-RustRoot
$targetDir = Resolve-TargetDir -RustRoot $rustRoot
if ($targetDir) { $env:CARGO_TARGET_DIR = $targetDir }

$usedSccache = if (-not $NoSccache) { Use-Sccache } else { $false }
if ($usedSccache) { Write-Host "[build] sccache active (cache: $($env:SCCACHE_DIR))" -ForegroundColor Cyan }

$cargoArgs = @('build')
if (-not $DebugProfile) { $cargoArgs += '--release' }
if ($Package) { $cargoArgs += @('-p', $Package) } else { $cargoArgs += '--workspace' }
if ($ExtraArgs) { $cargoArgs += $ExtraArgs }

$sem = Enter-BuildSlot -MaxConcurrent $MaxConcurrent
try {
    Write-Host "[build] cargo $($cargoArgs -join ' ')  (target-dir: $(if ($targetDir) { $targetDir } else { '<default>' }))" -ForegroundColor Cyan
    $code = Invoke-CargoWithReaper -Exe 'cargo' -CmdArgs $cargoArgs -WorkingDirectory $rustRoot
} finally {
    Exit-BuildSlot -Semaphore $sem
}
exit $code
