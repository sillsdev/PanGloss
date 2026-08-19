# STAGING: filter-passes/surface-consistency

**Target pass:** `surface.consistency.v1` -- **status:** `producer-blocked` -- **min_fire_count:** 1 (declared; measures 0 today)
**blocked_reasons:** `producer-emits-only-hc-confirmed-analyses`
(`filter-expectation.json` is the machine-readable form of those lines)

## Why this fixture exists

Pins the ground truth a candidate's **literal surface characters** must be checked against. One
suffix rule (`mrZ`) inserts a literal segment ("z") that appears in no lexical entry in this
grammar, and is morphotactically legal for the one part of speech every entry shares:

- `koloz` is a real word: root `KOLO` plus `mrZ`, and the surface genuinely contains the "z" the
  rule inserts.
- `nilo` is a real word with no suffix. Its root `NILO` contains no "z" anywhere. A proposer that
  enumerates rule x root combinations without checking surface composition can still construct a
  candidate `(NILO, mrZ)` over this surface -- `mrZ` asks only for part of speech `posN`, which
  `NILO` has. That candidate is surface-infeasible: no combination of `NILO`'s and `Z`'s own
  literal characters can ever contain a "z" the surface does not have.

It exists ahead of the pass it targets on purpose, exactly as the `ownership` fixture's own
argument states: a fixture pins this engine's own analyses for a construct, which is exactly the
reference a filter pass must not perturb, and that reference is authorable before any pass is.

## What it pins

- `kolo`, `koloz` parse (one analysis each) -- the rows the pass must never disturb, and the one
  (`koloz`) carrying the suffix whose literal character the infeasible candidate borrows.
- `nilo` parses (one analysis, the bare root) -- the row whose surface is the infeasibility probe:
  it names no candidate that fails today, because `pg_parse::Morpher` only ever proposes what it
  can confirm, and a confirmed analysis is, by construction, surface-feasible. The floor this
  fixture declares is what an over-generating producer's `(NILO, mrZ)` candidate would meet, not
  something replayed through this harness.

## Why `producer-blocked`, not `wired`

`surface.consistency.v1` (`rust/crates/pg-foma/src/candidate_filter/passes/surface_consistency.rs`)
is a **sound under-approximation** by construction: it can under-detect but never wrongly reject a
real derivation. `candidate_filter_fixture_weight.rs`'s harness adapts `pg_parse::Morpher::parse_word`'s
own confirmed output into candidates -- it never presents an unconfirmed, over-generated candidate at
all. So a sound check applied only to already-confirmed candidates measures zero here for the same
reason `structural.ownership.v1`'s own fixture does: the producer, not the pass, is what blocks it.

The real, over-generating producer this pass is built for is the FST/enumeration proposer measured in
`docs/superpowers/plans/2026-08-11-candidate-filter-first.md`'s "Surface consistency is measured"
section, against this repo's own private, gitignored corpora -- not reproducible here, since this
fixture (like every conformance-staging fixture) is synthetic-only per this repo's own hard rule. The
measured private-corpus numbers there are the real-world counterpart to the single synthetic
`(NILO, mrZ)` case this fixture pins.
