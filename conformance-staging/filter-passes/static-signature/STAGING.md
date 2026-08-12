# STAGING: filter-passes/static-signature

**Target pass:** `symbolic.static_signature.v1` -- **status:** `awaiting-pass` -- **min_fire_count:** 4
(`filter-expectation.json` is the machine-readable form of that line)

## Why this fixture exists

Pins **static POS/MPR signature conflict**, covering both halves of that pass. Two suffixes
each select a different part of speech, and one of them additionally refuses any stem carrying an
exception rule feature (`excludedMPRFeatures`). Both constraints are readable off the candidate's
morphemes without looking at the surface at all.

It exists ahead of the pass it targets on purpose. A fixture pins this engine's own analyses for a construct, which is exactly the reference a filter pass must not perturb, and that reference is authorable before any pass is.

## What it pins

- `vokadan` has no valid analysis: right category, wrong exception class. Its morphotactics are
  identical to the accepted `sanuan`, so the lexically declared rule feature is the only difference.
- `minoan`, `sanuki` have no valid analysis: category selection violated in each direction.
- `vokadki` has no valid analysis and carries two independent signature defects at once, pinning
  that a candidate must not need both checked to die.
- `sanuan`, `minoki` parse -- the rows both halves of the pass must never disturb.
- `vokad` parses bare, proving the rule feature blocks nothing by itself.

## Isolation

No `AffixTemplate` so no slot order, no co-occurrence rules (every negative row here
involves exactly one affix), no phonological rules, one unconstrained allomorph per entry, both
suffixes on the correct side of the root, fixed shapes tiling exactly.

Residual overlap: category selection is arguably also a morphotactic-transition fact. The separation
taken here is that `structural-transition` keeps **one** part of speech and violates only sidedness,
while this fixture keeps every affix on its correct side and violates only the signature -- so
neither fixture's negative rows are reachable by the other's pass.

## How min_fire_count was arrived at

**4.** One verified rejection each for `vokadan`, `minoan`, `sanuki`, `vokadki`. One
row is rule-feature-only and three are category-based, so a pass checking only rule features tops
out at 1 and one checking only categories tops out at 3; neither reaches the floor alone.

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
