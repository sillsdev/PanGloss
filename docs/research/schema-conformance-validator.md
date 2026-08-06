# Why `schema_conformance.rs` hand-writes its own JSON Schema subset

`pg-assess/tests/schema_conformance.rs` checks the wire schemas in `pg-assess/schemas/` against
artifacts the real emitters produce. A schema *derived* from the Rust types could never disagree
with the emitter — it would just restate the code — so it would be worthless as a conformance
check. These schemas are written independently, and the test validates them against artifacts the
code actually produces, so drift in either direction fails the build.

## A declared subset, not a JSON Schema implementation

The validator covers exactly what these schemas use: `type` (including `[..., "null"]`),
`required`, `properties`, `additionalProperties`, `enum`, `const`, `items`, `oneOf`, `$ref` to
`#/$defs/*`, `minimum`, `minLength`, `maxLength`, `minItems`, `maxItems`, and `pattern` (an anchored
literal-class subset — currently only `^sha256:[0-9a-f]{64}$`). Anything outside that set is a hard
error, never a silent pass: without that, a schema could grow a construct the validator does not
check and still look green, which would defeat the whole point of an independent check.

Pulling a real JSON Schema crate was the alternative and was rejected: this repo has not taken that
dependency, and inventing a general-purpose validator by hand would be worse than declaring a small,
honest subset and erroring loudly the moment a schema needs more of the spec than that subset
covers.
