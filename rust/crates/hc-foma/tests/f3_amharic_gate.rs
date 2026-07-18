//! P1d gate (docs/fst-plan/foma-fst-plan.md §P1d, "Amharic capability stage — required, no
//! fallback tier"): the emitter + `crate::preexpand` (rule-application pre-expansion and
//! boundary-fusion composite probing) against the real Amharic grammar (76 lexical entries, 87
//! `MorphologicalRule` + 1 `CompoundingRule`, 15 templates, 7 phonological rules over a
//! 417-Segment char-def table), with the FULL ENGINE (`hc_parse::Morpher`) as the recall oracle —
//! same denominator rules as `tests/f1_sena_gate.rs`/`tests/f2_indonesian_gate.rs`.
//!
//! ## History: why this gate's verdict changed (P1c -> P1d)
//!
//! The P1c stage of this gate measured 4/36 (~11%) recall and recorded a fallback verdict. That
//! verdict is SUPERSEDED by the revised plan §0 (settled architecture, John 2026-07-15): "We will
//! never do a full HC backup, we will always use FST to propose and HC to prune." There is no
//! fallback tier; a grammar below 100% proposer recall is a compiler capability gap to close.
//! P1c's investigation classified all 32 misses into exactly two classes (no third):
//!
//! - **Interdigitation (24/32):** `Role::Infix` stem-formation rules (`-pfv-`/`-conv-`/`-ipfv-`)
//!   interleave `InsertSegments` around a `Copy` of the root (root "ህይድ" + `-ää-` -> "ህäይäድ" ->
//!   phonology -> "ሄድ") — no literal two-entry lexc split exists.
//! - **Ge'ez boundary fusion (8/32):** root-final + suffix-initial glyphs fuse into a DIFFERENT
//!   glyph ("ልጅ" + "+ዮች" -> "ልጆች") — outside the deletion-only junction model
//!   (`PhonologyProbe::variants`/`deletion_junctions` both measurably return nothing useful here).
//!
//! **P1d closes both** with one generic mechanism (`hc-foma/src/preexpand.rs`, module doc there):
//! seed a real `hc_rules::word::Word` from each root allomorph (feature-bearing re-segmentation,
//! exactly like the engine's own lexical lookup), apply each candidate rule via the engine's own
//! `hc_rules::morph::synthesize`, render through the real phonological cascade
//! (`hc_rules::surface_probe::probe_synthesize`), and emit any result the ordinary two-entry lexc
//! path cannot reach as ONE composite entry carrying the whole chain's tags in the ENGINE'S OWN
//! morph order (computed by replaying `allomorphs_in_morph_order` over the synthesized word's morph
//! records — never assumed from the rule's role; plan §2's positional match trap). Chains recurse
//! to depth 3 THROUGH clean steps ("ሌባዎቹ" = root + def.m (clean) + pl (fuses with def.m's ው) +
//! poss.3m (fuses again) — found by this gate itself at 31/32). Nothing is Amharic-specific:
//! Sena (no phon rules, no infix rules) short-circuits to zero composites byte-for-byte, and
//! Indonesian emits zero composites because its junctions are all reachable through the existing
//! deletion-junction model (both re-asserted by their own gates, run as part of this suite).
//!
//! ## Engine morph-order findings (measured, the design's ground truth)
//!
//! `hc_parse::Morpher` (uncapped, `ParseOptions::default()`) on interdigitation corpus words:
//! - "ሄደ":  root_index=0, morphemes=[107(entry43/go), 11(mrule13/-pfv-), 16(mrule18/pfv.3m)]
//! - "ሄደች": root_index=0, morphemes=[107(entry43/go), 11(mrule13/-pfv-), 17(mrule19/pfv.3f)]
//! - "ሄዳችሁ": root_index=0, morphemes=[107(go), 4(mrule6/-conv-), 35(mrule37/conv.2p)] (and a
//!   second analysis via -pfv- + pfv.2p)
//! - "ሰብረህ": root_index=0, morphemes=[117(entry53/break), 4(-conv-), 30(mrule32/conv.2m)]
//!
//! The root morpheme is FIRST in every interdigitated analysis (the infix rule's tag follows the
//! root's), because the engine seeds the root's `MorphRecord` at order 0 and the rule's inserted
//! material lands at interior order 1+ (`hc-parse/src/morpher.rs:564`; `Word::morphs` sorted by
//! `order`). `preexpand::morph_order_tags` reproduces exactly this by construction, and a
//! prefix-rule composite ("ላንተ": [1(mrule3/to), 92(entry28/2m)], root_index=1) comes out
//! rule-first for the same reason — no per-role special-casing anywhere.
//!
//! ## Remaining uncovered constructs (honest, non-blocking)
//!
//! One item: `mrule30#allo0`, a `Modify` (process morph) action — plan §2's "not compilable as
//! strings", P6 (replace-rule compilation) material. No engine analysis in this corpus sample uses
//! it (recall is 100% with it uncovered); the infix items are GONE from `uncovered` (asserted in
//! test (a)). Tier stays `Partial { uncovered: 1 }` — honest, and no longer a routing decision
//! (there is no fallback tier to route to).
//!
//! ## Test-timing policy (revised 2026-07-17)
//! The default local `cargo test --workspace --release` run must stay under ~60s and must not
//! depend on the gitignored real-language corpus fixtures (`samples/data/*`) at all. Every test in
//! this file loads `samples/data/amharic-hc.xml`, so all four are unconditionally
//! `#[ignore = "..."]`d (replacing the old `cfg_attr(debug_assertions, ...)` debug-only ignore),
//! each with a self-skip guard so `--include-ignored` runs stay green where the fixture is absent
//! (CI). Run the full set locally with
//! `cargo test -p hc-foma --release --test f3_amharic_gate -- --include-ignored`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hc_foma::analyzer::FomaProposer;
use hc_foma::composite::FomaAnalyzer;
use hc_foma::emit;
use hc_foma::peel::ReduplicationPeeler;
use hc_grammar::model::Grammar;
use hc_parse::{Morpher, ParseOptions};

