# STAGING: filter-passes/allomorph-compatibility

**Target pass:** `local.allomorph.v1` -- **status:** `awaiting-pass` -- **min_fire_count:** 4
(`filter-expectation.json` is the machine-readable form of that line)

## Why this fixture exists

Pins **alternative exhaustion**: a root with two suppletive allomorphs, *both*
environment-restricted, with no unrestricted elsewhere form to catch the contexts neither covers. A
candidate naming this morpheme is impossible exactly when every one of its alternatives is
impossible, which is the property a sound allomorph pass has to implement and an unsound one
(rejecting at the first failing alternative) gets wrong.

It exists ahead of the pass it targets on purpose. A fixture pins this engine's own analyses for a construct, which is exactly the reference a filter pass must not perturb, and that reference is authorable before any pass is.

## What it pins

- `kapta` and `kapion` parse: in each, one alternative fits and the other does not. A pass that
  rejected on the first failing alternative would destroy these rows.
- `kapon`, `kapita` have no valid analysis: each is the crossed pairing, where the fitting-by-shape
  allomorph fails its environment and the other does not fit the surface at all.
- `kap`, `kapi` have no valid analysis: bare, with no following segment for either environment to
  match. Their counterpart `noris` parses, so these rows isolate the missing elsewhere allomorph
  rather than anything about bare roots.
- `noris`, `norista`, `norison` parse -- the contrast root, one unconstrained allomorph, in the same
  two suffix contexts, so a pass rejecting on the mere presence of several allomorphs or of an
  environment is caught here.

## Isolation

One part of speech, both suffixes on the correct side, no `AffixTemplate`, no
co-occurrence rules, no rule features, and **no `PhonologicalRule` or `MetathesisRule` at all** --
so `requires: []` and every negative row's pieces still tile the surface exactly, leaving nothing for
an exact-span check.

The `PhonologicalFeatureSystem` declares no phonology; it exists solely to give each segment a
unique symbol, without which a `SegmentNaturalClass` constraint degrades to "any segment" in this
port's bridge layer and the environment rows would pass for the wrong reason. Same reason and same
shape as `machine/conformance/edge-cases/free-fluctuating-allomorph-pair`.

Residual overlap: none identified.

## How min_fire_count was arrived at

**4.** One verified rejection each for `kapon`, `kapita`, `kap`, `kapi`. Every one
requires exhausting both alternatives rather than stopping at the first failure, which is exactly
the behaviour the floor is meant to certify.

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
