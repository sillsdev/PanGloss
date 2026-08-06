<#
  Counts comment-hygiene violations in Rust sources and fails when a category grows.

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

  BLOCK RULES ARE RUST-ONLY, on purpose rather than for convenience: every valid escape above is a
  Rust mechanism. PowerShell and Python have no machine-checked anchor, so the rule would there be a
  cap with no way to comply. Those files keep the line-level categories.
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [switch]$List,
    [int]$ListLimit = 400
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

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
#
# WORD-BOUNDARIED, and `unreachable` excludes the macro form. Substring matching made identifiers
# collide with claims: `unreachable!()` and a local `const UNREACHABLE_KIND` both tripped `unreachable`,
# and two agents reworded correct technical vocabulary to satisfy the regex. That is the same failure
# this file already records for `stage 1`/`phase a` -- a gate that pressures a cleanup pass into
# damaging accurate prose is worse than no gate. `\b` alone fixes `UNREACHABLE_KIND` (the `_` is a word
# character, so the boundary fails); the macro needs the explicit `(?!!)` because `!` is not.
$claimVerbs = @(
    '\brefuses\b', '\brejects\b', '\baccepts unconditionally\b', '\balready refuses\b',
    '\bonly caller\b', '\bsole caller\b', '\bcalled from exactly\b', '\bzero callers\b',
    '\bno production caller\b', '\bunreachable\b(?!!)', '\bnever called\b', '\bnever reached\b',
    '\bcannot happen\b', '\bcannot be reached\b', '\balways returns\b', '\bnever returns\b',
    '\bstays on\b', '\bis not wired\b', '\bnever fires\b', '\balways fires\b'
) -join '|'

# A citation to a test that pins the claim. This exists because the checker previously PUNISHED ITS
# OWN PREFERRED FIX: the skill ranks "cite the pinning test" ahead of adding a link, but only `[`..`]`
# suppressed a claim, so following the policy's first choice left the hit standing -- and self-defeated
# when the cited test was itself named `..._rejects_...`. All four sweep agents hit this.
#
# The citation must name something that EXISTS: the name is checked against every `fn` in the tree, so
# a citation is machine-verified rather than taken on faith. That is not a nicety -- this pass found
# two comments citing tests (`overwrite_group_composes_to_refuse`,
# `right_to_left_predicate_refuses_quantifier_shaped_rule`) that exist nowhere, both asserting the
# OPPOSITE of the prose citing them. `dead-citation` makes that class catchable instead of luck.
#
# The name must be BACKTICKED and contain an underscore. Without both, the phrase matches ordinary
# prose -- "pins this", "checked by the loader" -- and measured 122 hits that were almost entirely the
# checker's own noise rather than dead citations. Requiring a code span makes a citation deliberate,
# and requiring `_` distinguishes a snake_case fn from an English word.
#
# Backticks are safe to require now only BECAUSE the claim verbs above became word-boundaried: a test
# named `..._refuses_...` no longer trips `\brefuses\b` (the surrounding `_` is a word character), so
# citing it in a code span no longer re-flags the very line that resolves the claim. Those two changes
# have to land together; either alone reintroduces the trap the agents hit.
#
# `(?i)` is REQUIRED and is not belt-and-braces. These patterns are evaluated with
# [regex]::Matches, which is CASE-SENSITIVE by default -- the exact opposite of PowerShell's `-match`
# used for the line-level categories above, whose case-INsensitivity is itself documented below as a
# past bug. Both defaults have now caused one. Without `(?i)` a sentence starting "Pinned by `x`" is
# not recognised as a citation, so the anchor silently fails to count and the block reads as
# unanchored. Caught by falsification, not by review.
$citationPhrase = '(?i)(?:pinned by|pins|asserted by|witnessed by|proved by|checked by)\s+`([a-z_][A-Za-z0-9_]*_[A-Za-z0-9_]*)`'

# The `path/to/file.rs::test_name` form this repo already uses for curated evidence citations. Checked
# the same way and for the same reason: two such citations in this tree named tests that exist nowhere,
# and both were cited as proving a REFUSAL by prose sitting above a live test asserting ConfirmOnly. A
# citation nobody resolves is indistinguishable from a citation that is simply wrong.
#
# The FILE is captured too, and a citation is only judged when that file exists in this tree. Measured
# without that guard: 23 hits, essentially all false -- citations into `foma-rs` (`apply_init`,
# `flag_purge`, `flag_build`) and into ported C# test names, none of which this index can see because
# it scans only this repo's crates. Flagging them would report "dead citation" for references that are
# perfectly good, just not local. Same rule this file already applies to docs links: "I could not look"
# must not read as "it is broken."
#
# Names ending in `_` are skipped as line-wrap artifacts: a citation split across two comment lines is
# truncated at the newline, which produced `..._confirms_the_reversed_tag_`.
$citationPath = '(?i)([A-Za-z0-9_./\\-]+\.rs)::([a-z_][A-Za-z0-9_]*)'
$identToken = '`[A-Za-z_][A-Za-z0-9_]*(::[A-Za-z0-9_]+)*(\(\))?`'
$intraDocLink = '\[`[^`]+`\]'

