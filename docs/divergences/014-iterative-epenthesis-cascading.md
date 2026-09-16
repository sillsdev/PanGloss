# 014 — Iterative epenthesis cascading

## Kind
Behavioural.

## Status
Fixed-in-rust.

## C# site
`IterativePhonologicalPatternRule`, whose real semantics find one match, apply it (mutating the live
`Word`), and only then look for the next match against the partially-rewritten shape — a true
iterative cursor that never re-visits a position it has already advanced past.

## Rust site
`pg_rules::rewrite::syn_epenthesis` (`rust/crates/pg-rules/src/rewrite.rs`).

## What differs
The original two-epenthesis-rule claim was partly a porting error: the C# last reconfiguration's
feature-rule LHS had been dropped, and restoring that `<PhoneticInput>` makes
`epenthesis_rules_iterative_cascade_finding` pass even with the old Rust epenthesis loop.

The real divergence is the Iterative engine loop. C# finds one target on the live shape, inserts RHS
nodes immediately after it, keeps freshly inserted nodes eligible because an empty LHS has no Clean
filter, and resumes at the first inserted node without revisiting an advanced-past position. Rust
now mirrors that cursor and retains the C# `Shape.Count == 256` runaway guard. The Simultaneous
branch remains collect-all-then-apply against one unmutated snapshot.

**Precise rule each side follows:** C# applies one match, then re-scans the mutated shape for the
next one, for as many iterations as matches remain (each new match reflects earlier insertions). Rust
now does the same for `Iterative`; its `Simultaneous` branch still computes all matches against the
original shape once and applies them at once.

## Can it change a parse?
Yes: the oracle-verified `iterative-epenthesis-cascade` fixture distinguishes the live cursor. It
requires `uotaa -> BOTH|uotaa`, `uotaata -> OVER|uotaata`, and `uotatata -> no parse`.

## Evidence
The corrected port pin is `csharp_port_rewrite.rs::epenthesis_rules_iterative_cascade_finding`.
The cursor pin is `csharp_port_rewrite.rs::epenthesis_rules_iterative_rtl_self_feeds_until_cap`,
which expects the C# `InfiniteLoopException` message. The oracle pin is the upstream fixture
`machine/conformance/edge-cases/iterative-epenthesis-cascade` (red on the old engine and on a first
cursor attempt that let the left context reach back across the cursor); its siblings
`discontinuous-morph-environment`, `simultaneous-feeding`, and
`simultaneous-feeding-control-iterative` pin entries 006 and 016 through the same conformance gate.

## Upstream
None, not applicable — C#'s iterative cursor is the reference behavior and Rust now follows it.

## Notes
The Iterative cursor resumes at the first inserted node, while failed matches advance past their
target. The Simultaneous path remains snapshot-based for entry 016 and `sim_feature` parity.
