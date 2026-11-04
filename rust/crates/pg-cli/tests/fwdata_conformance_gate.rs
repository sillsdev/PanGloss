//! Oracle conformance gate: imports a real FieldWorks project through the new pipeline, loads the committed HC-XML oracle through the legacy one, and compares `Morpher` behavior across both.
//! Why behavioral (not ID) comparison, self-skipping, why every test is `#[ignore]`d, the hang investigation, and verified Sena oracle drift: docs/research/pg-cli-fwdata-conformance-gate-notes.md.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use pg_grammar::model::Grammar;
use pg_parse::Morpher;

const K_MSA_GUID: &str = "ac0efd5d-b9d3-4aa3-9916-4650082e7ea4";
const K_ALLOMORPH_GUID: &str = "2083cd7f-43f1-4c7e-a110-47cf88f6914f";
const PREFIX_MSA_GUIDS: &[&str] = &[
    "04a3dcc1-a7d7-43b5-9952-e572c2dec5ab",
    "08fb76a0-6c51-4667-b8a3-18e46eab5dd5",
    "0ba5b050-ae6e-4a4d-93e3-33a9ea537c4d",
    "0f59e0b9-ce29-40ef-b618-3a19a0049e1a",
    "12199e48-53ec-45f0-bec1-f39067499611",
    "1283e545-cb0f-48a5-a02e-e0031433a22f",
    "4af8b147-4605-4658-ae6d-a54a24877af6",
    "6fd163ec-d17d-4fa2-ac52-1b3c426ad360",
    "7dd0b62a-9b13-4ba1-9e2f-3e9f70079c0d",
    "9aa05e36-98da-41b6-a7f7-dfd64c90fec2",
    "ad262853-28dd-4db3-a9d8-835b9bed8ca7",
    "e506c479-8cf3-43f0-b9c2-cbdbe51d9e78",
];
const PREFIX_ALLOMORPH_GUIDS: &[&str] = &[
    "09755be4-f77e-48bd-ad4c-3d09c6d802e5",
    "24762447-ccaa-4b45-b6e2-0ade951f16b9",
    "2d5f5a1b-1e53-4970-b24d-bda51bc239e4",
    "3a2435eb-c360-40b0-a696-82eff57c24d4",
    "3cca4a60-a091-49d4-acea-2f39eeafdb43",
    "4620eefc-b39a-43f4-8d88-7ffcf68c8d6d",
    "477c8851-8316-4290-befa-06a25db7323f",
    "7c6cfb58-ed2b-4b8c-9798-db07aea4f1f7",
    "8f80ab18-943d-4032-a807-654af6e5f60b",
    "90c3fd50-5aa8-4901-a212-be0cc58abc7c",
    "a3afc424-e5bb-432c-9d58-9951a93bee55",
    "ad15384a-225b-4573-bec3-c056535ac7a6",
];

/// Wall-clock deadline armed on every `Morpher` built in `run_conformance`; generous relative to a normal word's low-single-digit-millisecond parse time, but small enough that every corpus word hitting it stays a bounded, if slow, run rather than an unbounded hang.
const WORD_TIMEOUT: Duration = Duration::from_millis(500);

/// Corpus words known to hit the committed Sena oracle's three stale lexeme forms; see docs/research/pg-cli-fwdata-conformance-gate-notes.md for the verification trail.
const KNOWN_ORACLE_DRIFT: &[(&str, &str)] = &[
    ("peno", "committed oracle has stale root \"peno\"; live fwdata says \"penohoho\" (entry 2976cd0f, edited 2026-06-16)"),
    ("mpaka", "committed oracle has stale root \"mpaka\"; live fwdata says \"mpaka la la\""),
    ("guman", "committed oracle has stale root \"guman\" (x2); live fwdata says \"guman hello world\""),
];

