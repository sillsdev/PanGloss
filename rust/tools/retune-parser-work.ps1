<#
  .SYNOPSIS
  After a parser change, tests whether the parser-work weights (pg-cost: work, bytes allocated, FST registers copied) still
  describe the code, and prints ACCEPT, KEEP or REFUSED for a refit. It never edits a weight.

  .DESCRIPTION
  Parser work is report-only, so retuning it cannot change a parse, and `work_steps` (the cap) is never refitted. Run this only
  after a change that reaches the parser got a GO from instruction-compare.ps1; see .claude/skills/retune-parser-work/SKILL.md.

  Inputs are two `pangloss` release binaries built with `--features cost-calibration` (Before and After), and a local
  manifest, never committed, listing grammars and word lists split into a fit set and a held-out set:
    { "version": 1, "grammars": [ { "name": "g1", "grammar": "g1-hc.xml", "words": "g1-words.txt", "set": "fit" }, ... ] }
  Relative paths resolve against the manifest's folder. Without "set", indonesian, aweti, amharic, mbugwe and sena are the fit
  set and swahili/swh and cebuano/ceb are held out. Every list must hold only words that parse with status ok; the tool takes
  -Words of them, evenly spaced, so every measurement uses the same sample.

  Steps, each refusing rather than guessing:
    a. verify  instruction-compare.ps1 on Before and After over the samples, plus a no-change control (Before against Before).
               The control must read NO-GO (within noise) on every grammar; the change must read GO on at least one.
               -PairResult / -ControlResult read the saved stdout of earlier instruction-compare runs instead; they are bound to
               the manifest only by grammar file names and count, so the builds and word samples behind them are trusted.
    b. refit   per-word instructions (Valgrind callgrind, std hash keys pinned) on the After build for the fit set, then
               `pangloss cost-calibrate --targets ... --counts-out` per grammar and `cost-calibrate --fit-counts` over all of them.
               The fit moves each weight at most x0.75-1.5 per run, so it is continued from its own result (the counts do not
               depend on the weights) until it settles; -MaxFitRounds caps that, and a fit that does not settle is KEEP, never ACCEPT.
    c. test    old and new weights scored on the held-out set: direction, Pearson r, median error and worst error, plus a
               scale-free median error (report-only) that shows whether the shape improved or the weights only rescaled.
    d. decide  ACCEPT only if the fit settled, direction is no worse, median error is lower and worst error is no worse on the
               held-out set; otherwise KEEP; REFUSED when a measurement cannot be trusted or a held-out grammar has fewer than 8
               words at 1000 work units. A real pair with no direction rows can ACCEPT on level error, and says "direction
               unmeasured (0 rows)". A weight that moves by more than a quarter prints a warning.
    e. report  peak memory, Valgrind simulated cache misses (--cache-sim=yes, load-independent) and CPU time (best-effort,
               moved by load) per side. They are printed beside the result and are never in the fit or the decision.
  -NoChangeCheck is for proving the tool: Before and After must be the same build, the pair is its own control, no GO is
  required, and the result is capped at KEEP (a proof run, direction unmeasurable); it never exits 0.

  -SaveDir keeps what holds no word text (the instruction-compare output and the per-word counts, keyed by the builds, the
  grammar files and the samples) and reuses it on a rerun, so a refit or a fix after the slow measurement does not repeat it.
  Plan about an hour for the default seven grammars; -SkipSideReports drops the memory and cache part (the last third).

  Word text is never printed or kept: samples, status rows and dumps live in a temp directory deleted on exit (Ctrl-C or a kill
  can skip that and leave /tmp/retune-parser-work-* behind, so delete it by hand), and a failing tool's stderr is never echoed
  (it can carry words). Linux/WSL only; Windows has no per-word instruction counts here.

  Exit codes: 0 ACCEPT; 10 KEEP; 3 REFUSED (a measurement cannot be trusted: failed control, status change, missing grammar or
  list, a build without cost-calibrate --counts-out, too few held-out words overall or in one grammar, a tool failure mid-run);
  2 the run could not start (bad arguments, manifest unreadable, valgrind, rustc or /usr/bin/time absent, not Linux).
  The decision is printed before the side reports, and a side-report failure never changes it or the exit code.

  .EXAMPLE
  pwsh -NoProfile -File rust/tools/retune-parser-work.ps1 -Manifest ~/scratch/retune/manifest.json -Before old/pangloss -After new/pangloss
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Manifest,
    [Parameter(Mandatory)][string]$Before,
    [Parameter(Mandatory)][string]$After,
    [ValidateRange(8, 400)][int]$Words = 40,
    [ValidateRange(2, 10)][int]$Repeats = 2,
    [string]$PairResult,
    [string]$ControlResult,
    [switch]$NoChangeCheck,
    [switch]$SkipSideReports,
    [string]$SaveDir,
    [ValidateRange(2, 50)][int]$MaxFitRounds = 20,
    [string]$Pattern = '*cost_calibrate::parse_one'
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\_instruction-compare.ps1"
. "$PSScriptRoot\_retune-parser-work.ps1"

