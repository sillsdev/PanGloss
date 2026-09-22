<#
  .DESCRIPTION
  Commit-headroom admission is a different resource gate from available physical memory. These
  tests inject measurements and callbacks so they never start Cargo or depend on host pressure.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"
$script:ProductionResourceSlotWidth = $script:MaxResourceSlotWidth
# Keep mocked-mutex tests isolated from the remaining machine-wide named slots.
$script:MaxResourceSlotWidth = 2

function New-CommitProbeMutex {
    param([ValidateSet('current', 'free', 'busy', 'abandoned', 'failure', 'release-failure')][string]$Probe)
    $mutex = [PSCustomObject]@{ Probe = $Probe; WaitCalls = 0; ReleaseCalls = 0 }
    Add-Member -InputObject $mutex -MemberType ScriptMethod -Name WaitOne -Value {
        param([int]$Timeout)
        $this.WaitCalls++
        switch ($this.Probe) {
            'current' { throw [System.InvalidOperationException]::new('current slot must not be reprobed') }
            'free' { return $true }
            'busy' { return $false }
            'abandoned' { throw [System.Threading.AbandonedMutexException]::new('fixture abandoned mutex') }
            'failure' { throw [System.InvalidOperationException]::new('fixture wait failure') }
            'release-failure' { return $true }
        }
    } -Force
    Add-Member -InputObject $mutex -MemberType ScriptMethod -Name ReleaseMutex -Value {
        $this.ReleaseCalls++
        if ($this.Probe -eq 'release-failure') { throw [System.InvalidOperationException]::new('fixture release failure') }
    } -Force
    return $mutex
}

Test-Case 'commit admission requires the selected job cap plus the interactive reserve' {
    $r = Test-CommitReserve -AvailableGB 26 -JobCapGB 20 -ReserveGB 6 -FailClosed
    Assert-True $r.Ok $r.Detail
    Assert-True $r.MeasurementKnown
    Assert-Equal 26 $r.AvailableGB
    Assert-Equal 20 $r.JobCapGB
    Assert-Equal 6 $r.ReserveGB
    Assert-Equal 26 $r.RequiredGB
}

Test-Case 'commit admission refuses below cap plus interactive reserve and names both numbers' {
    $r = Test-CommitReserve -AvailableGB 1.8 -JobCapGB 19 -ReserveGB 6 -FailClosed
    Assert-False $r.Ok 'low commit headroom must refuse a managed launch'
    Assert-True $r.Detail.Contains('1.8GB') $r.Detail
    Assert-True $r.Detail.Contains('25GB') $r.Detail
}

Test-Case 'commit admission accepts exact equality' {
    $r = Test-CommitReserve -AvailableGB 25 -JobCapGB 19 -ReserveGB 6 -FailClosed
    Assert-True $r.Ok $r.Detail
}

Test-Case 'commit admission compares exact headroom without rounding the requirement' {
    $r = Test-CommitReserve -AvailableGB 25.25 -JobCapGB 19.125 -ReserveGB 6.125 -FailClosed
    Assert-True $r.Ok $r.Detail
    Assert-Equal 25.25 $r.RequiredGB 'the required sum must not be rounded before comparison'
}

