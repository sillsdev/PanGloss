<#
.DESCRIPTION
Shares the native checker's classification with comment-only verification, once per file side.
#>
. (Join-Path $PSScriptRoot '_comment-hygiene-tool.ps1')

function Get-CommentLineData {
    param([AllowEmptyCollection()][string[]]$Lines, [string]$Extension)
    if ($null -eq $Lines) { $Lines = @() }
    $binary = Resolve-HygieneTool
    $request = @{ extension = $Extension; lines = @($Lines) } | ConvertTo-Json -Compress
    $output = $request | & $binary --classify-json
    if ($LASTEXITCODE -ne 0) { throw "Native comment classification failed (exit $LASTEXITCODE)." }
    $data = $output | ConvertFrom-Json
    if ($null -eq $data.supported -or $data.mask.Count -ne $Lines.Count -or $data.code.Count -ne $Lines.Count) {
        throw 'Native comment classification returned an incomplete file result.'
    }
    return $data
}
