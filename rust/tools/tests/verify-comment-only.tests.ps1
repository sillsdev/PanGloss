<#
.DESCRIPTION
Exercises native classification through the real git-diff verifier in an isolated repository.
#>
. "$PSScriptRoot/_test-harness.ps1"
$verifier = (Resolve-Path "$PSScriptRoot/../verify-comment-only.ps1").Path

function Invoke-CommentDiffFixture {
    param([string]$Before, [AllowEmptyString()][string]$After)
    $root = New-TestTempDir -Prefix 'pg-comment-diff'
    Push-Location $root
    try {
        & git init --quiet
        $blob = $Before | & git hash-object -w --stdin
        $info = [Diagnostics.ProcessStartInfo]::new('git')
        $info.ArgumentList.Add('mktree')
        $info.WorkingDirectory = $root
        $info.UseShellExecute = $false
        $info.CreateNoWindow = $true
        $info.RedirectStandardInput = $true
        $info.RedirectStandardOutput = $true
        $process = [Diagnostics.Process]::Start($info)
        $process.StandardInput.Write("100644 blob $blob`tfixture.rs`n")
        $process.StandardInput.Close()
        $tree = $process.StandardOutput.ReadToEnd().Trim()
        $process.WaitForExit()
        if ($process.ExitCode -ne 0) { throw 'Fixture git mktree failed.' }
        $process.Dispose()
        $commit = & git -c user.name=Fixture -c user.email=fixture@example.invalid commit-tree $tree -m fixture
        & git update-ref HEAD $commit
        & git read-tree $tree
        [IO.File]::WriteAllText((Join-Path $root 'fixture.rs'), $After)
        $out = & pwsh -NoProfile -File $verifier -Path fixture.rs 2>&1 | Out-String
        return @{ Code = $LASTEXITCODE; Output = $out }
    } finally {
        Pop-Location
        Remove-Item -LiteralPath $root -Recurse -Force
    }
}

Test-Case 'editing only a leading and trailing comment passes' {
    $result = Invoke-CommentDiffFixture -Before "// old`nlet url = `"http://host`"; // old`n" -After "// new`nlet url = `"http://host`"; // new`n"
    Assert-Equal 0 $result.Code $result.Output
    Assert-True ($result.Output -match 'PASS: every added and removed line')
}

Test-Case 'changing code beside a trailing comment is rejected' {
    $result = Invoke-CommentDiffFixture -Before "let value = 1; // note`n" -After "let value = 2; // note`n"
    Assert-Equal 1 $result.Code $result.Output
    Assert-True ($result.Output -match 'REMOVED')
}

Test-Case 'deleting code among removed comments is rejected' {
    $result = Invoke-CommentDiffFixture -Before "// first`n// second`nuse std::fmt;`n" -After ''
    Assert-Equal 1 $result.Code $result.Output
    Assert-True ($result.Output -match 'use std::fmt;')
}

Write-TestSummary
