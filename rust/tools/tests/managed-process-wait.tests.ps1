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

Test-Case 'the production wait wakes as soon as the managed process exits instead of sleeping the whole poll interval' {
    $fake = [PSCustomObject]@{ HasExited = $false; Id = 1; ExitCode = 0 }
    $busy = @((New-FakeProc -Pid_ 1 -Name 'procgov.exe' -ParentPid 0 -Created $now), (New-FakeProc -Pid_ 2 -Name 'cargo.exe' -ParentPid 1 -Created $now))
    $script:waitCalls = 0
    $script:waitMilliseconds = $null
    $waitAction = {
        param($ManagedProcess, $Milliseconds)
        $script:waitCalls++
        $script:waitMilliseconds = $Milliseconds
        $ManagedProcess.HasExited = $true
        return $true
    }
    $r = Wait-ManagedProcessTree -Process $fake -PollSeconds 10 -MaxIdleMinutes 3 `
        -SnapshotProvider { $busy } -WaitAction $waitAction -NowProvider { Get-Date }
    Assert-Equal 1 $script:waitCalls 'one exit-aware wait replaces an unconditional ten-second sleep'
    Assert-Equal 10000 $script:waitMilliseconds 'the exit-aware wait retains the configured ten-second liveness cadence'
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

# --- Lingering job helpers: the observed cause of a wedge on a CLEAN run (vctip.exe outlives link.exe inside the job) ---

Test-Case 'Remove-LingeringJobHelpers kills only the named telemetry helper among the job members and returns exactly that row' {
    $members = @(
        (New-FakeProc -Pid_ 1 -Name 'procgov.exe' -ParentPid 0 -Created $now.AddMinutes(-10)),
        (New-FakeProc -Pid_ 7 -Name 'vctip.exe' -ParentPid 4242 -Created $now.AddMinutes(-2)),
        (New-FakeProc -Pid_ 9 -Name 'sccache.exe' -ParentPid 1 -Created $now.AddMinutes(-9))
    )
    $script:killed = @()
    $reaped = @(Remove-LingeringJobHelpers -Members $members -KillAction { param($ProcessId) $script:killed += $ProcessId })
    Assert-Equal 1 $reaped.Count 'exactly one row reaped'
    Assert-Equal 7 $reaped[0].ProcessId 'the helper, found by job membership even though its parent (4242) is gone'
    Assert-Equal '7' ($script:killed -join ',') 'nothing but the helper is killed -- never the wrapper, never a real build process'
}

Test-Case 'an sccache SERVER (bare exe, no arguments) inside the job is reaped; an sccache CLIENT carrying a rustc command line is not' {
    $server = New-FakeProc -Pid_ 11 -Name 'sccache.exe' -ParentPid 4242 -Created $now.AddMinutes(-3)
    $server.CommandLine = '"C:\Users\x\.cargo\bin\sccache.exe"'
    $client = New-FakeProc -Pid_ 12 -Name 'sccache.exe' -ParentPid 5 -Created $now.AddMinutes(-1)
    $client.CommandLine = '"sccache" C:\toolchain\bin\rustc.exe --crate-name pg_rules --edition=2021 src\lib.rs'
    Assert-True (Test-SccacheServerRow -Row $server) 'the bare exe is the daemon'
    Assert-False (Test-SccacheServerRow -Row $client) 'a client is live build work and must never be killed'
    $script:killed = @()
    $reaped = @(Remove-LingeringJobHelpers -Members @($server, $client) -KillAction { param($ProcessId) $script:killed += $ProcessId })
    Assert-Equal '11' ($script:killed -join ',') 'only the daemon is reaped'
    Assert-Equal 1 $reaped.Count
}

Test-Case 'an empty member list reaps nothing and is not an error' {
    $reaped = @(Remove-LingeringJobHelpers -Members @())
    Assert-Equal 0 $reaped.Count
}

Test-Case 'Wait-ManagedProcessTree hands an idle tree to the LingerReaper and returns the exit code once the wrapper exits on its own' {
    $fake = [PSCustomObject]@{ HasExited = $false; Id = 1; ExitCode = 0 }
    $idle = @((New-FakeProc -Pid_ 1 -Name 'procgov.exe' -ParentPid 0 -Created $now))
    $script:reaperCalls = 0
    $reaper = {
        param($Snapshot)
        $script:reaperCalls++
        # Killing the helper empties the job; procgov then exits by itself, which is what the fake models here.
        $fake.HasExited = $true
        @((New-FakeProc -Pid_ 7 -Name 'vctip.exe' -ParentPid 4242 -Created $now))
    }
    $script:simNow = $now
    $sleep = { param($Seconds) $script:simNow = $script:simNow.AddMinutes(1) }
    $r = Wait-ManagedProcessTree -Process $fake -PollSeconds 1 -MaxIdleMinutes 3 `
        -SnapshotProvider { $idle } -SleepAction $sleep -NowProvider { $script:simNow } -LingerReaper $reaper
    Assert-Equal 1 $script:reaperCalls 'the reaper runs on the first idle poll, not after the idle bound'
    Assert-False $r.Wedged 'the wrapper exited once the helper was gone, so this is a normal return, not a wedge'
    Assert-Equal 0 $r.ExitCode
}

