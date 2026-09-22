. "$PSScriptRoot\_test-harness.ps1"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$agents = Get-Content -Raw -LiteralPath (Join-Path $repoRoot 'AGENTS.md')
$guidePath = Join-Path $repoRoot 'docs\development\agent-build-loop.md'

Test-Case 'AGENTS points build and test failures to the agent build loop' {
    Assert-True ($agents -match [regex]::Escape('docs/development/agent-build-loop.md')) `
        'the always-loaded instructions must point recurring build failures to one source of truth'
}

Test-Case 'agent build loop requires fresh PowerShell processes' {
    Assert-True (Test-Path -LiteralPath $guidePath -PathType Leaf) 'agent build loop guide must exist'
    $guide = Get-Content -Raw -LiteralPath $guidePath
    Assert-True ($guide -match [regex]::Escape('pwsh -NoProfile -File rust/tools/pg.ps1')) `
        'managed Rust commands must use a fresh PowerShell process'
    Assert-True ($guide -match [regex]::Escape('pwsh -NoProfile -File rust/tools/tests/run-all.ps1')) `
        'PowerShell suites must use the existing isolated aggregator'
}

Test-Case 'agent build loop classifies pre-launch helper failure separately from a build failure' {
    $guide = Get-Content -Raw -LiteralPath $guidePath
    Assert-True ($guide -match [regex]::Escape('helper_unknown_error: setup refresh had errors')) `
        'the recurring pre-launch helper signature must be named exactly'
    Assert-True ($guide -match 'before PowerShell starts') `
        'the guide must prevent agents from blaming repository code for a launcher failure'
    Assert-True ($guide -match 'require_escalated') `
        'the one permitted recovery retry must name the sandbox override agents need'
}

Write-TestSummary
