<#
  .DESCRIPTION
  Covers: the pure logic in rust/tools/_retune-parser-work.ps1 behind retune-parser-work.ps1 -- manifest handling, held-out scoring,
  the ACCEPT/KEEP/REFUSED rule, the instruction-compare result reader and its refusals -- and the driver's refusals that
  need no measurement (exit codes 2 and 3). All data is synthetic: invented counts and instruction totals, and invented
  file names. Starts no valgrind run; the end-to-end run on real grammars is a manual step in the skill.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_retune-parser-work.ps1"

$driver = Join-Path $PSScriptRoot '..\retune-parser-work.ps1'

function Write-Manifest([string]$Dir, [string]$Json, [string]$Name = 'manifest.json') {
    $p = Join-Path $Dir $Name
    [IO.File]::WriteAllText($p, $Json)
    return $p
}

# Rows whose instructions follow `$truth` exactly, with varied counts, so any weights can be scored against them.
function New-Rows([string]$Grammar, [double[]]$Truth, [int]$Count = 12, [double]$Scale = 1.0, [double]$RegisterScale = 1.0) {
    $rows = @()
    for ($i = 0; $i -lt $Count; $i++) {
        $work = (2000 + $i * 700) * $Scale
        $bytes = (90000 + (($i * $i * 913) % 400000)) * $Scale
        $regs = (500 + (($i * 311) % 3000)) * $Scale * $RegisterScale
        $rows += [PSCustomObject]@{ Grammar = $Grammar; Idx = $i; Work = $work; Bytes = $bytes; Registers = $regs; Instructions = $work * $Truth[0] + $bytes * $Truth[1] + $regs * $Truth[2] }
    }
    return $rows
}

$current = [double[]]@(676, 2, 268)

Test-Case 'a manifest takes the default split for the five grammars and Swahili/Cebuano, and explicit sets win' {
    $dir = New-TestTempDir
    try {
        $names = 'indonesian', 'aweti', 'amharic', 'mbugwe', 'sena', 'swahili', 'cebuano'
        $json = '{"version":1,"grammars":[' + (($names | ForEach-Object { "{`"name`":`"$_`",`"grammar`":`"$_.xml`",`"words`":`"$_.txt`"}" }) -join ',') + ',{"name":"synth","grammar":"/abs/s.xml","words":"w.txt","set":"heldout"}]}'
        $e = Read-RetuneManifest (Write-Manifest $dir $json)
        Assert-Equal 8 $e.Count
        Assert-Equal 5 @($e | Where-Object Set -eq 'fit').Count
        Assert-Equal 3 @($e | Where-Object Set -eq 'heldout').Count
        Assert-Equal (Join-Path $dir 'sena.xml') ($e | Where-Object Name -eq 'sena').Grammar
        Assert-Equal '/abs/s.xml' ($e | Where-Object Name -eq 'synth').Grammar
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force }
}

Test-Case 'manifest structure errors throw: unknown name without a set, duplicate, bad name, wrong version, no grammars' {
    $dir = New-TestTempDir
    try {
        $g = { param($n, $extra) "{`"name`":`"$n`",`"grammar`":`"g.xml`",`"words`":`"w.txt`"$extra}" }
        $cases = @(
            @{ Json = "{`"version`":1,`"grammars`":[$(& $g 'mystery' '')]}"; Match = 'needs "set"' },
            @{ Json = "{`"version`":1,`"grammars`":[$(& $g 'a' ',"set":"fit"'),$(& $g 'A' ',"set":"fit"')]}"; Match = 'twice' },
            @{ Json = "{`"version`":1,`"grammars`":[$(& $g 'bad name' ',"set":"fit"')]}"; Match = 'letters, digits' },
            @{ Json = "{`"version`":2,`"grammars`":[$(& $g 'a' ',"set":"fit"')]}"; Match = 'version' },
            @{ Json = '{"version":1,"grammars":[]}'; Match = 'no grammars' },
            @{ Json = '{not json'; Match = 'not valid JSON' })
        foreach ($c in $cases) {
            $msg = ''
            try { Read-RetuneManifest (Write-Manifest $dir $c.Json) | Out-Null } catch { $msg = $_.Exception.Message }
            Assert-True ($msg -match $c.Match) "expected '$($c.Match)', got '$msg'"
        }
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force }
}

