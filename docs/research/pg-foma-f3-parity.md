# `f3_parity` — corpus multiset parity harness

`rust/crates/pg-foma/tests/f3_parity.rs` compares the foma path
(`pg_foma::composite::FomaAnalyzer::analyze_word`) against the full engine
(`pg_parse::Morpher::parse_word_opts`) as multisets keyed by `(morpheme_ids sequence,
root_morpheme_index)`. The full engine is the parity oracle; the property under test is "the foma
path loses nothing vs full search."

## Denominators

- Indonesian: all 121 corpus words, required 100%. No reduplication exclusion — the composite
  `FomaAnalyzer` (propose union peel, then confirm) is exactly the mechanism that closes the redup
  gap end-to-end, so this file's Indonesian test covers the full, unfiltered corpus.
- Sena: sample-300 (first 300 lines of `sena-words.txt`), required 100%.
- Amharic: the full corpus (673 lines), required 100% on every word the full engine actually
  reaches a result for. A word where the full engine itself times out with a partial result cannot
  be a parity baseline (foma's confirm is uncapped and can legitimately find analyses the timed-out
  full search never reached); a word timing out with zero analyses is excluded outright.

## Test-timing policy

All three tests load a real grammar from the gitignored `samples/data/`, so all three are
unconditionally `#[ignore]`d, each with a self-skip guard so an `--include-ignored` run stays green
where the fixture is absent (CI). Run the full set locally with `cargo test -p pg-foma --release
--test f3_parity -- --include-ignored`.

## The known-failures ledger

`assert_against_ledger` requires the live mismatch set to equal a hand-maintained ledger exactly.
Three outcomes are all fatal: a mismatch not in the ledger (new or regressed gap), a ledger entry
whose live cardinalities changed (the gap moved), or a ledger entry that no longer mismatches (the
bug is fixed, so the entry must be deleted — a fix must shrink the ledger, or the ledger drifts from
reality). Both grammars currently have empty ledgers (full parity); the fixes that got them there:

- **Sena's `musandilesera` miss** (engine 10 analyses, foma 2) was not a confirm/positional bug: the
  8 missing analyses all had the root `é` (morpheme 542) as the first root of an `é + tentar`
  compound, inflected by prefix/suffix slots (`HAB`/`IND`) carried only by another group's template.
  `é`'s own feature-structure ID unified only with one group's key, so the emitter — which routes a
  compound by its main root's group — confined `é`-as-main to that one group, whose template lacked
  those slots; the full engine reaches them via compound-head re-categorization. Fixed in
  `emit.rs`'s `eligible_roots`: admit every root to every group when the grammar has compounding
  rules (upward-safe, since confirm still prunes).
- **Amharic's `ገለፀ` interdigitation miss** was a rendering bug in `preexpand.rs`'s
  `render_all_variants`: the composite emitter rendered only the table-order-first
  letter-series-merged spelling a probed Ge'ez glyph could carry, silently picking the wrong ጸ/ፀ
  series for one root+affix combination. Fixed by rendering every honest spelling variant.

## Amharic stack-size hardening

`amharic_corpus_words_multiset_parity_impl` runs the full 673-word corpus through
`FomaAnalyzer::analyze_word -> confirm::confirm_batch -> Morpher::parse_word_selected` directly on
the test harness's own per-test thread, which — unlike `pg-cli`'s `main()` and
`pg_parse::batch::hc_parse_batch`'s rayon workers — carries no enlarged stack for this recursion
class. This test was once observed to abort abnormally (no panic text) during a long run; a
controlled, otherwise-idle rerun of the same unmodified test completed cleanly, with heavy
concurrent build activity present at the time of the original report. That is inconclusive either
way, but giving this test the same 1 GiB stack its sibling call sites already carry is free (the
OS reserves, not commits, that address space) and closes the gap on the chance it ever is stack
depth — it does not depend on which explanation is right. The test body is spawned onto that
stack via a dedicated thread; this means its `println!` output and any assertion-failure text
bypass libtest's per-test capture (acceptable for an always-slow, diagnostic-heavy gate like this
one).
