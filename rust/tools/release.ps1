<#
  .DESCRIPTION
  Release entry point for the PanGloss Rust workspace. THIN FRONT END in the same family as
  build.ps1/test.ps1: every build runs through pg.ps1, so target-dir redirection, sccache, the
  build-slot mutex, and the job-object ceilings all apply unchanged. What this script adds is the
  release CONTRACT: it refuses to stamp, tag, or produce artifacts unless every gate below is green,
  because a release that skipped a gate is indistinguishable from one that passed it.

  Gates, in order (cheapest first, so a refusal costs the least machine time):
    1. clean working tree (nothing uncommitted; a release must be reproducible from its tag)
    2. comment hygiene: ZERO violations (comment-hygiene.ps1 exit 0 -- deliberate, not a ratchet)
    3. rustdoc: pg.ps1 -Mode doc (the only enforcement of broken_intra_doc_links = "deny")
    4. full test suite: pg.ps1 -Mode test (nextest, --no-fail-fast)
    5. C# founding-oracle differential: oracle-conformance.ps1 -Scope all (skipped with a loud
       warning when the oracle exe is absent on this machine -- an absent tool must never block the
       workflow, but the skip is printed in the release record so it can never read as "passed")

  Then, and only then:
    6. stamp [workspace.package] version in rust/Cargo.toml (single source; all crates inherit)
    7. verify CHANGELOG.md has a section for the new version (it will not write one for you --
       release notes are authored, not generated)
    8. commit the stamp, tag v<version> (annotated), and build the optimized artifact via
       pg.ps1 -Mode release
    9. print the artifact paths and the exact push command -- IT NEVER PUSHES.

  THIS SCRIPT NO LONGER TAGS FROM A WORKSTATION. Steps 6-9 run only inside GitHub Actions; locally
  it refuses unless -DryRun. The release surface is .github/workflows/release.yml, whose contract
  gate is the machine/conformance suite.

  Examples:
    rust\tools\release.ps1 -Version 0.2.0 -DryRun    # run every gate, mutate nothing
    rust\tools\release.ps1 -Version 0.2.0            # the real thing
    rust\tools\release.ps1 -Version 0.2.0 -SkipGate test  # emergency only; recorded in the tag message

  Exit codes: 30 dirty-tree, 31 hygiene, 32 doc, 33 test, 34 oracle, 35 version/changelog,
  36 tag-exists, 37 run-locally-without-DryRun, 0 success. Distinct codes so automation can tell
  WHICH gate refused.
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory = $true)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
    [switch]$DryRun,
    # Names of gates to skip, recorded in the tag annotation so a skipped gate stays visible in history.
    [ValidateSet('hygiene', 'doc', 'test', 'oracle')][string[]]$SkipGate = @(),
    [int]$MaxConcurrent = 2
)

$ErrorActionPreference = 'Stop'
$toolRoot = $PSScriptRoot
$repoRoot = (Resolve-Path (Join-Path $toolRoot '..\..')).Path
$cargoToml = Join-Path $repoRoot 'rust\Cargo.toml'
$changelog = Join-Path $repoRoot 'CHANGELOG.md'
# Gates that ran but could not judge (absent oracle); recorded in the tag alongside -SkipGate, kept apart because -SkipGate is validated.
$gateNotes = @()

function Write-Gate([string]$name, [string]$state) { Write-Host ("[release] gate {0,-8} {1}" -f $name, $state) }

# Judges a pg.ps1 mode by effect: exit 27 (wedged governor, payload done) passes only when the transcript proves success, and only a child pwsh puts cargo's -NoNewWindow console output into that transcript.
function Invoke-GatedPg([hashtable]$PgArgs, [string]$SuccessPattern) {
    $pwsh = (Get-Process -Id $PID).Path
    $argv = @('-NoProfile', '-NonInteractive', '-File', (Join-Path $toolRoot 'pg.ps1'))
    foreach ($k in $PgArgs.Keys) { $argv += "-$k"; $argv += "$($PgArgs[$k])" }
    $transcript = & $pwsh @argv 2>&1 | ForEach-Object { Write-Host $_; "$_" }
    $exit = $LASTEXITCODE
    if ($exit -eq 0) { return 0 }
    if ($exit -eq 27) {
        $joined = $transcript -join "`n"
        if ($joined -match $SuccessPattern -and $joined -notmatch '(?m)^\s*FAIL \[|(?m)^error(\[|:)') {
            Write-Host '[release] wrapper exited 27 after the payload finished; the transcript shows the payload succeeded, so this gate passes on evidence.' -ForegroundColor Yellow
            return 0
        }
    }
    return $exit
}

