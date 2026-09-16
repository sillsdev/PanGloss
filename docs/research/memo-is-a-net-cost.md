# Memoization is a net cost

## Method

44-word Aweti list, release build with `--features alloc-trace`, `--threads 1`, direct binary invocation so stderr is capturable.

## Measurements

Aweti, 44 words, `--step-cap 200000`: memo off = 9,683 ms / 24 capped / 43.4 MB peak. Memo on (256 MiB) = 21,262 ms / 16 capped / 362.7 MB peak. Per capped word (each burns exactly 200k steps): 363 ms off vs 1,136 ms on — each step ~3x more expensive with the memo on, which is the `AnalysisStateKey` clone (Shape + two FeatureStructs + BTreeMap) paid on every lookup, hit or miss.

Sena, 300 words, uncapped (2e9 step cap, all complete): memo off = 1,647 ms / 91.969558 MB; memo on = 1,604 ms / 91.969559 MB. 2.6% faster, memory identical to one byte (peak is dominated by loading the 54 MB grammar).

Mbugwe uncapped does NOT terminate: killed after 11,016 CPU-seconds with no result. Record this — it means step caps are load-bearing on that grammar.

The memo's real benefit is saving STEPS, and under a step cap that is what lets 8 Aweti words complete at all (`memo_corpus_gate` reports them as `completed only on=8`). With the memo off by default those 8 words no longer complete. This is a real cost of the default change, not a free optimization.
