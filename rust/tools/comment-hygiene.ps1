<#
  .DESCRIPTION
  Counts comment-hygiene violations in Rust sources and PowerShell tooling, and fails if any remain.

  ZERO TOLERANCE. Every violation is reported and every one is meant to go; there is no accepted
  count and no baseline file.

  This replaced a ratchet, which was the right instrument while an inherited backlog was being worked
  down and is the wrong one now. A baseline records the CURRENT count as acceptable -- so 4,330
  violations printed as "passing" -- and re-baselining after a rule change quietly relabels old debt as
  the new normal, which happened twice in one session.

  Locally this is a WARNING: `pg.ps1` prints the result on every managed build and never fails on it,
  because a documentation finding that blocks every build is the gate shape this repo has already
  watched get switched off. In CI it is FATAL: invoke this script directly and honour the exit code.

  Usage:
    rust\tools\comment-hygiene.ps1            # report; exit 1 if any violation exists
    rust\tools\comment-hygiene.ps1 -List      # show the offending lines

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

  SCRIPTS ARE HELD TO THE SAME RULES, and the interface/implementation split is what makes that
  possible. PowerShell COMMENT-BASED HELP -- the documented interface -- is the exact analogue of a
  Rust doc comment on a public item, so it gets the same treatment: scored for stale project state,
  never capped for length. Every `#` run, and every delimited block that is not help, is an
  implementation comment and takes the one-line cap.

  TWO CONDITIONS, and the second is not optional. The block must sit at the top of a script or at a
  function's own head, AND it must carry a help keyword (`.SYNOPSIS`, `.DESCRIPTION`, ...). Position
  alone was the first version of this rule and it was WRONG: measured across this tree, 67 delimited
  blocks sat in a help position, 0 carried a keyword, and Get-Help returned nothing for a single one
  of them. They were block comments wearing help's clothes. Worse, position alone is a typeable
  marker -- wrap any comment in a delimited block, put it at the head of a function, and the cap is
  gone -- which is this file's own documented failure mode for any escape hatch. Requiring the
  keyword makes "the documented interface" a fact Get-Help will confirm rather than a claim about
  where the text sits.

  Not every anchor survives the crossing: a doctest and a `pinned by <fn>` citation are Rust
  mechanisms and simply never occur in a script. What remains is a `docs/research/*.md` path or a
  URL, which is enough for the cap to be compliable -- and the escape a script actually wants is
  usually to move the argument up into its own help header, where length is free.

  A delimited block's BODY lines carry no marker of their own, so a line-start pattern reads them as
  code and the whole body escapes every rule. That was the state until this was written: 387 body
  lines -- every script header in the tree -- unscored, hiding five dates and a plan reference.
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [switch]$List,
    [int]$ListLimit = 400
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

# A claim plus its falsifier fits in three lines; one line cannot hold both.
$MaxBlockLines = 3

# Scored separately so one kind of cleanup cannot silently pay for another kind of regression.
$categories = [ordered]@{
    # Left-boundaried: unanchored, these match the tail of a legitimate research filename.
    'plan-reference'  = 'openspec[/\\]changes|(?<![\w-])(tasks|design|spec)\.md|docs/fst-plan|IMPLEMENTATION-READINESS'
    # `Phase`/`Stage` are case-SENSITIVE so "stage 1"/"stage 2" as real algorithm vocabulary survives.
    # See docs/research/comment-hygiene-checker-design.md
    'step-marker'     = 'Step \d+ of \d+|Step \d+ \(|§P\d|(?-i:Phase [A-Z]\b)|(?-i:Stage \d[A-Z]?\b)|task \d+\.\d+|D\d+ decision'
    'wiring-status'   = 'purely additive|Purely additive|not wired|NOT wired|reachable from no|Reachable from no|not yet consumed|Not yet consumed'
    'date-in-comment' = '\b20\d\d-\d\d-\d\d\b'
    'history-prose'   = 'used to read|previously read|this paragraph|renamed from|was stale|corrected in place'
}

# Present-tense active only, word-boundaried, `unreachable` excluding the macro: each narrowing fixed
# a measured false-positive class. See docs/research/comment-hygiene-checker-design.md
$claimVerbs = @(
    '\brefuses\b', '\brejects\b', '\baccepts unconditionally\b', '\balready refuses\b',
    '\bonly caller\b', '\bsole caller\b', '\bcalled from exactly\b', '\bzero callers\b',
    '\bno production caller\b', '\bunreachable\b(?!!)', '\bnever called\b', '\bnever reached\b',
    '\bcannot happen\b', '\bcannot be reached\b', '\balways returns\b', '\bnever returns\b',
    '\bstays on\b', '\bis not wired\b', '\bnever fires\b', '\balways fires\b'
) -join '|'

