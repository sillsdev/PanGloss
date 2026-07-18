<#
  Build entry point for the PanGloss Rust workspace. Run from any worktree (the main
  checkout or any .claude/worktrees/* checkout) -- it resolves its own paths, so there's
  no per-worktree copy-and-edit needed.

  Handles: worktree/path resolution, full-workspace vs single-crate builds, target-dir
  redirection (build output always lands under G:\cargo-build-cache, never on C:, so 30+
  worktrees can't fill the system drive), sccache wiring if installed,
  a cross-worktree concurrency gate, and process-tree cleanup so a killed/timed-out build
  doesn't leave orphaned rustc/link processes behind.

  Examples:
    rust\tools\build.ps1                       # full workspace, release
    rust\tools\build.ps1 -Package hc-foma       # single crate
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