Test-Case 'commit admission includes additional concurrent caps or a homogeneous concurrent count' {
    $byCaps = Test-CommitReserve -AvailableGB 44 -JobCapGB 19 -ReserveGB 6 -ConcurrentCapsGB @(19) -FailClosed
    Assert-True $byCaps.Ok $byCaps.Detail
    Assert-Equal 44 $byCaps.RequiredGB 'the explicit concurrent cap must be included once'

    $byCount = Test-CommitReserve -AvailableGB 43.99 -JobCapGB 19 -ReserveGB 6 -ConcurrentCount 1 -FailClosed
    Assert-False $byCount.Ok 'an occupied peer slot can make an otherwise single-slot fit unsafe'
    Assert-Equal 44 $byCount.RequiredGB 'the count represents additional jobs at the selected cap'
    Assert-True $byCount.Detail.Contains('plus 19GB for 1 occupied peer slot(s)') $byCount.Detail
    Assert-False (Test-CommitReserve -AvailableGB 100 -JobCapGB 19 -ReserveGB 6 `
        -ConcurrentCapsGB @(19) -ConcurrentCount 1).Ok 'mixed cap-list and count input must not be ambiguous'
}

Test-Case 'one shared slot-width contract bounds acquisition and census' {
    Assert-Equal 64 $script:ProductionResourceSlotWidth 'production uses one fixed maximum census/acquisition width'
    $contract = Get-ResourceSlotContract -Pool build -RequestedWidth 2
    Assert-Equal 2 $contract.RequestedWidth 'the requested width controls only the acquisition set'
    Assert-Equal 2 $contract.CensusWidth 'the test contract exposes its same authoritative census width'
    $threw = $false
    try { Get-ResourceSlotContract -Pool build -RequestedWidth 3 | Out-Null } catch { $threw = $true }
    Assert-True $threw 'requests beyond the shared supported width must be rejected explicitly'
    $threw = $false
    try { Enter-ResourceSlot -Pool build -MaxConcurrent 0 -TimeoutSeconds 1 | Out-Null } catch { $threw = $true }
    Assert-True $threw 'zero-width acquisition must be rejected, not silently clamped to one slot'
}

Test-Case 'peer caps use the maximum ordinary cap across supported build widths' {
    $oldJob = $env:PANGLOSS_JOB_MEM_GB
    $oldRun = $script:RunSlotMemoryGB
    try {
        Remove-Item Env:PANGLOSS_JOB_MEM_GB -ErrorAction SilentlyContinue
        $widthOneCap = Get-JobMemoryCapGB -MaxConcurrent 1 -TotalGB 64
        $widthTwoCap = Get-JobMemoryCapGB -MaxConcurrent 2 -TotalGB 64
        $peerCaps = Get-ResourcePeerCommitCaps -Pool build -ConcurrentCount 2 `
            -CurrentJobCapGB $widthTwoCap -TotalGB 64
        Assert-True $peerCaps.Ok $peerCaps.Detail
        Assert-True ($widthOneCap -gt $widthTwoCap) "fixture must exercise narrower peer's larger cap ($widthOneCap > $widthTwoCap)"
        Assert-Equal $widthOneCap $peerCaps.PeerCapGB 'each default build peer is reserved at the largest supported derived cap'
        Assert-Equal 2 $peerCaps.CapsGB.Count 'one conservative cap is returned per kernel-proven occupied peer'

        $script:RunSlotMemoryGB = 23
        $runPeers = Get-ResourcePeerCommitCaps -Pool run -ConcurrentCount 2 -CurrentJobCapGB 19
        Assert-True $runPeers.Ok $runPeers.Detail
        Assert-Equal 23 $runPeers.PeerCapGB 'light-run peers use their ordinary flat cap unless the current explicit cap is larger'
    } finally {
        $env:PANGLOSS_JOB_MEM_GB = $oldJob
        $script:RunSlotMemoryGB = $oldRun
    }
}

Test-Case 'slot occupancy counts its held mutex and releases every free probe immediately' {
    $current = New-CommitProbeMutex -Probe current
    $free = New-CommitProbeMutex -Probe free
    $slot = [PSCustomObject]@{ Index = 0; Mutexes = @($current, $free); Pool = 'build' }
    $result = Get-OccupiedResourceSlotCount -Slot $slot
    Assert-True $result.Ok $result.Detail
    Assert-Equal 1 $result.OccupiedCount 'the caller-owned mutex must count as occupied without being recursively probed'
    Assert-Equal 0 $current.WaitCalls 'the current slot must be counted from its owned handle'
    Assert-Equal 1 $free.ReleaseCalls 'a successfully probed free mutex must be released immediately'
}

Test-Case 'abandoned slot probes are treated as free only after releasing recovered ownership' {
    $current = New-CommitProbeMutex -Probe current
    $abandoned = New-CommitProbeMutex -Probe abandoned
    $slot = [PSCustomObject]@{ Index = 0; Mutexes = @($current, $abandoned); Pool = 'build' }
    $result = Get-OccupiedResourceSlotCount -Slot $slot
    Assert-True $result.Ok $result.Detail
    Assert-Equal 1 $result.OccupiedCount 'an abandoned mutex is recovered/free, not a live occupied job slot'
    Assert-Equal 1 $abandoned.ReleaseCalls 'WaitOne on an abandoned mutex transfers ownership that must be released'
}

