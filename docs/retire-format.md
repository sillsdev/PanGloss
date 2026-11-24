# What-if-retire report (`pangloss-retire/v1`)

`pangloss batch <grammar> <words.txt> <ignored.tsv> --retire <guid>[,<guid>...] --retire-out report.jsonl` asks: if these lexical entries (or allomorphs) were deleted, which words change? It is an exact engine evaluation, not a set computation. The grammar is loaded twice, the second time with the named entries or allomorphs removed from the imported snapshot in memory (nothing is written), and the words are parsed on both. Removing an allomorph can unblock a later one (`allomorph_blocked`), so readings can appear as well as vanish.

The grammar must be a `.fwdata`, `.fwbackup` or snapshot `.json`; an HC XML grammar has no entry guids and is refused. `<out.tsv>` is not written. `--retire` is refused with `--analyses`, `--stats`, `--partials`, `--engine`, `--start`, `--prefilter` and a diagnostic run. An id that is neither an entry guid nor an allomorph (`MoForm`) guid is an error.

Which words are re-parsed on the retired grammar: when only entries are retired, every word whose baseline readings include a retired entry, plus every word whose baseline was capped, timed out or had a reading that could not be projected. Words outside that set are reported `unchanged` with `reparsed: false`. When an allomorph is retired, every word is re-parsed (a blocked word has no reading to name the blocker). The baseline is always parsed in full, in process.

## Row

One JSON object per input word, in input order, schema tag `pangloss-retire/v1`; the row schema is [pangloss-retire-v1.schema.json](pangloss-retire-v1.schema.json).

| Field | Meaning |
| --- | --- |
| `index`, `word` | Position and text of the word. |
| `status` | `unchanged`; `lost` (had readings, now none); `reduced` (lost some, keeps others); `gained` (new readings, none lost); `changed` (both); `incomplete` (a work cap, memory cap or timeout stopped either parse, so no verdict; rerun with a larger `--work-cap` or `--word-timeout-ms`; a word of invalid shape has no readings on either side and is `unchanged`). |
| `reparsed` | Whether the retired grammar parsed the word. |
| `baselineReadings`, `retiredReadings` | Reading counts before and after. |
| `lost`, `gained` | Readings only before, or only after; each `{entries, morphs}` with `morphs` as in `fieldworks-parse-analysis/v1` and `entries` the sorted entry guids among its morphs (the reading's entry dependency). Readings are compared by their morph list. |
| `cause` | On `lost`, `reduced` and `changed` words: `{kind, via}`. `kind` is `no-alternative-chain` (`lost`: no reading survives) or `alternative-route` (other readings survive), `via` the retired entries the lost readings ran through. |

A summary of status counts is printed to stderr as `RETIRE`.

Not covered: causes finer than these (a class mismatch, dead continuation or repeated affix) are not derived; the retired entry is absent from the retired grammar, so a why-not on the lost reading has nothing to explain it with.

Checked on the Swahili project: with 1, 2, 3 and all 29 retire candidates, over all 10,319 gold words, every row equals a plain batch of a snapshot copy with the entries deleted, including words outside the re-parse scope.
