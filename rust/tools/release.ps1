<#
  .DESCRIPTION
  Cut a PanGloss release locally by stamping the workspace version, refreshing Cargo.lock through
  pg.ps1, committing the stamp, and creating an annotated vX.Y.Z tag. The script never pushes.
  Push main and the tag together; the tag-triggered Release workflow waits for Rust CI to finish.

  Preconditions:
    - clean working tree on the current, up-to-date main branch
    - a new numeric x.y.z version and a matching CHANGELOG.md section
    - managed offline lock refresh and locked pg.ps1 -Mode check succeed after stamping

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

function Invoke-ManagedMode([ValidateSet('lock-refresh', 'check')][string]$Mode) {
    $pwsh = (Get-Process -Id $PID).Path
    $argv = @('-NoProfile', '-NonInteractive', '-File', (Join-Path $toolRoot 'pg.ps1'), '-Mode', $Mode, '-MaxConcurrent', "$MaxConcurrent")
    if ($Mode -eq 'check') { $argv += '--locked' }
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

function Get-WorkspacePackageDefinitions {
    $rustRoot = Split-Path -Parent $cargoToml
    $manifest = Get-Content -LiteralPath $cargoToml -Raw
    $workspaceSection = [regex]::Match($manifest, '(?ms)^\[workspace\]\r?\n(?<body>.*?)(?=^\[|\z)')
    $memberList = if ($workspaceSection.Success) {
        [regex]::Match($workspaceSection.Groups['body'].Value, '(?ms)^\s*members\s*=\s*\[(?<body>.*?)\]')
    } else { $null }
    if (-not $memberList -or -not $memberList.Success) { return @() }

    $packages = @()
    foreach ($member in [regex]::Matches($memberList.Groups['body'].Value, '"(?<path>[^"\r\n]+)"')) {
        $memberPath = $member.Groups['path'].Value
        $manifests = @()
        if ($memberPath -match '[*?]') {
            $memberDirs = @(Get-ChildItem -Path (Join-Path $rustRoot $memberPath) -Directory -ErrorAction SilentlyContinue)
            $manifests = @($memberDirs | ForEach-Object { Join-Path $_.FullName 'Cargo.toml' } | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf })
        } else {
            $candidate = Join-Path (Join-Path $rustRoot $memberPath) 'Cargo.toml'
            if (Test-Path -LiteralPath $candidate -PathType Leaf) { $manifests = @($candidate) }
        }
        if ($manifests.Count -eq 0) {
            throw "workspace member '$memberPath' has no Cargo.toml"
        }
        foreach ($memberManifest in $manifests) {
            $memberText = Get-Content -LiteralPath $memberManifest -Raw
            $packageSection = [regex]::Match($memberText, '(?ms)^\[package\]\r?\n(?<body>.*?)(?=^\[|\z)')
            if (-not $packageSection.Success) { throw "workspace member manifest has no [package] section: $memberManifest" }
            $nameMatch = [regex]::Match($packageSection.Groups['body'].Value, '(?m)^\s*name\s*=\s*"(?<name>[^"]+)"')
            if (-not $nameMatch.Success) { throw "workspace member has no package name: $memberManifest" }
            $usesWorkspaceVersion = [regex]::IsMatch(
                $packageSection.Groups['body'].Value,
                '(?m)^\s*version\.workspace\s*=\s*true\s*(?:#.*)?$|^\s*version\s*=\s*\{\s*workspace\s*=\s*true(?:\s*,[^}]*)?\s*\}'
            )
            $packages += [PSCustomObject]@{
                Name = $nameMatch.Groups['name'].Value
                UsesWorkspaceVersion = $usesWorkspaceVersion
            }
        }
    }
    return $packages
}

# A Windows checkout with core.autocrlf holds CRLF, which Cargo rewrites as LF; only content may count.
function Get-LockText([string]$Path) {
    return (Get-Content -LiteralPath $Path -Raw) -replace "`r`n", "`n"
}

function Get-LockWithWorkspaceVersionsMasked([string]$LockText, [string[]]$WorkspacePackageNames) {
    $packagePattern = '(?ms)^\[\[package\]\]\r?\n(?<body>.*?)(?=^\[\[package\]\]|\z)'
    $builder = [System.Text.StringBuilder]::new()
    $cursor = 0
    foreach ($package in [regex]::Matches($LockText, $packagePattern)) {
        [void]$builder.Append($LockText.Substring($cursor, $package.Index - $cursor))
        $block = $package.Value
        $body = $package.Groups['body'].Value
        $nameMatch = [regex]::Match($body, '(?m)^name\s*=\s*"(?<name>[^"]+)"\s*$')
        $isLocalWorkspaceMember = $nameMatch.Success -and
            ($WorkspacePackageNames -ccontains $nameMatch.Groups['name'].Value) -and
            ($body -notmatch '(?m)^source\s*=')
        if ($isLocalWorkspaceMember) {
            $versionLine = [regex]::new('(?m)^(?<prefix>version\s*=\s*")[^"]+(?<suffix>"[ \t]*\r?)$')
            if ($versionLine.Matches($block).Count -eq 1) {
                $block = $versionLine.Replace($block, '${prefix}<workspace-version>${suffix}', 1)
            }
        }
        [void]$builder.Append($block)
        $cursor = $package.Index + $package.Length
    }
    [void]$builder.Append($LockText.Substring($cursor))
    return $builder.ToString()
}

