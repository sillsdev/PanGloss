## Why

`MorphRuleDef::Compounding` is fully implemented in the confirm engine but has no FST proposer owner,
so a grammar using compounding compiles and silently loses recall — the exact silent-overapproximation
the capability contract exists to catch. Until proven, it is fail-closed by `add-capability-
characteristics-check`; this change promotes it to supported. See `docs/adr/0001`.

## What Changes

- Define the **configuration-predicate capability boundary** for compounding at
  `compounding.non-recursive` (target: `ConfirmOnly`) vs `compounding.recursive` (stays
  `FailClosed` pending the chain-depth interaction), never a blanket variant claim. See `design.md`
  D2 for why the split is drawn there and not elsewhere.
- Author the over-approximating `Gate`/`Compose`/`Union` proposal shape for
  `compounding.non-recursive` on the reified compilation model (`design.md` D3) — **blocked on**
  `reify-compilation-plans` landing and on a measured big-O threshold; not claimed provable inside
  this change until both close.
- Ship the full kit: oracle witnesses (including a witness pinning the MPR group-(un)awareness trap,
  `design.md` D4); a **synthetic** `machine/conformance/` fixture (named by construct/composition,
  family only in comments); big-O characterization + resource thresholds; a per-construct
  runtime-feature declaration (ADR 0004); diagnostics.
- Flip the ledger/manifest disposition to supported **only after** the conformance fixture passes the
  Stage 0A gate — and only for `compounding.non-recursive`; `compounding.recursive` remains
  `FailClosed` (ADR 0005 override as its on-ramp) until a separate change characterizes the
  chain-depth interaction.

## Impact

Closes at most one third of the parity hole, and only its non-recursive slice — the recursive slice
is an explicitly declared, not a hidden, remaining gap. Depends on the characteristics-check keystone
and the reified compilation model (both hard blockers, not soft ones — see `design.md`
Dependencies). Interaction with MPR groups and templates is recorded as open in `design.md` D4, since
neither `cover-mpr-groups` nor `cover-template-truncation-reduplication` names compounding today.
Interaction with strata generally is proven separately (Stage 3 pairwise) or held fail-closed until
proven.
