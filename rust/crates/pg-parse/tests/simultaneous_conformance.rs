//! Conformance replay for the 4 oracle-verified `RewriteMode::Simultaneous` fixtures.
//! See `docs/research/pg-parse-simultaneous-conformance-notes.md`.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn fixture_path(name: &str, file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/rewrite")
        .join(name)
        .join(file)
}

fn load_fixture(name: &str) -> pg_grammar::model::Grammar {
    let xml = std::fs::read_to_string(fixture_path(name, "grammar.xml")).expect("read grammar.xml");
    load(&xml).unwrap_or_else(|e| panic!("{name}: grammar failed to load: {e}"))
}

/// Self-skip guard: `rust/conformance/` isn't a submodule yet, so `--include-ignored` runs must not panic on the missing directory.
fn have_fixture(name: &str) -> bool {
    fixture_path(name, "grammar.xml").exists()
}

/// Direct port of `RewriteRuleTests.MultipleApplicationRules`, tagged Simultaneous.
/// See `docs/research/pg-parse-simultaneous-conformance-notes.md`.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn simultaneous_feeding_matches_oracle() {
    if !have_fixture("simultaneous-feeding") {
        eprintln!("skipping: rust/conformance/rewrite/simultaneous-feeding not present on disk");
        return;
    }
    let g = load_fixture("simultaneous-feeding");
    let m = Morpher::new(&g, usize::MAX);
    let cases = [("gigugu", "|gigugu"), ("gigugi", "-"), ("gigigi", "-")];
    for (word, expected) in cases {
        assert_eq!(
            m.parse_word(word).signature(),
            expected,
            "simultaneous-feeding word {word:?}"
        );
    }
}

/// The identical rule with `multipleApplicationOrder` omitted (Iterative, C#'s default): the mirror-image oracle run.
/// See `docs/research/pg-parse-simultaneous-conformance-notes.md`.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn simultaneous_feeding_control_iterative_matches_oracle() {
    if !have_fixture("simultaneous-feeding-control-iterative") {
        eprintln!(
            "skipping: rust/conformance/rewrite/simultaneous-feeding-control-iterative not present on disk"
        );
        return;
    }
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

/// Direct port of `RewriteRuleTests.EpenthesisRules` sub-case (1), against a real morpheme-boundary-bearing root shape.
/// See `docs/research/pg-parse-simultaneous-conformance-notes.md`.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn simultaneous_epenthesis_matches_oracle() {
    if !have_fixture("simultaneous-epenthesis") {
        eprintln!("skipping: rust/conformance/rewrite/simultaneous-epenthesis not present on disk");
        return;
    }
    let g = load_fixture("simultaneous-epenthesis");
    let m = Morpher::new(&g, usize::MAX);
    let cases = [("buibui", "|b+?uibui"), ("bubu", "-"), ("bibu", "-")];
    for (word, expected) in cases {
        assert_eq!(
            m.parse_word(word).signature(),
            expected,
            "simultaneous-epenthesis word {word:?}"
        );
    }
}

/// Tests Rust's memo cache against the shape of C#'s confirmed nogood-cache bug. Result: sound, with one caveat.
/// See `docs/research/pg-parse-simultaneous-conformance-notes.md`.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn simultaneous_epenthesis_memo_cache_soundness_against_the_confirmed_csharp_bug_shape() {
    if !have_fixture("simultaneous-epenthesis") {
        eprintln!("skipping: rust/conformance/rewrite/simultaneous-epenthesis not present on disk");
        return;
    }
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
    assert_eq!(
        on_sig, "|b+?uibui",
        "both memo settings must agree with the traced/correct oracle value"
    );
}

/// A hand-designed rule whose epenthesized output re-satisfies its own trigger environment; the C# oracle crashes here, and this is a documented scope cut, not a silent gap.
/// See `docs/research/pg-parse-simultaneous-conformance-notes.md`.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn simultaneous_epenthesis_cascade_documented_scope_cut() {
    if !have_fixture("simultaneous-epenthesis-cascade") {
        eprintln!("skipping: rust/conformance/rewrite/simultaneous-epenthesis-cascade not present on disk");
        return;
    }
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
