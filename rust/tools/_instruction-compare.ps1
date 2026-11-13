# Pure logic for instruction-compare.ps1: nothing here starts a process or touches a trace, so rust/tools/tests/instruction-compare.tests.ps1 can cover it.

function Read-BatchSignatures {
    # idx -> "status<TAB>signature" for every result row of a batch TSV; the word column is never kept.
    param([Parameter(Mandatory)][string]$Path)
    $rows = @{}
    foreach ($line in [System.IO.File]::ReadAllLines($Path)) {
        $f = $line.Split("`t")
        if ($f.Count -lt 5 -or $f[2] -eq 'STARTED') { continue }
        $idx = 0
        if (-not [int]::TryParse($f[0], [ref]$idx)) { throw "a result row has a non-numeric index field (row $($rows.Count + 1)); batch output is not in the expected format" }
        $rows[$idx] = "$($f[3])`t$($f[4])"
    }
    return $rows
}

function Compare-BatchSignatures {
    # How many word indices differ in status or signature, counting an index present on one side only.
    param([Parameter(Mandatory)][hashtable]$Before, [Parameter(Mandatory)][hashtable]$After)
    $keys = @($Before.Keys) + @($After.Keys) | Sort-Object -Unique
    $differ = 0
    foreach ($k in $keys) {
        if (-not $Before.ContainsKey($k) -or -not $After.ContainsKey($k) -or $Before[$k] -ne $After[$k]) { $differ++ }
    }
    return [PSCustomObject]@{ Words = @($keys).Count; Differ = $differ }
}

function Get-NetInstructions {
    # Each round's full run minus the same round's empty-list run on that side, so start-up noise stays in the per-round spread the test sees.
    param([Parameter(Mandatory)][double[]]$Full, [Parameter(Mandatory)][double[]]$Empty)
    if ($Full.Count -ne $Empty.Count) { throw "each round needs one empty-list run per full run ($($Full.Count) full, $($Empty.Count) empty)" }
    return [double[]]@(for ($i = 0; $i -lt $Full.Count; $i++) { $Full[$i] - $Empty[$i] })
}

function Get-LogGamma {
    # Lanczos approximation (g = 7, n = 9), accurate to about 15 digits for x > 0.
    param([Parameter(Mandatory)][double]$X)
    $c = @(0.99999999999980993, 676.5203681218851, -1259.1392167224028, 771.32342877765313, -176.61502916214059,
        12.507343278686905, -0.13857109526572012, 9.9843695780195716e-6, 1.5056327351493116e-7)
    if ($X -lt 0.5) { return [Math]::Log([Math]::PI / [Math]::Abs([Math]::Sin([Math]::PI * $X))) - (Get-LogGamma (1.0 - $X)) }
    $x1 = $X - 1.0
    $sum = $c[0]
    for ($i = 1; $i -lt 9; $i++) { $sum += $c[$i] / ($x1 + $i) }
    $tt = $x1 + 7.5
    return 0.5 * [Math]::Log(2 * [Math]::PI) + ($x1 + 0.5) * [Math]::Log($tt) - $tt + [Math]::Log($sum)
}

