use super::*;
use std::path::{Path, PathBuf};

#[test]
fn crate_builds() {
    assert_eq!(2 + 2, 4);
}

const HAND_BUILT_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>Test</Name>
<PhonologicalFeatureSystem>
  <SymbolicFeature id="feat1">
    <Name>voice</Name>
    <Symbols>
      <Symbol id="symP">+</Symbol>
      <Symbol id="symM">-</Symbol>
    </Symbols>
  </SymbolicFeature>
</PhonologicalFeatureSystem>
<CharacterDefinitionTable id="table1">
  <Name>Main</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="char1">
      <Representations>
        <Representation>s</Representation>
      </Representations>
      <FeatureValue feature="feat1" symbolValues="symM" />
    </SegmentDefinition>
    <SegmentDefinition id="char2">
      <Representations>
        <Representation>y</Representation>
      </Representations>
    </SegmentDefinition>
    <SegmentDefinition id="char3">
      <Representations>
        <Representation>sy</Representation>
      </Representations>
    </SegmentDefinition>
    <SegmentDefinition id="char4">
      <Representations>
        <Representation>m</Representation>
        <Representation>n</Representation>
      </Representations>
    </SegmentDefinition>
  </SegmentDefinitions>
  <BoundaryDefinitions>
    <BoundaryDefinition id="char5">
      <Representations>
        <Representation>+</Representation>
      </Representations>
    </BoundaryDefinition>
  </BoundaryDefinitions>
</CharacterDefinitionTable>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn loads_hand_built_grammar() {
    let g = load_char_def_table_from_xml(HAND_BUILT_XML).unwrap();
    // 1 authored feature + the always-appended synthetic `Type` feature (plan §13.1 Tier-1 #1).
    assert_eq!(g.feature_system().len(), 2);
    assert_eq!(
        g.feature_system().flat_index("feat1"),
        Some(featsys::FlatIndex(0))
    );
    let table = g.main_table().unwrap();
    assert_eq!(table.len(), 5); // 4 segments + 1 boundary
    assert!(table.lookup_nfd("m").is_some());
    assert_eq!(table.lookup_nfd("m"), table.lookup_nfd("n"));
}

#[test]
fn hand_built_grammar_greedy_segmentation_prefers_two_char_rep() {
    let g = load_char_def_table_from_xml(HAND_BUILT_XML).unwrap();
    let table = g.main_table().unwrap();
    let shape = segment::segment(table, "sy").unwrap();
    assert_eq!(
        shape.interior().count(),
        1,
        "must match the 2-char 'sy' def, not 's'+'y'"
    );
}

// --- Finding N1 (phase2 audit C): PhonologicalFeatureSystem@isActive -----------------------

/// Two `<PhonologicalFeatureSystem>` blocks, the second (inactive) listed last with a different feature set than the first (active): the active block must win regardless of document position, so a regression that lets the last block win would select `feat_b` instead of `feat_a`, which this test's `flat_index` assertions would catch immediately.
const TWO_PHON_FEATURE_SYSTEMS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>N1Test</Name>
<PhonologicalFeatureSystem>
  <SymbolicFeature id="feat_a">
    <Name>active</Name>
    <Symbols>
      <Symbol id="a_p">+</Symbol>
      <Symbol id="a_m">-</Symbol>
    </Symbols>
  </SymbolicFeature>
</PhonologicalFeatureSystem>
<PhonologicalFeatureSystem isActive="no">
  <SymbolicFeature id="feat_b">
    <Name>inactive draft</Name>
    <Symbols>
      <Symbol id="b_p">+</Symbol>
    </Symbols>
  </SymbolicFeature>
</PhonologicalFeatureSystem>
<CharacterDefinitionTable id="table1">
  <Name>Main</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="char1">
      <Representations>
        <Representation>x</Representation>
      </Representations>
    </SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn phon_feature_system_honors_is_active_not_last_block() {
    let g = load_char_def_table_from_xml(TWO_PHON_FEATURE_SYSTEMS_XML).unwrap();
    assert!(
        g.feature_system().flat_index("feat_a").is_some(),
        "the active (first) block's feature must be loaded"
    );
    assert!(
        g.feature_system().flat_index("feat_b").is_none(),
        "the inactive (last) block's feature must NOT be loaded (last-block-wins bug)"
    );
}

/// A single inactive block (no active block at all) must behave like no `<PhonologicalFeatureSystem>` at all: zero authored features.
#[test]
fn phon_feature_system_all_inactive_yields_no_features() {
    let xml = TWO_PHON_FEATURE_SYSTEMS_XML.replacen(
        "<PhonologicalFeatureSystem>",
        "<PhonologicalFeatureSystem isActive=\"no\">",
        1,
    );
    let g = load_char_def_table_from_xml(&xml).unwrap();
    assert!(g.feature_system().flat_index("feat_a").is_none());
    assert!(g.feature_system().flat_index("feat_b").is_none());
}

#[test]
fn dump_char_defs_is_deterministic() {
    let g = load_char_def_table_from_xml(HAND_BUILT_XML).unwrap();
    let d1 = g.dump_char_defs();
    let g2 = load_char_def_table_from_xml(HAND_BUILT_XML).unwrap();
    let d2 = g2.dump_char_defs();
    assert_eq!(d1, d2);
    // 1 authored feature + the always-appended synthetic `Type` feature.
    assert!(d1.contains("features=2"));
    assert!(d1.contains("char_defs=5"));
}

