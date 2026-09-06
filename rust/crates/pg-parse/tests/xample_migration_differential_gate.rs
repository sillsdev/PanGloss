//! The XAMPLE migration differential gate: real XAmple vs real HC-Rust over the checked-in FieldWorks witness, before and after its empty-phoneme-inventory mutation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pg_grammar::model::Grammar;
use pg_parse::Morpher;
use pg_xample_oracle::fieldworks::{self, MutateResponse, ProjectResponse, Projector};
use pg_xample_oracle::fixture::{self, PhonologyMutations};
use pg_xample_oracle::model::XampleResult;
use pg_xample_oracle::{xample_result_from_hc_outcome, HcNormalizationError};
use sha2::{Digest, Sha256};

const ACCEPTED_WORDS: &[&str] = &["k", "xxxxxxk", "xxxxxxxxxxxxk"];
const MUTATION_CASE_ID: &str = "empty-phoneme-inventory";

// A ratchet, not a target: today's measured count over the 3 accepted words is 0 in each direction.
const XAMPLE_ONLY_RATCHET: usize = 0;
const HC_ONLY_RATCHET: usize = 0;

fn allow_no_fieldworks() -> bool {
    std::env::var(fieldworks::ALLOW_NO_FIELDWORKS_ENV).as_deref() == Ok("1")
}

fn sha256_hex(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("{:x}", hasher.finalize())
}

// Copies the witness into a fresh dir; opening a project in place can leave session artifacts beside it.
fn copy_witness_project(witness_dir: &Path, dest_dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dest_dir).unwrap_or_else(|e| panic!("create {}: {e}", dest_dir.display()));
    let dest_fwdata = dest_dir.join("project.fwdata");
    std::fs::copy(witness_dir.join("project.fwdata"), &dest_fwdata)
        .unwrap_or_else(|e| panic!("copy project.fwdata: {e}"));
    let dest_ws = dest_dir.join("WritingSystemStore");
    std::fs::create_dir_all(&dest_ws).unwrap();
    let src_ws = witness_dir.join("WritingSystemStore");
    if let Ok(entries) = std::fs::read_dir(&src_ws) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            std::fs::copy(entry.path(), dest_ws.join(&name))
                .unwrap_or_else(|e| panic!("copy {}: {e}", entry.path().display()));
        }
    }
    dest_fwdata
}

// The unmodified pg_fwdata/pg_grammar pipeline (no substrate completion); Err, never a panic, so a caller can treat failure as expected.
fn import_and_compile(fwdata_path: &Path) -> Result<Grammar, String> {
    let (snapshot, _report) =
        pg_fwdata::import_file(fwdata_path).map_err(|e| format!("import_file: {e}"))?;
    let (grammar, _warnings) =
        pg_grammar::compile_project(&snapshot).map_err(|e| format!("compile_project: {e}"))?;
    Ok(grammar)
}

// Every accepted word's HC result, in the same XampleResult shape a parse capture reads into.
fn hc_results_by_word(grammar: &Grammar, words: &[&str]) -> BTreeMap<String, XampleResult> {
    let morpher = Morpher::new(grammar, usize::MAX);
    words
        .iter()
        .map(|&word| {
            let outcome = morpher.parse_word(word);
            let result = xample_result_from_hc_outcome(&outcome, grammar, word).unwrap_or_else(|e| {
                if matches!(e, HcNormalizationError::GuessedAnalysesNotComparable) {
                    XampleResult { analyses: BTreeMap::new(), reached_max_analyses: None, engine_error: Some(e.to_string()) }
                } else {
                    panic!("word {word:?}: {e}")
                }
            });
            (word.to_string(), result)
        })
        .collect()
}

fn xample_results_by_word(parsed: &pg_xample_oracle::ParsedParseResponse) -> BTreeMap<String, XampleResult> {
    parsed.words.iter().cloned().collect()
}

