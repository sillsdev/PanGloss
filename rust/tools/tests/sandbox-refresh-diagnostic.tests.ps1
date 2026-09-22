. "$PSScriptRoot\_test-harness.ps1"

$diagnosticPath = Join-Path $PSScriptRoot '..\sandbox-refresh-diagnostic.ps1'

function New-RuntimeValidationLine {
    param(
        [Parameter(Mandatory)][string]$Timestamp,
        [Parameter(Mandatory)][string]$Path,
        [string]$FailedPath
    )
    if ([string]::IsNullOrEmpty($FailedPath)) { $FailedPath = $Path }
    return "[$Timestamp] runtime read/execute validation failed: validate runtime read/execute access on $Path`: CreateFileW failed for $FailedPath"
}

Test-Case 'selects the newest sandbox log and latest runtime validation path' {
    $fixture = New-TestTempDir -Prefix 'pg-sandbox-refresh-diagnostic'
    try {
        $codexRoot = Join-Path $fixture '.codex'
        $sandboxDir = Join-Path $codexRoot '.sandbox'
        New-Item -ItemType Directory -Path $sandboxDir -Force | Out-Null

        $oldPath = 'C:\old\runtime-cache-entry'
        $oldLog = Join-Path $sandboxDir 'sandbox.2026-09-21.log'
        Set-Content -LiteralPath $oldLog -Encoding utf8 -Value (New-RuntimeValidationLine -Timestamp '2026-09-21T10:00:00.0000000+00:00' -Path $oldPath)
        [IO.File]::SetLastWriteTimeUtc($oldLog, [DateTime]::UtcNow.AddMinutes(-5))

        $runtimeRoot = Join-Path $fixture 'OpenAI\Codex\runtimes\cua_node\runtime-hash'
        $cacheParent = Join-Path $runtimeRoot ('runtime-segment-' + ('x' * 80) + '\node_modules\@oai\sky\dist\pnpm-store\v11\links\@rollup\plugin-typescript\12.1.2')
        $latestPath = Join-Path $cacheParent ('b' * 64)
        New-Item -ItemType Directory -Force -Path $latestPath | Out-Null
        $newLog = Join-Path $sandboxDir 'sandbox.2026-09-22.log'
        @(
            (New-RuntimeValidationLine -Timestamp '2026-09-22T10:00:00.0000000+00:00' -Path $latestPath)
            'setup refresh: processed 6 write roots (read roots delegated); errors=["runtime read/execute validation failed: escaped aggregate"]'
            (New-RuntimeValidationLine -Timestamp '2026-09-22T10:01:00.0000000+00:00' -Path $latestPath)
            (New-RuntimeValidationLine -Timestamp '2026-09-22T09:59:00.0000000+00:00' -Path $oldPath)
        ) | Set-Content -LiteralPath $newLog -Encoding utf8
        [IO.File]::SetLastWriteTimeUtc($newLog, [DateTime]::UtcNow.AddMinutes(-1))

        $diagnostic = & $diagnosticPath -CodexRoot $codexRoot
        Assert-Equal ([IO.Path]::GetFullPath($newLog)) $diagnostic.LogPath 'newest sandbox log should be selected'
        Assert-Equal '2026-09-22T10:01:00.0000000+00:00' $diagnostic.Timestamp 'latest direct validation event should be selected'
        Assert-Equal $latestPath $diagnostic.ValidationPath 'exact validation path should be preserved'
        Assert-Equal $latestPath $diagnostic.FailedPath 'exact failing path should be preserved'
        Assert-Equal $latestPath.Length $diagnostic.PathLength 'path length should use the exact reported path'
        Assert-True ($diagnostic.PathLength -gt 260) 'the fixture should classify as a long path'
        Assert-Equal 'Verified long generated CUA Node pnpm cache path (> 260 characters)' $diagnostic.Classification
        Assert-True ($diagnostic.ManualAdvice -match [regex]::Escape($latestPath)) 'advice must name the exact target'
        Assert-True ($diagnostic.ManualAdvice -match 'Move-Item') 'advice must describe a recoverable manual move'
        Assert-True ($diagnostic.ManualAdvice -match 'ACL') 'advice must require checking the target ACL against its parent'
        Assert-True (Test-Path -LiteralPath $newLog) 'diagnosis must leave the selected log untouched'
        Assert-False (Test-Path -LiteralPath $diagnostic.QuarantineDirectory) 'diagnosis must not create a quarantine directory'
    } finally {
        Remove-Item -LiteralPath $fixture -Recurse -Force
    }
}

