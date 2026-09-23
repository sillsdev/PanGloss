use super::*;

/// A count cap re-added here would refuse Aweti (4096) and Mbugwe (256) for ordinary breadth.
#[test]
fn breadth_alone_never_reads_as_dropped_spellings() {
    assert!(!VariantLimit::Complete { warn: false }.drops_spellings());
    assert!(
        !VariantLimit::Complete { warn: true }.drops_spellings(),
        "a complete enumeration drops nothing however broad; treating the advisory threshold as \
         a recall gap is what refused Aweti and Mbugwe outright"
    );
    assert!(VariantLimit::Unbounded.drops_spellings());
    assert!(VariantLimit::BytesExhausted { bytes: 1 }.drops_spellings());
}

fn load(path: &str) -> Grammar {
    let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../machine/conformance")
        .join(path);
    let xml = std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()));
    pg_grammar::load(&xml).unwrap()
}

fn load_without_realizational_rules(path: &str) -> Grammar {
    let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../machine/conformance")
        .join(path);
    let mut xml =
        std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()));
    for id in ["rrPast", "rrRRealTest"] {
        xml = xml.replace(&format!(" {id}"), "");
        let start_marker = format!("<RealizationalRule id=\"{id}\">");
        let start = xml
            .find(&start_marker)
            .unwrap_or_else(|| panic!("fixture must contain {start_marker}"));
        let relative_end = xml[start..]
            .find("</RealizationalRule>")
            .unwrap_or_else(|| panic!("fixture must close {start_marker}"));
        let end = start + relative_end + "</RealizationalRule>".len();
        xml.replace_range(start..end, "");
    }
    pg_grammar::load(&xml).unwrap()
}

/// Loads a samples/data/*.xml reference grammar; None (test skips) if not present on disk.
fn load_sample(name: &str) -> Option<Grammar> {
    let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../samples/data")
        .join(name);
    let xml = std::fs::read_to_string(&full).ok()?;
    Some(pg_grammar::load(&xml).unwrap())
}

fn entry_id_of(g: &Grammar, xml_key: &str) -> LexEntryId {
    LexEntryId(
        g.entries
            .iter()
            .position(|e| g.morphemes[e.morpheme.0 as usize].xml_key == xml_key)
            .unwrap() as u32,
    )
}

/// A mandatory (non-pattern) class reference like `b[Vowel]t` must enumerate every class member as its own spelling.
#[test]
fn pattern_variants_enumerates_mandatory_class_members() {
    let g = load("edge-cases/loader-pattern-shapes/grammar.xml");
    let table = surface_table(&g);
    let entry = &g.entries[entry_id_of(&g, "e1").0 as usize];
    let (variants, limit) = pattern_variants(table, &entry.allomorphs[0].shape.shape);
    assert!(!limit.drops_spellings());
    let mut sorted = variants.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["bat".to_string(), "bet".to_string()]);
}

/// The compiled TSP network accepts a NOVEL spelling through an unbounded (`[Any]*`) pattern root via its regex entry, and decodes to that pattern entry's own tag identity -- the point of a guesser pattern: representing a language, not a spelling list.
#[test]
fn pattern_route_accepts_a_novel_spelling_under_the_pattern_entrys_own_tag() {
    let g = load("languages/polysynthetic-stratal-derivation-chain/grammar.xml");
    let pattern_entry = entry_id_of(&g, "eGuessPat");
    let pattern_morpheme = g.entries[pattern_entry.0 as usize].morpheme;
    let mut proposer =
        crate::analyzer::compile_proposer(&g).expect("TunedSurfaceProbed must compile");
    let novel_word = "kivu";
    let candidates = proposer.propose(novel_word);
    assert!(
        candidates
            .iter()
            .any(|c| c.morphemes == vec![pattern_morpheme]),
        "expected a bare-root candidate tagged with the pattern entry's own morpheme for a \
         novel word; got {candidates:?}"
    );
}

