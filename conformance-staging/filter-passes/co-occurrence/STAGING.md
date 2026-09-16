# STAGING: filter-passes/co-occurrence

**Target pass:** `symbolic.co_occurrence.v1` -- **status:** `awaiting-pass` -- **min_fire_count:** 2
(`filter-expectation.json` is the machine-readable form of that line)

## Why this fixture exists

Pins **static morpheme co-occurrence exclusion**. Three suffixes combine freely as far as the
morphotactics are concerned; one `MorphemeCoOccurrenceRule` constrains them -- PAST excludes FUT
anywhere in the word.

It exists ahead of the pass it targets on purpose. A fixture pins this engine's own analyses for a construct, which is exactly the reference a filter pass must not perturb, and that reference is authorable before any pass is.

## What it pins

- `tarosilu`, `tarolusi` (both tense markers) have no valid analysis -- the exclusion pin, in
  both orders, so the exclusion cannot be narrowed to adjacency.
- `tarosina`, `taronasi`, `tarona`, `minurna` all parse: EMPH attaches freely, since no
  requirement constrains it (see Conversion note below).
- `tarosi`, `tarolu`, `minursi`, `minurlu` parse, proving each suffix is individually attachable.

## Conversion (2026-09-16)

The `type="require"` rule (EMPH requires PAST) was removed: FieldWorks' HCLoader builds Exclude
constraints only (`LoadMorphemeCoOccurrenceRules`, HCLoader.cs:2213-2239, hardcodes
`ConstraintType.Exclude`; LibLCM's `MoMorphAdhocProhib` is prohibition-only), so a co-occurrence
requirement has no FieldWorks equivalent. The fixture is now exclude-only and
`fieldworks_producible: true`. `tarona`/`minurna` (EMPH without PAST) now parse as positive
controls instead of pinning a rejection; `min_fire_count` dropped from 4 to 2 accordingly.

## Isolation

One part of speech, no `AffixTemplate` so no slot order, no rule features, no
phonological rules, one unconstrained allomorph per entry, all three suffixes on the correct side of
the root, fixed shapes tiling exactly. The co-occurrence rules are the only constraints in the
grammar that any negative row violates.

Residual overlap: none identified.

## How min_fire_count was arrived at

**2.** One verified rejection each for `tarosilu`, `tarolusi`, the only two negative rows left
now that the fixture is exclude-only.

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

## Oracle provenance (reconciled 2026-09-16)

`rust/tools/oracle-conformance.ps1` ran `hc-conformance.exe` self-check (C# founding oracle, machine
commit `f150e2a005ce639f7d68ef17fb0db25b2f6aaa3c`) against the post-conversion `grammar.xml` +
`words.yaml`, materialized under a throwaway `edge-cases/<name>` mirror since `Fixture.DiscoverAll`
only scans `languages`/`edge-cases` (the real files here were never moved): PASS -- every word's
signature and traced `rules:` list matched. The "Oracle discipline" section above describes how this
fixture was originally authored, not its current verification status.

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
