param(
    [string]$Repo = 'C:\Users\johnm\Documents\repos\machine',
    [int]$BudgetMinutes = 300,
    [int]$StallSeconds = 150,
    [int]$StepCap = 500000,
    [int]$Threads = 1,
    [string]$WordsPath = '',
    [string]$GrammarPath = '',
    [string]$OutSubdir = 'rust-w10'
)

$ErrorActionPreference = 'Stop'

# Rust equivalent of the C# master-baseline wrapper used to generate
# parity-out/golden/master/sena-full.tsv. Same design: per-word STARTED sentinel + TSV growth
# as the liveness signal, kill+relaunch on stall, --start=N to resume, time-budget deadline.
# pangloss batch's --threads 1 path is required for this to work -- it's the only mode with
# per-word flush/STARTED (see pg-cli/src/main.rs's module doc); --threads>1 buffers the whole
# file until the end and would look permanently stalled.

$Bin      = Join-Path $Repo 'rust\target\release\pangloss.exe'
$Grammar  = if ($GrammarPath -ne '') { $GrammarPath } else { Join-Path $Repo 'samples\data\sena-hc.xml' }
$Words    = if ($WordsPath -ne '') { $WordsPath } else { Join-Path $Repo 'samples\data\sena-words.txt' }
$OutDir   = Join-Path $Repo "rust\parity-out\golden\$OutSubdir"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutTsv      = Join-Path $OutDir 'sena-full.tsv'
$RunLog      = Join-Path $OutDir 'run-log.txt'
$ResumeNotes = Join-Path $OutDir 'RESUME-NOTES.txt'

$TotalWords = (Get-Content -LiteralPath $Words | Where-Object { $_.Trim() -ne '' }).Count

function Log([string]$msg) {
    $ts = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
    "$ts  $msg" | Out-File -FilePath $RunLog -Append -Encoding utf8
}

# Determine the next 0-based word index to resume at. If the TSV's trailing line is a bare
# "idx<TAB>word<TAB>STARTED" sentinel with no matching result row, that word hung/crashed the
# previous run -- synthesize a TIMEOUT row for it (same convention as the C# baseline's log,
# so downstream tooling that already understands that shape doesn't need to change).
function Get-ResumeIndex {
    if (-not (Test-Path $OutTsv)) { return 0 }
    $lines = Get-Content -LiteralPath $OutTsv -Encoding utf8
    if ($lines.Count -eq 0) { return 0 }

    $maxCompleted = -1
    foreach ($line in $lines) {
        $fields = $line -split "`t"
        if ($fields.Count -ge 5) {
            $idx = 0
            if ([int]::TryParse($fields[0], [ref]$idx)) {
                if ($idx -gt $maxCompleted) { $maxCompleted = $idx }
            }
        }
    }

    $lastLine = $lines[$lines.Count - 1]
    $lastFields = $lastLine -split "`t"
    if ($lastFields.Count -eq 3 -and $lastFields[2] -eq 'STARTED') {
        $timeoutIdx = [int]$lastFields[0]
        $timeoutWord = $lastFields[1]
        if ($timeoutIdx -gt $maxCompleted) {
            "$timeoutIdx`t$timeoutWord`t$($StallSeconds*1000)`tTIMEOUT`t-" | Out-File -FilePath $OutTsv -Append -Encoding utf8
            Log "Marked idx=$timeoutIdx word='$timeoutWord' as TIMEOUT (trailing bare STARTED, no result row)."
            return $timeoutIdx + 1
        }
    }
    return $maxCompleted + 1
}

$startTime = Get-Date
$deadline = $startTime.AddMinutes($BudgetMinutes)

Log "=== Wrapper started. Budget=$BudgetMinutes min. StallSeconds=$StallSeconds StepCap=$StepCap Threads=$Threads Deadline=$deadline Total=$TotalWords ==="

