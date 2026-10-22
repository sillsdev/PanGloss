. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"
$path = Join-Path $PSScriptRoot '..\oracle-conformance.ps1'
$tokens = $null; $parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$parseErrors)
Assert-Equal 0 $parseErrors.Count 'oracle script must parse'
# Load actual function owners, excluding the command-line main and external process adapter.
foreach ($node in $ast.FindAll({param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst]}, $false)) {
    . ([scriptblock]::Create($node.Extent.Text))
}
function Invoke-OracleSelfCheck { param($ExePath, $FixturesRoot, [switch]$IncludePathological, [switch]$Propose) $script:OracleRun }
function Test-DeclaredForwardSynthesis { param($FixturesRoot, $Fixture) $script:ForwardDeclared }
function New-FilterPassesOracleRoot { param($FilterPassesRoot, $DestRoot) 'example' }
$script:ForwardDeclared = $false
function Set-OracleRun { param([string]$Output, [int]$ExitCode) $script:OracleRun = [pscustomobject]@{Output=$Output;ExitCode=$ExitCode} }
function Check-Run { param([hashtable]$Baseline=@{}, [switch]$Clean) Test-SelfCheckRun -ExePath ignored -FixturesRoot ignored -RootLabel test -Baseline $Baseline -ExpectCleanBaseline:$Clean }

Test-Case 'unrecognized crash output never becomes a passing zero-denominator run' {
    Set-OracleRun 'oracle crashed before writing its report' 1
    Assert-True ($null -eq (Check-Run)) 'unavailable evidence must return null'
}
Test-Case 'recognized rows without complete summary do not constitute a completed run' {
    Set-OracleRun '[PASS] edge-cases/example (1ms)' 0
    Assert-True ($null -eq (Check-Run)) 'truncated report must fail closed'
}
Test-Case 'all-skipped and inconsistent exit evidence refuse' {
    Set-OracleRun "discovered 1 fixture(s) under 'test'`n[SKIP] edge-cases/example (1ms) none`ntotals: 0 passed, 0 failed, 1 skipped (of 1 attempted)" 0
    Assert-True ($null -eq (Check-Run)) 'all skipped proves nothing'
    Set-OracleRun "discovered 1 fixture(s) under 'test'`n[PASS] edge-cases/example (1ms)`ntotals: 1 passed, 0 failed, 0 skipped (of 1 attempted)" 1
    Assert-True ($null -eq (Check-Run)) 'exit failure with only passing rows must refuse'
}
Test-Case 'missing or duplicated rows cannot shrink the reported denominator' {
    Set-OracleRun "discovered 2 fixture(s) under 'test'`n[PASS] edge-cases/example (1ms)`ntotals: 1 passed, 0 failed, 0 skipped (of 1 attempted)" 0
    Assert-True ($null -eq (Check-Run)) 'discovery and attempt denominators must reconcile'
    Set-OracleRun "discovered 2 fixture(s) under 'test'`n[PASS] edge-cases/example (1ms)`n[PASS] edge-cases/example (2ms)`ntotals: 2 passed, 0 failed, 0 skipped (of 2 attempted)" 0
    Assert-True ($null -eq (Check-Run)) 'duplicate fixture IDs are invalid'
}
Test-Case 'waivers match exact reviewed evidence, not entire fixtures' {
    $known = @{ 'edge-cases/example'=[pscustomobject]@{kind='rules-attribution-only';reason='known attribution mismatch'} }
    Set-OracleRun "discovered 1 fixture(s) under 'test'`n[FAIL] edge-cases/example (1ms) NEW signature mismatch`ntotals: 0 passed, 1 failed, 0 skipped (of 1 attempted)" 1
    Assert-False (Check-Run -Baseline $known) 'a different failure on the same fixture must gate'
    Set-OracleRun "discovered 1 fixture(s) under 'test'`n[FAIL] edge-cases/example (1ms) known attribution mismatch`ntotals: 0 passed, 1 failed, 0 skipped (of 1 attempted)" 1
    Assert-True (Check-Run -Baseline $known) 'reviewed exact failure remains a reported waiver'
    Assert-False (Check-Run -Baseline $known -Clean) 'upstream control never accepts attribution waivers'
}
Test-Case 'forward-synthesis declaration does not waive unrelated failures' {
    $script:ForwardDeclared=$true
    Set-OracleRun "discovered 1 fixture(s) under 'test'`n[FAIL] edge-cases/example (1ms) unexpected negative-control mismatch`ntotals: 0 passed, 1 failed, 0 skipped (of 1 attempted)" 1
    Assert-False (Check-Run) 'marker without reviewed exact evidence is insufficient'
    $script:ForwardDeclared=$false
}
Test-Case 'filter mirror shares empty-evidence and exact-waiver policy' {
    Set-OracleRun 'crash' 1
    Assert-True ($null -eq (Test-FilterPassesSelfCheckRun -ExePath ignored -FilterPassesRoot ignored -TempRoot ignored -Baseline @{})) 'mirror cannot silently pass'
    Set-OracleRun "discovered 1 fixture(s) under 'test'`n[FAIL] edge-cases/example (1ms) changed failure`ntotals: 0 passed, 1 failed, 0 skipped (of 1 attempted)" 1
    $known = @{ 'filter-passes/example'=[pscustomobject]@{kind='known';reason='old failure'} }
    Assert-False (Test-FilterPassesSelfCheckRun -ExePath ignored -FilterPassesRoot ignored -TempRoot ignored -Baseline $known) 'mirror maps ID but never expands waiver'
}
Test-Case 'complete passing evidence and declared pathological exclusion remain supported' {
    Set-OracleRun "discovered 2 fixture(s) under 'test'`n1 pathological (budget_ms) fixture(s) excluded by default (--include-pathological to run)`n[PASS] edge-cases/example (1ms)`ntotals: 1 passed, 0 failed, 0 skipped (of 1 attempted)" 0
    Assert-True (Check-Run) 'explicit exclusions reconcile discovery denominator'
}
Test-Case 'run totals reconcile discovered fixtures and preserve distinct named-run IDs' {
    Set-OracleRun "discovered 2 fixture(s) under 'test'`n[PASS] edge-cases/example (1ms)`n[PASS] edge-cases/example [variant strict] (2ms)`n[PASS] edge-cases/other (3ms)`ntotals: 3 runs passed, 0 failed, 0 skipped (of 3 runs attempted across 2 fixtures)" 0
    Assert-True (Check-Run) 'all run rows reconcile with the run and fixture totals'
}

