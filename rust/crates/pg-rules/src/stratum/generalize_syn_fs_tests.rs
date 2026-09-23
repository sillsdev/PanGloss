use super::*;
use pg_featstruct::{FeatId, FeatureStruct, FeatureStructBuilder, FeatureValue, SymbolBits};
use pg_shape::ShapeBuilder;

const FA: FeatId = FeatId(0);

fn fs_with(bits: u64) -> FeatureStruct {
    let mut b = FeatureStructBuilder::new();
    b.add(FA, FeatureValue::Symbolic(SymbolBits(bits)));
    b.build()
}

fn bare_word() -> Word {
    Word::new(ShapeBuilder::new().finish(), StratumId(0))
}

fn mask3(_: FeatId) -> u64 {
    0b111
}

#[test]
fn widens_the_canonical_to_the_union_when_the_alternative_differs() {
    let mut canonical = bare_word();
    canonical.syn_fs = fs_with(0b001);
    let mut alternative = bare_word();
    alternative.syn_fs = fs_with(0b010);

    generalize_syn_fs(&mut canonical, &alternative, &mask3);

    assert_eq!(
        canonical.syn_fs,
        union(&fs_with(0b001), &fs_with(0b010), &mask3)
    );
    assert_eq!(canonical.syn_fs, fs_with(0b011));
}

#[test]
fn leaves_the_canonical_unchanged_when_the_alternative_matches() {
    let mut canonical = bare_word();
    canonical.syn_fs = fs_with(0b011);
    let alternative_same = bare_word_with_syn_fs(fs_with(0b011));

    generalize_syn_fs(&mut canonical, &alternative_same, &mask3);

    assert_eq!(
        canonical.syn_fs,
        fs_with(0b011),
        "an equal syn_fs (the only reachable case on the key-hit fold, since an equal \
         AnalysisStateKey already forces equal syn_fs) must be a no-op"
    );
}

fn bare_word_with_syn_fs(fs: FeatureStruct) -> Word {
    let mut w = bare_word();
    w.syn_fs = fs;
    w
}
