. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$pgScript = Get-Content -Raw -LiteralPath (Join-Path $repoRoot 'rust\tools\pg.ps1')

Test-Case 'the local clippy invocation denies warnings over every target and example feature' {
    $inv = @(Get-ClippyInvocation -ExamplePackages @('pg-foma', 'pg-cli'))
    Assert-Equal 'clippy' $inv[0] 'clippy must be the cargo verb'
    Assert-Contains $inv '--all-targets' 'tests and examples must be linted'
    Assert-False ($inv -contains '--workspace') 'an ordinary run must honor Cargo default-members'
    Assert-Contains $inv 'pg-foma/examples,pg-cli/examples' 'every examples feature must be on'
    $sep = [array]::IndexOf($inv, '--')
    Assert-True ($sep -gt 0) 'clippy flags must follow a -- separator'
    Assert-Equal '-D warnings' (($inv[($sep + 1)..($inv.Count - 1)]) -join ' ') 'warnings must be denied, last'
}

Test-Case 'a narrowed clippy run keeps only that package''s examples feature' {
    $inv = @(Get-ClippyInvocation -Package 'pg-cli' -ExamplePackages @('pg-foma', 'pg-cli'))
    Assert-Contains $inv 'pg-cli/examples' 'the narrowed package keeps its examples feature'
    Assert-False ($inv -contains 'pg-foma/examples,pg-cli/examples') 'other packages'' features must be dropped'
    Assert-False ($inv -contains '--workspace') 'a narrowed run must not lint the whole workspace'
    Assert-Equal '-D' $inv[-2] 'extra args must not displace -D warnings from the end'
}

Test-Case 'extra args go before the -- separator so they reach cargo, not clippy' {
    $inv = @(Get-ClippyInvocation -ExtraArgs @('--locked') -ExamplePackages @())
    Assert-True ([array]::IndexOf($inv, '--locked') -lt [array]::IndexOf($inv, '--')) '--locked must precede --'
}

Test-Case 'the local clippy gate matches the clippy command CI runs' {
    $gates = Get-Content -Raw -LiteralPath (Join-Path $repoRoot '.github\workflows\rust-gates.yml')
    $ciLine = [regex]::Match($gates, '(?m)^\s*run:\s*(cargo clippy .+)$').Groups[1].Value
    Assert-True $ciLine 'rust-gates.yml must contain a cargo clippy run line'
    $ciFeatures = [regex]::Match($ciLine, '--features\s+(\S+)').Groups[1].Value -split ',' | Sort-Object
    $localFeatures = @(Get-ExampleFeaturePackages -DefaultOnly | ForEach-Object { "$_/examples" }) | Sort-Object
    Assert-Equal ($ciFeatures -join ',') ($localFeatures -join ',') 'local and CI examples features must agree'
    Assert-True ($ciLine -match '--all-targets') 'CI must lint all targets'
    Assert-True ($ciLine -match '--\s+-D warnings\s*$') 'CI must deny warnings'
}

Test-Case 'check runs clippy and every other cargo mode runs the lint gate first' {
    Assert-True ($pgScript -match "'check'\s*\{\s*#[^\n]*\n\s*\`$cargoArgs = @\(Get-ClippyInvocation") `
        'check mode must build its command from Get-ClippyInvocation'
    Assert-True ($pgScript -match "\`$Mode -in @\('quick', 'build', 'test', 'corpus-test', 'conformance-test', 'release'\)[^\n]*\n\s*\`$lintArgs = @\(Get-ClippyInvocation") `
        'every other cargo mode must set the lint gate'
    Assert-True ($pgScript -match '\$code = \$script:ExitCodeLint') 'a failed lint gate must exit with ExitCodeLint'
}

Test-Case 'the lint gate passes the selected Cargo features and no runner args' {
    $split = Split-CargoFeatureArgs -ExtraArgs @('--features', 'developer-tools', '-F', 'x', '--features=y', '--all-features', '--no-default-features', '--no-capture', '--', '--features', 'late')
    $lint = @(Get-ClippyInvocation -Package pg-cli -ExtraArgs $split.CargoArgs -ExamplePackages @())
    Assert-Equal 'developer-tools' $lint[([array]::IndexOf($lint, '--features') + 1)] 'the selected feature list must reach clippy'
    foreach ($a in @('-F', '--features=y', '--all-features', '--no-default-features')) { Assert-Contains $lint $a "$a must reach clippy" }
    Assert-False ($lint -contains '--no-capture') 'runner flags cannot reach clippy'
    Assert-False ($lint -contains 'late') 'nothing after the libtest separator reaches clippy'
    Assert-True ([array]::IndexOf($lint, 'developer-tools') -lt [array]::IndexOf($lint, '--')) 'features must precede the -- separator'
}

Test-Case 'pg.ps1 feeds the selected features into the pre-build lint gate' {
    Assert-True ($pgScript -match '\$lintArgs = @\(Get-ClippyInvocation [^\n]*-ExtraArgs @\(\(Split-CargoFeatureArgs -ExtraArgs \$ExtraArgs\)\.CargoArgs\)\)') `
        'the lint gate must receive the features selected for the build/test command'
}

Test-Case 'comment hygiene is fatal in every compile mode' {
    Assert-True ($pgScript -match "(?s)Invoke-CommentHygieneReport -ToolRoot \`$PSScriptRoot\)\)\s*\{.{0,300}exit \`$script:ExitCodeCommentHygiene") `
        'a hygiene violation must exit with ExitCodeCommentHygiene before cargo starts'
    Assert-Equal 40 $script:ExitCodeLint 'lint gate exit code'
    Assert-Equal 41 $script:ExitCodeCommentHygiene 'comment hygiene exit code'
}

Write-TestSummary
