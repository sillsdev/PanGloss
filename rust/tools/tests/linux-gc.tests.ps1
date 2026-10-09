. "$PSScriptRoot\_test-harness.ps1"

if (-not $IsLinux) {
    Test-Case 'Linux gc fixtures run only on Linux' { Assert-True $true }
    Write-TestSummary
}

. "$PSScriptRoot\..\_common.ps1"

$root = New-TestTempDir -Prefix 'pg-linux-gc'
$effectiveUid = Get-LinuxEffectiveUid
$fakeProc = Join-Path $root 'proc'
$ledger = Join-Path $root 'build-slots'
$runLedger = Join-Path $root 'run-slots'
New-Item -ItemType Directory -Force -Path $fakeProc, $ledger, $runLedger | Out-Null

function Add-FakeLinuxProcess {
    param(
        [int]$ProcessId,
        [int]$ParentProcessId,
        [string]$Name,
        [int]$Uid = $effectiveUid,
        [long]$StartTicks = 1000,
        [string[]]$Arguments = @($Name),
        [string]$Cwd = '/tmp',
        [string]$ExecutablePath = ''
    )
    $path = Join-Path $fakeProc "$ProcessId"
    Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $path | Out-Null
    $fields = @('S', [string]$ParentProcessId)
    for ($i = 0; $i -lt 17; $i++) { $fields += '0' }
    $fields += [string]$StartTicks
    for ($i = 0; $i -lt 8; $i++) { $fields += '0' }
    Set-Content -LiteralPath (Join-Path $path 'stat') -Value "$ProcessId ($Name) $($fields -join ' ')" -NoNewline
    Set-Content -LiteralPath (Join-Path $path 'status') -Value "Name:`t$Name`nUid:`t$Uid`t$Uid`t$Uid`t$Uid`n" -NoNewline
    $argv = ($Arguments -join ([string][char]0)) + [char]0
    [System.IO.File]::WriteAllBytes((Join-Path $path 'cmdline'), [System.Text.Encoding]::UTF8.GetBytes($argv))
    [void][System.IO.Directory]::CreateSymbolicLink((Join-Path $path 'cwd'), $Cwd)
    if ($ExecutablePath) {
        [void][System.IO.Directory]::CreateSymbolicLink((Join-Path $path 'exe'), $ExecutablePath)
    }
}

function Get-FakeLinuxSnapshot {
    return Get-LinuxProcessSnapshot -ProcRoot $fakeProc -EffectiveUid $effectiveUid
}

$fakeStopper = {
    param($Process, [int]$EffectiveUid, [string]$ProcRoot)
    return [PSCustomObject]@{ Ok = $true; Detail = 'fake process exit recorded' }
}

