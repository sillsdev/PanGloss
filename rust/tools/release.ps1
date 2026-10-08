<#
  .DESCRIPTION
  Cut a PanGloss release locally by stamping the workspace version, refreshing Cargo.lock through
  pg.ps1, committing the stamp, and creating an annotated vX.Y.Z tag. The script never pushes.
  Push main and the tag together; the tag-triggered Release workflow waits for Rust CI to finish.

  Preconditions:
    - clean working tree on the current, up-to-date main branch
    - a new numeric x.y.z version and a matching CHANGELOG.md section
    - managed pg.ps1 -Mode check succeeds after stamping, refreshing the lockfile

  -DryRun checks the branch, tree, version, tag, and changelog without stamping or tagging.
  CI accepts only annotated vX.Y.Z tags whose commit is on origin/main. Prereleases are not
  supported. Run workflow_dispatch with an explicit ref and version for a build-only rehearsal.

  Examples:
    rust\tools\release.ps1 -Version 0.7.0 -DryRun
    rust\tools\release.ps1 -Version 0.7.0

  Exit codes: 30 dirty tree, 31 branch or remote-main mismatch, 32 managed check failed,
  33 version or changelog refusal, 34 existing tag, 0 success.
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+$')][string]$Version,
    [switch]$DryRun,
    [int]$MaxConcurrent = 2
)

$ErrorActionPreference = 'Stop'
$toolRoot = $PSScriptRoot
$repoRoot = (Resolve-Path (Join-Path $toolRoot '..\..')).Path
$cargoToml = Join-Path $repoRoot 'rust\Cargo.toml'
$cargoLock = Join-Path $repoRoot 'rust\Cargo.lock'
$changelog = Join-Path $repoRoot 'CHANGELOG.md'
$tag = "v$Version"

function Write-Refusal([string]$Gate, [string]$Reason, [int]$ExitCode) {
    Write-Host "[release] $Gate REFUSED -- $Reason" -ForegroundColor Red
    exit $ExitCode
}

function Invoke-ManagedCheck {
    $pwsh = (Get-Process -Id $PID).Path
    $argv = @('-NoProfile', '-NonInteractive', '-File', (Join-Path $toolRoot 'pg.ps1'), '-Mode', 'check', '-MaxConcurrent', "$MaxConcurrent")
    Push-Location $repoRoot
    try {
        $transcript = & $pwsh @argv 2>&1 | ForEach-Object { Write-Host $_; "$_" }
        $exitCode = $LASTEXITCODE
    } finally {
        Pop-Location
    }

    if ($exitCode -eq 0) { return 0 }
    if ($exitCode -eq 27) {
        $joined = $transcript -join "`n"
        if ($joined -match '(?m)^\s*Finished `.*` profile' -and $joined -notmatch '(?m)^\s*FAIL \[|(?m)^error(\[|:)') {
            Write-Host '[release] pg.ps1 reported payload success before its wrapper exited 27; accepting the recorded check result.' -ForegroundColor Yellow
            return 0
        }
    }
    return $exitCode
}

$branch = git -C $repoRoot branch --show-current
if ($LASTEXITCODE -ne 0 -or $branch -cne 'main') {
    Write-Refusal 'branch' "run this cut from main (current branch: '$branch')" 31
}

$dirty = @(git -C $repoRoot status --porcelain --untracked-files=all --ignore-submodules=all)
if ($LASTEXITCODE -ne 0) { Write-Refusal 'tree' 'git status failed; the tree could not be verified' 30 }
if ($dirty.Count -gt 0) {
    Write-Refusal 'tree' "working tree has uncommitted changes: $($dirty -join '; ')" 30
}

git -C $repoRoot fetch --quiet --no-tags origin refs/heads/main:refs/remotes/origin/main
if ($LASTEXITCODE -ne 0) {
    Write-Refusal 'branch' 'could not fetch origin/main; the current main tip could not be verified' 31
}
$head = git -C $repoRoot rev-parse HEAD
$remoteMain = git -C $repoRoot rev-parse refs/remotes/origin/main
if ($LASTEXITCODE -ne 0 -or $head -cne $remoteMain) {
    Write-Refusal 'branch' "local main is not up to date with origin/main (local $head, remote $remoteMain)" 31
}

$localTag = git -C $repoRoot tag --list $tag
if ($LASTEXITCODE -ne 0) { Write-Refusal 'tag' 'git could not inspect local tags' 34 }
if ($localTag) { Write-Refusal 'tag' "$tag already exists locally" 34 }
$remoteTag = git -C $repoRoot ls-remote --exit-code --tags origin "refs/tags/$tag"
if ($LASTEXITCODE -eq 0 -or $remoteTag) { Write-Refusal 'tag' "$tag already exists on origin" 34 }
if ($LASTEXITCODE -notin @(0, 2)) { Write-Refusal 'tag' 'could not check origin for an existing tag' 34 }

$tomlText = Get-Content -LiteralPath $cargoToml -Raw
$workspace = [regex]::Match($tomlText, '(?ms)^\[workspace\.package\]\r?\n(?<body>.*?)(?=^\[|\z)')
if (-not $workspace.Success) { Write-Refusal 'version' 'rust/Cargo.toml has no [workspace.package] section' 33 }
$versionLine = [regex]::new('(?m)^(?<prefix>version\s*=\s*)"(?<version>[^"]+)"')
$currentVersion = $versionLine.Match($workspace.Groups['body'].Value)
if (-not $currentVersion.Success) { Write-Refusal 'version' 'rust/Cargo.toml has no workspace package version' 33 }
if ($currentVersion.Groups['version'].Value -ceq $Version) {
    Write-Refusal 'version' "workspace version is already $Version; refusing to cut the same version again" 33
}