Test-Case 'a LingerReaper that finds nothing leaves the wedge detector exactly as before' {
    $fake = [PSCustomObject]@{ HasExited = $false; Id = 1; ExitCode = $null }
    $idle = @((New-FakeProc -Pid_ 1 -Name 'procgov.exe' -ParentPid 0 -Created $now))
    $script:simNow = $now
    $sleep = { param($Seconds) $script:simNow = $script:simNow.AddMinutes(1) }
    $r = Wait-ManagedProcessTree -Process $fake -PollSeconds 1 -MaxIdleMinutes 3 `
        -SnapshotProvider { $idle } -SleepAction $sleep -NowProvider { $script:simNow } -LingerReaper { param($Snapshot) @() }
    Assert-True $r.Wedged 'with no helper to reap, an idle wrapper is still declared wedged at the bound'
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
    # 180s, not 60s: procgov ALONE, no wrapper, takes 14-119s to return after this same 4s payload exits.
    $finished = $p.WaitForExit(180000)
    $sw.Stop()

    Assert-True $finished 'the wrapper must still return once the payload genuinely exits'
    # Exit 27 AFTER >=4s is procgov itself wedging post-completion (a real, intermittent machine condition the detector exists for), so the pinned property is "never killed BEFORE the payload finished": a false positive returns 27 at ~1.2s.
    Assert-True (($p.ExitCode -eq 0) -or ($sw.Elapsed.TotalSeconds -ge 4)) `
        "the payload must never be killed mid-run: exit $($p.ExitCode) after $([math]::Round($sw.Elapsed.TotalSeconds,1))s"
}

# --- The job object's hold: procgov's printed limit table is a request, the kernel's count is the enforcement ---

