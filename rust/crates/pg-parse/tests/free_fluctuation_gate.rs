//! Corpus-gated regression: with `samples/data/sena-hc.xml` present, Sena's word "ana" must recover all 4 sub-analyses via `PatternBridge::id_lane`'s `StrRep` identity dimension; self-skips (like `batch_determinism.rs`) when the untracked corpus is absent, and stays `#[ignore]`d unconditionally so the default local run never depends on it.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn ana_recovers_free_fluctuating_analyses() {
    let Some(grammar_path) = sample_path("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let xml = std::fs::read_to_string(&grammar_path).expect("read grammar");
    let grammar = load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}"));
    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    let got = morpher.parse_word("ana").signature();
    assert_eq!(
        got,
        "+++|a+?[(^0)(*0)(&0)∅]?+?[mn]+?a;++|a+?[mn]+?a;+|[(^0)(*0)(&0)∅]?+?a[mn]a;+|[(^0)(*0)(&0)∅]?+?a[mn]a",
        "P10 regressed: expected all 4 sub-analyses for \"ana\" (= golden/master row), got {got:?}"
    );
}