Test-Case 'slot occupancy probe and release errors fail closed with explicit details' {
    foreach ($probe in @('failure', 'release-failure')) {
        $current = New-CommitProbeMutex -Probe current
        $other = New-CommitProbeMutex -Probe $probe
        $slot = [PSCustomObject]@{ Index = 0; Mutexes = @($current, $other); Pool = 'build' }
        $result = Get-OccupiedResourceSlotCount -Slot $slot
        Assert-False $result.Ok "$probe must not be misreported as an unoccupied slot"
        $expectedDetail = if ($probe -eq 'failure') { 'fixture wait failure' } else { 'fixture release failure' }
        Assert-True $result.Detail.Contains($expectedDetail) $result.Detail
    }
}

Test-Case 'commit snapshot exposes exact free commit separately from its display value' {
    $old = Get-Item Function:\global:Get-CimInstance -ErrorAction SilentlyContinue
    Set-Item Function:\global:Get-CimInstance -Value {
        param([string]$Class, [string]$ErrorAction)
        [PSCustomObject]@{ TotalVirtualMemorySize = 104857600; FreeVirtualMemory = 26266828 }
    }
    try {
        $snapshot = Get-CommitChargeGB
        Assert-Equal ([double]26266828 / 1MB) $snapshot.FreeGBExact 'the admission field must retain source KB precision'
        Assert-Equal ([math]::Round(([double]26266828 / 1MB), 1)) $snapshot.FreeGB 'the existing report field stays human-readable'
        $decision = Test-CommitReserve -AvailableGB $snapshot.FreeGBExact -JobCapGB 19 -ReserveGB 6 -FailClosed
        Assert-True $decision.Detail.Contains('25.05GB') $decision.Detail
        Assert-False $decision.Detail.Contains('25.049999') 'the human diagnostic should not expose binary floating-point noise'
    } finally {
        if ($old) { Set-Item Function:\global:Get-CimInstance -Value $old.ScriptBlock }
        else { Remove-Item Function:\global:Get-CimInstance -ErrorAction SilentlyContinue }
    }
}

Test-Case 'an unknown Windows commit measurement fails closed with an explicit result' {
    $r = Test-CommitReserve -AvailableGB $null -JobCapGB 19 -ReserveGB 6 -FailClosed
    Assert-False $r.Ok 'unknown commit headroom must not silently admit a Windows build'
    Assert-False $r.MeasurementKnown
    Assert-Equal $null $r.AvailableGB
    Assert-Equal 25 $r.RequiredGB
    Assert-True $r.Detail.Contains('unknown') $r.Detail
}

Test-Case 'unknown commit measurement can remain informational for adapter callers' {
    $r = Test-CommitReserve -AvailableGB $null -JobCapGB 2 -ReserveGB 6
    Assert-True $r.Ok $r.Detail
    Assert-False $r.MeasurementKnown
    Assert-Equal $null $r.AvailableGB
}

Test-Case 'invalid cap or reserve fails closed instead of producing a zero requirement' {
    Assert-False (Test-CommitReserve -AvailableGB 100 -JobCapGB 0 -ReserveGB 6).Ok
    Assert-False (Test-CommitReserve -AvailableGB 100 -JobCapGB 2 -ReserveGB -1).Ok
}

Test-Case 'low commit prevents rustfmt, slot, and Cargo callbacks from firing' {
    foreach ($stage in @('rustfmt', 'slot', 'cargo')) {
        $calls = [System.Collections.Generic.List[string]]::new()
        $r = Invoke-CommitGatedAction -AvailableGB 1.8 -JobCapGB 19 -ReserveGB 6 -FailClosed `
            -Action { [void]$calls.Add($stage) }.GetNewClosure()
        Assert-False $r.Ok "$stage must be refused under the same low-commit decision"
        Assert-Equal 0 $calls.Count "$stage callback fired despite a refused commit gate"
        Assert-False $r.ActionInvoked "$stage must be reported as not invoked"
    }
}

Test-Case 'sufficient commit invokes the supplied action exactly once and preserves stage order' {
    $calls = [System.Collections.Generic.List[string]]::new()
    foreach ($stage in @('rustfmt', 'slot', 'cargo')) {
        $r = Invoke-CommitGatedAction -AvailableGB 30 -JobCapGB 19 -ReserveGB 6 -FailClosed `
            -Action { [void]$calls.Add($stage) }.GetNewClosure()
        Assert-True $r.Ok $r.Detail
        Assert-True $r.ActionInvoked
    }
    Assert-Equal 'rustfmt,slot,cargo' ($calls -join ',') 'admitted stages must keep caller order'
}

