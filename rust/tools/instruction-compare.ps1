<#
  .SYNOPSIS
  Before/after retired-instruction comparison of two `pangloss` binaries, per grammar, with measured noise and a go/no-go line.

  .DESCRIPTION
  The standard check for an algorithm improvement: wall time and work_steps do not decide it.
  For each grammar/word-list pair, runs `pangloss batch --threads 1` with both binaries, at least twice each,
  alternating which side goes first, plus the same run on an empty word list per side. The empty-list run is
  the grammar-load cost and is subtracted, so the reported figures are parse instructions only.

  Windows: every run happens inside one PMC trace taken through the PanGloss PMC Start/Stop scheduled tasks (no
  elevation); each run's process is pinned to one CPU and its instructions are totalled from the trace.
  Linux/WSL: each run is `valgrind --tool=callgrind` with std HashMap keys pinned by an LD_PRELOAD shim.

  Refuses, naming why, when the PMC tasks are absent, the trace lost events or missed a process, a run exited
  nonzero, or any word's status or signature differs between the binaries (reported as a count; no verdict).
  Word text is never printed or kept: batch output goes to a temp directory that is deleted on exit.

  Verdict: GO when the change is an improvement larger than max(NoiseMultiple x noise, MinPercent), where noise is
  the larger repeat spread of the net counts plus the spread of the empty-list runs. A binary compared with itself
  is the no-change control and must read NO-GO (within noise); run it, since the noise floor is not otherwise proven.

  Exit codes: 0 every grammar GO; 10 at least one NO-GO and none refused; 3 at least one grammar refused (a differing
  word, SKIPPED rows, a row-count mismatch, a non-positive net); 2 the whole run refused (PMC tasks absent, trace lost
  or failed to process events, a run failed, another run holds the machine-wide lock, bad arguments).

  .EXAMPLE
  pwsh -NoProfile -File rust/tools/instruction-compare.ps1 -Before old.exe -After new.exe -Grammars g-hc.xml -Words g.txt
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Before,
    [Parameter(Mandatory)][string]$After,
    [Parameter(Mandatory)][string[]]$Grammars,
    [Parameter(Mandatory)][string[]]$Words,
    [int]$Repeats = 2,
    [int]$Cpu = 4,
    [double]$NoiseMultiple = 2.0,
    [double]$MinPercent = 0.5,
    [string[]]$BatchArgs = @(),
    [ValidateSet('auto', 'pmc', 'callgrind')][string]$Backend = 'auto',
    [string]$PmcTaskPath = '\PanGloss\'
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\_instruction-compare.ps1"

function Invoke-PmcTask([string]$Name, [int]$TimeoutSec) {
    $before = (Get-ScheduledTaskInfo -TaskPath $PmcTaskPath -TaskName $Name).LastRunTime
    Start-ScheduledTask -TaskPath $PmcTaskPath -TaskName $Name
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 500
        $state = (Get-ScheduledTask -TaskPath $PmcTaskPath -TaskName $Name).State
        $info = Get-ScheduledTaskInfo -TaskPath $PmcTaskPath -TaskName $Name
        if ($state -ne 'Running' -and $info.LastRunTime -gt $before) {
            if ($info.LastTaskResult -ne 0) { throw "$Name failed: result 0x$('{0:X}' -f $info.LastTaskResult)" }
            return
        }
    }
    throw "$Name did not finish within $TimeoutSec s"
}

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

