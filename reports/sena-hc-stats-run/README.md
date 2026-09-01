# Sena run through the HermitCrab engine — raw capture

Unedited output from one `pangloss batch` invocation. Nothing here is reformatted, filtered, or
summarised except where noted, so it can be quoted as evidence of what the tool actually prints.

The command, exactly:

```
pangloss batch samples/data/sena.fwdata words.txt batch.tsv \
    --engine=default --threads 1 --word-timeout-ms 60000
```

with `HC_STEP_STATS=1` and `HC_FST_PROFILE=1` set in the environment.

| File | What it is |
|---|---|
| `words.txt` | The 15 attested Sena words fed in, one per line. |
| `batch.tsv` | The parser's own TSV output file — one `STARTED` row per word, then one row per analysis. |
| `stdout.txt` | Standard output, byte for byte. Empty: this command writes its results to the TSV. |
| `stderr.txt` | Standard error, byte for byte — the capability line, load timings, per-word statistics, and the import warnings. |
| `stats.txt` | The `STEPS` / `FSTPROF` / `DEDUPPROF` lines lifted out of `stderr.txt` unchanged. The only edit is which lines are present; no line's text was touched. |

## Reading the statistics

`STEPS <index> <word> <n>` is the HermitCrab search step count for that word. The `FSTPROF` and
`DEDUPPROF` counters are **cumulative over the run**, not per word, so a single word's cost is the
difference between its row and the previous one.

`--engine=default` is the Rust HermitCrab engine. The `capability: ConfirmOnly` line is advisory
here — that gate only bites under `--engine=foma`.

An empty analysis list is not an error: the engine searched and the grammar licensed nothing.
Several of these trace to the import warnings in `stderr.txt`, where allomorphs such as `m'+` and
`n'+` could not be segmented and were skipped.
