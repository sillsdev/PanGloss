<#
  .DESCRIPTION
  Covers: the pure logic in rust/tools/_instruction-compare.ps1 behind instruction-compare.ps1 -- verdicts,
  startup subtraction, signature comparison, and the refusals. Starts no process and reads no trace.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_instruction-compare.ps1"

function Write-Batch([string]$Dir, [string]$Name, [string[]]$Lines) {
    $p = Join-Path $Dir $Name
    [IO.File]::WriteAllLines($p, $Lines)
    return $p
}

Test-Case 'a clear improvement beyond noise is GO' {
    $v = Get-InstructionVerdict -BeforeNet @(1000000, 1002000) -AfterNet @(930000, 931000) -DifferingWords 0
    Assert-Equal 'GO' $v.Verdict
    Assert-True ($v.DeltaPercent -lt -6.5 -and $v.DeltaPercent -gt -7.5) "delta $($v.DeltaPercent)"
}

Test-Case 'a self-comparison inside the noise floor is NO-GO, not GO' {
    $v = Get-InstructionVerdict -BeforeNet @(1000000, 1004000) -AfterNet @(1001000, 1003000) -DifferingWords 0
    Assert-Equal 'NO-GO (within noise)' $v.Verdict
}

Test-Case 'a change smaller than twice the repeat spread is NO-GO even when it points down' {
    $v = Get-InstructionVerdict -BeforeNet @(1000000, 1030000) -AfterNet @(985000, 1015000) -DifferingWords 0
    Assert-Equal 'NO-GO (within noise)' $v.Verdict
}

Test-Case 'a regression beyond noise is NO-GO and says so' {
    $v = Get-InstructionVerdict -BeforeNet @(1000000, 1001000) -AfterNet @(1100000, 1101000) -DifferingWords 0
    Assert-Equal 'NO-GO (regression beyond noise)' $v.Verdict
}

Test-Case 'identical repeats still need the minimum percent to be GO' {
    $v = Get-InstructionVerdict -BeforeNet @(1000000, 1000000) -AfterNet @(998000, 998000) -DifferingWords 0 -MinPercent 0.5
    Assert-Equal 'NO-GO (within noise)' $v.Verdict
}

Test-Case 'any differing word refuses the verdict and the reason carries the count' {
    $v = Get-InstructionVerdict -BeforeNet @(1000000, 1000000) -AfterNet @(500000, 500000) -DifferingWords 3
    Assert-True $v.Refused
    Assert-Equal 'REFUSED' $v.Verdict
    Assert-True ($v.Reason -match '^3 word') $v.Reason
}

Test-Case 'a single repeat per side is refused: noise cannot be measured' {
    $v = Get-InstructionVerdict -BeforeNet @(1000000) -AfterNet @(900000, 901000) -DifferingWords 0
    Assert-True $v.Refused
}

Test-Case 'a side with nothing beyond startup is refused' {
    $v = Get-InstructionVerdict -BeforeNet @(0, 0) -AfterNet @(10, 10) -DifferingWords 0
    Assert-True $v.Refused
}

Test-Case 'startup is the mean of the empty runs and is removed from every full run' {
    $net = Get-NetInstructions -Full @(1000, 1010) -Empty @(100, 110)
    Assert-Equal 895 $net[0]
    Assert-Equal 905 $net[1]
}

Test-Case 'spread is range over mean in percent' {
    Assert-Equal 2 (Get-SpreadPercent @(99, 101))
}

