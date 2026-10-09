//! Compares XAMPLE minimum keys and C# structured analysis identities.

use std::collections::BTreeMap;
use std::path::Path;

use pg_parse::identity::AnalysisIdentity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct StoredMorph {
    #[serde(alias = "allomorphGuid")]
    allomorph: String,
    #[serde(alias = "msaGuid")]
    msa: String,
    #[serde(alias = "inflectionTypeGuid")]
    inflection_type: Option<String>,
}

type StoredKey = Vec<StoredMorph>;

#[derive(Deserialize)]
struct EngineObservation {
    stored_analysis_keys: Vec<StoredKey>,
    engine_error: Option<String>,
}

#[derive(Deserialize)]
struct Word {
    word: String,
    xample: EngineObservation,
    csharp_hc: EngineObservation,
}

#[derive(Deserialize)]
struct Case {
    measurement_protocol: String,
    words: Vec<Word>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct Difference {
    word: String,
    identity: Option<AnalysisIdentity>,
    csharp_present: bool,
    pangloss_present: bool,
    csharp_error: Option<String>,
    pangloss_invalid_shape: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedDifference {
    difference: Difference,
    ledger: String,
    reason: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DifferenceTable {
    schema_version: u32,
    comparison_profile: String,
    expected_differences: Vec<ExpectedDifference>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct MinimumLoss {
    case: String,
    word: String,
    key: StoredKey,
    xample_count: usize,
    pangloss_count: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MinimumException {
    loss: MinimumLoss,
    reason: String,
    ledger: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MinimumExceptionTable {
    schema_version: u32,
    exceptions: Vec<MinimumException>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyHcCapture {
    words: Vec<LegacyHcWord>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyHcWord {
    word: String,
    analyses: Vec<LegacyHcAnalysis>,
    engine_error: Option<String>,
    projected_engine_error: Option<String>,
    projected_analysis_count: Option<usize>,
    projection_agrees: bool,
}

#[derive(Deserialize)]
struct LegacyHcAnalysis {
    morphemes: Vec<StoredMorph>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityCapture {
    schema_version: u32,
    comparison_profile: String,
    case_name: String,
    state: String,
    engine_version: String,
    part_of_speech_symbols: Vec<CapturePartOfSpeech>,
    provenance: CaptureProvenance,
    words: Vec<CapturedWord>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapturePartOfSpeech {
    id: String,
    name: String,
    stable_guid: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureProvenance {
    harness_source_sha256: String,
    harness_project_sha256: String,
    sil_machine_sha256: String,
    hermit_crab_sha256: String,
    input_xml_sha256: String,
    input_words_sha256: String,
    legacy_capture_sha256: String,
    input_invocation_sha256: String,
    parse_solver_solution_count: usize,
    source_project_path: String,
    source_project_sha256: String,
    part_of_speech_crosswalk_evidence: Vec<PartOfSpeechCrosswalkEvidence>,
    static_crosswalk_evidence: Vec<StaticCrosswalkEvidence>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartOfSpeechCrosswalkEvidence {
    machine_id: String,
    name: String,
    stable_guid: String,
    hc_xml_path: String,
    hc_xml_sha256: String,
    hc_xml_id_line: usize,
    hc_xml_id_line_text: String,
    hc_xml_name_line: usize,
    hc_xml_name_line_text: String,
    field_works_project_path: String,
    field_works_project_sha256: String,
    field_works_candidate_class: String,
    field_works_candidate_line: usize,
    field_works_candidate_line_text: String,
    field_works_name_line: usize,
    field_works_name_line_text: String,
    field_works_abbreviation_line: usize,
    field_works_abbreviation_line_text: String,
    matching_candidate_guids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StaticCrosswalkEvidence {
    kind: String,
    source_id: String,
    stable_guid: String,
    parse_solver_candidates: Vec<String>,
    resolution: String,
    hc_xml_path: String,
    hc_xml_sha256: String,
    hc_xml_id_line: usize,
    hc_xml_id_line_text: String,
    hc_xml_form: Option<String>,
    hc_xml_form_line: Option<usize>,
    hc_xml_form_line_text: Option<String>,
    hc_xml_gloss: String,
    hc_xml_gloss_line: usize,
    hc_xml_gloss_line_text: String,
    field_works_project_path: String,
    field_works_project_sha256: String,
    field_works_candidate_class: String,
    field_works_candidate_line: usize,
    field_works_candidate_line_text: String,
    field_works_form: Option<String>,
    field_works_form_line: Option<usize>,
    field_works_form_line_text: Option<String>,
    field_works_sense_gloss: String,
    field_works_sense_gloss_line: usize,
    field_works_sense_gloss_line_text: String,
    paired_allomorphs: Vec<PairedAllomorphEvidence>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairedAllomorphEvidence {
    guid: String,
    class_name: String,
    candidate_line: usize,
    candidate_line_text: String,
    form: String,
    form_line: usize,
    form_line_text: String,
    sense_gloss: String,
    sense_gloss_line: usize,
    sense_gloss_line_text: String,
}

#[derive(Deserialize)]
struct SavedMeasurementResponse {
    #[serde(rename = "sourcePath")]
    source_path: String,
    #[serde(rename = "sourceSha256")]
    source_sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapturedWord {
    word: String,
    analyses: Vec<CapturedAnalysis>,
    engine_error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapturedAnalysis {
    stored_analysis_key: StoredKey,
    identity: MachineIdentity,
    machine_morpheme_ids: Vec<String>,
    machine_category_id: Option<String>,
    machine_stored_morph_ids: Vec<MachineStoredMorphIds>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MachineStoredMorphIds {
    allomorph_id: String,
    morpheme_id: String,
    inflection_type_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MachineIdentity {
    morphemes: Vec<String>,
    root_index: i32,
    category: Option<String>,
}

#[derive(Deserialize)]
struct EngineProvenance {
    binaries: BTreeMap<String, EngineBinaryPin>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EngineBinaryPin {
    file_version: String,
    sha256: String,
}

fn check_minimum_losses(actual: &[MinimumLoss], expected: &[MinimumLoss]) -> Result<(), String> {
    let actual = actual.iter().collect::<std::collections::BTreeSet<_>>();
    let expected_set = expected.iter().collect::<std::collections::BTreeSet<_>>();
    if expected_set.len() != expected.len() {
        return Err("duplicate expected minimum exceptions".into());
    }
    if actual != expected_set {
        return Err(format!(
            "unlisted XAMPLE losses: {:?}; disappeared exceptions: {:?}",
            actual.difference(&expected_set).collect::<Vec<_>>(),
            expected_set.difference(&actual).collect::<Vec<_>>()
        ));
    }
    Ok(())
}

fn differences(
    word: &str,
    hc: &std::collections::BTreeSet<AnalysisIdentity>,
    pg: &std::collections::BTreeSet<AnalysisIdentity>,
    error: Option<&str>,
    invalid_shape: bool,
) -> Vec<Difference> {
    let identities = hc.union(pg).collect::<std::collections::BTreeSet<_>>();
    let mut result = Vec::new();
    for identity in identities {
        let csharp_present = hc.contains(identity);
        let pangloss_present = pg.contains(identity);
        if csharp_present != pangloss_present {
            result.push(Difference {
                word: word.into(),
                identity: Some(identity.clone()),
                csharp_present,
                pangloss_present,
                csharp_error: None,
                pangloss_invalid_shape: false,
            });
        }
    }
    if error.is_some() || invalid_shape {
        result.push(Difference {
            word: word.into(),
            identity: None,
            csharp_present: false,
            pangloss_present: false,
            csharp_error: error.map(str::to_string),
            pangloss_invalid_shape: invalid_shape,
        });
    }
    result
}

fn check_differences(actual: &[Difference], expected: &[Difference]) -> Result<(), String> {
    let actual = actual.iter().collect::<std::collections::BTreeSet<_>>();
    let expected_set = expected.iter().collect::<std::collections::BTreeSet<_>>();
    if expected_set.len() != expected.len() {
        return Err("duplicate expected difference rows".into());
    }
    if actual != expected_set {
        return Err(format!(
            "unlisted: {:?}; disappeared: {:?}",
            actual.difference(&expected_set).collect::<Vec<_>>(),
            expected_set.difference(&actual).collect::<Vec<_>>()
        ));
    }
    Ok(())
}

fn multiset(keys: impl IntoIterator<Item = StoredKey>) -> BTreeMap<StoredKey, usize> {
    let mut result = BTreeMap::new();
    for key in keys {
        assert!(!key.is_empty(), "an empty analysis is not a stored key");
        *result.entry(key).or_default() += 1;
    }
    result
}

fn projected_keys(
    outcome: &pg_parse::ParseOutcome,
    grammar: &pg_grammar::model::Grammar,
) -> BTreeMap<StoredKey, usize> {
    multiset(outcome.structured.iter().map(|analysis| {
        pg_parse::project_parse_analysis(analysis, grammar)
            .expect("confirmed analysis must retain complete source identities")
            .morphs
            .into_iter()
            .map(|morph| {
                assert!(
                    morph.guessed_string.is_none(),
                    "measured words are not guessed"
                );
                StoredMorph {
                    allomorph: morph.form.expect("authored allomorph GUID"),
                    msa: morph.msa.expect("authored MSA GUID"),
                    inflection_type: morph.infl_type,
                }
            })
            .collect()
    }))
}

fn projected_identities(
    outcome: &pg_parse::ParseOutcome,
    grammar: &pg_grammar::model::Grammar,
) -> std::collections::BTreeSet<AnalysisIdentity> {
    outcome
        .structured
        .iter()
        .map(|analysis| {
            AnalysisIdentity::project(analysis, grammar)
                .expect("confirmed analysis must retain complete source identities")
        })
        .collect()
}

fn csharp_identities(word: &CapturedWord) -> std::collections::BTreeSet<AnalysisIdentity> {
    word.analyses
        .iter()
        .map(|analysis| {
            let morphemes = analysis
                .identity
                .morphemes
                .iter()
                .cloned()
                .map(Some)
                .collect();
            AnalysisIdentity {
                morphemes,
                root_index: analysis.identity.root_index,
                category: analysis.identity.category.clone(),
            }
        })
        .collect()
}

fn sha256_file(path: &Path) -> String {
    let bytes =
        std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    format!("{:x}", Sha256::digest(bytes))
}

fn same_hash(actual: &str, expected: &str, description: &str) {
    assert!(
        actual.eq_ignore_ascii_case(expected),
        "{description} hash mismatch: expected {expected}, found {actual}"
    );
}

fn underdefined_evidence_path(
    root: &Path,
    relative: &str,
    description: &str,
) -> std::path::PathBuf {
    let path = Path::new(relative);
    assert!(
        !path.is_absolute()
            && path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_))),
        "{description} path must stay inside the underdefined staging tree: {relative}"
    );
    root.join(path)
}

fn verify_evidence_line(path: &Path, line: usize, expected: &str, description: &str) {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read {description} {}: {error}", path.display()));
    let actual = text
        .lines()
        .nth(
            line.checked_sub(1)
                .expect("evidence line numbers are one-based"),
        )
        .unwrap_or_else(|| {
            panic!(
                "{description} line {line} is absent from {}",
                path.display()
            )
        });
    assert_eq!(
        actual,
        expected,
        "{description} line evidence changed at {}:{line}",
        path.display()
    );
}

fn verify_part_of_speech_crosswalk_evidence(
    capture: &IdentityCapture,
    underdefined_root: &Path,
    case_name: &str,
    state: &str,
) {
    let expected_xml_path = format!("{case_name}/measurements/{state}/Probe.hc.xml");
    let mut seen_machine_ids = std::collections::BTreeSet::new();
    let mut seen_stable_guids = std::collections::BTreeSet::new();
    assert_eq!(
        capture.provenance.part_of_speech_crosswalk_evidence.len(),
        capture.part_of_speech_symbols.len(),
        "{case_name}/{state}: every C# POS symbol needs saved-file evidence"
    );

    for (symbol, evidence) in capture
        .part_of_speech_symbols
        .iter()
        .zip(&capture.provenance.part_of_speech_crosswalk_evidence)
    {
        assert!(seen_machine_ids.insert(symbol.id.as_str()));
        assert!(seen_stable_guids.insert(symbol.stable_guid.as_str()));
        assert_eq!(evidence.machine_id, symbol.id);
        assert_eq!(evidence.name, symbol.name);
        assert_eq!(evidence.stable_guid, symbol.stable_guid);
        assert_eq!(evidence.hc_xml_path, expected_xml_path);
        assert_eq!(
            evidence.field_works_project_path,
            capture.provenance.source_project_path
        );
        assert_eq!(evidence.field_works_candidate_class, "PartOfSpeech");
        assert_eq!(
            evidence.matching_candidate_guids.as_slice(),
            std::slice::from_ref(&symbol.stable_guid)
        );

        let xml_path =
            underdefined_evidence_path(underdefined_root, &evidence.hc_xml_path, "HC XML");
        same_hash(
            &evidence.hc_xml_sha256,
            &sha256_file(&xml_path),
            "POS crosswalk HC XML",
        );
        same_hash(
            &evidence.hc_xml_sha256,
            &capture.provenance.input_xml_sha256,
            "POS crosswalk input XML",
        );
        verify_evidence_line(
            &xml_path,
            evidence.hc_xml_id_line,
            &evidence.hc_xml_id_line_text,
            "HC XML POS id",
        );
        assert!(evidence
            .hc_xml_id_line_text
            .contains(&format!("id=\"{}\"", evidence.machine_id)));
        verify_evidence_line(
            &xml_path,
            evidence.hc_xml_name_line,
            &evidence.hc_xml_name_line_text,
            "HC XML POS name",
        );
        assert!(evidence.hc_xml_name_line_text.contains(&evidence.name));

        let project_path = underdefined_evidence_path(
            underdefined_root,
            &evidence.field_works_project_path,
            "saved FieldWorks project",
        );
        same_hash(
            &evidence.field_works_project_sha256,
            &sha256_file(&project_path),
            "POS crosswalk FieldWorks project",
        );
        same_hash(
            &evidence.field_works_project_sha256,
            &capture.provenance.source_project_sha256,
            "POS crosswalk saved project",
        );
        verify_evidence_line(
            &project_path,
            evidence.field_works_candidate_line,
            &evidence.field_works_candidate_line_text,
            "FieldWorks POS candidate",
        );
        assert!(evidence
            .field_works_candidate_line_text
            .contains(&format!("guid=\"{}\"", evidence.stable_guid)));
        assert!(evidence
            .field_works_candidate_line_text
            .contains("class=\"PartOfSpeech\""));
        verify_evidence_line(
            &project_path,
            evidence.field_works_name_line,
            &evidence.field_works_name_line_text,
            "FieldWorks POS name",
        );
        assert!(evidence.field_works_name_line_text.contains(&evidence.name));
        verify_evidence_line(
            &project_path,
            evidence.field_works_abbreviation_line,
            &evidence.field_works_abbreviation_line_text,
            "FieldWorks POS abbreviation",
        );
        assert!(evidence
            .field_works_abbreviation_line_text
            .contains(&evidence.name));
    }
}

fn verify_static_crosswalk_evidence(
    capture: &IdentityCapture,
    measurements: &Path,
    underdefined_root: &Path,
    case_name: &str,
    state: &str,
    source_allomorph_keys: &BTreeMap<String, String>,
    source_morpheme_keys: &BTreeMap<String, String>,
) {
    let provenance = &capture.provenance;
    assert!(provenance.parse_solver_solution_count > 0);
    if provenance.parse_solver_solution_count == 1 {
        assert!(
            provenance.static_crosswalk_evidence.is_empty(),
            "{case_name}/{state}: a unique parse-solver mapping should not need static disambiguation"
        );
    } else {
        assert!(
            !provenance.static_crosswalk_evidence.is_empty(),
            "{case_name}/{state}: ambiguous parse-solver mappings need saved-file evidence"
        );
    }

    let response: SavedMeasurementResponse = serde_json::from_slice(
        &std::fs::read(measurements.join("response.json")).expect("read measurement response"),
    )
    .expect("measurement response schema");
    let response_project = measurements
        .join(&response.source_path)
        .canonicalize()
        .expect("resolve saved project path from response.json");
    let evidence_root = underdefined_root
        .canonicalize()
        .expect("resolve underdefined evidence root");
    let relative_project = response_project
        .strip_prefix(&evidence_root)
        .expect("saved project must live under the underdefined root")
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    assert_eq!(
        provenance.source_project_path, relative_project,
        "{case_name}/{state}: sidecar must identify the exact saved project named by response.json"
    );
    let source_project = underdefined_evidence_path(
        underdefined_root,
        &provenance.source_project_path,
        "saved FieldWorks project",
    );
    same_hash(
        &provenance.source_project_sha256,
        &sha256_file(&source_project),
        "saved FieldWorks project",
    );
    same_hash(
        &response.source_sha256,
        &provenance.source_project_sha256,
        "response.json saved FieldWorks project",
    );

    let expected_xml_path = format!("{case_name}/measurements/{state}/Probe.hc.xml");
    let expected_project_path = provenance.source_project_path.as_str();
    let mut seen = std::collections::BTreeSet::new();
    for evidence in &provenance.static_crosswalk_evidence {
        assert!(
            seen.insert((evidence.kind.as_str(), evidence.source_id.as_str())),
            "{case_name}/{state}: duplicate static crosswalk evidence for {}:{}",
            evidence.kind,
            evidence.source_id
        );
        assert!(
            evidence.parse_solver_candidates.len() >= 2,
            "{case_name}/{state}/{}: static evidence is only for ambiguous parse-solver IDs",
            evidence.source_id
        );
        let mut candidates = evidence.parse_solver_candidates.clone();
        candidates.sort();
        candidates.dedup();
        assert_eq!(
            candidates.len(),
            evidence.parse_solver_candidates.len(),
            "{case_name}/{state}/{}: duplicate parse-solver candidate",
            evidence.source_id
        );
        assert!(
            evidence
                .parse_solver_candidates
                .contains(&evidence.stable_guid),
            "{case_name}/{state}/{}: selected GUID is not a parse-solver candidate",
            evidence.source_id
        );
        assert!(
            evidence.resolution.contains("unique")
                && evidence.resolution.contains("parse-solver candidate"),
            "{case_name}/{state}/{}: static resolution lacks a uniqueness and solver claim",
            evidence.source_id
        );
        assert_eq!(evidence.hc_xml_path, expected_xml_path);
        assert_eq!(evidence.field_works_project_path, expected_project_path);

        let xml_path =
            underdefined_evidence_path(underdefined_root, &evidence.hc_xml_path, "HC XML");
        same_hash(
            &evidence.hc_xml_sha256,
            &sha256_file(&xml_path),
            "static evidence HC XML",
        );
        same_hash(
            &evidence.field_works_project_sha256,
            &sha256_file(&source_project),
            "static evidence FieldWorks project",
        );
        verify_evidence_line(
            &xml_path,
            evidence.hc_xml_id_line,
            &evidence.hc_xml_id_line_text,
            "HC XML source ID",
        );
        assert!(evidence.hc_xml_id_line_text.contains(&evidence.source_id));
        verify_evidence_line(
            &xml_path,
            evidence.hc_xml_gloss_line,
            &evidence.hc_xml_gloss_line_text,
            "HC XML entry gloss",
        );
        assert!(evidence
            .hc_xml_gloss_line_text
            .contains(&evidence.hc_xml_gloss));
        verify_evidence_line(
            &source_project,
            evidence.field_works_candidate_line,
            &evidence.field_works_candidate_line_text,
            "FieldWorks candidate",
        );
        assert!(evidence
            .field_works_candidate_line_text
            .contains(&evidence.stable_guid));
        assert!(evidence
            .field_works_candidate_line_text
            .contains(&evidence.field_works_candidate_class));
        verify_evidence_line(
            &source_project,
            evidence.field_works_sense_gloss_line,
            &evidence.field_works_sense_gloss_line_text,
            "FieldWorks sense gloss",
        );
        assert_eq!(evidence.field_works_sense_gloss, evidence.hc_xml_gloss);
        assert!(evidence
            .field_works_sense_gloss_line_text
            .contains(&evidence.field_works_sense_gloss));

        let selected_key = match evidence.kind.as_str() {
            "allomorph" => {
                assert!(matches!(
                    evidence.field_works_candidate_class.as_str(),
                    "MoStemAllomorph" | "MoAffixAllomorph"
                ));
                assert_eq!(
                    evidence.hc_xml_form.as_deref(),
                    evidence.field_works_form.as_deref()
                );
                assert!(evidence.paired_allomorphs.is_empty());
                let xml_form_line = evidence.hc_xml_form_line.expect("HC XML form line");
                let xml_form_text = evidence
                    .hc_xml_form_line_text
                    .as_deref()
                    .expect("HC XML form line text");
                verify_evidence_line(&xml_path, xml_form_line, xml_form_text, "HC XML form");
                assert!(xml_form_text.contains(evidence.hc_xml_form.as_deref().unwrap()));
                let fieldworks_form_line = evidence
                    .field_works_form_line
                    .expect("FieldWorks allomorph form line");
                let fieldworks_form_text = evidence
                    .field_works_form_line_text
                    .as_deref()
                    .expect("FieldWorks allomorph form line text");
                verify_evidence_line(
                    &source_project,
                    fieldworks_form_line,
                    fieldworks_form_text,
                    "FieldWorks allomorph form",
                );
                assert!(
                    fieldworks_form_text.contains(evidence.field_works_form.as_deref().unwrap())
                );
                source_allomorph_keys.get(&evidence.source_id)
            }
            "morpheme" => {
                assert!(evidence.field_works_candidate_class.ends_with("Msa"));
                assert!(evidence.hc_xml_form.is_none());
                assert!(evidence.field_works_form.is_none());
                assert!(!evidence.paired_allomorphs.is_empty());
                for paired in &evidence.paired_allomorphs {
                    assert!(matches!(
                        paired.class_name.as_str(),
                        "MoStemAllomorph" | "MoAffixAllomorph"
                    ));
                    verify_evidence_line(
                        &source_project,
                        paired.candidate_line,
                        &paired.candidate_line_text,
                        "paired FieldWorks allomorph",
                    );
                    assert!(paired.candidate_line_text.contains(&paired.guid));
                    verify_evidence_line(
                        &source_project,
                        paired.form_line,
                        &paired.form_line_text,
                        "paired FieldWorks form",
                    );
                    assert!(paired.form_line_text.contains(&paired.form));
                    verify_evidence_line(
                        &source_project,
                        paired.sense_gloss_line,
                        &paired.sense_gloss_line_text,
                        "paired FieldWorks sense gloss",
                    );
                    assert_eq!(paired.sense_gloss, evidence.hc_xml_gloss);
                    assert!(paired.sense_gloss_line_text.contains(&paired.sense_gloss));
                }
                source_morpheme_keys.get(&evidence.source_id)
            }
            other => panic!("{case_name}/{state}: unsupported static source ID kind {other}"),
        };
        assert_eq!(
            selected_key.map(String::as_str),
            Some(evidence.stable_guid.as_str()),
            "{case_name}/{state}/{}: static evidence disagrees with the capture's source-ID crosswalk",
            evidence.source_id
        );
    }
}

fn verify_identity_capture(
    case_path: &Path,
    case_name: &str,
    state: &str,
    engine_provenance: &EngineProvenance,
    repo_root: &Path,
) -> IdentityCapture {
    let measurements = case_path.join("measurements").join(state);
    let capture_path = measurements.join("hc-identity.json");
    let capture: IdentityCapture = serde_json::from_slice(
        &std::fs::read(&capture_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", capture_path.display())),
    )
    .expect("structured C# capture schema");
    let legacy: LegacyHcCapture = serde_json::from_slice(
        &std::fs::read(measurements.join("hc.json")).expect("read original C# capture"),
    )
    .expect("original C# capture schema");

    assert_eq!(capture.schema_version, 3);
    assert_eq!(
        capture.comparison_profile,
        "underdefined-machine-word-analysis/v2"
    );
    assert_eq!(capture.case_name, case_name);
    assert_eq!(capture.state, state);

    let machine = engine_provenance
        .binaries
        .get("SIL.Machine.dll")
        .expect("Machine DLL pin");
    let hermit_crab = engine_provenance
        .binaries
        .get("SIL.Machine.Morphology.HermitCrab.dll")
        .expect("HermitCrab DLL pin");
    assert_eq!(capture.engine_version, hermit_crab.file_version);
    same_hash(
        &capture.provenance.sil_machine_sha256,
        &machine.sha256,
        "SIL.Machine.dll",
    );
    same_hash(
        &capture.provenance.hermit_crab_sha256,
        &hermit_crab.sha256,
        "HermitCrab DLL",
    );
    same_hash(
        &capture.provenance.input_xml_sha256,
        &sha256_file(&measurements.join("Probe.hc.xml")),
        "Probe.hc.xml",
    );
    same_hash(
        &capture.provenance.input_words_sha256,
        &sha256_file(&measurements.join("words.txt")),
        "words.txt",
    );
    same_hash(
        &capture.provenance.legacy_capture_sha256,
        &sha256_file(&measurements.join("hc.json")),
        "hc.json",
    );
    same_hash(
        &capture.provenance.input_invocation_sha256,
        &sha256_file(&measurements.join("parse-hc.invocation.json")),
        "parse-hc.invocation.json",
    );
    same_hash(
        &capture.provenance.harness_source_sha256,
        &sha256_file(&repo_root.join("tools/underdefined-identity-capture/Program.cs")),
        "capture harness source",
    );
    same_hash(
        &capture.provenance.harness_project_sha256,
        &sha256_file(
            &repo_root
                .join("tools/underdefined-identity-capture/UnderdefinedIdentityCapture.csproj"),
        ),
        "capture harness project",
    );
    let words_text =
        std::fs::read_to_string(measurements.join("words.txt")).expect("read measured words");
    let words: Vec<_> = words_text.lines().filter(|word| !word.is_empty()).collect();
    assert_eq!(capture.words.len(), words.len());
    assert_eq!(capture.words.len(), legacy.words.len());
    verify_part_of_speech_crosswalk_evidence(
        &capture,
        &repo_root.join("conformance-staging/underdefined"),
        case_name,
        state,
    );

    let category_ids = capture
        .part_of_speech_symbols
        .iter()
        .map(|symbol| symbol.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(category_ids.len(), capture.part_of_speech_symbols.len());
    let category_guids = capture
        .part_of_speech_symbols
        .iter()
        .map(|symbol| symbol.stable_guid.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(category_guids.len(), capture.part_of_speech_symbols.len());

    let mut source_allomorph_keys = BTreeMap::new();
    let mut source_morpheme_keys = BTreeMap::new();
    let mut source_inflection_type_keys = BTreeMap::new();
    for ((captured, original), expected_word) in capture.words.iter().zip(&legacy.words).zip(words)
    {
        assert_eq!(captured.word, expected_word);
        assert_eq!(captured.word, original.word);
        assert_eq!(captured.engine_error, original.engine_error);
        assert!(
            original.projection_agrees,
            "{case_name}/{state}/{}: saved XML projection did not agree with HCLoader",
            captured.word
        );
        assert_eq!(
            original.projected_engine_error, captured.engine_error,
            "{case_name}/{state}/{}: XML error differs from the saved capture",
            captured.word
        );
        assert_eq!(
            original.projected_analysis_count,
            captured
                .engine_error
                .is_none()
                .then_some(captured.analyses.len()),
            "{case_name}/{state}/{}: XML analysis count differs from the saved capture",
            captured.word
        );
        assert_eq!(captured.analyses.len(), original.analyses.len());

        let original_key_multiset = multiset(
            original
                .analyses
                .iter()
                .map(|analysis| analysis.morphemes.clone()),
        );
        let captured_key_multiset = multiset(
            captured
                .analyses
                .iter()
                .map(|analysis| analysis.stored_analysis_key.clone()),
        );
        assert_eq!(
            captured_key_multiset, original_key_multiset,
            "{case_name}/{state}/{}: projecting the new capture to its stored-key multiset differs from hc.json",
            captured.word
        );
        for analysis in &captured.analyses {
            assert_eq!(
                analysis.identity.morphemes.len(),
                analysis.machine_morpheme_ids.len(),
                "{case_name}/{state}/{}: structured identity id and key counts differ",
                captured.word
            );
            assert_eq!(
                analysis.machine_stored_morph_ids.len(),
                analysis.stored_analysis_key.len(),
                "{case_name}/{state}/{}: replayed morphology and stored-key morph counts differ",
                captured.word
            );
            match (&analysis.machine_category_id, &analysis.identity.category) {
                (Some(machine_id), Some(category)) => {
                    let symbol = capture
                        .part_of_speech_symbols
                        .iter()
                        .find(|symbol| symbol.id == *machine_id)
                        .unwrap_or_else(|| {
                            panic!(
                                "{case_name}/{state}/{}: C# identity names unknown POS id {machine_id}",
                                captured.word
                            )
                        });
                    assert_eq!(
                        category, &symbol.stable_guid,
                        "{case_name}/{state}/{}: C# category is not the saved FieldWorks source key",
                        captured.word
                    );
                    assert!(category_guids.contains(category.as_str()));
                }
                (None, None) => {}
                _ => panic!(
                    "{case_name}/{state}/{}: C# POS source id and stable category presence differ",
                    captured.word
                ),
            }
            for (source_ids, stored_morph) in analysis
                .machine_stored_morph_ids
                .iter()
                .zip(&analysis.stored_analysis_key)
            {
                for (map, source_id, stable_key, kind) in [
                    (
                        &mut source_allomorph_keys,
                        source_ids.allomorph_id.as_str(),
                        stored_morph.allomorph.as_str(),
                        "allomorph",
                    ),
                    (
                        &mut source_morpheme_keys,
                        source_ids.morpheme_id.as_str(),
                        stored_morph.msa.as_str(),
                        "morpheme",
                    ),
                ] {
                    if let Some(previous) = map.insert(source_id.to_owned(), stable_key.to_owned())
                    {
                        assert_eq!(
                            previous, stable_key,
                            "{case_name}/{state}/{}: one C# {kind} id maps to multiple stored keys",
                            captured.word
                        );
                    }
                }
                match (&source_ids.inflection_type_id, &stored_morph.inflection_type) {
                    (Some(source_id), Some(stable_key)) => {
                        if let Some(previous) = source_inflection_type_keys
                            .insert(source_id.clone(), stable_key.clone())
                        {
                            assert_eq!(
                                previous, *stable_key,
                                "{case_name}/{state}/{}: one C# inflection-type id maps to multiple stored keys",
                                captured.word
                            );
                        }
                    }
                    (None, None) => {}
                    _ => panic!(
                        "{case_name}/{state}/{}: replayed inflection-type presence differs from hc.json",
                        captured.word
                    ),
                }
            }
            for (machine_id, stable_key) in analysis
                .machine_morpheme_ids
                .iter()
                .zip(&analysis.identity.morphemes)
            {
                if let Some(previous) =
                    source_morpheme_keys.insert(machine_id.clone(), stable_key.clone())
                {
                    assert_eq!(
                        previous, *stable_key,
                        "{case_name}/{state}/{}: one structured C# morpheme id maps to multiple stable keys",
                        captured.word
                    );
                }
            }
            let projected_identity = analysis
                .machine_morpheme_ids
                .iter()
                .map(|id| {
                    Some(
                        source_morpheme_keys
                            .get(id)
                            .expect("structured Machine morpheme key crosswalk")
                            .clone(),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                projected_identity,
                analysis.identity.morphemes.iter().cloned().map(Some).collect::<Vec<_>>(),
                "{case_name}/{state}/{}: structured identity projection differs from stored source keys",
                captured.word
            );
        }
    }

    let underdefined_root = repo_root.join("conformance-staging/underdefined");
    verify_static_crosswalk_evidence(
        &capture,
        &measurements,
        &underdefined_root,
        case_name,
        state,
        &source_allomorph_keys,
        &source_morpheme_keys,
    );

    capture
}

#[test]
fn every_measured_underdefined_case_preserves_xample_stored_keys() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../conformance-staging/underdefined");
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let engine_provenance: EngineProvenance = serde_json::from_slice(
        &std::fs::read(root.join("engine-provenance.json")).expect("read pinned engine provenance"),
    )
    .expect("engine provenance schema");
    let mut cases = std::fs::read_dir(&root)
        .expect("measured underdefined staging must exist")
        .map(|entry| entry.expect("read case").path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    cases.sort();
    assert!(!cases.is_empty(), "a missing inventory cannot pass");
    let mut failures = Vec::new();
    let mut checked = 0;
    for path in &cases {
        let name = path.file_name().unwrap().to_str().unwrap();
        let text = std::fs::read_to_string(path.join("words.yaml")).expect("read words");
        let provenance = text.lines().next().expect("oracle provenance");
        assert!(provenance.starts_with("# oracle-provenance:"));
        assert!(provenance.contains("XAMPLE") && provenance.contains("Machine/HC"));
        let json = text
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        let case: Case = serde_json::from_str(&json).expect("measured words schema");
        assert_eq!(case.measurement_protocol, "underdefined-stored-keys-v1");

        let mut off_capture = None;
        for state in ["control", "off", "on"] {
            let capture =
                verify_identity_capture(path, name, state, &engine_provenance, &repo_root);
            if state == "off" {
                off_capture = Some(capture);
            }
        }
        let off_capture = off_capture.expect("OFF state identity capture");

        let (snapshot, report) = pg_fwdata::import_file(&path.join("fieldworks/project.fwdata"))
            .expect("import saved project");
        let compiled = pg_grammar::compile::compile_project_with_options_and_import_warnings(
            &snapshot,
            pg_grammar::compile::CompileOptions::default(),
            report.warnings,
        )
        .expect("saved underdefined project must compile without a semantic-loss override");
        let morpher = pg_parse::Morpher::new(&compiled.grammar, 1_000_000);
        let mut observed_differences = Vec::new();
        let mut observed_losses = Vec::new();
        assert_eq!(case.words.len(), off_capture.words.len());
        for (row, csharp_word) in case.words.into_iter().zip(&off_capture.words) {
            checked += 1;
            assert_eq!(row.word, csharp_word.word);
            assert_eq!(row.csharp_hc.engine_error, csharp_word.engine_error);
            assert_eq!(
                multiset(row.csharp_hc.stored_analysis_keys.clone()),
                multiset(
                    csharp_word
                        .analyses
                        .iter()
                        .map(|analysis| analysis.stored_analysis_key.clone())
                ),
                "{name}/{}: OFF C# identity capture does not project to words.yaml key multiset",
                row.word
            );
            assert!(
                row.xample.engine_error.is_none(),
                "XAMPLE errors are not minimum answers"
            );
            let outcome = morpher.parse_word(&row.word);
            assert!(
                !outcome.capped && !outcome.timed_out,
                "{name}/{} incomplete",
                row.word
            );
            let actual_keys = projected_keys(&outcome, &compiled.grammar);
            let csharp = csharp_identities(csharp_word);
            let actual = projected_identities(&outcome, &compiled.grammar);
            let minimum = multiset(row.xample.stored_analysis_keys);
            observed_differences.extend(differences(
                &row.word,
                &csharp,
                &actual,
                row.csharp_hc.engine_error.as_deref(),
                outcome.invalid_shape,
            ));
            eprintln!(
                "UNDERDEFINED_ROW {}",
                serde_json::json!({"case":name,"word":row.word,"pangloss_identities":actual.iter().collect::<Vec<_>>(),"xample_stored_keys":minimum.iter().collect::<Vec<_>>(),"csharp_identities":csharp.iter().collect::<Vec<_>>(),"hc_error":row.csharp_hc.engine_error,"invalid_shape":outcome.invalid_shape})
            );
            for (key, expected) in minimum {
                let count = actual_keys.get(&key).copied().unwrap_or_default();
                if count < expected {
                    observed_losses.push(MinimumLoss {
                        case: name.into(),
                        word: row.word.clone(),
                        key,
                        xample_count: expected,
                        pangloss_count: count,
                    });
                }
            }
        }
        let exceptions: MinimumExceptionTable = serde_json::from_str(
            &std::fs::read_to_string(path.join("xample-minimum-exceptions.json"))
                .expect("each measured case must explicitly list minimum exceptions"),
        )
        .expect("minimum exception table schema");
        assert_eq!(exceptions.schema_version, 1);
        let expected_losses = exceptions
            .exceptions
            .into_iter()
            .map(|row| {
                assert_eq!(row.loss.case, name);
                assert!(!row.reason.trim().is_empty());
                assert_eq!(row.ledger, "078-xample-minimum-authored-phonology.md");
                assert!(root
                    .join("../../docs/divergences")
                    .join(row.ledger)
                    .is_file());
                row.loss
            })
            .collect::<Vec<_>>();
        if let Err(error) = check_minimum_losses(&observed_losses, &expected_losses) {
            failures.push(format!("{name}: {error}"));
        }
        eprintln!(
            "UNDERDEFINED_V2_DIFFERENCES {} {}",
            name,
            serde_json::to_string(&observed_differences).expect("serialize v2 differences")
        );
        let table_path = path.join("expected-differences.json");
        let table_value: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&table_path)
                .expect("each measured case must have an explicit difference table"),
        )
        .expect("difference table JSON");
        if table_value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            != Some(2)
            || table_value
                .get("comparison_profile")
                .and_then(serde_json::Value::as_str)
                != Some("underdefined-structured-identity-and-status/v2")
        {
            failures.push(format!(
                "{name}: expected-differences.json is not comparison profile v2"
            ));
            continue;
        }
        let table: DifferenceTable =
            serde_json::from_value(table_value).expect("v2 difference table schema");
        assert_eq!(table.schema_version, 2);
        assert_eq!(
            table.comparison_profile,
            "underdefined-structured-identity-and-status/v2"
        );
        let expected = table
            .expected_differences
            .into_iter()
            .map(|row| {
                assert!(!row.reason.trim().is_empty());
                assert!(!row.ledger.contains('/') && !row.ledger.contains('\\'));
                assert!(
                    root.join("../../docs/divergences")
                        .join(&row.ledger)
                        .is_file(),
                    "difference must name a real ledger entry"
                );
                row.difference
            })
            .collect::<Vec<_>>();
        if let Err(error) = check_differences(&observed_differences, &expected) {
            failures.push(format!("{name}: {error}"));
        }
    }
    eprintln!("Measured {} cases and {checked} words", cases.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn difference_tables_reject_unlisted_disappeared_and_changed_identity_fields() {
    let identity = AnalysisIdentity {
        morphemes: vec![Some("msa".into()), Some("affix".into())],
        root_index: 0,
        category: Some("noun".into()),
    };
    let csharp = std::collections::BTreeSet::from([identity.clone()]);
    for field in ["morphemes", "root_index", "category"] {
        let mut changed = identity.clone();
        match field {
            "morphemes" => changed.morphemes[1] = Some("other-affix".into()),
            "root_index" => changed.root_index = 1,
            "category" => changed.category = Some("verb".into()),
            _ => unreachable!(),
        }
        let observed = differences(
            "probe",
            &csharp,
            &std::collections::BTreeSet::from([identity.clone(), changed]),
            None,
            false,
        );
        assert_eq!(observed.len(), 1);
        assert!(
            check_differences(&observed, &[])
                .unwrap_err()
                .contains("unlisted"),
            "{field} difference must be unlisted without an expected row"
        );
        assert!(check_differences(&observed, &observed).is_ok());

        let mut altered_expected = observed.clone();
        let expected_identity = altered_expected[0].identity.as_mut().unwrap();
        match field {
            "morphemes" => expected_identity.morphemes[1] = Some("wrong-affix".into()),
            "root_index" => expected_identity.root_index += 1,
            "category" => expected_identity.category = Some("wrong-category".into()),
            _ => unreachable!(),
        }
        assert!(
            check_differences(&observed, &altered_expected).is_err(),
            "changing {field} in the expected v2 row must fail"
        );
        assert!(
            check_differences(&observed[1..], &observed)
                .unwrap_err()
                .contains("unlisted"),
            "removing a v2 expected row must report an unlisted identity"
        );
        assert!(check_differences(&[], &observed).is_err());
    }

    let status = differences("probe", &csharp, &csharp, Some("engine failure"), false);
    assert!(check_differences(&status, &[]).is_err());
    assert!(check_differences(&[], &status).is_err());
    let duplicate = [status[0].clone(), status[0].clone()];
    assert!(check_differences(&status, &duplicate).is_err());
}
