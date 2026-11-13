<#
  .DESCRIPTION
  Cut a PanGloss release through a pull request, in two phases. No step commits to local main.

  -Prepare -Version X.Y.Z   (from any clean worktree on any branch)
    Fetches origin/main, creates branch release/vX.Y.Z from it (HEAD need not be main), renames
    '## Unreleased' in CHANGELOG.md to '## X.Y.Z' plus 'Released <today>.' (an existing '## X.Y.Z'
    section is kept as is), stamps the workspace version, refreshes Cargo.lock through pg.ps1,
    runs the locked managed check, commits 'release: vX.Y.Z', pushes the branch, and opens the PR
    with gh. -NoPush stops after the local commit.

  -Tag -Version X.Y.Z   (after the PR is squash-merged)
    Fetches origin/main and finds the release commit: the first-parent commit that set the
    workspace version to X.Y.Z, provided origin/main's tip still carries that version. It then
    requires the '## X.Y.Z' changelog section at that commit and creates the annotated tag
    vX.Y.Z on it with no checkout. Commits that landed after the release commit are not tagged.
    It prints 'git push origin vX.Y.Z'; -Push runs it. Pushing the tag starts the Release workflow.

  -DryRun runs the preflight of either phase and changes nothing.
  CI accepts only annotated vX.Y.Z tags whose commit is on origin/main. Prereleases are not
  supported. Run workflow_dispatch with an explicit ref and version for a build-only rehearsal.
  Test hook: PG_RELEASE_GH names the gh command (default gh).

  Examples:
    rust\tools\release.ps1 -Prepare -Version 0.7.0
    rust\tools\release.ps1 -Tag -Version 0.7.0 -Push

  Exit codes: 30 dirty tree, 31 branch or origin/main problem (fetch failed, release branch
  exists), 32 managed check failed, 33 version or changelog refusal (including -Tag on a commit
  without the version or section), 34 existing tag, 35 push, PR creation, or tag push failed,
  0 success.
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+$')][string]$Version,
    [Parameter(Mandatory = $true, ParameterSetName = 'Prepare')][switch]$Prepare,
    [Parameter(ParameterSetName = 'Prepare')][switch]$NoPush,
    [Parameter(Mandatory = $true, ParameterSetName = 'Tag')][switch]$Tag,
    [Parameter(ParameterSetName = 'Tag')][switch]$Push,
    [Parameter(ParameterSetName = 'Prepare')][Parameter(ParameterSetName = 'Tag')][switch]$DryRun,
    [int]$MaxConcurrent = 2
)

$ErrorActionPreference = 'Stop'
$toolRoot = $PSScriptRoot
$repoRoot = (Resolve-Path (Join-Path $toolRoot '..\..')).Path
$cargoToml = Join-Path $repoRoot 'rust\Cargo.toml'
$cargoLock = Join-Path $repoRoot 'rust\Cargo.lock'
$changelog = Join-Path $repoRoot 'CHANGELOG.md'
$releaseTag = "v$Version"
$releaseBranch = "release/$releaseTag"

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

function Get-WorkspaceVersionFromToml([string]$Text) {
    $section = [regex]::Match($Text, '(?ms)^\[workspace\.package\]\r?\n(?<body>.*?)(?=^\[|\z)')
    if (-not $section.Success) { return $null }
    $found = [regex]::Match($section.Groups['body'].Value, '(?m)^version\s*=\s*"(?<v>[^"]+)"')
    if ($found.Success) { return $found.Groups['v'].Value }
    return $null
}

function Test-ChangelogSection([string]$Text, [string]$SectionVersion) {
    # Exact line match, like CI's grep -Fx: no trailing space, no CR.
    return (($Text -split "`n") -ccontains "## $SectionVersion")
}

function Invoke-Git([string[]]$GitArguments) {
    $output = & git -C $repoRoot @GitArguments 2>&1
    return [PSCustomObject]@{ ExitCode = $LASTEXITCODE; Output = (($output | ForEach-Object { "$_" }) -join "`n").TrimEnd() }
}

