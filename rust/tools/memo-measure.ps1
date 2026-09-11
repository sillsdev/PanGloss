<#
.SYNOPSIS
  Measures one or more `pangloss batch` memo-mode runs: wall time, peak working set, CAP/TIMEOUT
  counts, and a timing-blind SHA-256 of the output TSV, so two runs can be compared byte-for-byte
  on everything except elapsed time.

.DESCRIPTION
  For each mode in -MemoModes (typically 'on' and 'off'), runs
  `pangloss batch <Grammar> <Words> <out.tsv> --threads N --step-cap <StepCap> --memo <mode>`
  through `pg.ps1 -Mode run -Exe <Exe>` (never `-Bin`/`-Example`, so this never triggers a cargo
  build itself -- build the exe once, ahead of time, with:
    rust\tools\pg.ps1 -Mode build -Bin pangloss -DebugProfile
  which places it at rust\target\<...>\dev\pangloss.exe under the managed target dir pg.ps1 prints).

  Runs in the foreground and samples the spawned pangloss-family process's working set every
  250ms itself, so the caller never needs to poll. Extra environment variables (memo tuning
  knobs, `HC_STEP_STATS=1`, ...) may be passed in -Env and are set only for the duration of each
  run, then restored -- never left leaked into the caller's shell.

  Prints one row per mode: wall seconds, peak working-set MB, CAP count, TIMEOUT count, the
  ms-stripped TSV's SHA-256, and (only when -Env carried `HC_STEP_STATS=1`) the run's total step
  count, summed from the per-word `STEPS` lines `pangloss batch` writes to stderr under that flag.

.PARAMETER Grammar
  Path to the grammar input (.xml/.fwdata/.json -- whatever `pangloss batch` accepts).

.PARAMETER Words
  Path to the newline-delimited word list.

.PARAMETER Exe
  Path to an already-built `pangloss` executable. Build one with:
    rust\tools\pg.ps1 -Mode build -Bin pangloss -DebugProfile

.PARAMETER StepCap
  `--step-cap` value: an integer, or the literal string "unbounded".

.PARAMETER MemoModes
  Which `--memo` values to run, in order; one output row per entry. Default: on, off.

.PARAMETER Env
  Extra environment variables applied for every run in this invocation, e.g. @{ HC_STEP_STATS = '1' }.

.PARAMETER WorkTree
  Repo root containing rust\tools\pg.ps1. Defaults to this script's own repo.

.PARAMETER OutDir
  Directory the per-mode output TSVs are written to. Defaults to the OS temp directory.

