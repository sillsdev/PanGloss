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
    # Backdate: gc treats a just-written directory as claimed, which every fixture here would be.
    Get-ChildItem -LiteralPath $dir -Recurse -File | ForEach-Object { $_.LastWriteTime = (Get-Date).AddHours(-3) }
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

Test-Case 'a busy process that claims no path cannot speak for any directory' {
    # Its own probe, so the shared fixtures stay intact for the ordering-independent cases below.
    $probe = New-FakeTarget -Root $root -Name 'claimless-probe' -Marker @{
        schema_version = 1; repository_id = 'REPO1'; worktree_path = 'C:\wt'; created_utc = 'x'; last_used_utc = 'x'; preserved = $false
    }
    $fakeBusyProcess = [PSCustomObject]@{ ProcessId = 99999; Name = 'cargo.exe' }
    $classified = [PSCustomObject]@{ Path = $probe; Class = 'disposable'; SizeGB = 0 }
    $r = Invoke-TargetGc -Classification @($classified) -Apply:$true -BusyProcesses @($fakeBusyProcess) -Roots @($root)
    Assert-Equal 1 $r.Deleted.Count
    Assert-False (Test-Path $probe) 'a process naming no path must not protect an unrelated directory'
}

Test-Case 'the sccache daemon is not a busy process, so it can never block -Apply' {
    # It ran permanently and blocked every -Apply; 32GB of disposable dirs sat unreclaimable behind it.
    $names = @(Get-LiveBuildProcesses | ForEach-Object { $_.Name })
    Assert-False ($names -contains 'sccache.exe') 'sccache is a shared daemon, never evidence of a live build'
}

Test-Case 'a live build elsewhere does not block an unrelated disposable directory' {
    # Abstaining machine-wide reclaimed nothing here; the busy claim is per-directory now.
    $probe = Join-Path $root 'effect-probe'
    New-Item -ItemType Directory -Force -Path $probe | Out-Null
    Set-Content -Path (Join-Path $probe 'filler.bin') -Value ('x' * 4096)
    # Backdate it: recent writes are their own claim, tested separately below.
    Get-ChildItem -LiteralPath $probe -Recurse -File | ForEach-Object { $_.LastWriteTime = (Get-Date).AddHours(-3) }
    $elsewhere = [PSCustomObject]@{ Name = 'rustc.exe'; CommandLine = 'rustc.exe --out-dir C:\somewhere-else\target x.rs' }
    $classified = [PSCustomObject]@{ Path = $probe; Class = 'disposable'; SizeGB = 0 }
    $r = Invoke-TargetGc -Classification @($classified) -Apply:$true -BusyProcesses @($elsewhere) -Roots @($root)
    Assert-False $r.Skipped "a build in another target dir must not stop gc: $($r.SkipReason)"
    Assert-False (Test-Path $probe) 'the disposable probe directory must actually be gone'
}

Test-Case 'a live build naming THIS directory does block it' {
    $probe = Join-Path $root 'claimed-probe'
    New-Item -ItemType Directory -Force -Path $probe | Out-Null
    Set-Content -Path (Join-Path $probe 'filler.bin') -Value 'x'
    Get-ChildItem -LiteralPath $probe -Recurse -File | ForEach-Object { $_.LastWriteTime = (Get-Date).AddHours(-3) }
    $claimer = [PSCustomObject]@{ Name = 'cargo.exe'; CommandLine = "cargo build --target-dir $probe" }
    $classified = [PSCustomObject]@{ Path = $probe; Class = 'disposable'; SizeGB = 0 }
    $r = Invoke-TargetGc -Classification @($classified) -Apply:$true -BusyProcesses @($claimer) -Roots @($root)
    Assert-Equal 0 $r.Deleted.Count
    Assert-True (Test-Path $probe) 'a directory a live build names must survive'
    Assert-True ($r.SkipReason -match 'command line') "skip reason must say why: $($r.SkipReason)"
}

Test-Case 'a recently written directory blocks itself, since CARGO_TARGET_DIR names no path on a command line' {
    $probe = Join-Path $root 'fresh-probe'
    New-Item -ItemType Directory -Force -Path $probe | Out-Null
    Set-Content -Path (Join-Path $probe 'just-written.bin') -Value 'x'
    $classified = [PSCustomObject]@{ Path = $probe; Class = 'disposable'; SizeGB = 0 }
    $r = Invoke-TargetGc -Classification @($classified) -Apply:$true -BusyProcesses @() -Roots @($root)
    Assert-Equal 0 $r.Deleted.Count
    Assert-True (Test-Path $probe) 'a directory written to seconds ago must survive'
    Assert-True ($r.SkipReason -match 'last') "skip reason must name the recency: $($r.SkipReason)"
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
