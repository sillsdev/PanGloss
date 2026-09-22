<#
  .DESCRIPTION
  Covers: Test-ParentAlive / Test-ReapableScanProcess (rust/tools/_common.ps1) -- the liveness and
  selection rules behind `pg.ps1 -Mode gc`'s process sweeps. Everything runs against synthetic
  process records, so this NEVER enumerates, spawns, or terminates a real process.

  The central property under test is a safety one, and it is the reason these rules are split out
  from the code that calls taskkill: this sweep is machine-wide, so it can see Rust builds
  belonging to OTHER worktrees. Reaping an orphaned scanner is cheap to get right and cheap to get
  wrong -- its output pipe already has no reader. Reaping a compiler is not. So a cargo/rustc/link
  process must be unreapable by the scan sweep under every combination of age, CPU, and parentage.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

$now = Get-Date

function New-FakeProc {
    param([int]$Pid_, [string]$Name, [int]$ParentPid, [datetime]$Created)
    [PSCustomObject]@{
        ProcessId = $Pid_; Name = $Name; ParentProcessId = $ParentPid
        CreationDate = $Created; CommandLine = "$Name (synthetic)"
    }
}

$script:KilledProcessIds = @()
function taskkill {
    $script:KilledProcessIds += [int]$args[-1]
}

# A live shell, and a scan that is genuinely its child.
$liveShell = New-FakeProc -Pid_ 100 -Name 'pwsh.exe'  -ParentPid 1  -Created $now.AddMinutes(-30)
$childScan = New-FakeProc -Pid_ 101 -Name 'find.exe'  -ParentPid 100 -Created $now.AddMinutes(-10)
# An orphan: parent 999 appears nowhere in the snapshot.
$orphanScan = New-FakeProc -Pid_ 102 -Name 'find.exe' -ParentPid 999 -Created $now.AddMinutes(-35)
# The PID-reuse trap: parent PID 200 exists but was created after the child, so it cannot be the real parent.
$recycled = New-FakeProc -Pid_ 201 -Name 'find.exe'   -ParentPid 200 -Created $now.AddMinutes(-35)
$reusedPid = New-FakeProc -Pid_ 200 -Name 'notepad.exe' -ParentPid 1 -Created $now.AddMinutes(-1)

$snapshot = @($liveShell, $childScan, $orphanScan, $recycled, $reusedPid)

Test-Case 'a scan whose parent is alive is NOT an orphan' {
    Assert-True (Test-ParentAlive -Proc $childScan -Snapshot $snapshot)
}

Test-Case 'a scan whose parent is absent from the snapshot IS an orphan' {
    Assert-False (Test-ParentAlive -Proc $orphanScan -Snapshot $snapshot)
}

Test-Case 'PID reuse: a "parent" created after its child is not the parent' {
    # Without this rule the recycled PID reads as a live parent and the orphan is skipped forever.
    Assert-False (Test-ParentAlive -Proc $recycled -Snapshot $snapshot)
}

Test-Case 'orphan cleanup safely selects its exact compiler/linker set and preserves other process families' {
    $expectedNames = 'rustc.exe,cargo.exe,link.exe,lld-link.exe,rust-lld.exe,cc1.exe'
    Assert-Equal $expectedNames ($script:OrphanedCargoProcessNames -join ',')

    $liveParent = New-FakeProc -Pid_ 500 -Name 'pwsh.exe' -ParentPid 1 -Created $now.AddMinutes(-30)
    $link = New-FakeProc -Pid_ 501 -Name 'link.exe' -ParentPid 999 -Created $now.AddMinutes(-29)
    $lldLink = New-FakeProc -Pid_ 502 -Name 'lld-link.exe' -ParentPid 999 -Created $now.AddMinutes(-29)
    $rustLld = New-FakeProc -Pid_ 503 -Name 'rust-lld.exe' -ParentPid 999 -Created $now.AddMinutes(-29)
    $liveLld = New-FakeProc -Pid_ 504 -Name 'lld-link.exe' -ParentPid 500 -Created $now.AddMinutes(-28)
    $cc1 = New-FakeProc -Pid_ 505 -Name 'cc1.exe' -ParentPid 999 -Created $now.AddMinutes(-29)
    $cc1plus = New-FakeProc -Pid_ 506 -Name 'cc1plus.exe' -ParentPid 999 -Created $now.AddMinutes(-29)
    $script:KilledProcessIds = @()

    Remove-OrphanedCargoProcesses -WhatIfOnly:$false -Snapshot @($liveParent, $link, $lldLink, $rustLld, $liveLld, $cc1, $cc1plus)

    Assert-Equal 4 $script:KilledProcessIds.Count 'the three dead-parent linker processes and cc1 should be selected for cleanup'
    foreach ($processIdToCheck in 501, 502, 503, 505) {
        Assert-Contains $script:KilledProcessIds $processIdToCheck "orphan linker PID $processIdToCheck should be selected"
    }
    Assert-False ($script:KilledProcessIds -contains 504) 'an LLD process with a live parent must never be selected'
    Assert-False ($script:KilledProcessIds -contains 506) 'cc1plus was never part of the orphan-cleanup policy'
}

