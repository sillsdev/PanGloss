//! Full backend scoreboard (`pg.ps1 -Mode run -Example conf_matrix`): a printer over
//! `pg_foma::scoreboard`'s typed per-(grammar, words) measurement. See docs/research/backend-measurement-instruments.md.

use pg_conformance_fixtures::discover;
use pg_foma::scoreboard::{self, CellOutcome, ScoredFixture, MAX_WORDS_PER_FIXTURE};
use pg_foma::strategy_coverage::ALL_STRATEGIES;

/// A fixture excluded by name (`expect_crash: true`), distinct from one scored zero backends.
struct Excluded {
    label: String,
    reason: String,
}

/// A [`ScoredFixture`] plus the caller-owned `words.yaml` metadata it no longer carries.
struct Row {
    scored: ScoredFixture,
    total_words: usize,
    subsampled: bool,
    expect_fail_count: usize,
    expect_skip_count: usize,
}

fn main() {
    // `discover` panics unless a run claims a scope; `all` reaches both fixture roots.
    std::env::set_var("PANGLOSS_CONFORMANCE_SCOPE", "all");

    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "<git rev-parse HEAD failed -- read it separately>".to_string());
    println!("commit under measurement: {commit}");
    println!(
        "scope: PANGLOSS_CONFORMANCE_SCOPE=all (conformance-staging/** + machine/conformance/**)"
    );
    println!(
        "strategies measured: {:?} -- capability selector BYPASSED (LoweredCandidate constructed \
         directly, evaluate_plans_observed_with_cache never calls select_backends), so a backend \
         `select_backends` would refuse ahead-of-time is still compiled and measured here",
        ALL_STRATEGIES
    );
    println!(
        "per-fixture word cap: {MAX_WORDS_PER_FIXTURE} (first-N subsample if a fixture exceeds \
         this; labeled on that fixture's own row when it applies -- no fixture on disk triggers it \
         today)"
    );
    println!(
        "\"compiles\" below means the candidate's network was actually built: \
         CellOutcome::Refused = no; every other CellOutcome = yes, since the network was built \
         even if the corpus-level verdict is not oracle-exact.\n"
    );
    println!(
        "fixtures whose words.yaml declares expect_crash: true are a NAMED EXCLUSION (no oracle \
         ground truth to measure against), listed separately below -- never scored as a 0/3 \
         defect and never silently dropped.\n"
    );

    let fixtures = discover();
    println!("discovered {} fixtures under scope=all\n", fixtures.len());

    let mut scored: Vec<Row> = Vec::with_capacity(fixtures.len());
    let mut excluded: Vec<Excluded> = Vec::new();
    let mut dist = [0usize; 4]; // index = count of strategies FullHcConfirmed (0..=3)
    let mut unmeasurable = 0usize;

    for fixture in &fixtures {
        let label = fixture.label();
        let words_yaml = fixture.load_words_yaml();
        if words_yaml.expect_crash {
            let reason = "words.yaml declares expect_crash: true -- the founding oracle run \
                          crashed, so there is no oracle ground truth to measure against"
                .to_string();
            println!("=== {label} === EXCLUDED (expect_crash): {reason}\n");
            excluded.push(Excluded { label, reason });
            continue;
        }

        let row = match pg_grammar::load(&fixture.load_grammar_xml()) {
            Err(e) => Row {
                scored: scoreboard::unmeasurable(&label, &format!("grammar failed to load: {e}")),
                total_words: 0,
                subsampled: false,
                expect_fail_count: 0,
                expect_skip_count: 0,
            },
            Ok(grammar) => {
                let total_words = words_yaml.words.len();
                let expect_fail_count = words_yaml.words.iter().filter(|w| w.expect_fail).count();
                let expect_skip_count = words_yaml.words.iter().filter(|w| w.expect_skip).count();
                let all_words: Vec<String> =
                    words_yaml.words.iter().map(|w| w.word.clone()).collect();
                let subsampled = all_words.len() > MAX_WORDS_PER_FIXTURE;
                let words: Vec<String> = if subsampled {
                    all_words[..MAX_WORDS_PER_FIXTURE].to_vec()
                } else {
                    all_words
                };
                Row {
                    scored: scoreboard::measure(&label, &grammar, &words),
                    total_words,
                    subsampled,
                    expect_fail_count,
                    expect_skip_count,
                }
            }
        };

        print_fixture_progress(&row);
        match row.scored.exact_count {
            Some(n) => dist[n] += 1,
            None => unmeasurable += 1,
        }
        println!(
            "  [running tally after {} scored fixture(s), {} excluded: 3/3={} 2/3={} 1/3={} \
             0/3={} unmeasurable={}]\n",
            scored.len() + 1,
            excluded.len(),
            dist[3],
            dist[2],
            dist[1],
            dist[0],
            unmeasurable
        );
        scored.push(row);
    }

    print_headline(&scored, &dist, unmeasurable, &excluded);
    print_full_table(&scored);
    print_could_not_measure(&scored);
    print_excluded(&excluded);

    println!("\ncommit under measurement: {commit}");
    println!(
        "reproduce: rust/tools/pg.ps1 -Mode run -Example conf_matrix   (from the repo root of the \
         worktree this was measured in)"
    );
}

