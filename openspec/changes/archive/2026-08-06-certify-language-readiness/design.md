## Context

Existing pieces this composes, none of which currently talk to each other: the conformance suite over
synthetic fixtures (correctness), `pangloss batch`'s per-word `elapsed_ms` column (latency),
`pangloss pack`'s artifact and its ADR 0005 trust stamp (size and trust), the capability gate's refusal
diagnostics (what a grammar cannot compile), and `docs/benchmark-matrix.md` (a hand-made precedent for
the table this should generate).

## Goals / Non-Goals

- **Goal:** a reproducible verdict someone who did not run it can trust and re-derive.
- **Goal:** a language that cannot reach the bar is told so, with the blocking construct named, so its
  team knows what to ask for. This is the primary purpose, not the consolation prize.
- **Non-Goal:** certifying correctness. Certification composes correctness evidence produced elsewhere;
  it does not independently establish it.
- **Non-Goal:** a single pass/fail bit. A bit cannot distinguish "too slow today" from "contains a
  permanently carved-out construct", and those call for completely different responses.

## Decisions

### The verdict is tiered, and one tier is about the compiler, not the language

A flat pass/fail would collapse two situations that must stay distinct:

- **Not yet** — the grammar compiles and runs, but misses a threshold (too slow, too large, coverage
  short). Actionable by the language team: more lexicon, fewer pathological rules, better data.
- **Not supported** — the grammar contains a construct the compiler refuses, so no amount of authoring
  effort will move it. Actionable only by *us*: it names the construct and the predicate that refused
  it, and it is the signal to ask for compiler work.

The second tier is the one the user asked for, and it must cite the real capability refusal rather than
inferring from a failure to run.

### A trust=unproven pack can never certify

ADR 0005's override exists so a grammar can be force-compiled with an indelible degraded-trust stamp.
If an overridden pack could certify, the override would become the shortest path to a certificate, and
the stamp would be decorative. So: refuse to certify, and say the override is why. This is the single
most important rule in the change, because it is the one whose violation would be most convenient.

### "Held out" is an attestation, not a measurement

The requirement is text held out of grammar authoring. Nothing in the artifact records what its author
read. Rather than pretend to verify it, the certificate records who attested it and when, and states
plainly that it is unverified. An attestation that is labelled as such is honest; a check that cannot
fail is not.

### Coverage is named as a rate, never as accuracy

Token-level analysis rate — the fraction of tokens receiving at least one analysis. A token may receive
a *wrong* analysis and still count. The certificate must use wording that cannot be read as accuracy,
because "95% coverage" invites exactly that misreading, and correctness is the conformance suite's job.

### Latency needs a device class and finer resolution than we have

Percentiles without a target device are unfalsifiable. The certificate names the class it was measured
against and does not silently generalize. Separately, `elapsed_ms` is integer milliseconds, so a fast
grammar's p50 lands in the 0 bucket — either finer timing is added at the measurement site, or sub-ms
results are reported as `<1` and the certificate says the floor exists. Reporting `0` would be a
precision claim we cannot support.

### The report names failures, not just the verdict

A report saying "not certified" without naming which checks failed and which constructs blocked them
is useless to the person deciding whether to ask for support. Every failed check is listed with its
measured value, its threshold, and — for a refusal — the predicate and construct.

## Risks / Trade-offs

- **A certificate is a green light, and green lights get cited.** The same reasoning that governed the
  conformance-gate flip applies: it must state what it did NOT test, carry the pinned revision of every
  input (submodule revision, pack, corpus), and be re-derivable. A stale or unreproducible certificate
  is worse than none.
- **Thresholds are policy, not fact.** They will be argued about, and they should be: they belong in
  one declared place with a version, so a verdict can say which policy version produced it, and an
  older certificate remains interpretable after the numbers move.
- **Held-out corpora are a data dependency we may not have** for a given language. Where absent, the
  coverage check reports "not assessed" rather than passing by default — an unassessed check must never
  read as a passed one.
- The initial expected result is that few or no languages certify. That is a correct outcome for a bar
  set honestly, and the change should not be softened to produce cheerful output.
