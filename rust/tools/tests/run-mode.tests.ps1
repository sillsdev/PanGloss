<#
  .DESCRIPTION
  Covers: Resolve-RunTarget and Get-ExhaustionConsumersFromMessage (rust/tools/_common.ps1) -- the
  pure decision/parsing logic behind `pg.ps1 -Mode run` and the resource-exhaustion history surfaced
  by `pg.ps1 -Mode doctor`.

  Neither function launches a process, calls cargo, or queries the real event log: Resolve-RunTarget
  takes its inputs as plain strings/switches and returns a plan object, and
  Get-ExhaustionConsumersFromMessage takes a message STRING rather than an event object, precisely so
  both are testable anywhere without a real build, a real probe binary, or a machine that happens to
  have exhaustion history in its System log. The job-object wrapping itself
  (Invoke-ProcessInJobObject) and the live Get-WinEvent query are deliberately NOT covered here --
  see the task notes for why: they were instead verified by hand against a real cheap process
  (cmd.exe under Invoke-ProcessInJobObject, with procgov present and forced-absent) and this
  machine's actual event log, rather than launched from an automated suite that runs on every CI box.
#>
. "$PSScriptRoot\_test-harness.ps1"
. "$PSScriptRoot\..\_common.ps1"

# --- Resolve-RunTarget: selector validation (exactly one of Example/Bin/Exe) ---

Test-Case 'zero selectors is rejected' {
    $r = Resolve-RunTarget
    Assert-False $r.Ok 'run requires exactly one of -Example/-Bin/-Exe'
    Assert-True ($r.Detail -like '*EXACTLY ONE*') "expected the usage message, got: $($r.Detail)"
}

Test-Case 'exactly one selector (Example) is accepted' {
    $r = Resolve-RunTarget -Example 'predict_census'
    Assert-True $r.Ok $r.Detail
}

Test-Case 'exactly one selector (Bin) is accepted' {
    $r = Resolve-RunTarget -Bin 'pangloss'
    Assert-True $r.Ok $r.Detail
}

Test-Case 'exactly one selector (Exe) is accepted' {
    $r = Resolve-RunTarget -Exe 'C:\some\already-built.exe'
    Assert-True $r.Ok $r.Detail
}

Test-Case 'two selectors is rejected, not silently resolved by picking one' {
    # A Where-Object result that unwraps to a single Hashtable reports .Count as its KEY count (2), not 1.
    $r = Resolve-RunTarget -Example 'foo' -Exe 'bar.exe'
    Assert-False $r.Ok 'passing two selectors must be rejected, not silently resolved'
}

Test-Case 'three selectors is rejected' {
    $r = Resolve-RunTarget -Example 'foo' -Bin 'bar' -Exe 'baz.exe'
    Assert-False $r.Ok
}

Test-Case 'the exactly-one-selector case is not misreported as two (the Where-Object unwrap trap)' {
    # Pins the exact failure text ("got 2") a lone-surviving Hashtable's key-count bug would have shown.
    $r = Resolve-RunTarget -Exe 'C:\some\already-built.exe'
    Assert-True $r.Ok "a single -Exe selector must be accepted, not rejected as if two were passed ($($r.Detail))"
}

# --- Resolve-RunTarget: the -Exe path (no cargo involved) ---

Test-Case '-Exe launches the path directly, not through cargo' {
    $r = Resolve-RunTarget -Exe 'C:\some\already-built.exe'
    Assert-Equal 'C:\some\already-built.exe' $r.LaunchExe
    Assert-True ($r.Label -like 'exe:*') "expected an exe: label, got $($r.Label)"
}

Test-Case '-Exe with no extra args produces an empty launch-args list, not a stray token' {
    $r = Resolve-RunTarget -Exe 'C:\some\already-built.exe'
    Assert-Equal 0 $r.LaunchArgs.Count
}

Test-Case '-Exe passes extra args straight through to the binary' {
    $r = Resolve-RunTarget -Exe 'C:\some\already-built.exe' -ExtraArgs @('--grammar', 'foo.xml')
    Assert-Equal 2 $r.LaunchArgs.Count
    Assert-Equal '--grammar' $r.LaunchArgs[0]
    Assert-Equal 'foo.xml' $r.LaunchArgs[1]
}

Test-Case '-Exe strips exactly one leading literal "--" separator' {
    # '--' is the conventional "everything after here is for the child" marker; must not forward as argv[1].
    $r = Resolve-RunTarget -Exe 'C:\some\already-built.exe' -ExtraArgs @('--', '--grammar', 'foo.xml')
    Assert-Equal 2 $r.LaunchArgs.Count
    Assert-Equal '--grammar' $r.LaunchArgs[0]
    Assert-Equal 'foo.xml' $r.LaunchArgs[1]
}