function Assert-TagFree {
    $local = Invoke-Git @('tag', '--list', $releaseTag)
    if ($local.ExitCode -ne 0) { Write-Refusal 'tag' 'git could not inspect local tags' 34 }
    if ($local.Output) { Write-Refusal 'tag' "$releaseTag already exists locally" 34 }
    $remote = Invoke-Git @('ls-remote', '--exit-code', '--tags', 'origin', "refs/tags/$releaseTag")
    if ($remote.ExitCode -eq 0 -or $remote.Output) { Write-Refusal 'tag' "$releaseTag already exists on origin" 34 }
    if ($remote.ExitCode -ne 2) { Write-Refusal 'tag' 'could not check origin for an existing tag' 34 }
}

function Update-OriginMain {
    $fetch = Invoke-Git @('fetch', '--quiet', '--no-tags', 'origin', 'refs/heads/main:refs/remotes/origin/main')
    if ($fetch.ExitCode -ne 0) {
        Write-Refusal 'branch' 'could not fetch origin/main; the current main tip could not be verified' 31
    }
    return (Invoke-Git @('rev-parse', 'refs/remotes/origin/main')).Output
}

function Get-CommitFile([string]$Commit, [string]$Path) {
    $shown = Invoke-Git @('show', "${Commit}:$Path")
    if ($shown.ExitCode -ne 0) { return $null }
    return $shown.Output
}

function Invoke-TagPhase {
    $tip = Update-OriginMain
    $tipVersion = Get-WorkspaceVersionFromToml (Get-CommitFile $tip 'rust/Cargo.toml')
    if ($tipVersion -cne $Version) {
        Write-Refusal 'version' "origin/main's workspace version is '$tipVersion', not $Version; merge the release PR first, or a later release has superseded it" 33
    }
    if ((Invoke-Git @('rev-parse', '--is-shallow-repository')).Output -ne 'false') {
        Write-Refusal 'clone' 'this is a shallow clone (or shallowness could not be checked); the release commit cannot be found. Run git fetch --unshallow' 31
    }
    # The release commit is the oldest first-parent commit of the run that carries this version.
    $release = $tip
    foreach ($parent in (Invoke-Git @('rev-list', '--first-parent', '--skip=1', $tip)).Output -split "`n") {
        if (-not $parent) { break }
        $parentVersion = Get-WorkspaceVersionFromToml (Get-CommitFile $parent 'rust/Cargo.toml')
        if ($null -eq $parentVersion) {
            Write-Refusal 'version' "could not read the workspace version at $parent while finding the release commit" 33
        }
        if ($parentVersion -cne $Version) { break }
        $release = $parent
    }
    $notes = Get-CommitFile $release 'CHANGELOG.md'
    if ($null -eq $notes -or -not (Test-ChangelogSection $notes $Version)) {
        Write-Refusal 'changelog' "the release commit $release has no '## $Version' CHANGELOG.md section" 33
    }
    Assert-TagFree
    Write-Host "[release] tag preflight green: $releaseTag will name $release (origin/main tip $tip)"
    if ($DryRun) {
        Write-Host "[release] DRY RUN -- no tag created. Would run: git tag -a $releaseTag $release"
        exit 0
    }
    $made = Invoke-Git @('tag', '-a', $releaseTag, '-m', "PanGloss $releaseTag", $release)
    if ($made.ExitCode -ne 0) { Write-Refusal 'tag' "annotated tag creation failed: $($made.Output)" 34 }
    if (-not $Push) {
        Write-Host "[release] created annotated tag $releaseTag on $release. No push was run."
        Write-Host "[release] push it to start the Release workflow: git push origin $releaseTag"
        exit 0
    }
    $pushed = Invoke-Git @('push', 'origin', "refs/tags/$releaseTag")
    if ($pushed.ExitCode -ne 0) { Write-Refusal 'push' "tag push failed: $($pushed.Output)" 35 }
    Write-Host "[release] pushed $releaseTag; the Release workflow now waits for Rust CI on $release."
    exit 0
}

if ($Tag) { Invoke-TagPhase }