Test-Case 'two held slots exceed one-cap headroom and post-slot refusal releases before Cargo' {
    $originalRelease = (Get-Item Function:\script:Exit-ResourceSlot).ScriptBlock
    $script:releasedSlots = 0
    $script:cargoInvocations = 0
    Set-Item Function:\script:Exit-ResourceSlot -Value {
        param($Slot)
        $script:releasedSlots++
    }
    try {
        $current = New-CommitProbeMutex -Probe current
        $occupiedPeer = New-CommitProbeMutex -Probe busy
        $slot = [PSCustomObject]@{ Index = 0; Mutexes = @($current, $occupiedPeer); Pool = 'build' }
        $occupancy = Get-OccupiedResourceSlotCount -Slot $slot
        Assert-True $occupancy.Ok $occupancy.Detail
        Assert-Equal 2 $occupancy.OccupiedCount 'the effect must see both the current and peer kernel-held slots'

        $result = Invoke-PostSlotCommitGatedAction -AvailableGB 25 -JobCapGB 19 -ReserveGB 6 `
            -ConcurrentCount ($occupancy.OccupiedCount - 1) -FailClosed -Slot $slot `
            -Action { $script:cargoInvocations++ }
        Assert-False $result.Ok 'combined occupied-slot caps exceed the available commit headroom'
        Assert-Equal 1 $script:releasedSlots 'a refused post-slot gate must release the acquired slot'
        Assert-Equal 0 $script:cargoInvocations 'Cargo must not launch after post-slot refusal'
        Assert-True $result.SlotReleased 'the result must expose the release effect'
        Assert-False $result.ActionInvoked 'the result must expose that Cargo was not invoked'
    } finally {
        Set-Item Function:\script:Exit-ResourceSlot -Value $originalRelease
        Remove-Variable -Scope Script -Name releasedSlots, cargoInvocations -ErrorAction SilentlyContinue
    }
}