Test-Case '-Exe does not strip a "--" that is not the FIRST token' {
    # Only a leading '--' is the separator convention; one appearing later is the caller's own argument.
    $r = Resolve-RunTarget -Exe 'C:\some\already-built.exe' -ExtraArgs @('--grammar', '--', 'foo.xml')
    Assert-Equal 3 $r.LaunchArgs.Count
    Assert-Equal '--grammar' $r.LaunchArgs[0]
    Assert-Equal '--' $r.LaunchArgs[1]
    Assert-Equal 'foo.xml' $r.LaunchArgs[2]
}

Test-Case '-Package is not consulted at all when -Exe is used' {
    $r = Resolve-RunTarget -Exe 'C:\some\already-built.exe' -Package 'pg-foma'
    Assert-Equal 'C:\some\already-built.exe' $r.LaunchExe
    Assert-False (@($r.LaunchArgs) -contains 'pg-foma') '-Package must be ignored on the -Exe path'
}

# --- Resolve-RunTarget: the -Example / -Bin path (goes through `cargo run`) ---

Test-Case '-Example builds a `cargo run --release --example NAME` command' {
    $r = Resolve-RunTarget -Example 'predict_census'
    Assert-Equal 'cargo' $r.LaunchExe
    Assert-Equal 'run' $r.LaunchArgs[0]
    Assert-Contains $r.LaunchArgs '--release'
    Assert-Contains $r.LaunchArgs '--example'
    Assert-Contains $r.LaunchArgs 'predict_census'
    Assert-True ($r.Label -like '*predict_census*') $r.Label
}

Test-Case '-Bin builds a `cargo run --release --bin NAME` command' {
    $r = Resolve-RunTarget -Bin 'pangloss'
    Assert-Equal 'cargo' $r.LaunchExe
    Assert-Contains $r.LaunchArgs '--bin'
    Assert-Contains $r.LaunchArgs 'pangloss'
    Assert-False (@($r.LaunchArgs) -contains '--example') 'a -Bin run must not also pass --example'
}

Test-Case '-DebugProfile omits --release from the cargo run command' {
    $released = Resolve-RunTarget -Example 'predict_census'
    $debug = Resolve-RunTarget -Example 'predict_census' -DebugProfile
    Assert-Contains $released.LaunchArgs '--release'
    Assert-False (@($debug.LaunchArgs) -contains '--release') 'a -DebugProfile run must not force --release'
}

Test-Case '-Package selects which workspace crate to build the example/bin from' {
    $r = Resolve-RunTarget -Example 'predict_census' -Package 'pg-foma'
    Assert-Contains $r.LaunchArgs '-p'
    Assert-Contains $r.LaunchArgs 'pg-foma'
}

Test-Case 'no -Package means no -p flag at all (cargo infers it), not an empty value' {
    $r = Resolve-RunTarget -Example 'predict_census'
    Assert-False (@($r.LaunchArgs) -contains '-p') 'omitted -Package must not emit a -p flag'
}

Test-Case 'passthrough args for `cargo run` are inserted after a single cargo-owned "--"' {
    $r = Resolve-RunTarget -Bin 'pangloss' -ExtraArgs @('batch', '--threads', '1')
    $sep = [array]::IndexOf($r.LaunchArgs, '--')
    Assert-True ($sep -ge 0) 'expected a -- separator before the passthrough args'
    Assert-Equal 'batch' $r.LaunchArgs[$sep + 1]
    Assert-Equal '--threads' $r.LaunchArgs[$sep + 2]
    Assert-Equal '1' $r.LaunchArgs[$sep + 3]
}

Test-Case 'a leading "--" typed by the caller is not doubled into a second "--" for cargo run' {
    $r = Resolve-RunTarget -Bin 'pangloss' -ExtraArgs @('--', 'batch', '--threads', '1')
    $count = @($r.LaunchArgs | Where-Object { $_ -eq '--' }).Count
    Assert-Equal 1 $count "expected exactly one '--' separator, got $count in: $($r.LaunchArgs -join ' ')"
}

Test-Case 'no passthrough args means no trailing "--" at all for cargo run' {
    $r = Resolve-RunTarget -Bin 'pangloss'
    Assert-False (@($r.LaunchArgs) -contains '--') 'an empty passthrough list must not still emit a bare --'
}

# --- Get-ExhaustionConsumersFromMessage: parsing Microsoft-Windows-Resource-Exhaustion-Detector text ---

# Real message text captured from a System log event ID 2004, not a synthetic string.
$script:RealExhaustionMessage = 'Windows successfully diagnosed a low virtual memory condition. The following programs consumed the most virtual memory: predict_census.exe (30004) consumed 118387073024 bytes, vmmemCmZygote (9984) consumed 853762048 bytes, and MsMpEng.exe (5320) consumed 529256448 bytes.'

