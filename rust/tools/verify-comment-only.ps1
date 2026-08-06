<#
  .DESCRIPTION
  Proves that a change which claims to touch only comments actually touches only comments.

  Why this exists, precisely. A comment sweep's single Edit removed 204 lines from
  `orthogonal_basis_group_a.rs` and added 2. Only the first ~102 removed lines were the module doc it
  meant to shorten; the rest were the `use` block, two type definitions and four `const` items. The
  file stopped parsing and the whole pg-foma integration suite went with it. Nothing caught it: the
  ad-hoc verification in use at the time inspected ADDED lines only -- it asked "is everything you
  wrote a comment?", which a pure deletion answers trivially and correctly. Deleting code is
  invisible to that question.

  So the rule here is symmetric: every line the diff ADDS and every line it REMOVES must be a comment
  line or blank. A comment-only edit that deletes a `use` statement is not a comment-only edit no
  matter how the deletion got there.

  This is a diff-shape check, not a semantic one, and the difference matters when reading a green
  result. It cannot tell a good comment from a bad one -- `comment-hygiene.ps1` does that -- and it
  cannot tell that a deleted comment SHOULD have been kept. What it can tell you, cheaply and with no
  compiler, is that the code is untouched, which is the property a sweep is actually claiming.

  A trailing comment is the one shape the line-start test gets wrong on its own: editing
  `let x = 1; // note` is a comment-only edit, but the line as a whole is code. So a second pass
  excuses any changed line whose CODE PORTION appears unchanged on the other side of the diff.
  Matching is per-file and per-code-portion, so a line that really changed has no partner to excuse
  it. Without this the tool fails entirely legitimate sweeps, and a gate that taxes ordinary work
  gets switched off and then protects nobody.

  Three things it reports that are TRUE but usually benign, so a reader is not surprised by them.
  rustfmt reflow, which `pg.ps1` applies on every managed build: shortening a comment can let the
  next statement fit on one line, and that is a code change. Prose inside a string literal, such as
  an assertion's failure message or an XML fixture's own `<!-- -->` comment -- prose to a human, code
  to the compiler. And the deletion of a dummy item (`const _: () = ();`) that existed only to anchor
  a doc comment which has since moved to `docs/research/`. All three want a glance, not a revert.

  Usage:
    rust\tools\verify-comment-only.ps1                    # working tree vs HEAD
    rust\tools\verify-comment-only.ps1 -Staged            # index vs HEAD
    rust\tools\verify-comment-only.ps1 -Range HEAD~3..HEAD
    rust\tools\verify-comment-only.ps1 -Path rust/crates/pg-foma

  Exit 0 clean, 1 on any violation, 2 if git itself failed. An agent running a comment sweep should
  run this after EVERY file and honour the exit code; it is the cheapest possible guard between an
  over-selected `old_string` and a broken build.
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [string]$Range,
    [switch]$Staged,
    [string[]]$Path,
    [int]$ListLimit = 200
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '_comment-lines.ps1')

$gitArgs = @('diff', '-U0', '--no-color', '--no-ext-diff')
if ($Staged) { $gitArgs += '--cached' }
if ($Range)  { $gitArgs += $Range }
if ($Path)   { $gitArgs += '--'; $gitArgs += $Path }

$diff = & git @gitArgs 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "git diff failed:`n$diff" -ForegroundColor Red
    exit 2
}

$candidates = New-Object System.Collections.Generic.List[object]
$violations = New-Object System.Collections.Generic.List[object]

# A `-U0` diff line carries no context, so both sides' full contents are masked and indexed by line
# number instead. See docs/research/comment-hygiene-checker-design.md
$oldRef, $newRef = if ($Range -match '^(.+?)\.\.\.?(.+)$') { $Matches[1], $Matches[2] }
                   elseif ($Staged) { 'HEAD', ':' }
                   else { 'HEAD', '' }
$maskCache = @{}
function Get-SideMask {
    param([string]$File, [string]$Ref)
    $key = "$Ref`u{1}$File"
    if ($maskCache.ContainsKey($key)) { return $maskCache[$key] }
    $spec = if ($Ref -eq ':') { ":$File" } elseif ($Ref -eq '') { $null } else { "${Ref}:$File" }
    $lines = if ($null -eq $spec) {
        if (Test-Path $File) { @(Get-Content -LiteralPath $File) } else { @() }
    } else {
        $out = & git show $spec 2>$null
        if ($LASTEXITCODE -ne 0) { @() } else { @($out) }
    }
    $m = Get-CommentLineMask -Lines $lines -Extension ([System.IO.Path]::GetExtension($File))
    $maskCache[$key] = $m
    return $m
}
$unclassified = New-Object System.Collections.Generic.List[string]
$stats = [ordered]@{}

$file = $null
$pattern = $null
$oldNo = 0
$newNo = 0