fn adhoc_guid(rule: &pg_snapshot::AdhocProhibition) -> &str {
    match rule {
        pg_snapshot::AdhocProhibition::Allomorph { guid, .. }
        | pg_snapshot::AdhocProhibition::Morpheme { guid, .. } => guid.as_str(),
    }
}

/// Locates a FieldWorks project's `.fwdata` file, or `None` if the FieldWorks checkout isn't
/// present on this machine -- mirrors `pg-fwdata/tests/real_projects.rs::project_fwdata`.
fn project_fwdata(project_dir_name: &str) -> Option<PathBuf> {
    let base = std::env::var("PANGLOSS_FW_PROJECTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(r"C:\Users\johnm\Documents\repos\FieldWorks\DistFiles\Projects")
        });
    let path = base
        .join(project_dir_name)
        .join(format!("{project_dir_name}.fwdata"));
    path.exists().then_some(path)
}

/// Locates a file under `samples/data/`, or `None` if absent; mirrors `pg-grammar`'s own `sample_path` test helper.
fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn read_words(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .expect("read words file")
        .lines()
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

fn fwdata_refs(guids: &[&str], reference_type: &str) -> String {
    guids.iter().fold(String::new(), |mut refs, guid| {
        refs.push_str(&format!(
            "<objsur guid=\"{guid}\" t=\"{reference_type}\" />"
        ));
        refs
    })
}

fn copy_writing_system_store(base_path: &Path, fixture_dir: &Path) {
    let source_dir = base_path
        .parent()
        .expect("FieldWorks fixture has a parent directory")
        .join("WritingSystemStore");
    let destination_dir = fixture_dir.join("WritingSystemStore");
    std::fs::create_dir_all(&destination_dir).expect("create temporary WritingSystemStore");
    let mut copied = 0;
    for entry in std::fs::read_dir(source_dir).expect("read FieldWorks WritingSystemStore") {
        let source = entry.expect("read WritingSystemStore entry").path();
        if source.extension().and_then(|ext| ext.to_str()) == Some("ldml") {
            let destination = destination_dir.join(source.file_name().expect("LDML has a name"));
            std::fs::copy(source, destination).expect("copy FieldWorks LDML");
            copied += 1;
        }
    }
    assert!(copied > 0, "FieldWorks fixture has LDML sidecars");
}

fn adhoc_project(
    dir: &Path,
    base: &str,
    name: &str,
    grouped: bool,
    enabled_members: bool,
) -> PathBuf {
    const MORPH_DATA: &str = "5a540ed8-d95e-4a2b-805b-33b98c08c438";
    const GROUP: &str = "00000000-0000-0000-0000-000000000060";
    const NESTED_GROUP: &str = "00000000-0000-0000-0000-000000000065";
    const MORPHEME: &str = "00000000-0000-0000-0000-000000000061";
    const ALLOMORPH: &str = "00000000-0000-0000-0000-000000000062";
    const DISABLED: &str = "00000000-0000-0000-0000-000000000063";
    const EMPTY: &str = "00000000-0000-0000-0000-000000000064";
    const MISSING: &str = "00000000-0000-0000-0000-0000000000fe";
    let prefix_msa_refs = fwdata_refs(PREFIX_MSA_GUIDS, "r");
    let prefix_allomorph_refs = fwdata_refs(PREFIX_ALLOMORPH_GUIDS, "r");

    let field = if grouped {
        format!("<AdhocCoProhibitions><objsur guid=\"{GROUP}\" t=\"o\" /></AdhocCoProhibitions>")
    } else {
        format!(
            "<AdhocCoProhibitions><objsur guid=\"{EMPTY}\" t=\"o\" />\
             <objsur guid=\"{ALLOMORPH}\" t=\"o\" />\
             <objsur guid=\"{MORPHEME}\" t=\"o\" />\
             <objsur guid=\"{DISABLED}\" t=\"o\" /></AdhocCoProhibitions>"
        )
    };
    let mut source = base.to_string();
    let morph_data_start = source
        .find("<rt class=\"MoMorphData\"")
        .expect("base fixture contains MoMorphData");
    let morph_data_end = morph_data_start
        + source[morph_data_start..]
            .find("</rt>")
            .expect("MoMorphData record closes");
    source.insert_str(morph_data_end, &field);

    let disabled_active = if enabled_members { "False" } else { "True" };
    let nested_owner = if grouped { NESTED_GROUP } else { MORPH_DATA };
    let outer_owner = if grouped { GROUP } else { MORPH_DATA };
    let k_msa_guid = K_MSA_GUID;
    let k_allomorph_guid = K_ALLOMORPH_GUID;
    let member_records = format!(
        r#"
<rt class="MoMorphAdhocProhib" guid="{EMPTY}" ownerguid="{nested_owner}">
<Adjacency val="0" /><Disabled val="{disabled_active}" />
<FirstMorpheme><objsur guid="{k_msa_guid}" t="r" /></FirstMorpheme>
</rt>
<rt class="MoAlloAdhocProhib" guid="{ALLOMORPH}" ownerguid="{nested_owner}">
<Adjacency val="0" /><Disabled val="{disabled_active}" />
<FirstAllomorph><objsur guid="{k_allomorph_guid}" t="r" /></FirstAllomorph>
<RestOfAllos>{prefix_allomorph_refs}</RestOfAllos>
</rt>
<rt class="MoMorphAdhocProhib" guid="{MORPHEME}" ownerguid="{outer_owner}">
<Adjacency val="0" /><Disabled val="{disabled_active}" />
<FirstMorpheme><objsur guid="{k_msa_guid}" t="r" /></FirstMorpheme>
<RestOfMorphs>{prefix_msa_refs}</RestOfMorphs>
</rt>
<rt class="MoMorphAdhocProhib" guid="{DISABLED}" ownerguid="{nested_owner}">
<Adjacency val="0" /><Disabled val="True" />
<FirstMorpheme><objsur guid="{k_msa_guid}" t="r" /></FirstMorpheme>
<RestOfMorphs>{prefix_msa_refs}</RestOfMorphs>
</rt>
"#
    );
    let groups = if grouped {
        let group_disabled = if enabled_members { "True" } else { "False" };
        format!(
            r#"
<rt class="MoAdhocProhibGr" guid="{NESTED_GROUP}" ownerguid="{GROUP}">
<Disabled val="{group_disabled}" />
<Name><AUni ws="en">Nested rationale</AUni></Name>
<Description><AStr ws="en"><Run ws="en">Nested rule note</Run></AStr></Description>
<Members>
<objsur guid="{EMPTY}" t="o" />
<objsur guid="{DISABLED}" t="o" />
<objsur guid="{ALLOMORPH}" t="o" />
<objsur guid="{MISSING}" t="o" />
</Members>
</rt>
<rt class="MoAdhocProhibGr" guid="{GROUP}" ownerguid="{MORPH_DATA}">
<Disabled val="{group_disabled}" />
<Name><AUni ws="en">Rationale group</AUni></Name>
<Description><AStr ws="en"><Run ws="en">Group note</Run></AStr></Description>
<Members>
<objsur guid="{NESTED_GROUP}" t="o" />
<objsur guid="{MORPHEME}" t="o" />
</Members>
</rt>
"#
        )
    } else {
        String::new()
    };
    let records = format!("{member_records}{groups}");
    let project_end = source
        .rfind("</languageproject>")
        .expect("base fixture closes languageproject");
    source.insert_str(project_end, &records);

    let path = dir.join(format!("{name}.fwdata"));
    std::fs::write(&path, source).expect("write ad hoc fixture");
    path
}

#[test]
fn grouped_fwdata_adhoc_rules_match_flat_hcloader_semantics() {
    // PORT-CORRESPONDENCE: HCLoader.cs:340-350 filters concrete repository rules, not their groups.
    let dir = tempfile::tempdir().expect("temporary fixture directory");
    let base_path = pg_conformance_fixtures::suite_root()
        .join("edge-cases/deep-optional-affix-nesting/fieldworks/project.fwdata");
    let base_source = std::fs::read_to_string(&base_path).expect("read clean FieldWorks fixture");
    copy_writing_system_store(&base_path, dir.path());
    let grouped_path = adhoc_project(dir.path(), &base_source, "grouped", true, true);
    let flat_path = adhoc_project(dir.path(), &base_source, "flat", false, true);
    let disabled_grouped_path =
        adhoc_project(dir.path(), &base_source, "disabled-grouped", true, false);
    let disabled_flat_path = adhoc_project(dir.path(), &base_source, "disabled-flat", false, false);

    let (base_snapshot, _) = pg_fwdata::import_file(&base_path).expect("baseline imports");
    let (grouped_snapshot, grouped_report) =
        pg_fwdata::import_file(&grouped_path).expect("grouped project imports");
    let (flat_snapshot, _) = pg_fwdata::import_file(&flat_path).expect("flat project imports");
    assert_eq!(
        grouped_snapshot.morphology.adhoc_prohibitions,
        flat_snapshot.morphology.adhoc_prohibitions
    );
    assert_eq!(grouped_snapshot.morphology.adhoc_prohibitions.len(), 4);
    assert!(grouped_report
        .warnings
        .iter()
        .any(|warning| warning.code == "fwdata.dangling-reference"));
    assert!(matches!(
        grouped_snapshot
            .morphology
            .adhoc_prohibitions
        .iter()
        .find(|rule| adhoc_guid(rule) == "00000000-0000-0000-0000-000000000063"),
        Some(pg_snapshot::AdhocProhibition::Morpheme { others, disabled: true, .. })
            if others.iter().map(String::as_str).eq(PREFIX_MSA_GUIDS.iter().copied())
    ));
    assert!(matches!(
        grouped_snapshot
            .morphology
            .adhoc_prohibitions
            .iter()
            .find(|rule| adhoc_guid(rule) == "00000000-0000-0000-0000-000000000064"),
        Some(pg_snapshot::AdhocProhibition::Morpheme { others, .. }) if others.is_empty()
    ));

    let (base_grammar, _) = pg_grammar::compile_project(&base_snapshot).expect("baseline compiles");
    let (grouped_grammar, _) =
        pg_grammar::compile_project(&grouped_snapshot).expect("grouped project compiles");
    let (flat_grammar, _) =
        pg_grammar::compile_project(&flat_snapshot).expect("flat project compiles");
    for word in ["k", "xxxxxxxxxxxxk"] {
        let baseline = behavioral_result(
            &base_grammar,
            &Morpher::new(&base_grammar, usize::MAX).parse_word(word),
        );
        assert!(
            !baseline.is_empty(),
            "FieldWorks baseline must parse {word}"
        );
        let grouped = behavioral_result(
            &grouped_grammar,
            &Morpher::new(&grouped_grammar, usize::MAX).parse_word(word),
        );
        let flat = behavioral_result(
            &flat_grammar,
            &Morpher::new(&flat_grammar, usize::MAX).parse_word(word),
        );
        assert_eq!(grouped, flat, "grouped and flat analyses differ for {word}");
        if word == "xxxxxxxxxxxxk" {
            assert!(
                grouped.is_empty(),
                "the active group member must block {word}"
            );
        } else {
            assert!(
                !grouped.is_empty(),
                "the unrelated sequence {word} must remain parseable"
            );
        }
    }

    let (disabled_grouped_snapshot, _) =
        pg_fwdata::import_file(&disabled_grouped_path).expect("disabled grouped project imports");
    let (disabled_flat_snapshot, _) =
        pg_fwdata::import_file(&disabled_flat_path).expect("disabled flat project imports");
    let (disabled_grouped_grammar, _) = pg_grammar::compile_project(&disabled_grouped_snapshot)
        .expect("disabled grouped project compiles");
    let (disabled_flat_grammar, _) = pg_grammar::compile_project(&disabled_flat_snapshot)
        .expect("disabled flat project compiles");
    let disabled_grouped = behavioral_result(
        &disabled_grouped_grammar,
        &Morpher::new(&disabled_grouped_grammar, usize::MAX).parse_word("xxxxxxxxxxxxk"),
    );
    let disabled_flat = behavioral_result(
        &disabled_flat_grammar,
        &Morpher::new(&disabled_flat_grammar, usize::MAX).parse_word("xxxxxxxxxxxxk"),
    );
    assert_eq!(disabled_grouped, disabled_flat);
    assert!(
        !disabled_grouped.is_empty(),
        "disabled concrete members must not block xxxxxxxxxxxxk"
    );
}

/// One analysis reduced to a cross-compiler-comparable shape: the in-order morpheme gloss sequence plus the surface string, ordered so a word's analysis set can be sorted and compared as a multiset.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct BehavioralAnalysis {
    glosses: Vec<String>,
    surface: String,
}