Test-Case 'a missing grammar file, a missing list, and a split with no held-out set are each refused by name' {
    $dir = New-TestTempDir
    try {
        [IO.File]::WriteAllText((Join-Path $dir 'w.txt'), "a`n")
        [IO.File]::WriteAllText((Join-Path $dir 'g.xml'), '<x/>')
        $e = @(
            [PSCustomObject]@{ Name = 'one'; Grammar = (Join-Path $dir 'g.xml'); Words = (Join-Path $dir 'w.txt'); Set = 'fit' },
            [PSCustomObject]@{ Name = 'two'; Grammar = (Join-Path $dir 'gone.xml'); Words = (Join-Path $dir 'gone.txt'); Set = 'heldout' })
        $why = @(Get-ManifestRefusals $e)
        Assert-Equal 2 $why.Count
        Assert-True ($why[0] -match "'two'.*grammar file is missing")
        $noHeld = @(Get-ManifestRefusals @($e[0]))
        Assert-True ($noHeld -match 'no held-out grammar')
        $shared = @(Get-ManifestRefusals @($e[0], [PSCustomObject]@{ Name = 'three'; Grammar = (Join-Path $dir 'g.xml'); Words = (Join-Path $dir 'w.txt'); Set = 'heldout' }))
        Assert-True ($shared -match "'three'.*also in the fit set") ($shared -join ';')
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force }
}

Test-Case 'the sample spreads like cost-calibrate: 40 of 79 start at 0 and rise, and a short list is taken whole' {
    $s = Get-SampleIndices -Length 79 -Count 40
    Assert-Equal 40 $s.Count
    Assert-Equal 0 $s[0]
    Assert-Equal 1 $s[1]
    Assert-Equal 77 $s[39]
    Assert-Equal 10 (Get-SampleIndices -Length 10 -Count 40).Count
}

Test-Case 'Pearson is 1 for a line, -1 for a falling one, and null when it is undefined' {
    Assert-True ([Math]::Abs((Get-Pearson @(1, 2, 3, 4) @(2, 4, 6, 8)) - 1) -lt 1e-12)
    Assert-True ([Math]::Abs((Get-Pearson @(1, 2, 3) @(3, 2, 1)) + 1) -lt 1e-12)
    Assert-Equal $null (Get-Pearson @(1, 1, 1) @(1, 2, 3))
    Assert-Equal $null (Get-Pearson @(1) @(1))
}

Test-Case 'candidate weights round to integers and never fall below 1' {
    $c = ConvertTo-CandidateWeights -Weights @(660.4, 0.3, 330.5)
    Assert-Equal 660 $c[0]
    Assert-Equal 1 $c[1]
    Assert-Equal 331 $c[2]
}

# What `cost-calibrate --fit-counts` prints; the numbers are invented.
function New-Fit([double[]]$Free, [double]$FreeError, [double[]]$Tied, [double]$TiedError, [double[]]$Without, $Tradeoff = -0.5) {
    $w = { param($v, $e) [PSCustomObject]@{ work = $v[0]; byte = $v[1]; register = $v[2]; error = $e } }
    return [PSCustomObject]@{ words = 100; free = (& $w $Free $FreeError); tied = (& $w $Tied $TiedError)
        errorWithout = [PSCustomObject]@{ work = $Without[0]; byte = $Without[1]; register = $Without[2] }
        workRegisterTradeoff = $Tradeoff; current = [PSCustomObject]@{ work = 676; byte = 2; register = 268 } }
}

Test-Case 'the free fit is the candidate only when leaving out work or registers each makes the fit at least a tenth worse' {
    $r = Get-FitReading (New-Fit @(200, 2, 100) 0.05 @(180, 2, 71) 0.06 @(0.09, 0.30, 0.08))
    Assert-Equal 'free' $r.Kind
    Assert-Equal 100 $r.Weights[2]
    Assert-True ($r.Left -match 'work x1\.80  byte x6\.00  register x1\.60') $r.Left
}

Test-Case 'words that cannot split work from registers give the tied fit, whichever of the two the free fit dropped' {
    # The fit is as good without registers.
    $a = Get-FitReading (New-Fit @(177, 2.2, 14) 0.0937 @(101, 2.3, 40) 0.0948 @(0.0992, 0.3228, 0.0940) -0.97)
    Assert-Equal 'tied' $a.Kind
    Assert-Equal 101 $a.Weights[0]
    Assert-Equal 40 $a.Weights[2]
    Assert-True ($a.Note -match 'cannot split work from registers' -and $a.Note -match 'trade off at -0\.97') $a.Note
    # Another sample of the same code: now the fit is as good without work, and the free weights are far from the first.
    $b = Get-FitReading (New-Fit @(58, 2.3, 61) 0.1245 @(111, 2.2, 44) 0.1253 @(0.1254, 0.2991, 0.1338) -0.89)
    Assert-Equal 'tied' $b.Kind
    Assert-Equal 111 $b.Weights[0]
    # A perfect fit that stays perfect without a term does not carry it.
    $c = Get-FitReading (New-Fit @(500, 2, 0) 0 @(250, 2, 99) 0 @(0, 0.3, 0) $null)
    Assert-Equal 'tied' $c.Kind
    Assert-True ($c.Left -match 'byte far') $c.Left
    Assert-True ($c.Note -notmatch 'trade off') $c.Note
}

