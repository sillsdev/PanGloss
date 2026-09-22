. "$PSScriptRoot\_test-harness.ps1"

$script:HelperPath = Join-Path $PSScriptRoot '..\_rustfmt-tool.ps1'

function New-RustFmtFixture {
    $repo = New-TestTempDir -Prefix 'pg-rustfmt-cache'
    $rust = Join-Path $repo 'rust'
    New-Item -ItemType Directory -Force (Join-Path $rust 'src') | Out-Null
    Set-Content -LiteralPath (Join-Path $rust 'Cargo.toml') -Value "[package]`nname = `"fixture`"`nversion = `"0.1.0`"" -Encoding UTF8
    Set-Content -LiteralPath (Join-Path $rust 'src/lib.rs') -Value 'pub fn answer() -> i32 { 42 }' -Encoding UTF8
    [PSCustomObject]@{ Repo = $repo; Rust = $rust; Source = (Join-Path $rust 'src/lib.rs') }
}

Test-Case 'an exact successful rustfmt input set is checked once and then reused' {
    . $script:HelperPath
    $fixture = New-RustFmtFixture
    $script:checks = 0
    $check = { param($Manifest, $Packages) $script:checks++; [PSCustomObject]@{ ExitCode = 0; Output = @() } }
    $first = Invoke-RustFmtCached -RepoRoot $fixture.Repo -RustRoot $fixture.Rust -ToolIdentity 'rustfmt-test-v1' -Head 'abc' -ChangedPaths @() -CheckAction $check
    $second = Invoke-RustFmtCached -RepoRoot $fixture.Repo -RustRoot $fixture.Rust -ToolIdentity 'rustfmt-test-v1' -Head 'abc' -ChangedPaths @() -CheckAction $check
    Assert-Equal 'Clean' $first.Status
    Assert-Equal 'Cached' $second.Status
    Assert-Equal 1 $script:checks 'an unchanged successful input set must not launch cargo fmt twice'
}

Test-Case 'source content changes invalidate the clean result even when timestamps are restored' {
    . $script:HelperPath
    $fixture = New-RustFmtFixture
    $stamp = (Get-Item -LiteralPath $fixture.Source).LastWriteTimeUtc
    $script:checks = 0
    $script:packages = @()
    $check = { param($Manifest, $Packages) $script:checks++; $script:packages = @($Packages); [PSCustomObject]@{ ExitCode = 0; Output = @() } }
    [void](Invoke-RustFmtCached -RepoRoot $fixture.Repo -RustRoot $fixture.Rust -ToolIdentity 'rustfmt-test-v1' -Head 'abc' -ChangedPaths @() -CheckAction $check)
    Set-Content -LiteralPath $fixture.Source -Value 'pub fn answer() -> i32 { 43 }' -Encoding UTF8
    (Get-Item -LiteralPath $fixture.Source).LastWriteTimeUtc = $stamp
    [void](Invoke-RustFmtCached -RepoRoot $fixture.Repo -RustRoot $fixture.Rust -ToolIdentity 'rustfmt-test-v1' -Head 'abc' -ChangedPaths @('rust/src/lib.rs') -CheckAction $check)
    Assert-Equal 2 $script:checks 'content, not mtime, owns invalidation'
    Assert-Equal 'fixture' ($script:packages -join ',') 'a changed Rust file formats only its owning package after the clean baseline'
}

Test-Case 'a failed rustfmt check never publishes a clean marker' {
    . $script:HelperPath
    $fixture = New-RustFmtFixture
    $script:checks = 0
    $check = { param($Manifest, $Packages) $script:checks++; [PSCustomObject]@{ ExitCode = 2; Output = @('formatter failed') } }
    $first = Invoke-RustFmtCached -RepoRoot $fixture.Repo -RustRoot $fixture.Rust -ToolIdentity 'rustfmt-test-v1' -Head 'abc' -ChangedPaths @() -CheckAction $check
    $second = Invoke-RustFmtCached -RepoRoot $fixture.Repo -RustRoot $fixture.Rust -ToolIdentity 'rustfmt-test-v1' -Head 'abc' -ChangedPaths @() -CheckAction $check
    Assert-Equal 'Failed' $first.Status
    Assert-Equal 'Failed' $second.Status
    Assert-Equal 2 $script:checks 'a tool failure must be retried, never read as clean'
}

Test-Case 'a successful check with concurrent input changes publishes no clean result' {
    . $script:HelperPath
    $fixture = New-RustFmtFixture
    $original = Get-Content -Raw -LiteralPath $fixture.Source
    $script:checks = 0
    $check = {
        param($Manifest, $Packages)
        $script:checks++
        if ($script:checks -eq 1) {
            Set-Content -LiteralPath $fixture.Source -Value 'pub fn answer() -> i32 { 43 }' -Encoding UTF8
        }
        [PSCustomObject]@{ ExitCode = 0; Output = @() }
    }
    $first = Invoke-RustFmtCached -RepoRoot $fixture.Repo -RustRoot $fixture.Rust -ToolIdentity 'rustfmt-test-v1' -Head 'abc' -ChangedPaths @() -CheckAction $check
    Set-Content -LiteralPath $fixture.Source -Value $original -Encoding UTF8
    $second = Invoke-RustFmtCached -RepoRoot $fixture.Repo -RustRoot $fixture.Rust -ToolIdentity 'rustfmt-test-v1' -Head 'abc' -ChangedPaths @() -CheckAction $check
    Assert-Equal 'ChangedDuringCheck' $first.Status 'a concurrent input change must not publish a clean result'
    Assert-Equal 'Clean' $second.Status 'restoring the input must require a fresh successful check'
    Assert-Equal 2 $script:checks 'a concurrent input change must not be treated as cached clean'
}