Test-Case 'width-one launch sees width-two peer on slot one and refuses before Cargo' {
    $oldBuildPrefix = $script:BuildSlotMutexPrefix
    $oldRunPrefix = $script:RunSlotMutexPrefix
    $oldJob = $env:PANGLOSS_JOB_MEM_GB
    $prefix = "Global\PanGlossCommitWidthTest$PID-$([guid]::NewGuid().ToString('N'))-"
    $script:BuildSlotMutexPrefix = $prefix
    $script:RunSlotMutexPrefix = "$prefix-run-"
    $childPath = Join-Path ([System.IO.Path]::GetTempPath()) "pg-commit-width-holder-$PID.ps1"
    $childOutput = Join-Path ([System.IO.Path]::GetTempPath()) "pg-commit-width-holder-$PID.out"
    $commonPath = (Resolve-Path "$PSScriptRoot\..\_common.ps1").Path
    Set-Content -LiteralPath $childPath -Encoding UTF8 -Value @'
param([string]$Common, [string]$Prefix, [int]$Width)
. $Common
$script:BuildSlotMutexPrefix = $Prefix
$slot = Enter-ResourceSlot -Pool build -MaxConcurrent $Width -TimeoutSeconds 10
if ($null -eq $slot) { 'DENIED'; exit 1 }
"HOLDING:$($slot.Index)"
Start-Sleep -Seconds 30
'@
    $widthOneSlot = $null
    $peerProcess = $null
    $script:cargoInvocations = 0
    try {
        Remove-Item Env:PANGLOSS_JOB_MEM_GB -ErrorAction SilentlyContinue
        $totalGB = 64.0
        $jobCap = Get-JobMemoryCapGB -MaxConcurrent 1 -TotalGB $totalGB
        $reserveGB = Get-InteractiveReserveGB -TotalGB $totalGB
        $widthOneSlot = Enter-ResourceSlot -Pool build -MaxConcurrent 1 -TimeoutSeconds 5
        Assert-True ($null -ne $widthOneSlot) 'the width-one launch must own slot zero first'
        $peerProcess = Start-Process -FilePath 'pwsh' -PassThru -NoNewWindow `
            -RedirectStandardOutput $childOutput `
            -ArgumentList @('-NoProfile', '-File', $childPath, $commonPath, $prefix, '2')
        $holdingSlotOne = $false
        for ($attempt = 0; $attempt -lt 100; $attempt++) {
            Start-Sleep -Milliseconds 100
            if ((Test-Path -LiteralPath $childOutput) -and (Get-Content -LiteralPath $childOutput -Raw) -match 'HOLDING:1') {
                $holdingSlotOne = $true
                break
            }
            if ($peerProcess.HasExited) { break }
        }
        Assert-True $holdingSlotOne 'the independent width-two process must be holding slot one'

        $occupancy = Get-OccupiedResourceSlotCount -Slot $widthOneSlot
        Assert-True $occupancy.Ok $occupancy.Detail
        Assert-Equal 2 $occupancy.OccupiedCount 'width-one census must include the wider peer slot'
        $peerCaps = Get-ResourcePeerCommitCaps -Pool build -ConcurrentCount $occupancy.ConcurrentCount `
            -CurrentJobCapGB $jobCap -TotalGB $totalGB
        Assert-True $peerCaps.Ok $peerCaps.Detail
        $requiredGB = $jobCap + $reserveGB + [double]($peerCaps.CapsGB | Measure-Object -Sum | Select-Object -ExpandProperty Sum)
        $result = Invoke-PostSlotCommitGatedAction -AvailableGB ($requiredGB - 0.01) -JobCapGB $jobCap -ReserveGB $reserveGB `
            -ConcurrentCapsGB $peerCaps.CapsGB -FailClosed -Slot $widthOneSlot `
            -Action { $script:cargoInvocations++ }
        $widthOneSlot = $null # the refusal helper released and disposed this token
        Assert-False $result.Ok 'the width-one current cap, width-one maximum peer cap, and reserve must all fit'
        Assert-Equal $requiredGB $result.RequiredGB 'the effect must reserve the derived maximum cap for the occupied peer'
        Assert-Equal 0 $script:cargoInvocations 'the low-commit decision must not invoke Cargo'
        Assert-True $result.SlotReleased 'refusal must release the width-one slot'
        $reacquired = Enter-ResourceSlot -Pool build -MaxConcurrent 1 -TimeoutSeconds 2
        Assert-True ($null -ne $reacquired) 'slot zero must be reusable while the peer still holds slot one'
        Exit-ResourceSlot -Slot $reacquired
    } finally {
        if ($peerProcess -and -not $peerProcess.HasExited) { Stop-Process -Id $peerProcess.Id -Force -ErrorAction SilentlyContinue }
        if ($widthOneSlot) { Exit-ResourceSlot -Slot $widthOneSlot }
        $script:BuildSlotMutexPrefix = $oldBuildPrefix
        $script:RunSlotMutexPrefix = $oldRunPrefix
        $env:PANGLOSS_JOB_MEM_GB = $oldJob
        Remove-Variable -Scope Script -Name cargoInvocations -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $childPath, $childOutput -Force -ErrorAction SilentlyContinue
    }
}

Test-Case 'width-two launch in slot one reserves a larger width-one peer cap' {
    $oldBuildPrefix = $script:BuildSlotMutexPrefix
    $oldRunPrefix = $script:RunSlotMutexPrefix
    $oldJob = $env:PANGLOSS_JOB_MEM_GB
    $prefix = "Global\PanGlossCommitInverseWidthTest$PID-$([guid]::NewGuid().ToString('N'))-"
    $script:BuildSlotMutexPrefix = $prefix
    $script:RunSlotMutexPrefix = "$prefix-run-"
    $childPath = Join-Path ([System.IO.Path]::GetTempPath()) "pg-commit-width-inverse-holder-$PID.ps1"
    $childOutput = Join-Path ([System.IO.Path]::GetTempPath()) "pg-commit-width-inverse-holder-$PID.out"
    $commonPath = (Resolve-Path "$PSScriptRoot\..\_common.ps1").Path
    Set-Content -LiteralPath $childPath -Encoding UTF8 -Value @'
param([string]$Common, [string]$Prefix, [int]$Width)
. $Common
$script:BuildSlotMutexPrefix = $Prefix
$slot = Enter-ResourceSlot -Pool build -MaxConcurrent $Width -TimeoutSeconds 10
if ($null -eq $slot) { 'DENIED'; exit 1 }
"HOLDING:$($slot.Index)"
Start-Sleep -Seconds 30
'@
    $currentSlot = $null
    $peerProcess = $null
    $script:cargoInvocations = 0
    try {
        Remove-Item Env:PANGLOSS_JOB_MEM_GB -ErrorAction SilentlyContinue
        $totalGB = 64.0
        $widthOneCap = Get-JobMemoryCapGB -MaxConcurrent 1 -TotalGB $totalGB
        $currentCap = Get-JobMemoryCapGB -MaxConcurrent 2 -TotalGB $totalGB
        $reserveGB = Get-InteractiveReserveGB -TotalGB $totalGB
        Assert-True ($widthOneCap -gt $currentCap) "width-one peer must have the larger policy-derived cap ($widthOneCap > $currentCap)"

        $peerProcess = Start-Process -FilePath 'pwsh' -PassThru -NoNewWindow `
            -RedirectStandardOutput $childOutput `
            -ArgumentList @('-NoProfile', '-File', $childPath, $commonPath, $prefix, '1')
        $holdingSlotZero = $false
        for ($attempt = 0; $attempt -lt 100; $attempt++) {
            Start-Sleep -Milliseconds 100
            if ((Test-Path -LiteralPath $childOutput) -and (Get-Content -LiteralPath $childOutput -Raw) -match 'HOLDING:0') {
                $holdingSlotZero = $true
                break
            }
            if ($peerProcess.HasExited) { break }
        }
        Assert-True $holdingSlotZero 'the independent width-one process must own slot zero first'

        $currentSlot = Enter-ResourceSlot -Pool build -MaxConcurrent 2 -TimeoutSeconds 5
        Assert-True ($null -ne $currentSlot) 'the wider invocation must acquire slot one while slot zero is occupied'
        Assert-Equal 1 $currentSlot.Index 'the width-two current launch must be in slot one'
        $occupancy = Get-OccupiedResourceSlotCount -Slot $currentSlot
        Assert-True $occupancy.Ok $occupancy.Detail
        Assert-Equal 2 $occupancy.OccupiedCount 'the wider launch must census its width-one peer'
        $peerCaps = Get-ResourcePeerCommitCaps -Pool build -ConcurrentCount $occupancy.ConcurrentCount `
            -CurrentJobCapGB $currentCap -TotalGB $totalGB
        Assert-True $peerCaps.Ok $peerCaps.Detail
        Assert-Equal $widthOneCap $peerCaps.PeerCapGB 'the narrower peer must be charged at its larger cap'
        $requiredGB = $currentCap + $reserveGB + [double]($peerCaps.CapsGB | Measure-Object -Sum | Select-Object -ExpandProperty Sum)
        $result = Invoke-PostSlotCommitGatedAction -AvailableGB ($requiredGB - 0.01) `
            -JobCapGB $currentCap -ReserveGB $reserveGB -ConcurrentCapsGB $peerCaps.CapsGB `
            -FailClosed -Slot $currentSlot -Action { $script:cargoInvocations++ }
        $currentSlot = $null
        Assert-False $result.Ok 'the low width-two cap plus wider width-one peer cap and reserve must refuse'
        Assert-Equal $requiredGB $result.RequiredGB 'post-slot requirement must use the heterogeneous derived peer cap'
        Assert-Equal 0 $script:cargoInvocations 'refusal must happen before Cargo'
        Assert-True $result.SlotReleased 'refusal must release slot one'
        $reacquired = Enter-ResourceSlot -Pool build -MaxConcurrent 2 -TimeoutSeconds 2
        Assert-True ($null -ne $reacquired) 'slot one must be reusable while the width-one peer still holds slot zero'
        Assert-Equal 1 $reacquired.Index 'the reusable slot must be the unoccupied second slot'
        Exit-ResourceSlot -Slot $reacquired
    } finally {
        if ($peerProcess -and -not $peerProcess.HasExited) { Stop-Process -Id $peerProcess.Id -Force -ErrorAction SilentlyContinue }
        if ($currentSlot) { Exit-ResourceSlot -Slot $currentSlot }
        $script:BuildSlotMutexPrefix = $oldBuildPrefix
        $script:RunSlotMutexPrefix = $oldRunPrefix
        $env:PANGLOSS_JOB_MEM_GB = $oldJob
        Remove-Variable -Scope Script -Name cargoInvocations -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $childPath, $childOutput -Force -ErrorAction SilentlyContinue
    }
}

