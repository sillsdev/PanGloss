//! Part C (delanguaging) measurement tool: does a SYNTHETIC deep standalone-affix chain
//! (`pg_grammar_gen::build::chain`) reproduce the real-language OOM/`apply_up`-explosion anchor
//! documented in `docs/fst-plan/p6-deep-truncation-chain-report.md`, without needing the
//! gitignored Aweti corpus at all?
//!
//! ## What this probes and why
//! The report root-caused Aweti's pre-fix `apply_up` non-termination (and the historical
//! `EnumerationBudgetExceeded`-guarded 8.8GB allocation, `pg_foma::analyzer::FomaError`'s own
//! module doc) to `pg_foma::emit::build_deriv_chain`'s legacy `TextMode::SurfaceProbed` strategy:
//! "EVERY level offers EVERY rule; depth = rules.len()" for a grammar's STANDALONE
//! (stratum-attached, non-template) prefix/suffix rules. That strategy is what MAINLINE `emit()`
//! (used by every reference grammar via `FomaProposer::new`) still uses UNCONDITIONALLY today --
//! the shipped chain-restriction fix (dedicated-level-per-rule) applies ONLY under
//! `TextMode::UnderlyingTokens` (the P6/Aweti-templated path), never to this one. A grammar with
//! `N` independent standalone suffix rules therefore builds an `N`-level chain where each of the
//! `N` levels independently offers all `N` rules -- the SAME rule can be "chosen" at any of `N`
//! levels, so a single target surface string using `k` of the `N` rules (in order) is reachable
//! via `C(N, k)` distinct raw `apply_up` paths, all decoding to the identical candidate. This is
//! `pg_grammar_gen::build::chain`'s own reproduction of exactly that shape, sized only by `N`
//! (capped at 25 by `build::tables`' 26-ASCII-letter ceiling for a single table) -- no gitignored
//! corpus needed.
//!
//! Two questions, measured separately (never assumed):
//! 1. **Compile-time resource envelope** (states/arcs/lexc-lines/compile-wall-time) via
//!    `FomaProposer::new_with_profile` -- the mainline production path.
//! 2. **Apply-time behavior** on a deliberately-maximally-ambiguous query word (`root_shape` +
//!    every other rule's own suffix character, in order -- `k = N/2` rules used out of `N`,
//!    maximizing `C(N, k)`): (a) the UNBOUNDED `FomaProposer::propose` call, wall-clock timed on a
//!    background thread with a hard cutoff (this is deliberately allowed to time out -- that IS
//!    the measurement); (b) the SAME word through `FomaProposer::propose_budgeted` with a small
//!    `ApplyBudget` (ADR 0003's already-shipped apply-path containment), to check whether the
//!    existing honest-failure guard actually catches this specific vector fast.
//!
//! Run with `cargo run -p pg-foma --release --example deep_chain_scale_probe`. Prints one line per
//! `N` tried; if a later `N` would time out on the unbounded call, later `N`s are skipped rather
//! than blocking for a long time (see `UNBOUNDED_TIMEOUT`/`MAX_N_AFTER_TIMEOUT`).

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use pg_foma::analyzer::FomaProposer;
use pg_foma::compose_budget::{ApplyBudget, ApplyOutcome};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

/// Hard wall-clock cutoff for the UNBOUNDED `propose()` call -- this is the "did it explode"
/// signal itself, not a correctness bound. Chosen generously (well above the sub-10ms/word
/// production target, `docs/fst-plan/synthetic-stress-grammar-plan.md` §3's V8) so a clean pass
/// is unambiguous and a timeout is unambiguous too.
const UNBOUNDED_TIMEOUT: Duration = Duration::from_secs(10);

/// A small, deliberately tight apply-path cap for the honest-failure half of the probe -- mirrors
/// every other Phase C gate's "explicit caps, never env vars" convention.
const TIGHT_PATH_CAP: usize = 2_000;

fn recipe(n: usize) -> Recipe {
    Recipe {
        name: "deep-chain-scale-probe",
        seed: 20260725,
        scale: ScaleKnobs {
            segment_inventory: n + 1,
            ..ScaleKnobs::default()
        },
        construct: ConstructKnobs {
            table_count: 1,
            chain_rule_count: n,
            ..Default::default()
        },
    }
}

/// `C(n, k)`, saturating at `u128::MAX` rather than overflowing -- purely for the printed
/// "expected raw-path order of magnitude" context line, never used to gate behavior.
fn choose(n: u128, k: u128) -> u128 {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut result: u128 = 1;
    for i in 0..k {
        result = match result.checked_mul(n - i) {
            Some(v) => v / (i + 1),
            None => return u128::MAX,
        };
    }
    result
}

