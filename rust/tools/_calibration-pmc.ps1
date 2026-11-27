# Report-only PMC rows and arithmetic; the callgrind scorer never consumes these instructions.

function Read-CalibrationPmcRows {
    param([string]$Path, $Rows, [int]$Rounds)
    if ($Rounds -lt 3) { throw 'PMC needs at least three rounds to report spread' }
    $byIndex = @{}; foreach ($r in $Rows) { $byIndex[[int]$r.Idx] = @{} }
    foreach ($line in [IO.File]::ReadAllLines($Path)) {
        if ($line -notmatch '^(\d+)\t(\d+)\t([1-9]\d*)$') { throw "invalid PMC row in $Path; expected idx, round (zero-based), positive instructions" }
        $idx = [int]$Matches[1]; $round = [int]$Matches[2]; $count = [long]$Matches[3]
        if (-not $byIndex.ContainsKey($idx)) { throw "PMC index $idx is absent from callgrind rows" }
        if ($round -ge $Rounds) { throw "PMC round $round exceeds the manifest round count" }
        if ($byIndex[$idx].ContainsKey($round)) { throw "duplicate PMC index $idx round $round" }
        $byIndex[$idx][$round] = $count
    }
    foreach ($r in $Rows) {
        $values = $byIndex[[int]$r.Idx]
        if ($values.Count -ne $Rounds) { throw "PMC index $($r.Idx) has $($values.Count) rounds, expected $Rounds" }
        [PSCustomObject]@{ Idx = [int]$r.Idx; Samples = [double[]]@(0..($Rounds - 1) | ForEach-Object { $values[$_] }) }
    }
}

function Write-CalibrationPmcFile {
    param([string]$Path, [string[]]$Lines, $Rows, [int]$Rounds)
    [IO.File]::WriteAllText($Path, ($Lines -join "`n") + "`n")
    $null = @(Read-CalibrationPmcRows -Path $Path -Rows $Rows -Rounds $Rounds)
}

function Get-PmcSpread {
    param([double[]]$Values)
    if ($Values.Count -lt 3 -or @($Values | Where-Object { $_ -le 0 -or -not [double]::IsFinite($_) }).Count) { throw 'PMC spread needs at least three finite positive counts' }
    $mean = ($Values | Measure-Object -Average).Average
    $variance = (($Values | ForEach-Object { ($_ - $mean) * ($_ - $mean) } | Measure-Object -Sum).Sum) / ($Values.Count - 1)
    [PSCustomObject]@{ Mean = $mean; CvPercent = 100 * [Math]::Sqrt($variance) / $mean; RangePercent = 100 * (($Values | Measure-Object -Maximum).Maximum - ($Values | Measure-Object -Minimum).Minimum) / $mean }
}

function Get-PmcPhaseCounts {
    param($Report, [int[]]$Pids, [int]$Words = 0)
    Assert-TraceIntact -Report $Report -Pids $Pids -RequirePhase:($Words -eq 0)
    foreach ($id in $Pids) {
        if ($Words -gt 0 -and [int]$Report.phase."$id" -ne 2) { throw "pid $id has incomplete per-word phase marks; expected $Words pairs of marker thread lifetimes" }
        $value = [double]$Report.parse_instructions."$id"
        if (-not [double]::IsFinite($value) -or $value -le 0) { throw "invalid PMC phase count for pid $id" }
        if ($Words -eq 0) { [long]$value; continue }
        $counts = @($Report.word_instructions."$id")
        if ($counts.Count -ne $Words) { throw "pid $id has $($counts.Count) word phases, expected $Words" }
        if (($counts | Measure-Object -Sum).Sum -ne $value) { throw "pid $id word phases do not sum to its parse phase count" }
        foreach ($count in $counts) {
            if (-not [double]::IsFinite([double]$count) -or $count -le 0) { throw "invalid PMC word count for pid $id" }
            [long]$count
        }
    }
}

