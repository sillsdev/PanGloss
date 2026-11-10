use super::tests::{
    copy, insert, load, prefix, rule, signature, spec, suffix, variants, ANY, ANY_ONE, CTX_ANY,
};
use super::tiled::{build_within, Limits};
use super::*;
use pg_parse::{Morpher, ParseOptions};

fn no_guess() -> ParseOptions {
    ParseOptions::default()
}

struct Setup {
    admission: RuleAdmission,
    tiled: Tiled,
}

fn setup(g: &Grammar) -> Setup {
    let admission = build_rule_admission(&FomaOptions::default(), g).expect("admission builds");
    let tiled = build_tiled(&FomaOptions::default(), g, &admission);
    Setup { admission, tiled }
}

fn has_analysis(morpher: &Morpher, word: &str) -> bool {
    let outcome = morpher.parse_word_selected(word, &no_guess(), None, None);
    assert!(!outcome.capped, "{word} hit the work cap");
    !signature(&outcome).is_empty()
}

/// Every word and its edits: a rejected one must have no analysis. Returns the rejected words and the analysed ones.
fn sweep(
    g: &Grammar,
    tiled: &Tiled,
    admission: &RuleAdmission,
    words: &[&str],
) -> (Vec<String>, Vec<String>) {
    let morpher = Morpher::new(g, 2_000_000);
    let (mut rejected, mut analysed) = (Vec::new(), Vec::new());
    for word in words.iter().flat_map(|w| variants(w)) {
        let admitted = admission.admit(&word);
        let analysis = has_analysis(&morpher, &word);
        if tiled.rejects(&word, &admitted, &no_guess()) {
            assert!(!analysis, "rejected {word:?} but the parser analyses it");
            rejected.push(word);
        } else if analysis {
            analysed.push(word);
        }
    }
    (rejected, analysed)
}

fn affixes() -> Grammar {
    let rules = vec![prefix(0, "ka"), suffix(1, "ta")];
    load(&spec("kaqtn", rules, &["qt", "tq"]))
}

#[test]
fn a_word_the_parser_analyses_is_accepted() {
    let g = affixes();
    let Setup { admission, tiled } = setup(&g);
    assert!(tiled.active(), "{:?}", tiled.inactive_reason());
    assert_eq!(tiled.inactive_reason(), None);
    let morpher = Morpher::new(&g, 2_000_000);
    let words = [
        "qt", "kaqt", "qtta", "kaqtta", "tq", "katq", "tqta", "katqta",
    ];
    for word in words {
        assert!(has_analysis(&morpher, word), "{word}");
        assert!(tiled.accepts(word), "{word}");
        let admitted = admission.admit(word);
        assert!(!tiled.rejects(word, &admitted, &no_guess()), "{word}");
    }
    let (_, analysed) = sweep(&g, &tiled, &admission, &words);
    assert!(analysed.len() >= words.len(), "{analysed:?}");
}

#[test]
fn a_non_word_is_rejected_and_the_parser_has_no_analysis() {
    let g = affixes();
    let Setup { admission, tiled } = setup(&g);
    let morpher = Morpher::new(&g, 2_000_000);
    for word in ["qq", "kaqq", "tt", "qtkaq", "nn", "ta", "ka", "qtqt"] {
        let admitted = admission.admit(word);
        assert!(
            tiled.rejects(word, &admitted, &no_guess()),
            "{word} is not rejected"
        );
        assert!(!has_analysis(&morpher, word), "{word}");
    }
    let (rejected, _) = sweep(&g, &tiled, &admission, &["kaqtta", "qt", "katq"]);
    assert!(rejected.len() > 10, "{rejected:?}");
}

/// A prefix, and an infix with two segments before the literal and two unbounded parts, which the model does not express.
fn with_partial_copy() -> Grammar {
    let infix = rule(
        1,
        &[ANY, CTX_ANY, ANY],
        &[copy(0), copy(1), copy(1), insert("ta"), copy(2)],
    );
    load(&spec("kuqta", vec![prefix(0, "kuu"), infix], &["q"]))
}

