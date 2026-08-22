//! Templated-morphotactics acceptance gate for Aweti; needs the gitignored real corpus. See
//! `docs/research/pg-foma-p6-aweti-gate.md` for the full investigation history and rationale.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use foma::apply::apply_init;
use foma::constructions::{fsm_compose, fsm_intersect, fsm_union, fsm_universal};
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
use pg_foma::replace::{compile_and_compose_rules_recall_safe, SegAlphabet};
use pg_foma::tags;
use pg_foma::templated_compile::compile_templated_morphotactics;
use pg_grammar::chardef::{CharDefId, CharDefKind};
use pg_grammar::model::{Grammar, MorphemeId, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions};

/// The `fsm_compose`/`fsm_minimize` constructions and this crate's morphotactic derivation-layer recursion both recurse deeply enough to overflow the default thread stack.
const STACK_BYTES: usize = 512 * 1024 * 1024;

/// Termination-probe query for test (c): `"parua"` is a skipped-rule-dependent miss, so this probes that `apply_up` terminates promptly, not that the word recalls.
const SAFE_WORD: &str = "parua";
const SAFE_WORD_RAW_CAP: usize = 50_000;

/// Every oracle `Morpher` call here uses this cap, never `usize::MAX`: one Aweti corpus word ran the engine uncapped for over 10 minutes.
const ORACLE_STEP_CAP: usize = 20_000;

/// Post-detection baseline miss list: every corpus word with an oracle analysis the composed net
/// does not recall; see `docs/research/pg-foma-p6-aweti-gate.md` for two entries' investigation.
const BASELINE_MISSES: &[&str] = &[
    "tsãkỹjokwaw",
    "tsãtomoʼatu",
    "parua",
    "tomoʼatu",
    "muʼazan",
    "an",
    "Paruape",
    "atozoko",
    "nuhijupe",
    "ʼyto",
    "kỹjtaw",
    "jatanete",
    "atoju",
    "mote",
    "uʼwywywot",
    "utu",
    "otokỹj",
    "kajekozokotu",
    "wemulujaʼjawype",
    "nãtsu",
    "wezanu",
    "tsãnupu",
    "ekyty",
    "warajuzan",
    "nutu",
    "enumania",
    "Awytyza",
    "ete",
    "tsãmopypu",
    "tonoly",
    "mian",
    "moʼazan",
    "Ywirytywype",
    "ozoamũjza",
    "tsãn",
    "nãti",
    "ʼetuti",
    "moʼaza",
    "kỹjokwaw",
    "wian",
    "nuhiju",
    "pokỹjokotu",
    "ʼypy",
    "karaʼiwa",
    "mã",
    "oteʼayka",
    "wijan",
    "ekozokotu",
    "wene",
    "ajkulula",
    "nekozokotu",
    "Ajkululape",
    "otiʼing",
    "nanype",
    "aʼyn",
    "oto",
    "itemimiʼing",
    "in",
    "wekozoko",
    "azoza",
    "tiretu",
    "awytyza",
    "azoamũjza",
    "nupu",
    "temimiʼing",
    "ʼYtoto",
    "tsãnekozokotu",
    "wemuluja",
    "mopypu",
    "ato",
    "ma",
    "epykaw",
    "outaw",
    "tsãnutu",
];

/// Current achieved recall as an executable regression boundary; correctness work must update this list and the exact numerator together.
const CURRENT_EXPECTED_MISSES: &[&str] = &[];

fn sample_path(name: &str) -> PathBuf {
    if let Some(root) = std::env::var_os("PANGLOSS_CORPUS_ROOT") {
        return PathBuf::from(root).join(name);
    }
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

/// Self-skip guard: gitignored real-corpus fixtures aren't present in a fresh clone or CI.
fn have(name: &str) -> bool {
    sample_path(name).exists()
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

/// (a) EMIT + COMPILE + COMPOSE: `emit_underlying_templated` must produce a usable network for Aweti, the templated lexc must foma-compile, the rule cascade must compile+compose, and the full composition + minimize must succeed -- all of this OOMs via the mainline `emit()`, so completing it at all is the deliverable.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/aweti.json); run with --include-ignored"]
fn a_templated_emit_compile_and_compose() {
    if !have("aweti.json") {
        eprintln!("skipping: aweti.json not present on disk");
        return;
    }
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run_emit_compile_compose)
        .expect("spawn large-stack worker thread");
    handle
        .join()
        .expect("aweti emit/compile/compose worker thread panicked");
}

fn run_emit_compile_compose() {
    let g = load_grammar();
    let compiled =
        compile_templated_morphotactics(&g).expect("Aweti templated compile pipeline must succeed");
    let report = compiled
        .proposer
        .report
        .as_ref()
        .expect("the templated emitter supplies its own report");
    let profile = &compiled.profile;
    println!(
        "aweti templated emit: {:?}; tier={:?}; uncovered={}",
        profile.templated_emit_elapsed,
        report.tier,
        report.uncovered.len()
    );
    for uncovered in &report.uncovered {
        println!(
            "  uncovered: [{}] {} -- {}",
            uncovered.kind, uncovered.id, uncovered.reason
        );
    }
    assert!(
        !matches!(report.tier, FomaTier::Unsupported { .. }),
        "emit_underlying_templated must not be Unsupported for Aweti: {:?}",
        report.tier
    );
    assert!(report.enum_budget_exceeded.is_none());
    assert!(report.counts.entries >= 855);
    assert!(report.counts.rules >= 135);
    assert!(report.counts.lexc_lines > 0);
    assert_eq!(
        profile.phonological_rule_count, 18,
        "Aweti declares exactly 18 phonological rules"
    );
    println!(
        "lexc compile: {:?}; net: {} states, {} arcs",
        profile.lexc_compile_elapsed, profile.lexc_state_count, profile.lexc_arc_count
    );
    println!(
        "rule compile+compose: {:?}; skipped={:?}",
        profile.rule_compile_compose_elapsed, profile.skipped_rules
    );
    let mut skipped_sorted = profile.skipped_rules.clone();
    skipped_sorted.sort();
    assert_eq!(
        skipped_sorted,
        Vec::<String>::new(),
        "expected all 18 Aweti phonological rules to compile"
    );
    println!(
        "full composition + minimize: {:?}; final net: {} states, {} arcs",
        profile.final_compose_minimize_elapsed, profile.final_state_count, profile.final_arc_count
    );
    assert!(
        compiled.network.statecount > 0,
        "composed network must be non-empty"
    );
}

/// One arc per already-tokenized character of `token_string`, used identically on both tapes: a linear identity transducer for one query word.
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

/// One arc per decoded tag-text symbol: a linear acceptor for one candidate analysis's tag sequence, in surface order.
fn tag_string_fsm(name: &str, tags: &[String]) -> Fsm {
    let mut h = fsm_construct_init(name);
    for (i, t) in tags.iter().enumerate() {
        fsm_construct_add_arc(&mut h, i as i32, (i + 1) as i32, t, t);
    }
    fsm_construct_set_initial(&mut h, 0);
    fsm_construct_set_final(&mut h, tags.len() as i32);
    fsm_construct_done(h)
}

/// (b) FULL-CORPUS RECALL GATE: per corpus word with an oracle analysis, restricts the composed net to that word's token string and checks whether any oracle analysis's tag sequence is reachable in the upper tape.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/aweti.json); run with --include-ignored"]
fn b_full_corpus_recall_via_compose() {
    if !have("aweti.json") {
        eprintln!("skipping: aweti.json not present on disk");
        return;
    }
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run_full_corpus_recall)
        .expect("spawn large-stack worker thread");
    handle
        .join()
        .expect("aweti full-corpus recall worker thread panicked");
}

