# Text analysis (`pangloss-text-analysis/v1`)

The analyzer takes text, a Segmentation profile and loaded Language Packs, and returns per-Occurrence evidence and statistics. `pangloss analyze-text` prints this result as one JSON document; the library entry point is `pg_analyzer::analyze_text`.

## Request

```
pangloss analyze-text --profile <qaa.ldml|profile.json> --pack <language>=<grammar> [--pack ...] [--dictionary <language>=<prefix> ...] [--max-suggestion-chars N] [--work-cap N] [--word-timeout-ms N] [--max-checked-occurrences N] [--max-alternatives N] [--all-readings] [--summary-only] [--format auto|plain|usfm] [--reject-pattern RE] [--accept-word W]... [--accept-pattern RE]... [--no-character-rule] <text-file>...
```

Each text file is one text, identified by its path. A pack is a grammar the `parse` command loads (`.xml`, `.fwdata`, snapshot `.json`) under a language code of the caller's choosing; each language has at most one pack. Root guessing is never used: a guessed root is not valid-word evidence. The work cap and word timeout apply to every pack's parser, and a search they stop is reported as incomplete.

## Result

| Field | Meaning |
| --- | --- |
| `schema` | `pangloss-text-analysis/v1` |
| `profile` | The Segmentation profile used, as a JSON profile (source identity and SHA-256 of the LDML bytes). |
| `resources` | The profile first, then each pack, then each Dictionary pack, in request order: `kind` (`segmentation_profile`, `language_pack`, `dictionary_pack`), `source_id`, `source_sha256` (a Dictionary pack hashes its `.aff`, `.dic` and word list), `language` for a pack, and `version` and `licence` for a Dictionary pack. |
| `texts[]` | One per input text: `id`, `occurrences[]`, `statistics`. |
| `corpus` | `statistics` over every text. |
| `not_a_word_rule` | The default not-a-word rule that applied: `valid_characters`, `fallback_digit_or_no_letter` (the profile has no exemplar sets) or `off`. |
| `detection` | Language detection over every text together; see below. Each `texts[]` entry carries its own `detection`. |

An Occurrence has `wordform` (the original text, never rewritten), `span` (`code_points`, `utf8` and `utf16`, each a half-open `[start, end]` in that unit), `outcome`, `valid_languages` (pack order, each language once), `misspelled_languages`, `checks[]` (the primary split's), `readings[]` and, when present, `limits`.

A check is one pack's evidence about one Occurrence: `resource` (index into `resources`), `language`, `evidence_kind` (`language_pack_analysis`: a HermitCrab structured analysis; `dictionary_acceptance`: Hunspell accepted the form; `word_list_membership`: the pack's word list holds it; the last two are never a parse and carry `analyses` 0), `status`, `analyses` (parser results behind a valid status), `limits`, `suggestions` (only on `misspelled`) and, for an unattempted check, `unattempted_reason`.

| `status` | Meaning |
| --- | --- |
| `valid` | The pack analysed the Wordform. `limits` may still be non-empty if the search was cut short after a finding. |
| `no_analysis` | The search finished and found nothing. |
| `incomplete` | The search did not finish and found nothing. `limits` lists `capped`, `capture_capped`, `timed_out` or `invalid_shape`. This is not a failure. |
| `unattempted` | The check never ran (`occurrence_limit`). |
| `misspelled` | A Dictionary pack rejected the primary split and Hunspell offered suggestions (`teh` -> `the`). Suggestions never replace the text and never make the form valid. A rejection with no suggestions is `no_analysis`: a miss alone is not a misspelling. A rejected form longer than `--max-suggestion-chars` (default 64) is `incomplete` with limit `suggestions_capped`, never an empty result. Alternative parts are checked for acceptance only, never for suggestions. |

## Not a word

Segmentation keeps LibLCM parity (ADR 0011), so `16`, `3:16` (two tokens), `16th` and a lone `'` are still tokens. A filter after segmentation decides which tokens are words (ADR 0016). A token is a word candidate only if every character, NFC and case-folded, is in the writing system's main or auxiliary exemplar set and at least one is a letter. Otherwise it is not a word because of its first offending character. If the profile has no exemplar sets, a numeric character or no letter at all is the rule instead, and `not_a_word_rule` says `fallback_digit_or_no_letter`.

