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
minimum-priority node whose `key` is not in the table's `in_progress` set and whose `seq` still
matches its entry's own `seq` (a stale node left behind by an earlier store or hit is discarded
without touching the entry), removes that entry, and advances `clock` to the evicted node's `pr`.
An in-flight node is popped (there is no decrease-key) but always re-pushed before the pass returns,
so a key currently on the call stack stays evictable once its guard clears rather than losing its
heap node permanently. Staleness is keyed on `seq`, not `pr`, because two nodes for the same key can
share one `pr` (an unmoved `clock` recomputes the same value on every hit); an earlier draft checked
`pr` and could let a stale, low-`seq` duplicate cause a just-hit entry to be evicted ahead of a truly
older one (caught by review before merge, fixed, and pinned — see Evidence). A repeated hit still
pushes a fresh node without a decrease-key, so a table's heap can accumulate many stale duplicates
for one hot key; `AnalysisScope` compacts a table's heap back down to (roughly) its live entry count
once stale duplicates exceed a small multiple of it, keeping the heap's own accounted bytes bounded
rather than proportional to total lookup volume (see Evidence for a measured before/after).

## Safety argument
Within one word's parse, this stays a pure cache in exactly the sense the pre-existing admission
caps already are (`pg-memo`'s own module doc: "a miss always falls back to full recomputation"):
evicting an entry can only turn a future hit into a future miss, which re-derives the identical
subtree, since the mrule cascade is a deterministic function of `AnalysisStateKey` alone. In an
uncapped search this can only change *when* work happens, never what a completed word returns.

## Documented limit
`docs/research/memo-entry-work-value.md` §11 (`memo_corpus_gate`) already establishes that, under a
*step cap*, the memo is provably not a pure performance device: some of Aweti's words finish only
because the memo is on (`completed only on`), because a degraded hit rate changes how many steps a
capped search spends before running out. Eviction degrades the hit rate on purpose, so this same
corpus gate — not `memo_parity_gate`'s uncapped identity comparison, which cannot see a completion
change — is what any future tuning must keep watching. This entry's own evidence reruns that gate
with eviction on at the default 256 MiB budget specifically to confirm nothing evicts there and the
tally does not move; characterizing what happens once eviction actually fires is the tuning pass's
job, not this one's. (This branch's current `completed only on` count is 8, not the 11 an earlier
research note recorded — reproduced identically on the pre-eviction base commit, so the drift
predates this change and traces to corpus/fixture movement elsewhere, not to anything here.)

## Evidence
`pg-memo/src/lib.rs::tests` (`eviction_off_by_default_never_touches_the_counters`,
`eviction_on_frees_room_and_decrements_both_counters`, `in_progress_keys_are_never_evicted`,
`in_flight_node_is_requeued_and_still_evictable_once_cleared`,
`a_stale_duplicate_never_causes_a_recently_hit_entry_to_be_evicted_over_an_older_one`,
`heap_compaction_bounds_stale_duplicate_growth`,
`eviction_sequence_is_deterministic_across_two_runs_of_the_same_input`) each assert a counter or
fire-count effect directly (a decrement, a nonzero eviction count, an entry's survival, a bounded
heap length, a correct-victim identity, a byte-identical repeat), not merely that an API returned
without panicking; each was confirmed to fail with its own guard reverted. `pg-parse::memo_parity_gate`
and `pg-foma::memo_corpus_gate` were rerun with eviction off and with eviction on at the default
256 MiB budget: unchanged in both configurations (see the implementing PR for verbatim counts).

**Heap-node cost, measured, not assumed**: a from-scratch review (before merge) flagged that every
insert and every hit pushes a full `AnalysisStateKey` clone with nothing ever compacting the heap.
`HC_MEMO_STATS=1`'s `MEMOPROF` line's `memo_heap_bytes`/`tpl_heap_bytes` (added for exactly this)
measured, on `oteʼikateʼika` at a 1,000,000-step cap, eviction on, default 256 MiB budget (so
`memo_evictions=0` — nothing evicted, only accumulated): **56,766,032 / 80,262,744 bytes** (~137 MB
combined) before compaction existed, against 11,824 live mrule-memo entries and 9,485 live
template-memo entries — i.e., heap overhead rivaling or exceeding the tables' own already-tracked
key-byte cost, confirming the review's concern rather than assuming it. After adding
`AnalysisScope::maybe_compact_heap` (rebuild a table's heap once stale duplicates exceed ~2x its live
entry count), the same run measured **30,935,088 / 16,758,832 bytes** (~48 MB combined) — roughly a
65% reduction, `tpl_heap_bytes` down ~79% since the template table's hit-to-insert ratio (and so its
stale-duplicate rate) was the higher of the two.

## Remaining work
No policy (`lru`/`size`/`gdsize`) or weight has been tuned; `HC_MEMO_EVICT_WEIGHT`'s default of `1`
exists only so the mechanism has a value, not because it was measured. `HEAP_COMPACT_RATIO`/`_SLACK`
(the compaction trigger) are likewise unmeasured placeholders, not tuned constants. The release-binary
`parse_compare.py` comparison across the five reference grammars with eviction actually forcing
evictions (a byte budget small enough to matter, not the 256 MiB default) has not been run — that
comparison, `memo_corpus_gate`'s four-way tally at whatever cap the tuning pass settles on, and any
policy/weight/compaction-ratio selection belong to that later pass, not this one.

## Upstream
None filed. C# has no eviction path to diverge from, and this entry does not propose adding one —
see the README's `efficiency` kind: no C# behavior change is suggested, only a Rust-side
memory-management device that is inert by default. Fold the tuning pass's findings back into this
entry once it lands, including whether `HC_MEMO_EVICT`'s default ever changes from OFF.
