use super::*;
use pg_featstruct::{FeatId, FeatureStruct, FeatureStructBuilder, FeatureValue, SymbolBits};
use pg_shape::ShapeBuilder;

const FA: FeatId = FeatId(0);

fn mask_none(_: FeatId) -> u64 {
    0
}

fn fs_with(bits: u64) -> FeatureStruct {
    let mut b = FeatureStructBuilder::new();
    b.add(FA, FeatureValue::Symbolic(SymbolBits(bits)));
    b.build()
}

fn bare_word() -> Word {
    Word::new(ShapeBuilder::new().finish(), StratumId(0))
}

#[test]
fn a_byte_identical_alternative_is_pruned() {
    let mut words = vec![bare_word()];
    let mut alt_keys: HashMap<usize, FxHashSet<AltKey>> = HashMap::default();
    let dup = bare_word();
    let key = dup.dedup_key();

    let kept = dedup_alternative(&mut words, &mut alt_keys, 0, &key, dup, &mask_none);

    assert!(
        kept.is_none(),
        "a duplicate of the canonical must be dropped"
    );
    assert!(words[0].alternatives.is_empty());
}

#[test]
fn same_word_key_but_different_syn_fs_is_kept() {
    // `WordKey` excludes `syn_fs`, so a genuinely different value must still survive.
    let mut words = vec![bare_word()];
    let mut alt_keys: HashMap<usize, FxHashSet<AltKey>> = HashMap::default();
    let mut alt = bare_word();
    alt.syn_fs = fs_with(0b01);
    let key = alt.dedup_key();

    let kept = dedup_alternative(&mut words, &mut alt_keys, 0, &key, alt, &mask_none);

    assert!(kept.is_some(), "a differing syn_fs must not be pruned");
}

#[test]
fn a_second_identical_alternative_is_pruned_after_the_first_is_kept() {
    let mut words = vec![bare_word()];
    let mut alt_keys: HashMap<usize, FxHashSet<AltKey>> = HashMap::default();
    let mut alt = bare_word();
    alt.obligatory.push(FeatId(1));
    let key = alt.dedup_key();

    let first = dedup_alternative(&mut words, &mut alt_keys, 0, &key, alt.clone(), &mask_none);
    assert!(first.is_some());
    if let Some(w) = first {
        words[0].alternatives.push(Rc::new(w));
    }
    let second = dedup_alternative(&mut words, &mut alt_keys, 0, &key, alt, &mask_none);

    assert!(
        second.is_none(),
        "an identical second alternative must be pruned"
    );
}