Test-Case 'words that do not carry the byte term offer no candidate' {
    $r = Get-FitReading (New-Fit @(200, 2, 100) 0.05 @(180, 2, 71) 0.06 @(0.09, 0.052, 0.08))
    Assert-Equal 'none' $r.Kind
    Assert-Equal $null $r.Weights
    Assert-True ($r.Note -match 'do not carry the byte term') $r.Note
}

Test-Case 'weights that match the code have a better shape than stale ones on held-out words, so the refit is ACCEPT' {
    $truth = [double[]]@(560, 2, 140)
    $after = @(New-Rows 'h1' $truth) + @(New-Rows 'h2' $truth 10 2.0)
    $old = Get-WeightScore -After $after -Before $after -Weights $current
    $new = Get-WeightScore -After $after -Before $after -Weights $truth
    Assert-Equal 22 $old.Words
    Assert-True ($new.ScaleFreeMedianError -lt 1e-9 -and $new.ScaleFreeWorstError -lt 1e-9) 'exact weights have no error'
    Assert-True ($old.ScaleFreeMedianError -gt 0.3) "stale shape $($old.ScaleFreeMedianError)"
    Assert-True ($old.PearsonLevel -gt 0.9) 'stale weights still correlate'
    $d = Get-RetuneDecision -Old $old -New $new -OldWeights $current -NewWeights $truth -ProofRun $false
    Assert-Equal 'ACCEPT' $d.Verdict
    Assert-True ($d.Reason -match 'shape better') $d.Reason
}

Test-Case 'a refit whose shape is no better is KEEP, and a worse worst shape error is KEEP even with a lower median' {
    $truth = [double[]]@(560, 2, 140)
    $after = New-Rows 'h' $truth
    $score = Get-WeightScore -After $after -Before $after -Weights $current
    $d = Get-RetuneDecision -Old $score -New $score -OldWeights $current -NewWeights @(677, 2, 268) -ProofRun $false
    Assert-Equal 'KEEP' $d.Verdict
    Assert-True ($d.Reason -match 'shape not better')
    $worse = $score.PSObject.Copy(); $worse.ScaleFreeMedianError = $score.ScaleFreeMedianError - 1; $worse.ScaleFreeWorstError = $score.ScaleFreeWorstError + 1
    $d2 = Get-RetuneDecision -Old $score -New $worse -OldWeights $current -NewWeights @(600, 2, 200) -ProofRun $false
    Assert-Equal 'KEEP' $d2.Verdict
    Assert-True ($d2.Reason -match 'worst shape error higher')
}

Test-Case 'a pure rescale fixes the level, changes no shape, and is KEEP: the level never decides' {
    $truth = [double[]]@(560, 2, 140)
    $rows = @(New-Rows 'h1' $truth) + @(New-Rows 'h2' $truth 10 2.0)
    $scaled = [double[]]@($truth | ForEach-Object { $_ * 2.3 })
    $a = Get-WeightScore -After $rows -Before $rows -Weights $scaled
    $b = Get-WeightScore -After $rows -Before $rows -Weights $truth
    Assert-True ($a.MedianError -gt 100 -and $b.MedianError -lt 1e-9) "level error of a rescale $($a.MedianError)"
    Assert-True ($a.ScaleFreeMedianError -lt 1e-9 -and $a.ScaleFreeWorstError -lt 1e-9) 'a rescale has no shape error'
    Assert-True ([Math]::Abs($a.MedianRatio - 2.3) -lt 1e-9) "ratio $($a.MedianRatio)"
    $d = Get-RetuneDecision -Old $a -New $b -OldWeights $scaled -NewWeights $truth -ProofRun $false
    Assert-Equal 'KEEP' $d.Verdict
    Assert-True ($d.Reason -match 'shape not better') $d.Reason
    Assert-True ($d.Level -match 'current weights predict x2\.30' -and $d.Level -match 'candidate x1\.00' -and $d.Level -match 'changes no comparison') $d.Level
}

Test-Case 'a refit that rounds to the current weights is KEEP without looking at errors' {
    $after = New-Rows 'h' $current
    $score = Get-WeightScore -After $after -Before $after -Weights $current
    $d = Get-RetuneDecision -Old $score -New $score -OldWeights $current -NewWeights $current -ProofRun $false
    Assert-Equal 'KEEP' $d.Verdict
    Assert-True ($d.Reason -match 'rounds to the current')
}

Test-Case 'no candidate is KEEP with the reason the fit gave' {
    $after = New-Rows 'h' $current
    $score = Get-WeightScore -After $after -Before $after -Weights $current
    $d = Get-RetuneDecision -Old $score -New $score -OldWeights $current -NewWeights $current -ProofRun $false -NoCandidate 'the words do not carry the byte term'
    Assert-Equal 'KEEP' $d.Verdict
    Assert-Equal 'the words do not carry the byte term' $d.Reason
}

