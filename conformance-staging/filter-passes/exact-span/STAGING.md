# STAGING: filter-passes/exact-span

**Target pass:** `local.exact_span.v1` -- **status:** `awaiting-pass` -- **min_fire_count:** 4
(`filter-expectation.json` is the machine-readable form of that line)

## Why this fixture exists

Pins **exact surface-span tiling**. Every morpheme has one fixed-shape allomorph and the
grammar declares no phonological or metathesis rule at all, so a word's surface is the literal
concatenation of its morphemes' shapes. That makes each span exact and knowable in advance, and
makes a candidate whose morphemes cannot tile the surface impossible on arithmetic alone -- before
any category, order, or environment is consulted.

It exists ahead of the pass it targets on purpose. A fixture pins this engine's own analyses for a construct, which is exactly the reference a filter pass must not perturb, and that reference is authorable before any pass is.

## What it pins

- `matinolu` (a segment inserted in the interior) and `matnlu` (a segment deleted) have no valid
  analysis. The two are opposite failures -- uncovered surface versus overrun pieces -- so a pass
  detecting only one of them cannot cover both rows.
- `matinlua`, `makesaluu` have no valid analysis: an extra segment past the end rather than in the
  interior.
- `matinlu`, `makesalu` parse -- the exactly-tiling rows every negative row is a one-segment
  perturbation of, and the ones the pass must never disturb.
- `tin`, `kesa`, `matin`, `tinlu`, `makesa`, `kesalu` are shorter positive controls at two different
  root lengths, so span arithmetic is not pinned to one size.

## Isolation

The root inventory is chosen so no root is a substring extension of another; a longer
near-homograph root would absorb the very segment the negative rows leave uncovered and the fixture
would be vacuous. One part of speech, one prefix and one suffix each on its own correct side, no
`AffixTemplate`, no co-occurrence rules, no rule features, one unconstrained allomorph per entry.

The Stratum is `morphologicalRuleOrder="linear"` for the same measured reason as
`structural-transition`: under `unordered` the two derivation orders of `matinlu` serialize to the
same text and duplicated that row's signature.

Residual overlap: none identified. The absence of phonology is what makes spans certifiable, and it
is also what leaves every other pass with nothing to say about these rows.

## How min_fire_count was arrived at

**4.** One verified rejection each for `matinolu`, `matnlu`, `matinlua`,
`makesaluu`. Two are uncovered-surface failures and two are overrun failures, so a pass detecting
only one direction tops out at 2.

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