$launchCount = 0
$fatal = $false
while ($true) {
    $idx = Get-ResumeIndex
    if ($idx -ge $TotalWords) {
        Log "All $TotalWords words completed."
        "=== DONE: all $TotalWords words completed ===" | Out-File -FilePath $RunLog -Append -Encoding utf8
        if (Test-Path $ResumeNotes) { Remove-Item $ResumeNotes -Force -ErrorAction SilentlyContinue }
        break
    }
    if ((Get-Date) -ge $deadline) {
        Log "Time budget exhausted before idx=$idx. Writing resume notes."
        @"
Next --start index: $idx  (of $TotalWords total)
To resume, either re-run this wrapper (it auto-detects the index from $OutTsv), or manually:
  & "$Bin" batch "$Grammar" "$Words" "$OutTsv" --step-cap $StepCap --memo=on --threads $Threads --start $idx
"@ | Out-File -FilePath $ResumeNotes -Encoding utf8
        break
    }

    $launchCount++
    Log "Launch #${launchCount}: resuming at idx=$idx"

    $stdoutLog = Join-Path $OutDir "stdout-$launchCount.log"
    $stderrLog = Join-Path $OutDir "stderr-$launchCount.log"

    $procArgs = @('batch', $Grammar, $Words, $OutTsv, '--step-cap', $StepCap, '--memo=on', '--threads', $Threads, '--start', $idx)
    $proc = Start-Process -FilePath $Bin -ArgumentList $procArgs `
        -RedirectStandardOutput $stdoutLog -RedirectStandardError $stderrLog `
        -PassThru -WindowStyle Hidden

    Log "Launched PID=$($proc.Id) (idx=$idx)"

    $lastSize = -1
    $lastGrowth = Get-Date
    $killed = $false
    while (-not $proc.HasExited) {
        Start-Sleep -Seconds 15
        try { $proc.Refresh() } catch {}
        if ($proc.HasExited) { break }

        if ((Get-Date) -ge $deadline) {
            Log "Deadline reached mid-run; killing PID=$($proc.Id) tree."
            cmd /c "taskkill /PID $($proc.Id) /T /F" | Out-Null
            Start-Sleep -Seconds 2
            $killed = $true
            break
        }

        $curSize = if (Test-Path $OutTsv) { (Get-Item $OutTsv).Length } else { 0 }
        if ($curSize -gt $lastSize) {
            $lastSize = $curSize
            $lastGrowth = Get-Date
        } else {
            $stalledSec = ((Get-Date) - $lastGrowth).TotalSeconds
            if ($stalledSec -ge $StallSeconds) {
                Log "STALL detected (no TSV growth for $([int]$stalledSec)s). Killing PID=$($proc.Id) tree."
                cmd /c "taskkill /PID $($proc.Id) /T /F" | Out-Null
                Start-Sleep -Seconds 2
                $killed = $true
                break
            }
        }
    }

    if ($proc.HasExited -and -not $killed) {
        Log "Process PID=$($proc.Id) exited on its own with code $($proc.ExitCode)."
    }

    # pg-cli writes "batch complete"/"CAP ..." and panic messages to STDERR (eprintln!), not
    # stdout -- check $stderrLog, not $stdoutLog (a bug caught in the smoke test: the marker
    # check was silently useless against the wrong stream, and worse, so was the panic check).
    $completeFound = $false
    if (Test-Path $stderrLog) {
        $m1 = Select-String -LiteralPath $stderrLog -Pattern 'batch complete' -Encoding Unicode -Quiet -ErrorAction SilentlyContinue
        $m2 = Select-String -LiteralPath $stderrLog -Pattern 'batch complete' -Quiet -ErrorAction SilentlyContinue
        $completeFound = [bool]$m1 -or [bool]$m2
        # 'panicked' catches a Rust panic; 'pangloss batch:' catches run_batch's own Err(e) path
        # (main.rs: `eprintln!("pangloss batch: {e}")` then ExitCode::FAILURE).
        $err1 = Select-String -LiteralPath $stderrLog -Pattern 'panicked|pangloss batch:' -Encoding Unicode -Quiet -ErrorAction SilentlyContinue
        $err2 = Select-String -LiteralPath $stderrLog -Pattern 'panicked|pangloss batch:' -Quiet -ErrorAction SilentlyContinue
        if ($err1 -or $err2) {
            Log "FATAL: panic/error detected in $stderrLog. Stopping wrapper."
            $fatal = $true
        }
    }
    if ($completeFound) {
        Log "Detected 'batch complete' in stderr log ($stderrLog) -- full remaining range finished cleanly."
    } elseif (-not $fatal) {
        Log "No 'batch complete' marker found; will recompute resume index and relaunch."
    }
    if ($fatal) { break }
}

Log "=== Wrapper exiting (launches=$launchCount) ==="
