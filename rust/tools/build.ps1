<#
  Build entry point for the PanGloss Rust workspace. THIN FRONT END: all actual policy (target-dir
  redirection, sccache wiring, worktree base-commit check, disk/build-slot gates, ownership
  markers) now lives in rust/tools/pg.ps1. This script just translates its own
  long-standing parameter names into a `pg.ps1 -Mode build` call, so existing callers keep working
  unchanged and there is exactly ONE place build policy is decided rather than two copies that drift.

  Examples:
    rust\tools\build.ps1                       # full workspace, release
    rust\tools\build.ps1 -Package pg-foma       # single crate
    rust\tools\build.ps1 -DebugProfile
    rust\tools\build.ps1 -Gc                    # gc -Apply first, then build
    rust\tools\build.ps1 -- --features foo      # extra args to cargo -- CALL OPERATOR ONLY, see below

  Passing extra cargo args: the `--` form above works when PowerShell itself parses the command
  (typing it, or `& .\build.ps1 -- --features foo`). It does NOT work via `pwsh -File build.ps1
  ... -- --features foo`, which fails with "the parameter name '' is ambiguous" -- under -File the
  bare `--` reaches the parameter binder instead of being consumed by the parser. Dropping the `--`
  is NOT a safe substitute: a single-dash cargo arg that prefix-matches a parameter here binds to it
  silently (`-p foo` -> -Package, so cargo never sees it). For -File / automation callers use the
  env channel instead, which never touches the binder:
    $env:PANGLOSS_EXTRA_ARGS = '--features foo'; pwsh -File rust\tools\build.ps1
  Verified; see Split-ExtraArgsSpec in _common.ps1 for the reproduction.
#>
# See pg.ps1's own note: without this, `build.ps1 --features foo` binds "--features" to -Package and
# the documented `-- --features foo` passthrough below never reaches cargo either.
[CmdletBinding(PositionalBinding = $false)]
param(
    [string]$Package = '',
    [switch]$DebugProfile,
    [int]$MaxConcurrent = 2,
    # Both default to "let pg.ps1 decide" (0 / empty) rather than restating its defaults here --
    # duplicating the numbers is exactly the two-copies-that-drift problem this front end exists to
    # avoid. Present only so a console user can override without dropping down to pg.ps1.
    [int]$Jobs = 0,
    [ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = '',
    [switch]$Gc,
    [switch]$NoSccache,
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$ExtraArgs
)

if ($Gc) {
    # Matches this flag's old behavior (reap orphans + delete stale caches before building), now
    # routed through pg.ps1's marker-aware gc instead of the old name-only sweep. Consequence worth
    # knowing: nothing currently carries an ownership marker, so this deletes NOTHING until managed
    # builds have written markers -- strictly safer than the old sweep, and reported rather than silent.
    & "$PSScriptRoot\pg.ps1" -Mode gc -Apply
}

# A hashtable, not an array: splatting an ARRAY of ('-Name','value',...) strings passes them as
# plain POSITIONAL arguments (each string becomes one positional value, dashes and all) -- it does
# NOT parse "-Name" tokens as parameter names the way typing them on a command line would. Only
# hashtable splatting (@ht below) maps keys to named parameters.
$pgArgs = @{ Mode = 'build' }
if ($Package) { $pgArgs.Package = $Package }
if ($DebugProfile) { $pgArgs.DebugProfile = $true }
if ($MaxConcurrent) { $pgArgs.MaxConcurrent = $MaxConcurrent }
if ($NoSccache) { $pgArgs.NoSccache = $true }
if ($Jobs -gt 0) { $pgArgs.Jobs = $Jobs }
if ($Priority) { $pgArgs.Priority = $Priority }

& "$PSScriptRoot\pg.ps1" @pgArgs @ExtraArgs
exit $LASTEXITCODE
