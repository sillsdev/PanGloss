# 013 — Boundary node counted as a spurious epenthesis site

## Kind
Behavioural.

## Status
Fixed-in-rust.

## C# site
`SynthesisRewriteRuleSpec.cs:26-29`: an empty-LHS (epenthesis) pattern is a single `Segment`-or-
`Anchor` constraint — **never** `Boundary`. A word-internal morpheme boundary is only ever traversed
*transparently* within an environment check; it is never itself a valid match position.

## Rust site
`pg_rules::rewrite::syn_epenthesis` (`rust/crates/pg-rules/src/rewrite.rs`)'s site-enumeration loop.

## What differs
Rust's `node_of` deliberately includes boundary entries (needed so environment checks can see
through them), but before the fix, the site-enumeration loop iterated **every** `node_of` entry
including boundary ones — double-counting a root's internal boundary and manufacturing a second,
C#-nonexistent epenthesis site immediately adjacent to it.

This was one of three findings behind one symptom (root "19"'s epenthesis-adjacent-to-internal-
boundary sub-cases (2)/(5) in `epenthesis_rules`); the other two are notable for being explicitly
**not bugs**:
1. `pg_rules::bridge::PatternBridge::nat_class_lanes`'s `NaturalClassKind::Feature` arm never pinned
   the synthetic `Type` lane the way C#'s `NaturalClass` ctor unconditionally stamps it
   (`NaturalClass.cs:9-13`) — real, but not decisive on its own for this symptom. Fixed by pinning
   `lanes[type_flat] = TYPE_SEGMENT_BITS` there too.
2. `pg_fst::traverse::Transduce::initialize`'s `start_anchor && optional` skip-arm was investigated
   as a suspected over-reach and found to faithfully port C#'s `TraversalMethodBase.Initialize`
   (`TraversalMethodBase.cs:203-222`), which has the identical "an anchored match may transparently
   skip a leading Optional annotation" behavior. Confirmed deliberate and shared in both engines, not
   a Rust-only bug.

## Can it change a parse?
Yes for the decisive mechanism (boundary-as-site): it manufactures an epenthesis application C#
never performs, at a position adjacent to an internal morpheme boundary.

## Evidence
Sub-case (7) of the same test (`"biiibuii" -> "18"`) turned out to be an unrelated fixture bug (the
shared lexicon's root "18" entry stored the wrong shape, confirmed against
`HermitCrabTestBase.cs:565`) — fixed at the fixture, not the engine. The decisive engine bug (boundary-
as-site) is fixed by skipping `NodeKind::Boundary` entries in `syn_epenthesis`'s site loop.

## Upstream
None, not applicable — pure Rust-side bug; C#'s empty-LHS pattern already excludes `Boundary`.

## Notes
A ninth sub-case of this same test, believed passing individually at the time, turned out to be the
separate divergence in entry 014 (iterative epenthesis cascading) — split out precisely so this
entry's fix could ship without waiting on that unresolved, structurally larger one.
