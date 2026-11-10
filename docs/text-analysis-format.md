# Text analysis (`pangloss-text-analysis/v1`)

The analyzer takes text, a Segmentation profile and loaded Language Packs, and returns per-Occurrence evidence and statistics. `pangloss analyze-text` prints this result as one JSON document; the library entry point is `pg_analyzer::analyze_text`. Dictionary packs and misspelling evidence are later work and add fields without changing these.

## Request

```
pangloss analyze-text --profile <qaa.ldml|profile.json> --pack <language>=<grammar> [--pack ...] [--work-cap N] [--word-timeout-ms N] [--max-checked-occurrences N] [--max-alternatives N] [--all-readings] <text-file>...
```

Each text file is one text, identified by its path. A pack is a grammar the `parse` command loads (`.xml`, `.fwdata`, snapshot `.json`) under a language code of the caller's choosing; each language has at most one pack. Root guessing is never used: a guessed root is not valid-word evidence. The work cap and word timeout apply to every pack's parser, and a search they stop is reported as incomplete.

## Result

| Field | Meaning |
| --- | --- |
| `schema` | `pangloss-text-analysis/v1` |
| `profile` | The Segmentation profile used, as a JSON profile (source identity and SHA-256 of the LDML bytes). |
| `resources` | The profile first, then each pack in request order: `kind`, `source_id`, `source_sha256`, and `language` for a pack. |
| `texts[]` | One per input text: `id`, `occurrences[]`, `statistics`. |
| `corpus` | `statistics` over every text. |

An Occurrence has `wordform` (the original text, never rewritten), `span` (`code_points`, `utf8` and `utf16`, each a half-open `[start, end]` in that unit), `outcome`, `valid_languages` (pack order), `checks[]` (the primary split's), `readings[]` and, when present, `limits`.

A check is one pack's evidence about one Occurrence: `resource` (index into `resources`), `language`, `evidence_kind` (`language_pack_analysis`: a HermitCrab structured analysis), `status`, `analyses` (parser results behind a valid status), `limits` and, for an unattempted check, `unattempted_reason`.

| `status` | Meaning |
| --- | --- |
| `valid` | The pack analysed the Wordform. `limits` may still be non-empty if the search was cut short after a finding. |
| `no_analysis` | The search finished and found nothing. |
| `incomplete` | The search did not finish and found nothing. `limits` lists `capped`, `capture_capped`, `timed_out` or `invalid_shape`. This is not a failure. |
| `unattempted` | The check never ran (`occurrence_limit`). |

## Segmentation alternatives

A word-forming apostrophe or hyphen (`'`, `‘`, `’`, `-`, U+2010, U+2011) inside an Occurrence can also be punctuation. For each such character the Occurrence has alternatives that split there, keeping the character on the left part (`l'` + `homme`) or leaving it out as punctuation (`l` + `homme`). Characters the profile does not make word-forming never join runs, so they give no alternatives.

`readings[]` lists every reading of the one source span: the primary FieldWorks-rule split first (`kind: primary`, one part), then alternatives (`kind: alternative`) whose every part is analysed, then the rest, each group in generation order (fewest changed characters first). A reading has `parts[]` (`wordform`, `span`, `valid_languages`, `checks[]`) and `analysed`: for the primary split, some pack analysed it; for an alternative, every part has a valid check. Every part of every alternative goes to every pack's parser, except that alternatives are not enumerated when the primary split is already analysed, since they cannot change the outcome; `--all-readings` enumerates them anyway (for `readings` and `valid_languages`). An Occurrence with no dual-role character has only its primary reading. An Occurrence past `--max-checked-occurrences` has only its primary reading, unattempted.

`--max-checked-occurrences` bounds Occurrences, not parser calls; `--max-alternatives` bounds alternatives. At most `--max-alternatives` alternatives (default 16) are kept per Occurrence. Past that the Occurrence carries `limits: ["alternatives_capped"]`; it is never dropped.

An Occurrence is analysed when its primary split or any alternative is. `valid_languages` is the languages of the valid parts of the analysed readings. An alternative whose parts are valid in different languages (`l'` in one, `homme` in another) counts as analysed and contributes all of them, but it makes none of them ambiguous: `ambiguous` counts only Occurrences in which two or more languages each analyse some reading by themselves. Per-language counts come from `valid_languages`, never from the primary split's `checks`. The set of dual-role characters is fixed (above); a profile that makes other punctuation word-forming gets no alternatives for it. The misses of the other readings are not counted or flagged.

## Outcome

`outcome` combines the checks. `valid` if any check is valid. Otherwise `no_analysis` only when every check finished and found nothing; else `unattempted` if any check never ran or no pack is loaded; else `incomplete`. A reading counts toward `incomplete` only while every part is analysed or unfinished and one is unfinished, so a finished miss in one reading does not hide behind another's. `alternatives_capped` also makes an unanalysed Occurrence `incomplete`; an analysed one stays `valid` and keeps the limit. So an interrupted or unrun check can never turn into `no_analysis`.

## Statistics

Every `statistics` object has `total`, `valid`, `no_analysis`, `incomplete`, `unattempted` and `ambiguous`, each `{occurrences, wordforms}`, and `languages`, a map from language code to `{valid: {occurrences, wordforms}}`.

- `occurrences` counts instances in the text; `wordforms` counts distinct exact surface strings. They are never added together.
- `total`, `valid`, `no_analysis`, `incomplete` and `unattempted` partition the Occurrences. A Wordform is counted in a category when any of its Occurrences is, so Wordform counts overlap only when `--max-checked-occurrences` leaves some Occurrences of one Wordform unattempted, or when `--word-timeout-ms` (wall-clock) interrupts the search for only some Occurrences of one Wordform.
- `languages[code].valid` counts Occurrences, and Wordforms, that the pack for `code` analysed. An Occurrence valid in several languages counts once in each.
- `ambiguous` counts valid Occurrences with two or more valid languages.
- Counts are of Occurrences and Wordforms, never of parser results, parts or alternatives.
- `texts[].statistics` covers one text and `corpus` covers all; Wordforms are distinct within the scope they sit in.

Offsets are in the units named by their keys; callers never have to guess. The format is versioned: a consumer rejects an unknown `schema`.
