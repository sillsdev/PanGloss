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

  The categories divide into two families, and the second is the one worth understanding.

  LINE-LEVEL (plan-reference, step-marker, wiring-status, date-in-comment, history-prose) score a
  single comment line against a regex. These catch project state: facts that were true when typed
  and are never checked again.

  BLOCK-LEVEL (comment-block-too-long, cross-reference-claim, docs-link-broken) score a run of
  consecutive comment lines. They exist because the line-level family provably missed a live defect:
  six comments asserted a function stayed on one lowering scope while the code passed another for
  eight days, and not one of them was a date, a plan reference, a step marker, a wiring-status
  phrase, or history prose. A comment that is simply FALSE ABOUT BEHAVIOR is a different class.

  Length is not why such a comment rots -- `// x refuses y anyway` is one line and rots identically.
  What rots is an unverifiable CLAIM ABOUT ANOTHER CODE ENTITY. So the block rules sort comments by
  whether a machine can falsify them:

    - an intra-doc link [`foo::bar`]  -- rustdoc's broken_intra_doc_links checks the path resolves
    - a ``` doctest                   -- cargo test EXECUTES it; the only comment form that cannot
                                         silently go stale
    - a docs/research/*.md path       -- checked here to exist on disk

  A block over $MaxBlockLines lines is legal only if it carries one of those anchors. That is
  deliberately not a bare marker: a marker you can type becomes universal and then means nothing,
  which is this repo's documented failure mode for any gate that taxes ordinary work. Anchored long
  blocks are counted and reported separately (never gated) so the escape hatch cannot quietly become
  the norm without showing up as a number.

  Honest limit: a link proves the path resolves, not that the sentence about it is true. Renames and
  deletions are caught; semantic drift is not. Only a test closes that, which is why the doctest and
  named-test anchors are the preferred ones and why docs/ is restricted to research -- durable
  external knowledge (a paper, an algorithm, an upstream bug) rather than internal behavior, since a
  docs page describing internal behavior rots exactly as the comment did with nothing compiling it.

  BLOCK RULES ARE RUST-ONLY, on purpose rather than for convenience: every valid escape above is a
  Rust mechanism. PowerShell and Python have no machine-checked anchor, so the rule would there be a
  cap with no way to comply. Those files keep the line-level categories.
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [switch]$List,
    [switch]$Update,
    [int]$ListLimit = 400,
    [string]$BaselinePath = ''
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if (-not $BaselinePath) { $BaselinePath = Join-Path $PSScriptRoot 'comment-hygiene-baseline.json' }

# One claim plus the falsifier that keeps it honest fits in three lines. One line cannot hold both,
# and a claim with no named falsifier is exactly what went stale for eight days.
$MaxBlockLines = 3

# Each category is scored separately so one kind of cleanup cannot silently pay for another kind of
# regression -- a single total would let 50 new plan references hide behind 50 deleted dates.
$categories = [ordered]@{
    'plan-reference'  = 'openspec[/\\]changes|tasks\.md|design\.md|docs/fst-plan|IMPLEMENTATION-READINESS|spec\.md'
    # `Phase`/`Stage` are case-SENSITIVE via `(?-i:...)`; everything else stays case-insensitive.
    # PowerShell's `-match` ignores case, and this codebase uses "stage 1"/"stage 2" for real
    # algorithm structure (propose then confirm), so a blanket match flags correct domain vocabulary
    # and pressures a cleanup pass into rewriting it. Scoping the two patterns rather than making the
    # whole line case-sensitive is deliberate: a capitalised task number must still be caught too.
    'step-marker'     = 'Step \d+ of \d+|Step \d+ \(|§P\d|(?-i:Phase [A-Z]\b)|(?-i:Stage \d[A-Z]?\b)|task \d+\.\d+|D\d+ decision'
    'wiring-status'   = 'purely additive|Purely additive|not wired|NOT wired|reachable from no|Reachable from no|not yet consumed|Not yet consumed'
    'date-in-comment' = '\b20\d\d-\d\d-\d\d\b'
    'history-prose'   = 'used to read|previously read|this paragraph|renamed from|was stale|corrected in place'
}

# A behavioral assertion about a NAMED entity other than the line of code below. Requires both a
# claim verb and a backticked identifier, so ordinary prose about the local statement is not caught.
# The fix is never deletion by default: turn it into an intra-doc link, or into the test that pins it.
#
# PRESENT-TENSE ACTIVE ONLY, and that is the whole precision of this category. "`x` refuses `y`" is a
# claim about what another entity does right now, which is what silently stopped being true. Past
# tense ("`load` rejected the grammar") almost always documents what an error variant MEANS, not a
# live cross-reference, and including it measured ~25% false positives on this tree. `guaranteed` is
# out for the same reason: "never guaranteed SMALL" is prose about a value, not a claim about a callee.
$claimVerbs = @(
    'refuses', 'rejects', 'accepts unconditionally', 'already refuses',
    'only caller', 'sole caller', 'called from exactly', 'zero callers', 'no production caller',
    'unreachable', 'never called', 'never reached', 'cannot happen', 'cannot be reached',
    'always returns', 'never returns', 'stays on', 'is not wired', 'never fires', 'always fires'
) -join '|'
$identToken = '`[A-Za-z_][A-Za-z0-9_]*(::[A-Za-z0-9_]+)*(\(\))?`'
$intraDocLink = '\[`[^`]+`\]'

