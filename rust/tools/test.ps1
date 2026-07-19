<#
  Test entry point for the PanGloss Rust workspace. Run from any worktree (the main
  checkout or any .claude/worktrees/* checkout) -- it resolves its own paths.

  Prefers cargo-nextest when installed: proper per-test process isolation/reaping (a
  hung test can't wedge the whole run) and a clean compile-vs-run boundary in its
  output, unlike `cargo test`'s single interleaved stream. Falls back to `cargo test`
  automatically if nextest isn't installed.

  Same target-dir redirection (prefers C:\cargo-targets while it has room, falls back to
  the G:\cargo-build-cache HDD root otherwise), sccache wiring, cross-worktree concurrency
  gate, and process-tree cleanup as build.ps1 -- see _common.ps1.

  Examples:
    rust\tools\test.ps1                                  # full workspace, release
    rust\tools\test.ps1 -Package pg-foma -Filter f5_diacritics
    rust\tools\test.ps1 -NoNextest                        # force plain `cargo test`
    rust\tools\test.ps1 -Gc
#>
param(
    [string]$Package = '',
    [string]$Filter = '',
    [switch]$DebugProfile,
    [switch]$NoNextest,
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
if ($usedSccache) { Write-Host "[test] sccache active (cache: $($env:SCCACHE_DIR))" -ForegroundColor Cyan }

$useNextest = (-not $NoNextest) -and (Get-Command cargo-nextest -ErrorAction SilentlyContinue)

if ($useNextest) {
    $cargoArgs = @('nextest', 'run')
    if (-not $DebugProfile) { $cargoArgs += '--release' }
    if ($Package) { $cargoArgs += @('-p', $Package) } else { $cargoArgs += '--workspace' }
    if ($Filter) { $cargoArgs += $Filter }
} else {
    $cargoArgs = @('test')
    if (-not $DebugProfile) { $cargoArgs += '--release' }
    if ($Package) { $cargoArgs += @('-p', $Package) } else { $cargoArgs += '--workspace' }
    if ($Filter) { $cargoArgs += @('--', $Filter) }
}
if ($ExtraArgs) { $cargoArgs += $ExtraArgs }

$sem = Enter-BuildSlot -MaxConcurrent $MaxConcurrent
try {
    $runner = if ($useNextest) { 'nextest' } else { 'cargo test' }
    Write-Host "[test] cargo $($cargoArgs -join ' ')  (target-dir: $(if ($targetDir) { $targetDir } else { '<default>' }), runner: $runner)" -ForegroundColor Cyan
    $code = Invoke-CargoWithReaper -Exe 'cargo' -CmdArgs $cargoArgs -WorkingDirectory $rustRoot
} finally {
    Exit-BuildSlot -Semaphore $sem
}
exit $code
