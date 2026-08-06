## Why

A grammar author has no way to see how their language is actually handled. The compiler makes real
structural decisions — which strata become which nodes, which rules join a replacement cascade, which
partition groups a gate splits on, which route a circumfix or a reduplicant takes — and all of it is
invisible. When a grammar is slow, or a construct is refused, or an analysis is missing, the author
has no picture to reason against.

The reified compilation plan already exists and is exactly the right artifact to render.
`reify-compilation-plans` (ADR 0002) turned compilation into an explicit AND-OR DAG with five closed
node kinds, and every node's identity is a content address over its own semantic content. So a
diagram of a plan is stable across runs, diffable between grammar revisions, and faithful by
construction rather than a hand-drawn approximation.

## What Changes

- Serialize a `Plan` to JSON — a stable, documented shape, not a debug dump.
- Render that JSON as a mermaid graph, so it displays anywhere markdown does.
- Label nodes by **the linguistic work they do**, not only by node kind: which stratum, which
  template, which rule class, which construct. The question being answered is "how is my language
  handled", not "what does the compiler's IR look like".
- Summarize rather than explode on large grammars: a plan over a realistic lexicon must render
  something a human can read, with collapsing and counts where detail would be noise.
- Surface the capability outcome per node where one applies, so a refused construct is visible in the
  picture rather than only in a diagnostic.

## Impact

Read-only and additive. No compile path changes; the renderer consumes a `Plan` the compiler already
builds. Nothing about admission, refusal, or the propose-and-confirm contract is affected.

Feeds `certify-language-readiness`'s per-language report, which embeds the diagram.
