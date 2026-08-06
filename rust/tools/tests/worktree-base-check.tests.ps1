<#
  .DESCRIPTION
  Covers: Test-WorktreeBase strict/development/off modes (rust/tools/_common.ps1), built against
  real temp `git init` repos so merge-base/ancestor logic is exercised for real, not mocked.
  Never touches the real checkout.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

function New-Commit {
    param([string]$Repo, [string]$File, [string]$Content, [string]$Message)
    Set-Content -Path (Join-Path $Repo $File) -Value $Content
    git -C $Repo add $File | Out-Null
    git -C $Repo -c user.email=test@example.com -c user.name=test commit -q -m $Message | Out-Null
    return (git -C $Repo rev-parse HEAD).Trim()
}

$repo = New-TestTempDir -Prefix 'pg-base-check'
git -C $repo init -q -b main | Out-Null
$commitA = New-Commit -Repo $repo -File 'a.txt' -Content 'a' -Message 'A'
Write-WorktreeMeta -RepoRoot $repo -RequestedRevision 'main' -ResolvedObjectId $commitA -Branch 'main' | Out-Null

Test-Case 'strict mode: Ok when HEAD equals the recorded base exactly' {
    $r = Test-WorktreeBase -Mode strict -RepoRoot $repo
    Assert-True $r.Checked
    Assert-True $r.Ok $r.Detail
}

$commitB = New-Commit -Repo $repo -File 'b.txt' -Content 'b' -Message 'B'

Test-Case 'strict mode: rejects a descendant HEAD, reporting expected+actual' {
    $r = Test-WorktreeBase -Mode strict -RepoRoot $repo
    Assert-True $r.Checked
    Assert-False $r.Ok 'strict mode must reject HEAD having moved past the recorded base'
    Assert-Equal $commitA $r.Expected
    Assert-Equal $commitB $r.Actual
}

Test-Case 'development mode: accepts a descendant HEAD (the recorded base is still an ancestor)' {
    $r = Test-WorktreeBase -Mode development -RepoRoot $repo
    Assert-True $r.Checked
    Assert-True $r.Ok $r.Detail
}

$repoUnrelated = New-TestTempDir -Prefix 'pg-base-check-unrelated'
git -C $repoUnrelated init -q -b main | Out-Null
New-Commit -Repo $repoUnrelated -File 'c.txt' -Content 'c' -Message 'C' | Out-Null
# Records a base commitA has never heard of, simulating a worktree checked out to something unrelated.
Write-WorktreeMeta -RepoRoot $repoUnrelated -RequestedRevision 'main' -ResolvedObjectId $commitA -Branch 'main' | Out-Null

Test-Case 'development mode: rejects a recorded base with no relation to current HEAD history' {
    $r = Test-WorktreeBase -Mode development -RepoRoot $repoUnrelated
    Assert-True $r.Checked
    Assert-False $r.Ok 'development mode must reject a base unrelated to HEAD (history diverged)'
    Assert-Equal $commitA $r.Expected
}

$repoFresh = New-TestTempDir -Prefix 'pg-base-check-fresh'
git -C $repoFresh init -q -b main | Out-Null
New-Commit -Repo $repoFresh -File 'x.txt' -Content 'x' -Message 'X' | Out-Null

Test-Case 'absent metadata is reported as unverified, never as a failure (strict mode)' {
    $r = Test-WorktreeBase -Mode strict -RepoRoot $repoFresh
    Assert-False $r.Checked 'a worktree with no recorded base must be Checked=$false'
    Assert-True $r.Ok 'absence of metadata must never itself fail the check'
}

Test-Case 'absent metadata is reported as unverified, never as a failure (development mode)' {
    $r = Test-WorktreeBase -Mode development -RepoRoot $repoFresh
    Assert-False $r.Checked
    Assert-True $r.Ok
}

Test-Case '-Mode off short-circuits regardless of recorded metadata' {
    $r = Test-WorktreeBase -Mode off -RepoRoot $repo
    Assert-False $r.Checked
    Assert-True $r.Ok
}

foreach ($dir in @($repo, $repoUnrelated, $repoFresh)) {
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
}

Write-TestSummary
