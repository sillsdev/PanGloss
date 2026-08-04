<#
  Test entry point for the PanGloss Rust workspace. THIN FRONT END: all actual policy (target-dir
  redirection, sccache wiring, worktree base-commit check, disk/build-slot gates, ownership markers,
  the optimized pg-test-opt profile) now lives in rust/tools/pg.ps1. This script just translates
  its own long-standing parameter names into a `pg.ps1 -Mode test` call, so existing callers keep
  working unchanged.

  For corpus-backed suites (anything gated on samples/data/), use
  `rust\tools\pg.ps1 -Mode corpus-test` directly instead -- that mode validates every required
  corpus file before Cargo starts, runs ignored tests (every corpus gate is #[ignore]d precisely
  because it needs the private corpus), and fails a run that records zero executed corpus cases.
  This script has no equivalent switch on purpose: silently promoting an ordinary `test.ps1` call
  into the fail-closed corpus contract would surprise callers who didn't ask for it.

  Examples:
    rust\tools\test.ps1                                  # full workspace, pg-test-opt profile
    rust\tools\test.ps1 -Package pg-foma -Filter f5_diacritics
    rust\tools\test.ps1 -NoNextest                        # force plain `cargo test`
    rust\tools\test.ps1 -Gc                               # gc -Apply first, then test

  Passing extra cargo/nextest args: `& .\test.ps1 -- --no-capture` works when PowerShell parses the
  command itself, but `pwsh -File test.ps1 ... -- --no-capture` FAILS ("the parameter name '' is
  ambiguous") -- under -File the bare `--` reaches the parameter binder instead of the parser. Do NOT
  just drop the `--`: a single-dash arg that prefix-matches a parameter here binds to it silently
  (`-p foo` -> -Package, so cargo never sees it). For -File / automation callers use the env channel,
  which never reaches the binder:
    $env:PANGLOSS_EXTRA_ARGS = '--no-capture'; pwsh -File rust\tools\test.ps1 -Package pg-foma
  Verified; see Split-ExtraArgsSpec in _common.ps1 for the reproduction.
#>
# See pg.ps1's own note: without this, `test.ps1 --no-capture` binds "--no-capture" to -Package,
# which turns a passthrough flag into a package name and fails (or worse, filters) rather than
# reaching cargo.
[CmdletBinding(PositionalBinding = $false)]
param(
    [string]$Package = '',
    [string]$Filter = '',
    [switch]$DebugProfile,
    [switch]$NoNextest,
    [int]$MaxConcurrent = 2,
    # See build.ps1's equivalent: 0/empty means "let pg.ps1 decide" rather than restating its
    # defaults in a second place.
    [int]$Jobs = 0,
    [int]$TestThreads = 0,
    [ValidateSet('Idle', 'BelowNormal', 'Normal')][string]$Priority = '',
    [switch]$Gc,
    [switch]$NoSccache,
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$ExtraArgs
)

if ($Gc) {
    # Matches this flag's old behavior, now routed through pg.ps1's marker-aware gc -- see build.ps1's
    # equivalent comment for why that deletes strictly less than the old name-only sweep did.
    & "$PSScriptRoot\pg.ps1" -Mode gc -Apply
}

# A hashtable, not an array: splatting an ARRAY of ('-Name','value',...) strings passes them as
# plain POSITIONAL arguments (each string becomes one positional value, dashes and all) -- it does
# NOT parse "-Name" tokens as parameter names the way typing them on a command line would. Only
# hashtable splatting (@ht below) maps keys to named parameters.
$pgArgs = @{ Mode = 'test' }
if ($Package) { $pgArgs.Package = $Package }
if ($Filter) { $pgArgs.Filter = $Filter }
if ($DebugProfile) { $pgArgs.DebugProfile = $true }
if ($NoNextest) { $pgArgs.NoNextest = $true }
if ($MaxConcurrent) { $pgArgs.MaxConcurrent = $MaxConcurrent }
if ($NoSccache) { $pgArgs.NoSccache = $true }
if ($Jobs -gt 0) { $pgArgs.Jobs = $Jobs }
if ($TestThreads -gt 0) { $pgArgs.TestThreads = $TestThreads }
if ($Priority) { $pgArgs.Priority = $Priority }

& "$PSScriptRoot\pg.ps1" @pgArgs @ExtraArgs
exit $LASTEXITCODE
