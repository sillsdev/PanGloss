# Pure logic for calibration-set.ps1 and the stored-set report of retune-parser-work.ps1: nothing here starts a process.

. "$PSScriptRoot\_retune-parser-work.ps1"

# A ladder rung more than this percent off its family mean ratio flags the family; why 7: docs/design/parse-cost.md, "The standing calibration set".
$script:DefaultDriftPercent = 7.0
# The real-grammar guard fails a candidate whose mean scale-free error is this many points above the current weights'.
$script:RealGuardMarginPoints = 1.0
# 'wasted' and 'band' groups mix drivers, so they are reported but are not ladder rungs (docs/design/parse-cost.md).
$script:NonLadderRungs = @('wasted', 'band')
# A pair is judged on the families whose measured total moved at least this many percent (a sign is not read off noise-sized moves).
$script:PairSignFloorPercent = 1.0
# A pair's mean change error limit in points (why 1: docs/design/parse-cost.md), and the margin a candidate may add to the current weights'.
$script:PairMeanLimitPoints = 1.0
$script:PairMarginPoints = 0.5

function Read-CalibrationManifest {
    # The manifest and, for every set, its counts rows (each tagged with the set name, family and rung).
    param([Parameter(Mandatory)][string]$Dir)
    $path = Join-Path $Dir 'manifest.json'
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "calibration manifest not found: $path" }
    $doc = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
    if ($doc.version -ne 1) { throw "calibration manifest needs `"version`": 1 (got $($doc.version))" }
    $sets = @()
    foreach ($s in @($doc.sets)) {
        $file = Join-Path $Dir "$($s.name).counts.tsv"
        if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "set '$($s.name)': counts file is missing: $file" }
        $rungs = @($s.rungs)
        $rows = @(Read-CountsFile $file | ForEach-Object {
                $rung = if ($_.Idx -lt $rungs.Count) { [string]$rungs[$_.Idx] } else { '' }
                $_ | Add-Member -NotePropertyName Set -NotePropertyValue ([string]$s.name) -PassThru | Add-Member -NotePropertyName Family -NotePropertyValue ([string]$s.family) -PassThru | Add-Member -NotePropertyName Rung -NotePropertyValue $rung -PassThru
            })
        if ($rows.Count -ne [int]$s.rows) { throw "set '$($s.name)': the manifest says $($s.rows) row(s), the counts file has $($rows.Count)" }
        $exclude = ($s.PSObject.Properties.Name -contains 'excludeFromMean') -and [bool]$s.excludeFromMean
        $sets += [PSCustomObject]@{ Name = [string]$s.name; Family = [string]$s.family; Rows = $rows; ExcludeFromMean = $exclude; Gap = $(if ($s.PSObject.Properties.Name -contains 'modellingGap') { [string]$s.modellingGap } else { $null }) }
    }
    return [PSCustomObject]@{ Manifest = $doc; Sets = $sets }
}

function Get-RungOrder {
    # Ladder rungs by size (1x, 4x, 16x ... then 1k, 5k ...), then the non-ladder ones.
    param([Parameter(Mandatory)][string]$Rung)
    if ($Rung -in $script:NonLadderRungs) { return [double]::MaxValue / 2 + [Array]::IndexOf($script:NonLadderRungs, $Rung) }
    if ($Rung -match '^(\d+(?:\.\d+)?)([xk])$') { return [double]$Matches[1] }
    return [double]::MaxValue
}

function Get-GeometricMean {
    param([Parameter(Mandatory)][double[]]$Values)
    if ($Values.Count -eq 0) { return $null }
    $sum = 0.0
    foreach ($v in $Values) { $sum += [Math]::Log($v) }
    return [Math]::Exp($sum / $Values.Count)
}

function Get-LogSlope {
    # Least-squares slope of ln(y) on ln(x); $null for fewer than two points or no spread in x.
    param([Parameter(Mandatory)][double[]]$X, [Parameter(Mandatory)][double[]]$Y)
    if ($X.Count -lt 2) { return $null }
    $lx = @($X | ForEach-Object { [Math]::Log($_) }); $ly = @($Y | ForEach-Object { [Math]::Log($_) })
    $mx = ($lx | Measure-Object -Average).Average; $my = ($ly | Measure-Object -Average).Average
    $sxy = 0.0; $sxx = 0.0
    for ($i = 0; $i -lt $lx.Count; $i++) { $sxy += ($lx[$i] - $mx) * ($ly[$i] - $my); $sxx += ($lx[$i] - $mx) * ($lx[$i] - $mx) }
    if ($sxx -le 1e-12) { return $null }
    return $sxy / $sxx
}

function Get-FamilyScore {
    # Per-word error and per-rung linearity of the weights on one family; rows under MinScoredWork are left out of both, as a fit leaves them out.
    param([Parameter(Mandatory)][string]$Family, [Parameter(Mandatory)]$Rows, [Parameter(Mandatory)][double[]]$Weights, [double]$DriftPercent = $script:DefaultDriftPercent, [double]$Scale = 1.0)
    $scored = @($Rows | Where-Object { $_.Work -ge $script:MinScoredWork -and $_.Instructions -gt 0 })
    $result = [ordered]@{ Family = $Family; Rows = @($Rows).Count; Scored = $scored.Count; MeanError = $null; RmsError = $null; ScaleFreeMeanError = $null; Bias = $null; Ratio = $null; Rungs = @(); Slope = $null; MaxDrift = $null; Flagged = $false; Note = $null }
    if ($scored.Count -eq 0) { $result.Note = 'no scored rows'; return [PSCustomObject]$result }
    $errs = @(); $errsFree = @(); $cost = 0.0; $instr = 0.0
    foreach ($r in $scored) {
        $c = Get-WeightedCost -Row $r -Weights $Weights
        $errs += [Math]::Abs($c / $r.Instructions - 1)
        $errsFree += [Math]::Abs($c / ($r.Instructions * $Scale) - 1)
        $cost += $c; $instr += $r.Instructions
    }
    $result.MeanError = 100 * ($errs | Measure-Object -Average).Average
    $result.RmsError = 100 * [Math]::Sqrt((($errs | ForEach-Object { $_ * $_ } | Measure-Object -Sum).Sum) / $errs.Count)
    $result.ScaleFreeMeanError = 100 * ($errsFree | Measure-Object -Average).Average
    $result.Ratio = $cost / $instr
    $result.Bias = 100 * ($cost / $instr - 1)
    $rungs = @()
    foreach ($g in @($scored | Group-Object Rung | Sort-Object { Get-RungOrder $_.Name })) {
        $gc = 0.0; $gi = 0.0
        foreach ($r in $g.Group) { $gc += Get-WeightedCost -Row $r -Weights $Weights; $gi += $r.Instructions }
        $rungs += [PSCustomObject]@{ Rung = $g.Name; Words = $g.Count; Instructions = $gi; Cost = $gc; Ratio = $gc / $gi; Ladder = ($g.Name -notin $script:NonLadderRungs -and $g.Name -ne ''); Drift = $null }
    }
    $ladder = @($rungs | Where-Object Ladder)
    if ($ladder.Count -ge 2) {
        $base = Get-GeometricMean -Values ([double[]]@($ladder | ForEach-Object Ratio))
        foreach ($r in $rungs) { $r.Drift = 100 * ($r.Ratio / $base - 1) }
        $result.MaxDrift = ($ladder | ForEach-Object { [Math]::Abs($_.Drift) } | Measure-Object -Maximum).Maximum
        $result.Flagged = $result.MaxDrift -gt $DriftPercent
        $means = [double[]]@($ladder | ForEach-Object { $_.Cost / $_.Words })
        $ir = [double[]]@($ladder | ForEach-Object { $_.Instructions / $_.Words })
        if (($means | Measure-Object -Maximum).Maximum / ($means | Measure-Object -Minimum).Minimum -ge 1.05) { $result.Slope = Get-LogSlope -X $means -Y $ir } else { $result.Note = 'cost is flat along the ladder (a slope needs it to move)' }
    } elseif ($ladder.Count -eq 1) { $result.Note = 'one ladder rung: no linearity to read' }
    else { $result.Note = 'no ladder rungs' }
    $result.Rungs = $rungs
    return [PSCustomObject]$result
}

function Get-SetScore {
    # Per-family scores over a manifest, with the set scale for the scale-free error; an excludeFromMean family is scored but not in the means.
    param([Parameter(Mandatory)]$Sets, [Parameter(Mandatory)][double[]]$Weights, [double]$DriftPercent = $script:DefaultDriftPercent)
    $families = [ordered]@{}
    $excluded = @{}
    foreach ($s in $Sets) {
        if (-not $families.Contains($s.Family)) { $families[$s.Family] = @() }
        $families[$s.Family] += @($s.Rows)
        if ($s.PSObject.Properties.Name -contains 'ExcludeFromMean' -and $s.ExcludeFromMean) { $excluded[$s.Family] = if ($s.Gap) { $s.Gap } else { 'excluded from the means' } }
    }
    $first = @($families.Keys | ForEach-Object { Get-FamilyScore -Family $_ -Rows $families[$_] -Weights $Weights -DriftPercent $DriftPercent })
    $ratios = [double[]]@($first | Where-Object { $null -ne $_.Ratio -and -not $excluded.ContainsKey($_.Family) } | ForEach-Object Ratio)
    $scale = if ($ratios.Count -gt 0) { Get-GeometricMean -Values $ratios } else { 1.0 }
    $scores = @($families.Keys | ForEach-Object {
            $f = Get-FamilyScore -Family $_ -Rows $families[$_] -Weights $Weights -DriftPercent $DriftPercent -Scale $scale
            $f | Add-Member -NotePropertyName ExcludedFromMean -NotePropertyValue $excluded.ContainsKey($_) -PassThru | Add-Member -NotePropertyName Gap -NotePropertyValue $(if ($excluded.ContainsKey($_)) { $excluded[$_] } else { $null }) -PassThru
        })
    $withError = @($scores | Where-Object { $null -ne $_.MeanError -and -not $_.ExcludedFromMean })
    return [PSCustomObject]@{
        Weights = $Weights; Scale = $scale; Families = $scores
        MeanOfFamilyErrors = if ($withError.Count) { ($withError | ForEach-Object MeanError | Measure-Object -Average).Average } else { $null }
        MeanOfScaleFreeErrors = if ($withError.Count) { ($withError | ForEach-Object ScaleFreeMeanError | Measure-Object -Average).Average } else { $null }
        Worst = if ($withError.Count) { ($withError | Sort-Object MeanError -Descending | Select-Object -First 1).Family } else { $null }
        Flagged = @($scores | Where-Object Flagged | ForEach-Object Family)
    }
}

function Read-RealTotals {
    # Per-grammar totals {name, rows, scored, work, bytes, registers, allocations, instructions}: the real-grammar guard, stored without per-word rows.
    param([Parameter(Mandatory)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "real-grammar totals not found: $Path" }
    $doc = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    if ($doc.version -ne 1) { throw "real-grammar totals need `"version`": 1 (got $($doc.version))" }
    return $doc
}

