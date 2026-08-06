# pg-foma morphotactics.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/src/morphotactics.rs` implementation comments
so the source can carry a one- or two-line pointer instead of the full argument.

## Enumeration budget (`EnumerationBudget`): why two measures, not one

`ProbeBudget` is measurement-only (off by default, panics when tripped — fine for a
developer-run diagnostic, wrong for production). `EnumerationBudget` is its default-on,
non-panicking sibling: always live in `crate::emit::emit`/`emit_with_precision`, latching a
shared, cross-thread flag the instant either of its two measures crosses threshold. Every
recursive enumeration call checks the latch before doing further work. A trip surfaces as
`FomaTier::Unsupported` plus `EnumBudgetExceeded`, which `FomaProposer::new` turns into a typed
`FomaError::EnumerationBudgetExceeded` — never a panic, never a silent OOM.

A pairs-probed cap alone does not catch every blow-up shape. The Aweti grammar (855 roots, 123
rules, 3 strata, 14 templates) probes "only" ~8.37 million (root, rule) pairs — unremarkable in
isolation — before producing 2,833,559 composite (fusion) entries, a 691 MB / 9.7M-line `.lexc`,
and an ~8.8 GB `apply_up` allocation that kills the process on the first word. The number that
predicts the disaster is the *result* of the probes (composite entries emitted), not the probe
count. So the budget tracks both: composite entries (primary, disaster-predicting) and pairs
probed (a cheap secondary backstop for a runaway search that has not yet produced many entries).

Calibration: `DEFAULT_ENTRY_BUDGET = 200_000` composite entries. Amharic (largest reference
grammar that must keep working) produces 22,775 fusion entries and zero interdigitation/structural
at this writing — ~8.8x margin. Aweti's 2,833,559 crosses it after ~7% of its own full
enumeration, well before the 691 MB lexc / 8.8 GB allocation. `DEFAULT_PROBE_BUDGET = 3_000_000`
pairs: Amharic probes ~305k for `build_composites` (~9.8x margin); Aweti probes ~8.37M, so a
grammar shaped like Aweti but with lower entry yield per probe still trips on search-tree size
alone. Override with `HC_ENUM_ENTRY_BUDGET`/`HC_ENUM_PROBE_BUDGET` (parsed as `usize`; unset or
unparsable falls back to the default), mirroring the existing `HC_PREEXPAND_PROBE_CAP` convention.
