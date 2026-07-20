//! P6 templated-morphotactics acceptance gate (`docs/fst-plan/p6-prototype-report.md` §6 item 2,
//! `docs/fst-plan/foma-fst-plan.md` §P6): the Aweti gate, mirroring
//! `examples/p6_aweti_replace_prototype.rs`'s own compose flow, as a real, CI-shaped `#[ignore]`d
//! test — matching `f2_indonesian_gate.rs`/`f3_amharic_gate.rs`'s own self-skip-guard convention
//! for a gitignored real-language corpus fixture.
//!
//! ## Why this exists (not just the example)
//! [`pg_foma::emit::emit`] (the enumeration-based emitter) OOMs on Aweti before ever reaching a
//! compilable lexc source (855 entries, 135 mrules — the composite pre-expansion stage trips Fix
//! 1's enumeration budget, `docs/fst-plan/p6-prototype-report.md` §5.2/§6 item 2). Test (a) below
//! is the first thing that gets Aweti's templated (`<AffixTemplate>`-based) morphotactics past
//! that wall at all, via [`pg_foma::emit::emit_underlying_templated`] + the P6 replace-rule
//! cascade (`pg_foma::replace::compile_and_compose_rules`) — this IS the P6 milestone, and it is
//! fully achieved and asserted here: valid `tier`, plausible counts, clean lexc/rule compile,
//! successful `.o.` composition + minimize (35,846 states / 800,354 arcs).
//!
//! ## `build_deriv_chain`'s dedicated-level-per-rule chain restriction (P6-Aweti finding)
//! An earlier investigation (`docs/fst-plan/p6-aweti-truncation-chain-report.md`) found `apply_up`
//! against the composed network hanging indefinitely for some query words (`"ti"` did not
//! complete even 500 raw results within 45s and had to be killed externally) — root-caused to
//! `build_deriv_chain`'s legacy strategy offering the SAME full standalone-rule set at EVERY one
//! of its ~11-24 levels, letting an epsilon-yielding rule's tag be chosen up to 22x (prefix)/48x
//! (suffix) along one path. `pg_foma::emit::build_deriv_chain`'s dedicated-level-per-rule strategy
//! (one rule per level, `TextMode::UnderlyingTokens` only — the `SurfaceProbed`/mainline `emit()`
//! path is completely unchanged, verified by the Indonesian/Sena/parity gates staying green)
//! fixes this: the composed network shrank from 35,846 states/800,354 arcs to 14,806 states/
//! 270,541 arcs, and `apply_up` on `"ti"`/`"an"`/`"parua"` all terminate promptly. See that
//! report's §1 for the full measurement trail.
//!
//! ## Full-corpus recall gate (composition-based, no `apply_up`)
//! [`b_aweti_full_corpus_recall_via_compose`] uses the composition technique (word-FST `.o.`
//! composed net, `fsm_upper`, intersect against each oracle analysis's own tag acceptor,
//! `fsm_isempty`) — an ordinary, terminating automaton construction with NO backtracking search
//! and NO query-ordering dependence, safe to run over the whole corpus (`Morpher::new(&g,
//! ORACLE_STEP_CAP)` for the oracle throughout — `usize::MAX` is NOT actually safe for Aweti,
//! `docs/fst-plan/p6-aweti-truncation-chain-report.md`'s own Q3 finding: the corpus word
//! `"tomoʼatu"` ran the HC engine itself for >10 minutes uncapped).
//!
//! **Measured: 68/104 = 65.4%.** The original investigation's own diagnostic (a throwaway example,
//! since deleted) reported "65/101": it excluded its own 3 hand-picked safety-check probe words
//! (`"parua"`/`"an"`/`"ti"`) from its totals (tested separately, not folded into the corpus-sweep
//! counters) — all 3 are themselves recalled, so 65/101 and this gate's 68/104 are the SAME
//! underlying result (101+3=104, 65+3=68); this gate counts every corpus word uniformly, with no
//! such exclusion. A companion investigation into a marker-token truncation mechanism for the 36
//! misses attributed to "structural" (LHS-material-dropping) rules found the mechanism sound in
//! isolation but its premise REFUTED for Aweti specifically: 0/16 anticipated recall gain, and it
//! separately regressed `apply_up` usability (`"parua"` no longer completing even one raw result
//! within 280s once composed in) — NOT shipped; see the report's §2 for the full root-cause trace
//! (the 41 flagged rules turned out to be floating-consonant phonological realization, not
//! genuine root-material truncation). A separate, still-unexplained gap (a bare root with zero
//! affixes, `"mã"`, also misses this recall check even with the entire phonological cascade
//! removed from the composition) is documented in the report's §3 as an open, unresolved finding.
//! The gate below therefore asserts the ACHIEVED figure (68/104), not a higher anticipated one,
//! and separately asserts no regression against the documented 36-word baseline miss list.
//!
//! ## `apply_up` spot-check (test (c))
//! With the chain restriction alone (no truncation cascade composed in), `apply_up` on `"parua"`
//! resolves its single oracle analysis at the very first raw result, in well under 1ms — kept as
//! a fast, precise regression check alongside the broader compose-based gate above. Uses
//! [`ORACLE_STEP_CAP`] (not `usize::MAX`) for the oracle `Morpher`, same rationale as (b).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use foma::apply::apply_init;
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

