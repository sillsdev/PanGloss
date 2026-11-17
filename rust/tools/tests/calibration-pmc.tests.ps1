. "$PSScriptRoot/_test-harness.ps1"
. "$PSScriptRoot/../_instruction-compare.ps1"
. "$PSScriptRoot/../_calibration-set.ps1"
. "$PSScriptRoot/../_calibration-pmc.ps1"

function Assert-PmcThrows([scriptblock]$Action, [string]$Pattern) {
    $caught = $false
    try { & $Action | Out-Null } catch { $caught = $true; Assert-True ($_.Exception.Message -match $Pattern) $_.Exception.Message }
    Assert-True $caught "expected refusal matching $Pattern"
}

function New-PmcFixture([string]$Dir, [switch]$WithPmc) {
    $doc = [ordered]@{ version = 1; conformance = @{ commit = ('a' * 40) }; pangloss = @{ commit = ('b' * 40) }; sets = @(@{ name = 'test'; family = 'test'; rows = 3; rungs = @('1x', '4x', '16x') }) }
    [IO.File]::WriteAllLines((Join-Path $Dir 'test.counts.tsv'), [string[]]@("0`t1000`t0`t0`t0`t200000", "1`t4000`t0`t0`t0`t800000", "2`t16000`t0`t0`t0`t3200000"))
    if ($WithPmc) {
        $file = Join-Path $Dir 'test.pmc.tsv'
        [IO.File]::WriteAllLines($file, [string[]]@(foreach ($round in 0..2) { foreach ($idx in 0..2) { "$idx`t$round`t$([long](400000 * [Math]::Pow(4, $idx)))" } }))
        $doc['pmc'] = @{ reportOnly = $true; sets = @(@{ name = 'test'; rounds = 3; sha256 = (Get-FileHash $file -Algorithm SHA256).Hash.ToLowerInvariant() }) }
    }
    [IO.File]::WriteAllText((Join-Path $Dir 'manifest.json'), ($doc | ConvertTo-Json -Depth 8))
}

Test-Case 'measurement mode binds separately from Linux callgrind and refuses fewer than three rounds before tracing' {
    $path = Join-Path $PSScriptRoot '../calibration-set.ps1'
    $command = Get-Command $path
    $mode = @($command.ParameterSets | Where-Object Name -eq 'Pmc')
    Assert-Equal 1 $mode.Count
    Assert-Contains @($mode[0].Parameters.Name) 'MeasurePmc'
    Assert-Contains @($mode[0].Parameters.Name) 'Suite'
    Assert-Contains @($mode[0].Parameters.Name) 'Exe'
    $output = & pwsh -NoProfile -File $path -MeasurePmc -Suite absent -Exe absent -Rounds 2 2>&1
    Assert-True ($LASTEXITCODE -ne 0)
    Assert-True (($output -join "`n") -match 'Rounds' -and ($output -join "`n") -match '3')
}

Test-Case 'PMC rows retain noncontiguous original indexes and all zero-based rounds' {
    $dir = New-TestTempDir
    try {
        $path = Join-Path $dir 'rows.tsv'
        [IO.File]::WriteAllLines($path, [string[]]@(foreach ($idx in 2, 9) { foreach ($r in 0..2) { "$idx`t$r`t$(100 + $r)" } }))
        $rows = @(Read-CalibrationPmcRows $path @(@{ Idx = 2 }, @{ Idx = 9 }) 3)
        Assert-Equal '2 9' ($rows.Idx -join ' ')
        Assert-Equal '100 101 102' ($rows[1].Samples -join ' ')
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force }
}

Test-Case 'PMC serialization uses canonical UTF-8 LF bytes so Git checkout cannot invalidate its manifest hash' {
    $dir = New-TestTempDir
    try {
        $path = Join-Path $dir 'rows.tsv'
        Write-CalibrationPmcFile -Path $path -Lines @("0`t0`t100", "0`t1`t101", "0`t2`t102") -Rows @(@{ Idx = 0 }) -Rounds 3
        $bytes = [IO.File]::ReadAllBytes($path)
        Assert-False ($bytes -contains 13) 'CRLF would be changed by the repository LF checkout policy'
        Assert-Equal 48 $bytes[0] 'UTF-8 file starts with index 0, without a BOM'
        Assert-Equal 10 $bytes[-1]
        Assert-Equal 3 @(Read-CalibrationPmcRows $path @(@{ Idx = 0 }) 3)[0].Samples.Count
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force }
}

