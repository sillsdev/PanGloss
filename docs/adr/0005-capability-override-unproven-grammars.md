# Capability override: unproven grammars load, run, and signal degraded trust

## Decision

The characteristics-check hard-fail (ADR 0001) is **overridable** through an explicit
**capability override**. Force-compiling a grammar the gate refused produces an artifact that
is **indelibly stamped unproven / recall-unsafe** in its manifest. Such an artifact MAY be
built, serialized, distributed, loaded, and **run** — including handed to an end user to try —
but the Runtime broadcasts a **strong, machine-readable degraded-trust signal** at load and on
every analysis result, which the consuming application keys off to warn the user ("this is
potentially broken"). The safety mechanism for an unproven pack is the *signal*, not a refusal
to run.

The unproven stamp is **indelible**: an overridden artifact can never pass the conformance gate,
never earns "supported" status, and the stamp is cleared only by *actually proving the
construct* — adding conformance coverage, flipping the gate to supported, and recompiling
**without** the override. A consumer cannot strip it.

## Why

Nothing ships today and there is no existing user (see the multi-topology-first stance), but
field-testing an unproven grammar with a real user is genuinely valuable, and the override is
also the **development on-ramp for promoting every construct**: force-compile experimentally →
iterate → prove correct against the oracle → add the conformance fixture → the gate flips the
construct to supported → the override is no longer needed. Without an escape hatch, you cannot
work on a construct that is (correctly) fail-closed, nor put a work-in-progress grammar in front
of a user.

The danger is exact and severe: an escape hatch silently reopens the overclaim hole this whole
architecture exists to close. A force-compiled artifact is recall-unsafe *by definition* — the
gate refused it because some valid HermitCrab analysis may be silently dropped. Containment via
an indelible, runtime-broadcast trust signal is what keeps "never overclaim" mechanical while
still allowing the artifact to run.

## Key consequences

- **Load-and-run, not publication-banned.** An unproven pack is distributable and executable;
  it is the *signal*, surfaced loudly, that protects the user — not a hard refusal. This
  deliberately distinguishes the capability override from an absolute quarantine.
- **The trust signal is first-class and two-level.** At load, the pack reports pack-level
  `unproven`/`overridden` status; on every analysis, each result carries a degraded/experimental
  flag. An app can therefore warn once (banner) and/or per-result, and a user can decide whether
  to trust an answer. A proven pack carries the clean status and neither signal fires.
- **The override is explicit and recorded.** Who/when/why, and exactly which fail-closed
  configurations were overridden, are written into the manifest override record — reusing
  ADR 0004's manifest admission/findings/override field rather than inventing a parallel one.
- **Indelible + conformance-excluded.** The stamp survives serialization and cannot be removed
  by a consumer; the conformance gate never passes for an overridden artifact, so an unproven
  pack can never be laundered into a "supported" claim. Only genuine proof + clean recompile
  clears it.
- **Distinct axis from cost/health.** Capability-override (a correctness-trust axis) composes
  with, but is separate from, the FST-health admission bands (Warning / Error-with-override /
  Critical — a cost/size axis, ADR 0001's cost side). A pack can be cost-healthy yet
  capability-unproven, or vice versa; the manifest carries both independently.
- **It is the construct-promotion workflow.** The override is not only a debugging valve — it is
  the standard loop by which each construct earns "supported" status one at a time, which is the
  slow, deliberate capability-growth pace ADR 0001 prescribes.
