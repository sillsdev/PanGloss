//! Diagnostic census tool (2026-07-27 templated-morphotactics recall investigation, Task 2 of
//! `docs/superpowers/plans/2026-07-21-aweti-correctness-performance-plan.md`): scans the whole
//! corpus for every word whose ONLY oracle analysis is a bare root (exactly one morpheme,
//! `root_index == 0`, i.e. zero affixes), and reports, for each, whether the composition-based
//! recall check (byte-for-byte the same technique `tests/p6_templated_morphotactics_gate.rs`'s
//! `b_full_corpus_recall_via_compose` uses) recalls it, plus its morpheme id and raw codepoints.
//!
//! ## Conclusion this tool's own output led to
//! Before the `pg_foma::tags` fix (module doc point 3 there), EVERY bare-root miss in this census
//! had a morpheme id whose zero-padded numeral contains a literal `0` digit (`"mã"`=400, `"ma"`=69,
//! `"nã"`=106, ... — 10/10 sampled), while every RECALLED bare root had an id with no `0` digit at
//! all (`"ta"`=894, `"me"`/`"ne"`=897, `"kitã"`=395 — including combining-mark-bearing roots,
//! ruling out a combining-mark cause). This pointed straight at the upstream `divvun/foma-rs`
//! `Multichar_Symbols` decomposition defect, not a language-membership gap — see
//! `tests/p6_templated_morphotactics_gate.rs`'s `d_bare_root_tag_atomicity_boundary` for the
//! concrete boundary assertion, and `pg_foma::tags`'s module doc (point 3) for the fix.
//!
//! Run: `cargo run --release -p pg-foma --example p6_templated_bare_root_scan`

use std::path::{Path, PathBuf};

use foma::constructions::{fsm_compose, fsm_intersect};
use foma::dynarray::{
    fsm_construct_add_arc, fsm_construct_done, fsm_construct_init, fsm_construct_set_final,
    fsm_construct_set_initial,
};
use foma::extract::fsm_upper;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::structures::fsm_isempty;
use foma::types::Fsm;

use pg_foma::emit::emit_underlying_templated;
use pg_foma::replace::{compile_and_compose_rules, SegAlphabet};
use pg_foma::tags;
use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions};

const ORACLE_STEP_CAP: usize = 20_000;
const STACK_BYTES: usize = 512 * 1024 * 1024;

fn sample_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../samples/data")
        .join(name)
}

fn load_grammar() -> Grammar {
    let path = sample_path("aweti.json");
    let json =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let snapshot = pg_snapshot::Snapshot::from_json(&json)
        .unwrap_or_else(|e| panic!("parse snapshot {}: {e}", path.display()));
    let (grammar, _warnings) = pg_grammar::compile_project(&snapshot)
        .unwrap_or_else(|e| panic!("compile_project {}: {e}", path.display()));
    grammar
}

fn linear_identity_fsm(name: &str, token_string: &str) -> Fsm {
    let mut h = fsm_construct_init(name);
    let chars: Vec<char> = token_string.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let sym = c.to_string();
        fsm_construct_add_arc(&mut h, i as i32, (i + 1) as i32, &sym, &sym);
    }
    fsm_construct_set_initial(&mut h, 0);
    fsm_construct_set_final(&mut h, chars.len() as i32);
    fsm_construct_done(h)
}

fn tag_string_fsm(name: &str, tags: &[String]) -> Fsm {
    let mut h = fsm_construct_init(name);
    for (i, t) in tags.iter().enumerate() {
        fsm_construct_add_arc(&mut h, i as i32, (i + 1) as i32, t, t);
    }
    fsm_construct_set_initial(&mut h, 0);
    fsm_construct_set_final(&mut h, tags.len() as i32);
    fsm_construct_done(h)
}

fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run)
        .expect("spawn large-stack worker thread");
    handle.join().expect("worker thread panicked");
}

