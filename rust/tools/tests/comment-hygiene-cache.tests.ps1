<#
.DESCRIPTION
Checks content-based freshness of the native hygiene tool cache.
#>
. "$PSScriptRoot/_test-harness.ps1"
$helper = Join-Path $PSScriptRoot '../_comment-hygiene-tool.ps1'
if (Test-Path $helper) { . $helper }

Test-Case 'source content, not timestamps, determines the tool cache key' {
    $root = New-TestTempDir -Prefix 'pg-hygiene-cache'
    try {
        $src = Join-Path $root 'rust/crates/pg-comment-hygiene/src'
        New-Item -ItemType Directory -Force $src | Out-Null
        Set-Content (Join-Path $root 'rust/Cargo.toml') '[workspace]'
        Set-Content (Join-Path $root 'rust/Cargo.lock') 'version = 4'
        $file = Join-Path $src 'lib.rs'
        Set-Content $file 'pub fn first() {}'
        $before = Get-HygieneInputFingerprint -RepoRoot $root
        $stamp = (Get-Item $file).LastWriteTimeUtc
        Set-Content $file 'pub fn other() {}'
        (Get-Item $file).LastWriteTimeUtc = $stamp
        $after = Get-HygieneInputFingerprint -RepoRoot $root
        Assert-True ($before -ne $after) 'same-size edit with restored timestamp must invalidate the cache'
        Set-Content (Join-Path $src 'extra.rs') 'pub fn extra() {}'
        Assert-True ($after -ne (Get-HygieneInputFingerprint -RepoRoot $root)) 'new inputs must invalidate cache'
    } finally {
        Remove-Item -LiteralPath $root -Recurse -Force
    }
}

Test-Case 'unchanged inputs use the same cache key' {
    $repo = (Resolve-Path "$PSScriptRoot/../../..").Path
    Assert-Equal (Get-HygieneInputFingerprint -RepoRoot $repo) (Get-HygieneInputFingerprint -RepoRoot $repo)
}

Test-Case 'artifact discovery honors Cargo executable paths, including configured target triples' {
    $lines = @('managed launcher banner', '{"reason":"compiler-artifact","target":{"name":"pg-comment-hygiene","kind":["lib"]},"executable":null}', '{"reason":"compiler-artifact","target":{"name":"pg-comment-hygiene","kind":["bin"]},"executable":"D:/configured/triple/debug/pg-comment-hygiene.exe"}')
    Assert-Equal 'D:/configured/triple/debug/pg-comment-hygiene.exe' (Get-HygieneArtifactPath -Lines $lines)
}

Test-Case 'successful Cargo without a checker executable cannot publish a tool' {
    $refused = $false
    try { Get-HygieneArtifactPath -Lines @('{"reason":"build-finished","success":true}') } catch { $refused = $true }
    Assert-True $refused
}

Test-Case 'bootstrap refuses test selectors before compiling' {
    $pg = Join-Path $PSScriptRoot '../pg.ps1'
    foreach ($selector in @('-TestTarget', '-Filter')) {
        $out = & pwsh -NoProfile -File $pg -Mode build -Package pg-comment-hygiene -DebugProfile -HygieneBootstrap $selector unwanted 2>&1 | Out-String
        Assert-Equal 2 $LASTEXITCODE "bootstrap must refuse $selector before Cargo: $out"
    }
}

Test-Case 'an executable with another source stamp is never published' {
    $binary = Resolve-HygieneTool
    $root = New-TestTempDir -Prefix 'pg-hygiene-wrong-stamp'
    try {
        $src = Join-Path $root 'rust/crates/pg-comment-hygiene/src'
        New-Item -ItemType Directory -Force $src | Out-Null
        Set-Content (Join-Path $src 'lib.rs') 'fixture source distinct from the real checker'
        $key = Get-HygieneInputFingerprint -RepoRoot $root
        $refused = $false
        try { Publish-HygieneTool -RepoRoot $root -SourcePath $binary -Fingerprint $key } catch {
            $refused = $_ -match 'does not match the requested source fingerprint'
        }
        Assert-True $refused 'wrong executable stamp must refuse publication explicitly'
        Assert-False (Test-Path (Get-HygieneCachedPath -RepoRoot $root -Fingerprint $key))
        Assert-Equal 0 @(Get-ChildItem $root -Recurse -File | Where-Object Name -Like '*pg-comment-hygiene*').Count
    } finally {
        Remove-Item -LiteralPath $root -Recurse -Force
    }
}

