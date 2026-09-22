$ErrorActionPreference = 'Stop'

function Get-RustFmtInputFiles {
    param([Parameter(Mandatory)][string]$RepoRoot, [Parameter(Mandatory)][string]$RustRoot)
    $files = @(Get-ChildItem -LiteralPath $RustRoot -Recurse -File | Where-Object {
        $_.Extension -eq '.rs' -or $_.Name -eq 'Cargo.toml' -or $_.Name -in @('rustfmt.toml', '.rustfmt.toml')
    })
    foreach ($relative in @('rust-toolchain', 'rust-toolchain.toml', 'rustfmt.toml', '.rustfmt.toml', '.cargo/config', '.cargo/config.toml', 'rust/.cargo/config', 'rust/.cargo/config.toml')) {
        $candidate = Join-Path $RepoRoot $relative
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { $files += Get-Item -LiteralPath $candidate }
    }
    @($files | Sort-Object FullName -Unique)
}

function Get-RustFmtInputFingerprint {
    param(
        [Parameter(Mandatory)][string]$RepoRoot,
        [Parameter(Mandatory)][string]$RustRoot,
        [Parameter(Mandatory)][string]$ToolIdentity
    )
    $builder = [Text.StringBuilder]::new()
    [void]$builder.AppendLine('pangloss-rustfmt-cache-v1')
    [void]$builder.AppendLine($ToolIdentity)
    foreach ($file in @(Get-RustFmtInputFiles -RepoRoot $RepoRoot -RustRoot $RustRoot)) {
        $relative = [IO.Path]::GetRelativePath($RepoRoot, $file.FullName).Replace('\', '/')
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $file.FullName).Hash
        [void]$builder.AppendLine("$relative`0$hash")
    }
    $bytes = [Text.Encoding]::UTF8.GetBytes($builder.ToString())
    ([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes))).ToLowerInvariant()
}

function Get-RustFmtMarkerPath {
    param([string]$RepoRoot, [string]$Kind, [string]$Key)
    Join-Path $RepoRoot ".tmp/rustfmt/$Kind/$Key.ok"
}

function Publish-RustFmtMarker {
    param([Parameter(Mandatory)][string]$Path)
    $directory = Split-Path $Path
    New-Item -ItemType Directory -Force -Path $directory | Out-Null
    $temporary = Join-Path $directory "$([guid]::NewGuid().ToString('N')).tmp"
    try {
        [IO.File]::WriteAllText($temporary, 'clean')
        [IO.File]::Move($temporary, $Path, $true)
    } finally {
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
    }
}

