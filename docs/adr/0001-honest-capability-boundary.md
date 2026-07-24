# Honest capability boundary: parity-or-explicit-carve-out, fail-closed by default

## Decision

PanGloss compilation is gated by a **characteristics check**: a grammar (plus its stem
data) is projected into a **characteristics profile**, matched against the composed
**capability envelope** of the compilation stages, and either compiled or **hard-failed at
compile time** with a typed diagnostic naming what cannot be done faithfully. "Faithful"
means recall-preserving (the propose-and-confirm invariant: never omit a valid HermitCrab
analysis). The goal is full parity with the frozen HermitCrab model, or an **explicit,
declared carve-out** ("we cannot compile these"). Silent overapproximation-that-loses is
never acceptable. Capability work then proceeds construct-by-construct in parallel, each
construct shipping with its full kit (detection predicate, oracle-backed witnesses,
conformance grammars, big-O characterization + resource thresholds, diagnostics); completing
a construct flips its configuration from fail-closed to supported.

## Why

Silent recall loss (e.g. `Compounding` and `MorphRuleOrder::Unordered` are fully implemented
in the confirm engine but the FST proposer never proposes them → 0 recall, no error) makes
the product quietly wrong rather than loudly failed. A comprehensive, enforced,
default-deny boundary converts "silent recall loss discovered in production" into "loud gap
discovered at the ledger," which is the only mechanically enforceable form of "never
overclaim."

## Key consequences

- **The characteristics check is the contract — a first-class citizen, not a byproduct.**
  It is not a passive ledger, a lint, or an after-the-fact report: it is the load-bearing
  artifact that decides whether a grammar compiles at all. It is *dynamic and composed of many
  moving parts* — per-stage capability predicates, interaction predicates, cardinality bounds —
  that assemble per compilation plan into the capability envelope the grammar's profile is
  matched against. **If the moving parts cannot be made to fit the grammar, it is game over: a
  hard compile-time failure with a typed diagnostic, never a degraded or partial compile.** It
  therefore owns its own keystone change (`add-capability-characteristics-check`), sequenced
  ahead of everything else, and absorbs the conformance-coverage CI gate (below). Its first act
  is to mark every not-yet-proven configuration — including the known silent-recall-loss gaps
  `MorphRuleDef::Compounding`, `MorphRuleOrder::Unordered`, and `MprGroup` — fail-closed, so the
  overclaim hole is closed the moment the keystone lands; constructs are then promoted to
  supported one at a time, each with its full kit. The pre-existing coverage ledger
  (`define-grammar-coverage-contract`) supplies evidence *into* this gate; it is not itself the
  gate.
- **Granularity is configuration-predicate, not variant.** "Simultaneous rewrite" is not
  wholesale supported/unsupported; it is "supported *unless* two subrules' environments can
  overlap at one position." Each predicate is itself a proof obligation (oracle-verified,
  conservative — may over-refuse, must never under-refuse).
- **Default-deny lives at the characterizer.** The enumerator is exhaustive over the frozen
  model with **no catch-all**, so adding a `model.rs` variant breaks the build until someone
  characterizes it. A profile that omits a construct would let the check pass vacuously —
  the exact way `Compounding` slipped through.
- **Capability is a joint proposer+confirm claim.** Where the oracle itself is unverified
  for a configuration (e.g. simultaneous-subrule overlap, never pinned against `hc.dll`), the
  configuration is unsupported *by definition* — there is no correct behavior to check
  against.
- **Confirm-only by default.** The standing rule for every construct: the FST proposer
  overapproximates (proposes broadly) and the confirm engine prunes to the exact HermitCrab
  set. Having the FST itself *filter* admissions — narrowing what it proposes so confirm does
  less work — is an **optimization**, and it carries a proof obligation: a construct's proposer
  may only admission-filter where a proven no-false-negative argument shows the filter can
  never drop a valid analysis. Absent that proof, the construct is **confirm-only** — it must
  propose the superset and lean entirely on confirm. This makes "never under-propose"
  *structural* rather than a matter of per-author diligence: the unsafe direction (a naive FST
  filter that silently omits, e.g. history-dependent `MprGroup::Overwrite`) is closed by
  default, and admission-filtering is an opt-in guarded by the same over-refuse-never-under-
  refuse discipline as the characterizer's predicates.
