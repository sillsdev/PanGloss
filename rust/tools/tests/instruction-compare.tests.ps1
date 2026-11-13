<#
  .DESCRIPTION
  Covers: the pure logic in rust/tools/_instruction-compare.ps1 behind instruction-compare.ps1 -- verdicts (Welch interval,
  critical t, verdict classes), per-round startup subtraction, signature comparison, and the refusals. Starts no process and reads no trace.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_instruction-compare.ps1"

function Write-Batch([string]$Dir, [string]$Name, [string[]]$Lines) {
    $p = Join-Path $Dir $Name
    [IO.File]::WriteAllLines($p, $Lines)
    return $p
}

Test-Case 'critical t matches published two-sided 95% values, including fractional df' {
    foreach ($case in @(@(1, 12.7062), @(2, 4.3027), @(4, 2.7764), @(8, 2.3060), @(30, 2.0423), @(1000, 1.9623))) {
        $tc = Get-StudentTCritical -Df $case[0]
        Assert-True ([Math]::Abs($tc - $case[1]) -lt 1e-3) "df $($case[0]): got $tc, want $($case[1])"
    }
    $mid = Get-StudentTCritical -Df 5.5
    Assert-True ($mid -lt (Get-StudentTCritical -Df 5) -and $mid -gt (Get-StudentTCritical -Df 6)) "df 5.5 gave $mid"
    $threw = $false
    try { Get-StudentTCritical -Df 0 | Out-Null } catch { $threw = $true }
    Assert-True $threw 'df 0 must throw'
}

Test-Case 'Welch interval: df, standard error and bounds match a hand-worked case' {
    # var(before) = 0.625, var(after) = 0.1, n = 5 each: se^2 = 0.145, df = 0.145^2 / (0.125^2/4 + 0.02^2/4) = 5.248.
    $ci = Get-WelchInterval -Before @(100, 101, 99, 100.5, 99.5) -After @(98, 98.4, 97.6, 98.2, 97.8)
    Assert-True ([Math]::Abs($ci.DeltaPercent + 2.0) -lt 1e-9) "delta $($ci.DeltaPercent)"
    Assert-True ([Math]::Abs($ci.Df - 5.24805) -lt 1e-4) "df $($ci.Df)"
    Assert-True ([Math]::Abs($ci.StdErrPercent - [Math]::Sqrt(0.145)) -lt 1e-9) "se $($ci.StdErrPercent)"
    $half = $ci.TCritical * [Math]::Sqrt(0.145)
    Assert-True ([Math]::Abs($ci.LowPercent - (-2.0 - $half)) -lt 1e-9 -and [Math]::Abs($ci.HighPercent - (-2.0 + $half)) -lt 1e-9) "[$($ci.LowPercent), $($ci.HighPercent)]"
    Assert-True ($ci.TCritical -gt 2.44 -and $ci.TCritical -lt 2.58) "t $($ci.TCritical)"
}

Test-Case 'Welch df falls to n-1 of the noisy side when the other side has no spread' {
    $ci = Get-WelchInterval -Before @(100, 100, 100, 100) -After @(97, 98, 99, 98)
    Assert-True ([Math]::Abs($ci.Df - 3) -lt 1e-9) "df $($ci.Df)"
}

Test-Case 'two sides with no spread give an exact point interval, not a division by zero' {
    $same = Get-WelchInterval -Before @(1000, 1000) -After @(1000, 1000)
    Assert-Equal 0 $same.LowPercent
    Assert-Equal 0 $same.HighPercent
    Assert-Equal 'NO-GO (no change)' (Get-InstructionVerdict -BeforeNet @(1000, 1000) -AfterNet @(1000, 1000) -DifferingWords 0).Verdict
    Assert-Equal 'GO' (Get-InstructionVerdict -BeforeNet @(1000, 1000) -AfterNet @(990, 990) -DifferingWords 0).Verdict
    Assert-Equal 'NO-GO (no change)' (Get-InstructionVerdict -BeforeNet @(1000, 1000) -AfterNet @(998, 998) -DifferingWords 0).Verdict
}

