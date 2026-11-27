# The per-word work cap

The per-word work cap is **resource containment**. A capped or timed-out result is incomplete, regardless of how many analyses it contains. Grammar representability and production admission follow their existing contracts; this allowance adds neither a refusal nor a semantic filter.

## Ownership and counting

A *work step* is one charged unit of search; a word's *work* is the total of its work steps, and the work cap limits that total. `pg_rules::stratum::WordBudget` owns one `pg_fst::work::WorkBudget` for the word. `WordBudget::new(work_cap)` takes the one limit and `Morpher::new(grammar, work_cap)` passes it through. `Morpher` installs it before surface segmentation and keeps it active through analysis, lookup, alternative expansion, synthesis confirmation, validity and surface matching. Stratum entry points also install their supplied budget so direct calls are bounded. Bounded generation installs the same allowance across its strata.

Synchronous matcher calls use a scoped thread-local adapter because the matcher, trie, alternative and rule APIs are also used independently. A scope guard restores its caller during normal return or unwinding and cannot cross threads; each batch word creates its own allowance on its worker thread. Standalone operations with no scope intentionally retain their uncapped API behavior. Clones of a `WorkBudget` consume shared state.

Analysis may spend the whole cap. Its checks refuse a unit once the total has reached the cap, and the analysis loops then stop taking units (remaining words and strata are skipped). Lookup and confirmation run against the cap plus a reserve of `cap/16` (`CONFIRMATION_RESERVE_DIVISOR`; none under a cap of 16; saturating), so they can confirm candidates found before analysis stopped. A word whose total work is at most the cap is never stopped: an analysis check refuses only when the total has reached the cap, which implies the word needs more than the cap, and non-consuming polls latch only above the cap. Each analysis check consumes one work step exactly as it did when it also incremented an attempt counter, so a word that finishes has the same `work_steps` as before the attempt counter was removed.
A successful reservation increments total work by one before the work starts. A refused reservation latches `capped` or `timed_out` and does no additional work. The total never exceeds `cap + cap/16`; analysis can pass the cap only by the in-flight work of the stratum it was in when it stopped, which comes out of the reserve. Synthesis, generation and phonology measurement called outside a parse use `WordBudget::without_reserve` and never exceed the cap. A reservation reads the clock once per 64 units, so a timeout can overshoot by at most 63 units; an explicit poll always reads it. The cap check is exact on every reservation. Polling at exactly the cap conservatively latches exhaustion: finishing a branch does not establish that the whole search completed. Failed matchers discard their partially assembled result list; the parse owner checks the latch before accepting confirmation. Earlier completed analyses can survive in an explicitly incomplete outcome.

| Phase | Reservation sites |
|---|---|
| Surface input | Each Unicode scalar before allocating the search shape |
| Analysis and synthesis | Cascade branch decisions, attempted morphological rules, synthesis orchestration, template slots and candidates |
| Rule application | Allomorph/subrule choices, analysis matches, non-head root resolution, optional reconstruction and dedup comparisons |
| FST | Start positions, recursive initialization/advance, each inspected arc, register commands, accepting outputs and distinct candidates |
| Phonological rewriting | Subrule choices, iterative passes, candidate spans and insertion sites; nested matching spends the same allowance |
| Root/overlay lookup | Query nodes, trie edges, optional skip branches and accepted entries |
| Alternatives | Each recursive expansion and source replay |
| Confirmation | Lexical/allomorph candidates, expansion outputs, synthesis outputs, environments and disjunctive allomorph checks |
| Guess/surface matching | Recursive match branches and lexical pattern choices |

The existing epenthesis shape bound and grammar-derived finite limits remain in force. No larger limit substitutes for proof of a complete result.

## Costs within a unit

A reservation bounds search branching, not CPU instructions or bytes. Feature comparisons and shape/capture copies operate on the finite grammar and current shape. Input size is reserved before segmentation; subsequent growth is bounded by charged applications/reconstruction and each grammar rule's finite output. Grammar-sized scans, hashing/shape keys, copies and serializing already accepted candidates are finite work within those reservations. Final FST sorting is `O(R log R)` on a result list whose construction has already spent at least one unit per result; dedup candidates also spend reservations. Consequently the allowance does not promise constant latency per unit or a portable millisecond bound.

Grammar loading, lexical index construction and cached matcher compilation precede per-word parsing. Their wall time must be measured separately. Caller-supplied selector callbacks are caller code; the allowance cannot preempt a callback that does not return. The optional deadline remains independent, and no new deadline is the deterministic-cap fix.

