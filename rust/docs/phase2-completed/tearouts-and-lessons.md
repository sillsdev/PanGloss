# Phase 2 — byte-parity tear-outs (T-A/T-B/T-C) and methodology lessons

**Gating decision (John, 2026-07-09):** the parity target is **parse-set parity** — each word
gets the same *set* of analyses — not byte-identical signatures across runs/threads/step-cap
truncation. Assess with `rust/tools/parse_compare.py` (IDENTICAL / MULTISET_EQUAL / SET_EQUAL all
count as "right answer"). This relaxed the phase-1 "byte-identical regardless of thread count"
invariant; three pieces of machinery existed only to satisfy the stricter one and were evaluated
for removal. Outcomes:

## T-A — `fixed_hash` module: TORN OUT (commit `9cf31048`)

`crates/{pg-parse,pg-rules}/src/fixed_hash.rs` aliased HashMap/HashSet to a fixed-seed
DefaultHasher across 8 files, solely so **step-capped** words were byte-reproducible across
process invocations (its own doc said uncapped results are order-independent). Under parse-set
parity a capped word is already a truncated answer; pinning the order of its incompleteness buys
nothing. Replaced with `rustc-hash` FxHashMap/FxHashSet — which is both faster on the
WordKey-keyed accumulators AND still deterministic (FxHasher has no random seed), so
run-to-run reproducibility survived as a free side effect. Deleted both modules and the "swap
uniformly or the gap regresses" maintenance constraint.

## T-B — `result_signature` duplicate retention: REJECTED, kept (same commit)

Flagged as byte-parity-only (Sena `mbali` renders the same string 9×; assumed cosmetic
C#-matching). The attempted dedup broke `sena_free_fluctuation_gate.rs`, which counts 2 identical
rendered sub-strings as *distinct recovered analyses*. Checked C#'s own gold references directly:
`parity-out/golden/{master,parse-opt}/sena-*.tsv` independently reproduce the identical 9×/6×
duplicate pattern. **The duplication is real signal, not an artifact**: free-fluctuating /
combinatorially-distinct allomorph choices can render to an identical string while remaining
separate analyses, and their count tracks recovery progress against gold. Reverted; regression
test `identical_rendered_signatures_are_kept_not_deduped` + expanded doc comment on
`result_signature` record why.

## T-C — FST-traversal priority trail + `ResultCompare` tiebreak: TORN OUT (commit `6642a29d`)

Removed `FstResult.priorities`, `Inst.priorities`, the arc-priority push in `advance()`, and the
zip tiebreak in `result_compare()` from `crates/pg-fst/src/traverse.rs`, plus their dedicated
unit tests. Verified the hard way (below); zero difference at any forced-truncation level on any
of the three grammars — byte-identical, not merely set-equal.

**Kept (semantic/perf, not byte-parity):** `morpheme_ids_in_order`'s `sort_by_key(|m| m.order)`,
`batch.rs` longest-word-first ordering, the FST `deterministic` flag (a real DFA-vs-NFA
property).

## Methodology lessons (apply to ALL future verification, not just tear-outs)

1. **Forced-truncation verification.** T-C's first pass ran Indonesian/Amharic at their natural
   step-cap and saw "byte-identical before/after" — vacuous, because 0 words hit the cap, so the
   code path being removed never executed (advisor review caught it). Redone by forcing
   truncation with artificially low `--step-cap` (Amharic @200 → 18/669 capped, @50 → all 669;
   Indonesian @30 → 71/121; bounded 1-thread Sena probe @2000 → 14/15) and diffing at each level.
   **Any "should be invisible" claim needs the code path demonstrably exercised, not a
   natural-conditions check.**
2. **"Assumed byte-parity-only" is a hypothesis, not a fact.** T-B looked as safe as T-A and was
   wrong; only the C#-gold cross-check caught it. Verify against the oracle's own output before
   removing anything justified by "the reference doesn't care."
3. **Unattended-run safety.** A stray background Sena run (full corpus, step-cap 500000 ≈
   uncapped, threads=20, no watchdog) grew to ~56GB RSS across 3 processes, leaving ~2.6GB free
   system memory before being caught — same shape as the phase-8b "45GB Server-GC incident" on
   the C# side. **Rule: any large-corpus run gets a bounded word list, low thread count, a
   step-cap, AND a watchdog wrapper (`rust/tools/run-sena-rust.ps1` pattern); never leave one
   unattended without a memory monitor.**
4. **Shared-root-cause hunting.** Wave 3's char_def staleness fix flipped four "independent"
   diverging fixtures at once. When several fixtures diverge in the same region, suspect one
   cause before scheduling four fixes.
5. **Watchdog scripts must read the right stream.** `pg-cli` emits "batch complete" and panics
   via `eprintln!` (stderr); the first wrapper draft checked stdout and both its completion and
   panic detection were silently useless. Smoke-test watchdogs on a happy path AND a forced
   failure before trusting them with a 10-hour run.
