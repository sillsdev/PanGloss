<#
  .DESCRIPTION
  Covers the pure logic in rust/tools/_calibration-set.ps1: error and linearity scoring on synthetic counts, the real-grammar guard, the manifest
  reader's refusals, and the stored set committed under rust/tools/calibration. Invented numbers only; no measurement is started.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_calibration-set.ps1"

$current = [double[]]@(200, 1.25, 0, 150)

# Rows whose instructions are `$Factor` times the cost of the weights, one per rung, so any drift between rungs is chosen by the test.
function New-LadderRows([double[]]$Factors, [string[]]$Rungs) {
    $rows = @()
    for ($i = 0; $i -lt $Factors.Count; $i++) {
        $work = 2000 * [Math]::Pow(4, $i); $bytes = 90000 * [Math]::Pow(4, $i); $allocs = 800 * [Math]::Pow(4, $i)
        $row = [PSCustomObject]@{ Idx = $i; Work = $work; Bytes = $bytes; Registers = 0.0; Allocations = $allocs; Instructions = 0.0; Rung = $Rungs[$i] }
        $row.Instructions = (Get-WeightedCost -Row $row -Weights $current) * $Factors[$i]
        $rows += $row
    }
    return $rows
}
$rungs = '1x', '4x', '16x', '64x', '256x'

Test-Case 'a family whose instructions are exactly the cost has no error and a unit slope' {
    $f = Get-FamilyScore -Family 'f' -Rows (New-LadderRows @(1, 1, 1, 1, 1) $rungs) -Weights $current
    Assert-True ($f.MeanError -lt 1e-9) "error $($f.MeanError)"
    Assert-True ([Math]::Abs($f.Slope - 1) -lt 1e-9) "slope $($f.Slope)"
    Assert-False $f.Flagged
}

Test-Case 'a uniform miss is level error but no drift, so the family stays linear' {
    $f = Get-FamilyScore -Family 'f' -Rows (New-LadderRows @(1.3, 1.3, 1.3, 1.3, 1.3) $rungs) -Weights $current
    Assert-True ([Math]::Abs($f.MeanError - 100 * (1 - 1 / 1.3)) -lt 1e-6)
    Assert-True ($f.MaxDrift -lt 1e-9)
    Assert-False $f.Flagged
}

Test-Case 'a ladder whose cost/instruction ratio drifts past the limit is flagged, and not below it' {
    $drifting = New-LadderRows @(1.0, 1.0, 1.0, 1.0, 2.0) $rungs
    Assert-True (Get-FamilyScore -Family 'f' -Rows $drifting -Weights $current).Flagged
    $mild = New-LadderRows @(1.0, 1.04, 1.0, 1.0, 1.05) $rungs
    Assert-False (Get-FamilyScore -Family 'f' -Rows $mild -Weights $current).Flagged
    Assert-True (Get-FamilyScore -Family 'f' -Rows $mild -Weights $current -DriftPercent 1).Flagged
}

Test-Case 'wasted-search and band rungs are reported but never flag a family' {
    $rows = New-LadderRows @(1, 1, 1, 3, 3) @('1x', '4x', '16x', 'wasted', 'band')
    $f = Get-FamilyScore -Family 'f' -Rows $rows -Weights $current
    Assert-False $f.Flagged
    Assert-True (@($f.Rungs | Where-Object { -not $_.Ladder }).Count -eq 2)
}

Test-Case 'rows under the work floor are left out of the score' {
    $rows = @(New-LadderRows @(1, 1, 1) @('1x', '4x', '16x'))
    $rows[0].Work = 10
    $f = Get-FamilyScore -Family 'f' -Rows $rows -Weights $current
    Assert-Equal 2 $f.Scored
}

Test-Case 'rungs sort by size, then the non-ladder ones' {
    $names = '256x', 'wasted', '4x', '1x', '16x', 'band' | Sort-Object { Get-RungOrder $_ }
    Assert-Equal '1x 4x 16x 256x wasted band' ($names -join ' ')
    $sizes = '50k', '1k', '10k', '5k' | Sort-Object { Get-RungOrder $_ }
    Assert-Equal '1k 5k 10k 50k' ($sizes -join ' ')
}

