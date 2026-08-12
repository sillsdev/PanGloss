# conformance-staging: what lives here

This repo's own conformance fixtures, committed ahead of upstream acceptance. See
`docs/conformance-staging-plan.md` for the design and `.claude/skills/conformance-grammars/SKILL.md`
for the authoring, staging, and graduation workflow. Every fixture directory carries its own
`STAGING.md` recording why it exists, what it pins, and which oracle produced its signatures.

## Categories

| Directory | Walked by `pg_conformance_fixtures::discover` | Purpose |
|---|---|---|
| `edge-cases/` | yes | Narrow, single-construct probes and bug pins. |
| `languages/` | yes | Dense typologically-themed demo grammars (none staged here yet). |
| `filter-passes/` | **no** | One fixture per planned candidate-filter pass; see below. |

`discover` walks only `edge-cases` and `languages`, so `filter-passes/**` is **not** replayed by
`pg-parse`'s `conformance_fixtures_gate`. It is replayed instead by
`rust/crates/pg-foma/tests/candidate_filter_fixture_weight.rs`, which uses the same
`pg_conformance_fixtures` parsing and oracle-replay helpers over a `FixtureRef` it builds itself.

## `filter-passes/`: one fixture per candidate-filter pass

Every pass in the candidate-filter program must earn its place on evidence rather than on argument,
so each one gets a synthetic grammar that provokes it and, as far as the construct allows, provokes
none of its siblings. A fixture that would trip four passes cannot show that any one of them earns
its place.

Each directory adds a third file the upstream fixture contract does not define,
`filter-expectation.json`:

```json
{ "pass_id": "structural.ownership.v1", "min_fire_count": 2, "status": "awaiting-pass" }
```

`status` is `awaiting-pass`, `wired`, or `not-yet-provokable`, and it is what stops an unwired
fixture from reading as coverage: the harness fails loudly if a fixture is still waiting on a pass
that has since been built, if it names a pass nobody declared, or if a built pass has no fixture at
all. `min_fire_count` is a floor on the verified rejections that pass must produce over the
fixture's words once it exists; each fixture's `STAGING.md` records how the number was arrived at.

| Fixture | Target pass | Status | `min_fire_count` | Provoking construct |
|---|---|---|---|---|
| `ownership` | `structural.ownership.v1` | awaiting-pass | 2 | Prefix homophonous with a free root, plus surfaces made only of affix material |
| `structural-transition` | `structural.transition.v1` | awaiting-pass | 3 | Affix material on the wrong side of the root, both directions |
| `slot-order` | `symbolic.slot_order.v1` | awaiting-pass | 2 | One `AffixTemplate` with two ordered suffix slots, reversed |
| `co-occurrence` | `symbolic.co_occurrence.v1` | awaiting-pass | 4 | `MorphemeCoOccurrenceRule` exclusion and requirement |
| `static-signature` | `symbolic.static_signature.v1` | awaiting-pass | 4 | Category selection plus an `excludedMPRFeatures` exception class |
| `allomorph-compatibility` | `local.allomorph.v1` | awaiting-pass | 4 | A root whose every allomorph is environment-restricted, with no elsewhere form |
| `exact-span` | `local.exact_span.v1` | awaiting-pass | 4 | Phonology-free fixed-shape morphology, surfaces one segment off |
| `local-environment` | `local.environment.v1` | awaiting-pass | 4 | Nasal place assimilation in a one-segment right window |
| `partner-pairing` | `symbolic.partner.v1` | **not-yet-provokable** | 0 | None -- no authored grammar can emit partner events; see its `STAGING.md` |

All eight authored grammars are synthetic, use invented lexemes, and name no language in any file,
feature, or symbol; where one mimics a real language's pathology the pathology is named and the
family appears in a comment only.

Signatures throughout were transcribed verbatim from `pg_parse::Morpher` -- `pangloss` **is** the
oracle for these fixtures, not the C# founding oracle, and machine acceptance must re-verify.
