//! E2 feasibility probe driver (`hc_foma::e2_infix_probe`, not mainline): builds the composed
//! network `underlying-lexc(+infix-composites) .o. rules .o. boundary-cleanup` for Amharic, then
//! runs the corpus parity gate against `hc_parse::Morpher` (same oracle shape as
//! `examples/p6_replace_prototype.rs`), broken out by infix-bearing vs non-infix-bearing words —
//! the exact split the E2 build session's census found (43/79 infix-bearing, 36/79 non-infix, on
//! the first 300 corpus words).
//!
//! Run: `cargo run --release -p hc-foma --example e2_infix_probe_amharic`

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use foma::apply::apply_init;
use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;

use hc_foma::e2_infix_probe::emit_underlying_amharic_probe;
use hc_foma::replace::{compile_and_compose_rules, SegAlphabet};
use hc_foma::tags;
use hc_grammar::chardef::CharDefKind;
use hc_grammar::model::{Grammar, MorphRuleDef, OutputAction, PartRef, PhonRuleDef};
use hc_parse::{Morpher, ParseOptions};

const STACK_BYTES: usize = 512 * 1024 * 1024;
const ENGINE_TIMEOUT: Duration = Duration::from_secs(5);

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

/// Same pinned-outlier convention `examples/deadend_census.rs::read_pinned` uses (see the E2 build
/// session's log for why: every time the corpus sample widened this session, a new gap class
/// surfaced -- this grammar's known-hard cases specifically live in the tail, which is exactly what
/// the pinned worst-words fixture exists to guarantee gets exercised regardless of where a `take`
/// cap would otherwise cut the sample). `#`-prefixed lines are provenance comments; skipped. Absent
/// file => empty, never an error.
fn read_pinned(words_file: &str) -> Vec<String> {
    let base = words_file.strip_suffix("-words.txt").unwrap_or(words_file);
    let path = sample_path(&format!("{base}-worst-words.txt"));
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|w| !w.is_empty() && !w.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn load_amharic() -> Grammar {
    let path = sample_path("amharic-hc.xml");
    let xml = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load amharic-hc.xml: {e}"))
}

fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run)
        .expect("spawn large-stack worker thread");
    handle.join().expect("worker thread panicked");
}

// --- local Role classification (duplicated from crate::emit -- pub(crate) there, not visible from
// an example binary; kept intentionally identical in shape to that module's `classify_affix`). ---

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Role { None, Prefix, Suffix, Infix, Reduplication, CircumfixPrefix, Process }

fn classify_affix(rhs: &[OutputAction]) -> Role {
    let copy_parts: Vec<PartRef> = rhs.iter().filter_map(|a| if let OutputAction::Copy(p) = a { Some(*p) } else { None }).collect();
    if copy_parts.iter().any(|p| copy_parts.iter().filter(|&&q| q == *p).count() >= 2) {
        return Role::Reduplication;
    }
    let mut first_copy: Option<usize> = None;
    let mut last_copy: usize = 0;
    for (i, action) in rhs.iter().enumerate() {
        if matches!(action, OutputAction::Copy(_)) {
            if first_copy.is_none() { first_copy = Some(i); }
            last_copy = i;
        }
    }
    let Some(first_copy) = first_copy else {
        return if rhs.iter().any(|a| matches!(a, OutputAction::Modify(_, _))) { Role::Process } else { Role::None };
    };
    if first_copy < last_copy {
        for action in &rhs[first_copy + 1..last_copy] {
            if !matches!(action, OutputAction::Copy(_)) { return Role::Infix; }
        }
    }
    let leading_insert = first_copy > 0;
    let trailing_insert = last_copy < rhs.len() - 1;
    if leading_insert && trailing_insert { Role::CircumfixPrefix }
    else if leading_insert { Role::Prefix }
    else if trailing_insert { Role::Suffix }
    else { Role::None }
}

fn allomorphs_of(g: &Grammar, def_idx: usize) -> &[hc_grammar::model::AffixAllomorphDef] {
    match &g.mrules[def_idx] {
        MorphRuleDef::AffixProcess(def) => &def.allomorphs,
        MorphRuleDef::Realizational(def) => &def.allomorphs,
        MorphRuleDef::Compounding(_) => &[],
    }
}

fn rule_role(g: &Grammar, def_idx: usize) -> Role {
    allomorphs_of(g, def_idx).first().map(|a| classify_affix(&a.rhs)).unwrap_or(Role::None)
}

