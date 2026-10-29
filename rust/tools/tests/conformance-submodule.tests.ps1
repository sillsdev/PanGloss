<#
  .DESCRIPTION
  Covers: Test-ConformanceSubmodulePresent / Get-ConformancePinnedCommit /
  Get-ConformanceSubmoduleSizeMB / Initialize-ConformanceSubmodule (rust/tools/_common.ps1) -- the
  auto-init that replaces "someone runs `git submodule update` by hand in every fresh worktree".

  Why this file exists: `pg.ps1 -Mode new-worktree` never initialized the `machine` submodule, so
  every fresh worktree failed pg-parse's `w91_affix_shapes_covered_by_upstream_fixtures` with
  "machine submodule initialized?" until a human ran the update manually -- real infrastructure
  breakage that reads exactly like a regression in whatever change the worktree was created for.

  NEVER touches network. The success/sparse-checkout path against the REAL github.com/sillsdev/
  machine is exercised by the acceptance proof (a throwaway `pg.ps1 -Mode new-worktree`), not here --
  these tests only cover the parts that must hold with zero network reachability: the fast idempotent
  path, pin resolution against a synthetic gitlink, and the actionable-failure shape when
  initialization is required and cannot succeed (simulated with a `.gitmodules` URL that is a
  nonexistent LOCAL path, so `git clone` fails immediately and deterministically regardless of
  whether the machine running this test happens to have internet access).
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

function New-FakeGitRepoWithMachineGitlink {
    # A synthetic gitlink (mode 160000) at `machine`; $Sha need not resolve, since ls-tree never dereferences it.
    param(
        [string]$Sha = '73599a89d5596bdc53c8fc6521962721bcc36bfa',
        [string]$Url = '',
        [string]$Branch = 'conformance-framework'
    )
    $repo = New-TestTempDir -Prefix 'pg-conformance-repo'
    git -C $repo init -q -b main | Out-Null
    if ($Url) {
        Set-Content -Path (Join-Path $repo '.gitmodules') -Value @"
[submodule "machine"]
	path = machine
	url = $Url
	branch = $Branch
"@
        git -C $repo add .gitmodules | Out-Null
    }
    git -C $repo update-index --add --cacheinfo "160000,$Sha,machine" | Out-Null
    git -C $repo -c user.email=test@example.com -c user.name=test commit -q -m 'add machine gitlink' | Out-Null
    return $repo
}

# --- Fast path: the sentinel file alone decides "already initialized", at no git cost. ---

Test-Case 'Test-ConformanceSubmodulePresent is false when the sentinel is absent' {
    $repo = New-TestTempDir -Prefix 'pg-conformance-empty'
    Assert-False (Test-ConformanceSubmodulePresent -RepoRoot $repo) 'a repo with no machine/ dir at all must read as not-present'
    Remove-Item -Recurse -Force $repo -ErrorAction SilentlyContinue
}

Test-Case 'Test-ConformanceSubmodulePresent is true once the sentinel file exists' {
    $repo = New-TestTempDir -Prefix 'pg-conformance-present'
    New-Item -ItemType Directory -Force -Path (Join-Path $repo 'machine\conformance') | Out-Null
    Set-Content -Path (Join-Path $repo 'machine\conformance\constructs.txt') -Value 'placeholder'
    Assert-True (Test-ConformanceSubmodulePresent -RepoRoot $repo)
    Remove-Item -Recurse -Force $repo -ErrorAction SilentlyContinue
}

Test-Case 'Initialize-ConformanceSubmodule takes the fast path (Ok, AlreadyPresent) without invoking git' {
    $repo = New-TestTempDir -Prefix 'pg-conformance-fastpath'
    New-Item -ItemType Directory -Force -Path (Join-Path $repo 'machine\conformance') | Out-Null
    Set-Content -Path (Join-Path $repo 'machine\conformance\constructs.txt') -Value 'placeholder'

    # Shadows `git` with a counting stub: PowerShell prefers a same-scope function over the real executable.
    $script:GitCallCount = 0
    function git { $script:GitCallCount++ }

    try {
        $r = Initialize-ConformanceSubmodule -RepoRoot $repo
        Assert-True $r.Ok
        Assert-True $r.AlreadyPresent
        Assert-Equal 'already-present' $r.Mode
        Assert-Equal 0 $script:GitCallCount 'the fast path must never invoke git'
    } finally {
        # function:git, not function:global:git -- the function: PSDrive has no scope segment in its path.
        Remove-Item function:git -ErrorAction SilentlyContinue
        Remove-Item -Recurse -Force $repo -ErrorAction SilentlyContinue
    }
}

# --- Pin resolution: reads the SUPERPROJECT's own tree, not .gitmodules' branch name. ---

Test-Case 'Get-ConformancePinnedCommit reads the gitlink SHA recorded in HEAD, not a live remote' {
    $sha = '73599a89d5596bdc53c8fc6521962721bcc36bfa'
    $repo = New-FakeGitRepoWithMachineGitlink -Sha $sha
    Assert-Equal $sha (Get-ConformancePinnedCommit -RepoRoot $repo)
    Remove-Item -Recurse -Force $repo -ErrorAction SilentlyContinue
}

