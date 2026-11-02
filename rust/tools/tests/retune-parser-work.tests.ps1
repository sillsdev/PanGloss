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

Test-Case 'candidate weights fold the measured speed in, round to integers, and never fall below 1' {
    $c = ConvertTo-CandidateWeights -Suggested @(600.0, 1.2, 300.4) -Speed 1.1
    Assert-Equal 660 $c[0]
    Assert-Equal 1 $c[1]
    Assert-Equal 330 $c[2]
}

Test-Case 'weights that match the code score better than stale ones on held-out words, so the refit is ACCEPT' {
    $truth = [double[]]@(560, 2, 140)
    $after = @(New-Rows 'h1' $truth) + @(New-Rows 'h2' $truth 10 2.0)
    $old = Get-WeightScore -After $after -Before $after -Weights $current
    $new = Get-WeightScore -After $after -Before $after -Weights $truth
    Assert-Equal 22 $old.Words
    Assert-True ($new.MedianError -lt 1e-9 -and $new.WorstError -lt 1e-9) 'exact weights have no error'
    Assert-True ($old.MedianError -gt 5) "stale median $($old.MedianError)"
    Assert-True ($old.PearsonLevel -gt 0.9) 'stale weights still correlate'
    $d = Get-RetuneDecision -Old $old -New $new -OldWeights $current -NewWeights $truth -Settled $true -ProofRun $false
    Assert-Equal 'ACCEPT' $d.Verdict
}

Test-Case 'a refit that is no better on median error is KEEP, and a worse worst error is KEEP even with a lower median' {
    $truth = [double[]]@(560, 2, 140)
    $after = New-Rows 'h' $truth
    $score = Get-WeightScore -After $after -Before $after -Weights $current
    $d = Get-RetuneDecision -Old $score -New $score -OldWeights $current -NewWeights @(677, 2, 268) -Settled $true -ProofRun $false
    Assert-Equal 'KEEP' $d.Verdict
    Assert-True ($d.Reason -match 'median error not lower')
    $worse = $score.PSObject.Copy(); $worse.MedianError = $score.MedianError - 1; $worse.WorstError = $score.WorstError + 1
    $d2 = Get-RetuneDecision -Old $score -New $worse -OldWeights $current -NewWeights @(600, 2, 200) -Settled $true -ProofRun $false
    Assert-Equal 'KEEP' $d2.Verdict
    Assert-True ($d2.Reason -match 'worst error higher')
}

Test-Case 'a refit that rounds to the current weights is KEEP without looking at errors' {
    $after = New-Rows 'h' $current
    $score = Get-WeightScore -After $after -Before $after -Weights $current
    $d = Get-RetuneDecision -Old $score -New $score -OldWeights $current -NewWeights $current -Settled $true -ProofRun $false
    Assert-Equal 'KEEP' $d.Verdict
    Assert-True ($d.Reason -match 'rounds to the current')
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
    $d = Get-RetuneDecision -Old $right -New ([PSCustomObject]@{ Words = $right.Words; MedianError = 0; WorstError = 0; DirectionRows = $right.DirectionRows; DirectionMatches = 0 }) -OldWeights $current -NewWeights @(676, 2, 150) -Settled $true -ProofRun $false
    Assert-Equal 'KEEP' $d.Verdict
    Assert-True ($d.Reason -match 'direction worse')
}

Test-Case 'a no-change pair has no direction rows and only the level error can move the decision' {
    $truth = [double[]]@(600, 2, 200)
    $rows = New-Rows 'h' $truth
    $same = Get-WeightScore -After $rows -Before $rows -Weights $current
    Assert-Equal 0 $same.DirectionRows
    Assert-Equal $null $same.PearsonChange -Message 'undefined with no change'
}

# Scores for a real pair whose counts did not change (a constant-factor change): level error is the only discriminator.
function Get-ZeroRowScores {
    $truth = [double[]]@(560, 2, 140)
    $rows = @(New-Rows 'h1' $truth) + @(New-Rows 'h2' $truth 10 2.0)
    return [PSCustomObject]@{
        Old = (Get-WeightScore -After $rows -Before $rows -Weights $current)
        New = (Get-WeightScore -After $rows -Before $rows -Weights $truth)
        Truth = $truth }
}

Test-Case 'an unsettled fit is KEEP with the reason, even when its rounded weights would ACCEPT' {
    $s = Get-ZeroRowScores
    $ok = Get-RetuneDecision -Old $s.Old -New $s.New -OldWeights $current -NewWeights $s.Truth -Settled $true -ProofRun $false
    Assert-Equal 'ACCEPT' $ok.Verdict 'the same scores ACCEPT when the fit settled'
    $d = Get-RetuneDecision -Old $s.Old -New $s.New -OldWeights $current -NewWeights $s.Truth -Settled $false -ProofRun $false
    Assert-Equal 'KEEP' $d.Verdict
    Assert-True ($d.Reason -match 'did not settle') $d.Reason
    Assert-Equal 10 (Get-RetuneExitCode $d.Verdict)
}

Test-Case 'settling needs every factor and the speed within 0.5% of 1' {
    Assert-True (Test-FitSettled -Factors @(1.001, 0.998, 1.0) -Speed 0.999)
    Assert-True (-not (Test-FitSettled -Factors @(1.001, 0.99, 1.0) -Speed 1.0))
    Assert-True (-not (Test-FitSettled -Factors @(1.0, 1.0, 1.0) -Speed 1.02))
}