## Default work cap and measured tradeoff

`--work-cap N|unbounded` bounds the reservation sites above; omitted, it is 5,000,000,000. That is the former default `--step-cap 50000000` times the former 100-fold work multiplier, so it limits what it always limited. `AnalysisPolicy` is 10,000,000 (browser, formerly 100,000 attempts) and 50,000,000 (native ABI v1, formerly 500,000); the oracle default is 2,000,000 (formerly 20,000). `Morpher::new` takes a work cap and every caller was converted by the same factor of 100. Generation no longer follows the parse work cap. It has its own explicit `generation_cap` per caller, equal to the limits it had before: FFI 500,000, browser 100,000, oracle 20,000, CLI 50,000,000.

The 94 completing Windows diagnostic rows from the original measurement used a median 1,456,880.5, nearest-rank p90 8,290,898 and maximum 19,717,051 work steps, so a 20,000,000 cap (the old 200,000-attempt setting) sat 1.4% above the observed maximum. At 0.31 ms per 1,000 work steps that is about 6.2 seconds of search on that host. The observation is not a completeness proof for capped words or other grammars.

The Windows integrator compared 400 Maasai sample words with an 8-second deadline: v0.6.1 had 73 words with analyses; a first shared-unit cap that reused the attempt number as a work limit had 6. That scar is why removing the attempt cap was measured before it was done. No Maasai grammar or sample exists on this machine, so the gate ran on the five local real grammars.

## Measured effect of removing the attempt cap

These runs are a sample, cut down on the owner's instruction to keep the evidence "accurate enough". Setup: v0.13.0 (`--step-cap C`, work 100 x C) against this build (`--work-cap 100 x C`), `batch --threads 1`, word timeout 900,000 ms (never fired, so no word was excluded). Sets: a 1,020-word sample (Indonesian 70, Mbugwe 200, Amharic 250, Aweti 100, Sena 400) at C = 200,000, 20,000 and 2,000 (these three finished before the scope was reduced), and the five full lists (8,740 words) at C = 200,000 only. Not run: the full lists at C = 20,000 and 2,000, and the Mbugwe full list at the default cap (it does not finish in either binary: 713 of 1,645 rows after 54 CPU-minutes on one word, then stopped). The reduced instruction and memory runs below say what they left out. Per word: status, analysis set and `work_steps`; counts only.

- Completed words: 0 exceptions. Every word `ok` before is `ok` after with the same signature and `work_steps` (7,426 on the full lists at C = 200,000; 1,014 / 945 / 806 on the sample at C = 200,000 / 20,000 / 2,000). By construction a word with total work at most the cap is never refused; `a_word_run_at_a_cap_equal_to_its_work_is_unchanged` checks it on every staged fixture word.
- Capped words (reserve on top of the cap): full lists at C = 200,000, 284 capped before, 149 now `ok`, 135 still capped; words with analyses 236 -> 184; 63 capped words lose every analysis they had (all Mbugwe). Sample: 6 / 75 / 214 capped before at the three caps, 6 / 68 / 107 now `ok`, 0 / 7 / 107 still capped, 0 / 0 / 21 lose everything. Tables in [divergence 052](../divergences/052-shared-search-work-cap.md). The earlier design that stopped analysis at 15/16 of the cap lost 65 (full lists) and 22 (sample, C = 2,000).
- Worst word: 3,928,759 -> 21,250,000 work steps (Aweti full list, C = 200,000; cap 20,000,000 plus the 1,250,000 reserve); list totals 100.3 -> 489.1 million (Aweti) and 2.39 -> 4.33 billion (Mbugwe).
- Memory: 20 capped words per grammar from the sample (Mbugwe, Aweti), single thread, 60 s word timeout (none reached it), 20 GB stop (never reached). At a 2,000,000 work cap v0.13.0 peaked at 31 MB / 21 MB and this build at 43 MB / 56 MB (17 and 20 of those words now complete); at the native 50,000,000 cap no sample word is capped on either side (peaks 45 / 56 MB against 46 / 56 MB). No capped word was measured near 50,000,000 work, so peak memory on the longest capped words is unmeasured.
## Instructions

`instruction-compare.ps1` (PMC, one CPU, two repeats, empty-list startup subtracted), reduced scope: every third word of each default-cap sample list (Indonesian 24, Mbugwe 67, Amharic 84, Aweti 34, Sena 134 words; 343 in all), every word `ok` and identical on both sides, v0.13.0 against the final build. No same-binary control was run in this reduced round. Parse instructions changed by +0.45% (Indonesian, noise 1.02%), +2.34% (Mbugwe, noise 0.11%), +0.23% (Amharic, noise 0.25%), +0.19% (Aweti, noise 0.45%) and +1.88% (Sena, noise 2.42%). Four verdicts read NO-GO within noise.