Test-Case 'the real-grammar guard gates on the scale-free error: a pure rescale passes, a changed split fails' {
    $totals = [PSCustomObject]@{ grammars = @(
            [PSCustomObject]@{ name = 'a'; scored = 10; work = 1000; bytes = 100000; registers = 0; allocations = 1000; instructions = 500000 },
            [PSCustomObject]@{ name = 'b'; scored = 10; work = 2000; bytes = 300000; registers = 0; allocations = 2000; instructions = 1100000 }) }
    $cur = Get-RealGuard -Totals $totals -Weights $current
    $same = Get-RealGuard -Totals $totals -Weights $current
    Assert-Equal 'PASS' (Get-RealGuardVerdict -Current $cur -Candidate $same).Verdict
    # Every weight doubled: the level error changes a lot, no change prediction does.
    $rescaled = Get-RealGuard -Totals $totals -Weights ([double[]]@(400, 2.5, 0, 300))
    Assert-True ([Math]::Abs($rescaled.MeanError - $cur.MeanError) -gt 20) 'the level did move'
    Assert-Equal 'PASS' (Get-RealGuardVerdict -Current $cur -Candidate $rescaled).Verdict
    # Allocations dropped and bytes raised: the grammars no longer sit in the same ratio to each other.
    $split = Get-RealGuard -Totals $totals -Weights ([double[]]@(200, 6, 0, 0))
    Assert-Equal 'WORSE' (Get-RealGuardVerdict -Current $cur -Candidate $split).Verdict
}

Test-Case 'a family marked excludeFromMean is scored and shown but left out of the scale and the means' {
    $a = New-LadderRows @(1.0, 1.0, 1.0) @('1x', '4x', '16x')
    $b = New-LadderRows @(2.0, 2.0, 2.0) @('1x', '4x', '16x')
    $sets = @(
        [PSCustomObject]@{ Name = 'a'; Family = 'a'; Rows = $a; ExcludeFromMean = $false; Gap = $null },
        [PSCustomObject]@{ Name = 'b'; Family = 'b'; Rows = $b; ExcludeFromMean = $true; Gap = 'a gap' })
    $score = Get-SetScore -Sets $sets -Weights $current
    Assert-Equal 2 $score.Families.Count
    Assert-True ($score.MeanOfFamilyErrors -lt 1e-9) "mean $($score.MeanOfFamilyErrors)"
    Assert-True ([Math]::Abs($score.Scale - 1) -lt 1e-9) "scale $($score.Scale)"
    Assert-True (($score.Families | Where-Object Family -eq 'b').ExcludedFromMean)
    Assert-True ((($score.Families | Where-Object Family -eq 'b').MeanError) -gt 40) 'b is still scored'
    Assert-True ((Format-FamilyTable $score | Where-Object { $_ -match 'not in the means: b: a gap' }).Count -eq 1)
}

Test-Case 'the drift limit is 7 percent, the size of the changes it exists to predict' {
    Assert-Equal 7.0 $script:DefaultDriftPercent
    $f = Get-FamilyScore -Family 'f' -Rows (New-LadderRows @(1.0, 1.0, 1.0, 1.0, 1.2) $rungs) -Weights $current
    Assert-True $f.Flagged "max drift $($f.MaxDrift)"
}

# Totals of a pair side, as the pair files store them; instructions are chosen by the test.
function New-Totals([long]$Work, [long]$Bytes, [long]$Allocs, [long]$Instr) { [PSCustomObject]@{ work = $Work; bytes = $Bytes; registers = 0; allocations = $Allocs; instructions = $Instr } }
function New-Pair([string]$Name, $Families) { [PSCustomObject]@{ name = $Name; title = 'invented'; families = @($Families) } }