Test-Case 'gc -WhatIf and the default process sweep leave a real orphan alive' {
    $fifo = Join-Path $root 'hold-open'
    & mkfifo -- $fifo
    Assert-Equal 0 $LASTEXITCODE 'mkfifo must create the owned process fixture'
    $launcher = Join-Path $root 'launch-orphan.ps1'
    Set-Content -LiteralPath $launcher -Value @'
param([string]$FifoPath, [string]$OutputRoot)
$command = "exec -a rustc /bin/bash -c 'read -t 300 < $FifoPath'"
$argumentList = '-c "' + $command.Replace('"', '\"') + '"'
$process = Start-Process -FilePath '/bin/bash' -ArgumentList $argumentList -PassThru `
    -RedirectStandardOutput (Join-Path $OutputRoot 'orphan.out') `
    -RedirectStandardError (Join-Path $OutputRoot 'orphan.err')
[Console]::WriteLine($process.Id)
'@
    $pidText = (& pwsh -NoProfile -File $launcher $fifo $root | Out-String).Trim()
    $orphanPid = [int]$pidText
    $procPath = "/proc/$orphanPid"
    try {
        $deadline = (Get-Date).AddSeconds(5)
        do {
            $orphan = Get-LinuxProcessRecord -ProcessId $orphanPid
            $parent = if ($orphan.ParentProcessId -gt 1) { Get-LinuxProcessRecord -ProcessId $orphan.ParentProcessId } else { $null }
            $adopted = $orphan.ParentProcessId -eq 1 -or ($parent -and (Get-LinuxReaperDescription -Process $parent))
            if ($adopted) { break }
            Start-Sleep -Milliseconds 50
        } while ((Get-Date) -lt $deadline)
        # PID 1 adopts orphans inside a PID namespace; on WSL the init relay does.
        Assert-Equal $true ([bool]$adopted) "the helper must exit so the test process is a true orphan (parent PID $($orphan.ParentProcessId))"

        $oldStateRoot = $env:PANGLOSS_STATE_ROOT
        $oldTargetRoot = $env:PANGLOSS_TARGET_ROOT
        try {
            $env:PANGLOSS_STATE_ROOT = Join-Path $root 'state'
            $env:PANGLOSS_TARGET_ROOT = Join-Path $root 'targets'
            New-Item -ItemType Directory -Force -Path $env:PANGLOSS_TARGET_ROOT | Out-Null
            $output = (& pwsh -NoProfile -File (Join-Path $PSScriptRoot '..\pg.ps1') -Mode gc -WhatIf 2>&1 | Out-String)
            $code = $LASTEXITCODE
        } finally {
            $env:PANGLOSS_STATE_ROOT = $oldStateRoot
            $env:PANGLOSS_TARGET_ROOT = $oldTargetRoot
        }
        Assert-Equal 0 $code 'gc -WhatIf must complete without a refusal'
        Assert-True (Test-Path -LiteralPath $procPath) 'gc -WhatIf must leave the spawned orphan alive'
        Assert-True ($output -match 'process sweep: killed 0; would reap \d+') 'the report must count zero process exits in WhatIf mode'

        $combined = (& pwsh -NoProfile -File (Join-Path $PSScriptRoot '..\pg.ps1') -Mode gc -Apply -WhatIf 2>&1 | Out-String)
        $combinedCode = $LASTEXITCODE
        Assert-Equal 0 $combinedCode 'WhatIf must take precedence when Apply is also supplied'
        Assert-True (Test-Path -LiteralPath $procPath) 'gc -Apply -WhatIf must leave the spawned orphan alive'
        Assert-True ($combined -match 'process sweep: killed 0; would reap \d+') 'combined Apply and WhatIf must report zero exits'

        # The adopting reaper must be in the census so ancestry can end at it.
        $orphanCensus = @($orphan) + @($parent | Where-Object { $_ })
        $default = Remove-LinuxOrphanedBuildProcesses -ProcessSnapshot ([PSCustomObject]@{
            Processes = $orphanCensus; Failures = @(); EffectiveUid = $effectiveUid
        }) -CallerPids @($PID) -ProcRoot '/proc' -LedgerPath $ledger -RunLedgerPath $runLedger
        Assert-Equal 0 $default.Killed.Count 'no -Apply must kill nothing'
        Assert-Equal 1 $default.WouldKill.Count 'no -Apply must report the eligible orphan by effect count'

        $applied = Remove-LinuxOrphanedBuildProcesses -ProcessSnapshot ([PSCustomObject]@{
            Processes = $orphanCensus; Failures = @(); EffectiveUid = $effectiveUid
        }) -Apply -CallerPids @($PID) -ProcRoot '/proc' -LedgerPath $ledger -RunLedgerPath $runLedger
        Assert-Equal 1 $applied.Killed.Count 'the reaper must count the process that actually exited'
        Assert-False (Test-Path -LiteralPath $procPath) 'the spawned orphan must no longer exist in /proc'
    } finally {
        if (Test-Path -LiteralPath $procPath) { Stop-Process -Id $orphanPid -Force -ErrorAction SilentlyContinue }
    }
}