fn engine_sequences(outcome: &hc_parse::ParseOutcome) -> Vec<(Vec<u32>, i32)> {
    let mut seqs: Vec<(Vec<u32>, i32)> = Vec::new();
    for a in &outcome.structured {
        let key = (a.morpheme_ids.clone(), a.root_morpheme_index);
        if !seqs.contains(&key) {
            seqs.push(key);
        }
    }
    seqs
}

fn candidates_cover(candidates: &[tags::Candidate], seq: &[u32], root_idx: i32) -> bool {
    candidates.iter().any(|c| {
        c.root_index == root_idx
            && c.morphemes.len() == seq.len()
            && c.morphemes.iter().zip(seq.iter()).all(|(m, s)| m.0 == *s)
    })
}

fn run() {
    println!("=== E2 feasibility probe: Amharic Infix splice + replace-rule cascade ===\n");

    let g = load_amharic();
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();

    // Infix morphemes -- for the per-word infix/non-infix split below.
    let mut infix_morphemes: HashSet<u32> = HashSet::new();
    for (mid, mrule) in g.mrules.iter().enumerate() {
        if matches!(mrule, MorphRuleDef::Compounding(_)) {
            continue;
        }
        if rule_role(&g, mid) == Role::Infix {
            let m = match mrule {
                MorphRuleDef::AffixProcess(def) => def.morpheme,
                MorphRuleDef::Realizational(def) => def.morpheme,
                MorphRuleDef::Compounding(_) => unreachable!(),
            };
            infix_morphemes.insert(m.0);
        }
    }
    println!("infix morphemes: {:?}", infix_morphemes);

    // ---------------------------------------------------------------------------------------
    // 1. Underlying-form lexc emit (templated skeleton + infix composites).
    // ---------------------------------------------------------------------------------------
    let t_emit = Instant::now();
    let ureport = emit_underlying_amharic_probe(&g, &alphabet);
    let emit_elapsed = t_emit.elapsed();
    println!(
        "underlying lexc emit: {emit_elapsed:?}; roots={} special_rules={} splice_composites={} \
         splice_pairs_probed={} splice_ambiguous_pairs={}",
        ureport.root_count, ureport.special_rule_count, ureport.splice_composite_count,
        ureport.splice_pairs_probed, ureport.splice_ambiguous_pairs
    );
    println!("uncovered ({}):", ureport.uncovered.len());
    for u in &ureport.uncovered {
        println!("  {u}");
    }

    let t_lexc = Instant::now();
    let lexc_net = fsm_lexc_parse_string(&opts, None, &ureport.lexc_source)
        .unwrap_or_else(|| panic!("underlying-form lexc failed to compile"));
    let lexc_elapsed = t_lexc.elapsed();
    println!(
        "lexc compile: {lexc_elapsed:?}; net: {} states, {} arcs; lexc bytes: {}",
        lexc_net.statecount, lexc_net.arccount, ureport.lexc_source.len()
    );

    // ---------------------------------------------------------------------------------------
    // 2. Rule cascade compile+compose (already proven: p6_amharic_probe.rs, 2.14s, 82st/1.1M arcs).
    // ---------------------------------------------------------------------------------------
    let mut rules_in_order: Vec<&PhonRuleDef> = Vec::new();
    for st in &g.strata {
        for &prid in &st.prules {
            rules_in_order.push(&g.prules[prid.0 as usize]);
        }
    }
    let t_rules = Instant::now();
    let mut skipped_rules: Vec<String> = Vec::new();
    let mut tuple_reports: Vec<(String, Vec<hc_foma::replace::TupleReport>)> = Vec::new();
    let rule_net = compile_and_compose_rules(&opts, &g, &alphabet, &rules_in_order, &mut skipped_rules, &mut tuple_reports);
    let rules_elapsed = t_rules.elapsed();
    println!("\nrule compile+compose: {rules_elapsed:?}; skipped: {skipped_rules:?}");
    let rule_net = rule_net.expect("Amharic's 7 rules must compile (see skipped_rules if not)");
    println!("composed rule net: {} states, {} arcs", rule_net.statecount, rule_net.arccount);

    // ---------------------------------------------------------------------------------------
    // 3. Boundary cleanup, compose, minimize.
    // ---------------------------------------------------------------------------------------
    let boundary_tokens: Vec<char> = table
        .iter()
        .filter(|(_, cd)| cd.kind() == CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect();
    let cleanup_regex = boundary_tokens.iter().map(|c| format!("{c} -> 0")).collect::<Vec<_>>().join(", ");
    let cleanup_net = fsm_parse_regex(&opts, &cleanup_regex, None, None)
        .unwrap_or_else(|| panic!("boundary cleanup regex failed to compile: {cleanup_regex:?}"));

    let t_compose = Instant::now();
    let composed = fsm_compose(&opts, lexc_net, rule_net);
    let composed = fsm_compose(&opts, composed, cleanup_net);
    let composed = fsm_minimize(&opts, composed);
    let compose_elapsed = t_compose.elapsed();
    println!(
        "\nfull composition + minimize: {compose_elapsed:?}; final net: {} states, {} arcs",
        composed.statecount, composed.arccount
    );

    let mut handle = apply_init(&composed);

    // ---------------------------------------------------------------------------------------
    // 4. Corpus parity gate, split by infix-bearing vs non-infix-bearing, vs the real engine.
    // ---------------------------------------------------------------------------------------
    let morpher = Morpher::new(&g, usize::MAX).with_word_timeout(Some(ENGINE_TIMEOUT));
    let popts = ParseOptions::default();
    let words_text = std::fs::read_to_string(sample_path("amharic-words.txt")).expect("read amharic-words.txt");
    let mut words: Vec<String> = words_text.lines().map(str::trim).filter(|w| !w.is_empty()).map(str::to_string).collect();
    let already: HashSet<String> = words.iter().cloned().collect();
    let pinned_extra: Vec<String> = read_pinned("amharic-words.txt")
        .into_iter()
        .filter(|w| !already.contains(w))
        .collect();
    println!(
        "\n--- corpus parity gate (FULL corpus: {} words + {} pinned worst-words not already in \
         the front slice) ---",
        words.len(), pinned_extra.len()
    );
    words.extend(pinned_extra);

    let mut n_total_infix = 0usize;
    let mut n_covered_infix = 0usize;
    let mut n_total_plain = 0usize;
    let mut n_covered_plain = 0usize;
    let mut misses: Vec<String> = Vec::new();
    let mut n_words_analyzed = 0usize;
    let mut propose_time = Duration::ZERO;
    let mut max_propose = Duration::ZERO;
    let mut total_overgenerated = 0usize;

    for word in &words {
        let outcome = morpher.parse_word_opts(word, &popts);
        if outcome.structured.is_empty() {
            continue;
        }
        n_words_analyzed += 1;

        let t0 = Instant::now();
        let candidates: Vec<tags::Candidate> = match alphabet.encode_query(word) {
            Some(query) => {
                let mut out = Vec::new();
                let mut seen = HashSet::new();
                for s in handle.up(&query) {
                    if let Some(path) = tags::decode_path(&s) {
                        for c in tags::to_candidates(&path) {
                            let key: (Vec<u32>, i32) = (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
                            if seen.insert(key) {
                                out.push(c);
                            }
                        }
                    }
                }
                out
            }
            None => Vec::new(),
        };
        let dt = t0.elapsed();
        propose_time += dt;
        max_propose = max_propose.max(dt);
        total_overgenerated += candidates.len();

        for (seq, root_idx) in engine_sequences(&outcome) {
            let is_infix = seq.iter().any(|id| infix_morphemes.contains(id));
            let covered = candidates_cover(&candidates, &seq, root_idx);
            if is_infix {
                n_total_infix += 1;
                if covered { n_covered_infix += 1; }
            } else {
                n_total_plain += 1;
                if covered { n_covered_plain += 1; }
            }
            if !covered {
                let names: Vec<String> = seq.iter().map(|&id| {
                    g.morphemes.get(id as usize).map(|mi| format!("{}({}/{})", id, mi.xml_key, mi.gloss.as_deref().unwrap_or("-"))).unwrap_or_else(|| format!("{id}(?)"))
                }).collect();
                misses.push(format!(
                    "word {word:?} [{}]: engine analysis root_index={root_idx} morphemes=[{}]",
                    if is_infix { "INFIX" } else { "plain" }, names.join(", ")
                ));
            }
        }
    }

    println!("words analyzed by engine: {n_words_analyzed} (of {} scanned)", words.len());
    println!("INFIX-bearing analyses:  {n_covered_infix}/{n_total_infix} covered");
    println!("plain (non-infix) analyses: {n_covered_plain}/{n_total_plain} covered");
    println!(
        "propose total: {propose_time:?}; max/word: {max_propose:?}; mean/word: {:?}; \
         total candidates (overgeneration): {total_overgenerated}",
        propose_time / (n_words_analyzed.max(1) as u32)
    );
    if !misses.is_empty() {
        println!("--- MISSES ({}) ---", misses.len());
        for m in &misses {
            println!("MISS {m}");
        }
    } else {
        println!("ZERO misses.");
    }

    println!("\n=== done ===");
}
