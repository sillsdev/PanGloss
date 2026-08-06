<#
  Which source lines are comment lines. Dot-sourced by `comment-hygiene.ps1` (to decide what to
  score) and by `verify-comment-only.ps1` (to decide what a comment-only edit is allowed to touch).

  Shared rather than duplicated because the two tools must agree BY CONSTRUCTION. If the verifier's
  notion of "comment" were even slightly wider than the checker's, a sweep could delete something the
  verifier waved through and the checker never sees -- which is exactly the failure this file exists
  to prevent, and exactly what happened on 2026-08-06.

  Per-language, not one union pattern: a shared `#` alternative matches every Rust ATTRIBUTE
  (`#[derive(Debug)]`), which cost 238 phantom long blocks in the checker. The `\*` form requires a
  following space or `/` so a Rust dereference statement (`*x = 5;`) is not read as a block-comment
  continuation either.
#>

$commentLineByExt = @{
    '.rs'   = '^\s*(///|//!|//|/\*|\*(\s|/|$))'
    '.ps1'  = '^\s*(#|<#)'
    '.py'   = '^\s*#'
}

# The token that starts a TRAILING comment on a code line. `verify-comment-only.ps1` needs this to
# tell `let x = 1; // old note` from `let x = 2; // old note`: only the first is a comment-only edit,
# and the whole line is code either way, so the line-start patterns above cannot decide it.
$lineCommentTokenByExt = @{
    '.rs'   = '//'
    '.ps1'  = '#'
    '.py'   = '#'
}

# Strip a trailing line comment, ignoring a token that falls inside a double-quoted string so that
# `let url = "http://x";` does not lose its value. Returns the code portion, trimmed.
#
# Honest limit: it does not model Rust raw strings (`r#"..."#`) or char literals. A mis-strip can
# only ever make two lines LOOK equal that were not, so the failure direction is a missed report,
# never a false alarm -- and the shapes it misses are rare enough that a stricter parser would cost
# more than it protects.
function Get-CodePortion {
    param([string]$Text, [string]$Token)
    if (-not $Token) { return $Text.Trim() }
    $inString = $false
    for ($i = 0; $i -lt $Text.Length; $i++) {
        $c = $Text[$i]
        if ($c -eq '\' -and $inString) { $i++; continue }
        if ($c -eq '"') { $inString = -not $inString; continue }
        if ($inString) { continue }
        if ($Text.Substring($i).StartsWith($Token)) { return $Text.Substring(0, $i).Trim() }
    }
    return $Text.Trim()
}
