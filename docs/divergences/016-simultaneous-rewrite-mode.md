# 016 — `RewriteMode::Simultaneous` real semantics (previously unsupported)

## Kind
Behavioural (now closed — kept as a record of a capability gap that was fixed).

## Status
Fixed-in-rust.

## C# site
`SimultaneousPhonologicalPatternRule.Apply`: collects every accepted match against **one pristine
snapshot** before applying any of them.

## Rust site
`pg_rules::rewrite::sim_feature` (`rust/crates/pg-rules/src/rewrite.rs`); dispatch happens in
`synthesize_with_mpr`/`synthesize_with_mpr_cached` on `(classify(rule, sr), rule.mode)`.

## What differs
`RewriteRuleTests.MultipleApplicationRules`'s point is that `Simultaneous` and `Iterative` produce
*different* results on the same rule over overlapping-match input. `RewriteMode::Simultaneous` used
to be parsed but silently executed identically to `Iterative` (a real behavioural divergence at the
time), and was later changed to hard-fail at grammar-load time instead (safer — a loud refusal rather
than a silent wrong answer, but still not a capability). Both gaps are now closed:
`RewriteMode::Simultaneous` loads and has real synthesis semantics — collecting every accepted match
against one pristine snapshot before applying any of them, mirroring C# exactly, instead of
`syn_feature`'s find-one-then-rescan Iterative shape (see entry 014 for where the *lack* of that
distinction still costs a real case, in `Iterative` epenthesis specifically).

## Can it change a parse?
Yes, when it was silently aliased to `Iterative` — that was a genuine, silent behavioural divergence
for any grammar using `Simultaneous` mode over overlapping matches. Not reachable today: the mode now
has real, distinct semantics.

## Evidence
Oracle-verified via the `rewrite/simultaneous-feeding`/`rewrite/simultaneous-feeding-control-iterative`
conformance fixtures (both oracle-diffed, not HC-Rust-only).

## Upstream
None, not applicable — closed capability gap; C# was already correct.

## Notes
`docs/hermitcrab-rust-port-audit.md`'s construct checklist records this as "Ported (P13) — previously
hard-linted as unsupported; now fully implemented with synthetic oracle fixtures, since no real
reference grammar exercises it" — worth noting that none of Indonesian/Amharic/Sena actually uses
`Simultaneous` mode, so this entry's correctness rests entirely on the synthetic fixtures, not on any
real-language corpus regression.