function Get-RegularizedBeta {
    # I_x(a, b) by Lentz's continued fraction, using the symmetry I_x(a,b) = 1 - I_(1-x)(b,a) where the fraction converges slowly.
    param([Parameter(Mandatory)][double]$X, [Parameter(Mandatory)][double]$A, [Parameter(Mandatory)][double]$B)
    if ($X -le 0) { return 0.0 }
    if ($X -ge 1) { return 1.0 }
    if ($X -gt ($A + 1.0) / ($A + $B + 2.0)) { return 1.0 - (Get-RegularizedBeta (1.0 - $X) $B $A) }
    $front = [Math]::Exp((Get-LogGamma ($A + $B)) - (Get-LogGamma $A) - (Get-LogGamma $B) + $A * [Math]::Log($X) + $B * [Math]::Log(1.0 - $X)) / $A
    $tiny = 1e-300
    $f = 1.0; $cc = 1.0; $d = 0.0
    for ($i = 0; $i -le 400; $i++) {
        $m = [Math]::Floor($i / 2)
        if ($i -eq 0) { $num = 1.0 }
        elseif ($i % 2 -eq 0) { $num = ($m * ($B - $m) * $X) / (($A + 2.0 * $m - 1.0) * ($A + 2.0 * $m)) }
        else { $num = - (($A + $m) * ($A + $B + $m) * $X) / (($A + 2.0 * $m) * ($A + 2.0 * $m + 1.0)) }
        $d = 1.0 + $num * $d
        if ([Math]::Abs($d) -lt $tiny) { $d = $tiny }
        $d = 1.0 / $d
        $cc = 1.0 + $num / $cc
        if ([Math]::Abs($cc) -lt $tiny) { $cc = $tiny }
        $step = $cc * $d
        $f *= $step
        if ([Math]::Abs(1.0 - $step) -lt 1e-14) { return $front * ($f - 1.0) }
    }
    throw "the incomplete beta function did not converge (x=$X, a=$A, b=$B)"
}

function Get-StudentTCritical {
    # Two-sided critical t for degrees of freedom $Df (fractional allowed): the t whose two tails together hold 1 - Confidence.
    param([Parameter(Mandatory)][double]$Df, [double]$Confidence = 0.95)
    if ($Df -le 0 -or [double]::IsNaN($Df)) { throw "degrees of freedom must be positive (got $Df)" }
    $alpha = 1.0 - $Confidence
    $lo = 0.0; $hi = 1.0
    while ((Get-RegularizedBeta ($Df / ($Df + $hi * $hi)) ($Df / 2.0) 0.5) -gt $alpha) { $hi *= 2.0; if ($hi -gt 1e7) { throw "no critical t below 1e7 for df $Df" } }
    for ($i = 0; $i -lt 200 -and ($hi - $lo) -gt 1e-12 * $hi; $i++) {
        $mid = ($lo + $hi) / 2.0
        if ((Get-RegularizedBeta ($Df / ($Df + $mid * $mid)) ($Df / 2.0) 0.5) -gt $alpha) { $lo = $mid } else { $hi = $mid }
    }
    return ($lo + $hi) / 2.0
}

function Get-WelchInterval {
    # Welch's t interval for mean(After) - mean(Before), in percent of mean(Before); before is treated as the fixed reference level.
    param([Parameter(Mandatory)][double[]]$Before, [Parameter(Mandatory)][double[]]$After, [double]$Confidence = 0.95)
    $nb = $Before.Count; $na = $After.Count
    if ($nb -lt 2 -or $na -lt 2) { throw 'each side needs at least 2 values for a variance' }
    $mb = ($Before | Measure-Object -Average).Average
    $ma = ($After | Measure-Object -Average).Average
    $vb = (($Before | ForEach-Object { ($_ - $mb) * ($_ - $mb) } | Measure-Object -Sum).Sum) / ($nb - 1)
    $va = (($After | ForEach-Object { ($_ - $ma) * ($_ - $ma) } | Measure-Object -Sum).Sum) / ($na - 1)
    $sb = $vb / $nb; $sa = $va / $na
    $se = [Math]::Sqrt($sb + $sa)
    # Both sides without spread (pinned callgrind repeats): the difference is exact, so the interval is a point and df is unbounded.
    if ($se -eq 0) { $df = [double]::PositiveInfinity; $tc = 0.0 }
    else {
        $df = ($sb + $sa) * ($sb + $sa) / ($sb * $sb / ($nb - 1) + $sa * $sa / ($na - 1))
        $tc = Get-StudentTCritical -Df $df -Confidence $Confidence
    }
    $scale = 100.0 / $mb
    $delta = ($ma - $mb) * $scale
    $half = $tc * $se * $scale
    return [PSCustomObject]@{ DeltaPercent = $delta; LowPercent = $delta - $half; HighPercent = $delta + $half; Df = $df; TCritical = $tc; StdErrPercent = $se * $scale }
}

