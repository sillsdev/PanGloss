# Wire schemas

The five artifact schemas required by the handoff spec §17.3, plus the shared definitions it also
names ("typed failures, diagnostics references, ... batch outcomes, and per-case outcomes").

The prose in `docs/grammar-assessment-handoff-spec.md` defines the semantics; these files close the
mechanics — required fields, optional fields, enums, nullability, and size bounds.

## Why they are checked in rather than generated

A schema derived from the Rust types would restate whatever the code happens to do, so it could
never disagree with the emitter and would be worthless as a check. These are written independently
and validated *against real emitted artifacts* in `tests/schema_conformance.rs`, so drift between
the wire contract and the code fails the build in whichever direction it occurs. That test is the
reason to trust them; without it a checked-in schema is documentation that silently rots.

## What validates them

`tests/schema_conformance.rs` carries a small validator covering exactly the JSON Schema subset
used here — `type`, `required`, `properties`, `additionalProperties`, `enum`, `const`, `items`,
`$ref` to `#/$defs/*`, `oneOf`, `minimum`, `minItems`, `maxLength`, and `nullable` via
`type: [..., "null"]`. It is deliberately **not** a general JSON Schema implementation: a full one
is a dependency this repo has not taken, and pretending to be one would be worse than declaring the
subset. Anything outside the subset is a hard error in the validator rather than a silent pass, so
a schema cannot quietly grow a construct nothing checks.

## Fixtures

- `fixtures/valid/*.json` — canonical positive fixtures; each must validate against its schema.
- `fixtures/invalid/*.json` — negative fixtures; each must be **rejected**, and the test asserts the
  rejection names the field at fault, so a fixture cannot pass the negative test for the wrong
  reason.