Test-Case 'a pair is scored on the change of each total: predicted against measured, in points, with the sign' {
    # Cost before 200*1000 + 1.25*100000 + 150*1000 = 475000. After: work -10%, bytes +20%, allocations +10%.
    $before = New-Totals 1000 100000 1000 500000
    $after = New-Totals 900 120000 1100 500000
    $pair = New-Pair 'p' @([PSCustomObject]@{ family = 'f'; words = 4; before = $before; after = $after })
    $s = Get-PairScore -Pair $pair -Weights $current
    $predicted = 100 * ((200 * 900 + 1.25 * 120000 + 150 * 1100) / 475000 - 1)
    Assert-True ([Math]::Abs($s.Families[0].Predicted - $predicted) -lt 1e-9) "predicted $($s.Families[0].Predicted)"
    Assert-True ([Math]::Abs($s.Families[0].Measured) -lt 1e-9) 'instructions did not move'
    Assert-True ([Math]::Abs($s.MeanError - [Math]::Abs($predicted)) -lt 1e-9)
    Assert-Equal 0 $s.SignChecked 'a family that did not move has no sign to read'
}

Test-Case 'a pair gate fails on a mean change error over the limit (a wrong sign is one), and passes on a right prediction; the sign is counted' {
    $before = New-Totals 1000 100000 1000 475000
    $right = New-Totals 900 100000 1000 (475000 - 20000)
    $wrongSign = New-Totals 1100 100000 1000 (475000 - 20000)
    $good = Get-PairScore -Pair (New-Pair 'p' @([PSCustomObject]@{ family = 'f'; words = 4; before = $before; after = $right })) -Weights $current
    Assert-Equal 1 $good.SignChecked
    Assert-Equal 'PASS' (Get-PairGate $good).Verdict
    $bad = Get-PairScore -Pair (New-Pair 'p' @([PSCustomObject]@{ family = 'f'; words = 4; before = $before; after = $wrongSign })) -Weights $current
    Assert-Equal 0 $bad.SignAgree
    Assert-Equal 'FAIL' (Get-PairGate $bad).Verdict
    $far = New-Totals 1000 100000 1000 (475000 * 2)
    $off = Get-PairScore -Pair (New-Pair 'p' @([PSCustomObject]@{ family = 'f'; words = 4; before = $before; after = $far })) -Weights $current
    Assert-Equal 'FAIL' (Get-PairGate $off).Verdict
}

Test-Case 'a weight set with the right level but the wrong split of work and allocations is caught by a pair, not by the level' {
    # The edit trades allocations for work: work -10%, allocations +30%, bytes unchanged; instructions follow the current weights exactly.
    $before = New-Totals 1000 100000 1000 475000
    $after = New-Totals 900 100000 1300 (200 * 900 + 1.25 * 100000 + 150 * 1300)
    $pair = New-Pair 'p' @([PSCustomObject]@{ family = 'f'; words = 4; before = $before; after = $after })
    $cur = Get-PairScore -Pair $pair -Weights $current
    Assert-True ($cur.MeanError -lt 1e-9) 'the current weights are exact on this pair'
    # No work term, allocations carry it all: the same level on `before` (cost 475000), a different split.
    $other = [double[]]@(0, 0, 0, 475)
    Assert-True ([Math]::Abs((Get-TotalsCost $before $other) - 475000) -lt 1e-6) 'same level on before'
    $cand = Get-PairScore -Pair $pair -Weights $other
    Assert-True ($cand.MeanError -gt 5) "candidate error $($cand.MeanError)"
    $v = Get-PairVerdict -Current $cur -Candidate $cand
    Assert-Equal 'WORSE' $v.Verdict
}