Test-Case 'verdict classes: GO, regression, no change and INCONCLUSIVE each from their interval' {
    Assert-Equal 'GO' (Get-VerdictFromInterval -DeltaPercent -2 -LowPercent -2.5 -HighPercent -1.5)
    Assert-Equal 'NO-GO (regression)' (Get-VerdictFromInterval -DeltaPercent 2 -LowPercent 1.5 -HighPercent 2.5)
    Assert-Equal 'NO-GO (no change)' (Get-VerdictFromInterval -DeltaPercent 0.1 -LowPercent -0.3 -HighPercent 0.4)
    # significant but under the minimum effect, and the interval reaches past it: neither claim holds
    Assert-Equal 'INCONCLUSIVE' (Get-VerdictFromInterval -DeltaPercent -0.4 -LowPercent -0.7 -HighPercent -0.1)
    # large point change, interval straddles zero
    Assert-Equal 'INCONCLUSIVE' (Get-VerdictFromInterval -DeltaPercent -2 -LowPercent -5 -HighPercent 1)
    # a significant change smaller than the minimum effect whose interval lies inside it is no change
    Assert-Equal 'NO-GO (no change)' (Get-VerdictFromInterval -DeltaPercent -0.3 -LowPercent -0.4 -HighPercent -0.2)
}

Test-Case 'verdict classes never overlap: GO, regression and no change are tested against each other on a grid' {
    $vals = @(-3, -1, -0.5, -0.49, -0.2, 0, 0.2, 0.49, 0.5, 1, 3)
    foreach ($lo in $vals) {
        foreach ($hi in $vals) {
            if ($hi -lt $lo) { continue }
            foreach ($d in $vals) {
                if ($d -lt $lo -or $d -gt $hi) { continue }
                $go = $hi -lt 0 -and $d -le -0.5
                $reg = $lo -gt 0 -and $d -ge 0.5
                $none = $lo -gt -0.5 -and $hi -lt 0.5
                Assert-True (([int]$go + [int]$reg + [int]$none) -le 1) "two classes hold at d=$d [$lo, $hi]"
                $want = if ($go) { 'GO' } elseif ($reg) { 'NO-GO (regression)' } elseif ($none) { 'NO-GO (no change)' } else { 'INCONCLUSIVE' }
                Assert-Equal $want (Get-VerdictFromInterval -DeltaPercent $d -LowPercent $lo -HighPercent $hi) "d=$d [$lo, $hi]"
            }
        }
    }
}

Test-Case 'five quiet repeats per side resolve a 2% improvement as GO' {
    $v = Get-InstructionVerdict -BeforeNet @(1000000, 1004000, 996000, 1002000, 998000) -AfterNet @(980000, 983000, 977000, 981000, 979000) -DifferingWords 0
    Assert-Equal 'GO' $v.Verdict
    Assert-True ($v.HighPercent -lt 0) "CI [$($v.LowPercent), $($v.HighPercent)]"
}

Test-Case 'a binary against itself with real spread reads no change or INCONCLUSIVE, never GO or regression' {
    $v = Get-InstructionVerdict -BeforeNet @(1000000, 1004000, 996000, 1002000, 998000) -AfterNet @(1001000, 997000, 1003000, 999000, 1000000) -DifferingWords 0
    Assert-Equal 'NO-GO (no change)' $v.Verdict
    $noisy = Get-InstructionVerdict -BeforeNet @(1000000, 1040000) -AfterNet @(1010000, 970000) -DifferingWords 0
    Assert-Equal 'INCONCLUSIVE' $noisy.Verdict
}