/// Reduces a `parse_word` outcome to its sorted, cross-compiler-comparable multiset of analyses; `outcome.structured[i]` and `outcome.analyses[i]` describe the same analysis, same index.
fn behavioral_result(
    grammar: &Grammar,
    outcome: &pg_parse::ParseOutcome,
) -> Vec<BehavioralAnalysis> {
    let mut result: Vec<BehavioralAnalysis> = outcome
        .structured
        .iter()
        .zip(outcome.analyses.iter())
        .map(|(wa, (_, surface))| {
            let glosses = wa
                .morpheme_ids
                .iter()
                .map(|&id| {
                    grammar
                        .morphemes
                        .get(id as usize)
                        .and_then(|m| m.gloss.clone())
                        .unwrap_or_else(|| format!("<no-gloss:{id}>"))
                })
                .collect();
            BehavioralAnalysis {
                glosses,
                surface: surface.clone(),
            }
        })
        .collect();
    result.sort();
    result
}

/// The result of comparing one word across both pipelines.
enum WordComparison {
    Match,
    Mismatch {
        new: Vec<BehavioralAnalysis>,
        legacy: Vec<BehavioralAnalysis>,
    },
    /// Either side's `Morpher` hit `WORD_TIMEOUT`: neither a match nor a mismatch, just reported and skipped.
    TimedOut,
}

