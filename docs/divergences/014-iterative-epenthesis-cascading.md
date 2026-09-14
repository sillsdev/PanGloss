# 014 — Iterative epenthesis cascading is unimplemented

## Kind
Behavioural.

## Status
Open. This is the most significant *currently unfixed, known* behavioural bug in the phonological
rule engine.

## C# site
`IterativePhonologicalPatternRule`, whose real semantics find one match, apply it (mutating the live
`Word`), and only then look for the next match against the partially-rewritten shape — a true
iterative cursor that never re-visits a position it has already advanced past.

## Rust site
`pg_rules::rewrite::syn_epenthesis` (`rust/crates/pg-rules/src/rewrite.rs`).

## What differs
`syn_epenthesis` collects every candidate site against **one unmutated snapshot** of the shape and
splices all accepted sites in unconditionally, regardless of the rule's declared `RewriteMode` — it
is structurally `Simultaneous`-shaped even when a rule asks for `Iterative` semantics.

Concretely, on `RewriteRuleTests.EpenthesisRules`' last reconfiguration (two bare `Iterative`-mode
rules composed in one stratum): `m.parse_word("butubu")` returns empty against the C# oracle's
`{"25"}` (re-verified directly against `dotnet test`). Root cause: on root 25's shape, `rule1` alone
produces two insertions correctly, but `rule2`'s `[V]_[V]` environment then finds **three** separate
V-V adjacencies in the resulting 7-segment intermediate shape (each of `rule1`'s freshly-inserted
vowel nodes creates a new adjacent pair) and inserts at all three, producing a shape that no longer
matches the expected surface — where C#'s true iterative cursor, which never re-visits an
already-advanced-past position, would only accept a subset.

**Precise rule each side follows:** C# applies one match, then re-scans the mutated shape for the
next one, for as many iterations as matches remain (each new match reflects earlier insertions). Rust
computes all matches against the original shape once and applies all of them at once — equivalent to
C#'s `Simultaneous` semantics, not `Iterative`.

## Can it change a parse?
Yes, and does today: the cited case returns empty where C# returns `{"25"}` — this is a straight
recall loss for any grammar with two or more cascading `Iterative` epenthesis rules in one stratum.

## Evidence
`csharp_port_rewrite.rs::epenthesis_rules_iterative_cascade_finding` is deliberately `#[ignore]`d,
split out from the rest of `epenthesis_rules` (entries 011/012/013) precisely so those fixes could
ship without regressing on this unresolved case. `docs/hermitcrab-rust-port-audit.md` §3a records
this as item 6 of the original squash-copy gap list, root cause "narrowed to two candidate
mechanisms" at the time, now narrowed further and stated definitively above.
`docs/hermitcrab-rust-port-audit.md` also separately flags `syn_epenthesis` as "structurally
Simultaneous-shaped regardless of a rule's declared Iterative mode" — the same root cause described
here from a different discovery path (two cascading Iterative epenthesis rules over-fire relative to
C#'s true cursor walk).

## Upstream
None, not applicable — this is a genuine Rust-side capability gap; C#'s iterative cursor is correct
and is the target to converge on.

## Notes
Deliberately not fixed at time of discovery: making `syn_epenthesis` faithfully iterative is
described in its own source as "a substantially larger, separate rewrite of the epenthesis synthesis
path that every other epenthesis reconfiguration in this file depends on," with real risk of
regressing the sub-cases that do pass (entries 011-013). Anyone picking this up should budget for a
rewrite of the synthesis-side epenthesis site-collection loop from "collect all, splice all" to
"find one, apply, re-scan," not a local patch.
