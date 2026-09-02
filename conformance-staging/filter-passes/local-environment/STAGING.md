# STAGING: filter-passes/local-environment

**Target pass:** `local.environment.v1` -- **status:** `awaiting-pass` -- **min_fire_count:** 4
(`filter-expectation.json` is the machine-readable form of that line)

## Why this fixture exists

Pins a **certified local rewrite environment**: nasal place assimilation across a prefix
boundary. The prefix ends in an underspecified nasal placeholder rewritten to a bilabial or an
alveolar nasal according to the single segment immediately to its right. The conditioning window is
one segment wide, adjacent, and on the surface -- exactly the shape a local-environment check can
certify and reject on.

It exists ahead of the pass it targets on purpose. A fixture pins this engine's own analyses for a construct, which is exactly the reference a filter pass must not perturb, and that reference is authorable before any pass is.

## What it pins

- `menbalo`, `menpaku` (alveolar nasal before a bilabial stop) and `memtilo`, `memduka`
  (bilabial nasal before an alveolar stop) have no valid analysis. Both rewrite rules are covered,
  so a pass certifying only one of them would leave half the rows alive.
- `membalo`, `mempaku`, `mentilo`, `menduka` parse -- each negative row's exact minimal pair, the
  rows the pass must never disturb.
- `balo`, `paku`, `tilo`, `duka` parse bare, with no prefix and so no nasal to place.

## Isolation

Each negative row is the same length as its positive counterpart, built from the same
morphemes in the same order on the same side of the root, with the same category and the same single
unconstrained allomorph per entry. Span arithmetic, ordering, ownership, signature, and
co-occurrence therefore all have nothing to say about them; the one-segment window is the only
difference.

The five vowels each carry a distinguishing `high`/`back` value. Without them all five share one
feature bundle and this engine renders an indistinguishable segment as an ambiguity class -- the
first draft's signatures read `BALO|b[aeiou]l[aeiou]` rather than `BALO|balo`, which would have left
the surface half-pinned.

Residual overlap: none identified.

## How min_fire_count was arrived at

**4.** One verified rejection each for `menbalo`, `menpaku`, `memtilo`, `memduka` --
two per rewrite rule, so a pass certifying only one rule tops out at 2.

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
`languages`/`edge-cases` (the real files here were never moved). Every word's SIGNATURE matched on the
first run -- no correctness divergence between HC-Rust and the founding oracle. The oracle's traced
`rules:` for `membalo`/`mempaku`/`mentilo`/`menduka` additionally named the phonological
place-assimilation rule (`prNasalBilabial`/`prNasalAlveolar`) alongside `mrNasalPfx`, which
`words.yaml`'s `rules:` lists had omitted; `words.yaml` is corrected to the oracle's traced set (an
HC-Rust-side authoring gap this reconciliation surfaced, not a signature divergence -- the Rust
conformance suite never compares `rules:` at all, see `pg_conformance_fixtures::assert_matches_oracle`).
After that correction: PASS, every word's signature and traced `rules:` list match. `words.yaml` now
carries `# oracle-provenance: founding-oracle`. The "Oracle discipline" section above describes how
this fixture was originally authored, not its current verification status.

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
