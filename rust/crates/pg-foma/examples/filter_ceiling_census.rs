//! Prices what a PERFECT candidate filter could delete from confirmation, using no filter at all: a candidate whose confirmation bucket comes back empty is one such a filter could have removed.

//! Headline is `removable_steps / (removable_steps + surviving_chunk_steps)` and never the share of candidates killed, because confirmation fuses candidates and only a wholly-doomed chunk disappears.

//! A share is not a latency, so the run also reports per-word distributions of steps and confirmation time, before and after the removable chunks are deleted, and the tail is the figure to read: a grammar that is already fast gains nothing from a large share, and a slow one is judged at p99 rather than at its median.

//! The filter's own cost is measured rather than modelled: the structural passes run over the same words and their evaluations and elapsed time are reported per word. Through the legacy adapter those passes establish nothing they could reject on, so this prices them and never credits them.

//! Usage: `pg.ps1 -Mode run -Example filter_ceiling_census -- <grammar> [--words N] [--word-timeout-ms M]`.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use foma::apply::apply_init;
use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;
use foma::types::Fsm;

use pg_foma::candidate_filter::test_support::filter_of;
use pg_foma::candidate_filter::{
    shadow, witnesses_for, CandidateFilter, CandidateFilterPass, CountingTraceSink, FilterBudget,
    FilterIndex, FilterMode, OwnershipPass, ProposedCandidate, RetainedCandidateSink,
    StructuralTransitionPass,
};
use pg_foma::compose_budget::ComposeBudget;
use pg_foma::confirm::{self, MorphemeOwner};
use pg_foma::emit;
use pg_foma::peel::ReduplicationPeeler;
use pg_foma::tags::{self, Candidate};
use pg_grammar::model::Grammar;
use pg_parse::Morpher;

/// A one-member removable chunk is a candidate a filter kills alone; a wide one needs every member killed together, so the ends of this histogram describe different filters.
const MEMBER_BUCKETS: [(usize, usize); 8] = [
    (1, 1),
    (2, 2),
    (3, 4),
    (5, 8),
    (9, 16),
    (17, 32),
    (33, 64),
    (65, usize::MAX),
];

const DEFAULT_WORDS: usize = 20;
const DEFAULT_WORD_TIMEOUT_MS: u64 = 10_000;

// Sample loading + propose/peel plumbing duplicated from `examples/deadend_census.rs`: examples in this crate cannot depend on each other's code, only duplicate it.

/// A manifest logical name resolved to its grammar and word-list file names, or a bare path pair.
struct Corpus {
    label: String,
    grammar_file: String,
    words_file: String,
}

fn known_corpus(name: &str) -> Option<Corpus> {
    let (grammar, words) = match name {
        "indonesian" => ("indonesian-hc.xml", "indonesian-words.txt"),
        "sena" => ("sena-hc.xml", "sena-words.txt"),
        "amharic" => ("amharic-hc.xml", "amharic-words.txt"),
        "aweti" => ("aweti.json", "aweti-words.txt"),
        "mbugwe" => ("mbugwe.fwdata", "mbugwe-words.txt"),
        _ => return None,
    };
    Some(Corpus {
        label: name.to_owned(),
        grammar_file: grammar.to_owned(),
        words_file: words.to_owned(),
    })
}

fn corpus_path(relative: &str) -> PathBuf {
    pg_conformance_fixtures::corpus::require(relative)
}

fn load_grammar(relative: &str) -> Grammar {
    let path = corpus_path(relative);
    let display = path.display().to_string();
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "json" => {
            let json = std::fs::read_to_string(&path).expect("read grammar snapshot");
            let snapshot =
                pg_snapshot::Snapshot::from_json(&json).expect("parse the grammar snapshot");
            pg_grammar::compile_project(&snapshot)
                .map(|(g, _)| g)
                .unwrap_or_else(|e| panic!("compile {display}: {e:?}"))
        }
        "fwdata" => {
            let (snapshot, _) = pg_fwdata::import_file(&path).expect("import fwdata");
            pg_grammar::compile_project(&snapshot)
                .map(|(g, _)| g)
                .unwrap_or_else(|e| panic!("compile {display}: {e:?}"))
        }
        _ => {
            let xml = std::fs::read_to_string(&path).expect("read grammar xml");
            pg_grammar::load(&xml).unwrap_or_else(|e| panic!("load {display}: {e}"))
        }
    }
}

