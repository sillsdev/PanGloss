<#
.DESCRIPTION
Resolves a content-keyed native checker, building through pg.ps1 only on a cache miss.
#>
function Get-HygieneInputFingerprint {
    param([Parameter(Mandatory)][string]$RepoRoot)
    $paths = @(
        Get-ChildItem -LiteralPath (Join-Path $RepoRoot 'rust/crates/pg-comment-hygiene') -Recurse -File
        foreach ($relative in @('rust/Cargo.toml', 'rust/Cargo.lock', '.cargo/config.toml', '.cargo/config', 'rust/.cargo/config.toml', 'rust/.cargo/config', 'rust-toolchain', 'rust-toolchain.toml', 'rust/rust-toolchain', 'rust/rust-toolchain.toml')) {
            $path = Join-Path $RepoRoot $relative
            if (Test-Path -LiteralPath $path -PathType Leaf) { Get-Item -LiteralPath $path }
        }
    ) | Sort-Object FullName
    $parts = foreach ($file in $paths) {
        $relative = [IO.Path]::GetRelativePath($RepoRoot, $file.FullName)
        "$relative`n$((Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash)"
    }
    $bytes = [Text.Encoding]::UTF8.GetBytes(($parts -join "`n"))
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
}

function Get-HygieneCachedPath {
    param([string]$RepoRoot, [string]$Fingerprint)
    $name = if ($IsWindows) { 'pg-comment-hygiene.exe' } else { 'pg-comment-hygiene' }
    return Join-Path $RepoRoot ".tmp/comment-hygiene/$Fingerprint/$name"
}

function Resolve-HygieneTool {
    $root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
    $fingerprint = Get-HygieneInputFingerprint -RepoRoot $root
    $binary = Get-HygieneCachedPath -RepoRoot $root -Fingerprint $fingerprint
    if (Test-Path -LiteralPath $binary -PathType Leaf) { return $binary }
    [Console]::Error.WriteLine('[hygiene] native checker missing or stale; bootstrapping through the managed build.')
    Push-Location $root
    try {
        & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'pg.ps1') -Mode build -Package pg-comment-hygiene -DebugProfile -HygieneBootstrap 2>&1 |
            ForEach-Object { [Console]::Error.WriteLine([string]$_) }
        if ($LASTEXITCODE -ne 0) { throw "Native hygiene checker bootstrap failed (exit $LASTEXITCODE)." }
    } finally { Pop-Location }
    $fingerprint = Get-HygieneInputFingerprint -RepoRoot $root
    $binary = Get-HygieneCachedPath -RepoRoot $root -Fingerprint $fingerprint
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'Managed bootstrap did not publish a current native hygiene checker.' }
    return $binary
}

function Get-HygieneArtifactPath {
    param([string[]]$Lines)
    $executables = @($Lines | Where-Object { $_.StartsWith('{') } | ForEach-Object {
        $message = $_ | ConvertFrom-Json
        if ($message.reason -eq 'compiler-artifact' -and $message.target.name -eq 'pg-comment-hygiene' -and $message.target.kind -contains 'bin' -and $message.executable) {
            $message.executable
        }
    } | Select-Object -Unique)
    if ($executables.Count -ne 1) { throw 'Cargo did not report exactly one native hygiene checker executable.' }
    return $executables[0]
}

function Publish-HygieneTool {
    param([string]$RepoRoot, [string]$SourcePath, [string]$Fingerprint)
    if ((Get-HygieneInputFingerprint -RepoRoot $RepoRoot) -ne $Fingerprint) {
        throw 'Hygiene build inputs changed during compilation; rerun bootstrap before using the checker.'
    }
    $destination = Get-HygieneCachedPath -RepoRoot $RepoRoot -Fingerprint $Fingerprint
    if (-not (Test-Path -LiteralPath $SourcePath -PathType Leaf)) { throw "Hygiene bootstrap produced no executable at $SourcePath." }
    New-Item -ItemType Directory -Force (Split-Path $destination) | Out-Null
    $temporary = Join-Path (Split-Path $destination) "$([guid]::NewGuid().ToString('N'))-$([IO.Path]::GetFileName($destination))"
    try {
        Copy-Item -LiteralPath $SourcePath -Destination $temporary
        $stamp = & $temporary --build-fingerprint
        if ($LASTEXITCODE -ne 0 -or $stamp -cne $Fingerprint) { throw 'Built hygiene executable does not match the requested source fingerprint; refusing publication.' }
        if ((Get-HygieneInputFingerprint -RepoRoot $RepoRoot) -ne $Fingerprint) { throw 'Hygiene inputs changed before publication; rerun bootstrap.' }
        [IO.File]::Move($temporary, $destination, $true)
    } finally {
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
    }
}
