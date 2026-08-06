<#
  .DESCRIPTION
  Covers: Enter-BuildSlot / Exit-BuildSlot (rust/tools/_common.ps1) after the switch from ONE
  counted semaphore to N named mutexes, plus the diagnostic slot ledger and commit-charge reporting.

  Why this file exists: the semaphore it replaced DEADLOCKED every worktree on this machine.
  A counted semaphore never restores its count when the holder dies, and in agent
  workflows the holder dies routinely (tool timeouts, agent stop/resume, detached invocations whose
  parent conversation is gone). Observed: 4+ worktrees waiting 20+ minutes with zero compilers alive
  machine-wide, recoverable only by hand-releasing the semaphore until it threw. The leak-on-kill
  test below is the regression gate for exactly that.

  TWO THINGS THAT SHAPE EVERY TEST HERE:

  1. Contention must be tested ACROSS PROCESSES, never within one. A Windows mutex has thread
     affinity and is RECURSIVE: the thread that already owns it re-acquires immediately instead of
     blocking. So a single-process "acquire twice, expect the second to fail" test passes trivially
     and proves nothing. (Found the hard way -- the first draft of this file asserted exactly that
     and reported the pool as broken when it was the test that was wrong.) This costs nothing in
     production, where every build is its own process, but it dictates the child-process helpers.
  2. Slot NAMES are redirected per-process, because the machine runs real builds and waiting on the
     live Global\PanGlossBuildSlot0 would either contend with them or block for the full timeout.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

$script:BuildSlotMutexPrefix = "Global\PanGlossTestSlot$PID-"
$script:CommonPath = (Resolve-Path "$PSScriptRoot\..\_common.ps1").Path
$script:ProbeDir = New-TestTempDir -Prefix 'pg-slot-probe'

# A child that tries to take a slot and reports the outcome, then releases immediately.
Set-Content -Path (Join-Path $script:ProbeDir 'probe.ps1') -Encoding UTF8 -Value @'
param([string]$Common, [string]$Prefix, [int]$Slots, [int]$TimeoutSec)
. $Common
$script:BuildSlotMutexPrefix = $Prefix
$s = Enter-BuildSlot -MaxConcurrent $Slots -TimeoutSeconds $TimeoutSec
if ($null -eq $s) { 'DENIED' } else { 'ACQUIRED'; Exit-BuildSlot -Semaphore $s }
'@

# A child that takes a slot and then sits on it, so the test can kill it mid-hold.
Set-Content -Path (Join-Path $script:ProbeDir 'holder.ps1') -Encoding UTF8 -Value @'
param([string]$Common, [string]$Prefix, [int]$Slots)
. $Common
$script:BuildSlotMutexPrefix = $Prefix
$s = Enter-BuildSlot -MaxConcurrent $Slots -TimeoutSeconds 30
if ($null -eq $s) { 'DENIED'; exit 1 }
'HOLDING'
Start-Sleep -Seconds 180
'@

function Invoke-SlotProbe {
    param([int]$Slots = 1, [int]$TimeoutSec = 2)
    $out = Join-Path $script:ProbeDir "probe-out-$([guid]::NewGuid().ToString('N')).txt"
    $p = Start-Process -FilePath 'pwsh' -PassThru -NoNewWindow -RedirectStandardOutput $out `
        -ArgumentList @('-NoProfile', '-File', (Join-Path $script:ProbeDir 'probe.ps1'),
        $script:CommonPath, $script:BuildSlotMutexPrefix, "$Slots", "$TimeoutSec")
    $p.WaitForExit(60000) | Out-Null
    return ((Get-Content $out -Raw) -split "`n" | Where-Object { $_ -match 'ACQUIRED|DENIED' } | Select-Object -First 1).Trim()
}