/// Leading lines the corpus manifest declares are not surface words; zero for an undeclared file.
fn skip_leading_lines(words_file: &str) -> usize {
    let Ok(manifest) = pg_conformance_fixtures::corpus::load_manifest() else {
        return 0;
    };
    manifest
        .corpora
        .iter()
        .flat_map(|c| c.files.iter())
        .find(|f| f.path == words_file)
        .and_then(|f| f.word_list.as_ref())
        .map(|w| w.skip_leading_lines as usize)
        .unwrap_or(0)
}

/// The first `count` words of the list, after the manifest's declared header, and nothing else: a slice chosen to make a ratio look good would not be evidence about any other slice.
fn read_words(words_file: &str, count: usize) -> Vec<String> {
    let text = std::fs::read_to_string(corpus_path(words_file)).expect("read word list");
    text.lines()
        .skip(skip_leading_lines(words_file))
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .take(count)
        .map(str::to_owned)
        .collect()
}

fn propose(net: &Fsm, word: &str) -> Vec<Candidate> {
    let normalized = pg_grammar::nfd::nfd(word);
    let mut handle = apply_init(net);
    let mut seen: HashSet<(Vec<u32>, i32)> = HashSet::new();
    let mut out = Vec::new();
    for s in handle.up(&normalized) {
        let Some(path) = tags::decode_path(&s) else {
            continue;
        };
        for c in tags::to_candidates(&path) {
            let key: (Vec<u32>, i32) = (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
            if seen.insert(key) {
                out.push(c);
            }
        }
    }
    out
}

fn propose_and_peel(
    net: &Fsm,
    g: &Grammar,
    peeler: &ReduplicationPeeler,
    word: &str,
) -> Vec<Candidate> {
    let mut candidates = propose(net, word);
    // Real corpus word lists are never adversarial, so an unbounded chain-depth budget is safe here.
    let budget = ComposeBudget::from_env();
    let peeled = peeler
        .peel_candidates(g, word, &budget, &mut |r: &str| propose(net, r))
        .unwrap_or_else(|e| {
            eprintln!("[filter_ceiling_census] reduplication peel refused for {word:?}: {e}");
            Vec::new()
        });
    for c in peeled {
        let already = candidates.iter().any(|existing| {
            existing.root_index == c.root_index && existing.morphemes == c.morphemes
        });
        if !already {
            candidates.push(c);
        }
    }
    candidates
}

/// The declared pass order an enforced run would use over one grammar's derived facts.
fn production_passes(index: &Arc<FilterIndex>) -> Vec<Box<dyn CandidateFilterPass>> {
    vec![
        Box::new(OwnershipPass::new(Arc::clone(index))),
        Box::new(StructuralTransitionPass::new(Arc::clone(index))),
    ]
}

/// Survivors are dropped as they arrive: collecting them would price a vector, not the passes.
struct DiscardingSink;

impl RetainedCandidateSink for DiscardingSink {
    fn accept(&mut self, _candidate: ProposedCandidate) {}
}

/// What one word cost before confirmation was reached at all.
struct PreConfirmCost {
    propose_elapsed: Duration,
    /// Building the legacy adapter's witnesses, which is the adapter's cost and not a pass's.
    witness_elapsed: Duration,
    filter_elapsed: Duration,
    evaluations: u64,
    candidates_rejected: u64,
}

fn measure_filter(
    filter: &CandidateFilter,
    candidates: &[Candidate],
    propose_elapsed: Duration,
) -> PreConfirmCost {
    let started = Instant::now();
    let proposals = witnesses_for(candidates, 0, 0);
    let witness_elapsed = started.elapsed();

    let mut retained = DiscardingSink;
    let mut trace = CountingTraceSink::new();
    let started = Instant::now();
    filter.filter_into(
        FilterMode::Shadow,
        proposals,
        &mut retained,
        &mut trace,
        FilterBudget::unlimited(),
    );
    let filter_elapsed = started.elapsed();

    let counters = trace.into_counters();
    PreConfirmCost {
        propose_elapsed,
        witness_elapsed,
        filter_elapsed,
        evaluations: counters.pass_evaluations,
        candidates_rejected: counters.candidates_rejected,
    }
}

/// What one word contributed, or why it contributed nothing.
enum WordOutcome {
    /// The proposer offered nothing, so there was no confirmation work to price either way.
    NoCandidates,
    /// The word deadline fired: `steps` is truncated and the empty buckets are abandoned work, not rejections, so this word is reported and then kept out of every aggregate.
    TimedOut(WordRow),
    Measured(WordRow),
}

struct WordRow {
    word: String,
    candidates: usize,
    empty_buckets: usize,
    chunks_total: usize,
    chunks_removable: usize,
    removable_steps: usize,
    surviving_chunk_steps: usize,
    /// Member count of each removable chunk, and each chunk's own steps, split by fate.
    removable_members: Vec<usize>,
    removable_chunk_steps: Vec<usize>,
    surviving_chunk_steps_each: Vec<usize>,
    removable_elapsed: Duration,
    surviving_elapsed: Duration,
    confirm_elapsed: Duration,
    pre: PreConfirmCost,
}

fn measure_word(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    word: &str,
    candidates: &[Candidate],
    pre: PreConfirmCost,
) -> WordOutcome {
    if candidates.is_empty() {
        return WordOutcome::NoCandidates;
    }

    let start = Instant::now();
    let (buckets, chunks) = confirm::confirm_batch_attributed(g, owners, morpher, candidates, word);
    let confirm_elapsed = start.elapsed();

    let doomed: Vec<usize> = buckets
        .iter()
        .enumerate()
        .filter(|(_, bucket)| bucket.is_empty())
        .map(|(index, _)| index)
        .collect();
    let presented: Vec<usize> = (0..candidates.len()).collect();
    let attribution = shadow::attribute(&doomed, &presented, &chunks);

    let mut removable_members = Vec::new();
    let mut removable_chunk_steps = Vec::new();
    let mut surviving_chunk_steps_each = Vec::new();
    let mut surviving_elapsed = Duration::ZERO;
    for chunk in &chunks {
        if shadow::chunk_is_removable(chunk, &doomed, &presented) {
            removable_members.push(chunk.members.len());
            removable_chunk_steps.push(chunk.steps);
        } else {
            surviving_chunk_steps_each.push(chunk.steps);
            surviving_elapsed += chunk.elapsed;
        }
    }

    let row = WordRow {
        word: word.to_owned(),
        candidates: candidates.len(),
        empty_buckets: doomed.len(),
        chunks_total: chunks.len(),
        chunks_removable: attribution.removable_chunks,
        removable_steps: attribution.removable_steps,
        surviving_chunk_steps: attribution.surviving_chunk_steps,
        removable_members,
        removable_chunk_steps,
        surviving_chunk_steps_each,
        removable_elapsed: attribution.removable_elapsed,
        surviving_elapsed,
        confirm_elapsed,
        pre,
    };
    if chunks.iter().any(|chunk| chunk.timed_out) {
        WordOutcome::TimedOut(row)
    } else {
        WordOutcome::Measured(row)
    }
}

/// Nearest-rank percentile over an ascending slice; zero for an empty one.
fn percentile(ascending: &[usize], percent: usize) -> usize {
    if ascending.is_empty() {
        return 0;
    }
    let rank = (ascending.len() * percent).div_ceil(100).max(1);
    ascending[rank.min(ascending.len()) - 1]
}

/// Nearest-rank percentile over an ascending slice; zero for an empty one.
fn percentile_f64(ascending: &[f64], percent: usize) -> f64 {
    if ascending.is_empty() {
        return 0.0;
    }
    let rank = (ascending.len() * percent).div_ceil(100).max(1);
    ascending[rank.min(ascending.len()) - 1]
}

/// One per-word series, summarized at the points a latency question is asked at.
#[derive(Clone, Copy, Default)]
struct Distribution {
    p50: f64,
    p90: f64,
    p99: f64,
    max: f64,
    mean: f64,
    total: f64,
}

fn distribution(values: &mut [f64]) -> Distribution {
    values.sort_by(f64::total_cmp);
    let total: f64 = values.iter().sum();
    Distribution {
        p50: percentile_f64(values, 50),
        p90: percentile_f64(values, 90),
        p99: percentile_f64(values, 99),
        max: values.last().copied().unwrap_or(0.0),
        mean: if values.is_empty() {
            0.0
        } else {
            total / values.len() as f64
        },
        total,
    }
}

fn print_distribution(label: &str, d: &Distribution, precision: usize) {
    println!(
        "{label:<26} {:>12.precision$} {:>12.precision$} {:>12.precision$} {:>12.precision$} {:>12.precision$} {:>14.precision$}",
        d.p50, d.p90, d.p99, d.max, d.mean, d.total
    );
}

fn ms(elapsed: Duration) -> f64 {
    elapsed.as_secs_f64() * 1000.0
}

fn describe(label: &str, steps: &mut Vec<usize>) -> String {
    steps.sort_unstable();
    format!(
        "{label:<10} n={:<6} median={:<9} p90={:<9} max={}",
        steps.len(),
        percentile(steps, 50),
        percentile(steps, 90),
        steps.last().copied().unwrap_or(0)
    )
}

fn print_row(row: &WordRow, status: &str) {
    println!(
        "{:<24} {:>6} {:>7} {:>7} {:>9} {:>12} {:>12} {:>9.1} {:>10.3} {}",
        row.word,
        row.candidates,
        row.empty_buckets,
        row.chunks_total,
        row.chunks_removable,
        row.removable_steps,
        row.surviving_chunk_steps,
        ms(row.confirm_elapsed),
        ms(row.pre.filter_elapsed),
        status
    );
}

fn run_census(corpus: &Corpus, word_count: usize, word_timeout: Duration) {
    let started = Instant::now();
    let g = load_grammar(&corpus.grammar_file);
    let load_ms = ms(started.elapsed());
    let words = read_words(&corpus.words_file, word_count);

    let stage = Instant::now();
    let emit::EmitResult { lexc_source, .. } = emit::emit(&g);
    let emit_ms = ms(stage.elapsed());
    let stage = Instant::now();
    let net = fsm_lexc_parse_string(&FomaOptions::default(), None, &lexc_source)
        .unwrap_or_else(|| panic!("foma failed to compile the emitted lexc source"));
    let compile_ms = ms(stage.elapsed());
    let peeler = ReduplicationPeeler::new(&g);
    let owners = confirm::build_morpheme_owners(&g);
    let morpher = Morpher::new(&g, usize::MAX).with_word_timeout(Some(word_timeout));
    let stage = Instant::now();
    let index = Arc::new(FilterIndex::build(&g));
    let filter = filter_of(production_passes(&index));
    let index_ms = ms(stage.elapsed());

    println!(
        "# filter ceiling census: {} ({} / {}), first {} words, word timeout {}ms",
        corpus.label,
        corpus.grammar_file,
        corpus.words_file,
        words.len(),
        word_timeout.as_millis()
    );
    // Kept out of every per-word figure below: a compile is paid once per grammar, not per word.
    println!(
        "grammar setup (one-off): load {load_ms:.1}ms, emit {emit_ms:.1}ms, foma compile {compile_ms:.1}ms, filter index {index_ms:.1}ms"
    );
    println!();
    println!(
        "{:<24} {:>6} {:>7} {:>7} {:>9} {:>12} {:>12} {:>9} {:>10} {}",
        "word",
        "cands",
        "empty",
        "chunks",
        "removable",
        "rem_steps",
        "surv_steps",
        "confirm_ms",
        "filter_ms",
        "status"
    );

    let mut measured = 0usize;
    let mut no_candidates = 0usize;
    let mut timed_out = 0usize;
    let mut total_candidates = 0usize;
    let mut total_empty = 0usize;
    let mut total_chunks = 0usize;
    let mut total_removable_chunks = 0usize;
    let mut removable_steps = 0usize;
    let mut surviving_steps = 0usize;
    let mut removable_members: Vec<usize> = Vec::new();
    let mut removable_chunk_steps: Vec<usize> = Vec::new();
    let mut surviving_chunk_steps: Vec<usize> = Vec::new();
    let mut confirm_elapsed = Duration::ZERO;
    let mut removable_elapsed = Duration::ZERO;
    let mut surviving_elapsed = Duration::ZERO;
    // Per-word series, one entry per measured word, in the order the words were read.
    let mut steps_before: Vec<f64> = Vec::new();
    let mut steps_after: Vec<f64> = Vec::new();
    let mut confirm_ms_before: Vec<f64> = Vec::new();
    let mut confirm_ms_after: Vec<f64> = Vec::new();
    let mut propose_ms: Vec<f64> = Vec::new();
    let mut filter_ms: Vec<f64> = Vec::new();
    let mut witness_ms: Vec<f64> = Vec::new();
    let mut filter_evaluations: Vec<f64> = Vec::new();
    let mut filter_rejections = 0u64;

    // Strictly sequential: fanning corpus words out concurrently is what has exhausted this machine's memory before.
    for word in &words {
        let proposing = Instant::now();
        let candidates = propose_and_peel(&net, &g, &peeler, word);
        let pre = measure_filter(&filter, &candidates, proposing.elapsed());
        filter_rejections += pre.candidates_rejected;
        match measure_word(&g, &owners, &morpher, word, &candidates, pre) {
            WordOutcome::NoCandidates => {
                no_candidates += 1;
                println!(
                    "{:<24} {:>6} {:>7} {:>7} {:>9} {:>12} {:>12} {:>9} {:>10} no-candidates",
                    word, 0, 0, 0, 0, 0, 0, "-", "-"
                );
            }
            WordOutcome::TimedOut(row) => {
                timed_out += 1;
                print_row(&row, "TIMEOUT(excluded)");
            }
            WordOutcome::Measured(row) => {
                measured += 1;
                total_candidates += row.candidates;
                total_empty += row.empty_buckets;
                total_chunks += row.chunks_total;
                total_removable_chunks += row.chunks_removable;
                removable_steps += row.removable_steps;
                surviving_steps += row.surviving_chunk_steps;
                removable_members.extend(&row.removable_members);
                removable_chunk_steps.extend(&row.removable_chunk_steps);
                surviving_chunk_steps.extend(&row.surviving_chunk_steps_each);
                confirm_elapsed += row.confirm_elapsed;
                removable_elapsed += row.removable_elapsed;
                surviving_elapsed += row.surviving_elapsed;
                steps_before.push((row.removable_steps + row.surviving_chunk_steps) as f64);
                steps_after.push(row.surviving_chunk_steps as f64);
                confirm_ms_before.push(ms(row.confirm_elapsed));
                confirm_ms_after.push(ms(row
                    .confirm_elapsed
                    .saturating_sub(row.removable_elapsed)));
                propose_ms.push(ms(row.pre.propose_elapsed));
                filter_ms.push(ms(row.pre.filter_elapsed));
                witness_ms.push(ms(row.pre.witness_elapsed));
                filter_evaluations.push(row.pre.evaluations as f64);
                print_row(&row, "ok");
            }
        }
    }

    let total_steps = removable_steps + surviving_steps;
    println!();
    println!("## aggregate ({}), measured words only", corpus.label);
    println!(
        "words: requested={word_count} read={} measured={measured} no_candidates={no_candidates} timed_out={timed_out}",
        words.len()
    );
    println!(
        "candidates={total_candidates} empty_buckets={total_empty} ({:.1}% of candidates)",
        percent(total_empty, total_candidates)
    );
    println!(
        "chunks={total_chunks} removable_chunks={total_removable_chunks} ({:.1}% of chunks)",
        percent(total_removable_chunks, total_chunks)
    );
    println!("removable_steps={removable_steps} surviving_chunk_steps={surviving_steps}");
    println!(
        "HEADLINE removable share of confirmation steps = {:.2}% ({removable_steps}/{total_steps})",
        percent(removable_steps, total_steps)
    );
    let chunk_elapsed = removable_elapsed + surviving_elapsed;
    println!(
        "confirm wall time over measured words = {:.1}ms (in chunks: removable {:.1}ms, surviving {:.1}ms)",
        confirm_elapsed.as_secs_f64() * 1000.0,
        removable_elapsed.as_secs_f64() * 1000.0,
        surviving_elapsed.as_secs_f64() * 1000.0
    );
    // Reported beside the steps ratio, never instead of it: a duration is an observation, not a reproducible figure.
    println!(
        "removable share of chunk wall time = {:.2}%",
        percent(
            removable_elapsed.as_nanos() as usize,
            chunk_elapsed.as_nanos() as usize
        )
    );
    let steps_before = distribution(&mut steps_before);
    let steps_after = distribution(&mut steps_after);
    let confirm_before = distribution(&mut confirm_ms_before);
    let confirm_after = distribution(&mut confirm_ms_after);
    let propose = distribution(&mut propose_ms);
    let filter_cost = distribution(&mut filter_ms);
    let witness = distribution(&mut witness_ms);
    let evaluations = distribution(&mut filter_evaluations);

    println!();
    println!(
        "## per-word distributions ({}), measured words only, n={measured}",
        corpus.label
    );
    println!(
        "{:<26} {:>12} {:>12} {:>12} {:>12} {:>12} {:>14}",
        "metric", "p50", "p90", "p99", "max", "mean", "total"
    );
    print_distribution("steps/word before", &steps_before, 0);
    print_distribution("steps/word after", &steps_after, 0);
    print_distribution("confirm ms/word before", &confirm_before, 3);
    print_distribution("confirm ms/word after", &confirm_after, 3);
    print_distribution("propose ms/word", &propose, 3);
    print_distribution("filter ms/word", &filter_cost, 3);
    print_distribution("witness build ms/word", &witness, 3);
    print_distribution("filter evals/word", &evaluations, 0);
    println!();
    println!(
        "TAIL p99 steps/word {:.0} -> {:.0} ({:+.1}%); p99 confirm ms/word {:.3} -> {:.3} ({:+.1}%)",
        steps_before.p99,
        steps_after.p99,
        change(steps_before.p99, steps_after.p99),
        confirm_before.p99,
        confirm_after.p99,
        change(confirm_before.p99, confirm_after.p99)
    );
    println!(
        "p50 steps/word {:.0} -> {:.0} ({:+.1}%); p50 confirm ms/word {:.3} -> {:.3} ({:+.1}%)",
        steps_before.p50,
        steps_after.p50,
        change(steps_before.p50, steps_after.p50),
        confirm_before.p50,
        confirm_after.p50,
        change(confirm_before.p50, confirm_after.p50)
    );
    // "after" charges nothing for the filter, so it is a ceiling; the filter row is what is added back.
    println!(
        "filter cost measured, not modelled: {} pass evaluations over {measured} words, {filter_rejections} candidate(s) rejected",
        evaluations.total
    );
    println!(
        "zero rejections is the expected result here and is not a filter verdict: the legacy adapter establishes no role, slot or stratum fact, so a structural pass can only defer."
    );

    println!();
    println!("per-chunk steps:");
    println!("  {}", describe("removable", &mut removable_chunk_steps));
    println!("  {}", describe("surviving", &mut surviving_chunk_steps));
    println!();
    println!("removable-chunk member counts:");
    if removable_members.is_empty() {
        println!("  (none)");
    } else {
        removable_members.sort_unstable();
        println!(
            "  median={} p90={} max={}",
            percentile(&removable_members, 50),
            percentile(&removable_members, 90),
            removable_members.last().copied().unwrap_or(0)
        );
        for (low, high) in MEMBER_BUCKETS {
            let count = removable_members
                .iter()
                .filter(|&&m| m >= low && m <= high)
                .count();
            let label = if low == high {
                format!("{low}")
            } else if high == usize::MAX {
                format!("{low}+")
            } else {
                format!("{low}-{high}")
            };
            println!(
                "  {label:>7} member(s): {count:>5} chunk(s) ({:.1}%)",
                percent(count, removable_members.len())
            );
        }
    }
    println!();
    println!(
        "steps is `pg_parse::ParseOutcome::steps`: it prices the restricted reparse as one number \
         and separates neither the mrule/template cascade from lexical lookup nor either from \
         allomorph selection."
    );
    println!(
        "census wall time: {:.1}ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
}

/// Signed percentage change from `before` to `after`; zero when there was nothing to change.
fn change(before: f64, after: f64) -> f64 {
    if before == 0.0 {
        return 0.0;
    }
    100.0 * (after - before) / before
}

fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    100.0 * part as f64 / whole as f64
}

fn parse_named<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    let position = args.iter().position(|a| a == name)?;
    args.get(position + 1)
        .and_then(|v| v.parse().ok())
        .or_else(|| panic!("{name} needs a value"))
}

fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(name) = args.first() else {
        eprintln!(
            "usage: filter_ceiling_census <indonesian|sena|amharic|aweti|mbugwe> \
             [--words N] [--word-timeout-ms M]"
        );
        std::process::exit(2);
    };
    let Some(corpus) = known_corpus(name) else {
        eprintln!("unknown corpus {name:?}");
        std::process::exit(2);
    };
    let word_count = parse_named(&args, "--words").unwrap_or(DEFAULT_WORDS);
    let timeout_ms = parse_named(&args, "--word-timeout-ms").unwrap_or(DEFAULT_WORD_TIMEOUT_MS);
    run_census(&corpus, word_count, Duration::from_millis(timeout_ms));
}

/// The deep composite/rule-chain recursion the real grammars reach needs a big stack under release inlining, exactly as this crate's other censuses do.
fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(512 * 1024 * 1024)
        .spawn(run)
        .expect("failed to spawn census thread");
    handle.join().expect("census thread panicked");
}