use pg_foma::emit::{emit_underlying_templated, FomaTier};
use pg_foma::replace::{compile_and_compose_rules, SegAlphabet};
use pg_foma::tags;
use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions};

/// Same large-stack convention `p6_gate_parity.rs`'s Amharic regression test and every P6
/// example driver use — the vendored foma-rs's own `fsm_compose`/`fsm_minimize` constructions and
/// this crate's own morphotactic derivation-layer recursion (14 templates/43 slots here) both
/// recurse deeply enough to overflow the default thread stack.
const STACK_BYTES: usize = 512 * 1024 * 1024;

/// Verified-safe-AND-correct query for test (c) — module doc. A bounded, generous raw-result cap
/// (module doc: "parua" resolves in <1ms, nowhere near this) is kept as a defensive backstop, not
/// because it is expected to bind.
const SAFE_WORD: &str = "parua";
const SAFE_WORD_RAW_CAP: usize = 50_000;

/// Any oracle `Morpher` call in this file uses this cap, never `usize::MAX` (module doc /
/// `docs/fst-plan/p6-aweti-truncation-chain-report.md`'s own Q3 finding: `Morpher::new(&g,
/// usize::MAX)` is NOT actually bounded for Aweti — the corpus word `"tomoʼatu"` ran the HC engine
/// itself for >10 minutes uncapped, two independent runs, neither `StepBudget` bound ever
/// tripping).
const ORACLE_STEP_CAP: usize = 20_000;

/// The 36-word baseline miss list this gate's recall figure must never regress below (module doc:
/// unchanged by the chain-restriction change — this is the SAME miss list the report's original
/// investigation documented before the restriction shipped).
const BASELINE_MISSES: &[&str] = &[
    "muʼazan", "ʼyto", "kỹjtaw", "uʼwywywot", "utu", "otokỹj", "kajekozokotu", "wemulujaʼjawype",
    "nãtsu", "tsãnupu", "ekyty", "warajuzan", "nutu", "ete", "tsãmopypu", "tonoly", "mian",
    "moʼazan", "tsãn", "nãti", "moʼaza", "kỹjokwaw", "mã", "oteʼayka", "nekozokotu", "otiʼing",
    "oto", "wekozoko", "tiretu", "nupu", "tsãnekozokotu", "wemuluja", "ma", "epykaw", "outaw",
    "tsãnutu",
];

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

/// Self-skip guard: gitignored real-corpus fixtures aren't present in a fresh clone or CI —
/// matches `f2_indonesian_gate.rs`/`f3_amharic_gate.rs`'s own `have()` convention exactly.
fn have(name: &str) -> bool {
    sample_path(name).exists()
}

fn load_aweti() -> Grammar {
    let path = sample_path("aweti.json");
    let json = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let snapshot = pg_snapshot::Snapshot::from_json(&json)
        .unwrap_or_else(|e| panic!("parse snapshot {}: {e}", path.display()));
    let (grammar, _warnings) = pg_grammar::compile_project(&snapshot)
        .unwrap_or_else(|e| panic!("compile_project {}: {e}", path.display()));
    grammar
}

