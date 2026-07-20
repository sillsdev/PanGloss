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
//! ## Why there is no full-corpus recall gate here — a second, more severe finding
//! Composing the templated lexc with Aweti's 18-rule phonological cascade + boundary cleanup
//! exposes a SEPARATE structural gap, discovered while building this gate, that is MORE severe
//! than the anticipated "41 truncation-rule" wrinkle (see the crate's own P6-Aweti task report for
//! the full write-up; summarized here since it is the reason this file's scope is what it is):
//!
//! `apply_up` against the composed network is safe and fast for SOME query words but can take
//! many minutes — or, empirically, run long enough that even an external process-level kill was
//! the only way to stop it — for OTHERS, with no reliable way to predict which from the query
//! alone, and NO safe way to bound it from inside the test process:
//! - A per-iteration wall-clock check (checked between successive `apply_up` yields) does not
//!   help: for a pathological query, the search can fail to yield even ONE result for a very long
//!   time — there is nothing to check between.
//! - A separate worker thread + `mpsc::Receiver::recv_timeout` (verified independently to work
//!   correctly in this exact build against a synthetic always-spinning thread) ALSO failed to
//!   bound wall time empirically once pointed at a real pathological query — the main thread's
//!   timeout never fired within several minutes of wall clock, for reasons not fully isolated
//!   (cross-thread `Fsm` sharing via `Arc` may itself be unsound here; not pursued further, out of
//!   scope for a P6 emitter task and squarely the kind of vendored-toolkit finding this codebase's
//!   own convention is to report precisely rather than debug blind, `p6-prototype-report.md` §2.2/
//!   §2.3's own precedent).
//! - Only an OS-level, OUT-OF-PROCESS timeout (the caller's own `timeout <n> cargo run ...`)
//!   reliably stops it.
//!
//! Empirically, of a small sample tested individually with external-kill safety: the bare root
//! `"parua"` (oracle: exactly 1 analysis, no affixes) resolves correctly in under 1ms; the short
//! word `"an"` (also oracle: exactly 1 analysis) completes 250,000 raw results FAST (~146ms) but
//! does NOT decode the correct analysis anywhere in that window (a genuine miss, not a timeout);
//! the short word `"ti"` did not complete even 500 raw results within 45 seconds of wall clock and
//! had to be killed externally. Given a `#[test]` that hangs indefinitely would block `cargo test`
//! itself (no per-test timeout in stock `cargo test`), shipping a full-corpus (or even
//! full-affixed-word) recall gate here would be irresponsible — it could hang CI or any
//! contributor's local run with no bound. Per the task brief's own "STOP and report a real
//! structural finding, don't hack around it" directive: this file's test (b) is deliberately
//! scoped to the ONE word verified safe AND correct (`"parua"`) rather than attempting a broader
//! claim this investigation cannot back up safely. The crate's own P6-Aweti task report carries
//! the full investigation trail, the specific words/behaviors observed, and the recommended next
//! step (very likely: fixing `build_deriv_chain`'s "same full rule set re-offered at every
//! level" design, which — combined with several of Aweti's mrules having an "elsewhere" allomorph
//! whose entire insert text is a single Boundary-kind token, collapsed to a true epsilon by
//! `boundary_cleanup` — is the best-supported hypothesis for why some queries make `apply_up`'s
//! search space effectively unbounded).

use std::path::{Path, PathBuf};
use std::time::Instant;

use foma::apply::apply_init;
use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;

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

/// Verified-safe-AND-correct query for test (b) — module doc. A bounded, generous raw-result cap
/// (module doc: "parua" resolves in <1ms, nowhere near this) is kept as a defensive backstop, not
/// because it is expected to bind.
const SAFE_WORD: &str = "parua";
const SAFE_WORD_RAW_CAP: usize = 50_000;

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

/// (b) SPOT-CHECK RECALL on the one word verified both safe AND correct (module doc — this file's
/// scope is deliberately narrow; see the module doc and the crate's own P6-Aweti task report for
/// why a broader recall claim is not made here). `"parua"` decodes its single oracle analysis at
/// the very first `apply_up` result.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/aweti.json); run with --include-ignored"]
fn b_aweti_spot_check_recall_parua() {
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

    let morpher = Morpher::new(&g, usize::MAX);
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
         raw result #1 in well under 1ms; a regression here is a real finding, not the known \
         apply_up-scale issue this file's module doc describes (that issue affects OTHER words, \
         not this one)",
        engine_seqs[0]
    );
}