function Get-VerdictFromInterval {
    # GO and regression need the interval to exclude zero and the point change to reach the minimum effect; no change needs the whole interval inside it.
    param([Parameter(Mandatory)][double]$DeltaPercent, [Parameter(Mandatory)][double]$LowPercent, [Parameter(Mandatory)][double]$HighPercent, [double]$MinPercent = 0.5)
    if ($HighPercent -lt 0 -and $DeltaPercent -le -$MinPercent) { return 'GO' }
    if ($LowPercent -gt 0 -and $DeltaPercent -ge $MinPercent) { return 'NO-GO (regression)' }
    if ($LowPercent -gt -$MinPercent -and $HighPercent -lt $MinPercent) { return 'NO-GO (no change)' }
    return 'INCONCLUSIVE'
}

function Get-InstructionVerdict {
    # $BeforeNet / $AfterNet are per-round parse counts (full minus that round's empty run); see Get-NetInstructions.
    param(
        [Parameter(Mandatory)][double[]]$BeforeNet,
        [Parameter(Mandatory)][double[]]$AfterNet,
        [Parameter(Mandatory)][int]$DifferingWords,
        [double]$MinPercent = 0.5,
        [double]$Confidence = 0.95,
        [string]$Problem = ''
    )
    $refuse = { param($why) [PSCustomObject]@{ Refused = $true; Reason = $why; Verdict = 'REFUSED'; DeltaPercent = $null; LowPercent = $null; HighPercent = $null; Df = $null } }
    if ($Problem) { return & $refuse $Problem }
    if ($DifferingWords -gt 0) { return & $refuse "$DifferingWords word(s) differ in status or signature between the binaries; an instruction count compares only identical parses" }
    if ($BeforeNet.Count -lt 2 -or $AfterNet.Count -lt 2) { return & $refuse 'each side needs at least 2 measured runs to estimate run-to-run variance' }
    if (@($BeforeNet + $AfterNet | Where-Object { $_ -le 0 }).Count -gt 0) { return & $refuse 'a run counted no instructions beyond its round''s empty-list run, so its parse count is meaningless' }
    $ci = Get-WelchInterval -Before $BeforeNet -After $AfterNet -Confidence $Confidence
    $verdict = Get-VerdictFromInterval -DeltaPercent $ci.DeltaPercent -LowPercent $ci.LowPercent -HighPercent $ci.HighPercent -MinPercent $MinPercent
    return [PSCustomObject]@{ Refused = $false; Reason = $null; Verdict = $verdict; DeltaPercent = $ci.DeltaPercent; LowPercent = $ci.LowPercent; HighPercent = $ci.HighPercent; Df = $ci.Df }
}

function Assert-PmcTasksPresent {
    param([string[]]$Found = @(), [string[]]$Required = @('PanGloss PMC Start', 'PanGloss PMC Stop'), [string]$TaskPath = '\PanGloss\')
    $missing = @($Required | Where-Object { $Found -notcontains $_ })
    if ($missing.Count -gt 0) {
        throw "PMC scheduled task(s) absent under ${TaskPath}: $($missing -join ', '). Without them no instruction count exists; an administrator installs them with rust/tools/pmc-setup.ps1 -Install, and this tool will not fall back to wall time or work_steps."
    }
}

function Invoke-PmcTask {
    # Runs one PMC scheduled task and waits for it to finish; throws on a nonzero result or a timeout.
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][int]$TimeoutSec, [string]$TaskPath = '\PanGloss\')
    $before = (Get-ScheduledTaskInfo -TaskPath $TaskPath -TaskName $Name).LastRunTime
    Start-ScheduledTask -TaskPath $TaskPath -TaskName $Name
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 500
        $state = (Get-ScheduledTask -TaskPath $TaskPath -TaskName $Name).State
        $info = Get-ScheduledTaskInfo -TaskPath $TaskPath -TaskName $Name
        if ($state -ne 'Running' -and $info.LastRunTime -gt $before) {
            if ($info.LastTaskResult -ne 0) { throw "$Name failed: result 0x$('{0:X}' -f $info.LastTaskResult)" }
            return
        }
    }
    throw "$Name did not finish within $TimeoutSec s"
}

