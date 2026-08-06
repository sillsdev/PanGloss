# `n2_realize_gate.rs`: pipeline, pinning discipline, and the subsampled corpus sweep

`pg-realize/tests/n2_realize_gate.rs` is the end-to-end demo: real sample-grammar words parsed
through `Morpher` -> `pg_realize::gloss_bundle` -> `pg_realize::to_ir` ->
`pg_realize::TableRealizer::realize`, plus an all-corpora robustness sweep.

## Conventions shared with the other gate-tier tests

Same self-skip discipline as the earlier gate tiers: every real-grammar test no-ops when the
grammar XML is absent on disk, and every test in this file loads a real grammar (the robustness
sweep also loads corpus word lists), so all are unconditionally `#[ignore]`d — the self-skip keeps
`--include-ignored` green when the fixture is absent, and the default local test run stays under
about a minute without depending on the gitignored real-language corpus at all.

Every pinned `eng:` string was obtained by first running `pg-cli parse <grammar> <word> --gloss
--natural-gloss=eng` to see the actual output, then writing the expected assertion — the pinned
value is a recorded observation, never a derived expectation.

## The subsampled full-corpus sweep

`realize_never_panics_on_a_subsampled_full_corpus_sweep` parses every word in all three
`samples/data/*-words.txt` corpora with a step-capped, wall-clock-bounded `Morpher`, realizes every
surviving analysis through both the real sidecar (or an empty one) and asserts: no panic,
non-empty realized text, and that the parity signature computed right after parsing is unchanged
by any `gloss_bundle`/`to_ir`/`realize` call for that word — this crate's functions only ever read
`&ParseOutcome`/`&Grammar`/`&RealizeMap`, never mutate, so this must hold.

Even with a step cap and a short per-word timeout, most individual words in the full Sena and
Amharic corpora individually time out at that deadline in an unoptimized build, and the full
sweep measured well over the target time-box. So the test samples every 3rd Amharic word and every
10th Sena word (Indonesian's corpus is small enough to run in full) — a deterministic subsample
that keeps the whole sweep comfortably under a minute.
