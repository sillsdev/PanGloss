# Pure logic for instruction-compare.ps1: nothing here starts a process or touches a trace, so rust/tools/tests/instruction-compare.tests.ps1 can cover it.

function Read-BatchSignatures {
    # idx -> "status<TAB>signature" for every result row of a batch TSV; the word column is never kept.
    param([Parameter(Mandatory)][string]$Path)
    $rows = @{}
    foreach ($line in [System.IO.File]::ReadLines($Path)) {
        $f = $line.Split("`t")
        if ($f.Count -lt 5 -or $f[2] -eq 'STARTED') { continue }
        $rows[[int]$f[0]] = "$($f[3])`t$($f[4])"
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
        [double]$MinPercent = 0.5
    )
    $refuse = { param($why) [PSCustomObject]@{ Refused = $true; Reason = $why; Verdict = 'REFUSED'; DeltaPercent = $null; NoisePercent = $null; ThresholdPercent = $null } }
    if ($DifferingWords -gt 0) { return & $refuse "$DifferingWords word(s) differ in status or signature between the binaries; an instruction count compares only identical parses" }
    if ($BeforeNet.Count -lt 2 -or $AfterNet.Count -lt 2) { return & $refuse 'each side needs at least 2 measured runs to measure run-to-run noise' }
    $meanBefore = ($BeforeNet | Measure-Object -Average).Average
    $meanAfter = ($AfterNet | Measure-Object -Average).Average
    if ($meanBefore -le 0 -or $meanAfter -le 0) { return & $refuse 'a side counted no instructions beyond startup, so the word list gave it nothing to measure' }
    $delta = 100.0 * ($meanAfter - $meanBefore) / $meanBefore
    $noise = [Math]::Max((Get-SpreadPercent $BeforeNet), (Get-SpreadPercent $AfterNet))
    $threshold = [Math]::Max($NoiseMultiple * $noise, $MinPercent)
    $verdict = if ($delta -le -$threshold) { 'GO' } elseif ($delta -ge $threshold) { 'NO-GO (regression beyond noise)' } else { 'NO-GO (within noise)' }
    return [PSCustomObject]@{ Refused = $false; Reason = $null; Verdict = $verdict; DeltaPercent = $delta; NoisePercent = $noise; ThresholdPercent = $threshold }
}

function Assert-PmcTasksPresent {
    param([string[]]$Found = @(), [string[]]$Required = @('PanGloss PMC Start', 'PanGloss PMC Stop'), [string]$TaskPath = '\PanGloss\')
    $missing = @($Required | Where-Object { $Found -notcontains $_ })
    if ($missing.Count -gt 0) {
        throw "PMC scheduled task(s) absent under ${TaskPath}: $($missing -join ', '). Without them no instruction count exists; they are set up per the PMC tasks note, and this tool will not fall back to wall time or work_steps."
    }
}

function Assert-TraceIntact {
    # $Report is the JSON object instruction_compare_etl.py prints; $Pids are the processes whose counts are used.
    param([Parameter(Mandatory)]$Report, [Parameter(Mandatory)][int[]]$Pids)
    if ($Report.rc -ne 0) { throw "trace processing returned $($Report.rc); the trace is unreadable" }
    if ($Report.events_lost -gt 0 -or $Report.buffers_lost -gt 0) {
        throw "trace lost events ($($Report.events_lost) events, $($Report.buffers_lost) buffers); the counts would be short by an unknown amount"
    }
    if ($Report.switches -le 0 -or $Report.counters -lt 1) { throw 'trace carries no counter values on context switches; the PMC tasks did not record hardware counters' }
    if ($Report.gaps_on_target -gt 0) { throw "$($Report.gaps_on_target) context-switch gap(s) touched a measured process; its instruction count is incomplete" }
    foreach ($p in $Pids) {
        if ([double]$Report.instructions."$p" -le 0) { throw "no instructions attributed to measured pid $p; the trace missed the process" }
    }
}

function Get-CallgrindTotal {
    param([Parameter(Mandatory)][string[]]$Lines)
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