Test-Case 'direction counts only words whose instructions moved at least 2%, and worse direction blocks ACCEPT' {
    # After copies a third as many registers, so instructions fell on every word.
    $truth = [double[]]@(676, 2, 268)
    $before = New-Rows 'h' $truth; $after = New-Rows 'h' $truth 12 1.0 0.33
    $right = Get-WeightScore -After $after -Before $before -Weights $truth
    $flat = Get-WeightScore -After $after -Before $before -Weights @(676, 2, 0)
    Assert-True ($right.DirectionRows -ge 10) "rows $($right.DirectionRows)"
    Assert-Equal $right.DirectionRows $right.DirectionMatches
    Assert-True ($right.PearsonChange -gt 0.99) "change r $($right.PearsonChange)"
    # With no register weight the cost still falls only through work and bytes, which did not change: no direction at all.
    Assert-Equal 0 $flat.DirectionMatches
    $d = Get-RetuneDecision -Old $right -New $flat -OldWeights $current -NewWeights @(676, 2, 150) -ProofRun $false
    Assert-Equal 'KEEP' $d.Verdict
    Assert-True ($d.Reason -match 'direction worse')
}

Test-Case 'a no-change pair has no direction rows' {
    $truth = [double[]]@(600, 2, 200)
    $rows = New-Rows 'h' $truth
    $same = Get-WeightScore -After $rows -Before $rows -Weights $current
    Assert-Equal 0 $same.DirectionRows
    Assert-Equal $null $same.PearsonChange -Message 'undefined with no change'
}

# Scores for a real pair whose counts did not change (a constant-factor change): only the shape discriminates.
function Get-ZeroRowScores {
    $truth = [double[]]@(560, 2, 140)
    $rows = @(New-Rows 'h1' $truth) + @(New-Rows 'h2' $truth 10 2.0)
    return [PSCustomObject]@{
        Old = (Get-WeightScore -After $rows -Before $rows -Weights $current)
        New = (Get-WeightScore -After $rows -Before $rows -Weights $truth)
        Truth = $truth }
}

Test-Case 'a proof run on a no-change pair is capped at KEEP and says direction is unmeasurable' {
    $s = Get-ZeroRowScores
    $d = Get-RetuneDecision -Old $s.Old -New $s.New -OldWeights $current -NewWeights $s.Truth -ProofRun $true
    Assert-Equal 'KEEP' $d.Verdict
    Assert-True ($d.Reason -match 'proof run, direction unmeasurable') $d.Reason
    Assert-True ($d.Reason -notmatch 'direction no worse') $d.Reason
    Assert-Equal 10 (Get-RetuneExitCode $d.Verdict)
}

Test-Case 'a real pair with 0 direction rows and no grammar that moved can ACCEPT on shape, and says both are unmeasured' {
    $s = Get-ZeroRowScores
    Assert-Equal 0 $s.New.DirectionRows
    $d = Get-RetuneDecision -Old $s.Old -New $s.New -OldWeights $current -NewWeights $s.Truth -ProofRun $false
    Assert-Equal 'ACCEPT' $d.Verdict
    Assert-True ($d.Reason -match 'direction unmeasured \(0 rows\)') $d.Reason
    Assert-True ($d.Reason -match 'change of grammar totals unmeasured \(0 grammars moved\)') $d.Reason
    Assert-True ($d.Reason -notmatch 'direction no worse') $d.Reason
}

Test-Case 'a few direction rows are called weak, and many say direction no worse' {
    $mk = { param($rows) [PSCustomObject]@{ Words = 20; MedianError = 5.0; WorstError = 9.0; DirectionRows = $rows; DirectionMatches = $rows; ScaleFreeMedianError = 1.0; ScaleFreeWorstError = 2.0; MedianRatio = 1.0; WordsByGrammar = @{} } }
    $better = & $mk 3; $better.ScaleFreeMedianError = 0.5
    $weak = Get-RetuneDecision -Old (& $mk 3) -New $better -OldWeights $current -NewWeights @(600, 2, 200) -ProofRun $false
    Assert-Equal 'ACCEPT' $weak.Verdict
    Assert-True ($weak.Reason -match 'direction weak') $weak.Reason
    $better12 = & $mk 12; $better12.ScaleFreeMedianError = 0.5
    $full = Get-RetuneDecision -Old (& $mk 12) -New $better12 -OldWeights $current -NewWeights @(600, 2, 200) -ProofRun $false
    Assert-True ($full.Reason -match '^direction no worse') $full.Reason
}

Test-Case 'a held-out grammar with too few scored words is refused by name, though the pool is large' {
    $truth = [double[]]@(560, 2, 140)
    $rows = @(New-Rows 'big' $truth 30) + @(New-Rows 'tiny' $truth 3)
    $score = Get-WeightScore -After $rows -Before $rows -Weights $current
    Assert-Equal 33 $score.Words
    Assert-Equal 3 $score.WordsByGrammar['tiny']
    $d = Get-RetuneDecision -Old $score -New $score -OldWeights $current -NewWeights $truth -ProofRun $false -HeldOutNames @('big', 'tiny')
    Assert-Equal 'REFUSED' $d.Verdict
    Assert-True ($d.Reason -match "'tiny' has only 3") $d.Reason
    $absent = Get-RetuneDecision -Old $score -New $score -OldWeights $current -NewWeights $truth -ProofRun $false -HeldOutNames @('big', 'gone')
    Assert-True ($absent.Reason -match "'gone' has only 0") $absent.Reason
}