/// An optional class node like `b([Vowel])t` must also admit the vowel-absent branch ("bt"), on top of "bat"/"bet".
#[test]
fn pattern_variants_optional_class_admits_the_absent_branch() {
    let g = load("edge-cases/loader-pattern-shapes/grammar.xml");
    let table = surface_table(&g);
    let entry = &g.entries[entry_id_of(&g, "e2").0 as usize];
    let (variants, limit) = pattern_variants(table, &entry.allomorphs[0].shape.shape);
    assert!(!limit.drops_spellings());
    let mut sorted = variants.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        vec!["bat".to_string(), "bet".to_string(), "bt".to_string()]
    );
}

/// A subtractive rule with a dropped LHS part must classify as is_structural_rule, or build_deriv_chain treats it as a silently wrong zero-morph.
#[test]
fn truncation_rules_classify_as_structural() {
    let g = load("edge-cases/truncate-morphotactic/grammar.xml");
    let structural: Vec<bool> = (0..g.mrules.len() as u32)
        .map(MRuleId)
        .map(|mid| is_structural_rule(&g, mid))
        .collect();
    assert_eq!(
        structural,
        vec![true, true, true],
        "all three rules in this grammar are structural"
    );
}

/// End-to-end: FomaProposer must actually propose every one of the fixture's analyses, not just classify correctly.
#[test]
fn truncation_composites_are_proposable() {
    let g = load("edge-cases/truncate-morphotactic/grammar.xml");
    let mut proposer = crate::analyzer::compile_proposer(&g).expect("compiles");
    for (word, min_candidates) in [("sa", 1), ("ag", 1), ("as", 1), ("gas", 2), ("gbubibi", 1)] {
        let n = proposer.propose(word).len();
        assert!(
            n >= min_candidates,
            "{word:?}: expected >= {min_candidates} candidate(s), got {n}"
        );
    }
}

/// probe_would_refuse must detect an empty-PhoneticInput (epenthesis-kind) rewrite subrule.
#[test]
fn probe_would_refuse_detects_epenthesis() {
    let g = load("languages/suffixing-vowel-harmony/grammar.xml");
    assert!(probe_would_refuse(&g));
}

/// probe_would_refuse is false for a grammar with real phonology but no epenthesis/metathesis construct.
#[test]
fn probe_would_refuse_is_false_for_ordinary_rewrite_rules() {
    let g = load("languages/suffixing-extension-slot-ordering/grammar.xml");
    assert!(!probe_would_refuse(&g));
}

/// A bare root whose surface exists only via an obligatory phonological rule, no morphological rule involved, must become proposable via collect_roots' bare-root phonology enrichment.
#[test]
fn bare_root_phonology_makes_post_nasal_voicing_proposable() {
    let g =
        load_without_realizational_rules("languages/suffixing-extension-slot-ordering/grammar.xml");
    // This checks a proposal path, not completeness of the broader fixture.
    let (proposer, _) = crate::analyzer::compile_proposer_unproven_with_profile(&g);
    let mut proposer = proposer.expect("development-only partial proposer compiles");
    assert!(
        !proposer.propose("mba").is_empty(),
        "\"mba\" must be proposable"
    );
}

/// probe_surface is POS-blind, so a bare root in a grammar that scopes different phonological rules to different POS in the same stratum needs generate_words' genuinely POS-gated pipeline instead.
#[test]
fn bare_root_phonology_prefers_the_pos_correct_generate_words_surface() {
    let g = load("languages/polysynthetic-stratal-derivation-chain/grammar.xml");
    let table = surface_table(&g);
    let cache = RuleCache::build(&g);
    let entry = &g.entries[entry_id_of(&g, "eBuiibuii").0 as usize];
    let feat_shape =
        pg_rules::shape_feat::segment_with_features(&g, table, &entry.allomorphs[0].shape.text)
            .unwrap();
    // Pinned so a future pg_rules fix making the probe POS-aware doesn't silently invalidate the generate_words-first ordering without this test flagging the change.
    assert_eq!(
        probe_surface(&g, table, &feat_shape, &cache),
        Some("bubu".to_string())
    );

    // This checks a proposal path despite the fixture's unrelated representation overflow.
    let (proposer, _) = crate::analyzer::compile_proposer_unproven_with_profile(&g);
    let mut proposer = proposer.expect("development-only partial proposer compiles");
    assert!(
        !proposer.propose("buuubuuu").is_empty(),
        "\"buuubuuu\" must be proposable despite the probe's own wrong answer"
    );
}