Test-Case 'a locked bootstrap capture cannot overturn a successful build' {
    $pgText = Get-Content -Raw (Join-Path $PSScriptRoot '../pg.ps1')
    Assert-True ($pgText -match 'Remove-Item -LiteralPath \$capturePath -Force -ErrorAction SilentlyContinue') `
        'temporary capture cleanup must tolerate the Start-Process redirect-handle race'
}

Test-Case 'native bootstrap isolates outer Cargo arguments and restores caller state' {
    $fixture = New-TestTempDir -Prefix 'pg-hygiene-env'
    $fixtureBinary = Join-Path $fixture 'isolated-checker.exe'
    $observations = [Collections.Generic.List[object]]::new()
    $hadPrevious = Test-Path Env:PANGLOSS_EXTRA_ARGS
    $previous = $env:PANGLOSS_EXTRA_ARGS
    $callerLocation = (Get-Location).Path
    function Get-HygieneInputFingerprint { param($RepoRoot) return 'isolated-fixture' }
    function Get-HygieneCachedPath { param($RepoRoot, $Fingerprint) return $fixtureBinary }
    function pwsh {
        $observations.Add([pscustomobject]@{ Present = (Test-Path Env:PANGLOSS_EXTRA_ARGS); Value = $env:PANGLOSS_EXTRA_ARGS })
        if ($childBehavior -eq 'throw') { throw 'fixture child threw' }
        if ($childBehavior -eq 'success') {
            New-Item -ItemType File -Path $fixtureBinary -Force | Out-Null
            $global:LASTEXITCODE = 0
        } else { $global:LASTEXITCODE = 37 }
    }
    try {
        foreach ($childBehavior in @('success', 'nonzero', 'throw')) {
            if (Test-Path -LiteralPath $fixtureBinary) { Remove-Item -LiteralPath $fixtureBinary -Force }
            $env:PANGLOSS_EXTRA_ARGS = '--target wasm32-unknown-unknown'
            $before = $observations.Count
            $errorText = ''
            try { $resolved = Resolve-HygieneTool } catch { $errorText = "$_" }
            Assert-Equal ($before + 1) $observations.Count 'a cold cache must invoke the child'
            Assert-False $observations[$before].Present 'native bootstrap must not inherit outer Cargo arguments'
            Assert-Equal '--target wasm32-unknown-unknown' $env:PANGLOSS_EXTRA_ARGS 'caller arguments must be restored'
            Assert-Equal $callerLocation (Get-Location).Path 'caller directory must be restored'
            if ($childBehavior -eq 'success') {
                Assert-Equal '' $errorText
                Assert-Equal $fixtureBinary $resolved
            } elseif ($childBehavior -eq 'nonzero') {
                Assert-True ($errorText -match 'bootstrap failed \(exit 37\)') 'nonzero child failure must propagate'
            } else {
                Assert-True ($errorText -match 'fixture child threw') 'thrown child failure must propagate'
            }
        }
        New-Item -ItemType File -Path $fixtureBinary -Force | Out-Null
        $before = $observations.Count
        Assert-Equal $fixtureBinary (Resolve-HygieneTool)
        Assert-Equal $before $observations.Count 'cache hit must not invoke a child'
        Assert-Equal '--target wasm32-unknown-unknown' $env:PANGLOSS_EXTRA_ARGS 'cache hit must preserve caller arguments'
        Remove-Item -LiteralPath $fixtureBinary -Force
        Remove-Item Env:PANGLOSS_EXTRA_ARGS -ErrorAction SilentlyContinue
        $childBehavior = 'success'
        Assert-Equal $fixtureBinary (Resolve-HygieneTool)
        Assert-False (Test-Path Env:PANGLOSS_EXTRA_ARGS) 'an absent caller variable must stay absent'
    } finally {
        if ($hadPrevious) { [Environment]::SetEnvironmentVariable('PANGLOSS_EXTRA_ARGS', $previous, 'Process') }
        else { Remove-Item Env:PANGLOSS_EXTRA_ARGS -ErrorAction SilentlyContinue }
        if (Test-Path -LiteralPath $fixtureBinary) { Remove-Item -LiteralPath $fixtureBinary -Force }
        Remove-Item -LiteralPath $fixture -Force
    }
}

Write-TestSummary
