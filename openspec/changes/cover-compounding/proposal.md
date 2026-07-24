## Why

`MorphRuleDef::Compounding` is fully implemented in the confirm engine but has no FST proposer owner,
so a grammar using compounding compiles and silently loses recall — the exact silent-overapproximation
the capability contract exists to catch. Until proven, it is fail-closed by `add-capability-
characteristics-check`; this change promotes it to supported. See `docs/adr/0001`.

## What Changes

- Define the **configuration-predicate capability boundary** for compounding (which compounding
  configurations are proven faithful; which stay fail-closed), never a blanket variant claim.
- Implement recall-preserving FST proposal of compounding on the reified compilation model.
- Ship the full kit: oracle witnesses; a **synthetic** `machine/conformance/` fixture (named by
  construct/composition, family only in comments); big-O characterization + resource thresholds; a
  per-construct runtime-feature declaration (ADR 0004); diagnostics.
- Flip the ledger/manifest disposition to supported **only after** the conformance fixture passes the
  Stage 0A gate.

## Impact

Closes one third of the parity hole. Depends on the characteristics-check keystone and the reified
compilation model. Interaction with templates/tables/strata is proven separately (Stage 3 pairwise)
or held fail-closed until proven.
