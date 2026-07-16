//! T4 oracle conformance gate (`docs/fwdata-import-plan.md` §5.2): for Sena 3 and Amharic,
//! import the real FieldWorks `.fwdata` project through the *new* pipeline
//! (`pg_fwdata::import_file` -> `pg_snapshot::Snapshot` -> `hc_grammar::compile_project` ->
//! `Grammar`) and independently load the committed HC-XML oracle through the *legacy* pipeline
//! (`hc_grammar::load`), then run every word in `samples/data/{sena,amharic}-words.txt` through a
//! [`hc_parse::Morpher`] built from each `Grammar` and compare results **behaviorally**.
//!
//! IDs cannot be compared directly: the legacy XML export keys morphemes by session-scoped `Hvo`
//! integers while the new pipeline keys everything by FieldWorks GUID, so even the parity
//! `signature()` strings (which embed those ids) differ trivially between the two paths even when
//! the underlying analysis is identical. Instead, each analysis is reduced to something
//! comparable across both compilers: the in-order sequence of morpheme *glosses*
//! (`Grammar::morphemes[i].gloss`, resolved from each analysis's own `WordAnalysis::morpheme_ids`)
//! paired with the analysis's surface-shape string. A word's full result is the *multiset* of
//! these `(glosses, surface)` pairs (order across analyses is not meaningful; order of morphemes
//! *within* an analysis is).
//!
//! # Self-skipping (like `hc-grammar`'s own `sample_path()` tests and `pg-fwdata`'s
//! `real_projects.rs`)
//! Both the real FieldWorks project directory (`PANGLOSS_FW_PROJECTS_DIR`, falling back to the
//! known sibling checkout) and the committed oracle/word-list files under `samples/data/` are
//! untracked local corpora; either being absent makes the relevant test self-skip with a printed
//! reason rather than fail.
//!
//! # Why the full-corpus tests are `#[ignore]`d
//! Comparison must be uncapped (`Morpher::new(&g, usize::MAX)`): a step cap truncates the
//! analysis cascade non-deterministically (`hc-parse/tests/batch_determinism.rs`'s own module
//! doc), which would surface as spurious cross-compiler mismatches having nothing to do with
//! either compiler. Uncapped analysis of the full corpora (7,121 Sena words / 673 Amharic words)
//! through *two* grammars each is expensive -- consistent with this workspace's existing
//! convention for full-corpus runs (`hc-hybrid/tests/f9_full_battery_gate.rs`'s `#[ignore] //
//! expensive: ... run with --release --ignored`), so these two tests follow the same convention:
//! not part of a plain `cargo test --workspace`, run explicitly with
//! `cargo test -p hc-cli --release --ignored`.
//!
//! # Known oracle drift (Sena 3) -- documented failure, not tolerance
//! The committed `samples/data/sena-hc.xml` no longer corresponds byte-for-byte to the live
//! `Sena 3.fwdata`: three lexeme forms were edited in FLEx after the oracle was exported
//! (each entry's `DateModified` is 2026-06-16). Verified precisely by regenerating a fresh
//! oracle with FieldWorks' own `GenerateHCConfig.exe` from the current `.fwdata` (2026-07-15)
//! and diffing the digit-stripped line multisets: the ONLY content differences are
//! `peno`→`penohoho` (entry 2976cd0f), `guman`→`guman.hello.world`, and `mpaka`→`mpaka.la.la`
//! (obvious "hello world"/"la la" test edits); everything else is Hvo drift. The three corpus
//! words that hit those stale forms ([`KNOWN_ORACLE_DRIFT`]) are therefore *expected* to
//! mismatch against the committed oracle -- the committed oracle is wrong for them, not the
//! new pipeline (the fresh oracle agrees with the new pipeline). Each is asserted to still
//! mismatch (so this list self-invalidates if the oracle is ever regenerated) and reported
//! separately, never counted as a conformance failure.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use hc_grammar::model::Grammar;
use hc_parse::Morpher;

/// Corpus words known to hit the committed Sena oracle's three stale lexeme forms (see the
/// module doc's "Known oracle drift" section for the full verification trail).
const KNOWN_ORACLE_DRIFT: &[(&str, &str)] = &[
    ("peno", "committed oracle has stale root \"peno\"; live fwdata says \"penohoho\" (entry 2976cd0f, edited 2026-06-16)"),
    ("mpaka", "committed oracle has stale root \"mpaka\"; live fwdata says \"mpaka la la\""),
    ("guman", "committed oracle has stale root \"guman\" (x2); live fwdata says \"guman hello world\""),
];

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

