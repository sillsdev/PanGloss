<#
  .DESCRIPTION
  Covers: Get-TargetOwnershipPath / Write-TargetOwnership (rust/tools/_common.ps1), including the
  different-repository refusal and the monotonic `preserved` flag. Operates entirely on a plain
  temp directory standing in for a target dir -- never a real cache root.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

$target = New-TestTempDir -Prefix 'pg-target-ownership'

Test-Case 'first write creates a marker naming this repository' {
    $r = Write-TargetOwnership -TargetDir $target -RepositoryId 'repoA' -WorktreePath 'C:\wtA'
    Assert-True $r.Ok $r.Detail
    Assert-True (Test-Path (Get-TargetOwnershipPath -TargetDir $target))
    $marker = Get-Content $r.Path -Raw | ConvertFrom-Json
    Assert-Equal 'repoA' $marker.repository_id
    Assert-Equal 'C:\wtA' $marker.worktree_path
    Assert-False $marker.preserved 'a plain write must not default to preserved'
}

$createdFirst = (Get-Content (Get-TargetOwnershipPath -TargetDir $target) -Raw | ConvertFrom-Json).created_utc

Test-Case 'a second write from the SAME repository succeeds and keeps created_utc' {
    Start-Sleep -Milliseconds 50
    $r = Write-TargetOwnership -TargetDir $target -RepositoryId 'repoA' -WorktreePath 'C:\wtA'
    Assert-True $r.Ok
    $marker = Get-Content $r.Path -Raw | ConvertFrom-Json
    Assert-Equal $createdFirst $marker.created_utc
}

Test-Case 'a write from a DIFFERENT repository is refused and does not overwrite the marker' {
    $before = Get-Content (Get-TargetOwnershipPath -TargetDir $target) -Raw | ConvertFrom-Json
    $r = Write-TargetOwnership -TargetDir $target -RepositoryId 'repoB' -WorktreePath 'C:\wtB'
    Assert-False $r.Ok 'a different repository_id must be refused, not silently adopted'
    $after = Get-Content (Get-TargetOwnershipPath -TargetDir $target) -Raw | ConvertFrom-Json
    Assert-Equal $before.repository_id $after.repository_id
    Assert-Equal 'repoA' $after.repository_id
}

Test-Case '-Preserved marks the target preserved' {
    $r = Write-TargetOwnership -TargetDir $target -RepositoryId 'repoA' -WorktreePath 'C:\wtA' -Preserved
    Assert-True $r.Ok
    $marker = Get-Content $r.Path -Raw | ConvertFrom-Json
    Assert-True $marker.preserved
}

Test-Case 'preserved is monotonic: a later plain write does not clear it' {
    $r = Write-TargetOwnership -TargetDir $target -RepositoryId 'repoA' -WorktreePath 'C:\wtA'
    Assert-True $r.Ok
    $marker = Get-Content $r.Path -Raw | ConvertFrom-Json
    Assert-True $marker.preserved 'an ordinary build/test write must not silently un-preserve a target'
}

Test-Case 'a fresh target dir with no marker starts unowned by nobody in particular' {
    $fresh = New-TestTempDir -Prefix 'pg-target-ownership-fresh'
    Assert-False (Test-Path (Get-TargetOwnershipPath -TargetDir $fresh))
    Remove-Item -Recurse -Force $fresh -ErrorAction SilentlyContinue
}

Remove-Item -Recurse -Force $target -ErrorAction SilentlyContinue

Write-TestSummary