Test-Case 'managed launch cap selector honors the exact run, heavy, and override policy' {
    $oldJob = $env:PANGLOSS_JOB_MEM_GB
    $oldRun = $script:RunSlotMemoryGB
    try {
        $env:PANGLOSS_JOB_MEM_GB = '23'
        $script:RunSlotMemoryGB = 3
        Assert-Equal 23 (Get-PgLaunchMemoryCapGB -Mode check -MaxConcurrent 2 -ResolveJobCap).JobCapGB
        Assert-Equal 3 (Get-PgLaunchMemoryCapGB -Mode run -MaxConcurrent 2 -ResolveJobCap).JobCapGB
        Assert-Equal 23 (Get-PgLaunchMemoryCapGB -Mode run -Heavy -MaxConcurrent 2 -ResolveJobCap).JobCapGB
        Assert-Equal 40 (Get-PgLaunchMemoryCapGB -Mode run -RunMemoryGB 40 -Heavy -MaxConcurrent 2 -ResolveJobCap).JobCapGB
        Assert-False (Get-PgLaunchMemoryCapGB -Mode doctor -MaxConcurrent 2 -ResolveJobCap).Launches
        Assert-False (Get-PgLaunchMemoryCapGB -Mode gc -MaxConcurrent 2 -ResolveJobCap).Launches

        $env:PANGLOSS_JOB_MEM_GB = 'not-a-number'
        $linux = Get-PgLaunchMemoryCapGB -Mode check -MaxConcurrent 2
        Assert-True $linux.Launches 'cap-resolution-disabled callers still need the launch classification'
        Assert-Equal $null $linux.JobCapGB 'Linux must not read or cast Windows-only cap configuration'
    } finally {
        $env:PANGLOSS_JOB_MEM_GB = $oldJob
        $script:RunSlotMemoryGB = $oldRun
    }
}