Test-Case 'Get-ConformancePinnedCommit returns $null (never throws) when there is no machine gitlink at all' {
    $repo = New-TestTempDir -Prefix 'pg-conformance-nogitlink'
    git -C $repo init -q -b main | Out-Null
    Set-Content -Path (Join-Path $repo 'readme.txt') -Value 'no submodule here'
    git -C $repo add readme.txt | Out-Null
    git -C $repo -c user.email=test@example.com -c user.name=test commit -q -m 'init' | Out-Null
    Assert-Equal $null (Get-ConformancePinnedCommit -RepoRoot $repo)
    Remove-Item -Recurse -Force $repo -ErrorAction SilentlyContinue
}

# --- Missing submodule is detected, and the failure path is actionable -- entirely offline. ---

Test-Case 'a missing submodule with no resolvable pin fails closed with an actionable, non-empty recovery command' {
    # No .gitmodules or gitlink at all, so this must fail before any git clone is even attempted.
    $repo = New-TestTempDir -Prefix 'pg-conformance-unresolvable'
    git -C $repo init -q -b main | Out-Null
    Set-Content -Path (Join-Path $repo 'readme.txt') -Value 'no submodule here'
    git -C $repo add readme.txt | Out-Null
    git -C $repo -c user.email=test@example.com -c user.name=test commit -q -m 'init' | Out-Null

    $r = Initialize-ConformanceSubmodule -RepoRoot $repo
    Assert-False $r.Ok
    Assert-False $r.AlreadyPresent
    Assert-Equal 'failed' $r.Mode
    Assert-True ($r.Detail.Length -gt 0) 'a failure must always carry a human-readable Detail'
    Assert-True ($r.RecoveryCommand -like '*submodule update --init*') "RecoveryCommand must name the exact command to run by hand; got: $($r.RecoveryCommand)"
    Remove-Item -Recurse -Force $repo -ErrorAction SilentlyContinue
}

Test-Case 'a submodule whose init cannot reach its remote fails closed with the distinct exit code and an actionable message' {
    # A nonexistent LOCAL path, not a real host: `git clone` fails immediately, deterministically, and offline.
    $badUrl = (Join-Path ([System.IO.Path]::GetTempPath()) "pg-does-not-exist-$([guid]::NewGuid().ToString('N'))\repo.git") -replace '\\', '/'
    $repo = New-FakeGitRepoWithMachineGitlink -Url $badUrl

    $r = Initialize-ConformanceSubmodule -RepoRoot $repo
    Assert-False $r.Ok 'an unreachable remote must never be reported as success'
    Assert-False $r.AlreadyPresent
    Assert-Equal 'failed' $r.Mode
    Assert-True ($r.Detail -like '*git clone*') "Detail must explain what failed; got: $($r.Detail)"
    Assert-True ($r.RecoveryCommand -like '*submodule update --init*') "RecoveryCommand must name the exact command to run by hand; got: $($r.RecoveryCommand)"
    # Ok=$false plus a populated RecoveryCommand: "I could not look" must never read as "everything is fine".
    Assert-True ($r.RecoveryCommand.Length -gt 0)
    Assert-False (Test-ConformanceSubmodulePresent -RepoRoot $repo) 'a failed init must not leave a false-positive sentinel behind'

    Remove-Item -Recurse -Force $repo -ErrorAction SilentlyContinue
}

# --- Exit code: distinct from every other preflight failure this repo already defines. ---