#[test]
fn a_rule_outside_the_model_that_is_admitted_for_the_word_bars_the_rejection() {
    let g = with_partial_copy();
    let Setup { admission, tiled } = setup(&g);
    assert!(tiled.active(), "{:?}", tiled.inactive_reason());
    assert_eq!(tiled.evidence().tiles.not_tileable, 1);
    assert!(!admission.free()[1], "the infix is admitted by its literal");
    let morpher = Morpher::new(&g, 2_000_000);
    let word = "qqta";
    assert!(has_analysis(&morpher, word), "{word}");
    let admitted = admission.admit(word);
    assert!(admitted[1]);
    assert!(!tiled.accepts(word), "the tiling cannot spell the infix");
    assert!(!tiled.applies(&admitted, &no_guess()));
    assert!(!tiled.rejects(word, &admitted, &no_guess()));
    let bare = "kuu";
    let admitted = admission.admit(bare);
    assert!(tiled.applies(&admitted, &no_guess()));
    assert!(tiled.rejects(bare, &admitted, &no_guess()));
    let (rejected, analysed) = sweep(&g, &tiled, &admission, &["qqta", "kuuq", "q"]);
    assert!(analysed.contains(&"qqta".to_owned()), "{analysed:?}");
    assert!(!rejected.is_empty());
}