Test-Case 'PMC parser refuses text, duplicates, unknown indexes, incomplete rounds and zero counts' {
    $dir = New-TestTempDir
    try {
        $path = Join-Path $dir 'rows.tsv'
        foreach ($case in @(
            @{ Lines = @("0`t0`t10", "0`t1`t20", "0`t2`tword"); Error = 'invalid PMC row' },
            @{ Lines = @("0`t0`t10", "0`t0`t20"); Error = 'duplicate' },
            @{ Lines = @("1`t0`t10"); Error = 'absent' },
            @{ Lines = @("0`t0`t10", "0`t1`t20"); Error = 'expected 3' },
            @{ Lines = @("0`t0`t0"); Error = 'invalid PMC row' },
            @{ Lines = @("0`t3`t10"); Error = 'exceeds' }
        )) {
            [IO.File]::WriteAllLines($path, [string[]]$case.Lines)
            Assert-PmcThrows { Read-CalibrationPmcRows $path @(@{ Idx = 0 }) 3 } $case.Error
        }
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force }
}

Test-Case 'spread is sample CV and range over mean, not a fixed #260 noise allowance' {
    $s = Get-PmcSpread @(99, 100, 101)
    Assert-Equal 100 $s.Mean
    Assert-Equal 1 $s.CvPercent
    Assert-Equal 2 $s.RangePercent
    Assert-PmcThrows { Get-PmcSpread @(100, 101) } 'three'
}

Test-Case 'per-word trace extraction validates phases and trace integrity before returning counts' {
    $r = [PSCustomObject]@{ rc = 0; events_lost = 0; buffers_lost = 0; callback_errors = 0; switches = 10; counters = 1; gaps_on_target = 0; instructions = @{ '7' = 200000000 }; phase = @{ '7' = 2 }; parse_instructions = @{ '7' = 30 }; word_instructions = @{ '7' = @(10, 20) } }
    Assert-Equal '10 20' ((Get-PmcPhaseCounts $r @(7) 2) -join ' ')
    Assert-PmcThrows { Get-PmcPhaseCounts $r @(7) 3 } 'expected 3'
    $r.parse_instructions['7'] = 31
    Assert-PmcThrows { Get-PmcPhaseCounts $r @(7) 2 } 'do not sum'
    $r.parse_instructions['7'] = 30
    $r.phase['7'] = 0
    Assert-PmcThrows { Get-PmcPhaseCounts $r @(7) 2 } 'phase marks'
    $r.phase['7'] = 2; $r.events_lost = 1
    Assert-PmcThrows { Get-PmcPhaseCounts $r @(7) 2 } 'lost events'
}

Test-Case 'optional PMC scores current weights without changing the callgrind gate or counts' {
    $dir = New-TestTempDir
    try {
        New-PmcFixture $dir -WithPmc
        $c = Read-CalibrationManifest $dir
        $before = Get-SetScore $c.Sets @(200, 1.25, 0, 150)
        $pmc = Get-CalibrationPmcReport $dir $c @(200, 1.25, 0, 150)
        Assert-True $pmc.ReportOnly
        Assert-Equal 'available' $pmc.Status
        Assert-Equal 2 $pmc.Families[0].Ratio
        Assert-Equal 50 $pmc.Families[0].MeanError
        Assert-Equal 0 $pmc.Families[0].CvPercent
        Assert-Equal 0 $pmc.Sets[0].CvPercent
        Assert-Equal 3 $pmc.Sets[0].RoundInstructions.Count
        Assert-False $pmc.Families[0].Disagree 'common Windows/Linux level is not a shape disagreement'
        Assert-Equal $before.MeanOfFamilyErrors (Get-SetScore $c.Sets @(200, 1.25, 0, 150)).MeanOfFamilyErrors
        Assert-Equal 200000 $c.Sets[0].Rows[0].Instructions
        $file = Join-Path $dir 'test.pmc.tsv'
        [IO.File]::AppendAllText($file, "0`t0`t1`n")
        $invalid = Get-CalibrationPmcReport $dir $c @(200, 1.25, 0, 150)
        Assert-Equal 'invalid' $invalid.Status
        Assert-True ($invalid.Errors[0] -match 'hash differs')
        Assert-Equal 0 $invalid.Families.Count
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force }
}

Test-Case 'more than 2 percent rung drift is called out report-only with the #294 caveat' {
    $dir = New-TestTempDir
    try {
        New-PmcFixture $dir -WithPmc
        $file = Join-Path $dir 'test.pmc.tsv'
        $text = [IO.File]::ReadAllText($file).Replace('6400000', '12800000')
        [IO.File]::WriteAllText($file, $text)
        $doc = Get-Content (Join-Path $dir 'manifest.json') -Raw | ConvertFrom-Json -AsHashtable
        $doc.pmc.sets[0].sha256 = (Get-FileHash $file -Algorithm SHA256).Hash.ToLowerInvariant()
        [IO.File]::WriteAllText((Join-Path $dir 'manifest.json'), ($doc | ConvertTo-Json -Depth 8))
        $c = Read-CalibrationManifest $dir
        $report = Get-CalibrationPmcReport $dir $c @(200, 1.25, 0, 150)
        Assert-True $report.Families[0].Disagree
        $printed = (Format-CalibrationPmcReport $report) -join "`n"
        Assert-True ($printed -match '>2% disagreement \(report-only\)' -and $printed -match '#294')
        Assert-Equal 0 @( (Get-SetScore $c.Sets @(200, 1.25, 0, 150)).Flagged ).Count
        $output = & pwsh -NoProfile -File (Join-Path $PSScriptRoot '../calibration-set.ps1') -Score -Dir $dir 2>&1
        Assert-Equal 0 $LASTEXITCODE ($output -join "`n")
        Assert-True (($output -join "`n") -match '>2% disagreement')
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force }
}

