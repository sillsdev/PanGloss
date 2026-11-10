<#
  .DESCRIPTION
  Covers: the pure logic in rust/tools/_pmc-setup.ps1 behind pmc-setup.ps1 -- which registered tasks count as
  ready, and how the memory integrity state is read. Registers no task and reads no registry.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_pmc-setup.ps1"

function New-Found([string]$Arguments = $null, [string]$RunLevel = 'Highest') {
    @(Get-PmcTaskSpecs | ForEach-Object {
            [PSCustomObject]@{ Name = $_.Name; Execute = $_.Execute; Arguments = $(if ($Arguments) { $Arguments } else { $_.Arguments }); RunLevel = $RunLevel }
        })
}

Test-Case 'tasks matching the specs are ready' {
    Assert-Equal 0 (Get-PmcTaskProblems -Found (New-Found)).Count
}

Test-Case 'a missing task is named' {
    $p = Get-PmcTaskProblems -Found @((New-Found)[0])
    Assert-Equal 1 $p.Count
    Assert-True ($p[0] -match 'PanGloss PMC Stop') $p[0]
}

Test-Case 'no tasks at all names both' {
    Assert-Equal 2 (Get-PmcTaskProblems -Found @()).Count
}

Test-Case 'a task running other arguments is a problem' {
    $p = Get-PmcTaskProblems -Found (New-Found -Arguments '-start other.wprp')
    Assert-Equal 2 $p.Count
    Assert-True ($p[0] -match 'expected') $p[0]
}

Test-Case 'a task without highest privileges is a problem' {
    $p = Get-PmcTaskProblems -Found (New-Found -RunLevel 'Limited')
    Assert-True ($p[0] -match 'Highest') $p[0]
}

Test-Case 'the start task records the PG profile from the PMC folder' {
    $start = (Get-PmcTaskSpecs -Dir 'X:\pmc')[0]
    Assert-Equal '-start "X:\pmc\pg.wprp!PG" -filemode' $start.Arguments
}

Test-Case 'memory integrity running and configured on is running, no reboot pending' {
    $s = Get-MemoryIntegrityState -Running @(2, 7) -Configured 1
    Assert-True ($s.Running -and $s.OnAfterReboot -and -not $s.RebootPending) $s.Summary
}

Test-Case 'memory integrity running but configured off waits for a reboot' {
    $s = Get-MemoryIntegrityState -Running @(2) -Configured 0
    Assert-True ($s.Running -and -not $s.OnAfterReboot -and $s.RebootPending) $s.Summary
    Assert-Equal 'running now; off after the next reboot' $s.Summary
}

Test-Case 'memory integrity not running and not configured is off' {
    $s = Get-MemoryIntegrityState -Running @(7) -Configured $null
    Assert-Equal 'off' $s.Summary
}

Test-Case 'a policy turns memory integrity on whatever the setting says' {
    $s = Get-MemoryIntegrityState -Running @() -Configured 0 -PolicyEnabled 1
    Assert-True ($s.PolicyManaged -and $s.OnAfterReboot -and $s.RebootPending) $s.Summary
}

Write-TestSummary
