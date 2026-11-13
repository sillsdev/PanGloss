---
status: accepted
---

# Skip tokens that are not words, by the writing system's valid characters

Owner decision, 2026-11-13, from the CEB real-data check (issue 209). Segmentation keeps exact LibLCM
WordMaker parity ([ADR 0011](0011-fieldworks-compatible-segmentation.md)), so `16`, `3:16`, `16th` and a
lone `'` are still segmented as tokens. This ADR adds a filter that runs after segmentation, in the
analysis and statistics layer of [ADR 0012](0012-multi-language-word-evidence.md).

## Decision

A token is a **word candidate** only if every character of it, NFC-normalised and case-folded, is in the
writing system's `main` or `auxiliary` exemplar set (the `.ldml` `exemplarCharacters`, set in FieldWorks'
Valid Characters dialog), and at least one character is a letter. A character also passes when every piece
of its canonical decomposition is in the set, so a decomposed `ñ` matches an exemplar `ñ` and the reverse.
Any other token is **not a word**, "because it has character X": X is the first offending character
(`1` for `16th`), or the first character of a token with no letter (`'`).

A not-a-word token is skipped. The parser and every Dictionary pack never see it, it takes no part in
`total`, coverage, `valid`, `misspelled`, `no_analysis`, `incomplete`, `unattempted`, per-language counts or
language detection, and it does not spend `--max-checked-occurrences`. It is returned as an Occurrence with
`outcome: not_a_word` and counted in `statistics.not_a_word`, with a total, counts by offending character, and
a list of distinct forms (see `docs/text-analysis-format.md`). Only simple lookups and regexes decide; the
parser (HermitCrab) is never run to decide it.

Tokens inside Chapter Number or Verse Number styled runs are never segmented as words (ADR 0011). They
are reported as not-a-word with reason `style`, and that exclusion holds even when the character rule is off.
For USFM input the CLI builds those styles itself from `\c`, `\cp`, `\ca`, `\v`, `\vp` and `\va`, and marks
every marker (`\p`, `\q1`, `\id GEN ...` lines) as non-word, so markers never become single-letter words.

### Exceptions, enumerated

Set per writing system in the segmentation profile (`accept_words`, `accept_patterns`, `reject_pattern`) or
per run on the CLI (`--accept-word`, `--accept-pattern`, `--reject-pattern`); run lists add to the profile's,
and a run's reject pattern replaces the profile's.

- **Accept list**: exact words (case-folded, NFC) and regexes that pass anyway. Accept wins over everything
  else, including the reject pattern. This is how CEB Psalm 119's Hebrew letter names, which are real words
  but outside the CEB valid set, are kept.
- **Extra reject regex**: a token it matches is not a word, reason `reject_pattern`.
- `--no-character-rule` switches the default rule off. The reject pattern and the style exclusion still apply.

### No exemplar sets

If the profile has neither a main nor an auxiliary set, the valid set is unknown. The rule falls back to
"a numeric character, or no letter at all, makes a token not a word", and the result says so in
`not_a_word_rule: fallback_digit_or_no_letter`. The rule that applied is always reported.

### Valid only through a split

Segmentation alternatives (hyphen, apostrophe; ADR 0012, Q6') are unchanged. `statistics.valid_via_alternative`
counts valid Occurrences whose primary split is not analysed but an alternative is, with how many split at a
hyphen and at an apostrophe, as issue 209 asked. For `'Ang` the alternative without the apostrophe is the
one counted.

### Summary only

`--summary-only` (`AnalyzeOptions::summary_only`) returns resources, statistics and detection with an empty
`occurrences` list. The 209 run measured about 4 KB of JSON per Occurrence.

## What FieldWorks and LibLCM already do

Checked in LibLCM `d564a719` and FieldWorks `936ae91fe`.

- `ValidCharacters.Load` (`src/SIL.LCModel.Core/WritingSystems/ValidCharacters.cs`) reads only the `main`
  set (as word-forming) and the `punctuation` set (as "other"). `IsValid` is "in either list". The
  `auxiliary` set is not part of FieldWorks' valid characters.
- `ValidCharacters.IsWordForming` is "in the main set, or ICU says word-forming";
  `CoreWritingSystemDefinition` turns each main-set character into a word-forming override, which is where
  ADR 0011's profile gets its additions.
- FieldWorks uses valid characters to build the Valid Characters dialog's inventory
  (`Src/FwCoreDlgs/ValidCharactersDlg.cs`, which lists characters found in texts against the valid set),
  to guess word breaks by splitting on the "other" characters (`Src/LexText/Interlinear/WordBreakGuesser.cs`),
  and to give HermitCrab unspecified graphemes (`Src/LexText/ParserCore/HCLoader.cs`, `m_acceptUnspecifiedGraphemes`).
- FieldWorks has no "this token is not a word" check and its parser does not refuse tokens with characters
  outside the valid set. The word-forming and numeric behaviour stays ADR 0011's.

Differences, deliberately: (1) the owner decision admits the `auxiliary` set, because auxiliary letters
(`c f j q v x z ñ` in CEB) are real letters of loanwords that the main set omits; FieldWorks' valid characters
would treat them as invalid. (2) FieldWorks never skips a token by this check, so the skip is new behaviour,
reported per character so a project can add its legitimate exceptions. (3) The numbers and index sets are
not consulted: a digit is never part of a word. (4) The `punctuation` set is not consulted either, since it
does not make a token a word.

## Consequences

- Rejected: a fixed digit or regex default (an earlier version of this change), because the writing system
  already says what its letters are and the rule becomes per-language for free.
- The filter uses the base profile. A styled run with its own writing-system profile is checked against the
  base profile's sets.
- CEB cebocb run (not parsed; see the PR): 968,767 tokens, of which 43,596 are not words (31,741 by style,
  11,855 for a character: the digits `0-9` account for 11,825). Hebrew letters and `à` account for the rest.
