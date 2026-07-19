//! Batch parsing across words (plan §7): the engine is single-threaded *per word* (the M6 memo
//! made within-word parallelism obsolete — no Rust descendant of C#'s
//! `ParallelCombinationRuleCascade`), so all parallelism here is **across** words, via a `rayon`
//! scoped thread pool sized by an explicit `max_threads` (not just rayon's global default — the
//! M9 benchmark matrix needs to run at specific thread counts deliberately).
//!
//! This lives in `pg-parse` (not `pg-cli`) because `pg-parse`'s own module doc says it is the
//! only crate `pg-ffi` and `pg-cli` call — `pg-ffi`'s `hc_parse_batch` FFI entry point (plan
//! §4.2, M8) needs the identical batch-parallelism engine, not a second copy bolted onto the CLI.
//!
//! ## Stack size
//! The analysis cascade recurses to the depth of a word's unapplication chain, deep enough to
//! overflow a default-size thread stack on heavy words (`pg-cli`'s `main()` already works around
//! this on the *main* thread with a 1 GiB worker stack). Rayon pool threads do **not** inherit
//! that; this module configures the pool itself with the same stack size, or heavy words that
//! didn't crash sequentially would crash under `hc_parse_batch`.
//!
//! ## Determinism
//! `Morpher::parse_word` takes `&self` with no interior mutability shared across calls (its only
//! `RefCell`, the M6 memo scope, is created fresh inside each call) — per-word computation shares
//! nothing mutable, so results are byte-identical regardless of `max_threads` or completion order
//! (plan §7, §8 layer 3; enforced by `pg-parse/tests/batch_determinism.rs`).
//!
//! ## Scheduling
//! Words are dispatched **longest-surface-first** into rayon's work-stealing queue (plan §7,
//! Phase 8a: cut packing slack 2.9× → 1.36× in the C# work) — a scheduling optimization (start
//! the slow/heavy words first so they don't straggle at the tail), not a correctness requirement.
//! Results are written into a pre-sized `Vec<Option<_>>` — each slot touched by exactly one
//! parallel task, no locking — then reindexed to original position in a final sequential pass, so
//! output order is independent of both completion order and thread count.

use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::morpher::{Morpher, ParseOutcome};

/// Stack size for each rayon pool worker thread. Matches `pg-cli::main`'s main-thread worker
/// stack (`src/main.rs`) — the same recursion depth risk applies to pool threads.
const WORKER_STACK_SIZE: usize = 1 << 30; // 1 GiB

/// A magic sentinel word that panics **only** when this crate is built with the
/// `test-panic-hook` feature (off by default — see `Cargo.toml`). Its sole consumer is
/// `pg-ffi`'s abort-safety test (plan §8 layer 7, M8), which needs a real panic raised *inside a
/// rayon worker thread, on this exact production call path* — not a lookalike loop bolted onto
/// the test binary — to prove `catch_unwind` at the FFI boundary actually catches an unwind that
/// crosses a rayon `par_iter` join point. Embedded NUL bytes make it un-typeable by accident and
/// impossible to collide with any real corpus word.
#[cfg(feature = "test-panic-hook")]
pub const TEST_PANIC_WORD: &str = "\u{0}__hc_test_panic_hook__\u{0}";

/// One word's batch outcome: the parse result plus wall-clock time spent on it (the
/// `BatchCommand` TSV's `elapsedMs` column).
pub struct BatchWordOutcome {
    pub outcome: ParseOutcome,
    pub elapsed: Duration,
}

/// Parse every word in `words` against `morpher`, using up to `max_threads` rayon worker threads
/// (`0` = rayon's default, i.e. `std::thread::available_parallelism()`).
///
/// Returns one [`BatchWordOutcome`] per input word, in the **same order as `words`** —
/// independent of dispatch/completion order or `max_threads` (see module docs).
///
/// # Panics
/// Panics if the rayon pool fails to build (e.g. `max_threads` combined with OS thread limits) —
/// this mirrors `pg-cli`'s existing `.expect("spawn worker")` treatment of the equivalent
/// main-thread setup failure: an environment problem, not a recoverable parse-time error.
pub fn hc_parse_batch(
    morpher: &Morpher,
    words: &[String],
    max_threads: usize,
) -> Vec<BatchWordOutcome> {
    let n = words.len();
    if n == 0 {
        return Vec::new();
    }

    // Longest-surface-first dispatch order (plan §7 / Phase 8a). Sorting *indices* (not the words
    // themselves) is what lets the final result carry original position independent of this order.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(words[i].chars().count()));

    let mut pool_builder = rayon::ThreadPoolBuilder::new().stack_size(WORKER_STACK_SIZE);
    if max_threads > 0 {
        pool_builder = pool_builder.num_threads(max_threads);
    }
    let pool = pool_builder
        .build()
        .expect("build rayon pool for hc_parse_batch");

    // Phase 1 (parallel): each task writes exactly one slot of `scratch`, indexed by its rank in
    // the dispatch order — a disjoint mutable borrow via `zip`, no unsafe, no locking.
    let mut scratch: Vec<Option<BatchWordOutcome>> = (0..n).map(|_| None).collect();
    pool.install(|| {
        order
            .par_iter()
            .zip(scratch.par_iter_mut())
            .for_each(|(&orig_idx, slot)| {
                #[cfg(feature = "test-panic-hook")]
                if words[orig_idx] == TEST_PANIC_WORD {
                    panic!(
                        "hc_parse_batch test-panic-hook: deliberate panic for abort-safety test"
                    );
                }
                let start = Instant::now();
                let outcome = morpher.parse_word(&words[orig_idx]);
                let elapsed = start.elapsed();
                *slot = Some(BatchWordOutcome { outcome, elapsed });
            });
    });

    // Phase 2 (sequential, cheap): reindex rank-order scratch back to original word order.
    let mut out: Vec<Option<BatchWordOutcome>> = (0..n).map(|_| None).collect();
    for (rank, &orig_idx) in order.iter().enumerate() {
        out[orig_idx] = scratch[rank].take();
    }
    out.into_iter()
        .enumerate()
        .map(|(i, s)| s.unwrap_or_else(|| panic!("hc_parse_batch: index {i} produced no result")))
        .collect()
}