Test-Case 'Get-NamedJobActiveProcessCount answers -1 (could not look), never 0 (empty), for a job nobody created' {
    Assert-Equal -1 (Get-NamedJobActiveProcessCount -JobName "PanGloss-no-such-job-$([guid]::NewGuid().ToString('N'))") `
        '"I could not open the job" must never be reported as "the job is empty"'
}

Test-Case 'Wait-JobObjectTakesHold reports Held as soon as the kernel counts a process in the job' {
    $proc = [PSCustomObject]@{ HasExited = $false; Id = 1 }
    $script:counts = @(0, 0, 3)
    $script:i = 0
    $r = Wait-JobObjectTakesHold -Process $proc -JobName 'X' -TimeoutSeconds 30 `
        -CountProvider { param($Name) $script:counts[[Math]::Min($script:i++, 2)] } `
        -SleepAction { param($Milliseconds) } -NowProvider { Get-Date }
    Assert-True $r.Held
    Assert-Equal 'members' $r.Reason
    Assert-Equal 3 $r.ActiveProcesses
}

Test-Case 'a wrapper that finished before the window closed counts as held -- its own exit code is what speaks then' {
    $proc = [PSCustomObject]@{ HasExited = $true; Id = 1 }
    $r = Wait-JobObjectTakesHold -Process $proc -JobName 'X' -CountProvider { param($Name) 0 } -SleepAction { param($Milliseconds) }
    Assert-True $r.Held
    Assert-Equal 'wrapper-exited' $r.Reason
}

Test-Case 'an empty job past the window is NOT held -- the ceiling was printed and never applied' {
    $proc = [PSCustomObject]@{ HasExited = $false; Id = 1 }
    $script:simNow = $now
    $r = Wait-JobObjectTakesHold -Process $proc -JobName 'X' -TimeoutSeconds 5 `
        -CountProvider { param($Name) 0 } -SleepAction { param($Milliseconds) $script:simNow = $script:simNow.AddSeconds(1) } `
        -NowProvider { $script:simNow }
    Assert-False $r.Held
    Assert-Equal 'timeout' $r.Reason
}

Test-Case 'a job that cannot be queried (-1) is not mistaken for a held one' {
    $proc = [PSCustomObject]@{ HasExited = $false; Id = 1 }
    $script:simNow = $now
    $r = Wait-JobObjectTakesHold -Process $proc -JobName 'X' -TimeoutSeconds 3 `
        -CountProvider { param($Name) -1 } -SleepAction { param($Milliseconds) $script:simNow = $script:simNow.AddSeconds(1) } `
        -NowProvider { $script:simNow }
    Assert-False $r.Held '-1 means "could not look", and a control that could not look has not acted'
}

# --- Real-launch falsification of the two defects that blocked every managed build on this machine ---

Test-Case 'the linger reaper resolves its helpers under `& script.ps1` -- the call shape every agent and release.ps1 use' {
    # The nested `&` shape specifically: under `pwsh -File` a closure resolves its helpers anyway.
    $childScript = @"
. '$($script:CommonPath -replace "'", "''")'
Import-PanGlossPlatformAdapter | Out-Null
`$reaper = New-JobLingerReaper
`$reaped = @(& `$reaper @() 'PanGloss-no-such-job-probe')
Write-Output "REAPER-RAN:`$(`$reaped.Count)"
"@
    $childPath = Join-Path $script:WedgeProbeDir 'reaper-scope-child.ps1'
    Set-Content -Path $childPath -Value $childScript -Encoding UTF8
    $out = & $childPath *>&1
    $text = ($out | ForEach-Object { "$_" }) -join "`n"
    Assert-True ($text -match 'REAPER-RAN:0') "the reaper must run and find nothing, not fail to resolve: $text"
}

Test-Case 'every governed launch in a row actually starts its payload inside the job -- no launch is lost to procgov job-setup failure' {
    # Six in a row, not one: the defect this pins broke 5 of 8 launches, so a single clean launch is a
    # 3-in-8 coincidence and six is ~0.3%. See docs/design/build-resource-governance.md.
    if (-not (Get-ProcGovPath)) { throw 'procgov is not installed: this gate cannot run, and a skip would read as a pass' }
    $childScript = @"
. '$($script:CommonPath -replace "'", "''")'
Import-PanGlossPlatformAdapter | Out-Null
foreach (`$i in 1..6) {
    # Captured to a file, not the pipeline: the payload writes to the inherited console, which never reaches this script's output stream.
    `$cap = Join-Path '$($script:WedgeProbeDir -replace "'", "''")' "launch-`$i.out"
    # A payload that outlives the hold window on purpose: one short enough to finish first would leave an empty job and prove nothing.
    `$code = Invoke-ProcessInJobObject -Exe 'pwsh' -CmdArgs @('-NoProfile', '-Command', "Write-Output 'LAUNCH-OK-`$i'; Start-Sleep -Seconds 2") ``
        -WorkingDirectory '$($script:WedgeProbeDir -replace "'", "''")' -CaptureStdoutPath `$cap -Priority BelowNormal ``
        -JobMemoryGB 2 -CpuRatePercent 25 -WaitPollSeconds 1 -WaitMaxIdleMinutes 3 -Subject 'launch-test'
    Write-Output (Get-Content `$cap -Raw -ErrorAction SilentlyContinue)
    Write-Output "LAUNCH-`$i-EXIT:`$code"
}
"@
    $childPath = Join-Path $script:WedgeProbeDir 'launch-hold-child.ps1'
    Set-Content -Path $childPath -Value $childScript -Encoding UTF8
    $out = & $childPath *>&1
    $text = ($out | ForEach-Object { "$_" }) -join "`n"
    Assert-False ($text -match 'Win32Exception \(87\)') "procgov failed to create its job object: $text"
    foreach ($i in 1..6) {
        Assert-True ($text -match "LAUNCH-OK-$i") "launch $i never started its payload: $text"
        Assert-True ($text -match "LAUNCH-$i-EXIT:0") "launch $i did not return 0: $text"
    }
    Assert-True ($text -match 'the ceiling is applied, not just requested') 'each launch must prove the job held a process, not just print procgov''s limit table'
}

Write-TestSummary
