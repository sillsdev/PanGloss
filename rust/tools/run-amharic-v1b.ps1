param(
    [string]$Repo = 'C:\Users\johnm\Documents\repos\machine\.worktrees\v1b-amharic-finish',
    [int]$BudgetMinutes = 45,
    [int]$StallSeconds = 420,
    [int]$WordTimeoutMs = 300000,
    [int]$Threads = 1,
    [int]$InitialStart = 181
)

$ErrorActionPreference = 'Stop'

# V1b adaptation of tools/run-sena-rust.ps1's external watchdog pattern (STARTED sentinel + TSV
# growth as the liveness signal, kill+relaunch on stall, --start=N to resume). Differences from
# the Sena original: (1) output lives under parity-out/work (scratch), not golden; (2) resume
# index starts at $InitialStart (181) rather than 0, since idx 0-180 was already measured by V1;
# (3) --word-timeout-ms replaces --step-cap as the internal bound (O1b just fixed its cadence --
# this external watchdog is belt-and-suspenders verification that the fix holds under real load).
#
# IMPORTANT correctness note (found and fixed during this task's own dry run): resuming at
# "maxCompletedIdx + 1" is ALWAYS correct and requires no special-casing of a trailing bare
# STARTED line -- `--start N` re-attempts word N from scratch regardless of why the previous
# attempt didn't finish (this chunk's own deadline, a genuine stall, or a crash). Do NOT
# synthesize a fake TIMEOUT row merely because the wrapper's own per-call deadline (needed to
# stay under the tool harness's ~10-minute single-call cap) cut a still-legitimately-running
# word off mid-flight -- that would silently discard a real measurement. A synthetic TIMEOUT row
# is only ever appended when a genuine STALL is caught red-handed inside the monitoring loop
# (no TSV growth for $StallSeconds while the process is still alive) -- that is the one signal
# worth flagging as a possible gap in O1b's fix, and it captures the actual idx/word directly
# from the process's own STARTED line rather than re-deriving it later.

$Bin      = Join-Path $Repo 'rust\target\release\pangloss.exe'
$Grammar  = Join-Path $Repo 'samples\data\amharic-hc.xml'
$Words    = Join-Path $Repo 'samples\data\amharic-words.txt'
$OutDir   = Join-Path $Repo 'rust\parity-out\work'
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutTsv      = Join-Path $OutDir 'v1b-amharic-post-w8.tsv'
$RunLog      = Join-Path $OutDir 'v1b-run-log.txt'
$ResumeNotes = Join-Path $OutDir 'v1b-RESUME-NOTES.txt'

$TotalWords = (Get-Content -LiteralPath $Words | Where-Object { $_.Trim() -ne '' }).Count

function Log([string]$msg) {
    $ts = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
    "$ts  $msg" | Out-File -FilePath $RunLog -Append -Encoding utf8
    Write-Host "$ts  $msg"
}

# Pure query: highest idx with a completed (>=5 field) result row, or $InitialStart-1 if none.
# Never mutates the file.
function Get-MaxCompletedIdx {
    if (-not (Test-Path $OutTsv)) { return $InitialStart - 1 }
    $lines = Get-Content -LiteralPath $OutTsv -Encoding utf8
    $maxCompleted = $InitialStart - 1
    foreach ($line in $lines) {
        $fields = $line -split "`t"
        if ($fields.Count -ge 5) {
            $idx = 0
            if ([int]::TryParse($fields[0], [ref]$idx)) {
                if ($idx -gt $maxCompleted) { $maxCompleted = $idx }
            }
        }
    }
    return $maxCompleted
}

# Read the idx/word off the trailing bare STARTED line, if the file ends with one (used only to
# label a confirmed-stall synthetic row; NOT used to decide the resume index).
function Get-TrailingStarted {
    if (-not (Test-Path $OutTsv)) { return $null }
    $lines = Get-Content -LiteralPath $OutTsv -Encoding utf8
    if ($lines.Count -eq 0) { return $null }
    $lastLine = $lines[$lines.Count - 1]
    $lastFields = $lastLine -split "`t"
    if ($lastFields.Count -eq 3 -and $lastFields[2] -eq 'STARTED') {
        return [PSCustomObject]@{ Idx = [int]$lastFields[0]; Word = $lastFields[1] }
    }
    return $null
}