fn main() {
    println!("=== deep_chain_scale_probe: measuring whether a synthetic deep standalone-affix");
    println!("=== chain reproduces the real Aweti apply_up-explosion/OOM anchor ===\n");

    let mut any_timeout = false;

    for &n in &[2usize, 4, 8, 12, 16, 20, 24] {
        if any_timeout {
            println!(
                "N={n}: SKIPPED (a smaller N already timed out on the unbounded call; larger N \
                 can only be worse)"
            );
            continue;
        }

        let recipe = recipe(n);
        let rendered = pg_grammar_gen::render_indexed(&recipe);
        let g = pg_grammar::load(&rendered.xml)
            .unwrap_or_else(|e| panic!("N={n}: generated XML failed to load: {e}"));
        let chain = rendered
            .chain
            .as_ref()
            .expect("recipe declared chain_rule_count > 0");
        assert_eq!(chain.rule_xml_ids.len(), n);

        // --- (1) compile-time resource envelope, mainline production path. ---
        let (result, profile) = FomaProposer::new_with_profile(&g);
        let mut proposer = match result {
            Ok(p) => p,
            Err(e) => {
                println!(
                    "N={n}: COMPILE FAILED (itself a finding): {e} -- profile: {profile:?}"
                );
                continue;
            }
        };
        println!(
            "N={n}: compile — {:?} wall, lexc_lines={:?}, states={:?}, arcs={:?}",
            Duration::from_millis(profile.total_elapsed_millis),
            profile.total_lexc_lines,
            profile.final_state_count,
            profile.final_arc_count,
        );

        // --- (2) apply-time probe: a maximally raw-path-ambiguous query word. ---
        // Use every OTHER rule (k = n/2, in increasing index/document order) -- module doc's
        // C(n, k) reasoning; using every rule (k = n) collapses to exactly one placement (no
        // freedom left), so a deliberately partial subset is what maximizes ambiguity.
        let k = n / 2;
        let mut word = chain.root_shape.clone();
        // `build::chain`'s own suffix characters are table.segments[1..], one per rule, in the
        // SAME order `rule_xml_ids` lists them -- reconstruct the word from the loaded grammar's
        // own char table (not by re-deriving the builder's internal indexing) so this probe stays
        // correct even if `build::chain`'s internal segment layout ever changes.
        let table = &g.char_tables[0];
        let mut suffix_chars: Vec<char> = Vec::new();
        for (_, cd) in table.iter() {
            for rep in cd.representations() {
                if let Some(c) = rep.chars().next() {
                    if c != chain.root_shape.chars().next().unwrap() {
                        suffix_chars.push(c);
                    }
                }
            }
        }
        suffix_chars.sort_unstable();
        suffix_chars.truncate(n);
        for &c in suffix_chars.iter().take(k) {
            word.push(c);
        }
        let expected_paths = choose(n as u128, k as u128);
        println!(
            "  probe word {word:?} (root + {k} of {n} rule suffixes, in order) — C({n},{k}) = \
             {expected_paths} raw apply_up placements expected if the legacy chain construction's \
             own ambiguity mechanism is real"
        );

        // (2a) UNBOUNDED propose(), wall-clock timed on a background thread with a hard cutoff.
        // `FomaProposer` is not `Send` in a way that lets us share `proposer` across the join
        // safely if it times out (the handle stays borrowed by the stuck thread) -- accepted here:
        // this is a diagnostic probe, not production code, and the process exits at the end
        // regardless.
        let (tx, rx) = mpsc::channel();
        let word_for_thread = word.clone();
        let handle = thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                let t0 = Instant::now();
                let candidates = proposer.propose(&word_for_thread);
                let elapsed = t0.elapsed();
                let _ = tx.send((candidates.len(), elapsed, proposer));
            })
            .expect("spawn probe thread");

        match rx.recv_timeout(UNBOUNDED_TIMEOUT) {
            Ok((n_candidates, elapsed, mut proposer_back)) => {
                println!(
                    "  UNBOUNDED propose(): completed in {elapsed:?}, {n_candidates} candidate(s) \
                     — NOT reproduced at N={n} (finished well inside the {UNBOUNDED_TIMEOUT:?} cutoff)"
                );
                let _ = handle.join();

                // (2b) honest-failure half: the SAME word through the already-shipped ADR-0003
                // apply-path budget, with a tight cap.
                let budget = ApplyBudget::with_caps(Some(TIGHT_PATH_CAP), None);
                let t0 = Instant::now();
                let outcome = proposer_back.propose_budgeted(&word, &budget);
                let elapsed = t0.elapsed();
                match outcome {
                    ApplyOutcome::Complete(cands) => {
                        println!(
                            "  propose_budgeted(path_cap={TIGHT_PATH_CAP}): Complete in {elapsed:?}, \
                             {} candidate(s) -- raw path count never reached the cap at N={n}",
                            cands.len()
                        );
                    }
                    ApplyOutcome::Incomplete {
                        dimension,
                        value,
                        limit,
                    } => {
                        println!(
                            "  propose_budgeted(path_cap={TIGHT_PATH_CAP}): Incomplete in {elapsed:?} \
                             — {}={value} (limit {limit}) — the ADR-0003 guard DID trip on this \
                             vector, fast and honestly",
                            dimension.label()
                        );
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                println!(
                    "  UNBOUNDED propose(): DID NOT COMPLETE within {UNBOUNDED_TIMEOUT:?} — \
                     REPRODUCED at N={n} (the historical apply_up non-termination anchor)"
                );
                any_timeout = true;
                // Deliberately leak/detach: the thread is still running against the moved
                // `proposer`+`tx`; there is no safe cancellation, matching this crate's own
                // documented stance elsewhere that a native thread cannot be hard-killed.
                drop(handle);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                println!("  UNBOUNDED propose(): probe thread panicked");
            }
        }
        println!();
    }

    println!("=== done ===");
}
