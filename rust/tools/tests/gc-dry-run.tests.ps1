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

$script:FakeCimProcessRows = @()
$script:LastCimFilter = ''
function Get-CimInstance {
    param([string]$ClassName, [string]$Filter)
    $script:LastCimFilter = $Filter
    $names = @([regex]::Matches($Filter, "Name='([^']+)'") | ForEach-Object { $_.Groups[1].Value })
    return @($script:FakeCimProcessRows | Where-Object { $_.Name -in $names })
}

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
# Schema 1 with the retired flag still set: it must now classify by worktree liveness like any other.
$dirLegacyFlag = New-FakeTarget -Root $root -Name 'dir-legacy-flag' -Marker @{
    schema_version = 1; repository_id = 'REPO1'; worktree_path = 'C:\wt'; created_utc = 'x'; last_used_utc = 'x'; preserved = $true
}
$dirLive = New-FakeTarget -Root $root -Name 'dir-live' -Marker @{
    schema_version = 2; repository_id = 'REPO1'; worktree_path = 'C:\wt'; created_utc = 'x'; last_used_utc = 'x'
}
$dirDisposable = New-FakeTarget -Root $root -Name 'dir-disposable' -Marker @{
    schema_version = 2; repository_id = 'REPO1'; worktree_path = 'C:\wt'; created_utc = 'x'; last_used_utc = 'x'
}
$dirOtherRepo = New-FakeTarget -Root $root -Name 'dir-other-repo' -Marker @{
    schema_version = 2; repository_id = 'REPO2'; worktree_path = 'C:\wt2'; created_utc = 'x'; last_used_utc = 'x'
}
# The shared compiler-cache directory, never a target dir; must be skipped by name despite having no marker.
$sccacheDir = Join-Path $root 'sccache'
New-Item -ItemType Directory -Force -Path $sccacheDir | Out-Null

$classification = Get-TargetClassification -RepositoryId 'REPO1' -Roots @($root) -LiveSlugs @('dir-live')
function Get-Class { param($Path) ($classification | Where-Object { $_.Path -eq $Path }).Class }

Test-Case 'an unmarked directory classifies as unknown' {
    Assert-Equal 'unknown' (Get-Class $dirUnknown)
}
Test-Case 'a stale preserved=true from a schema-1 marker no longer shields anything' {
    # It shielded 16 research caches (~110 GB) holding no deliverable, while gc deleted the one that did.
    Assert-Equal 'disposable' (Get-Class $dirLegacyFlag)
}
Test-Case 'a marker whose slug is a live worktree classifies as live' {
    Assert-Equal 'live' (Get-Class $dirLive)
}
Test-Case 'a marker whose slug is NOT a live worktree classifies as disposable' {
    Assert-Equal 'disposable' (Get-Class $dirDisposable)
}
Test-Case 'no directory can classify as preserved any more -- the class is gone, not merely unused' {
    Assert-Equal 0 @($classification | Where-Object { $_.Class -eq 'preserved' }).Count
}
Test-Case 'a marker naming a different repository_id classifies as other-repo' {
    Assert-Equal 'other-repo' (Get-Class $dirOtherRepo)
}
Test-Case 'the shared sccache directory is never classified at all (not a target dir)' {
    Assert-True ($null -eq (Get-Class $sccacheDir))
}
Test-Case 'classification itself never deletes anything' {
    foreach ($d in @($dirUnknown, $dirLegacyFlag, $dirLive, $dirDisposable, $dirOtherRepo, $sccacheDir)) {
        Assert-True (Test-Path $d) "classification must not have deleted $d"
    }
}

Test-Case 'dry run (-Apply not passed) deletes nothing, regardless of class' {
    $expectedBytes = [long]((Get-ChildItem -LiteralPath $dirLegacyFlag, $dirDisposable -Recurse -Force -File |
        Measure-Object -Property Length -Sum).Sum)
    $r = Invoke-TargetGc -Classification $classification -Apply:$false -Roots @($root)
    Assert-True $r.Skipped
    Assert-Equal 0 $r.Deleted.Count
    Assert-Equal 0 $r.BytesFreed 'dry-run must free no file bytes'
    Assert-Equal $expectedBytes $r.BytesWouldFree 'dry-run bytes must match the disposable files still present'
    foreach ($d in @($dirUnknown, $dirLegacyFlag, $dirLive, $dirDisposable, $dirOtherRepo)) {
        Assert-True (Test-Path $d) "dry run must not have deleted $d"
    }
}