// compared increments only when both sides are usable (no engine_error, not capped); an unusable side contributes nothing.
fn accumulate(word: &str, xample: &XampleResult, hc: &XampleResult, compared: &mut usize, xample_only: &mut usize, hc_only: &mut usize) {
    let usable = xample.engine_error.is_none()
        && xample.reached_max_analyses.is_none()
        && hc.engine_error.is_none()
        && hc.reached_max_analyses.is_none();
    if !usable {
        eprintln!(
            "  {word}: not counted (xample engine_error={:?} capped={:?}; hc engine_error={:?} capped={:?})",
            xample.engine_error, xample.reached_max_analyses, hc.engine_error, hc.reached_max_analyses
        );
        return;
    }
    *compared += 1;
    for (sig, &count) in &xample.analyses {
        let hc_count = hc.analyses.get(sig).copied().unwrap_or(0);
        if count > hc_count {
            *xample_only += count - hc_count;
        }
    }
    for (sig, &count) in &hc.analyses {
        let xample_count = xample.analyses.get(sig).copied().unwrap_or(0);
        if count > xample_count {
            *hc_only += count - xample_count;
        }
    }
}

// Compares only entries both mark deterministic:true; this fixture's own GAFAWS OUT file legitimately varies run to run.
fn assert_deterministic_generated_files_match(label: &str, first: &ProjectResponse, second: &ProjectResponse) {
    for entry in &first.generated {
        if !entry.deterministic {
            continue;
        }
        let other = second
            .generated
            .iter()
            .find(|g| g.path == entry.path)
            .unwrap_or_else(|| panic!("{label}: second run has no generated entry for {}", entry.path));
        assert!(
            other.deterministic,
            "{label}: {} was deterministic on the first run but not the second",
            entry.path
        );
        assert_eq!(
            entry.sha256, other.sha256,
            "{label}: {} sha256 differs between two runs of the same case",
            entry.path
        );
    }
}

// The checked-in-capture parser path, exercised unconditionally regardless of the live-run skip policy below.
#[test]
fn captured_project_response_parses_with_source_and_generated_digests() {
    const CAPTURED: &str = include_str!("data/captured-project-response.json");
    let response: ProjectResponse =
        serde_json::from_str(CAPTURED).expect("checked-in captured 'project' response must parse");
    assert_eq!(response.schema_version, 1);
    assert_eq!(response.mode, "project");
    assert_eq!(response.source_sha256.len(), 64, "source digest must be a 64-hex sha256");
    assert!(!response.generated.is_empty(), "a project response names at least one generated file");
    for g in &response.generated {
        assert_eq!(g.sha256.len(), 64, "{}: generated-file digest must be a 64-hex sha256", g.path);
    }
    assert!(
        response.generated_sha256("Sena3.hc.xml").is_some(),
        "the checked-in capture must name its own hc.xml entry"
    );
}