Mbugwe reads NO-GO as a regression beyond noise: +2.34%, and +2.29% / +2.48% against v0.13.0 / v0.13.1 after the `tick` was simplified. The earlier 1/16-reserve design measured +0.35% on this same third of the Mbugwe list, so the cost came in with the reserve redesign (the `analysis_exhausted` checks and the strict cap compare). The cause was not found and the cost is unfixed: the simplified `tick` changed nothing, and no bisect build was made. The parse is unaffected: every word's status, signature and `work_steps` are identical. Earlier whole-sample figures for the previous design (+0.01% to +0.48%) are superseded.

Worst words: capped words now run to the cap plus the reserve. Aweti's instructions per work step on its sample list are about 1,700, so the 21,250,000-step word costs on the order of 36 billion instructions, against about 6.7 billion for the old 3,928,759-step worst word. That per-word figure is an estimate from the list rate, not a per-word trace.

Optimizer recipes: the 7 conformance fixtures with the most confirmed candidates (5 each) were rerun with developer-tools builds of v0.13.0 and this build. 6 of 7 picked the same candidate; one picked the same candidate id and differed only in the backend suffix of the winner string; `morphotactic-attribute-breadth` has no winner on either side. This is a sample of 7 of 161 fixtures; the previous round over all fixtures (before the reserve redesign) had 157 of 160 the same winner.
## Raw-path weight in the optimizer ranking

`Score::key` used to add confirmation attempts and raw proposer paths 1:1. A work step is about 30 times finer than an attempt, so a plain swap to work would have shrunk raw paths to almost nothing and reopened the 575-versus-127-proposals mis-ranking (the Sena-shaped unit test shows the 1:1 sum picking the wrong candidate in work units). The weight is now `RAW_PATH_WORK_WEIGHT = 5` work steps per raw path.

Measurement (`lab raw_path_weight`, one phase per process, retired instructions from a PMC trace, process pinned to one CPU, two repeats per phase; word lists repeated 40 times so the net exceeds the startup spread): instructions per raw path is `(propose - startup) / raw_paths`; instructions per confirmation work step is `(analyze - propose) / confirmation_work`.

| conformance grammar | raw paths | confirmation work | instructions per raw path | per work step | weight |
|---|---|---|---|---|---|
| backend-strata-generic | 55,480 | 1,423,400 | 14,046 | 2,535 | 5.5 |
| backend-ordered-generic | 27,000 | 481,400 | 10,642 | 3,304 | 3.2 |
| backend-template-generic | 37,040 | 79,844,960 | 17,688 | 3,386 | 5.2 |
| optional-template-composite | 2,320 | 117,000 | 16,840 | 3,871 | 4.4 |

Mean 4.6, range 3.2 to 5.5; five is the nearest whole number. The Indonesian grammar's foma compile did not finish under the machine load of this run, so no local real grammar was measured; this is four small synthetic grammars. Repeat spread was under 5% for propose and analyze (startup 0.1% to 26%, subtracted).

## Diagnostics and synthetic verification

`work_steps`, rich trace `workSteps`, and the opt-in `WORK_STEPS` line report the total. `CAP` and `ParseOutcome.capped` mean the work cap fired (analysis reached the cap, or confirmation used up the reserve above it), and remain incomplete even when earlier complete confirmations survive. The independent deadline reports `TIMEOUT`. Short invalid surfaces report their input reservations; oversized input can reach the work cap before allocation.

The synthetic gates distinguish mechanisms by effect: a work cap of 8 stops a parse exactly at 8; an optional-stem synthesis and deterministic/nondeterministic optional matcher stop at exactly 64 work steps; alternative and trie gates stop at 32 and 16. Unit tests pin that analysis and synthesis spend one allowance, that analysis spends the whole cap and a word within it never stops, that confirmation spends the reserve and no more, that a cap under sixteen has no reserve, and that an unbounded allowance counts without capping. CLI tests cover `--work-cap` syntax, unbounded, zero refusal, the refusal of `--step-cap`, and sequential/parallel effects. Stats tests pin identical-cap reuse and refusal of older budget policies and schemas before TSV truncation.

See [divergence 052](../divergences/052-shared-search-work-cap.md) for coverage status.