Test-Case 'an orphaned scan over both thresholds is reapable' {
    Assert-True (Test-ReapableScanProcess -Proc $orphanScan -Snapshot $snapshot -CpuSeconds 2110 -Now $now)
}

Test-Case 'a scan with a LIVE parent is never reapable, however much CPU it has burned' {
    Assert-False (Test-ReapableScanProcess -Proc $childScan -Snapshot $snapshot -CpuSeconds 99999 -Now $now)
}

Test-Case 'an orphaned scan below the CPU threshold is left alone' {
    Assert-False (Test-ReapableScanProcess -Proc $orphanScan -Snapshot $snapshot -CpuSeconds 5 -Now $now)
}

Test-Case 'a freshly started orphan is left alone even at high CPU (age threshold)' {
    $fresh = New-FakeProc -Pid_ 103 -Name 'find.exe' -ParentPid 999 -Created $now.AddSeconds(-20)
    Assert-False (Test-ReapableScanProcess -Proc $fresh -Snapshot ($snapshot + $fresh) -CpuSeconds 5000 -Now $now)
}

Test-Case 'PLAY NICELY: no Rust build process is ever reapable by the scan sweep' {
    # Worst case on every threshold at once; each must still be refused purely by name -- it could be another worktree's build.
    foreach ($n in 'cargo.exe', 'rustc.exe', 'link.exe', 'cc1.exe', 'cargo-nextest.exe', 'sccache.exe') {
        $rust = New-FakeProc -Pid_ 300 -Name $n -ParentPid 999 -Created $now.AddHours(-3)
        Assert-False (Test-ReapableScanProcess -Proc $rust -Snapshot ($snapshot + $rust) -CpuSeconds 999999 -Now $now) `
            "$n must never be selected by the scan sweep"
    }
}

Test-Case 'the reapable-name list contains no Rust build binary' {
    foreach ($n in 'cargo.exe', 'rustc.exe', 'link.exe', 'cc1.exe', 'cargo-nextest.exe', 'sccache.exe') {
        Assert-False ($script:ReapableScanNames -contains $n) "$n must not be in ReapableScanNames"
    }
}

Test-Case 'an orphaned governor that supervises nothing is reapable' {
    # The observed leak: procgov outliving its launching shell with no build under it.
    $gov = New-FakeProc -Pid_ 400 -Name 'procgov.exe' -ParentPid 999 -Created $now.AddMinutes(-30)
    Assert-True (Test-ReapableGovernorProcess -Proc $gov -Snapshot ($snapshot + $gov) -Now $now)
}

Test-Case 'PLAY NICELY: an orphaned governor still supervising a live build is NEVER reapable' {
    # Launcher died but cargo/rustc still run underneath: this governor owns another worktree's build.
    $gov = New-FakeProc -Pid_ 401 -Name 'procgov.exe' -ParentPid 999 -Created $now.AddMinutes(-30)
    $build = New-FakeProc -Pid_ 402 -Name 'cargo.exe' -ParentPid 401 -Created $now.AddMinutes(-29)
    Assert-False (Test-ReapableGovernorProcess -Proc $gov -Snapshot ($snapshot + $gov + $build) -Now $now)
}

Test-Case 'a governor whose parent is alive is never reapable' {
    $gov = New-FakeProc -Pid_ 403 -Name 'procgov.exe' -ParentPid 100 -Created $now.AddMinutes(-10)
    Assert-False (Test-ReapableGovernorProcess -Proc $gov -Snapshot ($snapshot + $gov) -Now $now)
}

Test-Case 'a freshly started orphaned governor is left alone (age threshold)' {
    $fresh = New-FakeProc -Pid_ 404 -Name 'procgov.exe' -ParentPid 999 -Created $now.AddSeconds(-20)
    Assert-False (Test-ReapableGovernorProcess -Proc $fresh -Snapshot ($snapshot + $fresh) -Now $now)
}

Test-Case 'PID reuse: a "child" predating the governor does not protect it' {
    # Without the creation-time guard a recycled PID reads as a live child and the orphan survives forever.
    $gov = New-FakeProc -Pid_ 405 -Name 'procgov.exe' -ParentPid 999 -Created $now.AddMinutes(-30)
    $stale = New-FakeProc -Pid_ 406 -Name 'cargo.exe' -ParentPid 405 -Created $now.AddMinutes(-90)
    Assert-True (Test-ReapableGovernorProcess -Proc $gov -Snapshot ($snapshot + $gov + $stale) -Now $now)
}

Test-Case 'PLAY NICELY: no Rust build process is ever reapable by the governor sweep' {
    foreach ($n in 'cargo.exe', 'rustc.exe', 'link.exe', 'cc1.exe', 'cargo-nextest.exe', 'sccache.exe') {
        $rust = New-FakeProc -Pid_ 407 -Name $n -ParentPid 999 -Created $now.AddHours(-3)
        Assert-False (Test-ReapableGovernorProcess -Proc $rust -Snapshot ($snapshot + $rust) -Now $now) `
            "$n must never be selected by the governor sweep"
        Assert-False ($script:ReapableGovernorNames -contains $n) "$n must not be in ReapableGovernorNames"
    }
}

Write-TestSummary
