use super::*;

#[test]
fn round_trips_empty_word() {
    let outcome = ParseOutcome {
        analyses: Vec::new(),
        structured: Vec::new(),
        capped: false,
        invalid_shape: false,
        steps: 0,
        timed_out: false,
        guessed: false,
        candidates_generated: 0,
    };
    let bytes = encode_single(&outcome);
    let decoded = decode(&bytes).unwrap();
    assert_eq!(decoded.len(), 1);
    assert!(!decoded[0].invalid_shape);
    assert!(decoded[0].analyses.is_empty());
}

#[test]
fn round_trips_invalid_shape() {
    let outcome = ParseOutcome {
        analyses: Vec::new(),
        structured: Vec::new(),
        capped: false,
        invalid_shape: true,
        steps: 0,
        timed_out: false,
        guessed: false,
        candidates_generated: 0,
    };
    let decoded = decode(&encode_single(&outcome)).unwrap();
    assert!(decoded[0].invalid_shape);
}

#[test]
fn round_trips_analyses_in_canonical_order() {
    let outcome = ParseOutcome {
        analyses: vec![("b+c".into(), "surf".into()), ("a+c".into(), "surf".into())],
        structured: vec![
            WordAnalysis {
                morpheme_ids: vec![2, 3],
                morph_occurrences: Vec::new(),
                root_morpheme_index: 0,
                pos_id: Some(5),
                syn_fs: Default::default(),
                mpr: Default::default(),
                guessed: false,
                guessed_string: None,
                provenance: pg_parse::AnalysisProvenance::Grammar,
                supplied_root: None,
                morpheme_roots: vec![None; 2],
            },
            WordAnalysis {
                morpheme_ids: vec![1, 3],
                morph_occurrences: Vec::new(),
                root_morpheme_index: 1,
                pos_id: None,
                syn_fs: Default::default(),
                mpr: Default::default(),
                guessed: false,
                guessed_string: None,
                provenance: pg_parse::AnalysisProvenance::Grammar,
                supplied_root: None,
                morpheme_roots: vec![None; 2],
            },
        ],
        capped: true,
        invalid_shape: false,
        steps: 0,
        timed_out: false,
        guessed: false,
        candidates_generated: 0,
    };
    let decoded = decode(&encode_single(&outcome)).unwrap();
    assert!(decoded[0].capped);
    // "a+c|surf" sorts before "b+c|surf" — the second structured record must come first.
    assert_eq!(
        decoded[0].analyses,
        vec![
            DecodedAnalysis {
                pos_id: None,
                root_morpheme_index: 1,
                morpheme_ids: vec![1, 3]
            },
            DecodedAnalysis {
                pos_id: Some(5),
                root_morpheme_index: 0,
                morpheme_ids: vec![2, 3]
            },
        ]
    );
}

#[test]
fn rejects_bad_magic() {
    assert!(decode(&[0, 0, 0, 0]).is_none());
}

// Encoder-level overclaim guard: these construct a guessed analysis directly (bypassing `pg_lexicon`/`pg-ffi`'s own plumbing) and feed it straight to this module's `MAGIC` encoder, so the test below pins the guard itself, not merely that today's call sites happen to avoid the case.

fn guessed_analysis(surface: &str) -> ((String, String), WordAnalysis) {
    (
        (surface.to_string(), surface.to_string()),
        WordAnalysis {
            morpheme_ids: vec![u32::MAX],
            morph_occurrences: Vec::new(),
            root_morpheme_index: 0,
            pos_id: None,
            syn_fs: Default::default(),
            mpr: Default::default(),
            guessed: true,
            guessed_string: None,
            provenance: pg_parse::AnalysisProvenance::Guessed,
            supplied_root: None,
            morpheme_roots: vec![None; 1],
        },
    )
}

fn confirmed_analysis(surface: &str, morpheme_id: u32) -> ((String, String), WordAnalysis) {
    (
        (surface.to_string(), surface.to_string()),
        WordAnalysis {
            morpheme_ids: vec![morpheme_id],
            morph_occurrences: Vec::new(),
            root_morpheme_index: 0,
            pos_id: None,
            syn_fs: Default::default(),
            mpr: Default::default(),
            guessed: false,
            guessed_string: None,
            provenance: pg_parse::AnalysisProvenance::Grammar,
            supplied_root: None,
            morpheme_roots: vec![None; 1],
        },
    )
}

