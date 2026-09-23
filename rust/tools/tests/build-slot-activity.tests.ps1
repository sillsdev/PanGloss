# Tests the process-tree activity predicate used to identify stale build-slot holders.
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

function New-FakeProc {
    param([int]$Pid_, [string]$Name, [int]$ParentPid, [datetime]$Created)
    [PSCustomObject]@{
        ProcessId = $Pid_; Name = $Name; ParentProcessId = $ParentPid
        CreationDate = $Created; CommandLine = "$Name (synthetic)"
    }
}

$now = Get-Date

Test-Case 'a compiler descendant keeps a slot holder active' {
    $root = New-FakeProc -Pid_ 10 -Name 'pwsh.exe' -ParentPid 0 -Created $now.AddMinutes(-30)
    $cargo = New-FakeProc -Pid_ 11 -Name 'cargo.exe' -ParentPid 10 -Created $now.AddMinutes(-29)
    $holder = [PSCustomObject]@{ Pid = 10; Alive = $true }
    $snapshot = @($root, $cargo)
    Assert-False (Test-ManagedProcessTreeIdle -RootPid 10 -Snapshot $snapshot)
    Assert-False (Test-BuildSlotHolderStale -Holder $holder -Snapshot $snapshot -MinAgeMinutes 20 -Now $now)
}

Test-Case 'nested compiler activity keeps a slot holder active' {
    $root = New-FakeProc -Pid_ 20 -Name 'pwsh.exe' -ParentPid 0 -Created $now.AddMinutes(-30)
    $cargo = New-FakeProc -Pid_ 21 -Name 'cargo.exe' -ParentPid 20 -Created $now.AddMinutes(-29)
    $rustc = New-FakeProc -Pid_ 22 -Name 'rustc.exe' -ParentPid 21 -Created $now.AddMinutes(-28)
    Assert-False (Test-ManagedProcessTreeIdle -RootPid 20 -Snapshot @($root, $cargo, $rustc))
}

Test-Case 'a live slot holder with no build activity is stale after the age threshold' {
    $root = New-FakeProc -Pid_ 30 -Name 'pwsh.exe' -ParentPid 0 -Created $now.AddMinutes(-30)
    $holder = [PSCustomObject]@{ Pid = 30; Alive = $true }
    Assert-True (Test-ManagedProcessTreeIdle -RootPid 30 -Snapshot @($root))
    Assert-True (Test-BuildSlotHolderStale -Holder $holder -Snapshot @($root) -MinAgeMinutes 20 -Now $now)
}

Test-Case 'a young idle slot holder is not stale' {
    $root = New-FakeProc -Pid_ 40 -Name 'pwsh.exe' -ParentPid 0 -Created $now.AddMinutes(-5)
    $holder = [PSCustomObject]@{ Pid = 40; Alive = $true }
    Assert-False (Test-BuildSlotHolderStale -Holder $holder -Snapshot @($root) -MinAgeMinutes 20 -Now $now)
}

Test-Case 'a missing root is not classified as idle' {
    $other = New-FakeProc -Pid_ 50 -Name 'cargo.exe' -ParentPid 49 -Created $now.AddMinutes(-5)
    Assert-False (Test-ManagedProcessTreeIdle -RootPid 999 -Snapshot @($other))
}

Write-TestSummary
