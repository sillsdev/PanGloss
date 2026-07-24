## Why

`MorphRuleOrder::Unordered` is honored by the confirm engine but never proposed by the FST, so a
grammar declaring unordered morphological rule application compiles and silently loses the analyses
that depend on alternative application orders. Until proven, it is fail-closed by `add-capability-
characteristics-check`; this change promotes it. See `docs/adr/0001`. (The Aweti baseline regression —
honest 32/104 after unordered/RTL/simultaneous stopped being silently mistranslated — is a concrete
instance of this gap.)

## What Changes

- Define the **configuration-predicate capability boundary** for unordered rule application at
  `unordered-application.chain-depth-bounded` (target: `ConfirmOnly` via an ordering-union propose
  strategy, once a calibrated bound exists) vs `unordered-application.unbounded` (stays `FailClosed`
  until a bound is calibrated), never a blanket `Unordered` verdict. See `design.md` D1 for why the
  split is drawn on a chain-depth cardinality bound.
- Author the over-approximating ordering-union proposal as a search-discipline widening on the
  stratum's existing chain subtree (`design.md` D2) — **blocked on** `reify-compilation-plans`
  landing and on extending the ADR 0003 chain-depth budget with an ordering-multiplicity dimension;
  not claimed provable inside this change until both close.
- Ship the full kit: oracle witnesses (including a witness distinguishing the existing
  morphotactic-legality convention from a genuine proposal-language proof, `design.md` D1 blocker 2);
  a **synthetic** `machine/conformance/` fixture (named by construct, family only in comments); big-O
  characterization + resource thresholds; a per-construct runtime-feature declaration (ADR 0004);
  diagnostics.
- Flip the ledger/manifest disposition to supported **only after** the conformance fixture passes the
  Stage 0A gate — and only for `unordered-application.chain-depth-bounded`;
  `unordered-application.unbounded` remains `FailClosed` (ADR 0005 override as its on-ramp) until a
  calibrated bound exists.

## Impact

Closes one third of the parity hole and underwrites an honest Aweti-shaped target, but only within
the calibrated bound — configurations exceeding it are an explicitly declared, not a hidden, remaining
gap. Depends on the characteristics-check keystone, the reified compilation model, and an ADR 0003
extension (the ordering-multiplicity dimension, `design.md` D1/D3 — a required budget extension, not
an open question). Interaction with `cover-mpr-groups` (order-(in)dependence of accumulated MPR-group
state) and `cover-compounding` (compounding as a loose stratum rule) is recorded as
load-bearing/open, respectively, in `design.md` D3.
