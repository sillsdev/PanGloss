<#
  .DESCRIPTION
  Covers: .claude/hooks/block-root-find.py -- the PreToolUse hook that refuses a `find` invocation
  scanning from the filesystem root (see CLAUDE.md's "Never scan from the filesystem root"). Each
  case is run through the REAL hook script as a subprocess, exactly as the harness invokes it, with
  a synthetic PreToolUse JSON payload on stdin -- never against a re-implementation of its regex.
#>
. "$PSScriptRoot\_test-harness.ps1"

$script:HookPath = Join-Path $PSScriptRoot '..\..\..\.claude\hooks\block-root-find.py'

function Invoke-RootFindHook {
    param([Parameter(Mandatory)][string]$Command, [switch]$AllowEscapeHatch)
    $payload = (@{ tool_input = @{ command = $Command } } | ConvertTo-Json -Compress)
    if ($AllowEscapeHatch) {
        $prior = $env:PANGLOSS_ALLOW_ROOT_FIND
        $env:PANGLOSS_ALLOW_ROOT_FIND = '1'
        try { return ($payload | python $script:HookPath) }
        finally { $env:PANGLOSS_ALLOW_ROOT_FIND = $prior }
    }
    return ($payload | python $script:HookPath)
}

function Assert-Refused {
    param([string]$Command)
    $out = Invoke-RootFindHook -Command $Command
    Assert-True ($out -match '"permissionDecision":\s*"deny"') "expected a deny decision for: $Command (got: $out)"
}

function Assert-Allowed {
    param([string]$Command)
    $out = Invoke-RootFindHook -Command $Command
    Assert-True ([string]::IsNullOrEmpty($out)) "expected no output (allowed) for: $Command (got: $out)"
}

# --- refusals: bare root and bare drive roots, in every position the task calls out ---

Test-Case 'refuses find / -name x' { Assert-Refused 'find / -name x' }
Test-Case 'refuses find C:\ -name x' { Assert-Refused 'find C:\ -name x' }
Test-Case 'refuses find C:/ -name x' { Assert-Refused 'find C:/ -name x' }
Test-Case 'refuses a quoted find "C:\" -name x' { Assert-Refused 'find "C:\" -name x' }
Test-Case 'refuses a root find after a semicolon' { Assert-Refused 'echo done; find / -iname x' }
Test-Case 'refuses a root find after &&' { Assert-Refused 'echo done && find / -iname x' }
Test-Case 'refuses a root find after ||' { Assert-Refused 'echo done || find / -iname x' }
Test-Case 'refuses a root find after a pipe' { Assert-Refused 'echo done | find / -iname x' }
Test-Case 'refuses an absolute /usr/bin/find invocation' { Assert-Refused '/usr/bin/find / -iname x' }
Test-Case 'refuses the exact motivating incident command' { Assert-Refused 'find / -iname rewrite.rs -path *foma*' }

# --- allow-cases: a real, scoped search root must pass through untouched ---

Test-Case 'allows find . -name x' { Assert-Allowed 'find . -name x' }
Test-Case 'allows find ./rust -name x' { Assert-Allowed 'find ./rust -name x' }
Test-Case 'allows find rust/crates -name x' { Assert-Allowed 'find rust/crates -name x' }
Test-Case 'allows a quoted expanding path' { Assert-Allowed 'find "$HOME/foo" -name x' }
Test-Case 'allows a word merely containing find' { Assert-Allowed 'unfind / -name x' }

# --- escape hatch, matching PANGLOSS_ALLOW_BARE_CARGO's convention ---

Test-Case 'PANGLOSS_ALLOW_ROOT_FIND=1 allows an otherwise-refused root scan' {
    $out = Invoke-RootFindHook -Command 'find / -iname x' -AllowEscapeHatch
    Assert-True ([string]::IsNullOrEmpty($out)) "expected the escape hatch to suppress the refusal (got: $out)"
}

# --- a hook that cannot parse its input must not block real work ---

Test-Case 'malformed JSON on stdin is not treated as a refusal' {
    $out = 'not json' | python $script:HookPath
    Assert-True ([string]::IsNullOrEmpty($out)) "expected no output for unparseable input (got: $out)"
}

Write-TestSummary
