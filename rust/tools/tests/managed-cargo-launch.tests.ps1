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
        $code = Invoke-ManagedProcess -Exe 'cargo' -CmdArgs @('build', '--release') -WorkingDirectory 'C:\fixture'
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

Write-TestSummary