/// Ordinary suffix rules needing the harmony/gradation/epenthesis cascade, an infix rule in the same probe-refusing stratum, and bare-root vowel coalescence must all be proposable together.
#[test]
fn suffixing_vowel_harmony_full_cascade_words_are_proposable() {
    let g = load("languages/suffixing-vowel-harmony/grammar.xml");
    let past = (0..g.mrules.len() as u32)
        .map(MRuleId)
        .find(|&mid| g.morphemes[owning_morpheme(&g, mid).0 as usize].xml_key == "mrPast")
        .expect("fixture contains mrPast");
    assert!(candidate_requires_structural_route(&g, past));
    let generated = Morpher::new(&g, usize::MAX).generate_words(
        entry_id_of(&g, "eKutak"),
        &[GenMorpheme::Rule(past)],
        FeatureStruct::EMPTY,
    );
    assert!(
        generated.iter().any(|surface| surface == "kutagida"),
        "full-engine fallback returned {generated:?}"
    );
    let emitted = emit(&g);
    assert!(
        emitted.lexc_source.contains("kutagida"),
        "kutagida missing before foma compilation: {:?}",
        emitted.report.counts
    );
    let mut proposer = crate::analyzer::compile_proposer(&g).expect("compiles");
    for word in [
        "kutagida",
        "kutagila",
        "semitide",
        "semitideler",
        "semitile",
        "unitide",
        "satun",
        "guan",
        "duy",
        "sueb",
    ] {
        assert!(
            !proposer.propose(word).is_empty(),
            "{word:?} must be proposable"
        );
    }
}

/// Every proposed candidate must survive confirm end-to-end via the real conformance path.
fn assert_confirms(fixture: &str, words: &[&str]) {
    let g = load(&format!("{fixture}/grammar.xml"));
    // These broad fixtures provide proposal-path evidence, not artifact certification.
    let (proposer, _) = crate::analyzer::compile_proposer_unproven_with_profile(&g);
    let proposer = proposer.unwrap_or_else(|e| panic!("{fixture} compiles for development: {e}"));
    let mut analyzer = crate::composite::FomaAnalyzer::from_precompiled_proposer(&g, proposer);
    for &word in words {
        let outcome = analyzer.analyze_word(word);
        assert!(
            outcome.confirmed >= 1,
            "{fixture}/{word:?}: expected >= 1 confirmed analysis, got {}; candidates={}",
            outcome.confirmed,
            outcome.candidates_generated
        );
    }
}

/// A prefix on the compound head span needs the extra-root slot wired through a prefix-derivation chain; a bare extra-root slot cannot place a prefix between the two roots.
#[test]
fn compound_head_prefix_confirms_fusional_realizational_fixture() {
    assert_confirms("languages/fusional-realizational-morphology", &["lexbedom"]);
}

/// A POS-disjoint structural rule must not widen unrelated roots' closure.
#[test]
fn fusional_nonedge_anchor_reachability_is_root_specific() {
    let g = load("languages/fusional-realizational-morphology/grammar.xml");
    let ordinary = &g.entries[entry_id_of(&g, "eDomV").0 as usize];
    let ablaut = &g.entries[entry_id_of(&g, "eSing").0 as usize];

    assert!(
        !root_can_reach_nondecomposable_structural_anchor(&g, g.fs_interner.get(ordinary.syn_fs),),
        "posV2 must remain separated from the posAblautV-only ablaut rule"
    );
    assert!(
        root_can_reach_nondecomposable_structural_anchor(&g, g.fs_interner.get(ablaut.syn_fs),),
        "the posAblautV root must retain the exact structural closure"
    );
}

/// Emission records deterministic evidence that root-specific pruning ran.
#[test]
fn fusional_emission_reports_root_specific_candidate_pruning() {
    let g = load("languages/fusional-realizational-morphology/grammar.xml");
    let result = emit(&g);

    assert!(
        result.report.counts.structural_candidate_pairs_pruned > 0,
        "the mixed circumfix/ablaut grammar must exercise root-specific pruning"
    );
}

