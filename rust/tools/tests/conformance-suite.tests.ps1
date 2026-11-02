<#
  .DESCRIPTION
  Covers: Get-ConformancePin / Get-ConformanceSuiteDir / Sync-ConformanceSuite (rust/tools/_common.ps1)
  -- the fetch that replaces "someone clones the suite by hand in every fresh worktree". `conformance/` is
  a filtered clone of sillsdev/fieldworks-parser-conformance at the commit `conformance.commit` names.

  Why this file exists: a fresh worktree has no `conformance/`, and the fixture gates fail minutes into a
  build with a confusing panic. The function must also move a stale suite to a changed pin, refuse to
  overwrite local edits, and touch neither the network nor git when already at the pin.

  NEVER touches the network. The "remote" is a LOCAL source repository reached by overriding
  $script:ConformanceSuiteUrl; the failure shape points it at a nonexistent local path, so git fails
  immediately and deterministically whether or not this machine has internet access. The real
  github.com clone is exercised by a throwaway `pg.ps1 -Mode new-worktree`, not here.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

$script:Git = @('-c', 'user.email=test@example.com', '-c', 'user.name=test')
$script:SavedOverride = $env:HC_CONFORMANCE_SUITE
$env:HC_CONFORMANCE_SUITE = $null
$script:RealUrl = $script:ConformanceSuiteUrl

function New-SuiteSourceRepo {
    # A stand-in for the suite: one commit holding constructs.txt; returns the repo and its first commit.
    $source = New-TestTempDir -Prefix 'pg-suite-src'
    git -C $source init -q -b main | Out-Null
    Set-Content -Path (Join-Path $source 'constructs.txt') -Value 'one'
    git -C $source add -A | Out-Null
    git -C $source @script:Git commit -q -m one | Out-Null
    return $source
}

function Add-SourceCommit {
    param([string]$Source, [string]$Text)
    Set-Content -Path (Join-Path $Source 'constructs.txt') -Value $Text
    git -C $Source add -A | Out-Null
    git -C $Source @script:Git commit -q -m $Text | Out-Null
    return (git -C $Source rev-parse HEAD).Trim()
}

function New-PinnedRepoRoot {
    param([Parameter(Mandatory)][string]$Pin)
    $root = New-TestTempDir -Prefix 'pg-suite-root'
    Set-Content -Path (Join-Path $root 'conformance.commit') -Value $Pin
    return $root
}

function Get-TreeStamp {
    # The newest write time under a directory: any git write (fetch, checkout, gc) moves it.
    param([string]$Path)
    return (Get-ChildItem -LiteralPath $Path -Recurse -Force | Measure-Object -Property LastWriteTimeUtc -Maximum).Maximum
}

# --- The pin file ---

Test-Case 'Get-ConformancePin reads one full SHA and trims it' {
    $root = New-TestTempDir -Prefix 'pg-suite-pin'
    Set-Content -Path (Join-Path $root 'conformance.commit') -Value "  7a0f7419fc706425171080ddbe3db263197b2d0d  "
    Assert-Equal '7a0f7419fc706425171080ddbe3db263197b2d0d' (Get-ConformancePin -RepoRoot $root)
}

Test-Case 'Get-ConformancePin refuses a missing file, a short SHA, two lines and a branch name' {
    $root = New-TestTempDir -Prefix 'pg-suite-pin-bad'
    Assert-True ($null -eq (Get-ConformancePin -RepoRoot $root)) 'no file'
    foreach ($bad in @('7a0f7419fc', "7a0f7419fc706425171080ddbe3db263197b2d0d`n7a0f7419fc706425171080ddbe3db263197b2d0d", 'main', '')) {
        Set-Content -Path (Join-Path $root 'conformance.commit') -Value $bad
        Assert-True ($null -eq (Get-ConformancePin -RepoRoot $root)) "[$bad] must not read as a pin"
    }
}

Test-Case 'the repository root holds a valid conformance.commit and ignores conformance/' {
    $root = Get-RepoRoot
    Assert-True ($null -ne (Get-ConformancePin -RepoRoot $root)) 'conformance.commit must hold one full SHA'
    $ignored = & git -C $root check-ignore conformance/ 2>$null
    Assert-True ($ignored -match 'conformance') '/conformance/ must be gitignored'
    Assert-Equal 18 $script:ExitCodeConformanceSuiteMissing 'the preflight exit code'
}

# --- Fetch, exact commit ---

