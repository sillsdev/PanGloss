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

Test-Case 'the real-grammar guard is worse only when the candidate adds more than the margin to the mean error' {
    $totals = [PSCustomObject]@{ grammars = @(
            [PSCustomObject]@{ name = 'a'; scored = 10; work = 1000; bytes = 100000; registers = 0; allocations = 1000; instructions = 500000 },
            [PSCustomObject]@{ name = 'b'; scored = 10; work = 2000; bytes = 300000; registers = 0; allocations = 2000; instructions = 1100000 }) }
    $cur = Get-RealGuard -Totals $totals -Weights $current
    $same = Get-RealGuard -Totals $totals -Weights $current
    Assert-Equal 'PASS' (Get-RealGuardVerdict -Current $cur -Candidate $same).Verdict
    $far = Get-RealGuard -Totals $totals -Weights ([double[]]@(400, 2.5, 0, 300))
    Assert-Equal 'WORSE' (Get-RealGuardVerdict -Current $cur -Candidate $far).Verdict
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

Write-TestSummary
