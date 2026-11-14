<#
  .SYNOPSIS
  The standing calibration set for the parser-work weights (#240): measure it once into word-free counts files, score any weights on it from those files.

  .DESCRIPTION
  -Measure runs every `calibration/cal-*` family of fieldworks-parser-conformance (a checkout at the commit rust/tools/calibration/conformance.commit
  pins; the run refuses any other) through `pangloss cost-calibrate --counts-out` and per-word Valgrind callgrind instructions
  (collected inside the per-word parse, the same bracket retune-parser-work.ps1 uses), and writes into rust/tools/calibration/:
    <set>.counts.tsv   idx, work, bytes, registers, allocations, instructions: no word text
    manifest.json      the conformance and PanGloss commits, the binary hash, each set's flags and a rung for every word index
  `cal-work-cap` runs at --work-cap 200000 (its cap words are kept, they stop at a fixed step count); `cal-prefilter-scale` runs with --prefilter on.
  The binary needs `--features cost-calibration,foma-tools`. The calibration words are slow on purpose and are never part of a normal parse or test run.

  -Score scores a weight set on the stored counts (arithmetic only, no measurement): per-family mean error and a per-rung linearity report that flags a
  family whose cost-to-instruction ratio drifts more than -DriftPercent along its ladder, then the real-grammar guard from real-grammar-totals.json.

  -RealTotals writes real-grammar-totals.json from local word-free counts files that carry the allocations column. Real words are never stored.

  .EXAMPLE
  pwsh -NoProfile -File rust/tools/calibration-set.ps1 -Score
  pwsh -NoProfile -File rust/tools/calibration-set.ps1 -Score -Weights 150,0.9375,0,112.5
  pwsh -NoProfile -File rust/tools/calibration-set.ps1 -Measure -Suite ~/work/fpc.worktrees/calib-batch-b-structure -Exe ~/work/calib-240/bin/pangloss -SaveDir ~/work/calib-240/save
#>
[CmdletBinding(DefaultParameterSetName = 'Score')]
param(
    [Parameter(ParameterSetName = 'Measure', Mandatory)][switch]$Measure,
    [Parameter(ParameterSetName = 'Score')][switch]$Score,
    [Parameter(ParameterSetName = 'Real', Mandatory)][switch]$RealTotals,
    [Parameter(ParameterSetName = 'Measure', Mandatory)][string]$Suite,
    [Parameter(ParameterSetName = 'Measure', Mandatory)][string]$Exe,
    [Parameter(ParameterSetName = 'Measure')][string]$SaveDir,
    [Parameter(ParameterSetName = 'Measure')][string[]]$Only,
    [Parameter(ParameterSetName = 'Real', Mandatory)][string[]]$RealCounts,
    [string]$Dir = (Join-Path $PSScriptRoot 'calibration'),
    [Parameter(ParameterSetName = 'Score')][double[]]$Weights,
    [Parameter(ParameterSetName = 'Score')][double]$DriftPercent = -1,
    [Parameter(ParameterSetName = 'Score')][switch]$Json
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\_instruction-compare.ps1"
. "$PSScriptRoot\_calibration-set.ps1"

function Get-CurrentWeights {
    # The constants of pg-cost, in instructions: the file is the one place they are written.
    $src = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../crates/pg-cost/src/lib.rs') -Raw
    $get = { param($n) if ($src -match "pub const $n`: u64 = (\d+);") { [double]$Matches[1] } else { throw "pg-cost lib.rs has no $n" } }
    $scale = & $get 'WEIGHT_SCALE'
    return [double[]]@(((& $get 'WORK_WEIGHT') / $scale), ((& $get 'BYTE_WEIGHT') / $scale), ((& $get 'REGISTER_WEIGHT') / $scale), ((& $get 'ALLOCATION_WEIGHT') / $scale))
}

function Invoke-Tool([string]$File, [string[]]$Arguments, [string]$Err, [string]$Out) {
    $p = Start-Process -FilePath $File -ArgumentList (ConvertTo-ArgumentString $Arguments) -NoNewWindow -PassThru -RedirectStandardError $Err -RedirectStandardOutput $Out
    $null = $p.Handle
    $p.WaitForExit()
    return $p.ExitCode
}

if ($RealTotals) {
    $files = [ordered]@{}
    foreach ($f in $RealCounts) {
        if ((Split-Path -Leaf $f) -notmatch '-(?:fit|held-after|held-before|after|before)-([A-Za-z0-9_.]+?)-[0-9A-F]{8}-[0-9A-F]{8}\.tsv$') { throw "cannot read a grammar name from $f (expected counts-<build>-<kind>-<name>-<hash>-<hash>.tsv)" }
        $files[$Matches[1]] = $f
    }
    $doc = [ordered]@{ version = 1; note = 'Per-grammar totals over rows with work >= 1000: predicted cost is linear in them. No words, no per-word rows. Source: counts files with the allocations column (callgrind inside the per-word parse).'; grammars = @(New-RealTotals -Files $files) }
    $out = Join-Path $Dir 'real-grammar-totals.json'
    New-Item -ItemType Directory -Force -Path $Dir | Out-Null
    [IO.File]::WriteAllText($out, ($doc | ConvertTo-Json -Depth 5) + "`n")
    Write-Host "wrote $out ($($files.Count) grammars)"
    exit 0
}

if (-not $Measure) {
    $cur = Get-CurrentWeights
    $w = if ($Weights) { if ($Weights.Count -ne 4) { throw '-Weights takes work, byte, register, allocation' }; [double[]]$Weights } else { $cur }
    $drift = if ($DriftPercent -ge 0) { $DriftPercent } else { $script:DefaultDriftPercent }
    $set = Read-CalibrationManifest -Dir $Dir
    $setScore = Get-SetScore -Sets $set.Sets -Weights $w -DriftPercent $drift
    $realPath = Join-Path $Dir 'real-grammar-totals.json'
    $real = if (Test-Path -LiteralPath $realPath) { Get-RealGuard -Totals (Read-RealTotals $realPath) -Weights $w } else { $null }
    if ($Json) { [PSCustomObject]@{ weights = $w; driftPercent = $drift; synthetic = $setScore; real = $real } | ConvertTo-Json -Depth 8; exit 0 }
    Write-Host ("calibration set: conformance {0}, PanGloss {1}, {2} sets, {3} rows" -f $set.Manifest.conformance.commit.Substring(0, 7), $set.Manifest.pangloss.commit.Substring(0, 7), $set.Sets.Count, (($set.Sets | ForEach-Object { $_.Rows.Count } | Measure-Object -Sum).Sum))
    Write-Host ("weights {0} / {1} / {2} / {3}   drift limit {4}%" -f $w[0], $w[1], $w[2], $w[3], $drift)
    Format-FamilyTable $setScore | ForEach-Object { Write-Host $_ }
    Write-Host 'rung drift:'
    Format-RungTable $setScore | ForEach-Object { Write-Host "  $_" }
    if ($setScore.Flagged.Count -gt 0) { Write-Host "NON-LINEAR families: $($setScore.Flagged -join ', ')" } else { Write-Host 'every family is linear within the drift limit' }
    if ($real) {
        $curReal = Get-RealGuard -Totals (Read-RealTotals $realPath) -Weights $cur
        $verdict = Get-RealGuardVerdict -Current $curReal -Candidate $real
        Write-Host ('real grammars (stored totals): mean error {0:0.0}%  shape {1:0.0}%  spread max/min {2:0.00}  ({3} {4:+0.0;-0.0;0.0} points against current weights)' -f $real.MeanError, $real.MeanScaleFreeError, $real.Spread, $verdict.Verdict, $verdict.Points)
        foreach ($g in $real.Grammars) { Write-Host ('  {0,-12} words {1,3}  cost/Ir {2:0.000}  error {3:0.0}%' -f $g.Name, $g.Scored, $g.Ratio, $g.ErrorPoints) }
    }
    exit 0
}

# ---- Measure ----
if (-not $IsLinux) { throw 'the callgrind measurement runs in WSL or Linux' }
$pin = (Get-Content -LiteralPath (Join-Path $Dir 'conformance.commit') -Raw).Trim()
$head = (& git -C $Suite rev-parse HEAD).Trim()
if ($head -ne $pin) { throw "the suite at $Suite is at $head, the calibration set pins $pin; check that commit out" }
if ((& git -C $Suite status --porcelain).Count -gt 0) { throw "the suite at $Suite has local changes" }
$Exe = (Resolve-Path -LiteralPath $Exe).Path
$repo = (& git -C $PSScriptRoot rev-parse --show-toplevel).Trim()
$pgHead = (& git -C $repo rev-parse HEAD).Trim()
if ((& git -C $repo status --porcelain -- rust).Count -gt 0) { throw 'rust/ has uncommitted changes: the manifest pins a PanGloss commit, so build and measure from a commit' }
$work = Join-Path ([IO.Path]::GetTempPath()) "calibration-set-$PID"
New-Item -ItemType Directory -Force -Path $work, $Dir | Out-Null
if ($SaveDir) { New-Item -ItemType Directory -Force -Path $SaveDir | Out-Null }
$shim = Join-Path $work 'libfixedrandom.so'
& rustc -O --crate-type cdylib -C panic=abort "$PSScriptRoot/instruction-compare/fixed_random.rs" -o $shim 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'could not build the hash-key pinning shim' }

# The sets: one per family, plus the cap words of cal-work-cap as their own list.
$sets = @()
foreach ($d in (Get-ChildItem -LiteralPath (Join-Path $Suite 'calibration') -Directory -Filter 'cal-*' | Sort-Object Name)) {
    if ($Only -and $d.Name -notin $Only) { continue }
    $flags = @()
    if ($d.Name -eq 'cal-prefilter-scale') { $flags = @('--prefilter', 'on') }
    $lists = @([PSCustomObject]@{ Name = $d.Name; Words = Join-Path $d.FullName 'words.txt'; Flags = $flags })
    if ($d.Name -eq 'cal-work-cap') {
        $lists[0].Flags = @('--work-cap', '200000')
        foreach ($cw in (Get-ChildItem -LiteralPath $d.FullName -Filter 'cap-words.work-cap-*.txt')) {
            if ($cw.Name -notmatch '^cap-words\.work-cap-(\d+)\.txt$') { continue }
            $lists += [PSCustomObject]@{ Name = 'cal-work-cap-cap-words'; Words = $cw.FullName; Flags = @('--work-cap', $Matches[1], '--keep-capped') }
        }
    }
    foreach ($l in $lists) { $sets += [PSCustomObject]@{ Name = $l.Name; Family = $d.Name; Dir = $d.FullName; Words = $l.Words; Flags = $l.Flags } }
}

$manifestSets = @()
foreach ($s in $sets) {
    Write-Host "measuring $($s.Name)"
    $grammar = Join-Path $s.Dir 'grammar.xml'
    if (-not (Test-Path -LiteralPath $grammar)) {
        # Too large to commit: regenerate it and check the recorded hash (cal-lexsize-50k).
        $shaFile = Join-Path $s.Dir 'grammar.sha256'
        if (-not (Test-Path -LiteralPath $shaFile)) { throw "$($s.Name): no grammar.xml and no grammar.sha256" }
        $n = [int]([regex]::Match($s.Name, '(\d+)k$').Groups[1].Value) * 1000
        $grammar = Join-Path $work "$($s.Name).grammar.xml"
        $py = "import sys; sys.path.insert(0, sys.argv[1]); import calib_a_gen as g; open(sys.argv[2], 'w', newline='\n').write(g.fam_lexsize($n)['xml'])"
        & python3 -c $py (Join-Path $Suite 'scripts') $grammar
        if ($LASTEXITCODE -ne 0) { throw "$($s.Name): the generator failed" }
    }
    $gsha = (Get-FileHash -LiteralPath $grammar -Algorithm SHA256).Hash.ToLowerInvariant()
    $shaFile = Join-Path $s.Dir 'grammar.sha256'
    if ((Test-Path -LiteralPath $shaFile) -and ((Get-Content -LiteralPath $shaFile -Raw).Trim() -ne $gsha)) { throw "$($s.Name): the grammar hash differs from grammar.sha256" }
    $words = Read-WordList $s.Words
    $n = $words.Count
    $ph = Join-Path $work "$($s.Name).placeholder.tsv"
    [IO.File]::WriteAllLines($ph, [string[]]@(0..($n - 1) | ForEach-Object { "$_`t1" }))
    $cnt = Join-Path $work "$($s.Name).counts.tsv"
    $null = Invoke-Tool $Exe (@('cost-calibrate', $grammar, $s.Words, '--words', "$n", '--targets', $ph, '--counts-out', $cnt) + $s.Flags) (Join-Path $work "$($s.Name).cc.err") (Join-Path $work "$($s.Name).cc.out")
    if (-not (Test-Path -LiteralPath $cnt)) { throw "$($s.Name): cost-calibrate wrote no counts" }
    $rows = @(Read-CountsFile $cnt)
    # Callgrind collects inside the per-word parse and dumps at each return, so dump k is word k (a prefilter-rejected word still enters it).
    $base = Join-Path $work "$($s.Name).cg"
    $cgArgs = @('--tool=callgrind', '--collect-atstart=no', '--toggle-collect=*cost_calibrate::parse_one', '--dump-after=*cost_calibrate::parse_one', '--dump-instr=no', "--callgrind-out-file=$base.%p.out", $Exe, 'cost-calibrate', $grammar, $s.Words, '--parse-only') + $s.Flags
    $env:LD_PRELOAD = $shim
    try { $ec = Invoke-Tool 'valgrind' $cgArgs (Join-Path $work "$($s.Name).cg.err") (Join-Path $work "$($s.Name).status.tsv") } finally { Remove-Item Env:LD_PRELOAD -ErrorAction SilentlyContinue }
    if ($ec -ne 0) { throw "$($s.Name): valgrind exited $ec" }
    $dumps = Get-DumpPaths -Paths @(Get-ChildItem -Path "$base.*" | ForEach-Object FullName) -Words $n
    $instr = @($dumps | ForEach-Object { Get-CallgrindTotal -Lines (Get-Content -LiteralPath $_) })
    Get-ChildItem -Path "$base.*" | Remove-Item -Force
    $targets = @(0..($n - 1) | ForEach-Object { "$_`t$([long]$instr[$_])" })
    $final = @($rows | ForEach-Object { "{0}`t{1}`t{2}`t{3}`t{4}`t{5}" -f $_.Idx, [long]$_.Work, [long]$_.Bytes, [long]$_.Registers, [long]$_.Allocations, [long]$instr[$_.Idx] })
    [IO.File]::WriteAllLines((Join-Path $Dir "$($s.Name).counts.tsv"), [string[]]$final)
    if ($SaveDir) {
        [IO.File]::WriteAllLines((Join-Path $SaveDir "$($s.Name).targets.tsv"), [string[]]$targets)
        Copy-Item -LiteralPath $cnt -Destination (Join-Path $SaveDir "$($s.Name).counts-placeholder.tsv") -Force
        Copy-Item -LiteralPath (Join-Path $work "$($s.Name).status.tsv") -Destination (Join-Path $SaveDir "$($s.Name).status.tsv") -Force
    }
    # A rung for every word index, from ladder.tsv (word text stays out of the manifest); lexsize sets are rungs of one ladder by size.
    $rungOf = @{}
    $ladder = Join-Path $s.Dir 'ladder.tsv'
    $header = (Get-Content -LiteralPath $ladder -TotalCount 1).Split("`t")
    if ($header -contains 'rung') { foreach ($line in (Get-Content -LiteralPath $ladder | Select-Object -Skip 1)) { $f = $line.Split("`t"); $rungOf[$f[0]] = $f[1] } }
    $constant = if ($s.Name -match '^cal-lexsize-(\d+k)$') { $Matches[1] } else { $null }
    $rungs = @($words | ForEach-Object { if ($constant) { $constant } elseif ($rungOf.ContainsKey($_)) { $rungOf[$_] } else { '' } })
    $manifestSets += [ordered]@{
        name = $s.Name; family = $(if ($constant) { 'cal-lexsize' } else { $s.Family }); wordsFile = ($s.Words.Substring($Suite.TrimEnd('/').Length + 1)); words = $n; rows = $rows.Count
        wordsSha256 = (Get-FileHash -LiteralPath $s.Words -Algorithm SHA256).Hash.ToLowerInvariant(); grammarSha256 = $gsha; flags = @($s.Flags); rungs = $rungs
    }
}
$valgrind = (& valgrind --version).Trim()
$doc = [ordered]@{
    version = 1
    measuredOn = (Get-Date -Format 'yyyy-MM-dd')
    conformance = [ordered]@{ repo = 'sillsdev/fieldworks-parser-conformance'; commit = $pin; folder = 'calibration/' }
    pangloss = [ordered]@{ commit = $pgHead; binarySha256 = (Get-FileHash -LiteralPath $Exe -Algorithm SHA256).Hash.ToLowerInvariant(); features = 'cost-calibration,foma-tools' }
    backend = [ordered]@{ tool = $valgrind; collect = 'inside cost_calibrate::parse_one (the per-word parse, callees included), std hash keys pinned, one thread' }
    weightsAtMeasure = (Get-CurrentWeights)
    note = 'Instructions of a prefilter-scale word include the admission and tiled-filter walks, which cost does not count; rejected words enter the bracket with the filter walk only and fall under the 1000-work floor.'
    sets = $manifestSets
}
[IO.File]::WriteAllText((Join-Path $Dir 'manifest.json'), ($doc | ConvertTo-Json -Depth 6) + "`n")
Remove-Item -Recurse -Force $work
Write-Host "wrote $($manifestSets.Count) sets to $Dir"