$startTime = Get-Date
$deadline = $startTime.AddMinutes($BudgetMinutes)

Log "=== V1b wrapper started. Budget=$BudgetMinutes min. StallSeconds=$StallSeconds WordTimeoutMs=$WordTimeoutMs Threads=$Threads InitialStart=$InitialStart Deadline=$deadline Total=$TotalWords ==="

$launchCount = 0
$fatal = $false
while ($true) {
    $idx = (Get-MaxCompletedIdx) + 1
    if ($idx -ge $TotalWords) {
        Log "All $TotalWords words completed (from idx $InitialStart)."
        "=== DONE: all words through $TotalWords completed ===" | Out-File -FilePath $RunLog -Append -Encoding utf8
        if (Test-Path $ResumeNotes) { Remove-Item $ResumeNotes -Force -ErrorAction SilentlyContinue }
        break
    }
    if ((Get-Date) -ge $deadline) {
        Log "Time budget exhausted before idx=$idx. Writing resume notes."
        @"
Next --start index: $idx  (of $TotalWords total)
To resume: & "$Bin" batch "$Grammar" "$Words" "$OutTsv" --word-timeout-ms $WordTimeoutMs --threads $Threads --start $idx
"@ | Out-File -FilePath $ResumeNotes -Encoding utf8
        break
    }

    $launchCount++
    Log "Launch #${launchCount}: resuming at idx=$idx"

    $stdoutLog = Join-Path $OutDir "v1b-stdout-$launchCount.log"
    $stderrLog = Join-Path $OutDir "v1b-stderr-$launchCount.log"

    $procArgs = @('batch', $Grammar, $Words, $OutTsv, '--word-timeout-ms', $WordTimeoutMs, '--threads', $Threads, '--start', $idx)
    $proc = Start-Process -FilePath $Bin -ArgumentList $procArgs `
        -RedirectStandardOutput $stdoutLog -RedirectStandardError $stderrLog `
        -PassThru -WindowStyle Hidden

    Log "Launched PID=$($proc.Id) (idx=$idx)"

    $lastSize = -1
    $lastGrowth = Get-Date
    $killed = $false
    $stallKilled = $false
    while (-not $proc.HasExited) {
        Start-Sleep -Seconds 15
        try { $proc.Refresh() } catch {}
        if ($proc.HasExited) { break }

        if ((Get-Date) -ge $deadline) {
            Log "Deadline reached mid-run; killing PID=$($proc.Id) tree. (Chunk boundary only -- NOT a stall; the in-flight word, if any, will be re-attempted from scratch next launch, no synthetic row written.)"
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
                Log "STALL detected (no TSV growth for $([int]$stalledSec)s, exceeds StallSeconds=$StallSeconds -- word-timeout-ms=$WordTimeoutMs should have fired well before this). Killing PID=$($proc.Id) tree."
                cmd /c "taskkill /PID $($proc.Id) /T /F" | Out-Null
                Start-Sleep -Seconds 2
                $killed = $true
                $stallKilled = $true
                break
            }
        }
    }

    if ($proc.HasExited -and -not $killed) {
        Log "Process PID=$($proc.Id) exited on its own with code $($proc.ExitCode)."
    }

    if ($stallKilled) {
        $trailing = Get-TrailingStarted
        if ($null -ne $trailing) {
            "$($trailing.Idx)`t$($trailing.Word)`t$($StallSeconds*1000)`tTIMEOUT`t-" | Out-File -FilePath $OutTsv -Append -Encoding utf8
            Log "Marked idx=$($trailing.Idx) word='$($trailing.Word)' as TIMEOUT (confirmed STALL -- process alive with zero TSV growth past word-timeout-ms+margin). POSSIBLE O1b GAP -- flag in report."
        } else {
            Log "STALL killed the process but trailing line was not a bare STARTED (unexpected) -- not synthesizing a row; will recompute resume index."
        }
    }

    $completeFound = $false
    if (Test-Path $stderrLog) {
        $m1 = Select-String -LiteralPath $stderrLog -Pattern 'batch complete' -Encoding Unicode -Quiet -ErrorAction SilentlyContinue
        $m2 = Select-String -LiteralPath $stderrLog -Pattern 'batch complete' -Quiet -ErrorAction SilentlyContinue
        $completeFound = [bool]$m1 -or [bool]$m2
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

Log "=== V1b wrapper exiting (launches=$launchCount) ==="
