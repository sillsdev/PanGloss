<#
  .DESCRIPTION
  Minimal test harness for rust/tools/tests/*.tests.ps1 -- plain PowerShell asserting with `throw`
  on failure, deliberately NOT Pester: the build-hardening design rules out taking a Pester
  dependency, so these tests must run with nothing installed beyond PowerShell itself.

  Dot-source this from a *.tests.ps1 file, then:
    Test-Case "description" { <body that throws on failure, e.g. via Assert-*> }
    ...
    Write-TestSummary   # prints a pass/fail summary and calls `exit` with a non-zero code on any failure

  Every *.tests.ps1 file is runnable standalone: `pwsh -NoProfile -File <file>.tests.ps1`.
#>
$ErrorActionPreference = 'Stop'

$script:TestResults = @()

function Test-Case {
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][scriptblock]$Body)
    try {
        & $Body
        $script:TestResults += [PSCustomObject]@{ Name = $Name; Pass = $true; Error = $null }
        Write-Host "  PASS  $Name" -ForegroundColor Green
    } catch {
        $script:TestResults += [PSCustomObject]@{ Name = $Name; Pass = $false; Error = $_.Exception.Message }
        Write-Host "  FAIL  $Name" -ForegroundColor Red
        Write-Host "        $($_.Exception.Message)" -ForegroundColor Red
    }
}

function Assert-True {
    param($Condition, [string]$Message = 'expected a true condition')
    if (-not $Condition) { throw $Message }
}

function Assert-False {
    param($Condition, [string]$Message = 'expected a false condition')
    if ($Condition) { throw $Message }
}

function Assert-Equal {
    param($Expected, $Actual, [string]$Message = '')
    if ($Expected -ne $Actual) {
        throw "$Message (expected [$Expected], got [$Actual])".Trim()
    }
}

function Assert-Contains {
    param([Parameter(Mandatory)][array]$Haystack, $Needle, [string]$Message = '')
    if ($Haystack -notcontains $Needle) {
        throw "$Message (expected collection to contain [$Needle]; got: $($Haystack -join ', '))".Trim()
    }
}

function New-TestTempDir {
    # Isolated directory under the OS temp root, never a real cache root, so a fixture can never touch it.
    param([string]$Prefix = 'pg-tools-test')
    $dir = Join-Path ([System.IO.Path]::GetTempPath()) "$Prefix-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    return $dir
}

function Write-TestSummary {
    $failed = @($script:TestResults | Where-Object { -not $_.Pass })
    Write-Host ''
    $color = if ($failed.Count -eq 0) { 'Green' } else { 'Red' }
    Write-Host "$($script:TestResults.Count) test(s), $($failed.Count) failed" -ForegroundColor $color
    if ($failed.Count -gt 0) {
        Write-Host 'Failed:' -ForegroundColor Red
        foreach ($f in $failed) { Write-Host "  - $($f.Name): $($f.Error)" -ForegroundColor Red }
        exit 1
    }
    exit 0
}