/// Non-vacuous positive control: a plain confirmed analysis survives the encoder untouched, so the guard added in `write_word` is not filtering everything.
#[test]
fn plain_format_encoder_keeps_a_non_guessed_analysis() {
    let (pair, analysis) = confirmed_analysis("kad", 7);
    let outcome = ParseOutcome {
        analyses: vec![pair],
        structured: vec![analysis],
        capped: false,
        invalid_shape: false,
        steps: 0,
        timed_out: false,
        guessed: false,
        candidates_generated: 0,
    };
    let decoded = decode(&encode_single(&outcome)).unwrap();
    assert_eq!(
        decoded[0].analyses.len(),
        1,
        "the confirmed analysis must survive"
    );
    assert_eq!(decoded[0].analyses[0].morpheme_ids, vec![7]);
}

/// A `ParseOutcome` that is entirely a guessed analysis must decode to zero analyses through the plain `MAGIC` encoder/decoder, since this format cannot express `guessed`.
#[test]
fn plain_format_encoder_refuses_to_emit_a_guessed_analysis_even_when_constructed_directly() {
    let (pair, analysis) = guessed_analysis("gag");
    let outcome = ParseOutcome {
        analyses: vec![pair],
        structured: vec![analysis],
        capped: false,
        invalid_shape: false,
        steps: 0,
        timed_out: false,
        guessed: true,
        candidates_generated: 0,
    };
    let decoded = decode(&encode_single(&outcome)).unwrap();
    assert_eq!(decoded.len(), 1);
    assert!(
        decoded[0].analyses.is_empty(),
        "a guessed analysis must never be encoded through the guessed-less format: {:?}",
        decoded[0].analyses
    );
}

/// Mixed outcome: a guessed row alongside a confirmed row proves the guard is a per-analysis filter, not an all-or-nothing rejection of the whole word.
#[test]
fn plain_format_encoder_filters_only_the_guessed_row_out_of_a_mixed_outcome() {
    let (guessed_pair, guessed) = guessed_analysis("gag");
    let (confirmed_pair, confirmed) = confirmed_analysis("kad", 9);
    let outcome = ParseOutcome {
        analyses: vec![guessed_pair, confirmed_pair],
        structured: vec![guessed, confirmed],
        capped: false,
        invalid_shape: false,
        steps: 0,
        timed_out: false,
        guessed: false,
        candidates_generated: 0,
    };
    let decoded = decode(&encode_single(&outcome)).unwrap();
    assert_eq!(decoded[0].analyses.len(), 1);
    assert_eq!(decoded[0].analyses[0].morpheme_ids, vec![9]);
}

/// `encode_batch` applies the same per-word guard as `encode_single`, pinned separately since `hc_parse_batch` is one of the two entry points this guard exists for.
#[test]
fn batch_plain_format_encoder_also_refuses_a_guessed_analysis() {
    let (pair, analysis) = guessed_analysis("gag");
    let outcome = ParseOutcome {
        analyses: vec![pair],
        structured: vec![analysis],
        capped: false,
        invalid_shape: false,
        steps: 0,
        timed_out: false,
        guessed: true,
        candidates_generated: 0,
    };
    let outcomes = vec![BatchWordOutcome {
        outcome,
        elapsed: std::time::Duration::ZERO,
    }];
    let decoded = decode(&encode_batch(&outcomes)).unwrap();
    assert_eq!(decoded.len(), 1);
    assert!(decoded[0].analyses.is_empty());
}

// -- Guess-opt-in wire format (HC-rust port gap G3) --------------------------------------