Test-Case 'signature comparison counts status and signature differences and one-sided indices, ignoring words and timing' {
    $dir = New-TestTempDir
    try {
        $a = Write-Batch $dir 'a.tsv' @("0`tw-one`t5`tok`tsig-a", "1`tw-two`t6`tSTARTED", "1`tw-two`t6`tfail`t-", "2`tw-three`t7`tok`tsig-c")
        $b = Write-Batch $dir 'b.tsv' @("0`tother`t900`tok`tsig-a", "1`tother`t1`tfail`t-", "2`tother`t2`tok`tsig-DIFFERENT", "3`tother`t2`tok`tsig-d")
        $r = Compare-BatchSignatures -Before (Read-BatchSignatures $a) -After (Read-BatchSignatures $b)
        Assert-Equal 4 $r.Words
        Assert-Equal 2 $r.Differ
        $same = Compare-BatchSignatures -Before (Read-BatchSignatures $a) -After (Read-BatchSignatures $a)
        Assert-Equal 0 $same.Differ
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force }
}

Test-Case 'absent PMC tasks are refused by name' {
    $threw = $false
    try { Assert-PmcTasksPresent -Found @('PanGloss PMC Start') } catch { $threw = $true; Assert-True ($_.Exception.Message -match 'PanGloss PMC Stop') $_.Exception.Message }
    Assert-True $threw 'a missing Stop task must throw'
    Assert-PmcTasksPresent -Found @('PanGloss PMC Start', 'PanGloss PMC Stop', 'other')
}

Test-Case 'a trace that lost events, lacks counters, has gaps, or missed a pid is refused' {
    $good = [PSCustomObject]@{ rc = 0; events_lost = 0; buffers_lost = 0; switches = 10; counters = 3; gaps_on_target = 0; instructions = [PSCustomObject]@{ '41' = 5e6 } }
    Assert-TraceIntact -Report $good -Pids @(41)
    foreach ($case in @(
            @{ Field = 'events_lost'; Value = 2; Match = 'lost events' },
            @{ Field = 'buffers_lost'; Value = 1; Match = 'lost events' },
            @{ Field = 'switches'; Value = 0; Match = 'no counter values' },
            @{ Field = 'gaps_on_target'; Value = 1; Match = 'gap' },
            @{ Field = 'rc'; Value = 5; Match = 'unreadable' })) {
        $bad = $good.PSObject.Copy()
        $bad.($case.Field) = $case.Value
        $threw = $false
        try { Assert-TraceIntact -Report $bad -Pids @(41) } catch { $threw = $true; Assert-True ($_.Exception.Message -match $case.Match) "$($case.Field): $($_.Exception.Message)" }
        Assert-True $threw "$($case.Field) must refuse"
    }
    $threw = $false
    try { Assert-TraceIntact -Report $good -Pids @(41, 99) } catch { $threw = $true; Assert-True ($_.Exception.Message -match 'pid 99') $_.Exception.Message }
    Assert-True $threw 'a pid with no instructions must refuse'
}

Test-Case 'callgrind totals line wins over the summary line; summary is the fallback; neither throws' {
    Assert-Equal 900 (Get-CallgrindTotal @('summary: 100', 'totals: 900'))
    Assert-Equal 100 (Get-CallgrindTotal @('events: Ir', 'summary: 100'))
    $threw = $false
    try { Get-CallgrindTotal @('events: Ir') | Out-Null } catch { $threw = $true }
    Assert-True $threw
}

Test-Case 'the plan alternates which side runs first and gives each side an empty and a full run per round' {
    $plan = New-MeasurementPlan -Repeats 3
    Assert-Equal 12 $plan.Count
    Assert-Equal 'before' $plan[0].Side
    Assert-Equal 'after' $plan[4].Side
    Assert-Equal 'before' $plan[8].Side
    foreach ($side in 'before', 'after') {
        Assert-Equal 3 @($plan | Where-Object { $_.Side -eq $side -and $_.Kind -eq 'full' }).Count
        Assert-Equal 3 @($plan | Where-Object { $_.Side -eq $side -and $_.Kind -eq 'empty' }).Count
    }
}

Test-Case 'arguments with spaces are quoted for the child process' {
    Assert-Equal 'batch "a b.xml" c.txt' (ConvertTo-ArgumentString @('batch', 'a b.xml', 'c.txt'))
}

Write-TestSummary