Test-Case 'a process descended from a live build-slot owner is spared' {
    Add-FakeLinuxProcess -ProcessId 101 -ParentProcessId 1 -Name 'pwsh' -StartTicks 1010 `
        -Arguments @('pwsh', '/repo/live/rust/tools/pg.ps1', '-Mode', 'check') -Cwd '/repo/live'
    Add-FakeLinuxProcess -ProcessId 102 -ParentProcessId 101 -Name 'rustc' -StartTicks 1020 `
        -Arguments @('/usr/bin/rustc', '--out-dir', '/cache/live/debug/deps', 'src/lib.rs') -Cwd '/repo/live/rust'
    [PSCustomObject]@{
        Pid = 101; Mode = 'check'; Worktree = 'live'; WorktreePath = '/repo/live'
        TargetDir = '/cache/live'; ProcStartTicks = '1010'; AcquiredAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    } | ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path $ledger 'slot0.json')
    $snapshot = Get-FakeLinuxSnapshot
    $snapshot.Processes = @($snapshot.Processes | Where-Object { $_.ProcessId -in @(101, 102) })
    $result = Remove-LinuxOrphanedBuildProcesses -ProcessSnapshot $snapshot -Apply -CallerPids @(999999) `
        -ProcRoot $fakeProc -LedgerPath $ledger -RunLedgerPath $runLedger -Stopper $fakeStopper
    Assert-Equal 0 $result.Killed.Count 'a live managed build must have zero child processes killed'
    Assert-Equal 1 $result.Refused.Count 'the owned compiler must be counted as refused'
    Assert-True ($result.Refused[0].Reason -match 'owned by live build slot') 'the refusal must name the ownership evidence'
    Assert-True (@($snapshot.Processes | Where-Object { $_.ProcessId -eq 102 }).Count -eq 1) 'the owned process record must remain present'
}

Test-Case 'a detached Pangloss process matching a live run-slot record is spared' {
    Remove-Item -LiteralPath (Join-Path $ledger 'slot0.json') -Force -ErrorAction SilentlyContinue
    Add-FakeLinuxProcess -ProcessId 401 -ParentProcessId 1 -Name 'pwsh' -StartTicks 4010 `
        -Arguments @('pwsh', '/repo/live/rust/tools/pg.ps1', '-Mode', 'run') -Cwd '/repo/live'
    Add-FakeLinuxProcess -ProcessId 402 -ParentProcessId 1 -Name 'pangloss' -StartTicks 4020 `
        -Arguments @('/cache/live/debug/pangloss', 'batch') -Cwd '/cache/live'
    [PSCustomObject]@{
        Pid = 401; Mode = 'run'; Worktree = 'live'; WorktreePath = '/repo/live'
        TargetDir = '/cache/live'; ProcStartTicks = '4010'; AcquiredAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    } | ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path $runLedger 'slot0.json')
    $snapshot = Get-FakeLinuxSnapshot
    $snapshot.Processes = @($snapshot.Processes | Where-Object { $_.ProcessId -in @(401, 402) })
    $result = Remove-LinuxOrphanedBuildProcesses -ProcessSnapshot $snapshot -Apply -CallerPids @(999999) `
        -ProcRoot $fakeProc -LedgerPath $ledger -RunLedgerPath $runLedger -Stopper $fakeStopper
    Assert-Equal 0 $result.Killed.Count 'a detached process associated with a live run slot must remain alive'
    Assert-Equal 1 $result.Refused.Count 'the detached process must be counted as refused'
    Assert-True ($result.Refused[0].Reason -match 'owned by live run slot') "the refusal must identify its live run-slot owner: $($result.Refused[0].Reason)"
}

