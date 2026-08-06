<#
  Which source lines are comment lines. Dot-sourced by `comment-hygiene.ps1` (to decide what to
  score) and by `verify-comment-only.ps1` (to decide what a comment-only edit is allowed to touch).

  Shared rather than duplicated because the two tools must agree BY CONSTRUCTION. If the verifier's
  notion of "comment" were even slightly wider than the checker's, a sweep could delete something the
  verifier waved through and the checker never sees -- which is exactly the failure this file exists
  to prevent, and exactly the one that occurred.

  THREE TABLES, because "is this a comment?" has three different answers depending on who is asking.

  $commentLineByExt -- does this line START a comment? Per-language, not one union pattern: a shared
  `#` alternative matches every Rust ATTRIBUTE (`#[derive(Debug)]`), which cost 238 phantom long
  blocks. The `\*` form requires a following space or `/` so a Rust dereference (`*x = 5;`) is not
  read as a block-comment continuation either.

  $blockCommentByExt -- delimited forms whose CONTINUATION lines carry no marker at all, so a body
  line inside one looks exactly like code and a line-start pattern cannot see it. Rust has no entry
  because `//` prefixes every line of a Rust comment block, delimited or not. The opener is matched
  ANCHORED at line start by the caller: unanchored, the literal opener inside this file's own `.ps1`
  pattern above opened a phantom block and swallowed ten lines of code. `#Requires` is excluded at
  the use site -- a parser directive that merely looks like a comment, and capping it would be
  uncomplyable.

  Do not write PowerShell's block-comment CLOSING token in this header, even inside backticks: it
  ends the header wherever it appears, and the prose after it becomes code. That is how this file
  last broke.

  $lineCommentTokenByExt -- what starts a TRAILING comment on a code line. This is what tells
  `let x = 1; // old note` from `let x = 2; // old note`: only the first is a comment-only edit, and
  the whole line is code either way, so neither table above can decide it.

  Get-CodePortion strips such a trailing comment, ignoring a token inside a double-quoted string so
  `let url = "http://x";` keeps its value. Honest limit: it does not model Rust raw strings
  (`r#"..."#`) or char literals. A mis-strip can only make two lines LOOK equal that were not, so the
  failure direction is a missed report, never a false alarm.
#>

$commentLineByExt = @{
    '.rs'   = '^\s*(///|//!|//|/\*|\*(\s|/|$))'
    '.ps1'  = '^\s*(#|<#)'
    '.py'   = '^\s*#'
}

$blockCommentByExt = @{
    '.ps1' = @{ Open = '<#'; Close = '#>'; Same = $false }
    '.py'  = @{ Open = '"""'; Close = '"""'; Same = $true }
}

$lineCommentTokenByExt = @{
    '.rs'   = '//'
    '.ps1'  = '#'
    '.py'   = '#'
}

# Comment-or-not for EVERY line of a file, as a bool array indexed 0-based by line number. This is
# the single implementation of the question; both tools index into it rather than each running their
# own regex, because a delimited block's body can only be recognized with whole-file state and two
# state machines would eventually disagree -- which is the drift this file exists to rule out.
function Get-CommentLineMask {
    param([string[]]$Lines, [string]$Extension)
    $start = $commentLineByExt[$Extension]
    $delims = $blockCommentByExt[$Extension]
    $mask = New-Object 'bool[]' $Lines.Count
    $inDelimited = $false
    for ($i = 0; $i -lt $Lines.Count; $i++) {
        $line = $Lines[$i]
        $isComment = $start -and ($line -match $start)
        if ($delims) {
            $closes = [regex]::Matches($line, [regex]::Escape($delims.Close)).Count
            if ($inDelimited) {
                $isComment = $true
                if ($closes -gt 0) { $inDelimited = $false }
            } elseif ($line -match ('^\s*' + [regex]::Escape($delims.Open))) {
                $isComment = $true
                $inDelimited = if ($delims.Same) { ($closes % 2) -eq 1 } else { $closes -eq 0 }
            }
        }
        if ($isComment -and $line -match '^\s*#Requires\b') { $isComment = $false }
        $mask[$i] = [bool]$isComment
    }
    return $mask
}

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
