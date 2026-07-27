## 1. Timing in the conformance suite (the measurement layer)

- [ ] 1.1 Per-word timing in the synthetic-language conformance suite, in both engine modes (complete
      Rust HermitCrab, and compiled proposer + confirm)
- [ ] 1.2 Resolve the measurement floor: either finer-than-millisecond timing at the measurement site,
      or a below-floor indicator plus the stated floor. **Never emit `0`** — that is a precision claim
      the current path cannot support (see `docs/benchmark-matrix.md`)
- [ ] 1.3 Record a refusal as its own outcome naming the refusing predicate — not a zero, not an
      omitted row. Many fixtures' grammars ARE refused on the compiled path, so this is the common
      case, not an edge case
- [ ] 1.4 CSV emission plus a markdown-table rendering, grouped so speedup is attributable **per
      construct/typology**, which is the question worth answering
- [ ] 1.5 A runnable script, not a sequence of manual commands — the hand-run precedent in
      `docs/benchmark-matrix.md` is what this replaces

## 2. Threshold policy

- [ ] 2.1 One declared, **versioned** place for thresholds (pack size, lexicon scale, token analysis
      rate, p50/p90/p99), so a verdict can cite the policy version that produced it and older
      certificates stay interpretable after the numbers move
- [ ] 2.2 Name the target **device class** for latency thresholds; no silent generalization beyond it
- [ ] 2.3 Seed initial values from measured evidence where it exists and mark the rest explicitly as
      un-calibrated placeholders — same discipline as `calibrate-fst-resource-envelopes`' eight
      documented placeholders, never a number invented to look authoritative

## 3. Certification verdict

- [ ] 3.1 Tiered verdict, with **not-yet** (thresholds missed, actionable by the language team) kept
      distinct from **not-supported** (a refused construct, actionable only by compiler work)
- [ ] 3.2 The not-supported tier names the refusing predicate and construct from the **real** capability
      evaluation, never inferred from a failure to run
- [ ] 3.3 **An override-trusted (`trust=unproven`) artifact never certifies**, under any configuration,
      and the report says the override is why. This is the rule whose violation would be most
      convenient, so gate it explicitly and test it
- [ ] 3.4 Held-out corpus status recorded as an **attestation** (attestor + date + stated as unverified);
      absent corpus reports **not-assessed**, and not-assessed must never render as passed
- [ ] 3.5 Coverage worded as a **token-level analysis rate**, with the report stating that a token may
      receive an incorrect analysis and still count
- [ ] 3.6 Every failed check reported with measured value and threshold

## 4. `pangloss make-report` (composition layer — lands last)

- [ ] 4.1 One command → one markdown file: build time, artifact size, latency percentiles, the
      compilation-plan mermaid diagram (`visualize-compilation-plan`), conformance verdict
- [ ] 4.2 Passing case says so plainly; failing case **names each failing point** — a bare "not
      passing" is useless to someone deciding whether to ask for support
- [ ] 4.3 State what was **not** tested, and record pinned revisions of grammar, pack, corpus, and
      submodule sufficient to re-derive the report
- [ ] 4.4 Run it against the reference grammars and publish the result. Expected outcome per
      `docs/benchmark-matrix.md`: **none certifies today** — all three are refused on the compiled path,
      two by a permanent carve-out. If the report says otherwise, the report is wrong

## 5. Verification

- [ ] 5.1 A test that an unproven-trust pack cannot certify (3.3), proven non-vacuous by sabotage
- [ ] 5.2 A test that not-assessed coverage does not render as passed (3.4)
- [ ] 5.3 A test that the not-supported tier cites a real predicate refusal on a grammar known to carry
      a permanently carved-out construct
- [ ] 5.4 Golden report for one small synthetic fixture, regenerated from the generator's own output
