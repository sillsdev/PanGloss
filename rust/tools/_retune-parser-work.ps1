# Pure logic for retune-parser-work.ps1: nothing here starts a process, so rust/tools/tests/retune-parser-work.tests.ps1 can cover it.

# Names the design and the facts-profile report use; a manifest entry without a `set` takes its set from them.
$script:DefaultFitNames = @('indonesian', 'aweti', 'amharic', 'mbugwe', 'sena')
$script:DefaultHeldOutNames = @('swahili', 'swh', 'cebuano', 'ceb')
# Words under this many work units are dominated by start-up cost; pg_cost::calibrate drops them from a fit, and so does scoring.
$script:MinScoredWork = 1000
# Below this change in instructions a per-word direction is not resolved (docs/design/parse-cost.md, Limits).
$script:DirectionFloorPercent = 2.0
$script:MinHeldOutWords = 8
# A shape gain under this many percentage points of scale-free median error is read as a rescale.
$script:ShapeMarginPoints = 0.1
# The words carry a term only if leaving it out makes the fit's error at least this many times larger.
$script:CarriedErrorRatio = 1.10
# Below this change in a grammar's total instructions the sign of the change is not scored.
$script:ChangeFloorPercent = 0.5
# The mean error of the predicted change may rise by this many points before it counts as worse.
$script:ChangeMarginPoints = 0.5
# A weight that moves by more than this fraction is flagged for the owner (SKILL.md, What an owner must decide).
$script:LargeMoveFraction = 0.25

function Read-RetuneManifest {
    # Entries {Name, Grammar, Words, Set}. A structural problem throws; a file that is absent is for Get-ManifestRefusals.
    param([Parameter(Mandatory)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "manifest not found: $Path" }
    $dir = Split-Path -Parent (Resolve-Path -LiteralPath $Path).Path
    try { $doc = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json } catch { throw "manifest is not valid JSON: $($_.Exception.Message)" }
    if ($doc.version -ne 1) { throw "manifest needs `"version`": 1 (got $($doc.version))" }
    $entries = @($doc.grammars)
    if ($entries.Count -eq 0) { throw 'manifest lists no grammars' }
    $seen = @{}
    $out = @()
    foreach ($e in $entries) {
        foreach ($field in 'name', 'grammar', 'words') {
            if (-not $e.$field) { throw "a manifest grammar entry has no `"$field`"" }
        }
        $name = [string]$e.name
        if ($name -notmatch '^[A-Za-z0-9_.-]+$') { throw "grammar name '$name' must be letters, digits, '_', '.' or '-' (it is printed in results)" }
        if ($seen.ContainsKey($name.ToLowerInvariant())) { throw "grammar name '$name' is listed twice" }
        $seen[$name.ToLowerInvariant()] = $true
        $set = if ($e.set) { ([string]$e.set).ToLowerInvariant() } elseif ($script:DefaultFitNames -contains $name.ToLowerInvariant()) { 'fit' } elseif ($script:DefaultHeldOutNames -contains $name.ToLowerInvariant()) { 'heldout' } else { $null }
        if ($set -notin 'fit', 'heldout') { throw "grammar '$name' needs `"set`": `"fit`" or `"heldout`" (only the default five and Swahili/Cebuano have a default)" }
        $abs = { param($p) if ([IO.Path]::IsPathRooted($p)) { $p } else { Join-Path $dir $p } }
        $out += [PSCustomObject]@{ Name = $name; Grammar = (& $abs ([string]$e.grammar)); Words = (& $abs ([string]$e.words)); Set = $set }
    }
    return $out
}

