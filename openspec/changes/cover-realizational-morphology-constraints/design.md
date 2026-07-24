## Decisions

- Depend on merged template/truncation/reduplication work and take final exclusive morphology ownership.
- Prefer `confirm-only` for constraints requiring mutable derivational history or full feature
  unification. Add FST admission filters only with explicit no-false-negative proofs.
- `max_apps` affects bounded proposer construction where available; unbounded realizational behavior
  uses existing budgets and HermitCrab confirmation rather than arbitrary enumeration.
- The complete Rust HermitCrab implementation and model remain unchanged except separately proven bugs.

## Ownership and verification

Exclusive files: relevant regions of `pg-foma/src/emit.rs`, `morphotactics.rs`, `preexpand.rs`, and
`confirm.rs`, plus focused pg-foma tests. `pg-rules/src/morph.rs` and `word.rs` are oracle/read-only.

Run from `rust/`:

- `cargo test -p pg-rules --test morph_gate`
- `cargo test -p pg-rules --test max_apps_gate`
- `cargo test -p pg-rules --test validity_gate`
- `cargo test -p pg-rules --test memo_gate`
- `cargo test -p pg-foma --test f4_composite_gate`
- `cargo test -p pg-foma --test p6_gate_parity`

## Dependencies

This change is authored on the reified compilation model (`reify-compilation-plans`) rather than the
old hardcoded `should_run`/`probe_would_refuse`/`partition_entries` branching. Its capability boundary
is a configuration-predicate registered with `add-capability-characteristics-check`: constraints
requiring mutable derivational history or full feature unification are confirm-only-by-default per
ADR 0001, and an FST admission filter is added only where a proven no-false-negative argument licenses
it — consistent with this change's existing `confirm-only` preference above.