fn run_full_corpus_recall() {
    let g = load_grammar();
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let width = tags::tag_width(g.morphemes.len());

    let compiled =
        compile_templated_morphotactics(&g).expect("Aweti templated compile pipeline must succeed");
    println!(
        "lexc net: {} states, {} arcs",
        compiled.profile.lexc_state_count, compiled.profile.lexc_arc_count
    );
    println!(
        "composed net (lexc+rules+cleanup): {} states, {} arcs",
        compiled.profile.final_state_count, compiled.profile.final_arc_count
    );
    let composed = compiled.network;

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

    // A `>=` floor (not `==`) so a real RTL/Simultaneous compiler landing later raises recall without tripping this line -- such a win must also shrink `BASELINE_MISSES` below, so the two move together.
    assert!(
        n_recalled >= 32,
        "recall regressed below the honest post-detection baseline (32/104): {n_recalled}/{n_with_oracle} (miss list: {missed_words:?})"
    );

    // Every corpus word with an oracle analysis not in the documented baseline miss list must still recall now.
    let missed_set: HashSet<&str> = missed_words.iter().map(|s| s.as_str()).collect();
    let mut newly_missed: Vec<&str> = missed_set
        .iter()
        .filter(|w| !BASELINE_MISSES.contains(w))
        .copied()
        .collect();
    newly_missed.sort_unstable();
    assert!(
        newly_missed.is_empty(),
        "words recalled at baseline are now MISSED (a real regression): {newly_missed:?}"
    );

    let mut expected_misses = CURRENT_EXPECTED_MISSES.to_vec();
    expected_misses.sort_unstable();
    let mut actual_misses: Vec<&str> = missed_words.iter().map(String::as_str).collect();
    actual_misses.sort_unstable();
    pg_conformance_fixtures::corpus::record_cases(
        "aweti_full_corpus_recall_via_compose",
        n_with_oracle,
    );
    assert_eq!(
        (n_recalled, n_with_oracle, actual_misses),
        (106, 106, expected_misses),
        "current Aweti recall boundary changed; correctness work must update the exact numerator/denominator/miss set deliberately"
    );
}

/// (c) `apply_up` TERMINATION spot-check: `"parua"` needs an honestly-skipped rule so it is not recalled, but the durable property under test is that `apply_up` on the composed net terminates promptly and does not explode.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/aweti.json); run with --include-ignored"]
fn c_apply_up_terminates_parua() {
    if !have("aweti.json") {
        eprintln!("skipping: aweti.json not present on disk");
        return;
    }
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run_spot_check)
        .expect("spawn large-stack worker thread");
    handle
        .join()
        .expect("aweti spot-check worker thread panicked");
}

fn run_spot_check() {
    let g = load_grammar();
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);

    let compiled =
        compile_templated_morphotactics(&g).expect("Aweti templated compile pipeline must succeed");
    let composed = compiled.network;
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
                let key: (Vec<u32>, i32) =
                    (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
                if engine_seqs.contains(&key) {
                    covered = true;
                }
            }
        }
        if covered || raw_n >= SAFE_WORD_RAW_CAP {
            break;
        }
    }
    let elapsed = t0.elapsed();
    println!("{SAFE_WORD:?}: covered={covered} raw_n={raw_n} elapsed={elapsed:?}");
    // `covered` is expected false (a skipped-rule-dependent miss) and deliberately not asserted on, since that would be brittle against a future compiler flipping it true. The durable guarantee is termination/latency.
    assert!(
        elapsed < std::time::Duration::from_secs(30),
        "apply_up on {SAFE_WORD:?} took {elapsed:?} to enumerate {raw_n} raw results (cap \
         {SAFE_WORD_RAW_CAP}) -- the chain restriction is supposed to keep this prompt; a hang or \
         blowup regression is a real finding"
    );
}

