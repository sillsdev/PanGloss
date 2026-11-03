# 052 — One per-word work cap covers analysis, inner search and synthesis confirmation

Kind: behavioural
Status: permanent — the work cap is a Rust-only containment design. The evidence below was measured when the analysis-attempt cap was removed.

## C# site

Machine revision `a4b29742b6274a01c7398ca3e800a19fa6d2c9aa`:

- `SIL.Machine/FiniteState/TraversalMethodBase.cs`: `Initialize`, `Advance`, `CheckAccepting`.
- `SIL.Machine.Morphology.HermitCrab/Word.cs`: `ExpandAlternatives`.
- `SIL.Machine.Morphology.HermitCrab/Morpher.cs`: constructor, `SynthesizeSequential`, `SynthesizeParallel`.
- FieldWorks revision `089eb9027b6d81be7883c40960f0de3ffa04b699`, `Src/LexText/ParserCore/HCParser.cs`: parser parameter loading.

## Rust site

- `pg-fst/src/work.rs`: shared work allowance and synchronous thread-local scope.
- `pg-fst/src/traverse.rs`: optional continuation construction, starts, arcs, captures and results.
- `pg-rules/src/stratum.rs`: `WordBudget`, analysis and synthesis orchestration.
- `pg-rules/src/word.rs`: recursive alternative expansion.
- `pg-rules/src/cascade.rs`, `morph.rs`, `rewrite.rs`, `validity.rs`: branch and candidate enumeration.
- `pg-parse/src/morpher.rs`, `root_trie.rs`, `overlay.rs`, `guess.rs`, `surface.rs`: word entry, lookup, confirmation and guessing.

## Difference

This is **resource containment**. Rust exposes an incomplete `CAP` outcome, while HC has no deterministic per-word work allowance. A *work step* is one charged unit of search; a word's *work* is the total of its work steps, and the *work cap* limits that total. The cap is reserved before exploring a branch or materializing a candidate and never exceeds its configured value. Exhaustion is latched, incomplete matcher results are dropped, and an interrupted confirmation cannot be accepted as a completed parse. Completed confirmations from before exhaustion may remain in an explicitly capped outcome. No readiness or grammar-admission decision is added.

History: the cap started as a count of morphological analysis attempts. One attempt could enumerate arbitrarily many FST continuations, and confirmation spent nothing. A first shared-unit cap that reused the attempt number as a work limit took a 400-word Maasai sample from 73 words with analyses to 6, so the attempt cap was kept beside a work cap 100 times larger. The owner then removed the attempt count (`steps`, `--step-cap`, `ParseOutcome.steps`) with no deprecation period, so there is now one cap, in work.

Analysis may spend the whole cap. Once the running total reaches the cap, analysis stops and takes no further unit: the remaining words and strata are skipped. Lookup and confirmation of the candidates analysis had found may then spend up to `cap + cap/16` (saturating; no reserve under a cap of 16). So a word's `work_steps` is at most `cap + cap/16`, and a word whose total work is at most the cap never stops, so its status, parses and `work_steps` cannot change. The reserve exists because, with no attempt cap, analysis can spend the whole cap and starve confirmation of the same word. Analysis can still pass the cap by the in-flight work of the stratum it was in when it stopped; that overshoot comes out of the reserve (the largest measured was 88.8% of it). Synthesis entry points other than a parse (`synthesize_*`, generation, phonology measurement) take the cap exactly with no reserve, and generation has its own explicit cap per caller (`generation_cap`).

HC `Advance` recursively explores optional annotations and clones instances with no work allowance. `Initialize` recursively constructs optional starts. `Word.ExpandAlternatives` eagerly builds an `IList<Word>` recursively. HC's `MaxAlternatives` defaults to zero and is tested by the synthesis caller after that list has been built; a positive value does not contain the construction itself. FieldWorks reads `MaxRoots` and `MaxAlternatives`, defaulting the latter to zero. `MaxStemCount` and the existing iterative epenthesis shape limit do not bound these continuation lists.

These are source findings. The same Maasai runtime failure has **not** been reproduced in C#. There is no shared Machine issue or upstream PR for this entry; it does not assert a different correct grammar interpretation, and a completed parse is unchanged from before.

## Bounds and limitations

One allowance covers input characters, cascade decisions, template slots, allomorph/subrule choices, optional reconstruction, matcher starts/arcs/recursive continuations/commands/accepts, result dedup comparisons, trie choices, alternatives, lexical candidates, environment checks and surface/guess branches. See [the counting contract](../research/per-word-search-work-cap.md) for reservation sites.

Grammar loading and matcher compilation are outside the per-word cap. A work step is not a time or byte bound. A deadline remains an independent containment control. Neither control proves completeness after it fires.

## Measured effect of removing the attempt cap

Method: the v0.13.0 release binary (`--step-cap C`, work cap 100 x C) against this build (`--work-cap 100 x C`), `batch --threads 1`, a word timeout of 900,000 ms that never fired, on a 1,020-word sample set (Indonesian 70, Mbugwe 200, Amharic 250, Aweti 100, Sena 400) at C = 200,000, 20,000 and 2,000, and on the five full word lists (8,740 words) at C = 200,000 only. This is a sample: the full lists were not run at C = 20,000 or 2,000, the Mbugwe full list does not finish at the default cap on either side, and no Maasai grammar exists on this machine, so the Maasai gate could not be run. Words are compared per word by status, analysis set and `work_steps`. Counts only; no word text is kept.