#[test]
fn xample_migration_differential_gate() {
    let machine_dir = fieldworks::machine_dir();
    let witness_dir = fieldworks::witness_dir(&machine_dir);
    let witness_fwdata = witness_dir.join("project.fwdata");
    let manifest_path = witness_dir.join("phonology-mutations.yaml");

    if !witness_fwdata.is_file() || !manifest_path.is_file() {
        assert!(
            allow_no_fieldworks(),
            "no FieldWorks witness at {} (project.fwdata + phonology-mutations.yaml). Set \
             {} to a `machine` checkout that has it, or set {}=1 to explicitly skip this live \
             comparison.",
            witness_dir.display(),
            fieldworks::MACHINE_DIR_ENV,
            fieldworks::ALLOW_NO_FIELDWORKS_ENV
        );
        eprintln!(
            "SKIPPED (live XAMPLE migration comparison): no witness at {} ({}=1)",
            witness_dir.display(),
            fieldworks::ALLOW_NO_FIELDWORKS_ENV
        );
        return;
    }

    let projector = match Projector::locate() {
        Ok(p) => p,
        Err(e) => {
            assert!(allow_no_fieldworks(), "{e}");
            eprintln!("SKIPPED (live XAMPLE migration comparison): {e}");
            return;
        }
    };

    let manifest_text = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest_path.display()));
    let manifest: PhonologyMutations =
        fixture::parse_manifest(&manifest_text).unwrap_or_else(|e| panic!("{}: {e}", manifest_path.display()));
    fixture::verify_base_sha256(&manifest, &witness_fwdata).unwrap_or_else(|e| panic!("{e}"));
    let case = manifest
        .case(MUTATION_CASE_ID)
        .unwrap_or_else(|| panic!("{}: no case {MUTATION_CASE_ID:?}", manifest_path.display()));

    let temp_root = std::env::temp_dir().join(format!("pg-parse-xample-migration-gate-{}", std::process::id()));
    std::fs::create_dir_all(&temp_root).unwrap();

    // --- baseline ---
    let base_fwdata = copy_witness_project(&witness_dir, &temp_root.join("base"));
    let base_sha256_before = sha256_hex(&base_fwdata);
    assert_eq!(base_sha256_before, manifest.base_sha256, "the working copy must start identical to the verified witness");

    let base_projected = temp_root.join("base-projected");
    let base_project_response = projector
        .project(&base_fwdata, &base_projected, "MPBase")
        .unwrap_or_else(|e| panic!("baseline 'project' failed: {e}"));
    assert_eq!(base_project_response.source_sha256, base_sha256_before);

    let words: Vec<String> = ACCEPTED_WORDS.iter().map(|s| s.to_string()).collect();
    let base_parse = projector
        .parse(&base_fwdata, &base_projected, "MPBase", &words, &temp_root.join("base-parse.json"), 2000)
        .unwrap_or_else(|e| panic!("baseline 'parse' failed: {e}"));
    let base_xample = xample_results_by_word(&base_parse);

    let base_grammar = import_and_compile(&base_fwdata)
        .unwrap_or_else(|e| panic!("baseline import+compile must succeed (this project is the source of truth): {e}"));
    let base_hc = hc_results_by_word(&base_grammar, ACCEPTED_WORDS);

    let mut compared = 0usize;
    let mut xample_only = 0usize;
    let mut hc_only = 0usize;
    println!("--- baseline (real project) ---");
    for word in ACCEPTED_WORDS {
        accumulate(word, &base_xample[*word], &base_hc[*word], &mut compared, &mut xample_only, &mut hc_only);
    }

    // --- empty-phoneme-inventory mutation ---
    let request = case.to_request_json(&base_sha256_before);
    let mutate_out = temp_root.join("mutate-out");
    let mutate_response: MutateResponse = projector
        .mutate(&base_fwdata, &request, &mutate_out)
        .unwrap_or_else(|e| panic!("'{MUTATION_CASE_ID}' mutate failed: {e}"));
    assert!(mutate_response.reopened, "mutate response must prove reopened == true");
    assert_eq!(mutate_response.deleted_count as usize, case.operations.len().max(mutate_response.removed.len()));
    let mut removed_reps: Vec<String> =
        mutate_response.removed.iter().flat_map(|r| r.representations.iter().cloned()).collect();
    removed_reps.sort();
    let mut expected_segments = case.expect.inferred_segments.clone();
    expected_segments.sort();
    assert_eq!(removed_reps, expected_segments, "removed[] representations must equal the manifest's inferred_segments");

    let base_sha256_after_mutate = sha256_hex(&base_fwdata);
    assert_eq!(base_sha256_after_mutate, base_sha256_before, "mutate must never modify its source project");

    let clone_fwdata = mutate_out.join(&mutate_response.materialized_project_path);
    let clone_projected = temp_root.join("clone-projected");
    let clone_project_response = projector
        .project(&clone_fwdata, &clone_projected, "MPBase")
        .unwrap_or_else(|e| panic!("mutated-clone 'project' failed: {e}"));

    // xample_projection: byte-identical, or hvo-blind-equivalent -- see that function's own doc.
    fieldworks::xample_files_equivalent_ignoring_hvo_renumbering(&base_projected, &clone_projected, "MPBase")
        .unwrap_or_else(|mismatches| {
            panic!(
                "'{MUTATION_CASE_ID}': xample_projection is not same_as_base: {}",
                mismatches.iter().map(|m| m.to_string()).collect::<Vec<_>>().join("; ")
            )
        });

    let clone_parse = projector
        .parse(&clone_fwdata, &clone_projected, "MPBase", &words, &temp_root.join("clone-parse.json"), 2000)
        .unwrap_or_else(|e| panic!("mutated-clone 'parse' failed: {e}"));
    let clone_xample = xample_results_by_word(&clone_parse);

    println!("--- mutated clone ({MUTATION_CASE_ID}) ---");
    for word in ACCEPTED_WORDS {
        let base_result = &base_xample[*word];
        let clone_result = &clone_xample[*word];
        assert_eq!(
            base_result, clone_result,
            "word {word:?}: XAMPLE's own analysis multiset must be unaffected by a phoneme \
             deletion (XAmple loads a fixed, project-independent character table)"
        );
    }

    // Never asserted equal to baseline: a successful compile does not mean the character substrate is complete, so accumulate's own usability gate absorbs the expected divergence.
    match import_and_compile(&clone_fwdata) {
        Ok(clone_grammar) => {
            let clone_hc = hc_results_by_word(&clone_grammar, ACCEPTED_WORDS);
            for word in ACCEPTED_WORDS {
                accumulate(word, &clone_xample[*word], &clone_hc[*word], &mut compared, &mut xample_only, &mut hc_only);
            }
        }
        Err(e) => {
            println!(
                "NOTE: mutated-clone import+compile did not succeed without character-substrate \
                 completion (expected pre-Task-5): {e}"
            );
        }
    }

    // --- twice-run determinism: repeat the SAME case from the SAME base copy ---
    let mutate_out_2 = temp_root.join("mutate-out-2");
    let mutate_response_2: MutateResponse = projector
        .mutate(&base_fwdata, &request, &mutate_out_2)
        .unwrap_or_else(|e| panic!("second '{MUTATION_CASE_ID}' mutate failed: {e}"));
    assert_eq!(mutate_response_2.deleted_count, mutate_response.deleted_count, "deletedCount must be deterministic");
    assert_eq!(mutate_response_2.reopened, mutate_response.reopened);
    let mut removed_reps_2: Vec<String> =
        mutate_response_2.removed.iter().flat_map(|r| r.representations.iter().cloned()).collect();
    removed_reps_2.sort();
    assert_eq!(removed_reps_2, removed_reps, "removed[] must be deterministic across two runs of the same case");
    // Not asserted: raw .fwdata bytes (materializedSha256) may legitimately differ run to run (FieldWorks persistence metadata); the generated XAMPLE files below are the real determinism check.
    println!(
        "mutate ledger determinism: materializedSha256 run1={} run2={}",
        mutate_response.materialized_sha256, mutate_response_2.materialized_sha256
    );

    let clone_fwdata_2 = mutate_out_2.join(&mutate_response_2.materialized_project_path);
    let clone_projected_2 = temp_root.join("clone-projected-2");
    let clone_project_response_2 = projector
        .project(&clone_fwdata_2, &clone_projected_2, "MPBase")
        .unwrap_or_else(|e| panic!("second mutated-clone 'project' failed: {e}"));
    assert_deterministic_generated_files_match(MUTATION_CASE_ID, &clone_project_response, &clone_project_response_2);

    // --- before/after source-hash on the CANONICAL (checked-in) witness, not just the working copy ---
    let canonical_sha256_after = sha256_hex(&witness_fwdata);
    assert_eq!(
        canonical_sha256_after, manifest.base_sha256,
        "the checked-in witness itself must be byte-for-byte unchanged after this whole run"
    );

    println!("compared={compared} XAMPLE_ONLY={xample_only} HC_ONLY={hc_only}");
    assert!(compared > 0, "at least one word must have run through both engines usably");
    assert!(
        xample_only <= XAMPLE_ONLY_RATCHET,
        "XAMPLE_ONLY={xample_only} exceeds the ratchet ({XAMPLE_ONLY_RATCHET}) -- a new divergence, or the ratchet needs a fresh measurement"
    );
    assert!(
        hc_only <= HC_ONLY_RATCHET,
        "HC_ONLY={hc_only} exceeds the ratchet ({HC_ONLY_RATCHET}) -- a new divergence, or the ratchet needs a fresh measurement"
    );

    // Keep response objects alive for the assertions above; also silence unused-field lints without deny_unknown_fields drift.
    let _ = &base_project_response.database;
    let _ = &clone_project_response.database;

    std::fs::remove_dir_all(&temp_root).ok();
}

