# STAGING: filter-passes/partner-pairing

**Target pass:** `symbolic.partner.v1` -- **status:** `not-yet-provokable` -- **min_fire_count:** 0
(`filter-expectation.json` is the machine-readable form of that line)

## Why this directory has no grammar

`PartnerPairingPass` consumes only explicit `LocalEvent::PartnerOpen` / `LocalEvent::PartnerClose`
facts. Nothing an author can write in a `HermitCrabInput` grammar can cause those to be emitted,
because the two halves of a circumfix stop being two things long before any producer sees them.

Recorded as data rather than prose so a future reader does not read the absence as an oversight and
write a grammar that cannot work.

## What was checked, and what it showed

Verified against the code on the branch this fixture was authored on, not assumed:

- A circumfix reaches `pg-foma` as **one** `AffixAllomorphDef` with one flat `rhs`
  (`rust/crates/pg-grammar/src/model.rs`). The two halves are two `InsertSegments` entries whose
  only distinguishing property is their index relative to the `Copy`; there is no partner id, no
  half id, and no sibling linkage between allomorphs of a rule.
- `classify_affix` (`rust/crates/pg-foma/src/emit.rs`) assigns one whole-allomorph
  `Role::CircumfixPrefix` to that `rhs`. Its sibling `Role::CircumfixSuffix` is documented as never
  produced, so even the two-role vocabulary collapses to one used variant.
- The composite emitter pushes one morpheme tag with one surface per rule application, and
  `preexpand`'s candidate-rule filter admits only prefix/suffix/infix roles. `morphotactics.rs` and
  `structural_allomorph.rs` have no circumfix concept at all.
- `pg_parse` confirms the collapse end to end: `keadilan` (a `ke-...-an` circumfix) has the single
  signature `NMLZ+ADIL|keadilan`, one morph for both halves.

Two refinements worth recording, because the claim as usually stated is slightly wrong in each
direction:

1. "Compiles the halves into a single cross-product allomorph" is exact for HermitCrab-XML input,
   but the Rust FieldWorks compile path does not cross-product at all -- it **skips** a circumfix
   entry with a warning (`rust/crates/pg-grammar/src/compile/affixes.rs`). The halves lose identity
   either way, so the conclusion is unaffected, but by a different route.
2. The confirm engine does split a discontinuous morph: `pg-rules` emits one `MorphRecord` per
   contiguous run of a morph's output positions. Both runs carry the **same** allomorph and morpheme
   ids, there is no open/close marker, and nothing maps them into `LocalEvent`. That is a fact about
   future feasibility on the generator side, not about today.

There is also a broader reason this pass cannot fire yet, independent of circumfixes: no production
`CandidateFilterPass` exists, and `ProposalProducer::LegacyProposer` is never constructed anywhere.

## What would unblock it

A generator that emits a proof-verifiable `PartnerProvenanceCatalog` with stable partner-open and
partner-close events. At that point a circumfix grammar becomes authorable here: give it matched and
mismatched partner classes, a missing open half, a missing close half, and two independent pairs.
Set `status` to `awaiting-pass` in the same change, which is what the harness's anti-rot check keys
on.

## Verification

`rust/crates/pg-foma/tests/candidate_filter_fixture_weight.rs` asserts that a `not-yet-provokable`
fixture carries **no** `grammar.xml` -- an authored grammar here would mean the claim above had
lapsed -- and fails if `symbolic.partner.v1` ever appears among the built passes while this fixture
still says the construct cannot be provoked.

## Graduation

Nothing to graduate: there are no fixture files. This directory is a record, and it disappears into
a real fixture the moment the generator supplies partner provenance.