Test-Case 'run totals reconcile the current default-only pathological exclusions' {
    Set-OracleRun "discovered 3 fixture(s) under 'test'`n1 pathological run(s) (budget_ms) excluded by default (--include-pathological to run)`n[PASS] edge-cases/example (1ms)`n[PASS] edge-cases/example [variant strict] (2ms)`n[PASS] edge-cases/other (3ms)`ntotals: 3 runs passed, 0 failed, 0 skipped (of 3 runs attempted across 3 fixtures)" 0
    Assert-True (Check-Run) 'observed base fixtures plus default-only exclusions reconcile discovery'
}

Test-Case 'run totals refuse ambiguous pathological exclusions' {
    Set-OracleRun "discovered 2 fixture(s) under 'test'`n2 pathological run(s) (budget_ms) excluded by default (--include-pathological to run)`n[PASS] edge-cases/example (1ms)`n[PASS] edge-cases/example [variant strict] (2ms)`ntotals: 2 runs passed, 0 failed, 0 skipped (of 2 runs attempted across 2 fixtures)" 0
    Assert-True ($null -eq (Check-Run)) 'excluded run counts must reconcile one-for-one with absent base fixture IDs'
}

Test-Case 'run totals reject all-skipped and mismatched fixture counts' {
    Set-OracleRun "discovered 1 fixture(s) under 'test'`n[SKIP] edge-cases/example (1ms) no visible rows`ntotals: 0 runs passed, 0 failed, 1 skipped (of 1 runs attempted across 1 fixtures)" 0
    Assert-True ($null -eq (Check-Run)) 'all-skipped runs prove no completed result'
    Set-OracleRun "discovered 2 fixture(s) under 'test'`n[PASS] edge-cases/example (1ms)`ntotals: 1 runs passed, 0 failed, 0 skipped (of 1 runs attempted across 1 fixtures)" 0
    Assert-True ($null -eq (Check-Run)) 'reported fixture count must match actual discovery'
}
Test-Case 'named variant failures do not inherit a base fixture waiver' {
    $known = @{ 'edge-cases/example'=[pscustomobject]@{kind='known';reason='old failure'} }
    Set-OracleRun "discovered 1 fixture(s) under 'test'`n[PASS] edge-cases/example (1ms)`n[FAIL] edge-cases/example [variant strict] (2ms) old failure`ntotals: 1 runs passed, 1 failed, 0 skipped (of 2 runs attempted across 1 fixtures)" 1
    Assert-False (Check-Run -Baseline $known) 'a variant failure is not the base fixture failure'
}

Test-Case 'named variant rows cannot replace the default fixture run' {
    Set-OracleRun "discovered 1 fixture(s) under 'test'`n[PASS] edge-cases/example [variant strict] (1ms)`ntotals: 1 runs passed, 0 failed, 0 skipped (of 1 runs attempted across 1 fixtures)" 0
    Assert-True ($null -eq (Check-Run)) 'each observed fixture needs its default row as well as variant rows'
}
Write-TestSummary