Test-Case 'parses all three consumers out of a real captured message' {
    $consumers = Get-ExhaustionConsumersFromMessage -Message $script:RealExhaustionMessage
    Assert-Equal 3 $consumers.Count
    Assert-Equal 'predict_census.exe' $consumers[0].ProcessName
    Assert-Equal 30004 $consumers[0].Pid
    Assert-Equal 118387073024 $consumers[0].Bytes
    Assert-Equal 'vmmemCmZygote' $consumers[1].ProcessName
    Assert-Equal 'MsMpEng.exe' $consumers[2].ProcessName
}

Test-Case 'GB is derived from bytes, not a separate unit the message never actually provides' {
    # The real message only ever says "bytes"; GB is our own conversion, not a second thing parsed from the text.
    $consumers = Get-ExhaustionConsumersFromMessage -Message $script:RealExhaustionMessage
    Assert-Equal 110.3 $consumers[0].GB
}

Test-Case 'an empty or null message parses to zero consumers, not an error' {
    Assert-Equal 0 (Get-ExhaustionConsumersFromMessage -Message '').Count
    Assert-Equal 0 (Get-ExhaustionConsumersFromMessage -Message $null).Count
}

Test-Case 'unrecognized message text parses to zero consumers rather than throwing' {
    # Best-effort by design: Microsoft publishes no stable grammar, so a wording change must degrade, never crash.
    $consumers = Get-ExhaustionConsumersFromMessage -Message 'some totally different message shape'
    Assert-Equal 0 $consumers.Count
}

Test-Case 'a two-consumer message (fewer than the usual three) still parses correctly' {
    # The real detector reports "top N" -- do not assume the message always names exactly three.
    $msg = 'Windows successfully diagnosed a low virtual memory condition. The following programs consumed the most virtual memory: foo.exe (111) consumed 1073741824 bytes, and bar.exe (222) consumed 2147483648 bytes.'
    $consumers = Get-ExhaustionConsumersFromMessage -Message $msg
    Assert-Equal 2 $consumers.Count
    Assert-Equal 'foo.exe' $consumers[0].ProcessName
    Assert-Equal 1.0 $consumers[0].GB
    Assert-Equal 'bar.exe' $consumers[1].ProcessName
    Assert-Equal 2.0 $consumers[1].GB
}

# --- Get-ResourceExhaustionEvents: the live-query wrapper must never throw, on any machine ---

Test-Case 'Get-ResourceExhaustionEvents never throws, and always reports Queryable one way or the other' {
    # The one test that touches the real machine/log; asserts only the contract, never the live content.
    $r = Get-ResourceExhaustionEvents -Since ((Get-Date).AddDays(-7))
    Assert-True ($null -ne $r.Queryable) 'Queryable must always be present (true or false), never absent'
    Assert-True ($null -ne $r.Events) 'Events must always be an (possibly empty) collection, never null'
    if (-not $r.Queryable) {
        Assert-True ($r.Events.Count -eq 0) 'an unqueryable result must report zero events, not fabricated ones'
    }
}

# --- Split-ExtraArgsSpec: the binder-proof passthrough channel; see pg.ps1's own header for why it exists. ---

Test-Case 'a simple spec splits on whitespace' {
    $a = Split-ExtraArgsSpec '-p pg-foma --no-capture'
    Assert-Equal 3 $a.Count "got: $($a -join '|')"
    Assert-Equal '-p' $a[0]
    Assert-Equal 'pg-foma' $a[1]
    Assert-Equal '--no-capture' $a[2]
}

Test-Case 'double-quoted values survive as one argument' {
    # A filter expression or a path with spaces must not be split into pieces cargo cannot parse.
    $a = Split-ExtraArgsSpec '-E "test(foo bar)" --release'
    Assert-Equal 3 $a.Count "got: $($a -join '|')"
    Assert-Equal 'test(foo bar)' $a[1] 'quotes are stripped, the inner spaces preserved'
}

Test-Case 'runs of whitespace collapse rather than producing empty arguments' {
    # An empty string argument reaching cargo is a hard error, so this must never emit one.
    $a = Split-ExtraArgsSpec "  -p   foo`t--flag  "
    Assert-Equal 3 $a.Count "got: $($a -join '|')"
    Assert-False (@($a | Where-Object { $_ -eq '' }).Count -gt 0) 'no empty arguments'
}

Test-Case 'an empty or absent spec yields no arguments, not a one-element blank' {
    Assert-Equal 0 @(Split-ExtraArgsSpec '').Count
    Assert-Equal 0 @(Split-ExtraArgsSpec $null).Count
}

Test-Case 'a bare -- in the spec is preserved for cargo, not eaten' {
    # The env channel is a raw argv carrier, so cargo's own `--` separator must arrive intact -- no binder is involved.
    $a = Split-ExtraArgsSpec '-- --nocapture'
    Assert-Equal 2 $a.Count "got: $($a -join '|')"
    Assert-Equal '--' $a[0]
}

Write-TestSummary