/// Probe refusal makes ordinary affixes the structural route's payload, so retain them.
#[test]
fn metathesis_probe_refusal_keeps_affix_candidates_per_root() {
    let g = load("languages/metathesis-phase-isolation/grammar.xml");
    assert!(probe_would_refuse(&g));
    let grammar_rules = structural_candidate_rules(&g);
    assert!(!grammar_rules.is_empty());
    let suffix = grammar_rules
        .iter()
        .copied()
        .find(|&mid| g.morphemes[owning_morpheme(&g, mid).0 as usize].xml_key == "mrUSuffix")
        .expect("fixture contains mrUSuffix");
    assert!(candidate_requires_structural_route(&g, suffix));
    let entry = &g.entries[entry_id_of(&g, "eMi").0 as usize];

    let (root_rules, pruned) =
        structural_candidate_rules_for_root(&g, &grammar_rules, g.fs_interner.get(entry.syn_fs));

    assert_eq!(root_rules, grammar_rules);
    assert_eq!(pruned, 0);
}

#[test]
fn compound_head_prefix_confirms_polysynthetic_fixture() {
    assert_confirms(
        "languages/polysynthetic-stratal-derivation-chain",
        &["silamanuk"],
    );
}

/// redupMorphType="prefix" reduplication needs the peel to prepend the redup morpheme, not append it, or confirm's positional match fails.
#[test]
fn prefix_reduplication_confirms() {
    assert_confirms(
        "languages/metathesis-phase-isolation",
        &["tutula", "tulatula"],
    );
}

/// Metathesis that leaves a boundary char inside the surface: generate_words strips it, with_boundary_insertions must re-introduce it so the boundary-bearing query is reachable.
#[test]
fn metathesis_boundary_in_surface_confirms() {
    assert_confirms("languages/metathesis-phase-isolation", &["mu+i"]);
}

/// A no-op without boundary reps; enumerates every interior gap with them.
#[test]
fn boundary_insertion_enumerates_interior_gaps() {
    assert_eq!(
        with_boundary_insertions("mui", &[]),
        vec!["mui".to_string()]
    );
    let plus = vec!["+".to_string()];
    let got = with_boundary_insertions("mui", &plus);
    assert!(got.contains(&"mui".to_string()));
    assert!(got.contains(&"m+ui".to_string()));
    assert!(got.contains(&"mu+i".to_string()));
    assert_eq!(got.len(), 3, "original + 2 interior gaps");
}

/// The eligible_roots broadening for compound grammars must make every compound-head-re-categorized analysis proposable and confirmable, full parity end-to-end.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn sena_musandilesera_full_parity() {
    let Some(g) = load_sample("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let mut analyzer = crate::composite::compile_analyzer(&g).expect("sena compiles");
    let outcome = analyzer.analyze_word("musandilesera");
    assert_eq!(
        outcome.confirmed, 10,
        "musandilesera must confirm all 10 engine analyses (was 2 before the eligible_roots \
         compound-recategorization broadening), got {}",
        outcome.confirmed
    );
}

// Diacritics gate: narrower white-box pins on the fix's mechanism; end-to-end coverage lives in tests/f5_diacritics_gate.rs.

fn load_dia_fixture() -> Grammar {
    let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dia-hc.xml");
    let xml = std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()));
    pg_grammar::load(&xml).unwrap()
}

/// A precomposed Latin letter is not itself a combining mark; NFD's combining marks are. Cyrillic never NFD-decomposes into a base+combining pair.
#[test]
fn char_is_combining_matches_nfd_combining_marks_only() {
    assert!(!char_is_combining('e'));
    assert!(!char_is_combining('\u{e9}')); // é (precomposed, NFC) -- not itself combining.
    assert!(char_is_combining('\u{301}')); // COMBINING ACUTE ACCENT (é's NFD decomposition).
    assert!(char_is_combining('\u{303}')); // COMBINING TILDE (ñ's NFD decomposition).
    assert!(char_is_combining('\u{308}')); // COMBINING DIAERESIS (ö's NFD decomposition).
    assert!(!char_is_combining('а')); // Cyrillic а (U+0430) -- an ordinary base letter.
}

