use super::*;
use pg_shape::ShapeBuilder;

/// `size_of::<Word>()` once the rarely used parts sit behind `Extras`.
const MAX_WORD_BYTES: usize = 168;

fn word() -> Word {
    Word::new(ShapeBuilder::new().finish(), StratumId(0))
}

#[test]
fn word_does_not_regrow() {
    assert!(
        std::mem::size_of::<Word>() <= MAX_WORD_BYTES,
        "Word is {} bytes",
        std::mem::size_of::<Word>()
    );
}

#[test]
fn an_empty_box_and_no_box_are_indistinguishable() {
    let plain = word();
    let mut boxed = word();
    boxed.non_heads_mut();
    boxed.applied_realizational_rules_mut();
    boxed.applied_phonological_rules_mut();
    boxed.set_root_runtime_id(None);
    assert_eq!(format!("{plain:?}"), format!("{boxed:?}"));
    assert_eq!(plain.dedup_key(), boxed.dedup_key());
    assert!(plain.dedup_key().matches(&boxed));
    assert!(boxed.dedup_key().matches(&plain));
    assert_eq!(
        crate::word::estimate_word_bytes(&plain),
        crate::word::estimate_word_bytes(&boxed)
    );
}

#[test]
fn clones_copy_every_part() {
    let mut w = word();
    w.non_heads_mut().push(word());
    w.applied_realizational_rules_mut().push(MorphemeId(3));
    w.applied_phonological_rules_mut()
        .push((StratumId(1), PRuleId(2)));
    w.set_root_runtime_id(Some("r".to_owned()));
    let mut copy = w.clone();
    assert_eq!(format!("{w:?}"), format!("{copy:?}"));
    copy.non_heads_mut().clear();
    assert_eq!(w.non_heads().len(), 1);
    assert_eq!(w.root_runtime_id(), Some("r"));
    assert_ne!(w.dedup_key(), copy.dedup_key());
}
