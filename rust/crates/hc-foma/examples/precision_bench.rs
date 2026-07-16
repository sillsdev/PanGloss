//! FST precision knob bench matrix (spec `docs/superpowers/specs/2026-07-15-fst-precision-knob-
//! design.md` §6 last bullet, the "fst-stats successor"): per (grammar × preset), prints network
//! size, load/compile time, lookup throughput, candidates/word, and confirm time as a markdown
//! table — the "try out different combinations" playground, produced automatically.
//!
//! Run manually (not part of `cargo test`):
//!
//!   cargo run -p hc-foma --release --example precision_bench
//!
//! Only [`PrecisionConfig::Strip`] and [`PrecisionConfig::AllFlags`] are benched — `FullCompile`/
//! `Auto` are emit-identical to `Strip` as of this step (`crate::precision`'s own doc: "`crate::emit`
//! treats every config other than `AllFlags` identically to `Strip`"), so benching them would just
//! reprint the `Strip` row under a different name.
//!
//! The propose/peel/confirm plumbing below is copied (not imported — examples can't depend on test
//! code) from `tests/pk1_precision_recall_invariance.rs`; see that file for the property-level
//! justification of each step. This file only times it and prints a table; it makes no correctness
//! assertions of its own (that is `pk1`'s job).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use foma::apply::apply_init;
use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;
use foma::types::Fsm;

use hc_foma::confirm::{self, MorphemeOwner};
use hc_foma::emit::{self, EmitResult};
use hc_foma::peel::ReduplicationPeeler;
use hc_foma::precision::{ConstraintCatalog, PrecisionAction, PrecisionConfig};
use hc_foma::tags::{self, Candidate};
use hc_grammar::model::Grammar;
use hc_parse::Morpher;

// -------------------------------------------------------------------------------------------
// Corpus word caps (keeps the whole bench at a few minutes total -- Amharic's compile alone is
// ~35s per preset, and its engine-oracle confirm is the slowest step per word, so its cap is kept
// lower than Sena/Indonesian's).
// -------------------------------------------------------------------------------------------

const SENA_WORD_CAP: usize = 100;
const INDONESIAN_WORD_CAP: usize = 100;
const AMHARIC_WORD_CAP: usize = 40;

/// Per-word engine timeout passed to `Morpher` (mirrors `tests/f3_amharic_gate.rs`'s
/// `ENGINE_TIMEOUT`) -- without this, one pathological word's confirm could dominate the whole
/// bench's wall time.
const ENGINE_TIMEOUT: Duration = Duration::from_secs(10);

struct GrammarSpec {
    name: &'static str,
    xml_file: &'static str,
    words_file: &'static str,
    word_cap: usize,
}

const GRAMMARS: &[GrammarSpec] = &[
    GrammarSpec {
        name: "Sena",
        xml_file: "sena-hc.xml",
        words_file: "sena-words.txt",
        word_cap: SENA_WORD_CAP,
    },
    GrammarSpec {
        name: "Indonesian",
        xml_file: "indonesian-hc.xml",
        words_file: "indonesian-words.txt",
        word_cap: INDONESIAN_WORD_CAP,
    },
    GrammarSpec {
        name: "Amharic",
        xml_file: "amharic-hc.xml",
        words_file: "amharic-words.txt",
        word_cap: AMHARIC_WORD_CAP,
    },
];

const PRESETS: &[PrecisionConfig] = &[PrecisionConfig::Strip, PrecisionConfig::AllFlags];

fn preset_label(p: PrecisionConfig) -> &'static str {
    match p {
        PrecisionConfig::Strip => "Strip",
        PrecisionConfig::AllFlags => "AllFlags",
        PrecisionConfig::FullCompile => "FullCompile",
        PrecisionConfig::Auto(_) => "Auto",
    }
}

// -------------------------------------------------------------------------------------------
// Sample loading (mirrors `tests/pk1_precision_recall_invariance.rs`'s helpers exactly).
// -------------------------------------------------------------------------------------------

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

