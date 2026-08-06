# foma-rs: a `0`-digit multichar symbol decomposes silently in lexicon-entry text

## The bug

`foma::lexcread::lexc_string_to_tokens` (the tokenizer used for lexicon-entry text) and
`lexc_add_mc` (used for the `Multichar_Symbols` declaration) disagree about a multichar symbol
whose name contains a literal `0` digit — which lexc source must spell as `%0`, since a bare `0`
means the alignment epsilon.

- `lexc_add_mc` calls `normalize_mc_symbol`, which fully resolves the shared `nfst-lexc` lexer's
  `@ZERO@` marker (its representation of an escaped literal zero) back to the literal character
  `"0"` before registering the symbol.
- `lexc_string_to_tokens` instead checks for a literal `"@ZERO@"` substring first, converting it to
  a lone `"0"` symbol one character at a time, and only then tries the multichar-prefix match
  against the remaining text — which, at that point, no longer matches the fully-normalized
  registered symbol text.

The declared symbol is consequently never recognized as one token in entry text and gets silently
decomposed into its constituent single-character symbols instead (each of which usually already
exists in `sigma` as an ordinary one-character entry). The affected-tag pattern is exactly (and
only) the set of tags whose zero-padded numeral text contains a literal `0` digit — since
`pg_foma::tags::tag_width` only zero-pads once `morpheme_count > 10`, this affects essentially
every real, non-tiny grammar.

Filed upstream as `divvun/foma-rs`. The original C foma reader does not have this defect: it
de-escapes `%0` to a literal byte in a single unambiguous pass before any multichar matching
happens.

## The compiled network's language is unaffected

A fresh `foma::apply::apply_down` query for an affected tag's exact text (paired with a root tag
where the grammar's own structure requires one) returns `Some(_)` — the arc sequence is there, just
spelled via several single-character symbols in a row instead of one atomic multichar symbol,
which still concatenates to the correct string.

But any construction that expects a tag to be one indivisible alphabet symbol — e.g.
`foma::constructions::fsm_intersect`, which the corpus recall gate's compose-restrict-project-
intersect technique uses — silently miscounts: a real, silent recall-counting bug, not a language
bug.

## The fix, and why the detector still exists

`pg_foma::tags` fixes this at the source: no tag numeral this crate emits ever contains a literal
`0` byte, so the `lexc_string_to_tokens`/`@ZERO@`-normalization mismatch can never trigger for this
crate's own output. `pg_foma::emit::verify_tags_reachable`'s detection logic is kept as a defensive
safety net for a future code path that could still declare a zero-containing multichar symbol some
other way, but it should not normally fire.

Given that, `verify_tags_reachable` narrows what counts as a genuine reachability gap: `sigma`
membership is still checked first (a valid signal for a real disconnected fragment), but a tag
absent from `sigma` is recognized as this known decomposition artifact — rather than a new gap —
when its text contains a literal `0` digit AND every one of its individual Unicode-scalar
characters is present in `sigma`. `lexc_string_to_tokens` is a pure function of its input text, so
every occurrence of the same declared tag text decomposes into the same character sequence; if
every one of those characters made it into `sigma`, the arcs spelling this tag were written by this
emitter and survived the compile. A tag missing from `sigma` for any other reason (no `0` in its
text, or some of its characters also missing) is still reported exactly as before.
