## Why

`MprGroup` (morphophonemic rule groups) is implemented in the confirm engine but has no FST proposer
owner, so grammars using grouped rule application compile and silently lose recall. Until proven, it
is fail-closed by `add-capability-characteristics-check`; this change promotes it. See `docs/adr/0001`.

## What Changes

- Define the **configuration-predicate capability boundary** for `MprGroup` at
  `mpr-group.append-output` (target: `ConfirmOnly` via a non-narrowing propose baseline; `Admit`
  is a distinct, harder, unproven step) vs `mpr-group.overwrite-output` (stays `FailClosed`; never
  an FST admission filter without a replace-semantics proof — ADR 0001's own worked confirm-only
  trap), never a blanket `MprGroup` verdict. See `design.md` D1-D3 for why the split is drawn on
  `MprGroupOutput` and not elsewhere. Includes the interaction with realizational rules (same
  `AffixAllomorphDef` field surface, no special case) and with rule ordering (`design.md` D4).
- Author the non-narrowing over-approximating proposal baseline for `mpr-group.append-output` as a
  node-position characterization over the existing `Gate` node (`design.md` D5) — **blocked on**
  `reify-compilation-plans` landing; not claimed provable inside this change until it closes.
- Ship the full kit: oracle witnesses (including a witness pinning the `Append`/`Overwrite`
  order-(in)dependence distinction, `design.md` D4); a **synthetic** `machine/conformance/` fixture
  (named by construct, family only in comments); big-O characterization + resource thresholds; a
  per-construct runtime-feature declaration (ADR 0004); diagnostics.
- Flip the ledger/manifest disposition to supported **only after** the conformance fixture passes
  the Stage 0A gate — and only for `mpr-group.append-output`; `mpr-group.overwrite-output` remains
  `FailClosed` (ADR 0005 override as its on-ramp) pending a proof this change does not attempt.

## Impact

Closes at most one third of the parity hole, and only its `Append`-output slice — the `Overwrite`
slice is an explicitly declared, not a hidden, remaining gap. Depends on the characteristics-check
keystone and the reified compilation model (both hard blockers — see `design.md` Dependencies).
Registers, for the first time from this side, the group-(un)awareness contract `cover-compounding`
already named (`design.md` D4: `compound_match` is out of scope for these predicates). Interaction
with `cover-unordered-morph-rules` (order-(in)dependence of accumulated group state) and with
`cover-realizational-morphology-constraints` (shared field surface, no special case) is recorded as
open/shared in `design.md` D4.
