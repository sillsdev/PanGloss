# Windows tracing orchestration; arithmetic and trace validation live in _calibration-pmc.ps1.

function Invoke-CalibrationPmc {
    param([string]$Suite, [string]$Exe, [string]$Dir, [int]$Rounds, [int]$Cpu, [string[]]$Only, [string]$SaveDir)
    if (-not $IsWindows) { throw '-MeasurePmc needs Windows; use -Measure for Linux callgrind, or -Score without PMC files' }
    if ($Cpu -lt 0 -or $Cpu -ge [Environment]::ProcessorCount) { throw "CPU $Cpu is unavailable" }
    $Suite = (Resolve-Path -LiteralPath $Suite).Path
    $Exe = (Resolve-Path -LiteralPath $Exe).Path
    $Dir = (Resolve-Path -LiteralPath $Dir).Path
    $calibration = Read-CalibrationManifest -Dir $Dir
    $manifestPath = Join-Path $Dir 'manifest.json'
    $doc = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json -AsHashtable
    $pin = $doc.conformance.commit
    if ((& git -C $Suite rev-parse HEAD).Trim() -ne $pin) { throw "PMC suite must be at calibration pin $pin" }
    if (@(& git -C $Suite status --porcelain).Count) { throw 'PMC suite has local changes' }
    $describe = (& $Exe --describe) -join "`n"
    foreach ($flag in '--phase-marks', '--word-phase-marks', '--no-flush') {
        if ($LASTEXITCODE -ne 0 -or -not (Test-DescribesBatchFlag $describe $flag)) { throw "PMC binary must describe batch $flag; build release pg-cli with cost-calibration,foma-tools" }
    }
    $python = (Get-Command python -ErrorAction Stop).Source
    $found = @(Get-ScheduledTask -TaskPath '\PanGloss\' | ForEach-Object TaskName)
    Assert-PmcTasksPresent -Found $found
    if ((Get-FileHash 'C:/ProgramData/PanGlossPmc/pg.wprp' -Algorithm SHA256).Hash -ne (Get-FileHash "$PSScriptRoot/instruction-compare/pg.wprp" -Algorithm SHA256).Hash) { throw 'installed WPR profile differs; run pmc-setup.ps1 -Install and -Verify' }
    $binaryHash = (Get-FileHash -LiteralPath $Exe -Algorithm SHA256).Hash.ToLowerInvariant()
    $repo = (& git -C $PSScriptRoot rev-parse --show-toplevel).Trim()
    $sourceFiles = [ordered]@{}
    foreach ($path in @(& git -C $repo diff HEAD --name-only -- rust/crates rust/Cargo.toml rust/Cargo.lock)) {
        $sourceFiles[$path] = (Get-FileHash -LiteralPath (Join-Path $repo $path) -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $cpuInfo = Get-CimInstance Win32_Processor | Select-Object -First 1
    $os = Get-CimInstance Win32_OperatingSystem
    $machine = [ordered]@{ name = [Environment]::MachineName; cpu = $cpuInfo.Name.Trim(); processorId = $cpuInfo.ProcessorId; logicalProcessors = [Environment]::ProcessorCount; pinnedCpu = $Cpu; os = $os.Caption; osVersion = $os.Version; architecture = [string][Runtime.InteropServices.RuntimeInformation]::OSArchitecture }
    $provenance = [ordered]@{ sourceCommit = (& git -C $repo rev-parse HEAD).Trim(); sourceFileOverridesSha256 = $sourceFiles; binarySha256 = $binaryHash; features = 'cost-calibration,foma-tools'; profile = 'release'; machine = $machine; wprProfileSha256 = (Get-FileHash "$PSScriptRoot/instruction-compare/pg.wprp" -Algorithm SHA256).Hash.ToLowerInvariant(); readerSha256 = (Get-FileHash "$PSScriptRoot/instruction_compare_etl.py" -Algorithm SHA256).Hash.ToLowerInvariant() }
    $selected = @($doc.sets | Where-Object { -not $Only -or $_.name -in $Only })
    if (-not $selected.Count -or @($Only | Where-Object { $_ -notin @($doc.sets | ForEach-Object name) }).Count) { throw '-Only must name stored calibration sets' }
    $sameProvenance = $doc.ContainsKey('pmc') -and (($doc.pmc.provenance | ConvertTo-Json -Depth 8 -Compress) -eq ($provenance | ConvertTo-Json -Depth 8 -Compress))
    if (-not $sameProvenance) {
        if ($doc.ContainsKey('pmc') -and $selected.Count -ne @($doc.sets).Count) { throw 'stored PMC provenance differs; remeasure all sets together, or use a fresh -Dir with the callgrind files' }
        $doc['pmc'] = [ordered]@{ reportOnly = $true; collect = 'batch --phase-marks --word-phase-marks --threads 1 --no-flush; two marker threads around each parse/prefilter call; full original word lists; fresh hash layouts each round; no word text'; provenance = $provenance; sets = @() }
    }
    $lock = [Threading.Mutex]::new($false, 'Global\PanGlossInstructionCompare')
    $held = $false
    try { $held = $lock.WaitOne(0) } catch [Threading.AbandonedMutexException] { $held = $true }
    if (-not $held) { $lock.Dispose(); throw 'another PMC run holds Global\PanGlossInstructionCompare' }
    $work = Join-Path ([IO.Path]::GetTempPath()) "calibration-pmc-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Force $work | Out-Null
    if ($SaveDir) { New-Item -ItemType Directory -Force $SaveDir | Out-Null }
    $handles = @(); $jobs = @(); $plans = @()
    try {
        foreach ($entry in $selected) {
            $name = $entry.name
            $words = Join-Path $Suite $entry.wordsFile
            if ((Get-FileHash -LiteralPath $words -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.wordsSha256) { throw "$name word-list hash differs" }
            $n = @(Read-WordList $words).Count
            if ($n -ne $entry.words) { throw "$name word-list length differs" }
            $grammar = Join-Path (Split-Path -Parent $words) 'grammar.xml'
            if (-not (Test-Path -LiteralPath $grammar)) {
                if ($name -notmatch '^cal-lexsize-(\d+)k$') { throw "$name grammar is missing" }
                $size = [int]$Matches[1] * 1000
                $grammar = Join-Path $work "$name.xml"
                & $python -c "import sys; sys.path.insert(0, sys.argv[1]); import calib_a_gen as g; open(sys.argv[2], 'w', newline='\n').write(g.fam_lexsize(int(sys.argv[3]))['xml'])" (Join-Path $Suite 'scripts') $grammar $size
                if ($LASTEXITCODE -ne 0) { throw "$name grammar generation failed" }
            }
            if ((Get-FileHash -LiteralPath $grammar -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.grammarSha256) { throw "$name grammar hash differs" }
            $flags = @($entry.flags | Where-Object { $_ -ne '--keep-capped' })
            $rows = @((($calibration.Sets | Where-Object Name -eq $name).Rows))
            $baseline = Join-Path $work 'baseline.tsv'
            $baselineArgs = @('batch', $grammar, $words, $baseline, '--threads', '1', '--no-flush') + $flags
            $baselineProc = Start-Process -FilePath $Exe -ArgumentList (ConvertTo-ArgumentString $baselineArgs) -WindowStyle Hidden -PassThru -Wait -RedirectStandardError (Join-Path $work 'baseline.err') -RedirectStandardOutput (Join-Path $work 'baseline.out')
            try { if ($baselineProc.ExitCode -ne 0) { throw "$name unmarked identity control failed ($($baselineProc.ExitCode))" } } finally { $baselineProc.Dispose() }
            $reference = Read-BatchSignatures $baseline
            if ($reference.Count -ne $n) { throw "$name unmarked identity control is missing rows" }
            $plans += [PSCustomObject]@{ Name = $name; Grammar = $grammar; Words = $words; Flags = $flags; Rows = $rows; N = $n; Reference = $reference }
        }
        $t0 = Get-Date
        $startAttempted = $false
        try {
            $startAttempted = $true
            Invoke-PmcTask 'PanGloss PMC Start' 60
            for ($round = 0; $round -lt $Rounds; $round++) {
                foreach ($plan in $plans) {
                    $name = $plan.Name; $n = $plan.N
                    Write-Host "PMC REPORT-ONLY $name round $($round + 1)/$Rounds ($n word phases)"
                    $out = Join-Path $work 'batch.tsv'
                    $arguments = @('batch', $plan.Grammar, $plan.Words, $out, '--threads', '1', '--phase-marks', '--word-phase-marks', '--no-flush') + $plan.Flags
                    $proc = Start-Process -FilePath $Exe -ArgumentList (ConvertTo-ArgumentString $arguments) -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $work 'batch.err') -RedirectStandardOutput (Join-Path $work 'batch.out')
                    $null = $proc.Handle
                    $handles += $proc
                    $mask = [IntPtr]([long]1 -shl $Cpu)
                    $proc.ProcessorAffinity = $mask
                    if (-not $proc.HasExited -and $proc.ProcessorAffinity -ne $mask) { throw "CPU affinity failed for pid $($proc.Id)" }
                    $proc.WaitForExit()
                    if ($proc.ExitCode -ne 0) { throw "$name batch exited $($proc.ExitCode); see temporary batch.err" }
                    $signatures = Read-BatchSignatures $out
                    if ($signatures.Count -ne $n) { throw "$name batch returned $($signatures.Count) rows, expected $n" }
                    foreach ($r in $plan.Rows) {
                        if (-not $signatures.ContainsKey([int]$r.Idx)) { throw "$name batch is missing index $($r.Idx)" }
                        $status = $signatures[[int]$r.Idx].Split("`t")[0]
                        if ($status -notin 'ok', 'CAP' -or ($status -eq 'CAP' -and $name -ne 'cal-work-cap-cap-words')) { throw "$name incomplete parse at index $($r.Idx): $status" }
                    }
                    if ((Compare-BatchSignatures -Before $plan.Reference -After $signatures).Differ) { throw "$name marked status/signature identity differs from the unmarked control" }
                    $jobs += [PSCustomObject]@{ Pid = $proc.Id; Round = $round; Name = $name; Words = $n }
                }
            }
        } finally {
            foreach ($p in $handles) { if (-not $p.HasExited) { $p.Kill(); $p.WaitForExit() } }
            try { if ($startAttempted) { Invoke-PmcTask 'PanGloss PMC Stop' 900 } }
            finally { foreach ($p in $handles) { $p.Dispose() } }
        }
        $pids = [int[]]@($jobs | ForEach-Object Pid)
        Assert-DistinctPids $pids
        $trace = Get-Item 'C:/ProgramData/PanGlossPmc/current.etl'
        if ($trace.LastWriteTime -lt $t0) { throw 'PMC trace was not refreshed' }
        $etl = Join-Path $work 'trace.etl'
        Copy-Item -LiteralPath $trace.FullName -Destination $etl -Force
        $phasePlan = [ordered]@{}; foreach ($job in $jobs) { $phasePlan["$($job.Pid)"] = $job.Words }
        $planFile = Join-Path $work 'phases.json'
        [IO.File]::WriteAllText($planFile, ($phasePlan | ConvertTo-Json))
        $json = & $python -I "$PSScriptRoot/instruction_compare_etl.py" $etl @pids --word-phases-by-pid $planFile
        if ($LASTEXITCODE -ne 0) { throw "PMC ETL extraction failed ($LASTEXITCODE)" }
        if ($SaveDir) {
            [IO.File]::WriteAllText((Join-Path $SaveDir 'trace.json'), ($json -join "`n"))
            [IO.File]::WriteAllText((Join-Path $SaveDir 'jobs.json'), ($jobs | ConvertTo-Json))
        }
        $report = ($json -join "`n") | ConvertFrom-Json
        Assert-TraceIntact -Report $report -Pids $pids
        foreach ($plan in $plans) {
            $name = $plan.Name; $rows = $plan.Rows; $lines = @()
            foreach ($job in @($jobs | Where-Object Name -eq $name)) {
                $counts = @(Get-PmcPhaseCounts -Report $report -Pids @($job.Pid) -Words $plan.N)
                foreach ($r in $rows) { $lines += "$($r.Idx)`t$($job.Round)`t$($counts[[int]$r.Idx])" }
            }
            $file = Join-Path $Dir "$name.pmc.tsv"
            $pending = Join-Path $work 'pending.tsv'
            Write-CalibrationPmcFile -Path $pending -Lines $lines -Rows $rows -Rounds $Rounds
            Copy-Item -LiteralPath $pending -Destination $file -Force
            $doc.pmc.sets = @($doc.pmc.sets | Where-Object name -ne $name) + @([ordered]@{ name = $name; rounds = $Rounds; measuredOn = (Get-Date).ToUniversalTime().ToString('o'); sha256 = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant(); contextSwitches = $report.switches; eventsLost = $report.events_lost; identityControl = 'every marked row matched the status/signature of an unmarked full-list control in every round' })
            [IO.File]::WriteAllText($manifestPath, ($doc | ConvertTo-Json -Depth 12) + "`n")
            Write-Host "stored $name ($($rows.Count) indexes, $Rounds rounds; report-only)"
        }
    } finally {
        Remove-Item -LiteralPath $work -Recurse -Force
        $lock.ReleaseMutex(); $lock.Dispose()
    }
}
