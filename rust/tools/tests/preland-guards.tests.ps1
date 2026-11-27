. "$PSScriptRoot\_test-harness.ps1"

$tools = (Resolve-Path "$PSScriptRoot\..").Path
$rust = (Resolve-Path "$tools\..").Path
$cli = Join-Path $rust 'crates/pg-cli'

Test-Case 'preland runs the complete CI source-guard target without a filter' {
    $manifest = Get-Content -Raw (Join-Path $cli 'Cargo.toml')
    Assert-True ($manifest -match '(?s)\[\[test\]\]\s*name = "source_guards"\s*path = "tests/harnesses/source_guards.rs"\s*(?=\[|$)') 'CI must discover an unconditional source_guards target'
    $tokens = $null
    $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $tools 'preland.ps1'), [ref]$tokens, [ref]$errors)
    Assert-Equal 0 $errors.Count 'preland must parse'
    $calls = @($ast.FindAll({ param($node)
        $node -is [Management.Automation.Language.CommandAst] -and
        $node.GetCommandName() -eq 'Invoke-PrelandPg' -and
        $node.Extent.Text.Contains("'source_guards'")
    }, $true))
    Assert-Equal 1 $calls.Count 'preland must invoke the CI source-guard target exactly once'
    Assert-Equal '-PgArgs' $calls[0].CommandElements[1].Extent.Text
    $args = $calls[0].CommandElements[2].SafeGetValue()
    Assert-Equal '-Mode,test,-Package,pg-cli,-TestTarget,source_guards' ($args -join ',') 'preland must execute every source guard without filtering'
    $parent = $calls[0].Parent
    $conditions = @()
    while ($parent) {
        if ($parent -is [Management.Automation.Language.IfStatementAst]) {
            $conditions += @($parent.Clauses | ForEach-Object { $_.Item1.Extent.Text })
        }
        $parent = $parent.Parent
    }
    Assert-Equal '$compiles' ($conditions -join ',') 'only compilation success may gate the source guards'
}

Test-Case 'CI and preland share every registered source-guard module' {
    $harness = Get-Content -Raw (Join-Path $cli 'tests/harnesses/source_guards.rs')
    $modules = @([regex]::Matches($harness, '#\[path = "\.\./([^"/]+\.rs)"\]\s*mod (\w+);'))
    Assert-True ($modules.Count -ge 5) 'the shared target must retain the source and dependency guards'
    $all = Get-Content -Raw (Join-Path $cli 'tests/harnesses/all.rs')
    foreach ($module in $modules) {
        $file = $module.Groups[1].Value
        Assert-True (Test-Path (Join-Path $cli "tests/$file")) "missing guard source: $file"
        Assert-False ($all.Contains("../$file")) "guard duplicated in the general harness: $file"
    }
    Assert-True ($harness.Contains('../fixture_pins_never_self_skip.rs')) 'the guard missed by preland in #336 must run'
}

Write-TestSummary
