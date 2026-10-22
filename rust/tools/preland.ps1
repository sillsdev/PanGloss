<#
  .DESCRIPTION
  The pre-land check: the CI failures a local `-Mode check` cannot see, in a few minutes on a warm
  build. Run through `pg.ps1 -Mode preland`; required before landing on main.

  Checks, each named in the summary with its duration, or SKIPPED with the reason:
    clippy        `pg.ps1 -Mode check` (rustfmt, comment hygiene, CI's clippy). Always.
    tool-tests    every rust/tools/tests/*.tests.ps1, in parallel, as CI's Windows job runs them.
                  Always; overlaps the cargo checks below.
    conformance   the C# Machine driver over machine/conformance with a freshly built `pangloss` as
                  its adapter, filtered by known-conformance-divergences.txt exactly as
                  run-conformance.sh does in Conformance CI. `pangloss` is built with the
                  pg-test-opt profile check and test share. Path-gated.
    scoreboard    pg-foma-backend's backend_scoreboard_gate. Path-gated on its crate closure
                  (read from `cargo metadata`), conformance-staging/, and the `machine` pin.

  Gating diffs the working tree (plus untracked files) against merge-base(HEAD, -Base). If the base
  cannot be resolved every check runs and the summary says why. -All runs every check regardless.

  Usage:
    rust\tools\pg.ps1 -Mode preland
    rust\tools\pg.ps1 -Mode preland -Base origin/main -All

  Exit codes: 0 every check that ran passed; $script:ExitCodePrelandFailed (42) otherwise,
  including a check that could not run (missing driver, unmatched pin, unresolvable binary).
#>
[CmdletBinding()]
param(
    [string]$Base = 'origin/main',
    [switch]$All,
    [ValidateRange(1, 32)][int]$ToolTestThrottle = 8
)

. "$PSScriptRoot\_common.ps1"

function Test-PrelandPathTrigger {
    <#
      .DESCRIPTION
      Pure: returns the first changed path matching a trigger root, or $null. A root ending in `/`
      matches a whole directory; any other root matches that exact path (e.g. the `machine` gitlink).
    #>
    param([string[]]$ChangedPaths, [string[]]$Roots)
    foreach ($path in $ChangedPaths) {
        foreach ($root in $Roots) {
            if ($root.EndsWith('/')) {
                if ($path.StartsWith($root, [StringComparison]::Ordinal)) { return $path }
            } elseif ($path -eq $root) {
                return $path
            }
        }
    }
    return $null
}

function Get-PrelandGateDecision {
    <#
      .DESCRIPTION
      Pure: decides whether one path-gated check runs. Returns Run and Reason; the reason names the
      triggering path, the forcing condition, or why it skipped.
    #>
    param([string[]]$ChangedPaths, [string[]]$Roots, [string]$ForceReason = '', [string]$RootsLabel = '')
    if ($ForceReason) { return [PSCustomObject]@{ Run = $true; Reason = $ForceReason } }
    $hit = Test-PrelandPathTrigger -ChangedPaths $ChangedPaths -Roots $Roots
    if ($hit) { return [PSCustomObject]@{ Run = $true; Reason = "changed: $hit" } }
    $watched = if ($RootsLabel) { $RootsLabel } else { $Roots -join ', ' }
    return [PSCustomObject]@{ Run = $false; Reason = "no changed path under: $watched" }
}

function Get-WorkspaceCrateClosure {
    <#
      .DESCRIPTION
      Repo-relative directories (`rust/crates/x/`) of $Package and every path dependency it can
      observe: its own normal, dev and build dependencies, then normal and build dependencies
      transitively. Throws when `cargo metadata` cannot answer.
    #>
    param([Parameter(Mandatory)][string]$RustRoot, [Parameter(Mandatory)][string]$RepoRoot, [Parameter(Mandatory)][string]$Package)
    $json = & cargo metadata --no-deps --offline --format-version 1 --manifest-path (Join-Path $RustRoot 'Cargo.toml') 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $json) { throw "cargo metadata failed (exit $LASTEXITCODE)" }
    $meta = ($json -join "`n") | ConvertFrom-Json
    $byName = @{}
    foreach ($p in $meta.packages) { $byName[$p.name] = $p }
    if (-not $byName.ContainsKey($Package)) { throw "package $Package is not a workspace member" }
    $repoFull = [System.IO.Path]::GetFullPath($RepoRoot).TrimEnd('\', '/')
    $toRelative = {
        param([string]$dir)
        $full = [System.IO.Path]::GetFullPath($dir).TrimEnd('\', '/')
        if (-not $full.StartsWith($repoFull, [StringComparison]::OrdinalIgnoreCase)) { return $null }
        return ($full.Substring($repoFull.Length).TrimStart('\', '/') -replace '\\', '/') + '/'
    }
    $dirs = [System.Collections.Generic.HashSet[string]]::new()
    $seen = [System.Collections.Generic.HashSet[string]]::new()
    $queue = [System.Collections.Generic.Queue[object]]::new()
    $queue.Enqueue([PSCustomObject]@{ Name = $Package; Root = $true })
    while ($queue.Count -gt 0) {
        $item = $queue.Dequeue()
        if (-not $seen.Add($item.Name)) { continue }
        $pkg = $byName[$item.Name]
        if (-not $pkg) { continue }
        $rel = & $toRelative (Split-Path -Parent $pkg.manifest_path)
        if ($rel) { $null = $dirs.Add($rel) }
        foreach ($dep in $pkg.dependencies) {
            if (-not $dep.path) { continue }
            if ($dep.kind -eq 'dev' -and -not $item.Root) { continue }
            $depRel = & $toRelative $dep.path
            if ($depRel) { $null = $dirs.Add($depRel) }
            $queue.Enqueue([PSCustomObject]@{ Name = $dep.name; Root = $false })
        }
    }
    return @($dirs | Sort-Object)
}

function Get-PrelandChangedPaths {
    <#
      .DESCRIPTION
      Paths changed between merge-base(HEAD, $Base) and the working tree, plus untracked files.
      Ok is $false, with Detail, when the base cannot be resolved.
    #>
    param([Parameter(Mandatory)][string]$RepoRoot, [Parameter(Mandatory)][string]$Base)
    $mergeBase = (& git --no-optional-locks -C $RepoRoot merge-base HEAD $Base 2>$null | Select-Object -First 1)
    if ($LASTEXITCODE -ne 0 -or -not $mergeBase) {
        return [PSCustomObject]@{ Ok = $false; MergeBase = $null; Paths = @(); Detail = "cannot resolve merge-base(HEAD, $Base)" }
    }
    $diff = @(& git --no-optional-locks -C $RepoRoot diff --name-only $mergeBase 2>$null)
    if ($LASTEXITCODE -ne 0) {
        return [PSCustomObject]@{ Ok = $false; MergeBase = $mergeBase; Paths = @(); Detail = "git diff against $mergeBase failed" }
    }
    $untracked = @(& git --no-optional-locks -C $RepoRoot ls-files --others --exclude-standard 2>$null)
    $paths = @(@($diff) + @($untracked) | Where-Object { $_ } | Sort-Object -Unique)
    return [PSCustomObject]@{ Ok = $true; MergeBase = $mergeBase.Trim(); Paths = $paths; Detail = "$($paths.Count) changed path(s) since $($mergeBase.Substring(0, 12)) (merge-base with $Base)" }
}

function Get-SlotWaitSeconds {
    <#
      .DESCRIPTION
      Pure: total seconds a child pg.ps1 spent queued for a slot, from the `[build-env] took ... slot`
      lines Enter-ResourceSlot prints. Lets the summary separate other worktrees' load from this run's work.
    #>
    param([string[]]$Lines)
    $total = 0.0
    foreach ($line in $Lines) {
        if ($line -match '\[build-env\] took \w+ slot \d+ after ([\d.]+)s') { $total += [double]::Parse($Matches[1], [Globalization.CultureInfo]::InvariantCulture) }
    }
    return $total
}

function Get-PanglossFromBuildOutput {
    <#
      .DESCRIPTION
      Pure: the pangloss the build child just built, from the target dir its `[pg] cargo build` line
      names, so the driver never grades a binary from a different target dir.
    #>
    param([string[]]$Lines, [Parameter(Mandatory)][string]$RustRoot)
    $line = $Lines | Where-Object { $_ -match '^\[pg\] cargo build .*\(target-dir: (.+?), runner:' } | Select-Object -Last 1
    if (-not $line) { return $null }
    $null = $line -match '\(target-dir: (.+?), runner:'
    $dir = if ($Matches[1] -eq '<default>') { [IO.Path]::Combine($RustRoot, 'target') } else { $Matches[1] }
    return [IO.Path]::Combine($dir, 'pg-test-opt', $(if ($IsWindows) { 'pangloss.exe' } else { 'pangloss' }))
}

function Get-KnownConformanceDivergences {
    param([Parameter(Mandatory)][string]$Path)
    return @(Get-Content -LiteralPath $Path | ForEach-Object { $_.Trim() } | Where-Object { $_ -and -not $_.StartsWith('#') })
}

function Get-ConformanceDriverVerdict {
    <#
      .DESCRIPTION
      Pure: grades one adapter-mode driver run the way run-conformance.sh does, but also refuses a
      run with no totals line or zero attempted fixtures.
    #>
    param([string[]]$Lines, [int]$ExitCode, [string[]]$Known)
    $totals = $Lines | Where-Object { $_ -match '^totals: (\d+) passed, (\d+) failed, (\d+) skipped \(of (\d+) attempted\)' } | Select-Object -Last 1
    if ($ExitCode -eq 2) { return [PSCustomObject]@{ Ok = $false; Detail = 'driver exit 2: harness-level error (bad args, no fixtures, malformed metadata)' } }
    if (-not $totals) { return [PSCustomObject]@{ Ok = $false; Detail = "driver exit $ExitCode with no totals line" } }
    $null = $totals -match '^totals: (\d+) passed, (\d+) failed, (\d+) skipped \(of (\d+) attempted\)'
    $attempted = [int]$Matches[4]
    if ($attempted -eq 0) { return [PSCustomObject]@{ Ok = $false; Detail = 'driver attempted zero fixtures' } }
    $failed = @($Lines | Where-Object { $_ -match '^\[FAIL\] (\S+)' } | ForEach-Object { $null = $_ -match '^\[FAIL\] (\S+)'; $Matches[1] })
    if ($ExitCode -ne 0 -and $ExitCode -ne 1) { return [PSCustomObject]@{ Ok = $false; Detail = "driver exit $ExitCode" } }
    if ($ExitCode -eq 1 -and $failed.Count -eq 0) { return [PSCustomObject]@{ Ok = $false; Detail = 'driver exit 1 with no [FAIL] rows' } }
    $unexpected = @($failed | Where-Object { $Known -notcontains $_ })
    if ($unexpected.Count -gt 0) {
        $named = ($unexpected | Select-Object -First 5) -join ', '
        if ($unexpected.Count -gt 5) { $named += ", and $($unexpected.Count - 5) more (listed above)" }
        return [PSCustomObject]@{ Ok = $false; Detail = "$($unexpected.Count) unexpected failure(s) of $attempted attempted: $named" }
    }
    return [PSCustomObject]@{ Ok = $true; Detail = "$attempted attempted, $($failed.Count) known divergence(s)" }
}

function Invoke-PrelandPg {
    <#
      .DESCRIPTION
      Runs one child `pg.ps1` invocation in the foreground, streaming its output, and returns its
      exit code with the captured lines.
    #>
    param([Parameter(Mandatory)][string[]]$PgArgs)
    $pg = Join-Path $PSScriptRoot 'pg.ps1'
    Write-Host "[preland] pg.ps1 $($PgArgs -join ' ')" -ForegroundColor Cyan
    $lines = [System.Collections.Generic.List[string]]::new()
    & pwsh -NoProfile -File $pg @PgArgs 2>&1 | ForEach-Object { $s = "$_"; Write-Host $s; $lines.Add($s) }
    return [PSCustomObject]@{ ExitCode = $LASTEXITCODE; Lines = $lines }
}

function Start-PrelandPg {
    <#
      .DESCRIPTION
      Starts one child `pg.ps1` invocation concurrently, its output going to a log; pair it with
      Complete-PrelandPg. It takes its own slot, so a busy machine serializes it rather than
      oversubscribing.
    #>
    param([Parameter(Mandatory)][string[]]$PgArgs)
    $log = Join-Path ([System.IO.Path]::GetTempPath()) "pg-preland-$PID-$([guid]::NewGuid().ToString('N')).log"
    Write-Host "[preland] started concurrently: pg.ps1 $($PgArgs -join ' ')" -ForegroundColor Cyan
    $argList = @('-NoProfile', '-File', (Join-Path $PSScriptRoot 'pg.ps1')) + $PgArgs
    $proc = Start-Process -FilePath 'pwsh' -ArgumentList $argList -NoNewWindow -PassThru `
        -RedirectStandardOutput $log -RedirectStandardError "$log.err"
    return [PSCustomObject]@{ Process = $proc; Log = $log; Label = "pg.ps1 $($PgArgs -join ' ')" }
}

function Complete-PrelandPg {
    param([Parameter(Mandatory)]$Started)
    $Started.Process.WaitForExit()
    $lines = @(@(Get-Content -LiteralPath $Started.Log -ErrorAction SilentlyContinue) + @(Get-Content -LiteralPath "$($Started.Log).err" -ErrorAction SilentlyContinue))
    Write-Host "[preland] output of $($Started.Label):" -ForegroundColor Cyan
    $lines | ForEach-Object { Write-Host $_ }
    Remove-Item -LiteralPath $Started.Log, "$($Started.Log).err" -Force -ErrorAction SilentlyContinue
    return [PSCustomObject]@{ ExitCode = $Started.Process.ExitCode; Lines = $lines }
}

if ($MyInvocation.InvocationName -eq '.') { return }

Assert-ScriptAndCwdAgreeOnWorktree -ScriptRoot $PSScriptRoot
$repoRoot = Get-RepoRoot
$rustRoot = Get-RustRoot
$overall = [Diagnostics.Stopwatch]::StartNew()
$results = [System.Collections.Generic.List[object]]::new()
function Add-Result {
    param([string]$Check, [string]$Status, [double]$Seconds, [string]$Detail, [double]$Queued = 0)
    $results.Add([PSCustomObject]@{ Check = $Check; Status = $Status; Seconds = [math]::Round($Seconds, 1); Queued = [math]::Round($Queued, 1); Detail = $Detail })
}

$changes = Get-PrelandChangedPaths -RepoRoot $repoRoot -Base $Base
$force = if ($All) { 'forced by -All' } elseif (-not $changes.Ok) { "forced: $($changes.Detail)" } else { '' }
Write-Host "[preland] $(if ($changes.Ok) { $changes.Detail } else { $changes.Detail + ' -- running every check' })" -ForegroundColor Cyan

$shared = @('machine', 'rust/Cargo.toml', 'rust/Cargo.lock', 'rust/rust-toolchain.toml', 'rust/tools/preland.ps1')
$scoreboardRoots = $null
$scoreboardLabel = ''
try {
    $closure = @(Get-WorkspaceCrateClosure -RustRoot $rustRoot -RepoRoot $repoRoot -Package 'pg-foma-backend')
    $scoreboardRoots = $closure + @('conformance-staging/') + $shared
    $scoreboardLabel = "pg-foma-backend's $($closure.Count)-crate closure, $((@('conformance-staging/') + $shared) -join ', ')"
} catch {
    $scoreboardRoots = @()
    if (-not $force) { $force = "forced: crate closure unavailable ($($_.Exception.Message))" }
}
$conformanceRoots = @('rust/crates/pg-cli/', 'docs/formats/', 'rust/tools/run-conformance.sh',
    'rust/tools/known-conformance-divergences.txt', '.github/workflows/conformance-ci.yml') + $shared
$scoreboardGate = Get-PrelandGateDecision -ChangedPaths $changes.Paths -Roots $scoreboardRoots -ForceReason $force -RootsLabel $scoreboardLabel
$conformanceGate = Get-PrelandGateDecision -ChangedPaths $changes.Paths -Roots $conformanceRoots -ForceReason $force

# Tool tests take no build slot and touch no cargo state, so they overlap the cargo checks.
$toolTestFiles = @(Get-ChildItem -Path (Join-Path $PSScriptRoot 'tests') -Filter '*.tests.ps1' | Sort-Object Name)
$toolSw = [Diagnostics.Stopwatch]::StartNew()
$toolJob = $toolTestFiles | ForEach-Object -ThrottleLimit $ToolTestThrottle -AsJob -Parallel {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $out = & pwsh -NoProfile -File $_.FullName 2>&1 | Out-String
    [PSCustomObject]@{ File = $_.Name; ExitCode = $LASTEXITCODE; Seconds = $sw.Elapsed.TotalSeconds; Output = $out }
}
Write-Host "[preland] tool-tests: $($toolTestFiles.Count) script(s) started in parallel (throttle $ToolTestThrottle)" -ForegroundColor Cyan

# Both gated checks read machine/conformance; grading a submodule off the staged pin grades fixtures CI never sees.
$fixtureProblem = $null
if ($scoreboardGate.Run -or $conformanceGate.Run) {
    $submodule = Initialize-ConformanceSubmodule -RepoRoot $repoRoot
    $pinCommit = ((& git -C $repoRoot ls-files -s machine 2>$null) -split '\s+')[1]
    $headCommit = (& git -C (Join-Path $repoRoot 'machine') rev-parse HEAD 2>$null | Select-Object -First 1)
    if (-not $submodule.Ok) {
        $fixtureProblem = "machine/conformance unavailable: $($submodule.Detail)"
    } elseif (-not $pinCommit -or $pinCommit -ne $headCommit) {
        $fixtureProblem = "machine submodule HEAD $headCommit is not the staged pin $pinCommit, so these fixtures are not the ones CI tests. Run preland in the worktree being landed; stage a deliberate pin move, or check out $pinCommit in machine/"
    }
}

$sw = [Diagnostics.Stopwatch]::StartNew()
$check = Invoke-PrelandPg -PgArgs @('-Mode', 'check')
Add-Result 'clippy' $(if ($check.ExitCode -eq 0) { 'PASS' } else { 'FAIL' }) $sw.Elapsed.TotalSeconds "pg.ps1 -Mode check exit $($check.ExitCode)" (Get-SlotWaitSeconds $check.Lines)
$compiles = ($check.ExitCode -eq 0)

# The scoreboard is one long test; once it is past cargo's build lock, the pangloss build overlaps it.
$scoreboardRun = $null
if ($scoreboardGate.Run -and $compiles -and -not $fixtureProblem) {
    $scoreboardSw = [Diagnostics.Stopwatch]::StartNew()
    $scoreboardRun = Start-PrelandPg -PgArgs @('-Mode', 'test', '-Package', 'pg-foma-backend', '-TestTarget', 'backend_scoreboard_gate')
    if ($conformanceGate.Run) {
        while (-not $scoreboardRun.Process.HasExited -and
            -not (Select-String -LiteralPath $scoreboardRun.Log, "$($scoreboardRun.Log).err" -Pattern '^\s*Starting \d+ tests? across' -Quiet -ErrorAction SilentlyContinue)) {
            Start-Sleep -Milliseconds 500
        }
        Write-Host "[preland] scoreboard test running after $([math]::Round($scoreboardSw.Elapsed.TotalSeconds, 1))s; building pangloss alongside it" -ForegroundColor Cyan
    }
}

if (-not $conformanceGate.Run) {
    Add-Result 'conformance' 'SKIPPED' 0 $conformanceGate.Reason
} elseif ($fixtureProblem) {
    Add-Result 'conformance' 'FAIL' 0 $fixtureProblem
} elseif (-not $compiles) {
    Add-Result 'conformance' 'FAIL' 0 'not attempted: clippy failed, so pangloss cannot be built'
} else {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $verdict = $null
    $build = $null
    $driver = Find-PinMatchedConformanceDriver -PinCommit $pinCommit
    if (-not $driver.Ok) {
        $verdict = $driver.Detail
    } else {
        $previousExtra = $env:PANGLOSS_EXTRA_ARGS
        $env:PANGLOSS_EXTRA_ARGS = '--bin pangloss'
        try {
            $build = Invoke-PrelandPg -PgArgs @('-Mode', 'build', '-Package', 'pg-cli', '-TestOptBuild')
        } finally {
            $env:PANGLOSS_EXTRA_ARGS = $previousExtra
        }
        $pangloss = Get-PanglossFromBuildOutput -Lines $build.Lines -RustRoot $rustRoot
        if ($build.ExitCode -ne 0) {
            $verdict = "pangloss build failed (pg.ps1 exit $($build.ExitCode))"
        } elseif (-not $pangloss -or -not (Test-Path -LiteralPath $pangloss)) {
            $verdict = "built, but the build output named no pangloss that exists (resolved: '$pangloss')"
        } else {
            Write-Host "[preland] conformance: pangloss $pangloss (written $((Get-Item -LiteralPath $pangloss).LastWriteTimeUtc.ToString('u'))); driver $($driver.ExePath)" -ForegroundColor Cyan
            $driverLines = @(& $driver.ExePath --fixtures (Join-Path $repoRoot 'machine/conformance') `
                    --adapter "$pangloss batch {grammar} {words} {output}" --capabilities phonology 2>&1 | ForEach-Object { "$_" })
            $driverExit = $LASTEXITCODE
            $driverLines | Where-Object { $_ -notmatch '^\[PASS\]' } | ForEach-Object { Write-Host "  $_" }
            $known = Get-KnownConformanceDivergences -Path (Join-Path $PSScriptRoot 'known-conformance-divergences.txt')
            $graded = Get-ConformanceDriverVerdict -Lines $driverLines -ExitCode $driverExit -Known $known
            if ($graded.Ok) { Add-Result 'conformance' 'PASS' $sw.Elapsed.TotalSeconds "$($conformanceGate.Reason); $($graded.Detail)" (Get-SlotWaitSeconds $build.Lines) }
            else { $verdict = $graded.Detail }
        }
    }
    if ($verdict) { Add-Result 'conformance' 'FAIL' $sw.Elapsed.TotalSeconds "$($conformanceGate.Reason); $verdict" $(if ($build) { Get-SlotWaitSeconds $build.Lines } else { 0 }) }
}

if (-not $scoreboardGate.Run) {
    Add-Result 'scoreboard' 'SKIPPED' 0 $scoreboardGate.Reason
} elseif ($fixtureProblem) {
    Add-Result 'scoreboard' 'FAIL' 0 $fixtureProblem
} elseif (-not $compiles) {
    Add-Result 'scoreboard' 'FAIL' 0 'not attempted: clippy failed'
} else {
    $run = Complete-PrelandPg -Started $scoreboardRun
    $seconds = $scoreboardSw.Elapsed.TotalSeconds
    $ran = @($run.Lines | Where-Object { $_ -match '1 test run: 1 passed' }).Count -gt 0
    $evidence = @($run.Lines | Where-Object { $_ -match 'but the ratchet says|exclusion set changed|soundness violation|expected `' } | Select-Object -First 3) -join ' | '
    if ($run.ExitCode -eq 0 -and $ran) { Add-Result 'scoreboard' 'PASS' $seconds $scoreboardGate.Reason (Get-SlotWaitSeconds $run.Lines) }
    else { Add-Result 'scoreboard' 'FAIL' $seconds "$($scoreboardGate.Reason); pg.ps1 exit $($run.ExitCode); 1 passed seen: $ran; $evidence" (Get-SlotWaitSeconds $run.Lines) }
}

$null = Wait-Job -Job $toolJob
$toolSeconds = if ($toolJob.PSEndTime) { ($toolJob.PSEndTime - $toolJob.PSBeginTime).TotalSeconds } else { $toolSw.Elapsed.TotalSeconds }
$toolResults = @(Receive-Job -Job $toolJob -Wait -AutoRemoveJob)
$toolFailed = @($toolResults | Where-Object { $_.ExitCode -ne 0 })
foreach ($f in $toolFailed) {
    Write-Host "=== FAILED tool test: $($f.File) (exit $($f.ExitCode)) ===" -ForegroundColor Red
    Write-Host $f.Output
}
if ($toolResults.Count -ne $toolTestFiles.Count) {
    Add-Result 'tool-tests' 'FAIL' $toolSeconds "only $($toolResults.Count) of $($toolTestFiles.Count) script(s) reported"
} elseif ($toolFailed.Count -gt 0) {
    Add-Result 'tool-tests' 'FAIL' $toolSeconds "$($toolFailed.Count) of $($toolResults.Count) failed: $(($toolFailed | ForEach-Object File) -join ', ')"
} else {
    $slowest = $toolResults | Sort-Object Seconds -Descending | Select-Object -First 1
    Add-Result 'tool-tests' 'PASS' $toolSeconds "$($toolResults.Count) script(s); slowest $($slowest.File) $([math]::Round($slowest.Seconds, 1))s"
}

Write-Host ''
Write-Host '===== preland summary =====' -ForegroundColor Cyan
foreach ($r in $results) {
    $color = switch ($r.Status) { 'PASS' { 'Green' } 'SKIPPED' { 'Yellow' } default { 'Red' } }
    $queued = if ($r.Queued -gt 0) { " ($($r.Queued)s of it queued for a build slot)" } else { '' }
    Write-Host ("  {0,-8} {1,-12} {2,7}s  {3}{4}" -f $r.Status, $r.Check, $r.Seconds, $r.Detail, $queued) -ForegroundColor $color
}
$failures = @($results | Where-Object { $_.Status -eq 'FAIL' })
Write-Host ("preland: {0} check(s), {1} failed, {2} skipped, {3}s wall" -f $results.Count, $failures.Count, @($results | Where-Object { $_.Status -eq 'SKIPPED' }).Count, [math]::Round($overall.Elapsed.TotalSeconds, 1)) -ForegroundColor $(if ($failures.Count) { 'Red' } else { 'Green' })
if ($failures.Count -gt 0) { exit $script:ExitCodePrelandFailed }
exit 0