function Get-LocalLockVersions([string]$LockText, [string]$PackageName) {
    $versions = @()
    foreach ($package in [regex]::Matches($LockText, '(?ms)^\[\[package\]\]\r?\n(?<body>.*?)(?=^\[\[package\]\]|\z)')) {
        $body = $package.Groups['body'].Value
        $nameMatch = [regex]::Match($body, '(?m)^name\s*=\s*"(?<name>[^"]+)"\s*$')
        if ($nameMatch.Success -and $nameMatch.Groups['name'].Value -ceq $PackageName -and $body -notmatch '(?m)^source\s*=') {
            $versionMatch = [regex]::Match($body, '(?m)^version\s*=\s*"(?<version>[^"]+)"\s*$')
            if ($versionMatch.Success) { $versions += $versionMatch.Groups['version'].Value }
        }
    }
    return $versions
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
$lockBefore = if (Test-Path -LiteralPath $cargoLock -PathType Leaf) { Get-LockText $cargoLock } else { $null }
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

$workspacePackages = @()
try { $workspacePackages = @(Get-WorkspacePackageDefinitions) }
catch { Write-Host "[release] could not identify workspace members before lock validation: $_" -ForegroundColor Red; exit 33 }
if ($workspacePackages.Count -eq 0) { Write-Host '[release] could not identify any Cargo workspace member packages for lock validation' -ForegroundColor Red; exit 33 }

Write-Host '[release] refreshing Cargo.lock offline through managed pg.ps1 -Mode lock-refresh'
$refreshExit = Invoke-ManagedMode -Mode 'lock-refresh'
if ($refreshExit -ne 0) {
    Write-Host "[release] managed offline lock refresh failed with exit $refreshExit; the version stamp is left in the tree for inspection" -ForegroundColor Red
    exit 32
}
if (-not (Test-Path -LiteralPath $cargoLock -PathType Leaf)) {
    Write-Host '[release] managed offline lock refresh succeeded but rust/Cargo.lock is missing' -ForegroundColor Red
    exit 32
}
$lockAfterRefresh = Get-LockText $cargoLock
if ($lockAfterRefresh -ceq $lockBefore) {
    Write-Host '[release] managed offline lock refresh succeeded but did not refresh Cargo.lock for the new workspace version' -ForegroundColor Red
    exit 32
}
$workspacePackageNames = @($workspacePackages | Where-Object { $_.UsesWorkspaceVersion } | ForEach-Object { $_.Name })
$maskedLockBefore = Get-LockWithWorkspaceVersionsMasked -LockText $lockBefore -WorkspacePackageNames $workspacePackageNames
$maskedLockAfter = Get-LockWithWorkspaceVersionsMasked -LockText $lockAfterRefresh -WorkspacePackageNames $workspacePackageNames
if ($maskedLockAfter -cne $maskedLockBefore) {
    Write-Host '[release] managed offline lock refresh changed Cargo.lock outside local workspace-member version lines; refusing to commit the lockfile' -ForegroundColor Red
    exit 32
}
foreach ($workspacePackage in @($workspacePackages | Where-Object { $_.UsesWorkspaceVersion })) {
    $lockedVersions = @(Get-LocalLockVersions -LockText $lockAfterRefresh -PackageName $workspacePackage.Name)
    if ($lockedVersions.Count -ne 1 -or $lockedVersions[0] -cne $Version) {
        $lockedVersion = if ($lockedVersions.Count -eq 1) { $lockedVersions[0] } elseif ($lockedVersions.Count -eq 0) { '<missing>' } else { '<ambiguous>' }
        Write-Host "[release] Cargo.lock records $($workspacePackage.Name) version '$lockedVersion'; expected exactly one local workspace package at '$Version' after the managed refresh" -ForegroundColor Red
        exit 32
    }
}

Write-Host '[release] running all-target managed check with --locked after validating Cargo.lock'
$checkExit = Invoke-ManagedMode -Mode 'check'
if ($checkExit -ne 0) {
    Write-Host "[release] managed check failed with exit $checkExit; the version stamp is left in the tree for inspection" -ForegroundColor Red
    exit 32
}
if (-not (Test-Path -LiteralPath $cargoLock -PathType Leaf)) {
    Write-Host '[release] managed check succeeded but rust/Cargo.lock is missing; refusing to commit an unverifiable release' -ForegroundColor Red
    exit 32
}
$lockAfterCheck = Get-LockText $cargoLock
if ($lockAfterCheck -cne $lockAfterRefresh) {
    Write-Host '[release] managed locked check changed Cargo.lock after the validated offline refresh; refusing to commit an unverifiable lockfile' -ForegroundColor Red
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
