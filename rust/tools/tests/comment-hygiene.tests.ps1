<#
  .DESCRIPTION
  Covers: rust/tools/comment-hygiene.ps1 -- specifically that a change of Rust comment marker
  (`///`/`//!` vs `//`) starts a new comment block, so a `//` run sitting directly under a `///`
  doc comment is scored on its own rather than inheriting the doc block's classification. Runs the
  REAL script as a subprocess against a synthetic mini-repo (never a re-implementation of its regex),
  because the script computes its own repo root from $PSScriptRoot and cannot be pointed elsewhere.
#>
. "$PSScriptRoot\_test-harness.ps1"

$script:ToolsDir = Join-Path $PSScriptRoot '..'

function New-HygieneFixtureRepo {
    param([Parameter(Mandatory)][string]$RustFileContent)
    $root = New-TestTempDir -Prefix 'pg-hygiene-fixture'
    $toolsDir = Join-Path $root 'rust\tools'
    New-Item -ItemType Directory -Force -Path $toolsDir | Out-Null
    Copy-Item (Join-Path $script:ToolsDir 'comment-hygiene.ps1') (Join-Path $toolsDir 'comment-hygiene.ps1')
    Copy-Item (Join-Path $script:ToolsDir '_comment-lines.ps1') (Join-Path $toolsDir '_comment-lines.ps1')
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
        $out = & pwsh -NoProfile -File (Join-Path $root 'rust\tools\comment-hygiene.ps1') -List 2>&1 | Out-String
        # The doc block must stay unflagged (api, uncapped): only the split-off `//` run is a violation.
        $docHits = [regex]::Matches($out, 'lib\.rs:\d+: \d+ lines, no claim: /// Doc summary')
        $implHits = [regex]::Matches($out, 'lib\.rs:\d+: 3 lines, no claim: // Note one')
        Assert-Equal 0 $docHits.Count "the `///` doc block must not be flagged (got: $out)"
        Assert-Equal 1 $implHits.Count "expected exactly one impl-comment-too-long hit for the split `//` run (got: $out)"
    } finally {
        Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
    }
}

Write-TestSummary
