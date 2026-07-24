## Decisions

- Depend on merged bounded-quantifier work so `replace.rs` has one owner at a time.
- Use the frozen `MetathesisRule` switch identities and HermitCrab behavior as the oracle.
- Build a dedicated swap relation; do not encode metathesis as iterative ordinary replacement.
- Boundary movement, direction, environments, feature classes, and multiple tables receive separate
  witnesses and budget accounting.

## Ownership and verification

Exclusive files: metathesis regions of `pg-foma/src/replace.rs`, `gate.rs`, shared `lower.rs`, and
`pg-foma/tests/phase_c_metathesis.rs`. `pg-rules/src/metathesis.rs` is oracle/read-only except for a
demonstrated bug fix in a separately recorded task.

Run from `rust/`:

- `cargo test -p pg-foma --test phase_c_metathesis`
- `cargo test -p pg-rules --test rewrite_gate`
- `cargo test -p pg-foma --test phase_c_multi_table`
- `cargo test -p pg-foma --test phase_c_quantifier`

## Dependencies

This change is authored on the reified compilation model (`reify-compilation-plans`) rather than the
old hardcoded `should_run`/`probe_would_refuse`/`partition_entries` branching: the metathesis swap
relation is a `Plan` `Leaf`/`Compose` node the enumerator selects among, not a special-cased branch.
Its capability boundary — which switch/boundary/direction/feature-class/table combinations compile
faithfully — is a configuration-predicate registered with `add-capability-characteristics-check`,
confirm-only-by-default per ADR 0001 unless a proven no-false-negative admission-filter argument
exists.