# A machine-checked citation to the test that pins a claim; `(?i)` is load-bearing because
# [regex]::Matches is case-SENSITIVE. See docs/research/comment-hygiene-checker-design.md
$citationPhrase = '(?i)(?:pinned by|pins|asserted by|witnessed by|proved by|checked by)\s+`([a-z_][A-Za-z0-9_]*_[A-Za-z0-9_]*)`'

# The `file.rs::test_name` citation form; only judged when the named file is one of ours, so
# "I could not look" never reads as "it is broken". See docs/research/comment-hygiene-checker-design.md
$citationPath = '(?i)([A-Za-z0-9_./\\-]+\.rs)::([a-z_][A-Za-z0-9_]*)'
$identToken = '`[A-Za-z_][A-Za-z0-9_]*(::[A-Za-z0-9_]+)*(\(\))?`'
$intraDocLink = '\[`[^`]+`\]'

# The only grounds on which an implementation comment may exceed one line: a closed, named set, so a
# class cannot be invented. See docs/research/comment-hygiene-checker-design.md
$exceptionTags = @(
    'SAFETY:'   # An unsafe block's proof obligation. The ONLY class that buys extra lines.
)

# One class survived the cull; TRAP:/INVARIANT:/WHY-NOT:/PORT-* and FIXTURE: were all rejected, each
# for a recorded reason. See docs/research/comment-hygiene-checker-design.md

# Comment lines only, shared with verify-comment-only.ps1 so the two cannot disagree.
. (Join-Path $PSScriptRoot '_comment-lines.ps1')

# Tooling is scanned too: the first version covered only `rust/crates` and so exempted the scripts
# that enforce the rule. See docs/research/comment-hygiene-checker-design.md
$files = @(
    Get-ChildItem -Path (Join-Path $repoRoot 'rust\crates') -Filter '*.rs' -Recurse -File
    Get-ChildItem -Path (Join-Path $repoRoot 'rust\tools') -Filter '*.ps1' -Recurse -File
    Get-ChildItem -Path (Join-Path $repoRoot '.claude\hooks') -Filter '*.py' -File -ErrorAction SilentlyContinue
) | Where-Object { $_.FullName -notmatch '\\target\\' }

# `comment-block-too-long` is RETIRED: one length for every comment was the wrong axis.
# See docs/research/comment-hygiene-checker-design.md
$blockCategories = @('impl-comment-too-long', 'unanchored-exception', 'cross-reference-claim', 'docs-link-broken', 'dead-citation')
foreach ($cat in $blockCategories) { $categories[$cat] = $null }

# Every declared name in the tree, so a citation is checked rather than trusted.
$fnNames = [System.Collections.Generic.HashSet[string]]::new()
$rsBasenames = [System.Collections.Generic.HashSet[string]]::new()
foreach ($f in $files) {
    if ($f.Extension -ne '.rs') { continue }
    [void]$rsBasenames.Add($f.Name)
    foreach ($line in [System.IO.File]::ReadLines($f.FullName)) {
        # Not just `fn`: a citation legitimately names a fixture constant or a type, and indexing
        # only functions reported five live references as dead. See docs/research/comment-hygiene-checker-design.md
        foreach ($m in [regex]::Matches($line, '\b(?:fn|struct|enum|trait|union|type|const|static|mod)\s+([A-Za-z_][A-Za-z0-9_]*)')) {
            [void]$fnNames.Add($m.Groups[1].Value)
        }
    }
}

# Is THIS FILE's own module public? `//!` visibility is declared in the PARENT file, and is resolved
# by path because two crates can both have a `plan` module. See docs/research/comment-hygiene-checker-design.md
$publicModuleFile = [System.Collections.Generic.HashSet[string]]::new()
foreach ($f in $files) {
    if ($f.Extension -ne '.rs') { continue }
    $dir = $f.DirectoryName
    $stem = [System.IO.Path]::GetFileNameWithoutExtension($f.Name)
    # A crate root is the public front page by definition.
    if ($stem -eq 'lib' -or $stem -eq 'main') { [void]$publicModuleFile.Add($f.FullName); continue }
    # `tests/`/`examples/` are crate roots nobody consumes the docs of, so they are not an API.
    if ($f.FullName -match '\\(tests|examples|benches)\\') { continue }
    if ($stem -eq 'mod') {
        $stem = Split-Path $dir -Leaf
        $dir = Split-Path $dir -Parent
    }
    $parents = @(
        (Join-Path $dir 'mod.rs'),
        (Join-Path $dir 'lib.rs'),
        (Join-Path $dir 'main.rs'),
        ((Join-Path (Split-Path $dir -Parent) ((Split-Path $dir -Leaf) + '.rs')))
    )
    foreach ($p in $parents) {
        if (-not (Test-Path $p)) { continue }
        $decl = Select-String -Path $p -Pattern ('^\s*(pub(\([^)]*\))?\s+)?mod\s+' + [regex]::Escape($stem) + '\s*;') -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($decl) {
            if ($decl.Line -match '^\s*pub') { [void]$publicModuleFile.Add($f.FullName) }
            break
        }
    }
}

