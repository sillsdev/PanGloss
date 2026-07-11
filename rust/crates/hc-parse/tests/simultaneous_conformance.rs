//! P13 conformance replay for the 4 oracle-verified `RewriteMode::Simultaneous` fixtures under
//! `rust/conformance/rewrite/simultaneous-*/` (`rust/docs/p13-simultaneous-design.md` §5 step 6 /
//! §6), following the same convention as `rewrite_conformance.rs`: load each fixture's `grammar.xml`
//! exactly as authored, parse every word in `words.txt`, and check `Morpher::parse_word(...)
//! .signature()` against the literal signature transcribed from that fixture's oracle-generated
//! `expected.tsv`. Each fixture's own README documents the oracle-generating command and the full
//! derivation of every expected value — read those before touching this file.

use std::path::{Path, PathBuf};

use hc_grammar::load;
use hc_parse::Morpher;

fn fixture_path(name: &str, file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/rewrite")
        .join(name)
        .join(file)
}

fn load_fixture(name: &str) -> hc_grammar::model::Grammar {
    let xml = std::fs::read_to_string(fixture_path(name, "grammar.xml")).expect("read grammar.xml");
    load(&xml).unwrap_or_else(|e| panic!("{name}: grammar failed to load: {e}"))
}

/// `rust/conformance/rewrite/simultaneous-feeding/expected.tsv` — direct port of
/// `RewriteRuleTests.MultipleApplicationRules`, tagged Simultaneous. Proves §1.1's headline
/// algorithmic fact: a rewrite under Simultaneous computes every match's target+environment against
/// one fixed pre-rewrite snapshot, so it can never feed another match within the same application —
/// "gigugu" parses (both `u`s' `HFUVowel Cons` left environments are checked against the ORIGINAL,
/// unrewritten shape), "gigugi" does not (the rule is obligatory wherever its environment holds, so
/// the un-rewritten form itself never survives).
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn simultaneous_feeding_matches_oracle() {
    let g = load_fixture("simultaneous-feeding");
    let m = Morpher::new(&g, usize::MAX);
    let cases = [("gigugu", "|gigugu"), ("gigugi", "-"), ("gigigi", "-")];
    for (word, expected) in cases {
        assert_eq!(m.parse_word(word).signature(), expected, "simultaneous-feeding word {word:?}");
    }
}

/// `rust/conformance/rewrite/simultaneous-feeding-control-iterative/expected.tsv` — the identical
/// rule with `multipleApplicationOrder` omitted (Iterative, C#'s default): the mirror-image oracle
/// run. Iterative's cursor re-matches against the shape AS MUTATED SO FAR, so the first rewrite
/// (which turns the second `u`'s preceding environment from `i` to `u`, no longer `HFUVowel`) BLEEDS
/// the second match — "gigugi" parses, "gigugu" does not. Together with
/// `simultaneous_feeding_matches_oracle` this is the primary, cleanest, highest-confidence pin of
/// the whole Simultaneous-vs-Iterative divergence (a real C# unit test transcribed, not a
/// hand-invented scenario).
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn simultaneous_feeding_control_iterative_matches_oracle() {
    let g = load_fixture("simultaneous-feeding-control-iterative");
    let m = Morpher::new(&g, usize::MAX);
    let cases = [("gigugu", "-"), ("gigugi", "|gigugi"), ("gigigi", "-")];
    for (word, expected) in cases {
        assert_eq!(
            m.parse_word(word).signature(),
            expected,
            "simultaneous-feeding-control-iterative word {word:?}"
        );
    }
}

/// `rust/conformance/rewrite/simultaneous-epenthesis/expected.tsv` — direct port of
/// `RewriteRuleTests.EpenthesisRules` sub-case (1): insert an HFU vowel after any high vowel,
/// tagged Simultaneous, against root 19's real `"b+ubu"` shape (morpheme-boundary-bearing, per
/// `HermitCrabTestBase.cs`).
///
/// **Read this fixture's own README before touching this test.** Its `expected.tsv` deliberately
/// freezes the TRACED/correct signature for `buibui` (`|b+?uibui`, root 19), NOT the live C#
/// oracle's DEFAULT (non-tracing) path's output, which is confirmed buggy (`-`) via three
/// independent checks (§3 of the design doc): the real NUnit test passes non-traced; a from-scratch
/// in-memory reconstruction succeeds non-traced; the SAME loaded grammar object flips from 0 to 1
/// result purely on `TraceManager.IsTracing`. The bug is in C#'s own nogood-memoization cache
/// (`AnalysisScope`, installed only when not tracing), not in this fixture's construction.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn simultaneous_epenthesis_matches_oracle() {
    let g = load_fixture("simultaneous-epenthesis");
    let m = Morpher::new(&g, usize::MAX);
    let cases = [("buibui", "|b+?uibui"), ("bubu", "-"), ("bibu", "-")];
    for (word, expected) in cases {
        assert_eq!(m.parse_word(word).signature(), expected, "simultaneous-epenthesis word {word:?}");
    }
}

