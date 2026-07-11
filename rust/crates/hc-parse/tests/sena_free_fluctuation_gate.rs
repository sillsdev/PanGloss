//! R3 real-grammar regression/gain guard (plan §13.1.1 / §13.2 step 10): Sena's word "ana" is the
//! one non-capped, deterministic word (out of a Sena first-100 corpus run, uncapped here since it
//! completes in well under a second) whose analysis set actually changes once the disjunctive-
//! allomorph break is gated on `FreeFluctuatesWith` instead of firing unconditionally
//! (`SynthesisAffixProcessRule.cs:235-242`).
//!
//! Before this fix (`3c36cbd3` baseline, directly re-measured this session): `ana` analyzes to
//! zero results (`-`). After: 3 of the 4 sub-analyses in the `parse-opt` golden
//! (`rust/parity-out/golden/parse-opt/sena-fast.tsv`, `ana` row) are recovered, and every one of
//! them is a literal substring of gold's own answer (no over-generation) — real evidence the
//! mechanism moves in the correct direction, even though the 4th sub-analysis
//! (`+++|a+?[(^0)(*0)(&0)∅]?+?[mn]+?a`) needs a different, unrelated gap to complete the match.
//!
//! **P10 update:** that "different, unrelated gap" is closed — it was the missing `StrRep`
//! identity dimension (`PatternBridge::id_lane`'s doc tells the whole story): on Sena's
//! zero-phonological-feature grammar every `SegmentNaturalClass` and every concrete char-def
//! constraint in a morphological LHS degenerated to "matches any segment", so during
//! synthesis-confirm an earlier, spuriously-matching subrule (e.g. `mu-2`'s `mw+`, which requires
//! a stem-initial `[V-back]`, `msubrule117`) fired and the disjunctive break stopped the walk
//! before the null (`^0+`) subrule was ever tried. With the id lane, `ana` recovers all **4**
//! sub-analyses, byte-identical to `golden/master/sena-full.tsv`'s `ana` row.
//! Self-skips like the existing convention (`batch_determinism.rs`) when the untracked Sena corpus
//! isn't present on disk.

use std::path::{Path, PathBuf};

use hc_grammar::load;
use hc_parse::Morpher;

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

#[test]
fn sena_ana_recovers_free_fluctuating_analyses() {
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