class Refusal : System.Exception { Refusal([string]$m) : base($m) {} }
function Stop-Refused([string]$Message) { throw [Refusal]::new($Message) }

function Invoke-Tool([string]$File, [string[]]$Arguments, [string]$Err, [string]$Out) {
    $p = Start-Process -FilePath $File -ArgumentList (ConvertTo-ArgumentString $Arguments) -NoNewWindow -PassThru -RedirectStandardError $Err -RedirectStandardOutput $Out
    $null = $p.Handle
    $p.WaitForExit()
    return $p.ExitCode
}

function Invoke-Pinned([scriptblock]$Body) {
    $env:LD_PRELOAD = $script:Shim
    try { & $Body } finally { Remove-Item Env:LD_PRELOAD -ErrorAction SilentlyContinue }
}

function Format-Weights([double[]]$W) { '{0} / {1} / {2}' -f $W[0], $W[1], $W[2] }
function Format-Pct($Value) { if ($null -eq $Value) { 'n/a' } else { '{0:0.00}%' -f $Value } }
function Format-R($Value) { if ($null -eq $Value) { 'n/a' } else { '{0:0.000}' -f $Value } }

# Callgrind collects only inside the per-word parse and dumps at each return, so dump k is word k.
function Measure-PerWord($Exe, $Entry, [string]$Tag) {
    $sample = $script:Sample[$Entry.Name]
    $out = Join-Path $script:Work "$Tag.status.tsv"
    $base = Join-Path $script:Work "$Tag.cg"
    $cgArgs = @('--tool=callgrind', '--collect-atstart=no', "--toggle-collect=$Pattern", "--dump-after=$Pattern", '--dump-instr=no', "--callgrind-out-file=$base.%p.out", $Exe, 'cost-calibrate', $Entry.Grammar, $sample.Path, '--parse-only')
    Invoke-Pinned { $script:ec = Invoke-Tool 'valgrind' $cgArgs (Join-Path $script:Work "$Tag.err") $out }
    if ($script:ec -ne 0) { Stop-Refused "grammar '$($Entry.Name)': valgrind or cost-calibrate exited $($script:ec) for the $Tag run (stderr withheld: it can hold words)" }
    $dumps = Get-DumpPaths -Paths @(Get-ChildItem -Path "$base.*" | ForEach-Object FullName) -Words $sample.Count
    $instr = @($dumps | ForEach-Object { Get-CallgrindTotal -Lines (Get-Content -LiteralPath $_) })
    $status = @{}
    foreach ($line in [IO.File]::ReadAllLines($out)) { $f = $line.Split("`t"); if ($f.Count -eq 2) { $status[[int]$f[0]] = $f[1] } }
    Get-ChildItem -Path "$base.*" | Remove-Item -Force
    return [PSCustomObject]@{ Instructions = $instr; Status = $status }
}

