# 048 — `batch --stats` parsed each HC word twice

Kind: efficiency
Status: optimized-in-rust

## Sites

- C# site: none; `--stats` is a Rust CLI facility.
- Rust site: `pg-cli/src/main.rs::run_batch` and `parse_batch_with_stats`; `pg-cli/src/stats_cmd.rs::prepare_batch_stats_hc` and `finish_batch_stats_hc`.

## Difference

The Rust CLI previously parsed words for the batch TSV, then parsed them again to collect stats-cache records. The Motif caller launches one `batch --stats` request; its later `stats` requests read the resulting cache and do not launch another batch.

`run_batch` now obtains the `ParseOutcome`, stats rows, and prune rows from one call to `Morpher::parse_word_with_stats_and_prunes`. For uncached words, the TSV outcome and stats-cache record come from that same parse; cache records are flushed in input order through the cache opened before parsing.

With `--start`, cached words before the output start are skipped without parsing; uncached prefix words are still parsed once to populate stats, though their TSV rows are suppressed. Every word at or after `--start` is parsed once for its TSV row. A cache hit at or after the start keeps its existing cache record.

With `--word-timeout-ms`, the combined TSV parse includes stats-collector work under the same wall-clock deadline. A word near the deadline can therefore time out with `--stats` when it would finish without stats. Wall-clock timeouts can vary across runs, and the TSV and stats for an uncached word reflect the same timed parse.

The stable TSV fields and cache dimensions and counters match the old two-pass result for the same options. Elapsed-time fields and run metadata are measured per execution and are not byte-stable. The parity test covers `--guess`, `--step-cap`, `--word-timeout-ms`, `--always-enforce-final-templates`, `--start`, and one versus multiple threads.

## Evidence

- Implementation commits: `08297a27` (`collect trace and stats together`) added the combined Morpher API; `17d5a7d0` (`perf(cli): parse stats batches once`) integrated it into `batch --stats`.
- `stats_cmd::tests::batch_stats_parses_each_uncached_word_once` uses a per-run test-only atomic counter, including Rayon workers. Restoring the duplicate stats parse made it fail with 4 entries for 2 words instead of the expected 2; this red result was independently reproduced. Disabling the cached-prefix guard made the same test fail with 3 entries instead of 2. The fixed test verifies that, with word 0 cached and `--start 1`, exactly the two output words are parsed under both one and two threads.
- `stats_cmd::tests::batch_stats_preserves_legacy_cache_and_tsv_across_thread_counts_and_options` compares every non-timing TSV column and cache dimensions/counters with the former two-pass stats collection, then compares one-thread and three-thread results. It validates five- and six-column result rows and requires non-empty comparisons. Temporarily corrupting a stable TSV field made this assertion fail; reverting the corruption restored the passing parity result.