$counts = [ordered]@{}
$hits = @{}
foreach ($cat in $categories.Keys) { $counts[$cat] = 0; $hits[$cat] = @() }
$apiDocsLong = 0
$referenceBacked = 0
$claimed = [ordered]@{}
foreach ($t in $exceptionTags) { $claimed[$t] = 0 }

# A checked pointer to where the long form lives; looser than an anchor because it buys only a
# second line. See docs/research/comment-hygiene-checker-design.md
function Get-BlockReference {
    param([string]$Text, [string]$RepoRoot, [bool]$Cited)
    if ($Cited) { return 'test-citation' }
    if ($Text -match 'https?://') { return 'url' }
    foreach ($m in [regex]::Matches($Text, '(?:rust/)?docs/[A-Za-z0-9._/\-]+\.md')) {
        $p = $m.Value -replace '/', '\'
        if ((Test-Path (Join-Path $RepoRoot $p)) -or (Test-Path (Join-Path $RepoRoot (Join-Path 'rust' $p)))) {
            return 'docs'
        }
    }
    return $null
}

# The script arm of the interface/implementation split: comment-based help is uncapped, everything
# else takes the one-line cap. See docs/research/comment-hygiene-checker-design.md
function Get-BlockKindScript {
    param([string]$FirstLine, [string]$Text, [string[]]$AllLines, [int]$StartIdx, [string]$Extension)
    # Python needs no keyword: a docstring is bound to `__doc__`, so there position IS the mechanism.
    # See docs/research/comment-hygiene-checker-design.md
    if ($Extension -eq '.py') {
        if ($FirstLine -notmatch '^\s*"""') { return 'impl' }
        for ($k = $StartIdx - 2; $k -ge 0; $k--) {
            $c = $AllLines[$k]
            if ($c.Trim() -eq '' -or $c -match '^#!') { continue }
            if ($c -match '^\s*(def|class)\s') { return 'api' }
            return 'impl'
        }
        return 'api'
    }
    if ($FirstLine -notmatch '^\s*<#') { return 'impl' }
    # Position is NOT enough: without a help keyword Get-Help renders nothing, and position alone is a
    # typeable marker. See docs/research/comment-hygiene-checker-design.md
    if ($Text -notmatch '(?m)^\s*\.(SYNOPSIS|DESCRIPTION|PARAMETER|EXAMPLE|NOTES|OUTPUTS|INPUTS|LINK|COMPONENT|ROLE|FUNCTIONALITY|FORWARDHELPTARGETNAME|EXTERNALHELP)\b') {
        return 'impl'
    }
    for ($k = $StartIdx - 2; $k -ge 0; $k--) {
        $c = $AllLines[$k]
        if ($c.Trim() -eq '') { continue }
        if ($c -match '^\s*function\s') { return 'api' }
        return 'impl'
    }
    return 'api'   # nothing above it: the file header
}

