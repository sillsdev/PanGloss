<#
  .SYNOPSIS
  Sets up, checks and proves the Windows hardware-counter (PMC) path that instruction-compare.ps1 measures with.

  .DESCRIPTION
  instruction-compare.ps1 counts retired instructions from a WPR trace that records the CPU's fixed counters on
  every context switch. wpr.exe needs elevation, so two scheduled tasks under \PanGloss\ hold it and the
  comparison itself runs unelevated. This script is the one place that installs them.

  With no switch: reports what is in place and exits 0 when the machine is ready, 1 when it is not. No elevation.
  -Install                  copies pg.wprp to C:\ProgramData\PanGlossPmc and registers the two tasks for -User.
  -DisableMemoryIntegrity   turns memory integrity (HVCI) off; it takes effect after a reboot. On 2026-11-09 a
                            trace taken with it running carried no counter values. That cause is not confirmed;
                            -Verify after the reboot is what settles it.
  -Verify                   takes a real trace of a short busy process and refuses unless it carries counter
                            values for that process. No elevation. Run it after any reboot that changed the setup.
  -Install and -DisableMemoryIntegrity need an administrator. From an unelevated shell the script asks for
  elevation once (one UAC prompt), runs the elevated steps, and passes -User through so the tasks belong to you.

  Exit codes: 0 ready (or the step succeeded); 1 not ready; 2 a step failed.

  .EXAMPLE
  pwsh -NoProfile -File rust/tools/pmc-setup.ps1 -Install -DisableMemoryIntegrity   # then reboot
  pwsh -NoProfile -File rust/tools/pmc-setup.ps1 -Verify