Test-Case 'a busy process that claims no path cannot speak for any directory' {
    # Its own probe, so the shared fixtures stay intact for the ordering-independent cases below.
    $probe = New-FakeTarget -Root $root -Name 'claimless-probe' -Marker @{
        schema_version = 2; repository_id = 'REPO1'; worktree_path = 'C:\wt'; created_utc = 'x'; last_used_utc = 'x'
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

Test-Case 'gc queries exactly the shared compiler and linker names' {
    $script:FakeCimProcessRows = @(
        [PSCustomObject]@{ Name = 'rustc.exe' },
        [PSCustomObject]@{ Name = 'cargo.exe' },
        [PSCustomObject]@{ Name = 'link.exe' },
        [PSCustomObject]@{ Name = 'lld-link.exe' },
        [PSCustomObject]@{ Name = 'rust-lld.exe' },
        [PSCustomObject]@{ Name = 'cc1.exe' },
        [PSCustomObject]@{ Name = 'cc1plus.exe' },
        [PSCustomObject]@{ Name = 'sccache.exe' },
        [PSCustomObject]@{ Name = 'cargo-nextest.exe' },
        [PSCustomObject]@{ Name = 'pangloss.exe' }
    )
    if ($IsLinux) {
        $linuxRows = @(
            [PSCustomObject]@{ Name = 'rustc'; Argv0 = 'rustc'; Arguments = @('rustc'); CommandLine = 'rustc'; Cwd = '/tmp' },
            [PSCustomObject]@{ Name = 'cargo'; Argv0 = 'cargo'; Arguments = @('cargo'); CommandLine = 'cargo'; Cwd = '/tmp' },
            [PSCustomObject]@{ Name = 'ld.lld'; Argv0 = 'ld.lld'; Arguments = @('ld.lld'); CommandLine = 'ld.lld'; Cwd = '/tmp' },
            [PSCustomObject]@{ Name = 'mold'; Argv0 = 'mold'; Arguments = @('mold'); CommandLine = 'mold'; Cwd = '/tmp' },
            [PSCustomObject]@{ Name = 'pangloss'; Argv0 = 'pangloss'; Arguments = @('pangloss'); CommandLine = 'pangloss'; Cwd = '/tmp' },
            [PSCustomObject]@{ Name = 'sccache'; Argv0 = 'sccache'; Arguments = @('sccache'); CommandLine = 'sccache'; Cwd = '/tmp' },
            [PSCustomObject]@{ Name = 'sleep'; Argv0 = 'sleep'; Arguments = @('sleep'); CommandLine = 'sleep'; Cwd = '/tmp' }
        )
        $names = @(Get-LinuxLiveBuildProcesses -Snapshot $linuxRows | ForEach-Object { $_.Name })
        Assert-Equal 'rustc,cargo,ld.lld,mold,pangloss,sccache' ($names -join ',') 'Linux gc must select compiler, linker, Pangloss, and shared-cache activity names'
    } else {
        $expectedNames = @('rustc.exe', 'cargo.exe', 'link.exe', 'lld-link.exe', 'rust-lld.exe')
        $expectedFilter = @($expectedNames | ForEach-Object { "Name='$_'" }) -join ' or '
        try {
            $names = @(Get-LiveBuildProcesses | ForEach-Object { $_.Name })
            Assert-Equal ($expectedNames -join ',') ($names -join ',')
            Assert-Equal $expectedFilter $script:LastCimFilter 'the WQL query must be derived from the exact gc process set'
        } finally {
            $script:FakeCimProcessRows = @()
        }
    }
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
    $claimedPath = if ($IsLinux) { Join-Path $probe 'debug/deps' } else { $probe }
    $claimer = [PSCustomObject]@{
        Name = if ($IsLinux) { 'cargo' } else { 'cargo.exe' }
        CommandLine = "cargo build --target-dir $claimedPath"
        Arguments = @('cargo', 'build', '--target-dir', $claimedPath)
    }
    $classified = [PSCustomObject]@{ Path = $probe; Class = 'disposable'; SizeGB = 0 }
    $r = Invoke-TargetGc -Classification @($classified) -Apply:$true -BusyProcesses @($claimer) -Roots @($root)
    Assert-Equal 0 $r.Deleted.Count
    Assert-Equal 0 $r.BytesFreed 'a claimed target directory must contribute no freed bytes'
    Assert-Equal 1 $r.SkippedDirs.Count 'a claimed directory must be counted as skipped'
    Assert-True (Test-Path $probe) 'a directory a live build names must survive'
    Assert-True ($r.SkippedDirs[0].Reason -match 'uses or names') 'the skip must identify the live process path claim'
}

Test-Case 'compiler linkers restrict gc to their named target directory' {
    $linkers = if ($IsLinux) { @('cc', 'ld.lld', 'mold', 'rust-lld') } else { @('link.exe', 'lld-link.exe', 'rust-lld.exe') }
    foreach ($n in $linkers) {
        $busyDir = New-FakeTarget -Root $root -Name "busy-$($n.Replace('.', '-'))" -Marker $null
        $unrelatedDir = New-FakeTarget -Root $root -Name "free-$($n.Replace('.', '-'))" -Marker $null
        $process = [PSCustomObject]@{
            Name = $n
            CommandLine = "$n --target-dir $busyDir"
            Argv0 = $n
            Arguments = @($n, '--target-dir', $busyDir)
        }
        $script:FakeCimProcessRows = @($process)

        try {
            $busy = if ($IsLinux) { @(Get-LinuxLiveBuildProcesses -Snapshot @($process)) } else { @(Get-LiveBuildProcesses) }
            Assert-Equal 1 $busy.Count "$n must be returned by the live-build process query"
            Assert-Equal $n $busy[0].Name

            $classification = @(
                [PSCustomObject]@{ Path = $busyDir; Class = 'disposable'; SizeGB = 0 },
                [PSCustomObject]@{ Path = $unrelatedDir; Class = 'disposable'; SizeGB = 0 }
            )
            $r = Invoke-TargetGc -Classification $classification -Apply:$true -BusyProcesses $busy -Roots @($root)
            Assert-Equal 1 $r.Deleted.Count "$n should protect its target directory and still allow unrelated cleanup"
            Assert-Contains $r.Deleted $unrelatedDir
            Assert-True (Test-Path $busyDir) "$n's target directory must survive"
            Assert-False (Test-Path $unrelatedDir) 'an unrelated disposable target directory should still be removed'
        } finally {
            $script:FakeCimProcessRows = @()
        }
    }
}

Test-Case 'a recently written directory blocks itself, since CARGO_TARGET_DIR names no path on a command line' {
    $probe = Join-Path $root 'fresh-probe'
    New-Item -ItemType Directory -Force -Path $probe | Out-Null
    Set-Content -Path (Join-Path $probe 'just-written.bin') -Value 'x'
    $classified = [PSCustomObject]@{ Path = $probe; Class = 'disposable'; SizeGB = 0 }
    $r = Invoke-TargetGc -Classification @($classified) -Apply:$true -BusyProcesses @() -Roots @($root)
    Assert-Equal 0 $r.Deleted.Count
    Assert-True (Test-Path $probe) 'a directory written to seconds ago must survive'
    Assert-True ($r.SkippedDirs[0].Reason -match 'last') 'the per-directory skip reason must name the recency'
}

Test-Case '-Apply with no busy processes deletes ONLY the disposable directories' {
    $expectedBytes = [long]((Get-ChildItem -LiteralPath $dirLegacyFlag, $dirDisposable -Recurse -Force -File |
        Measure-Object -Property Length -Sum).Sum)
    $r = Invoke-TargetGc -Classification $classification -Apply:$true -BusyProcesses @() -Roots @($root)
    Assert-False $r.Skipped
    # Two, not one: the legacy-flag dir joined this class when `preserved` stopped being consulted.
    Assert-Equal 2 $r.Deleted.Count
    Assert-Equal $expectedBytes $r.BytesFreed 'reported file bytes must match the files removed'
    Assert-Equal $expectedBytes $r.BytesWouldFree 'the pre-delete file-byte count must match the final removal count'
    Assert-Contains $r.Deleted $dirDisposable
    Assert-Contains $r.Deleted $dirLegacyFlag
    Assert-False (Test-Path $dirDisposable) 'the disposable directory must actually be removed'
    Assert-True (Test-Path $dirUnknown) 'unknown must survive -Apply'
    Assert-False (Test-Path $dirLegacyFlag) 'a schema-1 dir with the retired flag and a dead worktree must now be reclaimed, not shielded'
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

if ($IsLinux) {
    Test-Case 'Linux containment compares target roots with case-sensitive paths' {
        $caseRoot = Join-Path $root 'CaseRoot'
        $caseSibling = Join-Path $root 'caseroot'
        $outside = Join-Path $caseSibling 'target'
        New-Item -ItemType Directory -Force -Path $caseRoot, $outside | Out-Null
        Set-Content -LiteralPath (Join-Path $outside 'filler.bin') -Value 'bytes'
        Get-Item -LiteralPath (Join-Path $outside 'filler.bin') | ForEach-Object { $_.LastWriteTime = (Get-Date).AddHours(-3) }
        $classified = @([PSCustomObject]@{ Path = $outside; Class = 'disposable'; SizeGB = 0 })
        $threw = $false
        try { Invoke-TargetGc -Classification $classified -Apply -BusyProcesses @() -Roots @($caseRoot) } catch {
            $threw = $_.Exception.Message -match 'not contained in any configured cache root'
        }
        Assert-True $threw 'a case-distinct sibling directory must not pass Linux cache-root containment'
        Assert-True (Test-Path -LiteralPath $outside) 'the sibling target must remain untouched'
    }
}

Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue

Write-TestSummary
