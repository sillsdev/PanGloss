<#
  Covers: Test-MemoryReserve, Get-MemoryProcessBudget, Get-PerJobMemoryGB and
  Resolve-ConcurrencyBudget (rust/tools/_common.ps1) -- the memory half of the "do not spawn
  without headroom" gate.

  Every function under test takes an available-memory NUMBER rather than querying the machine,
  precisely so these are testable at any real memory pressure: a test that called
  Get-AvailableMemoryGB would assert something different on every run, and would pass on a busy
  machine for the wrong reason. Nothing here reads real memory or starts a process.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

# ---------------------------------------------------------------------------------------------
# Test-MemoryReserve: the hard spawn gate
# ---------------------------------------------------------------------------------------------

Test-Case 'plenty of available memory is Ok' {
    $r = Test-MemoryReserve -AvailableGB 40 -MinFreeGB 8
    Assert-True $r.Ok $r.Detail
    Assert-Equal 40 $r.AvailableGB
}

Test-Case 'available memory below the reserve is rejected' {
    $r = Test-MemoryReserve -AvailableGB 3 -MinFreeGB 8
    Assert-False $r.Ok 'a machine under the memory reserve must not start a build'
}

Test-Case 'available memory exactly at the reserve is Ok (>=, not >)' {
    $r = Test-MemoryReserve -AvailableGB 8 -MinFreeGB 8
    Assert-True $r.Ok $r.Detail
}

Test-Case 'unknown available memory does not block the build' {
    # Same rule as Test-DiskReserve's null case, and the same trap: a [double] parameter would
    # coerce $null to 0.0, making "could not measure" indistinguishable from "nothing left" and
    # blocking every build on any machine where the CIM query fails.
    $r = Test-MemoryReserve -AvailableGB $null -MinFreeGB 8
    Assert-True $r.Ok 'an unqueryable memory counter must not itself fail the preflight'
    Assert-Equal $null $r.AvailableGB
}

Test-Case 'default reserve is used when -MinFreeGB is not passed' {
    $ok = Test-MemoryReserve -AvailableGB ($script:InteractiveReserveGB + 10)
    $notOk = Test-MemoryReserve -AvailableGB ($script:InteractiveReserveGB - 1)
    Assert-True $ok.Ok
    Assert-False $notOk.Ok
}

# ---------------------------------------------------------------------------------------------
# Get-MemoryProcessBudget: available memory -> a concurrency number
# ---------------------------------------------------------------------------------------------

Test-Case 'budget subtracts the reserve before dividing' {
    # 40 available - 8 reserve = 32 usable; at 4GB/process that is 8, NOT 10.
    $n = Get-MemoryProcessBudget -AvailableGB 40 -PerProcessGB 4 -ReserveGB 8 -MaxConcurrent 1
    Assert-Equal 8 $n 'the interactive reserve must be withheld, not handed to the build'
}

Test-Case 'budget divides by MaxConcurrent so two slots together stay inside the reserve' {
    $one = Get-MemoryProcessBudget -AvailableGB 40 -PerProcessGB 4 -ReserveGB 8 -MaxConcurrent 1
    $two = Get-MemoryProcessBudget -AvailableGB 40 -PerProcessGB 4 -ReserveGB 8 -MaxConcurrent 2
    Assert-Equal 8 $one
    Assert-Equal 4 $two 'each of two permitted builds must be sized for the case where both run'
}

Test-Case 'budget floors at 1, never 0' {
    # Reaching here means Test-MemoryReserve already passed, so the honest answer is "one at a
    # time". A 0 would be reported as a concurrency setting while actually meaning "cannot run".
    $n = Get-MemoryProcessBudget -AvailableGB 9 -PerProcessGB 4 -ReserveGB 8 -MaxConcurrent 2
    Assert-Equal 1 $n
}

Test-Case 'budget never goes negative when available is under the reserve' {
    $n = Get-MemoryProcessBudget -AvailableGB 2 -PerProcessGB 4 -ReserveGB 8 -MaxConcurrent 1
    Assert-Equal 1 $n 'usable memory must clamp at 0, not go negative and produce a negative budget'
}

Test-Case 'unknown available memory yields no opinion (null), not a fabricated cap' {
    $n = Get-MemoryProcessBudget -AvailableGB $null -PerProcessGB 4 -ReserveGB 8 -MaxConcurrent 1
    Assert-Equal $null $n 'an unmeasurable machine must not silently clamp concurrency'
}

Test-Case 'a nonsensical per-process allowance yields no opinion rather than a divide-by-zero' {
    Assert-Equal $null (Get-MemoryProcessBudget -AvailableGB 40 -PerProcessGB 0 -ReserveGB 8)
    Assert-Equal $null (Get-MemoryProcessBudget -AvailableGB 40 -PerProcessGB -1 -ReserveGB 8)
}

# ---------------------------------------------------------------------------------------------
# Get-PerJobMemoryGB: fat-LTO linking is the outlier that took the machine down
# ---------------------------------------------------------------------------------------------

Test-Case 'fat-LTO builds assume a heavier per-job allowance than thin-LTO ones' {
    $thin = Get-PerJobMemoryGB
    $fat = Get-PerJobMemoryGB -FatLto
    Assert-True ($fat -gt $thin) "fat-LTO linking holds a whole dependency graph's IR in one address space; it must not be sized like a per-crate codegen (thin=$thin fat=$fat)"
}