function Assert-TraceIntact {
    # $Report is the JSON object instruction_compare_etl.py prints; $Pids are the processes whose counts are used.
    param([Parameter(Mandatory)]$Report, [Parameter(Mandatory)][int[]]$Pids, [switch]$RequirePhase)
    if ($Report.rc -ne 0) { throw "trace processing returned $($Report.rc); the trace is unreadable" }
    if ($Report.events_lost -gt 0 -or $Report.buffers_lost -gt 0) {
        throw "trace lost events ($($Report.events_lost) events, $($Report.buffers_lost) buffers); the counts would be short by an unknown amount"
    }
    if ($Report.callback_errors -gt 0) { throw "$($Report.callback_errors) trace event(s) failed to process; the counts would be short by an unknown amount" }
    if ($Report.switches -le 0 -or $Report.counters -lt 1) { throw 'trace carries no counter values on context switches; the PMC tasks did not record hardware counters' }
    if ($Report.counters -ne 1) { throw "trace records $($Report.counters) counters per switch; the reader takes only the first, which need not be InstructionRetired. pg.wprp must name exactly one counter" }
    if ($Report.gaps_on_target -gt 0) { throw "$($Report.gaps_on_target) context-switch gap(s) touched a measured process; its instruction count is incomplete" }
    foreach ($p in $Pids) {
        if ([double]$Report.instructions."$p" -le 0) { throw "no instructions attributed to measured pid $p; the trace missed the process" }
        if ($RequirePhase) {
            if ([int]$Report.phase."$p" -ne 2) { throw "pid $p did not run both phase marks (reached phase $($Report.phase."$p") of 2; thread totals $(@($Report.thread_instructions."$p") -join ', ')); the parse phase cannot be cut from the load. The reader needs exactly two threads of 51M to 69M instructions besides the busiest; is the binary's batch --phase-marks working?" }
            if ([double]$Report.parse_instructions."$p" -le 0) { throw "no instructions attributed to the parse phase of pid $p" }
        }
    }
}

function Get-LoadRatioProblem {
    # Subtraction only: the parse count is a small difference of two large runs, so when the start-up is many times the parse its noise decides the verdict. $null when the ratio is acceptable.
    param([Parameter(Mandatory)][double]$Startup, [Parameter(Mandatory)][double]$Parse, [double]$MaxRatio = 4)
    if ($Parse -le 0) { return "the parse count ($('{0:N0}' -f $Parse)) is not positive beside a start-up of $('{0:N0}' -f $Startup) instructions" }
    $ratio = $Startup / $Parse
    if ($ratio -gt $MaxRatio) { return ('grammar load is {0:0.0} times the parse ({1:N0} against {2:N0} instructions); above {3:0.#} times the start-up noise decides the figure and the subtraction cannot resolve a change. Use binaries that list batch --phase-marks (PMC) or run callgrind, which count the parse directly' -f $ratio, $Startup, $Parse, $MaxRatio) }
    return $null
}

function Get-CallgrindParseArguments {
    # Collects only while `Morpher::parse_word_opts` is on the stack, callees included, so the total is the parse and the grammar load is never counted.
    return @('--collect-atstart=no', '--toggle-collect=*parse_word_opts*')
}

function Get-CallgrindTotal {
    param([Parameter(Mandatory)][AllowEmptyString()][string[]]$Lines)
    $summary = $null
    foreach ($line in $Lines) {
        if ($line -like 'totals:*') { return [double](($line -split '\s+')[1]) }
        if ($line -like 'summary:*' -and $null -eq $summary) { $summary = [double](($line -split '\s+')[1]) }
    }
    if ($null -eq $summary) { throw 'callgrind output has neither a totals nor a summary line' }
    return $summary
}

function ConvertTo-ArgumentString {
    param([Parameter(Mandatory)][string[]]$Arguments)
    return ($Arguments | ForEach-Object { if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ } }) -join ' '
}

