# STAGING: filter-passes/co-occurrence

**Target pass:** `symbolic.co_occurrence.v1` -- **status:** `awaiting-pass` -- **min_fire_count:** 4
(`filter-expectation.json` is the machine-readable form of that line)

## Why this fixture exists

Pins **static morpheme co-occurrence**, both directions. Three suffixes combine freely as
far as the morphotactics are concerned; two `MorphemeCoOccurrenceRule`s constrain them -- PAST
excludes FUT anywhere in the word, and EMPH requires PAST anywhere in the word.

It exists ahead of the pass it targets on purpose. A fixture pins this engine's own analyses for a construct, which is exactly the reference a filter pass must not perturb, and that reference is authorable before any pass is.

## What it pins

- `tarosilu`, `tarolusi` (both tense markers) have no valid analysis -- the exclusion pin, in
  both orders, so the exclusion cannot be narrowed to adjacency.
- `tarona`, `minurna` (emphatic with its required partner absent) have no valid analysis -- the
  requirement pin. A pass implementing only the exclusion kind would leave these alive.
- `tarosina` and `taronasi` both parse: the requirement is declared `adjacency="anywhere"`, so this
  pair also pins that the pass must not narrow it to adjacency.
- `tarosi`, `tarolu`, `minursi`, `minurlu` parse, proving each suffix is individually attachable.

## Isolation

One part of speech, no `AffixTemplate` so no slot order, no rule features, no
phonological rules, one unconstrained allomorph per entry, all three suffixes on the correct side of
the root, fixed shapes tiling exactly. The co-occurrence rules are the only constraints in the
grammar that any negative row violates.

Residual overlap: none identified.

## How min_fire_count was arrived at

**4.** One verified rejection each for `tarosilu`, `tarolusi`, `tarona`, `minurna`.
Two rows per rule kind, so a pass that implemented exclusion but not requirement (or the reverse)
tops out at 2 and cannot reach the floor.

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
