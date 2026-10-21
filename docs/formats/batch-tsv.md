# `pangloss batch` TSV (layout version 2)

`pangloss batch <grammar> <words.txt> <out.tsv>` writes one tab-separated result row per word. A
host can gate on the layout: `pangloss --describe` carries `"schema_version": 2` and
`"interfaces": {"batchTsv": 2}`. A reader written for describe `schema_version` 1 must refuse a
binary that reports 2 rather than read its rows.

## Result rows

```
idx  word  elapsed_ms  status  signature  [guessed]  steps  work_steps
```

| column | meaning |
|---|---|
| `idx` | 0-based position of the word in `words.txt` (blank lines are skipped and not counted) |
| `word` | the word, trimmed |
| `elapsed_ms` | wall-clock milliseconds for this word: time on this computer, which varies with load and thread count |
| `status` | `ok`, `CAP` (stopped at the analysis-attempt cap `--step-cap` or the work cap `--work-cap`), `TIMEOUT` (stopped by `--word-timeout-ms`), or `SKIPPED` (the word did not segment) |
| `signature` | the sorted, `;`-joined analyses, or `-` when there are none |
| `guessed` | `true` or `false`; present **only** when `--guess` was passed |
| `steps` | analysis attempts: the quantity `--step-cap` bounds |
| `work_steps` | parser work: every work unit the word reserved, across analysis, the inner matchers, and confirmation. This is the quantity `--work-cap` bounds |

The two count columns are always the last two fields. Every earlier column keeps the index it had
in layout 1, including `guessed` at index 5. So a row has 7 fields without `--guess` and 8 with
it, and a reader takes `steps` and `work_steps` from the end of the row.

### Determinism

`steps` and `work_steps` depend only on the grammar, the word, the cap settings and the PanGloss
version. They do not depend on thread count, machine or load. Use `work_steps` to compare parse
cost, for example "these words took 8% less work to parse"; `elapsed_ms` is a secondary figure.

- A `CAP` row is deterministic: the same inputs stop at the same point with the same counts. On a
  work-cap stop, `work_steps` equals the work cap.
- A `TIMEOUT` row is **not** deterministic: the deadline is wall-clock time, so its `steps`,
  `work_steps` and partial result depend on how fast this computer ran. Never compare a TIMEOUT
  row's counts across runs.

## Other lines a reader must accept

- `idx  word  STARTED` (3 fields): written by the single-threaded writer (`--threads 1`) just
  before a word is parsed, so an external watchdog can see which word is in flight. A completed
  word's result row follows it.
- A watchdog that kills a stalled run may append its own `idx  word  ms  TIMEOUT  -` row (5
  fields, no counts). PanGloss never writes such a row itself.
- `--start N` appends to an existing `out.tsv`. If that file was written by a PanGloss binary that
  used layout 1, the file holds both layouts.

## Relation to `batch --stats`

The statistics cache's `word` table records the same two counts: `word.attempts` is `steps` and
`word.work` is `work_steps`. The cache schema version is 9 (`run.schema_version` and
`cache_identity.schema_version`); a cache with another version is wiped and recreated, never
migrated.

`fact.work` (per grammar object, and the trace-details `categories` that report it) is a
**different unit**. It counts shape segments examined each time the object was tried. It does not
count work reservations, so a word's fact rows never add up to its `work_steps`, and the two do not
bound each other. To get a word's total work, read `work_steps` (or `word.work`). Do not sum
`fact.work`.

What the two have in common: every analysis attempt reserves exactly one work unit, so
`steps <= work_steps`. The `work_steps` remainder comes from work that no fact row records: the
matchers' inner steps, segmentation, and confirmation reservations. Measured on the conformance
fixtures `languages/metathesis-phase-isolation` and `edge-cases/truncate-morphotactic`, with no
cap:

| word | steps | work_steps | sum of `fact.work` |
|---|---|---|---|
| sumulat | 28 | 935 | 401 |
| keadilan | 132 | 2601 | 1558 |
| katibɯd | 28 | 1055 | 369 |
| katibɯt | 19 | 779 | 273 |
| pur | 19 | 445 | 169 |
| gas | 14 | 727 | 123 |

The `fact_work_is_segments_examined_not_the_word_work_steps_total` test in
`rust/crates/pg-parse/tests/stats_collector_gate.rs` pins these numbers.
