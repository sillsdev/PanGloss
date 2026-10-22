//! End-to-end Rust conformance for FieldWorks `<HC><MergeMSAs>` from project import through parse identity.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use pg_parse::{AnalysisIdentity, Morpher, ParseOptions};
use serde::{Deserialize, Serialize};

const PROJECT: &str = include_str!("data/merge-msas/project.fwdata");
const EXPECTATIONS: &str = include_str!("data/merge-msas/expected.json");
const PROFILE: &str = pg_parse::IDENTITY_PROFILE;
const SETTING_ANCHOR: &str = "&lt;NotOnClitics&gt;false&lt;/NotOnClitics&gt;";

#[derive(Debug, Clone, Copy)]
enum MergeSetting {
    Absent,
    Enabled,
    Disabled,
}

impl MergeSetting {
    fn name(self) -> &'static str {
        match self {
            Self::Absent => "default",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }

    fn expected_value(self) -> bool {
        !matches!(self, Self::Disabled)
    }

    fn materialize(self) -> String {
        assert!(
            !PROJECT.contains("&lt;MergeMSAs&gt;"),
            "the checked-in source must leave MergeMSAs absent"
        );
        assert_eq!(PROJECT.matches(SETTING_ANCHOR).count(), 1);
        match self {
            Self::Absent => PROJECT.to_string(),
            Self::Enabled | Self::Disabled => {
                let value = if matches!(self, Self::Enabled) {
                    "true"
                } else {
                    "false"
                };
                PROJECT.replacen(
                    SETTING_ANCHOR,
                    &format!("{SETTING_ANCHOR}&lt;MergeMSAs&gt;{value}&lt;/MergeMSAs&gt;"),
                    1,
                )
            }
        }
    }
}

struct TempProjectDir(PathBuf);

impl TempProjectDir {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "pangloss-merge-msas-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&path)
            .unwrap_or_else(|e| panic!("create temporary project dir {}: {e}", path.display()));
        Self(path)
    }

    fn write_project(&self, setting: MergeSetting) -> PathBuf {
        let path = self.0.join(format!("{}.fwdata", setting.name()));
        std::fs::write(&path, setting.materialize())
            .unwrap_or_else(|e| panic!("write materialized project {}: {e}", path.display()));
        path
    }
}

impl Drop for TempProjectDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum WordStatus {
    Parsed,
    NoAnalysis,
    InvalidShape,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WordExpectation {
    status: WordStatus,
    capped: bool,
    timed_out: bool,
    guessed: bool,
    identities: Vec<AnalysisIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunExpectation {
    merge_msas: bool,
    words: BTreeMap<String, WordExpectation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expectations {
    profile: String,
    runs: BTreeMap<String, RunExpectation>,
}

fn observe(setting: MergeSetting) -> RunExpectation {
    let temp = TempProjectDir::new();
    let project_path = temp.write_project(setting);
    let (snapshot, _report) = pg_fwdata::import_file(&project_path)
        .unwrap_or_else(|e| panic!("{} import failed: {e}", setting.name()));
    let fatal_import_issues: Vec<_> = snapshot
        .conversion_provenance
        .import_issues
        .iter()
        .filter(|issue| issue.fatal)
        .collect();
    assert!(
        fatal_import_issues.is_empty(),
        "{} import must have no fatal source issues: {fatal_import_issues:#?}",
        setting.name()
    );
    let equality = snapshot
        .lexicon
        .msa_equality
        .as_ref()
        .expect("project import records raw MSA equality");
    let equal_guids = |guid: &str| {
        equality
            .iter()
            .find(|row| row.msa == guid)
            .map(|row| row.equal_to.as_slice())
            .unwrap_or(&[])
    };
    assert_eq!(
        equal_guids("00000000-0000-0000-0000-0000000000e4"),
        ["00000000-0000-0000-0000-000000000042"],
        "the earlier sense-less Verb MSA must equal the later sense-bearing MSA"
    );
    assert_eq!(
        equal_guids("00000000-0000-0000-0000-000000000042"),
        ["00000000-0000-0000-0000-0000000000e4"],
        "the raw equality relation must include both directed comparisons"
    );
    assert!(
        equal_guids("00000000-0000-0000-0000-0000000000e5").is_empty(),
        "the Noun control must remain non-equal to the Verb MSAs"
    );
    assert_eq!(
        snapshot.morphology.parser_parameters.merge_msas,
        setting.expected_value(),
        "{} MergeMSAs value must be applied before import",
        setting.name()
    );
    let (grammar, _warnings) = pg_grammar::compile_project(&snapshot)
        .unwrap_or_else(|e| panic!("{} compile failed: {e}", setting.name()));
    let morpher = Morpher::new(&grammar, usize::MAX);
    let opts = ParseOptions::default();
    let mut words = BTreeMap::new();
    for word in ["ranna", "rann", "?"] {
        let outcome = morpher.parse_word_opts(word, &opts);
        let mut identities: Vec<_> = outcome
            .structured
            .iter()
            .map(|analysis| {
                AnalysisIdentity::project(analysis, &grammar)
                    .unwrap_or_else(|e| panic!("{word:?} identity projection failed: {e}"))
            })
            .collect();
        identities.sort();
        let status = if outcome.invalid_shape {
            WordStatus::InvalidShape
        } else if identities.is_empty() {
            WordStatus::NoAnalysis
        } else {
            WordStatus::Parsed
        };
        assert!(!outcome.capped, "{word:?} must finish without capping");
        assert!(
            !outcome.timed_out,
            "{word:?} must finish without timing out"
        );
        assert!(
            !outcome.guessed,
            "{word:?} must not return guessed analyses"
        );
        words.insert(
            word.to_string(),
            WordExpectation {
                status,
                capped: outcome.capped,
                timed_out: outcome.timed_out,
                guessed: outcome.guessed,
                identities,
            },
        );
    }
    RunExpectation {
        merge_msas: setting.expected_value(),
        words,
    }
}

#[test]
fn imported_project_merge_msas_matches_complete_identity_multisets() {
    let actual = Expectations {
        profile: PROFILE.to_string(),
        runs: [
            (
                MergeSetting::Absent.name().to_string(),
                observe(MergeSetting::Absent),
            ),
            (
                MergeSetting::Enabled.name().to_string(),
                observe(MergeSetting::Enabled),
            ),
            (
                MergeSetting::Disabled.name().to_string(),
                observe(MergeSetting::Disabled),
            ),
        ]
        .into_iter()
        .collect(),
    };
    eprintln!(
        "MERGE_MSAS_ACTUAL_JSON_BEGIN\n{}\nMERGE_MSAS_ACTUAL_JSON_END",
        serde_json::to_string_pretty(&actual).expect("observations serialize")
    );
    let expected: Expectations = serde_json::from_str(EXPECTATIONS)
        .expect("merge-msas expected.json is valid and matches the identity schema");
    assert_eq!(expected.profile, PROFILE, "fixture identity profile");
    assert_eq!(
        actual, expected,
        "complete import/compile/parse output changed"
    );
}