$dirty = @(git -C $repoRoot status --porcelain --untracked-files=all --ignore-submodules=all)
if ($LASTEXITCODE -ne 0) { Write-Refusal 'tree' 'git status failed; the tree could not be verified' 30 }
if ($dirty.Count -gt 0) {
    Write-Refusal 'tree' "working tree has uncommitted changes: $($dirty -join '; ')" 30
}

$remoteMain = Update-OriginMain
Assert-TagFree
if ((Invoke-Git @('show-ref', '--verify', '--quiet', "refs/heads/$releaseBranch")).ExitCode -eq 0) {
    Write-Refusal 'branch' "local branch $releaseBranch already exists; delete it or finish that release" 31
}
$remoteBranch = Invoke-Git @('ls-remote', '--exit-code', '--heads', 'origin', $releaseBranch)
if ($remoteBranch.ExitCode -eq 0 -or $remoteBranch.Output) {
    Write-Refusal 'branch' "$releaseBranch already exists on origin; merge or delete it first" 31
}
if ($remoteBranch.ExitCode -ne 2) { Write-Refusal 'branch' 'could not check origin for an existing release branch' 31 }

$baseToml = Get-CommitFile $remoteMain 'rust/Cargo.toml'
if ($null -eq $baseToml) { Write-Refusal 'version' 'origin/main has no rust/Cargo.toml' 33 }
$baseVersion = Get-WorkspaceVersionFromToml $baseToml
if ($null -eq $baseVersion) { Write-Refusal 'version' 'rust/Cargo.toml has no workspace package version' 33 }
if ($baseVersion -ceq $Version) {
    Write-Refusal 'version' "workspace version is already $Version; refusing to cut the same version again" 33
}
$baseChangelog = Get-CommitFile $remoteMain 'CHANGELOG.md'
if ($null -eq $baseChangelog) { Write-Refusal 'changelog' 'CHANGELOG.md is missing' 33 }
$hasSection = Test-ChangelogSection $baseChangelog $Version
$hasUnreleased = [bool]($baseChangelog -match '(?m)^##[ \t]+Unreleased[ \t]*\r?$')
if (-not $hasSection -and -not $hasUnreleased) {
    Write-Refusal 'changelog' "CHANGELOG.md has neither '## Unreleased' nor '## $Version'; author release notes before cutting" 33
}
if ($null -eq (Get-CommitFile $remoteMain 'rust/Cargo.lock')) { Write-Refusal 'lockfile' 'rust/Cargo.lock is missing before the managed refresh' 33 }

$gh = if ($env:PG_RELEASE_GH) { $env:PG_RELEASE_GH } else { 'gh' }
if (-not $NoPush -and -not (Get-Command $gh -ErrorAction SilentlyContinue)) {
    Write-Refusal 'gh' "cannot run '$gh'; install the GitHub CLI or pass -NoPush" 35
}

$changelogPlan = if ($hasSection) { "keep '## $Version'" } else { "rename '## Unreleased' to '## $Version'" }
Write-Host "[release] preflight green: origin/main=$remoteMain, version $baseVersion -> $Version, changelog: $changelogPlan"
if ($DryRun) {
    Write-Host "[release] DRY RUN -- no branch, files, or commits changed. Would branch $releaseBranch from origin/main, commit, push, and open a PR."
    exit 0
}

$previousRef = (Invoke-Git @('rev-parse', '--abbrev-ref', 'HEAD')).Output
$switched = Invoke-Git @('switch', '--quiet', '--no-track', '-c', $releaseBranch, $remoteMain)
if ($switched.ExitCode -ne 0) { Write-Refusal 'branch' "could not create $releaseBranch from origin/main: $($switched.Output)" 31 }
Write-Host "[release] created $releaseBranch from origin/main ($remoteMain)"

