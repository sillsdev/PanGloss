<#
  Covers: Get-FilterZeroMatchHint and Assert-ScriptAndCwdAgreeOnWorktree (rust/tools/_common.ps1) --
  the two guards added after the same operator mistake recurred seven times in one session.

  WHY THESE EXIST AS TESTS AT ALL. Both guards only fire in conditions that are expensive or awkward
  to reach for real: the hint fires after a cargo run reports "no tests to run", and a cold cargo run
  on this repo is ~996s; the worktree guard fires only when a script is invoked by absolute path from
  a foreign directory. A guard whose only exercise costs sixteen minutes, or requires a second
  worktree to exist, is a guard nobody re-checks after editing it. So the hint is a PURE function over
  (filter, target list) and is asserted here with no build, no cargo, and no process; the worktree
  guard's own end-to-end proof is recorded in its commit message, and what is asserted here is the
  decision it rests on.

  THE MISTAKE BEING GUARDED, for whoever edits this next. `-Filter` is appended as a bare positional
  to the test runner, where it matches TEST NAMES as a SUBSTRING. It never matches file names or
  test-target names, and it never reduces what cargo COMPILES. `-TestTarget` maps to cargo's
  `--test <name>` and does reduce compilation -- to one binary instead of ~78 for pg-foma. Measured
  on this machine: 10.6s warm via -TestTarget versus ~996s cold for the whole package.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

$targets = @(
    'net_dedup_gate',
    'net_dedup_sizing_census',
    'orthogonal_basis_group_a',
    'orthogonal_basis_group_b',
    'morphotactics_boundary_cleanup_slice',
    'parity_divergence_census'
)

# ---------------------------------------------------------------------------------------------
# The exact seven-times mistake: a test TARGET name handed to a TEST-NAME filter.
# ---------------------------------------------------------------------------------------------

Test-Case 'a filter that names a test TARGET is told to use -TestTarget instead' {
    $hint = Get-FilterZeroMatchHint -Filter 'orthogonal_basis_group_b' -TestTargets $targets
    Assert-True ($hint.Count -ge 2) 'expected a multi-line hint'
    $text = ($hint | ForEach-Object { $_.Text }) -join "`n"
    Assert-True ($text -match 'is a test TARGET') 'the hint must say the filter named a target, not a test'
    Assert-True ($text -match '-TestTarget orthogonal_basis_group_b') 'the hint must name the exact corrected invocation'
    # The correction must be the actionable line, i.e. green rather than buried in yellow prose.
    $green = @($hint | Where-Object { $_.Color -eq 'Green' })
    Assert-True ($green.Count -ge 1) 'the corrected invocation must be highlighted, not buried'
}

Test-Case 'a near-miss filter offers the nearest test targets' {
    # `net_dedup` was one of the real wasted invocations: a prefix of two target names.
    $hint = Get-FilterZeroMatchHint -Filter 'net_dedup' -TestTargets $targets
    $text = ($hint | ForEach-Object { $_.Text }) -join "`n"
    Assert-True ($text -match 'Did you mean -TestTarget') 'a near-miss must suggest candidates'
    Assert-True ($text -match 'net_dedup_gate') 'the suggestion must include the obvious target'
    Assert-True ($text -match 'net_dedup_sizing_census') 'the suggestion must include the other matching target'
}

Test-Case 'a filter that matches no target still explains the two mechanisms' {
    $hint = Get-FilterZeroMatchHint -Filter 'totally_unrelated_xyz' -TestTargets $targets
    $text = ($hint | ForEach-Object { $_.Text }) -join "`n"
    Assert-True ($text -match 'matches TEST NAMES as a substring') 'must still state what -Filter does'
    Assert-True ($text -notmatch 'Did you mean') 'must not invent a suggestion when nothing is near'
}

Test-Case 'no filter yields no hint' {
    # The hint must never fire on a run that did not use -Filter at all -- a zero-test run has other
    # causes (an empty package, everything #[ignore]d) and blaming the filter would misdirect.
    Assert-Equal 0 (Get-FilterZeroMatchHint -Filter '' -TestTargets $targets).Count 'empty filter must produce no hint'
}

Test-Case 'an empty target list degrades to the explanation, never to a throw' {
    # Target discovery is a filesystem walk that can legitimately return nothing (wrong package name,
    # a crate with no tests/ dir). It must not turn a helpful hint into an error.
    $hint = Get-FilterZeroMatchHint -Filter 'net_dedup' -TestTargets @()
    Assert-True ($hint.Count -ge 1) 'must still explain the mechanism with no targets known'
    $text = ($hint | ForEach-Object { $_.Text }) -join "`n"
    Assert-True ($text -notmatch 'Did you mean') 'must not suggest from an empty list'
}

# ---------------------------------------------------------------------------------------------
# The worktree guard's decision. Asserted via the same resolution the guard uses, so that a future
# edit that stops comparing repo roots fails here.
# ---------------------------------------------------------------------------------------------

Test-Case 'the guard is a no-op when the script and cwd share a worktree' {
    # Running FROM this repo, against THIS repo's own copy of the script: must not refuse. If this
    # ever fails, every ordinary managed build on the machine is refusing, which is the one regression
    # that would be worse than the bug the guard fixes.
    $scriptRoot = Join-Path (Split-Path $PSScriptRoot -Parent) ''
    $scriptRepo = (Resolve-Path (Join-Path $scriptRoot '..\..')).Path.TrimEnd('\', '/')
    Push-Location $scriptRepo
    try {
        $cwdRepo = (Resolve-Path (Get-RepoRoot)).Path.TrimEnd('\', '/')
        Assert-Equal $scriptRepo $cwdRepo 'script root and cwd root must agree when run from the repo itself'
    } finally { Pop-Location }
}

Test-Case 'a foreign cwd resolves to a DIFFERENT repo root than the script (the bug the guard catches)' {
    # Not asserting the exit -- that would end this test process. Asserting the CONDITION the guard
    # branches on, using a real second worktree if one exists. Skipped with a printed reason rather
    # than passing vacuously when there is no second worktree to stand in.
    $scriptRepo = (Resolve-Path (Join-Path (Split-Path $PSScriptRoot -Parent) '..\..')).Path.TrimEnd('\', '/')
    $other = @(Get-ChildItem -Path (Join-Path $scriptRepo '.claude\worktrees') -Directory -ErrorAction SilentlyContinue |
        Where-Object { Test-Path (Join-Path $_.FullName 'rust\tools\pg.ps1') } | Select-Object -First 1)
    if ($other.Count -eq 0) {
        Write-Host '    (skipped: no sibling worktree present to stand in -- the guard itself is proven end-to-end in its commit)'
        return
    }
    Push-Location $other[0].FullName
    try {
        $cwdRepo = (Resolve-Path (Get-RepoRoot)).Path.TrimEnd('\', '/')
        Assert-True ($cwdRepo -ine $scriptRepo) "standing in $($other[0].Name) must resolve to a different repo root than the script's"
    } finally { Pop-Location }
}

# REQUIRED, and omitting it is the reason this line has a comment. `Test-Case` records failures and
# prints them, but the SCRIPT still exits 0 unless Write-TestSummary is called -- so a *.tests.ps1
# without this line reports FAIL lines and a success exit code, i.e. it cannot gate anything. This
# file shipped that way for one revision and only a deliberate mutation of the code under test
# revealed it: the passing run looked identical either way. Same defect class as every "green gate
# that never fails" this repo has had to fix.
Write-TestSummary