/// combining_run_symbols must recover exactly the four base+combining-mark runs the NFD-normalized precomposed char-defs decompose into, nothing else.
#[test]
fn combining_run_symbols_finds_every_decomposed_diacritic() {
    let g = load_dia_fixture();
    let table = surface_table(&g);
    let mut got: Vec<String> = combining_run_symbols(table).into_iter().collect();
    got.sort();
    let mut want = vec![
        "e\u{301}".to_string(), // é
        "i\u{302}".to_string(), // î
        "n\u{303}".to_string(), // ñ
        "o\u{308}".to_string(), // ö
    ];
    want.sort();
    assert_eq!(got, want);
}

/// The emitted lexc source must actually declare these runs in Multichar_Symbols, not just compute and drop them.
#[test]
fn emitted_lexc_declares_the_combining_runs() {
    let g = load_dia_fixture();
    let result = emit(&g);
    let header_end = result
        .lexc_source
        .find("\nLEXICON")
        .unwrap_or(result.lexc_source.len());
    let header = &result.lexc_source[..header_end];
    for run in ["e\u{301}", "i\u{302}", "n\u{303}", "o\u{308}"] {
        assert!(
            header.contains(run),
            "Multichar_Symbols header must declare {run:?}; header was:\n{header}"
        );
    }
}

/// Measured, not assumed, against the three real reference grammars: Sena/Indonesian have no such run (byte-identical emitted lexc); Amharic has exactly one, exercised by f3_amharic_gate's own parity check.
#[test]
fn combining_run_symbols_measured_per_reference_grammar() {
    let cases: &[(&str, &[&str])] = &[
        ("sena-hc.xml", &[]),
        ("indonesian-hc.xml", &[]),
        ("amharic-hc.xml", &["a\u{308}"]),
    ];
    for &(name, want) in cases {
        let Some(g) = load_sample(name) else {
            eprintln!("skipping {name}: not present on disk");
            continue;
        };
        let table = surface_table(&g);
        let mut found: Vec<String> = combining_run_symbols(table).into_iter().collect();
        found.sort();
        let mut want: Vec<String> = want.iter().map(|s| s.to_string()).collect();
        want.sort();
        assert_eq!(found, want, "{name}: unexpected combining-run symbol set");
    }
}

// Boundary diacritics gate: white-box on boundary_combining_run_symbols; end-to-end coverage lives in tests/f5_diacritics_gate.rs.

fn load_boundary_fixture() -> Grammar {
    let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/boundary-mark-hc.xml");
    let xml = std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()));
    pg_grammar::load(&xml).unwrap()
}

/// boundary_combining_run_symbols must declare every length-1 (P ++ M) and length-2 (P ++ M ++ M) boundary run across the fixture's char-defs, nothing more.
#[test]
fn boundary_combining_run_symbols_finds_cross_char_def_runs() {
    let g = load_boundary_fixture();
    let table = surface_table(&g);
    let mut got: Vec<String> = boundary_combining_run_symbols(table).into_iter().collect();
    got.sort();
    let mut want = vec![
        "b\u{301}".to_string(),               // "b" (P) . mark
        "e\u{301}\u{301}".to_string(),        // "e\u{301}" (P's trailing run) . mark
        "\u{301}\u{301}".to_string(),         // mark-initial char-def itself as P . mark
        "b\u{301}\u{301}".to_string(),        // chain-of-2: "b" . mark . mark
        "e\u{301}\u{301}\u{301}".to_string(), // chain-of-2: "e\u{301}" . mark . mark
        "\u{301}\u{301}\u{301}".to_string(),  // chain-of-2: mark . mark . mark
    ];
    want.sort();
    assert_eq!(got, want);
}

/// Same convention as emitted_lexc_declares_the_combining_runs, for boundary runs.
#[test]
fn emitted_lexc_declares_the_boundary_runs() {
    let g = load_boundary_fixture();
    let result = emit(&g);
    let header_end = result
        .lexc_source
        .find("\nLEXICON")
        .unwrap_or(result.lexc_source.len());
    let header = &result.lexc_source[..header_end];
    for run in ["b\u{301}", "b\u{301}\u{301}", "\u{301}\u{301}\u{301}"] {
        assert!(
            header.contains(run),
            "Multichar_Symbols header must declare the boundary run {run:?}; header:\n{header}"
        );
    }
}