Test-Case 'the fat-LTO allowance actually narrows concurrency where the thin one would not' {
    $thinN = Get-MemoryProcessBudget -AvailableGB 36 -PerProcessGB (Get-PerJobMemoryGB) -ReserveGB 8 -MaxConcurrent 1
    $fatN = Get-MemoryProcessBudget -AvailableGB 36 -PerProcessGB (Get-PerJobMemoryGB -FatLto) -ReserveGB 8 -MaxConcurrent 1
    Assert-True ($fatN -lt $thinN) "the fat-LTO allowance must bind sooner than the thin one (thin=$thinN fat=$fatN)"
}

Test-Case 'an idle machine is NOT throttled: the gate costs nothing when memory is free' {
    # The other half of the contract, and the one an over-cautious constant silently breaks. A
    # draft of this gate assumed 8GB per fat-LTO job and cut every `-Mode build` from 7 jobs to 2;
    # measurement then put the real peak at 0.71GB (see $script:MemoryPerLtoLinkJobGB). A gate that
    # taxes every ordinary build gets turned off, and then protects nothing -- so "does it stay out
    # of the way at rest" is as much a requirement as "does it refuse under pressure".
    $total = Get-TotalMemoryGB
    if ($null -eq $total) { return }  # unmeasurable: nothing to calibrate against
    $cpu = Get-CargoJobBudget -MaxConcurrent 2
    foreach ($perProc in @((Get-PerJobMemoryGB), (Get-PerJobMemoryGB -FatLto), $script:MemoryPerTestProcessGB)) {
        $n = Get-MemoryProcessBudget -AvailableGB $total -PerProcessGB $perProc -MaxConcurrent 2
        $r = Resolve-ConcurrencyBudget -CpuBudget $cpu -MemoryBudget $n
        Assert-Equal 'cpu' $r.Bound "with ${total}GB installed and nothing running, a ${perProc}GB/process budget must not narrow the cores-only cap (cpu=$cpu memory=$n)"
    }
}

Test-Case 'a machine under real pressure IS throttled below the cores-only cap' {
    # The half that does the protecting: same budgets, but with most of the memory already spoken
    # for by something else (another worktree's build, a corpus test holding several GB).
    $cpu = Get-CargoJobBudget -MaxConcurrent 2
    foreach ($perProc in @((Get-PerJobMemoryGB), (Get-PerJobMemoryGB -FatLto), $script:MemoryPerTestProcessGB)) {
        $n = Get-MemoryProcessBudget -AvailableGB 14 -PerProcessGB $perProc -MaxConcurrent 2
        $r = Resolve-ConcurrencyBudget -CpuBudget $cpu -MemoryBudget $n
        Assert-Equal 'memory' $r.Bound "with only 14GB available, a ${perProc}GB/process budget must bind before the cores-only cap (cpu=$cpu memory=$n)"
    }
}

Test-Case 'test processes are assumed heavier than a thin-LTO compile job' {
    # A test process here can be a whole grammar compile; a rustc under thin LTO cannot.
    Assert-True ($script:MemoryPerTestProcessGB -gt $script:MemoryPerCompileJobGB)
}

# ---------------------------------------------------------------------------------------------
# Resolve-ConcurrencyBudget: which constraint won, and does the record say so
# ---------------------------------------------------------------------------------------------

Test-Case 'the lower of the cpu and memory budgets wins, and reports memory as the binder' {
    $r = Resolve-ConcurrencyBudget -CpuBudget 7 -MemoryBudget 3
    Assert-Equal 3 $r.Value
    Assert-Equal 'memory' $r.Bound
}

Test-Case 'a roomy machine stays cpu-bound' {
    $r = Resolve-ConcurrencyBudget -CpuBudget 7 -MemoryBudget 20
    Assert-Equal 7 $r.Value
    Assert-Equal 'cpu' $r.Bound
}

Test-Case 'equal budgets are reported as cpu-bound, not memory-bound' {
    # Strictly-less-than, so an incidental tie does not claim memory forced a number it did not.
    $r = Resolve-ConcurrencyBudget -CpuBudget 7 -MemoryBudget 7
    Assert-Equal 7 $r.Value
    Assert-Equal 'cpu' $r.Bound
}

Test-Case 'unmeasurable memory falls back to the cpu budget rather than to 1' {
    $r = Resolve-ConcurrencyBudget -CpuBudget 7 -MemoryBudget $null
    Assert-Equal 7 $r.Value
    Assert-Equal 'cpu' $r.Bound
}

Test-Case 'an explicit override is never narrowed by either budget' {
    # -Jobs/-TestThreads exist for the case where the operator is at the console and knows better;
    # silently overriding them would make the printed number a lie.
    $r = Resolve-ConcurrencyBudget -CpuBudget 16 -MemoryBudget 2 -Explicit
    Assert-Equal 16 $r.Value
    Assert-Equal 'explicit' $r.Bound
}

# ---------------------------------------------------------------------------------------------
# Wiring: the gate has to have a distinct exit code, and the live query has to answer sanely
# ---------------------------------------------------------------------------------------------

Test-Case 'low memory has its own exit code, distinct from low disk' {
    Assert-Equal 17 $script:ExitCodeLowMemory
    Assert-True ($script:ExitCodeLowMemory -ne $script:ExitCodeLowDisk) 'two failures with different recoveries must not share a code'
}

Test-Case 'Get-AvailableMemoryGB answers with a plausible number or null, and never throws' {
    # The one test that touches the real machine. It asserts only the contract every caller relies
    # on -- a positive number no larger than installed RAM, or $null -- not any particular value.
    $avail = Get-AvailableMemoryGB
    $total = Get-TotalMemoryGB
    if ($null -ne $avail) {
        Assert-True ($avail -gt 0) "available memory must be positive when measurable (got $avail)"
        if ($null -ne $total) {
            Assert-True ($avail -le $total) "available ($avail GB) cannot exceed installed ($total GB)"
        }
    }
}

Write-TestSummary
