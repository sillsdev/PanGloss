# STAGING: filter-passes/structural-transition

**Target pass:** `structural.transition.v1` -- **status:** `awaiting-pass` -- **min_fire_count:** 3
(`filter-expectation.json` is the machine-readable form of that line)

## Why this fixture exists

Pins **affix sidedness** as a morphotactic-automaton fact. One prefix and one suffix over a
single category: every legal word puts prefix material strictly left of the root and suffix material
strictly right of it, so a morpheme sequence placing a prefix-rule morpheme after the root, or a
suffix-rule morpheme before it, is unreachable however it is otherwise ordered, categorised, or
conditioned.

It exists ahead of the pass it targets on purpose. A fixture pins this engine's own analyses for a construct, which is exactly the reference a filter pass must not perturb, and that reference is authorable before any pass is.

## What it pins

- `kolurta`, `rekata` (prefix material right of the root) and `mukolur` (suffix material left of
  it) have no valid analysis -- `expect_fail: true`. Both directions are covered, so a pass that
  knew only one side of the automaton would still fail here.
- `takolur`, `tareka`, `kolurmu`, `rekamu`, `takolurmu` parse -- the positive controls, including
  the both-affixes row the pass must never disturb.

## Isolation

No `AffixTemplate` at all, which is the deliberate separation from the sibling
`slot-order` fixture: there are no slots here to order, and no sidedness violation there. One part
of speech, no co-occurrence rules, no rule features, no phonological rules, one unconstrained
allomorph per entry, fixed-shape affixes tiling exactly.

The Stratum is `morphologicalRuleOrder="linear"`. Under `unordered` this engine recursively
interleaves free-standing rules and the two derivation orders of `takolurmu` serialize to the same
text, which was measured producing a duplicated identical signature for that row.

Residual overlap: a `MorphotacticIndex` rich enough to model template slots would also call
`slot-order`'s reversed row a forbidden transition. The two fixtures are separated by construct
rather than by outcome -- sidedness/reachability here, intra-template slot sequence there -- which
is the sharpest separation the grammar model supports.

## How min_fire_count was arrived at

**3.** One verified rejection each for `kolurta`, `rekata`, and `mukolur`, the three
words with no valid analysis. Each has exactly one plausible morpheme sequence, so one rejection per
row is both the floor and the expected count.

It is a floor for the run *after* the pass exists and a producer supplies the facts it needs, not a prediction about today. With the current legacy adapter every allomorph, role, slot, stratum, span, and local-event fact is `Deferred`, so most of these rows would defer rather than reject; that is why the fixture is `awaiting-pass` and the harness asserts no fire count for it yet.

## Oracle discipline

**Oracle: `pangloss` (this repo's own Rust engine), NOT the C# founding oracle.** Every signature in
`words.yaml` was captured by driving `pg_parse::Morpher::parse_word` directly over every word in the
list (a throwaway in-repo test, deleted once transcription was done) and transcribed verbatim.
Nothing was hand-derived. Per `docs/conformance-staging-plan.md`'s oracle-discipline note this is an
accepted staging-time substitute; **machine acceptance must re-verify against
`SIL.Machine.Morphology.HermitCrab.Tool`**, and any divergence found there is itself a finding.

`grammar.xml` is well-formed XML (verified with a strict parser), so unlike most of the fixtures
under `conformance-staging/edge-cases/` it can actually be loaded by the C# oracle and is
graduation-ready on that axis.

## Verification

Replayed in `rust/crates/pg-foma/tests/candidate_filter_fixture_weight.rs`, which walks
`conformance-staging/filter-passes/**`, replays every word against `pg_parse::Morpher` through
`pg_conformance_fixtures::assert_matches_oracle`, and compares `FilterMode::Off` against
`FilterMode::Enforce` over proposals adapted from the resulting analyses. Note that
`pg_conformance_fixtures::discover` walks only the `edge-cases` and `languages` categories, so this
fixture is NOT picked up by `pg-parse`'s `conformance_fixtures_gate`; the harness named above is the
one that runs it.

## Graduation

Not yet proposed upstream (no `sillsdev/machine` PR opened). This fixture's directory carries a
third file (`filter-expectation.json`) that the upstream fixture contract does not define, so
graduation means contributing `grammar.xml` + `words.yaml` under
`machine/conformance/edge-cases/<name>/` and leaving the expectation file behind here.
