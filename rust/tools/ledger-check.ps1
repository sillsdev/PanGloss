<#
  .DESCRIPTION
  Mechanical checks for the optimization ledger (docs/research/optimization/README.md).

    -Mode Lint  (default) Ledger lint, offline: every table row has an allowed status, every
                `rejected` row carries a killing number with a source, every entry link resolves, and
                idea names are unique. Exit 1 with one message per failure naming the row and the fix.
    -Mode Pr    PR "Ledger:" line: when the changed files touch the parser paths, the PR body must hold
                `Ledger: <row name>` naming an existing row, or `Ledger: n/a (<reason>)`.
                Reads -Body (or $env:PR_BODY) and -ChangedFile (or one path per line in -ChangedFilesPath).

  The format is .claude/skills/optimization-research/references/ledger-format.md. Dot-source this
  file to reuse the functions (rust/tools/tests/ledger-check.tests.ps1 does).
#>
[CmdletBinding()]
param(
    [ValidateSet('Lint', 'Pr')][string]$Mode = 'Lint',
    [string]$LedgerPath = '',
    [string]$Body = $env:PR_BODY,
    [string[]]$ChangedFile = @(),
    [string]$ChangedFilesPath = ''
)

$script:LedgerStatuses = @('idea', 'measured', 'issue-filed', 'ready', 'in-progress', 'landed', 'rejected', 'open-upstream')
# `open` predates the status list and is used by many rows; it reads as "issue exists, no work yet".
$script:LedgerLegacyStatuses = @('open')

# The one place that lists the paths whose change needs a `Ledger:` line (HermitCrab analysis and
# synthesis). A path matches when it starts with an entry ending in `/`, or equals / starts with
# any other entry (so `traverse` covers traverse.rs and traverse/).
$script:LedgerParserPaths = @(
    'rust/crates/pg-rules/src/',
    'rust/crates/pg-parse/src/',
    'rust/crates/pg-realize/src/',
    'rust/crates/pg-shape/src/',
    'rust/crates/pg-featstruct/src/',
    'rust/crates/pg-grammar/src/compile',
    'rust/crates/pg-fst/src/traverse'
)

function Get-LedgerSlug {
    param([string]$Heading)
    $s = $Heading.Trim().ToLowerInvariant()
    $s = [regex]::Replace($s, '[^\p{L}\p{N} _-]', '')
    return ($s -replace ' ', '-')
}

function Get-LedgerRowName {
    # The row's idea cell without a trailing parenthetical, lower case.
    param([string]$Cell)
    $c = $Cell.Trim().Trim('`')
    $c = [regex]::Replace($c, '\s*\([^)]*\)\s*$', '')
    return $c.Trim().ToLowerInvariant()
}

function Read-Ledger {
    param([Parameter(Mandatory)][string]$Path)
    $lines = Get-Content -LiteralPath $Path
    $rows = @()
    $sections = @{}   # slug -> body text
    $inTable = $false
    $current = $null
    $level = 0
    for ($i = 0; $i -lt $lines.Count; $i++) {
        $line = $lines[$i]
        $h = [regex]::Match($line, '^(#{1,6})\s+(.+?)\s*$')
        if ($h.Success) {
            $current = Get-LedgerSlug $h.Groups[2].Value
            $level = $h.Groups[1].Value.Length
            $sections[$current] = @{ Level = $level; Text = '' }
            $inTable = $false
            continue
        }
        if ($line -match '^\|\s*Idea\s*\|\s*Status\s*\|') { $inTable = $true; continue }
        if ($inTable -and $line -match '^\|') {
            if ($line -match '^\|\s*-{2,}') { continue }
            # Split on pipes that are not inside backticks.
            $cells = [regex]::Split($line.Trim().Trim('|'), '\s\|\s') | ForEach-Object { $_.Trim() }
            $rows += [pscustomobject]@{ Line = $i + 1; Cells = @($cells); Raw = $line }
            continue
        }
        if ($inTable) { $inTable = $false }
        if ($current) { $sections[$current].Text += $line + "`n" }
    }
    # A section's text includes its sub-sections.
    $order = @($sections.Keys)
    return [pscustomobject]@{ Rows = $rows; Sections = $sections; Lines = $lines }
}