function New-RealTotals {
    # Totals of word-free counts files over rows at or above MinScoredWork. `Files` maps a grammar name to its counts file.
    param([Parameter(Mandatory)][System.Collections.IDictionary]$Files)
    $out = @()
    foreach ($name in $Files.Keys) {
        $rows = @(Read-CountsFile $Files[$name])
        $scored = @($rows | Where-Object { $_.Work -ge $script:MinScoredWork -and $_.Instructions -gt 0 })
        $sum = { param($f) [long](($scored | ForEach-Object { $_.$f } | Measure-Object -Sum).Sum) }
        $out += [ordered]@{ name = $name; rows = $rows.Count; scored = $scored.Count; work = (& $sum 'Work'); bytes = (& $sum 'Bytes'); registers = (& $sum 'Registers'); allocations = (& $sum 'Allocations'); instructions = (& $sum 'Instructions') }
    }
    return $out
}

function Get-RealGuard {
    # Each grammar's total predicted cost against its total measured instructions, and the mean of the absolute errors.
    param([Parameter(Mandatory)]$Totals, [Parameter(Mandatory)][double[]]$Weights)
    $grammars = @($Totals.grammars | ForEach-Object {
            $cost = $_.work * $Weights[0] + $_.bytes * $Weights[1] + $_.registers * $Weights[2] + $_.allocations * $Weights[3]
            $ratio = $cost / $_.instructions
            [PSCustomObject]@{ Name = $_.name; Scored = $_.scored; Ratio = $ratio; ErrorPoints = 100 * [Math]::Abs($ratio - 1) }
        })
    $scale = Get-GeometricMean -Values ([double[]]@($grammars | ForEach-Object Ratio))
    foreach ($g in $grammars) { $g | Add-Member -NotePropertyName ScaleFreeErrorPoints -NotePropertyValue (100 * [Math]::Abs($g.Ratio / $scale - 1)) }
    return [PSCustomObject]@{ Grammars = $grammars; MeanError = ($grammars | ForEach-Object ErrorPoints | Measure-Object -Average).Average; MeanScaleFreeError = ($grammars | ForEach-Object ScaleFreeErrorPoints | Measure-Object -Average).Average; Spread = (($grammars | ForEach-Object Ratio | Measure-Object -Maximum).Maximum / ($grammars | ForEach-Object Ratio | Measure-Object -Minimum).Minimum) }
}

