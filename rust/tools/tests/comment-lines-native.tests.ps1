<#
.DESCRIPTION
Tests the shared native classification seam used by comment-only verification.
#>
. "$PSScriptRoot/_test-harness.ps1"
. "$PSScriptRoot/../_comment-lines.ps1"

Test-Case 'Rust directives and dereferences remain code while comments are masked' {
    $data = Get-CommentLineData -Extension '.rs' -Lines @('#![allow(dead_code)]', '// note', '*value = 1;', 'let url = "http://host"; // note')
    Assert-True $data.supported
    Assert-Equal 'False,True,False,False' ($data.mask -join ',')
    Assert-Equal 'let url = "http://host";' $data.code[3]
}

Test-Case 'PowerShell block bodies are comments but Requires is a directive' {
    $data = Get-CommentLineData -Extension '.ps1' -Lines @('#Requires -Version 7', '<#', 'body', '#>', 'Write-Host "#text" # note')
    Assert-Equal 'False,True,True,True,False' ($data.mask -join ',')
    Assert-Equal 'Write-Host "#text"' $data.code[4]
}

Test-Case 'unknown syntax and empty files are explicit' {
    $data = Get-CommentLineData -Extension '.xml' -Lines @('<!-- text -->')
    Assert-False $data.supported
    $empty = Get-CommentLineData -Extension '.rs' -Lines @()
    Assert-Equal 0 $empty.mask.Count
    Assert-Equal 0 $empty.code.Count
    $absent = Get-CommentLineData -Extension '.rs' -Lines $null
    Assert-Equal 0 $absent.mask.Count
}

Write-TestSummary