# Interface or implementation, for Rust: `//!` takes the module's visibility, `///` the next item's.
# See docs/research/comment-hygiene-checker-design.md
function Get-BlockKind {
    param([string]$FirstLine, [string[]]$AllLines, [int]$EndIdx, [bool]$InPublicModule)
    if ($FirstLine -match '^\s*//!') { if ($InPublicModule) { return 'api' } else { return 'impl' } }
    if ($FirstLine -notmatch '^\s*///') { return 'impl' }
    for ($j = $EndIdx; $j -lt [Math]::Min($EndIdx + 12, $AllLines.Count); $j++) {
        $l = $AllLines[$j]
        if ($l -match '^\s*#!?\[') { continue }
        if ($l.Trim() -eq '') { continue }
        # `pub(crate)` is API, but `pub` inside a PRIVATE module reaches nobody; requiring both makes
        # this mean reachability rather than spelling. See docs/research/comment-hygiene-checker-design.md
        if ($l -match '^\s*pub(\s|\()') { if ($InPublicModule) { return 'api' } else { return 'impl' } }
        # Enum variants and trait items INHERIT visibility and never carry `pub`; struct fields do not.
        # So walk out to the enclosing declaration rather than guessing. See docs/research/comment-hygiene-checker-design.md
        $itemIndent = ($l -replace '^(\s*).*', '$1').Length
        for ($k = $j - 1; $k -ge [Math]::Max(0, $j - 400); $k--) {
            $c = $AllLines[$k]
            if ($c.Trim() -eq '' -or $c -match '^\s*(///|//!|//)') { continue }
            if ((($c -replace '^(\s*).*', '$1').Length) -ge $itemIndent) { continue }
            if ($c -match '^\s*(pub(\([^)]*\))?\s+)?(enum|trait)\s+[A-Za-z_]') {
                if ($c -match '^\s*pub' -and $InPublicModule) { return 'api' }
                return 'impl'
            }
            # Any other enclosing construct: the item needed its own `pub` and did not have one.
            if ($c -match '^\s*(pub(\([^)]*\))?\s+)?(struct|union|impl|fn|mod)\b') { return 'impl' }
        }
        return 'impl'
    }
    return 'impl'
}

