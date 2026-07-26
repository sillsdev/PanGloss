//! DEV-ONLY measurement harness for `docs/research/spellcheck/13-first-measurements.md`.
//!
//! Not part of any production surface: an `examples/` binary, built and run manually, never
//! invoked by `pangloss` itself or by any shipped tooling. It answers report 13's questions
//! (analysis-ambiguity census, D1/D4 backoff-rung class cardinality, syn_fs/mpr population) by
//! driving the existing `pg-fwdata` + `pg-grammar` + `pg-parse::Morpher`/`hc_parse_batch` surface
//! against a real FieldWorks project's wordform inventory. It reads and reports; it does not
//! change any production semantics, gate, or budget.
//!
//! Usage:
//!   cargo run -p pg-cli --release --example spellcheck_measure -- <grammar> <wordforms.txt> [--threads N] [--step-cap N] [--word-timeout-ms N]
//!
//! `<grammar>` dispatches on extension exactly like `pg-cli`'s own `load_grammar` (src/main.rs):
//! `.fwdata` -> `pg_fwdata::import_file` + `pg_grammar::compile_project`; `.json` -> a
//! `pg_snapshot::Snapshot` + `pg_grammar::compile_project`; anything else (`.xml`) -> the legacy
//! `pg_grammar::load`.
//!
//! `<wordforms.txt>` is one surface wordform per line (no analyses attached — this harness
//! re-derives analyses itself via the named "Rust HermitCrab-only" pipeline, `pg_parse::Morpher`
//! — see the report for why that pipeline was chosen over `--engine=foma`).

use std::collections::HashMap;
use std::fs;
use std::time::Duration;

use pg_featstruct::FeatureValue;
use pg_grammar::model::Grammar;
use pg_parse::{hc_parse_batch, Morpher, WordAnalysis};

