# STAGING: filter-passes/ownership

**Target pass:** `structural.ownership.v1` -- **status:** `producer-blocked` -- **min_fire_count:** 2 (declared; measures 0 today)
**blocked_reasons:** `producer-emits-only-hc-confirmed-analyses`
(`filter-expectation.json` is the machine-readable form of those lines)

## Why this fixture exists

Pins the ground truth a candidate's **root designation** must be checked against. The
grammar makes a derivational prefix and a free root homophonous (`pa-` and the root `pa`), and adds
a second prefix (`ta-`) homophonous with nothing, so two shapes of ownership defect are expressible
over real surfaces:

- `papa` is a real word whose morpheme sequence is `(PA, ROOTPA)`. Exactly one of the two positions
  is owned by a lexical entry, so a proposer that enumerates root positions over the decoded
  sequence can designate index 0 -- an affix -- as the root. That candidate is impossible on
  ownership alone.
- `ta` and `pata` are surfaces built entirely from affix material. Any candidate over them names
  only rule-owned morphemes, so no position it could designate is owned by a lexical entry at all.

It exists ahead of the pass it targets on purpose. A fixture pins this engine's own analyses for a construct, which is exactly the reference a filter pass must not perturb, and that reference is authorable before any pass is.

## What it pins

- `papa`, `tapa`, `pa` parse (one analysis each) -- the rows the pass must never disturb, and the
  ones carrying the invalid alternative root designation.
- `ta`, `pata` have no valid analysis -- `expect_fail: true`. These are the cleanest ownership
  negatives in the fixture: nothing else in the grammar objects to them.
- `kolo`, `nilo`, `pakolo`, `takolo`, `panilo` are unambiguous positive controls.

## Isolation

One part of speech, so no signature conflict is expressible. No `AffixTemplate`, so no
slot order exists. No `MorphemeCoOccurrenceRule`s. No `MorphologicalPhonologicalRuleFeature`s. No
`PhonologicalRule`s, so `requires: []` and the surface is a literal concatenation -- every negative
row's pieces tile exactly, leaving nothing for an exact-span check. One unconstrained allomorph per
entry, so no allomorph-compatibility question arises. Both affixes sit on their own correct side of
the root, so no sidedness transition is violated.

Residual overlap: none identified. The homophony construct is what makes ownership answerable
independently of everything else.

## How min_fire_count was arrived at

**2, declared and held; it measures 0 today**, which is what `producer-blocked` records. The
declared number is not lowered to match the measurement, because the measurement is a fact about
this harness's producer rather than about the pass or the grammar.

Measured over this fixture's words with the pass built and enforced: 8 witnesses evaluated, 8 kept,
0 deferred, 0 rejected. Every witness is pin-resolvable, so `OwnershipPass` answers `Keep` at its
first branch and never reaches a rejection. The floor was not wrong about the grammar -- it was wrong about
what the harness proposes. `ta` and `pata` are `expect_fail` rows, the harness proposes only the
analyses `pg_parse::Morpher` accepted, and a word with no accepted analysis contributes no
candidate at all. The impossible candidates the floor counts are exactly the ones nothing here
emits. Raising this floor needs a producer that enumerates candidates HC refuses; no change to this
grammar can do it.

**The floor, as authored: 2.** One verified rejection each for `ta` and `pata`, the two words with no valid
analysis whose every candidate designates a rule-owned morpheme as root. Deliberately a floor rather
than an estimate of the total: `papa`, `tapa`, and `pa` each additionally carry an invalid root
designation over a sequence that also has a valid one, so the true count once a proposer emits those
alternatives should be higher. Counting only the rows that cannot be right at all keeps the number
defensible without knowing how many root positions the future generator enumerates.

That number is the floor to enforce for the run *after* a producer supplies both the candidates and
the facts the pass needs. Until then the harness holds this fixture to the opposite claim -- that it
still reaches **zero** -- so the first rejection it ever reaches fails the suite and forces promotion
to `wired`. `blocked_reasons` names `producer-emits-only-hc-confirmed-analyses`, which is the whole
of what is missing here: the adapter's deferred facts never bind, because `OwnershipPass` answers
`Keep` on pin-resolvability before reading one.

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

## Oracle provenance (reconciled 2026-09-02)

`rust/tools/oracle-conformance.ps1` ran `hc-conformance.exe` self-check (C# founding oracle, machine
commit `caa4ddde8782557c6fb58cac57e4761ffcafc2a6`) against this fixture's `grammar.xml` + `words.yaml`,
materialized under a throwaway `edge-cases/<name>` mirror since `Fixture.DiscoverAll` only scans
`languages`/`edge-cases` (the real files here were never moved): PASS -- every word's signature and
traced `rules:` list matched. `words.yaml` now carries `# oracle-provenance: founding-oracle`. The
"Oracle discipline" section above describes how this fixture was originally authored, not its current
verification status.

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