# One build's counts paired with its instructions; the parse-only and counting runs must agree on every status.
function Measure-Grammar($Exe, $Entry, [string]$Tag) {
    $saved = if ($SaveDir) { Join-Path $SaveDir "counts-$((Get-FileHash -LiteralPath $Exe).Hash.Substring(0, 8))-$Tag-$((Get-FileHash -LiteralPath $Entry.Grammar).Hash.Substring(0, 8))-$((Get-FileHash -LiteralPath $script:Sample[$Entry.Name].Path).Hash.Substring(0, 8)).tsv" }
    if ($saved -and (Test-Path -LiteralPath $saved)) {
        Write-Host "  reusing saved counts for $Tag"
        Copy-Item -LiteralPath $saved -Destination (Join-Path $script:Work "$Tag.counts.tsv") -Force
        return @(Read-CountsFile $saved | ForEach-Object { $_ | Add-Member -NotePropertyName Grammar -NotePropertyValue $Entry.Name -PassThru })
    }
    $pw = Measure-PerWord $Exe $Entry $Tag
    $targets = Join-Path $script:Work "$Tag.targets.tsv"
    [IO.File]::WriteAllLines($targets, [string[]]@(0..($pw.Instructions.Count - 1) | ForEach-Object { "$_`t$([long]$pw.Instructions[$_])" }))
    $counts = Join-Path $script:Work "$Tag.counts.tsv"
    $sample = $script:Sample[$Entry.Name]
    # A refused fit is not an error here: the counts file is written before the fit is attempted, and the fit is redone over all grammars.
    $null = Invoke-Tool $Exe @('cost-calibrate', $Entry.Grammar, $sample.Path, '--words', "$($sample.Count)", '--targets', $targets, '--counts-out', $counts) (Join-Path $script:Work "$Tag.cc.err") (Join-Path $script:Work "$Tag.cc.out")
    if (-not (Test-Path -LiteralPath $counts)) { Stop-Refused "grammar '$($Entry.Name)': cost-calibrate wrote no counts for the $Tag run (stderr withheld)" }
    $rows = @(Read-CountsFile $counts | ForEach-Object { $_ | Add-Member -NotePropertyName Grammar -NotePropertyValue $Entry.Name -PassThru })
    $problems = Get-StatusProblems -Grammar $Entry.Name -BatchStatus $pw.Status -Words $sample.Count -CountedIdx @($rows | ForEach-Object Idx)
    if ($problems.Count -gt 0) { Stop-Refused ($problems -join '; ') }
    if ($saved) { Copy-Item -LiteralPath $counts -Destination $saved -Force }
    return $rows
}

function Invoke-InstructionCompare([string]$BeforeExe, [string]$AfterExe, [string]$Label) {
    $ic = Join-Path $PSScriptRoot 'instruction-compare.ps1'
    $q = { param($s) "'" + ($s -replace "'", "''") + "'" }
    $grammars = ($script:Entries | ForEach-Object { & $q $_.Grammar }) -join ','
    $lists = ($script:Entries | ForEach-Object { & $q $script:Sample[$_.Name].Path }) -join ','
    $cmd = "& $(& $q $ic) -Before $(& $q $BeforeExe) -After $(& $q $AfterExe) -Grammars $grammars -Words $lists -Repeats $Repeats; exit `$LASTEXITCODE"
    $lines = @(& pwsh -NoProfile -Command $cmd 2>&1 | ForEach-Object { "$_" })
    $ec = $LASTEXITCODE
    if ($ec -eq 2) { Stop-Refused "instruction-compare refused the $Label run: $(($lines | Where-Object { $_ -like 'instruction-compare REFUSED*' } | Select-Object -First 1))" }
    return $lines
}