function Get-ManifestRefusals {
    # Reasons the measurement cannot start: a listed file that is absent, or a split with no fit or no held-out side.
    param([Parameter(Mandatory)]$Entries)
    $why = @()
    foreach ($e in $Entries) {
        if (-not (Test-Path -LiteralPath $e.Grammar -PathType Leaf)) { $why += "grammar '$($e.Name)': grammar file is missing" }
        if (-not (Test-Path -LiteralPath $e.Words -PathType Leaf)) { $why += "grammar '$($e.Name)': word list is missing" }
    }
    if (@($Entries | Where-Object Set -eq 'fit').Count -eq 0) { $why += 'the manifest has no fit grammar' }
    if (@($Entries | Where-Object Set -eq 'heldout').Count -eq 0) { $why += 'the manifest has no held-out grammar' }
    $fitFiles = @($Entries | Where-Object Set -eq 'fit' | ForEach-Object { try { (Resolve-Path -LiteralPath $_.Grammar -ErrorAction Stop).Path } catch { $_.Grammar } })
    foreach ($h in @($Entries | Where-Object Set -eq 'heldout')) {
        $path = try { (Resolve-Path -LiteralPath $h.Grammar -ErrorAction Stop).Path } catch { $h.Grammar }
        if ($fitFiles -contains $path) { $why += "grammar '$($h.Name)': its grammar file is also in the fit set, so it is not held out" }
    }
    return $why
}

function Get-SampleIndices {
    # Same spread as cost_calibrate.rs: index i*len/count, or every index when the list is no longer than count.
    param([Parameter(Mandatory)][int]$Length, [Parameter(Mandatory)][int]$Count)
    if ($Count -ge $Length) { return [int[]](0..($Length - 1)) }
    return [int[]]@(0..($Count - 1) | ForEach-Object { [int][Math]::Floor($_ * $Length / $Count) })
}

function Read-WordList {
    param([Parameter(Mandatory)][string]$Path)
    return [string[]]@([IO.File]::ReadAllLines($Path) | ForEach-Object { $_.Trim() } | Where-Object { $_ })
}

function Read-CountsFile {
    # Rows of `idx work bytes registers instructions`, as `cost-calibrate --counts-out` writes them; there is no word text in it.
    param([Parameter(Mandatory)][string]$Path)
    $rows = @()
    foreach ($line in [IO.File]::ReadAllLines($Path)) {
        $f = $line.Split("`t")
        if ($f.Count -ne 5) { throw "counts row has $($f.Count) fields, expected 5: $Path" }
        $rows += [PSCustomObject]@{ Idx = [int]$f[0]; Work = [double]$f[1]; Bytes = [double]$f[2]; Registers = [double]$f[3]; Instructions = [double]$f[4] }
    }
    return $rows
}

function Get-Median {
    param([Parameter(Mandatory)][double[]]$Values)
    $s = @($Values | Sort-Object)
    return $s[[Math]::Floor($s.Count / 2)]
}

function Get-Pearson {
    # $null when it is undefined: fewer than 2 points, or no spread on either side.
    param([Parameter(Mandatory)][double[]]$X, [Parameter(Mandatory)][double[]]$Y)
    if ($X.Count -ne $Y.Count) { throw 'Pearson inputs differ in length' }
    if ($X.Count -lt 2) { return $null }
    $mx = ($X | Measure-Object -Average).Average; $my = ($Y | Measure-Object -Average).Average
    $sxy = 0.0; $sxx = 0.0; $syy = 0.0
    for ($i = 0; $i -lt $X.Count; $i++) {
        $dx = $X[$i] - $mx; $dy = $Y[$i] - $my
        $sxy += $dx * $dy; $sxx += $dx * $dx; $syy += $dy * $dy
    }
    if ($sxx -le 0 -or $syy -le 0) { return $null }
    return $sxy / [Math]::Sqrt($sxx * $syy)
}

function Get-WeightedCost {
    param([Parameter(Mandatory)]$Row, [Parameter(Mandatory)][double[]]$Weights)
    return $Row.Work * $Weights[0] + $Row.Bytes * $Weights[1] + $Row.Registers * $Weights[2]
}

function ConvertTo-CandidateWeights {
    # Rounded to integers, at least 1, because pg-cost holds integer weights.
    param([Parameter(Mandatory)][double[]]$Weights)
    return [double[]]@($Weights | ForEach-Object { [Math]::Max(1.0, [Math]::Round($_, [MidpointRounding]::AwayFromZero)) })
}

