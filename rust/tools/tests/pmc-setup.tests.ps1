<#
  .DESCRIPTION
  Covers: the pure logic in rust/tools/_pmc-setup.ps1 behind pmc-setup.ps1 -- which registered tasks count as
  ready, how the memory integrity state is read, and when a known-count loop's counts verify the counter.
  Registers no task, reads no registry and takes no trace.
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

Test-Case 'counts matching the known loop difference verify' {
    # Counts from the first machine (issue #242): each process adds 16M to 20M on top of 2 instructions per iteration.
    $v = Get-KnownLoopVerdict -Iterations @(1000000000, 3000000000) -Counted @(2016243525, 6017540488)
    Assert-True $v.Pass "off by $($v.ErrorPercent)%"
    Assert-Equal 4000000000 $v.Expected
}

Test-Case 'a counter that is not counting instructions fails the known loop' {
    # The two-counter trace from parse-cost.md read about 2.1x the instructions; any such ratio misses the 1% window.
    $v = Get-KnownLoopVerdict -Iterations @(1000000000, 3000000000) -Counted @(4200000000, 12600000000)
    Assert-True (-not $v.Pass) "passed at $($v.ErrorPercent)%"
}

Test-Case 'a count just outside 1% of the known difference fails' {
    $v = Get-KnownLoopVerdict -Iterations @(1000000000, 3000000000) -Counted @(2016000000, (2016000000 + 4041000000))
    Assert-True (-not $v.Pass) "passed at $($v.ErrorPercent)%"
    $v = Get-KnownLoopVerdict -Iterations @(1000000000, 3000000000) -Counted @(2016000000, (2016000000 + 4039000000))
    Assert-True $v.Pass "failed at $($v.ErrorPercent)%"
}

Test-Case 'any count above zero is not enough' {
    $v = Get-KnownLoopVerdict -Iterations @(1000000000, 3000000000) -Counted @(1, 2)
    Assert-True (-not $v.Pass) 'a tiny positive count verified'
}

Test-Case 'the long run must be longer than the short run' {
    $threw = $false
    try { Get-KnownLoopVerdict -Iterations @(3000000000, 1000000000) -Counted @(1, 2) | Out-Null } catch { $threw = $true }
    Assert-True $threw 'reversed lengths were accepted'
}

Test-Case 'verify refuses on a non-x86_64 machine' {
    foreach ($arch in 'Arm64', 'X86', 'Arm') {
        $msg = $null
        try { Assert-KnownLoopArchitecture -Architecture $arch } catch { $msg = $_.Exception.Message }
        Assert-True ($msg -match 'unsupported architecture') "$arch was not refused: $msg"
    }
    Assert-KnownLoopArchitecture -Architecture 'X64'
}

Test-Case 'task specs build without WINDIR and use it when set' {
    $saved = $env:WINDIR
    try {
        $env:WINDIR = $null
        $unset = Get-PmcTaskSpecs
        Assert-Equal 'C:\Windows\System32\wpr.exe' $unset[0].Execute 'falls back to the default Windows directory'
        $env:WINDIR = 'D:\WinX\'
        Assert-Equal 'D:\WinX\System32\wpr.exe' (Get-PmcTaskSpecs)[0].Execute 'a set WINDIR wins'
    } finally { $env:WINDIR = $saved }
}

Write-TestSummary