Test-Case 'a separate orphan remains reapable while a live build slot exists' {
    Remove-Item -LiteralPath (Join-Path $ledger 'slot0.json') -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath (Join-Path $runLedger 'slot0.json') -Force -ErrorAction SilentlyContinue
    Add-FakeLinuxProcess -ProcessId 601 -ParentProcessId 1 -Name 'pwsh' -StartTicks 6010 `
        -Arguments @('pwsh', '/repo/live/rust/tools/pg.ps1', '-Mode', 'check') -Cwd '/repo/live'
    Add-FakeLinuxProcess -ProcessId 602 -ParentProcessId 1 -Name 'cargo' -StartTicks 6020 `
        -Arguments @('/usr/bin/cargo', 'build') -Cwd '/tmp'
    [PSCustomObject]@{
        Pid = 601; Mode = 'check'; Worktree = 'live'; WorktreePath = '/repo/live'
        TargetDir = '/cache/live'; ProcStartTicks = '6010'; AcquiredAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    } | ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path $ledger 'slot0.json')
    $snapshot = Get-FakeLinuxSnapshot
    $snapshot.Processes = @($snapshot.Processes | Where-Object { $_.ProcessId -in @(601, 602) })
    $result = Remove-LinuxOrphanedBuildProcesses -ProcessSnapshot $snapshot -Apply -CallerPids @(999999) `
        -ProcRoot $fakeProc -LedgerPath $ledger -RunLedgerPath $runLedger -Stopper $fakeStopper
    Assert-Equal 1 $result.Killed.Count 'a path-separated orphan must still be reaped'
    Assert-Equal 0 $result.Refused.Count 'the separate orphan must not be refused as owned by the live build'
}

Test-Case 'an executable path alone cannot separate an orphan from a live build slot' {
    Remove-Item -LiteralPath (Join-Path $ledger 'slot0.json') -Force -ErrorAction SilentlyContinue
    Add-FakeLinuxProcess -ProcessId 611 -ParentProcessId 1 -Name 'pwsh' -StartTicks 6110 `
        -Arguments @('pwsh', '/repo/live/rust/tools/pg.ps1', '-Mode', 'check') -Cwd '/repo/live'
    Add-FakeLinuxProcess -ProcessId 612 -ParentProcessId 1 -Name 'cargo' -StartTicks 6120 `
        -Arguments @('/usr/bin/cargo') -Cwd '/'
    [PSCustomObject]@{
        Pid = 611; Mode = 'check'; Worktree = 'live'; WorktreePath = '/repo/live'
        TargetDir = '/cache/live'; ProcStartTicks = '6110'; AcquiredAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    } | ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path $ledger 'slot0.json')
    $snapshot = Get-FakeLinuxSnapshot
    $snapshot.Processes = @($snapshot.Processes | Where-Object { $_.ProcessId -in @(611, 612) })
    $result = Remove-LinuxOrphanedBuildProcesses -ProcessSnapshot $snapshot -Apply -CallerPids @(999999) `
        -ProcRoot $fakeProc -LedgerPath $ledger -RunLedgerPath $runLedger -Stopper $fakeStopper
    Assert-Equal 0 $result.Killed.Count 'a generic root working directory and executable path are insufficient evidence to kill'
    Assert-Equal 1 $result.Refused.Count 'unclear ownership alongside a live slot must be refused'
    Assert-True ($result.Refused[0].Reason -match 'too broad to separate') 'the refusal must explain why the process path is insufficient'
}

Test-Case 'an aged but live build-slot holder is refused rather than terminated' {
    $oldTime = (Get-Date).ToUniversalTime().AddMinutes(-60).ToString('o')
    Add-FakeLinuxProcess -ProcessId 101 -ParentProcessId 1 -Name 'pwsh' -StartTicks 1010 `
        -Arguments @('pwsh', '/repo/live/rust/tools/pg.ps1', '-Mode', 'check') -Cwd '/repo/live'
    [PSCustomObject]@{
        Pid = 101; Mode = 'check'; Worktree = 'live'; WorktreePath = '/repo/live'
        TargetDir = '/cache/live'; ProcStartTicks = '1010'; AcquiredAtUtc = $oldTime
    } | ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path $ledger 'slot0.json')
    $snapshot = Get-FakeLinuxSnapshot
    $snapshot.Processes = @($snapshot.Processes | Where-Object { $_.ProcessId -eq 101 })
    $result = Get-LinuxAgedBuildSlotRefusals -ProcessSnapshot $snapshot -CallerPids @(999999) `
        -LedgerPath $ledger -Now (Get-Date)
    Assert-Equal 1 $result.Refused.Count 'an unverifiable live slot must be reported as refused'
    Assert-True ($result.Refused[0].Reason -match 'cannot prove its complete work tree is idle') 'the refusal must explain the missing Linux ownership evidence'
}

Test-Case 'unknown process ownership is refused and counted' {
    Remove-Item -LiteralPath (Join-Path $ledger 'slot0.json') -Force -ErrorAction SilentlyContinue
    Add-FakeLinuxProcess -ProcessId 201 -ParentProcessId 987654 -Name 'rustc' -StartTicks 2010 `
        -Arguments @('/usr/bin/rustc', '--out-dir', '/cache/unknown/debug/deps', 'src/lib.rs')
    $snapshot = Get-FakeLinuxSnapshot
    $snapshot.Processes = @($snapshot.Processes | Where-Object { $_.ProcessId -eq 201 })
    $result = Remove-LinuxOrphanedBuildProcesses -ProcessSnapshot $snapshot -Apply -CallerPids @(999999) `
        -ProcRoot $fakeProc -LedgerPath $ledger -RunLedgerPath $runLedger -Stopper $fakeStopper
    Assert-Equal 0 $result.Killed.Count 'unknown parentage must never produce a kill'
    Assert-Equal 1 $result.Refused.Count 'the unresolved process must be counted as refused'
    Assert-True ($result.Refused[0].Reason -match 'ownership cannot be determined') 'the refusal must expose the unresolved ownership'
}