function Get-FitReading {
    # Which fit of `cost-calibrate --fit-counts` is the candidate: the free one only when the words carry work and registers each.
    param([Parameter(Mandatory)]$Fit)
    $free = [double]$Fit.free.error
    $names = 'work', 'byte', 'register'
    $ratio = @{}; $carried = @{}
    foreach ($n in $names) {
        $without = [double]$Fit.errorWithout.$n
        $ratio[$n] = if ($free -gt 0) { $without / $free } elseif ($without -gt 1e-9) { [double]::PositiveInfinity } else { 1.0 }
        $carried[$n] = $ratio[$n] -ge $script:CarriedErrorRatio
    }
    $w = { param($f) [double[]]@([double]$f.work, [double]$f.byte, [double]$f.register) }
    $times = { param($n) if ([double]::IsInfinity($ratio[$n])) { 'far' } else { 'x{0:0.00}' -f $ratio[$n] } }
    $tradeoff = if ($null -ne $Fit.workRegisterTradeoff) { ', and their estimates trade off at {0:0.00}' -f [double]$Fit.workRegisterTradeoff } else { '' }
    $left = "leaving a term out makes the fit worse by: work $(& $times 'work')  byte $(& $times 'byte')  register $(& $times 'register')"
    if (-not $carried['byte']) {
        return [PSCustomObject]@{ Kind = 'none'; Weights = $null; Left = $left; Note = "the words do not carry the byte term (the fit is as good without it), so no candidate is offered" }
    }
    if ($carried['work'] -and $carried['register']) {
        return [PSCustomObject]@{ Kind = 'free'; Weights = (& $w $Fit.free); Left = $left; Note = 'the words carry work, bytes and registers each, so every weight is refitted' }
    }
    return [PSCustomObject]@{ Kind = 'tied'; Weights = (& $w $Fit.tied); Left = $left
        Note = "the words cannot split work from registers (a fit without one of them is under x$('{0:0.00}' -f $script:CarriedErrorRatio) worse$tradeoff), so the candidate keeps their current ratio and refits their sum against bytes" }
}

