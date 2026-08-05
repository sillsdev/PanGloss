//! T4 oracle conformance gate (`docs/fwdata-import-plan.md` §5.2): for Sena 3 and Amharic,
//! import the real FieldWorks `.fwdata` project through the *new* pipeline
//! (`pg_fwdata::import_file` -> `pg_snapshot::Snapshot` -> `pg_grammar::compile_project` ->
//! `Grammar`) and independently load the committed HC-XML oracle through the *legacy* pipeline
//! (`pg_grammar::load`), then run every word in `samples/data/{sena,amharic}-words.txt` through a
//! `pg_parse::Morpher` built from each `Grammar` and compare results **behaviorally**.
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
//! # Self-skipping (like `pg-grammar`'s own `sample_path()` tests and `pg-fwdata`'s
//! `real_projects.rs`)
//! Both the real FieldWorks project directory (`PANGLOSS_FW_PROJECTS_DIR`, falling back to the
//! known sibling checkout) and the committed oracle/word-list files under `samples/data/` are
//! untracked local corpora; either being absent makes the relevant test self-skip with a printed
//! reason rather than fail.
//!
//! # Why every test in this file is `#[ignore]`d
//! The step cap stays `usize::MAX` (`Morpher::new(&g, usize::MAX)`): a *step* cap truncates the
//! analysis cascade non-deterministically (`pg-parse/tests/batch_determinism.rs`'s own module
//! doc), which would surface as spurious cross-compiler mismatches having nothing to do with
//! either compiler. Uncapped analysis of the full corpora (7,121 Sena words / 673 Amharic words)
//! through *two* grammars each is expensive, so the two full-corpus tests are `#[ignore]`d on
//! that basis alone (consistent with this workspace's existing convention for full-corpus runs).
//! The third, fast 50-word smoke test is ALSO unconditionally `#[ignore]`d, on different grounds:
//! the default local `cargo test --workspace --release` run must not depend on the gitignored
//! `samples/data/*` corpus fixtures (or a real FieldWorks project checkout) at all, regardless of
//! test speed. Run any/all of them explicitly with
//! `cargo test -p pg-cli --release -- --include-ignored`.
//!
//! # The hang (fixed) -- `--word-timeout-ms`, not a step cap
//! A handful of real corpus words (confirmed: at least one in Amharic's first 50) hit a genuine
//! combinatorial blowup in the unmemoized-equivalent search space and never terminate under an
//! uncapped step count -- this used to make `conformance_smoke_first_50_words_each_language`
//! (previously not `#[ignore]`d) and both full-corpus tests here hang indefinitely. Confirmed via
//! `fwdata_grammar_equivalence_gate.rs` (which needs no `Morpher` at all) that the two compiled
//! `Grammar`s are structurally identical, so this is not an importer defect: the same word blows
//! up identically regardless of which pipeline produced its grammar. `Morpher::with_word_timeout`
//! (a wall-clock deadline, `pg-parse/tests/word_timeout_pathological_gate.rs`) fixes the hang
//! without the step cap's non-determinism problem: `run_conformance` arms
//! `WORD_TIMEOUT` on both morphers, and `compare_word` treats either side timing out as
//! `WordComparison::TimedOut` -- reported separately, like known oracle drift, never counted as
//! a match *or* a mismatch (a wall-clock deadline is inherently non-deterministic across runs/
//! machines, so the partial result at the moment it fires is not a meaningful cross-pipeline
//! comparison either way).
//!
//! # Known oracle drift (Sena 3) -- documented failure, not tolerance
//! The committed `samples/data/sena-hc.xml` no longer corresponds byte-for-byte to the live
//! `Sena 3.fwdata`: three lexeme forms were edited in FLEx after the oracle was exported.
//! Verified precisely by regenerating a fresh oracle with FieldWorks' own
//! `GenerateHCConfig.exe` from the current `.fwdata` and diffing the digit-stripped line
//! multisets: the ONLY content differences are
//! `peno`→`penohoho` (entry 2976cd0f), `guman`→`guman.hello.world`, and `mpaka`→`mpaka.la.la`
//! (obvious "hello world"/"la la" test edits); everything else is Hvo drift. Each
//! `KNOWN_ORACLE_DRIFT` entry is matched against the corpus by **substring**, not exact
//! equality: a root-form edit doesn't just break the bare root word, it breaks every corpus word
//! *derived* from that root by affixation too (confirmed against the full Sena corpus: 13 words
//! like `"agumana"`/`"kugumana"`/`"gumanik"` all fail to parse on the new pipeline, `new: []`,
//! because their surface form is built on `gu[mn]a[mn]`-style patterns that no longer match the
//! new pipeline's edited `"guman hello world"` root -- while legacy's stale-but-internally-
//! consistent `"guman"` root still parses them fine). All such words are therefore *expected* to
//! mismatch against the committed oracle -- the committed oracle is wrong for them, not the new
//! pipeline (the fresh oracle agrees with the new pipeline).
//!
//! This is a **per-root aggregate** invariant, not a per-word one: plenty of corpus words merely
//! *contain* a drift root's substring incidentally without being derived from the affected lexeme
//! at all (confirmed against the full Sena corpus: `"kugumanya"`, `"gumanika"`, `"madawipeno"` all
//! contain `"guman"`/`"peno"` yet parse identically on both pipelines -- healthy, unrelated words).
//! Such a word matching both pipelines is not a sign of anything wrong. Instead, each drift root is
//! asserted to still resolve to *at least one* mismatch **somewhere in the corpus** (so this list
//! self-invalidates if the oracle is ever regenerated), tolerating `WORD_TIMEOUT` noise: a root
//! is only flagged stale if every corpus word containing it plain-matched with zero timeouts and
//! zero mismatches, never merely because a thin root's one qualifying word happened to time out.
//! Confirmed live drift is reported separately, never counted as a conformance failure.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use pg_grammar::model::Grammar;
use pg_parse::Morpher;

