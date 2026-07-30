<#
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

# A live shell, and a scan that is genuinely its child.
$liveShell = New-FakeProc -Pid_ 100 -Name 'pwsh.exe'  -ParentPid 1  -Created $now.AddMinutes(-30)
$childScan = New-FakeProc -Pid_ 101 -Name 'find.exe'  -ParentPid 100 -Created $now.AddMinutes(-10)
# An orphan: parent 999 appears nowhere in the snapshot.
$orphanScan = New-FakeProc -Pid_ 102 -Name 'find.exe' -ParentPid 999 -Created $now.AddMinutes(-35)
# The PID-reuse trap: parent PID 200 EXISTS, but was created after the child, so it cannot be the
# process that spawned it -- the real parent is gone.
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
    # The load-bearing test. Each of these is an orphan, ancient, and burning enormous CPU -- the
    # worst case on every threshold at once -- and every one must still be refused purely on name,
    # because it could belong to a build running in another worktree.
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

Write-TestSummary
