<#
  .DESCRIPTION
  Runs every rust/tools/tests/*.tests.ps1 file as its own child pwsh process (each one calls
  `exit` via Write-TestSummary, which would otherwise tear down this aggregator too) and prints
  one overall pass/fail summary. Non-zero exit if any file failed.

  Usage: pwsh -NoProfile -File rust/tools/tests/run-all.ps1
#>
$ErrorActionPreference = 'Stop'

$files = Get-ChildItem -Path $PSScriptRoot -Filter '*.tests.ps1' | Sort-Object Name
$overall = @()

foreach ($f in $files) {
    Write-Host ''
    Write-Host "=== $($f.Name) ===" -ForegroundColor Cyan
    & pwsh -NoProfile -File $f.FullName
    $code = $LASTEXITCODE
    $overall += [PSCustomObject]@{ File = $f.Name; ExitCode = $code }
}

Write-Host ''
Write-Host '===== rust/tools/tests summary =====' -ForegroundColor Cyan
$failed = @($overall | Where-Object { $_.ExitCode -ne 0 })
foreach ($r in $overall) {
    $status = if ($r.ExitCode -eq 0) { 'PASS' } else { 'FAIL' }
    $color = if ($r.ExitCode -eq 0) { 'Green' } else { 'Red' }
    Write-Host "  $status  $($r.File)" -ForegroundColor $color
}
Write-Host "$($overall.Count) file(s), $($failed.Count) failed" -ForegroundColor $(if ($failed.Count -eq 0) { 'Green' } else { 'Red' })

if ($failed.Count -gt 0) { exit 1 }
exit 0
