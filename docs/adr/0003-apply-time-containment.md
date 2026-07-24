# Apply-time containment: cooperative magnitude budgets in-process; engine reports, app decides

## Decision

Compile-time work runs in a killable native worker under the parent watchdog. **Apply-time
(word analysis) runs in-process** and is contained by **deterministic cooperative logical
budgets**, not a watchdog. Per-word analysis is gated by **magnitude-only** per-dimension
budgets; a word either completes (possibly with zero analyses) or returns a typed incomplete
outcome naming the dimension and value it hit. Aggregate/batch resource *policy* ("this
10,000-word spell-check is struggling — stop") belongs to the **application**, which the
engine serves with a cumulative batch budget, cooperative cancellation, streamed per-word
outcomes, and reported health evidence.

## Why

The watchdog protects the one place we spawn native compile work; apply runs constantly,
per word, in the caller's process, where a native thread cannot be safely hard-killed in
Rust. The observed failure modes (stack overflow — the 1 GiB-stack workaround exists because
this happened; the Aweti 24-level chain; OOM between RSS samples) are all **deterministically
boundable** by tracking the right dimensions, and cooperative logical budgets port identically
to native Windows, native Linux, and WASM — which is the only way the "identical budget
schema across all three" claim is honest (watchdogs do not port to WASM).

## Key consequences

- **Two dimensions are added to the tracked set:** derivation/unapplication **chain depth**
  (closes stack overflow deterministically, unlike a larger stack) and an
  **allocation/logical-memory** budget reserved *before* material allocation via a proven
  work bound (closes the between-samples OOM before it happens).
- **Magnitude-only, never yield-based, for the per-word kill.** Zero analyses is a valid
  complete result; a low-yield / high-rejection computation is not pathological (e.g. correct
  text analyzed against the wrong-language pack finds nothing, fast). Killing on yield would
  misclassify healthy empty results as runaway. Rejection share, confirmation count, and
  duplicate ratio are **reported diagnostic evidence** for the caller — explicitly *not*
  inputs to the per-word kill decision.
- **Budgets are calibrated, not guessed.** Each dimension's default = N× the worst word
  observed to legitimately terminate across every target language + stress grammar + long/
  ambiguous corpus. Deterministic counters → reproducible kills (same word fails identically
  on a fast laptop and a slow CI box). Wall-clock is the outer net for genuinely
  uninstrumented stalls only — best-effort, non-reproducible, never the normal incomplete
  trigger. Governed like `calibrate-fst-resource-envelopes`: evidence + proposed diff +
  human-reviewed commit.
- **Two-sided calibration validation.** Lower bound: every legitimate-corpus word must
  complete within the default (a real word tripping it is a cost problem for the dead-end
  census, never a silent cap raise). Upper bound: default × max concurrency ≪ absolute
  ceiling ≪ host capacity.
- **"Slow but will find it" past the default** is handled by explicit caller retry with a
  larger named envelope (no automatic escalation); the typed incomplete outcome names the
  dimension so the caller can decide, and the retry disambiguates hard-but-real from loose.
- **The uninstrumented-hang residual** (an infinite loop touching no counter) is contained by
  a documented **host contract**: run analysis in your own killable worker — Worker +
  `terminate()` for WASM — because the engine cannot hard-kill an in-process native thread.