function Get-RealGuardVerdict {
    # PASS unless the candidate's mean scale-free error is over RealGuardMarginPoints above the current weights'; level is not gated.
    param([Parameter(Mandatory)]$Current, [Parameter(Mandatory)]$Candidate)
    $worse = $Candidate.MeanScaleFreeError - $Current.MeanScaleFreeError
    if ($worse -gt $script:RealGuardMarginPoints) { return [PSCustomObject]@{ Verdict = 'WORSE'; Points = $worse } }
    return [PSCustomObject]@{ Verdict = 'PASS'; Points = $worse }
}

function Format-FamilyTable {
    param([Parameter(Mandatory)]$Score)
    $lines = @()
    $lines += ('{0,-26} {1,5} {2,7} {3,9} {4,9} {5,10} {6,8} {7,10} {8,6} {9}' -f 'family', 'rows', 'scored', 'mean err%', 'rms err%', 'shape err%', 'cost/Ir', 'max drift%', 'slope', 'linear')
    foreach ($f in $Score.Families) {
        $fmt = { param($v, $p) if ($null -eq $v) { 'n/a' } else { $v.ToString($p) } }
        $verdict = if ($null -eq $f.MaxDrift) { 'n/a' } elseif ($f.Flagged) { 'NON-LINEAR' } else { 'ok' }
        if ($f.ExcludedFromMean) { $verdict += ' (not in means)' }
        $lines += ('{0,-26} {1,5} {2,7} {3,9} {4,9} {5,10} {6,8} {7,10} {8,6} {9}' -f $f.Family, $f.Rows, $f.Scored, (& $fmt $f.MeanError '0.0'), (& $fmt $f.RmsError '0.0'), (& $fmt $f.ScaleFreeMeanError '0.0'), (& $fmt $f.Ratio '0.000'), (& $fmt $f.MaxDrift '0.0'), (& $fmt $f.Slope '0.00'), $verdict)
    }
    $lines += ('mean of family errors {0:0.0}%  (shape {1:0.0}%), set scale cost/Ir {2:0.000}, worst family {3}' -f $Score.MeanOfFamilyErrors, $Score.MeanOfScaleFreeErrors, $Score.Scale, $Score.Worst)
    foreach ($f in @($Score.Families | Where-Object ExcludedFromMean)) { $lines += ('not in the means: {0}: {1}' -f $f.Family, $f.Gap) }
    return $lines
}