Test-Case 'Cargo enforces the explicit cap admitted by pg instead of deriving it again' {
    $oldJob = $env:PANGLOSS_JOB_MEM_GB
    $originalInvoker = (Get-Item Function:\script:Invoke-ProcessInJobObject).ScriptBlock
    $script:CapturedJobMemoryGB = $null
    $mockInvoker = {
        param(
            [string]$Exe, [string[]]$CmdArgs, [string]$WorkingDirectory, [string]$CaptureStdoutPath,
            [string]$Priority, [Nullable[int]]$JobMemoryGB, [Nullable[int]]$CpuRatePercent, [string]$Subject
        )
        $script:CapturedJobMemoryGB = $JobMemoryGB
        return 37
    }
    try {
        Set-Item Function:\script:Invoke-ProcessInJobObject -Value $mockInvoker
        $env:PANGLOSS_JOB_MEM_GB = '13'
        $explicit = Invoke-CargoWithReaper -Exe cargo -CmdArgs @('check') -WorkingDirectory '.' -JobMemoryGB 7
        Assert-Equal 37 $explicit
        Assert-Equal 7 $script:CapturedJobMemoryGB 'the admitted cap must reach the process launcher unchanged'
        $derived = Invoke-CargoWithReaper -Exe cargo -CmdArgs @('check') -WorkingDirectory '.' -JobMaxConcurrent 2
        Assert-Equal 37 $derived
        Assert-Equal 13 $script:CapturedJobMemoryGB 'legacy callers without an explicit cap retain Get-JobMemoryCapGB policy'
    } finally {
        Set-Item Function:\script:Invoke-ProcessInJobObject -Value $originalInvoker
        $env:PANGLOSS_JOB_MEM_GB = $oldJob
        Remove-Variable -Scope Script -Name CapturedJobMemoryGB -ErrorAction SilentlyContinue
    }
}

