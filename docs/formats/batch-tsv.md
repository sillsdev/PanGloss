# `pangloss batch` TSV

`pangloss batch <grammar> <words.txt> <out.tsv>` writes one tab-separated result row per word.

By default a row has 5 columns, or 6 with `--guess`. External adapters, such as the conformance
suite's driver, depend on exactly that layout. Only the command line decides it: a FieldWorks
project that guesses roots by default (`<HC><GuessRoots>`, see below) still writes 5 columns unless
`--guess` is passed. The two parser-work columns appear only when
`--work-columns` is passed.

A host can find out whether the flag exists. `pangloss --describe` reports
`"interfaces": {"batchTsv": 3}` and lists `--work-columns` under `batch`. Version 3 keeps
version 2's layout and shares the result-status decision with `serve`; version 2 already supports
`--work-columns`. The describe `schema_version` stays 1 because the default output did not change.
Pass `--work-columns` only when `interfaces.batchTsv` is at least 2. An older binary refuses the flag.

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
| `guessed` | `true` when the analyses were guessed from a lexical pattern, else `false`; present **only** when `--guess` or `--guess=true` was passed |
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

## Root guessing

A word the lexicon cannot analyse can be retried by guessing its root from a lexical-pattern
allomorph (a root form such as `t[V]*d`). Whether `batch` guesses:

| source | guesses |
|---|---|
| FieldWorks project (`.fwdata`, `.fwbackup`, snapshot `.json`) | unless `<HC><GuessRoots>` is false; FieldWorks' default is true |
| HermitCrab XML grammar | never, as the C# `hc` tool does not |
| `--guess` or `--guess=true` | always, and the `guessed` column is written |
| `--guess=false` | never |

Without `--guess`, a guessed row looks like any other in the TSV: its signature names the guessed
text in place of a morpheme id. To tell guessed analyses apart, pass `--guess` (the `guessed`
column), or pass `--analyses <path>` and read each morph's `guessedString`: a guessed morph carries
the matched text there, with the lexical pattern's MoForm and MSA as `form` and `msa`, as FieldWorks
records a guess. A project without a lexical-pattern root never guesses, so its rows are the same
whatever the setting.

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
`word.work` is `work_steps`. The cache schema version is 10 (`run.schema_version` and
`cache_identity.schema_version`); a cache with another version is wiped and recreated, never
migrated.

`fact.work_steps` is the number of successful work-budget reservations assigned to the row's
object, stratum, allomorph and direction. The innermost instrumented object scope receives a
reservation, including inner matcher reservations; nested scopes never double-count. The
synthetic object with `key = 'unattributed'` and `kind = 'unattributed'` has one row per word,
with `direction = 'unattributed'` and sentinel stratum/allomorph IDs. It accounts for work outside
instrumented object scopes, including segmentation, template and search orchestration, and
matching or confirmation outside such a scope. Summing `fact.work_steps` across every row for a
word, including that row, exactly equals `word.work`. The `facts` export's existing object-counter
projection excludes the remainder because it is not a grammar object.

The batch TSV remains interface version 3: its optional columns describe the whole word and do
not contain object rows. Cache counter semantics version 7 identifies this attribution, including
final allomorph-environment matching under its owning lexical entry or morphological rule.

`fact.work` (per grammar object, and the trace-details `categories` that report it) is a
**different unit**. It counts shape segments examined each time the object was tried. It does not
count work reservations, so a word's fact rows do not generally add up to its `work_steps`, and the two do not
bound each other. To get a word's total work, read `work_steps` (or `word.work`). Do not sum
`fact.work`.

What the two have in common: every analysis attempt reserves exactly one work unit, so
`steps <= work_steps`. The `work_steps` remainder beyond `steps` includes reservations for matchers' inner steps,
segmentation, and confirmation; it is not the sum of fact work. Measured on the conformance
fixtures `languages/metathesis-phase-isolation` and `edge-cases/truncate-morphotactic`, with no
cap:

| word | steps | work_steps | sum of `fact.work` |
|---|---|---|---|
| sumulat | 28 | 901 | 394 |
| keadilan | 132 | 2567 | 1552 |
| katibɯd | 28 | 1021 | 364 |
| katibɯt | 19 | 779 | 273 |
| pur | 19 | 411 | 163 |
| gas | 14 | 692 | 109 |

The `fact_work_is_segments_examined_not_the_word_work_steps_total` test in
`rust/crates/pg-parse/tests/stats_collector_gate.rs` pins these numbers.