/// Same word cap as the P1c stage (first 100 corpus words; `amharic-words.txt` has 673 lines).
const WORD_CAP: usize = 100;

/// Per-word engine-oracle timeout. Words that time out with ZERO analyses are skipped (can't tell
/// what the engine would eventually find) and reported; words that time out with PARTIAL analyses
/// count for recall (a partial engine result is still real — it can only shrink the denominator's
/// completeness, never make it wrong) but are EXCLUDED from the end-to-end multiset parity test
/// (the foma path's confirm is uncapped, so it can legitimately find analyses the timed-out
/// full-search pass didn't get to — set inequality there is a timeout artifact, not a parity bug).
const ENGINE_TIMEOUT: Duration = Duration::from_secs(10);

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

/// Self-skip guard: gitignored real-corpus fixtures aren't present in a fresh clone or CI.
fn have(name: &str) -> bool {
    sample_path(name).exists()
}

fn load_amharic() -> Grammar {
    let path = sample_path("amharic-hc.xml");
    let xml = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load amharic-hc.xml: {e}"))
}

fn corpus_words() -> Vec<String> {
    let words_path = sample_path("amharic-words.txt");
    let words_text = std::fs::read_to_string(&words_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", words_path.display()));
    let words: Vec<String> = words_text
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .take(WORD_CAP)
        .map(str::to_string)
        .collect();
    assert_eq!(words.len(), WORD_CAP, "corpus has at least {WORD_CAP} words");
    words
}

fn morpheme_name(g: &Grammar, id: u32) -> String {
    match g.morphemes.get(id as usize) {
        Some(m) => {
            let gloss = m.gloss.as_deref().unwrap_or("-");
            format!("{}({}/{})", id, m.xml_key, gloss)
        }
        None => format!("{id}(?)"),
    }
}

fn seq_names(g: &Grammar, seq: &[u32]) -> String {
    seq.iter()
        .map(|&id| morpheme_name(g, id))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Distinct `(morpheme_ids, root_morpheme_index)` sequences in an engine outcome (recall's unit).
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

/// The FULL multiset (duplicates preserved, sorted for comparison) — the end-to-end parity unit.
fn multiset(structured: &[hc_parse::WordAnalysis]) -> Vec<(Vec<u32>, i32)> {
    let mut m: Vec<(Vec<u32>, i32)> = structured
        .iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect();
    m.sort();
    m
}

fn candidates_cover(candidates: &[hc_foma::tags::Candidate], seq: &[u32], root_idx: i32) -> bool {
    candidates.iter().any(|c| {
        c.root_index == root_idx
            && c.morphemes.len() == seq.len()
            && c.morphemes.iter().zip(seq.iter()).all(|(m, s)| m.0 == *s)
    })
}

// -------------------------------------------------------------------------------------------
// (a) emit + compile: counts, uncovered (the infix items must be GONE), tier, wall time.
//     Soft time expectation < 60s for emit+compile (plan P1d gate item a) — measured and printed;
//     only an outright compile error fails the test on time-unrelated grounds. Measured on this
//     machine (release): emit ~30s (dominated by `preexpand`'s ~305k (stem × rule) synthesize
//     probes — the documented O(roots × rules × depth) scale bridge, P6 successor named in
//     preexpand's module doc), emit+compile ~35-40s.
// -------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn a_amharic_emits_and_compiles() {
    if !have("amharic-hc.xml") {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    }
    let g = load_amharic();

    let t_emit = Instant::now();
    let emitted = emit::emit(&g);
    let emit_elapsed = t_emit.elapsed();

    assert!(
        emitted.report.counts.entries >= 76,
        "expected >= 76 entries, got {}",
        emitted.report.counts.entries
    );
    assert!(
        emitted.report.counts.rules >= 87,
        "expected >= 87 mrules, got {}",
        emitted.report.counts.rules
    );
    assert!(emitted.report.counts.lexc_lines > 0);
    assert!(
        !matches!(emitted.report.tier, emit::FomaTier::Unsupported { .. }),
        "Amharic failed to emit at all — got Unsupported: {:?}",
        emitted.report.tier
    );

    // P1d headline assertions: both composite mechanisms fired, and the Role::Infix rules are
    // GONE from `uncovered` (they are representable now, via rule-application pre-expansion).
    assert!(
        emitted.report.counts.composite_interdigitation_entries > 0,
        "expected interdigitation composites (Amharic has 3 Role::Infix rules matching 36 roots)"
    );
    assert!(
        emitted.report.counts.composite_fusion_entries > 0,
        "expected boundary-fusion composites (Ge'ez coalescence)"
    );
    assert!(
        emitted.report.counts.composite_pairs_probed > 0,
        "composite pair probing must have run"
    );
    let infix_uncovered: Vec<_> = emitted
        .report
        .uncovered
        .iter()
        .filter(|u| u.kind == "infix")
        .collect();
    assert!(
        infix_uncovered.is_empty(),
        "P1d requires the infix items GONE from uncovered; still present: {infix_uncovered:?}"
    );

    println!(
        "amharic emit: {emit_elapsed:?}; lexc lines: {}; lexc bytes: {}; tier: {:?}; uncovered: {}",
        emitted.report.counts.lexc_lines,
        emitted.lexc_source.len(),
        emitted.report.tier,
        emitted.report.uncovered.len(),
    );
    println!(
        "counts: entries={} rules={} slots={} groups={} allomorphs_emitted={} allomorphs_skipped={}",
        emitted.report.counts.entries,
        emitted.report.counts.rules,
        emitted.report.counts.slots,
        emitted.report.counts.groups,
        emitted.report.counts.allomorphs_emitted,
        emitted.report.counts.allomorphs_skipped,
    );
    println!(
        "composites: pairs_probed={} interdigitation_entries={} fusion_entries={}",
        emitted.report.counts.composite_pairs_probed,
        emitted.report.counts.composite_interdigitation_entries,
        emitted.report.counts.composite_fusion_entries,
    );
    for u in &emitted.report.uncovered {
        println!("  uncovered: [{}] {} — {}", u.kind, u.id, u.reason);
    }

    let t_compile = Instant::now();
    let proposer = FomaProposer::new(&g);
    let compile_elapsed = t_compile.elapsed();
    println!(
        "amharic emit+foma-compile (fresh emit inside FomaProposer::new): {compile_elapsed:?} \
         (soft expectation < 60s; if this regresses far past that, the preexpand chain probing is \
         the first place to profile)"
    );
    proposer.unwrap_or_else(|e| panic!("Amharic lexc failed to foma-compile: {e}"));
}

// -------------------------------------------------------------------------------------------
// (b) RECALL — asserted 100% (plan §0: no fallback tier exists; a miss is a compiler bug).
//     Denominator: first WORD_CAP corpus words, engine-analyzed only, ENGINE_TIMEOUT per word
//     (zero-analysis timeouts skipped and reported). No reduplication exclusion: Amharic has no
//     reduplication rule (asserted below — its five redupMorphType-tagged subrules each reference
//     their Input part once, so nothing classifies Role::Reduplication; same census as
//     hc-rules/src/morph.rs's module doc).
// -------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn b_amharic_recall_first_100_words_is_100_percent() {
    if !have("amharic-hc.xml") {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    }
    let g = load_amharic();
    assert!(
        !ReduplicationPeeler::new(&g).has_redup_rules(),
        "Amharic was verified to have zero Role::Reduplication rules; if this grammar changed, \
         re-derive the denominator exclusion rule (plan P1d gate item b)"
    );
    let mut proposer = FomaProposer::new(&g).expect("Amharic compiles");
    let morpher = Morpher::new(&g, usize::MAX).with_word_timeout(Some(ENGINE_TIMEOUT));
    let opts = ParseOptions::default();

    let words = corpus_words();

    let mut n_total = 0usize;
    let mut n_covered = 0usize;
    let mut misses: Vec<String> = Vec::new();
    let mut n_words_analyzed = 0usize;
    let mut skipped: Vec<String> = Vec::new();
    let mut n_partial_timeout = 0usize;
    let mut engine_time = Duration::ZERO;
    let mut propose_time = Duration::ZERO;
    let mut max_propose = Duration::ZERO;

    for word in &words {
        let t0 = Instant::now();
        let outcome = morpher.parse_word_opts(word, &opts);
        engine_time += t0.elapsed();

        if outcome.structured.is_empty() {
            if outcome.timed_out {
                skipped.push(format!(
                    "{word:?}: engine timed out ({ENGINE_TIMEOUT:?}) with zero analyses"
                ));
            }
            continue;
        }
        if outcome.timed_out {
            n_partial_timeout += 1;
        }
        n_words_analyzed += 1;

        let t1 = Instant::now();
        let candidates = proposer.propose(word);
        let dt = t1.elapsed();
        propose_time += dt;
        max_propose = max_propose.max(dt);

        for (seq, root_idx) in engine_sequences(&outcome) {
            n_total += 1;
            if candidates_cover(&candidates, &seq, root_idx) {
                n_covered += 1;
            } else {
                misses.push(format!(
                    "word {word:?}: engine analysis root_index={root_idx} morphemes=[{}]",
                    seq_names(&g, &seq)
                ));
            }
        }
    }

    for s in &skipped {
        println!("SKIPPED {s}");
    }
    println!(
        "recall: {n_covered}/{n_total} engine analyses covered across {n_words_analyzed} analyzed \
         words (of {WORD_CAP} scanned; {} skipped on zero-analysis timeout; {n_partial_timeout} \
         analyzed words had partial/timed-out engine results)",
        skipped.len()
    );
    println!(
        "engine total: {engine_time:?}; propose total: {propose_time:?}; propose max/word: \
         {max_propose:?}; propose mean/word: {:?}",
        propose_time / (n_words_analyzed.max(1) as u32)
    );
    if !misses.is_empty() {
        println!("--- MISSES ({} of {n_total}) — every one is a P1d bug ---", misses.len());
        for m in &misses {
            println!("MISS {m}");
        }
    }
    assert!(n_total > 0, "recall gate must exercise at least one engine analysis");
    assert_eq!(
        n_covered, n_total,
        "P1d requires 100% Amharic proposer recall (plan §0: no fallback tier); {} engine \
         analyses not proposed — see MISS lines above",
        n_total - n_covered
    );
}

// -------------------------------------------------------------------------------------------
// (c) END-TO-END: FomaAnalyzer::analyze_word (propose UNION peel -> batched confirm) multiset ==
//     engine parse_word_opts multiset on the same denominator words — stronger than proposer
//     recall (exercises confirm's positional matching + D4 multiplicity recovery through the
//     composite entries). Timed-out words excluded (see ENGINE_TIMEOUT's doc).
// -------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn c_amharic_end_to_end_multiset_parity() {
    if !have("amharic-hc.xml") {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    }
    let g = load_amharic();
    let mut analyzer = FomaAnalyzer::new(&g).expect("Amharic compiles");
    let morpher = Morpher::new(&g, usize::MAX).with_word_timeout(Some(ENGINE_TIMEOUT));
    let opts = ParseOptions::default();

    let words = corpus_words();

    let mut n_words_compared = 0usize;
    let mut n_excluded_timeout = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    let mut analyze_time = Duration::ZERO;
    let mut max_analyze = Duration::ZERO;

    for word in &words {
        let outcome = morpher.parse_word_opts(word, &opts);
        if outcome.timed_out {
            // Partial full-search results cannot be a parity baseline (foma confirm is uncapped).
            n_excluded_timeout += 1;
            continue;
        }
        if outcome.structured.is_empty() {
            continue; // same denominator rule as (b): engine-analyzed words only.
        }
        n_words_compared += 1;

        let t0 = Instant::now();
        let foma_outcome = analyzer.analyze_word(word);
        let dt = t0.elapsed();
        analyze_time += dt;
        max_analyze = max_analyze.max(dt);

        let engine_ms = multiset(&outcome.structured);
        let foma_ms = multiset(&foma_outcome.structured);
        if engine_ms != foma_ms {
            let fmt = |ms: &[(Vec<u32>, i32)]| {
                ms.iter()
                    .map(|(seq, ri)| format!("[{}]@{ri}", seq_names(&g, seq)))
                    .collect::<Vec<_>>()
                    .join(" | ")
            };
            mismatches.push(format!(
                "word {word:?}: engine {} analyses vs foma {} —\n  engine: {}\n  foma:   {}",
                engine_ms.len(),
                foma_ms.len(),
                fmt(&engine_ms),
                fmt(&foma_ms),
            ));
        }
    }

    println!(
        "end-to-end parity: {n_words_compared} words compared ({n_excluded_timeout} excluded on \
         engine timeout); analyze_word total: {analyze_time:?}; max/word: {max_analyze:?}; \
         mean/word: {:?}",
        analyze_time / (n_words_compared.max(1) as u32)
    );
    if !mismatches.is_empty() {
        println!("--- MULTISET MISMATCHES ({}) ---", mismatches.len());
        for m in &mismatches {
            println!("MISMATCH {m}");
        }
    }
    assert!(n_words_compared > 0, "parity gate must compare at least one word");
    assert!(
        mismatches.is_empty(),
        "{} word(s) with foma-path multiset != full-engine multiset (see MISMATCH lines above)",
        mismatches.len()
    );
}

