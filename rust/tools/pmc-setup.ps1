<#
  .SYNOPSIS
  Sets up, checks and proves the Windows hardware-counter (PMC) path that instruction-compare.ps1 measures with.

  .DESCRIPTION
  instruction-compare.ps1 counts retired instructions from a WPR trace that records the CPU's counters on
  every context switch. wpr.exe needs elevation, so two scheduled tasks under \PanGloss\ hold it and the
  comparison itself runs unelevated. This script is the one place that installs them.

  With no switch: reports what is in place and exits 0 when the machine is ready, 1 when it is not. No elevation.
  -Install                  copies pg.wprp to C:\ProgramData\PanGlossPmc and registers the two tasks for -User.
  -DisableMemoryIntegrity   turns memory integrity (HVCI) off; it takes effect after a reboot. Not needed on the
                            first machine set up (docs/design/parse-cost.md); use it only if -Verify fails and
                            memory integrity is the remaining suspect.
  -Verify                   builds instruction-compare\known_loop.rs with rustc, traces it at two lengths, and
                            refuses unless the counted difference is within 1% of the known instruction
                            difference. Holds the Global\PanGlossInstructionCompare lock, so it never cuts an
                            instruction-compare trace. x86_64 only; elsewhere it refuses (unsupported architecture).
                            No elevation. Run it after any reboot that changed the setup.
  -Install and -DisableMemoryIntegrity need an administrator. From an unelevated shell the script asks for
  elevation once (one UAC prompt), runs the elevated steps, and passes -User through so the tasks belong to you.

  Exit codes: 0 ready (or the step succeeded); 1 not ready; 2 a step failed; 3 ready once the pending reboot
  applies a memory integrity change.

  .EXAMPLE
  pwsh -NoProfile -File rust/tools/pmc-setup.ps1 -Install
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
    if ($proc.ExitCode -notin 0, 3) { Write-Host "pmc-setup: the elevated run exited $($proc.ExitCode)"; exit 2 }
    Write-Host 'pmc-setup: the elevated steps succeeded'
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
    Assert-KnownLoopArchitecture -Architecture "$([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture)"
    if ($Cpu -lt 0 -or $Cpu -ge [Environment]::ProcessorCount) { throw "-Cpu $Cpu is not one of the $([Environment]::ProcessorCount) logical processors" }
    $python = (Get-Command python -ErrorAction SilentlyContinue).Source
    if (-not $python) { throw 'python is not on PATH; it reads the trace' }
    if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) { throw 'rustc is not on PATH; it builds the known-count loop' }
    $problems = Get-PmcTaskProblems -Found (Get-RegisteredPmcTasks)
    if ($problems.Count -gt 0) { throw "the PMC tasks are not ready: $($problems -join '; ')" }
    # The same machine-wide lock instruction-compare.ps1 holds: starting or stopping the PMC tasks mid-comparison would cut its trace.
    $lock = [System.Threading.Mutex]::new($false, 'Global\PanGlossInstructionCompare')
    $held = $false
    try { $held = $lock.WaitOne(0) } catch [System.Threading.AbandonedMutexException] { $held = $true }
    if (-not $held) { $lock.Dispose(); throw 'an instruction-compare run (or another -Verify) holds the machine-wide lock Global\PanGlossInstructionCompare; starting a trace now would stop its trace' }
    $work = Join-Path ([IO.Path]::GetTempPath()) "pmc-verify-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Force $work | Out-Null
    try {
        $exe = Join-Path $work 'known_loop.exe'
        $built = & rustc -O --edition 2021 "$PSScriptRoot\instruction-compare\known_loop.rs" -o $exe 2>&1
        if ($LASTEXITCODE -ne 0) { throw "could not build the known-count loop: $($built -join ' ')" }
        $lengths = $script:KnownLoopLengths
        $runs = @()
        $t0 = Get-Date
        Invoke-PmcTask 'PanGloss PMC Start' 60
        try {
            foreach ($n in $lengths) {
                $p = Start-Process -FilePath $exe -ArgumentList @("$n") -PassThru -WindowStyle Hidden
                $p.ProcessorAffinity = [IntPtr]([int64]1 -shl $Cpu)
                $p.WaitForExit()
                if ($p.ExitCode -ne 0) { throw "the known-count loop ($n iterations, pid $($p.Id)) exited $($p.ExitCode)" }
                $runs += [PSCustomObject]@{ Iterations = [int64]$n; Pid = $p.Id }
            }
        } finally { Invoke-PmcTask 'PanGloss PMC Stop' 900 }
        $pids = @($runs | ForEach-Object Pid)
        Assert-DistinctPids -Pids $pids
        $trace = Get-Item (Join-Path $script:PmcDir 'current.etl')
        if ($trace.LastWriteTime -lt $t0) { throw "trace not refreshed: $($trace.FullName) is older than this run" }
        $copy = Join-Path $work 'trace.etl'
        Copy-Item $trace.FullName $copy -Force
        $json = & $python -I "$PSScriptRoot\instruction_compare_etl.py" $copy @pids
        if ($LASTEXITCODE -ne 0) { throw "trace extraction failed with exit code $LASTEXITCODE" }
        $report = $json | ConvertFrom-Json
        Assert-TraceIntact -Report $report -Pids $pids
        $counted = @($runs | ForEach-Object { [double]$report.instructions."$($_.Pid)" })
        $verdict = Get-KnownLoopVerdict -Iterations @($runs | ForEach-Object Iterations) -Counted $counted -PerIteration $script:KnownLoopInstructionsPerIteration
        for ($i = 0; $i -lt $runs.Count; $i++) {
            Write-Host ("pmc-setup: {0:N0} iterations (known loop {1:N0} instructions): counted {2:N0} (pid {3})" -f $runs[$i].Iterations, ($runs[$i].Iterations * $script:KnownLoopInstructionsPerIteration), $counted[$i], $runs[$i].Pid)
        }
        $line = 'difference: expected {0:N0}, counted {1:N0}, off by {2:N3}% (tolerance {3}%), {4} context switches on CPU {5}' -f $verdict.Expected, $verdict.Measured, $verdict.ErrorPercent, $verdict.TolerancePercent, $report.switches, $Cpu
        if (-not $verdict.Pass) { throw "the counter does not count instructions: $line" }
        Write-Host "pmc-setup: VERIFIED - $line"
    } finally {
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
        $lock.ReleaseMutex(); $lock.Dispose()
    }
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
if ($mi.RebootPending) { Write-Host 'pmc-setup: reboot for the memory integrity change to take effect'; $ready = $false }
if ($ready) { Write-Host 'pmc-setup: ready; prove it with -Verify, which takes a real trace' } else { Write-Host 'pmc-setup: NOT READY' }
exit $(if ($ready) { 0 } elseif ($problems.Count -eq 0 -and $mi.RebootPending) { 3 } else { 1 })
