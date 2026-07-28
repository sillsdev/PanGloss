# Aweti performance follow-on plan

Status: proposed after Tasks 4, 5, and 7. This document ships no new optimization.

## Decision and evidence boundary

Keep Task 5's prepared-outgoing-arcs path. Make **semantic path canonicalization** the next experiment only if new diagnostics show that duplicate semantic paths materially drive proposal or confirmation cost. If they do not, run a **targeted automaton-intersection membership** spike. All remaining options below are hypotheses, not predicted wins.

The only shipped speedup is outgoing-arc preparation: one-time cost **5.364 ms**; bounded traversal for `parua`/`an`/`ti` fell **2.159 → 0.889 ms** (2.43x), with exact recorded candidate/analysis identities, **100/106** recall, all 18 rules, and the **10,609-state / 298,830-arc** network unchanged.

## Measured time split

| Area | Measured result | What it supports—and does not support |
|---|---|---|
| Exact P6 compile | 913.535 ms: emission 30.1%, lexc 24.4%, rules 4.9%, cleanup 0.2%, final compose/minimize 40.4% | Startup work is worth isolating, but the combined final bucket cannot yet justify deleting or altering an operation. |
| Bounded proposal | Prepared traversal 0.889 ms vs 2.159 ms before; break-even about 13 lookups | Retain preparation. |
| Decode/dedup | 0.042 ms / 5.113 ms measured word work (0.8%) | Incremental decoding is low priority unless a broader sample changes this. |
| Confirmation | 2.912 ms / 5.113 ms (56.9%), 3–5 true groups per probe | Confirmation is the leading word-time hypothesis; the present sample proves no safe partition. |
| Task 7 | Aweti sweep 12.0115363 s; Sena/Amharic engine totals much larger than proposer timing | Cross-language/full-engine totals do not identify a P6 internal bottleneck. |

The Task 4/5 traces and Task 7 release runs are distinct revisions/invocations. Do not compare them as one benchmark.

## Safety contract

- Keep the 50,000 raw-path allowance; a cap change is not a speedup.
- Confirm only complete candidate sets; preserve candidate **multisets** and confirmed analyses, not first-result reachability.
- Preserve the 100/106 recall set and six named misses, 18 compiled rules, and final states/arcs unless an approved semantic change explains a difference.
- A watchdog expiry is `UNMEASURED`, never zero analyses.

## Ranked experiments

| Rank | Change | Red test first | Trigger / bounded experiment | Ship criterion |
|---:|---|---|---|---|
| 1 | Semantic path canonicalization | Equivalent-chain fixture: raw paths exceed canonical paths, but candidate multiset and analyses are equal. | Add canonical-path count to the P6 trace; run `parua`, `an`, `ti`, and one duplicate-heavy word under the same cap; record confirmation time. | Material duplicate reduction and traversal/confirmation improvement with full equality. |
| 2 | Targeted automaton intersection for membership | Compare current confirmation with intersection membership for covered, uncovered, ambiguous, and tag-bearing candidates. | Same candidate inputs; count false accepts/rejects, engine calls, and group timing, including one capped oracle word. | Exact acceptance/rejection and analysis equality, no timeout, material confirmation reduction. |
| 3 | Earlier quotienting/determinization | Fixture proves proposed quotient preserves `apply_up`/`apply_down` relation and tags. | Split the final 369.271-ms bucket; record intermediate states/arcs and stage durations, then sweep all 106 words. | Exact relation/recall, no skipped rule, material startup or network reduction. |
| 4 | Confirm-group partitioning | Group-key test proves no distinct candidates merge and no formerly shared result splits. | Trace group-size distribution, calls, and timing over representative words; compare confirmed-analysis multisets. | Material engine-call/time reduction with exact analyses and no cap regression. |
| 5 | Incremental decoding | Long-output fixture preserves decoded candidate multiset under chunked decode. | Do not prototype until broader diagnostics put decode/dedup at ≥10% of proposal-plus-confirm time. | Threshold first, then material decode improvement with exact candidates. |
| 6 | Content-addressed compiled-network cache | Cache key test: identical grammar/config hits; changed grammar/rule/config misses; stale network cannot be reused. | Cold/warm process benchmark, hit/miss counters, state/arc equality, corrupt-entry recovery. | Material warm-start gain, every invalidation miss, relation unchanged. |