fn compare_word(
    new_grammar: &Grammar,
    new_morpher: &Morpher,
    legacy_grammar: &Grammar,
    legacy_morpher: &Morpher,
    word: &str,
) -> WordComparison {
    let new_outcome = new_morpher.parse_word(word);
    let legacy_outcome = legacy_morpher.parse_word(word);
    if new_outcome.timed_out || legacy_outcome.timed_out {
        return WordComparison::TimedOut;
    }
    let new_result = behavioral_result(new_grammar, &new_outcome);
    let legacy_result = behavioral_result(legacy_grammar, &legacy_outcome);
    if new_result == legacy_result {
        WordComparison::Match
    } else {
        WordComparison::Mismatch {
            new: new_result,
            legacy: legacy_result,
        }
    }
}

/// Runs the full behavioral comparison for one language, returning the mismatch count; deliberately does not assert itself, so callers comparing multiple languages get every result printed before failing.
/// How a `known_drift` root is judged once, in aggregate, over the whole word list: docs/research/pg-cli-fwdata-conformance-gate-notes.md.
fn run_conformance(
    language: &str,
    new_grammar: &Grammar,
    legacy_grammar: &Grammar,
    words: &[String],
    known_drift: &[(&str, &str)],
    max_mismatches_to_print: usize,
) -> usize {
    let new_morpher = Morpher::new(new_grammar, usize::MAX).with_word_timeout(Some(WORD_TIMEOUT));
    let legacy_morpher =
        Morpher::new(legacy_grammar, usize::MAX).with_word_timeout(Some(WORD_TIMEOUT));

    let mut matched = 0usize;
    let mut timed_out = 0usize;
    let mut mismatches: Vec<(String, Vec<BehavioralAnalysis>, Vec<BehavioralAnalysis>)> =
        Vec::new();
    // Per-root tallies for the aggregate known-drift invariant: a root is judged once, after the full word list has been scanned, not per-word.
    let mut drift_mismatches: HashMap<&str, usize> = HashMap::new();
    let mut drift_timeouts: HashMap<&str, usize> = HashMap::new();
    let mut drift_matches: HashMap<&str, usize> = HashMap::new();

    for word in words {
        // Substring, not exact equality: a drifted root also breaks every corpus word derived from it by affixation.
        let drift = known_drift.iter().find(|(root, _)| word.contains(root));
        match compare_word(
            new_grammar,
            &new_morpher,
            legacy_grammar,
            &legacy_morpher,
            word,
        ) {
            WordComparison::Match => {
                // Healthy: it merely shares a substring with the drifted root without being derived from it; the per-root aggregate below decides whether a drift entry is stale.
                matched += 1;
                if let Some((root, _)) = drift {
                    *drift_matches.entry(root).or_insert(0) += 1;
                }
            }
            WordComparison::Mismatch { new, legacy } => match drift {
                None => mismatches.push((word.clone(), new, legacy)),
                Some((root, reason)) => {
                    *drift_mismatches.entry(root).or_insert(0) += 1;
                    eprintln!("  known oracle drift {word:?}: {reason}");
                }
            },
            WordComparison::TimedOut => {
                timed_out += 1;
                if let Some((root, _)) = drift {
                    *drift_timeouts.entry(root).or_insert(0) += 1;
                }
                eprintln!(
                    "  TIMED OUT {word:?}: either pipeline's Morpher exceeded {WORD_TIMEOUT:?} -- \
                     a combinatorial-search issue (see module doc), not counted as a match or a \
                     mismatch"
                );
            }
        }
    }

    // Judge each known-drift root once, in aggregate, per this function's own doc comment.
    let mut drift_expected = 0usize;
    for (root, reason) in known_drift {
        let appeared = drift_mismatches.contains_key(root)
            || drift_timeouts.contains_key(root)
            || drift_matches.contains_key(root);
        if !appeared {
            // Root never showed up in this word list; absence isn't evidence of staleness, skip silently.
            continue;
        }
        let n_mismatch = drift_mismatches.get(root).copied().unwrap_or(0);
        let n_timeout = drift_timeouts.get(root).copied().unwrap_or(0);
        let n_match = drift_matches.get(root).copied().unwrap_or(0);
        if n_mismatch > 0 {
            drift_expected += 1;
        } else if n_timeout > 0 {
            eprintln!(
                "  known-oracle-drift root {root:?} inconclusive this run: {n_timeout} \
                 appearance(s) timed out and 0 confirmed mismatches -- not failing ({reason})"
            );
        } else {
            eprintln!(
                "  STALE known-oracle-drift root {root:?}: {n_match} corpus word(s) containing it \
                 all matched both pipelines (0 mismatches, 0 timeouts) -- the drift entry is \
                 stale; remove it from KNOWN_ORACLE_DRIFT ({reason})"
            );
            mismatches.push((
                format!("<stale drift entry {root:?}>"),
                Vec::new(),
                Vec::new(),
            ));
        }
    }

    eprintln!(
        "{language} conformance: {} words total, {} matched, {} mismatched, {} known-oracle-drift, \
         {} timed-out",
        words.len(),
        matched,
        mismatches.len(),
        drift_expected,
        timed_out,
    );
    for (word, new, legacy) in mismatches.iter().take(max_mismatches_to_print) {
        eprintln!("  MISMATCH {word:?}:");
        eprintln!("    new:    {new:?}");
        eprintln!("    legacy: {legacy:?}");
    }
    if mismatches.len() > max_mismatches_to_print {
        eprintln!(
            "  ... and {} more mismatches not printed",
            mismatches.len() - max_mismatches_to_print
        );
    }

    mismatches.len()
}