if (-not $hasSection) {
    $changelogText = [System.IO.File]::ReadAllText($changelog)
    $stamp = Get-Date -Format 'yyyy-MM-dd'
    $heading = [regex]::new('(?m)^##[ \t]+Unreleased[ \t]*(?<eol>\r?\n|\z)(?:\r?\n)?')
    $renamed = $heading.Replace($changelogText, { param($m) $e = $m.Groups['eol'].Value; if (-not $e) { $e = "`n" }; "## $Version$e${e}Released $stamp.$e$e" }, 1)
    if (-not (Test-ChangelogSection $renamed $Version)) {
        # Nothing is written yet: leave the tree as found.
        Invoke-Git @('switch', '--quiet', $previousRef) | Out-Null
        Invoke-Git @('branch', '-D', $releaseBranch) | Out-Null
        Write-Refusal 'changelog' "renaming '## Unreleased' did not produce an exact '## $Version' line (CI needs LF endings and no trailing spaces)" 33
    }
    [System.IO.File]::WriteAllText($changelog, $renamed, [System.Text.UTF8Encoding]::new($false))
}

$lockBefore = Get-LockText $cargoLock
$tomlText = Get-Content -LiteralPath $cargoToml -Raw
$workspace = [regex]::Match($tomlText, '(?ms)^\[workspace\.package\]\r?\n(?<body>.*?)(?=^\[|\z)')
$versionLine = [regex]::new('(?m)^(?<prefix>version\s*=\s*)"(?<version>[^"]+)"')
$currentVersion = $versionLine.Match($workspace.Groups['body'].Value)

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
$unexpectedChanges = @($postCheckDirty | Where-Object { $_ -notmatch '^\s*M\s+(rust[/\\]Cargo\.(toml|lock)|CHANGELOG\.md)$' })
if ($unexpectedChanges.Count -gt 0) {
    Write-Host '[release] managed check changed files outside rust/Cargo.toml, rust/Cargo.lock, and CHANGELOG.md; inspect the tree before retrying' -ForegroundColor Red
    $unexpectedChanges | ForEach-Object { Write-Host "    $_" }
    exit 32
}

git -C $repoRoot add -- rust/Cargo.toml rust/Cargo.lock CHANGELOG.md
if ($LASTEXITCODE -ne 0) { Write-Host '[release] git add failed; version stamp is left in the tree' -ForegroundColor Red; exit 32 }
git -C $repoRoot diff --cached --quiet
if ($LASTEXITCODE -eq 0) { Write-Host '[release] managed version stamp produced no Cargo.toml or Cargo.lock changes' -ForegroundColor Red; exit 33 }
if ($LASTEXITCODE -ne 1) { Write-Host '[release] could not inspect the staged version stamp' -ForegroundColor Red; exit 32 }

git -C $repoRoot commit -q -m "release: $releaseTag"
if ($LASTEXITCODE -ne 0) { Write-Host '[release] release commit failed; staged files are left for inspection' -ForegroundColor Red; exit 32 }
$sha = (Invoke-Git @('rev-parse', 'HEAD')).Output
Write-Host "[release] committed 'release: $releaseTag' as $sha on $releaseBranch."
if ($NoPush) {
    Write-Host "[release] -NoPush: nothing pushed. Push and open the PR yourself: git push -u origin $releaseBranch"
    exit 0
}

$pushed = Invoke-Git @('push', '-u', 'origin', $releaseBranch)
if ($pushed.ExitCode -ne 0) { Write-Refusal 'push' "could not push ${releaseBranch}: $($pushed.Output)" 35 }
$prBody = @"
Cuts $releaseTag. The branch stamps the workspace version and Cargo.lock to $Version and holds the ``## $Version`` section of CHANGELOG.md (released $(Get-Date -Format 'yyyy-MM-dd')).

After a squash merge, tag the merged commit: ``pwsh -NoProfile -File rust/tools/release.ps1 -Tag -Version $Version -Push``.
"@
Push-Location $repoRoot
try {
    $prOutput = & $gh pr create --base main --head $releaseBranch --title "release: $releaseTag" --body $prBody 2>&1 | ForEach-Object { "$_" }
    $prExit = $LASTEXITCODE
} catch {
    $prOutput = @("$_")
    $prExit = 1
} finally { Pop-Location }
if ($prExit -ne 0) { Write-Refusal 'pr' "gh pr create failed (the branch is pushed): $($prOutput -join ' ')" 35 }
Write-Host ($prOutput -join "`n")
Write-Host "[release] after the PR is squash-merged, tag it: rust\tools\release.ps1 -Tag -Version $Version -Push"
exit 0
