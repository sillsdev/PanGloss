<#
  .DESCRIPTION
  Covers: Wait-StileAdmission (rust/tools/_common.ps1) with a fake `stile` runner and sleeper, so no
  real stile, load or clock is involved: waits on exit 10 and proceeds on 0, times out, fails closed
  on any other exit, skips visibly, and keeps preland's child runs from waiting twice.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

function New-FakeStile {
    param([int[]]$Codes)
    $state = @{ Codes = $Codes; Calls = [System.Collections.Generic.List[object]]::new() }
    $state.Runner = {
        param($StileArgs)
        $i = $state.Calls.Count
        $state.Calls.Add($StileArgs)
        $c = $state.Codes[[Math]::Min($i, $state.Codes.Count - 1)]
        [PSCustomObject]@{ Code = $c; Out = $(if ($c -eq 10) { 'waiting: load 53.71 > 2.5 x 20 cores = 50' } else { 'start: ok' }) }
    }.GetNewClosure()
    $state
}

function Invoke-Admit {
    param($Fake, [int]$MaxWait = 3600, [string]$Blocker = '')
    $slept = [System.Collections.Generic.List[int]]::new()
    $env:PANGLOSS_ADMITTED = $null; $env:PANGLOSS_SKIP_ADMIT = $null
    $out = & { Wait-StileAdmission -Jobs 3 -Mode build -MaxWaitSeconds $MaxWait -ReleaseBlocker $Blocker -Runner $Fake.Runner -Sleeper { param($s) $slept.Add($s) } } 6>&1
    $code = @($out | Where-Object { $_ -is [int] })[-1]
    [PSCustomObject]@{ Code = $code; Slept = $slept; Text = (($out | Where-Object { $_ -isnot [int] }) | ForEach-Object { "$_" }) -join "`n" }
}

Test-Case 'exit 10, 10, 0 waits twice with backoff, then admits and marks the run admitted' {
    $f = New-FakeStile @(10, 10, 0)
    $r = Invoke-Admit $f
    Assert-Equal 0 $r.Code
    Assert-Equal 3 $f.Calls.Count
    Assert-Equal 2 $r.Slept.Count
    Assert-Equal 15 $r.Slept[0]
    Assert-Equal 22 $r.Slept[1]
    Assert-Equal 2 ([regex]::Matches($r.Text, '\[pg\] admit: waiting').Count) 'one wait line per exit 10'
    Assert-Equal '1' $env:PANGLOSS_ADMITTED
    $env:PANGLOSS_ADMITTED = $null
}

Test-Case 'stile is asked for the jobs the mode uses and passes -ReleaseBlocker through' {
    $f = New-FakeStile @(0)
    $null = Invoke-Admit $f -Blocker 'queue blocked'
    $a = $f.Calls[0]
    Assert-Equal 'admit' $a[0]
    Assert-Equal '3' $a[$a.IndexOf('--jobs') + 1]
    Assert-Equal 'queue blocked' $a[$a.IndexOf('--release-blocker') + 1]
    $env:PANGLOSS_ADMITTED = $null
}

Test-Case 'exit 10 forever times out with ExitCodeAdmitTimeout and does not mark admitted' {
    $f = New-FakeStile @(10)
    $r = Invoke-Admit $f -MaxWait 0
    Assert-Equal $script:ExitCodeAdmitTimeout $r.Code
    Assert-Equal 1 $f.Calls.Count
    Assert-True ($r.Text -match 'nothing was started') 'timeout must say nothing started'
    Assert-True (-not $env:PANGLOSS_ADMITTED) 'a timed-out run must not claim admission'
}

Test-Case 'any exit other than 0 or 10 fails closed with ExitCodeAdmit' {
    foreach ($code in 1, 2, 3, 11, 127) {
        $r = Invoke-Admit (New-FakeStile @($code))
        Assert-Equal $script:ExitCodeAdmit $r.Code "stile exit $code"
    }
}

Test-Case 'PANGLOSS_SKIP_ADMIT=1 and PANGLOSS_ADMITTED=1 skip without calling stile' {
    foreach ($name in 'PANGLOSS_SKIP_ADMIT', 'PANGLOSS_ADMITTED') {
        $f = New-FakeStile @(10)
        $env:PANGLOSS_ADMITTED = $null; $env:PANGLOSS_SKIP_ADMIT = $null
        Set-Item "env:$name" '1'
        $out = Wait-StileAdmission -Jobs 1 -Mode build -MaxWaitSeconds 0 -Runner $f.Runner 6>&1
        $env:PANGLOSS_ADMITTED = $null; $env:PANGLOSS_SKIP_ADMIT = $null
        Assert-Equal 0 @($out | Where-Object { $_ -is [int] })[-1]
        Assert-Equal 0 $f.Calls.Count
        Assert-True ((($out | ForEach-Object { "$_" }) -join ' ') -match 'admit: skipped') 'skip must print one visible line'
    }
}

Test-Case 'Clear-StileAdmissionMarker clears only a marker this invocation set, never one inherited from the parent' {
    $f = New-FakeStile @(0)
    $null = Invoke-Admit $f
    Assert-Equal '1' $env:PANGLOSS_ADMITTED
    Clear-StileAdmissionMarker
    Assert-True (-not $env:PANGLOSS_ADMITTED) 'the invocation that set the marker clears it on exit'
    $env:PANGLOSS_ADMITTED = '1'; $env:PANGLOSS_SKIP_ADMIT = $null
    $null = Wait-StileAdmission -Jobs 1 -Mode build -Runner $f.Runner 6>&1
    Clear-StileAdmissionMarker
    Assert-Equal '1' $env:PANGLOSS_ADMITTED
    $env:PANGLOSS_ADMITTED = $null
}

Test-Case 'stile absent from PATH skips with one visible line' {
    $env:PANGLOSS_ADMITTED = $null; $env:PANGLOSS_SKIP_ADMIT = $null
    $oldPath = $env:PATH
    try {
        $env:PATH = [System.IO.Path]::GetTempPath()
        $out = Wait-StileAdmission -Jobs 1 -Mode build -MaxWaitSeconds 0 6>&1
    } finally { $env:PATH = $oldPath }
    Assert-Equal 0 @($out | Where-Object { $_ -is [int] })[-1]
    Assert-True ((($out | ForEach-Object { "$_" }) -join ' ') -match 'stile not on PATH') 'must say why it skipped'
}

Write-TestSummary
