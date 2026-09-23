# Try-a-word timing coverage

Goal: make every supported timing category measure the work it claims, and show the
remaining unclassified parse time honestly. Keep the stats-off search unchanged.

## Evidence from six read-only Herdr reviews

- Morphological synthesis records work without a timer (`stratum.rs::guided_synth`).
- Guesser and overlay record work but expose no time (`morpher.rs`).
- Non-head root lookup drops stats and is charged to the enclosing morph rule.
- Phonological synthesis and analysis metathesis have timers but incomplete counters.
- The rich trace aggregates both directions and offers no reconciliation to parse elapsed.
- The existing tests use conformance fixtures; none establish a five-real-grammar timing hit map.

## Implementation

1. Add failing tests for morph synthesis time, guesser/overlay time, non-head lookup
   time, and category-to-wall-clock reconciliation. Preserve parse outcome parity.
2. Add timers at the actual operation boundaries. Keep nested timers disjoint; include
   traced allomorph loops so per-allomorph timing works in Try-a-word.
3. Add a measured/unattributed breakdown to rich-trace JSON, and direction totals so
   aggregate timing cannot conceal an untimed direction. Treat absent activity as zero,
   not as an unsupported category.
4. Use `pg.ps1 -Mode check` first, focused tests second, then run a bounded trace on
   one parseable word from each available real grammar. Record what categories fired
   and the residual rather than claiming that every grammar exercises every branch.
5. Review the full diff and re-run authoritative focused verification. Do not touch
   unrelated main-checkout files.
