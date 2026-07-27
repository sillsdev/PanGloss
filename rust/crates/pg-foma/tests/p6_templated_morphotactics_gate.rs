//! ## Delanguaging Part C note (2026-07-25)
//! Renamed off the real language's name (was `p6_aweti_gate.rs`). Still corpus-blocked: needs
//! `samples/data/aweti.json` + `samples/data/aweti-words.txt` (gitignored). This IS the gate for
//! the exact historical pathology Part C set out to reproduce synthetically (this file's own
//! §"deriv chain" doc below, and `docs/fst-plan/p6-deep-truncation-chain-report.md`) — MEASURED
//! result (see `tests/phase_c_chain_scale.rs`'s own module doc, `examples/deep_chain_scale_probe.rs`/
//! `examples/deep_chain_compose_probe.rs`): a synthetic deep standalone-affix chain
//! (`pg_grammar_gen::build::chain`) at this grammar's own real per-zone rule-count scale (N=24)
//! does NOT reproduce the apply_up explosion/OOM this grammar historically hit — both the bare net
//! and one composed against a trivial identity rule stay in the microsecond range even on a
//! deliberately maximally-path-ambiguous query. The likely missing ingredient (not independently
//! confirmed) is real, content-differentiated rule interaction (this grammar's own real
//! phonological conditioning + its two independent per-zone chain instances), which a synthetic
//! recipe using inert identity-like rules cannot exercise. This gate therefore stays genuinely
//! corpus-blocked, not merely unattempted. Kept `#[ignore]`d unconditionally.
//!
//! P6 templated-morphotactics acceptance gate (`docs/fst-plan/p6-prototype-report.md` §6 item 2,
//! `docs/fst-plan/foma-fst-plan.md` §P6): the Aweti gate, mirroring
//! `examples/p6_aweti_replace_prototype.rs`'s own compose flow, as a real, CI-shaped `#[ignore]`d
//! test — matching `f2_junction_gate.rs`/`f3_interdigitation_gate.rs`'s own self-skip-guard convention
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
//! [`b_full_corpus_recall_via_compose`] uses the composition technique (word-FST `.o.`
//! composed net, `fsm_upper`, intersect against each oracle analysis's own tag acceptor,
//! `fsm_isempty`) — an ordinary, terminating automaton construction with NO backtracking search
//! and NO query-ordering dependence, safe to run over the whole corpus (`Morpher::new(&g,
//! ORACLE_STEP_CAP)` for the oracle throughout — `usize::MAX` is NOT actually safe for Aweti,
//! `docs/fst-plan/p6-aweti-truncation-chain-report.md`'s own Q3 finding: the corpus word
//! `"tomoʼatu"` ran the HC engine itself for >10 minutes uncapped).
//!
//! **Measured: 32/104 = 30.8% (honest, post-detection).** History of this number:
//! - A chain-restriction-era diagnostic measured 68/104 (equivalently 65/101 with 3 probe words
//!   excluded). That figure was inflated: it was reachability inside a network in which Aweti's
//!   two non-`Iterative`/`LeftToRight` rules were SILENTLY MIS-COMPILED as plain foma `->` (the
//!   old `replace.rs` read and discarded `rule.mode`/`rule.dir`), i.e. a wrong-but-permissive net.
//! - Phase C (`docs/fst-plan/phase-c-generator-design.md` §5/§6) wired
//!   `pg_foma::replace::is_fully_supported_shape` into `compile_rewrite_rule_subset`: a rule
//!   outside `Iterative`/`LeftToRight` is now DETECTED and honestly reported `skipped` rather than
//!   silently mis-mapped. Aweti has exactly two such rules —
//!   `e41e45d9-6eb8-45f1-a16b-a6a05fa6bb6c` (`Dir::RightToLeft`) and
//!   `2996dcb3-2e00-4d41-926e-fe5ed11f0753` (`RewriteMode::Simultaneous`) — so composing only the
//!   16 correctly-compilable rules drops recall to the honest 32/104. This is the INTENDED
//!   consequence of the fix (design doc §5: "recall drops honestly; never silently wrong"), chosen
//!   deliberately (John, 2026-07-20) over keeping the silent-but-lucky 68.
//! - **Recovering the ~36 skipped-rule-dependent words is the job of a real `RightToLeft`
//!   (`fsm_reverse`) + `Simultaneous` compiler — scheduled follow-on work, not a defect in this
//!   gate.** When it lands, this gate's floor rises and `BASELINE_MISSES` shrinks accordingly.
//!
//! **UPDATE (`openspec/changes/compile-right-to-left-rewrites`, 2026-07-24):** the `RightToLeft`
//! half of that follow-on has now landed (`pg_foma::replace::compile_rtl_branch_net`, real
//! reversal-plus-safety-net-union semantics — see that function's own doc). `is_fully_supported_
//! shape` now reports Aweti's `Dir::RightToLeft` rule (`e41e45d9-6eb8-45f1-a16b-a6a05fa6bb6c`)
//! fully-supported; only the `Simultaneous` rule (`2996dcb3-2e00-4d41-926e-fe5ed11f0753`) remains
//! honestly skipped below (`Simultaneous` is a separate, not-yet-built algorithm — design.md of
//! this change: "separate from Simultaneous mode because both require different algorithms").
//! **NOT RUN**: re-measuring this gate's actual recall floor against the real, gitignored Aweti
//! corpus (`samples/data/aweti.json`, absent from this checkout/CI) is the corpus re-run this
//! change's own `tasks.md` item 2.2 calls for — recorded here as `not_run` (missing prerequisite:
//! the gitignored corpus data), per this repo's own not-run convention, rather than blocked on or
//! guessed at. The `32/104`/`BASELINE_MISSES` figures below are therefore STALE (pre-RTL-fix) and
//! are expected to rise once someone with the real corpus data re-runs this `#[ignore]`d test —
//! left as-is (not fabricated a new number) until that re-run happens.
//!
//! A separate, still-unexplained gap (a bare root with zero affixes, `"mã"`, also misses this
//! recall check even with the entire phonological cascade removed from the composition) is
//! documented in `docs/fst-plan/p6-aweti-truncation-chain-report.md` §3 as an open finding.
//! (A companion marker-token truncation mechanism was designed and validated sound but NOT shipped
//! — premise refuted for Aweti, 0/16 recall gain; report §2.) The gate below asserts the ACHIEVED
//! honest figure (`>= 32`), and separately asserts no regression against the documented 72-word
//! post-detection miss list (`BASELINE_MISSES`).
//!
//! ## `apply_up` termination spot-check (test (c))
//! `"parua"` is now itself among the skipped-rule-dependent misses (its single oracle analysis
//! needs one of the two skipped rules), so test (c) no longer asserts recall. Its durable value is
//! the chain restriction's real guarantee: `apply_up` on the composed net TERMINATES promptly and
//! does not explode (pre-restriction, `"ti"`/`"parua"` hung indefinitely). It enumerates up to
//! [`SAFE_WORD_RAW_CAP`] raw results and asserts the enumeration completes well within a generous
//! wall-clock bound. Uses [`ORACLE_STEP_CAP`] (not `usize::MAX`) for the oracle `Morpher`.

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