/// (d) Bare-root TAG ATOMICITY boundary: pins where the historically-missing bare root `"mã"` diverged from a recalled bare root of the same shape; see
/// `docs/research/pg-foma-p6-aweti-gate.md`.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/aweti.json); run with --include-ignored"]
fn d_bare_root_tag_atomicity_boundary() {
    if !have("aweti.json") {
        eprintln!("skipping: aweti.json not present on disk");
        return;
    }
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run_tag_atomicity_boundary)
        .expect("spawn large-stack worker thread");
    handle
        .join()
        .expect("tag atomicity boundary worker thread panicked");
}

fn run_tag_atomicity_boundary() {
    let g = load_grammar();
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let width = tags::tag_width(g.morphemes.len());

    let compiled =
        compile_templated_morphotactics(&g).expect("Aweti templated compile pipeline must succeed");
    let composed = compiled.network;

    // Four bare-root probes: two historically-missing words whose zero-padded morpheme id contains a `0` ("mã", "ma"), and two recalled controls whose id does not ("ta", "kitã").
    let probes = [
        ("mã", 400u32),
        ("ma", 69u32),
        ("ta", 894u32),
        ("kitã", 395u32),
    ];

    for (word, mid) in probes {
        let query = alphabet
            .encode_query(word)
            .unwrap_or_else(|| panic!("{word:?} failed to segment into token space"));
        let tag = tags::root_tag_text(pg_grammar::model::MorphemeId(mid), width);

        // Boundary 1: apply_up on the fully composed net finds the tag directly, for every probe -- rules out "the network never contained this path at all".
        let mut handle = apply_init(&composed);
        let found_via_apply_up = handle.up(&query).any(|s| s.contains(&tag));
        assert!(
            found_via_apply_up,
            "{word:?}: apply_up on the composed net must find {tag:?} directly (the network's own \
             language always contains this bare-root analysis)"
        );

        // Boundary 2: restrict to the query, project upper, and check whether the exact tag string is registered as one atomic symbol in sigma -- this is where the divergence used to live (see docs/research/pg-foma-p6-aweti-gate.md).
        let word_fsm = linear_identity_fsm("word", &query);
        let restricted = fsm_minimize(&opts, fsm_compose(&opts, composed.clone(), word_fsm));
        let upper = fsm_minimize(&opts, fsm_upper(restricted));
        let in_sigma = upper.sigma.iter().any(|s| s.symbol == tag.as_str());
        assert!(
            in_sigma,
            "{word:?}: expected tag {tag:?} to be registered as ONE atomic symbol in the \
             restricted net's own sigma table (tags.rs module doc point 3) -- its absence here is \
             exactly the boundary that made the corpus recall check misreport {word:?} as missing \
             even though apply_up (boundary 1, just above) proves the network's language already \
             contains it"
        );

        // Boundary 3: the actual intersect-based recall check itself must now succeed too.
        let tag_fsm = tag_string_fsm("tagcheck", std::slice::from_ref(&tag));
        let mut intersected = fsm_intersect(&opts, upper, tag_fsm);
        assert!(
            !fsm_isempty(&opts, &mut intersected),
            "{word:?}: compose-restrict-project-intersect must recall the bare-root tag {tag:?}"
        );
    }
}
/// Diagnostic ceilings checked after each synchronous foma operation returns; classify evidence as inconclusive but cannot interrupt a call already in flight.
const DIAGNOSTIC_STATE_CAP: i32 = 2_000_000;
const DIAGNOSTIC_ARC_CAP: i32 = 20_000_000;
const DIAGNOSTIC_OPERATION_SECS: u64 = 60;
const DIAGNOSTIC_ORACLE_TIMEOUT_SECS: u64 = 10;

