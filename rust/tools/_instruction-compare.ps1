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

function Get-SpreadPercent {
    # (max - min) / mean of repeated measurements, in percent.
    param([Parameter(Mandatory)][double[]]$Values)
    $mean = ($Values | Measure-Object -Average).Average
    if ($mean -le 0) { return [double]::PositiveInfinity }
    $range = ($Values | Measure-Object -Maximum).Maximum - ($Values | Measure-Object -Minimum).Minimum
    return 100.0 * $range / $mean
}

function Get-NetInstructions {
    # Each full run minus the mean of the empty-word-list runs, which is the grammar-load cost.
    param([Parameter(Mandatory)][double[]]$Full, [Parameter(Mandatory)][double[]]$Empty)
    $startup = ($Empty | Measure-Object -Average).Average
    return [double[]]@($Full | ForEach-Object { $_ - $startup })
}

function Get-InstructionVerdict {
    param(
        [Parameter(Mandatory)][double[]]$BeforeNet,
        [Parameter(Mandatory)][double[]]$AfterNet,
        [Parameter(Mandatory)][int]$DifferingWords,
        [double]$NoiseMultiple = 2.0,
        [double]$MinPercent = 0.5,
        [double]$EmptySpreadInstructions = 0.0,
        [string]$Problem = ''
    )
    $refuse = { param($why) [PSCustomObject]@{ Refused = $true; Reason = $why; Verdict = 'REFUSED'; DeltaPercent = $null; NoisePercent = $null; ThresholdPercent = $null } }
    if ($Problem) { return & $refuse $Problem }
    if ($DifferingWords -gt 0) { return & $refuse "$DifferingWords word(s) differ in status or signature between the binaries; an instruction count compares only identical parses" }
    if ($BeforeNet.Count -lt 2 -or $AfterNet.Count -lt 2) { return & $refuse 'each side needs at least 2 measured runs to measure run-to-run noise' }
    if (@($BeforeNet + $AfterNet | Where-Object { $_ -le 0 }).Count -gt 0) { return & $refuse 'a run counted no instructions beyond startup, so its net count is meaningless' }
    $meanBefore = ($BeforeNet | Measure-Object -Average).Average
    $meanAfter = ($AfterNet | Measure-Object -Average).Average
    if ($meanBefore -le 0 -or $meanAfter -le 0) { return & $refuse 'a side counted no instructions beyond startup, so the word list gave it nothing to measure' }
    $delta = 100.0 * ($meanAfter - $meanBefore) / $meanBefore
    $noise = [Math]::Max((Get-SpreadPercent $BeforeNet), (Get-SpreadPercent $AfterNet)) + 100.0 * $EmptySpreadInstructions / $meanBefore
    $threshold = [Math]::Max($NoiseMultiple * $noise, $MinPercent)
    $verdict = if ($delta -le -$threshold) { 'GO' } elseif ($delta -ge $threshold) { 'NO-GO (regression beyond noise)' } else { 'NO-GO (within noise)' }
    return [PSCustomObject]@{ Refused = $false; Reason = $null; Verdict = $verdict; DeltaPercent = $delta; NoisePercent = $noise; ThresholdPercent = $threshold }
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
    param([Parameter(Mandatory)]$Report, [Parameter(Mandatory)][int[]]$Pids)
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
    }
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
    # Alternating order: round 1 runs Before first, round 2 After first, and so on; each side runs an empty-list and a full-list job per round.
    param([Parameter(Mandatory)][int]$Repeats)
    $plan = @()
    for ($round = 1; $round -le $Repeats; $round++) {
        $sides = if ($round % 2 -eq 1) { @('before', 'after') } else { @('after', 'before') }
        foreach ($side in $sides) {
            foreach ($kind in 'empty', 'full') { $plan += [PSCustomObject]@{ Round = $round; Side = $side; Kind = $kind } }
        }
    }
    return $plan
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
    # 0 every grammar GO, 10 any NO-GO, 3 any grammar refused (refusal wins), 2 is reserved for whole-run refusals.
    param([Parameter(Mandatory)][string[]]$Verdicts)
    if ($Verdicts -contains 'REFUSED') { return 3 }
    if (@($Verdicts | Where-Object { $_ -ne 'GO' }).Count -gt 0) { return 10 }
    return 0
}