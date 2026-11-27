# What-if-retire report (`pangloss-retire/v1`)

`pangloss batch <grammar> <words.txt> <ignored.tsv> --retire <guid>[,<guid>...] --retire-out report.jsonl` asks: if these lexical entries (or allomorphs) were deleted, which words change? It is an exact engine evaluation, not a set computation. The grammar is loaded twice, the second time with the named entries or allomorphs removed from the imported snapshot in memory (nothing is written), and the words are parsed on both. Removing an allomorph can unblock a later one (`allomorph_blocked`), so readings can appear as well as vanish.

The grammar must be a `.fwdata`, `.fwbackup` or snapshot `.json`; an HC XML grammar has no entry guids and is refused. `<out.tsv>` is not written. `--retire` is refused with `--analyses`, `--stats`, `--partials`, `--engine`, `--start`, `--prefilter` and a diagnostic run. An id that is neither an entry guid nor an allomorph (`MoForm`) guid is an error.

Which words are re-parsed on the retired grammar: the scope is narrowed only when the rest of the grammar is unchanged, meaning everything except the retired entries and their allomorphs and MSAs is identical in the snapshot (phonology, features, rules, ad hoc prohibitions) and the compiler lays out strata and template slots the same way around the surviving rules. Then only the words whose baseline readings include a retired entry are re-parsed, plus every word whose baseline was capped, timed out or had a reading that could not be projected; the rest are `unchanged` with `reparsed: false`. In every other case every word is re-parsed, because the change can alter a word no reading names: an allomorph retire (a blocked word has no reading to name the blocker), an ad hoc prohibition that keeps other items and so becomes stricter, or the last affix of a template slot, which makes the compiler drop the slot and its template more permissive. The baseline is always parsed in full, in process.

## Row

One JSON object per input word, in input order, schema tag `pangloss-retire/v1`; the row schema is [pangloss-retire-v1.schema.json](pangloss-retire-v1.schema.json).

| Field | Meaning |
| --- | --- |
| `index`, `word` | Position and text of the word. |
| `status` | `unchanged`; `lost` (had readings, now none); `reduced` (lost some, keeps others); `gained` (new readings, none lost); `changed` (both); `incomplete` (a work cap, memory cap or timeout stopped either parse, so no verdict; rerun with a larger `--work-cap` or `--word-timeout-ms`; also `incomplete`: a reading on either side cannot be projected, so it cannot be compared; `reason` says which; a word of invalid shape has no readings on either side and is `unchanged`). |
| `reason` | On `incomplete` words only: why there is no verdict. |
| `reparsed` | Whether the retired grammar parsed the word. |
| `baselineReadings`, `retiredReadings` | Reading counts before and after. |
| `lost`, `gained` | Readings only before, or only after; each `{entries, morphs}` with `morphs` as in `fieldworks-parse-analysis/v1` and `entries` the sorted entry guids among its morphs (the reading's entry dependency). Readings are compared by their morph list. |
| `cause` | On `lost`, `reduced` and `changed` words: `{kind, via}`. `kind` is `no-alternative-reading` (`lost`: no reading survives) `alternative-route` (a reading that was there before survives) or `replaced` (`changed` with no earlier reading surviving), `via` the retired entry and allomorph guids the lost readings ran through. |

A summary of status counts is printed to stderr as `RETIRE`.

Not covered: causes finer than these (a class mismatch, dead continuation or repeated affix) are not derived; the retired entry is absent from the retired grammar, so a why-not on the lost reading has nothing to explain it with.

Retiring an entry or allomorph that an ad hoc prohibition names drops that reference, and the prohibition when it loses its key item or every other item, as FieldWorks does when the item is deleted. `--retire-out` cannot be the grammar, word list or `<out.tsv>` path.

## `--without`: the analyses with objects left out

`pangloss parse <grammar> <word> --without <guid>[,<guid>...]` and `pangloss batch <grammar> <words.txt> <out.tsv> --without <guid>[,<guid>...]` parse with the named objects disabled and return the ordinary output (the `parse` signature lines, the batch TSV, an `--analyses` sidecar), not a diff. They run on the same reduced grammar as `--retire` (one implementation, `retire::load_without`), so the result for each word equals a plain parse of a grammar copy with those entries or allomorphs deleted, including the ad hoc prohibition handling above.

Supported objects, each deleted from the grammar snapshot before it is compiled (`without::remove_rule_objects`), so the result equals a plain parse of a grammar with that object deleted:

- lexical entries and allomorphs, as above;
- phonological rules (rewrite and metathesis);
- compound rules (endocentric and exocentric), with their max-application setting;
- affix templates (the slots stay; affixes that fill them then belong to no template);
- ad hoc co-occurrence (prohibition) rules, and their rationale-group memberships.

Refused, each with an error that names the kind of object: an affix template slot (deleting it would silently turn every affix that fills it into a partial affix outside the template; leave out the template instead), and any other object (MSA, phoneme, natural class, environment, part of speech, feature and so on). A guid found nowhere in the grammar is an error. A removal that would leave a reference to the removed guid behind is refused with that reason rather than guessed at. No guid is ever ignored. The grammar must be a `.fwdata`, `.fwbackup` or snapshot `.json`.

`--without` is refused with `--retire`, `--stats`, `--partials`, `--engine`, `--prefilter`, `--cache`, `--stats-manifest` and a rejection trace on `batch`, and with `--trace-details` and a rejection trace on `parse`. `batch --result-cache` keys on the flag.
