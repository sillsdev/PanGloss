<#
  Covers: Get-WorktreeMetaPath / Write-WorktreeMeta / Read-WorktreeMeta (rust/tools/_common.ps1),
  the worktree side of the exact-base contract (design doc part 2). Uses a temp git repo only --
  never touches the real checkout's own .pangloss-worktree.json.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

$repo = New-TestTempDir -Prefix 'pg-worktree-meta'
git -C $repo init -q -b main | Out-Null
Set-Content -Path (Join-Path $repo 'a.txt') -Value 'a'
git -C $repo add a.txt | Out-Null
git -C $repo -c user.email=test@example.com -c user.name=test commit -q -m 'initial' | Out-Null
$commit = (git -C $repo rev-parse HEAD).Trim()

Test-Case 'Get-WorktreeMetaPath points at a file directly under the worktree root' {
    $p = Get-WorktreeMetaPath -RepoRoot $repo
    Assert-Equal (Join-Path $repo '.pangloss-worktree.json') $p
}

Test-Case 'Read-WorktreeMeta returns $null when no metadata file exists yet' {
    $m = Read-WorktreeMeta -RepoRoot $repo
    Assert-True ($null -eq $m) 'expected $null for a worktree with no recorded metadata'
}

Test-Case 'Write-WorktreeMeta then Read-WorktreeMeta round-trips every field' {
    Write-WorktreeMeta -RepoRoot $repo -RequestedRevision 'main' -ResolvedObjectId $commit `
        -Branch 'main' -CorpusPolicy 'local-samples-data' -ManagedTarget 'G:\cargo-build-cache\pg-worktree-meta' | Out-Null
    $m = Read-WorktreeMeta -RepoRoot $repo
    Assert-True ($null -ne $m) 'expected metadata to be readable after Write-WorktreeMeta'
    Assert-Equal 1 $m.schema_version
    Assert-Equal (Get-RepoIdentity -RepoRoot $repo) $m.repository_id
    Assert-Equal 'main' $m.requested_revision
    Assert-Equal $commit $m.resolved_object_id
    Assert-Equal 'main' $m.branch
    Assert-Equal 'local-samples-data' $m.corpus_policy
    Assert-Equal 'G:\cargo-build-cache\pg-worktree-meta' $m.managed_target
    Assert-True (-not [string]::IsNullOrWhiteSpace($m.created_utc)) 'created_utc must be recorded'
    Assert-Equal $repo $m.worktree_path
}

Test-Case 'Get-RepoIdentity is stable across repeated calls on the same repo' {
    $a = Get-RepoIdentity -RepoRoot $repo
    $b = Get-RepoIdentity -RepoRoot $repo
    Assert-Equal $a $b
    Assert-True (-not [string]::IsNullOrWhiteSpace($a)) 'repository identity must not be empty'
}

Test-Case 'a corrupt metadata file is treated as absent, not thrown' {
    Set-Content -Path (Get-WorktreeMetaPath -RepoRoot $repo) -Value '{ not valid json'
    $m = Read-WorktreeMeta -RepoRoot $repo
    Assert-True ($null -eq $m) 'a corrupt metadata file must read back as $null, not throw'
}

Remove-Item -Recurse -Force $repo -ErrorAction SilentlyContinue

Write-TestSummary
