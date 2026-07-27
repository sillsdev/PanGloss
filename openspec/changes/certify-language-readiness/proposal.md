## Why

There is no answer today to the question a project lead actually asks: *will this language work well
on a device?* The pieces exist in fragments — a conformance suite that says whether constructs behave,
a benchmark matrix produced by hand, a capability gate that refuses grammars, a pack with a size — but
nothing composes them into a verdict, and nothing is reproducible by someone who did not run it.

Two consequences. First, a grammar that is quietly too slow, or too large, or that covers only 60% of
real text, ships looking the same as one that is fine. Second — and this is the point of the exercise
— a language that **cannot** reach the bar with today's compiler has no way to say so, so nobody knows
to ask for the support it would need. Some languages will never certify as things stand; naming which,
and exactly what blocks them, is the deliverable, not a failure of it.

## What Changes

Three layers, in dependency order.

- **Timing, measured by the suite rather than by hand.** The synthetic-language conformance suite gains
  per-word timing and can run in either engine mode — complete Rust HermitCrab, or the compiled
  proposer plus confirm — emitting CSV, rendered as a markdown table. The interesting output is
  **speedup per typology**, because the fixtures are named by construct and typology, so the table
  answers which *kinds* of language the compiled path actually helps.
- **A certification with explicit, published thresholds**: pack size, lexicon scale, token coverage on
  a held-out text, and p50/p90/p99 latency against a stated device class. It produces a tiered verdict,
  including tiers that mean "not achievable with today's compiler support, and here is the construct
  responsible".
- **`pangloss make-report`**: one command, one markdown file — build time, size, latency percentiles,
  the plan diagram, and the conformance verdict, with every failed check named.

## Impact

Additive and read-only with respect to compilation. No change to admission, refusal, or the
propose-and-confirm contract.

The honesty constraints are load-bearing rather than decorative, and are specified as requirements:

- A `trust=unproven` pack (produced via the ADR 0005 capability override) is **never certifiable**.
  Otherwise the override becomes a back door to a certification stamp, which would invert its purpose.
- "Held out" cannot be verified mechanically — PanGloss does not train, and nothing in the artifact
  records what its author looked at while authoring. So it is recorded as an **attestation with a named
  attestor**, not presented as a checked property.
- Coverage is a **token-level analysis rate**, not correctness. A word with an analysis may have the
  wrong one. The certificate must not let "95% coverage" be read as "95% correct".
- Latency percentiles are meaningless without a **stated target device class**, and the current
  measurement path has an integer-millisecond floor, so sub-millisecond results must be reported as
  such rather than as zero.

Depends on `visualize-compilation-plan` for the embedded diagram.

Known at authoring time, and a useful early test of the design: per `docs/benchmark-matrix.md`, all
three reference grammars are currently refused on the compiled path, two of them by a permanent
carve-out. Under an honest certification **none of them certifies today** — which is exactly the
signal this change exists to make visible.