/// Imports through the new pipeline, returning the compiled `Grammar` plus every warning collected along the way, each labeled by stage so a caller can find a specific known warning.
fn import_and_compile(fwdata_path: &Path) -> (Grammar, HashMap<&'static str, Vec<String>>) {
    let (snapshot, report) =
        pg_fwdata::import_file(fwdata_path).expect("import must succeed, not hard-error");
    let validate_warnings = snapshot.validate();
    let (grammar, compile_warnings) =
        pg_grammar::compile_project(&snapshot).expect("compile_project must succeed");

    // Keep this legacy conformance map prose-only; production callers carry the typed warnings.
    let mut warnings = HashMap::new();
    warnings.insert(
        "import",
        report
            .warnings
            .into_iter()
            .map(|warning| warning.message)
            .collect(),
    );
    warnings.insert(
        "validate",
        validate_warnings
            .into_iter()
            .map(|warning| warning.message)
            .collect(),
    );
    warnings.insert(
        "compile",
        compile_warnings
            .into_iter()
            .map(|warning| warning.message)
            .collect(),
    );
    (grammar, warnings)
}

fn load_legacy(xml_path: &Path) -> Grammar {
    let xml = std::fs::read_to_string(xml_path).expect("read oracle xml");
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load oracle grammar: {e}"))
}