const RESIDUAL_DIAGNOSTIC_PROBES: &[(&str, &str)] = &[
    ("shared-(z)an", "muʼazan"),
    ("kyj-oko-aw-plus-clitic", "tsãkỹjokwaw"),
    ("shared-(z)an", "moʼazan"),
    ("bare-tsan-ambiguity", "tsãn"),
    ("moa-plus-distinct-za", "moʼaza"),
    ("kyj-oko-aw", "kỹjokwaw"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiagnosticCell {
    Present,
    Absent,
    InconclusiveBudgetExceeded,
    HarnessMismatch,
    NotRun,
}

impl DiagnosticCell {
    fn label(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::Absent => "absent",
            Self::InconclusiveBudgetExceeded => "inconclusive_budget_exceeded",
            Self::HarnessMismatch => "harness_mismatch",
            Self::NotRun => "not_run",
        }
    }
}

struct DiagnosticStageStats {
    name: &'static str,
    elapsed: Duration,
    states: i32,
    arcs: i32,
}

struct DiagnosticPipeline {
    lexc: Fsm,
    post_rules: Fsm,
    final_network: Fsm,
    boundary_tokens: Vec<char>,
    stats: Vec<DiagnosticStageStats>,
}

impl DiagnosticPipeline {
    /// Mirrors `templated_compile.rs` locally because its public result intentionally exposes only the final network.
    fn compile(g: &Grammar) -> Result<Self, String> {
        let table = g
            .char_tables
            .first()
            .ok_or("grammar has no character table")?;
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();
        let mut stats = Vec::new();

        let started = Instant::now();
        let emitted = emit_underlying_templated(g, &alphabet, None);
        if emitted.report.enum_budget_exceeded.is_some()
            || matches!(emitted.report.tier, FomaTier::Unsupported { .. })
        {
            return Err(format!(
                "templated emission is not diagnostic-ready: {:?}",
                emitted.report.tier
            ));
        }
        let lexc = fsm_lexc_parse_string(&opts, None, &emitted.lexc_source)
            .ok_or("templated lexc failed to compile")?;
        let lexc = match pg_foma::structural_allomorph::compile_layer(&opts, g, &alphabet) {
            Some(structural) => fsm_compose(&opts, lexc, structural),
            None => lexc,
        };
        let elapsed = started.elapsed();
        diagnostic_budget("lexc", &lexc, elapsed)?;
        stats.push(DiagnosticStageStats {
            name: "lexc",
            elapsed,
            states: lexc.statecount,
            arcs: lexc.arccount,
        });

        let rules_in_order: Vec<&PhonRuleDef> = g
            .strata
            .iter()
            .flat_map(|stratum| stratum.prules.iter().map(|id| &g.prules[id.0 as usize]))
            .collect();
        if rules_in_order.len() != 18 {
            return Err(format!(
                "selected {} rules; Aweti production expects 18",
                rules_in_order.len()
            ));
        }
        let mut skipped = Vec::new();
        let mut tuple_reports = Vec::new();
        let started = Instant::now();
        let rule_net = compile_and_compose_rules_recall_safe(
            &opts,
            g,
            &alphabet,
            &rules_in_order,
            &mut skipped,
            &mut tuple_reports,
        )
        .map_err(|error| format!("rule compile/compose failed: {error}"))?
        .ok_or("no phonological rule compiled")?;
        if !skipped.is_empty() {
            return Err(format!(
                "diagnostic skipped rules unlike production: {skipped:?}"
            ));
        }
        let post_rules = fsm_compose(&opts, lexc.clone(), rule_net);
        let post_rules = match pg_foma::structural_allomorph::compile_authored_deletion_fallback(
            &opts, g, &alphabet,
        ) {
            Some(fallback) => fsm_compose(&opts, post_rules, fallback),
            None => post_rules,
        };
        let elapsed = started.elapsed();
        diagnostic_budget("post_rules", &post_rules, elapsed)?;
        stats.push(DiagnosticStageStats {
            name: "post_rules",
            elapsed,
            states: post_rules.statecount,
            arcs: post_rules.arccount,
        });

        let boundary_tokens: Vec<char> = table
            .iter()
            .filter(|(_, definition)| definition.kind() == CharDefKind::Boundary)
            .map(|(id, _)| alphabet.token(id))
            .collect();
        if boundary_tokens.is_empty() {
            return Err("no boundary tokens for cleanup".into());
        }
        let cleanup_regex = boundary_tokens
            .iter()
            .map(|token| format!("{token} -> 0"))
            .collect::<Vec<_>>()
            .join(", ");
        let cleanup = fsm_parse_regex(&opts, &cleanup_regex, None, None)
            .ok_or_else(|| format!("cleanup regex failed: {cleanup_regex:?}"))?;
        let started = Instant::now();
        let final_network = fsm_minimize(&opts, fsm_compose(&opts, post_rules.clone(), cleanup));
        let elapsed = started.elapsed();
        diagnostic_budget("final", &final_network, elapsed)?;
        if final_network.statecount == 0 {
            return Err("diagnostic final network is empty".into());
        }
        stats.push(DiagnosticStageStats {
            name: "final",
            elapsed,
            states: final_network.statecount,
            arcs: final_network.arccount,
        });
        Ok(Self {
            lexc,
            post_rules,
            final_network,
            boundary_tokens,
            stats,
        })
    }
}

fn diagnostic_budget(stage: &str, net: &Fsm, elapsed: Duration) -> Result<(), String> {
    if elapsed > Duration::from_secs(DIAGNOSTIC_OPERATION_SECS) {
        return Err(format!(
            "{stage} exceeded {}s: {elapsed:?}",
            DIAGNOSTIC_OPERATION_SECS
        ));
    }
    if net.statecount > DIAGNOSTIC_STATE_CAP || net.arccount > DIAGNOSTIC_ARC_CAP {
        return Err(format!(
            "{stage} exceeded network caps: {} states / {} arcs",
            net.statecount, net.arccount
        ));
    }
    Ok(())
}

/// Accept one encoded surface with cleanup-deleted boundaries before, between, and after tokens.
fn boundary_flexible_identity_fsm(name: &str, token_string: &str, boundaries: &[char]) -> Fsm {
    let mut handle = fsm_construct_init(name);
    let chars: Vec<char> = token_string.chars().collect();
    for state in 0..=chars.len() {
        for boundary in boundaries {
            let symbol = boundary.to_string();
            fsm_construct_add_arc(&mut handle, state as i32, state as i32, &symbol, &symbol);
        }
        if let Some(token) = chars.get(state) {
            let symbol = token.to_string();
            fsm_construct_add_arc(
                &mut handle,
                state as i32,
                (state + 1) as i32,
                &symbol,
                &symbol,
            );
        }
    }
    fsm_construct_set_initial(&mut handle, 0);
    fsm_construct_set_final(&mut handle, chars.len() as i32);
    fsm_construct_done(handle)
}

fn diagnostic_tag_texts(ids: &[u32], root: i32, width: usize) -> Option<Vec<String>> {
    if ids.is_empty() || root < 0 || root as usize >= ids.len() {
        return None;
    }
    Some(
        ids.iter()
            .enumerate()
            .map(|(index, &id)| {
                let id = MorphemeId(id);
                if index as i32 == root {
                    tags::root_tag_text(id, width)
                } else {
                    tags::morph_tag_text(id, width)
                }
            })
            .collect(),
    )
}

fn complete_analysis_in_lexc(net: &Fsm, tags: &[String]) -> DiagnosticCell {
    let opts = FomaOptions::default();
    let started = Instant::now();
    let upper = fsm_minimize(&opts, fsm_upper(net.clone()));
    if diagnostic_budget("lexc_probe", &upper, started.elapsed()).is_err() {
        return DiagnosticCell::InconclusiveBudgetExceeded;
    }
    let mut hit = fsm_intersect(&opts, upper, tag_string_fsm("diagnostic-lexc-tags", tags));
    if fsm_isempty(&opts, &mut hit) {
        DiagnosticCell::Absent
    } else {
        DiagnosticCell::Present
    }
}

fn exact_analysis_surface_pair(
    net: &Fsm,
    matcher: Fsm,
    tags: &[String],
) -> (DiagnosticCell, Option<bool>) {
    let opts = FomaOptions::default();
    let started = Instant::now();
    let restricted = fsm_minimize(&opts, fsm_compose(&opts, net.clone(), matcher));
    if diagnostic_budget("pair_probe", &restricted, started.elapsed()).is_err() {
        return (DiagnosticCell::InconclusiveBudgetExceeded, None);
    }
    let upper = fsm_minimize(&opts, fsm_upper(restricted));
    let atomic = tags
        .iter()
        .all(|tag| upper.sigma.iter().any(|symbol| symbol.symbol == *tag));
    let mut hit = fsm_intersect(&opts, upper, tag_string_fsm("diagnostic-pair-tags", tags));
    let cell = if fsm_isempty(&opts, &mut hit) {
        DiagnosticCell::Absent
    } else {
        DiagnosticCell::Present
    };
    (cell, Some(atomic))
}

struct DiagnosticRow {
    cluster: &'static str,
    word: &'static str,
    analysis_index: Option<usize>,
    morpheme_ids: Vec<u32>,
    root_index: i32,
    oracle: DiagnosticCell,
    oracle_partial: bool,
    lexc: DiagnosticCell,
    post_rules: DiagnosticCell,
    final_pair: DiagnosticCell,
    final_tags_atomic: Option<bool>,
}

impl DiagnosticRow {
    fn earliest_failure(&self) -> &'static str {
        for (stage, cell) in [
            ("oracle", self.oracle),
            ("lexc", self.lexc),
            ("post_rules", self.post_rules),
            ("final", self.final_pair),
        ] {
            if cell != DiagnosticCell::Present {
                return stage;
            }
        }
        "none"
    }
}

