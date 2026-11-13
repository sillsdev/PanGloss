<#
  .SYNOPSIS
  Before/after retired-instruction comparison of two `pangloss` binaries, per grammar, with a 95% confidence interval and a verdict.

  .DESCRIPTION
  The standard check for an algorithm improvement: wall time and work_steps do not decide it.
  For each grammar/word-list pair, runs `pangloss batch --threads 1` with both binaries, -Repeats rounds each,
  alternating which side goes first. Each round gives one parse count per side, counted directly where the backend can:
    callgrind  collects only inside `Morpher::parse_word_opts` (--toggle-collect), so the grammar load is never counted.
    PMC        when both binaries list `batch --phase-marks`, the run passes it: the process starts and joins an empty thread
               at each edge of the parse loop, and the trace reader totals what the process runs between them.
  Otherwise (PMC, a binary without the flag) each round also runs each side on an empty word list; that run is the
  grammar-load cost and is subtracted from the same round's full run, and a grammar whose load exceeds -MaxLoadRatio times
  its parse is refused: the load's noise then decides the figure (#277, #278).

  Windows: every run happens inside one PMC trace taken through the PanGloss PMC Start/Stop scheduled tasks (no
  elevation); each run's process is pinned to one CPU and its instructions are totalled from the trace, counting only
  the last process to hold its pid. Rows are buffered (batch --no-flush) when both binaries have the flag.
  Linux/WSL: each run is `valgrind --tool=callgrind` with std HashMap keys pinned by an LD_PRELOAD shim.

  Refuses, naming why, when the PMC tasks are absent, the trace lost events, missed a process or a phase mark, a run exited
  nonzero, callgrind collected nothing inside the parse function, the subtracted load dwarfs the parse, or any word's status or signature differs between the binaries (reported as a count; no verdict).
  Word text is never printed or kept: batch output goes to a temp directory that is deleted on exit.

  Verdict, from Welch's t interval (95%, two-sided) on the per-round parse counts of the two sides and a minimum
  effect of MinPercent (default 0.5%), always printed with the interval:
    GO                  the interval lies below zero and the change is at least MinPercent smaller.
    NO-GO (regression)  the interval lies above zero and the change is at least MinPercent larger.
    NO-GO (no change)   the whole interval lies within +/- MinPercent.
    INCONCLUSIVE        anything else: the interval is too wide to claim either; rerun with more -Repeats.
  Instrument check: a binary compared with itself must read neither GO nor NO-GO (regression). Run it once per
  machine session, or when a verdict looks odd; it is not part of every comparison and sets no threshold.

  Exit codes: 0 every grammar GO; 10 at least one NO-GO and no grammar INCONCLUSIVE or refused; 11 at least one
  grammar INCONCLUSIVE and none refused; 3 at least one grammar refused (a differing word, SKIPPED rows, a row-count
  mismatch, a non-positive parse count); 2 the whole run refused (PMC tasks absent, trace lost or failed to process
  events, a run failed, another run holds the machine-wide lock, bad arguments).

  .EXAMPLE
  pwsh -NoProfile -File rust/tools/instruction-compare.ps1 -Before old.exe -After new.exe -Grammars g-hc.xml -Words g.txt
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Before,
    [Parameter(Mandatory)][string]$After,
    [Parameter(Mandatory)][string[]]$Grammars,
    [Parameter(Mandatory)][string[]]$Words,
    # Rounds per side; 0 picks the backend default: 5 on pmc, whose runs vary, 2 on callgrind, whose pinned runs do not.
    [int]$Repeats = 0,
    [int]$Cpu = 4,
    [double]$MinPercent = 0.5,
    [string[]]$BatchArgs = @(),
    [ValidateSet('auto', 'pmc', 'callgrind')][string]$Backend = 'auto',
    [string]$PmcTaskPath = '\PanGloss\',
    # Subtraction only: refuse a grammar whose load is more than this many times its parse.
    [double]$MaxLoadRatio = 4
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\_instruction-compare.ps1"