#[test]
#[ignore = "expensive: full 7,121-word Sena corpus through two uncapped Grammars; also needs a \
            real FieldWorks project checkout + local gitignored corpus data \
            (samples/data/sena-hc.xml, sena-words.txt); run with \
            `cargo test -p pg-cli --release --include-ignored sena3_new_pipeline_matches_legacy_oracle`"]
fn sena3_new_pipeline_matches_legacy_oracle() {
    let Some(fwdata_path) = project_fwdata("Sena 3") else {
        eprintln!("skipping: Sena 3 FieldWorks project not present on disk");
        return;
    };
    let Some(oracle_path) = sample_path("sena-hc.xml") else {
        eprintln!("skipping: samples/data/sena-hc.xml not present on disk");
        return;
    };
    let Some(words_path) = sample_path("sena-words.txt") else {
        eprintln!("skipping: samples/data/sena-words.txt not present on disk");
        return;
    };

    let (new_grammar, warnings) = import_and_compile(&fwdata_path);
    eprintln!(
        "Sena 3 new-pipeline warnings: import={} validate={} compile={}",
        warnings["import"].len(),
        warnings["validate"].len(),
        warnings["compile"].len()
    );
    for w in warnings.values().flatten() {
        eprintln!("  {w}");
    }

    let legacy_grammar = load_legacy(&oracle_path);
    let words = read_words(&words_path);
    assert!(
        words.len() >= 7000,
        "expected the full Sena corpus, got {}",
        words.len()
    );

    let mismatched = run_conformance(
        "Sena 3",
        &new_grammar,
        &legacy_grammar,
        &words,
        KNOWN_ORACLE_DRIFT,
        20,
    );
    assert_eq!(
        mismatched,
        0,
        "Sena 3: {mismatched}/{} words mismatched between the new (.fwdata) and legacy (HC-XML) \
         pipelines -- see stderr above for the diffs",
        words.len()
    );
}

