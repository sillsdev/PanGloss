<#
  .DESCRIPTION
  The managed Cargo path has no procgov committed-memory cap or commit-headroom gate. The separate
  light-run cap and explicit run override remain. Tests use the actual Cargo wrapper with a mocked
  process seam and inspect pg.ps1's launch path; nothing starts Cargo or reads machine memory.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

Test-Case 'managed Cargo launches reach the process seam without a JobMemoryGB value' {
    $originalInvoker = (Get-Item Function:\script:Invoke-ProcessInJobObject).ScriptBlock
    $script:CapturedCargoJobMemoryGB = 'not-captured'
    $script:CapturedCargoCpuRate = $null
    $mockInvoker = {
        param(
            [string]$Exe, [string[]]$CmdArgs, [string]$WorkingDirectory, [string]$CaptureStdoutPath,
            [string]$Priority, [Nullable[int]]$JobMemoryGB, [Nullable[int]]$CpuRatePercent, [string]$Subject
        )
        $script:CapturedCargoJobMemoryGB = $JobMemoryGB
        $script:CapturedCargoCpuRate = $CpuRatePercent
        return 37
    }
    try {
        Set-Item Function:\script:Invoke-ProcessInJobObject -Value $mockInvoker
        $result = Invoke-CargoWithReaper -Exe cargo -CmdArgs @('check') -WorkingDirectory '.' -Threads 8
        Assert-Equal 37 $result 'the Cargo wrapper must preserve the process result'
        Assert-Equal $null $script:CapturedCargoJobMemoryGB 'ordinary Cargo must not receive a process-memory cap'
        Assert-Equal (Get-JobCpuRatePercent -Threads 8) $script:CapturedCargoCpuRate `
            'removing the memory cap must preserve the per-slot CPU limit'
        Assert-False ((Get-Command Invoke-CargoWithReaper).Parameters.ContainsKey('JobMemoryGB')) `
            'Cargo callers must not be able to reintroduce a job-memory override'
    } finally {
        Set-Item Function:\script:Invoke-ProcessInJobObject -Value $originalInvoker
        Remove-Variable -Scope Script -Name CapturedCargoJobMemoryGB -ErrorAction SilentlyContinue
        Remove-Variable -Scope Script -Name CapturedCargoCpuRate -ErrorAction SilentlyContinue
    }
}

Test-Case 'Cargo procgov arguments retain CPU, priority, and tree handling without --maxjobmem' {
    $a = Get-ProcGovArgs -CpuRatePercent 35 -Priority 'BelowNormal' -Exe 'cargo' -CmdArgs @('build', '--release')
    Assert-False (@($a | Where-Object { $_ -like '--maxjobmem*' }).Count -gt 0) `
        'managed Cargo launches must not request --maxjobmem'
    Assert-Contains $a '--cpurate=35'
    Assert-Contains $a '--priority=BelowNormal'
    Assert-Contains $a '-r'
    Assert-Contains $a '--terminate-job-on-exit'
    Assert-Contains $a 'cargo'
}

Test-Case 'run memory policy keeps the flat light cap and explicit override, with Heavy uncapped by default' {
    $oldRun = $script:RunSlotMemoryGB
    try {
        $script:RunSlotMemoryGB = 2
        Assert-Equal 2 (Get-RunJobMemoryCapGB) 'a light run keeps the 2GB default'
        Assert-Equal 2 (Get-RunJobMemoryCapGB -RunMemoryGB 0) 'zero preserves the light-run default'
        Assert-Equal $null (Get-RunJobMemoryCapGB -Heavy) 'Heavy must not inherit the removed build cap'
        Assert-Equal 40 (Get-RunJobMemoryCapGB -RunMemoryGB 40) 'an explicit run cap must be honored'
        Assert-Equal 40 (Get-RunJobMemoryCapGB -RunMemoryGB 40 -Heavy) 'an explicit cap must also work for Heavy'
    } finally {
        $script:RunSlotMemoryGB = $oldRun
    }
}

Test-Case 'pg has no commit admission or peer-cap census around managed Cargo launches' {
    $toolRoot = Split-Path $PSScriptRoot -Parent
    $pgText = Get-Content -LiteralPath (Join-Path $toolRoot 'pg.ps1') -Raw
    $commonText = Get-Content -LiteralPath (Join-Path $toolRoot '_common.ps1') -Raw
    foreach ($removedName in @(
        'Test-CommitReserve', 'Invoke-CommitGatedAction', 'Invoke-PostSlotCommitGatedAction',
        'Get-ResourcePeerCommitCaps', 'Get-OccupiedResourceSlotCount', 'Get-JobMemoryCapGB'
    )) {
        Assert-False ($null -ne (Get-Command $removedName -CommandType Function -ErrorAction SilentlyContinue)) `
            "$removedName must not remain as an active build-cap control"
        Assert-False $pgText.Contains($removedName) "$removedName must not be called by pg.ps1"
        Assert-False $commonText.Contains("function $removedName") "$removedName must not remain in _common.ps1"
    }

    $cargoStart = $pgText.IndexOf('} elseif ($HygieneBootstrap) {', [StringComparison]::Ordinal)
    $cargoEnd = $pgText.IndexOf('} finally {', $cargoStart, [StringComparison]::Ordinal)
    Assert-True ($cargoStart -ge 0 -and $cargoEnd -gt $cargoStart) 'managed Cargo branch must be locatable'
    $cargoBranch = $pgText.Substring($cargoStart, $cargoEnd - $cargoStart)
    Assert-False ($cargoBranch -match 'JobMemoryGB|--maxjobmem|commit.headroom|Get-CommitChargeGB') `
        'no build cap or commit-headroom gate may refuse or constrain a Cargo launch'
    Assert-False ($pgText -match 'Invoke-CommitGatedAction|Invoke-PostSlotCommitGatedAction|Get-ResourcePeerCommitCaps|Get-OccupiedResourceSlotCount') `
        'pg.ps1 must not retain cap-based pre-slot or post-slot admission'
    Assert-Equal 3 ([regex]::Matches($pgText,'\[''Threads''\] = \[Math\]::Max\(\$Jobs, \$TestThreads\)').Count) `
        'each managed Cargo path must retain its CPU-width input'
}

Write-TestSummary