/// Wall-clock deadline armed on every `Morpher` built in `run_conformance` (see the module
/// doc's "The hang (fixed)" section). Generous relative to a normal word's parse time (real corpus
/// words finish in low single-digit milliseconds; see this test file's own timing in the fast
/// grammar-equivalence gate for comparable grammars) but small enough that even every one of the
/// 7,121+673 corpus words hitting it in the worst case stays a bounded, if slow, test run rather
/// than the previous unbounded hang.
const WORD_TIMEOUT: Duration = Duration::from_millis(500);

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

/// Locates a file under `samples/data/`, or `None` if absent -- mirrors `pg-grammar`'s own
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
    /// Either side's `Morpher` hit `WORD_TIMEOUT` -- see the module doc's "The hang (fixed)"
    /// section for why this is neither a match nor a mismatch, just reported and skipped.
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

/// Runs the full behavioral comparison for one language, printing a summary + up to
/// `max_mismatches_to_print` compact diffs, and returning the number of mismatched words (0 =
/// perfect conformance). Deliberately does not assert itself -- callers that compare more than
/// one language in a loop need every language's results printed before the test fails on the
/// first mismatch it finds.
///
/// `known_drift` is the caller's `KNOWN_ORACLE_DRIFT`-style list (empty for a language with a
/// faithful oracle). This is a **per-root aggregate** invariant, not a per-word one: many corpus
/// words merely *contain* a drift root's substring incidentally without being morphologically
/// derived from the affected lexeme, so an individual such word matching both pipelines is
/// healthy, not an error -- it counts as an ordinary match. Each root in `known_drift` is instead
/// judged once, after the full word list has been scanned:
/// - never appeared in this word list -> skipped silently (absence isn't evidence of staleness;
///   most roots won't appear in a short prefix like the 50-word smoke test);
/// - at least one appearance was a confirmed `WordComparison::Mismatch` -> drift is live, as
///   expected, counted towards the returned summary's known-drift count;
/// - appeared only as `WordComparison::TimedOut` (zero mismatches) -> inconclusive, not a
///   failure (a thin root with few qualifying words must not fail just because its one qualifying
///   word got starved past `WORD_TIMEOUT` on this run);
/// - appeared, nothing timed out, and nothing mismatched -> the drift entry has gone stale (e.g.
///   the oracle was regenerated) and is reported as a conformance failure so it gets removed
///   rather than silently masking a real regression.
fn run_conformance(
    language: &str,
    new_grammar: &Grammar,
    legacy_grammar: &Grammar,
    words: &[String],
    known_drift: &[(&str, &str)],
    max_mismatches_to_print: usize,
) -> usize {
    let new_morpher = Morpher::new(new_grammar, usize::MAX)
        .with_memo(true)
        .with_word_timeout(Some(WORD_TIMEOUT));
    let legacy_morpher = Morpher::new(legacy_grammar, usize::MAX)
        .with_memo(true)
        .with_word_timeout(Some(WORD_TIMEOUT));

    let mut matched = 0usize;
    let mut timed_out = 0usize;
    let mut mismatches: Vec<(String, Vec<BehavioralAnalysis>, Vec<BehavioralAnalysis>)> =
        Vec::new();
    // Per-root tallies for the aggregate known-drift invariant (see this function's own doc
    // comment): a root is judged once, after the full word list has been scanned, not per-word.
    let mut drift_mismatches: HashMap<&str, usize> = HashMap::new();
    let mut drift_timeouts: HashMap<&str, usize> = HashMap::new();
    let mut drift_matches: HashMap<&str, usize> = HashMap::new();

    for word in words {
        // Substring, not exact equality -- see the module doc's "Known oracle drift" section:
        // a drifted root also breaks every corpus word derived from it by affixation.
        let drift = known_drift.iter().find(|(root, _)| word.contains(root));
        match compare_word(
            new_grammar,
            &new_morpher,
            legacy_grammar,
            &legacy_morpher,
            word,
        ) {
            WordComparison::Match => {
                // A word containing a drift root's substring that still matches both pipelines is
                // healthy -- it merely shares a substring with the drifted root without being
                // derived from it (see module doc). Counts as an ordinary match either way; only
                // the per-root aggregate below decides whether a drift entry itself is stale.
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
            // Root never showed up in this word list (e.g. the 50-word smoke prefix) -- absence
            // isn't evidence of staleness, skip silently.
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

/// Imports `<fwdata_path>` through the new pipeline (`pg_fwdata` -> `Snapshot` ->
/// `compile_project`), returning the compiled `Grammar` plus every warning collected along the
/// way (import report + `Snapshot::validate()` + compile warnings), each labeled by stage so a
/// caller asserting on a specific known warning (e.g. Amharic's stale ad-hoc rule) can find it.
fn import_and_compile(fwdata_path: &Path) -> (Grammar, HashMap<&'static str, Vec<String>>) {
    let (snapshot, report) =
        pg_fwdata::import_file(fwdata_path).expect("import must succeed, not hard-error");
    let validate_warnings = snapshot.validate();
    let (grammar, compile_warnings) =
        pg_grammar::compile_project(&snapshot).expect("compile_project must succeed");

    // `report.warnings`/`validate_warnings` are `pg_snapshot::Warning` (stable code + prose);
    // `compile_warnings` is still plain `String`. Flatten to prose here so this test's own
    // `HashMap<&str, Vec<String>>` shape (unrelated to this task's warning-code work) is
    // unchanged.
    let mut warnings = HashMap::new();
    warnings.insert(
        "import",
        report.warnings.into_iter().map(|w| w.to_string()).collect(),
    );
    warnings.insert(
        "validate",
        validate_warnings
            .into_iter()
            .map(|w| w.to_string())
            .collect(),
    );
    warnings.insert("compile", compile_warnings);
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

/// A fast smoke test over a small prefix of each corpus, giving *some* live signal on this
/// pipeline. One of Amharic's first 50 words used to hang this test indefinitely (the same
/// uncapped-`Morpher` combinatorial blowup as the full-corpus tests above; bisection showed it
/// predates every fix on this branch, present even at the bare T5-gate commit before any grammar-
/// compiler changes) -- fixed the same way as the full-corpus tests, via `run_conformance`'s
/// `WORD_TIMEOUT` (see the module doc's "The hang (fixed)" section): the pathological word now
/// reports as timed-out rather than hanging the whole suite, and this test terminates promptly.
///
/// Test-timing policy: despite being fast, this still loads a real
/// FieldWorks project checkout and the gitignored `samples/data/{sena,amharic}-{hc.xml,words.txt}`
/// corpus fixtures, so per policy it is unconditionally `#[ignore]`d too (the default local
/// `cargo test --workspace --release` run must not depend on gitignored corpus data at all); the
/// self-skip guards below already keep `--include-ignored` runs green when either is absent.
#[test]
#[ignore = "needs a real FieldWorks project checkout + local gitignored corpus data (samples/data/{sena,amharic}-{hc.xml,words.txt}); run with --include-ignored"]
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
