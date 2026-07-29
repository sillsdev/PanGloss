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

The positive and negative fixtures §17.3 requires are **constructed in
`tests/schema_conformance.rs`**, not checked in as `.json` files.

That is deliberate, and it is the stronger arrangement. A positive fixture stored as a static file
proves only that the file matches the schema — it says nothing about whether the *emitter* still
does, so schema and code can drift apart while the fixture stays green forever. Here every positive
fixture is an artifact the real emitter produced during the test (`full_report()`, `compare(...)`,
`golden_diff(...)`, `investigate(...)`), so the schema is checked against the code on every run and
drift fails the build whichever side moved.

Negative fixtures are built by taking a real artifact and corrupting one field, then asserting it is
rejected **at that field** — `assert_rejected` matches the failure path, so a negative fixture
cannot pass for an unrelated reason and silently stop testing what it names.

The trade-off is that these fixtures are not available to consumers as sample files. If that is ever
wanted, generate them from the same emitter calls rather than hand-writing them, or they will rot in
exactly the way this arrangement avoids.

## Trace references (handoff spec §17.3) are a deliberate non-goal, not a missing schema

§17.3 lists "trace references" among the shared definitions these schemas should close, alongside
typed failures, diagnostics, batch outcomes, and per-case outcomes. There is no `--trace <trace.json>`
flag on any command and no schema field distinct from `investigation-handoff`'s `evidence` object
represents a stored trace artifact — see design.md D15 for the full reasoning. In short: `investigate`
supplies binding plus a pruned failure narrative rather than competing with FieldWorks on trace
presentation (D9/D10), and the FST-propose stage of `foma-confirm` — the default pipeline — has no
trace facility to reference in the first place. A future change adding a persisted trace artifact
would need a `--trace` output path, trace support on FST-propose, and a staleness story for the
persisted trace file distinct from `EvidenceAvailability`.
