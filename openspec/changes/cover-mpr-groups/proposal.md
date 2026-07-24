## Why

`MprGroup` (morphophonemic rule groups) is implemented in the confirm engine but has no FST proposer
owner, so grammars using grouped rule application compile and silently lose recall. Until proven, it
is fail-closed by `add-capability-characteristics-check`; this change promotes it. See `docs/adr/0001`.

## What Changes

- Define the **configuration-predicate capability boundary** for `MprGroup` (which grouping
  configurations are proven faithful; which stay fail-closed), including interaction with realizational
  rules and rule ordering.
- Implement recall-preserving FST proposal of grouped application on the reified compilation model.
- Ship the full kit: oracle witnesses; a **synthetic** `machine/conformance/` fixture; big-O
  characterization + resource thresholds; runtime-feature declaration; diagnostics.
- Flip to supported only after the conformance fixture passes the Stage 0A gate.

## Impact

Closes the final third of the parity hole. Depends on the keystone and the reified model; its
interaction with `cover-realizational-morphology-constraints` is proven separately or held fail-closed.
