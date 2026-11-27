. "$PSScriptRoot/_test-harness.ps1"

$script:PmcPython = if (Get-Command python -ErrorAction SilentlyContinue) { 'python' } else { 'python3' }
$script:PmcTallyBody = @'
import json, sys
sys.path.insert(0, sys.argv[1])
from instruction_compare_etl import Tally
t = Tally([7], word_phases=2)
t.thread_start(7, 70)
t.thread_start(7, 71)
t.thread_start(7, 72)
t.cswitch(0, 70, 0, [0])
t.cswitch(0, 71, 70, [10000])
t.cswitch(0, 70, 71, [60010000])
t.cswitch(0, 72, 70, [60011000])
t.cswitch(0, 70, 72, [120011000])
# Windows may reuse a marker's tid in the same process; each lifetime is a distinct marker.
t.thread_start(7, 71)
t.cswitch(0, 71, 70, [120021000])
t.cswitch(0, 70, 71, [180021000])
t.thread_start(7, 72)
t.cswitch(0, 72, 70, [180023000])
t.cswitch(0, 70, 72, [240023000])
print(json.dumps(t.report()))
'@

Test-Case 'per-word ETL phases exclude load, framing and marker spins even when marker tids are reused' {
    $output = & $script:PmcPython -I -c $script:PmcTallyBody (Join-Path $PSScriptRoot '..')
    Assert-Equal 0 $LASTEXITCODE
    $r = ($output -join "`n") | ConvertFrom-Json
    Assert-Equal '1000 2000' ($r.word_instructions.'7' -join ' ')
    Assert-Equal 3000 $r.parse_instructions.'7'
    Assert-Equal 2 $r.phase.'7'
    Assert-Equal 5 $r.threads.'7'
}

Test-Case 'missing or ambiguous per-word markers refuse a partial measurement' {
    foreach ($bad in @($script:PmcTallyBody.Replace('[240023000]', '[200023000]'), $script:PmcTallyBody.Replace('word_phases=2', 'word_phases=3'))) {
        $output = & $script:PmcPython -I -c $bad (Join-Path $PSScriptRoot '..')
        Assert-Equal 0 $LASTEXITCODE
        $r = ($output -join "`n") | ConvertFrom-Json
        Assert-Equal 0 $r.phase.'7'
        Assert-Equal 0 @($r.word_instructions.'7').Count
    }
}

Test-Case 'one trace can carry a different word-phase count for each calibration process' {
    $snippet = @'
import json, sys
sys.path.insert(0, sys.argv[1])
from instruction_compare_etl import Tally
t = Tally([7, 8], word_phases={7: 2, 8: 1})
counter = 0
for pid, words in [(7, 2), (8, 1)]:
    main = pid * 100
    t.thread_start(pid, main)
    t.cswitch(0, main, 0, [counter])
    for idx in range(words):
        a, b = main + idx * 2 + 1, main + idx * 2 + 2
        t.thread_start(pid, a)
        counter += 10000; t.cswitch(0, a, main, [counter])
        counter += 60000000; t.cswitch(0, main, a, [counter])
        t.thread_start(pid, b)
        counter += (idx + 1) * 1000; t.cswitch(0, b, main, [counter])
        counter += 60000000; t.cswitch(0, main, b, [counter])
    counter += 10000; t.cswitch(0, 0, main, [counter])
print(json.dumps(t.report()))
'@
    $output = & $script:PmcPython -I -c $snippet (Join-Path $PSScriptRoot '..')
    Assert-Equal 0 $LASTEXITCODE
    $r = ($output -join "`n") | ConvertFrom-Json
    Assert-Equal '1000 2000' ($r.word_instructions.'7' -join ' ')
    Assert-Equal '1000' ($r.word_instructions.'8' -join ' ')
    Assert-Equal 2 $r.phase.'8'
}

Test-Case 'word phases exclude an overlapping earlier incarnation of another measured PID' {
    $snippet = @'
import json, sys
sys.path.insert(0, sys.argv[1])
from instruction_compare_etl import Tally

t = Tally([7, 8], word_phases=1)
t.process_start(8)
t.thread_start(8, 80)
t.process_start(7)
for tid in (70, 71, 72):
    t.thread_start(7, tid)
t.cswitch(0, 71, 0, [0])
t.cswitch(0, 70, 71, [60000000])
t.cswitch(0, 80, 70, [60001000])
t.cswitch(0, 70, 80, [60010000])
t.cswitch(0, 72, 70, [60011000])
t.cswitch(0, 0, 72, [120011000])
# The final measured PID 8 starts after PID 7, with reused thread IDs.
t.process_start(8)
for tid in (80, 81, 82):
    t.thread_start(8, tid)
t.cswitch(0, 81, 0, [120021000])
t.cswitch(0, 80, 81, [180021000])
t.cswitch(0, 82, 80, [180023000])
t.cswitch(0, 0, 82, [240023000])
print(json.dumps(t.report()))
'@
    $output = & $script:PmcPython -I -c $snippet (Join-Path $PSScriptRoot '..')
    Assert-Equal 0 $LASTEXITCODE
    $r = ($output -join "`n") | ConvertFrom-Json
    Assert-Equal '2000' ($r.word_instructions.'7' -join ' ')
    Assert-Equal '2000' ($r.word_instructions.'8' -join ' ')
    Assert-Equal 2 $r.phase.'7'
    Assert-Equal 2 $r.phase.'8'
    Assert-Equal 1 $r.earlier_processes
    Assert-Equal 0 $r.gaps_on_target
}

Write-TestSummary
