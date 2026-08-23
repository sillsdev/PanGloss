# Capability override: unproven grammars are developer-only and signal degraded trust

## Decision

The characteristics-check hard-fail (ADR 0001) is **overridable only by a hidden,
developer-build-only** `--allow-unproven` correctness override. Force-compiling a grammar the
gate refused may omit valid parses by definition, so any resulting artifact or construction
state is **indelibly stamped unproven / recall-unsafe** in its pack manifest. The switch is
rejected in production builds and is never a publication, distribution, certification, or
conformance path. A developer may load or run the result for grounding, but the Runtime must
broadcast a **strong, machine-readable degraded-trust signal** at load and on every analysis
result. The signal makes the risk visible; it does not make the result accurate.

The unproven stamp is **indelible**: an overridden artifact can never pass the conformance gate,
never earns "supported" status, and the stamp is cleared only by *actually proving the
construct* — adding conformance coverage, flipping the gate to supported, and recompiling
**without** the override. A consumer cannot strip it.

## Why

The override is the **development on-ramp for promoting every construct**: expose a refused
shape for inspection → iterate → prove correct against the oracle → add the conformance fixture
→ the gate flips the construct to supported → the override is no longer needed. It is a
developer grounding tool, not a way to put a work-in-progress grammar in front of a user or to
turn omitted analyses into a product claim.

The danger is exact and severe: an escape hatch silently reopens the overclaim hole this whole
architecture exists to close. A force-compiled artifact is recall-unsafe *by definition* — the
gate refused it because some valid HermitCrab analysis may be silently dropped. Quarantine via
an indelible, runtime-broadcast trust signal is what keeps "never overclaim" mechanical while
still allowing the artifact to run.

## Key consequences

- **Developer inspection, not publication.** An unproven result may be loaded or executed by
  an explicitly enabled developer tool for grounding, but production builds, publication,
  distribution, certification, and conformance must reject it. The signal is required even in
  that developer path and is not a substitute for correctness.
- **The trust signal is first-class and two-level.** At load, the pack reports pack-level
  `unproven`/`overridden` status; on every analysis, each result carries a degraded/experimental
  flag. Developer tooling can therefore warn once (banner) and/or per-result while inspecting
  behavior; production consumers reject the result before publication or certification. A proven
  pack carries the clean status and neither signal fires.
- **The override is explicit and recorded.** Who/when/why, and exactly which fail-closed
  configurations were overridden, are written into the **pack manifest** override record —
  reusing ADR 0004's pack-manifest admission/findings/override field rather than inventing a
  parallel one. ("Pack manifest" is the per-`.pgpack` blob; it is distinct from the source-
  controlled **capability registry** of ADR 0001. Bare unqualified "manifest" is banned.)
- **Indelible + conformance-excluded.** The stamp survives serialization and cannot be removed
  by a consumer; the conformance gate never passes for an overridden artifact, so an unproven
  pack can never be laundered into a "supported" claim. Only genuine proof + clean recompile
  clears it.
- **Distinct axis from cost/health and containment.** `--allow-unproven` is a correctness-only
  override and may omit valid parses; it does not waive a resource limit. `Error` is a health /
  production-readiness finding, not a capability refusal: an explicit developer stress attempt
  may use hidden `--remove-size-limits` to disable only internal deterministic size/work caps,
  while retaining worker isolation, bounded I/O, external watchdog/RSS/absolute ceilings,
  capability checks, exact completion, finalized payload, and parity. A complete stress result
  can therefore be accurate evidence while remaining production-unready because its health is
  Error. `Critical` correctness/capability gaps still refuse trusted production output; only
  `--allow-unproven` can expose them for developer inspection, with the unproven stamp. The
  legacy `--no-enforce-capability` switch is developer-only and non-production. The pack manifest
  carries correctness trust, health, and containment provenance independently.
- **It is the construct-promotion workflow.** The override is not only a debugging valve — it is
  the standard loop by which each construct earns "supported" status one at a time, which is the
  slow, deliberate capability-growth pace ADR 0001 prescribes.