/// `apply_up` termination-probe query for test (c) — module doc. `"parua"` is now a
/// skipped-rule-dependent miss (its analysis needs one of Aweti's two honestly-skipped
/// RightToLeft/Simultaneous rules), so test (c) probes that `apply_up` enumerates up to this
/// bounded raw-result cap and TERMINATES promptly (the chain restriction's guarantee), not that
/// the word is recalled.
const SAFE_WORD: &str = "parua";
const SAFE_WORD_RAW_CAP: usize = 50_000;

/// Any oracle `Morpher` call in this file uses this cap, never `usize::MAX` (module doc /
/// `docs/fst-plan/p6-aweti-truncation-chain-report.md`'s own Q3 finding: `Morpher::new(&g,
/// usize::MAX)` is NOT actually bounded for Aweti — the corpus word `"tomoʼatu"` ran the HC engine
/// itself for >10 minutes uncapped, two independent runs, neither `StepBudget` bound ever
/// tripping).
const ORACLE_STEP_CAP: usize = 20_000;

/// The POST-DETECTION baseline miss list — every corpus word with an oracle analysis that the
/// composed net does NOT recall. The no-regression assertion in (b) requires every word NOT in
/// this list to keep recalling. When a real RightToLeft/Simultaneous compiler lands (now landed,
/// see the module doc's "UPDATE" notes and (a)'s `skipped_rules == []` assertion), any newly-
/// recalling word simply drops off the *miss* list on its own (recall test (b) never requires
/// pruning entries that start passing) — a word leaving the recalled set (going from "not in this
/// list" to "in this list") is a real regression and must be investigated before being added here.
///
/// **2026-07-25 addition — `"tsãkỹjokwaw"`/`"tsãtomoʼatu"` (deep-truncation-chain corpus-gate
/// investigation):** these two words newly appear in the miss list not because anything about
/// pg-foma's compiled FST regressed, but because the ORACLE (`pg_parse::Morpher`, used only to
/// decide which corpus words even get counted/checked at all) started finding an analysis for them,
/// within the SAME fixed `ORACLE_STEP_CAP`, where it previously found none. Full investigation:
///
/// 1. **Why they are newly COUNTED (an accounting change, not a language-modeling one).** Both
///    words got exactly 0 oracle analyses at `ORACLE_STEP_CAP` (20,000) on every commit through
///    `485d566` (the parent of `391c2c3`) — confirmed by direct instrumentation, not inferred —
///    so `n_with_oracle` never included them and they could not appear in this list. Raising the
///    step cap alone (no code change) to 2,000,000 makes the oracle find exactly the same
///    analyses these words now get at the unmodified 20,000 cap
///    (`tsãkỹjokwaw` → `[805, 359, 715, 14]` root_idx=1; `tsãtomoʼatu` → 3 analyses incl.
///    `[805, 885, 795]` root_idx=1) — i.e. these analyses were always reachable, just not within
///    budget. `git bisect run` against the actual `b_aweti_full_corpus_recall_via_compose` output
///    pinned the exact commit where the miss-list membership flips: `391c2c3` ("parse: support
///    supplied-root overlays"). That commit touches ONLY `pg-rules`/`pg-parse` (never `pg-foma`),
///    and its one behavior-relevant change for an overlay-less `Morpher::new` call (this gate's own
///    usage) is adding a `root_provenance` field to `pg_rules::word::WordKey` — the
///    analysis-memoization dedup key. That extra field perturbs `WordKey`'s hash, which perturbs
///    `HashMap<WordKey, Word>` iteration order during the step-capped BFS analysis search, which
///    perturbs which candidates get explored before the cap trips — a resource-budget artifact, not
///    a correctness change on either the oracle or FST side. None of the nine pg-foma Stage-2
///    commits this investigation's brief flagged as suspects (`2a98634`, `9473233`, `1576531`,
///    `18e6835`, `00994e7`, `0ed2545`, `42d5757`, `c4a3d22`, `318efe6`) are responsible — recall
///    for these two words is identical at every one of those commits (verified by checking out
///    each and re-running the recall test).
///
/// 2. **Why `"tsãkỹjokwaw"` genuinely does not recall.** Both of its oracle analyses' roots
///    require morpheme 805 (Aweti's `"tsã(n)="` proclitic, `mrule105`, a standalone `AffixProcess`
///    rule that lives in stratum 1 — this grammar's own `"Clitics"` stratum, layered above stratum
///    0's root/template stratum). `emit_underlying_templated` DOES classify `mrule105` as
///    `Role::Prefix`, include it in `deriv_prefix`, declare its `<M:0805>` tag in
///    `Multichar_Symbols`, and write its lexicon entries (`TmplDispatch`/`TLRoots`/`G0Roots`/
///    `G1Roots`/`G2Roots` continuations all present, verified by grepping the emitted
///    `lexc_source`).
///
///    **CORRECTION (2026-07-25) — the sigma-based explanation below was WRONG; the CONCLUSION
///    (this word does not recall) is nevertheless still correct and re-verified.** The original
///    reasoning was: `<M:0805>` is absent from the compiled `lexc_net`'s sigma, therefore foma
///    treats the fragment as dead code, therefore the stratum-1 layer is unreachable. That
///    inference is invalid. A dedicated investigation of the `foma` crate found a narrow port
///    defect (filed as `divvun/foma-rs#2`): any `Multichar_Symbols` name containing a literal `0`
///    digit is silently omitted from `sigma` because the declaration path normalizes the lexer's
///    `@ZERO@` marker while the entry tokenizer still looks for the un-normalized form. `0805`
///    contains zeros, so its absence from `sigma` is that bookkeeping artifact — NOT evidence of
///    unreachability. `apply_down` traverses such tags fine; the network's language is intact.
///    C foma does not have this defect (`lexc_deescape_string` normalizes in one unified pass).
///
///    So: `"tsãkỹjokwaw"` IS still genuinely missed — re-measured against the real corpus on
///    2026-07-25 after upgrading to foma 0.4.2, RECALL 68/106 with this word still in the miss
///    list — but **the true cause is NOT YET DETERMINED.** The stratum-above-root wrapping
///    hypothesis is plausible and unrefuted, but it is no longer supported by the sigma evidence
///    that originally motivated it, and must be re-established (or discarded) by a fresh
///    investigation that does not rely on `sigma` for reachability. Recorded this way deliberately:
///    an unexplained-but-verified miss is honest; a confidently-wrong explanation is not.
///
/// 3. **`"tsãtomoʼatu"` is murkier — NOT proven to be the same root cause.** Unlike
///    `"tsãkỹjokwaw"`, the word-restricted composed net for `"tsãtomoʼatu"` is NON-empty (the FST
///    does produce this surface form), and `apply_up` on the full composed net independently
///    decodes a raw candidate `[805, 885, 795]` root_idx=1 — an EXACT match for one of the oracle's
///    3 analyses. Yet the same investigation that found this also found `<M:0805>` absent from
///    that restricted-and-projected net's own sigma, so that decoded candidate cannot be trusted at
///    face value either (most likely an `apply_up` unknown-symbol/identity artifact echoing input
///    it could not actually match against a real arc, though this was not conclusively pinned down
///    the way `"tsãkỹjokwaw"`'s cause was). Net effect: `"tsãtomoʼatu"` most likely fails for the
///    SAME `mrule105`/stratum-1 wiring gap as `"tsãkỹjokwaw"` (it needs the identical morpheme 805),
///    but this file does not claim that with the same certainty. Recorded as an open, honestly
///    uncertain sub-finding rather than papered over.
///
/// Both words are added below rather than left to fail this gate, because (per this file's own
/// module doc / the repo's never-overclaim standard) the honest conclusion is "these were never
/// actually recallable by the FST, and are now counted" — not "the FST regressed." A silently
/// adjusted assertion without this documented proof would have been the wrong fix.
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

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

