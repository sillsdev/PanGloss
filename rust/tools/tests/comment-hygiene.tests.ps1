<#
  .DESCRIPTION
  Covers: rust/tools/comment-hygiene.ps1 -- specifically that a change of Rust comment marker
  (`///`/`//!` vs `//`) starts a new comment block, so a `//` run sitting directly under a `///`
  doc comment is scored on its own rather than inheriting the doc block's classification. Runs the
  REAL launcher against a synthetic scan root, using the current worktree's native checker.
#>
. "$PSScriptRoot\_test-harness.ps1"

$script:ToolsDir = Join-Path $PSScriptRoot '..'

function New-HygieneFixtureRepo {
    param([Parameter(Mandatory)][string]$RustFileContent)
    $root = New-TestTempDir -Prefix 'pg-hygiene-fixture'
    $toolsDir = Join-Path $root 'rust\tools'
    New-Item -ItemType Directory -Force -Path $toolsDir | Out-Null
    $crateSrc = Join-Path $root 'rust\crates\hygienefixture\src'
    New-Item -ItemType Directory -Force -Path $crateSrc | Out-Null
    Set-Content -Path (Join-Path $crateSrc 'lib.rs') -Value $RustFileContent
    return $root
}

Test-Case 'a `//` run directly under a `///` doc block is its own block, capped at one line' {
    $rustFile = @'
/// Doc summary for the fixture item.
/// Second line of the doc comment.
// Note one about the implementation.
// Note two about the implementation.
// Note three about the implementation.
pub fn fixture_item() -> i32 {
    0
}
'@
    $root = New-HygieneFixtureRepo -RustFileContent $rustFile
    try {
        $out = & pwsh -NoProfile -File (Join-Path $script:ToolsDir 'comment-hygiene.ps1') -RepoRoot $root -List 2>&1 | Out-String
        Assert-Equal 1 $LASTEXITCODE 'violations must retain their distinct exit status'
        # The doc block must stay unflagged (api, uncapped): only the split-off `//` run is a violation.
        $docHits = [regex]::Matches($out, 'lib\.rs:\d+: \d+ lines, no claim: /// Doc summary')
        $implHits = [regex]::Matches($out, 'lib\.rs:\d+: 3 lines, no claim: // Note one')
        Assert-Equal 0 $docHits.Count "the `///` doc block must not be flagged (got: $out)"
        Assert-Equal 1 $implHits.Count "expected exactly one impl-comment-too-long hit for the split `//` run (got: $out)"
    } finally {
        Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
    }
}

Test-Case 'an empty source tree is clean and supports structured output' {
    $root = New-HygieneFixtureRepo -RustFileContent 'pub fn fixture_item() {}'
    try {
        $out = & pwsh -NoProfile -File (Join-Path $script:ToolsDir 'comment-hygiene.ps1') -RepoRoot $root -Json 2>&1 | Out-String
        Assert-Equal 0 $LASTEXITCODE "clean fixture failed: $out"
        $report = $out | ConvertFrom-Json
        Assert-Equal 0 $report.total 'clean tree must report zero findings'
    } finally {
        Remove-Item -LiteralPath $root -Recurse -Force
    }
}

Test-Case 'a nonexistent scan root fails with no clean verdict' {
    $missing = Join-Path ([IO.Path]::GetTempPath()) "pg-hygiene-missing-$([guid]::NewGuid())"
    $out = & pwsh -NoProfile -File (Join-Path $script:ToolsDir 'comment-hygiene.ps1') -RepoRoot $missing 2>&1 | Out-String
    Assert-Equal 2 $LASTEXITCODE "missing root must be a tool error: $out"
    Assert-False ($out -match '\[comment-hygiene\] clean')
}

Write-TestSummary