Test-Case 'an absent suite is cloned at exactly the pinned commit, not at the remote head' {
    $source = New-SuiteSourceRepo
    $first = (git -C $source rev-parse HEAD).Trim()
    Add-SourceCommit -Source $source -Text two | Out-Null
    $root = New-PinnedRepoRoot -Pin $first
    $script:ConformanceSuiteUrl = $source
    $r = Sync-ConformanceSuite -RepoRoot $root
    Assert-True $r.Ok "clone failed: $($r.Detail)"
    Assert-Equal 'cloned' $r.Mode
    Assert-Equal $first ((git -C (Join-Path $root 'conformance') rev-parse HEAD).Trim()) 'HEAD must be the pin, though the remote main moved on'
    Assert-Equal 'one' ((Get-Content (Join-Path $root 'conformance/constructs.txt')).Trim()) 'the pinned tree is checked out'
    Assert-Equal 0 @(Get-ChildItem -Path $root -Filter '.conformance-clone-*' -Force).Count 'no temp clone left behind'
}

Test-Case 'already at the pin needs no network and writes nothing' {
    $source = New-SuiteSourceRepo
    $pin = (git -C $source rev-parse HEAD).Trim()
    $root = New-PinnedRepoRoot -Pin $pin
    $script:ConformanceSuiteUrl = $source
    Assert-True (Sync-ConformanceSuite -RepoRoot $root).Ok
    $suite = Join-Path $root 'conformance'
    $before = Get-TreeStamp -Path $suite
    $script:ConformanceSuiteUrl = Join-Path $source 'does-not-exist'
    Start-Sleep -Milliseconds 1100
    $r = Sync-ConformanceSuite -RepoRoot $root
    Assert-True $r.Ok $r.Detail
    Assert-Equal 'already-present' $r.Mode
    Assert-Equal $before (Get-TreeStamp -Path $suite) 'no file under conformance/ was written'
}

Test-Case 'a pin change moves the suite, fetching the commit when it is not local' {
    $source = New-SuiteSourceRepo
    $pin1 = (git -C $source rev-parse HEAD).Trim()
    $root = New-PinnedRepoRoot -Pin $pin1
    $script:ConformanceSuiteUrl = $source
    Assert-True (Sync-ConformanceSuite -RepoRoot $root).Ok
    $pin2 = Add-SourceCommit -Source $source -Text two
    Set-Content -Path (Join-Path $root 'conformance.commit') -Value $pin2
    $r = Sync-ConformanceSuite -RepoRoot $root
    Assert-True $r.Ok $r.Detail
    Assert-Equal 'moved' $r.Mode
    $suite = Join-Path $root 'conformance'
    Assert-Equal $pin2 ((git -C $suite rev-parse HEAD).Trim())
    Assert-Equal 'two' ((Get-Content (Join-Path $suite 'constructs.txt')).Trim())
    Set-Content -Path (Join-Path $root 'conformance.commit') -Value $pin1
    Assert-Equal 'moved' (Sync-ConformanceSuite -RepoRoot $root).Mode 'moving back to a local commit also works'
    Assert-Equal $pin1 ((git -C $suite rev-parse HEAD).Trim())
}

Test-Case 'a suite with uncommitted changes is left alone when the pin moved' {
    $source = New-SuiteSourceRepo
    $pin1 = (git -C $source rev-parse HEAD).Trim()
    $pin2 = Add-SourceCommit -Source $source -Text two
    $root = New-PinnedRepoRoot -Pin $pin1
    $script:ConformanceSuiteUrl = $source
    Assert-True (Sync-ConformanceSuite -RepoRoot $root).Ok
    $suite = Join-Path $root 'conformance'
    Set-Content -Path (Join-Path $suite 'constructs.txt') -Value 'my local edit'
    Set-Content -Path (Join-Path $root 'conformance.commit') -Value $pin2
    $r = Sync-ConformanceSuite -RepoRoot $root
    Assert-False $r.Ok 'a dirty suite at the wrong commit must stop the run'
    Assert-Equal 'dirty' $r.Mode
    Assert-True ($r.Detail -match 'uncommitted') $r.Detail
    Assert-True ($r.RecoveryCommand -match 'conformance.ps1') 'names what to run'
    Assert-Equal $pin1 ((git -C $suite rev-parse HEAD).Trim()) 'HEAD did not move'
    Assert-Equal 'my local edit' ((Get-Content (Join-Path $suite 'constructs.txt')).Trim()) 'the edit survived'
}