Test-Case 'count totals cover only the words both builds counted' {
    $before = @(New-Rows 'g' $current 4); $after = @(New-Rows 'g' $current 4 0.5 | Select-Object -Skip 1)
    $tot = Get-CountTotals -Before $before -After $after
    Assert-Equal 3 $tot.Words
    Assert-Equal ((2700 + 3400 + 4100)) $tot.Before[0]
    Assert-Equal ((2700 + 3400 + 4100) * 0.5) $tot.After[0]
}

# One grammar's change: the After counts are the Before counts with bytes cut by `$ByteCut`, and the instructions follow `$Truth`.
function New-Change([string]$Grammar, [double[]]$Truth, [double]$ByteCut, [double]$WorkGain = 0.0) {
    $before = [double[]]@(1000000, 300000000, 2500000)
    $after = [double[]]@(($before[0] * (1 + $WorkGain)), ($before[1] * (1 - $ByteCut)), $before[2])
    $cost = { param($c) $c[0] * $Truth[0] + $c[1] * $Truth[1] + $c[2] * $Truth[2] }
    return [PSCustomObject]@{ Grammar = $Grammar; Before = $before; After = $after; MeasuredPercent = 100.0 * ((& $cost $after) / (& $cost $before) - 1) }
}

Test-Case 'the change of a grammar total is predicted from its counts and scored in points against the measured change' {
    $truth = [double[]]@(200, 2, 100)
    $changes = @((New-Change 'a' $truth 0.30), (New-Change 'b' $truth 0.10), (New-Change 'c' $truth 0.0))
    $exact = Get-ChangeScore -Changes $changes -Weights $truth
    Assert-Equal 3 $exact.Grammars
    Assert-Equal 2 $exact.Rows -Message 'the grammar that did not move is not a sign row'
    Assert-Equal 2 $exact.Matches
    Assert-True ($exact.MeanError -lt 1e-9 -and $exact.WorstError -lt 1e-9) "exact weights predict the change: $($exact.MeanError)"
    # Weights that give bytes too little say the cut saved less than it did.
    $stale = Get-ChangeScore -Changes $changes -Weights $current
    Assert-Equal 2 $stale.Matches
    Assert-True ($stale.MeanError -gt 2) "stale mean error $($stale.MeanError)"
    Assert-True ($stale.ByGrammar[0].Predicted -gt $stale.ByGrammar[0].Measured) 'the stale prediction is the smaller cut'
    # Work alone gets the sign wrong when work rose while bytes fell.
    $mixed = @((New-Change 'a' $truth 0.30 0.05))
    $workOnly = Get-ChangeScore -Changes $mixed -Weights @(1, 0, 0)
    Assert-Equal 1 $workOnly.Rows
    Assert-Equal 0 $workOnly.Matches
    Assert-Equal 0 (Get-ChangeScore -Changes @() -Weights $truth).Grammars
}

Test-Case 'a candidate that predicts the change of grammar totals worse is KEEP, though its shape is better' {
    $mk = { param([double]$shape) [PSCustomObject]@{ Words = 20; MedianError = 5.0; WorstError = 9.0; DirectionRows = 12; DirectionMatches = 12; ScaleFreeMedianError = $shape; ScaleFreeWorstError = 2.0; MedianRatio = 1.0; WordsByGrammar = @{} } }
    $ch = { param([int]$signs, [double]$mean) [PSCustomObject]@{ Grammars = 7; Rows = 7; Matches = $signs; MeanError = $mean; WorstError = $mean * 2 } }
    $worse = Get-RetuneDecision -Old (& $mk 8.0) -New (& $mk 3.0) -OldChange (& $ch 7 5.0) -NewChange (& $ch 7 5.6) -OldWeights $current -NewWeights @(101, 2, 40) -ProofRun $false
    Assert-Equal 'KEEP' $worse.Verdict
    Assert-True ($worse.Reason -match 'change error higher \(5\.60 against 5\.00 points over 7 grammar\(s\)\)') $worse.Reason
    $sign = Get-RetuneDecision -Old (& $mk 8.0) -New (& $mk 3.0) -OldChange (& $ch 7 5.0) -NewChange (& $ch 6 4.0) -OldWeights $current -NewWeights @(101, 2, 40) -ProofRun $false
    Assert-Equal 'KEEP' $sign.Verdict
    Assert-True ($sign.Reason -match 'change direction worse \(6 of 7 grammar\(s\) against 7\)') $sign.Reason
    # Within half a point the change error is no worse, and the better shape decides.
    $ok = Get-RetuneDecision -Old (& $mk 8.0) -New (& $mk 3.0) -OldChange (& $ch 7 5.0) -NewChange (& $ch 7 5.4) -OldWeights $current -NewWeights @(101, 2, 40) -ProofRun $false
    Assert-Equal 'ACCEPT' $ok.Verdict
    Assert-True ($ok.Reason -match 'change of grammar totals no worse over 7 grammar\(s\), shape better') $ok.Reason
}