Test-Case 'a report-only pair is shown but neither gates nor fails a candidate' {
    $before = New-Totals 1000 100000 1000 475000
    $far = New-Totals 1000 100000 1000 (475000 * 2)
    $pair = New-Pair 'p' @([PSCustomObject]@{ family = 'f'; words = 4; before = $before; after = $far })
    $pair | Add-Member -NotePropertyName gate -NotePropertyValue $false
    $pair | Add-Member -NotePropertyName gateNote -NotePropertyValue 'cannot move'
    $s = Get-PairScore -Pair $pair -Weights $current
    Assert-Equal 'REPORT-ONLY' (Get-PairGate $s).Verdict
    Assert-True (Get-PairGate $s).Pass
    Assert-Equal 'REPORT-ONLY' (Get-PairVerdict -Current $s -Candidate $s).Verdict
    Assert-True ((Format-PairTable $s | Where-Object { $_ -match 'report-only, not a gate: cannot move' }).Count -eq 1)
}

Test-Case 'New-PairFamilies aligns the two sides by word index, drops words under the work floor on both, and keeps no word text' {
    $b = @(
        [PSCustomObject]@{ Idx = 0; Work = 5000; Bytes = 10; Registers = 0; Allocations = 2; Instructions = 100 },
        [PSCustomObject]@{ Idx = 1; Work = 50; Bytes = 10; Registers = 0; Allocations = 2; Instructions = 100 },
        [PSCustomObject]@{ Idx = 2; Work = 50; Bytes = 10; Registers = 0; Allocations = 2; Instructions = 100 },
        [PSCustomObject]@{ Idx = 3; Work = 5000; Bytes = 10; Registers = 0; Allocations = 2; Instructions = 100 })
    $a = @(
        [PSCustomObject]@{ Idx = 0; Work = 4000; Bytes = 8; Registers = 0; Allocations = 1; Instructions = 90 },
        [PSCustomObject]@{ Idx = 1; Work = 50; Bytes = 8; Registers = 0; Allocations = 1; Instructions = 90 },
        [PSCustomObject]@{ Idx = 2; Work = 3000; Bytes = 8; Registers = 0; Allocations = 1; Instructions = 90 })
    $fam = @(New-PairFamilies -Lists @([PSCustomObject]@{ Family = 'f'; Before = $b; After = $a }, [PSCustomObject]@{ Family = 'f'; Before = $b[0..0]; After = $a[0..0] }))
    Assert-Equal 1 $fam.Count
    Assert-Equal 3 $fam[0].words 'word 0 twice, word 2 (floor reached on the after side only); word 1 under the floor on both, word 3 has no after row'
    Assert-Equal 10050 $fam[0].before.work 'before work: 5000 + 50 + 5000'
    Assert-Equal 11000 $fam[0].after.work 'after work: 4000 + 3000 + 4000'
    Assert-Equal 'allocations bytes instructions registers work' (($fam[0].before.Keys | Sort-Object) -join ' ')
}

Test-Case 'a manifest pair whose file is missing, or named otherwise, is refused' {
    $dir = New-TestTempDir
    try {
        [IO.File]::WriteAllText((Join-Path $dir 'manifest.json'), '{"version":1,"sets":[],"pairs":[{"name":"p","file":"pairs/p.json"}]}')
        $threw = $false
        try { $null = Read-CalibrationPairs -Dir $dir } catch { $threw = $_.Exception.Message -match 'file is missing' }
        Assert-True $threw 'expected a missing-file refusal'
        New-Item -ItemType Directory -Path (Join-Path $dir 'pairs') | Out-Null
        [IO.File]::WriteAllText((Join-Path $dir 'pairs/p.json'), '{"version":1,"name":"q","families":[{"family":"f"}]}')
        $threw = $false
        try { $null = Read-CalibrationPairs -Dir $dir } catch { $threw = $_.Exception.Message -match "named 'q'" }
        Assert-True $threw 'expected a name refusal'
        [IO.File]::WriteAllText((Join-Path $dir 'pairs/p.json'), '{"version":1,"name":"p","families":[{"family":"f"}]}')
        Assert-Equal 1 @(Read-CalibrationPairs -Dir $dir).Count
        [IO.File]::WriteAllText((Join-Path $dir 'manifest.json'), '{"version":1,"sets":[]}')
        Assert-Equal 0 @(Read-CalibrationPairs -Dir $dir).Count
    } finally { Remove-Item -Recurse -Force $dir }
}

