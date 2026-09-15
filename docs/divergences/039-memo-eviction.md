# 039 — Memo eviction

This entry separates implementation status from the evidence needed to trust it across both engines.

## Kind
efficiency.

## Status
open. Implemented, env-gated OFF by default (`HC_MEMO_EVICT`). No policy/weight has been tuned and
no release-binary five-grammar parity sweep has been run — see Remaining work.

## C# site
(no C# equivalent — `AnalysisScope.cs:97-108`'s three admission caps have no eviction path at all;
a refused store is permanent for the rest of that parse.)

## Rust site
`pg-memo/src/lib.rs` (`evict` module, `AnalysisScope::evict_for`/`store_entry`/`note_hit`/
`has_byte_capacity_after_evicting`), called from `pg-rules/src/stratum.rs::memo_apply_rules` and
`run_template_batch`.

## Mechanism
Each table (`memo`, `template_memo`) keeps a min-heap of `(pr, seq, key)` nodes alongside its
existing `HashMap`. `pr` is a GreedyDual-Size-style priority derived from a per-scope logical
`clock`, integer arithmetic only (no floats — determinism is load-bearing under a step cap):
`HC_MEMO_EVICT_POLICY=lru` sets `pr = clock` (recency only), `size` sets `pr = SCALE / bytes` (size
only), and `gdsize` (default once eviction is on) sets `pr = clock + weight * SCALE / bytes`, with
`HC_MEMO_EVICT_WEIGHT` the recency-vs-size trade. A byte-budget refusal first pops the
minimum-priority node whose `pr` still matches its entry's own `pr` (a stale node left behind by an
earlier hit is discarded without touching the entry) and whose key is not in the table's
`in_progress` set, removes that entry, and advances `clock` to the evicted node's own `pr`.

## Safety argument
Within one word's parse, this stays a pure cache in exactly the sense the pre-existing admission
caps already are (`pg-memo`'s own module doc: "a miss always falls back to full recomputation"):
evicting an entry can only turn a future hit into a future miss, which re-derives the identical
subtree, since the mrule cascade is a deterministic function of `AnalysisStateKey` alone. In an
uncapped search this can only change *when* work happens, never what a completed word returns.

## Documented limit
`docs/research/memo-entry-work-value.md` §11 (`memo_corpus_gate`) already establishes that, under a
*step cap*, the memo is provably not a pure performance device: 11 of Aweti's words finish only
because the memo is on (`aweti completed only on=11`), because a degraded hit rate changes how many
steps a capped search spends before running out. Eviction degrades the hit rate on purpose, so this
same corpus gate — not `memo_parity_gate`'s uncapped identity comparison, which cannot see a
completion change — is what any future tuning must keep watching. This entry's own evidence reruns
that gate with eviction on at the default 256 MiB budget specifically to confirm nothing evicts
there and the tally does not move; characterizing what happens once eviction actually fires is the
tuning pass's job, not this one's.

## Evidence
`pg-memo/src/lib.rs::tests` (`eviction_off_by_default_never_touches_the_counters`,
`eviction_on_frees_room_and_decrements_both_counters`, `in_progress_keys_are_never_evicted`,
`eviction_sequence_is_deterministic_across_two_runs_of_the_same_input`) each assert a counter or
fire-count effect directly (a decrement, a nonzero eviction count, an entry's survival, a
byte-identical repeat), not merely that an API returned without panicking; each was confirmed to
fail with its own guard reverted. `pg-parse::memo_parity_gate` and `pg-foma::memo_corpus_gate` were
rerun with eviction off and with eviction on at the default 256 MiB budget: unchanged in both
configurations, `aweti completed only on=11` included (see the implementing PR for verbatim counts).

## Remaining work
No policy (`lru`/`size`/`gdsize`) or weight has been tuned; `HC_MEMO_EVICT_WEIGHT`'s default of `1`
exists only so the mechanism has a value, not because it was measured. The release-binary
`parse_compare.py` comparison across the five reference grammars with eviction actually forcing
evictions (a byte budget small enough to matter, not the 256 MiB default) has not been run — that
comparison, `memo_corpus_gate`'s four-way tally at whatever cap the tuning pass settles on, and any
policy/weight selection belong to that later pass, not this one.

## Upstream
None filed. C# has no eviction path to diverge from, and this entry does not propose adding one —
see the README's `efficiency` kind: no C# behavior change is suggested, only a Rust-side
memory-management device that is inert by default. Fold the tuning pass's findings back into this
entry once it lands, including whether `HC_MEMO_EVICT`'s default ever changes from OFF.