- **Interactions do not compose for free.** A composition node's capability is not the union
  of its children's; composing two safe stages can create an emergent hazard
  (feeding/bleeding non-termination, order-dependence). Orthogonal branches compose by union;
  non-orthogonal ones require a proven interaction predicate at the node, else fail closed.
  Proving a set of constructs orthogonal (per composition topology) retires whole swaths of
  the combination space at once — the convergence mechanism that keeps "fail a lot at first"
  from meaning "refuse everything forever."
- **"Supported" is mechanically gated on passing conformance coverage.** A construct or
  configuration cannot flip from fail-closed to supported unless the in-repo conformance
  suite (`machine/conformance/`) actually exercises it and passes: CI cross-checks the
  **capability registry** (the source-controlled, per-construct supported/unsupported contract —
  distinct from a per-`.pgpack` **pack manifest**; bare unqualified "manifest" is banned) against
  conformance coverage (`constructs.txt` / per-word `exercises:` / `rules.csv`), and marking
  anything supported without a covering, passing fixture **breaks the build.** This turns the claim *"if a grammar compiles, it is accurate"* into an enforced
  property rather than a promise, and makes the conformance suite the literal gate through
  which a construct earns "supported" status. It is the same default-deny discipline as the
  characterizer's no-catch-all, extended all the way to accuracy evidence — closing the exact
  hole `Compounding` fell through (implemented, never proposed, never conformance-covered, no
  build failure). The suite's ground truth is the committed per-fixture `words.yaml`, authored
  from the founding C# HermitCrab oracle and human-accepted (never blindly regenerated); the
  `expected.tsv` a run diffs against is **materialized at runtime** by `FixtureMaterializer`
  from that `words.yaml` (it is not itself committed, so there is no second copy to drift).
  PanGloss is validated through the engine-agnostic adapter contract (`PROTOCOL.md`), diffing
  its output against that materialized ground truth. There is therefore
  no separate "certification" stage and no prior artifact to go stale — the integration tests
  run the current engine against versioned-in-repo truth.
- **Capability and cost are gated by different standards.** Capability/correctness is
  proven a-priori and hard-fails. Cost/size is **cost-uncertain**: a predicted conservative
  bound that only *warns*, with the real limit enforced by runtime logical-work counters
  under the watchdog. Cost never produces a supported/unsupported correctness verdict.
- **Two-tier, migrating.** The production mainline hands one lexc source to a black-box foma
  compiler; its capability is proven **behaviorally, by oracle witnesses**, and its compiled
  size is unobservable (cost is enumeration-proxy + runtime only). The controllable
  composition path (today's P6 `replace.rs`/`gate.rs`, wired to production in Stage 2) admits
  **structural** capability proof and real state/arc cost gating. The characteristics profile
  carries a **capability evidence provenance** field (`behavioral` vs `structural`) so the
  two are never conflated. The capability/cost requirements are the explicit forcing function
  for migrating constructs onto controllable composition.

## Considered and rejected

- **Variant-level granularity** — cannot express "supported except when…", so it either
  overclaims (whole variant marked supported, bad config leaks) or over-refuses (whole
  variant refused for one unproven config).
- **Strict n-way interaction proof** — maximally honest but never ships a real language
  (real grammars are rich n-way combinations).
- **Pairwise-confidence as a trust level** — rejected. The trust axis is **binary** (proven
  vs unproven; see ADR 0005): there is no "pairwise-confident" middle tier a grammar can be
  admitted at. Interaction coverage is instead a *test-coverage evidence method* feeding the
  binary gate, and its right shape is **tree-structured node/subtree fuzzing** over the reified
  compilation plans (ADR 0002), not covering arrays over raw knobs: the compilation tree *is*
  the interaction surface, so fuzz each non-orthogonal composition node and its connected
  subtree, index every conformance fixture by the node/subtree it exercises, and apply
  covering-array minimization over composition-types (not raw knobs) to cover legal
  co-occurrences absent from the authored corpus. A proven-orthogonal set still retires
  combination space by union; a residual pairwise-only limitation is *declared and stamped*,
  never a hidden claim.
