# Text analysis (`pangloss-text-analysis/v2`)

The analyzer takes text, a Segmentation profile and loaded Language Packs, and returns per-Occurrence evidence and statistics. `pangloss analyze-text` prints this result as one JSON document; the library entry point is `pg_analyzer::analyze_text`.

## Request

```
pangloss analyze-text --profile <qaa.ldml|profile.json> --pack <language>=<grammar> [--pack ...] [--dictionary <language>=<prefix> ...] [--max-suggestion-chars N] [--work-cap N] [--word-timeout-ms N] [--max-checked-occurrences N] [--max-alternatives N] [--all-readings] <text-file>...
```

A Dictionary pack manifest can set `max_edit_distance` (a nonnegative integer, default 2). It sets the Levenshtein distance over Unicode scalar values used to identify bounded edits. The suggestion search length is controlled separately by `--max-suggestion-chars`.

Each text file is one text, identified by its path. A pack is a grammar the `parse` command loads (`.xml`, `.fwdata`, snapshot `.json`) under a language code of the caller's choosing; each language has at most one pack. Root guessing is never used: a guessed root is not valid-word evidence. The work cap and word timeout apply to every pack's parser, and a search they stop is reported as incomplete.

## Result

| Field | Meaning |
| --- | --- |
| `schema` | `pangloss-text-analysis/v2` |
| `profile` | The Segmentation profile used, as a JSON profile (source identity and SHA-256 of the LDML bytes). |
| `resources` | The profile first, then each pack, then each Dictionary pack, in request order: `kind` (`segmentation_profile`, `language_pack`, `dictionary_pack`), `source_id`, `source_sha256` (a Dictionary pack hashes its `.aff`, `.dic` and word list), `language` for a pack, and `version`, `licence` and `max_edit_distance` for a Dictionary pack. |
| `texts[]` | One per input text: `id`, `occurrences[]`, `statistics`. |
| `corpus` | `statistics` over every text. |
| `detection` | Language detection over every text together; see below. Each `texts[]` entry carries its own `detection`. |

An Occurrence has `wordform` (the original text, never rewritten), `span` (`code_points`, `utf8` and `utf16`, each a half-open `[start, end]` in that unit), `outcome`, `valid_languages` (pack order, each language once), `misspelled_languages`, `checks[]` (the primary split's), `readings[]` and, when present, `limits`.

A check is one pack's evidence about one Occurrence: `resource` (index into `resources`), `language`, `evidence_kind` (`language_pack_analysis`: a HermitCrab structured analysis; `dictionary_acceptance`: Hunspell dictionary and affix evidence; `word_list_membership`: the pack's word list holds it; the last two are never a parse and carry `analyses` 0), `status`, `analyses` (parser results behind a valid status), `limits`, `suggestions`, `suggestion_methods` (one entry per suggestion in the same order) and, for an unattempted check, `unattempted_reason`.

| `status` | Meaning |
| --- | --- |
| `valid` | The pack analysed the Wordform. `limits` may still be non-empty if the search was cut short after a finding. |
| `no_analysis` | The check completed without acceptance or a correction suggestion. |
| `incomplete` | The search did not finish and found nothing. `limits` lists `capped`, `capture_capped`, `timed_out` or `invalid_shape`. This is not a failure. |
| `unattempted` | The check never ran (`occurrence_limit`). |
| `misspelled` | A Dictionary pack rejected the primary split and produced at least one correction suggestion. This includes Hunspell and n-gram fallback suggestions. Occurrence flags additionally follow the rules below. |

Suggestions keep the pack's `resource` attribution and the original Wordform. A suggestion has method `edit_distance` when the Hunspell suggester produces it with n-gram fallback disabled and its edit distance is within the pack bound. `hunspell_rule` identifies a suggestion generated with fallback disabled whose distance exceeds the bound. `ngram_fallback` identifies a suggestion available only with fallback enabled, including fallback on a hyphen-separated subpart. The latter two remain correction suggestions. A rejected form longer than `--max-suggestion-chars` (default 64) is `incomplete` with `suggestions_capped`. Alternative parts are checked for acceptance only.

## Segmentation alternatives

