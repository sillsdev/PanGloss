<#
  .DESCRIPTION
  Pure logic behind rust/tools/pmc-setup.ps1: what the PMC scheduled tasks must run, and how to read the
  memory integrity state. Reads no registry and touches no task; the caller passes what it found.
#>

$script:PmcDir = 'C:\ProgramData\PanGlossPmc'
$script:PmcTaskPath = '\PanGloss\'

function Get-PmcTaskSpecs {
    # The two tasks instruction-compare.ps1 starts. wpr.exe needs elevation; the tasks hold it so the comparison does not.
    param([string]$Dir = $script:PmcDir)
    $wpr = Join-Path $env:WINDIR 'System32\wpr.exe'
    @(
        [PSCustomObject]@{ Name = 'PanGloss PMC Start'; Execute = $wpr; Arguments = "-start `"$Dir\pg.wprp!PG`" -filemode"; WorkingDirectory = $Dir }
        [PSCustomObject]@{ Name = 'PanGloss PMC Stop'; Execute = $wpr; Arguments = "-stop `"$Dir\current.etl`" -skipPdbGen"; WorkingDirectory = $Dir }
    )
}

function Get-PmcTaskProblems {
    # $Found: one object per registered task with Name, Execute, Arguments, RunLevel. Returns one line per problem; empty means ready.
    param([object[]]$Found = @(), [object[]]$Specs = (Get-PmcTaskSpecs))
    $problems = @()
    foreach ($spec in $Specs) {
        $task = @($Found | Where-Object Name -eq $spec.Name) | Select-Object -First 1
        if (-not $task) { $problems += "task '$($spec.Name)' is not registered under $script:PmcTaskPath"; continue }
        if ($task.Execute -ne $spec.Execute -or $task.Arguments -ne $spec.Arguments) {
            $problems += "task '$($spec.Name)' runs '$($task.Execute) $($task.Arguments)', expected '$($spec.Execute) $($spec.Arguments)'"
        }
        if ("$($task.RunLevel)" -ne 'Highest') { $problems += "task '$($spec.Name)' runs at level '$($task.RunLevel)'; wpr.exe needs Highest" }
    }
    return , $problems
}

function Get-MemoryIntegrityState {
    # $Running: Win32_DeviceGuard.SecurityServicesRunning (2 is memory integrity, HVCI).
    # $Configured: the Enabled value under ...\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity, or $null when absent.
    # $PolicyEnabled: the HypervisorEnforcedCodeIntegrity value under the DeviceGuard policy key, or $null when absent.
    param([int[]]$Running = @(), $Configured = $null, $PolicyEnabled = $null)
    $isRunning = $Running -contains 2
    $policyOn = ($null -ne $PolicyEnabled) -and ([int]$PolicyEnabled -ne 0)
    $willRun = $policyOn -or (($null -ne $Configured) -and ([int]$Configured -ne 0))
    $summary = if ($isRunning -and -not $willRun) { 'running now; off after the next reboot' }
    elseif ($isRunning) { 'running' }
    elseif ($willRun) { 'not running now; on after the next reboot' }
    else { 'off' }
    [PSCustomObject]@{ Running = $isRunning; OnAfterReboot = $willRun; PolicyManaged = $policyOn; RebootPending = ($isRunning -ne $willRun); Summary = $summary }
}