Test-Case 'explicit log injection overrides root discovery and non-candidate paths do not recommend quarantine' {
    $fixture = New-TestTempDir -Prefix 'pg-sandbox-refresh-log-override'
    try {
        $codexRoot = Join-Path $fixture '.codex'
        $sandboxDir = Join-Path $codexRoot '.sandbox'
        New-Item -ItemType Directory -Path $sandboxDir -Force | Out-Null
        $discoveredLog = Join-Path $sandboxDir 'sandbox.2026-09-22.log'
        Set-Content -LiteralPath $discoveredLog -Encoding utf8 -Value 'no validation events'

        $explicitLog = Join-Path $fixture 'injected.log'
        $explicitPath = 'C:\short\runtime-entry'
        Set-Content -LiteralPath $explicitLog -Encoding utf8 -Value (New-RuntimeValidationLine -Timestamp '2026-09-22T10:02:00.0000000+00:00' -Path $explicitPath)

        $diagnostic = & $diagnosticPath -CodexRoot $codexRoot -LogPath $explicitLog
        Assert-Equal ([IO.Path]::GetFullPath($explicitLog)) $diagnostic.LogPath 'explicit log should take priority'
        Assert-Equal $explicitPath $diagnostic.ValidationPath 'explicit log path should be parsed'
        Assert-Equal $explicitPath $diagnostic.FailedPath 'explicit failing path should be parsed'
        Assert-True ($diagnostic.Classification -match '^No verified quarantine candidate:') 'non-candidate paths should be classified explicitly'
        Assert-Equal $null $diagnostic.ManualMoveCommand 'non-candidate paths must not receive a move command'
        Assert-True ($diagnostic.ManualAdvice -match 'Do not quarantine') 'non-candidate paths must not get quarantine advice'
        Assert-True (Test-Path -LiteralPath $explicitLog) 'diagnosis must leave the explicit log untouched'
    } finally {
        Remove-Item -LiteralPath $fixture -Recurse -Force
    }
}

Test-Case 'mismatched paths are exposed and explicit logs work without USERPROFILE' {
    $fixture = New-TestTempDir -Prefix 'pg-sandbox-refresh-mismatch'
    $hadUserProfile = Test-Path Env:USERPROFILE
    $oldUserProfile = $env:USERPROFILE
    $hadCodexHome = Test-Path Env:CODEX_HOME
    $oldCodexHome = $env:CODEX_HOME
    try {
        $log = Join-Path $fixture 'mismatch.log'
        $validationPath = 'C:\Users\tester\runtime-validation-entry'
        $failedPath = 'C:\Users\tester\different-failing-entry'
        Set-Content -LiteralPath $log -Encoding utf8 -Value (New-RuntimeValidationLine -Timestamp '2026-09-22T10:03:00.0000000+00:00' -Path $validationPath -FailedPath $failedPath)
        Remove-Item Env:USERPROFILE -ErrorAction SilentlyContinue
        Remove-Item Env:CODEX_HOME -ErrorAction SilentlyContinue

        $diagnostic = & $diagnosticPath -LogPath $log
        Assert-Equal $validationPath $diagnostic.ValidationPath 'validation path should be retained for mismatched paths'
        Assert-Equal $failedPath $diagnostic.FailedPath 'failing path should be exposed for mismatched paths'
        Assert-Equal 'Validation and failing paths differ; manual inspection required' $diagnostic.Classification
        Assert-Equal $null $diagnostic.ManualMoveCommand 'mismatched paths must not receive a move command'
    } finally {
        if ($hadUserProfile) { Set-Item Env:USERPROFILE $oldUserProfile } else { Remove-Item Env:USERPROFILE -ErrorAction SilentlyContinue }
        if ($hadCodexHome) { Set-Item Env:CODEX_HOME $oldCodexHome } else { Remove-Item Env:CODEX_HOME -ErrorAction SilentlyContinue }
        Remove-Item -LiteralPath $fixture -Recurse -Force
    }
}

