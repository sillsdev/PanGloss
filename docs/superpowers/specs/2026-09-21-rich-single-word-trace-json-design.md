# Minimal single-word trace JSON

PanGloss exposes a small, opt-in JSON envelope for the data needed by a Try-a-Word-style consumer. It reuses the existing trace tree and per-word statistics instead of adding parser instrumentation.

## Interface

```text
pangloss parse <grammar> <word> --trace --trace-format=json --trace-details
```

`parse` already accepts exactly one word. `--trace-details` is valid only with `--trace` and JSON format. It is absent from `batch`. `--gloss` and `--natural-gloss` are rejected in this mode so the trace destination remains a single JSON document. With no `--trace-details`, parse and trace output follow the existing paths unchanged.

## Output

The versioned `pangloss.trace-details.v1` envelope contains:

- the input `word`;
- authoritative search state from `ParseOutcome`: completion, cap, timeout, invalid-shape, step count, and whole-run elapsed nanoseconds;
- the parse signature, guessed status, and successful `(morphemes, surface)` analyses;
- existing `StatsCollector` counters grouped by `ObjectKind`;
- the existing compact JSON trace tree under `trace`, or `null` if parsing ended before a root was created.

Category timing is the sum of existing nesting-aware `self_time_ns` rows. `self_time_supported` remains the authority for whether a category is measured. Unsupported categories report `timingAvailable: false` and `selfElapsedNs: null`; a numeric zero is therefore never used to imply missing instrumentation was measured.

## Boundaries

The feature does not add trace fields, typed failure payloads, word snapshots, per-node timers, or a second trace renderer. It does not infer parser decisions. Trace, outcome, and stats come from one call to `parse_word_traced_with_stats`, whose search has the same unmerged behavior as ordinary tracing. Overall elapsed time is measured only on the opt-in branch.

Motif integration and presentation are outside this change.

## Verification

Tests cover flag validation, the envelope and embedded tree, counter aggregation, explicit unsupported timing, and equivalence between the combined run and ordinary traced parsing. Managed checks cover `pg-parse` and `pg-cli`; the existing compact renderer remains unchanged.