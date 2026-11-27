. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\ledger-check.ps1"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("ledger-check-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null

function New-Ledger {
    param([string[]]$Rows, [string]$Entries = "### alpha`n- **Status:** x`n")
    $p = Join-Path $tmp ("l" + [guid]::NewGuid() + '.md')
    $head = "# L`n`n| Idea | Status | Target blowup | Issues | Entry |`n|---|---|---|---|---|`n"
    Set-Content -LiteralPath $p -Value ($head + ($Rows -join "`n") + "`n`n## Entries`n`n" + $Entries)
    return $p
}

Test-Case 'the repository ledger passes the lint' {
    $errs = @(Test-LedgerFile -Path (Join-Path $repoRoot 'docs/research/optimization/README.md'))
    Assert-Equal 0 $errs.Count ($errs -join "`n")
}

Test-Case 'a clean fixture passes, with qualified statuses' {
    $p = New-Ledger -Rows @(
        '| alpha (first) | landed (PR #1) | b | #1 | [entry](#alpha) |',
        '| beta | idea, owner decision needed | b | none | [entry](#alpha) |')
    Assert-Equal 0 @(Test-LedgerFile -Path $p).Count
}

Test-Case 'an unknown status names the row and the allowed list' {
    $p = New-Ledger -Rows @('| alpha | maybe | b | #1 | [entry](#alpha) |')
    $e = @(Test-LedgerFile -Path $p)
    Assert-Equal 1 $e.Count
    Assert-True ($e[0] -match "row 'alpha'" -and $e[0] -match 'open-upstream') $e[0]
}

Test-Case 'a rejected row without a killing number fails; with one and a source it passes' {
    $bad = New-Ledger -Rows @('| alpha | rejected (it did not help) | b | #12 | [entry](#alpha) |')
    $e = @(Test-LedgerFile -Path $bad)
    Assert-True ($e.Count -eq 1 -and $e[0] -match 'killing number') ($e -join ';')
    foreach ($ok in @('rejected (+7.7% work, PR #572)', 'rejected: 1.1x (#9)', 'rejected, 0 steps (machine#564)')) {
        $p = New-Ledger -Rows @("| alpha | $ok | b | none | [entry](#alpha) |")
        Assert-Equal 0 @(Test-LedgerFile -Path $p).Count "should pass: $ok"
    }
}

Test-Case 'a number without a source fails, and the entry text can supply either' {
    $p = New-Ledger -Rows @('| alpha | rejected (1.1x) | b | none | [entry](#alpha) |')
    Assert-Equal 1 @(Test-LedgerFile -Path $p).Count
    $p2 = New-Ledger -Rows @('| alpha | rejected | b | none | [entry](#alpha) |') -Entries "### alpha`n- 1.1x more steps, see #44`n"
    Assert-Equal 0 @(Test-LedgerFile -Path $p2).Count
}

Test-Case 'a broken anchor or file link is reported' {
    $p = New-Ledger -Rows @('| alpha | idea | b | none | [entry](#missing) |', '| beta | idea | b | none | [file](nope.md) |')
    $e = @(Test-LedgerFile -Path $p)
    Assert-Equal 2 $e.Count
    Assert-True ($e[0] -match 'anchor' -and $e[1] -match 'does not exist') ($e -join ';')
}

Test-Case 'a file link resolves next to the ledger' {
    $p = New-Ledger -Rows @('| alpha | idea | b | none | [file](side.md) |')
    Set-Content -LiteralPath (Join-Path $tmp 'side.md') -Value 'x'
    Assert-Equal 0 @(Test-LedgerFile -Path $p).Count
}

Test-Case 'duplicate idea names are reported, ignoring the parenthetical and case' {
    $p = New-Ledger -Rows @('| alpha | idea | b | none | [entry](#alpha) |', '| Alpha (again) | idea | b | none | [entry](#alpha) |')
    $e = @(Test-LedgerFile -Path $p)
    Assert-True ($e.Count -eq 1 -and $e[0] -match 'duplicate') ($e -join ';')
}

Test-Case 'rejected: no isolated measurement (<reason>) is accepted and counted; an empty reason or other text is not' {
    $script:LedgerExceptionCount = 0
    $p = New-Ledger -Rows @('| alpha | rejected: no isolated measurement (reason here) | b | none | [entry](#alpha) |')
    Assert-Equal 0 @(Test-LedgerFile -Path $p).Count
    Assert-Equal 1 $script:LedgerExceptionCount
    foreach ($bad in @('rejected: no isolated measurement ()', 'rejected: no isolated measurement', 'rejected: not measured (reason)', 'rejected: it is slow')) {
        $p = New-Ledger -Rows @("| alpha | $bad | b | none | [entry](#alpha) |")
        Assert-Equal 1 @(Test-LedgerFile -Path $p).Count "should fail: $bad"
    }
}

Test-Case 'dead-end bullets need a number or the exception' {
    $p = New-Ledger -Rows @('| alpha | idea | b | none | [entry](#alpha) |') -Entries "### alpha`n- **One:** rejected: 2.3x more instances (machine#596).`n- **Two:** rejected: no isolated measurement (out of scope).`n- **Three:** rejected: boring.`n"
    $e = @(Test-LedgerFile -Path $p)
    Assert-True ($e.Count -eq 1 -and $e[0] -match 'bullet') ($e -join ';')
}

$names = @('shape-table', 'work-cap')
$parser = @('rust/crates/pg-rules/src/cascade.rs')

Test-Case 'a PR that touches no parser path needs no Ledger line' {
    Assert-Equal 0 @(Test-LedgerPrBody -Body '' -ChangedFiles @('docs/x.md', 'rust/crates/pg-cli/src/main.rs') -RowNames $names).Count
}

Test-Case 'each parser path is recognised' {
    foreach ($f in @('rust/crates/pg-rules/src/a.rs', 'rust/crates/pg-parse/src/a.rs', 'rust/crates/pg-grammar/src/compile/a.rs', 'rust/crates/pg-grammar/src/compile.rs', 'rust/crates/pg-fst/src/traverse.rs', 'rust/crates/pg-fst/src/traverse/x.rs')) {
        Assert-True (Test-LedgerPathIsParser $f) $f
    }
    Assert-False (Test-LedgerPathIsParser 'rust/crates/pg-grammar/src/load.rs')
    Assert-False (Test-LedgerPathIsParser 'rust/crates/pg-fst/src/compile.rs')
}

Test-Case 'a parser PR without a Ledger line fails' {
    $e = @(Test-LedgerPrBody -Body "## Summary`nstuff" -ChangedFiles $parser -RowNames $names)
    Assert-True ($e.Count -eq 1 -and $e[0] -match "no 'Ledger:' line") ($e -join ';')
}

Test-Case 'a Ledger line naming a row, a row with a qualifier, or n/a with a reason passes' {
    foreach ($b in @("Ledger: shape-table", "- Ledger: Shape-Table (per-word graph)", "text`nLedger: ``work-cap``", "Ledger: n/a (tooling)")) {
        Assert-Equal 0 @(Test-LedgerPrBody -Body $b -ChangedFiles $parser -RowNames $names).Count $b
    }
}

Test-Case 'an unknown row, bare n/a, or empty name fails' {
    foreach ($b in @('Ledger: no-such-row', 'Ledger: n/a', 'Ledger: n/a ()', 'Ledger:')) {
        Assert-Equal 1 @(Test-LedgerPrBody -Body $b -ChangedFiles $parser -RowNames $names).Count $b
    }
}

Remove-Item -Recurse -Force $tmp
Write-TestSummary