#>
[CmdletBinding()]
param(
    [switch]$Install,
    [switch]$DisableMemoryIntegrity,
    [switch]$Verify,
    [string]$User = "$env:USERDOMAIN\$env:USERNAME",
    [int]$Cpu = 4
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\_pmc-setup.ps1"
. "$PSScriptRoot\_instruction-compare.ps1"

if (-not $IsWindows) { Write-Host 'pmc-setup: the PMC path is Windows only; on Linux instruction-compare.ps1 uses callgrind.'; exit 2 }

$hvciKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity'
$policyKey = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DeviceGuard'
$profileSource = Join-Path $PSScriptRoot 'instruction-compare\pg.wprp'
$profileTarget = Join-Path $script:PmcDir 'pg.wprp'

function Test-Elevated {
    ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Read-RegistryValue([string]$Key, [string]$Name) {
    $item = Get-ItemProperty -Path $Key -Name $Name -ErrorAction SilentlyContinue
    if ($item) { $item.$Name } else { $null }
}

function Get-MemoryIntegrity {
    $dg = Get-CimInstance -ClassName Win32_DeviceGuard -Namespace root\Microsoft\Windows\DeviceGuard -ErrorAction SilentlyContinue
    Get-MemoryIntegrityState -Running @($(if ($dg) { $dg.SecurityServicesRunning })) `
        -Configured (Read-RegistryValue $hvciKey 'Enabled') -PolicyEnabled (Read-RegistryValue $policyKey 'HypervisorEnforcedCodeIntegrity')
}

function Get-RegisteredPmcTasks {
    @(Get-ScheduledTask -TaskPath $script:PmcTaskPath -ErrorAction SilentlyContinue | ForEach-Object {
            [PSCustomObject]@{ Name = $_.TaskName; Execute = $_.Actions[0].Execute; Arguments = $_.Actions[0].Arguments; RunLevel = $_.Principal.RunLevel }
        })
}

function Invoke-ElevatedSteps {
    $argList = @('-NoProfile', '-File', $PSCommandPath, '-User', $User)
    if ($Install) { $argList += '-Install' }
    if ($DisableMemoryIntegrity) { $argList += '-DisableMemoryIntegrity' }
    Write-Host "pmc-setup: asking for elevation to run: $($argList[3..($argList.Count - 1)] -join ' ')"
    $proc = Start-Process -FilePath (Get-Process -Id $PID).Path -ArgumentList $argList -Verb RunAs -Wait -PassThru
    if ($proc.ExitCode -ne 0) { Write-Host "pmc-setup: the elevated run exited $($proc.ExitCode)"; exit 2 }
}

function Install-PmcPath {
    New-Item -ItemType Directory -Force -Path $script:PmcDir | Out-Null
    $acl = New-Object Security.AccessControl.DirectorySecurity
    $acl.SetAccessRuleProtection($true, $false)
    $inherit = [Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit'
    $none = [Security.AccessControl.PropagationFlags]::None
    foreach ($rule in @(@('NT AUTHORITY\SYSTEM', 'FullControl'), @('BUILTIN\Administrators', 'FullControl'), @($User, 'ReadAndExecute'))) {
        $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($rule[0], $rule[1], $inherit, $none, 'Allow')))
    }
    Set-Acl -Path $script:PmcDir -AclObject $acl
    Copy-Item -LiteralPath $profileSource -Destination $profileTarget -Force
    $principal = New-ScheduledTaskPrincipal -UserId $User -LogonType Interactive -RunLevel Highest
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 30) -MultipleInstances IgnoreNew -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    foreach ($spec in Get-PmcTaskSpecs) {
        $action = New-ScheduledTaskAction -Execute $spec.Execute -Argument $spec.Arguments -WorkingDirectory $spec.WorkingDirectory
        Register-ScheduledTask -TaskPath $script:PmcTaskPath -TaskName $spec.Name -Action $action -Principal $principal -Settings $settings -Force | Out-Null
        Write-Host "pmc-setup: registered $($script:PmcTaskPath)$($spec.Name) for $User"
    }
    Write-Host "pmc-setup: profile copied to $profileTarget"
}

function Disable-MemoryIntegrity {
    $state = Get-MemoryIntegrity
    if ($state.PolicyManaged) { throw "memory integrity is set by policy ($policyKey); turning off the setting would not hold. Change the policy instead." }
    if (-not (Test-Path $hvciKey)) { New-Item -Path $hvciKey -Force | Out-Null }
    Set-ItemProperty -Path $hvciKey -Name 'Enabled' -Value 0 -Type DWord
    $after = Get-MemoryIntegrity
    if ($after.OnAfterReboot) { throw 'memory integrity still reads as on after the next reboot; the change did not take' }
    Write-Host "pmc-setup: memory integrity is $($after.Summary)"
}

function Invoke-Verify {
    $python = (Get-Command python -ErrorAction SilentlyContinue).Source
    if (-not $python) { throw 'python is not on PATH; it reads the trace' }
    $problems = Get-PmcTaskProblems -Found (Get-RegisteredPmcTasks)
    if ($problems.Count -gt 0) { throw "the PMC tasks are not ready: $($problems -join '; ')" }
    $t0 = Get-Date
    $busy = $null
    Invoke-PmcTask 'PanGloss PMC Start' 60
    try {
        $busy = Start-Process -FilePath (Get-Process -Id $PID).Path -ArgumentList @('-NoProfile', '-Command', '$x = 0; for ($i = 0; $i -lt 3000000; $i++) { $x += $i }') -PassThru -WindowStyle Hidden
        $busy.ProcessorAffinity = [IntPtr]([int64]1 -shl $Cpu)
        $busy.WaitForExit()
    } finally { Invoke-PmcTask 'PanGloss PMC Stop' 900 }
    $trace = Get-Item (Join-Path $script:PmcDir 'current.etl')
    if ($trace.LastWriteTime -lt $t0) { throw "trace not refreshed: $($trace.FullName) is older than this run" }
    $copy = Join-Path ([IO.Path]::GetTempPath()) "pmc-verify-$PID.etl"
    Copy-Item $trace.FullName $copy -Force
    try {
        $json = & $python -I "$PSScriptRoot\instruction_compare_etl.py" $copy $busy.Id
        if ($LASTEXITCODE -ne 0) { throw "trace extraction failed with exit code $LASTEXITCODE" }
    } finally { Remove-Item $copy -Force -ErrorAction SilentlyContinue }
    $report = $json | ConvertFrom-Json
    Assert-TraceIntact -Report $report -Pids @($busy.Id)
    $count = [double]$report.instructions."$($busy.Id)"
    if ($count -le 0) { throw "the trace has counter values but none for the busy process (pid $($busy.Id)); nothing was counted for it" }
    Write-Host ("pmc-setup: VERIFIED - the trace counted {0:N0} instructions for the busy process across {1} context switches" -f $count, $report.switches)
}

try {
    if ($Install -or $DisableMemoryIntegrity) {
        if (-not (Test-Elevated)) { Invoke-ElevatedSteps }
        else {
            if ($Install) { Install-PmcPath }
            if ($DisableMemoryIntegrity) { Disable-MemoryIntegrity }
        }
    }
    if ($Verify) { Invoke-Verify; exit 0 }
} catch {
    Write-Host "pmc-setup: REFUSED - $($_.Exception.Message)"
    exit 2
}

$ready = $true
$problems = Get-PmcTaskProblems -Found (Get-RegisteredPmcTasks)
if (-not (Test-Path $profileTarget)) { $problems += "profile $profileTarget is missing" }
elseif ((Get-FileHash $profileTarget).Hash -ne (Get-FileHash $profileSource).Hash) { $problems += "profile $profileTarget differs from $profileSource" }
foreach ($p in $problems) { Write-Host "pmc-setup: $p"; $ready = $false }
if ($problems.Count -eq 0) { Write-Host 'pmc-setup: profile and tasks are in place' }
$mi = Get-MemoryIntegrity
Write-Host "pmc-setup: memory integrity is $($mi.Summary)"
if ($mi.Running) { $ready = $false }
if ($mi.RebootPending) { Write-Host 'pmc-setup: reboot for the memory integrity change to take effect' }
if ($ready) { Write-Host 'pmc-setup: ready; prove it with -Verify, which takes a real trace' } else { Write-Host 'pmc-setup: NOT READY' }
exit $(if ($ready) { 0 } else { 1 })