/// A grammar with no mark-initial char-def at all must short-circuit to an empty set, no spurious declarations.
#[test]
fn boundary_combining_run_symbols_empty_with_no_mark_initial_char_def() {
    let g = load_dia_fixture();
    let table = surface_table(&g);
    assert!(
        boundary_combining_run_symbols(table).is_empty(),
        "dia-hc.xml has no mark-initial char-def; boundary function must declare nothing"
    );
}

// Compile-profile instrumentation on the production emit_with_budget_profiled path.

/// The explicit default surface-emission strategy is an API seam only: it must retain the existing profiled wrapper's emitted artifact exactly.
#[test]
fn explicit_default_surface_strategy_matches_profiled_wrapper() {
    let g = load("languages/suffixing-extension-slot-ordering/grammar.xml");
    let mut wrapper_builder = crate::profile::CompileProfileBuilder::production();
    let through_wrapper =
        emit_with_budget_profiled(&g, PrecisionConfig::Strip, Some(&mut wrapper_builder));
    let wrapper_profile = wrapper_builder.finish(None, None);

    let mut explicit_builder = crate::profile::CompileProfileBuilder::production();
    let through_explicit_default = emit_with_budget_profiled_with_strategy(
        &g,
        PrecisionConfig::Strip,
        Some(&mut explicit_builder),
        SurfaceEmitStrategy::default(),
    );
    let explicit_profile = explicit_builder.finish(None, None);

    assert_eq!(
        through_explicit_default.lexc_source, through_wrapper.lexc_source,
        "the explicit default strategy must be byte-identical to the compatibility wrapper"
    );
    assert_eq!(
        through_explicit_default.report, through_wrapper.report,
        "the explicit default strategy must preserve the complete emission report"
    );
    assert_eq!(
        explicit_profile.pipeline, wrapper_profile.pipeline,
        "profile pipeline"
    );
    assert_eq!(
        explicit_profile
            .stages
            .iter()
            .map(|timing| timing.stage)
            .collect::<Vec<_>>(),
        wrapper_profile
            .stages
            .iter()
            .map(|timing| timing.stage)
            .collect::<Vec<_>>(),
        "the default strategy must preserve compile-stage order"
    );
    assert_eq!(
        explicit_profile.group_lines, wrapper_profile.group_lines,
        "the default strategy must preserve per-group line counts"
    );
    assert_eq!(
        explicit_profile.total_lexc_lines, wrapper_profile.total_lexc_lines,
        "the default strategy must preserve total emitted lines"
    );
    assert_eq!(
        explicit_profile.final_state_count, wrapper_profile.final_state_count,
        "the default strategy must preserve compiled state count"
    );
    assert_eq!(
        explicit_profile.final_arc_count, wrapper_profile.final_arc_count,
        "the default strategy must preserve compiled arc count"
    );
}

/// The profile must collect real per-stage data, not all-zero placeholders, including a stage only a real-phonology grammar exercises.
#[test]
fn fst_profile_collects_per_stage_data_on_a_synthetic_grammar() {
    let g = load("languages/suffixing-vowel-harmony/grammar.xml");
    let mut builder = crate::profile::CompileProfileBuilder::production();
    let result = emit_with_budget_profiled(&g, PrecisionConfig::Strip, Some(&mut builder));
    assert!(
        !matches!(result.report.tier, FomaTier::Unsupported { .. }),
        "sanity: this fixture must emit a usable network, got {:?}",
        result.report.tier
    );
    let profile = builder.finish(None, None);

    let stages: Vec<crate::profile::CompileStage> =
        profile.stages.iter().map(|s| s.stage).collect();
    assert!(stages.contains(&crate::profile::CompileStage::SurfaceSetup));
    assert!(stages.contains(&crate::profile::CompileStage::RootCollection));
    assert!(stages.contains(&crate::profile::CompileStage::PreexpandComposites));
    assert!(stages.contains(&crate::profile::CompileStage::StructuralComposites));
    assert!(stages.contains(&crate::profile::CompileStage::LexcConstruction));
    assert!(
        profile.total_lexc_lines.unwrap_or(0) > 0,
        "a real grammar must emit at least one lexc line"
    );
    assert_eq!(
        profile.total_lexc_lines,
        Some(result.report.counts.lexc_lines as u64),
        "the profile's own total must match the EmitReport's own count exactly"
    );
}

