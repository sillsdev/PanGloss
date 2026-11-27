# Formats — written for outside readers

This directory documents the file formats PanGloss produces, for readers outside this repository:
a linguist who has never touched PanGloss's own code, or a chat model reading a Motif Handoff
folder someone dragged in front of it. Every document here explains a format on its own terms —
what the fields mean, what produced them, how to reason about what you're looking at — without
assuming you have this repository open or know its internal vocabulary.

- [`grammar-format.md`](grammar-format.md) — the grammar snapshot (`grammar.json`): every rule,
  part of speech, phoneme, and lexicon entry PanGloss compiled from a FieldWorks project.
- [`hc-mechanics.md`](hc-mechanics.md) — how the HermitCrab engine actually parses a word against
  that grammar, and the ways a grammar author's intent can go wrong without an error message.
- [`trace-format.md`](trace-format.md) — the JSON `pangloss parse --trace` writes: the full,
  unmerged derivation tree behind one word's parse or failure.
- [`rejection-trace-v1.md`](rejection-trace-v1.md) covers bounded rejection observations from ordinary HC parse and batch.
- [`why-not.md`](why-not.md) — `pangloss why-not` (`pangloss.whynot.v1`): parse one word along given morph candidates and name the first failing constraint. Slice 0: the contract, answering `unknown`.
- [`batch-tsv.md`](batch-tsv.md) — the per-word result rows `pangloss batch` writes and, with
  `--work-columns`, each word's deterministic parser work (`work_steps`).
- [`fst-verify.md`](fst-verify.md) — per-word FST versus HC-Rust analysis-set comparison, incomplete outcomes and provenance.
- [`stats-cache.md`](stats-cache.md) — feature-gate counters, authored identities, measurement state, and replay.
- [`serve-protocol.md`](serve-protocol.md) — the line-delimited JSON protocol of `pangloss serve`.
- [`INTERFACES.md`](INTERFACES.md) — the one inventory: every command, serve op, format, env var and WASM item, with versions and known callers.
- The formats that live one level up in `docs/` rather than here: [`generation.md`](../generation.md) (generate request),
  [`retire-format.md`](../retire-format.md) (`pangloss-retire/v1`), [`parse-analysis-format.md`](../parse-analysis-format.md),
  [`grammar-diagnostics.md`](../grammar-diagnostics.md) (grammar-health), [`text-analysis-format.md`](../text-analysis-format.md),
  [`snapshot-format.md`](../snapshot-format.md).
- [`trace-details.md`](trace-details.md) — the opt-in one-word diagnostic envelope: recorded analyses, morph metadata, rejection context, provenance, and aggregate timing.

This is **not** the same audience, and not the same material, as `../research/`, `../history/`,
and `../divergences/`. Those three are written for people porting or maintaining HermitCrab itself
— they cite Rust file and line numbers, track measured performance, and record where this port is
known or suspected to disagree with the C# original it was built from. A linguist's chat model has
no use for a divergence census or a build-order design note; sending it there instead of here is
how a reader ends up drowning in port-audit material when what they needed was "what does this
field mean."