/// Mirrors `pg-cli`'s `load_grammar` dispatch exactly (src/main.rs) so this harness accepts the
/// same three grammar-path shapes the production CLI does.
fn load_grammar(path: &str) -> (Grammar, Vec<String>) {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "json" => {
            let json = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
            let snapshot = pg_snapshot::Snapshot::from_json(&json)
                .unwrap_or_else(|e| panic!("parse snapshot {path}: {e}"));
            let (grammar, warnings) = pg_grammar::compile_project(&snapshot)
                .unwrap_or_else(|e| panic!("compile {path}: {e:?}"));
            (grammar, warnings)
        }
        "fwdata" => {
            let (snapshot, report) = pg_fwdata::import_file(std::path::Path::new(path))
                .unwrap_or_else(|e| panic!("import {path}: {e}"));
            let mut warnings = report.warnings;
            warnings.extend(snapshot.validate());
            let (grammar, compile_warnings) = pg_grammar::compile_project(&snapshot)
                .unwrap_or_else(|e| panic!("compile {path}: {e:?}"));
            warnings.extend(compile_warnings);
            (grammar, warnings)
        }
        _ => {
            let xml = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
            let grammar =
                pg_grammar::load(&xml).unwrap_or_else(|e| panic!("load {path}: {e:?}"));
            (grammar, Vec::new())
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut positional: Vec<&str> = Vec::new();
    let mut threads: usize = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let mut step_cap: usize = 200_000;
    let mut word_timeout_ms: u64 = 10_000;

    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--threads" => threads = it.next().unwrap().parse().unwrap(),
            "--step-cap" => step_cap = it.next().unwrap().parse().unwrap(),
            "--word-timeout-ms" => word_timeout_ms = it.next().unwrap().parse().unwrap(),
            "--dump-pos-only" => {}
            s => positional.push(s),
        }
    }
    let [fwdata_path, words_path] = positional[..] else {
        eprintln!("usage: spellcheck_measure <grammar (.fwdata|.json|.xml)> <wordforms.txt> [--threads N] [--step-cap N] [--word-timeout-ms N]");
        std::process::exit(2);
    };

    eprintln!("loading grammar from {fwdata_path} ...");
    let t0 = std::time::Instant::now();
    let (grammar, warnings) = load_grammar(fwdata_path);
    for w in &warnings {
        eprintln!("load/compile warning: {w}");
    }
    eprintln!(
        "grammar loaded+compiled in {:.1}s: {} lex entries, {} morphemes, {} syn features, {} mpr features",
        t0.elapsed().as_secs_f64(),
        grammar.entries.len(),
        grammar.morphemes.len(),
        grammar.syn_features.features.len(),
        grammar.mpr_names.len(),
    );

    // Print the syn feature inventory once, up front -- this is D1's "exact syn_fs feature
    // inventory actually populated" question's static half (what the grammar CAN carry); the
    // dynamic half (what confirmed analyses actually DO carry) is measured below.
    eprintln!("\n--- syn_features declared ({}) ---", grammar.syn_features.features.len());
    for (i, f) in grammar.syn_features.features.iter().enumerate() {
        match &f.kind {
            pg_grammar::model::SynFeatureKind::Symbolic { symbols, .. } => {
                eprintln!("  [{i}] {} ({}) -- symbolic, {} symbols", f.name, f.xml_id, symbols.len());
            }
            pg_grammar::model::SynFeatureKind::Complex => {
                eprintln!("  [{i}] {} ({}) -- complex", f.name, f.xml_id);
            }
        }
    }
    eprintln!("pos feature id = {}", grammar.syn_features.pos.0);
    if let Some(h) = grammar.syn_features.head {
        eprintln!("head feature id = {}", h.0);
    }
    if let Some(ft) = grammar.syn_features.foot {
        eprintln!("foot feature id = {}", ft.0);
    }
    eprintln!("mpr_names ({}): {:?}", grammar.mpr_names.len(), grammar.mpr_names);
    if let pg_grammar::model::SynFeatureKind::Symbolic { symbols, .. } =
        &grammar.syn_features.features[grammar.syn_features.pos.0 as usize].kind
    {
        eprintln!("POS symbols ({}):", symbols.len());
        for (i, (xml_id, name)) in symbols.iter().enumerate() {
            eprintln!("  [{i}] xml_id={xml_id:?} name={name:?}");
        }
    }
    if args.iter().any(|a| a == "--dump-pos-only") {
        return;
    }

    let words: Vec<String> = fs::read_to_string(words_path)
        .expect("read wordforms")
        .lines()
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect();
    eprintln!("\nloaded {} wordforms from {words_path}", words.len());

    let morpher = Morpher::new(&grammar, step_cap)
        .with_memo(true)
        .with_word_timeout(Some(Duration::from_millis(word_timeout_ms)));

    eprintln!(
        "parsing {} words (threads={threads}, step_cap={step_cap}, word_timeout_ms={word_timeout_ms}) ...",
        words.len()
    );
    let t1 = std::time::Instant::now();
    let results = hc_parse_batch(&morpher, &words, threads);
    eprintln!("batch parse complete in {:.1}s", t1.elapsed().as_secs_f64());

    // --- Aggregate ---
    let mut invalid_shape = 0usize;
    let mut timed_out = 0usize;
    let mut capped = 0usize;
    let mut zero_analyses = 0usize; // valid shape, not capped/timed-out, but 0 confirmed analyses
    let mut ambiguity: Vec<usize> = Vec::new(); // analyses-per-word, for words with >=1 analysis

    let mut total_analyses = 0usize;
    let mut analyses_with_syn_fs_beyond_pos = 0usize;
    let mut analyses_with_nonempty_mpr = 0usize;
    let mut mpr_union: u64 = 0;
    let mut guessed_analyses = 0usize;

    // Backoff-rung class tables (D1/D4). Key -> count of analyses in that class.
    let mut rung1_full_decomp_synfs: HashMap<String, u32> = HashMap::new(); // morpheme seq + full syn_fs
    let mut rung2_pos_synfs: HashMap<String, u32> = HashMap::new(); // pos + full syn_fs
    let mut rung3_pos_head: HashMap<String, u32> = HashMap::new(); // pos + head features only (approx feature subset)
    let mut rung4_pos_mpr: HashMap<(Option<u32>, u64), u32> = HashMap::new(); // pos + mpr
    let mut rung5_pos: HashMap<Option<u32>, u32> = HashMap::new(); // pos alone
    let mut rung6_openclosed: HashMap<&'static str, u32> = HashMap::new(); // open/closed

    let mut feature_symbol_cardinality: HashMap<u16, std::collections::HashSet<u32>> = HashMap::new();
    let mut feature_occurrence_count: HashMap<u16, u32> = HashMap::new();

    // Open-class heuristic (documented in the report as [S], and NOT robust across grammars --
    // see the report's explicit caveat on rung 6): match on the grammar's own declared POS
    // *names*, since neither pg-grammar's Grammar nor pg-fwdata's Snapshot mark open/closed
    // explicitly, and each reference grammar abbreviates its tagset differently. Sena/Amharic/
    // Indonesian all mark verb subtypes with a LEADING v/V (Vaux, Vrel, v.pfv, v.conv...);
    // Aweti instead marks them with a TRAILING V (STV, ACTV, INTV, TRV) -- both conventions are
    // covered here, but a fifth grammar could use a convention this still misses. Exact-match a
    // small canonical open set (N/V/Adj/Adv), plus treat any label starting OR ending with "v",
    // or containing "irreg", as an open verbal subtype.
    let open_exact = ["n", "v", "adj", "adv"];
    let is_open_pos = |name: &str| {
        let l = name.to_lowercase();
        open_exact.contains(&l.as_str())
            || l.starts_with('v')
            || l.ends_with('v')
            || l.contains("irreg")
    };
    let pos_symbol_name = |pos_id: Option<u32>| -> Option<&str> {
        let id = pos_id?;
        match &grammar.syn_features.features[grammar.syn_features.pos.0 as usize].kind {
            pg_grammar::model::SynFeatureKind::Symbolic { symbols, .. } => {
                symbols.get(id as usize).map(|(_, name)| name.as_str())
            }
            _ => None,
        }
    };

    fn syn_fs_key(fs: &pg_featstruct::FeatureStruct) -> String {
        // Deterministic string key: sorted entries already guaranteed by FeatureStruct's own
        // invariant; format is (featid:value) pairs. Complex values recurse.
        fn fmt(fs: &pg_featstruct::FeatureStruct) -> String {
            let mut parts = Vec::new();
            for (feat, val) in fs.entries() {
                match val {
                    FeatureValue::Symbolic(bits) => parts.push(format!("{}={:x}", feat.0, bits.raw())),
                    FeatureValue::Complex(inner) => parts.push(format!("{}=({})", feat.0, fmt(inner))),
                }
            }
            parts.join(",")
        }
        fmt(fs)
    }

    fn head_only_key(g: &pg_grammar::model::Grammar, fs: &pg_featstruct::FeatureStruct) -> String {
        // Approximation of D4's rung 3 ("POS + a selected feature subset"): here, POS + the head
        // complex feature only (excluding foot), since no per-grammar feature-subset selection
        // has been made -- see report 13's explicit caveat about this being an approximation.
        let mut parts = Vec::new();
        if let Some(pos_val) = fs.get(g.syn_features.pos) {
            if let FeatureValue::Symbolic(bits) = pos_val {
                parts.push(format!("pos={:x}", bits.raw()));
            }
        }
        if let Some(head_id) = g.syn_features.head {
            if let Some(head_val) = fs.get(head_id) {
                match head_val {
                    FeatureValue::Complex(inner) => parts.push(format!("head=({})", syn_fs_key(inner))),
                    FeatureValue::Symbolic(bits) => parts.push(format!("head={:x}", bits.raw())),
                }
            }
        }
        parts.join(",")
    }

    fn record_feature_occurrences(
        fs: &pg_featstruct::FeatureStruct,
        card: &mut HashMap<u16, std::collections::HashSet<u32>>,
        occ: &mut HashMap<u16, u32>,
    ) {
        for (feat, val) in fs.entries() {
            *occ.entry(feat.0).or_insert(0) += 1;
            match val {
                FeatureValue::Symbolic(bits) => {
                    let set = card.entry(feat.0).or_default();
                    let mut b = bits.raw();
                    while b != 0 {
                        let idx = b.trailing_zeros();
                        set.insert(idx);
                        b &= b - 1;
                    }
                }
                FeatureValue::Complex(inner) => record_feature_occurrences(inner, card, occ),
            }
        }
    }

    // Per-POS breakdown: (total analyses, analyses with syn_fs beyond bare POS) -- tests whether
    // syn_fs richness is concentrated in particular POS categories (e.g. nominal agreement vs.
    // bare verbal forms) rather than uniformly thin/rich across the whole tagset.
    let mut per_pos: HashMap<Option<u32>, (u32, u32)> = HashMap::new();

    let mut per_word_rows: Vec<String> = Vec::with_capacity(words.len());

    for (word, r) in words.iter().zip(results.iter()) {
        let o = &r.outcome;
        if o.invalid_shape {
            invalid_shape += 1;
            per_word_rows.push(format!("{word}\tINVALID_SHAPE\t0"));
            continue;
        }
        if o.timed_out {
            timed_out += 1;
            per_word_rows.push(format!("{word}\tTIMEOUT\t0"));
            continue;
        }
        if o.capped {
            capped += 1;
        }
        let n = o.structured.len();
        if n == 0 {
            zero_analyses += 1;
        } else {
            ambiguity.push(n);
        }
        per_word_rows.push(format!("{word}\tOK\t{n}"));

        for a in &o.structured {
            total_analyses += 1;
            let WordAnalysis {
                pos_id,
                syn_fs,
                mpr,
                guessed,
                morpheme_ids,
                ..
            } = a;
            if *guessed {
                guessed_analyses += 1;
            }
            let beyond_pos = syn_fs.len() > 1
                || (syn_fs.len() == 1 && syn_fs.get(grammar.syn_features.pos).is_none());
            if beyond_pos {
                analyses_with_syn_fs_beyond_pos += 1;
            }
            {
                let entry = per_pos.entry(*pos_id).or_insert((0, 0));
                entry.0 += 1;
                if beyond_pos {
                    entry.1 += 1;
                }
            }
            if mpr.0 != 0 {
                analyses_with_nonempty_mpr += 1;
                mpr_union |= mpr.0;
            }
            record_feature_occurrences(syn_fs, &mut feature_symbol_cardinality, &mut feature_occurrence_count);

            // Rung 1: full morpheme decomposition + full syn_fs.
            let morph_key = morpheme_ids
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("+");
            let r1key = format!("{morph_key}|{}", syn_fs_key(syn_fs));
            *rung1_full_decomp_synfs.entry(r1key).or_insert(0) += 1;

            // Rung 2: POS + full syn_fs.
            let r2key = format!("{:?}|{}", pos_id, syn_fs_key(syn_fs));
            *rung2_pos_synfs.entry(r2key).or_insert(0) += 1;

            // Rung 3: POS + head-only (approximation of a selected feature subset).
            let r3key = head_only_key(&grammar, syn_fs);
            *rung3_pos_head.entry(r3key).or_insert(0) += 1;

            // Rung 4: POS + mpr.
            *rung4_pos_mpr.entry((*pos_id, mpr.0)).or_insert(0) += 1;

            // Rung 5: POS alone.
            *rung5_pos.entry(*pos_id).or_insert(0) += 1;

            // Rung 6: open/closed.
            let bucket = match pos_symbol_name(*pos_id) {
                Some(name) if is_open_pos(name) => "open",
                Some(_) => "closed",
                None => "unknown_pos",
            };
            *rung6_openclosed.entry(bucket).or_insert(0) += 1;
        }
    }

    // --- Report ---
    println!("=== spellcheck_measure report ===");
    println!("grammar: {fwdata_path}");
    println!("wordforms input: {words_path} ({} words)", words.len());
    println!(
        "pipeline: pg_parse::Morpher (Rust-HermitCrab-only, full-search) via hc_parse_batch, threads={threads}, step_cap={step_cap}, word_timeout_ms={word_timeout_ms}"
    );
    println!();
    println!("--- Coverage ---");
    println!("total words: {}", words.len());
    println!(
        "invalid_shape (unsegmentable): {invalid_shape} ({:.2}%)",
        pct(invalid_shape, words.len())
    );
    println!(
        "timed_out (word_timeout_ms exceeded): {timed_out} ({:.2}%)",
        pct(timed_out, words.len())
    );
    println!("capped (step_cap hit, partial result kept): {capped}");
    println!(
        "zero_analyses (valid shape, 0 confirmed analyses): {zero_analyses} ({:.2}%)",
        pct(zero_analyses, words.len())
    );
    println!(
        "words with >=1 analysis: {} ({:.2}%)",
        ambiguity.len(),
        pct(ambiguity.len(), words.len())
    );
    println!();

    println!("--- Ambiguity census (analyses per word, words with >=1 analysis only) ---");
    if !ambiguity.is_empty() {
        let mut sorted = ambiguity.clone();
        sorted.sort_unstable();
        let sum: usize = sorted.iter().sum();
        let mean = sum as f64 / sorted.len() as f64;
        let median = percentile(&sorted, 0.50);
        let p90 = percentile(&sorted, 0.90);
        let p99 = percentile(&sorted, 0.99);
        let max = *sorted.last().unwrap();
        println!("n = {}", sorted.len());
        println!("mean = {mean:.3}");
        println!("median (p50) = {median}");
        println!("p90 = {p90}");
        println!("p99 = {p99}");
        println!("max = {max}");
        // Histogram of small counts, then a tail bucket.
        let mut hist: HashMap<usize, u32> = HashMap::new();
        for &v in &sorted {
            *hist.entry(v).or_insert(0) += 1;
        }
        let mut keys: Vec<usize> = hist.keys().copied().collect();
        keys.sort_unstable();
        for k in keys.iter().take(20) {
            println!("  analyses={k}: {} words", hist[k]);
        }
        if keys.len() > 20 {
            println!("  ... ({} more distinct analysis-counts up to {})", keys.len() - 20, max);
        }
    } else {
        println!("(no words produced any analysis)");
    }
    println!();

    println!("--- syn_fs / mpr population (over {total_analyses} total confirmed analyses) ---");
    println!(
        "analyses with syn_fs carrying more than bare POS: {analyses_with_syn_fs_beyond_pos} ({:.2}%)",
        pct(analyses_with_syn_fs_beyond_pos, total_analyses)
    );
    println!(
        "analyses with nonempty mpr: {analyses_with_nonempty_mpr} ({:.2}%)",
        pct(analyses_with_nonempty_mpr, total_analyses)
    );
    println!(
        "distinct mpr bits ever observed (popcount of union): {}",
        mpr_union.count_ones()
    );
    println!(
        "guessed analyses: {guessed_analyses} ({:.2}%)",
        pct(guessed_analyses, total_analyses)
    );
    println!();
    println!("Per-feature occurrence + observed-value cardinality (feature id -> name):");
    let mut feat_ids: Vec<u16> = feature_occurrence_count.keys().copied().collect();
    feat_ids.sort_unstable();
    for fid in feat_ids {
        let name = grammar
            .syn_features
            .features
            .get(fid as usize)
            .map(|f| f.name.as_str())
            .unwrap_or("?");
        let occ = feature_occurrence_count.get(&fid).copied().unwrap_or(0);
        let card = feature_symbol_cardinality.get(&fid).map(|s| s.len()).unwrap_or(0);
        println!(
            "  [{fid}] {name}: occurs in {occ} analyses ({:.2}% of confirmed), {card} distinct symbol-values observed",
            pct(occ as usize, total_analyses)
        );
    }
    println!();

    println!("--- D1/D4 backoff-rung class cardinality (over {total_analyses} total confirmed analyses) ---");
    report_rung("rung1_full_decomp+full_synfs", rung1_full_decomp_synfs.len(), &rung1_full_decomp_synfs.values().copied().collect::<Vec<_>>(), total_analyses);
    report_rung("rung2_pos+full_synfs", rung2_pos_synfs.len(), &rung2_pos_synfs.values().copied().collect::<Vec<_>>(), total_analyses);
    report_rung("rung3_pos+head_only(approx)", rung3_pos_head.len(), &rung3_pos_head.values().copied().collect::<Vec<_>>(), total_analyses);
    report_rung("rung4_pos+mpr", rung4_pos_mpr.len(), &rung4_pos_mpr.values().copied().collect::<Vec<_>>(), total_analyses);
    report_rung("rung5_pos_alone", rung5_pos.len(), &rung5_pos.values().copied().collect::<Vec<_>>(), total_analyses);
    report_rung("rung6_open_closed", rung6_openclosed.len(), &rung6_openclosed.values().copied().collect::<Vec<_>>(), total_analyses);
    println!();
    println!("rung6 buckets: {rung6_openclosed:?}");
    println!();

    println!("--- Per-POS syn_fs population (rung5 class contents, sorted by size desc) ---");
    let mut pos_rows: Vec<(Option<u32>, u32, u32)> = per_pos
        .iter()
        .map(|(&pos, &(total, beyond))| (pos, total, beyond))
        .collect();
    pos_rows.sort_by_key(|&(_, total, _)| std::cmp::Reverse(total));
    for (pos, total, beyond) in pos_rows {
        let name = pos_symbol_name(pos).unwrap_or("?");
        println!(
            "  pos={pos:?} ({name}): {total} analyses, {beyond} ({:.1}%) carry syn_fs beyond bare POS",
            pct(beyond as usize, total as usize)
        );
    }

    // Dump per-word rows for independent inspection.
    let out_dump = "spellcheck_measure_perword.tsv";
    fs::write(out_dump, per_word_rows.join("\n")).ok();
    eprintln!("\nwrote per-word rows to {out_dump}");
}

fn pct(n: usize, d: usize) -> f64 {
    if d == 0 {
        0.0
    } else {
        100.0 * n as f64 / d as f64
    }
}

fn percentile(sorted: &[usize], p: f64) -> usize {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn report_rung(name: &str, distinct_classes: usize, sizes: &[u32], total_analyses: usize) {
    let max = sizes.iter().copied().max().unwrap_or(0);
    let min = sizes.iter().copied().min().unwrap_or(0);
    let mean = if sizes.is_empty() {
        0.0
    } else {
        sizes.iter().sum::<u32>() as f64 / sizes.len() as f64
    };
    let singleton_classes = sizes.iter().filter(|&&s| s == 1).count();
    println!(
        "{name}: {distinct_classes} distinct classes over {total_analyses} analyses (mean class size {mean:.2}, min {min}, max {max}, {singleton_classes} singleton classes = {:.2}% of classes)",
        pct(singleton_classes, distinct_classes)
    );
}