function Start-SlotHolder {
    param([int]$Slots = 1)
    $out = Join-Path $script:ProbeDir "holder-out-$([guid]::NewGuid().ToString('N')).txt"
    $p = Start-Process -FilePath 'pwsh' -PassThru -NoNewWindow -RedirectStandardOutput $out `
        -ArgumentList @('-NoProfile', '-File', (Join-Path $script:ProbeDir 'holder.ps1'),
        $script:CommonPath, $script:BuildSlotMutexPrefix, "$Slots")
    foreach ($attempt in 1..100) {
        Start-Sleep -Milliseconds 100
        if ((Test-Path $out) -and ((Get-Content $out -Raw) -match 'HOLDING')) { return $p }
        if ($p.HasExited) { throw "holder child exited early: $(Get-Content $out -Raw)" }
    }
    throw 'holder child never reported HOLDING'
}

Test-Case 'a slot is acquired and released, and can be reacquired' {
    $a = Enter-BuildSlot -MaxConcurrent 2 -TimeoutSeconds 5
    Assert-True ($null -ne $a) 'first acquire must succeed on an idle pool'
    Exit-BuildSlot -Semaphore $a
    $b = Enter-BuildSlot -MaxConcurrent 2 -TimeoutSeconds 5
    Assert-True ($null -ne $b) 'a released slot must be reusable'
    Exit-BuildSlot -Semaphore $b
}

Test-Case 'a full 1-slot pool denies another PROCESS' {
    # Cross-process, for the recursion reason in this file's header.
    $holder = Start-SlotHolder -Slots 1
    try {
        Assert-Equal 'DENIED' (Invoke-SlotProbe -Slots 1 -TimeoutSec 2) 'a held 1-slot pool must deny a second process'
    } finally { Stop-Process -Id $holder.Id -Force -ErrorAction SilentlyContinue }
}

Test-Case 'MaxConcurrent is honoured per invocation, not frozen by the first caller' {
    # A semaphore's max is fixed by whichever process creates it first; a mutex's is just how many names a caller waits on.
    $holder = Start-SlotHolder -Slots 1          # holds slot0 only
    try {
        Assert-Equal 'DENIED' (Invoke-SlotProbe -Slots 1 -TimeoutSec 2) '1-slot caller sees a full pool'
        Assert-Equal 'ACQUIRED' (Invoke-SlotProbe -Slots 2 -TimeoutSec 5) '2-slot caller may still take slot1'
    } finally { Stop-Process -Id $holder.Id -Force -ErrorAction SilentlyContinue }
}

Test-Case 'timing out returns null rather than throwing or hanging' {
    $holder = Start-SlotHolder -Slots 1
    try {
        # DENIED is Enter-BuildSlot having returned null, which pg.ps1 maps to exit 15.
        Assert-Equal 'DENIED' (Invoke-SlotProbe -Slots 1 -TimeoutSec 1)
    } finally { Stop-Process -Id $holder.Id -Force -ErrorAction SilentlyContinue }
}

Test-Case 'a slot whose holder is KILLED is reclaimed by the kernel, not leaked' {
    # A mutex is used because the kernel releases it when a holder dies; a counted semaphore leaks its count instead.
    $holder = Start-SlotHolder -Slots 1
    Assert-Equal 'DENIED' (Invoke-SlotProbe -Slots 1 -TimeoutSec 2) 'precondition: the slot is genuinely held'
    Stop-Process -Id $holder.Id -Force
    Start-Sleep -Milliseconds 750
    Assert-Equal 'ACQUIRED' (Invoke-SlotProbe -Slots 1 -TimeoutSec 15) `
        'the slot MUST be reclaimable after its holder was killed -- this is the deadlock regression'
}

Test-Case 'Exit-BuildSlot tolerates null and a double release without throwing' {
    # Runs inside a finally, so it must never itself throw and mask a build failure underneath it.
    Exit-BuildSlot -Semaphore $null
    $s = Enter-BuildSlot -MaxConcurrent 2 -TimeoutSeconds 5
    Exit-BuildSlot -Semaphore $s
    Exit-BuildSlot -Semaphore $s
    Assert-True $true 'no throw'
}

# Ledger: DIAGNOSTIC ONLY -- never consulted to decide whether a slot is free; the mutexes are the exclusion.

Test-Case 'the ledger records a holder and reports it as alive' {
    $env:PANGLOSS_STATE_ROOT = New-TestTempDir -Prefix 'pg-slots'
    try {
        Write-BuildSlotHolder -Slot 0 -Mode 'corpus-test' -Worktree 'crp-objective'
        $h = @(Get-BuildSlotHolders)
        Assert-Equal 1 $h.Count
        Assert-Equal 'corpus-test' $h[0].Mode
        Assert-Equal 'crp-objective' $h[0].Worktree
        Assert-True $h[0].Alive 'this process is the recorded holder, so it must read as alive'
        Clear-BuildSlotHolder -Slot 0
        Assert-Equal 0 @(Get-BuildSlotHolders).Count
    } finally { Remove-Item Env:\PANGLOSS_STATE_ROOT -ErrorAction SilentlyContinue }
}

Test-Case 'a stale entry from a dead holder reads as NOT alive rather than being trusted' {
    # Expected and harmless after a kill. This is a label on a diagnostic, never a decision input.
    $env:PANGLOSS_STATE_ROOT = New-TestTempDir -Prefix 'pg-slots'
    try {
        $dir = Get-BuildSlotLedgerPath
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        '{"Pid":999999,"Mode":"build","Worktree":"ghost","AcquiredAt":"01:02:03"}' |
            Set-Content -Path (Join-Path $dir 'slot1.json') -Encoding UTF8
        $h = @(Get-BuildSlotHolders)
        Assert-Equal 1 $h.Count
        Assert-False $h[0].Alive 'a nonexistent pid must never read as alive'
    } finally { Remove-Item Env:\PANGLOSS_STATE_ROOT -ErrorAction SilentlyContinue }
}

Test-Case 'a corrupt ledger entry is skipped, not thrown on' {
    $env:PANGLOSS_STATE_ROOT = New-TestTempDir -Prefix 'pg-slots'
    try {
        $dir = Get-BuildSlotLedgerPath
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        'not json at all' | Set-Content -Path (Join-Path $dir 'slot0.json') -Encoding UTF8
        Assert-Equal 0 @(Get-BuildSlotHolders).Count 'a half-written file must degrade to no-data'
    } finally { Remove-Item Env:\PANGLOSS_STATE_ROOT -ErrorAction SilentlyContinue }
}

Test-Case 'commit charge is reported and is internally consistent' {
    # Commit charge, not physical memory: a git fork can fail on MEM_COMMIT while physical memory is free.
    $c = Get-CommitChargeGB
    if ($null -ne $c) {
        Assert-True ($c.LimitGB -gt 0) 'commit limit must be positive'
        Assert-True ($c.CommittedGB -ge 0)
        Assert-True ($c.CommittedGB -le $c.LimitGB) 'committed cannot exceed the limit'
        Assert-True ($c.PercentUsed -ge 0 -and $c.PercentUsed -le 100) "percent out of range: $($c.PercentUsed)"
    }
}

Write-TestSummary
