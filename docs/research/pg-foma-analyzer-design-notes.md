# pg-foma analyzer.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/src/analyzer.rs` implementation comments so the
source can carry a one- or two-line pointer instead of the full argument.

## `ARC_SORT_MIN_ARCS`: why the sort is gated by arc count, not applied unconditionally

`fsm_sort_arcs` trades a one-time sort cost for switching `apply_up`'s per-word traversal from
foma's linear arc-scan branch to its binary-search branch. Measured with a prototype tracer
(`examples/sort_probe.rs`): sorting is a clear win on real grammars — sena (85,763 arcs) 1.49x
propose speedup, amharic (177,177 arcs) 2.05x, with traversal-identical results (states-entered and
candidate sets identical, sorted vs. unsorted) — but on a tiny network (indonesian, 3,263 arcs) the
binary-search bookkeeping outweighs the win: propose throughput regressed ~30%. `ARC_SORT_MIN_ARCS`
(10,000) sits between indonesian's 3,263 (stays unsorted) and sena's 85,763 (gets sorted), so small
grammars keep the cheaper linear scan while large ones get the binary-search speedup.

## The Aweti enumeration-budget motivation

`crate::morphotactics::EnumerationBudget`'s fail-fast check exists because Aweti's uncapped
composite enumeration (855 roots, 123 rules, 3 strata) does not merely run slowly — it produces
2,833,559 fusion entries, a 691MB/9.7M-line lexc source, and an `apply_up` allocation that reached
~8.8GB and killed the process outright, with no panic and no typed error to catch. The budget
returns an honest, compiler-gap error the instant either of two measures (composite entries, or
probed (root, rule) pairs) crosses its cap, before any of that material is built.

`budget_tests::OVERSHOOT_FACTOR` (50) exists to catch a check that stops running per-item and fires
once at the end: in that failure mode the reported value would be Aweti's entire enumeration, orders
of magnitude above any small multiple of the caps used in these tests (10 and 5), which is what
makes a modest overshoot factor enough to distinguish "noticed promptly" from "noticed at the end."