A word-forming apostrophe or hyphen (`'`, `‘`, `’`, `-`, U+2010, U+2011) inside an Occurrence can also be punctuation. For each such character the Occurrence has alternatives that split there, keeping the character on the left part (`l'` + `homme`) or leaving it out as punctuation (`l` + `homme`). Characters the profile does not make word-forming never join runs, so they give no alternatives.

`readings[]` lists every reading of the one source span: the primary FieldWorks-rule split first (`kind: primary`, one part), then alternatives (`kind: alternative`) whose every part is analysed, then the rest, each group in generation order (fewest changed characters first). A reading has `parts[]` (`wordform`, `span`, `valid_languages`, `checks[]`) and `analysed`: for the primary split, some pack analysed it; for an alternative, every part has a valid check. Every part of every alternative goes to every pack's parser, except that alternatives are not enumerated when the primary split is already analysed, since they cannot change the outcome; `--all-readings` enumerates them anyway (for `readings` and `valid_languages`). An Occurrence with no dual-role character has only its primary reading. An Occurrence past `--max-checked-occurrences` has only its primary reading, unattempted.

`--max-checked-occurrences` bounds Occurrences, not parser calls; `--max-alternatives` bounds alternatives. At most `--max-alternatives` alternatives (default 16) are kept per Occurrence. Past that the Occurrence carries `limits: ["alternatives_capped"]`; it is never dropped.

An analysed alternative carries `grammar_support`, keyed by each loaded grammar's language code. For that language, `vernacular_only` means every part has a valid check from its grammar. `mixed` means some parts have a valid grammar check and others require another resource. `all_contact` means each part requires another resource. A part that is also accepted by a Dictionary pack still counts as grammar-backed if the grammar accepts it. The map is omitted on the primary reading and when no grammar was checked.

An Occurrence is analysed when its primary split or any alternative is. `valid_languages` is the languages of the valid parts of the analysed readings. An alternative whose parts are valid in different languages (`l'` in one, `homme` in another) counts as analysed and contributes all of them, but it makes none of them ambiguous: `ambiguous` counts only Occurrences in which two or more languages each analyse some reading by themselves. Per-language valid counts come from `valid_languages`. The set of dual-role characters is fixed (above); a profile that makes other punctuation word-forming gets no alternatives for it. Q6′ suppresses spelling flags when only an alternative is analysed. Joined grammar misses follow the coverage rule below.

## Outcome

`outcome` combines the checks. `valid` if any reading is analysed (a vernacular parse stays visible beside a dictionary acceptance). Otherwise `unattempted` if a check never ran or no pack is loaded, then `incomplete` if any grammar search did not finish or alternatives were capped (an open search outranks a spelling verdict: a timed-out grammar is never reported as a misspelling), then `misspelled` if `misspelled_languages` holds an eligible primary spelling flag. Otherwise `no_analysis` when the checks completed without acceptance or an eligible spelling flag; else `incomplete` (this includes an unfinished suggestion search). A reading counts toward `incomplete` only while every part is analysed or unfinished and one is unfinished, so a finished miss in one reading does not hide behind another's. `alternatives_capped` also makes an unanalysed Occurrence `incomplete`; an analysed one stays `valid` and keeps the limit. So an interrupted or unrun check can never turn into `no_analysis`.

With no accepted reading, a completed joined grammar miss prevents a Dictionary pack for a different language from adding its language to `misspelled_languages`. Even a bounded edit from that pack leaves the top-level grammar miss visible. With no competing grammar miss, any correction suggestion can support the Dictionary pack's Occurrence spelling flag, including in Dictionary-only analysis. The guard affects the Occurrence verdict and flags. Each pack's check and spelling summary use its own evidence.

The joined grammar result is queryable by language under `checks[]` with `evidence_kind: "language_pack_analysis"`. Its `status`, `analyses`, and `limits` describe the original Wordform even when the Occurrence has a spelling flag or an analysed alternative.

## Statistics

Every `statistics` object has `total`, `valid`, `misspelled`, `no_analysis`, `incomplete`, `unattempted` and `ambiguous`, each `{occurrences, wordforms}`, and `languages`, a map from language code to `{valid, misspelled, ambiguous, no_analysis}`, each `{occurrences, wordforms}`.