Test-Case 'WSL init relay parents are treated as reapers' {
    Remove-Item -LiteralPath (Join-Path $ledger 'slot0.json') -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath (Join-Path $runLedger 'slot0.json') -Force -ErrorAction SilentlyContinue
    Add-FakeLinuxProcess -ProcessId 701 -ParentProcessId 9001 -Name 'Relay(349)' -StartTicks 7010 `
        -Arguments @('/init') -ExecutablePath '/usr/bin/relay'
    Add-FakeLinuxProcess -ProcessId 702 -ParentProcessId 701 -Name 'rustc' -StartTicks 7020 `
        -Arguments @('/usr/bin/rustc', '--out-dir', '/cache/relay/debug/deps', 'src/lib.rs')
    Add-FakeLinuxProcess -ProcessId 703 -ParentProcessId 9002 -Name 'init' -StartTicks 7030 `
        -Arguments @('/init') -ExecutablePath '/usr/bin/relay'
    Add-FakeLinuxProcess -ProcessId 704 -ParentProcessId 703 -Name 'cargo' -StartTicks 7040 `
        -Arguments @('/usr/bin/cargo', 'build')
    Add-FakeLinuxProcess -ProcessId 705 -ParentProcessId 9003 -Name 'init' -StartTicks 7050 `
        -Arguments @('init') -ExecutablePath '/init'
    Add-FakeLinuxProcess -ProcessId 706 -ParentProcessId 705 -Name 'ld.lld' -StartTicks 7060 `
        -Arguments @('/usr/bin/ld.lld', 'input.o')
    $snapshot = Get-FakeLinuxSnapshot
    $snapshot.Processes = @($snapshot.Processes | Where-Object { $_.ProcessId -in @(701, 702, 703, 704, 705, 706) })
    $result = Remove-LinuxOrphanedBuildProcesses -ProcessSnapshot $snapshot -Apply -CallerPids @(999999) `
        -ProcRoot $fakeProc -LedgerPath $ledger -RunLedgerPath $runLedger -Stopper $fakeStopper
    Assert-Equal 3 $result.Killed.Count 'relay name, argv0 /init, and exe /init parents must each allow one confirmed reap'
    Assert-Equal 0 $result.Refused.Count 'recognized WSL init relay parents must not be refused for their own parentage'
}

Test-Case 'systemd user parents are treated as reapers' {
    Remove-Item -LiteralPath (Join-Path $ledger 'slot0.json') -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath (Join-Path $runLedger 'slot0.json') -Force -ErrorAction SilentlyContinue
    Add-FakeLinuxProcess -ProcessId 711 -ParentProcessId 9004 -Name 'systemd' -StartTicks 7110 `
        -Arguments @('/usr/lib/systemd/systemd', '--user')
    Add-FakeLinuxProcess -ProcessId 712 -ParentProcessId 711 -Name 'rustc' -StartTicks 7120 `
        -Arguments @('/usr/bin/rustc', '--out-dir', '/cache/systemd/debug/deps', 'src/lib.rs')
    $snapshot = Get-FakeLinuxSnapshot
    $snapshot.Processes = @($snapshot.Processes | Where-Object { $_.ProcessId -in @(711, 712) })
    $result = Remove-LinuxOrphanedBuildProcesses -ProcessSnapshot $snapshot -Apply -CallerPids @(999999) `
        -ProcRoot $fakeProc -LedgerPath $ledger -RunLedgerPath $runLedger -Stopper $fakeStopper
    Assert-Equal 1 $result.Killed.Count 'a child of systemd --user must be counted as reaped'
    Assert-Equal 0 $result.Refused.Count 'systemd --user parentage must be sufficient reaper evidence'
}

