# `grammar_semantics_owner_gate` — the single-owner invariant

`rust/crates/pg-foma/tests/grammar_semantics_owner_gate.rs` pins the invariant that no migrated
consumer re-reads `Grammar` to decide something `pg_foma::grammar_semantics::GrammarSemantics`
already owns.

## Two shapes of evidence

1. **Call counting (the real gate).** `pg_foma::capability::characterize` is the expensive walk —
   it builds real `foma::types::Fsm` networks for `Simultaneous`-mode subrules — so it must be
   memoized rather than re-run per candidate or per consumer. `select_plan` must characterize the
   grammar once, not once per candidate plan; `preflight_findings` must characterize once, not
   twice (once for the profile, once for the capability verdict). Both assertions are the
   load-bearing tests: they fail if either call site regresses to walking per-candidate or
   duplicating the walk.
2. **The declared-vs-cascade phonology split.** `Applicability::HasPhonology` (the grammar-wide
   declaration) and `PhonologyProbe::new`'s existence gate (the per-stratum rewrite cascade) are
   different predicates that genuinely disagree on a fixture where a `<PhonologicalRule>` is
   declared but no stratum names it. Unifying them would change which recipe families a grammar is
   offered, so a test asserting they disagree is what catches that "simplification" before it ships.

## What is deliberately not claimed

Projection equalities like `Applicability::HasTemplates == declared_templates` are tautologies
given the implementation and would pass either way; they are not asserted here, because a test
that cannot fail is not evidence.

## Why one `#[test]` function for the call-counting evidence

`characterize_call_count` is thread-local, so it cannot be polluted by other test binaries or by
tests on other threads — but two `#[test]`s in the same file could still be scheduled on the same
thread by the harness's thread reuse. Keeping the counting work in one function makes the reading
unambiguous without depending on how the harness schedules tests.
