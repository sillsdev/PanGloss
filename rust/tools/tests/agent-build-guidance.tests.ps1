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

Test-Case 'agent build loop requires elevated read-only diagnosis after the retry fails' {
    $guide = Get-Content -Raw -LiteralPath $guidePath
    Assert-True ($guide -match [regex]::Escape('pwsh -NoProfile -File rust/tools/sandbox-refresh-diagnostic.ps1')) `
        'the guide must name the sandbox refresh diagnostic command'
    Assert-True ($guide -match '(?is)retry.{0,160}require_escalated.{0,240}diagnos') `
        'a failed elevated retry must lead to elevated diagnosis'
    Assert-True ($guide -match '(?is)diagnostic itself.{0,100}require_escalated') `
        'the diagnostic command must also run with require_escalated'
}

Test-Case 'agent build loop requires exact-target recoverable quarantine and effect verification' {
    $guide = Get-Content -Raw -LiteralPath $guidePath
    Assert-True ($guide -match '(?is)exact (?:generated )?target.{0,600}Move-Item') `
        'the guide must require verifying the exact target before a manual move'
    Assert-True ($guide -match '(?is)Move-Item.{0,200}(?:outside|out of) the runtime tree') `
        'the guide must move one entry outside the runtime tree'
    Assert-True ($guide -match '(?is)normal sandboxed command.{0,160}apply_patch') `
        'the guide must verify both the sandboxed command and apply_patch effects'
    Assert-True ($guide -match '(?is)do not delete') `
        'the quarantine must remain recoverable'
    Assert-True ($guide -match '(?is)source.{0,120}quarantine parent chain.{0,120}reparse point') `
        'the guide must require checking both filesystem chains for redirection'
    Assert-True ($guide -match '(?is)recheck.{0,120}immediately before moving') `
        'the guide must reduce the manual move TOCTOU window explicitly'
}

Test-Case 'sandbox refresh diagnostic only reads logs and never moves or deletes targets' {
    $diagnosticPath = Join-Path $repoRoot 'rust\tools\sandbox-refresh-diagnostic.ps1'
    Assert-True (Test-Path -LiteralPath $diagnosticPath -PathType Leaf) 'sandbox refresh diagnostic must exist'
    $diagnostic = Get-Content -Raw -LiteralPath $diagnosticPath
    Assert-False ($diagnostic -match '(?im)^\s*(?:Move-Item|Remove-Item|New-Item)\b') `
        'the diagnostic must not contain a top-level filesystem mutation command'
}

Write-TestSummary