Test-Case 'the conformance-submodule exit code is distinct from every other preflight exit code' {
    Assert-Equal 18 $script:ExitCodeConformanceSubmoduleMissing
    $others = @(
        $script:ExitCodeWrongBase, $script:ExitCodeMissingCorpus, $script:ExitCodeLowDisk,
        $script:ExitCodeCacheUnavailable, $script:ExitCodeBadTargetOwnership,
        $script:ExitCodeBuildSlotTimeout, $script:ExitCodeZeroCorpusCases, $script:ExitCodeLowMemory
    )
    foreach ($o in $others) {
        Assert-True ($script:ExitCodeConformanceSubmoduleMissing -ne $o) `
            "conformance-submodule exit code must not collide with existing code $o"
    }
}

# --- Size reporting: a pure helper, no filesystem surprises. ---

Test-Case 'Get-ConformanceSubmoduleSizeMB is 0 for a repo with no machine/ directory at all' {
    $repo = New-TestTempDir -Prefix 'pg-conformance-size'
    Assert-Equal 0 (Get-ConformanceSubmoduleSizeMB -RepoRoot $repo)
    Remove-Item -Recurse -Force $repo -ErrorAction SilentlyContinue
}

# --- Drift: a moved pin is only acted on with an explicit opt-in, and a pin already in the local clone never touches the network. ---

function New-DriftedSubmoduleRepo {
    # A superproject whose machine/ is a clone checked out at the OLD pin while HEAD pins the NEW one. $PinInLocalClone=$false makes the new pin exist only in the source repo, then breaks that source.
    param([bool]$PinInLocalClone = $true)
    $git = @('-c', 'user.email=test@example.com', '-c', 'user.name=test')
    $source = New-TestTempDir -Prefix 'pg-conformance-drift-src'
    git -C $source init -q -b main | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $source 'conformance') | Out-Null
    Set-Content -Path (Join-Path $source 'conformance\constructs.txt') -Value 'one'
    git -C $source add -A | Out-Null
    git -C $source @git commit -q -m one | Out-Null
    $first = (git -C $source rev-parse HEAD).Trim()
    $repo = New-FakeGitRepoWithMachineGitlink -Sha $first -Url ($source -replace '\\', '/')
    git clone -q $source (Join-Path $repo 'machine') | Out-Null
    Set-Content -Path (Join-Path $source 'conformance\constructs.txt') -Value 'two'
    git -C $source @git commit -q -am two | Out-Null
    $second = (git -C $source rev-parse HEAD).Trim()
    if ($PinInLocalClone) { git -C (Join-Path $repo 'machine') fetch -q origin | Out-Null }
    git -C $repo update-index --cacheinfo "160000,$second,machine" | Out-Null
    git -C $repo @git commit -q -m 'move pin' | Out-Null
    return [PSCustomObject]@{ Repo = $repo; Source = $source; First = $first; Second = $second }
}

Test-Case 'a drifted submodule is reported by Get-ConformanceSubmoduleDrift and absent when the pin matches' {
    $d = New-DriftedSubmoduleRepo
    Assert-Equal $d.First (Get-ConformanceSubmoduleDrift -RepoRoot $d.Repo).CheckedOut
    git -C (Join-Path $d.Repo 'machine') checkout -q $d.Second | Out-Null
    Assert-Equal $null (Get-ConformanceSubmoduleDrift -RepoRoot $d.Repo)
    Remove-Item -Recurse -Force $d.Repo, $d.Source -ErrorAction SilentlyContinue
}

Test-Case 'without the opt-in a drifted checkout is warned about and left exactly where it is' {
    $d = New-DriftedSubmoduleRepo
    $saved = $env:PANGLOSS_RESYNC_MACHINE; $env:PANGLOSS_RESYNC_MACHINE = $null
    try {
        $r = Initialize-ConformanceSubmodule -RepoRoot $d.Repo
        Assert-True $r.Ok
        Assert-Equal 'drift-kept' $r.Mode
        Assert-True ($r.Detail -like '*PANGLOSS_RESYNC_MACHINE=1*') "the message must name the opt-in; got: $($r.Detail)"
        Assert-Equal $d.First (git -C (Join-Path $d.Repo 'machine') rev-parse HEAD).Trim() 'the checkout must not move'
    } finally { $env:PANGLOSS_RESYNC_MACHINE = $saved; Remove-Item -Recurse -Force $d.Repo, $d.Source -ErrorAction SilentlyContinue }
}

Test-Case 'with the opt-in and the pin already local, the checkout moves without touching the network' {
    $d = New-DriftedSubmoduleRepo
    # Break the remote: if the code fetched, it would fail and keep the old commit.
    Remove-Item -Recurse -Force $d.Source
    $saved = $env:PANGLOSS_RESYNC_MACHINE; $env:PANGLOSS_RESYNC_MACHINE = '1'
    try {
        $r = Initialize-ConformanceSubmodule -RepoRoot $d.Repo
        Assert-True $r.Ok $r.Detail
        Assert-Equal 'resynced' $r.Mode
        Assert-Equal $d.Second (git -C (Join-Path $d.Repo 'machine') rev-parse HEAD).Trim()
    } finally { $env:PANGLOSS_RESYNC_MACHINE = $saved; Remove-Item -Recurse -Force $d.Repo -ErrorAction SilentlyContinue }
}

Test-Case 'with the opt-in but the pin absent and the remote unreachable, it keeps the checkout and says so clearly' {
    $d = New-DriftedSubmoduleRepo -PinInLocalClone $false
    Remove-Item -Recurse -Force $d.Source
    $saved = $env:PANGLOSS_RESYNC_MACHINE; $env:PANGLOSS_RESYNC_MACHINE = '1'
    try {
        $r = Initialize-ConformanceSubmodule -RepoRoot $d.Repo
        Assert-True $r.Ok 'an unreachable remote must not turn a usable checkout into a failure'
        Assert-Equal 'drift-offline' $r.Mode
        Assert-True ($r.Detail -like '*fetching it failed*') "got: $($r.Detail)"
        Assert-True ($r.RecoveryCommand.Length -gt 0)
        Assert-Equal $d.First (git -C (Join-Path $d.Repo 'machine') rev-parse HEAD).Trim()
    } finally { $env:PANGLOSS_RESYNC_MACHINE = $saved; Remove-Item -Recurse -Force $d.Repo -ErrorAction SilentlyContinue }
}

Write-TestSummary
