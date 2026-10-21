# `pangloss batch` TSV

`pangloss batch <grammar> <words.txt> <out.tsv>` writes one tab-separated result row per word.

By default a row has 5 columns, or 6 with `--guess`. External adapters, such as the conformance
suite's driver, depend on exactly that layout. The two parser-work columns appear only when
`--work-columns` is passed.

A host can find out whether the flag exists. `pangloss --describe` reports
`"interfaces": {"batchTsv": 2}` and lists `--work-columns` under `batch`. The describe
`schema_version` stays 1, because the default output did not change. Pass `--work-columns` only
when `interfaces.batchTsv` is at least 2. An older binary refuses the unknown flag.

## Result rows

```
idx  word  elapsed_ms  status  signature  [guessed]  [steps  work_steps]
```

| column | meaning |
|---|---|
| `idx` | 0-based position of the word in `words.txt` (blank lines are skipped and not counted) |
| `word` | the word, trimmed |
| `elapsed_ms` | wall-clock milliseconds for this word: time on this computer, which varies with load and thread count |
| `status` | `ok`, `CAP` (stopped at the analysis-attempt cap `--step-cap` or the work cap `--work-cap`), `TIMEOUT` (stopped by `--word-timeout-ms`), or `SKIPPED` (the word did not segment) |
| `signature` | the sorted, `;`-joined analyses, or `-` when there are none |
| `guessed` | `true` or `false`; present **only** when `--guess` was passed |
| `steps` | present only with `--work-columns`. Analysis attempts: the quantity `--step-cap` bounds |
| `work_steps` | present only with `--work-columns`. Parser work: every work unit the word reserved, across analysis, the inner matchers and confirmation. This is the quantity `--work-cap` bounds |

When present, the two count columns are always the last two fields, and every other column keeps
its default index, including `guessed` at index 5. With `--work-columns` a row has 7 fields, or 8
with `--guess`. Read `steps` and `work_steps` from the end of the row.

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
- `--start N` appends to an existing `out.tsv`. If that file was written without
  `--work-columns`, or by an older binary, the file holds both layouts.

## Relation to `batch --stats`

The statistics cache's `word` table records the same two counts whether or not `--work-columns` is passed: `word.attempts` is `steps` and
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
