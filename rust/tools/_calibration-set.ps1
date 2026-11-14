# Pure logic for calibration-set.ps1 and the stored-set report of retune-parser-work.ps1: nothing here starts a process.

. "$PSScriptRoot\_retune-parser-work.ps1"

# A family is flagged when the cost-to-instruction ratio of one of its ladder rungs is more than this percent away from the
# family's own mean rung ratio. Why 15: docs/design/parse-cost.md, "The standing calibration set".
$script:DefaultDriftPercent = 15.0
# The real-grammar guard fails a candidate whose mean per-grammar total error is this many points above the current weights'.
$script:RealGuardMarginPoints = 1.0
# Rungs the words are picked for; 'wasted' (no analysis) and 'band' (a spread, not a rung) are reported but are not ladder rungs.
$script:NonLadderRungs = @('wasted', 'band')

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
        $sets += [PSCustomObject]@{ Name = [string]$s.name; Family = [string]$s.family; Rows = $rows }
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
    # Per-family scores over every set of a manifest, with the set's own scale (the geometric mean of the family ratios) for the scale-free error.
    param([Parameter(Mandatory)]$Sets, [Parameter(Mandatory)][double[]]$Weights, [double]$DriftPercent = $script:DefaultDriftPercent)
    $families = [ordered]@{}
    foreach ($s in $Sets) {
        if (-not $families.Contains($s.Family)) { $families[$s.Family] = @() }
        $families[$s.Family] += @($s.Rows)
    }
    $first = @($families.Keys | ForEach-Object { Get-FamilyScore -Family $_ -Rows $families[$_] -Weights $Weights -DriftPercent $DriftPercent })
    $ratios = [double[]]@($first | Where-Object { $null -ne $_.Ratio } | ForEach-Object Ratio)
    $scale = if ($ratios.Count -gt 0) { Get-GeometricMean -Values $ratios } else { 1.0 }
    $scores = @($families.Keys | ForEach-Object { Get-FamilyScore -Family $_ -Rows $families[$_] -Weights $Weights -DriftPercent $DriftPercent -Scale $scale })
    $withError = @($scores | Where-Object { $null -ne $_.MeanError })
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
    # PASS unless the candidate's mean per-grammar error is more than RealGuardMarginPoints above the current weights'.
    param([Parameter(Mandatory)]$Current, [Parameter(Mandatory)]$Candidate)
    $worse = $Candidate.MeanError - $Current.MeanError
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
        $lines += ('{0,-26} {1,5} {2,7} {3,9} {4,9} {5,10} {6,8} {7,10} {8,6} {9}' -f $f.Family, $f.Rows, $f.Scored, (& $fmt $f.MeanError '0.0'), (& $fmt $f.RmsError '0.0'), (& $fmt $f.ScaleFreeMeanError '0.0'), (& $fmt $f.Ratio '0.000'), (& $fmt $f.MaxDrift '0.0'), (& $fmt $f.Slope '0.00'), $verdict)
    }
    $lines += ('mean of family errors {0:0.0}%  (shape {1:0.0}%), set scale cost/Ir {2:0.000}, worst family {3}' -f $Score.MeanOfFamilyErrors, $Score.MeanOfScaleFreeErrors, $Score.Scale, $Score.Worst)
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