/// (a) EMIT + COMPILE + COMPOSE: the P6 milestone itself. `emit_underlying_templated` must
/// produce a usable, non-`Unsupported` network for Aweti with plausible counts, the templated
/// lexc must foma-compile, the 18-rule cascade must compile+compose, and the full
/// `lexc .o. rules .o. cleanup` composition + minimize must succeed — all of this is exactly what
/// OOMs via the mainline `emit()` (Fix 1's enumeration budget trips in the composite
/// pre-expansion stage before any of this is reached), so completing it at all is the deliverable.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/aweti.json); run with --include-ignored"]
fn a_aweti_templated_emit_compile_and_compose() {
    if !have("aweti.json") {
        eprintln!("skipping: aweti.json not present on disk");
        return;
    }
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run_emit_compile_compose)
        .expect("spawn large-stack worker thread");
    handle.join().expect("aweti emit/compile/compose worker thread panicked");
}

fn run_emit_compile_compose() {
    let g = load_aweti();
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();

    let t_emit = Instant::now();
    let result = emit_underlying_templated(&g, &alphabet, None);
    let emit_elapsed = t_emit.elapsed();
    println!(
        "aweti templated emit: {emit_elapsed:?}; tier={:?}; uncovered={}",
        result.report.tier,
        result.report.uncovered.len()
    );
    for u in &result.report.uncovered {
        println!("  uncovered: [{}] {} -- {}", u.kind, u.id, u.reason);
    }

    assert!(
        !matches!(result.report.tier, FomaTier::Unsupported { .. }),
        "emit_underlying_templated must not be Unsupported for Aweti: {:?}",
        result.report.tier
    );
    assert!(
        result.report.enum_budget_exceeded.is_none(),
        "the enumeration budget must not trip for the templated path (it never calls the \
         composite pipeline that trips it for the mainline emit()): {:?}",
        result.report.enum_budget_exceeded
    );
    assert!(
        result.report.counts.entries >= 855,
        "counts.entries={} looks too small for the real Aweti grammar (expected >= 855)",
        result.report.counts.entries
    );
    assert!(
        result.report.counts.rules >= 135,
        "counts.rules={} looks too small for the real Aweti grammar (expected >= 135)",
        result.report.counts.rules
    );
    assert!(result.report.counts.lexc_lines > 0, "expected at least one lexc line");

    let t_lexc = Instant::now();
    let lexc_net = fsm_lexc_parse_string(&opts, None, &result.lexc_source)
        .unwrap_or_else(|| panic!("Aweti templated lexc failed to foma-compile"));
    let lexc_elapsed = t_lexc.elapsed();
    println!(
        "lexc compile: {lexc_elapsed:?}; net: {} states, {} arcs",
        lexc_net.statecount, lexc_net.arccount
    );

    let mut rules_in_order: Vec<&PhonRuleDef> = Vec::new();
    for st in &g.strata {
        for &prid in &st.prules {
            rules_in_order.push(&g.prules[prid.0 as usize]);
        }
    }
    assert_eq!(rules_in_order.len(), 18, "Aweti declares exactly 18 phonological rules");

    let mut skipped_rules: Vec<String> = Vec::new();
    let mut tuple_reports = Vec::new();
    let t_rules = Instant::now();
    let rule_net = compile_and_compose_rules(
        &opts,
        &g,
        &alphabet,
        &rules_in_order,
        &mut skipped_rules,
        &mut tuple_reports,
    )
    .expect("compose budget ok")
    .expect("Aweti's 18 rules must compile");
    println!("rule compile+compose: {:?}; skipped={skipped_rules:?}", t_rules.elapsed());
    assert!(skipped_rules.is_empty(), "no Aweti rule should be skipped: {skipped_rules:?}");

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

    let t_compose = Instant::now();
    let composed = fsm_compose(&opts, lexc_net, rule_net);
    let composed = fsm_compose(&opts, composed, cleanup_net);
    let composed = fsm_minimize(&opts, composed);
    println!(
        "full composition + minimize: {:?}; final net: {} states, {} arcs",
        t_compose.elapsed(),
        composed.statecount,
        composed.arccount
    );
    assert!(composed.statecount > 0, "composed network must be non-empty");
}