Test-Case 'a manifest whose row count disagrees with its counts file is refused' {
    $dir = New-TestTempDir
    try {
        [IO.File]::WriteAllText((Join-Path $dir 's.counts.tsv'), "0`t2000`t90000`t0`t800`t1000000`n")
        $m = '{"version":1,"sets":[{"name":"s","family":"f","rows":2,"rungs":["1x"]}]}'
        [IO.File]::WriteAllText((Join-Path $dir 'manifest.json'), $m)
        $threw = $false
        try { $null = Read-CalibrationManifest -Dir $dir } catch { $threw = $_.Exception.Message -match '2 row' }
        Assert-True $threw 'expected a row-count refusal'
        [IO.File]::WriteAllText((Join-Path $dir 'manifest.json'), $m.Replace('"rows":2', '"rows":1'))
        $ok = Read-CalibrationManifest -Dir $dir
        Assert-Equal '1x' $ok.Sets[0].Rows[0].Rung
        Assert-Equal 'f' $ok.Sets[0].Rows[0].Family
    } finally { Remove-Item -Recurse -Force $dir }
}

Test-Case 'the committed set loads, pins both commits, carries no words and scores every family' {
    $dir = Join-Path $PSScriptRoot '..\calibration'
    $set = Read-CalibrationManifest -Dir $dir
    Assert-True ($set.Manifest.conformance.commit -match '^[0-9a-f]{40}$') 'conformance pin'
    Assert-True ($set.Manifest.pangloss.commit -match '^[0-9a-f]{40}$') 'PanGloss pin'
    Assert-Equal ((Get-Content -LiteralPath (Join-Path $dir 'conformance.commit') -Raw).Trim()) $set.Manifest.conformance.commit
    foreach ($s in $set.Sets) {
        foreach ($line in [IO.File]::ReadAllLines((Join-Path $dir "$($s.Name).counts.tsv"))) {
            Assert-True ($line -match '^\d+(\t\d+){5}$') "set $($s.Name) has a row that is not six integers: $line"
        }
    }
    $score = Get-SetScore -Sets $set.Sets -Weights $current
    Assert-True ($score.Families.Count -ge 12) "families: $($score.Families.Count)"
    Assert-True (@($score.Families | Where-Object { $null -eq $_.MeanError }).Count -eq 0)
}

Test-Case 'the committed pairs load, hold only counters, and every gating pair passes its gate at the current weights' {
    $dir = Join-Path $PSScriptRoot '..\calibration'
    $pairs = @(Read-CalibrationPairs -Dir $dir)
    Assert-True ($pairs.Count -ge 2) "pairs: $($pairs.Count)"
    Assert-True (@($pairs | Where-Object { -not ($_.PSObject.Properties.Name -contains 'gate') -or $_.gate }).Count -ge 2) 'at least two pairs gate'
    foreach ($p in $pairs) {
        Assert-True ($p.conformance.commit -match '^[0-9a-f]{40}$') "pair $($p.name): conformance pin"
        Assert-True ($p.before.binarySha256 -match '^[0-9a-f]{64}$' -and $p.after.binarySha256 -match '^[0-9a-f]{64}$') "pair $($p.name): binary hashes"
        foreach ($f in $p.families) {
            foreach ($side in 'before', 'after') {
                Assert-Equal 'allocations bytes instructions registers work' ((($f.$side.PSObject.Properties.Name) | Sort-Object) -join ' ') "pair $($p.name) $($f.family) $side keys"
                Assert-True ($f.$side.allocations -gt 0) "pair $($p.name) $($f.family) $side has no allocations column"
            }
        }
        $s = Get-PairScore -Pair $p -Weights $current
        Assert-True ((Get-PairGate $s).Pass) "pair $($p.name) fails its gate at the current weights: mean $($s.MeanError), signs $($s.SignAgree) of $($s.SignChecked)"
    }
}

Write-TestSummary
