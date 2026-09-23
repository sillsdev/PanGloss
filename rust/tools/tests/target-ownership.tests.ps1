<#
  .DESCRIPTION
  Covers: Get-TargetOwnershipPath / Write-TargetOwnership / Export-ReleaseArtifact
  (rust/tools/_common.ps1), including the different-repository refusal and the removal of the
  `preserved` flag the export replaced. Operates entirely on a plain temp directory standing in for
  a target dir -- never a real cache root.
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
    Assert-Equal 2 $marker.schema_version
    Assert-True ($null -eq $marker.preserved) 'schema 2 carries no preserved flag at all -- the deliverable is exported, not protected in place'
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

Test-Case 'a schema-1 marker left over from the flag era still classifies, its stale preserved:true ignored' {
    # Its own root, never the shared temp dir: -Roots is enumerated and sized recursively.
    $root = New-TestTempDir -Prefix 'pg-target-legacy-root'
    $legacy = Join-Path $root 'dead-worktree'
    New-Item -ItemType Directory -Force -Path $legacy | Out-Null
    @{ schema_version = 1; repository_id = 'repoA'; worktree_path = 'C:\gone'; created_utc = 'x'; last_used_utc = 'x'; preserved = $true } |
        ConvertTo-Json | Set-Content -Path (Get-TargetOwnershipPath -TargetDir $legacy) -Encoding utf8
    $class = @(Get-TargetClassification -RepositoryId 'repoA' -Roots @($root) -LiveSlugs @() |
        Where-Object { $_.Path -eq $legacy })
    Assert-Equal 1 $class.Count 'the legacy dir must still be classified, not skipped'
    Assert-Equal 'disposable' $class[0].Class 'the old flag must no longer shield a dir whose worktree is gone -- it shielded ~110 GB of empty caches'
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}

Test-Case 'a fresh target dir with no marker starts unowned by nobody in particular' {
    $fresh = New-TestTempDir -Prefix 'pg-target-ownership-fresh'
    Assert-False (Test-Path (Get-TargetOwnershipPath -TargetDir $fresh))
    Remove-Item -Recurse -Force $fresh -ErrorAction SilentlyContinue
}

# --- Export-ReleaseArtifact: what replaced the flag. The deliverable leaves the cache. ---

Test-Case 'a release binary is copied to dist/v<version>/ with a .sha256 matching the copy''s own bytes' {
    $tgt = New-TestTempDir -Prefix 'pg-export-target'
    $repo = New-TestTempDir -Prefix 'pg-export-repo'
    New-Item -ItemType Directory -Force -Path (Join-Path $tgt 'release') | Out-Null
    $bin = Join-Path $tgt 'release\pangloss.exe'
    Set-Content -Path $bin -Value 'MZ fake binary bytes' -Encoding ascii

    $r = Export-ReleaseArtifact -TargetDir $tgt -RepoRoot $repo -Version '9.9.9'
    Assert-True $r.Ok $r.Detail
    $dest = Join-Path $repo 'dist\v9.9.9\pangloss.exe'
    Assert-True (Test-Path $dest) 'the binary must exist OUTSIDE the target dir once exported'
    Assert-Equal (Get-FileHash -LiteralPath $bin -Algorithm SHA256).Hash (Get-FileHash -LiteralPath $dest -Algorithm SHA256).Hash
    $sidecar = Get-Content "$dest.sha256" -Raw
    Assert-True ($sidecar -match (Get-FileHash -LiteralPath $dest -Algorithm SHA256).Hash) 'the .sha256 must record the hash of the copy that shipped'

    # The point of the whole change: deleting the cache must not delete the deliverable.
    Remove-Item -Recurse -Force $tgt -ErrorAction SilentlyContinue
    Assert-True (Test-Path $dest) 'the export must survive its target dir being reclaimed -- this is exactly what the preserved flag failed to guarantee for v0.3.0'
    Remove-Item -Recurse -Force $repo -ErrorAction SilentlyContinue
}

Test-Case 'a release profile that produced NO binary refuses instead of reporting a successful export' {
    $tgt = New-TestTempDir -Prefix 'pg-export-empty'
    $repo = New-TestTempDir -Prefix 'pg-export-empty-repo'
    New-Item -ItemType Directory -Force -Path (Join-Path $tgt 'release') | Out-Null
    $r = Export-ReleaseArtifact -TargetDir $tgt -RepoRoot $repo -Version '9.9.9'
    Assert-False $r.Ok 'an export that copied nothing must never read as success'
    Assert-Equal 0 @($r.Exported).Count
    Assert-True ($r.Detail -match 'no release binary') 'the refusal must name what it could not find'
    Assert-False (Test-Path (Join-Path $repo 'dist')) 'a failed export must not leave an empty dist/ implying an artifact exists'
    Remove-Item -Recurse -Force $tgt, $repo -ErrorAction SilentlyContinue
}

Test-Case 'Get-WorkspaceVersion reads the stamped [workspace.package] version, and reports absence rather than guessing' {
    $rustRoot = New-TestTempDir -Prefix 'pg-export-version'
    Set-Content -Path (Join-Path $rustRoot 'Cargo.toml') -Value "[workspace.package]`nversion = `"1.2.3`"`n" -Encoding utf8
    Assert-Equal '1.2.3' (Get-WorkspaceVersion -RustRoot $rustRoot)
    $empty = New-TestTempDir -Prefix 'pg-export-noversion'
    Assert-True ($null -eq (Get-WorkspaceVersion -RustRoot $empty)) 'a missing Cargo.toml must return $null so the caller can refuse, not export to dist/v/'
    Remove-Item -Recurse -Force $rustRoot, $empty -ErrorAction SilentlyContinue
}

Remove-Item -Recurse -Force $target -ErrorAction SilentlyContinue

Write-TestSummary