function Get-RustFmtPackageForPath {
    param([Parameter(Mandatory)][string]$RepoRoot, [Parameter(Mandatory)][string]$RustRoot, [Parameter(Mandatory)][string]$RelativePath)
    $candidate = Join-Path $RepoRoot $RelativePath
    $directory = if (Test-Path -LiteralPath $candidate -PathType Container) { $candidate } else { Split-Path $candidate }
    $rustFull = [IO.Path]::GetFullPath($RustRoot).TrimEnd('\', '/')
    while ($directory -and [IO.Path]::GetFullPath($directory).StartsWith($rustFull, [StringComparison]::OrdinalIgnoreCase)) {
        $manifest = Join-Path $directory 'Cargo.toml'
        if (Test-Path -LiteralPath $manifest -PathType Leaf) {
            $text = Get-Content -Raw -LiteralPath $manifest
            $packageSection = [regex]::Match($text, '(?ms)^\[package\]\s*(.*?)(?=^\[|\z)')
            if (-not $packageSection.Success) { return $null }
            $name = [regex]::Match($packageSection.Groups[1].Value, '(?m)^\s*name\s*=\s*"([^"]+)"')
            if ($name.Success) { return $name.Groups[1].Value }
            return $null
        }
        $parent = Split-Path $directory
        if (-not $parent -or $parent -eq $directory) { break }
        $directory = $parent
    }
    return $null
}

function Get-RustFmtChangedPaths {
    param([Parameter(Mandatory)][string]$RepoRoot)
    $pathspecs = @('rust', '.cargo', 'rust-toolchain', 'rust-toolchain.toml', 'rustfmt.toml', '.rustfmt.toml')
    $tracked = @(& git -C $RepoRoot diff --name-only --diff-filter=ACDMRTUXB HEAD -- @pathspecs 2>$null)
    if ($LASTEXITCODE -ne 0) { return [PSCustomObject]@{ Success = $false; Paths = @() } }
    $untracked = @(& git -C $RepoRoot ls-files --others --exclude-standard -- @pathspecs 2>$null)
    if ($LASTEXITCODE -ne 0) { return [PSCustomObject]@{ Success = $false; Paths = @() } }
    $relevant = @($tracked + $untracked | Where-Object {
        $_ -and ($_ -match '(?i)\.rs$|(^|/)Cargo\.toml$|(^|/)(\.rustfmt|rustfmt)\.toml$|(^|/)rust-toolchain(\.toml)?$|(^|/)\.cargo/config(\.toml)?$')
    } | Sort-Object -Unique)
    [PSCustomObject]@{ Success = $true; Paths = $relevant }
}

function Invoke-RustFmtCached {
    param(
        [Parameter(Mandatory)][string]$RepoRoot,
        [Parameter(Mandatory)][string]$RustRoot,
        [Parameter(Mandatory)][string]$ToolIdentity,
        [Parameter(Mandatory)][string]$Head,
        [AllowNull()][string[]]$ChangedPaths = $null,
        [scriptblock]$CheckAction = $null,
        [scriptblock]$ApplyAction = $null
    )
    $manifest = Join-Path $RustRoot 'Cargo.toml'
    if (-not $CheckAction) {
        $CheckAction = {
            param($Manifest, $Packages)
            $args = @('fmt', '--manifest-path', $Manifest)
            if (@($Packages).Count) { foreach ($package in $Packages) { $args += @('--package', $package) } } else { $args += '--all' }
            $args += @('--', '--check')
            $output = @(& cargo @args 2>&1)
            [PSCustomObject]@{ ExitCode = $LASTEXITCODE; Output = $output }
        }
    }
    if (-not $ApplyAction) {
        $ApplyAction = {
            param($Manifest, $Packages)
            $args = @('fmt', '--manifest-path', $Manifest)
            if (@($Packages).Count) { foreach ($package in $Packages) { $args += @('--package', $package) } } else { $args += '--all' }
            $output = @(& cargo @args 2>&1)
            [PSCustomObject]@{ ExitCode = $LASTEXITCODE; Output = $output }
        }
    }

    $fingerprint = Get-RustFmtInputFingerprint -RepoRoot $RepoRoot -RustRoot $RustRoot -ToolIdentity $ToolIdentity
    $cleanMarker = Get-RustFmtMarkerPath -RepoRoot $RepoRoot -Kind 'clean' -Key $fingerprint
    if (Test-Path -LiteralPath $cleanMarker -PathType Leaf) {
        return [PSCustomObject]@{ Status = 'Cached'; Hunks = 0; Packages = @(); Output = @() }
    }

    if (-not $PSBoundParameters.ContainsKey('ChangedPaths')) {
        $changed = Get-RustFmtChangedPaths -RepoRoot $RepoRoot
        $ChangedPaths = if ($changed.Success) { @($changed.Paths) } else { @('__force_full__') }
    }

    $baselineKeyBytes = [Text.Encoding]::UTF8.GetBytes("$Head`0$ToolIdentity")
    $baselineKey = ([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($baselineKeyBytes))).ToLowerInvariant()
    $baselineMarker = Get-RustFmtMarkerPath -RepoRoot $RepoRoot -Kind 'baseline' -Key $baselineKey
    $packages = @()
    if ((Test-Path -LiteralPath $baselineMarker -PathType Leaf) -and @($ChangedPaths).Count) {
        $canNarrow = $true
        foreach ($path in @($ChangedPaths)) {
            if (-not $path.EndsWith('.rs', [StringComparison]::OrdinalIgnoreCase)) { $canNarrow = $false; break }
            $package = Get-RustFmtPackageForPath -RepoRoot $RepoRoot -RustRoot $RustRoot -RelativePath $path
            if (-not $package) { $canNarrow = $false; break }
            $packages += $package
        }
        if ($canNarrow) { $packages = @($packages | Sort-Object -Unique) } else { $packages = @() }
    }

    $checked = & $CheckAction $manifest $packages
    $hunks = @($checked.Output | Where-Object { $_ -match '^Diff in ' }).Count
    if ($checked.ExitCode -eq 0) {
        $afterCheck = Get-RustFmtInputFingerprint -RepoRoot $RepoRoot -RustRoot $RustRoot -ToolIdentity $ToolIdentity
        if ($afterCheck -ne $fingerprint) {
            return [PSCustomObject]@{ Status = 'ChangedDuringCheck'; Hunks = 0; Packages = $packages; Output = @($checked.Output) }
        }
        Publish-RustFmtMarker -Path $cleanMarker
        if (@($packages).Count -eq 0) { Publish-RustFmtMarker -Path $baselineMarker }
        return [PSCustomObject]@{ Status = 'Clean'; Hunks = 0; Packages = $packages; Output = @($checked.Output) }
    }
    if ($hunks -eq 0) {
        return [PSCustomObject]@{ Status = 'Failed'; Hunks = 0; Packages = $packages; Output = @($checked.Output) }
    }

    $applied = & $ApplyAction $manifest $packages
    if ($applied.ExitCode -ne 0) {
        return [PSCustomObject]@{ Status = 'Failed'; Hunks = $hunks; Packages = $packages; Output = @($applied.Output) }
    }
    $afterApply = Get-RustFmtInputFingerprint -RepoRoot $RepoRoot -RustRoot $RustRoot -ToolIdentity $ToolIdentity
    $verified = & $CheckAction $manifest $packages
    $afterVerify = Get-RustFmtInputFingerprint -RepoRoot $RepoRoot -RustRoot $RustRoot -ToolIdentity $ToolIdentity
    if ($verified.ExitCode -ne 0) {
        return [PSCustomObject]@{ Status = 'Failed'; Hunks = $hunks; Packages = $packages; Output = @($applied.Output) + @($verified.Output) }
    }
    if ($afterVerify -ne $afterApply) {
        return [PSCustomObject]@{ Status = 'ChangedDuringCheck'; Hunks = $hunks; Packages = $packages; Output = @($applied.Output) + @($verified.Output) }
    }
    Publish-RustFmtMarker -Path (Get-RustFmtMarkerPath -RepoRoot $RepoRoot -Kind 'clean' -Key $afterVerify)
    if (@($packages).Count -eq 0) { Publish-RustFmtMarker -Path $baselineMarker }
    [PSCustomObject]@{ Status = 'Applied'; Hunks = $hunks; Packages = $packages; Output = @($applied.Output) + @($verified.Output) }
}
