$ErrorActionPreference = 'Stop'
$mode = if ($env:DROVER_CI_KIND -eq 'checks') { 'quick' } else { 'test' }
& pwsh -NoProfile -File rust/tools/pg.ps1 -Mode $mode
exit $LASTEXITCODE