# --- gate 0: not the release surface; tagging is .github/workflows/release.yml's alone ---------
if (-not $DryRun -and $env:GITHUB_ACTIONS -ne 'true') {
    Write-Gate 'surface' 'REFUSED -- releases are cut by CI, not locally.'
    Write-Host '    Run the Release workflow (Actions -> Release -> Run workflow) with the version to tag.'
    Write-Host '    To check a tree before dispatching it, re-run this script with -DryRun.'
    exit 37
}

# --- gate 1: clean tree ---------------------------------------------------------------------
$dirty = git -C $repoRoot status --porcelain --ignore-submodules=all
if ($dirty) {
    Write-Gate 'tree' 'REFUSED -- working tree has uncommitted changes:'
    $dirty | ForEach-Object { Write-Host "    $_" }
    exit 30
}
# pg.ps1 APPLIES rustfmt before every compile, so an unformatted tree would be rewritten mid-release and every later gate would rebuild from changed sources.
$fmtHunks = @(& cargo fmt --all --manifest-path $cargoToml -- --check 2>&1 | Where-Object { $_ -match '^Diff in ' }).Count
if ($fmtHunks -gt 0) {
    Write-Gate 'tree' "REFUSED -- $fmtHunks rustfmt hunk(s) not yet applied; run pg.ps1 -Mode check, commit the reflow, then release"
    exit 30
}
Write-Gate 'tree' 'clean (and rustfmt-clean)'

# --- gate 2: hygiene ------------------------------------------------------------------------
if ($SkipGate -contains 'hygiene') { Write-Gate 'hygiene' 'SKIPPED (recorded)' }
else {
    & (Join-Path $toolRoot 'comment-hygiene.ps1') | Out-Null
    if ($LASTEXITCODE -eq 1) { Write-Gate 'hygiene' 'REFUSED -- violations present (run comment-hygiene.ps1 -List)'; exit 31 }
    if ($LASTEXITCODE -ne 0) { Write-Gate 'hygiene' "REFUSED -- checker failed (exit $LASTEXITCODE); no hygiene verdict"; exit 31 }
    Write-Gate 'hygiene' 'clean (0 violations)'
}

# --- gate 3: rustdoc ------------------------------------------------------------------------
if ($SkipGate -contains 'doc') { Write-Gate 'doc' 'SKIPPED (recorded)' }
else {
    $docExit = Invoke-GatedPg -PgArgs @{ Mode = 'doc'; MaxConcurrent = $MaxConcurrent } -SuccessPattern '(?m)^\s*Finished `dev` profile'
    if ($docExit -ne 0) { Write-Gate 'doc' "REFUSED -- pg.ps1 -Mode doc exited $docExit"; exit 32 }
    Write-Gate 'doc' 'green'
}

# --- gate 4: full suite ---------------------------------------------------------------------
if ($SkipGate -contains 'test') { Write-Gate 'test' 'SKIPPED (recorded)' }
else {
    $env:PANGLOSS_CONFORMANCE_SCOPE = 'all'
    # nextest's summary is the evidence: a run with any failure prints "N failed" there.
    $testExit = Invoke-GatedPg -PgArgs @{ Mode = 'test'; MaxConcurrent = $MaxConcurrent } -SuccessPattern '(?m)^\s*Summary \[.*\] \d+ tests? run: \d+ passed(?!.*\d+ (failed|timed out))'
    if ($testExit -ne 0) { Write-Gate 'test' "REFUSED -- pg.ps1 -Mode test exited $testExit"; exit 33 }
    Write-Gate 'test' 'green'
}