- `occurrences` counts instances in the text; `wordforms` counts distinct exact surface strings. They are never added together.
- `total`, `valid`, `misspelled`, `no_analysis`, `incomplete` and `unattempted` partition the Occurrences. A Wordform is counted in a category when any of its Occurrences is, so Wordform counts overlap only when `--max-checked-occurrences` leaves some Occurrences of one Wordform unattempted, or when `--word-timeout-ms` (wall-clock) interrupts the search for only some Occurrences of one Wordform.
- `languages[code].valid` counts Occurrences, and Wordforms, that a pack for `code` analysed. An Occurrence valid in several languages counts once in each. Valid evidence takes precedence over spelling flags for the same language. Q6′ suppresses both Occurrence and per-language spelling flags when only an alternative is analysed. Otherwise `languages[code].misspelled` uses the primary Dictionary pack's rejection with any correction suggestion, even when the Occurrence spelling flag for that language is suppressed by a competing grammar miss. `ambiguous` per language counts Occurrences valid in that language and standalone-valid in two or more.
- `languages[code].no_analysis` generally counts completed primary misses with no valid result or spelling flag for that language. An alternative-only success normally suppresses that count. A completed joined grammar miss contributes once if any analysed alternative has `mixed` or `all_contact` support for the grammar, including when a `vernacular_only` alternative also succeeds. A mixed split can contribute to both `valid` for its accepted parts and `no_analysis` for the joined grammar miss.
- `ambiguous` counts Occurrences where at least two languages each support every part of some analysed reading.
- Counts are of Occurrences and Wordforms, never of parser results, parts or alternatives.
- `texts[].statistics` covers one text and `corpus` covers all; Wordforms are distinct within the scope they sit in.

## Language detection

`detection` is a proposal of which languages a text contains, such as 231 English Occurrences meaning "turn on English". It is never a verdict on an Occurrence: `valid_languages` and `outcome` are untouched by it. It has `proposals[]` (most Occurrences first, then language code) and `sufficient` (true when at least one language is `proposed`).

A proposal has `language`, `status`, and `occurrences`, `dictionary`, `shared` and `ngram`, each `{occurrences, wordforms}`.

- `dictionary`: Occurrences valid in this language by a Language Pack or a Dictionary pack (Hunspell acceptance or word list). `shared` is the part of that also valid in another language.
- `ngram`: Occurrences valid nowhere (outcome `no_analysis` or `misspelled`; a misspelled word is still evidence of its language, while `incomplete` and `unattempted` Occurrences have no finished verdict and cast no vote) of at least 4 characters, where this language's Dictionary pack covers at least 80% of the form's character trigrams and leads the next language by at least 20 points. A tie or a weak lead gives no evidence. The profile holds stems only, with no affix expansion, so an affixed form is covered only through its stem's trigrams.
- `occurrences` is `dictionary` plus `ngram`.
- `status` is `proposed` when at least 5 Occurrences are explained by this language alone (valid only here, or n-gram winner); otherwise `ambiguous`. Short input, or words shared by several languages, therefore stays `ambiguous`, and the evidence is still shown.

The thresholds (5 Occurrences, 4 characters, 80% coverage, 20-point lead) are untuned initial values, set against tiny fixtures and not yet checked on real text; expect them to change. `detection` was added in v1. V2 keeps the existing field shapes and adds suggestion methods, the pack's edit-distance bound, and grammar support on analysed alternatives. The outcome and per-language count semantics change in v2, so consumers must explicitly recognize it. Missing additive fields in saved v1 results provide no evidence of a suggestion method or grammar support.

The n-gram profile is built when a Dictionary pack loads, from that pack's own `.dic` stems and word list, and kept as a set of trigrams in memory. Its size is bounded by the alphabet: 8,351 trigrams for a 49,568-stem English dictionary and 11,428 for a 146,269-stem Russian one, about 0.13 and 0.18 MB. Nothing else is bundled; see ADR 0012, "Addendum: language-detection data".

Offsets are in the units named by their keys; callers never have to guess. The format is versioned: a consumer rejects an unknown `schema`.