function Format-RungTable {
    param([Parameter(Mandatory)]$Score)
    $lines = @()
    foreach ($f in $Score.Families) {
        if (@($f.Rungs).Count -lt 2) { continue }
        $parts = @($f.Rungs | ForEach-Object { if ($null -eq $_.Drift) { '{0}:n/a' -f $_.Rung } else { '{0}:{1:+0.0;-0.0;0.0}{2}' -f $_.Rung, $_.Drift, $(if ($_.Ladder) { '' } else { '*' }) } })
        $lines += ('{0,-26} {1}' -f $f.Family, ($parts -join '  '))
    }
    if ($lines.Count -gt 0) { $lines += '(drift of each rung''s cost/Ir from the family''s mean rung ratio, in percent; * is not a ladder rung and does not flag)' }
    return $lines
}

# ---- Change pairs: the same words through a before and an after, scored on the CHANGE of each family's total ----

function Read-CalibrationPairs {
    # The pairs a manifest lists (`pairs`: [{name, file}], files relative to the calibration folder); none listed is none.
    param([Parameter(Mandatory)][string]$Dir)
    $path = Join-Path $Dir 'manifest.json'
    $doc = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
    $out = @()
    if ($doc.PSObject.Properties.Name -notcontains 'pairs') { return $out }
    foreach ($p in @($doc.pairs)) {
        $file = Join-Path $Dir ([string]$p.file)
        if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "pair '$($p.name)': file is missing: $file" }
        $pair = Get-Content -LiteralPath $file -Raw | ConvertFrom-Json
        if ($pair.version -ne 1) { throw "pair '$($p.name)' needs `"version`": 1 (got $($pair.version))" }
        if ([string]$pair.name -ne [string]$p.name) { throw "pair file $file is named '$($pair.name)', the manifest lists '$($p.name)'" }
        if (@($pair.families).Count -eq 0) { throw "pair '$($p.name)' has no families" }
        $out += $pair
    }
    return $out
}

function New-PairFamilies {
    # Per-family totals of both sides over the words both parsed that reach the work floor on either side; no word text is kept.
    param([Parameter(Mandatory)]$Lists)
    $fields = 'Work', 'Bytes', 'Registers', 'Allocations', 'Instructions'
    $acc = [ordered]@{}
    foreach ($l in $Lists) {
        $after = @{}; foreach ($r in @($l.After)) { $after[[int]$r.Idx] = $r }
        foreach ($b in @($l.Before)) {
            $a = $after[[int]$b.Idx]
            if ($null -eq $a -or $b.Instructions -le 0 -or $a.Instructions -le 0) { continue }
            if ($b.Work -lt $script:MinScoredWork -and $a.Work -lt $script:MinScoredWork) { continue }
            if (-not $acc.Contains($l.Family)) { $acc[$l.Family] = [ordered]@{ words = 0; before = [ordered]@{}; after = [ordered]@{} }; foreach ($f in $fields) { $acc[$l.Family].before[$f.ToLowerInvariant()] = [long]0; $acc[$l.Family].after[$f.ToLowerInvariant()] = [long]0 } }
            $acc[$l.Family].words++
            foreach ($f in $fields) { $acc[$l.Family].before[$f.ToLowerInvariant()] += [long]$b.$f; $acc[$l.Family].after[$f.ToLowerInvariant()] += [long]$a.$f }
        }
    }
    return @($acc.Keys | ForEach-Object { [ordered]@{ family = $_; words = $acc[$_].words; before = $acc[$_].before; after = $acc[$_].after } })
}

function Get-TotalsCost($Totals, [double[]]$Weights) {
    return $Totals.work * $Weights[0] + $Totals.bytes * $Weights[1] + $Totals.registers * $Weights[2] + $Totals.allocations * $Weights[3]
}

function Get-PairScore {
    # Per family: predicted change of the total against measured change in points; a sign is read only when the family moved PairSignFloorPercent.
    param([Parameter(Mandatory)]$Pair, [Parameter(Mandatory)][double[]]$Weights)
    $rows = @($Pair.families | ForEach-Object {
            $predicted = 100 * ((Get-TotalsCost $_.after $Weights) / (Get-TotalsCost $_.before $Weights) - 1)
            $measured = 100 * ($_.after.instructions / $_.before.instructions - 1)
            $checked = [Math]::Abs($measured) -ge $script:PairSignFloorPercent
            [PSCustomObject]@{ Family = [string]$_.family; Words = [int]$_.words; Predicted = $predicted; Measured = $measured; ErrorPoints = [Math]::Abs($predicted - $measured); SignChecked = $checked; SignOk = (-not $checked) -or ([Math]::Sign($predicted) -eq [Math]::Sign($measured)) }
        })
    $checkedRows = @($rows | Where-Object SignChecked)
    return [PSCustomObject]@{
        Name = [string]$Pair.name; Title = [string]$Pair.title; Weights = $Weights; Families = $rows
        Gates = (-not ($Pair.PSObject.Properties.Name -contains 'gate')) -or [bool]$Pair.gate
        GateNote = $(if ($Pair.PSObject.Properties.Name -contains 'gateNote') { [string]$Pair.gateNote } else { $null })
        MeanError = ($rows | ForEach-Object ErrorPoints | Measure-Object -Average).Average
        WorstError = ($rows | ForEach-Object ErrorPoints | Measure-Object -Maximum).Maximum
        Worst = ($rows | Sort-Object ErrorPoints -Descending | Select-Object -First 1).Family
        SignChecked = $checkedRows.Count; SignAgree = @($checkedRows | Where-Object SignOk).Count
    }
}

function Get-PairGate {
    # The standing gate on one pair at one weight set: mean change error within PairMeanLimitPoints. A report-only pair gates nothing.
    param([Parameter(Mandatory)]$Score)
    if (-not $Score.Gates) { return [PSCustomObject]@{ Pass = $true; Verdict = 'REPORT-ONLY' } }
    $ok = $Score.MeanError -le $script:PairMeanLimitPoints
    return [PSCustomObject]@{ Pass = $ok; Verdict = $(if ($ok) { 'PASS' } else { 'FAIL' }) }
}

function Get-PairVerdict {
    # A candidate fails a pair when its mean change error is more than PairMarginPoints above the current weights', or it loses a sign.
    param([Parameter(Mandatory)]$Current, [Parameter(Mandatory)]$Candidate)
    $worse = $Candidate.MeanError - $Current.MeanError
    if (-not $Candidate.Gates) { return [PSCustomObject]@{ Verdict = 'REPORT-ONLY'; Points = $worse; LostSign = $false } }
    $lostSign = $Candidate.SignAgree -lt $Current.SignAgree
    $verdict = if ($worse -gt $script:PairMarginPoints -or $lostSign) { 'WORSE' } else { 'PASS' }
    return [PSCustomObject]@{ Verdict = $verdict; Points = $worse; LostSign = $lostSign }
}

function Format-PairTable {
    param([Parameter(Mandatory)]$Score)
    $lines = @()
    $lines += ('pair {0}: {1}' -f $Score.Name, $Score.Title)
    if (-not $Score.Gates) { $lines += ('  report-only, not a gate: {0}' -f $Score.GateNote) }
    $lines += ('  {0,-26} {1,6} {2,10} {3,10} {4,10} {5}' -f 'family', 'words', 'predicted%', 'measured%', 'error pts', 'sign')
    foreach ($f in $Score.Families) {
        $sign = if (-not $f.SignChecked) { '-' } elseif ($f.SignOk) { 'ok' } else { 'WRONG' }
        $lines += ('  {0,-26} {1,6} {2,10} {3,10} {4,10} {5}' -f $f.Family, $f.Words, $f.Predicted.ToString('+0.00;-0.00;0.00'), $f.Measured.ToString('+0.00;-0.00;0.00'), $f.ErrorPoints.ToString('0.00'), $sign)
    }
    $lines += ('  mean change error {0:0.00} points, worst {1:0.00} ({2}); sign right on {3} of {4} families that moved {5}% or more' -f $Score.MeanError, $Score.WorstError, $Score.Worst, $Score.SignAgree, $Score.SignChecked, $script:PairSignFloorPercent)
    return $lines
}