Test-Case 'a proof run on a no-change pair is capped at KEEP and says direction is unmeasurable' {
    $s = Get-ZeroRowScores
    $d = Get-RetuneDecision -Old $s.Old -New $s.New -OldWeights $current -NewWeights $s.Truth -Settled $true -ProofRun $true
    Assert-Equal 'KEEP' $d.Verdict
    Assert-True ($d.Reason -match 'proof run, direction unmeasurable') $d.Reason
    Assert-True ($d.Reason -notmatch 'direction no worse') $d.Reason
    Assert-Equal 10 (Get-RetuneExitCode $d.Verdict)
}

Test-Case 'a real pair with 0 direction rows can ACCEPT on level error and says direction is unmeasured' {
    $s = Get-ZeroRowScores
    Assert-Equal 0 $s.New.DirectionRows
    $d = Get-RetuneDecision -Old $s.Old -New $s.New -OldWeights $current -NewWeights $s.Truth -Settled $true -ProofRun $false
    Assert-Equal 'ACCEPT' $d.Verdict
    Assert-True ($d.Reason -match 'direction unmeasured \(0 rows\)') $d.Reason
    Assert-True ($d.Reason -notmatch 'direction no worse') $d.Reason
}

Test-Case 'a few direction rows are called weak, and many say direction no worse' {
    $mk = { param($rows) [PSCustomObject]@{ Words = 20; MedianError = 5.0; WorstError = 9.0; DirectionRows = $rows; DirectionMatches = $rows; ScaleFreeMedianError = 1.0; ScaleFreeWorstError = 2.0; WordsByGrammar = @{} } }
    $better = & $mk 3; $better.MedianError = 2.0
    $weak = Get-RetuneDecision -Old (& $mk 3) -New $better -OldWeights $current -NewWeights @(600, 2, 200) -Settled $true -ProofRun $false
    Assert-Equal 'ACCEPT' $weak.Verdict
    Assert-True ($weak.Reason -match 'direction weak') $weak.Reason
    $better12 = & $mk 12; $better12.MedianError = 2.0
    $full = Get-RetuneDecision -Old (& $mk 12) -New $better12 -OldWeights $current -NewWeights @(600, 2, 200) -Settled $true -ProofRun $false
    Assert-True ($full.Reason -match '^direction no worse') $full.Reason
}

Test-Case 'a held-out grammar with too few scored words is refused by name, though the pool is large' {
    $truth = [double[]]@(560, 2, 140)
    $rows = @(New-Rows 'big' $truth 30) + @(New-Rows 'tiny' $truth 3)
    $score = Get-WeightScore -After $rows -Before $rows -Weights $current
    Assert-Equal 33 $score.Words
    Assert-Equal 3 $score.WordsByGrammar['tiny']
    $d = Get-RetuneDecision -Old $score -New $score -OldWeights $current -NewWeights $truth -Settled $true -ProofRun $false -HeldOutNames @('big', 'tiny')
    Assert-Equal 'REFUSED' $d.Verdict
    Assert-True ($d.Reason -match "'tiny' has only 3") $d.Reason
    $absent = Get-RetuneDecision -Old $score -New $score -OldWeights $current -NewWeights $truth -Settled $true -ProofRun $false -HeldOutNames @('big', 'gone')
    Assert-True ($absent.Reason -match "'gone' has only 0") $absent.Reason
}

Test-Case 'scale-free error is zero for a pure rescale, so an ACCEPT shows whether the shape improved' {
    $truth = [double[]]@(560, 2, 140)
    $rows = @(New-Rows 'h1' $truth) + @(New-Rows 'h2' $truth 10 2.0)
    $scaled = [double[]]@($truth | ForEach-Object { $_ * 2.3 })
    $a = Get-WeightScore -After $rows -Before $rows -Weights $scaled
    $b = Get-WeightScore -After $rows -Before $rows -Weights $truth
    Assert-True ($a.MedianError -gt 100) "level error of a rescale $($a.MedianError)"
    Assert-True ($a.ScaleFreeMedianError -lt 1e-9 -and $a.ScaleFreeWorstError -lt 1e-9) 'a rescale has no shape error'
    Assert-True ((Get-ShapeNote $a $b) -match 'unchanged, the gain is a rescale')
    # stale shape against matching shape
    $stale = Get-WeightScore -After $rows -Before $rows -Weights @(100, 2, 600)
    Assert-True ($stale.ScaleFreeMedianError -gt 1) "stale shape error $($stale.ScaleFreeMedianError)"
    Assert-True ((Get-ShapeNote $stale $b) -match '^shape improved')
    Assert-True ((Get-ShapeNote $b $stale) -match '^shape worse')
    $d = Get-RetuneDecision -Old $a -New $b -OldWeights $scaled -NewWeights $truth -Settled $true -ProofRun $false
    Assert-True ($d.Shape -match 'rescale') $d.Shape
}

Test-Case 'the driver hands the fit state and the proof-run flag to the decision, so neither can be dropped' {
    $text = Get-Content -LiteralPath $driver -Raw
    Assert-True ($text -match 'Get-RetuneDecision [^\r\n]*-Settled \$settled [^\r\n]*-ProofRun \(\[bool\]\$NoChangeCheck\)')
    Assert-True ($text -match 'Test-FitSettled')
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
    $d = Get-RetuneDecision -Old $score -New $score -OldWeights $current -NewWeights @(600, 2, 200) -Settled $true -ProofRun $false
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