Test-Case 'an ordinary live shell parent is refused as not orphaned' {
    Remove-Item -LiteralPath (Join-Path $ledger 'slot0.json') -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath (Join-Path $runLedger 'slot0.json') -Force -ErrorAction SilentlyContinue
    Add-FakeLinuxProcess -ProcessId 721 -ParentProcessId 1 -Name 'bash' -StartTicks 7210 `
        -Arguments @('/bin/bash', '-c', 'sleep 30') -ExecutablePath '/bin/bash'
    Add-FakeLinuxProcess -ProcessId 722 -ParentProcessId 721 -Name 'rustc' -StartTicks 7220 `
        -Arguments @('/usr/bin/rustc', '--out-dir', '/cache/shell/debug/deps', 'src/lib.rs')
    $snapshot = Get-FakeLinuxSnapshot
    $snapshot.Processes = @($snapshot.Processes | Where-Object { $_.ProcessId -in @(721, 722) })
    $result = Remove-LinuxOrphanedBuildProcesses -ProcessSnapshot $snapshot -Apply -CallerPids @(999999) `
        -ProcRoot $fakeProc -LedgerPath $ledger -RunLedgerPath $runLedger -Stopper $fakeStopper
    Assert-Equal 0 $result.Killed.Count 'a compiler with an ordinary live shell parent must not be killed'
    Assert-Equal 1 $result.Refused.Count 'the live-parent candidate must be counted as refused'
    Assert-True ($result.Refused[0].Reason -match 'parent PID 721 is still alive') 'the refusal must name the live ordinary parent'
}

Test-Case 'a process owned by another UID is never selected for termination' {
    Remove-Item -LiteralPath (Join-Path $ledger 'slot0.json') -Force -ErrorAction SilentlyContinue
    Add-FakeLinuxProcess -ProcessId 301 -ParentProcessId 1 -Name 'cargo' -Uid ($effectiveUid + 1) -StartTicks 3010 `
        -Arguments @('/usr/bin/cargo', 'check')
    $snapshot = Get-FakeLinuxSnapshot
    $snapshot.Processes = @($snapshot.Processes | Where-Object { $_.ProcessId -eq 301 })
    $result = Remove-LinuxOrphanedBuildProcesses -ProcessSnapshot $snapshot -Apply -CallerPids @(999999) `
        -ProcRoot $fakeProc -LedgerPath $ledger -RunLedgerPath $runLedger -Stopper $fakeStopper
    Assert-Equal 0 $result.Killed.Count 'a process belonging to another UID must never be killed'
    Assert-Equal 1 $result.OtherUsers.Count 'the untouched process must be counted separately'
}

Test-Case 'a candidate in the caller process tree is refused' {
    Remove-Item -LiteralPath (Join-Path $ledger 'slot0.json') -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath (Join-Path $runLedger 'slot0.json') -Force -ErrorAction SilentlyContinue
    Add-FakeLinuxProcess -ProcessId 501 -ParentProcessId 1 -Name 'pwsh' -StartTicks 5010 -Arguments @('pwsh')
    Add-FakeLinuxProcess -ProcessId 502 -ParentProcessId 501 -Name 'rustc' -StartTicks 5020 `
        -Arguments @('/usr/bin/rustc', '--out-dir', '/cache/current/debug/deps', 'src/lib.rs')
    $snapshot = Get-FakeLinuxSnapshot
    $snapshot.Processes = @($snapshot.Processes | Where-Object { $_.ProcessId -in @(501, 502) })
    $result = Remove-LinuxOrphanedBuildProcesses -ProcessSnapshot $snapshot -Apply -CallerPids @(501) `
        -ProcRoot $fakeProc -LedgerPath $ledger -RunLedgerPath $runLedger -Stopper $fakeStopper
    Assert-Equal 0 $result.Killed.Count 'a process in the caller tree must never be killed'
    Assert-Equal 1 $result.Refused.Count 'a caller-tree process must be counted as refused'
    Assert-True ($result.Refused[0].Reason -match "caller's process tree") 'the refusal must name the caller-tree claim'
}

Test-Case 'Rust test binaries are recognized from their Cargo target path and harness arguments' {
    foreach ($binary in @(
        '/cache/pangloss/target/debug/deps/pg_cli-0123456789abcdef',
        '/cache/pangloss/target/x86_64-unknown-linux-gnu/pg-test-opt/deps/pg_cli-0123456789abcdef'
    )) {
        $process = [PSCustomObject]@{
            Name = 'pg_cli-0123456789'; Argv0 = $binary
            CommandLine = "$binary --test-threads 2"; Arguments = @($binary, '--test-threads', '2')
        }
        Assert-Equal 'rust-test-binary' (Get-LinuxGcProcessKind -Process $process)
    }
}

Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
Write-TestSummary