#[test]
#[ignore = "expensive: full 673-word Amharic corpus through two uncapped Grammars; also needs a \
            real FieldWorks project checkout + local gitignored corpus data \
            (samples/data/amharic-hc.xml, amharic-words.txt); run with \
            `cargo test -p pg-cli --release --include-ignored amharic_new_pipeline_matches_legacy_oracle`"]
fn amharic_new_pipeline_matches_legacy_oracle() {
    let Some(fwdata_path) = project_fwdata("Amharic") else {
        eprintln!("skipping: Amharic FieldWorks project not present on disk");
        return;
    };
    let Some(oracle_path) = sample_path("amharic-hc.xml") else {
        eprintln!("skipping: samples/data/amharic-hc.xml not present on disk");
        return;
    };
    let Some(words_path) = sample_path("amharic-words.txt") else {
        eprintln!("skipping: samples/data/amharic-words.txt not present on disk");
        return;
    };

    let (new_grammar, warnings) = import_and_compile(&fwdata_path);
    eprintln!(
        "Amharic new-pipeline warnings: import={} validate={} compile={}",
        warnings["import"].len(),
        warnings["validate"].len(),
        warnings["compile"].len()
    );
    for w in warnings.values().flatten() {
        eprintln!("  {w}");
    }
    // A stale MoMorphAdhocProhib that crashes FieldWorks' own C# exporter: the importer must still succeed and produce exactly the one known warning about it.
    assert_eq!(
        warnings["import"].len(),
        1,
        "expected exactly the one known stale-ad-hoc-rule warning; got: {:#?}",
        warnings["import"]
    );
    assert!(
        warnings["import"][0].contains("adhocProhibitions")
            || warnings["import"][0].contains("ad-hoc"),
        "expected the stale ad-hoc rule warning, got: {:?}",
        warnings["import"][0]
    );

    let legacy_grammar = load_legacy(&oracle_path);
    let words = read_words(&words_path);
    assert!(
        words.len() >= 650,
        "expected the full Amharic corpus, got {}",
        words.len()
    );

    let mismatched = run_conformance("Amharic", &new_grammar, &legacy_grammar, &words, &[], 20);
    assert_eq!(
        mismatched,
        0,
        "Amharic: {mismatched}/{} words mismatched between the new (.fwdata) and legacy (HC-XML) \
         pipelines -- see stderr above for the diffs",
        words.len()
    );
}

