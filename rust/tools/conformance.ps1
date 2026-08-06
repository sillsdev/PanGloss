<#
  .DESCRIPTION
  Standalone front end onto Initialize-ConformanceSubmodule (rust/tools/_common.ps1) -- matches how
  build.ps1/test.ps1 front-end pg.ps1: this script exists so the same logic pg.ps1 wires into its
  own `test`/`corpus-test`/`new-worktree` preflight can also be run directly, on demand, without
  invoking any Cargo mode at all.

  Makes `machine/conformance` show up in a fresh worktree without anyone running
  `git submodule update` by hand. Checks a sentinel file first (machine/conformance/constructs.txt)
  and returns immediately -- at no cost -- if it's already there; otherwise does a SPARSE,
  path-scoped submodule init (conformance/ only, ~1MB) rather than a full checkout (~415MB) of the
  `machine` submodule, since this repo's own test suite only ever reads machine/conformance. See
  Initialize-ConformanceSubmodule's own comment in _common.ps1 for the measurements and the exact
  git recipe (and why it differs from the shorter recipe you might expect -- `git submodule update
  --no-checkout` is not valid syntax; a `git clone --separate-git-dir` equivalent is used instead).

  Exit codes: 0 on success (including "was already initialized"), 18
  ($script:ExitCodeConformanceSubmoduleMissing, same code pg.ps1 preflight exits with for this
  failure) if initialization was needed and failed -- e.g. no network reachable to github.com. The
  printed Detail always names the exact command to run by hand.

  Usage:
    rust\tools\conformance.ps1              # initialize if needed, report, exit
#>
[CmdletBinding()]
param()

. "$PSScriptRoot\_common.ps1"

$repoRoot = Get-RepoRoot
$result = Initialize-ConformanceSubmodule -RepoRoot $repoRoot

if ($result.Ok) {
    $color = if ($result.AlreadyPresent) { 'Green' } else { 'Cyan' }
    Write-Host "[conformance] ok ($($result.Mode)): $($result.Detail)" -ForegroundColor $color
    exit 0
}

Write-Host "[conformance] FAILED ($($result.Mode)): $($result.Detail)" -ForegroundColor Red
if ($result.RecoveryCommand) {
    Write-Host "[conformance] run by hand: $($result.RecoveryCommand)" -ForegroundColor Yellow
}
exit $script:ExitCodeConformanceSubmoduleMissing