/// Self-skip guard: gitignored real-corpus fixtures aren't present in a fresh clone or CI —
/// matches `f2_junction_gate.rs`/`f3_interdigitation_gate.rs`'s own `have()` convention exactly.
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

/// (a) EMIT + COMPILE + COMPOSE: the P6 milestone itself. `emit_underlying_templated` must
/// produce a usable, non-`Unsupported` network for Aweti with plausible counts, the templated
/// lexc must foma-compile, the 18-rule cascade must compile+compose, and the full
/// `lexc .o. rules .o. cleanup` composition + minimize must succeed — all of this is exactly what
/// OOMs via the mainline `emit()` (Fix 1's enumeration budget trips in the composite
/// pre-expansion stage before any of this is reached), so completing it at all is the deliverable.
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
    assert!(
        result.report.counts.lexc_lines > 0,
        "expected at least one lexc line"
    );

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
    assert_eq!(
        rules_in_order.len(),
        18,
        "Aweti declares exactly 18 phonological rules"
    );

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
    println!(
        "rule compile+compose: {:?}; skipped={skipped_rules:?}",
        t_rules.elapsed()
    );
    // UPDATE (2026-07-25, `compile-simultaneous-rewrites`, commit 42d5757 "feat(pg-foma): compile
    // admitted Simultaneous rewrites"): Aweti's `RewriteMode::Simultaneous` rule
    // (`2996dcb3-2e00-4d41-926e-fe5ed11f0753`) now ALSO compiles -- `capability::
    // simultaneous_rule_admitted_for_compile` proves this rule's subrules pairwise non-overlapping
    // (the D3 `simultaneous.subrule-overlap` predicate), so `is_fully_supported_shape` admits it and
    // `compile_rewrite_rule_subset` compiles it via the same sequential-compose loop `Iterative`
    // rules use (module doc on `compile_rewrite_rule_subset`, "Mode/dir detection"). Combined with
    // the earlier `compile-right-to-left-rewrites` landing (module doc's "UPDATE" note just above),
    // ALL 18 of Aweti's phonological rules now compile honestly -- `skipped_rules` is genuinely
    // empty, real forward progress, not a stale/relaxed expectation. Pinning `[]` (not just "fewer
    // are skipped") is still the meaningful guard: if a rule EVER starts being skipped again (a
    // regression) or a 19th/20th rule is added and mis-shaped, this fails and forces this gate's
    // recall floor / miss list to be revisited (module doc).
    let mut skipped_sorted = skipped_rules.clone();
    skipped_sorted.sort();
    assert_eq!(
        skipped_sorted,
        Vec::<String>::new(),
        "expected all 18 of Aweti's phonological rules to compile now that both RightToLeft and \
         Simultaneous are supported; got {skipped_rules:?}"
    );

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
    assert!(
        composed.statecount > 0,
        "composed network must be non-empty"
    );
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
/// the miss list; asserts the ACHIEVED honest recall (`>= 32/104` — module doc explains the
/// 68→32 drop from honestly skipping Aweti's two RightToLeft/Simultaneous rules) and that no
/// previously-recalled word has regressed (every corpus word NOT in [`BASELINE_MISSES`] must
/// still recall).
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

    let result = emit_underlying_templated(&g, &alphabet, None);
    let lexc_net = fsm_lexc_parse_string(&opts, None, &result.lexc_source)
        .unwrap_or_else(|| panic!("Aweti templated lexc failed to foma-compile"));
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

    // Honest achieved-figure floor (module doc): 32/104 with Aweti's two RightToLeft/Simultaneous
    // rules honestly skipped rather than silently mis-compiled. A `>=` floor (not `==`) so a real
    // RTL/Simultaneous compiler landing later RAISES recall without tripping this line — but such a
    // win must also shrink BASELINE_MISSES (the no-regression check below), so the two move together.
    assert!(
        n_recalled >= 32,
        "recall regressed below the honest post-detection baseline (32/104): {n_recalled}/{n_with_oracle} (miss list: {missed_words:?})"
    );

    // No-regression assertion: every corpus word with an oracle analysis NOT in the documented
    // baseline miss list must still recall now.
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
}