fn print_fixture_progress(row: &Row) {
    let f = &row.scored;
    println!("=== {} ===", f.label);
    println!(
        "  words.yaml: {} word(s) declared ({} expect_fail, {} expect_skip -- negative controls \
         the oracle itself resolves to zero/invalid-shape, not measurement gaps)",
        row.total_words, row.expect_fail_count, row.expect_skip_count
    );
    if row.subsampled {
        println!(
            "  SUBSAMPLED: measuring the first {MAX_WORDS_PER_FIXTURE} of {} words (bound = \
             MAX_WORDS_PER_FIXTURE)",
            row.total_words
        );
    }
    if f.excluded_words.is_empty() {
        println!("  words excluded by the oracle: 0/{}", f.measured_words);
    } else {
        println!(
            "  words excluded by the oracle: {}/{} (named, never dropped silently):",
            f.excluded_words.len(),
            f.measured_words
        );
        for (word, reason) in &f.excluded_words {
            println!("    excluded {word:?}: {reason}");
        }
    }
    for cell in &f.cells {
        match &cell.outcome {
            CellOutcome::Refused { reason, predicates } => {
                println!(
                    "  [{:?}] compiles=no certification={} -- REFUSED/FAILED: {reason} \
                     (envelope predicates: {predicates:?})",
                    cell.strategy, cell.certification_debug
                );
            }
            CellOutcome::Unmeasurable { reason } => {
                println!(
                    "  [{:?}] compiles=yes certification={}",
                    cell.strategy, cell.certification_debug
                );
                println!("    COULD NOT MEASURE per-word evidence: {reason}");
            }
            CellOutcome::OracleExact | CellOutcome::CompilesButMisses { .. } => {
                let recall = match cell.outcome {
                    CellOutcome::CompilesButMisses { recall_deficit } => recall_deficit,
                    _ => 0,
                };
                let soundness = cell
                    .divergence
                    .map(|d| d.candidate_only_identities)
                    .unwrap_or(0);
                println!(
                    "  [{:?}] compiles=yes certification={}",
                    cell.strategy, cell.certification_debug
                );
                println!(
                    "    recall(oracle_only,DEFECT)={recall} soundness(candidate_only \
                     post-confirm,DEFECT)={soundness} \
                     legal_overgeneration(pre-confirm pruned by confirm,INFORMATIONAL per \
                     ADR-0001)={} words_measured={}/{} exact={}",
                    cell.legal_overgeneration.unwrap_or(0),
                    cell.words_measured.unwrap_or(0),
                    f.measured_words,
                    cell.exact()
                );
            }
        }
    }
    println!(
        "  HEADLINE: {}/{} backends produce oracle-exact confirmed output for this fixture\n",
        f.exact_count.unwrap_or(0),
        ALL_STRATEGIES.len()
    );
}

