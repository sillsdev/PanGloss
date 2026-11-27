# The Motif contract

Motif is the one pinned consumer. It lives in a separate repository; paths below are in that repo.

## Where Motif pins

- `src/SIL.Motif.Host/PanGloss/PanGlossInterfaceVersions.cs`: `SourceTag` (a PanGloss tag; `pangloss-release.json` `interfaces.sourceTag` is the other place; read the current value there),
  `TraceDetailsSchemaId` (`pangloss.trace-details.v5`), grammar-health schema 5, facts schema 9, batch-TSV column positions.
- `PanGlossRequest.cs` and `WarmPanGlossPool.cs`: the argv and serve requests it sends (serve `parse` only, with `traceDetails`).
- `PanGlossSurface` probes `--describe`; a missing key means the op is absent.
- `FakePanGloss`: a test double "matching the pinned binary" (Motif ADR 0019), with its own surface test. A third copy of
  the truth; it can drift from both PanGloss and the constants.
- The Motif build fails when the bundled parser or any of its formats does not match, and GrammarCheckQuery refuses a mismatch.

Read these before judging whether a change is safe. Motif lands on Windows: never read Motif's pins or caller data from the
WSL clone, which goes stale (the first API review did, and got callers and the pin wrong). Ask the motif-cli lead, or read the
Windows clone (`herdr-x run win`). Motif is under an owner order of no legacy and a tight API (sillsdev/motif#572).

## What is frozen

Anything Motif parses keeps its shape and its default bytes: batch TSV 5, trace-details v5, `pangloss.diagnostic.v1`,
grammar-health 5, facts 9, stats cache 13, the serve `parse` result, `--describe`. Only additive keys until the pin moves.

## The near-miss (#521)

The first draft of #521 made trace-details v6 the default. The LAND review caught it, and the merged version made v6 opt-in
with the default byte-identical v5. It shipped no break. The lesson: no test compares the shipped binary's default output
bytes against the version Motif pins, so only a reviewer stood between the draft and a break.

## The contract test (proposed shape)

- A directory of fixtures for each PanGloss call Motif makes (argv or serve request): the minimal expected response shape
  (JSON Schema open for objects, closed for version fields) plus the format ids pinned.
- A PanGloss CI job builds `pangloss`, runs each request on a tiny synthetic grammar, validates the response, asserts
  `--describe` lists every flag, op and field in the contract, and asserts default bytes of the pinned formats equal Motif's golden.
- Motif CI validates FakePanGloss against the same files.
- A PanGloss PR that must change a contract file is a breaking PR: Motif issue linked, Motif pin move linked.
- Open: where the fixtures live (Motif repo, PanGloss repo, release asset); owner decision.