if (-not (Test-Path -LiteralPath $changelog -PathType Leaf)) {
    Write-Refusal 'changelog' 'CHANGELOG.md is missing' 33
}
$lockBefore = if (Test-Path -LiteralPath $cargoLock -PathType Leaf) { Get-Content -LiteralPath $cargoLock -Raw } else { $null }
if ($null -eq $lockBefore) { Write-Refusal 'lockfile' 'rust/Cargo.lock is missing before the managed refresh' 33 }
$changelogText = Get-Content -LiteralPath $changelog -Raw
if ($changelogText -notmatch ('(?m)^##\s+' + [regex]::Escape($Version) + '\s*$')) {
    Write-Refusal 'changelog' "CHANGELOG.md has no '## $Version' section; author release notes before cutting" 33
}

Write-Host "[release] preflight green: main=$head, version $($currentVersion.Groups['version'].Value) -> $Version, changelog section present"
if ($DryRun) {
    Write-Host "[release] DRY RUN -- no files, commits, or tags changed. Ready to stamp and tag $tag."
    Write-Host "[release] push main and the tag to start Rust CI and Release: git push --atomic origin main $tag"
    exit 0
}

$bodyGroup = $workspace.Groups['body']
$body = $bodyGroup.Value
$updatedBody = $versionLine.Replace($body, ('$1"' + $Version + '"'), 1)
if ($updatedBody -ceq $body) { Write-Refusal 'version' 'workspace version stamp changed nothing' 33 }
$stampedToml = $tomlText.Substring(0, $bodyGroup.Index) + $updatedBody + $tomlText.Substring($bodyGroup.Index + $bodyGroup.Length)
[System.IO.File]::WriteAllText($cargoToml, $stampedToml, [System.Text.UTF8Encoding]::new($false))

Write-Host '[release] refreshing Cargo.lock and checking all targets through managed pg.ps1 -Mode check'
$checkExit = Invoke-ManagedCheck
if ($checkExit -ne 0) {
    Write-Host "[release] managed check failed with exit $checkExit; the version stamp is left in the tree for inspection" -ForegroundColor Red
    exit 32
}
if (-not (Test-Path -LiteralPath $cargoLock -PathType Leaf)) {
    Write-Host '[release] managed check succeeded but rust/Cargo.lock is missing; refusing to commit an unverifiable release' -ForegroundColor Red
    exit 32
}
$lockAfter = Get-Content -LiteralPath $cargoLock -Raw
if ($lockAfter -ceq $lockBefore) {
    Write-Host '[release] managed check succeeded but did not refresh Cargo.lock for the new workspace version' -ForegroundColor Red
    exit 32
}
$lockedCliVersion = $null
foreach ($package in [regex]::Matches($lockAfter, '(?ms)^\[\[package\]\]\r?\n(?<body>.*?)(?=^\[\[package\]\]|\z)')) {
    if ($package.Groups['body'].Value -match '(?m)^name\s*=\s*"pg-cli"\s*$') {
        $versionMatch = [regex]::Match($package.Groups['body'].Value, '(?m)^version\s*=\s*"(?<version>[^"]+)"\s*$')
        if ($versionMatch.Success) { $lockedCliVersion = $versionMatch.Groups['version'].Value }
    }
}
if ($lockedCliVersion -cne $Version) {
    Write-Host "[release] Cargo.lock records pg-cli version '$lockedCliVersion'; expected '$Version' after the managed refresh" -ForegroundColor Red
    exit 32
}
$postCheckDirty = @(git -C $repoRoot status --porcelain --untracked-files=all --ignore-submodules=all)
if ($LASTEXITCODE -ne 0) { Write-Host '[release] could not verify the managed check left only release files changed' -ForegroundColor Red; exit 32 }
$unexpectedChanges = @($postCheckDirty | Where-Object { $_ -notmatch '^\s*M\s+rust[/\\]Cargo\.(toml|lock)$' })
if ($unexpectedChanges.Count -gt 0) {
    Write-Host '[release] managed check changed files outside rust/Cargo.toml and rust/Cargo.lock; inspect the tree before retrying' -ForegroundColor Red
    $unexpectedChanges | ForEach-Object { Write-Host "    $_" }
    exit 32
}

git -C $repoRoot add -- rust/Cargo.toml rust/Cargo.lock
if ($LASTEXITCODE -ne 0) { Write-Host '[release] git add failed; version stamp is left in the tree' -ForegroundColor Red; exit 32 }
git -C $repoRoot diff --cached --quiet
if ($LASTEXITCODE -eq 0) { Write-Host '[release] managed version stamp produced no Cargo.toml or Cargo.lock changes' -ForegroundColor Red; exit 33 }
if ($LASTEXITCODE -ne 1) { Write-Host '[release] could not inspect the staged version stamp' -ForegroundColor Red; exit 32 }

git -C $repoRoot commit -m "release: $tag"
if ($LASTEXITCODE -ne 0) { Write-Host '[release] version stamp commit failed; staged files are left for inspection' -ForegroundColor Red; exit 32 }
git -C $repoRoot tag -a $tag -m "PanGloss $tag"
if ($LASTEXITCODE -ne 0) { Write-Host "[release] annotated tag creation failed after the version commit $tag" -ForegroundColor Red; exit 34 }

Write-Host "[release] created release commit and annotated tag $tag. No push was run."
Write-Host "[release] push main and the tag to start Rust CI and Release: git push --atomic origin main $tag"
exit 0