fn result_of(sigs: &[(&str, usize)], engine_error: Option<&str>, reached_max_analyses: Option<usize>) -> XampleResult {
    let mut analyses = BTreeMap::new();
    for (msa, count) in sigs {
        let signature = pg_xample_oracle::model::AnalysisSignature {
            morphemes: vec![],
            msa_ids: vec![msa.to_string()],
            category_id: None,
            surface_nfd: "w".to_string(),
        };
        analyses.insert(signature, *count);
    }
    XampleResult { analyses, reached_max_analyses, engine_error: engine_error.map(str::to_string) }
}

// Falsifies accumulate's usability gate: an errored side must not increment compared or fabricate a divergence.
#[test]
fn accumulate_does_not_count_an_errored_result() {
    let xample = result_of(&[("m1", 1)], None, None);
    let hc = result_of(&[], Some("invalid shape"), None);
    let (mut compared, mut xample_only, mut hc_only) = (0, 0, 0);
    accumulate("w", &xample, &hc, &mut compared, &mut xample_only, &mut hc_only);
    assert_eq!((compared, xample_only, hc_only), (0, 0, 0), "an errored side must not be counted");
}

// Same gate, the capped side.
#[test]
fn accumulate_does_not_count_a_capped_result() {
    let xample = result_of(&[("m1", 1)], None, Some(1));
    let hc = result_of(&[("m1", 1)], None, None);
    let (mut compared, mut xample_only, mut hc_only) = (0, 0, 0);
    accumulate("w", &xample, &hc, &mut compared, &mut xample_only, &mut hc_only);
    assert_eq!((compared, xample_only, hc_only), (0, 0, 0), "a capped side must not be counted");
}