$code = 0
$jobs = @()
$work = $null
$lock = $null
try {
    foreach ($exe in $Before, $After) { if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "binary not found: $exe" } }
    if ($Repeats -lt 2) { throw '-Repeats must be at least 2: noise is measured from the spread of repeats' }
    if ($Words.Count -ne 1 -and $Words.Count -ne $Grammars.Count) { throw "-Words has $($Words.Count) path(s) for $($Grammars.Count) grammar(s); give one per grammar, or one for all" }
    foreach ($p in $Grammars + $Words) { if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { throw "input not found: $p" } }
    $Before = (Resolve-Path -LiteralPath $Before).Path; $After = (Resolve-Path -LiteralPath $After).Path
    $Grammars = @($Grammars | ForEach-Object { (Resolve-Path -LiteralPath $_).Path })
    $Words = @($Words | ForEach-Object { (Resolve-Path -LiteralPath $_).Path })

    if ($Backend -eq 'auto') { $Backend = if ($IsWindows) { 'pmc' } else { 'callgrind' } }
    $python = $null
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
    } else {
        if (-not (Get-Command valgrind -ErrorAction SilentlyContinue)) { throw 'valgrind is not on PATH; the callgrind backend needs it' }
        if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) { throw 'rustc is not on PATH; it builds the hash-key pinning shim' }
    }

    $work = Join-Path ([IO.Path]::GetTempPath()) "instruction-compare-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Force $work | Out-Null
    $emptyList = Join-Path $work 'empty.txt'
    [IO.File]::WriteAllText($emptyList, '')
    $plan = New-MeasurementPlan -Repeats $Repeats
    $exeOf = @{ before = $Before; after = $After }

    for ($g = 0; $g -lt $Grammars.Count; $g++) {
        $list = if ($Words.Count -eq 1) { $Words[0] } else { $Words[$g] }
        foreach ($step in $plan) {
            $tag = "g$g-r$($step.Round)-$($step.Side)-$($step.Kind)"
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
            Invoke-PmcTask 'PanGloss PMC Start' 60
            foreach ($job in $jobs) { & $runJobs $job $launch }
        } catch { $runError = $_ }
        if ($startAttempted) {
            try { Invoke-PmcTask 'PanGloss PMC Stop' 900 } catch { $stopError = $_ }
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
        Assert-TraceIntact -Report $report -Pids @($jobs | ForEach-Object Pid)
        foreach ($job in $jobs) { $job.Instructions = [double]$report.instructions."$($job.Pid)" }
        $backendNote = "PMC trace on CPU $Cpu, $($report.switches) context switches, 0 events lost; hash keys are not pinned on Windows, so repeat spread includes hash-order noise"
    } else {
        $shim = Join-Path $work 'libfixedrandom.so'
        & rustc -O --crate-type cdylib -C panic=abort "$PSScriptRoot/instruction-compare/fixed_random.rs" -o $shim 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'could not build the hash-key pinning shim' }
        $env:LD_PRELOAD = $shim
        try {
            foreach ($job in $jobs) {
                $cg = Join-Path $work "$($job.Tag).cg"
                $launch = { param($Job) Start-BatchRun $Job.Exe $Grammars[$Job.Grammar] $Job.Words $Job.Tsv $Job.Err $Job.Out 'valgrind' @('--tool=callgrind', "--callgrind-out-file=$cg") }
                & $runJobs $job $launch
                $job.Instructions = Get-CallgrindTotal -Lines (Get-Content -LiteralPath $cg)
            }
        } finally { Remove-Item Env:LD_PRELOAD -ErrorAction SilentlyContinue }
        foreach ($g in 0..($Grammars.Count - 1)) {
            foreach ($s in 'before', 'after') {
                $vals = [double[]]@($jobs | Where-Object { $_.Grammar -eq $g -and $_.Side -eq $s -and $_.Kind -eq 'full' } | ForEach-Object Instructions)
                if (-not (Test-RepeatsIdentical $vals)) { throw "callgrind repeats of the $s side of grammar #$($g + 1) differ although hash keys are pinned; the pinning did not act" }
            }
        }
        $backendNote = 'valgrind callgrind totals, std HashMap keys pinned by shim. UNVERIFIED: this backend has not been run end to end; treat its figures as unconfirmed until a PMC run agrees'
    }

    Write-Host "instruction-compare: before=$([IO.Path]::GetFileName($Before)) after=$([IO.Path]::GetFileName($After)) repeats=$Repeats backend=$Backend"
    Write-Host "  $backendNote"
    Write-Host '  no-change control: not run by this invocation; run -Before X -After X on the same inputs to measure the noise floor'
    Write-Host '  startup subtracted: each side also ran an empty word list; its mean is removed from every run, so figures below are parse-only'
    $verdicts = @()
    for ($g = 0; $g -lt $Grammars.Count; $g++) {
        $mine = @($jobs | Where-Object Grammar -eq $g)
        $side = @{}
        foreach ($s in 'before', 'after') {
            $full = @($mine | Where-Object { $_.Side -eq $s -and $_.Kind -eq 'full' } | Sort-Object Round)
            $empty = @($mine | Where-Object { $_.Side -eq $s -and $_.Kind -eq 'empty' } | Sort-Object Round)
            $side[$s] = [PSCustomObject]@{
                Full = $full; Startup = ($empty | ForEach-Object Instructions | Measure-Object -Average).Average
                Net = Get-NetInstructions -Full ([double[]]@($full | ForEach-Object Instructions)) -Empty ([double[]]@($empty | ForEach-Object Instructions))
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
        $emptySpread = 0.0
        foreach ($s in 'before', 'after') {
            $ev = [double[]]@($mine | Where-Object { $_.Side -eq $s -and $_.Kind -eq 'empty' } | ForEach-Object Instructions)
            $emptySpread = [Math]::Max($emptySpread, ($ev | Measure-Object -Maximum).Maximum - ($ev | Measure-Object -Minimum).Minimum)
        }
        $v = Get-InstructionVerdict -BeforeNet $side.before.Net -AfterNet $side.after.Net -DifferingWords $differing -NoiseMultiple $NoiseMultiple -MinPercent $MinPercent -EmptySpreadInstructions $emptySpread -Problem $problem
        $verdicts += $v.Verdict
        Write-Host ''
        Write-Host "grammar #$($g + 1) ($([IO.Path]::GetFileName($Grammars[$g]))), $($cmp.Words) words, status/signature differences: $($cmp.Differ) between sides, $repeatDiffs between repeats"
        Write-Host "  startup (mean, subtracted)  before $(Format-Count $side.before.Startup)  after $(Format-Count $side.after.Startup)"
        Write-Host "  instructions before         $(($side.before.Net | ForEach-Object { Format-Count $_ }) -join ' | ')   mean $(Format-Count ($side.before.Net | Measure-Object -Average).Average)"
        Write-Host "  instructions after          $(($side.after.Net | ForEach-Object { Format-Count $_ }) -join ' | ')   mean $(Format-Count ($side.after.Net | Measure-Object -Average).Average)"
        if ($v.Refused) {
            Write-Host "  VERDICT REFUSED: $($v.Reason)"
        } else {
            Write-Host ("  change {0:+0.00;-0.00;0.00}%   noise from repeats and empty runs {1:0.00}%   threshold {2:0.00}%" -f $v.DeltaPercent, $v.NoisePercent, $v.ThresholdPercent)
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
            if ($code -eq 0 -or $code -eq 10) { $code = 2 }
        }
    }
    if ($lock) { $lock.ReleaseMutex(); $lock.Dispose() }
}
exit $code