function New-MeasurementPlan {
    # Alternating order: round 1 runs Before first, round 2 After first, and so on; each side runs an empty-list and a full-list job per round, or only the full-list job when the parse phase is counted directly.
    param([Parameter(Mandatory)][int]$Repeats, [switch]$NoStartup)
    $plan = @()
    for ($round = 1; $round -le $Repeats; $round++) {
        $sides = if ($round % 2 -eq 1) { @('before', 'after') } else { @('after', 'before') }
        foreach ($side in $sides) {
            foreach ($kind in $(if ($NoStartup) { @('full') } else { @('empty', 'full') })) { $plan += [PSCustomObject]@{ Round = $round; Side = $side; Kind = $kind } }
        }
    }
    return $plan
}

function Get-RunTag {
    # Neutral, equal-length side labels: 'before'/'after' in the output path biased a self-comparison about 1% (docs/design/parse-cost.md).
    param([Parameter(Mandatory)][int]$Grammar, [Parameter(Mandatory)][int]$Round, [Parameter(Mandatory)][ValidateSet('before', 'after')][string]$Side, [Parameter(Mandatory)][string]$Kind)
    $label = @{ before = 'sideA'; after = 'sideB' }[$Side]
    return "g$Grammar-r$Round-$label-$Kind"
}

function Assert-DistinctPids {
    param([Parameter(Mandatory)][int[]]$Pids)
    $dup = @($Pids | Group-Object | Where-Object Count -gt 1)
    if ($dup.Count -gt 0) { throw "$($dup.Count) process id(s) were used by more than one run; their instructions would merge in the trace" }
}

function Get-ComparabilityProblem {
    # $null when the two result sets are comparable, otherwise the reason they are not.
    param([int]$Words, [int]$SkippedBefore, [int]$SkippedAfter, [int]$RowsBefore, [int]$RowsAfter, [int]$InputWords)
    if ($Words -eq 0) { return 'no words were compared' }
    if ($SkippedBefore + $SkippedAfter -gt 0) { return "$($SkippedBefore + $SkippedAfter) result row(s) are SKIPPED, so those words were never parsed" }
    if ($RowsBefore -ne $InputWords -or $RowsAfter -ne $InputWords) { return "result rows ($RowsBefore before, $RowsAfter after) do not match the $InputWords input word(s)" }
    return $null
}

function Test-RepeatsIdentical {
    # Exact by default; RelativeTolerance is the allowed (max - min) / min, for callgrind runs whose pinned residue is not zero.
    param([Parameter(Mandatory)][double[]]$Values, [double]$RelativeTolerance = 0)
    if (@($Values | Select-Object -Unique).Count -le 1) { return $true }
    $min = ($Values | Measure-Object -Minimum).Minimum
    return $min -gt 0 -and (($Values | Measure-Object -Maximum).Maximum - $min) / $min -le $RelativeTolerance
}

function Get-VerdictExitCode {
    # 0 every grammar GO; 10 any NO-GO; 11 any INCONCLUSIVE (wins over 10: rerun with more repeats before reading the rest); 3 any refused (wins); 2 is reserved for whole-run refusals.
    param([Parameter(Mandatory)][string[]]$Verdicts)
    if ($Verdicts -contains 'REFUSED') { return 3 }
    if ($Verdicts -contains 'INCONCLUSIVE') { return 11 }
    if (@($Verdicts | Where-Object { $_ -ne 'GO' }).Count -gt 0) { return 10 }
    return 0
}
function Test-DescribesBatchFlag {
    # $DescribeJson: stdout of `pangloss --describe` ($null or unparsable for a binary without it); true when its batch command lists $Flag.
    param([AllowNull()][string]$DescribeJson, [Parameter(Mandatory)][string]$Flag)
    if (-not $DescribeJson) { return $false }
    try { $doc = $DescribeJson | ConvertFrom-Json } catch { return $false }
    $batch = @($doc.commands | Where-Object { $_.name -eq 'batch' }) | Select-Object -First 1
    return [bool]($batch -and @($batch.flags | Where-Object { $_.name -eq $Flag }).Count -gt 0)
}