Test-Case 'the driver hands the change scores and the proof-run flag to the decision, and reads the fit before choosing a candidate' {
    $text = Get-Content -LiteralPath $driver -Raw
    Assert-True ($text -match 'Get-RetuneDecision [^\r\n]*-OldChange \$oldChange -NewChange \$newChange [^\r\n]*-ProofRun \(\[bool\]\$NoChangeCheck\)[^\r\n]*-NoCandidate \$noCandidate')
    Assert-True ($text -match '\$reading = Get-FitReading \$fit')
    Assert-True ($text -notmatch 'MaxFitRounds')
}

Test-Case 'a weight that moves by more than a quarter is flagged, and a small move is not' {
    Assert-Equal 0 @(Get-WeightMoves -Current $current -Candidate @(700, 2, 280)).Count
    $m = @(Get-WeightMoves -Current $current -Candidate @(55, 2, 99))
    Assert-Equal 2 $m.Count
    Assert-True ($m[0] -match '^work x0\.08') $m[0]
    Assert-Equal 1 @(Get-WeightMoves -Current $current -Candidate @(676, 3, 268)).Count
}

Test-Case 'too few held-out words reaching the work floor refuses to score' {
    $rows = New-Rows 'h' $current 5
    $score = Get-WeightScore -After $rows -Before $rows -Weights $current
    $d = Get-RetuneDecision -Old $score -New $score -OldWeights $current -NewWeights @(600, 2, 200) -ProofRun $false
    Assert-Equal 'REFUSED' $d.Verdict
    Assert-True ($d.Reason -match 'at least 8')
    $tiny = @($rows | ForEach-Object { $_.PSObject.Copy() })
    foreach ($t in $tiny) { $t.Work = 10 }
    Assert-Equal 0 (Get-WeightScore -After $tiny -Before $tiny -Weights $current).Words
}

Test-Case 'exit codes: ACCEPT 0, KEEP 10, REFUSED 3' {
    Assert-Equal 0 (Get-RetuneExitCode 'ACCEPT')
    Assert-Equal 10 (Get-RetuneExitCode 'KEEP')
    Assert-Equal 3 (Get-RetuneExitCode 'REFUSED')
}

$goText = @(
    'instruction-compare: before=a after=b repeats=2 backend=callgrind',
    '  no-change control: not run by this invocation',
    '',
    'grammar #1 (one-hc.xml), 40 words, status/signature differences: 0 between sides, 0 between repeats',
    '  instructions before         1 | 1   mean 1',
    '  change -6.50%   noise from repeats and empty runs 0.01%   threshold 0.50%',
    '  VERDICT GO',
    '',
    'grammar #2 (two-hc.xml), 40 words, status/signature differences: 0 between sides, 0 between repeats',
    '  change +0.00%   noise from repeats and empty runs 0.00%   threshold 0.50%',
    '  VERDICT NO-GO (within noise)')
$noGoText = @($goText | ForEach-Object { $_ -replace 'VERDICT GO', 'VERDICT NO-GO (within noise)' })

Test-Case 'instruction-compare output is read per grammar with verdict, change and differences, and no word text is in it' {
    $r = @(Read-CompareResult $goText)
    Assert-Equal 2 $r.Count
    Assert-Equal 'GO' $r[0].Verdict
    Assert-Equal -6.5 $r[0].DeltaPercent
    Assert-Equal 'NO-GO (within noise)' $r[1].Verdict
    $refused = @(Read-CompareResult @('grammar #1 (a.xml), 40 words, status/signature differences: 2 between sides, 1 between repeats', '  VERDICT REFUSED: 3 word(s) differ'))
    Assert-Equal 'REFUSED' $refused[0].Verdict
    Assert-Equal 3 $refused[0].Differ
}

