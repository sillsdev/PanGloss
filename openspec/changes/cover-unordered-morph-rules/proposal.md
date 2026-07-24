## Why

`MorphRuleOrder::Unordered` is honored by the confirm engine but never proposed by the FST, so a
grammar declaring unordered morphological rule application compiles and silently loses the analyses
that depend on alternative application orders. Until proven, it is fail-closed by `add-capability-
characteristics-check`; this change promotes it. See `docs/adr/0001`. (The Aweti baseline regression —
honest 32/104 after unordered/RTL/simultaneous stopped being silently mistranslated — is a concrete
instance of this gap.)

## What Changes

- Define the **configuration-predicate capability boundary** for unordered rule application (which
  configurations are proven faithful, including the interaction with rule count / chain depth; which
  stay fail-closed).
- Implement recall-preserving FST proposal of unordered application on the reified compilation model,
  bounded by the ADR 0003 chain-depth budget.
- Ship the full kit: oracle witnesses; a **synthetic** `machine/conformance/` fixture; big-O
  characterization + resource thresholds; runtime-feature declaration; diagnostics.
- Flip to supported only after the conformance fixture passes the Stage 0A gate.

## Impact

Closes one third of the parity hole and underwrites an honest Aweti-shaped target. Depends on the
keystone, the reified model, and the chain-depth budget dimension (Stage 0C extension).
