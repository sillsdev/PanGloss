<#
  Covers: Test-DiskReserve (rust/tools/_common.ps1). Takes a free-space NUMBER rather than
  querying a real drive, precisely so this is testable without touching any real disk -- these
  tests never call Get-FreeSpaceGB or reference C:\cargo-targets / G:\cargo-build-cache.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

Test-Case 'plenty of free space is Ok' {
    $r = Test-DiskReserve -FreeGB 100 -MinFreeGB 5
    Assert-True $r.Ok $r.Detail
    Assert-Equal 100 $r.FreeGB
}

Test-Case 'free space below the reserve is rejected' {
    $r = Test-DiskReserve -FreeGB 2 -MinFreeGB 5
    Assert-False $r.Ok 'free space under the reserve must be rejected'
}

Test-Case 'free space exactly at the reserve boundary is Ok (>=, not >)' {
    $r = Test-DiskReserve -FreeGB 5 -MinFreeGB 5
    Assert-True $r.Ok $r.Detail
}

Test-Case 'unknown free space (drive not queryable) does not block the build' {
    $r = Test-DiskReserve -FreeGB $null -MinFreeGB 5
    Assert-True $r.Ok 'an unqueryable drive must not itself fail the preflight'
}

Test-Case 'default reserve is 5GB when -MinFreeGB is not passed' {
    $ok = Test-DiskReserve -FreeGB 10
    $notOk = Test-DiskReserve -FreeGB 1
    Assert-True $ok.Ok
    Assert-False $notOk.Ok
}

Write-TestSummary