function Get-LedgerSectionText {
    # Text under a heading, including deeper headings, up to the next heading of the same or higher level.
    param([string[]]$Lines, [string]$Slug)
    $out = ''
    $capturing = $false
    $level = 0
    foreach ($line in $Lines) {
        $h = [regex]::Match($line, '^(#{1,6})\s+(.+?)\s*$')
        if ($h.Success) {
            $lv = $h.Groups[1].Value.Length
            if ($capturing -and $lv -le $level) { break }
            if (-not $capturing -and (Get-LedgerSlug $h.Groups[2].Value) -eq $Slug) { $capturing = $true; $level = $lv; continue }
        }
        if ($capturing) { $out += $line + "`n" }
    }
    return $out
}

function Test-LedgerStatus {
    param([string]$Cell)
    $c = $Cell.Trim().Trim('`').ToLowerInvariant()
    foreach ($s in ($script:LedgerStatuses + $script:LedgerLegacyStatuses)) {
        # Any trailing qualifier is allowed: '(...)', ', ...', or plain words ('rejected as ported').
        if ($c -match ('^' + [regex]::Escape($s) + '(?![\w-])')) { return $s }
    }
    return $null
}

function Test-LedgerKillingNumber {
    # A numeric value with a unit or a ratio, and a source (issue, PR, divergence, command or file).
    param([string]$Text)
    $number = '(?i)([+-]?\d+(\.\d+)?\s*(%|x\b|×)|\b\d+(\.\d+)?\s+(steps?|instructions?|work|words?|analyses|allocations?|bytes|ms|s|of \d+)\b|\b\d+(\.\d+)?\s*(MiB|KiB|GiB)\b|\b\d{1,3}(,\d{3})+\b|\bzero\s+\w+)'
    $source = '(?i)(#\d+|\bPR\s*\d+|divergence\s+\d+|`[^`]*(pg\.ps1|cargo|pangloss|stile|\.rs|\.md|\.ps1)[^`]*`|docs/\S+)'
    return ($Text -match $number) -and ($Text -match $source)
}

function Test-LedgerFile {
    param([Parameter(Mandatory)][string]$Path)
    $errors = @()
    $dir = Split-Path -Parent $Path
    $ledger = Read-Ledger -Path $Path
    if ($ledger.Rows.Count -eq 0) {
        return @("ledger: no table found in $Path; the table header must start '| Idea | Status |'")
    }
    $seen = @{}
    foreach ($row in $ledger.Rows) {
        $cells = $row.Cells
        $where = "ledger row at line $($row.Line)"
        if ($cells.Count -lt 5) {
            $errors += "$where : expected 5 columns (Idea | Status | Target blowup | Issues | Entry), found $($cells.Count). Fix: use one pipe per column and no bare '|' inside a cell."
            continue
        }
        $name = Get-LedgerRowName $cells[0]
        $where = "ledger row '$($cells[0])' (line $($row.Line))"
        if ($seen.ContainsKey($name)) {
            $errors += "$where : duplicate idea name, first seen at line $($seen[$name]). Fix: update the existing row instead of adding another."
        } else { $seen[$name] = $row.Line }

        $status = Test-LedgerStatus $cells[1]
        if (-not $status) {
            $errors += "$where : status '$($cells[1])' must start with one of $($script:LedgerStatuses -join ', ') (open is also accepted), optionally followed by a qualifier such as '(...)' or ', ...'. Fix: begin the Status cell with the status word."
        }

        $link = [regex]::Match($cells[4], '\]\(([^)]+)\)')
        $entryText = ''
        if (-not $link.Success) {
            $errors += "$where : the Entry cell has no link. Fix: link '[entry](#<heading-anchor>)' or '[file](<name>.md)'."
        } else {
            $target = $link.Groups[1].Value
            if ($target.StartsWith('#')) {
                $slug = $target.Substring(1)
                if (-not $ledger.Sections.ContainsKey($slug)) {
                    $errors += "$where : entry anchor '$target' matches no heading in the ledger. Fix: add a '### ...' heading whose GitHub slug is '$slug', or correct the link."
                } else { $entryText = Get-LedgerSectionText -Lines $ledger.Lines -Slug $slug }
            } else {
                $file = ($target -split '#')[0]
                $full = Join-Path $dir $file
                if (-not (Test-Path -LiteralPath $full -PathType Leaf)) {
                    $errors += "$where : entry file '$file' does not exist next to the ledger. Fix: create docs/research/optimization/$file or correct the link."
                } else { $entryText = Get-Content -Raw -LiteralPath $full }
            }
        }

        if ($status -eq 'rejected') {
            $text = ($cells -join ' ') + "`n" + $entryText
            if (-not (Test-LedgerKillingNumber $text)) {
                $errors += "$where : a rejected idea needs its killing number: a value with a unit or a ratio (for example '+7.7% work', '1.1x', '0 steps') and a source (issue, PR or command) in the row or its entry. Fix: add both to the Status cell or the entry."
            }
        }
    }
    return $errors
}

