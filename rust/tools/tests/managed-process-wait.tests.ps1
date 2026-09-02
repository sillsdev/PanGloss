<#
  .DESCRIPTION
  Covers: Test-ManagedProcessTreeIdle / Wait-ManagedProcessTree / Invoke-ProcessInJobObject's wedge
  handling (rust/tools/_common.ps1).

  Why this file exists: observed live during a real release run, `Invoke-ProcessInJobObject`'s wait
  was a bare `Wait-Process -Id $psi.Id` with no timeout and no liveness check. nextest printed its
  full summary (cargo had finished, zero cargo/rustc/link/test processes remained anywhere), yet the
  outer AND inner procgov.exe stayed alive with a completely empty job tree, and pg.ps1 (then
  release.ps1's test gate) hung forever. Killing both procgov pids by hand let pg.ps1 return
  correctly nonzero -- the refusal path was sound, the unbounded WAIT was the defect.

  Most of this file is synthetic (like orphan-reaping.tests.ps1): no real process, no real sleep, no
  real clock, so the polling loop's logic is covered in milliseconds. The last section is a REAL
  process falsification -- procgov wrapping a real child that will not exit on its own for an hour --
  proving the fix by EFFECT (a bounded exit, a real kill) rather than by reading the code. The
  wrapped child there is a plain `pwsh -Command Start-Sleep`, not `cargo`, with the payload-counts-
  as-live derivation explicitly overridden (`-WaitExtraLiveNames @()`) so its tree reads idle
  immediately -- reproducing the exact alive-wrapper/empty-tree shape of the incident without
  needing a real build. The test right after it covers the other direction: WITHOUT that override
  the payload counts as live work, so a `-Mode run` probe absent from LiveBuildActivityNames is
  never killed mid-run.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

function New-FakeProc {
    param([int]$Pid_, [string]$Name, [int]$ParentPid, [datetime]$Created)
    [PSCustomObject]@{ ProcessId = $Pid_; Name = $Name; ParentProcessId = $ParentPid; CreationDate = $Created; CommandLine = "$Name (synthetic)" }
}

$now = Get-Date

# --- Test-ManagedProcessTreeIdle: pure predicate, synthetic snapshots ---

Test-Case 'a root with a live-build-activity descendant is NOT idle' {
    $root = New-FakeProc -Pid_ 1 -Name 'procgov.exe' -ParentPid 0 -Created $now.AddMinutes(-10)
    $child = New-FakeProc -Pid_ 2 -Name 'cargo.exe' -ParentPid 1 -Created $now.AddMinutes(-9)
    Assert-False (Test-ManagedProcessTreeIdle -RootPid 1 -Snapshot @($root, $child))
}

Test-Case 'a root with NO live-build-activity anywhere in its tree IS idle' {
    $root = New-FakeProc -Pid_ 1 -Name 'procgov.exe' -ParentPid 0 -Created $now.AddMinutes(-10)
    Assert-True (Test-ManagedProcessTreeIdle -RootPid 1 -Snapshot @($root))
}

Test-Case 'procgov.exe itself is deliberately excluded from live-build-activity -- an idle wrapper IS the stuck shape' {
    $root = New-FakeProc -Pid_ 1 -Name 'procgov.exe' -ParentPid 0 -Created $now.AddMinutes(-10)
    $innerProcgov = New-FakeProc -Pid_ 2 -Name 'procgov.exe' -ParentPid 1 -Created $now.AddMinutes(-9)
    Assert-True (Test-ManagedProcessTreeIdle -RootPid 1 -Snapshot @($root, $innerProcgov)) `
        'two nested procgov processes with nothing else running must still read as idle'
}

Test-Case 'live activity two levels deep (procgov -> cargo -> rustc) still counts, matching the nested-job shape' {
    $root = New-FakeProc -Pid_ 1 -Name 'procgov.exe' -ParentPid 0 -Created $now.AddMinutes(-10)
    $cargo = New-FakeProc -Pid_ 2 -Name 'cargo.exe' -ParentPid 1 -Created $now.AddMinutes(-9)
    $rustc = New-FakeProc -Pid_ 3 -Name 'rustc.exe' -ParentPid 2 -Created $now.AddMinutes(-8)
    Assert-False (Test-ManagedProcessTreeIdle -RootPid 1 -Snapshot @($root, $cargo, $rustc))
}

Test-Case 'a root absent from the snapshot (already exited) is never reported idle -- liveness is the caller''s job' {
    $other = New-FakeProc -Pid_ 2 -Name 'cargo.exe' -ParentPid 1 -Created $now.AddMinutes(-9)
    Assert-False (Test-ManagedProcessTreeIdle -RootPid 999 -Snapshot @($other))
}

Test-Case 'an ExtraLiveNames payload (a -Mode run probe) counts as live work; the SAME tree without it reads idle' {
    $root = New-FakeProc -Pid_ 1 -Name 'procgov.exe' -ParentPid 0 -Created $now.AddMinutes(-10)
    $probe = New-FakeProc -Pid_ 2 -Name 'predict_census.exe' -ParentPid 1 -Created $now.AddMinutes(-9)
    Assert-False (Test-ManagedProcessTreeIdle -RootPid 1 -Snapshot @($root, $probe) -ExtraLiveNames @('predict_census.exe')) `
        'a running payload named as extra-live must keep its tree non-idle for the whole run'
    Assert-True (Test-ManagedProcessTreeIdle -RootPid 1 -Snapshot @($root, $probe)) `
        'the same tree WITHOUT the extra name is the payload-exited incident shape and must read idle'
}

# --- Wait-ManagedProcessTree: the polling loop, driven by injected fakes (no real sleep, no real clock) ---

Test-Case 'a process that stays busy and then exits normally is never declared wedged' {
    $fake = [PSCustomObject]@{ HasExited = $false; Id = 1; ExitCode = 0 }
    $busy = @((New-FakeProc -Pid_ 1 -Name 'procgov.exe' -ParentPid 0 -Created $now), (New-FakeProc -Pid_ 2 -Name 'cargo.exe' -ParentPid 1 -Created $now))
    $polls = 0
    $sleep = { param($Seconds) $script:polls++; if ($script:polls -ge 3) { $fake.HasExited = $true } }
    $script:polls = 0
    $r = Wait-ManagedProcessTree -Process $fake -PollSeconds 1 -MaxIdleMinutes 3 `
        -SnapshotProvider { $busy } -SleepAction $sleep -NowProvider { Get-Date }
    Assert-False $r.Wedged
    Assert-Equal 0 $r.ExitCode
}

Test-Case 'a tree that goes idle and STAYS idle past MaxIdleMinutes is declared wedged, never exiting on its own' {
    $fake = [PSCustomObject]@{ HasExited = $false; Id = 1; ExitCode = $null }
    $idle = @((New-FakeProc -Pid_ 1 -Name 'procgov.exe' -ParentPid 0 -Created $now))
    $script:simNow = $now
    $sleep = { param($Seconds) $script:simNow = $script:simNow.AddMinutes(1) }
    $r = Wait-ManagedProcessTree -Process $fake -PollSeconds 1 -MaxIdleMinutes 3 `
        -SnapshotProvider { $idle } -SleepAction $sleep -NowProvider { $script:simNow }
    Assert-True $r.Wedged 'a tree idle for 3+ simulated minutes with a root that never exits must be declared wedged'
    Assert-False $fake.HasExited 'the fake root never exited on its own -- this proves the wait terminated on tree-idleness, not on the process itself'
}

Test-Case 'a BRIEF idle gap that recovers before MaxIdleMinutes must NOT be declared wedged -- legitimate builds have gaps' {
    $fake = [PSCustomObject]@{ HasExited = $false; Id = 1; ExitCode = 0 }
    $idleSnap = @((New-FakeProc -Pid_ 1 -Name 'procgov.exe' -ParentPid 0 -Created $now))
    $busySnap = @((New-FakeProc -Pid_ 1 -Name 'procgov.exe' -ParentPid 0 -Created $now), (New-FakeProc -Pid_ 2 -Name 'link.exe' -ParentPid 1 -Created $now))
    $script:simNow = $now
    $script:tick = 0
    $snapshotProvider = { if ($script:tick -lt 2) { $idleSnap } else { $busySnap } }
    $sleep = {
        param($Seconds)
        $script:tick++
        $script:simNow = $script:simNow.AddMinutes(1)
        if ($script:tick -ge 5) { $fake.HasExited = $true }
    }
    $r = Wait-ManagedProcessTree -Process $fake -PollSeconds 1 -MaxIdleMinutes 3 `
        -SnapshotProvider $snapshotProvider -SleepAction $sleep -NowProvider { $script:simNow }
    Assert-False $r.Wedged 'a 2-minute idle gap that recovers before the 3-minute bound must not trip the detector'
    Assert-Equal 0 $r.ExitCode
}

Test-Case 'idleness must be CONTIGUOUS -- flapping in and out never accumulates toward the bound' {
    # Idle/busy/idle/busy forever: if idle time accumulated ACROSS gaps this would eventually trip.
    $fake = [PSCustomObject]@{ HasExited = $false; Id = 1; ExitCode = 0 }
    $idleSnap = @((New-FakeProc -Pid_ 1 -Name 'procgov.exe' -ParentPid 0 -Created $now))
    $busySnap = @((New-FakeProc -Pid_ 1 -Name 'procgov.exe' -ParentPid 0 -Created $now), (New-FakeProc -Pid_ 2 -Name 'cargo.exe' -ParentPid 1 -Created $now))
    $script:simNow = $now
    $script:tick = 0
    $snapshotProvider = { if ($script:tick % 2 -eq 0) { $idleSnap } else { $busySnap } }
    $sleep = {
        param($Seconds)
        $script:tick++
        $script:simNow = $script:simNow.AddMinutes(2)
        if ($script:tick -ge 20) { $fake.HasExited = $true }
    }
    $r = Wait-ManagedProcessTree -Process $fake -PollSeconds 1 -MaxIdleMinutes 3 `
        -SnapshotProvider $snapshotProvider -SleepAction $sleep -NowProvider { $script:simNow }
    Assert-False $r.Wedged 'alternating idle/busy across 40 simulated minutes must never trip a 3-minute contiguous bound'
}

# --- Real-process falsification (see this file's own header for why a plain pwsh sleep stands in for cargo) ---

$script:CommonPath = (Resolve-Path "$PSScriptRoot\..\_common.ps1").Path
$script:WedgeProbeDir = New-TestTempDir -Prefix 'pg-wedge-probe'

Test-Case 'Invoke-ProcessInJobObject exits with the wedged code within seconds, against a REAL never-self-exiting root' {
    $childScript = @"
. '$($script:CommonPath -replace "'", "''")'
Import-PanGlossPlatformAdapter | Out-Null
`$code = Invoke-ProcessInJobObject -Exe pwsh -CmdArgs @('-NoProfile','-Command','Start-Sleep -Seconds 3600') ``
    -WorkingDirectory '$($script:WedgeProbeDir -replace "'", "''")' -Priority BelowNormal ``
    -WaitPollSeconds 1 -WaitMaxIdleMinutes 0.05 -Subject 'wedge-test' -WaitExtraLiveNames @()
"@
    $childPath = Join-Path $script:WedgeProbeDir 'wedge-child.ps1'
    Set-Content -Path $childPath -Value $childScript -Encoding UTF8
    $outPath = Join-Path $script:WedgeProbeDir 'wedge-out.txt'

    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath pwsh -ArgumentList @('-NoProfile', '-File', $childPath) `
        -PassThru -NoNewWindow -RedirectStandardOutput $outPath
    $finished = $p.WaitForExit(30000)
    $sw.Stop()

    Assert-True $finished 'the wrapper must return within 30s -- BEFORE this fix, a bare Wait-Process here hung indefinitely (observed live)'
    Assert-True ($sw.Elapsed.TotalSeconds -lt 20) "expected a bounded exit in well under 20s, took $($sw.Elapsed.TotalSeconds)s"
    Assert-Equal $script:ExitCodeManagedProcessWedged $p.ExitCode 'must exit with the dedicated wedged-process code, not 0 or a generic failure'

    $output = if (Test-Path $outPath) { Get-Content $outPath -Raw } else { '' }
    Assert-True ($output -match 'REFUSING to wait any longer') 'the refusal must be printed loudly, not silent'
    Assert-True ($output -match "exit $($script:ExitCodeManagedProcessWedged) means exactly this") 'the diagnostic must name the exit code it is about to use'
}

Test-Case 'a REAL payload absent from LiveBuildActivityNames is NOT killed mid-run -- the payload itself counts as live work' {
    # The false-positive direction: without the payload-name derivation, this 4s sleep would be declared wedged at ~1.2s and exit 27.
    $childScript = @"
. '$($script:CommonPath -replace "'", "''")'
Import-PanGlossPlatformAdapter | Out-Null
`$code = Invoke-ProcessInJobObject -Exe pwsh -CmdArgs @('-NoProfile','-Command','Start-Sleep -Seconds 4') ``
    -WorkingDirectory '$($script:WedgeProbeDir -replace "'", "''")' -Priority BelowNormal ``
    -WaitPollSeconds 1 -WaitMaxIdleMinutes 0.02 -Subject 'wedge-test'
exit `$code
"@
    $childPath = Join-Path $script:WedgeProbeDir 'live-payload-child.ps1'
    Set-Content -Path $childPath -Value $childScript -Encoding UTF8

    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath pwsh -ArgumentList @('-NoProfile', '-File', $childPath) -PassThru -NoNewWindow
    $finished = $p.WaitForExit(60000)
    $sw.Stop()

    Assert-True $finished 'the wrapper must still return once the payload genuinely exits'
    # Exit 27 AFTER >=4s is procgov itself wedging post-completion (a real, intermittent machine condition the detector exists for), so the pinned property is "never killed BEFORE the payload finished": a false positive returns 27 at ~1.2s.
    Assert-True (($p.ExitCode -eq 0) -or ($sw.Elapsed.TotalSeconds -ge 4)) `
        "the payload must never be killed mid-run: exit $($p.ExitCode) after $([math]::Round($sw.Elapsed.TotalSeconds,1))s"
}

Write-TestSummary