#[test]
fn guess_format_round_trips_a_guessed_analysis_and_carries_word_and_analysis_level_bits() {
    let outcome = ParseOutcome {
        analyses: vec![("gag".into(), "gag".into())],
        structured: vec![WordAnalysis {
            morpheme_ids: vec![u32::MAX],
            morph_occurrences: Vec::new(),
            root_morpheme_index: 0,
            pos_id: None,
            syn_fs: Default::default(),
            mpr: Default::default(),
            guessed: true,
            guessed_string: None,
            provenance: pg_parse::AnalysisProvenance::Guessed,
            supplied_root: None,
            morpheme_roots: vec![None; 1],
        }],
        capped: false,
        invalid_shape: false,
        steps: 0,
        timed_out: false,
        guessed: true,
        candidates_generated: 0,
    };
    let decoded = decode_guess(&encode_single_guess(&outcome)).unwrap();
    assert_eq!(decoded.len(), 1);
    assert!(decoded[0].guessed, "word-level guessed bit must round-trip");
    assert_eq!(decoded[0].analyses.len(), 1);
    assert!(
        decoded[0].analyses[0].guessed,
        "per-analysis guessed bit must round-trip"
    );
    assert_eq!(decoded[0].analyses[0].morpheme_ids, vec![u32::MAX]);
}

#[test]
fn guess_format_round_trips_a_non_guessed_analysis_as_false() {
    let outcome = ParseOutcome {
        analyses: vec![("KAD".into(), "kad".into())],
        structured: vec![WordAnalysis {
            morpheme_ids: vec![2],
            morph_occurrences: Vec::new(),
            root_morpheme_index: 0,
            pos_id: None,
            syn_fs: Default::default(),
            mpr: Default::default(),
            guessed: false,
            guessed_string: None,
            provenance: pg_parse::AnalysisProvenance::Grammar,
            supplied_root: None,
            morpheme_roots: vec![None; 1],
        }],
        capped: false,
        invalid_shape: false,
        steps: 0,
        timed_out: false,
        guessed: false,
        candidates_generated: 0,
    };
    let decoded = decode_guess(&encode_single_guess(&outcome)).unwrap();
    assert!(!decoded[0].guessed);
    assert!(!decoded[0].analyses[0].guessed);
}

/// The two magics are never cross-decodable, even though the two formats share a byte-layout prefix shape.
#[test]
fn guess_format_and_plain_format_are_not_cross_decodable() {
    let outcome = ParseOutcome {
        analyses: Vec::new(),
        structured: Vec::new(),
        capped: false,
        invalid_shape: false,
        steps: 0,
        timed_out: false,
        guessed: false,
        candidates_generated: 0,
    };
    assert!(decode_guess(&encode_single(&outcome)).is_none());
    assert!(decode(&encode_single_guess(&outcome)).is_none());
}

#[test]
fn guess_format_rejects_bad_magic() {
    assert!(decode_guess(&[0, 0, 0, 0]).is_none());
}

#[test]
fn guess_format_batch_preserves_original_request_order() {
    let make = |text: &str, guessed: bool| ParseOutcome {
        analyses: vec![(text.into(), text.into())],
        structured: vec![WordAnalysis {
            morpheme_ids: vec![0],
            morph_occurrences: Vec::new(),
            root_morpheme_index: 0,
            pos_id: None,
            syn_fs: Default::default(),
            mpr: Default::default(),
            guessed,
            guessed_string: None,
            provenance: if guessed {
                pg_parse::AnalysisProvenance::Guessed
            } else {
                pg_parse::AnalysisProvenance::Grammar
            },
            supplied_root: None,
            morpheme_roots: vec![None; 1],
        }],
        capped: false,
        invalid_shape: false,
        steps: 0,
        timed_out: false,
        guessed,
        candidates_generated: 0,
    };
    let outcomes = vec![
        BatchWordOutcome {
            outcome: make("z", false),
            elapsed: std::time::Duration::ZERO,
        },
        BatchWordOutcome {
            outcome: make("a", true),
            elapsed: std::time::Duration::ZERO,
        },
    ];
    let decoded = decode_guess(&encode_batch_guess(&outcomes)).unwrap();
    assert_eq!(decoded.len(), 2);
    assert!(
        !decoded[0].guessed,
        "batch order must be request order, not signature-sorted"
    );
    assert!(decoded[1].guessed);
}