fn load_grammar(name: &str) -> Option<Grammar> {
    let path = sample_path(name);
    let xml = std::fs::read_to_string(&path).ok()?;
    Some(hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {name}: {e}")))
}

fn read_words(name: &str) -> Option<Vec<String>> {
    let path = sample_path(name);
    let text = std::fs::read_to_string(&path).ok()?;
    Some(
        text.lines()
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

// -------------------------------------------------------------------------------------------
// Propose/peel/confirm plumbing -- copied from `tests/pk1_precision_recall_invariance.rs` (module
// doc there explains why this can't just be `use`d: `FomaAnalyzer`/`FomaProposer` hardcode
// `emit::emit(g)`, i.e. always `Strip`, so both this bench and pk1 re-plumb the public pieces
// against a caller-supplied network instead).
// -------------------------------------------------------------------------------------------

fn propose(net: &Fsm, word: &str) -> Vec<Candidate> {
    let normalized = hc_grammar::nfd::nfd(word);
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

fn propose_and_peel(net: &Fsm, g: &Grammar, peeler: &ReduplicationPeeler, word: &str) -> Vec<Candidate> {
    let mut candidates = propose(net, word);
    let peeled = peeler.peel_candidates(g, word, &mut |r: &str| propose(net, r));
    for c in peeled {
        let already = candidates
            .iter()
            .any(|existing| existing.root_index == c.root_index && existing.morphemes == c.morphemes);
        if !already {
            candidates.push(c);
        }
    }
    candidates
}

/// Total confirmed-analysis COUNT for `word` (flattened across all candidate buckets) -- this bench
/// only needs cardinality, not the identity `pk1`'s `confirmed_multiset` checks, so this skips that
/// function's sort/collect-into-a-comparable-key step.
fn confirmed_count(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidates: &[Candidate],
    word: &str,
) -> usize {
    confirm::confirm_batch(g, owners, morpher, candidates, word)
        .iter()
        .map(Vec::len)
        .sum()
}

// -------------------------------------------------------------------------------------------
// Per (grammar, preset) measurement.
// -------------------------------------------------------------------------------------------

struct ConstraintStats {
    total: usize,
    keep_flag: usize,
    strip: usize,
}

struct PresetRow {
    preset: PrecisionConfig,
    emit_ms: f64,
    lexc_bytes: usize,
    compile_ms: f64,
    states: i32,
    arcs: i32,
    constraints: ConstraintStats,
    propose_words_per_sec: f64,
    avg_candidates_per_word: f64,
    avg_confirmed_per_word: f64,
    confirm_total_ms: f64,
    words_scanned: usize,
}

fn measure_preset(
    g: &Grammar,
    catalog: &ConstraintCatalog,
    peeler: &ReduplicationPeeler,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    words: &[String],
    preset: PrecisionConfig,
) -> PresetRow {
    let emit_start = Instant::now();
    let EmitResult { lexc_source, .. } = emit::emit_with_precision(g, preset);
    let emit_ms = emit_start.elapsed().as_secs_f64() * 1000.0;
    let lexc_bytes = lexc_source.len();

    let opts = FomaOptions::default();
    let compile_start = Instant::now();
    let net = fsm_lexc_parse_string(&opts, None, &lexc_source)
        .unwrap_or_else(|| panic!("{preset:?}: foma failed to compile the emitted lexc source"));
    let compile_ms = compile_start.elapsed().as_secs_f64() * 1000.0;
    let states = net.statecount;
    let arcs = net.arccount;

    let report = catalog.decide(preset);
    let keep_flag = report
        .decisions
        .iter()
        .filter(|d| matches!(d.action, PrecisionAction::KeepFlag))
        .count();
    let strip = report.decisions.len() - keep_flag;
    let constraints = ConstraintStats {
        total: report.decisions.len(),
        keep_flag,
        strip,
    };

    // Proposer throughput: `apply_up` (propose only, no peel/confirm) over the corpus.
    let propose_start = Instant::now();
    let mut raw_candidate_lists: Vec<Vec<Candidate>> = Vec::with_capacity(words.len());
    for word in words {
        raw_candidate_lists.push(propose(&net, word));
    }
    let propose_elapsed = propose_start.elapsed();
    let propose_words_per_sec = if propose_elapsed.as_secs_f64() > 0.0 {
        words.len() as f64 / propose_elapsed.as_secs_f64()
    } else {
        f64::INFINITY
    };
    drop(raw_candidate_lists); // was only needed to time bare propose(); peel below redoes it per word.

    // Raw candidates/word: propose+peel, deduped (mirrors `FomaAnalyzer::analyze_word`'s assembly).
    let mut total_candidates = 0usize;
    let mut per_word_candidates: Vec<Vec<Candidate>> = Vec::with_capacity(words.len());
    for word in words {
        let cands = propose_and_peel(&net, g, peeler, word);
        total_candidates += cands.len();
        per_word_candidates.push(cands);
    }
    let avg_candidates_per_word = total_candidates as f64 / words.len().max(1) as f64;

    // Confirm: total wall time + average confirmed analyses/word.
    let confirm_start = Instant::now();
    let mut total_confirmed = 0usize;
    for (word, cands) in words.iter().zip(per_word_candidates.iter()) {
        total_confirmed += confirmed_count(g, owners, morpher, cands, word);
    }
    let confirm_total_ms = confirm_start.elapsed().as_secs_f64() * 1000.0;
    let avg_confirmed_per_word = total_confirmed as f64 / words.len().max(1) as f64;

    PresetRow {
        preset,
        emit_ms,
        lexc_bytes,
        compile_ms,
        states,
        arcs,
        constraints,
        propose_words_per_sec,
        avg_candidates_per_word,
        avg_confirmed_per_word,
        confirm_total_ms,
        words_scanned: words.len(),
    }
}

fn fmt_ratio(a: f64, b: f64) -> String {
    if b == 0.0 {
        "n/a".to_string()
    } else {
        format!("{:.2}x", a / b)
    }
}

fn print_grammar_table(spec: &GrammarSpec, rows: &[PresetRow]) {
    println!();
    println!("## {}", spec.name);
    println!(
        "(corpus cap: {} words; {} words actually scanned)",
        spec.word_cap,
        rows.first().map(|r| r.words_scanned).unwrap_or(0)
    );
    println!();
    println!(
        "| preset | emit (ms) | lexc bytes | compile (ms) | states | arcs | constraints (total/keep/strip) | propose (words/s) | avg candidates/word | avg confirmed/word | confirm total (ms) |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    for row in rows {
        println!(
            "| {} | {:.1} | {} | {:.1} | {} | {} | {}/{}/{} | {:.1} | {:.3} | {:.3} | {:.1} |",
            preset_label(row.preset),
            row.emit_ms,
            row.lexc_bytes,
            row.compile_ms,
            row.states,
            row.arcs,
            row.constraints.total,
            row.constraints.keep_flag,
            row.constraints.strip,
            row.propose_words_per_sec,
            row.avg_candidates_per_word,
            row.avg_confirmed_per_word,
            row.confirm_total_ms,
        );
    }

    if let (Some(strip), Some(allflags)) = (
        rows.iter().find(|r| matches!(r.preset, PrecisionConfig::Strip)),
        rows.iter().find(|r| matches!(r.preset, PrecisionConfig::AllFlags)),
    ) {
        println!();
        println!("Deltas (AllFlags vs Strip):");
        println!(
            "- candidates/word: {:.3} -> {:.3} ({} of Strip; the precision win)",
            strip.avg_candidates_per_word,
            allflags.avg_candidates_per_word,
            fmt_ratio(allflags.avg_candidates_per_word, strip.avg_candidates_per_word)
        );
        println!(
            "- compile time: {:.1}ms -> {:.1}ms ({} of Strip; the size cost)",
            strip.compile_ms,
            allflags.compile_ms,
            fmt_ratio(allflags.compile_ms, strip.compile_ms)
        );
        println!(
            "- states: {} -> {} ({} of Strip)",
            strip.states,
            allflags.states,
            fmt_ratio(allflags.states as f64, strip.states as f64)
        );
        println!(
            "- arcs: {} -> {} ({} of Strip)",
            strip.arcs,
            allflags.arcs,
            fmt_ratio(allflags.arcs as f64, strip.arcs as f64)
        );
        println!(
            "- avg confirmed/word: {:.3} -> {:.3} (must be equal -- recall invariance)",
            strip.avg_confirmed_per_word, allflags.avg_confirmed_per_word
        );
    }
}

fn run_grammar(spec: &GrammarSpec) {
    let Some(g) = load_grammar(spec.xml_file) else {
        println!();
        println!("## {}", spec.name);
        println!("skipped: {} not present on disk", spec.xml_file);
        return;
    };
    let Some(all_words) = read_words(spec.words_file) else {
        println!();
        println!("## {}", spec.name);
        println!("skipped: {} not present on disk", spec.words_file);
        return;
    };
    let words: Vec<String> = all_words.into_iter().take(spec.word_cap).collect();
    if words.is_empty() {
        println!();
        println!("## {}", spec.name);
        println!("skipped: {} contained no words", spec.words_file);
        return;
    }

    let catalog = ConstraintCatalog::build(&g);
    let peeler = ReduplicationPeeler::new(&g);
    let owners = confirm::build_morpheme_owners(&g);
    let morpher = Morpher::new(&g, usize::MAX).with_word_timeout(Some(ENGINE_TIMEOUT));

    let rows: Vec<PresetRow> = PRESETS
        .iter()
        .map(|&preset| measure_preset(&g, &catalog, &peeler, &owners, &morpher, &words, preset))
        .collect();

    print_grammar_table(spec, &rows);
}

/// Amharic's deep composite/rule-chain recursion (`hc-foma/src/preexpand.rs`'s depth-3 chains,
/// plus the engine's own recursive descent in `hc_rules`) overflows the default ~8MB main-thread
/// stack under a release build's larger inlined frames -- run the whole bench on a dedicated thread
/// with a generous stack instead (the same trick `f3_amharic_gate.rs`'s test-harness thread gets for
/// free from the `cargo test` runner's own stack size, which a plain `cargo run --example` binary's
/// main thread does not).
fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(run_all)
        .expect("failed to spawn bench thread");
    handle.join().expect("bench thread panicked");
}

fn run_all() {
    println!("# FST precision knob bench matrix");
    println!();
    println!(
        "Presets benched: Strip, AllFlags (FullCompile/Auto are emit-identical to Strip this step \
         -- see `hc_foma::precision`'s own doc)."
    );
    println!(
        "Timing numbers vary run to run; this is a playground, not a golden test. No timestamps in \
         this output by design."
    );

    for spec in GRAMMARS {
        run_grammar(spec);
    }
}
