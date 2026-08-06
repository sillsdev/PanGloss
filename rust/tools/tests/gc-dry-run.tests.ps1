<#
  .DESCRIPTION
  Covers: Get-TargetClassification / Invoke-TargetGc (rust/tools/_common.ps1) -- gc's
  classification and the only function allowed to delete a managed target directory. Everything
  runs against a temp directory standing in for a cache root, with -Roots/-LiveSlugs passed
  explicitly so this NEVER reads C:\cargo-targets, G:\cargo-build-cache, or the real
  `git worktree list` -- and never requires those drives to exist.

  The central property under test: dry-run gc (the default -- -Apply not passed) never deletes
  anything, in ANY classification, and -Apply only ever removes the 'disposable' class.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

$root = New-TestTempDir -Prefix 'pg-gc-root'

function New-FakeTarget {
    param([string]$Root, [string]$Name, $Marker)
    $dir = Join-Path $Root $Name
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    Set-Content -Path (Join-Path $dir 'dummy.bin') -Value 'x'
    if ($null -ne $Marker) {
        ($Marker | ConvertTo-Json -Depth 4) | Set-Content -Path (Join-Path $dir '.pangloss-owner.json')
    }
    return $dir
}

$dirUnknown = New-FakeTarget -Root $root -Name 'dir-unknown' -Marker $null
$dirPreserved = New-FakeTarget -Root $root -Name 'dir-preserved' -Marker @{
    schema_version = 1; repository_id = 'REPO1'; worktree_path = 'C:\wt'; created_utc = 'x'; last_used_utc = 'x'; preserved = $true
}
$dirLive = New-FakeTarget -Root $root -Name 'dir-live' -Marker @{
    schema_version = 1; repository_id = 'REPO1'; worktree_path = 'C:\wt'; created_utc = 'x'; last_used_utc = 'x'; preserved = $false
}
$dirDisposable = New-FakeTarget -Root $root -Name 'dir-disposable' -Marker @{
    schema_version = 1; repository_id = 'REPO1'; worktree_path = 'C:\wt'; created_utc = 'x'; last_used_utc = 'x'; preserved = $false
}
$dirOtherRepo = New-FakeTarget -Root $root -Name 'dir-other-repo' -Marker @{
    schema_version = 1; repository_id = 'REPO2'; worktree_path = 'C:\wt2'; created_utc = 'x'; last_used_utc = 'x'; preserved = $false
}
# The shared compiler-cache directory, never a target dir; must be skipped by name despite having no marker.
$sccacheDir = Join-Path $root 'sccache'
New-Item -ItemType Directory -Force -Path $sccacheDir | Out-Null

$classification = Get-TargetClassification -RepositoryId 'REPO1' -Roots @($root) -LiveSlugs @('dir-live')
function Get-Class { param($Path) ($classification | Where-Object { $_.Path -eq $Path }).Class }

Test-Case 'an unmarked directory classifies as unknown' {
    Assert-Equal 'unknown' (Get-Class $dirUnknown)
}
Test-Case 'a marker with preserved=true classifies as preserved' {
    Assert-Equal 'preserved' (Get-Class $dirPreserved)
}
Test-Case 'a non-preserved marker whose slug is a live worktree classifies as live' {
    Assert-Equal 'live' (Get-Class $dirLive)
}
Test-Case 'a non-preserved marker whose slug is NOT a live worktree classifies as disposable' {
    Assert-Equal 'disposable' (Get-Class $dirDisposable)
}
Test-Case 'a marker naming a different repository_id classifies as other-repo' {
    Assert-Equal 'other-repo' (Get-Class $dirOtherRepo)
}
Test-Case 'the shared sccache directory is never classified at all (not a target dir)' {
    Assert-True ($null -eq (Get-Class $sccacheDir))
}
Test-Case 'classification itself never deletes anything' {
    foreach ($d in @($dirUnknown, $dirPreserved, $dirLive, $dirDisposable, $dirOtherRepo, $sccacheDir)) {
        Assert-True (Test-Path $d) "classification must not have deleted $d"
    }
}

Test-Case 'dry run (-Apply not passed) deletes nothing, regardless of class' {
    $r = Invoke-TargetGc -Classification $classification -Apply:$false -Roots @($root)
    Assert-True $r.Skipped
    Assert-Equal 0 $r.Deleted.Count
    foreach ($d in @($dirUnknown, $dirPreserved, $dirLive, $dirDisposable, $dirOtherRepo)) {
        Assert-True (Test-Path $d) "dry run must not have deleted $d"
    }
}

Test-Case '-Apply with a live build process present still deletes nothing' {
    $fakeBusyProcess = [PSCustomObject]@{ ProcessId = 99999; Name = 'cargo.exe' }
    $r = Invoke-TargetGc -Classification $classification -Apply:$true -BusyProcesses @($fakeBusyProcess) -Roots @($root)
    Assert-True $r.Skipped
    Assert-Equal 0 $r.Deleted.Count
    foreach ($d in @($dirUnknown, $dirPreserved, $dirLive, $dirDisposable, $dirOtherRepo)) {
        Assert-True (Test-Path $d) "must not delete $d while a build process is reported busy"
    }
}

Test-Case '-Apply with no busy processes deletes ONLY the disposable directory' {
    $r = Invoke-TargetGc -Classification $classification -Apply:$true -BusyProcesses @() -Roots @($root)
    Assert-False $r.Skipped
    Assert-Equal 1 $r.Deleted.Count
    Assert-Contains $r.Deleted $dirDisposable
    Assert-False (Test-Path $dirDisposable) 'the disposable directory must actually be removed'
    Assert-True (Test-Path $dirUnknown) 'unknown must survive -Apply'
    Assert-True (Test-Path $dirPreserved) 'preserved must survive -Apply'
    Assert-True (Test-Path $dirLive) 'live must survive -Apply'
    Assert-True (Test-Path $dirOtherRepo) 'other-repo must survive -Apply'
}

Test-Case 'a disposable path outside every configured root is refused, not deleted' {
    # The deletion-time containment re-check, guarding a future caller that hand-builds a classification list.
    $outside = Join-Path ([System.IO.Path]::GetTempPath()) "pg-gc-outside-$PID"
    New-Item -ItemType Directory -Force -Path $outside | Out-Null
    $forged = @([PSCustomObject]@{ Path = $outside; Class = 'disposable'; SizeGB = 0; Detail = 'forged' })
    $threw = $false
    try { Invoke-TargetGc -Classification $forged -Apply:$true -BusyProcesses @() -Roots @($root) } catch { $threw = $true }
    Assert-True $threw 'deleting a path outside every configured root must throw'
    Assert-True (Test-Path $outside) 'the out-of-root directory must still exist'
    Remove-Item -Recurse -Force $outside -ErrorAction SilentlyContinue
}

Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue

Write-TestSummary
