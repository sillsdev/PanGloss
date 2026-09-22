[CmdletBinding()]
param(
    [string]$LogPath,
    [string]$CodexRoot = ''
)

$ErrorActionPreference = 'Stop'

function Resolve-DiagnosticCodexRoot {
    param([string]$RequestedRoot)

    $candidate = $RequestedRoot
    if ([string]::IsNullOrWhiteSpace($candidate)) { $candidate = $env:CODEX_HOME }
    if ([string]::IsNullOrWhiteSpace($candidate) -and -not [string]::IsNullOrWhiteSpace($env:USERPROFILE)) {
        $candidate = Join-Path $env:USERPROFILE '.codex'
    }
    if ([string]::IsNullOrWhiteSpace($candidate) -and -not [string]::IsNullOrWhiteSpace($env:HOME)) {
        $candidate = Join-Path $env:HOME '.codex'
    }
    if ([string]::IsNullOrWhiteSpace($candidate)) { return $null }
    return [IO.Path]::GetFullPath($candidate)
}

function ConvertTo-PowerShellSingleQuotedLiteral {
    param([Parameter(Mandatory)][string]$Value)
    return "'$( $Value.Replace("'", "''") )'"
}

function Test-PathContainsTraversal {
    param([Parameter(Mandatory)][string]$Path)
    return $Path -match '(?i)(^|[\\/])\.{1,2}([\\/]|$)'
}

function Test-PathWithin {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Root
    )

    $normalizedPath = [IO.Path]::GetFullPath($Path) -replace '[\\/]+$', ''
    $normalizedRoot = [IO.Path]::GetFullPath($Root) -replace '[\\/]+$', ''
    return $normalizedPath.Equals($normalizedRoot, [StringComparison]::OrdinalIgnoreCase) -or
        $normalizedPath.StartsWith($normalizedRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
        $normalizedPath.StartsWith($normalizedRoot + [IO.Path]::AltDirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)
}

