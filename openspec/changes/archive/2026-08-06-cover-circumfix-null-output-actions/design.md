## Decisions

- Depend on merged metathesis work, then take exclusive ownership of morphology emitter hotspots.
- Treat each `OutputAction` sequence and role combination as its own ledger variant.
- A compiled or preexpanded path must preserve one morpheme identity across discontinuous output.
- Variants not representable in the proposer remain peeled, confirm-only, or honestly unsupported;
  they are never silently reduced to the first inserted segment.

## Ownership and verification

Exclusive files: relevant regions of `pg-foma/src/emit.rs`, `preexpand.rs`, `peel.rs`, and
`pg-foma/tests/phase_c_circumfix.rs`. Do not modify `replace.rs`.

Run from `rust/`:

- `cargo test -p pg-foma --test phase_c_circumfix`
- `cargo test -p pg-foma --test f4_composite_gate`
- `cargo test -p pg-rules --test morph_gate`
- `cargo test -p pg-rules --test stratum_gate`

## Dependencies

This change is authored on the reified compilation model (`reify-compilation-plans`) rather than the
old hardcoded `should_run`/`probe_would_refuse`/`partition_entries` branching: circumfix/null-output
emission is expressed as `Plan` node structure the enumerator selects among. Its capability boundary
— which `OutputAction` sequence/role combinations compile faithfully — is a configuration-predicate
registered with `add-capability-characteristics-check`, confirm-only-by-default per ADR 0001 unless a
proven no-false-negative admission-filter argument exists.
