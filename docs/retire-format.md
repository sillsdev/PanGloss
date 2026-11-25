# What-if-retire report (`pangloss-retire/v1`)

`pangloss batch <grammar> <words.txt> <ignored.tsv> --retire <guid>[,<guid>...] --retire-out report.jsonl` asks: if these lexical entries (or allomorphs) were deleted, which words change? It is an exact engine evaluation, not a set computation. The grammar is loaded twice, the second time with the named entries or allomorphs removed from the imported snapshot in memory (nothing is written), and the words are parsed on both. Removing an allomorph can unblock a later one (`allomorph_blocked`), so readings can appear as well as vanish.

The grammar must be a `.fwdata`, `.fwbackup` or snapshot `.json`; an HC XML grammar has no entry guids and is refused. `<out.tsv>` is not written. `--retire` is refused with `--analyses`, `--stats`, `--partials`, `--engine`, `--start`, `--prefilter` and a diagnostic run. An id that is neither an entry guid nor an allomorph (`MoForm`) guid is an error.

Which words are re-parsed on the retired grammar: when only entries are retired, every word whose baseline readings include a retired entry, plus every word whose baseline was capped, timed out or had a reading that could not be projected. Words outside that set are reported `unchanged` with `reparsed: false`. When an allomorph is retired, or an ad hoc prohibition that names a retired item keeps other members and so becomes stricter (it fires only when all its other items co-occur), every word is re-parsed: a blocked or newly blocked word has no reading to name the cause. The baseline is always parsed in full, in process.

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