/// Compiles stages once and prints the earliest failing boundary for every complete oracle analysis of the pinned residual misses; intentionally red until those misses are fixed.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/aweti.json); run with --include-ignored"]
fn e_residual_misses_report_first_failing_stage() {
    if !have("aweti.json") {
        eprintln!("skipping: aweti.json not present on disk");
        return;
    }
    std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run_residual_miss_diagnostic)
        .expect("spawn residual diagnostic worker")
        .join()
        .expect("residual diagnostic panicked");
}

fn run_residual_miss_diagnostic() {
    let g = load_grammar();
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let width = tags::tag_width(g.morphemes.len());
    let morpher = Morpher::new(&g, ORACLE_STEP_CAP)
        .with_word_timeout(Some(Duration::from_secs(DIAGNOSTIC_ORACLE_TIMEOUT_SECS)));
    let popts = ParseOptions::default();
    let pipeline = DiagnosticPipeline::compile(&g);
    if let Ok(pipeline) = &pipeline {
        println!("diagnostic-bounds\toracle_steps={ORACLE_STEP_CAP}\toracle_timeout_s={DIAGNOSTIC_ORACLE_TIMEOUT_SECS}\toperation_s={DIAGNOSTIC_OPERATION_SECS}\tstate_cap={DIAGNOSTIC_STATE_CAP}\tarc_cap={DIAGNOSTIC_ARC_CAP}");
        for stage in &pipeline.stats {
            println!(
                "diagnostic-stage\t{}\t{:?}\t{}\t{}",
                stage.name, stage.elapsed, stage.states, stage.arcs
            );
        }
    }
    let mut rows = Vec::new();
    for &(cluster, word) in RESIDUAL_DIAGNOSTIC_PROBES {
        let outcome = morpher.parse_word_opts(word, &popts);
        let partial = outcome.capped || outcome.timed_out;
        if outcome.structured.is_empty() {
            rows.push(DiagnosticRow {
                cluster,
                word,
                analysis_index: None,
                morpheme_ids: vec![],
                root_index: -1,
                oracle: if partial {
                    DiagnosticCell::InconclusiveBudgetExceeded
                } else if outcome.invalid_shape {
                    DiagnosticCell::HarnessMismatch
                } else {
                    DiagnosticCell::Absent
                },
                oracle_partial: partial,
                lexc: DiagnosticCell::NotRun,
                post_rules: DiagnosticCell::NotRun,
                final_pair: DiagnosticCell::NotRun,
                final_tags_atomic: None,
            });
            continue;
        }
        for (analysis_index, analysis) in outcome.structured.iter().enumerate() {
            let Some(tag_texts) =
                diagnostic_tag_texts(&analysis.morpheme_ids, analysis.root_morpheme_index, width)
            else {
                rows.push(DiagnosticRow {
                    cluster,
                    word,
                    analysis_index: Some(analysis_index),
                    morpheme_ids: analysis.morpheme_ids.clone(),
                    root_index: analysis.root_morpheme_index,
                    oracle: DiagnosticCell::HarnessMismatch,
                    oracle_partial: partial,
                    lexc: DiagnosticCell::NotRun,
                    post_rules: DiagnosticCell::NotRun,
                    final_pair: DiagnosticCell::NotRun,
                    final_tags_atomic: None,
                });
                continue;
            };
            let Some(query) = alphabet.encode_query(word) else {
                rows.push(DiagnosticRow {
                    cluster,
                    word,
                    analysis_index: Some(analysis_index),
                    morpheme_ids: analysis.morpheme_ids.clone(),
                    root_index: analysis.root_morpheme_index,
                    oracle: DiagnosticCell::HarnessMismatch,
                    oracle_partial: partial,
                    lexc: DiagnosticCell::NotRun,
                    post_rules: DiagnosticCell::NotRun,
                    final_pair: DiagnosticCell::NotRun,
                    final_tags_atomic: None,
                });
                continue;
            };
            let decoded_query: Vec<String> = query
                .chars()
                .map(|token| {
                    let definition = table.get(CharDefId((token as u32).saturating_sub(0xE000)));
                    definition
                        .representations()
                        .first()
                        .cloned()
                        .unwrap_or_else(|| definition.xml_id().to_string())
                })
                .collect();
            println!("diagnostic-query\t{}\t{:?}", word, decoded_query);
            let (lexc, post_rules, final_pair, atomic) = match &pipeline {
                Ok(p) => {
                    let mut lexc_apply = apply_init(&p.lexc);
                    let underlying: Vec<Vec<String>> = lexc_apply
                        .down(&tag_texts.concat())
                        .take(32)
                        .map(|encoded| {
                            encoded
                                .chars()
                                .map(|token| {
                                    if (token as u32) >= 0xF0000 {
                                        format!("<STRUCT:{:X}>", token as u32 - 0xF0000)
                                    } else {
                                        let id = CharDefId((token as u32).saturating_sub(0xE000));
                                        let definition = table.get(id);
                                        let representation = definition
                                            .representations()
                                            .first()
                                            .cloned()
                                            .unwrap_or_else(|| definition.xml_id().to_string());
                                        if definition.kind() == CharDefKind::Boundary {
                                            format!("<{representation}>")
                                        } else {
                                            representation
                                        }
                                    }
                                })
                                .collect()
                        })
                        .collect();
                    println!(
                        "diagnostic-underlying\t{}\t{:?}\t{:?}",
                        word, analysis_index, underlying
                    );
                    let lexc = complete_analysis_in_lexc(&p.lexc, &tag_texts);
                    let (post, _) = exact_analysis_surface_pair(
                        &p.post_rules,
                        boundary_flexible_identity_fsm(
                            "diagnostic-post-word",
                            &query,
                            &p.boundary_tokens,
                        ),
                        &tag_texts,
                    );
                    let (final_pair, atomic) = exact_analysis_surface_pair(
                        &p.final_network,
                        linear_identity_fsm("diagnostic-final-word", &query),
                        &tag_texts,
                    );
                    (lexc, post, final_pair, atomic)
                }
                Err(_) => (
                    DiagnosticCell::HarnessMismatch,
                    DiagnosticCell::HarnessMismatch,
                    DiagnosticCell::HarnessMismatch,
                    None,
                ),
            };
            rows.push(DiagnosticRow {
                cluster,
                word,
                analysis_index: Some(analysis_index),
                morpheme_ids: analysis.morpheme_ids.clone(),
                root_index: analysis.root_morpheme_index,
                oracle: DiagnosticCell::Present,
                oracle_partial: partial,
                lexc,
                post_rules,
                final_pair,
                final_tags_atomic: atomic,
            });
        }
    }
    println!("cluster\tword\tanalysis_index\tmorpheme_ids\troot_index\toracle\toracle_partial\tlexc_complete\tpost_rules_exact_pair\tfinal_exact_pair\tfinal_tags_atomic_diagnostic_only\tfirst_failure");
    for row in &rows {
        println!(
            "{}\t{}\t{:?}\t{:?}\t{}\t{}\t{}\t{}\t{}\t{}\t{:?}\t{}",
            row.cluster,
            row.word,
            row.analysis_index,
            row.morpheme_ids,
            row.root_index,
            row.oracle.label(),
            row.oracle_partial,
            row.lexc.label(),
            row.post_rules.label(),
            row.final_pair.label(),
            row.final_tags_atomic,
            row.earliest_failure()
        );
    }
    if let Ok(pipeline) = &pipeline {
        let recovered = diagnostic_optional_marker_cleanup(&g, &alphabet, pipeline, &rows, width);
        println!(
            "backend-candidate\toptional-floating-marker-cleanup\t{}",
            recovered.join(",")
        );
    }

    if let Some(trace_word) = std::env::var_os("PANGLOSS_AWETI_TRACE_WORD") {
        let trace_word = trace_word.to_string_lossy();
        if let Ok(pipeline) = &pipeline {
            for row in rows.iter().filter(|row| row.word == trace_word) {
                trace_analysis_through_rules(&g, &alphabet, pipeline, row, width);
            }
        }
    }

    if std::env::var_os("PANGLOSS_AWETI_RULE_VARIATIONS").is_some() {
        if let Ok(pipeline) = &pipeline {
            run_rule_variation_diagnostics(&g, &alphabet, pipeline, &rows, width);
        }
    }

    assert!(
        rows.iter().all(|row| row.analysis_index.is_some()),
        "every residual word must supply an oracle analysis; see matrix above"
    );
    let failures: Vec<_> = rows
        .iter()
        .filter(|row| row.earliest_failure() != "none")
        .map(|row| {
            format!(
                "{} analysis {:?}: {}",
                row.word,
                row.analysis_index,
                row.earliest_failure()
            )
        })
        .collect();
    assert!(
        failures.is_empty(),
        "residual exact relations remain red:\n{}",
        failures.join("\n")
    );
}
/// Three bounded diagnostic-only cascade experiments after stage localization: prefix reachability, leave-one-out ablation, and adjacent swaps.
fn run_rule_variation_diagnostics(
    g: &Grammar,
    alphabet: &SegAlphabet,
    pipeline: &DiagnosticPipeline,
    rows: &[DiagnosticRow],
    width: usize,
) {
    let indexed_rules: Vec<(u32, &PhonRuleDef)> = g
        .strata
        .iter()
        .flat_map(|stratum| {
            stratum
                .prules
                .iter()
                .map(|id| (id.0, &g.prules[id.0 as usize]))
        })
        .collect();
    let baseline_order: Vec<&PhonRuleDef> = indexed_rules.iter().map(|(_, rule)| *rule).collect();
    for (position, (id, rule)) in indexed_rules.iter().enumerate() {
        match rule {
            PhonRuleDef::Rewrite(rule) => println!(
                "rule-order\t{position}\t{id}\t{}\t{:?}\t{:?}",
                rule.xml_id, rule.name, rule.dir
            ),
            PhonRuleDef::Metathesis(rule) => println!(
                "rule-order\t{position}\t{id}\t{}\t{:?}\t{:?}",
                rule.xml_id, rule.name, rule.dir
            ),
        }
    }

    println!("rule-variation\talgorithm\tvariant\trecovered_exact_pairs");
    let opts = FomaOptions::default();
    let mut optional_rules = Vec::new();
    for (position, rule) in baseline_order.iter().enumerate() {
        let mut skipped = Vec::new();
        let mut reports = Vec::new();
        let compiled = compile_and_compose_rules_recall_safe(
            &opts,
            g,
            alphabet,
            std::slice::from_ref(rule),
            &mut skipped,
            &mut reports,
        );
        let Ok(Some(rule_net)) = compiled else {
            println!(
                "rule-variation\ttargeted-optional\tposition={position}\tharness_compile_failure"
            );
            optional_rules.clear();
            break;
        };
        if !skipped.is_empty() {
            println!("rule-variation\ttargeted-optional\tposition={position}\tharness_skipped={skipped:?}");
            optional_rules.clear();
            break;
        }
        optional_rules.push(fsm_union(&opts, rule_net, fsm_universal()));
    }
    if optional_rules.len() == baseline_order.len() {
        let outcomes =
            targeted_optional_recoveries(alphabet, pipeline, rows, width, &optional_rules);
        println!(
            "rule-variation\ttargeted-optional\texact-analysis-first\t{}",
            outcomes.join(",")
        );
    }
    for prefix_len in 0..=baseline_order.len() {
        let rules = &baseline_order[..prefix_len];
        let recovered = diagnostic_variant_recoveries(g, alphabet, pipeline, rows, width, rules);
        if !recovered.is_empty() {
            println!(
                "rule-variation\tprefix\t{prefix_len}\t{}",
                recovered.join(",")
            );
        }
    }

    for (omitted, (rule_id, _)) in indexed_rules.iter().enumerate() {
        let rules: Vec<&PhonRuleDef> = baseline_order
            .iter()
            .enumerate()
            .filter_map(|(index, rule)| (index != omitted).then_some(*rule))
            .collect();
        let recovered = diagnostic_variant_recoveries(g, alphabet, pipeline, rows, width, &rules);
        if !recovered.is_empty() {
            println!(
                "rule-variation\tleave-one-out\tposition={omitted};prule={rule_id}\t{}",
                recovered.join(",")
            );
        }
    }

    for left in 0..baseline_order.len().saturating_sub(1) {
        let mut rules = baseline_order.clone();
        rules.swap(left, left + 1);
        let recovered = diagnostic_variant_recoveries(g, alphabet, pipeline, rows, width, &rules);
        if !recovered.is_empty() {
            println!(
                "rule-variation\tadjacent-swap\tpositions={left},{};prules={},{}\t{}",
                left + 1,
                indexed_rules[left].0,
                indexed_rules[left + 1].0,
                recovered.join(",")
            );
        }
    }
}