/// (c) `apply_up` TERMINATION spot-check (module doc). `"parua"`'s single oracle analysis now needs
/// one of Aweti's two honestly-skipped RightToLeft/Simultaneous rules, so it is NOT recalled — but
/// the durable property under test is the chain restriction's guarantee that `apply_up` on the
/// composed net TERMINATES promptly and does not explode (pre-restriction it hung indefinitely).
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
    // parua is a skipped-rule-dependent miss now (module doc), so `covered` is expected false — we
    // intentionally do NOT assert on it (that would be brittle against a future RTL/Simultaneous
    // compiler flipping it true). The durable guarantee is termination/latency: the chain
    // restriction bounds apply_up so enumerating up to SAFE_WORD_RAW_CAP results completes quickly
    // (pre-restriction this hung indefinitely and had to be killed externally). 30s is a generous
    // machine-independent ceiling; the real figure is well under a few seconds.
    assert!(
        elapsed < std::time::Duration::from_secs(30),
        "apply_up on {SAFE_WORD:?} took {elapsed:?} to enumerate {raw_n} raw results (cap \
         {SAFE_WORD_RAW_CAP}) -- the chain restriction is supposed to keep this prompt; a hang or \
         blowup regression is a real finding"
    );
}

/// (d) Bare-root TAG ATOMICITY boundary (2026-07-27 templated-morphotactics recall investigation,
/// docs/superpowers/plans/2026-07-21-aweti-correctness-performance-plan.md Tasks 2/3): pins the
/// EXACT boundary where the historically-missing bare root `"mã"` (morpheme 400) diverged from a
/// recalled bare root of the same entry shape, and stands as a permanent regression guard for the
/// root cause.
///
/// ## The divergence, established by direct investigation (not inspection)
/// `foma::apply::apply_up` on the FULLY COMPOSED net finds the correct `<R:400>` tag for `"mã"`
/// directly, at every pipeline stage (lexc alone, lexc+rules, lexc+rules+cleanup, minimized) --
/// proving the compiled network's LANGUAGE always contained this analysis. Yet
/// `b_full_corpus_recall_via_compose`'s own compose-restrict-project-intersect recall-counting
/// technique reported it MISSING (along with 31 other words) before the fix below. The FIRST place
/// the two techniques diverge is `foma::constructions::fsm_intersect`: it requires the tag to be
/// registered as ONE atomic multichar symbol in both operands' `sigma`, and the restricted net's own
/// `upper.sigma` was missing the exact string `"<R:400>"` even though `apply_up` finds it fine.
/// `pg_foma::tags`'s own module doc (point 3) explains why: a `divvun/foma-rs` upstream defect in
/// `lexc_string_to_tokens` silently decomposes any `Multichar_Symbols` declaration whose NAME
/// contains a literal `0` digit into a run of single-character arcs -- invisible to `apply_up`/
/// `apply_down` (the concatenated string is identical either way) but fatal to any construction,
/// like `fsm_intersect`, that expects the tag to be one indivisible alphabet symbol. Every one of
/// the 32 words this fix newly recalls has a morpheme id whose zero-padded numeral contains a `0`;
/// every remaining miss does not (see `BASELINE_MISSES` and the current miss list).
///
/// **This is NOT the already-fixed combining-mark boundary bug**
/// (`pg_foma::emit::boundary_combining_run_symbols`): `"mã"`'s own char-def is ONE precomposed
/// segment (not a base char-def immediately followed by a standalone-combining-mark char-def
/// straddling the boundary that fix covers), and other combining-mark-bearing roots recall fine
/// (e.g. `"kitã"`, morpheme 395, probed below) -- the divergence tracks the tag NUMBER (does its
/// zero-padded id contain a `0`?), never the word's own spelling.
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

    // Four bare-root probes, all `root_index==0`/zero-affix single-morpheme entries:
    //   - "mã" (400): the historically-missing word this whole investigation started from.
    //   - "ma" (69): plain ASCII, ALSO historically missing -- rules out a combining-mark cause
    //     (no diacritic at all, yet the same divergence: 69's zero-padded id also contains a `0`).
    //   - "ta" (894): recalled control, no `0` in its zero-padded id.
    //   - "kitã" (395): recalled control that DOES carry a combining-mark root, no `0` in its
    //     zero-padded id -- proves the boundary-combining-mark fix is irrelevant to this bug.
    let probes = [("mã", 400u32), ("ma", 69u32), ("ta", 894u32), ("kitã", 395u32)];

    for (word, mid) in probes {
        let query = alphabet
            .encode_query(word)
            .unwrap_or_else(|| panic!("{word:?} failed to segment into token space"));
        let tag = tags::root_tag_text(pg_grammar::model::MorphemeId(mid), width);

        // Boundary 1: apply_up on the FULLY COMPOSED net finds the tag directly, for every probe
        // (module doc: "the compiled network's language is unaffected"). Rules out "the network
        // never contained this path at all".
        let mut handle = apply_init(&composed);
        let found_via_apply_up = handle.up(&query).any(|s| s.contains(&tag));
        assert!(
            found_via_apply_up,
            "{word:?}: apply_up on the composed net must find {tag:?} directly (the network's own \
             language always contains this bare-root analysis)"
        );

        // Boundary 2: the compose-restrict-project-intersect technique
        // (`b_full_corpus_recall_via_compose`'s own method) -- restrict to the query, project
        // upper, and check whether the exact tag string is registered as one atomic symbol in the
        // restricted net's own sigma. THIS is where the divergence used to live: pre-fix,
        // "mã"/"ma" (ids containing a `0`) failed this exact check while "ta"/"kitã" (no `0`)
        // passed -- not because the language differed, but because `tags.rs`'s numeral encoding
        // used to let a literal `0` reach the compiled `Multichar_Symbols` name (see that module's
        // doc, point 3, for the upstream `lexc_string_to_tokens` mechanism).
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