/// One arc per character of `token_string` (already single-codepoint tokens in `SegAlphabet`'s PUA
/// scheme), used identically on both tapes — a linear identity transducer for one query word.
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

/// One arc per DECODED tag-text symbol (`tags::root_tag_text`/`morph_tag_text`) — a linear
/// acceptor for one candidate analysis's own tag sequence, in surface order.
fn tag_string_fsm(name: &str, tags: &[String]) -> Fsm {
    let mut h = fsm_construct_init(name);
    for (i, t) in tags.iter().enumerate() {
        fsm_construct_add_arc(&mut h, i as i32, (i + 1) as i32, t, t);
    }
    fsm_construct_set_initial(&mut h, 0);
    fsm_construct_set_final(&mut h, tags.len() as i32);
    fsm_construct_done(h)
}

/// (b) FULL-CORPUS RECALL GATE (module doc): composition-based, no `apply_up`. Builds the SAME
/// network as (a) — `lexc .o. rules .o. cleanup` — then, per corpus word with `>=1` oracle
/// analysis, restricts the composed net to exactly that word's own token string (`fsm_compose`
/// with a linear identity transducer), projects the UPPER (tag) tape, and checks whether ANY
/// oracle analysis's own tag sequence intersects it non-emptily. Prints the full recall figure and
/// the miss list; asserts the ACHIEVED recall (68/104 — module doc explains why) and that no
/// previously-recalled word has regressed (every corpus word NOT in [`BASELINE_MISSES`] must
/// still recall).
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/aweti.json); run with --include-ignored"]
fn b_aweti_full_corpus_recall_via_compose() {
    if !have("aweti.json") {
        eprintln!("skipping: aweti.json not present on disk");
        return;
    }
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run_full_corpus_recall)
        .expect("spawn large-stack worker thread");
    handle.join().expect("aweti full-corpus recall worker thread panicked");
}

fn run_full_corpus_recall() {
    let g = load_aweti();
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let width = tags::tag_width(g.morphemes.len());

    let result = emit_underlying_templated(&g, &alphabet, None);
    let lexc_net = fsm_lexc_parse_string(&opts, None, &result.lexc_source)
        .unwrap_or_else(|| panic!("Aweti templated lexc failed to foma-compile"));
    println!("lexc net: {} states, {} arcs", lexc_net.statecount, lexc_net.arccount);

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
    .expect("Aweti's 18 rules must compile");

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
    let words_raw = std::fs::read_to_string(&words_path).unwrap_or_else(|e| panic!("read {}: {e}", words_path.display()));
    let words: Vec<&str> = words_raw.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();

    let mut n_with_oracle = 0usize;
    let mut n_recalled = 0usize;
    let mut missed_words: Vec<String> = Vec::new();
    let t_all = Instant::now();
    for &word in &words {
        let outcome = morpher.parse_word_opts(word, &popts);
        if outcome.structured.is_empty() {
            continue;
        }
        let Some(query) = alphabet.encode_query(word) else {
            continue; // unsegmentable query -- not counted either way
        };
        n_with_oracle += 1;

        let word_fsm = linear_identity_fsm("word", &query);
        let restricted = fsm_compose(&opts, composed.clone(), word_fsm);
        let restricted = fsm_minimize(&opts, restricted);
        let upper = fsm_minimize(&opts, fsm_upper(restricted));

        let mut any_recalled = false;
        for a in &outcome.structured {
            let mut tag_texts: Vec<String> = Vec::with_capacity(a.morpheme_ids.len());
            for (i, &m) in a.morpheme_ids.iter().enumerate() {
                let is_root = i as i32 == a.root_morpheme_index;
                let mid = pg_grammar::model::MorphemeId(m);
                tag_texts.push(if is_root {
                    tags::root_tag_text(mid, width)
                } else {
                    tags::morph_tag_text(mid, width)
                });
            }
            let tag_fsm = tag_string_fsm("tagcheck", &tag_texts);
            let mut intersected = fsm_intersect(&opts, upper.clone(), tag_fsm);
            if !fsm_isempty(&opts, &mut intersected) {
                any_recalled = true;
                break;
            }
        }
        if any_recalled {
            n_recalled += 1;
        } else {
            missed_words.push(word.to_string());
        }
    }
    println!("full corpus sweep: {:?}", t_all.elapsed());
    println!(
        "RECALL = {n_recalled}/{n_with_oracle} = {:.1}%",
        100.0 * n_recalled as f64 / n_with_oracle.max(1) as f64
    );
    println!("miss list ({}): {missed_words:?}", missed_words.len());

    // Achieved-figure assertion (module doc: NOT a higher anticipated figure -- see the module
    // doc's own summary and the crate's own P6-Aweti task report for the full writeup).
    assert!(
        n_recalled >= 68,
        "recall regressed below the documented baseline: {n_recalled}/{n_with_oracle} (miss list: {missed_words:?})"
    );

    // No-regression assertion: every corpus word with an oracle analysis NOT in the documented
    // baseline miss list must still recall now.
    let missed_set: HashSet<&str> = missed_words.iter().map(|s| s.as_str()).collect();
    let mut newly_missed: Vec<&str> =
        missed_set.iter().filter(|w| !BASELINE_MISSES.contains(w)).copied().collect();
    newly_missed.sort_unstable();
    assert!(
        newly_missed.is_empty(),
        "words recalled at baseline are now MISSED (a real regression): {newly_missed:?}"
    );
}