A not-a-word Occurrence has `outcome: not_a_word`, `not_a_word: {reason, character}` and no checks or readings; the parser and Dictionary packs never see it. `reason` is `character` (with the offending `character`), `reject_pattern` (the extra reject regex matched) or `style` (inside a Chapter Number or Verse Number run; USFM `\c` and `\v` numbers and every USFM marker never become words). It is outside `total` and every other count and does not spend `--max-checked-occurrences`.

Exceptions: `accept_words` and `accept_patterns` pass anyway and win over every other rule; `reject_pattern` is an extra reject regex. Each is a field of a JSON segmentation profile, or `--accept-word`, `--accept-pattern` and `--reject-pattern` on the command line (run accept lists and reject pattern add to the profile's; an accept regex must match the whole token, as `^(?:re)$`). `not_a_word` is a new `outcome` value and these tokens no longer count in `total`, `valid` or `no_analysis`: a consumer should treat an unknown outcome as skippable. `--no-character-rule` switches the default rule off; the reject pattern and style exclusion still apply. `--format auto` (default) reads `.usfm` and `.sfm` files as USFM; `plain` and `usfm` force it.

`--summary-only` leaves `texts[].occurrences` empty and keeps `resources`, `statistics` and `detection`.

## Segmentation alternatives

A word-forming apostrophe or hyphen (`'`, `‘`, `’`, `-`, U+2010, U+2011) inside an Occurrence can also be punctuation. For each such character the Occurrence has alternatives that split there, keeping the character on the left part (`l'` + `homme`) or leaving it out as punctuation (`l` + `homme`). Characters the profile does not make word-forming never join runs, so they give no alternatives.

`readings[]` lists every reading of the one source span: the primary FieldWorks-rule split first (`kind: primary`, one part), then alternatives (`kind: alternative`) whose every part is analysed, then the rest, each group in generation order (fewest changed characters first). A reading has `parts[]` (`wordform`, `span`, `valid_languages`, `checks[]`) and `analysed`: for the primary split, some pack analysed it; for an alternative, every part has a valid check. Every part of every alternative goes to every pack's parser, except that alternatives are not enumerated when the primary split is already analysed, since they cannot change the outcome; `--all-readings` enumerates them anyway (for `readings` and `valid_languages`). An Occurrence with no dual-role character has only its primary reading. An Occurrence past `--max-checked-occurrences` has only its primary reading, unattempted.

`--max-checked-occurrences` bounds Occurrences, not parser calls; `--max-alternatives` bounds alternatives. At most `--max-alternatives` alternatives (default 16) are kept per Occurrence. Past that the Occurrence carries `limits: ["alternatives_capped"]`; it is never dropped.

An Occurrence is analysed when its primary split or any alternative is. `valid_languages` is the languages of the valid parts of the analysed readings. An alternative whose parts are valid in different languages (`l'` in one, `homme` in another) counts as analysed and contributes all of them, but it makes none of them ambiguous: `ambiguous` counts only Occurrences in which two or more languages each analyse some reading by themselves. Per-language counts come from `valid_languages`, never from the primary split's `checks`. The set of dual-role characters is fixed (above); a profile that makes other punctuation word-forming gets no alternatives for it. The misses of the other readings are not counted or flagged.

## Outcome

`outcome` combines the checks. `valid` if any reading is analysed (a vernacular parse stays visible beside a dictionary acceptance). Otherwise `unattempted` if a check never ran or no pack is loaded, then `incomplete` if any grammar search did not finish or alternatives were capped (an open search outranks a spelling verdict: a timed-out grammar is never reported as a misspelling), then `misspelled` if a Dictionary pack rejected the primary split with suggestions. Otherwise `no_analysis` only when every check finished and found nothing; else `incomplete` (this includes an unfinished suggestion search). A reading counts toward `incomplete` only while every part is analysed or unfinished and one is unfinished, so a finished miss in one reading does not hide behind another's. `alternatives_capped` also makes an unanalysed Occurrence `incomplete`; an analysed one stays `valid` and keeps the limit. So an interrupted or unrun check can never turn into `no_analysis`.

## Statistics

Every `statistics` object has `not_a_word`, `valid_via_alternative` and the counts below. `not_a_word` is `{total, by_character, reject_pattern, style, forms[]}`: `by_character` maps each first offending character to `{occurrences, wordforms}`, and `forms[]` lists every distinct `{wordform, reason, character?, occurrences}`, most frequent first. `valid_via_alternative` is `{total, hyphen, apostrophe}`: valid Occurrences whose primary split is not analysed but whose first analysed alternative splits at a hyphen or an apostrophe (an Occurrence split at both counts in both and once in `total`); it is part of `valid`. Each Occurrence of that kind carries `alternative_split`.

Every `statistics` object has `total`, `valid`, `misspelled`, `no_analysis`, `incomplete`, `unattempted` and `ambiguous`, each `{occurrences, wordforms}`, and `languages`, a map from language code to `{valid, misspelled, ambiguous, no_analysis}`, each `{occurrences, wordforms}`.

- `total` and the six outcome counts exclude not-a-word tokens. `occurrences` counts instances in the text; `wordforms` counts distinct exact surface strings. They are never added together.
- `total`, `valid`, `misspelled`, `no_analysis`, `incomplete` and `unattempted` partition the Occurrences. A Wordform is counted in a category when any of its Occurrences is, so Wordform counts overlap only when `--max-checked-occurrences` leaves some Occurrences of one Wordform unattempted, or when `--word-timeout-ms` (wall-clock) interrupts the search for only some Occurrences of one Wordform.
- `languages[code].valid` counts Occurrences, and Wordforms, that the pack for `code` analysed. An Occurrence valid in several languages counts once in each. For one language code valid wins: a language that is valid for the Occurrence (say a grammar pack and a Dictionary pack share the code) is never also misspelled, and `misspelled_languages` omits it. `misspelled_languages` is empty when only an alternative reading is analysed. A different language keeps its own state: `languages[code].misspelled` counts an Occurrence that is valid in another language when a pack of `code` rejected the primary split with suggestions. `ambiguous` per language counts Occurrences valid in that language and standalone-valid in two or more. `no_analysis` per language counts finished misses of the primary split, and is skipped for unfinished checks and when only an alternative is analysed.
- `ambiguous` counts valid Occurrences with two or more valid languages.
- Counts are of Occurrences and Wordforms, never of parser results, parts or alternatives.
- `texts[].statistics` covers one text and `corpus` covers all; Wordforms are distinct within the scope they sit in.

## Language detection

`detection` is a proposal of which languages a text contains, such as 231 English Occurrences meaning "turn on English". It is never a verdict on an Occurrence: `valid_languages` and `outcome` are untouched by it. It has `proposals[]` (most Occurrences first, then language code) and `sufficient` (true when at least one language is `proposed`).

A proposal has `language`, `status`, and `occurrences`, `dictionary`, `shared` and `ngram`, each `{occurrences, wordforms}`.

- `dictionary`: Occurrences valid in this language by a Language Pack or a Dictionary pack (Hunspell acceptance or word list). `shared` is the part of that also valid in another language.
- `ngram`: Occurrences valid nowhere (outcome `no_analysis` or `misspelled`; a misspelled word is still evidence of its language, while `incomplete` and `unattempted` Occurrences have no finished verdict and cast no vote) of at least 4 characters, where this language's Dictionary pack covers at least 80% of the form's character trigrams and leads the next language by at least 20 points. A tie or a weak lead gives no evidence. The profile holds stems only, with no affix expansion, so an affixed form is covered only through its stem's trigrams.
- `occurrences` is `dictionary` plus `ngram`.
- `status` is `proposed` when at least 5 Occurrences are explained by this language alone (valid only here, or n-gram winner); otherwise `ambiguous`. Short input, or words shared by several languages, therefore stays `ambiguous`, and the evidence is still shown.

The thresholds (5 Occurrences, 4 characters, 80% coverage, 20-point lead) are untuned initial values, set against tiny fixtures and not yet checked on real text; expect them to change. `detection` is an additive field, so the schema stays `v1`: consumers ignore fields they do not know.

The n-gram profile is built when a Dictionary pack loads, from that pack's own `.dic` stems and word list, and kept as a set of trigrams in memory. Its size is bounded by the alphabet: 8,351 trigrams for a 49,568-stem English dictionary and 11,428 for a 146,269-stem Russian one, about 0.13 and 0.18 MB. Nothing else is bundled; see ADR 0012, "Addendum: language-detection data".

Offsets are in the units named by their keys; callers never have to guess. The format is versioned: a consumer rejects an unknown `schema`.