Test-Case 'malformed and equal latest timestamps are rejected as ambiguous' {
    $fixture = New-TestTempDir -Prefix 'pg-sandbox-refresh-timestamps'
    try {
        $malformedLog = Join-Path $fixture 'malformed.log'
        @(
            (New-RuntimeValidationLine -Timestamp '2026-09-22T10:04:00.0000000+00:00' -Path 'C:\first\entry')
            (New-RuntimeValidationLine -Timestamp 'not-a-timestamp' -Path 'C:\later\entry')
        ) | Set-Content -LiteralPath $malformedLog -Encoding utf8
        $malformedFailure = $false
        try { $null = & $diagnosticPath -LogPath $malformedLog } catch { $malformedFailure = $_.Exception.Message -match 'timestamp is malformed' }
        Assert-True $malformedFailure 'a malformed matching timestamp must not make an older event look latest'

        $equalLog = Join-Path $fixture 'equal.log'
        @(
            (New-RuntimeValidationLine -Timestamp '2026-09-22T10:05:00.0000000+00:00' -Path 'C:\first\entry')
            (New-RuntimeValidationLine -Timestamp '2026-09-22T10:05:00.0000000+00:00' -Path 'C:\second\entry')
        ) | Set-Content -LiteralPath $equalLog -Encoding utf8
        $equalFailure = $false
        try { $null = & $diagnosticPath -LogPath $equalLog } catch { $equalFailure = $_.Exception.Message -match 'timestamp .*ambiguous' }
        Assert-True $equalFailure 'equal latest timestamps must be reported as ambiguous'
    } finally {
        Remove-Item -LiteralPath $fixture -Recurse -Force
    }
}