fn run() {
    let g = load_grammar();
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let width = tags::tag_width(g.morphemes.len());

    let result = emit_underlying_templated(&g, &alphabet, None);
    let lexc_net = fsm_lexc_parse_string(&opts, None, &result.lexc_source)
        .unwrap_or_else(|| panic!("templated lexc failed to foma-compile"));
    println!(
        "lexc net: {} states, {} arcs",
        lexc_net.statecount, lexc_net.arccount
    );

    let mut rules_in_order: Vec<&PhonRuleDef> = Vec::new();
    for st in &g.strata {
        for &prid in &st.prules {
            rules_in_order.push(&g.prules[prid.0 as usize]);
        }
    }
    let mut skipped_rules: Vec<String> = Vec::new();
    let mut tuple_reports = Vec::new();
    let rule_net = compile_and_compose_rules(
        &opts,
        &g,
        &alphabet,
        &rules_in_order,
        &mut skipped_rules,
        &mut tuple_reports,
    )
    .expect("compose budget ok")
    .expect("rules must compile");
    println!("skipped_rules = {skipped_rules:?}");

    let boundary_tokens: Vec<char> = table
        .iter()
        .filter(|(_, cd)| cd.kind() == CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect();
    let cleanup_regex = boundary_tokens
        .iter()
        .map(|c| format!("{c} -> 0"))
        .collect::<Vec<_>>()
        .join(", ");
    let cleanup_net = fsm_parse_regex(&opts, &cleanup_regex, None, None)
        .unwrap_or_else(|| panic!("boundary cleanup regex failed to compile: {cleanup_regex:?}"));

    let composed = fsm_compose(&opts, lexc_net, rule_net);
    let composed = fsm_compose(&opts, composed, cleanup_net);
    let composed = fsm_minimize(&opts, composed);
    println!(
        "composed net (lexc+rules+cleanup): {} states, {} arcs",
        composed.statecount, composed.arccount
    );

    let morpher = Morpher::new(&g, ORACLE_STEP_CAP);
    let popts = ParseOptions::default();

    let words_path = sample_path("aweti-words.txt");
    let words_raw = std::fs::read_to_string(&words_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", words_path.display()));
    let words: Vec<&str> = words_raw
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();

    println!("--- bare-root corpus words (exactly one oracle analysis, single morpheme, root_index==0) ---");
    for &word in &words {
        let outcome = morpher.parse_word_opts(word, &popts);
        if outcome.structured.is_empty() {
            continue;
        }
        // Restrict to words whose EVERY oracle analysis is a single-morpheme bare root -- avoids
        // ambiguity with a word that also has a longer derived analysis.
        let all_bare = outcome
            .structured
            .iter()
            .all(|a| a.morpheme_ids.len() == 1 && a.root_morpheme_index == 0);
        if !all_bare {
            continue;
        }
        let mid = outcome.structured[0].morpheme_ids[0];
        let Some(query) = alphabet.encode_query(word) else {
            println!("{word:?}: morpheme={mid} UNSEGMENTABLE (encode_query -> None)");
            continue;
        };

        let word_fsm = linear_identity_fsm("word", &query);
        let restricted = fsm_compose(&opts, composed.clone(), word_fsm);
        let restricted = fsm_minimize(&opts, restricted);
        let upper = fsm_minimize(&opts, fsm_upper(restricted));

        let tag_texts = vec![tags::root_tag_text(
            pg_grammar::model::MorphemeId(mid),
            width,
        )];
        let tag_fsm = tag_string_fsm("tagcheck", &tag_texts);
        let mut intersected = fsm_intersect(&opts, upper, tag_fsm);
        let recalled = !fsm_isempty(&opts, &mut intersected);

        // Raw (as-stored) codepoints of the word, for spotting combining marks at a glance.
        let cps: Vec<String> = word
            .chars()
            .map(|c| format!("U+{:04X}", c as u32))
            .collect();

        println!("{word:?}: morpheme={mid} recalled={recalled} query={query:?} codepoints={cps:?}");
    }

    println!(
        "--- entry-shape dump for a handful of morpheme ids (failing vs recalled bare roots) ---"
    );
    let interesting = [
        30u32, 62, 63, 69, 106, 206, 400, 804, 820, 950, // recalled=false
        665, 897, 894, 831, 695, 939, 858, // recalled=true
    ];
    for &mid in &interesting {
        for (ei, entry) in g.entries.iter().enumerate() {
            if entry.morpheme.0 != mid {
                continue;
            }
            let fs_text = format!("{:?}", g.fs_interner.get(entry.syn_fs));
            for allo in &entry.allomorphs {
                let underlying = alphabet.encode_shape(&allo.shape.shape);
                println!(
                    "morpheme={mid} entry_idx={ei} authored_id={:?} allo_id={:?} is_bound={} \
                     stem_name={:?} shape_text={:?} encoded={:?} fs={}",
                    entry.authored_id,
                    allo.id,
                    allo.is_bound,
                    allo.stem_name,
                    allo.shape.text,
                    underlying,
                    fs_text
                );
            }
        }
    }
}