fn targeted_optional_recoveries(
    alphabet: &SegAlphabet,
    pipeline: &DiagnosticPipeline,
    rows: &[DiagnosticRow],
    width: usize,
    optional_rules: &[Fsm],
) -> Vec<String> {
    let opts = FomaOptions::default();
    let mut outcomes = Vec::new();
    for row in rows.iter().filter(|row| row.analysis_index.is_some()) {
        let Some(tags) = diagnostic_tag_texts(&row.morpheme_ids, row.root_index, width) else {
            continue;
        };
        let Some(query) = alphabet.encode_query(row.word) else {
            continue;
        };
        let mut net = fsm_minimize(
            &opts,
            fsm_compose(
                &opts,
                tag_string_fsm("diagnostic-target-tags", &tags),
                pipeline.lexc.clone(),
            ),
        );
        let mut over_budget = None;
        for (position, rule) in optional_rules.iter().enumerate() {
            net = fsm_minimize(&opts, fsm_compose(&opts, net, rule.clone()));
            if diagnostic_budget("targeted_optional", &net, Duration::ZERO).is_err() {
                over_budget = Some(position);
                break;
            }
        }
        if let Some(position) = over_budget {
            outcomes.push(format!(
                "{}#{:?}=budget@{position}",
                row.word, row.analysis_index
            ));
            continue;
        }
        let matcher = boundary_flexible_identity_fsm(
            "diagnostic-target-word",
            &query,
            &pipeline.boundary_tokens,
        );
        let result = exact_analysis_surface_pair(&net, matcher, &tags).0;
        outcomes.push(format!(
            "{}#{:?}={}",
            row.word,
            row.analysis_index,
            result.label()
        ));
    }
    outcomes
}