// -------------------------------------------------------------------------------------------
// (d) overgeneration sanity + no panic on nonsense words (valid Ge'ez segments and unsegmentable
//     Latin), through both the raw proposer and the full analyzer.
// -------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn d_nonsense_word_proposes_boundedly_and_never_panics() {
    if !have("amharic-hc.xml") {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    }
    let g = load_amharic();
    let mut proposer = FomaProposer::new(&g).expect("Amharic compiles");
    let t0 = Instant::now();
    let candidates = proposer.propose("ዝጎጠቃኝዬ");
    println!("ዝጎጠቃኝዬ: {} candidates in {:?}", candidates.len(), t0.elapsed());
    assert!(
        candidates.len() <= 20,
        "nonsense word should propose boundedly few candidates, got {}",
        candidates.len()
    );

    let candidates2 = proposer.propose("zzzq");
    assert!(candidates2.is_empty(), "unsegmentable word should propose nothing");

    // Through the full composite (propose -> confirm): nonsense must confirm to zero analyses.
    let mut analyzer = FomaAnalyzer::new(&g).expect("Amharic compiles");
    let outcome = analyzer.analyze_word("ዝጎጠቃኝዬ");
    assert!(
        outcome.structured.is_empty(),
        "nonsense word must not confirm ({} confirmed)",
        outcome.confirmed
    );
    let outcome2 = analyzer.analyze_word("zzzq");
    assert!(outcome2.structured.is_empty());
}