Test-Case 'an older malformed record does not hide a later valid direct failure' {
    $fixture = New-TestTempDir -Prefix 'pg-sandbox-refresh-old-malformed'
    try {
        $log = Join-Path $fixture 'old-malformed.log'
        @(
            (New-RuntimeValidationLine -Timestamp '2026-09-22T10:04:00.0000000+00:002026-09-22T10:04:00.1000000+00:00' -Path 'C:\older\entry')
            (New-RuntimeValidationLine -Timestamp '2026-09-22T10:05:00.0000000+00:00' -Path 'C:\later\entry')
        ) | Set-Content -LiteralPath $log -Encoding utf8

        $diagnostic = & $diagnosticPath -LogPath $log
        Assert-Equal '2026-09-22T10:05:00.0000000+00:00' $diagnostic.Timestamp `
            'a later valid append must supersede an older interleaved timestamp'
        Assert-Equal 'C:\later\entry' $diagnostic.ValidationPath
    } finally {
        Remove-Item -LiteralPath $fixture -Recurse -Force
    }
}

Test-Case 'equally newest sandbox logs are rejected instead of chosen by filename' {
    $fixture = New-TestTempDir -Prefix 'pg-sandbox-refresh-log-tie'
    try {
        $codexRoot = Join-Path $fixture '.codex'
        $sandboxDir = Join-Path $codexRoot '.sandbox'
        New-Item -ItemType Directory -Path $sandboxDir -Force | Out-Null
        $first = Join-Path $sandboxDir 'sandbox.a.log'
        $second = Join-Path $sandboxDir 'sandbox.b.log'
        Set-Content -LiteralPath $first -Encoding utf8 -Value (New-RuntimeValidationLine -Timestamp '2026-09-22T10:05:00.0000000+00:00' -Path 'C:\first\entry')
        Set-Content -LiteralPath $second -Encoding utf8 -Value (New-RuntimeValidationLine -Timestamp '2026-09-22T10:06:00.0000000+00:00' -Path 'C:\second\entry')
        $sameTime = [DateTime]::UtcNow.AddMinutes(-1)
        [IO.File]::SetLastWriteTimeUtc($first, $sameTime)
        [IO.File]::SetLastWriteTimeUtc($second, $sameTime)

        $ambiguous = $false
        try { $null = & $diagnosticPath -CodexRoot $codexRoot } catch { $ambiguous = $_.Exception.Message -match 'newest sandbox log.*ambiguous' }
        Assert-True $ambiguous 'equal newest log mtimes must fail loudly'
    } finally {
        Remove-Item -LiteralPath $fixture -Recurse -Force
    }
}

Test-Case 'only an existing exact cache directory can receive quarantine advice' {
    $fixture = New-TestTempDir -Prefix 'pg-sandbox-refresh-candidates'
    try {
        $codexRoot = Join-Path $fixture '.codex'
        $candidateParent = Join-Path $fixture 'OpenAI\Codex\runtimes\cua_node\runtime-hash\pnpm-store\v11\links\pkg\1.0.0'
        $existingDirectory = Join-Path $candidateParent ('a' * 64)
        $missingDirectory = Join-Path $candidateParent ('b' * 64)
        $fileEntry = Join-Path $candidateParent ('c' * 64)
        New-Item -ItemType Directory -Force -Path $existingDirectory | Out-Null
        New-Item -ItemType File -Force -Path $fileEntry | Out-Null
        $lookalikeParent = Join-Path $fixture 'OpenAI\Codex\not-runtimes\cua_node\runtime-hash\pnpm-store\v11\links\pkg\1.0.0'
        $lookalikeDirectory = Join-Path $lookalikeParent ('d' * 64)
        New-Item -ItemType Directory -Force -Path $lookalikeDirectory | Out-Null
        $traversalPath = $candidateParent + '\..\' + ('a' * 64)

        $cases = @(
            [PSCustomObject]@{ Name = 'existing'; Path = $existingDirectory; Expected = 'verified' }
            [PSCustomObject]@{ Name = 'missing'; Path = $missingDirectory; Expected = 'unverified' }
            [PSCustomObject]@{ Name = 'file'; Path = $fileEntry; Expected = 'unverified' }
            [PSCustomObject]@{ Name = 'lookalike'; Path = $lookalikeDirectory; Expected = 'unverified' }
            [PSCustomObject]@{ Name = 'traversal'; Path = $traversalPath; Expected = 'unverified' }
        )
        foreach ($case in $cases) {
            $log = Join-Path $fixture "$($case.Name).log"
            Set-Content -LiteralPath $log -Encoding utf8 -Value (New-RuntimeValidationLine -Timestamp '2026-09-22T10:06:00.0000000+00:00' -Path $case.Path)
            $diagnostic = & $diagnosticPath -CodexRoot $codexRoot -LogPath $log
            Assert-Equal $null $diagnostic.ManualMoveCommand "$($case.Name) must not emit a move command unless fully verified"
            if ($case.Expected -eq 'verified') {
                Assert-True ($diagnostic.Classification -match '^Verified generated CUA Node pnpm cache path') 'the existing exact directory should be recognized'
            } else {
                Assert-True ($diagnostic.Classification -match '^No verified quarantine candidate:') "$($case.Name) must be explicitly rejected"
            }
        }
    } finally {
        Remove-Item -LiteralPath $fixture -Recurse -Force
    }
}

Test-Case 'hostile path characters remain safely quoted and an in-tree destination is refused' {
    $fixture = New-TestTempDir -Prefix 'pg-sandbox-refresh-quoting'
    try {
        $codexRoot = Join-Path $fixture '.codex'
        $runtimeRoot = Join-Path $fixture 'OpenAI\Codex\runtimes\cua_node\runtime-hash'
        $hostileSegment = 'host'' & ; $()'
        $cacheParent = Join-Path $runtimeRoot ('long-segment-' + ('z' * 100) + "\$hostileSegment\pnpm-store\v11\links\pkg\1.0.0")
        $target = Join-Path $cacheParent ('e' * 64)
        New-Item -ItemType Directory -Force -Path $target | Out-Null
        $log = Join-Path $fixture 'hostile.log'
        Set-Content -LiteralPath $log -Encoding utf8 -Value (New-RuntimeValidationLine -Timestamp '2026-09-22T10:07:00.0000000+00:00' -Path $target)

        $diagnostic = & $diagnosticPath -CodexRoot $codexRoot -LogPath $log
        $quotedTarget = "'$( $target.Replace("'", "''") )'"
        Assert-True ($diagnostic.ManualMoveCommand -match [regex]::Escape($quotedTarget)) 'hostile path characters must remain inside one single-quoted literal'
        Assert-True ($diagnostic.ManualMoveCommand -match 'Move-Item -LiteralPath') 'the advice must use LiteralPath for the source'

        $insideRoot = Join-Path $fixture 'OpenAI\Codex\runtimes\cua_node'
        $inside = & $diagnosticPath -CodexRoot $insideRoot -LogPath $log
        Assert-Equal $null $inside.ManualMoveCommand 'a destination inside the runtime tree must never receive a move command'
        Assert-True ($inside.Classification -match 'inside the runtime tree') 'an unsafe destination must be classified explicitly'
    } finally {
        Remove-Item -LiteralPath $fixture -Recurse -Force
    }
}

Test-Case 'source and destination reparse chains never receive move advice' {
    $fixture = New-TestTempDir -Prefix 'pg-sandbox-refresh-reparse'
    try {
        $actualRuntime = Join-Path $fixture 'actual-runtime'
        $runtimeParent = Join-Path $fixture 'OpenAI\Codex\runtimes'
        New-Item -ItemType Directory -Path $actualRuntime -Force | Out-Null
        New-Item -ItemType Directory -Path $runtimeParent -Force | Out-Null
        $runtimeJunction = Join-Path $runtimeParent 'cua_node'
        New-Item -ItemType Junction -Path $runtimeJunction -Target $actualRuntime | Out-Null
        $sourceParent = Join-Path $runtimeJunction ('runtime-hash\long-' + ('s' * 100) + '\pnpm-store\v11\links\pkg\1.0.0')
        $source = Join-Path $sourceParent ('a' * 64)
        New-Item -ItemType Directory -Path $source -Force | Out-Null
        $sourceLog = Join-Path $fixture 'source-reparse.log'
        Set-Content -LiteralPath $sourceLog -Encoding utf8 -Value (New-RuntimeValidationLine -Timestamp '2026-09-22T10:08:00.0000000+00:00' -Path $source)

        $sourceResult = & $diagnosticPath -CodexRoot (Join-Path $fixture '.codex') -LogPath $sourceLog
        Assert-Equal $null $sourceResult.ManualMoveCommand 'a source junction chain must not receive a move command'
        Assert-True ($sourceResult.Classification -match 'reparse') 'source junction refusal must name the reparse reason'

        $expectedPlainRuntime = Join-Path $fixture 'other\OpenAI\Codex\runtimes\cua_node'
        $plainParent = Join-Path $expectedPlainRuntime ('runtime-hash\long-' + ('d' * 100) + '\pnpm-store\v11\links\pkg\1.0.0')
        $plainSource = Join-Path $plainParent ('b' * 64)
        New-Item -ItemType Directory -Path $plainSource -Force | Out-Null
        $destinationRoot = Join-Path $fixture '.codex'
        $actualDestination = Join-Path $fixture 'actual-destination'
        New-Item -ItemType Directory -Path $destinationRoot -Force | Out-Null
        New-Item -ItemType Directory -Path $actualDestination -Force | Out-Null
        New-Item -ItemType Junction -Path (Join-Path $destinationRoot 'tmp') -Target $actualDestination | Out-Null
        $destinationLog = Join-Path $fixture 'destination-reparse.log'
        Set-Content -LiteralPath $destinationLog -Encoding utf8 -Value (New-RuntimeValidationLine -Timestamp '2026-09-22T10:09:00.0000000+00:00' -Path $plainSource)

        $destinationResult = & $diagnosticPath -CodexRoot $destinationRoot -LogPath $destinationLog
        Assert-Equal $null $destinationResult.ManualMoveCommand 'a destination junction chain must not receive a move command'
        Assert-True ($destinationResult.Classification -match 'reparse') 'destination junction refusal must name the reparse reason'
    } finally {
        Remove-Item -LiteralPath $fixture -Recurse -Force
    }
}

Test-Case 'diagnostic source contains no filesystem mutation commands' {
    $tokens = $null
    $parseErrors = $null
    $diagnosticAst = [System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path -LiteralPath $diagnosticPath), [ref]$tokens, [ref]$parseErrors)
    Assert-Equal 0 $parseErrors.Count 'diagnostic source must parse without errors'
    $mutatingCommands = @('Move-Item', 'Remove-Item', 'New-Item', 'Set-Content', 'Add-Content', 'Out-File', 'Copy-Item', 'Rename-Item', 'Clear-Content')
    $commands = @($diagnosticAst.FindAll({ param($node) $node -is [System.Management.Automation.Language.CommandAst] }, $true) | ForEach-Object { $_.GetCommandName() })
    foreach ($command in $mutatingCommands) {
        Assert-False ($commands -contains $command) "diagnostic must not invoke filesystem mutator $command"
    }
    $source = Get-Content -Raw -LiteralPath $diagnosticPath
    Assert-False ($source -match '(?im)\[IO\.File\]::(Move|Delete|WriteAll|AppendAll|Create)') 'diagnostic must not call direct file mutation APIs'
}

Test-Case 'reports missing runtime validation entries instead of looking successful' {
    $fixture = New-TestTempDir -Prefix 'pg-sandbox-refresh-no-entry'
    try {
        $emptyLog = Join-Path $fixture 'empty.log'
        Set-Content -LiteralPath $emptyLog -Encoding utf8 -Value 'setup refresh completed successfully'
        $failed = $false
        try {
            $null = & $diagnosticPath -LogPath $emptyLog
        } catch {
            $failed = $_.Exception.Message -match 'No direct runtime read/execute validation failure'
        }
        Assert-True $failed 'absence of a failure entry must be reported explicitly'
    } finally {
        Remove-Item -LiteralPath $fixture -Recurse -Force
    }
}

Write-TestSummary