Test-Case 'a regression beyond the minimum effect reads NO-GO (regression)' {
    $v = Get-InstructionVerdict -BeforeNet @(1000000, 1001000, 999000) -AfterNet @(1100000, 1101000, 1099000) -DifferingWords 0
    Assert-Equal 'NO-GO (regression)' $v.Verdict
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

Test-Case 'each round''s empty run is removed from the same round''s full run, so start-up noise reaches the variance' {
    $net = Get-NetInstructions -Full @(1000, 1010) -Empty @(100, 130)
    Assert-Equal 900 $net[0]
    Assert-Equal 880 $net[1]
    $threw = $false
    try { Get-NetInstructions -Full @(1000, 1010) -Empty @(100) | Out-Null } catch { $threw = $true }
    Assert-True $threw 'unpaired rounds must throw'
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
    $good = [PSCustomObject]@{ rc = 0; events_lost = 0; buffers_lost = 0; switches = 10; counters = 1; gaps_on_target = 0; callback_errors = 0; instructions = [PSCustomObject]@{ '41' = 5e6 } }
    Assert-TraceIntact -Report $good -Pids @(41)
    foreach ($case in @(
            @{ Field = 'events_lost'; Value = 2; Match = 'lost events' },
            @{ Field = 'buffers_lost'; Value = 1; Match = 'lost events' },
            @{ Field = 'switches'; Value = 0; Match = 'no counter values' },
            @{ Field = 'counters'; Value = 2; Match = 'records 2 counters' },
            @{ Field = 'gaps_on_target'; Value = 1; Match = 'gap' },
            @{ Field = 'callback_errors'; Value = 4; Match = 'failed to process' },
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
    Assert-Equal 100 (Get-CallgrindTotal @('part: 1', '', 'summary: 100', ''))
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

Test-Case 'duplicate pids are refused and distinct pids pass' {
    Assert-DistinctPids -Pids @(10, 11, 12)
    $threw = $false
    try { Assert-DistinctPids -Pids @(10, 11, 10) } catch { $threw = $true }
    Assert-True $threw
}

Test-Case 'a noisy start-up widens the interval and can turn a GO into INCONCLUSIVE' {
    $full = [double[]]@(1000000, 1000000, 1000000)
    $quiet = Get-InstructionVerdict -BeforeNet (Get-NetInstructions $full @(500000, 500000, 500000)) -AfterNet (Get-NetInstructions @(970000, 970000, 971000) @(500000, 500000, 500000)) -DifferingWords 0
    Assert-Equal 'GO' $quiet.Verdict
    $noisy = Get-InstructionVerdict -BeforeNet (Get-NetInstructions $full @(480000, 500000, 520000)) -AfterNet (Get-NetInstructions @(970000, 970000, 971000) @(520000, 480000, 500000)) -DifferingWords 0
    Assert-Equal 'INCONCLUSIVE' $noisy.Verdict
}
Test-Case 'a non-positive single net is refused even when the mean is positive' {
    $v = Get-InstructionVerdict -BeforeNet @(1000000, 1000000) -AfterNet @(2000000, -5) -DifferingWords 0
    Assert-True $v.Refused
}

Test-Case 'comparability: zero words, SKIPPED rows and row-count mismatch are each refused' {
    Assert-Equal $null (Get-ComparabilityProblem -Words 5 -SkippedBefore 0 -SkippedAfter 0 -RowsBefore 5 -RowsAfter 5 -InputWords 5)
    Assert-True ((Get-ComparabilityProblem -Words 0 -SkippedBefore 0 -SkippedAfter 0 -RowsBefore 0 -RowsAfter 0 -InputWords 0) -match 'no words')
    Assert-True ((Get-ComparabilityProblem -Words 5 -SkippedBefore 1 -SkippedAfter 0 -RowsBefore 5 -RowsAfter 5 -InputWords 5) -match 'SKIPPED')
    Assert-True ((Get-ComparabilityProblem -Words 5 -SkippedBefore 0 -SkippedAfter 0 -RowsBefore 5 -RowsAfter 4 -InputWords 5) -match 'do not match')
    $v = Get-InstructionVerdict -BeforeNet @(1000000, 1000000) -AfterNet @(500000, 500000) -DifferingWords 0 -Problem 'x'
    Assert-True $v.Refused
}

Test-Case 'a non-numeric index field is refused without echoing its text' {
    $dir = New-TestTempDir
    try {
        $p = Write-Batch $dir 'bad.tsv' @("SECRETTEXT`tw`t1`tok`tsig")
        $msg = ''
        try { Read-BatchSignatures $p | Out-Null } catch { $msg = $_.Exception.Message }
        Assert-True ($msg.Length -gt 0) 'must refuse'
        Assert-False ($msg -match 'SECRETTEXT') 'the refusal echoed row text'
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force }
}

Test-Case 'pinned repeats must be identical, or within the given tolerance' {
    Assert-True (Test-RepeatsIdentical @(5, 5, 5))
    Assert-False (Test-RepeatsIdentical @(5, 6))
    Assert-True (Test-RepeatsIdentical @(746640359, 746636048) -RelativeTolerance 5e-5)
    Assert-False (Test-RepeatsIdentical @(745625497, 745390312) -RelativeTolerance 5e-5)
}

Test-Case 'exit code: all GO is 0, any NO-GO is 10, any INCONCLUSIVE is 11 over NO-GO, any refusal is 3 and wins' {
    Assert-Equal 0 (Get-VerdictExitCode @('GO', 'GO'))
    Assert-Equal 10 (Get-VerdictExitCode @('GO', 'NO-GO (no change)'))
    Assert-Equal 10 (Get-VerdictExitCode @('NO-GO (regression)'))
    Assert-Equal 11 (Get-VerdictExitCode @('GO', 'INCONCLUSIVE'))
    Assert-Equal 11 (Get-VerdictExitCode @('NO-GO (regression)', 'INCONCLUSIVE'))
    Assert-Equal 3 (Get-VerdictExitCode @('INCONCLUSIVE', 'REFUSED'))
}

Test-Case 'run tags name the two sides with neutral labels of equal length' {
    # 'before'/'after' in the output path biased the parse thread about +1% on the 'after' side of a binary compared with itself.
    foreach ($kind in 'empty', 'full') {
        $a = Get-RunTag -Grammar 0 -Round 1 -Side before -Kind $kind
        $b = Get-RunTag -Grammar 0 -Round 1 -Side after -Kind $kind
        Assert-True ($a -ne $b) "both sides got tag $a"
        Assert-Equal $a.Length $b.Length
        foreach ($t in $a, $b) { Assert-True ($t -notmatch 'before|after') "tag $t names its side" }
    }
    Assert-Equal 'g2-r3-sideA-full' (Get-RunTag -Grammar 2 -Round 3 -Side before -Kind full)
    Assert-Equal 'g2-r3-sideB-empty' (Get-RunTag -Grammar 2 -Round 3 -Side after -Kind empty)
}

function Invoke-Tally([string]$Body) {
    # Runs a Python snippet against the Tally class (no Windows needed) and returns its printed JSON.
    $py = (Get-Command python3, python -ErrorAction SilentlyContinue | Select-Object -First 1).Source
    if (-not $py) { throw 'python is not on PATH' }
    $script = "import json, sys`nsys.path.insert(0, sys.argv[1])`nfrom instruction_compare_etl import Tally`n$Body"
    return ($script | & $py -I - (Resolve-Path "$PSScriptRoot\..").Path) | ConvertFrom-Json
}

Test-Case 'trace reader counts only the last process that held a measured pid' {
    # pid 7 is used first by another process (thread 70), then by the measured run (threads 71, 72); CPU 0 carries every switch.
    $r = Invoke-Tally @'
t = Tally([7])
t.process_start(7); t.thread_start(7, 70)
t.cswitch(0, 70, 1, (100,)); t.cswitch(0, 1, 70, (600,))
t.process_start(7); t.thread_start(7, 71); t.thread_start(7, 72)
t.cswitch(0, 71, 1, (1000,)); t.cswitch(0, 72, 71, (1030,)); t.cswitch(0, 1, 72, (1100,))
print(json.dumps(t.report()))
'@
    Assert-Equal 100 ([double]$r.instructions.'7')
    Assert-Equal '30 70' (@($r.thread_instructions.'7') -join ' ')
    Assert-Equal 1 ([int]$r.earlier_processes)
    Assert-Equal 2 ([int]$r.threads.'7')
}

Test-Case 'a gap that touches only an earlier process that reused the pid is not a gap on the measured run (#269)' {
    # Thread 70 belongs to the earlier process; a switch is missing on CPU 0 while it ran (the next switch names 70 although 1 came in).
    $r = Invoke-Tally @'
t = Tally([7])
t.process_start(7); t.thread_start(7, 70)
t.cswitch(0, 70, 1, (100,)); t.cswitch(0, 1, 5, (150,)); t.cswitch(0, 1, 70, (600,))
t.process_start(7); t.thread_start(7, 71)
t.cswitch(0, 71, 1, (1000,)); t.cswitch(0, 1, 71, (1100,))
print(json.dumps(t.report()))
'@
    Assert-Equal 0 ([int]$r.gaps_on_target) 'the gap was on the earlier process'
    Assert-Equal 100 ([double]$r.instructions.'7')
    Assert-Equal 1 ([int]$r.earlier_processes)
}

Test-Case 'a gap that touches the last process of a measured pid still counts, and a reused thread id does not hide it (#269)' {
    $r = Invoke-Tally @'
t = Tally([7])
t.process_start(7); t.thread_start(7, 70)
t.cswitch(0, 70, 1, (100,)); t.cswitch(0, 1, 5, (150,))
t.process_start(7); t.thread_start(7, 70)
t.cswitch(0, 70, 1, (1000,)); t.cswitch(0, 1, 6, (1050,)); t.cswitch(0, 1, 70, (1100,))
print(json.dumps(t.report()))
'@
    Assert-True ([int]$r.gaps_on_target -ge 1) "gaps_on_target $($r.gaps_on_target)"
}

Test-Case 'phase marks: the trace reader totals what the other threads run between the two marker threads' {
    # Thread 10 loads, runs marker 11, parses (preempted once by foreign thread 99), runs marker 12; thread 9 is a loader thread and 13 a pool thread started after the end mark.
    $r = Invoke-Tally @'
M = 1_000_000
t = Tally([7])
t.process_start(7); t.thread_start(7, 10); t.thread_start(7, 9)
t.cswitch(0, 10, 1, (0,))
t.thread_start(7, 11)
c = 2_000_000_000
t.cswitch(0, 11, 10, (c,))
c += 60 * M; t.cswitch(0, 10, 11, (c,))
c += 700 * M; t.cswitch(0, 99, 10, (c,))
c += 1000; t.cswitch(0, 10, 99, (c,))
t.thread_start(7, 12)
c += 300 * M; t.cswitch(0, 12, 10, (c,))
c += 60 * M; t.cswitch(0, 10, 12, (c,))
t.thread_start(7, 13)
c += 200; t.cswitch(0, 13, 10, (c,))
c += 90000; t.cswitch(0, 1, 13, (c,))
t.cswitch(0, 9, 1, (c + 5,)); t.cswitch(0, 1, 9, (c + 5 + 12 * M,))
print(json.dumps(t.report()))
'@
    Assert-Equal 2 ([int]$r.phase.'7') 'both marks ran'
    Assert-Equal 1000000000 ([double]$r.parse_instructions.'7') 'parse = 700M + 300M between the markers'
    Assert-Equal 3120090200 ([double]$r.instructions.'7' - 12 * 1000000) 'every thread of the process still totals'
}

Test-Case 'phase marks: a process with fewer or more than two marker threads reports phase 0, as does one without marks' {
    $r = Invoke-Tally @'
M = 1_000_000
t = Tally([7, 8, 9])
t.process_start(7); t.thread_start(7, 10); t.thread_start(7, 11)
t.cswitch(0, 10, 1, (0,)); t.cswitch(0, 11, 10, (500 * M,)); t.cswitch(0, 10, 11, (560 * M,)); t.cswitch(0, 1, 10, (1000 * M,))
t.process_start(8); t.thread_start(8, 20)
t.cswitch(0, 20, 1, (2000 * M,)); t.cswitch(0, 1, 20, (2300 * M,))
t.process_start(9); t.thread_start(9, 30); t.thread_start(9, 31); t.thread_start(9, 32); t.thread_start(9, 33)
t.cswitch(0, 30, 1, (3000 * M,)); t.cswitch(0, 31, 30, (3500 * M,)); t.cswitch(0, 32, 31, (3560 * M,)); t.cswitch(0, 33, 32, (3620 * M,)); t.cswitch(0, 1, 33, (3680 * M,))
print(json.dumps(t.report()))
'@
    Assert-Equal 0 ([int]$r.phase.'7') 'one marker thread'
    Assert-Equal 0 ([int]$r.phase.'8') 'no marker threads'
    Assert-Equal 0 ([double]$r.parse_instructions.'8')
    Assert-Equal 0 ([int]$r.phase.'9') 'three marker threads'
}

Test-Case 'a trace without a completed phase is refused when the parse phase is required' {
    $ok = [PSCustomObject]@{ rc = 0; events_lost = 0; buffers_lost = 0; callback_errors = 0; switches = 10; counters = 1; gaps_on_target = 0
        instructions = [PSCustomObject]@{ '41' = 5e9 }; parse_instructions = [PSCustomObject]@{ '41' = 1e9 }; phase = [PSCustomObject]@{ '41' = 2 } }
    Assert-TraceIntact -Report $ok -Pids @(41) -RequirePhase
    foreach ($bad in 0, 1) {
        $r = $ok | ConvertTo-Json -Depth 4 | ConvertFrom-Json
        $r.phase.'41' = $bad
        $msg = ''
        try { Assert-TraceIntact -Report $r -Pids @(41) -RequirePhase } catch { $msg = $_.Exception.Message }
        Assert-True ($msg -match 'phase mark') "phase $bad : $msg"
    }
    $r = $ok | ConvertTo-Json -Depth 4 | ConvertFrom-Json
    $r.parse_instructions.'41' = 0
    $msg = ''
    try { Assert-TraceIntact -Report $r -Pids @(41) -RequirePhase } catch { $msg = $_.Exception.Message }
    Assert-True ($msg -match 'parse phase') $msg
    $r.phase.'41' = 0
    Assert-TraceIntact -Report $r -Pids @(41)
}

Test-Case 'direct counting plans only the full-list run, in the same alternating order' {
    $plan = New-MeasurementPlan -Repeats 3 -NoStartup
    Assert-Equal 6 @($plan).Count
    Assert-True (@($plan | Where-Object Kind -eq 'empty').Count -eq 0) 'no empty-list runs'
    Assert-Equal 'before after after before before after' (@($plan | ForEach-Object Side) -join ' ')
}

Test-Case 'load ratio: a load many times the parse is refused, a modest one is not (#277, #278)' {
    Assert-True ($null -eq (Get-LoadRatioProblem -Startup 2.4e9 -Parse 0.8e9))
    $why = Get-LoadRatioProblem -Startup 2.55e9 -Parse 0.51e9 -MaxRatio 4
    Assert-True ($why -match 'times the parse' -and $why -match 'phase-marks') "Swahili json was not refused: $why"
    Assert-True ($null -ne (Get-LoadRatioProblem -Startup 27.6e9 -Parse 95e6))
    Assert-True ($null -ne (Get-LoadRatioProblem -Startup 27e9 -Parse -78e6)) 'a negative parse count'
    Assert-True ($null -eq (Get-LoadRatioProblem -Startup 5e9 -Parse 1e9 -MaxRatio 6))
}

Test-Case 'callgrind counts the parse by collecting only inside parse_word_opts' {
    $a = @(Get-CallgrindParseArguments)
    Assert-Contains $a '--collect-atstart=no'
    Assert-Contains $a '--toggle-collect=*parse_word_opts*'
}

Test-Case 'describe probe: only a batch command that lists the flag counts' {
    $with = '{"commands":[{"name":"batch","flags":[{"name":"--threads"},{"name":"--no-flush"}]},{"name":"parse","flags":[]}]}'
    $other = '{"commands":[{"name":"batch","flags":[{"name":"--threads"}]},{"name":"parse","flags":[{"name":"--no-flush"}]}]}'
    Assert-True (Test-DescribesBatchFlag $with '--no-flush') 'listed under batch'
    Assert-True (-not (Test-DescribesBatchFlag $other '--no-flush')) 'listed only under another command'
    Assert-True (-not (Test-DescribesBatchFlag $null '--no-flush')) 'no describe output'
    Assert-True (-not (Test-DescribesBatchFlag 'pangloss 0.16 usage text' '--no-flush')) 'unparsable output'
}

Write-TestSummary
