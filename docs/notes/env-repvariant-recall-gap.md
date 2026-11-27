# env-repvariant: Aweti/Mbugwe recall-gap census (archived)

Status: archived. The branch `fix/env-repvariant` is retired; its tip is kept as local tag `archive/env-repvariant`. Refs #40.

## What was measured

Two measurement-only additions to pg-foma, both `#[ignore]`d and corpus-gated (no production behaviour changed):

- **Overflow census** (`emit::rep_variant_overflow_census`, commit bf78f396, `rust/crates/pg-foma/src/emit.rs`).
  Per grammar: root allomorphs past `REP_VARIANT_CAP`, whether the overflow is a finite chained-segment
  product or an unbounded iterative/class node, and the analytical needed-variant count.
- **Recall-gap census** (`examples/aweti_mbugwe_rep_variant_recall_gap.rs`, commits bf78f396, e701ccce,
  49f0aa3d, 63b3a7ff, 8cd25436). Oracle-vs-propose recall: for each oracle-analyzed word, does
  `FomaProposer::propose` return zero candidates? Gap = `fst_zero_when_oracle_ok / oracle_analyzed`.
  Final form: a plain example binary (run with `pg.ps1 -Mode run -Example aweti_mbugwe_rep_variant_recall_gap`),
  200-word subsample per grammar, 5 s per-word timeout, per-word progress lines.

## Key numbers

- **No result from the branch is recorded.** No commit carries a measured gap figure. The runs failed to finish:
  - the 200-word / 8 s version ran 25+ minutes with no output;
  - the 200-word / 5 s version under nextest hit the 10-minute kill on both grammars (Aweti at 603 s, Mbugwe at 600 s);
  - the 100-word / 3 s version and then the example form were not recorded as completed.
- The measurement that did settle the question lives on `main`, in `docs/research/grill-me-queue.md` (G7),
  from `root_variant_census` over the reference grammars:

  | grammar | roots | overflow at 64 | unbounded | worst finite |
  |---|---|---|---|---|
  | Aweti | 923 | 105 | 0 | 4096 |
  | Mbugwe | 165 | 3 | 0 | 256 |

  Zero unbounded shapes in any real grammar. Every overflowing root is a finite product of per-segment spellings.

## The aweti_mbugwe example

The example prints, per grammar, the sampled count, invalid-shape and timed-out words, `oracle_no_parse`,
`oracle_analyzed`, `fst_zero_candidates_when_oracle_analyzed`, and up to 15 example words with zero FST
candidates despite an oracle analysis. The example words were never captured, so no word-level finding is
available from this branch.
## Stale against REP_VARIANT_WARN_THRESHOLD

The branch assumes `REP_VARIANT_CAP = 64` and that overflow drops spellings. `main` no longer works that way:

- G7 (`docs/research/grill-me-queue.md`) raised the cap from 64 to 8192.
- `main` removed the count cap. `emit.rs` now emits the full Cartesian product. Breadth above
  `REP_VARIANT_WARN_THRESHOLD` (8192, `pg-foma-runtime/src/emission_support.rs`) is advisory only.
- Only `REP_VARIANT_BYTE_BUDGET` or an unbounded `*` shape can now leave spellings out, and each is a separate `VariantLimit`.

So "overflow count" and "recall gap from the cap" are no longer the live question. The census would need rework
against the advisory threshold before it could measure anything on `main`.

## Commits (for reference)

- bf78f396 test(pg-foma): measure REP_VARIANT_CAP overflow and recall gap for Aweti/Mbugwe
- e701ccce style(pg-foma): shorten recall-gap census comments to hold the comment-hygiene baseline
- 49f0aa3d fix(pg-foma): bound and instrument the Aweti/Mbugwe recall-gap census
- 63b3a7ff fix(pg-foma): shrink the recall-gap census further to clear nextest's 10-minute kill
- 8cd25436 fix(pg-foma): move recall-gap census from a nextest test to a plain example (branch tip)