// Falsifies the ratchet itself: a real divergence must make XAMPLE_ONLY/HC_ONLY nonzero.
#[test]
fn accumulate_counts_a_real_divergence_in_both_directions() {
    let xample = result_of(&[("m1", 1), ("m2", 1)], None, None);
    let hc = result_of(&[("m1", 1), ("m3", 1)], None, None);
    let (mut compared, mut xample_only, mut hc_only) = (0, 0, 0);
    accumulate("w", &xample, &hc, &mut compared, &mut xample_only, &mut hc_only);
    assert_eq!(compared, 1, "both sides were usable, so this word counts once");
    assert_eq!(xample_only, 1, "m2 is XAMPLE-only");
    assert_eq!(hc_only, 1, "m3 is HC-only");
    assert!(
        xample_only > XAMPLE_ONLY_RATCHET || hc_only > HC_ONLY_RATCHET,
        "a genuine divergence must be able to exceed this gate's own ratchets"
    );
}

// Deliberately failing: SubstrateReport does not exist on this branch yet -- see the panic message.
#[test]
fn empty_phoneme_inventory_substrate_report_matches_manifest_inferred_segments() {
    panic!(
        "BLOCKED on Task 5 (character-substrate completion, `pg_grammar::compile::substrate` / \
         `SubstrateReport`): that type does not exist on this branch, so the \
         `empty-phoneme-inventory` case's manifest `inferred_segments` (machine/conformance/\
         edge-cases/deep-optional-affix-nesting/fieldworks/phonology-mutations.yaml) cannot yet be \
         compared against a compiled grammar's inferred, featureless character definitions. Wire \
         this comparison once `pg_grammar::compile_project_with` returns a populated \
         `SubstrateReport`, and delete this panic."
    );
}