/// Profiling must never change the emitted artifact: byte-identical lexc_source and EmitCounts with profiling on vs. off.
#[test]
fn fst_profile_emitted_artifact_is_byte_identical_with_profiling_on_or_off() {
    let g = load("languages/suffixing-vowel-harmony/grammar.xml");
    let without_profile = emit_with_budget_profiled(&g, PrecisionConfig::Strip, None);

    let mut builder = crate::profile::CompileProfileBuilder::production();
    let with_profile = emit_with_budget_profiled(&g, PrecisionConfig::Strip, Some(&mut builder));

    assert_eq!(
        without_profile.lexc_source, with_profile.lexc_source,
        "profiling must never change the emitted lexc source"
    );
    assert_eq!(
        without_profile.report.counts.entries, with_profile.report.counts.entries,
        "entries"
    );
    assert_eq!(
        without_profile.report.counts.rules, with_profile.report.counts.rules,
        "rules"
    );
    assert_eq!(
        without_profile.report.counts.slots, with_profile.report.counts.slots,
        "slots"
    );
    assert_eq!(
        without_profile.report.counts.groups, with_profile.report.counts.groups,
        "groups"
    );
    assert_eq!(
        without_profile.report.counts.allomorphs_emitted,
        with_profile.report.counts.allomorphs_emitted,
        "allomorphs_emitted"
    );
    assert_eq!(
        without_profile.report.counts.allomorphs_skipped,
        with_profile.report.counts.allomorphs_skipped,
        "allomorphs_skipped"
    );
    assert_eq!(
        without_profile.report.counts.lexc_lines, with_profile.report.counts.lexc_lines,
        "lexc_lines"
    );
    assert_eq!(
        without_profile.report.counts.composite_pairs_probed,
        with_profile.report.counts.composite_pairs_probed,
        "composite_pairs_probed"
    );
    assert_eq!(
        without_profile
            .report
            .counts
            .composite_interdigitation_entries,
        with_profile.report.counts.composite_interdigitation_entries,
        "composite_interdigitation_entries"
    );
    assert_eq!(
        without_profile.report.counts.composite_fusion_entries,
        with_profile.report.counts.composite_fusion_entries,
        "composite_fusion_entries"
    );
    assert_eq!(
        without_profile.report.counts.composite_structural_entries,
        with_profile.report.counts.composite_structural_entries,
        "composite_structural_entries"
    );
    assert_eq!(
        without_profile.report.counts.bare_root_arcs_pruned,
        with_profile.report.counts.bare_root_arcs_pruned,
        "bare_root_arcs_pruned"
    );
    assert_eq!(without_profile.report.tier, with_profile.report.tier);
    assert_eq!(
        without_profile.report.uncovered, with_profile.report.uncovered,
        "profiling must preserve every uncovered item, id, and reason"
    );
    assert_eq!(
        without_profile.report.enum_budget_exceeded, with_profile.report.enum_budget_exceeded,
        "profiling must preserve bounded-refusal detail"
    );

    // Also exercise emit_with_budget's thin wrapper, for parity with the non-profiled entry point every existing caller uses.
    let via_wrapper = emit_with_budget(&g, PrecisionConfig::Strip);
    assert_eq!(via_wrapper.lexc_source, without_profile.lexc_source);
}

/// Every group must report a nonzero line count, and the per-group counts must sum to no more than the total.
#[test]
fn fst_profile_group_line_counts_are_real_and_bounded_by_the_total() {
    let g = load("languages/suffixing-vowel-harmony/grammar.xml");
    let mut builder = crate::profile::CompileProfileBuilder::production();
    let _ = emit_with_budget_profiled(&g, PrecisionConfig::Strip, Some(&mut builder));
    let profile = builder.finish(None, None);

    assert!(
        !profile.group_lines.is_empty(),
        "this fixture has templates and must report groups"
    );
    for group in &profile.group_lines {
        assert!(
            group.lines > 0,
            "group {} reported zero lines",
            group.group_index
        );
    }
    let group_total: u64 = profile.group_lines.iter().map(|g| g.lines).sum();
    assert!(group_total <= profile.total_lexc_lines.unwrap());
}

// Anti-drift guard for plan_topology_decisions, using only inline synthetic fixtures, pinning the invariant itself rather than any one grammar's verdict.

