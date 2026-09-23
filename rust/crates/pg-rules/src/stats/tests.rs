use super::{self_time_supported, self_time_supported_in_direction, Direction, ObjectKind};

#[test]
fn self_time_support_matches_every_instrumented_kind() {
    let support = [
        (ObjectKind::MorphRule, true),
        (ObjectKind::PhonRule, true),
        (ObjectKind::LexEntry, true),
        (ObjectKind::RootIndex, true),
        (ObjectKind::Guesser, true),
        (ObjectKind::Overlay, true),
    ];
    for (kind, expected) in support {
        assert_eq!(self_time_supported(kind), expected, "{kind:?}");
    }
    assert!(!self_time_supported_in_direction(
        ObjectKind::LexEntry,
        Direction::Synthesis
    ));
    assert!(self_time_supported_in_direction(
        ObjectKind::MorphRule,
        Direction::Synthesis
    ));
}