/// (c) SPOT-CHECK RECALL on the one word verified both safe AND correct (module doc). `"parua"`
/// decodes its single oracle analysis at the very first `apply_up` result.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/aweti.json); run with --include-ignored"]
fn c_aweti_spot_check_recall_parua() {
    if !have("aweti.json") {
        eprintln!("skipping: aweti.json not present on disk");
        return;
    }
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run_spot_check)
        .expect("spawn large-stack worker thread");
    handle.join().expect("aweti spot-check worker thread panicked");
}

fn run_spot_check() {
    let g = load_aweti();
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();

    let result = emit_underlying_templated(&g, &alphabet, None);
    let lexc_net = fsm_lexc_parse_string(&opts, None, &result.lexc_source)
        .unwrap_or_else(|| panic!("Aweti templated lexc failed to foma-compile"));

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
    .expect("Aweti's 18 rules must compile");

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
    let mut handle = apply_init(&composed);

    let morpher = Morpher::new(&g, ORACLE_STEP_CAP);
    let popts = ParseOptions::default();
    let outcome = morpher.parse_word_opts(SAFE_WORD, &popts);
    let engine_seqs: Vec<(Vec<u32>, i32)> = outcome
        .structured
        .iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect();
    assert_eq!(
        engine_seqs.len(),
        1,
        "sanity: {SAFE_WORD:?} is expected to have exactly one oracle analysis (verified during \
         this gate's own investigation); got {}: {:?}",
        engine_seqs.len(),
        engine_seqs
    );

    let query = alphabet
        .encode_query(SAFE_WORD)
        .unwrap_or_else(|| panic!("{SAFE_WORD:?} failed to segment into token space"));

    let t0 = Instant::now();
    let mut covered = false;
    let mut raw_n = 0usize;
    for s in handle.up(&query) {
        raw_n += 1;
        if let Some(path) = tags::decode_path(&s) {
            for c in tags::to_candidates(&path) {
                let key: (Vec<u32>, i32) = (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
                if engine_seqs.contains(&key) {
                    covered = true;
                }
            }
        }
        if covered || raw_n >= SAFE_WORD_RAW_CAP {
            break;
        }
    }
    println!("{SAFE_WORD:?}: covered={covered} raw_n={raw_n} elapsed={:?}", t0.elapsed());
    assert!(
        covered,
        "{SAFE_WORD:?}'s single oracle analysis {:?} was not found within {SAFE_WORD_RAW_CAP} \
         raw apply_up results (raw_n={raw_n}) -- this word was previously verified to resolve at \
         raw result #1 in well under 1ms; a regression here is a real finding",
        engine_seqs[0]
    );
}