$code = 2
$work = $null
$script:Work = $null
$stage = [Diagnostics.Stopwatch]::StartNew()
function Write-Stage([string]$Text) { Write-Host ('[{0,5:0}s] {1}' -f $stage.Elapsed.TotalSeconds, $Text) }
try {
    if (-not $IsLinux) { throw 'retune-parser-work runs on Linux or WSL: per-word instruction counts come from Valgrind, and Windows has no per-word capture here' }
    foreach ($tool in 'valgrind', 'rustc') { if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "$tool is not on PATH" } }
    if (-not $SkipSideReports -and -not (Test-Path -LiteralPath '/usr/bin/time' -PathType Leaf)) { throw '/usr/bin/time is absent (the side reports need it); install it or pass -SkipSideReports' }
    foreach ($exe in $Before, $After) { if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "binary not found: $exe" } }
    $Before = (Resolve-Path -LiteralPath $Before).Path; $After = (Resolve-Path -LiteralPath $After).Path
    $sameBuild = (Get-FileHash -LiteralPath $Before).Hash -eq (Get-FileHash -LiteralPath $After).Hash
    if ($NoChangeCheck -and -not $sameBuild) { throw '-NoChangeCheck needs Before and After to be the same build' }
    if ($sameBuild -and -not $NoChangeCheck) { throw 'Before and After are the same build; pass -NoChangeCheck to run the tool on a no-change pair' }
    $Entries = @(Read-RetuneManifest -Path $Manifest)
    $script:Entries = $Entries
    $work = Join-Path ([IO.Path]::GetTempPath()) "retune-parser-work-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Force $work | Out-Null
    $script:Work = $work
    if ($SaveDir) { New-Item -ItemType Directory -Force $SaveDir | Out-Null; $SaveDir = (Resolve-Path -LiteralPath $SaveDir).Path }
    $code = 3

    $refusals = @(Get-ManifestRefusals $Entries)
    if ($refusals.Count -gt 0) { Stop-Refused ($refusals -join '; ') }
    foreach ($exe in $Before, $After) {
        $err = Join-Path $work 'probe.err'
        $null = Invoke-Tool $exe @('cost-calibrate', '--fit-counts') $err (Join-Path $work 'probe.out')
        if (-not ((Get-Content -LiteralPath $err -Raw) -match '--fit-counts')) { Stop-Refused "$([IO.Path]::GetFileName($exe)) has no cost-calibrate --counts-out/--fit-counts: build it with --features cost-calibration from a tree that has the retune tool" }
    }

    $script:Sample = @{}
    foreach ($e in $Entries) {
        $all = Read-WordList $e.Words
        if ($all.Count -lt 1) { Stop-Refused "grammar '$($e.Name)': the word list is empty" }
        $take = Get-SampleIndices -Length $all.Count -Count $Words
        $path = Join-Path $work "$($e.Name).sample.txt"
        [IO.File]::WriteAllLines($path, [string[]]@($take | ForEach-Object { $all[$_] }))
        $script:Sample[$e.Name] = [PSCustomObject]@{ Path = $path; Count = $take.Count }
    }
    $shim = Join-Path $work 'libfixedrandom.so'
    & rustc -O --crate-type cdylib -C panic=abort "$PSScriptRoot/instruction-compare/fixed_random.rs" -o $shim 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'could not build the hash-key pinning shim' }
    $script:Shim = $shim

    Write-Host "retune-parser-work: before=$([IO.Path]::GetFileName($Before)) after=$([IO.Path]::GetFileName($After)) manifest grammars=$($Entries.Count) (fit $(@($Entries | Where-Object Set -eq 'fit').Count), held out $(@($Entries | Where-Object Set -eq 'heldout').Count)) words per grammar=$Words backend=callgrind"
    if ($NoChangeCheck) { Write-Host '  no-change pair: Before and After are the same build, so the pair is its own control and the decision is capped at KEEP (a proof run)' }

    # a. verify
    Write-Stage 'verify: instruction-compare on the samples'
    $keys = @(@($Entries | ForEach-Object { (Get-FileHash -LiteralPath $_.Grammar).Hash.Substring(0, 8) + (Get-FileHash -LiteralPath $script:Sample[$_.Name].Path).Hash.Substring(0, 8) }) -join '')
    $builds = "$((Get-FileHash -LiteralPath $Before).Hash.Substring(0, 8))-$((Get-FileHash -LiteralPath $After).Hash.Substring(0, 8))-r$Repeats-$((Get-FileHash -InputStream ([IO.MemoryStream]::new([Text.Encoding]::UTF8.GetBytes($keys)))).Hash.Substring(0, 8))"
    $cached = { param($Kind, $Label, $B, $A)
        $f = if ($SaveDir) { Join-Path $SaveDir "compare-$Kind-$builds.txt" }
        if ($f -and (Test-Path -LiteralPath $f)) { Write-Host "  reusing saved instruction-compare output ($Kind)"; return [IO.File]::ReadAllLines($f) }
        $l = Invoke-InstructionCompare $B $A $Label
        if ($f) { [IO.File]::WriteAllLines($f, [string[]]$l) }
        return $l }
    $pairLines = if ($PairResult) { [IO.File]::ReadAllLines($PairResult) } else { & $cached 'pair' 'pair' $Before $After }
    $pair = @(Read-CompareResult $pairLines)
    $control = $null
    if (-not $NoChangeCheck) {
        $ctlLines = if ($ControlResult) { [IO.File]::ReadAllLines($ControlResult) } else { & $cached 'control' 'control' $Before $Before }
        $control = @(Read-CompareResult $ctlLines)
    }
    for ($i = 0; $i -lt $pair.Count -and $i -lt $Entries.Count; $i++) {
        $c = if ($control -and $i -lt $control.Count) { $control[$i].Verdict } elseif ($NoChangeCheck) { 'same run' } else { 'missing' }
        Write-Host ("  {0,-14} pair {1} {2}   control {3}" -f $Entries[$i].Name, $pair[$i].Verdict, $(if ($null -ne $pair[$i].DeltaPercent) { '({0:+0.00;-0.00;0.00}%)' -f $pair[$i].DeltaPercent } else { '' }), $c)
    }
    $problems = @(Get-VerifyProblems -Pair $pair -Control $control -Grammars $Entries.Count -NoChangeCheck:$NoChangeCheck -GrammarFiles @($Entries | ForEach-Object { [IO.Path]::GetFileName($_.Grammar) }))
    if ($problems.Count -gt 0) { Stop-Refused ($problems -join '; ') }

    # b. refit
    $fitFiles = @()
    foreach ($e in @($Entries | Where-Object Set -eq 'fit')) {
        Write-Stage "refit: measuring fit grammar $($e.Name) on the After build"
        $null = Measure-Grammar $After $e "fit-$($e.Name)"
        $fitFiles += (Join-Path $work "fit-$($e.Name).counts.tsv")
    }
    $fitErr = Join-Path $work 'fit.err'
    $fitOnce = { param([double[]]$Start, [string[]]$Files)
        $ec = Invoke-Tool $After (@('cost-calibrate', '--fit-counts', '--weights', ($Start -join ',')) + $Files) $fitErr (Join-Path $work 'fit.out')
        if ($ec -ne 0) { Stop-Refused "the pooled fit refused: $((Get-Content -LiteralPath $fitErr -Raw).Trim())" }
        (Get-Content -LiteralPath (Join-Path $work 'fit.out') -Raw) | ConvertFrom-Json }
    $cur = [double[]]@(676, 2, 268)
    $fit = & $fitOnce $cur $fitFiles
    $cur = [double[]]@($fit.current.work, $fit.current.byte, $fit.current.register)
    $sug = [double[]]@($fit.suggested.work, $fit.suggested.byte, $fit.suggested.register)
    $atBound = @(0..2 | Where-Object { $r = $sug[$_] / $cur[$_]; [Math]::Abs($r - 0.75) -lt 0.001 -or [Math]::Abs($r - 1.5) -lt 0.001 } | ForEach-Object { 'work', 'byte', 'register' | Select-Object -Index $_ })
    Write-Host "refit over $($fit.words) fit words:"
    Write-Host ("  first fit, bounded to x0.75-1.5 per weight: work {0:0.000}  byte {1:0.000}  register {2:0.000}   speed {3:0.000} instructions per cost unit" -f ($sug[0] / $cur[0]), ($sug[1] / $cur[1]), ($sug[2] / $cur[2]), $fit.speed)
    if ($atBound.Count -gt 0) { Write-Host "  at the bound: $($atBound -join ', ')" }
    # The counts do not depend on the weights, so a fit that hit its bound is continued from its own result until it settles.
    $w = [double[]]@(($sug[0] * $fit.speed), ($sug[1] * $fit.speed), ($sug[2] * $fit.speed))
    $rounds = 1; $settled = $false
    while ($rounds -lt $MaxFitRounds) {
        $next = & $fitOnce $w $fitFiles
        $rounds++
        $factors = [double[]]@(($next.suggested.work / $next.current.work), ($next.suggested.byte / $next.current.byte), ($next.suggested.register / $next.current.register))
        $w = [double[]]@(($next.suggested.work * $next.speed), ($next.suggested.byte * $next.speed), ($next.suggested.register * $next.speed))
        if (Test-FitSettled -Factors $factors -Speed $next.speed) { $settled = $true; break }
    }
    $cand = ConvertTo-CandidateWeights -Suggested $w -Speed 1.0
    Write-Host ("  continued from its own result: {0} after {1} fit(s); unrounded {2:0.##} / {3:0.##} / {4:0.##}" -f $(if ($settled) { 'settled' } else { 'did NOT settle' }), $rounds, $w[0], $w[1], $w[2])
    Write-Host "  candidate weights (rounded, at least 1): $(Format-Weights $cand)   current: $(Format-Weights $cur)"
    if (-not $settled) { Write-Host "  the fit did not settle in $MaxFitRounds rounds; the candidate is not a fixed point, so the decision is KEEP" }
    $spread = @()
    foreach ($e in @($Entries | Where-Object Set -eq 'fit')) {
        $f = Join-Path $work "fit-$($e.Name).counts.tsv"
        $one = & $fitOnce $w @($f)
        $spread += , ([double[]]@(($one.suggested.work * $one.speed), ($one.suggested.byte * $one.speed), ($one.suggested.register * $one.speed)))
    }
    if ($spread.Count -gt 1) {
        $rng = { param($i) $v = @($spread | ForEach-Object { $_[$i] }); '{0:0.#}-{1:0.#}' -f ($v | Measure-Object -Minimum).Minimum, ($v | Measure-Object -Maximum).Maximum }
        Write-Host "  each fit grammar alone, one bounded step from the candidate: work $(& $rng 0)  byte $(& $rng 1)  register $(& $rng 2)"
    }

    # c. test
    $heldAfter = @(); $heldBefore = @()
    foreach ($e in @($Entries | Where-Object Set -eq 'heldout')) {
        Write-Stage "test: measuring held-out grammar $($e.Name)"
        $a = Measure-Grammar $After $e "held-after-$($e.Name)"
        $b = if ($sameBuild) { $a } else { Measure-Grammar $Before $e "held-before-$($e.Name)" }
        $ia = @($a | ForEach-Object Idx); $ib = @($b | ForEach-Object Idx)
        if (Compare-Object $ia $ib) { Stop-Refused "grammar '$($e.Name)': the builds counted different words (a status change between Before and After)" }
        $heldAfter += $a; $heldBefore += $b
    }
    $old = Get-WeightScore -After $heldAfter -Before $heldBefore -Weights $cur
    $new = Get-WeightScore -After $heldAfter -Before $heldBefore -Weights $cand
    Write-Host "held-out scores over $($old.Words) words of $(@($Entries | Where-Object Set -eq 'heldout').Count) grammar(s) (level error is predicted against measured instructions per word on the After build):"
    foreach ($row in @(@('old', $cur, $old), @('new', $cand, $new))) {
        Write-Host ("  {0}  {1,-14} direction {2} of {3}   r level {4}  r change {5}   median error {6}   worst error {7}   scale-free median {8} worst {9} (report-only)" -f $row[0], (Format-Weights $row[1]), $row[2].DirectionMatches, $row[2].DirectionRows, (Format-R $row[2].PearsonLevel), (Format-R $row[2].PearsonChange), (Format-Pct $row[2].MedianError), (Format-Pct $row[2].WorstError), (Format-Pct $row[2].ScaleFreeMedianError), (Format-Pct $row[2].ScaleFreeWorstError))
    }

    # d. decide
    $heldNames = @($Entries | Where-Object Set -eq 'heldout' | ForEach-Object Name)
    $decision = Get-RetuneDecision -Old $old -New $new -OldWeights $cur -NewWeights $cand -Settled $settled -ProofRun ([bool]$NoChangeCheck) -HeldOutNames $heldNames
    if ($decision.Verdict -eq 'REFUSED') { Stop-Refused $decision.Reason }
    Write-Host ''
    Write-Host "DECISION $($decision.Verdict): $($decision.Reason)"
    if ($decision.Shape) { Write-Host "  $($decision.Shape)" }
    Write-Host "  candidate weights (work / byte / register), not applied: $(Format-Weights $cand); the owner decides, then edits pg-cost"
    $moves = @(Get-WeightMoves -Current $cur -Candidate $cand)
    foreach ($move in $moves) { Write-Host "  WARNING: large move, $move" -ForegroundColor Yellow }
    if ($moves.Count -gt 0) { Write-Host "  WARNING: a move of more than a quarter needs the owner's review; a run on -Words $Words samples is not adoptable evidence" -ForegroundColor Yellow }
    $code = Get-RetuneExitCode $decision.Verdict

    # e. report: never in the fit or the decision
    if (-not $SkipSideReports) { try {
        Write-Stage 'report: memory, simulated cache misses and CPU time (beside the result, not in it)'
        $sides = [Collections.Generic.List[object]]::new()
        if ($sameBuild) { $sides.Add(@('same build', $After)) } else { $sides.Add(@('before', $Before)); $sides.Add(@('after', $After)) }
        foreach ($e in $Entries) {
            foreach ($s in $sides) {
                $exe = $s[1]
                $one = { param($list, $tag)
                    $timeFile = Join-Path $work "$tag.time"
                    $null = Invoke-Tool '/usr/bin/time' @('-f', '%M %U %S', '-o', $timeFile, $exe, 'cost-calibrate', $e.Grammar, $list, '--parse-only') (Join-Path $work "$tag.err") (Join-Path $work "$tag.out")
                    $t = (Get-Content -LiteralPath $timeFile -Tail 1) -split '\s+'
                    $cg = Join-Path $work "$tag.cs.out"
                    Invoke-Pinned { $script:csec = Invoke-Tool 'valgrind' @('--tool=callgrind', '--cache-sim=yes', "--callgrind-out-file=$cg", $exe, 'cost-calibrate', $e.Grammar, $list, '--parse-only') (Join-Path $work "$tag.cs.err") (Join-Path $work "$tag.cs.out2") }
                    if ($script:csec -ne 0) { Stop-Refused "grammar '$($e.Name)': the cache-simulation run exited $($script:csec) (stderr withheld)" }
                    [PSCustomObject]@{ RssKb = [double]$t[0]; Cpu = [double]$t[1] + [double]$t[2]; Misses = Get-CacheMissTotals (Get-CallgrindEvents (Get-Content -LiteralPath $cg)) } }
                $emptyList = Join-Path $work 'empty.txt'; [IO.File]::WriteAllText($emptyList, '')
                $tag = "side-$($e.Name)-$($s[0] -replace ' ', '-')"
                $full = & $one $script:Sample[$e.Name].Path "$tag-full"
                $empty = & $one $emptyList "$tag-empty"
                Write-Host ("  {0,-12} {1,-10} peak RSS {2:0} MB (start-up alone {3:0})   simulated misses beyond start-up: L1 {4:N0}  last level {5:N0}   CPU {6:0.0}s best-effort" -f $e.Name, $s[0], ($full.RssKb / 1024), ($empty.RssKb / 1024), ($full.Misses.L1 - $empty.Misses.L1), ($full.Misses.LastLevel - $empty.Misses.LastLevel), ($full.Cpu - $empty.Cpu))
            }
        }
    } catch { Write-Host "side reports stopped: $($_.Exception.Message); the decision above stands" -ForegroundColor Yellow } }
} catch [Refusal] {
    Write-Host ''
    Write-Host "DECISION REFUSED: $($_.Exception.Message)" -ForegroundColor Red
    $code = 3
} catch {
    Write-Host "retune-parser-work could not run: $($_.Exception.Message)" -ForegroundColor Red
    if ($code -ne 3) { $code = 2 }
} finally {
    if ($work -and (Test-Path -LiteralPath $work)) {
        try { Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction Stop } catch { }
        if (Test-Path -LiteralPath $work) {
            Write-Host "retune-parser-work could not delete its temp directory $work; it holds word samples, so delete it by hand" -ForegroundColor Red
            if ($code -eq 0 -or $code -eq 10) { $code = 3 }
        }
    }
}
exit $code