# The ONLY grounds on which an implementation comment may exceed one line. An author must CLAIM one
# by name; there is no generic escape.
#
# Enumerated deliberately, and narrow. A free-form marker becomes universal and then means nothing --
# this file already records that failure mode for a different gate. A closed set is different in kind:
# you cannot invent a class, each claim is falsifiable ("what change does this TRAP prevent?"), and the
# per-class counts are printed, so inflation of any one class is visible rather than silent.
#
# The set comes from what long comments demonstrably bought here, and from the standard advice that
# comments must carry what code cannot -- a design decision's rationale, the conditions under which a
# call makes sense, and hazards. Descriptive prose is absent on purpose: the code, the LSP and git
# already provide it.
$exceptionTags = @(
    'SAFETY:'   # An unsafe block's proof obligation. The ONLY class that buys extra lines.
)

# ONE class, and it survived a cull the others did not.
#
# The rule is one line. A reference document REPLACES a long comment rather than licensing one: if the
# knowledge needs a paragraph, it belongs in `docs/research/` and the comment is the single line that
# points there. Under that rule almost nothing needs an allowance, because a one-line comment requires
# no justification in the first place -- which is precisely what made the earlier tag set redundant.
#
# `TRAP:`, `INVARIANT:` and `WHY-NOT:` were dropped for exactly that reason. Each states something a
# single line can carry ("apply_up cost lives in abandoned branches, so a path count is a floor, not a
# bound"), and where it genuinely cannot, the argument is a document, not a comment. Keeping unused
# tags around would only offer three ready-made ways to buy length.
#
# `PORT-CORRESPONDENCE:` / `PORT-DIVERGENCE:` were dropped as LENGTH licenses but kept as review
# vocabulary in the skill: they classify a claim so a reviewer knows what to verify, and that value does
# not depend on the comment being long.
#
# `SAFETY:` stays because it is the one obligation with no external home. Measured: 209 unsafe sites in
# this tree, concentrated at the FFI boundary (json_api.rs 87, grammar.rs 25, parse.rs 12), with 14 of
# 19 crates forbidding unsafe entirely. An FFI precondition -- what the caller must guarantee about a
# pointer's validity and lifetime -- is a proof about THIS call site, so there is nothing to point at.
# It is also Rust's own convention and what clippy's undocumented_unsafe_blocks expects.

# PORT: earns its place on the same ground the skill already grants a paper or an upstream issue: the
# knowledge is DURABLE because its subject is external and frozen. This crate ports FieldWorks'
# HermitCrab, and the C# does not change under us -- so a comment recording "the CLR does X, we do Y,
# and here is why they differ" ages far better than one describing our own code, which is the thing
# that actually moves. One line cannot carry three clauses.
#
# It is measured, not assumed: 297 of the over-long private blocks mention the C# oracle in their FIRST
# line alone, the largest identifiable class by a wide margin, and that is a lower bound.
#
# SCOPE IT TIGHTLY WHEN REVIEWING. PORT: licenses a correspondence or a divergence. It does NOT license
# narrating what the C# does -- that is description, and description is what this whole rule exists to
# remove. A PORT: block that never says what THIS code does in response is misfiled.
#
# `FIXTURE:` was considered for the second-largest class (211 blocks of test-fixture rationale) and
# REJECTED, on this session's own evidence: a fixture was swapped and five rationale comments went
# stale describing the wrong grammar. Fixture rationale rots exactly the way this rule exists to
# prevent, because its subject is our own test data. If such a comment needs length it can cite the
# test, which the anchor rule already handles.

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

# `comment-block-too-long` is RETIRED and replaced by the two below. It applied one length to every
# comment regardless of kind, which is the wrong axis: an API docstring IS the abstraction and should
# run as long as the contract needs, while an implementation comment explains code the reader is
# already looking at. Conflating them produced 3,269 violations and no way to tell the two apart.
$blockCategories = @('impl-comment-too-long', 'unanchored-exception', 'cross-reference-claim', 'docs-link-broken', 'dead-citation')
foreach ($cat in $blockCategories) { $categories[$cat] = $null }