/// Locates a real sample grammar file on disk; these are untracked corpus files, present on this development machine but not guaranteed present elsewhere. Returns `None` if absent so callers can self-skip rather than fail.
fn sample_path(name: &str) -> Option<PathBuf> {
    // CARGO_MANIFEST_DIR = .../rust/crates/pg-grammar ; samples live at repo_root/samples/data.
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn load_words(name: &str) -> Option<Vec<String>> {
    let path = sample_path(name)?;
    let text = std::fs::read_to_string(path).ok()?;
    Some(
        text.lines()
            .map(str::to_string)
            .filter(|l| !l.is_empty())
            .collect(),
    )
}

/// Shared body for the three real-grammar tests: load, sanity-check the feature system and char-def table against exact counts independently confirmed by scanning the raw XML (not just non-empty), segment every requested word without error, and check determinism + node-count sanity. `expected_feature_count` is the authored (XML-visible) count; the assertion below adds 1 for the always-appended synthetic `Type` feature, which is not an XML-grep-able quantity.
fn check_grammar(
    xml_name: &str,
    words_name: &str,
    sample_words: &[&str],
    expected_feature_count: usize,
    expected_char_def_count: usize,
) {
    let Some(xml_path) = sample_path(xml_name) else {
        eprintln!("skipping {xml_name}: sample grammar not present on disk");
        return;
    };
    let xml = std::fs::read_to_string(&xml_path).expect("read sample grammar");
    let grammar = load_char_def_table_from_xml(&xml)
        .unwrap_or_else(|e| panic!("failed to load {xml_name}: {e}"));

    assert_eq!(
        grammar.feature_system().len(),
        expected_feature_count + 1,
        "{xml_name}: phonological feature count mismatch (authored count + the always-appended \
         Type feature) — loader may be dropping or double-counting <SymbolicFeature> elements"
    );

    let table = grammar
        .main_table()
        .unwrap_or_else(|| panic!("{xml_name}: expected at least one CharacterDefinitionTable"));
    assert_eq!(
        table.len(),
        expected_char_def_count,
        "{xml_name}: char-def count mismatch — loader may be dropping or double-counting \
         <SegmentDefinition>/<BoundaryDefinition> elements"
    );

    let words = load_words(words_name).unwrap_or_else(|| panic!("failed to read {words_name}"));
    for &w in sample_words {
        assert!(
            words.iter().any(|line| line == w),
            "{w:?} not found in {words_name} — test data assumption stale"
        );
    }

    for &word in sample_words {
        let nfd_len = nfd::nfd(word).chars().count();
        let shape1 = segment::segment(table, word)
            .unwrap_or_else(|e| panic!("{xml_name}: failed to segment {word:?}: {e}"));
        let interior_len = shape1.interior().count();
        assert!(
            (1..=nfd_len).contains(&interior_len),
            "{xml_name}: segmenting {word:?} produced {interior_len} interior nodes, expected \
             1..={nfd_len} (each match step consumes >=1 char and emits exactly 1 node)"
        );
        let shape2 = segment::segment(table, word).unwrap();
        assert_eq!(
            shape1, shape2,
            "{xml_name}: segmenting {word:?} must be deterministic"
        );
    }
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn loads_and_segments_indonesian() {
    // Counts independently confirmed via grep -c: 14 SymbolicFeature; 29 SegmentDefinition + 3 BoundaryDefinition = 32 char defs.
    check_grammar(
        "indonesian-hc.xml",
        "indonesian-words.txt",
        &["ajar", "amat", "ambil", "baca", "bagi"],
        14,
        32,
    );
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn loads_and_segments_amharic() {
    // amharic-words.txt's first few lines are English glosses, not target-language forms; the actual surface words are in Ge'ez/Ethiopic script. Counts confirmed via grep -c: 22 SymbolicFeature; 417 SegmentDefinition + 3 BoundaryDefinition = 420.
    check_grammar(
        "amharic-hc.xml",
        "amharic-words.txt",
        &["ሂዱ", "ሄደ", "ሆድ"],
        22,
        420,
    );
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn loads_and_segments_sena() {
    // Sena's real XML has no <PhonologicalFeatureSystem> element at all, so expected_feature_count: 0 documents a verified data reality, not a bug (the loader still reports len() == 1 for the always-appended synthetic Type feature). Char-def count confirmed via grep -c: 40 SegmentDefinition + 3 BoundaryDefinition = 43.
    check_grammar(
        "sena-hc.xml",
        "sena-words.txt",
        &["pibubu", "piratu", "mbali", "atawirambo"],
        0,
        43,
    );
}

/// Diagnostic, not a required gate: segments every word in each corpus and reports the failure rate; `#[ignore]`d so `cargo test` stays fast.
#[test]
#[ignore = "diagnostic full-corpus survey, not part of the M1 acceptance gate"]
fn full_corpus_segmentation_survey() {
    for (xml_name, words_name) in [
        ("indonesian-hc.xml", "indonesian-words.txt"),
        ("amharic-hc.xml", "amharic-words.txt"),
        ("sena-hc.xml", "sena-words.txt"),
    ] {
        let Some(xml_path) = sample_path(xml_name) else {
            eprintln!("skipping {xml_name}: not present on disk");
            continue;
        };
        let xml = std::fs::read_to_string(&xml_path).unwrap();
        let grammar = load_char_def_table_from_xml(&xml).unwrap();
        let table = grammar.main_table().unwrap();
        let words = load_words(words_name).unwrap();
        let mut ok = 0usize;
        let mut failures: Vec<(String, usize)> = Vec::new();
        for w in &words {
            match segment::segment(table, w) {
                Ok(_) => ok += 1,
                Err(e) => failures.push((w.clone(), e.position)),
            }
        }
        eprintln!(
            "{xml_name}: {ok}/{} segmented without error ({} failures)",
            words.len(),
            failures.len()
        );
        for (w, pos) in failures.iter().take(10) {
            eprintln!("  fail: {w:?} at position {pos}");
        }
    }
}