Test-Case 'verify passes on a GO pair with a quiet control, and refuses each way it can fail' {
    $pair = @(Read-CompareResult $goText); $ctl = @(Read-CompareResult $noGoText)
    Assert-Equal 0 @(Get-VerifyProblems -Pair $pair -Control $ctl -Grammars 2).Count
    # no control at all
    Assert-True ((@(Get-VerifyProblems -Pair $pair -Control $null -Grammars 2) -join ';') -match 'no no-change control')
    # a control that reads GO has not proven the noise floor
    $loud = @(Read-CompareResult $goText)
    Assert-True ((@(Get-VerifyProblems -Pair $pair -Control $loud -Grammars 2) -join ';') -match "control read 'GO'")
    # no GO anywhere: a retune follows a GO
    Assert-True ((@(Get-VerifyProblems -Pair $ctl -Control $ctl -Grammars 2) -join ';') -match 'read GO on no grammar')
    # a status change and a refused grammar
    $bad = @(Read-CompareResult @('grammar #1 (a.xml), 40 words, status/signature differences: 1 between sides, 0 between repeats', '  VERDICT REFUSED: 1 word(s) differ'))
    $why = @(Get-VerifyProblems -Pair $bad -Control $bad -Grammars 1) -join ';'
    Assert-True ($why -match 'refused' -and $why -match 'changed status or signature') $why
    # a grammar missing from the comparison
    Assert-True ((@(Get-VerifyProblems -Pair $pair -Control $ctl -Grammars 3) -join ';') -match 'for 3 in the manifest')
}

Test-Case 'a saved comparison that names other grammar files than the manifest is refused' {
    $pair = @(Read-CompareResult $goText); $ctl = @(Read-CompareResult $noGoText)
    Assert-Equal 0 @(Get-VerifyProblems -Pair $pair -Control $ctl -Grammars 2 -GrammarFiles @('one-hc.xml', 'two-hc.xml')).Count
    $why = @(Get-VerifyProblems -Pair $pair -Control $ctl -Grammars 2 -GrammarFiles @('one-hc.xml', 'other.xml')) -join ';'
    Assert-True ($why -match "comparison names grammar #2 as 'two-hc.xml' but the manifest has 'other.xml'") $why
    Assert-True ($why -match 'control names grammar #2') $why
}

Test-Case 'a no-change pair is its own control, needs no GO, and refuses if it reads GO' {
    $quiet = @(Read-CompareResult $noGoText)
    Assert-Equal 0 @(Get-VerifyProblems -Pair $quiet -Control $null -Grammars 2 -NoChangeCheck).Count
    $loud = @(Read-CompareResult $goText)
    Assert-True ((@(Get-VerifyProblems -Pair $loud -Control $null -Grammars 2 -NoChangeCheck) -join ';') -match "control read 'GO'")
}

Test-Case 'cache totals are read by event name, and a run without --cache-sim is refused' {
    $e = Get-CallgrindEvents @('part: 1', '', 'events: Ir Dr Dw I1mr D1mr D1mw ILmr DLmr DLmw', '', 'totals: 1000 300 200 5 40 30 2 7 6', '')
    $m = Get-CacheMissTotals $e
    Assert-Equal 75 $m.L1
    Assert-Equal 15 $m.LastLevel
    $threw = $false
    try { Get-CacheMissTotals (Get-CallgrindEvents @('events: Ir', 'totals: 1000')) | Out-Null } catch { $threw = $true; Assert-True ($_.Exception.Message -match 'cache-sim') }
    Assert-True $threw
}

Test-Case 'per-word dumps are ordered by number, not by text, and the final file is ignored' {
    $paths = @('x.cg.9.out', 'x.cg.9.out.10', 'x.cg.9.out.9', 'x.cg.9.out.11', 'x.cg.9.out.8')
    $d = @(Get-DumpPaths -Paths $paths -Words 4)
    Assert-Equal 'x.cg.9.out.8' $d[0]
    Assert-Equal 'x.cg.9.out.10' $d[2]
    Assert-Equal 'x.cg.9.out.11' $d[3]
}

Test-Case 'a dump count that is not the word count is refused, and so is a gap in the numbers' {
    $threw = $false
    try { Get-DumpPaths -Paths @('a.out.1', 'a.out.2') -Words 3 | Out-Null } catch { $threw = $true; Assert-True ($_.Exception.Message -match '2 per-word callgrind dump') }
    Assert-True $threw
    $threw = $false
    try { Get-DumpPaths -Paths @('a.out.1', 'a.out.3') -Words 2 | Out-Null } catch { $threw = $true; Assert-True ($_.Exception.Message -match 'gaps') }
    Assert-True $threw
    Assert-Equal 3 @(Get-DumpPaths -Paths @('a.out', 'a.out.5', 'a.out.4', 'a.out.6') -Words 3).Count
}

Test-Case 'a skipped or capped word, or a word the two tools disagree on, is a status problem' {
    $ok = @{ 0 = 'ok'; 1 = 'ok'; 2 = 'ok' }
    Assert-Equal 0 @(Get-StatusProblems -Grammar 'g' -BatchStatus $ok -Words 3 -CountedIdx @(0, 1, 2)).Count
    $skip = @{ 0 = 'ok'; 1 = 'SKIPPED'; 2 = 'CAP' }
    Assert-True ((@(Get-StatusProblems -Grammar 'g' -BatchStatus $skip -Words 3 -CountedIdx @(0)) -join ';') -match '2 sampled word')
    Assert-True ((@(Get-StatusProblems -Grammar 'g' -BatchStatus $ok -Words 3 -CountedIdx @(0, 2)) -join ';') -match 'left out 1')
    Assert-True ((@(Get-StatusProblems -Grammar 'g' -BatchStatus @{ 0 = 'ok' } -Words 3 -CountedIdx @(0)) -join ';') -match 'wrote 1 row')
}