/// Locates a file under `samples/data/`, or `None` if absent -- mirrors `hc-grammar`'s own
/// `sample_path()` test helper (`load.rs`).
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

/// One analysis reduced to a cross-compiler-comparable shape: the in-order morpheme gloss
/// sequence plus the surface string. `Ord`/`Eq` so a word's full analysis set (a `Vec` of these)
/// can be sorted into a canonical order and compared as a multiset.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct BehavioralAnalysis {
    glosses: Vec<String>,
    surface: String,
}

/// Reduce a `Morpher::parse_word` outcome to its sorted, cross-compiler-comparable multiset of
/// analyses. `outcome.structured[i]` and `outcome.analyses[i]` describe the same analysis (same
/// index, per `ParseOutcome`'s own doc) -- `structured[i].morpheme_ids` gives the numeric
/// morpheme sequence (resolved against *this* `Grammar`'s own `morphemes` table for the gloss),
/// `analyses[i].1` gives the surface string.
fn behavioral_result(grammar: &Grammar, outcome: &hc_parse::ParseOutcome) -> Vec<BehavioralAnalysis> {
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
    Mismatch { new: Vec<BehavioralAnalysis>, legacy: Vec<BehavioralAnalysis> },
}

fn compare_word(new_grammar: &Grammar, new_morpher: &Morpher, legacy_grammar: &Grammar, legacy_morpher: &Morpher, word: &str) -> WordComparison {
    let new_outcome = new_morpher.parse_word(word);
    let legacy_outcome = legacy_morpher.parse_word(word);
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

/// Runs the full behavioral comparison for one language, printing a summary + up to
/// `max_mismatches_to_print` compact diffs, and returning the number of mismatched words (0 =
/// perfect conformance). Deliberately does not assert itself -- callers that compare more than
/// one language in a loop need every language's results printed before the test fails on the
/// first mismatch it finds.
///
/// `known_drift` is the caller's [`KNOWN_ORACLE_DRIFT`]-style list (empty for a language with a
/// faithful oracle): a listed word is *expected* to mismatch (its reason is printed, and it never
/// counts as a conformance failure), while a listed word that unexpectedly MATCHES counts as a
/// failure -- the drift entry has gone stale (e.g. the oracle was regenerated) and must be
/// removed rather than silently masking a real word.
fn run_conformance(
    language: &str,
    new_grammar: &Grammar,
    legacy_grammar: &Grammar,
    words: &[String],
    known_drift: &[(&str, &str)],
    max_mismatches_to_print: usize,
) -> usize {
    let new_morpher = Morpher::new(new_grammar, usize::MAX).with_memo(true);
    let legacy_morpher = Morpher::new(legacy_grammar, usize::MAX).with_memo(true);

    let mut matched = 0usize;
    let mut drift_expected = 0usize;
    let mut mismatches: Vec<(String, Vec<BehavioralAnalysis>, Vec<BehavioralAnalysis>)> = Vec::new();

    for word in words {
        let drift = known_drift.iter().find(|(w, _)| w == word);
        match compare_word(new_grammar, &new_morpher, legacy_grammar, &legacy_morpher, word) {
            WordComparison::Match => match drift {
                None => matched += 1,
                Some((_, reason)) => {
                    eprintln!(
                        "  UNEXPECTED MATCH for known-oracle-drift word {word:?} ({reason}) -- \
                         the drift entry is stale; remove it"
                    );
                    mismatches.push((word.clone(), Vec::new(), Vec::new()));
                }
            },
            WordComparison::Mismatch { new, legacy } => match drift {
                None => mismatches.push((word.clone(), new, legacy)),
                Some((_, reason)) => {
                    drift_expected += 1;
                    eprintln!("  known oracle drift {word:?}: {reason}");
                }
            },
        }
    }

    eprintln!(
        "{language} conformance: {} words total, {} matched, {} mismatched, {} known-oracle-drift",
        words.len(),
        matched,
        mismatches.len(),
        drift_expected,
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

/// Imports `<fwdata_path>` through the new pipeline (`pg_fwdata` -> `Snapshot` ->
/// `compile_project`), returning the compiled `Grammar` plus every warning collected along the
/// way (import report + `Snapshot::validate()` + compile warnings), each labeled by stage so a
/// caller asserting on a specific known warning (e.g. Amharic's stale ad-hoc rule) can find it.
fn import_and_compile(fwdata_path: &Path) -> (Grammar, HashMap<&'static str, Vec<String>>) {
    let (snapshot, report) =
        pg_fwdata::import_file(fwdata_path).expect("import must succeed, not hard-error");
    let validate_warnings = snapshot.validate();
    let (grammar, compile_warnings) =
        hc_grammar::compile_project(&snapshot).expect("compile_project must succeed");

    let mut warnings = HashMap::new();
    warnings.insert("import", report.warnings);
    warnings.insert("validate", validate_warnings);
    warnings.insert("compile", compile_warnings);
    (grammar, warnings)
}

fn load_legacy(xml_path: &Path) -> Grammar {
    let xml = std::fs::read_to_string(xml_path).expect("read oracle xml");
    hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load oracle grammar: {e}"))
}

#[test]
#[ignore] // expensive: full 7,121-word Sena corpus through two uncapped Grammars; run with
          // `cargo test -p hc-cli --release --ignored sena3_new_pipeline_matches_legacy_oracle`
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
    assert!(words.len() >= 7000, "expected the full Sena corpus, got {}", words.len());

    let mismatched =
        run_conformance("Sena 3", &new_grammar, &legacy_grammar, &words, KNOWN_ORACLE_DRIFT, 20);
    assert_eq!(
        mismatched, 0,
        "Sena 3: {mismatched}/{} words mismatched between the new (.fwdata) and legacy (HC-XML) \
         pipelines -- see stderr above for the diffs",
        words.len()
    );
}

#[test]
#[ignore] // expensive: full 673-word Amharic corpus through two uncapped Grammars; run with
          // `cargo test -p hc-cli --release --ignored amharic_new_pipeline_matches_legacy_oracle`
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
    // §1's motivating example: a stale MoMorphAdhocProhib that crashes FieldWorks' own C#
    // exporter. The importer must succeed (asserted via `.expect` in `import_and_compile`) and
    // produce exactly the one known warning about it.
    assert_eq!(
        warnings["import"].len(),
        1,
        "expected exactly the one known stale-ad-hoc-rule warning; got: {:#?}",
        warnings["import"]
    );
    assert!(
        warnings["import"][0].contains("adhocProhibitions") || warnings["import"][0].contains("ad-hoc"),
        "expected the stale ad-hoc rule warning, got: {:?}",
        warnings["import"][0]
    );

    let legacy_grammar = load_legacy(&oracle_path);
    let words = read_words(&words_path);
    assert!(words.len() >= 650, "expected the full Amharic corpus, got {}", words.len());

    let mismatched = run_conformance("Amharic", &new_grammar, &legacy_grammar, &words, &[], 20);
    assert_eq!(
        mismatched, 0,
        "Amharic: {mismatched}/{} words mismatched between the new (.fwdata) and legacy (HC-XML) \
         pipelines -- see stderr above for the diffs",
        words.len()
    );
}

/// Was intended as a fast, always-run (not `#[ignore]`d) smoke test over a small prefix of each
/// corpus, to give `cargo test --workspace` *some* live signal on this pipeline even without
/// `--release --ignored`. In practice this does not hold: one of Amharic's first 50 words hits
/// the same uncapped-`Morpher` combinatorial blowup as the full-corpus tests above (confirmed via
/// bisection -- the hang predates every fix on this branch, present even at the bare T5-gate
/// commit before any grammar-compiler changes). Confirmed out of scope for this branch: the
/// grammar-equivalence gate (`fwdata_grammar_equivalence_gate.rs`) passes every category for both
/// languages, so the two pipelines' grammars are structurally equivalent -- the same word hangs
/// identically on both the new and legacy `Grammar`, meaning this is a parser-search-behavior
/// issue (tracked as complete-conformance/FST-speedup work on other branches), not an importer
/// defect. `#[ignore]`d for the same reason the two tests above are; run with `--ignored
/// --release` like them.
#[test]
#[ignore] // pre-existing uncapped-Morpher blowup on an Amharic word, unrelated to importer
          // correctness -- see this fn's doc; run with --ignored --release
fn conformance_smoke_first_50_words_each_language() {
    // Collect every language's result before asserting -- otherwise the first mismatching
    // language would panic before a second, independent language ever got to run.
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

        let known_drift = if project == "Sena 3" { KNOWN_ORACLE_DRIFT } else { &[] };
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