function Start-BatchRun([string]$Exe, [string]$Grammar, [string]$WordList, [string]$OutTsv, [string]$ErrFile, [string]$OutFile, [string]$Tool, [string[]]$ToolArgs) {
    $batch = @('batch', $Grammar, $WordList, $OutTsv, '--threads', '1') + $BatchArgs
    $file = $Exe
    $argList = $batch
    if ($Tool) { $file = $Tool; $argList = $ToolArgs + @($Exe) + $batch }
    return Start-Process -FilePath $file -ArgumentList (ConvertTo-ArgumentString $argList) -NoNewWindow -PassThru -RedirectStandardError $ErrFile -RedirectStandardOutput $OutFile
}

function Complete-Run($Process, [string]$Describe) {
    $null = $Process.Handle
    $Process.WaitForExit()
    if ($Process.ExitCode -ne 0) { throw "run failed: $Describe exited with code $($Process.ExitCode)" }
}

function Format-Count([double]$Value) { '{0:N0}' -f $Value }

function Get-Describe([string]$Exe) {
    $out = & $Exe --describe 2>$null
    if ($LASTEXITCODE -ne 0) { return $null }
    return ($out -join "`n")
}

$code = 0
$jobs = @()
$work = $null
$lock = $null
try {
    foreach ($exe in $Before, $After) { if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "binary not found: $exe" } }
    if ($Repeats -ne 0 -and $Repeats -lt 2) { throw '-Repeats must be at least 2: the verdict needs a variance from each side' }
    if ($Words.Count -ne 1 -and $Words.Count -ne $Grammars.Count) { throw "-Words has $($Words.Count) path(s) for $($Grammars.Count) grammar(s); give one per grammar, or one for all" }
    foreach ($p in $Grammars + $Words) { if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { throw "input not found: $p" } }
    $Before = (Resolve-Path -LiteralPath $Before).Path; $After = (Resolve-Path -LiteralPath $After).Path
    $Grammars = @($Grammars | ForEach-Object { (Resolve-Path -LiteralPath $_).Path })
    $Words = @($Words | ForEach-Object { (Resolve-Path -LiteralPath $_).Path })

    if ($Backend -eq 'auto') { $Backend = if ($IsWindows) { 'pmc' } else { 'callgrind' } }
    if ($Repeats -eq 0) { $Repeats = if ($Backend -eq 'pmc') { 5 } else { 2 } }
    $python = $null
    $direct = $Backend -eq 'callgrind'
    if ($Backend -eq 'pmc') {
        if (-not $IsWindows) { throw 'the pmc backend needs Windows; use -Backend callgrind here' }
        if ($Cpu -lt 0 -or $Cpu -ge [Environment]::ProcessorCount) { throw "-Cpu $Cpu is not one of the $([Environment]::ProcessorCount) logical processors" }
        $lock = [System.Threading.Mutex]::new($false, 'Global\PanGlossInstructionCompare')
        $held = $false
        try { $held = $lock.WaitOne(0) } catch [System.Threading.AbandonedMutexException] { $held = $true }
        if (-not $held) { $lock = $null; throw 'another instruction-compare run holds the machine-wide lock; two runs would share one trace' }
        $found = @(Get-ScheduledTask -TaskPath $PmcTaskPath -ErrorAction SilentlyContinue | ForEach-Object TaskName)
        Assert-PmcTasksPresent -Found $found -TaskPath $PmcTaskPath
        $python = (Get-Command python -ErrorAction SilentlyContinue).Source
        if (-not $python) { throw 'python is not on PATH; it reads the trace' }
        # Per-word flushes add file-system kernel work to the parsing thread, in bursts of 3M to 21M instructions on some runs (#260); both sides buffer or neither does.
        $noFlush = (Test-DescribesBatchFlag (Get-Describe $Before) '--no-flush') -and (Test-DescribesBatchFlag (Get-Describe $After) '--no-flush')
        if ($noFlush -and $BatchArgs -notcontains '--no-flush') { $BatchArgs = @($BatchArgs) + '--no-flush' }
        $direct = (Test-DescribesBatchFlag (Get-Describe $Before) '--phase-marks') -and (Test-DescribesBatchFlag (Get-Describe $After) '--phase-marks')
        if ($direct -and $BatchArgs -notcontains '--phase-marks') { $BatchArgs = @($BatchArgs) + '--phase-marks' }
    } else {
        if (-not (Get-Command valgrind -ErrorAction SilentlyContinue)) { throw 'valgrind is not on PATH; the callgrind backend needs it' }
        if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) { throw 'rustc is not on PATH; it builds the hash-key pinning shim' }
    }

    $work = Join-Path ([IO.Path]::GetTempPath()) "instruction-compare-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Force $work | Out-Null
    $emptyList = Join-Path $work 'empty.txt'
    [IO.File]::WriteAllText($emptyList, '')
    $plan = New-MeasurementPlan -Repeats $Repeats -NoStartup:$direct
    $exeOf = @{ before = $Before; after = $After }

    for ($g = 0; $g -lt $Grammars.Count; $g++) {
        $list = if ($Words.Count -eq 1) { $Words[0] } else { $Words[$g] }
        foreach ($step in $plan) {
            $tag = Get-RunTag -Grammar $g -Round $step.Round -Side $step.Side -Kind $step.Kind
            $jobs += [PSCustomObject]@{
                Grammar = $g; Round = $step.Round; Side = $step.Side; Kind = $step.Kind; Tag = $tag
                Exe = $exeOf[$step.Side]; Words = $(if ($step.Kind -eq 'empty') { $emptyList } else { $list })
                Tsv = Join-Path $work "$tag.tsv"; Err = Join-Path $work "$tag.err"; Out = Join-Path $work "$tag.out"
                Pid = 0; Proc = $null; Instructions = 0.0; InputWords = $(if ($step.Kind -eq 'empty') { 0 } else { @([IO.File]::ReadLines($list)).Count })
            }
        }
    }

    $shim = $null
    $runJobs = {
        param($Job, [scriptblock]$Launch)
        $describe = "$($Job.Side) $($Job.Kind)-list run of grammar #$($Job.Grammar + 1) (round $($Job.Round))"
        $proc = & $Launch $Job
        $Job.Pid = $proc.Id
        $Job.Proc = $proc
        Complete-Run $proc $describe
    }

    if ($Backend -eq 'pmc') {
        $etl = Join-Path $work 'trace.etl'
        $mask = [IntPtr]([int64]1 -shl $Cpu)
        $launch = {
            param($Job)
            $proc = Start-BatchRun $Job.Exe $Grammars[$Job.Grammar] $Job.Words $Job.Tsv $Job.Err $Job.Out '' @()
            try { $proc.ProcessorAffinity = $mask } catch { if (-not $proc.HasExited) { throw "could not pin pid $($proc.Id) to CPU ${Cpu}: $($_.Exception.Message)" } }
            if (-not $proc.HasExited -and $proc.ProcessorAffinity -ne $mask) { throw "pinning pid $($proc.Id) to CPU $Cpu did not take effect" }
            $proc
        }
        $t0 = Get-Date
        $startAttempted = $false; $runError = $null; $stopError = $null
        try {
            $startAttempted = $true
            Invoke-PmcTask 'PanGloss PMC Start' 60 -TaskPath $PmcTaskPath
            foreach ($job in $jobs) { & $runJobs $job $launch }
        } catch { $runError = $_ }
        if ($startAttempted) {
            try { Invoke-PmcTask 'PanGloss PMC Stop' 900 -TaskPath $PmcTaskPath } catch { $stopError = $_ }
        }
        if ($runError -and $stopError) { throw "$($runError.Exception.Message); additionally the PMC Stop task failed: $($stopError.Exception.Message)" }
        if ($runError) { throw $runError }
        if ($stopError) { throw $stopError }
        Assert-DistinctPids -Pids @($jobs | ForEach-Object Pid)
        $traceFile = Get-Item 'C:\ProgramData\PanGlossPmc\current.etl'
        if ($traceFile.LastWriteTime -lt $t0) { throw "trace not refreshed: $($traceFile.FullName) is older than this run" }
        Copy-Item $traceFile.FullName $etl -Force
        $json = & $python -I "$PSScriptRoot\instruction_compare_etl.py" $etl @($jobs | ForEach-Object Pid)
        if ($LASTEXITCODE -ne 0) { throw "trace extraction failed with exit code $LASTEXITCODE" }
        $report = $json | ConvertFrom-Json
        Assert-TraceIntact -Report $report -Pids @($jobs | ForEach-Object Pid) -RequirePhase:$direct
        foreach ($job in $jobs) { $job.Instructions = [double]$(if ($direct) { $report.parse_instructions."$($job.Pid)" } else { $report.instructions."$($job.Pid)" }) }
        $backendNote = "PMC trace on CPU $Cpu, $($report.switches) context switches, 0 events lost, $($report.earlier_processes) earlier process(es) with a measured pid left out; $(if ($noFlush) { 'rows buffered (--no-flush)' } else { 'rows flushed per word: a binary lacks --no-flush, so file-system work can add outliers' }); $(if ($direct) { 'parse phase counted between the --phase-marks threads' } else { 'a binary lacks --phase-marks, so the parse is the full run minus an empty-list run' }); hash keys are drawn afresh every run, so the spread averages over hash layouts"
    } else {
        $shim = Join-Path $work 'libfixedrandom.so'
        & rustc -O --crate-type cdylib -C panic=abort "$PSScriptRoot/instruction-compare/fixed_random.rs" -o $shim 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'could not build the hash-key pinning shim' }
        $env:LD_PRELOAD = $shim
        try {
            foreach ($job in $jobs) {
                $cg = Join-Path $work "$($job.Tag).cg"
                $launch = { param($Job) Start-BatchRun $Job.Exe $Grammars[$Job.Grammar] $Job.Words $Job.Tsv $Job.Err $Job.Out 'valgrind' (@('--tool=callgrind') + (Get-CallgrindParseArguments) + @("--callgrind-out-file=$cg")) }
                & $runJobs $job $launch
                $job.Instructions = Get-CallgrindTotal -Lines (Get-Content -LiteralPath $cg)
                if ($job.Instructions -le 0) { throw "callgrind collected no instructions inside Morpher::parse_word_opts for $($job.Tag); the binary lacks the symbol (stripped or inlined) or never parsed" }
            }
        } finally { Remove-Item Env:LD_PRELOAD -ErrorAction SilentlyContinue }
        foreach ($g in 0..($Grammars.Count - 1)) {
            foreach ($s in 'before', 'after') {
                $vals = [double[]]@($jobs | Where-Object { $_.Grammar -eq $g -and $_.Side -eq $s -and $_.Kind -eq 'full' } | ForEach-Object Instructions)
                if (-not (Test-RepeatsIdentical $vals -RelativeTolerance 5e-5)) { throw "callgrind repeats of the $s side of grammar #$($g + 1) differ by more than 0.005% although hash keys are pinned; the pinning did not act" }
            }
        }
        $backendNote = 'valgrind callgrind, collecting only inside Morpher::parse_word_opts (callees included), std HashMap keys pinned by shim. run end to end on same-build pairs of real grammars, repeats within 0.005%; no PMC (perf stat) run has cross-checked its change figures on a real change yet'
    }

    Write-Host "instruction-compare: before=$([IO.Path]::GetFileName($Before)) after=$([IO.Path]::GetFileName($After)) repeats=$Repeats backend=$Backend"
    Write-Host "  $backendNote"
    Write-Host "  verdict: Welch's t, 95% interval, minimum effect $MinPercent%; the instrument check (-Before X -After X) is a separate run"
    if ($direct) { Write-Host '  parse phase counted directly; the grammar load is not in the figures' }
    else { Write-Host "  startup subtracted: each round also ran an empty word list per side; it is removed from that round's full run; a load above $MaxLoadRatio times the parse is refused" }
    $verdicts = @()
    for ($g = 0; $g -lt $Grammars.Count; $g++) {
        $mine = @($jobs | Where-Object Grammar -eq $g)
        $side = @{}
        foreach ($s in 'before', 'after') {
            $full = @($mine | Where-Object { $_.Side -eq $s -and $_.Kind -eq 'full' } | Sort-Object Round)
            $empty = @($mine | Where-Object { $_.Side -eq $s -and $_.Kind -eq 'empty' } | Sort-Object Round)
            $side[$s] = if ($direct) {
                [PSCustomObject]@{ Full = $full; Startup = $null; StartupSpread = $null; Net = [double[]]@($full | ForEach-Object Instructions) }
            } else {
                [PSCustomObject]@{
                    Full = $full; Startup = ($empty | ForEach-Object Instructions | Measure-Object -Average).Average
                    StartupSpread = ($empty | ForEach-Object Instructions | Measure-Object -Maximum).Maximum - ($empty | ForEach-Object Instructions | Measure-Object -Minimum).Minimum
                    Net = Get-NetInstructions -Full ([double[]]@($full | ForEach-Object Instructions)) -Empty ([double[]]@($empty | ForEach-Object Instructions))
                }
            }
        }
        $sigBefore = Read-BatchSignatures $side.before.Full[0].Tsv
        $sigAfter = Read-BatchSignatures $side.after.Full[0].Tsv
        $cmp = Compare-BatchSignatures -Before $sigBefore -After $sigAfter
        $repeatDiffs = 0
        foreach ($s in 'before', 'after') {
            $ref = if ($s -eq 'before') { $sigBefore } else { $sigAfter }
            foreach ($r in $side[$s].Full | Select-Object -Skip 1) { $repeatDiffs += (Compare-BatchSignatures -Before $ref -After (Read-BatchSignatures $r.Tsv)).Differ }
        }
        $differing = $cmp.Differ + $repeatDiffs
        $skipped = { param($sig) @($sig.Values | Where-Object { $_ -like "SKIPPED`t*" }).Count }
        $problem = Get-ComparabilityProblem -Words $cmp.Words -SkippedBefore (& $skipped $sigBefore) -SkippedAfter (& $skipped $sigAfter) -RowsBefore $sigBefore.Count -RowsAfter $sigAfter.Count -InputWords $side.before.Full[0].InputWords
        if (-not $problem -and -not $direct) {
            foreach ($s in 'before', 'after') {
                $problem = Get-LoadRatioProblem -Startup $side[$s].Startup -Parse (($side[$s].Net | Measure-Object -Average).Average) -MaxRatio $MaxLoadRatio
                if ($problem) { $problem = "$s side: $problem"; break }
            }
        }
        $v = Get-InstructionVerdict -BeforeNet $side.before.Net -AfterNet $side.after.Net -DifferingWords $differing -MinPercent $MinPercent -Problem $problem
        $verdicts += $v.Verdict
        Write-Host ''
        Write-Host "grammar #$($g + 1) ($([IO.Path]::GetFileName($Grammars[$g]))), $($cmp.Words) words, status/signature differences: $($cmp.Differ) between sides, $repeatDiffs between repeats"
        if (-not $direct) { Write-Host "  startup mean (max-min)      before $(Format-Count $side.before.Startup) ($(Format-Count $side.before.StartupSpread))  after $(Format-Count $side.after.Startup) ($(Format-Count $side.after.StartupSpread))" }
        Write-Host "  instructions before         $(($side.before.Net | ForEach-Object { Format-Count $_ }) -join ' | ')   mean $(Format-Count ($side.before.Net | Measure-Object -Average).Average)"
        Write-Host "  instructions after          $(($side.after.Net | ForEach-Object { Format-Count $_ }) -join ' | ')   mean $(Format-Count ($side.after.Net | Measure-Object -Average).Average)"
        if ($v.Refused) {
            Write-Host "  VERDICT REFUSED: $($v.Reason)"
        } else {
            Write-Host ("  change {0:+0.00;-0.00;0.00}%   95% CI [{1:+0.00;-0.00;0.00}%, {2:+0.00;-0.00;0.00}%]   df {3:0.0}   minimum effect {4:0.00}%" -f $v.DeltaPercent, $v.LowPercent, $v.HighPercent, $v.Df, $MinPercent)
            Write-Host "  VERDICT $($v.Verdict)"
        }
    }
    $code = Get-VerdictExitCode -Verdicts $verdicts
} catch {
    Write-Host "instruction-compare REFUSED: $($_.Exception.Message)" -ForegroundColor Red
    $code = 2
} finally {
    foreach ($job in $jobs) {
        if ($job.Proc) {
            try { if (-not $job.Proc.HasExited) { $job.Proc.Kill($true); $job.Proc.WaitForExit(10000) | Out-Null } } catch { Write-Host "could not stop pid $($job.Pid): $($_.Exception.Message)" -ForegroundColor Red }
            $job.Proc.Dispose()
        }
    }
    if ($work -and (Test-Path -LiteralPath $work)) {
        try { Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction Stop } catch { }
        if (Test-Path -LiteralPath $work) {
            Write-Host "instruction-compare could not delete its temp directory $work; it can hold word text, so delete it by hand" -ForegroundColor Red
            if ($code -eq 0 -or $code -eq 10 -or $code -eq 11) { $code = 2 }
        }
    }
    if ($lock) { $lock.ReleaseMutex(); $lock.Dispose() }
}
exit $code
