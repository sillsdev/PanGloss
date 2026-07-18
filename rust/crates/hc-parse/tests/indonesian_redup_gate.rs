//! Tier-2 #8 real-grammar regression guard (plan §13.1.1 / §13.2 step 10): Indonesian's 3 actual
//! reduplication subrules (`msubrule5`/`mrule7` "-Cont", `msubrule11`/`mrule13` "-Pl",
//! `msubrule13`/`mrule15` "REDUP-meN") must keep resynthesizing their gold-matching surfaces once
//! `classify_redup`'s morph-attribution fix lands. `hc-rules/tests/redup_and_free_fluctuation_gate.rs`
//! is the actual mechanism gate (hand-built, order-sensitive); this file is the "did the real
//! grammar regress" belt-and-suspenders check, self-skipping like the existing convention
//! (`batch_determinism.rs`) when the untracked sample corpus isn't present.
//!
//! Full-corpus re-measurement (this session) confirms Indonesian stays 121/121 against the
//! `parse-opt` golden with both Tier-2 #8 and R3 applied — these 3 words in particular are
//! byte-identical to the pre-fix `3c36cbd3` baseline (see the module docs on
//! `redup_and_free_fluctuation_gate.rs` for why: the two `Suffix`-hint real subrules are
//! `order`-invariant under the fix, and the one `Prefix`-hint subrule is never selected by these
//! words' winning analysis chain).
//!
//! Test-timing policy (revised 2026-07-17): the default local `cargo test --workspace --release`
//! run must stay under ~60s and must not depend on this gitignored fixture at all, so this test is
//! unconditionally `#[ignore = "..."]`d; run with `--include-ignored` locally.

use std::path::{Path, PathBuf};

use hc_grammar::load;
use hc_parse::Morpher;

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn indonesian_reduplicated_words_keep_their_gold_signature() {
    let Some(grammar_path) = sample_path("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let xml = std::fs::read_to_string(&grammar_path).expect("read grammar");
    let grammar = load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}"));
    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    // (word, expected signature) — expected values are the `parse-opt` golden's own rows
    // (`rust/parity-out/golden/parse-opt/indonesian.tsv`), reconfirmed against a fresh oracle-free
    // full-corpus run in this session (121/121, byte-identical to the pre-fix baseline for these 3).
    let cases = [
        ("memijit-mijit", "++|mem+?ijit+?-mijit"),
        ("menulis-nulis", "++|men+?ulis+?-nulis"),
        ("menyewa-nyewa", "++|me(ny)+?ewa+?-(ny)ewa"),
    ];
    for (word, expected) in cases {
        let got = morpher.parse_word(word).signature();
        assert_eq!(got, expected, "word {word:?}: signature regressed");
    }
}
