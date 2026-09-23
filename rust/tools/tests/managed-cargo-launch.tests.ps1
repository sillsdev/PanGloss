# Managed Cargo launch tests mock process boundaries; no test starts Cargo.
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

Test-Case 'managed Cargo launches the payload directly, applies priority, and preserves its exit code' {
    $originalStartProcess = Get-Item Function:\script:Start-Process -ErrorAction SilentlyContinue
    $script:CapturedStart = $null
    $script:FakeWaitArguments = @()
    $mockStartProcess = {
        param(
            [string]$FilePath, [string[]]$ArgumentList, [string]$WorkingDirectory,
            [switch]$NoNewWindow, [switch]$PassThru, [string]$RedirectStandardOutput
        )
        $script:CapturedStart = [PSCustomObject]@{
            FilePath = $FilePath; ArgumentList = @($ArgumentList); WorkingDirectory = $WorkingDirectory
            NoNewWindow = $NoNewWindow.IsPresent; PassThru = $PassThru.IsPresent
            RedirectStandardOutput = $RedirectStandardOutput
        }
        $fake = [PSCustomObject]@{ Id = 77; HasExited = $false; ExitCode = 37; PriorityClass = $null }
        $script:CapturedProcess = $fake
        $fake | Add-Member ScriptMethod WaitForExit {
            param([object]$Milliseconds)
            if ($PSBoundParameters.ContainsKey('Milliseconds')) { $script:FakeWaitArguments += $Milliseconds }
            $this.HasExited = $true
            return $null
        }
        return $fake
    }
    try {
        Set-Item Function:\script:Start-Process -Value $mockStartProcess
        $code = Invoke-CargoWithReaper -Exe 'cargo' -CmdArgs @('build', '--release') -WorkingDirectory 'C:\fixture'
        Assert-Equal 37 $code 'the direct managed launch must return the payload exit code'
        Assert-Equal 'cargo' $script:CapturedStart.FilePath 'Cargo must be the launched executable, not a wrapper'
        Assert-Equal 'build,--release' ($script:CapturedStart.ArgumentList -join ',')
        Assert-Equal 'C:\fixture' $script:CapturedStart.WorkingDirectory
        Assert-True $script:CapturedStart.NoNewWindow
        Assert-True $script:CapturedStart.PassThru
        Assert-Equal 'BelowNormal' $script:CapturedProcess.PriorityClass 'the payload must receive the requested priority'
        Assert-Equal 0 $script:FakeWaitArguments.Count 'the direct path must wait with WaitForExit()'
    } finally {
        if ($originalStartProcess) { Set-Item Function:\script:Start-Process -Value $originalStartProcess.ScriptBlock }
        else { Remove-Item Function:\script:Start-Process -ErrorAction SilentlyContinue }
        Remove-Variable -Scope Script -Name CapturedStart -ErrorAction SilentlyContinue
        Remove-Variable -Scope Script -Name CapturedProcess -ErrorAction SilentlyContinue
        Remove-Variable -Scope Script -Name FakeWaitArguments -ErrorAction SilentlyContinue
    }
}

Test-Case 'managed Cargo forwards the direct launch contract and preserves the process result' {
    $originalInvoker = (Get-Item Function:\script:Invoke-ManagedProcess).ScriptBlock
    $script:CapturedCargoLaunch = $null
    $mockInvoker = {
        param(
            [string]$Exe, [string[]]$CmdArgs, [string]$WorkingDirectory, [string]$CaptureStdoutPath,
            [string]$Priority
        )
        $script:CapturedCargoLaunch = [PSCustomObject]@{
            Exe = $Exe; CmdArgs = @($CmdArgs); WorkingDirectory = $WorkingDirectory
            CaptureStdoutPath = $CaptureStdoutPath; Priority = $Priority
        }
        return 37
    }
    try {
        Set-Item Function:\script:Invoke-ManagedProcess -Value $mockInvoker
        $result = Invoke-CargoWithReaper -Exe cargo -CmdArgs @('check') -WorkingDirectory '.'
        Assert-Equal 37 $result 'the Cargo wrapper must preserve the process result'
        Assert-Equal 'cargo' $script:CapturedCargoLaunch.Exe
        Assert-Equal 'check' ($script:CapturedCargoLaunch.CmdArgs -join ',')
        Assert-Equal 'BelowNormal' $script:CapturedCargoLaunch.Priority
        Assert-False ((Get-Command Invoke-CargoWithReaper).Parameters.ContainsKey('Threads')) `
            'Cargo callers must not pass a removed CPU-width override'
    } finally {
        Set-Item Function:\script:Invoke-ManagedProcess -Value $originalInvoker
        Remove-Variable -Scope Script -Name CapturedCargoLaunch -ErrorAction SilentlyContinue
    }
}

Test-Case 'pg has no removed process-cap controls around managed Cargo launches' {
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
    Assert-False ($cargoBranch -match 'commit.headroom|Get-CommitChargeGB') `
        'no removed process cap or commit-headroom gate may constrain a Cargo launch'
    Assert-False ($pgText -match 'Invoke-CommitGatedAction|Invoke-PostSlotCommitGatedAction|Get-ResourcePeerCommitCaps|Get-OccupiedResourceSlotCount') `
        'pg.ps1 must not retain cap-based pre-slot or post-slot admission'
    Assert-False ($pgText -match "\[''Threads''\]") 'managed Cargo paths must not pass removed CPU-width plumbing'
}

Write-TestSummary