function Get-ChangeScore {
    # How well weights predict the change of each grammar's total: `Changes` holds {Grammar, MeasuredPercent, Before, After}, the last two being count totals (work, bytes, registers).
    param([Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Changes, [Parameter(Mandatory)][double[]]$Weights)
    $dot = { param($c) $c[0] * $Weights[0] + $c[1] * $Weights[1] + $c[2] * $Weights[2] }
    $rows = 0; $match = 0; $errs = @(); $by = @()
    foreach ($c in $Changes) {
        $before = & $dot $c.Before
        if ($before -le 0) { throw "grammar '$($c.Grammar)': the Before counts total to no cost" }
        $pred = 100.0 * ((& $dot $c.After) - $before) / $before
        $errs += [Math]::Abs($pred - $c.MeasuredPercent)
        $by += [PSCustomObject]@{ Grammar = $c.Grammar; Measured = $c.MeasuredPercent; Predicted = $pred }
        if ([Math]::Abs($c.MeasuredPercent) -ge $script:ChangeFloorPercent) {
            $rows++
            if ([Math]::Sign($pred) -eq [Math]::Sign($c.MeasuredPercent)) { $match++ }
        }
    }
    return [PSCustomObject]@{
        Grammars = @($Changes).Count; Rows = $rows; Matches = $match
        MeanError = if ($errs.Count) { ($errs | Measure-Object -Average).Average } else { $null }
        WorstError = if ($errs.Count) { ($errs | Measure-Object -Maximum).Maximum } else { $null }
        ByGrammar = $by
    }
}

function Get-CountTotals {
    # Work, bytes and registers summed over the words both builds counted, so the two totals describe the same words.
    param([Parameter(Mandatory)]$Before, [Parameter(Mandatory)]$After)
    $b = @{}; foreach ($r in $Before) { $b[$r.Idx] = $r }
    $tb = [double[]]@(0, 0, 0); $ta = [double[]]@(0, 0, 0); $n = 0
    foreach ($a in $After) {
        $r = $b[$a.Idx]
        if (-not $r) { continue }
        $n++
        $tb[0] += $r.Work; $tb[1] += $r.Bytes; $tb[2] += $r.Registers
        $ta[0] += $a.Work; $ta[1] += $a.Bytes; $ta[2] += $a.Registers
    }
    return [PSCustomObject]@{ Words = $n; Before = $tb; After = $ta }
}

function Get-WeightScore {
    # Level error is predicted against measured instructions per After word; direction is the sign of the change between builds.
    param([Parameter(Mandatory)]$After, [Parameter(Mandatory)]$Before, [Parameter(Mandatory)][double[]]$Weights)
    $scored = @($After | Where-Object { $_.Work -ge $script:MinScoredWork -and $_.Instructions -gt 0 })
    $errs = @(); $pred = @(); $meas = @(); $ratios = @(); $byGrammar = @{}
    foreach ($r in $scored) {
        $p = Get-WeightedCost $r $Weights
        $errs += 100.0 * [Math]::Abs($p - $r.Instructions) / $r.Instructions
        $pred += $p; $meas += $r.Instructions; $ratios += $p / $r.Instructions
        $byGrammar[$r.Grammar] = 1 + [int]$byGrammar[$r.Grammar]
    }
    # Scale-free error divides out the median predicted/measured ratio, so a pure rescale of the weights scores zero.
    $scale = if ($ratios.Count) { Get-Median $ratios } else { 0.0 }
    $flat = @(if ($scale -gt 0) { $ratios | ForEach-Object { 100.0 * [Math]::Abs($_ / $scale - 1.0) } })
    $beforeBy = @{}
    foreach ($b in $Before) { $beforeBy["$($b.Grammar)|$($b.Idx)"] = $b }
    $rows = 0; $match = 0; $predChange = @(); $realChange = @()
    foreach ($a in $scored) {
        $b = $beforeBy["$($a.Grammar)|$($a.Idx)"]
        if (-not $b -or $b.Work -lt $script:MinScoredWork -or $b.Instructions -le 0) { continue }
        $real = 100.0 * ($a.Instructions - $b.Instructions) / $b.Instructions
        $costBefore = Get-WeightedCost $b $Weights
        $pc = 100.0 * ((Get-WeightedCost $a $Weights) - $costBefore) / $costBefore
        $predChange += $pc; $realChange += $real
        if ([Math]::Abs($real) -ge $script:DirectionFloorPercent) {
            $rows++
            if ([Math]::Sign($pc) -eq [Math]::Sign($real)) { $match++ }
        }
    }
    return [PSCustomObject]@{
        Words = $scored.Count
        MedianError = if ($errs.Count) { Get-Median $errs } else { $null }
        WorstError = if ($errs.Count) { ($errs | Measure-Object -Maximum).Maximum } else { $null }
        MedianRatio = if ($ratios.Count) { $scale } else { $null }
        ScaleFreeMedianError = if ($flat.Count) { Get-Median $flat } else { $null }
        ScaleFreeWorstError = if ($flat.Count) { ($flat | Measure-Object -Maximum).Maximum } else { $null }
        WordsByGrammar = $byGrammar
        PearsonLevel = if ($pred.Count) { Get-Pearson $pred $meas } else { $null }
        PearsonChange = if ($realChange.Count) { Get-Pearson $predChange $realChange } else { $null }
        DirectionRows = $rows
        DirectionMatches = $match
    }
}

function Get-LevelNote {
    # Report-only: how far the predicted level is from the measured one. Cost is only ever compared as a ratio, so the level decides nothing.
    param([Parameter(Mandatory)]$Old, [Parameter(Mandatory)]$New)
    if ($null -eq $Old.MedianRatio -or $null -eq $New.MedianRatio) { return $null }
    return ('level, report-only: the current weights predict x{0:0.00} of the measured instructions and the candidate x{1:0.00}; a uniform rescale changes no comparison' -f $Old.MedianRatio, $New.MedianRatio)
}

function Get-RetuneDecision {
    # ACCEPT needs direction no worse, the predicted change of grammar totals no worse and a better shape. The level never decides, and a proof run never exceeds KEEP.
    param([Parameter(Mandatory)]$Old, [Parameter(Mandatory)]$New, $OldChange, $NewChange, [double[]]$OldWeights, [double[]]$NewWeights,
        [Parameter(Mandatory)][bool]$ProofRun, [string[]]$HeldOutNames, [string]$NoCandidate)
    $min = $script:MinHeldOutWords
    $out = { param($v, $r) [PSCustomObject]@{ Verdict = $v; Reason = $r; Level = $(if ($v -ne 'REFUSED') { Get-LevelNote $Old $New }) } }
    if ($Old.Words -lt $min) { return & $out 'REFUSED' "only $($Old.Words) held-out word(s) reach $($script:MinScoredWork) work units; at least $min are needed to score" }
    foreach ($name in $HeldOutNames) {
        $n = [int]$Old.WordsByGrammar[$name]
        if ($n -lt $min) { return & $out 'REFUSED' "held-out grammar '$name' has only $n word(s) that reach $($script:MinScoredWork) work units; at least $min per grammar are needed to score" }
    }
    if ($Old.DirectionRows -ne $New.DirectionRows) { throw 'old and new weights were scored on different direction rows' }
    if (($null -eq $OldChange) -ne ($null -eq $NewChange)) { throw 'old and new weights need the same change scores' }
    if ($OldChange -and $OldChange.Rows -ne $NewChange.Rows) { throw 'old and new weights were scored on different grammar changes' }
    $proof = if ($ProofRun) { 'proof run, direction unmeasurable on a no-change pair: ' } else { '' }
    if ($NoCandidate) { return & $out 'KEEP' "$proof$NoCandidate" }
    $same = $OldWeights -and $NewWeights -and (@(0..2 | Where-Object { $OldWeights[$_] -ne $NewWeights[$_] }).Count -eq 0)
    if ($same) { return & $out 'KEEP' "${proof}the refit rounds to the current weights" }
    $problems = @()
    if ($New.DirectionMatches -lt $Old.DirectionMatches) { $problems += "direction worse ($($New.DirectionMatches) of $($New.DirectionRows) against $($Old.DirectionMatches))" }
    $moved = if ($NewChange) { $NewChange.Rows } else { 0 }
    if ($NewChange) {
        if ($NewChange.Matches -lt $OldChange.Matches) { $problems += "change direction worse ($($NewChange.Matches) of $moved grammar(s) against $($OldChange.Matches))" }
        if ($null -ne $NewChange.MeanError -and $NewChange.MeanError -gt $OldChange.MeanError + $script:ChangeMarginPoints) {
            $problems += ('change error higher ({0:0.00} against {1:0.00} points over {2} grammar(s))' -f $NewChange.MeanError, $OldChange.MeanError, $NewChange.Grammars)
        }
    }
    if (-not ($New.ScaleFreeMedianError -le $Old.ScaleFreeMedianError - $script:ShapeMarginPoints)) { $problems += ('shape not better (scale-free median error {0:0.00}% against {1:0.00}%)' -f $New.ScaleFreeMedianError, $Old.ScaleFreeMedianError) }
    if ($New.ScaleFreeWorstError -gt $Old.ScaleFreeWorstError) { $problems += ('worst shape error higher ({0:0.00}% against {1:0.00}%)' -f $New.ScaleFreeWorstError, $Old.ScaleFreeWorstError) }
    if ($problems.Count -gt 0) { return & $out 'KEEP' "$proof$($problems -join '; ')" }
    $rows = $New.DirectionRows
    $direction = if ($rows -eq 0) { 'direction unmeasured (0 rows)' } elseif ($rows -lt 8) { "direction weak (no worse on only $rows row(s))" } else { 'direction no worse' }
    $change = if ($moved -eq 0) { 'change of grammar totals unmeasured (0 grammars moved)' } else { "change of grammar totals no worse over $moved grammar(s)" }
    $passed = "$direction, $change, shape better on the held-out set"
    if ($ProofRun) { return & $out 'KEEP' "${proof}the tool ran end to end and would otherwise pass ($passed); a no-change pair never issues ACCEPT" }
    return & $out 'ACCEPT' $passed
}

function Get-WeightMoves {
    # One line per weight that moved by more than a quarter (as a ratio of candidate to current).
    param([Parameter(Mandatory)][double[]]$Current, [Parameter(Mandatory)][double[]]$Candidate)
    $names = 'work', 'byte', 'register'
    return [string[]]@(0..2 | Where-Object { [Math]::Abs($Candidate[$_] / $Current[$_] - 1.0) -gt $script:LargeMoveFraction } | ForEach-Object { '{0} x{1:0.00} ({2} to {3})' -f $names[$_], ($Candidate[$_] / $Current[$_]), $Current[$_], $Candidate[$_] })
}

function Read-CompareResult {
    # Per-grammar verdicts from the text instruction-compare.ps1 prints (its stdout), in grammar order. Word text is never in it.
    param([Parameter(Mandatory)][AllowEmptyString()][string[]]$Lines)
    $out = @(); $cur = $null
    foreach ($line in $Lines) {
        if ($line -match '^grammar #(\d+) \((.*)\), (\d+) words, status/signature differences: (\d+) between sides, (\d+) between repeats') {
            $cur = [PSCustomObject]@{ Number = [int]$Matches[1]; File = $Matches[2]; Words = [int]$Matches[3]; Differ = [int]$Matches[4] + [int]$Matches[5]; Verdict = $null; Reason = $null; DeltaPercent = $null }
            $out += $cur
        } elseif ($cur -and $line -match '^\s+VERDICT REFUSED: (.*)$') {
            $cur.Verdict = 'REFUSED'; $cur.Reason = $Matches[1]
        } elseif ($cur -and $line -match '^\s+VERDICT (GO|NO-GO.*)$') {
            $cur.Verdict = $Matches[1]
        } elseif ($cur -and $line -match '^\s+change ([+-]?\d+\.\d+)%') {
            $cur.DeltaPercent = [double]$Matches[1]
        }
    }
    return $out
}

function Get-VerifyProblems {
    # Reasons the comparison cannot back a retune; with -NoChangeCheck the pair is its own control. Builds and samples cannot be bound to saved output.
    param([Parameter(Mandatory)]$Pair, $Control, [Parameter(Mandatory)][int]$Grammars, [switch]$NoChangeCheck, [string[]]$GrammarFiles)
    $why = @()
    if (@($Pair).Count -ne $Grammars) { $why += "the comparison reported $(@($Pair).Count) grammar(s) for $Grammars in the manifest" }
    $sets = [Collections.Generic.List[object]]::new()
    $sets.Add(@('comparison', $Pair)); if (-not $NoChangeCheck) { $sets.Add(@('control', $Control)) }
    foreach ($set in $sets) {
        if ($GrammarFiles -and $set[1]) {
            $files = @($set[1] | ForEach-Object File)
            for ($i = 0; $i -lt [Math]::Min($files.Count, $GrammarFiles.Count); $i++) {
                if ($files[$i] -ne $GrammarFiles[$i]) { $why += "the $($set[0]) names grammar #$($i + 1) as '$($files[$i])' but the manifest has '$($GrammarFiles[$i])'" }
            }
        }
    }
    foreach ($g in $Pair) {
        if ($null -eq $g.Verdict) { $why += "grammar #$($g.Number): the comparison printed no verdict" }
        elseif ($g.Verdict -eq 'REFUSED') { $why += "grammar #$($g.Number): the comparison refused ($($g.Reason))" }
        if ($g.Differ -gt 0) { $why += "grammar #$($g.Number): $($g.Differ) word(s) changed status or signature, so the two builds do not parse alike" }
    }
    $ctl = if ($NoChangeCheck) { $Pair } else { $Control }
    if ($null -eq $ctl -or @($ctl).Count -eq 0) { $why += 'no no-change control: the noise floor is unproven' }
    else {
        if (@($ctl).Count -ne $Grammars) { $why += "the control reported $(@($ctl).Count) grammar(s) for $Grammars in the manifest" }
        foreach ($g in $ctl) {
            if ($g.Verdict -ne 'NO-GO (within noise)') { $why += "grammar #$($g.Number): the no-change control read '$($g.Verdict)', not NO-GO (within noise)" }
        }
    }
    if (-not $NoChangeCheck -and @($Pair | Where-Object Verdict -eq 'GO').Count -eq 0) {
        $why += 'the change read GO on no grammar; a retune follows a GO from instruction-compare'
    }
    return $why
}

function Get-CallgrindEvents {
    # name -> total for a callgrind file, from its `events:` and `totals:` (else `summary:`) lines.
    param([Parameter(Mandatory)][AllowEmptyString()][string[]]$Lines)
    $names = $null; $totals = $null
    foreach ($line in $Lines) {
        if ($line -like 'events:*') { $names = @(($line -split '\s+') | Select-Object -Skip 1) }
        elseif ($line -like 'totals:*') { $totals = @(($line -split '\s+') | Select-Object -Skip 1) }
        elseif ($line -like 'summary:*' -and $null -eq $totals) { $totals = @(($line -split '\s+') | Select-Object -Skip 1) }
    }
    if ($null -eq $names -or $null -eq $totals -or $names.Count -ne $totals.Count) { throw 'callgrind output has no matching events and totals lines' }
    $h = @{}
    for ($i = 0; $i -lt $names.Count; $i++) { $h[$names[$i]] = [double]$totals[$i] }
    return $h
}

function Get-CacheMissTotals {
    # Simulated misses from a --cache-sim=yes callgrind total: L1 (instruction + data) and last level, as the manual names them.
    param([Parameter(Mandatory)][hashtable]$Events)
    foreach ($need in 'I1mr', 'D1mr', 'D1mw', 'ILmr', 'DLmr', 'DLmw') {
        if (-not $Events.ContainsKey($need)) { throw "callgrind total has no $need event; was --cache-sim=yes given?" }
    }
    return [PSCustomObject]@{
        L1 = $Events.I1mr + $Events.D1mr + $Events.D1mw
        LastLevel = $Events.ILmr + $Events.DLmr + $Events.DLmw
    }
}

function Get-DumpPaths {
    # Per-word callgrind dumps in order: `<base>.<n>` files, not the final `<base>` file. Throws unless there is one per word.
    param([Parameter(Mandatory)][string[]]$Paths, [Parameter(Mandatory)][int]$Words)
    $dumps = @($Paths | Where-Object { $_ -match '\.\d+$' } | Sort-Object { [int]($_ -replace '^.*\.(\d+)$', '$1') })
    if ($dumps.Count -ne $Words) { throw "pairing failed: $($dumps.Count) per-word callgrind dump(s) for $Words word(s); a word did not enter the measured function exactly once" }
    $seq = @($dumps | ForEach-Object { [int]($_ -replace '^.*\.(\d+)$', '$1') })
    for ($i = 1; $i -lt $seq.Count; $i++) { if ($seq[$i] -ne $seq[$i - 1] + 1) { throw 'pairing failed: the dump numbers have gaps' } }
    return $dumps
}

function Get-StatusProblems {
    # A word not status ok, or counted differently by the two runs, makes a word-level measurement untrustworthy.
    param([Parameter(Mandatory)][string]$Grammar, [Parameter(Mandatory)][hashtable]$BatchStatus, [Parameter(Mandatory)][int]$Words, [int[]]$CountedIdx)
    $why = @()
    if ($BatchStatus.Count -ne $Words) { $why += "grammar '$Grammar': the parse-only run wrote $($BatchStatus.Count) row(s) for $Words word(s)" }
    $bad = @($BatchStatus.Values | Where-Object { $_ -ne 'ok' })
    if ($bad.Count -gt 0) { $why += "grammar '$Grammar': $($bad.Count) sampled word(s) are not status ok (SKIPPED, CAP or TIMEOUT); prune them from the word list" }
    $okIdx = @($BatchStatus.Keys | Where-Object { $BatchStatus[$_] -eq 'ok' } | Sort-Object)
    $missing = @($okIdx | Where-Object { $CountedIdx -notcontains $_ })
    if ($missing.Count -gt 0) { $why += "grammar '$Grammar': the counting run left out $($missing.Count) word(s) that the parse-only run parsed ok; the two runs disagree on status" }
    return $why
}

function Get-RetuneExitCode {
    param([Parameter(Mandatory)][string]$Verdict)
    switch ($Verdict) { 'ACCEPT' { 0 } 'KEEP' { 10 } 'REFUSED' { 3 } default { throw "unknown verdict $Verdict" } }
}
