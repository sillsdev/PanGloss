## Why

PanGloss reports two per-word counters, and neither can yet be trusted as an up/down signal.

`steps` (analysis attempts) is a poor cost measure. On five real grammars, instructions per step span
52x (p1 to p99), and per-step cost has differed by about 570x between grammars. The word "steps" also
means three different things in the code: attempts, the work total, and oracle ticks.

`work_steps` is much better: instructions per unit span 2.2x, R² against instructions is 0.93-0.996
per grammar, and one per-grammar constant predicts a word's instructions to a median 3-14%. But it
is blind to uncharged work. `Word::clone`, `dedup_key`, hashing, sorting and FST compilation under
one unit all cost nothing, and `pg-featstruct` and `pg-shape` have no reservation sites. History has
the resulting failures: identical steps with a 2.7x speedup, and fewer steps with 33% more time.

Cycles cannot replace these counters. Parses depend on the cap, so its unit must be exact on every
machine. Unprivileged Windows has no core-cycle counter, and core cycles repeat only to 5-17% per
word on a loaded host. Instructions repeat to 0.15-0.40%, which makes them the ground truth to
calibrate against, not the unit a host can compute.

## What Changes

- **Measurement**: instructions become the ground truth for parser cost decisions, with a repeatable
  tool on Windows and on Linux, and a stated noise floor.
- **Accuracy**: `work_steps` charges the work it currently hides, in proportion to size, so a real
  cost change moves it in the right direction. Report-only first; charged only after evidence.
- **Calibration**: a stated acceptance test defines "reliable": the sign of a change in `work_steps`
  agrees with the sign of the change in instructions, over a maintained set of before/after pairs.
- **Retirement (BREAKING)**: `steps` leaves the product entirely: `--step-cap`, the attempt cap,
  `ParseOutcome.steps`, the batch TSV column, serve `steps`, the `STEPS` line, oracle step ticks and
  `Score.confirmation_steps`. `work_steps` is the one work unit.
- **Host signal**: hosts such as Motif compare total work on a fixed word list, at a fixed PanGloss
  version, under `--work-cap` only.
- **Landed first, with no parse effect**: `SynthesisBudget::steps()` is renamed to `work_steps()`,
  and `WorkBudget::consume` stops reading the clock on every unit (about 90 instructions per unit,
  3.5-4% of parsing when a deadline is set).

## Capabilities

### New Capabilities

- `work-measurement`: the ground truth, noise floor, acceptance test and host comparison rules.

### Modified Capabilities

- None published: the existing limits are described by `docs/research/per-word-search-work-cap.md`
  and divergence 052, both of which this change updates.

## Impact

- Parser limits: `pg-fst/src/work.rs`, `pg-rules/src/stratum.rs`, `pg-parse/src/morpher.rs`.
- Size charges: `pg-fst/src/traverse.rs`, `pg-rules/src/morph.rs`, `pg-parse/src/root_trie.rs`, and
  word, shape and feature-structure copies.
- Ranking: `pg-foma-backend/src/backend_optimizer.rs`, `backend_runtime.rs`,
  `pg-foma-runtime/src/confirm.rs`.
- Contracts: `docs/formats/batch-tsv.md` (`interfaces.batchTsv`), `docs/formats/serve-protocol.md`,
  `pg-stats` schema, `SEARCH_BUDGET_SEMANTICS`, FFI and WASM outcome fields.
- Hosts: Motif and any `--step-cap` caller.

## Dependencies

Independent of the changes in `openspec/changes/STAGING.md`: it does not touch `emit.rs`,
`replace.rs` or `gate.rs`. It owns `pg-fst/src/work.rs` while active.

This change makes support and observation claims only. It makes no recall or certification claim.
A capped or timed-out result stays incomplete and non-certifying at every phase.