# --- gate 5: founding oracle ----------------------------------------------------------------
if ($SkipGate -contains 'oracle') { Write-Gate 'oracle' 'SKIPPED (recorded)' }
else {
    & (Join-Path $toolRoot 'oracle-conformance.ps1') -Scope all
    $oracleExit = $LASTEXITCODE
    if ($oracleExit -eq 25) {
        # Exe-not-found is the absent-tool case, not a divergence: warn loudly, record, continue.
        Write-Gate 'oracle' 'UNAVAILABLE on this machine (exe not found) -- recorded, not treated as passed'
        $gateNotes += 'oracle-unavailable'
    }
    elseif ($oracleExit -ne 0) { Write-Gate 'oracle' "REFUSED -- oracle-conformance exited $oracleExit"; exit 34 }
    else { Write-Gate 'oracle' 'green (no divergence outside baseline)' }
}

# --- version + changelog --------------------------------------------------------------------
$tomlText = Get-Content $cargoToml -Raw
if ($tomlText -notmatch '(?m)^version\s*=\s*"(?<v>[^"]+)"') { Write-Host '[release] cannot find [workspace.package] version in rust/Cargo.toml'; exit 35 }
$current = $Matches['v']
Write-Host "[release] version: $current -> $Version"
if ($current -eq $Version) { Write-Host '[release] version unchanged -- refusing a re-release of the same number'; exit 35 }
if (git -C $repoRoot tag --list "v$Version") { Write-Host "[release] tag v$Version already exists"; exit 36 }
if (-not (Test-Path $changelog) -or ((Get-Content $changelog -Raw) -notmatch [regex]::Escape("## $Version"))) {
    Write-Host "[release] CHANGELOG.md has no '## $Version' section -- write the release notes first (they are authored, not generated)"
    exit 35
}
Write-Gate 'version' "ok (v$Version is new, changelog section present)"

if ($DryRun) {
    Write-Host "[release] DRY RUN -- every gate evaluated; nothing stamped, tagged, or built."
    exit 0
}

# --- stamp, commit, tag ---------------------------------------------------------------------
$stamped = $tomlText -replace '(?m)^(version\s*=\s*)"[^"]+"', ('$1"' + $Version + '"')
Set-Content -Path $cargoToml -Value $stamped -NoNewline
# Cargo.lock records every workspace crate's version; regenerate it or the tagged tree won't build with --locked.
$checkExit = Invoke-GatedPg -PgArgs @{ Mode = 'check'; MaxConcurrent = $MaxConcurrent } -SuccessPattern '(?m)^\s*Finished `.*` profile'
if ($checkExit -ne 0) { Write-Host '[release] post-stamp check failed; version stamp left in tree for inspection'; exit 33 }

git -C $repoRoot add rust/Cargo.toml rust/Cargo.lock CHANGELOG.md
$recorded = @($SkipGate) + @($gateNotes)
$skipNote = if ($recorded) { "`n`nGates skipped or unavailable: $($recorded -join ', ')" } else { '' }
git -C $repoRoot commit -m "release: v$Version$skipNote"
git -C $repoRoot tag -a "v$Version" -m "PanGloss v$Version$skipNote"

# --- artifact -------------------------------------------------------------------------------

# The export line, not just cargo's: -Mode release exits 28 when it compiled but had nothing to copy out.
$artifactExit = Invoke-GatedPg -PgArgs @{ Mode = 'release'; MaxConcurrent = $MaxConcurrent } -SuccessPattern '(?m)^\[pg\] release artifact: '
if ($artifactExit -ne 0) { Write-Host '[release] artifact build failed AFTER tagging -- fix and re-run -Mode release; the tag itself is sound'; exit 33 }

$distDir = Join-Path $repoRoot "dist\v$Version"
$artifacts = @(Get-ChildItem $distDir -File -ErrorAction SilentlyContinue | Where-Object { $_.Extension -ne '.sha256' })
if ($artifacts.Count -eq 0) {
    Write-Host "[release] -Mode release reported success but $distDir holds no binary -- refusing to call this release complete."
    exit 33
}

Write-Host ''
Write-Host "[release] v$Version tagged. Artifacts (outside every reclaimable target dir):"
foreach ($a in $artifacts) { Write-Host "    $($a.FullName)  ($([math]::Round($a.Length / 1MB, 1)) MB, sha256 in $($a.Name).sha256)" }
Write-Host '[release] Publishing stays manual:'
Write-Host "    git push origin HEAD --follow-tags"
exit 0