fn diagnostic_net_recoveries(
    alphabet: &SegAlphabet,
    pipeline: &DiagnosticPipeline,
    rows: &[DiagnosticRow],
    width: usize,
    post_rules: &Fsm,
) -> Vec<String> {
    let mut recovered = Vec::new();
    for row in rows.iter().filter(|row| row.analysis_index.is_some()) {
        let Some(tags) = diagnostic_tag_texts(&row.morpheme_ids, row.root_index, width) else {
            continue;
        };
        let Some(query) = alphabet.encode_query(row.word) else {
            continue;
        };
        let matcher = boundary_flexible_identity_fsm(
            "diagnostic-net-word",
            &query,
            &pipeline.boundary_tokens,
        );
        if exact_analysis_surface_pair(post_rules, matcher, &tags).0 == DiagnosticCell::Present {
            recovered.push(format!("{}#{:?}", row.word, row.analysis_index));
        }
    }
    recovered
}

fn diagnostic_variant_recoveries(
    g: &Grammar,
    alphabet: &SegAlphabet,
    pipeline: &DiagnosticPipeline,
    rows: &[DiagnosticRow],
    width: usize,
    rules: &[&PhonRuleDef],
) -> Vec<String> {
    let opts = FomaOptions::default();
    let post_rules = if rules.is_empty() {
        pipeline.lexc.clone()
    } else {
        let mut skipped = Vec::new();
        let mut reports = Vec::new();
        let Ok(Some(rule_net)) = compile_and_compose_rules_recall_safe(
            &opts,
            g,
            alphabet,
            rules,
            &mut skipped,
            &mut reports,
        ) else {
            return vec!["harness_compile_failure".into()];
        };
        if !skipped.is_empty() {
            return vec![format!("harness_skipped={skipped:?}")];
        }
        fsm_compose(&opts, pipeline.lexc.clone(), rule_net)
    };
    let mut recovered = Vec::new();
    for row in rows.iter().filter(|row| row.analysis_index.is_some()) {
        let Some(tags) = diagnostic_tag_texts(&row.morpheme_ids, row.root_index, width) else {
            continue;
        };
        let Some(query) = alphabet.encode_query(row.word) else {
            continue;
        };
        let matcher = boundary_flexible_identity_fsm(
            "diagnostic-variant-word",
            &query,
            &pipeline.boundary_tokens,
        );
        if exact_analysis_surface_pair(&post_rules, matcher, &tags).0 == DiagnosticCell::Present {
            recovered.push(format!("{}#{:?}", row.word, row.analysis_index));
        }
    }
    recovered
}
fn trace_analysis_through_rules(
    g: &Grammar,
    alphabet: &SegAlphabet,
    pipeline: &DiagnosticPipeline,
    row: &DiagnosticRow,
    width: usize,
) {
    let Some(tags) = diagnostic_tag_texts(&row.morpheme_ids, row.root_index, width) else {
        return;
    };
    let tag_input = tags.concat();
    let opts = FomaOptions::default();
    let mut net = fsm_minimize(
        &opts,
        fsm_compose(
            &opts,
            tag_string_fsm("diagnostic-trace-tags", &tags),
            pipeline.lexc.clone(),
        ),
    );
    print_trace_outputs(g, row.word, "lexc", &net, &tag_input);
    let indexed_rules: Vec<(u32, &PhonRuleDef)> = g
        .strata
        .iter()
        .flat_map(|stratum| {
            stratum
                .prules
                .iter()
                .map(|id| (id.0, &g.prules[id.0 as usize]))
        })
        .collect();
    for (position, (id, rule)) in indexed_rules.into_iter().enumerate() {
        let mut skipped = Vec::new();
        let mut reports = Vec::new();
        let Ok(Some(rule_net)) = compile_and_compose_rules_recall_safe(
            &opts,
            g,
            alphabet,
            &[rule],
            &mut skipped,
            &mut reports,
        ) else {
            println!(
                "rule-trace\t{}\t{position}\t{id}\tharness_compile_failure",
                row.word
            );
            return;
        };
        net = fsm_minimize(&opts, fsm_compose(&opts, net, rule_net));
        print_trace_outputs(g, row.word, &format!("{position}:{id}"), &net, &tag_input);
    }
}

