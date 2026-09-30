. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

Test-Case 'managed native launch preserves filter expressions, quotes, empty values and trailing slashes' {
    $probe = Join-Path ([IO.Path]::GetTempPath()) "pg-argv-$PID.ps1"
    $output = Join-Path ([IO.Path]::GetTempPath()) "pg-argv-$PID.json"
    [IO.File]::WriteAllText($probe, '[Console]::Write((ConvertTo-Json -InputObject @($args) -Compress))')
    try {
        $expected = @('test(/^schema_conformance::/) & test(case_evidence)', 'C:\fixture path\trailing\', 'quoted "analysis" text', '')
        $code = Invoke-ManagedProcess -Exe 'pwsh' -CmdArgs (@('-NoProfile', '-File', $probe) + $expected) `
            -WorkingDirectory ([IO.Path]::GetTempPath()) -CaptureStdoutPath $output
        Assert-Equal 0 $code 'argv probe must execute'
        $actual = @(Get-Content -Raw -LiteralPath $output | ConvertFrom-Json)
        Assert-Equal $expected.Count $actual.Count 'argv element boundaries must survive the native launcher'
        for ($i = 0; $i -lt $expected.Count; $i++) {
            Assert-Equal $expected[$i] $actual[$i] "argv value $i must survive unchanged"
        }
    } finally {
        foreach ($path in @($probe, $output)) {
            try { [IO.File]::Delete($path) } catch { }
        }
    }
}
Write-TestSummary