# Comment lines only. A plan path inside a string literal is usually a real file the code opens.
#
# Per-language, not one union pattern: a shared `#` alternative matches every Rust ATTRIBUTE
# (`#[derive(Debug)]`), which cost 238 phantom long blocks -- runs of attributes scored as comment
# prose, and attributes adjacent to a doc block silently extended it. A block rule is only as good as
# its notion of where a block ends. The `\*` form requires a following space or `/` so a Rust
# dereference statement (`*x = 5;`) is not read as a block-comment continuation either.
$commentLineByExt = @{
    '.rs'   = '^\s*(///|//!|//|/\*|\*(\s|/|$))'
    '.ps1'  = '^\s*(#|<#)'
    '.py'   = '^\s*#'
}

# Tooling is included, not just crates. The first version scanned only `rust/crates` and so missed
# every violation in the scripts that enforce the rule -- a checker exempt from its own check.
$files = @(
    Get-ChildItem -Path (Join-Path $repoRoot 'rust\crates') -Filter '*.rs' -Recurse -File
    Get-ChildItem -Path (Join-Path $repoRoot 'rust\tools') -Filter '*.ps1' -Recurse -File
    Get-ChildItem -Path (Join-Path $repoRoot '.claude\hooks') -Filter '*.py' -File -ErrorAction SilentlyContinue
) | Where-Object { $_.FullName -notmatch '\\target\\' }

$blockCategories = @('comment-block-too-long', 'cross-reference-claim', 'docs-link-broken')
foreach ($cat in $blockCategories) { $categories[$cat] = $null }

$counts = [ordered]@{}
$hits = @{}
foreach ($cat in $categories.Keys) { $counts[$cat] = 0; $hits[$cat] = @() }
$anchoredLongBlocks = 0

# Returns the anchor kind a block carries, or $null. A docs path counts only under docs/research/
# AND only if the file is really there -- an anchor nobody can follow is not an anchor.
function Get-BlockAnchor {
    param([string]$Text, [string]$RepoRoot)
    if ($Text -match '\[`[^`]+`\]') { return 'intra-doc-link' }
    if ($Text -match '```') { return 'doctest' }
    if ($Text -match 'include_str!') { return 'include-str' }
    foreach ($m in [regex]::Matches($Text, '(?:rust/)?docs/research/[A-Za-z0-9._/\-]+\.md')) {
        $p = $m.Value -replace '/', '\'
        if ((Test-Path (Join-Path $RepoRoot $p)) -or (Test-Path (Join-Path $RepoRoot (Join-Path 'rust' $p)))) {
            return 'docs-research'
        }
    }
    return $null
}

