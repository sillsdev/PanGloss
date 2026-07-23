## Decisions

Execution order and ownership follow `openspec/changes/STAGING.md`.

- Rust is the sole producer of canonical FST health facts and findings.
- Finding codes use `PGF` plus four decimal digits and never change meaning after publication.
- Each finding records code, severity, preflight/observed phase, metric, predicted/observed value,
  effective thresholds, grammar/rule/construct identifiers, concise explanation, and zero or more
  ranked remedy records with applicability conditions.
- Findings explain computational consequences only. They may suggest constraining or reordering a
  rule if linguistically equivalent, but never assert that such a change improves the grammar.
- Overall admission is the worst applicable severity after validating any explicit override.
- Error requires an explicit per-compilation override and remains recorded in reports and packages.
  Critical cannot be overridden. Warning and below publish normally.
- FST payload uses decimal byte bands: Ideal <=10,000,000; Info <=20,000,000; Warning
  <=100,000,000; Error <=500,000,000; Critical above 500,000,000.
- Size does not dominate other dimensions. Unknown/unbounded work, intermediate-net growth,
  compile work/time, candidates, paths, and application time have independent policies.
- Canonical JSON is the source artifact; Markdown is a rendering of the same findings.

## Ownership and verification

Owns a new Rust health schema/registry module and golden serialization tests. It does not own
`emit.rs`, `replace.rs`, budget counters, or measurement collection.

Run from `rust/`:

- `cargo test -p pg-foma fst_health_schema`
- `cargo test -p pg-foma fst_health_size_bands`
- `cargo test -p pg-foma fst_health_override_policy`