#[test]
fn a_rule_outside_the_model_admitted_for_every_word_switches_the_test_off() {
    let modify = rule(
        1,
        &[CTX_ANY],
        &[r#"<ModifyFromInput index="p0"><SimpleContext naturalClass="ncAny" /></ModifyFromInput>"#.to_owned()],
    );
    let g = load(&spec("kaqt", vec![prefix(0, "ka"), modify], &["qt"]));
    let Setup { admission, tiled } = setup(&g);
    assert!(admission.free()[1]);
    assert!(!tiled.active());
    let reason = tiled.inactive_reason().expect("a reason");
    assert!(reason.contains("admitted for every word"), "{reason}");
    assert_eq!(tiled.evidence().tiles.not_tileable_free.len(), 1);
    for word in ["qq", "tt", "kaqt", "zz"] {
        let admitted = admission.admit(word);
        assert!(tiled.accepts(word));
        assert!(!tiled.rejects(word, &admitted, &no_guess()), "{word}");
    }
}

#[test]
fn full_reduplication_is_modelled_as_a_repeated_stem() {
    let twice = rule(0, &[ANY_ONE], &[copy(0), copy(0)]);
    let g = load(&spec("qkn", vec![twice, suffix(1, "n")], &["qk"]));
    let Setup { admission, tiled } = setup(&g);
    assert!(admission.free()[0], "admission cannot bound a copy");
    assert!(!admission.free()[1]);
    assert!(tiled.active(), "{:?}", tiled.inactive_reason());
    assert_eq!(tiled.evidence().tiles.repeating, 1);
    let morpher = Morpher::new(&g, 2_000_000);
    assert!(has_analysis(&morpher, "qkqk"));
    assert!(tiled.accepts("qkqk"));
    assert!(tiled.accepts("qk"));
    let admitted = admission.admit("qkq");
    assert!(tiled.rejects("qkq", &admitted, &no_guess()));
    assert!(!has_analysis(&morpher, "qkq"));
    let (rejected, analysed) = sweep(&g, &tiled, &admission, &["qk", "qkqk"]);
    assert!(analysed.contains(&"qkqk".to_owned()), "{analysed:?}");
    assert!(!rejected.is_empty());
}

/// A prefix and a rule that drops the first segment of the stem: one unbounded part copied once behind a one-segment window.
const WINDOWED_WORDS: [&str; 7] = ["qk", "k", "taaqk", "taak", "aaqk", "qknu", "knu"];

fn with_first_segment_dropped() -> Grammar {
    let drop = rule(1, &[CTX_ANY, ANY_ONE], &[copy(1)]);
    let rules = vec![prefix(0, "taa"), drop, suffix(2, "nu")];
    load(&spec("taqknu", rules, &["qk"]))
}

#[test]
fn a_windowed_edit_keeps_the_test_active_and_accepts_what_it_produces() {
    let g = with_first_segment_dropped();
    let Setup { admission, tiled } = setup(&g);
    assert!(tiled.active(), "{:?}", tiled.inactive_reason());
    let tiles = &tiled.evidence().tiles;
    assert_eq!((tiles.windowed, tiles.not_tileable), (1, 0));
    assert!(tiles.edit_converged && !tiles.edit_widened);
    let morpher = Morpher::new(&g, 2_000_000);
    for word in WINDOWED_WORDS {
        assert!(has_analysis(&morpher, word), "{word}");
        assert!(tiled.accepts(word), "{word}");
    }
    let (rejected, analysed) = sweep(&g, &tiled, &admission, &WINDOWED_WORDS);
    assert!(analysed.len() >= WINDOWED_WORDS.len(), "{analysed:?}");
    assert!(!rejected.is_empty());
    let admitted = admission.admit("nn");
    assert!(tiled.rejects("nn", &admitted, &no_guess()));
}

#[test]
fn zones_that_do_not_settle_in_the_rounds_widen_and_stay_sound() {
    let g = with_first_segment_dropped();
    let Setup { admission, .. } = setup(&g);
    let limits = Limits {
        rounds: 0,
        ..Limits::default()
    };
    let tiled = build_within(&FomaOptions::default(), &g, &admission, limits);
    assert!(tiled.evidence().tiles.edit_widened);
    let (_, analysed) = sweep(&g, &tiled, &admission, &WINDOWED_WORDS);
    assert!(analysed.len() >= WINDOWED_WORDS.len(), "{analysed:?}");
}

#[test]
fn a_size_bound_leaves_the_test_inactive_instead_of_failing() {
    let g = affixes();
    let Setup { admission, .. } = setup(&g);
    let build_bound = Limits {
        stop: 1,
        ..Limits::default()
    };
    let table_bound = Limits {
        table_states: 1,
        ..Limits::default()
    };
    for (limits, cause) in [
        (build_bound, "build refused: containment"),
        (table_bound, "table needs more than"),
    ] {
        let tiled = build_within(&FomaOptions::default(), &g, &admission, limits);
        assert!(!tiled.active());
        let reason = tiled.inactive_reason().expect("a reason");
        assert!(reason.starts_with(cause), "{reason}");
        let admitted = admission.admit("qq");
        assert!(tiled.accepts("qq"));
        assert!(!tiled.rejects("qq", &admitted, &no_guess()));
    }
}

#[test]
fn a_parse_that_guesses_roots_or_a_foreign_rule_set_is_never_rejected() {
    let g = affixes();
    let Setup { admission, tiled } = setup(&g);
    let admitted = admission.admit("qq");
    assert!(tiled.rejects("qq", &admitted, &no_guess()));
    let guessing = no_guess().with_guess_root(true);
    assert!(!tiled.applies(&admitted, &guessing));
    assert!(!tiled.rejects("qq", &admitted, &guessing));
    let guess_only = no_guess().with_guess_only(true);
    assert!(!tiled.rejects("qq", &admitted, &guess_only));
    assert!(!tiled.applies(&[], &no_guess()));
    assert!(!tiled.rejects("qq", &[true; 7], &no_guess()));
}

#[test]
fn an_admission_built_for_another_grammar_leaves_the_test_inactive() {
    let g = affixes();
    let other = load(&spec(
        "kaqtnu",
        vec![prefix(0, "ka"), suffix(1, "ta"), suffix(2, "nu")],
        &["qt"],
    ));
    let Setup { admission, .. } = setup(&other);
    let tiled = build_tiled(&FomaOptions::default(), &g, &admission);
    assert!(!tiled.active());
    let reason = tiled.inactive_reason().expect("a reason");
    assert!(reason.contains("another grammar"), "{reason}");
}

/// Every word of every synthetic conformance fixture: a word the acceptor rejects has no analysis.
#[test]
fn no_conformance_word_with_an_analysis_is_rejected() {
    let fixtures = pg_conformance_fixtures::discover();
    let (mut built, mut active, mut words, mut rejected) = (0, 0, 0, 0);
    for fixture in &fixtures {
        let listed = fixture.load_words_yaml();
        if listed.expect_crash {
            continue;
        }
        let more = listed.variants.iter().map(|v| (&v.morpher, &v.words));
        for (settings, entries) in std::iter::once((&listed.morpher, &listed.words)).chain(more) {
            let Ok(mut g) = pg_grammar::load(&fixture.load_grammar_xml()) else {
                continue;
            };
            settings.apply_to(&mut g);
            let Ok(admission) = build_rule_admission(&FomaOptions::default(), &g) else {
                continue;
            };
            let tiled = build_tiled(&FomaOptions::default(), &g, &admission);
            built += 1;
            active += usize::from(tiled.active());
            let morpher = Morpher::new(&g, 2_000_000);
            for entry in entries {
                let word = entry.word.as_str();
                words += 1;
                let admitted = admission.admit(word);
                if !tiled.rejects(word, &admitted, &no_guess()) {
                    continue;
                }
                rejected += 1;
                let plain = morpher.parse_word_selected(word, &no_guess(), None, None);
                let label = fixture.label();
                assert!(signature(&plain).is_empty(), "{label}: rejected {word:?}");
            }
        }
    }
    assert!(built >= 40 && active >= 20, "built {built} active {active}");
    assert!(
        words > 300 && rejected > 0,
        "words {words} rejected {rejected}"
    );
}
