//! Real-grammar regression guard: Indonesian's 3 actual reduplication subrules must keep resynthesizing their gold-matching surfaces under `classify_redup`'s fix; the hand-built mechanism gate lives in `pg-rules/tests/redup_and_free_fluctuation_gate.rs`, this is the belt-and-suspenders "did the real grammar regress" check. Self-skips (like `batch_determinism.rs`) when the untracked corpus is absent, and stays `#[ignore]`d unconditionally.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn reduplicated_words_keep_their_gold_signature() {
    let Some(grammar_path) = sample_path("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let xml = std::fs::read_to_string(&grammar_path).expect("read grammar");
    let grammar = load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}"));
    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    // (word, expected signature) — expected values are the `parse-opt` golden's own rows.
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
