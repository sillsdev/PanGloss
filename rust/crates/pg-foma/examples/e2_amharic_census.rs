//! E2 scoping census (not mainline, standalone diagnostic): quantify how much of Amharic's corpus
//! recall depends on constructs the P6 replace-rule + concatenative-lexc architecture cannot
//! represent (Role::Infix interdigitation, Role::CircumfixPrefix, process morphs), so the E2 build
//! decision ("implement interdigitation" vs "accept Partial and decline / scope to non-infix
//! subset") is made with real numbers instead of a guess.
//!
//! Run: `cargo run --release -p pg-foma --example e2_amharic_census`

use std::path::{Path, PathBuf};

use pg_grammar::model::{Grammar, MorphRuleDef, MorphemeId, OutputAction, PartRef};
use pg_parse::{Morpher, ParseOptions};

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

fn load_amharic() -> Grammar {
    let path = sample_path("amharic-hc.xml");
    let xml = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load amharic-hc.xml: {e}"))
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Role {
    None,
    Prefix,
    Suffix,
    Infix,
    Reduplication,
    CircumfixPrefix,
    Process,
}

fn classify_affix(rhs: &[OutputAction]) -> Role {
    let copy_parts: Vec<PartRef> = rhs
        .iter()
        .filter_map(|a| if let OutputAction::Copy(p) = a { Some(*p) } else { None })
        .collect();
    if copy_parts
        .iter()
        .any(|p| copy_parts.iter().filter(|&&q| q == *p).count() >= 2)
    {
        return Role::Reduplication;
    }
    let mut first_copy: Option<usize> = None;
    let mut last_copy: usize = 0;
    for (i, action) in rhs.iter().enumerate() {
        if matches!(action, OutputAction::Copy(_)) {
            if first_copy.is_none() {
                first_copy = Some(i);
            }
            last_copy = i;
        }
    }
    let Some(first_copy) = first_copy else {
        return if rhs.iter().any(|a| matches!(a, OutputAction::Modify(_, _))) {
            Role::Process
        } else {
            Role::None
        };
    };
    if first_copy < last_copy {
        for action in &rhs[first_copy + 1..last_copy] {
            if !matches!(action, OutputAction::Copy(_)) {
                return Role::Infix;
            }
        }
    }
    let leading_insert = first_copy > 0;
    let trailing_insert = last_copy < rhs.len() - 1;
    if leading_insert && trailing_insert {
        Role::CircumfixPrefix
    } else if leading_insert {
        Role::Prefix
    } else if trailing_insert {
        Role::Suffix
    } else {
        Role::None
    }
}

fn owning_morpheme(g: &Grammar, def_idx: usize) -> Option<MorphemeId> {
    match &g.mrules[def_idx] {
        MorphRuleDef::AffixProcess(def) => Some(def.morpheme),
        MorphRuleDef::Realizational(def) => Some(def.morpheme),
        MorphRuleDef::Compounding(_) => None,
    }
}

fn allomorphs_of(g: &Grammar, def_idx: usize) -> &[pg_grammar::model::AffixAllomorphDef] {
    match &g.mrules[def_idx] {
        MorphRuleDef::AffixProcess(def) => &def.allomorphs,
        MorphRuleDef::Realizational(def) => &def.allomorphs,
        MorphRuleDef::Compounding(_) => &[],
    }
}

fn main() {
    let g = load_amharic();
    println!("=== E2 Amharic construct census ===\n");
    println!("mrules: {}", g.mrules.len());
    println!("templates: {}", g.templates.len());
    println!("prules: {}", g.prules.len());

    // 1. Classify every mrule's role, per allomorph (not just first — matches emit.rs's own
    // per-allomorph granularity in emit_rule_allomorphs).
    let mut role_counts: std::collections::BTreeMap<&'static str, usize> = Default::default();
    let mut infix_morphemes: std::collections::HashSet<u32> = Default::default();
    let mut circumfix_morphemes: std::collections::HashSet<u32> = Default::default();
    let mut process_morphemes: std::collections::HashSet<u32> = Default::default();
    let mut infix_rule_ids: Vec<String> = Vec::new();
    for (mid, mrule) in g.mrules.iter().enumerate() {
        if matches!(mrule, MorphRuleDef::Compounding(_)) {
            continue;
        }
        let morpheme = owning_morpheme(&g, mid);
        let name = morpheme
            .and_then(|m| g.morphemes.get(m.0 as usize))
            .map(|mi| format!("{}({})", mi.xml_key, mi.gloss.as_deref().unwrap_or("-")))
            .unwrap_or_else(|| format!("mrule{mid}"));
        for (ai, allo) in allomorphs_of(&g, mid).iter().enumerate() {
            let role = classify_affix(&allo.rhs);
            let label = match role {
                Role::None => "none",
                Role::Prefix => "prefix",
                Role::Suffix => "suffix",
                Role::Infix => "infix",
                Role::Reduplication => "reduplication",
                Role::CircumfixPrefix => "circumfix-prefix",
                Role::Process => "process",
            };
            *role_counts.entry(label).or_default() += 1;
            if role == Role::Infix {
                if let Some(m) = morpheme {
                    infix_morphemes.insert(m.0);
                }
                infix_rule_ids.push(format!("mrule{mid}#allo{ai} ({name})"));
            }
            if role == Role::CircumfixPrefix {
                if let Some(m) = morpheme {
                    circumfix_morphemes.insert(m.0);
                }
            }
            if role == Role::Process {
                if let Some(m) = morpheme {
                    process_morphemes.insert(m.0);
                }
            }
        }
    }
    println!("\nallomorph role counts (per-allomorph, all mrules):");
    for (k, v) in &role_counts {
        println!("  {k}: {v}");
    }
    println!("\ninfix rules ({}):", infix_rule_ids.len());
    for r in &infix_rule_ids {
        println!("  {r}");
    }
    println!("\ninfix morphemes: {} distinct", infix_morphemes.len());
    println!("circumfix-prefix morphemes: {} distinct", circumfix_morphemes.len());
    println!("process morphemes: {} distinct", process_morphemes.len());

    // 2. Templates: does any template slot route to an infix/circumfix rule?
    let mut template_infix_slots = 0usize;
    for t in &g.templates {
        for slot in &t.slots {
            for &mrid in &slot.rules {
                let mid = mrid.0 as usize;
                if matches!(g.mrules[mid], MorphRuleDef::Compounding(_)) {
                    continue;
                }
                for allo in allomorphs_of(&g, mid) {
                    if classify_affix(&allo.rhs) == Role::Infix {
                        template_infix_slots += 1;
                    }
                }
            }
        }
    }
    println!("\ntemplate slot allomorphs classifying Infix: {template_infix_slots}");

    // 3. Real engine recall dependency: parse the corpus with the FULL engine oracle, and for each
    // distinct analysis, check whether ANY of its morpheme ids is an infix/circumfix/process
    // morpheme. This is the number that actually matters for the go/no-go call.
    let words_text = std::fs::read_to_string(sample_path("amharic-words.txt")).expect("read words");
    let words: Vec<&str> = words_text.lines().map(str::trim).filter(|w| !w.is_empty()).collect();
    println!("\ncorpus words: {}", words.len());

    let morpher = Morpher::new(&g, usize::MAX)
        .with_word_timeout(Some(std::time::Duration::from_secs(5)));
    let opts = ParseOptions::default();

    let mut n_words_analyzed = 0usize;
    let mut n_words_with_infix_analysis = 0usize;
    let mut n_words_with_circumfix_analysis = 0usize;
    let mut n_words_with_process_analysis = 0usize;
    let mut n_words_ONLY_infix_analyses = 0usize; // every analysis needs infix -- no escape route
    let mut n_total_analyses = 0usize;
    let mut n_infix_analyses = 0usize;
    let mut n_timeouts = 0usize;

    for (i, word) in words.iter().enumerate() {
        if i >= 300 {
            break; // bound the census run time; 300 words is plenty to see the pattern
        }
        let outcome = morpher.parse_word_opts(word, &opts);
        if outcome.timed_out && outcome.structured.is_empty() {
            n_timeouts += 1;
            continue;
        }
        if outcome.structured.is_empty() {
            continue;
        }
        n_words_analyzed += 1;
        let mut seqs: Vec<Vec<u32>> = Vec::new();
        for a in &outcome.structured {
            if !seqs.contains(&a.morpheme_ids) {
                seqs.push(a.morpheme_ids.clone());
            }
        }
        n_total_analyses += seqs.len();
        let mut any_infix = false;
        let mut any_circumfix = false;
        let mut any_process = false;
        let mut all_infix = true;
        for seq in &seqs {
            let has_infix = seq.iter().any(|id| infix_morphemes.contains(id));
            let has_circumfix = seq.iter().any(|id| circumfix_morphemes.contains(id));
            let has_process = seq.iter().any(|id| process_morphemes.contains(id));
            if has_infix {
                any_infix = true;
                n_infix_analyses += 1;
            } else {
                all_infix = false;
            }
            any_circumfix |= has_circumfix;
            any_process |= has_process;
        }
        if any_infix {
            n_words_with_infix_analysis += 1;
        }
        if any_circumfix {
            n_words_with_circumfix_analysis += 1;
        }
        if any_process {
            n_words_with_process_analysis += 1;
        }
        if all_infix {
            n_words_ONLY_infix_analyses += 1;
        }
    }

    println!("\n--- recall dependency (first up-to-300 corpus words) ---");
    println!("words analyzed by engine: {n_words_analyzed} ({n_timeouts} zero-analysis timeouts)");
    println!("total distinct analyses: {n_total_analyses}");
    println!("analyses using >=1 infix morpheme: {n_infix_analyses}");
    println!("words with >=1 infix-bearing analysis: {n_words_with_infix_analysis}");
    println!("words where EVERY analysis needs an infix morpheme (no non-infix escape route): {n_words_ONLY_infix_analyses}");
    println!("words with >=1 circumfix-bearing analysis: {n_words_with_circumfix_analysis}");
    println!("words with >=1 process-morph-bearing analysis: {n_words_with_process_analysis}");

    println!("\n=== done ===");
}