# Every `fn` name in the tree, so a "pinned by X" citation can be checked rather than trusted. One
# extra pass over files already being read; cheap enough not to need a cache.
$fnNames = [System.Collections.Generic.HashSet[string]]::new()
$rsBasenames = [System.Collections.Generic.HashSet[string]]::new()
foreach ($f in $files) {
    if ($f.Extension -ne '.rs') { continue }
    [void]$rsBasenames.Add($f.Name)
    foreach ($line in [System.IO.File]::ReadLines($f.FullName)) {
        # Not just `fn`: a curated citation legitimately names a fixture constant
        # (`f3_parity.rs::ENGINE_TIMEOUT`, `::GRAMMAR_XML`) or a type (`::TrieEdge`). Indexing only
        # functions reported five such live references as dead -- the same false-positive direction as
        # the foma-rs case, and the reason this category is worth nothing until its index is honest.
        foreach ($m in [regex]::Matches($line, '\b(?:fn|struct|enum|trait|union|type|const|static|mod)\s+([A-Za-z_][A-Za-z0-9_]*)')) {
            [void]$fnNames.Add($m.Groups[1].Value)
        }
    }
}

# Is THIS FILE's own module public? `//!` documents the module it sits in, so its visibility is
# declared in the PARENT file, not this one -- which is why a `//!` block could previously hide any
# amount of prose in a private module and be waved through as "API".
#
# Resolved by path rather than by matching module names globally: two crates can both have a `plan`
# module with different visibility, and a name-keyed map would silently pick one.
$publicModuleFile = [System.Collections.Generic.HashSet[string]]::new()
foreach ($f in $files) {
    if ($f.Extension -ne '.rs') { continue }
    $dir = $f.DirectoryName
    $stem = [System.IO.Path]::GetFileNameWithoutExtension($f.Name)
    # A crate root is the public front page by definition.
    if ($stem -eq 'lib' -or $stem -eq 'main') { [void]$publicModuleFile.Add($f.FullName); continue }
    # `tests/` and `examples/` are their own crate roots, but nobody consumes their docs -- they are not
    # an API, so their headers are held to the implementation cap like any other private prose.
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

# A checked pointer to somewhere the long form actually lives. Looser than Get-BlockAnchor on purpose:
# this only buys a SECOND LINE, whereas an anchor licenses a whole SAFETY: argument, so it admits an
# external URL that cannot be validated offline. A paper or an upstream issue is durable in the way
# this rule cares about even though no local check can follow it.
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

# Is this block an API docstring or an implementation comment? The standard distinction (Ousterhout's
# interface vs implementation documentation; Java's doc vs implementation comments) is what decides
# the cap, so getting it right is the whole point of this function.
#
# `//!` is a module/crate front page and always counts as API. `///` documents the NEXT item, so the
# item's visibility decides -- and attributes and blank lines sit between the two, which is why this
# skips forward rather than reading one line. Anything else (`//`, `/* */`) is implementation.
function Get-BlockKind {
    param([string]$FirstLine, [string[]]$AllLines, [int]$EndIdx, [bool]$InPublicModule)
    if ($FirstLine -match '^\s*//!') { if ($InPublicModule) { return 'api' } else { return 'impl' } }
    if ($FirstLine -notmatch '^\s*///') { return 'impl' }
    for ($j = $EndIdx; $j -lt [Math]::Min($EndIdx + 12, $AllLines.Count); $j++) {
        $l = $AllLines[$j]
        if ($l -match '^\s*#!?\[') { continue }
        if ($l.Trim() -eq '') { continue }
        # `pub(crate)` counts as API: it is a real interface for every other module in the crate, and
        # its callers are exactly as unable to see the body as an external caller is.
        #
        # But `pub` INSIDE A PRIVATE MODULE reaches nobody -- the module gate closes over it, so the
        # item is effectively private and its doc is implementation documentation. Requiring both is
        # what makes "is this an interface?" mean reachability rather than just spelling.
        if ($l -match '^\s*pub(\s|\()') { if ($InPublicModule) { return 'api' } else { return 'impl' } }
        return 'impl'
    }
    return 'impl'
}

# Returns the anchor kind a block carries, or $null. A docs path counts only under docs/research/
# AND only if the file is really there -- an anchor nobody can follow is not an anchor.
function Get-BlockAnchor {
    param([string]$Text, [string]$RepoRoot)
    # An intra-doc link is deliberately NOT an anchor any more. It only ever proved that a path
    # resolved, so it licensed length without licensing truth -- and treating it as an anchor rewarded
    # adding links at the same time the repo concluded code-to-code links should be deleted (the LSP
    # already navigates; 551 broken ones had gone unnoticed). The three anchors left all survive
    # semantic drift.
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
            # Citations first: a verified "pinned by <fn>" is an anchor in its own right, and the
            # STRONGEST one, because a test is the only falsifier that survives semantic drift. It
            # must therefore be computed before the block-length check consults $anchor.
            $blockCited = $false
            $cited = [System.Collections.Generic.List[string]]::new()
            foreach ($m in [regex]::Matches($text, $citationPhrase)) { $cited.Add($m.Groups[1].Value) }
            foreach ($m in [regex]::Matches($text, $citationPath)) {
                # Only judge a citation whose file is one of ours; see $citationPath's own note.
                $base = [System.IO.Path]::GetFileName(($m.Groups[1].Value -replace '/', '\'))
                if ($rsBasenames.Contains($base)) { $cited.Add($m.Groups[2].Value) }
            }
            foreach ($name in $cited) {
                if ($name.EndsWith('_')) { continue }
                # A citation wrapped across two comment lines is captured truncated, and the break is
                # not always at an underscore -- `..._on_subrule` + `_finding` on the next line reads
                # as a complete name. So a captured name that is a PREFIX of a real one counts as live.
                # This trades a little strictness for removing a whole false-positive class; a gate that
                # cries wolf on correct citations gets ignored, and then the real one is ignored with it.
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

            $kind = Get-BlockKind -FirstLine $blockLines[0] -AllLines $allLines -EndIdx ($lineNo - 1) `
                -InPublicModule $publicModuleFile.Contains($f.FullName)
            if ($kind -eq 'api') {
                # An API docstring may run as long as the contract needs. Counted, never gated: the
                # number is worth watching, but capping an interface is how you destroy the abstraction.
                if ($blockLines.Count -gt $MaxBlockLines) { $apiDocsLong++ }
            } elseif ($blockLines.Count -gt 1) {
                $tag = $null
                foreach ($t in $exceptionTags) {
                    if ($text -match ('(?m)^\s*(?:///|//!|//|\*)\s*' + [regex]::Escape($t))) { $tag = $t; break }
                }
                # TWO lines when one of them is a checked reference: the pointer gets its own line so the
                # other can say WHY you would follow it. A bare `see docs/research/<topic>.md` is close
                # to useless -- the reader cannot tell whether it is worth the detour -- and forcing the
                # summary and the pointer to share one line is how you get neither.
                #
                # The angle brackets in that example are load-bearing: a literal path here made the
                # checker flag its own documentation as a dead link, which is the second time this file
                # has caught its own prose.
                #
                # Deliberately capped at two. Three would reintroduce room for an argument, and the
                # argument is the thing that belongs in the document.
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
                    # A claim buys three lines on the tag alone; past that it must also be falsifiable.
                    # The graduation matters: the short form covers "state the hazard", while anything
                    # longer is an argument, and an argument is exactly what rots without a test.
                    if ($blockLines.Count -gt $MaxBlockLines -and -not $anchor) {
                        $counts['unanchored-exception']++
                        if ($List) {
                            $hits['unanchored-exception'] +=
                                "$rel`:$blockStart`: $($blockLines.Count) lines, $tag claimed but no anchor"
                        }
                    }
                }
            }
            # A doc block on a #[test] is SELF-ANCHORING for this category: the claim's falsifier is the
            # very item the block documents, sitting directly beneath it. "This test proves X refuses Y"
            # cannot rot unnoticed the way the same sentence can three modules away -- if it stops being
            # true, the test fails. Demanding a `pinned by` citation there would ask a test to cite
            # itself, which was three of the four hits when this was measured.
            $documentsATest = $false
            for ($j = $lineNo - 1; $j -lt [Math]::Min($lineNo + 11, $allLines.Count); $j++) {
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

# ZERO TOLERANCE, not a ratchet. The ratchet was the right instrument against an inherited backlog
# nobody had budgeted to clear -- it stopped the number growing while cleanup happened. It is the wrong
# instrument now, for two reasons the ratchet itself surfaced: a baseline records the CURRENT count as
# acceptable, so 4,330 violations read as "passing"; and re-baselining after a rule change quietly
# relabels old debt as the new normal, which happened twice in one session here.
#
# Every violation is reported and every violation must go. Locally this is a WARNING (pg.ps1 prints it
# and never fails the build -- a documentation finding that blocks every managed build is the gate shape
# this repo has watched get switched off). In CI it is fatal: run this script directly and honour the
# exit code.
$total = 0
foreach ($cat in $categories.Keys) {
    $now = [int]$counts[$cat]
    $total += $now
    $color = if ($now -gt 0) { 'Red' } else { 'Green' }
    Write-Host ("  {0,-24} {1,5}" -f $cat, $now) -ForegroundColor $color
}

# Reported, never gated. Per class on purpose: a closed set of exceptions is only trustworthy while
# you can see which one is being leaned on. `TRAP:` quietly becoming the most-claimed class would mean
# it has turned into the generic escape hatch this design exists to avoid.
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
