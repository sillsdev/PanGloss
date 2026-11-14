# `pangloss batch` TSV

`pangloss batch <grammar> <words.txt> <out.tsv>` writes one tab-separated result row per word.

By default a row has 5 columns, or 6 with `--guess`. External adapters, such as the conformance
suite's driver, depend on exactly that layout. Only the command line decides it: a FieldWorks
project that guesses roots by default (`<HC><GuessRoots>`, see below) still writes 5 columns unless
`--guess` is passed. The parser-work column appears only when `--work-columns` is passed.

A host can find out whether the flag exists. `pangloss --describe` reports
`"interfaces": {"batchTsv": 5}` and lists `--work-columns` and `--cost-column` under `batch`. Version 5
removes the `steps` column: `--work-columns` now writes only `work_steps`, `--cost-column` writes `work_steps` before `cost`, and `--step-cap` is refused
like any unknown option (use `--work-cap`). The default 5- and 6-column layout is unchanged, so a
reader that never passes `--work-columns` need not change. Version 4 added `--cost-column` and changed
nothing without it. Version 3 keeps version 2's layout and shares the result-status decision with
`serve`; version 2 already supports `--work-columns`. The describe `schema_version` stays 1 because the
default output did not change. Pass `--work-columns` only when `interfaces.batchTsv` is at least 2,
and read it as one column only when it is at least 5 (versions 2 to 4 wrote `steps` before
`work_steps`). Pass `--cost-column` only when it is at least 4. A host pinned to an older version must
refuse a binary that reports 5, and an older binary refuses a flag it does not have.

## Result rows

```
idx  word  elapsed_ms  status  signature  [guessed]  [work_steps]  [cost]
```

| column | meaning |
|---|---|
| `idx` | 0-based position of the word in `words.txt` (blank lines are skipped and not counted) |
| `word` | the word, trimmed |
| `elapsed_ms` | wall-clock milliseconds for this word: time on this computer, which varies with load and thread count |
| `status` | `ok`, `CAP` (stopped at the work cap `--work-cap`), `MEMCAP` (stopped at a memory ceiling; the signature is partial, like `CAP`), `TIMEOUT` (stopped by `--word-timeout-ms`), or `SKIPPED` (the word did not segment) |
| `signature` | the sorted, `;`-joined analyses, or `-` when there are none |
| `guessed` | `true` when the analyses were guessed from a lexical pattern, else `false`; present **only** when `--guess` or `--guess=true` was passed |
| `work_steps` | present only with `--work-columns`. The word's work: the total of work steps it reserved, across analysis, the inner matchers and confirmation. `--work-cap` bounds the analysis; confirmation may spend one sixteenth of the cap more (none under a cap of 16), so `work_steps` never exceeds the cap plus one sixteenth |
| `cost` | present only with `--cost-column`, which also writes `work_steps` before it. The word's fixed-weight parse cost; see `docs/design/parse-cost.md`. Refused together with `--stats` |

When present, `cost` is always the last field, `work_steps` comes just before it, and every
other column keeps its default index, including `guessed` at index 5. With `--work-columns` alone a row
has 6 fields, or 7 with `--guess`; with `--cost-column` (with or without `--work-columns`) it has 7, or 8
with `--guess`. The extra field count therefore tells a cost run from a work-columns run. Read `cost` from
the end of the row, then `work_steps` from the end of what remains.

The per-word parser-state ceiling is 100 MiB by default; `--word-mem-cap-mib N` sets it and
`--word-mem-cap-mib 0` disables it. It counts retained search structures with fixed byte weights,
not process RSS or the loaded grammar. The separate `--capture-cap-mib` ceiling defaults to
256 MiB and covers affix-matching capture state. Either produces `MEMCAP`.

### Determinism

`work_steps` depends only on the grammar, the word, the cap setting and the PanGloss
version. It does not depend on thread count, machine or load. `cost` repeats exactly for one word list only with `--threads 1`. Under more threads the first words on each thread carry one-time allocation, and a word's cost has been seen to differ by 0.07% across thread counts, so per-word equality is not promised. The run's total moved at most 0.0001% when measured, so compare totals: use `pangloss cost-compare before.tsv after.tsv`, which totals only words that are `ok` in both runs. `elapsed_ms` is a secondary figure.

- A `CAP` row is deterministic: the same inputs stop at the same point with the same counts. On a
  work-cap stop, `work_steps` is at least the work cap and at most the cap plus one sixteenth of it. A word whose `work_steps` is within the cap never stops.
- A `TIMEOUT` row is **not** deterministic: the deadline is wall-clock time, so its
  `work_steps` and partial result depend on how fast this computer ran. Never compare a TIMEOUT
  row's count across runs.

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

The statistics cache's `word` table records the same count whether or not `--work-columns` is passed: `word.work` is
`work_steps`. The cache schema version is 11 (`run.schema_version` and
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

The batch TSV's optional columns stay at the word level: they describe the whole word and do not contain
object rows. Cache counter semantics version 8 attributes cascade reservations to the
attempted morphological rule, lookup reservations to the root index, lexical entry, or overlay,
and validation to the rule or allomorph being checked. Synthetic `parser` objects named
`parser:candidate_expansion`, `parser:stratum_transition`, and `parser:surface_match` account for
the corresponding work steps. The remaining `unattributed` row includes setup and other
orchestration outside these scopes. Version 8 changes attribution, not per-word work totals.
The `facts` export excludes parser rows because it projects grammar-object counters and has no
work-step column. Its `lex_entry` facts can have no allomorph, and its `morph_rule` facts can
have zero attempts while carrying work or self time. In the stats cache, the new object kind is
`parser`; detailed trace JSON reports its work steps in `categories.parser.workStepsByPhase`,
with keys `candidate_expansion`, `stratum_transition`, and `surface_match`. Version 7 attributed
final allomorph-environment matching to its owner.

`pangloss stats --group object` reports `work_steps` per object: the sum of `fact.work_steps` over the
object's rows under the report's filters, as a `work_steps` field on each `--format jsonl` row and a
`work_steps` column with `--wide`. It is the same unit as `word.work`. The `unattributed` remainder is
not a grammar object, so it has no row: with no filter, the object rows' `work_steps` plus the
remainder equal the sum of `word.work`. A cache whose engine records no work reservations
(`foma`) leaves the field out and lists it under `unmeasured`. No other `--group` reports it. The
cache schema version is unchanged and `--describe` has no interface version for `stats` output, so
a reader tells a binary that reports it by the field's presence.

`fact.work` (per grammar object, and the trace-details `categories` that report it) is a
**different unit**. It counts shape segments examined each time the object was tried. It does not
count work reservations, so a word's fact rows do not generally add up to its `work_steps`, and the two do not
bound each other. To get a word's total work, read `work_steps` (or `word.work`). Do not sum
`fact.work`.

`work_steps` includes reservations for matchers' inner steps, segmentation, and confirmation; it is
not the sum of fact work. Measured on the conformance fixtures `languages/metathesis-phase-isolation`
and `edge-cases/truncate-morphotactic`, with no cap:

| word | work_steps | sum of `fact.work` |
|---|---|---|
| sumulat | 799 | 359 |
| keadilan | 2475 | 1525 |
| katibɯd | 922 | 336 |
| katibɯt | 657 | 238 |
| pur | 363 | 148 |
| gas | 683 | 95 |

The `fact_work_is_segments_examined_not_the_word_work_steps_total` test in
`rust/crates/pg-parse/tests/stats_collector_gate.rs` pins these numbers.