foreach ($f in $files) {
    $isRust = $f.Extension -eq '.rs'
    $commentLine = $commentLineByExt[$f.Extension]
    if (-not $commentLine) { continue }
    $rel = $f.FullName.Substring($repoRoot.Length + 1)
    $lineNo = 0
    $blockLines = New-Object System.Collections.Generic.List[string]
    $blockStart = 0

    # A block ends at the first non-comment line (or EOF), so the sentinel below runs the same
    # evaluation once more after the loop rather than duplicating it.
    $allLines = @([System.IO.File]::ReadLines($f.FullName)) + @('<<EOF-SENTINEL>>')

    foreach ($line in $allLines) {
        $lineNo++
        $isComment = ($line -ne '<<EOF-SENTINEL>>') -and ($line -match $commentLine)

        if ($isComment) {
            if ($blockLines.Count -eq 0) { $blockStart = $lineNo }
            $blockLines.Add($line)

            foreach ($cat in $categories.Keys) {
                if ($null -eq $categories[$cat]) { continue }
                if ($line -match $categories[$cat]) {
                    $counts[$cat]++
                    if ($List) { $hits[$cat] += "$rel`:$lineNo`: $($line.Trim())" }
                }
            }
            continue
        }

        if ($blockLines.Count -eq 0) { continue }
        $text = $blockLines -join "`n"

        if ($isRust) {
            $anchor = Get-BlockAnchor -Text $text -RepoRoot $repoRoot
            if ($blockLines.Count -gt $MaxBlockLines) {
                if ($anchor) {
                    $anchoredLongBlocks++
                } else {
                    $counts['comment-block-too-long']++
                    if ($List) {
                        $hits['comment-block-too-long'] +=
                            "$rel`:$blockStart`: $($blockLines.Count) lines, no anchor: $($blockLines[0].Trim())"
                    }
                }
            }
            foreach ($bl in $blockLines) {
                if ($bl -match $intraDocLink) { continue }
                if (($bl -match $claimVerbs) -and ($bl -match $identToken)) {
                    $counts['cross-reference-claim']++
                    if ($List) { $hits['cross-reference-claim'] += "$rel`:$blockStart`: $($bl.Trim())" }
                }
            }
        }

        # Applies to every language: a docs path that does not resolve is broken wherever it appears.
        #
        # Both `docs/...` and `rust/docs/...` are written in this tree, so a match is resolved against
        # the repo root AND against `rust/`. Capturing only from `docs/` and testing one location
        # reported three live files as missing -- the inverse of this repo's "I could not look must
        # not read as everything is fine": I looked in the wrong place and called the file broken.
        foreach ($m in [regex]::Matches($text, '(?:rust/)?docs/[A-Za-z0-9._/\-]+\.md')) {
            $p = $m.Value -replace '/', '\'
            $found = (Test-Path (Join-Path $repoRoot $p)) -or (Test-Path (Join-Path $repoRoot (Join-Path 'rust' $p)))
            if (-not $found) {
                $counts['docs-link-broken']++
                if ($List) { $hits['docs-link-broken'] += "$rel`:$blockStart`: missing $($m.Value)" }
            }
        }

        $blockLines.Clear()
    }
}

if ($List) {
    foreach ($cat in $categories.Keys) {
        if ($hits[$cat].Count -eq 0) { continue }
        Write-Host "";  Write-Host "### $cat ($($counts[$cat]))" -ForegroundColor Cyan
        # High enough to list a whole category: truncating at 40 silently hid 56 of 96 hits while a
        # cleanup pass was being partitioned from this output, which is how work goes unassigned.
        $hits[$cat] | Select-Object -First $ListLimit | ForEach-Object { Write-Host "  $_" }
        if ($hits[$cat].Count -gt $ListLimit) { Write-Host "  ... $($hits[$cat].Count - $ListLimit) more" }
    }
    Write-Host ''
}

if ($Update) {
    ($counts | ConvertTo-Json) | Set-Content -Path $BaselinePath -Encoding utf8
    Write-Host "[comment-hygiene] baseline written: $BaselinePath" -ForegroundColor Green
    foreach ($cat in $categories.Keys) { Write-Host ("  {0,-24} {1}" -f $cat, $counts[$cat]) }
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
    # A category absent from the baseline is new. Scoring it against an implicit 0 would fail every
    # run until someone re-baselines, so it reports as unbaselined and gates nothing yet.
    if ($null -eq $baseline.PSObject.Properties[$cat]) {
        Write-Host ("  {0,-24} {1,5}  (no baseline)  new -- run -Update to start its ratchet" -f $cat, $counts[$cat]) -ForegroundColor Yellow
        continue
    }
    $was = [int]$baseline.$cat
    $now = [int]$counts[$cat]
    $mark = if ($now -gt $was) { $regressed += "$cat`: $was -> $now"; 'WORSE' }
            elseif ($now -lt $was) { $improved += "$cat`: $was -> $now"; 'better' }
            else { 'same' }
    $color = if ($now -gt $was) { 'Red' } elseif ($now -lt $was) { 'Green' } else { 'Gray' }
    Write-Host ("  {0,-24} {1,5}  (baseline {2,5})  {3}" -f $cat, $now, $was, $mark) -ForegroundColor $color
}

# Reported, never gated: this is how many long blocks bought their length with a machine-checked
# anchor. It climbing while comment-block-too-long falls means the escape hatch is becoming the norm.
Write-Host ("  {0,-24} {1,5}  (informational, not gated)" -f 'long-blocks-anchored', $anchoredLongBlocks) -ForegroundColor Gray

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