fn print_headline(rows: &[Row], dist: &[usize; 4], unmeasurable: usize, excluded: &[Excluded]) {
    println!(
        "\n================ HEADLINE: oracle-exact confirmed output, per fixture ================"
    );
    println!("fixtures discovered and scored: {}", rows.len());
    println!("fixtures excluded (expect_crash): {}", excluded.len());
    println!("fixtures measurable: {}", rows.len() - unmeasurable);
    for k in (0..=3).rev() {
        println!("  supported by {k}/3 backends: {} fixture(s)", dist[k]);
    }
    println!("  unmeasurable (see COULD NOT MEASURE section below): {unmeasurable} fixture(s)");
    if dist[0] > 0 {
        println!("\n  fixtures supported by 0/3 backends:");
        for row in rows.iter().filter(|r| r.scored.exact_count == Some(0)) {
            println!("    {}", row.scored.label);
        }
    }
}

fn print_full_table(rows: &[Row]) {
    println!("\n================ FULL TABLE ================");
    println!(
        "columns per strategy: compiles(y/n) / recall(oracle-only,DEFECT) / \
         soundness(candidate-only,DEFECT) / legal-overgen(informational) / exact(y/n)"
    );
    for row in rows {
        let f = &row.scored;
        println!(
            "\n{} [{} words measured of {} total{}, {} excluded, {} expect_fail, {} expect_skip]",
            f.label,
            f.measured_words,
            row.total_words,
            if row.subsampled { ", SUBSAMPLED" } else { "" },
            f.excluded_words.len(),
            row.expect_fail_count,
            row.expect_skip_count
        );
        for cell in &f.cells {
            let compiles = if cell.compiled() { "y" } else { "n" };
            match &cell.outcome {
                CellOutcome::Refused { reason, .. } => {
                    println!(
                        "  {:?}: compiles={compiles} certification={} COULD NOT MEASURE -- {reason}",
                        cell.strategy, cell.certification_debug
                    );
                }
                CellOutcome::Unmeasurable { reason } => {
                    println!(
                        "  {:?}: compiles={compiles} certification={} COULD NOT MEASURE -- {reason}",
                        cell.strategy, cell.certification_debug
                    );
                }
                CellOutcome::OracleExact | CellOutcome::CompilesButMisses { .. } => {
                    let recall = match cell.outcome {
                        CellOutcome::CompilesButMisses { recall_deficit } => recall_deficit,
                        _ => 0,
                    };
                    let soundness = cell
                        .divergence
                        .map(|d| d.candidate_only_identities)
                        .unwrap_or(0);
                    println!(
                        "  {:?}: compiles={compiles} certification={} recall={recall} \
                         soundness={soundness} legal_overgen={} words_measured={} exact={}",
                        cell.strategy,
                        cell.certification_debug,
                        cell.legal_overgeneration.unwrap_or(0),
                        cell.words_measured.unwrap_or(0),
                        cell.exact()
                    );
                }
            }
        }
    }
}

fn print_could_not_measure(rows: &[Row]) {
    let mut any = false;
    println!("\n================ CELLS THAT COULD NOT BE MEASURED ================");
    for row in rows {
        for cell in &row.scored.cells {
            let reason = match &cell.outcome {
                CellOutcome::Refused { reason, .. } => Some(reason),
                CellOutcome::Unmeasurable { reason } => Some(reason),
                _ => None,
            };
            if let Some(reason) = reason {
                any = true;
                println!("{} [{:?}]: {reason}", row.scored.label, cell.strategy);
            }
        }
    }
    if !any {
        println!("none -- every (fixture, backend) cell was measured");
    }
}

fn print_excluded(excluded: &[Excluded]) {
    println!("\n================ FIXTURES EXCLUDED (expect_crash) ================");
    if excluded.is_empty() {
        println!("none");
        return;
    }
    for ex in excluded {
        println!("{}: {}", ex.label, ex.reason);
    }
}
