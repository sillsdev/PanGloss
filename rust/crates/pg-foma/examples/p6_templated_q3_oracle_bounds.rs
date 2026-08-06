//! Diagnostic: measures the HC engine's actual standalone-rule application depth and per-rule repetition over the Aweti corpus. The only per-word cap the engine enforces is per-`MRuleId` (`rule.max_apps()`), so the recall-relevant question is how many times the same `MorphemeId` repeats in one analysis — exactly what `WordAnalysis::morpheme_ids` exposes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

fn sample_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../samples/data")
        .join(name)
}

fn load_aweti() -> Grammar {
    let path = sample_path("aweti.json");
    let json =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let snapshot = pg_snapshot::Snapshot::from_json(&json)
        .unwrap_or_else(|e| panic!("parse snapshot {}: {e}", path.display()));
    let (grammar, _warnings) = pg_grammar::compile_project(&snapshot)
        .unwrap_or_else(|e| panic!("compile_project {}: {e}", path.display()));
    grammar
}

fn main() {
    println!("=== P6 Aweti Q3: oracle rule-application bounds over the corpus ===\n");
    let g = load_aweti();

    // --- Static grammar fact: does ANY rule declare max_apps > 1? -----------------------------
    let mut max_apps_gt1: Vec<(u32, u16)> = Vec::new();
    for (i, m) in g.mrules.iter().enumerate() {
        let ma = m.max_apps();
        if ma > 1 && ma != u16::MAX {
            max_apps_gt1.push((i as u32, ma));
        }
    }
    println!(
        "rules with declared max_apps in (1, u16::MAX) = {} {:?}",
        max_apps_gt1.len(),
        max_apps_gt1
    );
    let realizational_count = g
        .mrules
        .iter()
        .filter(|m| matches!(m, pg_grammar::model::MorphRuleDef::Realizational(_)))
        .count();
    println!("Realizational rules (uncapped, max_apps()==u16::MAX by construction) = {realizational_count}\n");

    // --- Corpus sweep ---------------------------------------------------------------------------
    let words_path = sample_path("aweti-words.txt");
    let words_raw = std::fs::read_to_string(&words_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", words_path.display()));
    let words: Vec<&str> = words_raw
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    println!("corpus words: {}", words.len());

    // An uncapped `Morpher::new(&g, usize::MAX)` with no wall-clock timeout is not actually bounded for Aweti: word "tomoʼatu" ran unbounded for >10 minutes, so this uses a large-but-finite cap instead.
    let morpher = Morpher::new(&g, 20_000);
    let popts = ParseOptions::default();

    let mut max_total_affixes = 0usize; // longest morpheme_ids.len() - 1 (excluding root) over ANY analysis
    let mut max_total_affixes_word = String::new();
    let mut max_same_morpheme_repeat = 0usize; // worst same-MorphemeId repeat count within ONE analysis
    let mut max_same_morpheme_repeat_word = String::new();
    let mut words_with_no_analysis = 0usize;
    let mut words_with_repeat = 0usize;
    let mut total_analyses = 0usize;
    let mut max_prefix_affixes = 0usize;
    let mut max_suffix_affixes = 0usize;

    for (wi, &word) in words.iter().enumerate() {
        use std::io::Write;
        eprint!("[{}/{}] {word:?} ... ", wi + 1, words.len());
        std::io::stderr().flush().ok();
        let t_word = std::time::Instant::now();
        let outcome = morpher.parse_word_opts(word, &popts);
        eprintln!(
            "{} analyses in {:?}",
            outcome.structured.len(),
            t_word.elapsed()
        );
        if outcome.structured.is_empty() {
            words_with_no_analysis += 1;
            continue;
        }
        for a in &outcome.structured {
            total_analyses += 1;
            let root_idx = if a.root_morpheme_index >= 0 {
                a.root_morpheme_index as usize
            } else {
                0
            };
            let total_affixes = a.morpheme_ids.len().saturating_sub(1);
            if total_affixes > max_total_affixes {
                max_total_affixes = total_affixes;
                max_total_affixes_word = word.to_string();
            }
            if a.root_morpheme_index >= 0 {
                let prefixes = root_idx;
                let suffixes = a.morpheme_ids.len() - root_idx - 1;
                max_prefix_affixes = max_prefix_affixes.max(prefixes);
                max_suffix_affixes = max_suffix_affixes.max(suffixes);
            }
            let mut counts: HashMap<u32, usize> = HashMap::new();
            for &m in &a.morpheme_ids {
                *counts.entry(m).or_insert(0) += 1;
            }
            let worst = counts.values().copied().max().unwrap_or(1);
            if worst > 1 {
                words_with_repeat += 1;
            }
            if worst > max_same_morpheme_repeat {
                max_same_morpheme_repeat = worst;
                max_same_morpheme_repeat_word = word.to_string();
            }
        }
    }

    println!("\n--- Results ---");
    println!(
        "words with >=1 analysis: {}/{}",
        words.len() - words_with_no_analysis,
        words.len()
    );
    println!("total analyses across corpus: {total_analyses}");
    println!(
        "MAX total affix-morpheme count in any single analysis: {max_total_affixes} (word {:?})",
        max_total_affixes_word
    );
    println!("MAX prefix-side affix count (any single analysis): {max_prefix_affixes}");
    println!("MAX suffix-side affix count (any single analysis): {max_suffix_affixes}");
    println!(
        "MAX same-MorphemeId repeat count within one analysis: {max_same_morpheme_repeat} (word {:?})",
        max_same_morpheme_repeat_word
    );
    println!("analyses (not words) containing ANY repeated MorphemeId: {words_with_repeat}");
    println!(
        "\n=> the real corpus's own oracle analyses use AT MOST {max_total_affixes} total affix \
         positions and repeat the SAME morpheme at most {max_same_morpheme_repeat} time(s) per \
         analysis -- compare against deriv_prefix.len()=11 / deriv_suffix.len()=24 (p6_aweti_q2_epsilon_mass)\n\
         and the 10 / 10 independent prefix/suffix chain-instance reofferings (also p6_aweti_q2_epsilon_mass):\n\
         the CHAIN currently allows an epsilon rule's tag to be chosen up to 22/48 times on one path,\n\
         while the oracle itself never repeats a morpheme more than {max_same_morpheme_repeat} time(s)\n\
         over this whole corpus."
    );

    println!("\n=== done ===");
}
