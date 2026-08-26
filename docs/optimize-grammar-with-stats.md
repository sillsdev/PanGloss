# Optimize a grammar with PanGloss stats

PanGloss statistics help a grammar author find expensive or over-applying parts of a grammar. They
do not judge linguistic correctness and do not recommend automatic edits. Use them to decide where
to investigate, make one grammar change, and then rerun the grammar's correctness tests.

The workflow uses the default HermitCrab engine. Do not pass `--engine=foma`: that path records word
timings but cannot collect the per-object counters needed for grammar optimization.

## 1. Collect statistics

Start with a representative UTF-8 word list containing one form per line. Use an explicit cache so
the collection and reporting commands unambiguously share the same data:

```powershell
pangloss batch <grammar> <words.txt> out.tsv --stats --threads 1 --word-timeout-ms 1000 --cache <cache.sqlite3>
```

`<grammar>` may be a `.fwdata`, PanGloss snapshot `.json`, or HermitCrab `.xml` file. Statistics are
off unless `--stats` is present. The normal batch TSV remains unchanged; measurements go into the
SQLite cache.

The cache accumulates new words. A word already present is not parsed again. PanGloss rejects a
cache written by a different engine and recreates it when the grammar identity changes. Delete the
cache or choose a new `--cache` path when you intentionally need a fresh measurement of unchanged
grammar data.

Use `--threads 1` for a first run against an unfamiliar grammar. It limits concurrent memory demand
and makes timing easier to interpret. A wall-clock timeout bounds pathological words, but it is
machine- and load-dependent. Add `--step-cap N` when comparing runs and keep both caps identical
across the comparison.

## 2. Start with three reports

### Where did the measured time go?

```powershell
pangloss stats <grammar> --cache <cache.sqlite3> --group group
```

This prints one row per authored-object kind, such as `morph_rule`, `phon_rule`, and `lex_entry`.
`time_ms` is the sum of measured self-time for objects of that kind. Nested measured work is
subtracted from its parent, so it is not counted twice.

Some object kinds have no timing boundary. Their time is unavailable (`—` in text and `null` in
JSON), not zero.

### What is repeatedly attempted but never produces output?

```powershell
pangloss stats <grammar> --cache <cache.sqlite3> --group never-fires
```

This report includes morphological and phonological rules attempted at least 1,000 times in one
direction with zero outputs in that direction. It is evidence about this word list, not proof that a
rule can never fire. Increase corpus coverage before deleting or narrowing a rule.

### What manufactures forms with no lexical root?

```powershell
pangloss stats <grammar> --cache <cache.sqlite3> --group object --sort no-root
```

High `no_root` counts identify morphological rules whose outputs repeatedly lead to failed lexical
lookup. These rules often create expensive dead search branches. A high count localizes the work; it
does not prove which grammar edit is correct.

## 3. Drill down

PanGloss ships six report orientations:

| `--group` value | Question answered |
|---|---|
| `word` | Which input words took the most total time or hit a cap? |
| `object` | Which authored rules or lexical entries account for the recorded work? |
| `allomorph` | Which allomorphs of an object produced the recorded outcomes? |
| `morpheme` | What is the combined activity for entries representing one morpheme? |
| `group` | What are the totals for each compatible authored-object kind? |
| `never-fires` | Which heavily attempted rules produced no output? |

Useful filters include:

```powershell
# Objects involved in one slow word
pangloss stats <grammar> --cache <cache.sqlite3> --group object --word <form>

# One object kind, kept separate from incompatible attempt units
pangloss stats <grammar> --cache <cache.sqlite3> --group object --kind morph_rule

# One analysis/synthesis direction
pangloss stats <grammar> --cache <cache.sqlite3> --group object --direction analysis

# One stratum
pangloss stats <grammar> --cache <cache.sqlite3> --group object --stratum <key>

# Extra diagnostic counters
pangloss stats <grammar> --cache <cache.sqlite3> --group object --kind morph_rule --wide
```

Stratum and direction are filters, not report orientations. Aggregating attempts across unrelated
object kinds would mix different units, so attempt shares and comparisons remain within compatible
kinds. For example, a morphological-rule attempt and a lexical-entry lookup are both useful counts,
but they are not interchangeable units.

`--top N` limits displayed rows within each kind without changing totals or percentages. Use
`--exclude-censored` to omit words stopped by a step cap or timeout. Without it, their counters are
real work observed before the cutoff, but they are lower bounds rather than complete totals.

Run `pangloss stats` without arguments for the complete current option list.

## 4. Use machine-readable output

Select one report orientation when producing JSON Lines:

```powershell
pangloss stats <grammar> --cache <cache.sqlite3> --group object --kind morph_rule --format jsonl --out stats.jsonl
```

The first line is metadata containing run identity, filters, totals, and unavailable-measurement
explanations. Each following line is one report row. Unavailable measurements are JSON `null`; do not
coerce them to zero. A single output file cannot contain the multi-orientation default view, so
`--format jsonl` and `--out` require an explicit `--group`.

## 5. Interpret the numbers safely

- **Time and attempts answer different questions.** Time ranks measured cost. Attempts show how
  often compatible objects participated. A frequently attempted rule may still be cheap.
- **Compare attempts only within compatible object kinds.** Do not rank a lexical-entry lookup
  against a morphological-rule application as if one attempt meant the same work.
- **A timeout produces a floor.** The recorded work is real but incomplete. Keep censored inputs
  visible when finding pathological words; exclude them for like-for-like totals.
- **Zero output is corpus-relative.** A rule absent from the chosen words may be necessary elsewhere.
- **Statistics do not establish correctness.** They identify where work occurred, not whether a
  grammar change preserves intended analyses.
- **Wall-clock time is noisy.** Compare on the same machine under similar load, using the same word
  list, thread count, timeout, and step cap. Prefer deterministic attempts when the timing difference
  is small.

## 6. Optimize without losing correctness

1. Run the collection command on a representative word list.
2. Use `--group word` to locate pathological inputs and the three starting reports above to locate
   the responsible kind or object.
3. Form a linguistic hypothesis. Narrow one rule, allomorph, environment, or lexical class at a time.
4. Rerun the grammar's correctness tests and compare its accepted analyses with the prior version.
5. Collect a fresh statistics cache for the changed grammar using the same words and limits.
6. Keep the edit only when correctness still holds and the relevant measured signal improves.

For storage schema, counter attribution, identities, and exact aggregation rules, see
[Stats attribution semantics and aggregation format](research/pangloss-stats-attribution-and-aggregation-spec.md).