fn print_trace_outputs(g: &Grammar, word: &str, stage: &str, net: &Fsm, tag_input: &str) {
    let table = &g.char_tables[0];
    let mut handle = apply_init(net);
    let outputs: Vec<String> = handle
        .down(tag_input)
        .take(32)
        .map(|encoded| {
            encoded
                .chars()
                .map(|token| {
                    if (token as u32) >= 0xF0000 {
                        format!("<STRUCT:{:X}>", token as u32 - 0xF0000)
                    } else {
                        let id = CharDefId((token as u32).saturating_sub(0xE000));
                        let definition = table.get(id);
                        let representation = definition
                            .representations()
                            .first()
                            .map(String::as_str)
                            .unwrap_or_else(|| definition.xml_id());
                        if definition.kind() == CharDefKind::Boundary {
                            format!("<{representation}>")
                        } else {
                            representation.to_string()
                        }
                    }
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .collect();
    println!("rule-trace\t{word}\t{stage}\t{outputs:?}");
}
fn diagnostic_optional_marker_cleanup(
    g: &Grammar,
    alphabet: &SegAlphabet,
    pipeline: &DiagnosticPipeline,
    rows: &[DiagnosticRow],
    width: usize,
) -> Vec<String> {
    let table = &g.char_tables[0];
    let marker_tokens: Vec<char> = ["ᵀ", "°"]
        .iter()
        .filter_map(|representation| table.lookup_nfd(representation))
        .map(|id| alphabet.token(id))
        .collect();
    if marker_tokens.len() != 2 {
        return vec![format!("harness_marker_count={}", marker_tokens.len())];
    }
    let regex = marker_tokens
        .iter()
        .map(|token| format!("{token} -> 0"))
        .collect::<Vec<_>>()
        .join(", ");
    let opts = FomaOptions::default();
    let Some(delete_markers) = fsm_parse_regex(&opts, &regex, None, None) else {
        return vec!["harness_marker_regex_failed".into()];
    };
    let optional_cleanup = fsm_union(&opts, delete_markers, fsm_universal());
    let candidate = fsm_minimize(
        &opts,
        fsm_compose(&opts, pipeline.post_rules.clone(), optional_cleanup),
    );
    if let Err(error) = diagnostic_budget("optional_marker_cleanup", &candidate, Duration::ZERO) {
        return vec![format!("budget={error}")];
    }
    diagnostic_net_recoveries(alphabet, pipeline, rows, width, &candidate)
}