Test-Case 'a successful apply publishes only the post-format content fingerprint' {
    . $script:HelperPath
    $fixture = New-RustFmtFixture
    $script:checks = 0
    $script:applies = 0
    $check = {
        param($Manifest, $Packages)
        $script:checks++
        if ((Get-Content -Raw -LiteralPath $fixture.Source) -match '  42') {
            return [PSCustomObject]@{ ExitCode = 1; Output = @('Diff in src/lib.rs:1:') }
        }
        [PSCustomObject]@{ ExitCode = 0; Output = @() }
    }
    $apply = {
        param($Manifest, $Packages)
        $script:applies++
        Set-Content -LiteralPath $fixture.Source -Value 'pub fn answer() -> i32 { 42 }' -Encoding UTF8
        [PSCustomObject]@{ ExitCode = 0; Output = @() }
    }
    Set-Content -LiteralPath $fixture.Source -Value 'pub fn answer() -> i32 {  42 }' -Encoding UTF8
    $first = Invoke-RustFmtCached -RepoRoot $fixture.Repo -RustRoot $fixture.Rust -ToolIdentity 'rustfmt-test-v1' -Head 'abc' -ChangedPaths @() -CheckAction $check -ApplyAction $apply
    $second = Invoke-RustFmtCached -RepoRoot $fixture.Repo -RustRoot $fixture.Rust -ToolIdentity 'rustfmt-test-v1' -Head 'abc' -ChangedPaths @() -CheckAction $check -ApplyAction $apply
    Assert-Equal 'Applied' $first.Status
    Assert-Equal 'Cached' $second.Status
    Assert-Equal 1 $script:applies
    Assert-Equal 2 $script:checks 'an applied result must be verified before it is cached'
}

Test-Case 'a successful apply that leaves unformatted content is never cached' {
    . $script:HelperPath
    $fixture = New-RustFmtFixture
    $script:checks = 0
    $script:applies = 0
    $check = {
        param($Manifest, $Packages)
        $script:checks++
        if ((Get-Content -Raw -LiteralPath $fixture.Source) -match '  42') {
            return [PSCustomObject]@{ ExitCode = 1; Output = @('Diff in src/lib.rs:1:') }
        }
        [PSCustomObject]@{ ExitCode = 0; Output = @() }
    }
    $apply = {
        param($Manifest, $Packages)
        $script:applies++
        Set-Content -LiteralPath $fixture.Source -Value 'pub fn answer() -> i32 {  42 }' -Encoding UTF8
        [PSCustomObject]@{ ExitCode = 0; Output = @() }
    }
    Set-Content -LiteralPath $fixture.Source -Value 'pub fn answer() -> i32 {   42 }' -Encoding UTF8
    $first = Invoke-RustFmtCached -RepoRoot $fixture.Repo -RustRoot $fixture.Rust -ToolIdentity 'rustfmt-test-v1' -Head 'abc' -ChangedPaths @() -CheckAction $check -ApplyAction $apply
    $second = Invoke-RustFmtCached -RepoRoot $fixture.Repo -RustRoot $fixture.Rust -ToolIdentity 'rustfmt-test-v1' -Head 'abc' -ChangedPaths @() -CheckAction $check -ApplyAction $apply
    Assert-Equal 'Failed' $first.Status 'a formatter that leaves a diff must not publish clean'
    Assert-Equal 'Failed' $second.Status 'the next invocation must retry an uncached failed apply'
    Assert-Equal 2 $script:applies 'an uncached failed apply must run again'
    Assert-Equal 4 $script:checks 'each apply must include its post-apply verification check'
}

Test-Case 'a changed manifest falls back to the full workspace formatter' {
    . $script:HelperPath
    $fixture = New-RustFmtFixture
    $script:packages = @('not-run')
    $check = { param($Manifest, $Packages) $script:packages = @($Packages); [PSCustomObject]@{ ExitCode = 0; Output = @() } }
    [void](Invoke-RustFmtCached -RepoRoot $fixture.Repo -RustRoot $fixture.Rust -ToolIdentity 'rustfmt-test-v1' -Head 'abc' -ChangedPaths @() -CheckAction $check)
    Set-Content -LiteralPath (Join-Path $fixture.Rust 'Cargo.toml') -Value "[package]`nname = `"fixture`"`nversion = `"0.2.0`"" -Encoding UTF8
    [void](Invoke-RustFmtCached -RepoRoot $fixture.Repo -RustRoot $fixture.Rust -ToolIdentity 'rustfmt-test-v1' -Head 'abc' -ChangedPaths @('rust/Cargo.toml') -CheckAction $check)
    Assert-Equal 0 $script:packages.Count 'manifest changes must use --all because workspace/package ownership may have changed'
}

Write-TestSummary
