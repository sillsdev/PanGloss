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
    9. print the artifact paths and the exact push command -- IT NEVER PUSHES; publishing a tag is
       the one step that should stay a deliberate human act.

  Examples:
    rust\tools\release.ps1 -Version 0.2.0 -DryRun    # run every gate, mutate nothing
    rust\tools\release.ps1 -Version 0.2.0            # the real thing
    rust\tools\release.ps1 -Version 0.2.0 -SkipGate test  # emergency only; recorded in the tag message

  Exit codes: 30 dirty-tree, 31 hygiene, 32 doc, 33 test, 34 oracle, 35 version/changelog,
  36 tag-exists, 0 success. Distinct codes so automation can tell WHICH gate refused.
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

function Write-Gate([string]$name, [string]$state) { Write-Host ("[release] gate {0,-8} {1}" -f $name, $state) }

# --- gate 1: clean tree ---------------------------------------------------------------------
$dirty = git -C $repoRoot status --porcelain --ignore-submodules=all
if ($dirty) {
    Write-Gate 'tree' 'REFUSED -- working tree has uncommitted changes:'
    $dirty | ForEach-Object { Write-Host "    $_" }
    exit 30
}
Write-Gate 'tree' 'clean'

# --- gate 2: hygiene ------------------------------------------------------------------------
if ($SkipGate -contains 'hygiene') { Write-Gate 'hygiene' 'SKIPPED (recorded)' }
else {
    & (Join-Path $toolRoot 'comment-hygiene.ps1') | Out-Null
    if ($LASTEXITCODE -ne 0) { Write-Gate 'hygiene' 'REFUSED -- violations present (run comment-hygiene.ps1 -List)'; exit 31 }
    Write-Gate 'hygiene' 'clean (0 violations)'
}

# --- gate 3: rustdoc ------------------------------------------------------------------------
if ($SkipGate -contains 'doc') { Write-Gate 'doc' 'SKIPPED (recorded)' }
else {
    & (Join-Path $toolRoot 'pg.ps1') -Mode doc -MaxConcurrent $MaxConcurrent
    if ($LASTEXITCODE -ne 0) { Write-Gate 'doc' "REFUSED -- pg.ps1 -Mode doc exited $LASTEXITCODE"; exit 32 }
    Write-Gate 'doc' 'green'
}

# --- gate 4: full suite ---------------------------------------------------------------------
if ($SkipGate -contains 'test') { Write-Gate 'test' 'SKIPPED (recorded)' }
else {
    $env:PANGLOSS_CONFORMANCE_SCOPE = 'all'
    & (Join-Path $toolRoot 'pg.ps1') -Mode test -MaxConcurrent $MaxConcurrent
    if ($LASTEXITCODE -ne 0) { Write-Gate 'test' "REFUSED -- pg.ps1 -Mode test exited $LASTEXITCODE"; exit 33 }
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
        $SkipGate = @($SkipGate) + 'oracle-unavailable'
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
& (Join-Path $toolRoot 'pg.ps1') -Mode check -MaxConcurrent $MaxConcurrent
if ($LASTEXITCODE -ne 0) { Write-Host '[release] post-stamp check failed; version stamp left in tree for inspection'; exit 33 }

git -C $repoRoot add rust/Cargo.toml rust/Cargo.lock CHANGELOG.md
$skipNote = if ($SkipGate) { "`n`nGates skipped or unavailable: $($SkipGate -join ', ')" } else { '' }
git -C $repoRoot commit -m "release: v$Version$skipNote"
git -C $repoRoot tag -a "v$Version" -m "PanGloss v$Version$skipNote"

# --- artifact -------------------------------------------------------------------------------
& (Join-Path $toolRoot 'pg.ps1') -Mode release -MaxConcurrent $MaxConcurrent
if ($LASTEXITCODE -ne 0) { Write-Host '[release] artifact build failed AFTER tagging -- fix and re-run -Mode release; the tag itself is sound'; exit 33 }

Write-Host ''
Write-Host "[release] v$Version tagged. Publishing stays manual:"
Write-Host "    git push origin HEAD --follow-tags"
exit 0