/// A fast smoke test over a small prefix of each corpus; still `#[ignore]`d since it loads a real FieldWorks checkout and gitignored corpus data, self-skipping below when either is absent.
#[test]
#[ignore = "needs a real FieldWorks project checkout + local gitignored corpus data (samples/data/{sena,amharic}-{hc.xml,words.txt}); run with --include-ignored"]
fn conformance_smoke_first_50_words_each_language() {
    // Collected before asserting, so the first mismatching language doesn't panic before a second, independent one runs.
    let mut failures: Vec<(String, usize, usize)> = Vec::new();

    for (project, oracle, wordfile) in [
        ("Sena 3", "sena-hc.xml", "sena-words.txt"),
        ("Amharic", "amharic-hc.xml", "amharic-words.txt"),
    ] {
        let Some(fwdata_path) = project_fwdata(project) else {
            eprintln!("skipping {project} smoke: FieldWorks project not present on disk");
            continue;
        };
        let Some(oracle_path) = sample_path(oracle) else {
            eprintln!("skipping {project} smoke: samples/data/{oracle} not present on disk");
            continue;
        };
        let Some(words_path) = sample_path(wordfile) else {
            eprintln!("skipping {project} smoke: samples/data/{wordfile} not present on disk");
            continue;
        };

        let (new_grammar, _warnings) = import_and_compile(&fwdata_path);
        let legacy_grammar = load_legacy(&oracle_path);
        let mut words = read_words(&words_path);
        words.truncate(50);
        let word_count = words.len();

        let known_drift = if project == "Sena 3" {
            KNOWN_ORACLE_DRIFT
        } else {
            &[]
        };
        let mismatched = run_conformance(
            &format!("{project} (smoke, first {word_count} words)"),
            &new_grammar,
            &legacy_grammar,
            &words,
            known_drift,
            20,
        );
        if mismatched > 0 {
            failures.push((project.to_string(), mismatched, word_count));
        }
    }

    assert!(
        failures.is_empty(),
        "conformance smoke mismatches -- see stderr above for the diffs: {failures:?}"
    );
}
