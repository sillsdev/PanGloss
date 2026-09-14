# 017 — RTL + bounded quantifier: shallow-reverse mirror risk

## Kind
Behavioural (structural risk — not yet reproduced on a real word).

## Status
Open.

## C# site
None directly — this is a structural risk internal to the Rust FST-compilation path
(`pg-foma`), not a place where C# and Rust run different algorithms. It is catalogued here because
its consequence, if reachable, is a silent Rust-only omission relative to what the confirm-time
engine (which does match C#) would otherwise find.

## Rust site
`pg_foma::replace::reversed_slots` (`rust/crates/pg-foma/src/replace.rs:394-396`) and
`compile_rtl_branch_net` (`:502-507`).

## What differs
`reversed_slots` is a **shallow** reverse: `slots.iter().rev().cloned()`. It does not recurse into a
`Slot::Repeat`'s own `children`. The `RightToLeft` construction applies it to the LHS, RHS, and both
environments to build the mirror rule. For a slot list containing a `Repeat` whose children are not
themselves palindromic, the mirror is not the true reverse of the original: reversing
`[Fixed(y), Repeat{[a,b]}, Fixed(x)]` must yield `[Fixed(x), Repeat{[b,a]}, Fixed(y)]`, but the
shallow reverse leaves the group's interior in document order — `[Fixed(x), Repeat{[a,b]}, Fixed(y)]`.

The combination is reachable in principle: `is_fully_supported_shape` gates only on `RewriteMode`,
returning `true` unconditionally for `Iterative`, and `pattern_slots` has accepted bounded
quantifiers (building `Slot::Repeat`) since a separate, earlier change
(`compile-bounded-fst-quantifiers`). So the capability-check side's
`RightToLeftRewriteDetail::reversal_construction_attempted` — which re-runs `pattern_slots` — now
reports `true` for these rules and the FST-path predicate admits them at `ConfirmOnly`.

## Can it change a parse?
Potentially yes, but asymmetrically: `ConfirmOnly` protects against **over-generation** (a
confirmation step still checks every FST-proposed candidate against the real engine), not omission.
A missed RTL-preferred outcome under this bug would be **silent and unrecoverable** — the FST
proposer would simply never offer the correct candidate for confirmation to accept, and nothing
downstream would flag the gap, since the engine only ever sees what the proposer offers.

## Evidence
None yet — deliberately stated as structural, not measured.
`conformance-staging/edge-cases/right-to-left-bounded-quantifier-rewrite` (the existing fixture for
this general area) does **not** detect it: its quantifier wraps a single `<SimpleContext>`, so its
children list is trivially palindromic (length 1) and the shallow reverse is accidentally correct.

## Upstream
Not applicable — this concerns only Rust's own optimized FST-proposer path, not a C#-vs-Rust
divergence in the confirm-time parser itself.

## Notes
The first task of any fix is to author a multi-child-quantifier RTL fixture and confirm the miss
actually reproduces before changing `reversed_slots` — per `docs/hermitcrab-rust-port-audit.md` §3a,
where this was found: "the existing fixture... does not detect it... the first task of any fix is to
author a multi-child-quantifier RTL fixture and confirm the miss before changing `reversed_slots`."
Also note that `capability.rs:391-395`'s doc claiming the RTL-detection check "must avoid
`Quantifier`" is stale as a description of what `pattern_slots` currently does — the two changes
(RTL detection, bounded-quantifier acceptance) were each correct in isolation; the hole is only in
their interaction.