## Execution order

1. Extend opt-in diagnostics before changing behavior. Record raw/canonical paths, candidate multiset, analyses, confirmation calls, cap status, and timing.
2. If canonical duplicates are material, implement only the canonicalizer behind its focused equality test. Otherwise run the membership helper as test-only code against the current full-engine oracle.
3. Separately split final composition from minimization. Only the largest repeatable substage may receive a quotient/determinization prototype.
4. Reconsider confirmation partitioning, incremental decoding, or a cache only when their table trigger is observed.

Every candidate follows red-green-refactor and is reported with exact command, SHA, toolchain, watchdog, inputs, states/arcs, candidate multiset, confirmed-analysis multiset, and stage timings. At minimum run:

```powershell
cargo test -p pg-foma --release --test p6_templated_morphotactics_gate -- --include-ignored --nocapture --test-threads=1
cargo run --release -p pg-foma --example p6_aweti_perf_trace
```

## Explicitly rejected shortcuts

- Raised budgets hide path growth and weaken the typed preflight refusal.
- Early stopping changes ambiguity and multiplicity semantics.
- Beam pruning can discard required analyses.
- Truncation cascades turn incomplete traversal into false negative evidence.
- Multiplicity loss invalidates candidate/analysis equality and parity expectations.

The release gate must stay **100/106** with the same six misses; this is not permission to convert those misses into acceptable losses. A change without both a measured trigger and an exact-behavior proof remains a documented hypothesis.

## Scope

This targets the Aweti P6 compiler/proposer path only. It neither changes recipe-space search nor treats Sena, Indonesian, or Amharic timing rows as Aweti bottleneck evidence.

## Reader-tested operational definitions

For this plan, **material** means at least a 20% reduction in the targeted measured stage on the same bounded inputs, with no invariant failure. A **semantic path** is the normalized candidate identity used by the existing decode/dedup comparator; a canonicalizer may merge paths only when that comparator says their candidate identities are equal. Equality is evaluated as the exact candidate multiset and confirmed-analysis multiset, using the current test helper's normalization—not an unordered set or first result.

The new opt-in trace fields are `raw_paths`, `canonical_paths`, `unique_candidates`, `confirmation_groups`, `confirmation_calls`, `cap_status`, traversal/decode/confirmation elapsed time, and exact-equality status. The duplicate-heavy probe must be selected from a preliminary capped corpus scan and named in the report; if no word has a material duplicate ratio, the canonicalization branch stops.

The baseline is the Task 5 after profile (`fb3e753`) and its reported 50,000 path allowance, 5.364-ms preparation, 0.889-ms three-probe traversal, 100/106 recall, 18 rules, and 10,609/298,830 final network. “No timeout” means every bounded input returns before the existing external watchdog and reports `cap_status=complete`; a typed preflight refusal remains `UNMEASURED`.

The membership spike is test-only: it must run beside, not replace, current full-engine confirmation until equality is demonstrated. Its report compares both paths on identical candidate inputs before any production-path proposal.

Canonicalization pressure is **material** only when `(raw_paths - canonical_paths) / raw_paths >= 20%` on at least three naturally occurring corpus words, including the named duplicate-heavy probe, with every probe complete under the 50,000-path allowance. A synthetic fixture proves equality only; it cannot satisfy this workload trigger.

For a sub-millisecond stage, run each before/after bounded probe 30 times in one process after the same preparation step, report median, minimum, maximum, and all 30 equality outcomes, and use the median for the ≥20% ship threshold. Any incomplete run, watchdog expiry, or equality failure rejects the timing comparison rather than being discarded.
