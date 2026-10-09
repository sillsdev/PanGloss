$ErrorActionPreference = 'Stop'
$testPath = 'rust/crates/pg-foma-backend/tests/backend_scoreboard_gate.rs'
$saved = @{}
$fixture = Join-Path $PWD 'conformance-staging/edge-cases/strrep-rewrite-unapplication'
$heldFixture = '/tmp/pangloss-lanes/strrep-scoreboard-held-fixture'
$moved = $false
try {
    foreach ($path in @($testPath, 'rust/crates/pg-rules/src/bridge.rs', 'rust/crates/pg-rules/src/rewrite.rs')) {
        $saved[$path] = [IO.File]::ReadAllText((Join-Path $PWD $path))
    }
    $needle = '                    let label = outcome_label(&cell.outcome);'
    $measurement = '                    eprintln!("SCOREBOARD_CELL\t{}\t{:?}\t{}", row.label, cell.strategy, label);'
    [IO.File]::WriteAllText((Join-Path $PWD $testPath), $saved[$testPath].Replace($needle, $needle + "`n" + $measurement))
    pwsh -NoProfile -File rust/tools/pg.ps1 -Mode check -Package pg-foma-backend 2>&1 | Tee-Object /tmp/pangloss-lanes/strrep-scoreboard-cells-check.log
    if ($LASTEXITCODE -ne 0) { throw "Diagnostic check failed: $LASTEXITCODE" }
    pwsh -NoProfile -File rust/tools/pg.ps1 -Mode test -Package pg-foma-backend -TestTarget backend_scoreboard_gate -ExtraArgs '--nocapture' 2>&1 | Tee-Object /tmp/pangloss-lanes/strrep-scoreboard-cells-after.log
    $after = $LASTEXITCODE
    if ($after -ne 101) { throw "Unexpected after result: $after" }
    foreach ($path in @('rust/crates/pg-rules/src/bridge.rs', 'rust/crates/pg-rules/src/rewrite.rs')) {
        $original = (git show "aa59f3d0:$path") -join "`n"
        if ($LASTEXITCODE -ne 0) { throw "Cannot read $path" }
        [IO.File]::WriteAllText((Join-Path $PWD $path), $original + "`n")
    }
    Move-Item $fixture $heldFixture
    $moved = $true
    pwsh -NoProfile -File rust/tools/pg.ps1 -Mode check -Package pg-foma-backend 2>&1 | Tee-Object /tmp/pangloss-lanes/strrep-scoreboard-cells-original-check.log
    if ($LASTEXITCODE -ne 0) { throw "Original check failed: $LASTEXITCODE" }
    pwsh -NoProfile -File rust/tools/pg.ps1 -Mode test -Package pg-foma-backend -TestTarget backend_scoreboard_gate -ExtraArgs '--nocapture' 2>&1 | Tee-Object /tmp/pangloss-lanes/strrep-scoreboard-cells-before.log
    $before = $LASTEXITCODE
    Write-Output "Original inventory test exit=$before; proposed inventory exit=$after."
} finally {
    foreach ($path in $saved.Keys) {
        [IO.File]::WriteAllText((Join-Path $PWD $path), $saved[$path])
    }
    if ($moved) { Move-Item $heldFixture $fixture }
}
exit $before
