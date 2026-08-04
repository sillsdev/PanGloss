<#
  Counts comment-hygiene violations in Rust sources and fails when a category grows.

  A hard "zero violations" gate is unusable against a large existing backlog, and a gate that
  cannot pass gets disabled and then protects nothing. So this is a RATCHET: the baseline records
  the current count per category and the run fails only if a category goes UP. Cleanup lowers the
  baseline; the number cannot climb back.

  Usage:
    rust\tools\comment-hygiene.ps1            # check against the baseline
    rust\tools\comment-hygiene.ps1 -List      # show the offending lines
    rust\tools\comment-hygiene.ps1 -Update    # re-baseline after a cleanup pass

  Rules enforced are stated in .claude/skills/code-comments/SKILL.md. Summary: a comment explains
  why; code says what, git says when, the plan says where the project is. Project state in a
  source comment is true when written and unchecked forever after.
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [switch]$List,
    [switch]$Update,
    [string]$BaselinePath = ''
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if (-not $BaselinePath) { $BaselinePath = Join-Path $PSScriptRoot 'comment-hygiene-baseline.json' }

# Each category is scored separately so one kind of cleanup cannot silently pay for another kind of
# regression -- a single total would let 50 new plan references hide behind 50 deleted dates.
$categories = [ordered]@{
    'plan-reference'  = 'openspec[/\\]changes|tasks\.md|design\.md|docs/fst-plan|IMPLEMENTATION-READINESS|spec\.md'
    'step-marker'     = 'Step \d+ of \d+|Step \d+ \(|§P\d|Phase [A-Z]\b|Stage \d[A-Z]?\b|task \d+\.\d+|D\d+ decision'
    'wiring-status'   = 'purely additive|Purely additive|not wired|NOT wired|reachable from no|Reachable from no|not yet consumed|Not yet consumed'
    'date-in-comment' = '\b20\d\d-\d\d-\d\d\b'
    'history-prose'   = 'used to read|previously read|this paragraph|renamed from|was stale|corrected in place'
}

# Comment lines only. A plan path inside a string literal is usually a real file the code opens.
# `#` covers PowerShell and Python; the block-comment forms cover Rust and PowerShell's <# #>.
$commentLine = '^\s*(//|///|//!|\*|/\*|#|<#)'

# Tooling is included, not just crates. The first version scanned only `rust/crates` and so missed
# every violation in the scripts that enforce the rule -- a checker exempt from its own check.
$files = @(
    Get-ChildItem -Path (Join-Path $repoRoot 'rust\crates') -Filter '*.rs' -Recurse -File
    Get-ChildItem -Path (Join-Path $repoRoot 'rust\tools') -Filter '*.ps1' -Recurse -File
    Get-ChildItem -Path (Join-Path $repoRoot '.claude\hooks') -Filter '*.py' -File -ErrorAction SilentlyContinue
) | Where-Object { $_.FullName -notmatch '\\target\\' }

$counts = [ordered]@{}
$hits = @{}
foreach ($cat in $categories.Keys) { $counts[$cat] = 0; $hits[$cat] = @() }

foreach ($f in $files) {
    $lineNo = 0
    foreach ($line in [System.IO.File]::ReadLines($f.FullName)) {
        $lineNo++
        if ($line -notmatch $commentLine) { continue }
        foreach ($cat in $categories.Keys) {
            if ($line -match $categories[$cat]) {
                $counts[$cat]++
                if ($List) {
                    $rel = $f.FullName.Substring($repoRoot.Length + 1)
                    $hits[$cat] += "$rel`:$lineNo`: $($line.Trim())"
                }
            }
        }
    }
}

if ($List) {
    foreach ($cat in $categories.Keys) {
        if ($hits[$cat].Count -eq 0) { continue }
        Write-Host "";  Write-Host "### $cat ($($counts[$cat]))" -ForegroundColor Cyan
        $hits[$cat] | Select-Object -First 40 | ForEach-Object { Write-Host "  $_" }
        if ($hits[$cat].Count -gt 40) { Write-Host "  ... $($hits[$cat].Count - 40) more" }
    }
    Write-Host ''
}

if ($Update) {
    ($counts | ConvertTo-Json) | Set-Content -Path $BaselinePath -Encoding utf8
    Write-Host "[comment-hygiene] baseline written: $BaselinePath" -ForegroundColor Green
    foreach ($cat in $categories.Keys) { Write-Host ("  {0,-18} {1}" -f $cat, $counts[$cat]) }
    exit 0
}

if (-not (Test-Path $BaselinePath)) {
    # Absent baseline is NOT a pass. "I could not look" must never read as "everything is fine".
    Write-Host "[comment-hygiene] no baseline at $BaselinePath -- run with -Update to create one." -ForegroundColor Red
    exit 2
}

$baseline = Get-Content $BaselinePath -Raw | ConvertFrom-Json
$regressed = @()
$improved = @()
foreach ($cat in $categories.Keys) {
    $was = [int]$baseline.$cat
    $now = [int]$counts[$cat]
    $mark = if ($now -gt $was) { $regressed += "$cat`: $was -> $now"; 'WORSE' }
            elseif ($now -lt $was) { $improved += "$cat`: $was -> $now"; 'better' }
            else { 'same' }
    $color = if ($now -gt $was) { 'Red' } elseif ($now -lt $was) { 'Green' } else { 'Gray' }
    Write-Host ("  {0,-18} {1,5}  (baseline {2,5})  {3}" -f $cat, $now, $was, $mark) -ForegroundColor $color
}

if ($regressed.Count -gt 0) {
    Write-Host ''
    Write-Host '[comment-hygiene] FAILED -- a category grew:' -ForegroundColor Red
    $regressed | ForEach-Object { Write-Host "    $_" -ForegroundColor Red }
    Write-Host '[comment-hygiene] See .claude/skills/code-comments/SKILL.md. Run -List to find them.' -ForegroundColor Yellow
    exit 1
}

if ($improved.Count -gt 0) {
    Write-Host ''
    Write-Host '[comment-hygiene] improved -- re-baseline with -Update so the gain is locked in:' -ForegroundColor Green
    $improved | ForEach-Object { Write-Host "    $_" -ForegroundColor Green }
}
exit 0