foreach ($line in $diff) {
    if ($line -match '^\+\+\+ (?:b/)?(.+)$') {
        $p = $Matches[1]
        # A pure deletion of a whole file shows `+++ /dev/null`; keep the `--- a/<path>` name for it.
        if ($p -ne '/dev/null') { $file = $p }
        $ext = [System.IO.Path]::GetExtension($file)
        $pattern = $commentLineByExt[$ext]
        if (-not $pattern -and -not $unclassified.Contains($file)) { $unclassified.Add($file) }
        if (-not $stats.Contains($file)) { $stats[$file] = [pscustomobject]@{ Added = 0; Removed = 0 } }
        continue
    }
    if ($line -match '^--- (?:b/|a/)?(.+)$') {
        if ($Matches[1] -ne '/dev/null') { $file = $Matches[1] }
        continue
    }
    if ($line -match '^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@') {
        $oldNo = [int]$Matches[1]
        $newNo = [int]$Matches[2]
        continue
    }
    if (-not $file) { continue }

    $sign = if ($line.Length -gt 0) { $line[0] } else { ' ' }
    if ($sign -ne '+' -and $sign -ne '-') { continue }
    $text = $line.Substring(1)

    if ($sign -eq '+') { $stats[$file].Added++;   $no = $newNo; $newNo++ }
    else               { $stats[$file].Removed++; $no = $oldNo; $oldNo++ }

    if (-not $pattern) { continue }          # not a language we classify; reported separately
    if ($text.Trim() -eq '') { continue }    # blank-line churn is not code
    $mask = if ($sign -eq '+') { Get-SideMask -File $file -Ref $newRef } else { Get-SideMask -File $file -Ref $oldRef }
    if ($no -ge 1 -and $no -le $mask.Count -and $mask[$no - 1]) { continue }

    $candidates.Add([pscustomobject]@{
        File = $file
        Side = if ($sign -eq '+') { 'added' } else { 'REMOVED' }
        Line = $no
        Text = $text.Trim()
        Code = Get-CodePortion -Text $text -Token $lineCommentTokenByExt[[System.IO.Path]::GetExtension($file)]
    })
}

# Second pass: excuse a code line whose code portion is unchanged (see this script's help header).
$codeOnOtherSide = @{}
foreach ($c in $candidates) {
    $key = "$($c.File)`u{1}$($c.Side)`u{1}$($c.Code)"
    $codeOnOtherSide[$key] = $true
}
foreach ($c in $candidates) {
    $other = if ($c.Side -eq 'added') { 'REMOVED' } else { 'added' }
    if ($c.Code -ne '' -and $codeOnOtherSide.ContainsKey("$($c.File)`u{1}$other`u{1}$($c.Code)")) { continue }
    $violations.Add($c)
}

Write-Host ''
Write-Host 'verify-comment-only' -ForegroundColor Cyan
$scope = if ($Staged) { 'index vs HEAD' } elseif ($Range) { $Range } else { 'working tree vs HEAD' }
Write-Host "  scope: $scope"

if ($stats.Count -eq 0) {
    Write-Host '  no changes in scope.' -ForegroundColor Green
    exit 0
}

foreach ($k in $stats.Keys) {
    $s = $stats[$k]
    # Printed even when clean: a lopsided deletion count is the signature the rule cannot judge.
    Write-Host ("  {0,6} {1,-7} {2}" -f "+$($s.Added)", "-$($s.Removed)", $k)
}

if ($unclassified.Count -gt 0) {
    Write-Host ''
    Write-Host "  not classified (no comment syntax known; NOT verified): $($unclassified.Count)" -ForegroundColor Yellow
    $unclassified | Select-Object -First 20 | ForEach-Object { Write-Host "    $_" }
}

if ($violations.Count -eq 0) {
    Write-Host ''
    Write-Host '  PASS: every added and removed line is a comment or blank.' -ForegroundColor Green
    exit 0
}

$removed = @($violations | Where-Object { $_.Side -eq 'REMOVED' })
Write-Host ''
Write-Host "  FAIL: $($violations.Count) non-comment line(s) changed, $($removed.Count) of them DELETED." -ForegroundColor Red
Write-Host ''
foreach ($v in ($violations | Select-Object -First $ListLimit)) {
    $colour = if ($v.Side -eq 'REMOVED') { 'Red' } else { 'Yellow' }
    Write-Host ("    {0}:{1} {2,-7} {3}" -f $v.File, $v.Line, $v.Side, $v.Text) -ForegroundColor $colour
}
if ($violations.Count -gt $ListLimit) {
    Write-Host "    ... $($violations.Count - $ListLimit) more"
}
Write-Host ''
Write-Host '  Revert the offending file with `git checkout -- <file>` and redo the edit in smaller' -ForegroundColor Red
Write-Host '  pieces, anchoring the end of `old_string` on the last comment line, never on the code' -ForegroundColor Red
Write-Host '  that follows it.' -ForegroundColor Red
exit 1
