<#
.DESCRIPTION
Runs the native comment-hygiene checker. Findings exit 1; tool errors exit 2.
The first invocation builds the isolated tooling crate through pg.ps1; unchanged inputs
reuse its executable. Local managed builds warn; direct and CI callers must honor the exit code.
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [switch]$List,
    [int]$ListLimit = 400,
    [string]$RepoRoot = (Join-Path $PSScriptRoot '../..'),
    [switch]$Json
)
$ErrorActionPreference = 'Stop'
try {
    . (Join-Path $PSScriptRoot '_comment-hygiene-tool.ps1')
    $binary = Resolve-HygieneTool
    $arguments = @('--repo-root', $RepoRoot, '--list-limit', "$ListLimit")
    if ($List) { $arguments += '--list' }
    if ($Json) { $arguments += '--json' }
    & $binary @arguments
    exit $LASTEXITCODE
} catch {
    [Console]::Error.WriteLine("comment-hygiene: $_")
    exit 2
}