Test-Case 'counts files are read without word text and a short row is refused' {
    $dir = New-TestTempDir
    try {
        $p = Join-Path $dir 'c.tsv'
        [IO.File]::WriteAllText($p, "3`t1200`t90000`t40`t2500000`n")
        $r = @(Read-CountsFile $p)
        Assert-Equal 1 $r.Count
        Assert-Equal 90000 $r[0].Bytes
        [IO.File]::WriteAllText($p, "3`t1200`n")
        $threw = $false
        try { Read-CountsFile $p | Out-Null } catch { $threw = $true }
        Assert-True $threw
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force }
}

# Driver refusals that need no measurement; they run the real script, so they need Linux with valgrind and rustc.
function Invoke-Driver([string[]]$Arguments) {
    $out = & pwsh -NoProfile -File $driver @Arguments 2>&1 | ForEach-Object { "$_" }
    return [PSCustomObject]@{ Exit = $LASTEXITCODE; Text = ($out -join "`n") }
}
$canRun = $IsLinux -and (Get-Command valgrind -ErrorAction SilentlyContinue) -and (Get-Command rustc -ErrorAction SilentlyContinue)

Test-Case 'the driver cannot start without Linux, valgrind and rustc, and says so with exit 2' {
    if ($canRun) { return }
    $r = Invoke-Driver @('-Manifest', 'm.json', '-Before', 'a', '-After', 'b')
    Assert-Equal 2 $r.Exit
}

if ($canRun) {
    $dir = New-TestTempDir
    try {
        $before = Join-Path $dir 'before'; $after = Join-Path $dir 'after'
        Copy-Item (Get-Command true).Source $before
        Copy-Item (Get-Command false).Source $after
        [IO.File]::WriteAllText((Join-Path $dir 'w.txt'), ("alpha`nbeta`n" * 8))
        [IO.File]::WriteAllText((Join-Path $dir 'g.xml'), '<x/>')
        [IO.File]::WriteAllText((Join-Path $dir 'g2.xml'), '<y/>')
        $manifest = Write-Manifest $dir '{"version":1,"grammars":[{"name":"f1","grammar":"g.xml","words":"w.txt","set":"fit"},{"name":"h1","grammar":"g2.xml","words":"w.txt","set":"heldout"}]}' 'ok.json'
        $missing = Write-Manifest $dir '{"version":1,"grammars":[{"name":"f1","grammar":"g.xml","words":"w.txt","set":"fit"},{"name":"h1","grammar":"gone.xml","words":"w.txt","set":"heldout"}]}' 'missing.json'

        Test-Case 'a missing grammar is REFUSED with exit 3 before any measurement' {
            $r = Invoke-Driver @('-Manifest', $missing, '-Before', $before, '-After', $after)
            Assert-Equal 3 $r.Exit $r.Text
            Assert-True ($r.Text -match "DECISION REFUSED: grammar 'h1': grammar file is missing") $r.Text
        }
        Test-Case 'a build without cost-calibrate --counts-out is REFUSED with exit 3 and named' {
            $r = Invoke-Driver @('-Manifest', $manifest, '-Before', $before, '-After', $after)
            Assert-Equal 3 $r.Exit $r.Text
            Assert-True ($r.Text -match 'has no cost-calibrate --counts-out') $r.Text
        }
        Test-Case 'the same build without -NoChangeCheck, and different builds with it, are exit 2' {
            $r = Invoke-Driver @('-Manifest', $manifest, '-Before', $before, '-After', $before)
            Assert-Equal 2 $r.Exit $r.Text
            Assert-True ($r.Text -match 'pass -NoChangeCheck') $r.Text
            $r2 = Invoke-Driver @('-Manifest', $manifest, '-Before', $before, '-After', $after, '-NoChangeCheck')
            Assert-Equal 2 $r2.Exit $r2.Text
        }
        Test-Case 'a manifest that is not JSON is exit 2' {
            $bad = Write-Manifest $dir '{nope' 'bad.json'
            $r = Invoke-Driver @('-Manifest', $bad, '-Before', $before, '-After', $after)
            Assert-Equal 2 $r.Exit $r.Text
            Assert-True ($r.Text -match 'not valid JSON') $r.Text
        }
        Test-Case 'a binary that is absent is exit 2' {
            $r = Invoke-Driver @('-Manifest', $manifest, '-Before', (Join-Path $dir 'nope'), '-After', $after)
            Assert-Equal 2 $r.Exit $r.Text
        }
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force }
}

Write-TestSummary
