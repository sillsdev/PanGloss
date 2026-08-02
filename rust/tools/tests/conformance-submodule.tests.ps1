<#
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
    # A real, local, offline `git init` repo (same technique worktree-base-check.tests.ps1 uses)
    # carrying a synthetic gitlink (mode 160000) at path `machine`, pointed at $Sha -- exactly the
    # tree shape `git ls-tree HEAD -- machine` sees in the real repo, without cloning or fetching
    # anything. $Sha need not be a real, resolvable commit: ls-tree only reads what the SUPERPROJECT's
    # own tree recorded, never dereferences into the submodule's own object database.
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

# ---------------------------------------------------------------------------------------------
# Fast path: the sentinel file alone decides "already initialized", at no git cost.
# ---------------------------------------------------------------------------------------------

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

    # Shadow `git` with a counting stub in THIS scope. Because _common.ps1 was dot-sourced into the
    # same scope, Initialize-ConformanceSubmodule resolves the bare word `git` dynamically at call
    # time and will hit this function instead of git.exe if it (wrongly) tries to invoke git at all
    # on the fast path -- PowerShell's command resolution always prefers a function over an external
    # executable of the same name. This is what actually proves "no git invoked", rather than just
    # inferring it from the result being correct (which the fast path AND a lucky failure could both
    # produce).
    $script:GitCallCount = 0
    function git { $script:GitCallCount++ }

    try {
        $r = Initialize-ConformanceSubmodule -RepoRoot $repo
        Assert-True $r.Ok
        Assert-True $r.AlreadyPresent
        Assert-Equal 'already-present' $r.Mode
        Assert-Equal 0 $script:GitCallCount 'the fast path must never invoke git'
    } finally {
        # function:git (NOT function:global:git -- the function: PSDrive has no scope segment in
        # its path; that variant silently fails to match anything with -ErrorAction
        # SilentlyContinue swallowing it, which leaked this stub into every later test in this file
        # and made them fail in a very confusing way: real `git` calls silently became no-ops.
        Remove-Item function:git -ErrorAction SilentlyContinue
        Remove-Item -Recurse -Force $repo -ErrorAction SilentlyContinue
    }
}

# ---------------------------------------------------------------------------------------------
# Pin resolution: reads the SUPERPROJECT's own tree, not .gitmodules' branch name.
# ---------------------------------------------------------------------------------------------

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

# ---------------------------------------------------------------------------------------------
# Missing submodule is detected, and the failure path is actionable -- entirely offline.
# ---------------------------------------------------------------------------------------------

Test-Case 'a missing submodule with no resolvable pin fails closed with an actionable, non-empty recovery command' {
    # No .gitmodules AND no gitlink at all: Get-ConformancePinnedCommit can't resolve anything, so
    # this must fail BEFORE any git clone is even attempted -- the cheapest possible failure.
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
    # The URL is a nonexistent LOCAL path, not a real host -- `git clone` fails immediately and
    # deterministically (no DNS, no timeout, no dependency on this machine's actual connectivity),
    # which is exactly what "network unreachable" looks like from Initialize-ConformanceSubmodule's
    # point of view: the clone step fails, and nothing else in the sequence can proceed past it.
    $badUrl = (Join-Path ([System.IO.Path]::GetTempPath()) "pg-does-not-exist-$([guid]::NewGuid().ToString('N'))\repo.git") -replace '\\', '/'
    $repo = New-FakeGitRepoWithMachineGitlink -Url $badUrl

    $r = Initialize-ConformanceSubmodule -RepoRoot $repo
    Assert-False $r.Ok 'an unreachable remote must never be reported as success'
    Assert-False $r.AlreadyPresent
    Assert-Equal 'failed' $r.Mode
    Assert-True ($r.Detail -like '*git clone*') "Detail must explain what failed; got: $($r.Detail)"
    Assert-True ($r.RecoveryCommand -like '*submodule update --init*') "RecoveryCommand must name the exact command to run by hand; got: $($r.RecoveryCommand)"
    # "I could not look" must never read as "everything is fine" -- Ok=$false plus a populated
    # Detail/RecoveryCommand is what makes this legible instead of a silent false positive.
    Assert-True ($r.RecoveryCommand.Length -gt 0)
    Assert-False (Test-ConformanceSubmodulePresent -RepoRoot $repo) 'a failed init must not leave a false-positive sentinel behind'

    Remove-Item -Recurse -Force $repo -ErrorAction SilentlyContinue
}

# ---------------------------------------------------------------------------------------------
# Exit code: distinct from every other preflight failure this repo already defines.
# ---------------------------------------------------------------------------------------------

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

# ---------------------------------------------------------------------------------------------
# Size reporting: a pure helper, no filesystem surprises.
# ---------------------------------------------------------------------------------------------

Test-Case 'Get-ConformanceSubmoduleSizeMB is 0 for a repo with no machine/ directory at all' {
    $repo = New-TestTempDir -Prefix 'pg-conformance-size'
    Assert-Equal 0 (Get-ConformanceSubmoduleSizeMB -RepoRoot $repo)
    Remove-Item -Recurse -Force $repo -ErrorAction SilentlyContinue
}

Write-TestSummary