function Find-DiagnosticReparsePoint {
    param([Parameter(Mandatory)][string]$Path)

    $cursorPath = [IO.Path]::GetFullPath($Path)
    while (-not (Test-Path -LiteralPath $cursorPath)) {
        $parentPath = [IO.Path]::GetDirectoryName($cursorPath)
        if ([string]::IsNullOrEmpty($parentPath) -or $parentPath -eq $cursorPath) { return $null }
        $cursorPath = $parentPath
    }

    $cursor = Get-Item -LiteralPath $cursorPath -Force -ErrorAction Stop
    while ($null -ne $cursor) {
        if (($cursor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $cursor.FullName }
        $cursor = $cursor.Parent
    }
    return $null
}

function Get-VerifiedCuaPnpmCacheEntry {
    param([Parameter(Mandatory)][string]$ReportedPath)

    $invalid = {
        param([string]$Reason)
        [PSCustomObject]@{
            Valid = $false
            Reason = $Reason
            CanonicalPath = $null
            RuntimeTreeRoot = $null
            Target = $null
        }
    }

    if ([string]::IsNullOrWhiteSpace($ReportedPath)) { return (& $invalid 'The reported path is empty.') }
    if (Test-PathContainsTraversal -Path $ReportedPath) {
        return (& $invalid 'The reported path contains traversal components.')
    }

    try { $canonicalPath = [IO.Path]::GetFullPath($ReportedPath) }
    catch { return (& $invalid "The reported path is not a valid filesystem path: $($_.Exception.Message)") }

    $runtimeMarker = [regex]::Match($canonicalPath, '(?i)[\\/]OpenAI[\\/]Codex[\\/]runtimes[\\/]cua_node(?=[\\/])')
    if (-not $runtimeMarker.Success) {
        return (& $invalid 'The path is not under the expected OpenAI/Codex runtimes/cua_node tree.')
    }

    $runtimeTreeRoot = $canonicalPath.Substring(0, $runtimeMarker.Index + $runtimeMarker.Length)
    $relativePath = $canonicalPath.Substring($runtimeTreeRoot.Length).TrimStart([char[]]@('\', '/'))
    $segments = @($relativePath -split '[\\/]+' | Where-Object { $_ })
    if ($segments.Count -lt 5) {
        return (& $invalid 'The path does not contain a runtime, pnpm-store, links, package, version, and hash entry.')
    }

    $pnpmIndexes = @(
        for ($index = 0; $index -lt $segments.Count; $index++) {
            if ($segments[$index].Equals('pnpm-store', [StringComparison]::OrdinalIgnoreCase)) { $index }
        }
    )
    if ($pnpmIndexes.Count -ne 1) {
        return (& $invalid 'The path must contain exactly one pnpm-store component.')
    }
    $pnpmIndex = $pnpmIndexes[0]
    $linksIndexes = @(
        for ($index = $pnpmIndex + 1; $index -lt $segments.Count; $index++) {
            if ($segments[$index].Equals('links', [StringComparison]::OrdinalIgnoreCase)) { $index }
        }
    )
    if ($linksIndexes.Count -ne 1 -or $linksIndexes[0] -le $pnpmIndex + 1 -or $linksIndexes[0] -ge $segments.Count - 1) {
        return (& $invalid 'The path must contain one links component below pnpm-store.')
    }
    $entryName = $segments[$segments.Count - 1]
    if ($entryName -notmatch '(?i)^[0-9a-f]{64}$') {
        return (& $invalid 'The final path component is not a 64-character hexadecimal cache-entry name.')
    }

    $runtimeRootItem = @(Get-Item -LiteralPath $runtimeTreeRoot -Force -ErrorAction SilentlyContinue)
    if ($runtimeRootItem.Count -ne 1 -or -not $runtimeRootItem[0].PSIsContainer) {
        return (& $invalid 'The expected runtime tree does not exist as one directory.')
    }

    $targetItems = @(Get-Item -LiteralPath $canonicalPath -Force -ErrorAction SilentlyContinue)
    if ($targetItems.Count -ne 1 -or -not $targetItems[0].PSIsContainer) {
        return (& $invalid 'The reported cache entry is not one existing directory.')
    }
    if (($targetItems[0].Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        return (& $invalid 'The reported cache entry is a reparse point, not a plain directory.')
    }

    $sourceReparsePoint = Find-DiagnosticReparsePoint -Path $canonicalPath
    if ($null -ne $sourceReparsePoint) {
        return (& $invalid "The reported cache entry path contains a reparse point: $sourceReparsePoint")
    }

    return [PSCustomObject]@{
        Valid = $true
        Reason = ''
        CanonicalPath = $canonicalPath
        RuntimeTreeRoot = $runtimeTreeRoot
        Target = $targetItems[0]
    }
}

$resolvedLogPath = $null
if ([string]::IsNullOrWhiteSpace($LogPath)) {
    $resolvedCodexRoot = Resolve-DiagnosticCodexRoot -RequestedRoot $CodexRoot
    if ($null -eq $resolvedCodexRoot) { throw 'Codex root is required when discovering the newest sandbox log.' }
    $sandboxLogDirectory = Join-Path $resolvedCodexRoot '.sandbox'
    if (-not (Test-Path -LiteralPath $sandboxLogDirectory -PathType Container)) {
        throw "Sandbox log directory not found: $sandboxLogDirectory"
    }

    $sandboxLogs = @(Get-ChildItem -LiteralPath $sandboxLogDirectory -Filter 'sandbox*.log' -File)
    if ($sandboxLogs.Count -eq 0) {
        throw "No sandbox*.log file found under: $sandboxLogDirectory"
    }
    $newestWriteTime = ($sandboxLogs | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1).LastWriteTimeUtc
    $newestLogs = @($sandboxLogs | Where-Object { $_.LastWriteTimeUtc -eq $newestWriteTime })
    if ($newestLogs.Count -ne 1) {
        throw "The newest sandbox log is ambiguous: $($newestLogs.Count) files share LastWriteTimeUtc $($newestWriteTime.ToString('o'))."
    }
    $newestLog = $newestLogs[0]
    $resolvedLogPath = $newestLog.FullName
} else {
    $resolvedLogPath = [IO.Path]::GetFullPath($LogPath)
}

if (-not (Test-Path -LiteralPath $resolvedLogPath -PathType Leaf)) {
    throw "Sandbox log file not found: $resolvedLogPath"
}

$validationPattern = '^\[(?<timestamp>[^\]]+)\]\s+runtime read/execute validation failed: validate runtime read/execute access on (?<path>.+): CreateFileW failed for (?<failedPath>.+)$'
$validations = @()
$lineNumber = 0
foreach ($line in [IO.File]::ReadLines($resolvedLogPath)) {
    $lineNumber++
    $line = $line.TrimStart([char]0xFEFF)
    $candidate = [regex]::Match($line, $validationPattern)
    if (-not $candidate.Success) { continue }

    $candidateTime = [DateTimeOffset]::MinValue
    $timestampParsed = [DateTimeOffset]::TryParse(
        $candidate.Groups['timestamp'].Value,
        [Globalization.CultureInfo]::InvariantCulture,
        [Globalization.DateTimeStyles]::AssumeUniversal,
        [ref]$candidateTime
    )
    $validations += [PSCustomObject]@{
        Match = $candidate
        Timestamp = $candidate.Groups['timestamp'].Value
        ParsedTimestamp = $candidateTime
        TimestampParsed = $timestampParsed
        LineNumber = $lineNumber
    }
}

if ($validations.Count -eq 0) {
    throw "No direct runtime read/execute validation failure found in: $resolvedLogPath"
}
$malformed = @($validations | Where-Object { -not $_.TimestampParsed })
$parsedValidations = @($validations | Where-Object { $_.TimestampParsed })
if ($parsedValidations.Count -eq 0) {
    throw "Cannot determine the latest direct runtime read/execute validation failure because every matching timestamp is malformed (line $($malformed[0].LineNumber)): $($malformed[0].Timestamp)"
}

$latestTimestamp = ($parsedValidations | Sort-Object ParsedTimestamp -Descending | Select-Object -First 1).ParsedTimestamp
$latest = @($parsedValidations | Where-Object { $_.ParsedTimestamp -eq $latestTimestamp })
if ($latest.Count -ne 1) {
    throw "Cannot determine the latest direct runtime read/execute validation failure because timestamp $($latestTimestamp.ToString('o')) is ambiguous across $($latest.Count) matching events."
}
$malformedAfterLatest = @($malformed | Where-Object { $_.LineNumber -gt $latest[0].LineNumber })
if ($malformedAfterLatest.Count -gt 0) {
    throw "Cannot determine the latest direct runtime read/execute validation failure because a later matching timestamp is malformed (line $($malformedAfterLatest[0].LineNumber)): $($malformedAfterLatest[0].Timestamp)"
}

$latestValidation = $latest[0].Match
$timestamp = $latest[0].Timestamp
$validationPath = $latestValidation.Groups['path'].Value
$failedPath = $latestValidation.Groups['failedPath'].Value
$pathLength = $validationPath.Length
$sameFailurePath = [StringComparer]::OrdinalIgnoreCase.Equals($validationPath, $failedPath)
$candidate = if ($sameFailurePath) { Get-VerifiedCuaPnpmCacheEntry -ReportedPath $validationPath } else { $null }
$isLongPath = $pathLength -gt 260

$resolvedCodexRoot = $null
$quarantineDirectory = $null
$manualMoveCommand = $null
$classification = $null
$manualAdvice = $null
if (-not $sameFailurePath) {
    $classification = 'Validation and failing paths differ; manual inspection required'
    $manualAdvice = 'Do not quarantine this entry. Verify both paths and the runtime log manually; the validation and failing paths do not identify one safe target.'
} elseif ($null -eq $candidate -or -not $candidate.Valid) {
    $reason = if ($null -eq $candidate) { 'The validation path was not eligible for candidate verification.' } else { $candidate.Reason }
    $classification = "No verified quarantine candidate: $reason"
    $manualAdvice = 'Do not quarantine this entry. Verify the exact path and its ACL manually; the diagnostic could not prove one existing generated CUA pnpm cache directory under the expected runtime tree.'
} elseif (-not $isLongPath) {
    $classification = 'Verified generated CUA Node pnpm cache path (<= 260 characters); quarantine not indicated'
    $manualAdvice = 'Do not quarantine this entry. The path is a verified generated cache directory, but it is not longer than the legacy Windows path limit.'
} else {
    $resolvedCodexRoot = Resolve-DiagnosticCodexRoot -RequestedRoot $CodexRoot
    if ($null -eq $resolvedCodexRoot) {
        $classification = 'Verified long generated CUA Node pnpm cache path, but Codex root is unavailable; no quarantine command emitted'
        $manualAdvice = 'Do not quarantine this entry. Supply -CodexRoot or CODEX_HOME, then verify the exact target, ACL, and idle state before any manual move.'
    } else {
        $dateMatch = [regex]::Match($timestamp, '^(?<date>\d{4})-(?<month>\d{2})-(?<day>\d{2})')
        $dateStamp = if ($dateMatch.Success) {
            '{0}{1}{2}' -f $dateMatch.Groups['date'].Value, $dateMatch.Groups['month'].Value, $dateMatch.Groups['day'].Value
        } else {
            $latestTimestamp.ToString('yyyyMMdd')
        }
        $quarantineDirectory = [IO.Path]::GetFullPath((Join-Path (Join-Path $resolvedCodexRoot 'tmp') "sandbox-runtime-quarantine-$dateStamp"))
        $destinationReparsePoint = Find-DiagnosticReparsePoint -Path $quarantineDirectory
        if ($null -ne $destinationReparsePoint) {
            $quarantineDirectory = $null
            $classification = "Verified long generated CUA Node pnpm cache path, but the proposed quarantine destination contains a reparse point ($destinationReparsePoint); no command emitted"
            $manualAdvice = 'Do not quarantine this entry. Choose a recoverable destination with a reparse-free parent chain outside the runtime tree, then verify the target, ACL, and idle state manually.'
        } elseif (Test-PathWithin -Path $quarantineDirectory -Root $candidate.RuntimeTreeRoot) {
            $quarantineDirectory = $null
            $classification = 'Verified long generated CUA Node pnpm cache path, but the proposed quarantine destination is inside the runtime tree; no command emitted'
            $manualAdvice = 'Do not quarantine this entry. Choose a recoverable destination outside the runtime tree and verify the target, ACL, and idle state manually.'
        } else {
            $quotedValidationPath = ConvertTo-PowerShellSingleQuotedLiteral -Value $candidate.CanonicalPath
            $quotedQuarantineDirectory = ConvertTo-PowerShellSingleQuotedLiteral -Value $quarantineDirectory
            $manualMoveCommand = "Move-Item -LiteralPath $quotedValidationPath -Destination $quotedQuarantineDirectory"
            $classification = 'Verified long generated CUA Node pnpm cache path (> 260 characters)'
            $manualAdvice = "Verify the exact target '$($candidate.CanonicalPath)' is idle and has the expected ACL, verify the destination '$quarantineDirectory' is outside the runtime tree, create that destination if needed, and run the provided Move-Item command yourself. Do not delete the target or any parent. If sandboxed verification still fails, move the quarantined entry back to its original path, then verify a normal sandboxed command and a real apply_patch operation."
        }
    }
}

[PSCustomObject]@{
    LogPath = $resolvedLogPath
    Timestamp = $timestamp
    ValidationPath = $validationPath
    FailedPath = $failedPath
    PathLength = $pathLength
    Classification = $classification
    QuarantineDirectory = $quarantineDirectory
    ManualMoveCommand = $manualMoveCommand
    ManualAdvice = $manualAdvice
}