Test-Case 'pg places the commit gate before rustfmt and slot admission, then repeats it before Cargo' {
    $toolRoot = Split-Path $PSScriptRoot -Parent
    $pgText = Get-Content -LiteralPath (Join-Path $toolRoot 'pg.ps1') -Raw
    $firstGate = $pgText.IndexOf('$commitGate = Invoke-CommitGatedAction', [StringComparison]::Ordinal)
    $widthValidationAt = $pgText.IndexOf('if ($MaxConcurrent -lt 1 -or $MaxConcurrent -gt $script:MaxResourceSlotWidth)', [StringComparison]::Ordinal)
    $fmtFallbackAt = $pgText.LastIndexOf('Invoke-RustFmt -RustRoot $rustRoot', [StringComparison]::Ordinal)
    $hygieneAt = $pgText.IndexOf('Invoke-CommentHygieneReport -ToolRoot $PSScriptRoot', [StringComparison]::Ordinal)
    $slotAt = $pgText.IndexOf('Enter-ResourceSlot -Pool $slotPool', [StringComparison]::Ordinal)
    $occupancyAt = $pgText.IndexOf('Get-OccupiedResourceSlotCount -Slot $sem', [StringComparison]::Ordinal)
    $secondGate = $pgText.IndexOf('Invoke-PostSlotCommitGatedAction -AvailableGB', [StringComparison]::Ordinal)
    $cargoAt = $pgText.IndexOf('$code = Invoke-CargoWithReaper @invokeArgs', [StringComparison]::Ordinal)
    Assert-True ($widthValidationAt -ge 0 -and $widthValidationAt -lt $firstGate) 'invalid slot widths must refuse before the commit gate can format or launch anything'
    Assert-True ($firstGate -ge 0 -and $firstGate -lt $fmtFallbackAt) 'the gate call must precede the ungated platform fallback'
    Assert-True ($hygieneAt -gt $firstGate) 'the initial commit gate must precede the native hygiene process or its nested bootstrap build'
    Assert-True ($firstGate -lt $slotAt) 'initial commit gate must precede slot admission'
    Assert-True ($occupancyAt -gt $slotAt -and $occupancyAt -lt $secondGate) 'kernel slot occupancy must be measured after owning a slot and before admission'
    Assert-True ($secondGate -gt $occupancyAt -and $secondGate -lt $cargoAt) 'post-slot commit recheck must precede Cargo'
    Assert-True $pgText.Contains('-ConcurrentCapsGB $peerCommitCaps.CapsGB') 'the post-slot gate must include conservative per-peer caps'
    Assert-True $pgText.Contains('$commitSnapshot.FreeGBExact') 'the pre-format decision must use exact commit headroom'
    Assert-True $pgText.Contains('$commitSnapshotNow.FreeGBExact') 'the post-slot decision must use exact commit headroom'
    Assert-True $pgText.Contains('-Action $formatAction') 'Windows rustfmt must be invoked through the admitted callback'
    $conditionalCapAssignments = [regex]::Matches($pgText, 'if \(\$IsWindows\) \{\s*\$invokeArgs\[''JobMemoryGB''\] = \$launchCapSelection\.JobCapGB\s*\$invokeArgs\[''Threads''\] = \[Math\]::Max\(\$Jobs, \$TestThreads\)\s*\}').Count
    Assert-Equal 3 $conditionalCapAssignments 'each Windows Cargo path must receive its exact preflight cap and thread limit'
    Assert-True $pgText.Contains("if (`$IsWindows) { `$invokeArgs['JobMemoryGB'] = `$BuildJobMemoryGB }") `
        'backend regeneration must pass its selected cap only to the Windows wrapper'
    Assert-False $pgText.Contains('JobMemoryGB = $launchCapSelection.JobCapGB') `
        'the platform-specific cap must not be an unconditional Cargo splat entry'
    Assert-Equal 3 ([regex]::Matches($pgText, '\[''Threads''\] = \[Math\]::Max\(\$Jobs, \$TestThreads\)').Count) `
        'thread arguments must exist at each of the three Windows-only Cargo callsites'
}

Write-TestSummary