Test-Case 'a dirty suite already at the pin is not a problem' {
    $source = New-SuiteSourceRepo
    $pin = (git -C $source rev-parse HEAD).Trim()
    $root = New-PinnedRepoRoot -Pin $pin
    $script:ConformanceSuiteUrl = $source
    Assert-True (Sync-ConformanceSuite -RepoRoot $root).Ok
    Set-Content -Path (Join-Path $root 'conformance/constructs.txt') -Value 'edit'
    $r = Sync-ConformanceSuite -RepoRoot $root
    Assert-True $r.Ok
    Assert-Equal 'already-present' $r.Mode
}

# --- Failure shapes ---

Test-Case 'an unreachable remote fails with the command to run and leaves no half clone' {
    $source = New-SuiteSourceRepo
    $pin = (git -C $source rev-parse HEAD).Trim()
    $root = New-PinnedRepoRoot -Pin $pin
    $script:ConformanceSuiteUrl = Join-Path $source 'does-not-exist'
    $r = Sync-ConformanceSuite -RepoRoot $root
    Assert-False $r.Ok
    Assert-Equal 'failed' $r.Mode
    Assert-True ($r.RecoveryCommand -match 'rust/tools/conformance.ps1') $r.RecoveryCommand
    Assert-False (Test-Path (Join-Path $root 'conformance')) 'nothing at conformance/'
    Assert-Equal 0 @(Get-ChildItem -Path $root -Filter '.conformance-clone-*' -Force).Count 'no temp clone left behind'
}

Test-Case 'a pin the remote does not have fails and leaves nothing' {
    $source = New-SuiteSourceRepo
    $root = New-PinnedRepoRoot -Pin ('0' * 40)
    $script:ConformanceSuiteUrl = $source
    $r = Sync-ConformanceSuite -RepoRoot $root
    Assert-False $r.Ok
    Assert-False (Test-Path (Join-Path $root 'conformance')) 'nothing at conformance/'
}

Test-Case 'a missing pin file and a conformance/ that is not a clone both fail with a reason' {
    $root = New-TestTempDir -Prefix 'pg-suite-nopin'
    $r = Sync-ConformanceSuite -RepoRoot $root
    Assert-False $r.Ok
    Assert-True ($r.Detail -match 'conformance.commit') $r.Detail
    $root2 = New-PinnedRepoRoot -Pin ('1' * 40)
    New-Item -ItemType Directory -Path (Join-Path $root2 'conformance') | Out-Null
    $r2 = Sync-ConformanceSuite -RepoRoot $root2
    Assert-False $r2.Ok
    Assert-True ($r2.Detail -match 'not a git clone') $r2.Detail
}

# --- The local-copy override ---

Test-Case 'HC_CONFORMANCE_SUITE uses the directory as is and skips the pin' {
    $local = New-TestTempDir -Prefix 'pg-suite-local'
    Set-Content -Path (Join-Path $local 'constructs.txt') -Value 'local'
    $root = New-PinnedRepoRoot -Pin ('2' * 40)
    $env:HC_CONFORMANCE_SUITE = $local
    try {
        $r = Sync-ConformanceSuite -RepoRoot $root
        Assert-True $r.Ok $r.Detail
        Assert-Equal 'override' $r.Mode
        Assert-Equal $local (Get-ConformanceSuiteDir -RepoRoot $root)
        Assert-False (Test-Path (Join-Path $root 'conformance')) 'nothing was cloned'
        $env:HC_CONFORMANCE_SUITE = (Join-Path $local 'missing')
        Assert-False (Sync-ConformanceSuite -RepoRoot $root).Ok 'an override that is not a suite fails'
    } finally { $env:HC_CONFORMANCE_SUITE = $null }
}

Test-Case 'conformance.ps1 exits 0 for a good override and 18 for a bad one' {
    $local = New-TestTempDir -Prefix 'pg-suite-local2'
    Set-Content -Path (Join-Path $local 'constructs.txt') -Value 'local'
    $script = Join-Path $PSScriptRoot '..\conformance.ps1'
    $env:HC_CONFORMANCE_SUITE = $local
    try {
        & pwsh -NoProfile -File $script *> $null
        Assert-Equal 0 $LASTEXITCODE 'good override'
        $env:HC_CONFORMANCE_SUITE = (Join-Path $local 'missing')
        & pwsh -NoProfile -File $script *> $null
        Assert-Equal 18 $LASTEXITCODE 'bad override'
    } finally { $env:HC_CONFORMANCE_SUITE = $null }
}

$script:ConformanceSuiteUrl = $script:RealUrl
$env:HC_CONFORMANCE_SUITE = $script:SavedOverride
Write-TestSummary