Test-Case 'Linux-compatible scoring works without PMC files, even when the manifest lists optional data' {
    $dir = New-TestTempDir
    try {
        foreach ($listed in $false, $true) {
            New-PmcFixture $dir -WithPmc:$listed
            Remove-Item -LiteralPath (Join-Path $dir 'test.pmc.tsv') -ErrorAction SilentlyContinue
            $c = Read-CalibrationManifest $dir
            $report = Get-CalibrationPmcReport $dir $c @(200, 1.25, 0, 150)
            Assert-Equal 'unavailable' $report.Status
            Assert-Equal 0 $report.Families.Count
            Assert-Equal 'test' ($report.Unavailable -join ',')
            $scriptPath = Join-Path $PSScriptRoot '../calibration-set.ps1'
            $wrapper = Join-Path $dir 'score.ps1'
            [IO.File]::WriteAllText($wrapper, 'param($s,$d); function global:Get-ScheduledTask { throw "platform service called by scorer" }; & $s -Score -Dir $d')
            $output = & pwsh -NoProfile -File $wrapper $scriptPath $dir 2>&1
            Assert-Equal 0 $LASTEXITCODE ($output -join "`n")
            Assert-True (($output -join "`n") -match 'no optional PMC data available')
        }
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force }
}

Test-Case 'a missing optional set is reported as partial without discarding valid PMC results' {
    $dir = New-TestTempDir
    try {
        New-PmcFixture $dir -WithPmc
        $path = Join-Path $dir 'manifest.json'
        $doc = Get-Content $path -Raw | ConvertFrom-Json -AsHashtable
        $second = $doc.sets[0].Clone(); $second.name = 'second'
        $doc.sets += $second
        Copy-Item (Join-Path $dir 'test.counts.tsv') (Join-Path $dir 'second.counts.tsv')
        [IO.File]::WriteAllText($path, ($doc | ConvertTo-Json -Depth 8))
        $report = Get-CalibrationPmcReport $dir (Read-CalibrationManifest $dir) @(200, 1.25, 0, 150)
        Assert-Equal 'partial' $report.Status
        Assert-Equal 'second' ($report.Unavailable -join ',')
        Assert-Equal 1 $report.Families.Count
        Assert-Equal 0 $report.Errors.Count
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force }
}

Test-Case 'stored PMC data covers the same callgrind indexes, has portable hashes, and pins binary and machine provenance' {
    $dir = Join-Path $PSScriptRoot '../calibration'
    $c = Read-CalibrationManifest $dir
    Assert-True $c.Manifest.pmc.reportOnly
    $p = $c.Manifest.pmc.provenance
    Assert-True ($p.sourceCommit -match '^[a-f0-9]{40}$')
    Assert-True ($p.binarySha256 -match '^[a-f0-9]{64}$' -and $p.readerSha256 -match '^[a-f0-9]{64}$')
    Assert-Equal $p.readerSha256 (Get-FileHash (Join-Path $PSScriptRoot '../instruction_compare_etl.py') -Algorithm SHA256).Hash.ToLowerInvariant()
    Assert-Equal 'cost-calibration,foma-tools' $p.features
    Assert-Equal 'release' $p.profile
    Assert-True ($p.machine.cpu -and $p.machine.osVersion -and $p.machine.name)
    $report = Get-CalibrationPmcReport $dir $c @(200, 1.25, 0, 150)
    Assert-Equal 'available' $report.Status
    Assert-Equal $c.Sets.Count $report.Sets.Count
    Assert-Equal 0 $report.Unavailable.Count
    foreach ($s in $report.Sets) {
        Assert-True ($s.Rounds -ge 3)
        $file = Join-Path $dir "$($s.Set).pmc.tsv"
        Assert-False ([IO.File]::ReadAllBytes($file) -contains 13) "$($s.Set) has nonportable CRLF bytes"
    }
}

