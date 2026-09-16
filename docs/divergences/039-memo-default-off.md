# 039 — Memoization default differs from C#

## Kind

behavioural. The underlying memo is an efficiency mechanism, but its step savings change the observable capped/completed outcome under PanGloss's finite step budgets.

## Status

open. PanGloss deliberately defaults the library `Morpher` and CLI to memoization off; callers can opt back in with `Morpher::with_memo(true)` or `--memo=on`. C# keeps memoization on.

## C# site

`AnalysisScope` memoization used by `AnalysisStratumRule` and `AnalysisAffixTemplatesRule`.

## Rust site

`pg-parse/src/morpher.rs::Morpher::new` and `pg-cli/src/main.rs::run_batch`; the retained implementation is in `pg-memo` and `pg-rules/src/stratum.rs`.

## Evidence

Current Rust commits: `779776c8` (preserved measurement instrumentation and gate fix) and `2493d11b` (memo default change).

Current C# checkout commit: `a4b29742b6274a01c7398ca3e800a19fa6d2c9aa` (`master`, `Port transductive alignment model (#466)`).

The direct release-build measurements are recorded in [`docs/research/memo-is-a-net-cost.md`](../research/memo-is-a-net-cost.md): Aweti memo off is 9,683 ms / 24 capped / 43.4 MB peak versus memo on at 21,262 ms / 16 capped / 362.7 MB peak; Sena is 1,647 ms / 91.969558 MB off versus 1,604 ms / 91.969559 MB on; and uncapped Mbugwe was killed after 11,016 CPU-seconds without a result.

The corpus gate records the cost directly: 8 Aweti words complete only with memo on. With memo off by default those 8 words no longer complete under the same step cap. A step cap is therefore load-bearing on Mbugwe and is not interchangeable with an uncapped run.

`pg-parse`'s `memo_is_off_by_default_but_can_be_enabled_explicitly` test pins the constructor default and the explicit opt-in. `pg-foma/tests/memo_corpus_gate.rs` compares memo-on and memo-off identities for words that complete on both sides and reports the one-sided completion buckets and timeouts.

## Remaining work

This is an intentional PanGloss policy divergence, not a claim that memo replay changes the accepted parse. Capped words are incomplete evidence and must not be used as parse parity. Any future default change must repeat the differential corpus measurement and preserve the explicit memo opt-in.

## Upstream

No Machine issue or fix PR is claimed: C#'s memo-on policy remains the founding-oracle behavior, while this is a PanGloss resource/performance default chosen from the measurements above.
