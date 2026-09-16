# 024 — Analysis memo key saturates unapplication counts; C# does not

## Kind
Representational (self-described in the source as a "deliberate divergence," argued behaviorally
inert).

## Status
Superseded — memoization is being removed from HC-Rust entirely (branch feat/remove-memoization), so the memo-side mechanism this entry compares no longer exists as a target; kept for history per the ledger's never-delete rule.

Former status: Open — argued sound, actively guarded by a trip-wire test, not proposed upstream (nothing to propose: this is a Rust-only performance choice with no observable target in C#).

## C# site
`AnalysisStateKey.cs:14-34` — keeps the **full**, unsaturated per-rule unapplication count in the
state key.

## Rust site
`pg_rules::stratum::state_key` (`rust/crates/pg-rules/src/stratum.rs`):

```rust
/// The order-independent memo key for `w`, with each rule's count saturated at its `max_apps` --
/// the only reader compares `count >= max_apps`, so counts above it are behaviorally identical
/// (C# does not saturate, `AnalysisStateKey.cs:14-34` -- deliberate divergence).
fn state_key(&self, w: &Word) -> AnalysisStateKey { ... }
```

## What differs
Rust caps each rule's count in the memo key at that rule's own `max_apps` before hashing/comparing;
C# stores the raw, unbounded count. The argument for soundness: `apply_one_mrule`'s admission gate
only ever asks `count >= max_apps` — never the exact value — so two states differing only in "how far
past `max_apps` a rule's count has climbed" are indistinguishable to every actual reader, and merging
them in the memo key cannot merge two states that would behave differently downstream.

**Precise rule each side follows:** C# treats `(rule, count=5)` and `(rule, count=7)` as different
memo keys even if `max_apps = 3` for that rule, potentially missing a cache hit an equivalent-state
argument would allow. Rust treats both as `(rule, count=3)` (saturated), deliberately over-merging
states that are provably equivalent for every consumer that exists today.

## Can it change a parse?
Argued no, conditional on exactly one invariant: `apply_one_mrule`'s `>= max_apps` gate must remain
the field's **only** consuming reader. If a future reader ever needed the exact count past
saturation (e.g. a trace/diagnostic feature reporting "this rule fired N times"), the saturation
would then discard information a real behavior depends on, and the divergence would become
behavioural.

## Evidence
`pg-rules/tests/memo_gate.rs::state_key_saturates_unapplication_counts_past_max_apps` pins the
saturation behavior directly: two words populate one shared memo scope and are asserted to hit the
*same* saturated-key entry. `pg-rules/tests/unapplied_rule_counts_reader_gate.rs` is a dedicated
"reader-audit trip-wire" specifically for the soundness invariant above — its own doc states its
purpose as auditing, by hand, whether a new reader of the unsaturated count has appeared, since that
would silently invalidate the saturation's soundness argument.

## Upstream
Not applicable — a Rust-only performance optimization; C#'s unsaturated key is not "wrong," it is
simply a design C# had no reason to adopt (C# does not need the same order-invariant memoization
scheme Rust ported and then optimized on top of, per `docs/history/rust-conversion.md` §1.2's
description of the underlying #451 memoization design).

## Notes
This is a textbook example of this repo's own "build the differential measurement before the change"
rule (CLAUDE.md): the trip-wire test exists specifically so a *future* change (a new reader of the
unsaturated field) cannot silently invalidate this divergence's soundness argument without being
caught.