function Get-CalibrationPmcReport {
    param([string]$Dir, $Calibration, [double[]]$Weights)
    try {
        $report = Get-ValidatedCalibrationPmcReport -Dir $Dir -Calibration $Calibration -Weights $Weights
        $status = if (-not $report.Families.Count) { 'unavailable' } elseif ($report.Unavailable.Count) { 'partial' } else { 'available' }
        $report | Add-Member -NotePropertyName Status -NotePropertyValue $status
        $report | Add-Member -NotePropertyName Errors -NotePropertyValue @()
        return $report
    } catch {
        # Invalid optional data must remain visible without suppressing the authoritative callgrind verdict.
        return [PSCustomObject]@{ ReportOnly = $true; Status = 'invalid'; Errors = @($_.Exception.Message); Weights = $Weights; Families = @(); Sets = @(); Unavailable = @($Calibration.Sets | ForEach-Object Name); Scale = $null; Disagree = @() }
    }
}

function Get-ValidatedCalibrationPmcReport {
    param([string]$Dir, $Calibration, [double[]]$Weights)
    $families = [ordered]@{}; $unavailable = @(); $setSpreads = @()
    $manifest = $Calibration.Manifest
    if ($manifest.PSObject.Properties.Name -notcontains 'pmc') { return [PSCustomObject]@{ ReportOnly = $true; Weights = $Weights; Families = @(); Sets = @(); Unavailable = @($Calibration.Sets | ForEach-Object Name); Scale = $null; Disagree = @() } }
    foreach ($s in $Calibration.Sets) {
        $pin = @($manifest.pmc.sets | Where-Object name -eq $s.Name)
        $path = Join-Path $Dir "$($s.Name).pmc.tsv"
        if ($pin.Count -eq 0 -or -not (Test-Path -LiteralPath $path)) { $unavailable += $s.Name; continue }
        if ($pin.Count -ne 1) { throw "duplicate PMC manifest entry for $($s.Name)" }
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $pin[0].sha256) { throw "PMC file hash differs for $($s.Name)" }
        $samples = @(Read-CalibrationPmcRows -Path $path -Rows $s.Rows -Rounds $pin[0].rounds)
        $totals = [double[]]@(for ($i = 0; $i -lt $pin[0].rounds; $i++) { ($samples | ForEach-Object { $_.Samples[$i] } | Measure-Object -Sum).Sum })
        $spread = Get-PmcSpread $totals
        $setSpreads += [PSCustomObject]@{ Set = $s.Name; Rows = $s.Rows.Count; Rounds = $pin[0].rounds; CvPercent = $spread.CvPercent; RangePercent = $spread.RangePercent; RoundInstructions = $totals }
        $byIndex = @{}; foreach ($p in $samples) { $byIndex[$p.Idx] = $p.Samples }
        if (-not $families.Contains($s.Family)) { $families[$s.Family] = @() }
        foreach ($r in $s.Rows) {
            $copy = $r.PSObject.Copy()
            $spread = Get-PmcSpread $byIndex[[int]$r.Idx]
            $copy | Add-Member -NotePropertyName CallgrindInstructions -NotePropertyValue $r.Instructions
            $copy.Instructions = $spread.Mean
            $copy | Add-Member -NotePropertyName Samples -NotePropertyValue $byIndex[[int]$r.Idx]
            $copy | Add-Member -NotePropertyName CvPercent -NotePropertyValue $spread.CvPercent
            $families[$s.Family] += $copy
        }
    }
    $out = @()
    foreach ($name in $families.Keys) {
        $rows = @($families[$name])
        $score = Get-FamilyScore -Family $name -Rows $rows -Weights $Weights
        $cg = ($rows | Measure-Object CallgrindInstructions -Sum).Sum
        $pmc = ($rows | Measure-Object Instructions -Sum).Sum
        $rungs = @($rows | Group-Object Rung | ForEach-Object {
            [PSCustomObject]@{ Rung = $_.Name; Ratio = (($_.Group | Measure-Object Instructions -Sum).Sum / ($_.Group | Measure-Object CallgrindInstructions -Sum).Sum); Ladder = ($_.Name -ne '' -and $_.Name -notin $script:NonLadderRungs) }
        })
        $ladder = @($rungs | Where-Object Ladder)
        $drift = $null
        if ($ladder.Count -ge 2) {
            $center = Get-GeometricMean ([double[]]@($ladder | ForEach-Object Ratio))
            $drift = ($ladder | ForEach-Object { 100 * [Math]::Abs($_.Ratio / $center - 1) } | Measure-Object -Maximum).Maximum
        }
        $rounds = @($rows[0].Samples).Count
        if (@($rows | Where-Object { @($_.Samples).Count -ne $rounds }).Count) { throw "PMC family $name mixes round counts; remeasure all its sets together" }
        $totals = [double[]]@(for ($i = 0; $i -lt $rounds; $i++) { ($rows | ForEach-Object { $_.Samples[$i] } | Measure-Object -Sum).Sum })
        $spread = Get-PmcSpread $totals
        $out += [PSCustomObject]@{ Family = $name; Rows = $rows.Count; Scored = $score.Scored; Rounds = $rounds; Ratio = $pmc / $cg; RelativePercent = $null; MaxDriftPercent = $drift; MeanError = $score.MeanError; Bias = $score.Bias; CvPercent = $spread.CvPercent; RangePercent = $spread.RangePercent; MaxWordCvPercent = ($rows | Measure-Object CvPercent -Maximum).Maximum; Disagree = $false; Rungs = $rungs }
    }
    $scale = if ($out.Count) { Get-GeometricMean ([double[]]@($out | ForEach-Object Ratio)) } else { $null }
    foreach ($f in $out) {
        $f.RelativePercent = 100 * ($f.Ratio / $scale - 1)
        $f.Disagree = [Math]::Abs($f.RelativePercent) -gt 2 -or ($null -ne $f.MaxDriftPercent -and $f.MaxDriftPercent -gt 2)
    }
    [PSCustomObject]@{ ReportOnly = $true; Weights = $Weights; Families = $out; Sets = $setSpreads; Unavailable = $unavailable; Scale = $scale; Disagree = @($out | Where-Object Disagree | ForEach-Object Family) }
}

