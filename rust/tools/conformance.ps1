<#
  .DESCRIPTION
  Standalone front end onto Sync-ConformanceSuite (rust/tools/_common.ps1) -- matches how
  build.ps1/test.ps1 front-end pg.ps1: this script exists so the same logic pg.ps1 wires into its
  own `test`/`corpus-test`/`new-worktree` preflight can also be run directly, on demand, without
  invoking any Cargo mode at all.

  Makes `conformance/` (the suite, sillsdev/fieldworks-parser-conformance) a filtered clone at exactly
  the commit named in `conformance.commit`. Returns at once, with no network and no git writes, when it
  is already there; moves it when the pin moved (unless it has uncommitted changes, which it leaves
  alone and says so). HC_CONFORMANCE_SUITE=<path> uses a local copy as is and skips the pin check.
  See Sync-ConformanceSuite in _common.ps1 and docs/design/conformance-suite.md.

  Exit codes: 0 on success (including "was already there"), 18
  ($script:ExitCodeConformanceSuiteMissing, same code pg.ps1 preflight exits with for this failure)
  if the suite could not be brought to the pin -- e.g. no network reachable to github.com, or local
  changes in conformance/. The printed Detail always names the command to run by hand.

  Usage:
    rust\tools\conformance.ps1              # fetch or move if needed, report, exit
#>
[CmdletBinding()]
param()

. "$PSScriptRoot\_common.ps1"

$repoRoot = Get-RepoRoot
$result = Sync-ConformanceSuite -RepoRoot $repoRoot

if ($result.Ok) {
    $color = if ($result.Mode -in @('already-present', 'override')) { 'Green' } else { 'Cyan' }
    Write-Host "[conformance] ok ($($result.Mode)): $($result.Detail)" -ForegroundColor $color
    exit 0
}

Write-Host "[conformance] FAILED ($($result.Mode)): $($result.Detail)" -ForegroundColor Red
if ($result.RecoveryCommand) {
    Write-Host "[conformance] run by hand: $($result.RecoveryCommand)" -ForegroundColor Yellow
}
exit $script:ExitCodeConformanceSuiteMissing
