## Decisions

- Depend on merged circumfix/null/output-action work and take the next exclusive emitter ownership.
- Test template alternatives and ordering at the emitted-network boundary; test reduplication at the
  peeler-to-confirm boundary.
- Bounded truncation preexpansion must use existing work budgets and never truncate silently.
- Existing HermitCrab template/reduplication behavior is oracle-only unless a demonstrated bug is
  opened separately.

## Ownership and verification

Exclusive files: template/truncation/reduplication regions of `pg-foma/src/emit.rs`, `preexpand.rs`,
`peel.rs`, and new focused `pg-foma` tests. Existing `pg-rules` gates are read-only oracle evidence.

Run from `rust/`:

- `cargo test -p pg-foma --test p6_gate_parity`
- `cargo test -p pg-foma --test f1_sena_gate`
- `cargo test -p pg-foma --test f2_indonesian_gate`
- `cargo test -p pg-rules --test template_partial_gate`
- `cargo test -p pg-rules --test redup_and_free_fluctuation_gate`
- `cargo test -p pg-rules --test max_apps_gate`

When available, run `tools/run-conformance.sh edge-cases/truncate-morphotactic`; otherwise record
that external conformance evidence as `not_run` and continue self-contained verification.