Test-Case 'invalid optional PMC never suppresses JSON callgrind results or changes gate exit status' {
    $dir = New-TestTempDir
    try {
        $scriptPath = Join-Path $PSScriptRoot '../calibration-set.ps1'
        foreach ($nonlinear in $false, $true) {
            foreach ($damage in 'hash', 'partial', 'duplicate', 'rounds', 'mixed') {
                New-PmcFixture $dir -WithPmc
                if ($nonlinear) {
                    $cg = Join-Path $dir 'test.counts.tsv'
                    [IO.File]::WriteAllText($cg, [IO.File]::ReadAllText($cg).Replace('3200000', '6400000'))
                }
                $baseline = (& pwsh -NoProfile -File $scriptPath -Score -Dir $dir -Json) -join "`n"
                Assert-Equal 0 $LASTEXITCODE
                $baselineDoc = $baseline | ConvertFrom-Json -AsHashtable
                $null = $baselineDoc.Remove('pmcReportOnly')
                $null = & pwsh -NoProfile -File $scriptPath -Score -Dir $dir
                $gateExit = $LASTEXITCODE
                Assert-Equal ([int]$nonlinear) $gateExit
                $file = Join-Path $dir 'test.pmc.tsv'
                $doc = Get-Content (Join-Path $dir 'manifest.json') -Raw | ConvertFrom-Json -AsHashtable
                switch ($damage) {
                    'hash' { [IO.File]::AppendAllText($file, "0`t0`t1`n") }
                    'partial' { [IO.File]::WriteAllText($file, "0`t0`t400000`n") }
                    'duplicate' { [IO.File]::AppendAllText($file, "0`t0`t400000`n") }
                    'rounds' { $doc.pmc.sets[0].rounds = 4 }
                    'mixed' {
                        Copy-Item (Join-Path $dir 'test.counts.tsv') (Join-Path $dir 'second.counts.tsv')
                        $second = $doc.sets[0].Clone(); $second.name = 'second'
                        $doc.sets += $second
                        $secondFile = Join-Path $dir 'second.pmc.tsv'
                        [IO.File]::WriteAllLines($secondFile, [string[]]@(foreach ($round in 0..3) { foreach ($idx in 0..2) { "$idx`t$round`t$([long](400000 * [Math]::Pow(4, $idx)))" } }))
                        $doc.pmc.sets += @{ name = 'second'; rounds = 4; sha256 = (Get-FileHash $secondFile -Algorithm SHA256).Hash.ToLowerInvariant() }
                    }
                }
                if ($damage -ne 'hash') { $doc.pmc.sets[0].sha256 = (Get-FileHash $file -Algorithm SHA256).Hash.ToLowerInvariant() }
                [IO.File]::WriteAllText((Join-Path $dir 'manifest.json'), ($doc | ConvertTo-Json -Depth 8))
                if ($damage -eq 'mixed') {
                    # Adding a callgrind set changes the baseline; compare against the same directory without PMC metadata.
                    $withoutPmc = $doc.Clone(); $null = $withoutPmc.Remove('pmc')
                    [IO.File]::WriteAllText((Join-Path $dir 'manifest.json'), ($withoutPmc | ConvertTo-Json -Depth 8))
                    $baselineDoc = ((& pwsh -NoProfile -File $scriptPath -Score -Dir $dir -Json) -join "`n") | ConvertFrom-Json -AsHashtable
                    Assert-Equal 0 $LASTEXITCODE
                    $null = $baselineDoc.Remove('pmcReportOnly')
                    [IO.File]::WriteAllText((Join-Path $dir 'manifest.json'), ($doc | ConvertTo-Json -Depth 8))
                }
                $output = & pwsh -NoProfile -File $scriptPath -Score -Dir $dir -Json 2>&1
                Assert-Equal 0 $LASTEXITCODE ($output -join "`n")
                $actual = ($output -join "`n") | ConvertFrom-Json -AsHashtable
                Assert-Equal 'invalid' $actual.pmcReportOnly.Status
                Assert-Equal 1 $actual.pmcReportOnly.Errors.Count
                if ($damage -eq 'mixed') { Assert-True ($actual.pmcReportOnly.Errors[0] -match 'mixes round counts') }
                Assert-Equal 0 $actual.pmcReportOnly.Families.Count
                $null = $actual.Remove('pmcReportOnly')
                Assert-Equal ($baselineDoc | ConvertTo-Json -Depth 8 -Compress) ($actual | ConvertTo-Json -Depth 8 -Compress)
                $text = & pwsh -NoProfile -File $scriptPath -Score -Dir $dir 2>&1
                Assert-Equal $gateExit $LASTEXITCODE ($text -join "`n")
                Assert-True (($text -join "`n") -match 'PMC status: invalid' -and ($text -join "`n") -match 'PMC integrity error:')
            }
        }
    } finally { Remove-Item -LiteralPath $dir -Recurse -Force }
}

Write-TestSummary
