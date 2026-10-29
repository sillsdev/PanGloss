//! Every conformance fixture replayed in shadow mode: a `violated` or `completed_under_fired` count means the look-ahead would have pruned a real parse.

use pg_conformance_fixtures::{discover, replay_against_oracle};
use pg_parse::Morpher;
use pg_rules::required_slots::{take_shadow, RequiredSlotLookahead};

#[test]
fn shadow_replay_of_every_conformance_fixture_has_no_violation() {
    let fixtures = discover();
    assert!(!fixtures.is_empty(), "no conformance fixtures discovered");
    take_shadow();
    let (mut runs, mut mismatches, mut fired) = (0usize, 0usize, 0u64);
    for f in &fixtures {
        let words_yaml = f.load_words_yaml();
        if words_yaml.skip_in_generic_replay().is_some() {
            continue;
        }
        let variants = words_yaml
            .variants
            .iter()
            .map(|v| words_yaml.for_variant(v))
            .collect::<Vec<_>>();
        for words in std::iter::once(&words_yaml).chain(variants.iter()) {
            let grammar = f.load_grammar_with_settings(&words.morpher);
            let morpher = Morpher::new(&grammar, usize::MAX)
                .with_required_slot_lookahead(RequiredSlotLookahead::Shadow);
            let replay = replay_against_oracle(words, &morpher);
            let shadow = take_shadow();
            assert_eq!(
                (shadow.violated, shadow.completed_under_fired),
                (0, 0),
                "{}: the look-ahead would have pruned a parse: {shadow:?}",
                f.label()
            );
            fired += shadow.fired;
            runs += 1;
            mismatches += replay.mismatches.len();
        }
    }
    assert!(runs > 100, "replayed only {runs} runs");
    assert!(fired > 0, "no check fired, so the replay proved nothing");
    eprintln!(
        "shadow replay: {runs} runs, {fired} fired, 0 violated, {mismatches} oracle mismatches"
    );
}