function Format-CalibrationPmcReport {
    param($Report)
    'PMC REPORT-ONLY (callgrind remains the gating source)'
    "  PMC status: $($Report.Status)"
    foreach ($message in $Report.Errors) { "  PMC integrity error: $message" }
    if ($Report.Status -eq 'invalid') { '  optional PMC results withheld; callgrind scoring is unchanged'; return }
    if (-not $Report.Families.Count) { '  no optional PMC data available'; return }
    ('  current PMC weights: {0}' -f ($Report.Weights -join ' / '))
    ('  family scale PMC/callgrind {0:0.0000}; relative% removes this common cross-platform level' -f $Report.Scale)
    '  family                      rows rounds PMC/CG relative% rung drift% error% round CV% range% max word CV%'
    foreach ($f in $Report.Families) {
        $drift = if ($null -eq $f.MaxDriftPercent) { 'n/a' } else { '{0:0.00}' -f $f.MaxDriftPercent }
        $errorText = if ($null -eq $f.MeanError) { 'n/a' } else { '{0:0.00}' -f $f.MeanError }
        '  {0,-27} {1,4} {2,6} {3,6:0.000} {4,9:+0.00;-0.00;0.00} {5,11} {6,6} {7,9:0.00} {8,6:0.00} {9,12:0.00}{10}' -f $f.Family, $f.Rows, $f.Rounds, $f.Ratio, $f.RelativePercent, $drift, $errorText, $f.CvPercent, $f.RangePercent, $f.MaxWordCvPercent, $(if ($f.Disagree) { ' >2% disagreement (report-only)' } else { '' })
    }
    '  #294: word marks omit batch TSV/signature work, unlike the whole-batch PMC phase, but include marker join/start and kernel work. Ratios also compare Windows and Linux code generation. These are levels/ladder drift, not a before/after regression verdict.'
    '  Spread is sample SD/mean (CV) and (max-min)/mean of round totals; max word CV exposes noisy short words (#260). Error is mean absolute per-word current-weight error on PMC, using the callgrind work floor.'
    '  per-set spread (report-only): set, rounds, CV%, range%'
    foreach ($s in $Report.Sets) { '    {0,-29} {1} {2:0.00} {3:0.00}' -f $s.Set, $s.Rounds, $s.CvPercent, $s.RangePercent }
    if ($Report.Unavailable.Count) { "  optional PMC unavailable: $($Report.Unavailable -join ', ')" }
}
