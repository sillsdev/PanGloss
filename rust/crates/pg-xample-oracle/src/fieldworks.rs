//! Locates and invokes `tools/xample-projector`'s managed helper, parses its JSON responses, and
//! compares its generated XAMPLE files across a mutation.
//!
//! This crate never links FieldWorks/LibLCM itself (see `crate` doc's "Deviation from the original
//! plan"): every fact here comes from running the helper's `project`/`mutate`/`parse` subcommands
//! and reading their JSON, per `tools/xample-projector/README.md`'s own contract.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;
use serde::Deserialize;

use crate::reader::{read_parse_response, ParsedParseResponse, ReadError};

/// Overrides which `machine` checkout holds the FieldWorks witness (`fieldworks/project.fwdata` +
/// `fieldworks/phonology-mutations.yaml` beside `edge-cases/deep-optional-affix-nesting`). This is
/// deliberately NOT the `machine` git submodule pinned in this worktree -- see this crate's own
/// gate test doc for why.
pub const MACHINE_DIR_ENV: &str = "PANGLOSS_MACHINE_DIR";

/// Overrides the built `XampleProjector.exe` path, bypassing the default
/// `tools/xample-projector/bin/Debug/XampleProjector.exe` this repo's `build.ps1` produces.
pub const PROJECTOR_EXE_ENV: &str = "PANGLOSS_XAMPLE_PROJECTOR_EXE";

/// Set to downgrade a missing FieldWorks/XAMPLE witness from a hard failure to a loud, named skip.
/// Never the default: see this crate's gate test doc for the skip policy this implements.
pub const ALLOW_NO_FIELDWORKS_ENV: &str = "PANGLOSS_ALLOW_NO_FIELDWORKS";

// A second eligible fixture would add a second named path here, not a parameter threaded through every function.
const WITNESS_CATEGORY: &str = "edge-cases";
const WITNESS_NAME: &str = "deep-optional-affix-nesting";

pub fn machine_dir() -> PathBuf {
    std::env::var(MACHINE_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"C:\Users\johnm\Documents\repos\machine"))
}

/// `<machine_dir>/conformance/edge-cases/deep-optional-affix-nesting/fieldworks`.
pub fn witness_dir(machine_dir: &Path) -> PathBuf {
    machine_dir
        .join("conformance")
        .join(WITNESS_CATEGORY)
        .join(WITNESS_NAME)
        .join("fieldworks")
}

/// `<machine_dir>/conformance/edge-cases/deep-optional-affix-nesting/grammar.xml` -- the checked-in
/// HC-native fixture this witness project must never semantically diverge from.
pub fn witness_grammar_xml(machine_dir: &Path) -> PathBuf {
    machine_dir
        .join("conformance")
        .join(WITNESS_CATEGORY)
        .join(WITNESS_NAME)
        .join("grammar.xml")
}

/// This crate's own root, two levels up from `rust/crates/pg-xample-oracle`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Locates a built `XampleProjector.exe`, honouring [`PROJECTOR_EXE_ENV`]. Never invokes `dotnet`
/// or MSBuild itself -- `tools/xample-projector/build.ps1 -Mode test` is the one place that builds
/// it, so a missing exe here means the build step was skipped, not that this crate should
/// silently build one.
pub fn locate_projector_exe() -> Result<PathBuf, FieldworksError> {
    if let Ok(over) = std::env::var(PROJECTOR_EXE_ENV) {
        let path = PathBuf::from(&over);
        if path.is_file() {
            return Ok(path);
        }
        return Err(FieldworksError::ExeNotFound {
            checked: path,
            build_hint: format!("{PROJECTOR_EXE_ENV} is set but does not name an existing file"),
        });
    }
    let default_path = repo_root()
        .join("tools")
        .join("xample-projector")
        .join("bin")
        .join("Debug")
        .join("XampleProjector.exe");
    if default_path.is_file() {
        return Ok(default_path);
    }
    Err(FieldworksError::ExeNotFound {
        checked: default_path,
        build_hint: r"build it first: & .\tools\xample-projector\build.ps1 -Mode test".to_string(),
    })
}