/// Asserts plan_topology_decisions's two booleans equal the real, directly-called seam functions' results; expected also pins the literal verdict, so a regression on either side is caught.
fn assert_plan_topology_matches_real_seams(g: &Grammar, expected: (bool, bool)) {
    let phon = PhonologyProbe::new(g);
    let real_composite_emission = crate::preexpand::should_run(g, phon.as_ref());
    let real_structural = !structural_candidate_rules(g).is_empty();
    assert_eq!(
        (real_composite_emission, real_structural),
        expected,
        "fixture's own expected topology is wrong -- fix the test, not the assertion below"
    );

    let (plan_composite_emission, plan_structural) = plan_topology_decisions(g, phon.as_ref());
    assert_eq!(
        plan_composite_emission, real_composite_emission,
        "plan-derived composite-emission decision (D2 row 1) must equal preexpand::should_run"
    );
    assert_eq!(
        plan_structural, real_structural,
        "plan-derived structural-composite decision (D2 row 2) must equal \
         !structural_candidate_rules(g).is_empty()"
    );
}

fn load_xml(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// No phonological rules and no rule of any kind: both should_run and structural_candidate_rules must be empty/false, the baseline case.
#[test]
fn plan_topology_decisions_matches_real_seams_bare_grammar() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>PlanTopologyBare</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
                  <Gloss>e1</Gloss>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    assert_plan_topology_matches_real_seams(&load_xml(XML), (false, false));
}

/// An ordinary phonological rewrite rule with no morphological/structural construct: phon alone makes should_run true, but the structural route stays absent.
#[test]
fn plan_topology_decisions_matches_real_seams_ordinary_phonology_only() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>PlanTopologyOrdinaryPhon</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="cp" /></SegmentNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><Segment segment="cb" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata>
            <Stratum characterDefinitionTable="t1" phonologicalRules="pr1">
              <Name>S</Name>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
                  <Gloss>e1</Gloss>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    assert_plan_topology_matches_real_seams(&load_xml(XML), (true, false));
}

/// An epenthesis-kind rewrite subrule plus an ordinary suffix rule widens structural_candidate_rules to include the suffix rule, so both subtrees are present at once.
#[test]
fn plan_topology_decisions_matches_real_seams_epenthesis_plus_suffix() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>PlanTopologyEpenthesisPlusSuffix</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cy"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <SegmentNaturalClass id="ncE"><Name>Epenthetic</Name><Segment segment="ce" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncX"><Name>X</Name><Segment segment="cx" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncY"><Name>Y</Name><Segment segment="cy" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="cx" /><Segment segment="ce" /><Segment segment="cy" /></SegmentNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="prEpenthesis">
              <Name>epenthesisAlone</Name>
              <PhoneticInput><PhoneticSequence /></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncE" /></PhoneticSequence></PhoneticOutput>
                  <Environment>
                    <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncX" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
                    <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncY" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
                  </Environment>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata>
            <Stratum characterDefinitionTable="t1" phonologicalRules="prEpenthesis" morphologicalRuleOrder="unordered" morphologicalRules="mr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <MorphologicalRule id="mr1">
                  <Name>-x</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="sub1">
                      <MorphologicalInput>
                        <PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence>
                      </MorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="stem" />
                        <InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments>
                      </MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </MorphologicalRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>xy</PhoneticShape></Allomorph></Allomorphs>
                  <Gloss>e1</Gloss>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    assert_plan_topology_matches_real_seams(&load_xml(XML), (true, true));
}

/// A lone circumfix rule with no phonological rules and no infix rule: should_run is false, but is_structural_rule admits CircumfixPrefix unconditionally, so the structural route is present anyway.
#[test]
fn plan_topology_decisions_matches_real_seams_circumfix_only() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>PlanTopologyCircumfixOnly</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <MorphologicalRule id="mr1">
                  <Name>circumfix</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="sub1">
                      <MorphologicalInput>
                        <PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence>
                      </MorphologicalInput>
                      <MorphologicalOutput>
                        <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                        <CopyFromInput index="stem" />
                        <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                      </MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </MorphologicalRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
                  <Gloss>e1</Gloss>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    assert_plan_topology_matches_real_seams(&load_xml(XML), (false, true));
}