/// P13 §7 open question 3 / design doc §3: the C# oracle's confirmed nogood-cache bug (above) is
/// specific to a repeat-until-fixpoint reapplication loop (`SelfOpaquing`) interacting unsoundly
/// with memoization. Rust's analysis path has its OWN, unrelated memo cache
/// (`Morpher::with_memo`/`hc-memo`, rust-optimizations-phase2.md Phases 2/9/10) — the design doc
/// explicitly asks whoever implements the `self_opaquing` repeat-wrapper (§4.4) to test it against
/// this exact fixture shape, since the *shape* of the risk (a repeat-until-fixpoint loop + a memo
/// cache) is precise enough to test for directly even without knowing C#'s exact trigger mechanism.
///
/// **Result: SOUND, with one caveat.** Parsing `"buibui"` through this exact fixture with Rust's
/// memo cache ON (`Morpher::new`'s default, `memo: true`) and OFF (`with_memo(false)`) gives the
/// IDENTICAL signature either way (`|b+?uibui`, root 19). CAVEAT (confirmed via temporary
/// instrumentation, then reverted): on this fixture the `self_opaquing` `while` loop around
/// `ana_epenthesis` in both `analyze` and `analyze_cached` (`hc-rules/src/rewrite.rs`) runs its body
/// exactly ONCE for `"buibui"` under BOTH memo settings — it does not actually reapply to a
/// fixpoint here. So this test is solid evidence of memo-cache/self-opaquing-wrapper consistency in
/// general on this shape, but it is NOT evidence that the loop×memo interaction specifically (a
/// wrapper that repeats ≥2 times, with memoization active partway through) is sound — no fixture in
/// this pass drives the loop past one iteration. That narrower claim remains untested and is a
/// reasonable follow-on if a grammar requiring ≥2 self-opaquing iterations is ever found or built.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn simultaneous_epenthesis_memo_cache_soundness_against_the_confirmed_csharp_bug_shape() {
    let g = load_fixture("simultaneous-epenthesis");
    let memo_on = Morpher::new(&g, usize::MAX);
    let memo_off = Morpher::new(&g, usize::MAX).with_memo(false);
    let on_sig = memo_on.parse_word("buibui").signature();
    let off_sig = memo_off.parse_word("buibui").signature();
    assert_eq!(
        on_sig, off_sig,
        "Rust's memo cache must not change the answer on this self-opaquing-epenthesis shape \
         (this is exactly the shape that trips C#'s own nogood-cache bug, §3/§7 open question 3)"
    );
    assert_eq!(on_sig, "|b+?uibui", "both memo settings must agree with the traced/correct oracle value");
}

/// `rust/conformance/rewrite/simultaneous-epenthesis-cascade/` — a hand-designed (not C#-test-
/// derived) rule whose own epenthesized output re-satisfies its own trigger environment, run
/// **Iterative** (no `multipleApplicationOrder` attribute). Under the real C# oracle this crashes
/// with an uncaught `InfiniteLoopException` (the fixture's own `expected.tsv` is a truncated file
/// containing only the `STARTED` sentinel for word 0 — the batch process died before writing a
/// result row; that truncation IS the ground truth here, not a defect to "fix" into a normal row).
///
/// **Deliberate, documented scope cut (design doc §2.3/§7 open question 2), not a silent gap:**
/// today's `syn_epenthesis` collects every epenthesis site against ONE pristine snapshot before
/// applying any of them (§4.1 — this is also exactly why it's correct, as-is, for `Simultaneous`
/// mode), so it has no per-call rescan loop to cascade through in the first place; it cannot
/// reproduce C#'s crash-via-runaway-self-feeding-Iterative-cursor behavior. No reference grammar
/// (Indonesian/Amharic/Sena) has a self-referential Iterative epenthesis rule, so this is not a
/// correctness gap on any real corpus — it is a pre-existing, narrower-than-full-fidelity property
/// of `syn_epenthesis` that this fixture is the first to distinguish, surfaced by (not introduced
/// by) the P13 pass. Decision recorded here per the design doc's own ask: accepted as a permanent
/// scope cut for this pass (a faithful iteration-cap-to-raised-error rewrite of `syn_epenthesis`'s
/// site-collection loop would be a real, separate, follow-on task) — Rust's actual behavior (no
/// crash, no hang, no parse) is asserted directly rather than silently left unverified.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn simultaneous_epenthesis_cascade_documented_scope_cut() {
    let g = load_fixture("simultaneous-epenthesis-cascade");
    let m = Morpher::new(&g, usize::MAX);
    assert_eq!(
        m.parse_word("bubu").signature(),
        "-",
        "Rust's syn_epenthesis cannot cascade (one snapshot, collect-then-apply) and so cannot \
         reproduce the C# oracle's InfiniteLoopException crash here -- a deliberate, documented \
         scope cut, not a silent gap (see this test's doc comment)"
    );
}