# The anchor a block carries, or $null; a docs path must really exist to count as one.
function Get-BlockAnchor {
    param([string]$Text, [string]$RepoRoot)
    # An intra-doc link is NOT an anchor: it licensed length without licensing truth. See docs/research/comment-hygiene-checker-design.md
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

    # The sentinel lets end-of-block evaluation run once after the loop instead of being duplicated.
    $realLines = @([System.IO.File]::ReadLines($f.FullName))
    $allLines = $realLines + @('<<EOF-SENTINEL>>')
    $delims = $blockCommentByExt[$f.Extension]
    # A delimited block's body lines carry no marker, so 387 of them were once scored by nothing.
    # See docs/research/comment-hygiene-checker-design.md
    $mask = Get-CommentLineMask -Lines $realLines -Extension $f.Extension

    foreach ($line in $allLines) {
        $lineNo++
        $isComment = ($line -ne '<<EOF-SENTINEL>>') -and $mask[$lineNo - 1]

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

        if ($isRust -or $delims) {
            # Citations first: a verified citation IS an anchor, so it must precede the length check.
            $blockCited = $false
            $cited = [System.Collections.Generic.List[string]]::new()
            # Rust only: $fnNames is a Rust symbol table, and a script was never written against it.
            foreach ($m in $(if ($isRust) { [regex]::Matches($text, $citationPhrase) } else { @() })) { $cited.Add($m.Groups[1].Value) }
            foreach ($m in $(if ($isRust) { [regex]::Matches($text, $citationPath) } else { @() })) {
                # Only judge a citation whose file is one of ours; see $citationPath's own note.
                $base = [System.IO.Path]::GetFileName(($m.Groups[1].Value -replace '/', '\'))
                if ($rsBasenames.Contains($base)) { $cited.Add($m.Groups[2].Value) }
            }
            foreach ($name in $cited) {
                if ($name.EndsWith('_')) { continue }
                # A citation wrapped across two lines is captured truncated, so a PREFIX of a real name
                # counts as live. See docs/research/comment-hygiene-checker-design.md
                if ($name.Length -ge 12) {
                    $wrapped = $false
                    foreach ($known in $fnNames) { if ($known.StartsWith($name)) { $wrapped = $true; break } }
                    if ($wrapped) { $blockCited = $true; continue }
                }
                if ($fnNames.Contains($name)) {
                    $blockCited = $true
                } else {
                    $counts['dead-citation']++
                    if ($List) { $hits['dead-citation'] += "$rel`:$blockStart`: cites unknown fn ``$name``" }
                }
            }
            $anchor = Get-BlockAnchor -Text $text -RepoRoot $repoRoot
            if (-not $anchor -and $blockCited) { $anchor = 'test-citation' }

            $kind = if ($isRust) {
                Get-BlockKind -FirstLine $blockLines[0] -AllLines $allLines -EndIdx ($lineNo - 1) `
                    -InPublicModule $publicModuleFile.Contains($f.FullName)
            } else {
                Get-BlockKindScript -FirstLine $blockLines[0] -Text $text -AllLines $allLines `
                    -StartIdx $blockStart -Extension $f.Extension
            }
            if ($kind -eq 'api') {
                # Counted, never gated: capping an interface is how you destroy the abstraction.
                if ($blockLines.Count -gt $MaxBlockLines) { $apiDocsLong++ }
            } elseif ($blockLines.Count -gt 1) {
                $tag = $null
                foreach ($t in $exceptionTags) {
                    if ($text -match ('(?m)^\s*(?:///|//!|//|\*|#)\s*' + [regex]::Escape($t))) { $tag = $t; break }
                }
                # TWO lines, so the pointer gets its own and the other can say why to follow it.
                # See docs/research/comment-hygiene-checker-design.md
                $reference = if ($blockLines.Count -eq 2) {
                    Get-BlockReference -Text $text -RepoRoot $repoRoot -Cited $blockCited
                } else { $null }
                if ($reference) {
                    $referenceBacked++
                } elseif (-not $tag) {
                    $counts['impl-comment-too-long']++
                    if ($List) {
                        $hits['impl-comment-too-long'] +=
                            "$rel`:$blockStart`: $($blockLines.Count) lines, no claim: $($blockLines[0].Trim())"
                    }
                } else {
                    $claimed[$tag]++
                    # A tag buys three lines; past that the argument must also be falsifiable.
                    if ($blockLines.Count -gt $MaxBlockLines -and -not $anchor) {
                        $counts['unanchored-exception']++
                        if ($List) {
                            $hits['unanchored-exception'] +=
                                "$rel`:$blockStart`: $($blockLines.Count) lines, $tag claimed but no anchor"
                        }
                    }
                }
            }
            # A doc on a #[test] is SELF-ANCHORING: its falsifier is the item directly beneath it.
            # See docs/research/comment-hygiene-checker-design.md
            $documentsATest = -not $isRust   # the claim category below is Rust-only; see its own note
            for ($j = $lineNo - 1; $isRust -and $j -lt [Math]::Min($lineNo + 11, $allLines.Count); $j++) {
                $l2 = $allLines[$j]
                if ($l2 -match '^\s*#\[test\]|^\s*#\[tokio::test\]') { $documentsATest = $true; break }
                if ($l2 -match '^\s*#!?\[') { continue }
                if ($l2.Trim() -eq '') { continue }
                break
            }

            foreach ($bl in $blockLines) {
                if ($blockCited -or $documentsATest) { break }
                if ($bl -match $intraDocLink) { continue }
                if (($bl -match $claimVerbs) -and ($bl -match $identToken)) {
                    $counts['cross-reference-claim']++
                    if ($List) { $hits['cross-reference-claim'] += "$rel`:$blockStart`: $($bl.Trim())" }
                }
            }
        }

        # Every language. Resolved against the repo root AND `rust/`, because testing one location
        # reported three live files as missing. See docs/research/comment-hygiene-checker-design.md
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
        # High enough to list a whole category: truncating at 40 once hid 56 of 96 hits.
        $hits[$cat] | Select-Object -First $ListLimit | ForEach-Object { Write-Host "  $_" }
        if ($hits[$cat].Count -gt $ListLimit) { Write-Host "  ... $($hits[$cat].Count - $ListLimit) more" }
    }
    Write-Host ''
}

# ZERO TOLERANCE, not a ratchet: a baseline records the current count as acceptable, and 4,330 once
# read as "passing". See docs/research/comment-hygiene-checker-design.md
$total = 0
foreach ($cat in $categories.Keys) {
    $now = [int]$counts[$cat]
    $total += $now
    $color = if ($now -gt 0) { 'Red' } else { 'Green' }
    Write-Host ("  {0,-24} {1,5}" -f $cat, $now) -ForegroundColor $color
}

# Reported never gated, and per class: you must be able to see which exception is being leaned on.
Write-Host ("  {0,-24} {1,5}  (informational, not gated)" -f 'api-docstrings-long', $apiDocsLong) -ForegroundColor Gray
Write-Host ("  {0,-24} {1,5}  (2-line: summary + checked reference)" -f 'reference-backed', $referenceBacked) -ForegroundColor Gray
foreach ($t in $exceptionTags) {
    Write-Host ("  {0,-24} {1,5}  (claimed exception)" -f ("  " + $t), $claimed[$t]) -ForegroundColor Gray
}

if ($total -gt 0) {
    Write-Host ''
    Write-Host "[comment-hygiene] $total violation(s). Every one must go -- there is no accepted count." -ForegroundColor Red
    Write-Host '[comment-hygiene] rules: .claude/skills/code-comments/SKILL.md   offenders: -List' -ForegroundColor Yellow
    exit 1
}

Write-Host ''
Write-Host '[comment-hygiene] clean.' -ForegroundColor Green
exit 0
