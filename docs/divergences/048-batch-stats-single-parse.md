# 048 — `batch --stats` parsed each HC word twice

Kind: efficiency
Status: optimized-in-rust

## Sites

- C# site: none; `--stats` is a Rust CLI facility.
- Rust site: `pg-cli/src/main.rs::run_batch` and `parse_batch_with_stats`; `pg-cli/src/stats_cmd.rs::run_batch_stats_hc`.

## Difference

The Rust CLI previously parsed words for the batch TSV, then parsed them again to collect stats-cache records. The Motif caller launches one `batch --stats` request; its later `stats` requests read the resulting cache and do not launch another batch.

`run_batch` now obtains the `ParseOutcome`, stats rows, and prune rows from one call to `Morpher::parse_word_with_stats_and_prunes`. It passes the outcome to the TSV writer and collects cache records in input order before one cache flush. With `--start`, words before the output start are still analyzed for stats, matching the former cache pass.

The combined parse preserves the stable TSV fields and cache dimensions and counters for the same options. Elapsed-time values and cache run metadata are measured per execution and are not byte-stable. The parity test covers `--guess`, `--step-cap`, `--word-timeout-ms`, `--always-enforce-final-templates`, `--start`, and one versus multiple threads.

## Evidence

- `stats_cmd::tests::batch_stats_parses_each_uncached_word_once` counts two Morpher entries for two words with `--stats`.
- `stats_cmd::tests::batch_stats_preserves_legacy_cache_and_tsv_across_thread_counts_and_options` compares stable TSV fields and cache dimensions/counters with the former two-pass stats collection, then compares one-thread and three-thread results.

The test fails when the duplicate stats parse is restored: it observes four entries for two words.