**Words that completed under the old cap.** Every word that was `ok` before is `ok` after with an identical signature and `work_steps`: 0 exceptions of 7,426 (full lists, C = 200,000), 1,014 (sample, C = 200,000), 945 (C = 20,000) and 806 (C = 2,000). This is also guaranteed by construction: a word whose total work is at most the cap is never refused, and `a_word_run_at_a_cap_equal_to_its_work_is_unchanged` checks it on every staged fixture word.

**Words that stopped under the old cap.** Reserve on top of the cap (analysis spends the whole cap, confirmation up to `cap + cap/16`). Columns: words capped before; words with analyses before -> after; words that became `ok`; still capped; still capped with analyses before -> after; capped words that lost every analysis they had.

| Set and C (old attempts) | capped before | with analyses | now ok | still capped | still capped with analyses | lost all |
|---|---|---|---|---|---|---|
| full lists, 200,000 | 284 | 236 -> 184 | 149 | 135 | 102 -> 39 | 63 |
| sample, 200,000 | 6 | 5 -> 6 | 6 | 0 | 0 -> 0 | 0 |
| sample, 20,000 | 75 | 61 -> 69 | 68 | 7 | 5 -> 6 | 0 |
| sample, 2,000 | 214 | 131 -> 134 | 107 | 107 | 55 -> 45 | 21 |

For comparison, the earlier design that stopped analysis at 15/16 of the cap lost 65 (full lists, C = 200,000) and 22 (sample, C = 2,000); with no reserve at all, 94 and 38. All 63 words that lose everything on the full lists are in the Mbugwe list.

**What the reserve does not fix.** A partial signature of a `CAP` word is an unconfirmed snapshot kept for inspection and is not a result. Capped words can lose analyses they had under the old cap because analysis now spends its share on a deeper search.

**Worst per-word work.** Capped words run to the cap plus at most the reserve: the largest work for one word rose from 3,928,759 to 21,250,000 (Aweti full list, C = 200,000: cap 20,000,000 plus 1,250,000) and total work on that list from 100.3 million to 489.1 million; the Mbugwe full list from 2.39 to 4.33 billion. Instruction counts (including a +2.3% Mbugwe regression beyond noise whose cause was not found) are in [the research note](../research/per-word-search-work-cap.md).

**Memory.** Peak working set, single thread, 20 words per grammar capped at C = 20,000 on the sample (Mbugwe, Aweti; the other three have none), 60 s word timeout, 20 GB stop (never reached): at a 2,000,000 work cap, v0.13.0 with `--step-cap 20,000` peaked at 31 MB (Mbugwe) and 21 MB (Aweti), this build at 43 MB and 56 MB, because 17 and 20 of those words now complete. At the native 50,000,000 cap no sample word is capped on either side (peaks 45 / 56 MB base, 46 / 56 MB new). Larger measurements were not made.
Resource limits that still derive from a count of 100 x the old default: the CLI default work cap is 5,000,000,000; `AnalysisPolicy` is 10,000,000 (browser) and 50,000,000 (native ABI v1); the oracle default is 2,000,000. Generation no longer follows the parse work cap and keeps the limits it had before the rename, set explicitly per caller: FFI 500,000, browser 100,000, oracle 20,000, CLI 50,000,000.

## Regression coverage

- `pg-rules/src/stratum/word_budget_tests.rs`: `a_word_capped_on_work_alone_stops_analysis_and_reports_capped`, `analysis_and_synthesis_spend_one_work_allowance`, `analysis_spends_the_whole_cap_and_a_word_within_it_never_stops`, `confirmation_spends_the_reserve_beyond_the_cap_and_no_further`, `confirmation_within_the_reserve_is_not_a_cap`, `a_cap_under_sixteen_has_no_reserve`.
- `pg-parse/tests/work_cap_gate.rs`, registered in `timing`: a work-capped parse reports `CAP`; ordinary words are unchanged; `a_candidate_found_before_the_cap_is_confirmed_in_the_reserve` (a candidate found before the cap is confirmed only through the reserve, and `work_steps <= cap + cap/16`); `a_word_run_at_a_cap_equal_to_its_work_is_unchanged` (every staged fixture word rerun at cap = its own total work keeps status, signature and `work_steps`); oversized and invalid input counters; `generation_reads_its_own_cap_not_the_parse_work_cap`.
- Not caught by any test: removing the analysis-loop `analysis_exhausted()` breaks. They bound analysis work spent after exhaustion in grammars with several strata and many leftover words; the staged fixtures and the homophonous grammar have none, and the measured effect is small (4,472 versus 4,669 units summed over the words that exhausted).
- `pg-cli/src/tests.rs`: `the_removed_step_cap_flag_is_refused_like_any_unknown_option`; `pg-cli/src/stats_cmd/tests.rs`: `a_cache_from_an_older_search_budget_is_refused`; `pg-stats/src/cache/tests.rs`: `a_cache_written_by_the_step_cap_schema_is_wiped_not_read`.
- `pg-fst/tests/fst.rs`, `pg-rules/tests/stratum_gate.rs`, `pg-rules/src/word/tests.rs`, `pg-parse/src/root_trie/tests.rs`, `pg-fst/src/work/tests.rs`: the matcher, synthesis, alternative and trie bounds, unchanged by this entry.

All test data is synthetic. The real-language measurements above are local and no corpus data is committed.