.EXAMPLE
  rust\tools\pg.ps1 -Mode build -Bin pangloss -DebugProfile
  rust\tools\memo-measure.ps1 -Grammar samples\data\sena.fwdata -Words samples\data\sena-words.txt `
    -Exe <path from the build above>\pangloss.exe -StepCap 50000000 -MemoModes on,off
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory)] [string]$Grammar,
    [Parameter(Mandatory)] [string]$Words,
    [Parameter(Mandatory)] [string]$Exe,
    [Parameter(Mandatory)] [string]$StepCap,
    [string[]]$MemoModes = @('on', 'off'),
    [hashtable]$Env = @{},
    [string]$WorkTree = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path,
    [string]$OutDir = [System.IO.Path]::GetTempPath(),
    [int]$Threads = 1,
    [int]$RunMemoryGB = 6,
    [string]$Label = ''
)

$ErrorActionPreference = 'Stop'
$pgScript = Join-Path $WorkTree 'rust\tools\pg.ps1'
if (-not (Test-Path $pgScript)) { throw "pg.ps1 not found at $pgScript" }
if (-not (Test-Path $Exe)) { throw "-Exe not found: $Exe" }
$processName = [System.IO.Path]::GetFileNameWithoutExtension($Exe)

# One process-lifetime measurement; called once per -MemoModes entry.
function Measure-OneRun {
    param([string]$Memo, [string]$OutTsv)

    $stderrLog = [System.IO.Path]::GetTempFileName()
    $stdoutLog = [System.IO.Path]::GetTempFileName()
    $pwsh = (Get-Process -Id $PID).Path
    # A `-Command` string, not `-File` + a raw argument array: pwsh's own param binder treats a bare `--` as a positional token to bind, not a passthrough marker.
    $cmd = "& '$pgScript' -Mode run -Exe '$Exe' -RunMemoryGB $RunMemoryGB -- " +
        "batch '$Grammar' '$Words' '$OutTsv' --threads $Threads --step-cap $StepCap --memo $Memo"

    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $proc = Start-Process -FilePath $pwsh -ArgumentList @('-NoProfile', '-NonInteractive', '-Command', $cmd) `
        -WorkingDirectory $WorkTree `
        -RedirectStandardOutput $stdoutLog -RedirectStandardError $stderrLog `
        -PassThru -NoNewWindow

    $peakWsBytes = 0L
    while (-not $proc.HasExited) {
        $kids = Get-Process -Name $processName -ErrorAction SilentlyContinue
        if ($kids) {
            $sample = ($kids | Measure-Object -Property WorkingSet64 -Sum).Sum
            if ($sample -gt $peakWsBytes) { $peakWsBytes = $sample }
        }
        Start-Sleep -Milliseconds 250
    }
    # One last sample: the child can still be alive between the loop's last check and $proc exiting.
    $kids = Get-Process -Name $processName -ErrorAction SilentlyContinue
    if ($kids) {
        $sample = ($kids | Measure-Object -Property WorkingSet64 -Sum).Sum
        if ($sample -gt $peakWsBytes) { $peakWsBytes = $sample }
    }
    $sw.Stop()

    $stdoutText = Get-Content -Raw -Path $stdoutLog -ErrorAction SilentlyContinue
    $stderrText = Get-Content -Raw -Path $stderrLog -ErrorAction SilentlyContinue
    Remove-Item $stdoutLog, $stderrLog -ErrorAction SilentlyContinue

    # "batch complete: N words parsed (S skipped), C hit the step cap, T timed out [memo=M, threads=X]"
    $capped = $null
    $timedOut = $null
    if ($stderrText -match 'batch complete: \d+ words parsed \(\d+ skipped\), (\d+) hit the step cap, (\d+) timed out') {
        $capped = [int]$Matches[1]
        $timedOut = [int]$Matches[2]
    }

    # Per-word "STEPS\t<idx>\t<word>\t<steps>" lines, present only under HC_STEP_STATS=1.
    $totalSteps = $null
    if ($stderrText) {
        $stepLines = [regex]::Matches($stderrText, '(?m)^STEPS\t\d+\t[^\t]+\t(\d+)')
        if ($stepLines.Count -gt 0) {
            $totalSteps = 0L
            foreach ($m in $stepLines) { $totalSteps += [int64]$m.Groups[1].Value }
        }
    }

    # SHA-256 over the TSV with column 3 (0-indexed 2, the elapsed-ms column) replaced by a constant, so timing noise never breaks a byte-for-byte comparison. Row shape: idx<TAB>word<TAB>ms<TAB>status<TAB>signature[<TAB>guessed].
    $tsvHash = $null
    if (Test-Path $OutTsv) {
        $sha = [System.Security.Cryptography.SHA256]::Create()
        $normalized = New-Object System.Text.StringBuilder
        foreach ($line in (Get-Content -Path $OutTsv)) {
            if ($line -eq '') { continue }
            $parts = $line -split "`t"
            if ($parts.Length -ge 3) { $parts[2] = 'MS' }
            [void]$normalized.AppendLine(($parts -join "`t"))
        }
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($normalized.ToString())
        $tsvHash = ([System.BitConverter]::ToString($sha.ComputeHash($bytes)) -replace '-', '').ToLowerInvariant()
    }

    [pscustomobject]@{
        Label       = $Label
        Memo        = $Memo
        StepCap     = $StepCap
        ExitCode    = $proc.ExitCode
        WallSeconds = [math]::Round($sw.Elapsed.TotalSeconds, 3)
        PeakWSMB    = [math]::Round($peakWsBytes / 1MB, 1)
        CapCount    = $capped
        TimeoutCount = $timedOut
        TsvSha256   = $tsvHash
        TotalSteps  = $totalSteps
        Stdout      = $stdoutText
    }
}

$savedEnv = @{}
foreach ($k in $Env.Keys) { $savedEnv[$k] = [System.Environment]::GetEnvironmentVariable($k) }
try {
    foreach ($k in $Env.Keys) { [System.Environment]::SetEnvironmentVariable($k, [string]$Env[$k]) }

    $results = foreach ($mode in $MemoModes) {
        $wordsBase = [System.IO.Path]::GetFileNameWithoutExtension($Words)
        $outTsv = Join-Path $OutDir "memo-measure-$wordsBase-$mode-$StepCap.tsv"
        Measure-OneRun -Memo $mode -OutTsv $outTsv
    }
}
finally {
    foreach ($k in $Env.Keys) { [System.Environment]::SetEnvironmentVariable($k, $savedEnv[$k]) }
}

$results | Select-Object Label, Memo, StepCap, ExitCode, WallSeconds, PeakWSMB, CapCount, TimeoutCount, TsvSha256, TotalSteps |
    Format-Table -AutoSize
$results
