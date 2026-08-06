//! Demonstrates that a synthetic deep standalone-affix chain reproduces the real `apply_up`
//! explosion this shape triggers; see `docs/research/pg-foma-deep-chain-scale-probe.md`.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use pg_foma::analyzer::FomaProposer;
use pg_foma::compose_budget::{ApplyBudget, ApplyOutcome};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

/// Hard wall-clock cutoff for the unbounded `propose()` call -- the "did it explode" signal itself, not a correctness bound.
const UNBOUNDED_TIMEOUT: Duration = Duration::from_secs(10);

/// A small, deliberately tight apply-path cap for the honest-failure half of the probe.
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

/// `C(n, k)`, saturating at `u128::MAX` rather than overflowing -- for the printed context line only, never used to gate behavior.
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
                println!("N={n}: COMPILE FAILED (itself a finding): {e} -- profile: {profile:?}");
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

        // Apply-time probe: uses every other rule (k = n/2) rather than all n, since using all n collapses to exactly one placement with no freedom left to maximize ambiguity.
        let k = n / 2;
        let mut word = chain.root_shape.clone();
        // Reconstructs the suffix chars from the loaded grammar's own char table, not by re-deriving the builder's internal indexing, so this probe stays correct if `build::chain`'s segment layout changes.
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

        // `FomaProposer` isn't `Send` in a way that lets the join share `proposer` if it times out; accepted since this is a diagnostic probe, not production code.
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

                // Honest-failure half: the same word through the already-shipped apply-path budget, with a tight cap.
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
                // Deliberately leak/detach: the thread still holds the moved `proposer`+`tx`, and a native thread cannot be hard-killed.
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