#[derive(Debug)]
pub enum FieldworksError {
    ExeNotFound {
        checked: PathBuf,
        build_hint: String,
    },
    Spawn {
        exe: PathBuf,
        source: std::io::Error,
    },
    NonZeroExit {
        command: String,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    ParseResponse(ReadError),
}

impl std::fmt::Display for FieldworksError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldworksError::ExeNotFound {
                checked,
                build_hint,
            } => {
                write!(
                    f,
                    "no XampleProjector.exe at {} -- {build_hint}",
                    checked.display()
                )
            }
            FieldworksError::Spawn { exe, source } => {
                write!(f, "failed to run {}: {source}", exe.display())
            }
            FieldworksError::NonZeroExit {
                command,
                exit_code,
                stdout,
                stderr,
            } => write!(
                f,
                "'{command}' exited {exit_code:?}\nstdout:\n{stdout}\nstderr:\n{stderr}"
            ),
            FieldworksError::Io { path, source } => write!(f, "{}: {source}", path.display()),
            FieldworksError::Json { path, source } => {
                write!(f, "{}: malformed JSON: {source}", path.display())
            }
            FieldworksError::ParseResponse(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for FieldworksError {}

fn path_str(p: &Path) -> &str {
    p.to_str()
        .unwrap_or_else(|| panic!("path is not valid UTF-8: {}", p.display()))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, FieldworksError> {
    let text = std::fs::read_to_string(path).map_err(|source| FieldworksError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| FieldworksError::Json {
        path: path.to_path_buf(),
        source,
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedFileEntry {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub deterministic: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HcLoadDiagnostic {
    pub kind: String,
    pub message: String,
}

/// `project`'s `response.json` (`tools/xample-projector/README.md`'s `project` contract).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectResponse {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    pub mode: String,
    #[serde(rename = "fieldWorksVersion")]
    pub field_works_version: String,
    #[serde(rename = "assemblyVersions")]
    pub assembly_versions: serde_json::Value,
    #[serde(rename = "sourcePath")]
    pub source_path: String,
    #[serde(rename = "sourceSha256")]
    pub source_sha256: String,
    pub database: String,
    pub generated: Vec<GeneratedFileEntry>,
    #[serde(rename = "hcLoadDiagnostics")]
    pub hc_load_diagnostics: Vec<HcLoadDiagnostic>,
    pub diagnostics: Vec<String>,
}

impl ProjectResponse {
    pub fn generated_sha256(&self, relative_path: &str) -> Option<&str> {
        self.generated
            .iter()
            .find(|g| g.path == relative_path)
            .map(|g| g.sha256.as_str())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemovedPhoneme {
    pub guid: String,
    pub representations: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboundReference {
    #[serde(rename = "targetGuid")]
    pub target_guid: String,
    #[serde(rename = "referrerGuid")]
    pub referrer_guid: String,
    #[serde(rename = "referrerClass")]
    pub referrer_class: String,
}

/// `mutate`'s `mutation-response.json` (`tools/xample-projector/README.md`'s `mutate` contract).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutateResponse {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    pub mode: String,
    #[serde(rename = "caseId")]
    pub case_id: String,
    #[serde(rename = "baseSha256")]
    pub base_sha256: String,
    #[serde(rename = "materializedSha256")]
    pub materialized_sha256: String,
    #[serde(rename = "materializedProjectPath")]
    pub materialized_project_path: String,
    pub removed: Vec<RemovedPhoneme>,
    #[serde(rename = "inboundReferences")]
    pub inbound_references: Vec<InboundReference>,
    pub reopened: bool,
    #[serde(rename = "deletedCount")]
    pub deleted_count: u64,
    pub diagnostics: Vec<String>,
}

/// A located, invokable `XampleProjector.exe`.
pub struct Projector {
    exe_path: PathBuf,
}

impl Projector {
    pub fn locate() -> Result<Self, FieldworksError> {
        Ok(Self {
            exe_path: locate_projector_exe()?,
        })
    }

    pub fn exe_path(&self) -> &Path {
        &self.exe_path
    }

    fn run(&self, args: &[&str]) -> Result<std::process::Output, FieldworksError> {
        Command::new(&self.exe_path)
            .args(args)
            .output()
            .map_err(|source| FieldworksError::Spawn {
                exe: self.exe_path.clone(),
                source,
            })
    }

    fn run_ok(&self, args: &[&str], label: &str) -> Result<(), FieldworksError> {
        let output = self.run(args)?;
        if !output.status.success() {
            return Err(FieldworksError::NonZeroExit {
                command: label.to_string(),
                exit_code: output.status.code(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(())
    }

    /// Runs `project --project <project_fwdata> --out-dir <out_dir> --database <database>`.
    pub fn project(
        &self,
        project_fwdata: &Path,
        out_dir: &Path,
        database: &str,
    ) -> Result<ProjectResponse, FieldworksError> {
        std::fs::create_dir_all(out_dir).map_err(|source| FieldworksError::Io {
            path: out_dir.to_path_buf(),
            source,
        })?;
        self.run_ok(
            &[
                "project",
                "--project",
                path_str(project_fwdata),
                "--out-dir",
                path_str(out_dir),
                "--database",
                database,
            ],
            "project",
        )?;
        read_json(&out_dir.join("response.json"))
    }

    /// Runs `mutate --project <project_fwdata> --request <request> --out-dir <out_dir>`, writing
    /// `request` (built by `crate::fixture::MutationCase::to_request_json`) to a temp file first.
    pub fn mutate(
        &self,
        project_fwdata: &Path,
        request: &serde_json::Value,
        out_dir: &Path,
    ) -> Result<MutateResponse, FieldworksError> {
        std::fs::create_dir_all(out_dir).map_err(|source| FieldworksError::Io {
            path: out_dir.to_path_buf(),
            source,
        })?;
        let request_path = out_dir.join("mutation-request.json");
        let request_text =
            serde_json::to_string_pretty(request).map_err(|source| FieldworksError::Json {
                path: request_path.clone(),
                source,
            })?;
        std::fs::write(&request_path, request_text).map_err(|source| FieldworksError::Io {
            path: request_path.clone(),
            source,
        })?;
        self.run_ok(
            &[
                "mutate",
                "--project",
                path_str(project_fwdata),
                "--request",
                path_str(&request_path),
                "--out-dir",
                path_str(out_dir),
            ],
            "mutate",
        )?;
        read_json(&out_dir.join("mutation-response.json"))
    }

    /// Runs `parse --project <project_fwdata> --project-dir <project_dir> --database <database>
    /// --words <words_path> --out <out_path> --max-analyses <max_analyses>`, writing `words`
    /// (one per line) to `out_path`'s directory first.
    pub fn parse(
        &self,
        project_fwdata: &Path,
        project_dir: &Path,
        database: &str,
        words: &[String],
        out_path: &Path,
        max_analyses: u32,
    ) -> Result<ParsedParseResponse, FieldworksError> {
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| FieldworksError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let words_path = out_path.with_extension("words.txt");
        std::fs::write(&words_path, words.join("\n")).map_err(|source| FieldworksError::Io {
            path: words_path.clone(),
            source,
        })?;
        let max_analyses_text = max_analyses.to_string();
        self.run_ok(
            &[
                "parse",
                "--project",
                path_str(project_fwdata),
                "--project-dir",
                path_str(project_dir),
                "--database",
                database,
                "--words",
                path_str(&words_path),
                "--out",
                path_str(out_path),
                "--max-analyses",
                &max_analyses_text,
            ],
            "parse",
        )?;
        let text = std::fs::read_to_string(out_path).map_err(|source| FieldworksError::Io {
            path: out_path.to_path_buf(),
            source,
        })?;
        read_parse_response(&text).map_err(FieldworksError::ParseResponse)
    }
}

// hvo canonicalization for XAMPLE-file comparison -- see `xample_files_equivalent_ignoring_hvo_renumbering`'s own doc.

// The closed set of "morphotactic name" prefixes gram.txt/adctl.txt glue an hvo onto with no separator.
const HVO_IDENTIFIER_PREFIXES: &[&str] = &[
    "IrregInflFormInSlot",
    "IrregInflForm",
    "DefaultExcpFeatures",
    "FromExcpFeat",
    "ToExcpFeat",
    "ExcpFeat",
    "FromInflClass",
    "ToInflClass",
    "InflClass",
    "FromMSFS",
    "ToMSFS",
    "MSEnvFS",
    "MSFS",
    "InflectionFS",
    "FromPOS",
    "ToPOS",
    "RootPOS",
    "MSEnvPOS",
    "CliticPOS",
    "CFP",
    "StemName",
    "ICA",
];

// (pattern, capture-group index carrying the hvo digits) -- everything else in each match is literal context, copied through unchanged.
struct HvoCanonicalizer {
    prefix_id: Regex,
    paren_slot: Regex,
    angle_slot: Regex,
    syn_cat: Regex,
    role_cat: Regex,
    template: Regex,
    lx: Regex,
    wc: Regex,
    allomorph: Regex,
}

impl HvoCanonicalizer {
    fn new() -> Self {
        let alternation = HVO_IDENTIFIER_PREFIXES.join("|");
        Self {
            prefix_id: Regex::new(&format!(r"\b({alternation})(\d+)\b")).expect("static pattern"),
            paren_slot: Regex::new(r"\((\d+)(_\d+)\)").expect("static pattern"),
            angle_slot: Regex::new(r"<(\d+)(_\d+) ").expect("static pattern"),
            syn_cat: Regex::new(r"(synCat>\s*=\s*)(\d+)(\s*)$").expect("static pattern"),
            role_cat: Regex::new(r"((?:root|from|to|env)Cat:)(\d+)").expect("static pattern"),
            template: Regex::new(r"(template\s*)(\d+)(\})").expect("static pattern"),
            lx: Regex::new(r"^(\\lx )(\d+)\s*$").expect("static pattern"),
            wc: Regex::new(r"^(\\wc )(\d+)\s*$").expect("static pattern"),
            allomorph: Regex::new(r"^(\\a \S+ \{)(\d+)(\})").expect("static pattern"),
        }
    }

    // Replaces every match's hvo capture group with its canonical index in `map`, leaving the rest of the match and all surrounding text untouched.
    fn canonicalize_one(
        re: &Regex,
        group: usize,
        text: &str,
        map: &mut HashMap<String, usize>,
        next_id: &mut usize,
    ) -> String {
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for caps in re.captures_iter(text) {
            let whole = caps.get(0).expect("group 0 always matches");
            let hvo = caps
                .get(group)
                .expect("hvo group always present when the pattern matches");
            out.push_str(&text[last..whole.start()]);
            out.push_str(&text[whole.start()..hvo.start()]);
            let canon = *map.entry(hvo.as_str().to_string()).or_insert_with(|| {
                let id = *next_id;
                *next_id += 1;
                id
            });
            out.push_str(&canon.to_string());
            out.push_str(&text[hvo.end()..whole.end()]);
            last = whole.end();
        }
        out.push_str(&text[last..]);
        out
    }

    // One file's canonicalized lines; map/next_id are threaded through every line, never reset, so "first appearance" spans the whole file.
    fn canonicalize_text(&self, text: &str) -> Vec<String> {
        let mut map = HashMap::new();
        let mut next_id = 0usize;
        text.lines()
            .map(|line| {
                let s = Self::canonicalize_one(&self.prefix_id, 2, line, &mut map, &mut next_id);
                let s = Self::canonicalize_one(&self.paren_slot, 1, &s, &mut map, &mut next_id);
                let s = Self::canonicalize_one(&self.angle_slot, 1, &s, &mut map, &mut next_id);
                let s = Self::canonicalize_one(&self.syn_cat, 2, &s, &mut map, &mut next_id);
                let s = Self::canonicalize_one(&self.role_cat, 2, &s, &mut map, &mut next_id);
                let s = Self::canonicalize_one(&self.template, 2, &s, &mut map, &mut next_id);
                let s = Self::canonicalize_one(&self.lx, 2, &s, &mut map, &mut next_id);
                let s = Self::canonicalize_one(&self.wc, 2, &s, &mut map, &mut next_id);
                Self::canonicalize_one(&self.allomorph, 2, &s, &mut map, &mut next_id)
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct FileComparisonMismatch {
    pub file_name: String,
    pub detail: String,
}

impl std::fmt::Display for FileComparisonMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.file_name, self.detail)
    }
}

/// Compares `<database>adctl.txt`/`gram.txt`/`lex.txt` between `base_dir` and `clone_dir`: exact
/// byte identity if present, else equal after CANONICALIZING only the hvo-bearing token shapes
/// below (never every digit). Returns every file that still differs after canonicalization.
///
/// FieldWorks' M3 dump writes every referenced object by its raw, per-load-session hvo
/// (`SIL.LCModel.DomainServices.M3ModelExportServices.cs`), and deleting one phoneme record shifts
/// every later object's hvo within a reopened clone's own load session -- baked in by
/// `Src\Transforms\Application\FxtM3ParserToXAmpleADCtl.xsl`'s `bMorphnameIsMsaId=y` default, not
/// something this crate introduces (`tools/xample-projector/README.md`'s "Comparing XAMPLE outputs
/// across a mutation" section documents the same fact; the closed set of hvo-bearing token shapes
/// below ports that file's own `Get-HvoBlindLines` rather than re-deriving it, and
/// `hvo_identifier_prefixes_match_build_ps1_verbatim` checks that port against the source text so
/// the two cannot silently drift apart).
///
/// The comparison CANONICALIZES rather than blinds: each distinct raw hvo digit string is mapped to
/// its own index in order of first appearance, independently per file (`HvoCanonicalizer::
/// canonicalize_text`), so a consistent renumbering (every reference to the same real object
/// shifted by the same constant delta) normalizes to an identical index sequence on both sides,
/// while a single occurrence changed to an unrelated value lands at a different index -- or
/// introduces a brand-new one -- because it disagrees with the other, uncorrupted occurrences of
/// the SAME real object elsewhere in the same file. Blinding every hvo to one placeholder cannot
/// tell these apart, since both collapse to the identical placeholder; canonicalization can,
/// because it preserves the "same object" relation an all-or-nothing blind throws away. Never every
/// digit, which would also erase a real regression in a `\maxp`/`\maxs`/`\maxi`/`\maxr`/`\maxn`/
/// `\maxnull` cap or a count (`a_corrupted_cap_is_still_caught_after_canonicalization` pins this).
pub fn xample_files_equivalent_ignoring_hvo_renumbering(
    base_dir: &Path,
    clone_dir: &Path,
    database: &str,
) -> Result<(), Vec<FileComparisonMismatch>> {
    let patterns = HvoCanonicalizer::new();
    let mut mismatches = Vec::new();
    for name in ["adctl.txt", "gram.txt", "lex.txt"] {
        let file_name = format!("{database}{name}");
        let base_path = base_dir.join(&file_name);
        let clone_path = clone_dir.join(&file_name);
        let base_text = match std::fs::read_to_string(&base_path) {
            Ok(t) => t,
            Err(e) => {
                mismatches.push(FileComparisonMismatch {
                    file_name: file_name.clone(),
                    detail: format!("could not read base file {}: {e}", base_path.display()),
                });
                continue;
            }
        };
        let clone_text = match std::fs::read_to_string(&clone_path) {
            Ok(t) => t,
            Err(e) => {
                mismatches.push(FileComparisonMismatch {
                    file_name: file_name.clone(),
                    detail: format!("could not read clone file {}: {e}", clone_path.display()),
                });
                continue;
            }
        };
        if base_text == clone_text {
            continue;
        }
        let base_canon = patterns.canonicalize_text(&base_text);
        let clone_canon = patterns.canonicalize_text(&clone_text);
        if base_canon != clone_canon {
            let first_diff = base_canon
                .iter()
                .zip(clone_canon.iter())
                .enumerate()
                .find(|(_, (a, b))| a != b)
                .map(|(i, (a, b))| format!("first differing line {}: {a:?} vs {b:?}", i + 1))
                .unwrap_or_else(|| {
                    format!(
                        "line count differs: base={} clone={}",
                        base_canon.len(),
                        clone_canon.len()
                    )
                });
            mismatches.push(FileComparisonMismatch {
                file_name,
                detail: format!("differs beyond hvo renumbering ({first_diff})"),
            });
        }
    }
    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(mismatches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn witness_dir_matches_the_verified_path() {
        // Composed, not spelled: a literal pinned this to one machine's drive letter and separators.
        let base = Path::new("machine");
        assert_eq!(
            witness_dir(base),
            base.join("conformance")
                .join("edge-cases")
                .join("deep-optional-affix-nesting")
                .join("fieldworks")
        );
    }

    #[test]
    fn locate_projector_exe_env_override_refuses_a_missing_path() {
        std::env::set_var(PROJECTOR_EXE_ENV, r"Z:\definitely\does\not\exist.exe");
        let err = locate_projector_exe().expect_err("a nonexistent override path must be refused");
        assert!(matches!(err, FieldworksError::ExeNotFound { .. }));
        std::env::remove_var(PROJECTOR_EXE_ENV);
    }

    #[test]
    fn identical_files_compare_equal_without_any_canonicalization() {
        let dir = std::env::temp_dir().join(format!(
            "pg-xample-oracle-cmp-identical-{}",
            std::process::id()
        ));
        let base = dir.join("base");
        let clone = dir.join("clone");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&clone).unwrap();
        std::fs::write(base.join("MPBaseadctl.txt"), "\\maxp 12\n").unwrap();
        std::fs::write(clone.join("MPBaseadctl.txt"), "\\maxp 12\n").unwrap();
        std::fs::write(base.join("MPBasegram.txt"), "rule {template 5}\n").unwrap();
        std::fs::write(clone.join("MPBasegram.txt"), "rule {template 5}\n").unwrap();
        std::fs::write(base.join("MPBaselex.txt"), "\\lx 7\n").unwrap();
        std::fs::write(clone.join("MPBaselex.txt"), "\\lx 7\n").unwrap();
        xample_files_equivalent_ignoring_hvo_renumbering(&base, &clone, "MPBase")
            .expect("byte-identical files must compare equal");
        std::fs::remove_dir_all(&dir).ok();
    }

    // Writes a byte-identical adctl/gram/lex triplet into both dirs; the caller overwrites whichever one it means to perturb.
    fn write_matching_triplet(base: &Path, clone: &Path, database: &str) {
        for name in ["adctl.txt", "gram.txt", "lex.txt"] {
            let text = format!("-- unperturbed {name} --\n");
            std::fs::write(base.join(format!("{database}{name}")), &text).unwrap();
            std::fs::write(clone.join(format!("{database}{name}")), &text).unwrap();
        }
    }

    #[test]
    fn hvo_renumbered_files_compare_equal_after_canonicalization() {
        let dir = std::env::temp_dir().join(format!(
            "pg-xample-oracle-cmp-hvocanon-{}",
            std::process::id()
        ));
        let base = dir.join("base");
        let clone = dir.join("clone");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&clone).unwrap();
        write_matching_triplet(&base, &clone, "MPBase");
        // RootPOS106 -> RootPOS94 after a deletion shifts every later hvo; the surrounding text is unchanged.
        std::fs::write(
            base.join("MPBasegram.txt"),
            "rule { rootCat:106 template 5}\nRootPOS106\n\\wc 106\n(106_2)\n",
        )
        .unwrap();
        std::fs::write(
            clone.join("MPBasegram.txt"),
            "rule { rootCat:94 template 5}\nRootPOS94\n\\wc 94\n(94_2)\n",
        )
        .unwrap();
        xample_files_equivalent_ignoring_hvo_renumbering(&base, &clone, "MPBase")
            .expect("a file differing only by a consistent hvo renumbering must compare equal after canonicalization");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_corrupted_cap_is_still_caught_after_canonicalization() {
        // Falsification: an over-broad raw-\d+ blind would also erase this real \maxp regression.
        let dir = std::env::temp_dir().join(format!(
            "pg-xample-oracle-cmp-corrupt-{}",
            std::process::id()
        ));
        let base = dir.join("base");
        let clone = dir.join("clone");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&clone).unwrap();
        write_matching_triplet(&base, &clone, "MPBase");
        std::fs::write(base.join("MPBaseadctl.txt"), "\\maxp 12\nRootPOS106\n").unwrap();
        std::fs::write(clone.join("MPBaseadctl.txt"), "\\maxp 3\nRootPOS94\n").unwrap();
        let mismatches = xample_files_equivalent_ignoring_hvo_renumbering(&base, &clone, "MPBase")
            .expect_err("a corrupted \\maxp cap must not be canonicalized away");
        assert_eq!(
            mismatches.len(),
            1,
            "only adctl.txt should differ: {mismatches:?}"
        );
        assert_eq!(mismatches[0].file_name, "MPBaseadctl.txt");
    }

    // A wrong-category assignment, not a renumbering: RootPOS/rootCat still resolve to index 0, the corrupted \wc value is a brand-new one.
    #[test]
    fn wc_field_corruption_is_caught() {
        let dir = std::env::temp_dir().join(format!(
            "pg-xample-oracle-cmp-wc-corrupt-{}",
            std::process::id()
        ));
        let base = dir.join("base");
        let clone = dir.join("clone");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&clone).unwrap();
        write_matching_triplet(&base, &clone, "MPBase");
        std::fs::write(
            base.join("MPBasegram.txt"),
            "rule { rootCat:106 template 5}\nRootPOS106\n\\wc 106\n",
        )
        .unwrap();
        // rootCat/RootPOS correctly renumbered to 94; \wc wrongly set to an unrelated 999.
        std::fs::write(
            clone.join("MPBasegram.txt"),
            "rule { rootCat:94 template 5}\nRootPOS94\n\\wc 999\n",
        )
        .unwrap();
        let mismatches = xample_files_equivalent_ignoring_hvo_renumbering(&base, &clone, "MPBase")
            .expect_err(
                "a \\wc value inconsistent with the rest of the file's renumbering must be caught",
            );
        assert_eq!(
            mismatches.len(),
            1,
            "only gram.txt should differ: {mismatches:?}"
        );
        assert_eq!(mismatches[0].file_name, "MPBasegram.txt");
    }

    // A mis-wired reference at the same slot index, not a renumbering.
    #[test]
    fn paren_slot_corruption_is_caught() {
        let dir = std::env::temp_dir().join(format!(
            "pg-xample-oracle-cmp-paren-corrupt-{}",
            std::process::id()
        ));
        let base = dir.join("base");
        let clone = dir.join("clone");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&clone).unwrap();
        write_matching_triplet(&base, &clone, "MPBase");
        std::fs::write(
            base.join("MPBasegram.txt"),
            "rule { rootCat:106 template 5}\nRootPOS106\n(106_2)\n",
        )
        .unwrap();
        // rootCat/RootPOS correctly renumbered to 94; the parenthesized slot reference wrongly set to an unrelated 999.
        std::fs::write(
            clone.join("MPBasegram.txt"),
            "rule { rootCat:94 template 5}\nRootPOS94\n(999_2)\n",
        )
        .unwrap();
        let mismatches = xample_files_equivalent_ignoring_hvo_renumbering(&base, &clone, "MPBase")
            .expect_err("a (hvo_slotIndex) reference inconsistent with the rest of the file's renumbering must be caught");
        assert_eq!(
            mismatches.len(),
            1,
            "only gram.txt should differ: {mismatches:?}"
        );
        assert_eq!(mismatches[0].file_name, "MPBasegram.txt");
    }

    // Checks the ported prefix list against build.ps1's own source text so the two cannot silently drift apart.
    #[test]
    fn hvo_identifier_prefixes_match_build_ps1_verbatim() {
        let ps1_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tools/xample-projector/build.ps1");
        let text = std::fs::read_to_string(&ps1_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", ps1_path.display()));
        let block_re = Regex::new(r"(?s)\$script:HvoIdentifierPrefixes\s*=\s*@\((.*?)\)").unwrap();
        let block = block_re
            .captures(&text)
            .unwrap_or_else(|| {
                panic!(
                    "{}: no longer defines $script:HvoIdentifierPrefixes",
                    ps1_path.display()
                )
            })
            .get(1)
            .unwrap()
            .as_str();
        let item_re = Regex::new(r"'([^']+)'").unwrap();
        let extracted: Vec<&str> = item_re
            .captures_iter(block)
            .map(|c| c.get(1).unwrap().as_str())
            .collect();
        assert_eq!(
            extracted, HVO_IDENTIFIER_PREFIXES,
            "this crate's HVO_IDENTIFIER_PREFIXES has drifted from build.ps1's own $script:HvoIdentifierPrefixes"
        );
    }

    #[test]
    fn missing_file_is_reported_not_silently_ignored() {
        let dir = std::env::temp_dir().join(format!(
            "pg-xample-oracle-cmp-missing-{}",
            std::process::id()
        ));
        let base = dir.join("base");
        let clone = dir.join("clone");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&clone).unwrap();
        std::fs::write(base.join("MPBaseadctl.txt"), "\\maxp 12\n").unwrap();
        // clone's file deliberately absent.
        let mismatches = xample_files_equivalent_ignoring_hvo_renumbering(&base, &clone, "MPBase")
            .expect_err("a missing clone file must be reported, never silently skipped");
        assert!(mismatches.iter().any(|m| m.file_name == "MPBaseadctl.txt"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