function Test-LedgerPathIsParser {
    param([string]$Path)
    $p = $Path.Replace('\', '/')
    foreach ($root in $script:LedgerParserPaths) {
        if ($p.StartsWith($root, [StringComparison]::Ordinal)) { return $true }
    }
    return $false
}

function Test-LedgerPrBody {
    # Returns @() when the PR passes, otherwise the failure messages.
    param([string]$Body, [string[]]$ChangedFiles, [string[]]$RowNames)
    $touched = @($ChangedFiles | Where-Object { $_ -and (Test-LedgerPathIsParser $_) })
    if ($touched.Count -eq 0) { return @() }
    $example = "Fix: add a line 'Ledger: <row name>' (a row of docs/research/optimization/README.md) or 'Ledger: n/a (<reason>)' to the PR description."
    $m = [regex]::Matches([string]$Body, '(?im)^[ \t>*-]*Ledger:[ \t]*(.*?)[ \t]*$')
    if ($m.Count -eq 0) {
        return @("PR changes parser code ($($touched[0])$(if ($touched.Count -gt 1) { " and $($touched.Count - 1) more" })) but its description has no 'Ledger:' line. $example")
    }
    $errs = @()
    $ok = $false
    foreach ($x in $m) {
        $v = $x.Groups[1].Value.Trim().Trim('`')
        if ($v -match '(?i)^n/a\s*\(\s*\S.*\)$') { $ok = $true; continue }
        if ($v -match '(?i)^n/a') { $errs += "'Ledger: $v' needs a reason in parentheses, for example 'Ledger: n/a (tooling)'."; continue }
        $n = Get-LedgerRowName $v
        if ($n -and ($RowNames -contains $n)) { $ok = $true } else { $errs += "'Ledger: $v' names no ledger row. Known rows are the Idea column of docs/research/optimization/README.md; add the row first or use 'Ledger: n/a (<reason>)'." }
    }
    if ($ok) { return @() }
    return $errs
}

if ($MyInvocation.InvocationName -ne '.') {
    $repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
    if (-not $LedgerPath) { $LedgerPath = Join-Path $repoRoot 'docs/research/optimization/README.md' }
    if ($Mode -eq 'Lint') {
        $errs = @(Test-LedgerFile -Path $LedgerPath)
        foreach ($e in $errs) { Write-Host "ledger-lint: $e" }
        if ($errs.Count) { Write-Host "ledger-lint: $($errs.Count) problem(s); format: .claude/skills/optimization-research/references/ledger-format.md"; exit 1 }
        Write-Host 'ledger-lint: ok'
        exit 0
    }
    $files = @($ChangedFile)
    if ($ChangedFilesPath) { $files += @(Get-Content -LiteralPath $ChangedFilesPath) }
    $names = @((Read-Ledger -Path $LedgerPath).Rows | ForEach-Object { Get-LedgerRowName $_.Cells[0] })
    $errs = @(Test-LedgerPrBody -Body $Body -ChangedFiles $files -RowNames $names)
    foreach ($e in $errs) { Write-Host "::error::$e" }
    if ($errs.Count) { exit 1 }
    Write-Host 'ledger-line: ok'
    exit 0
}
