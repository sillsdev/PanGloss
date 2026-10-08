//! Owns the immutable manifest for one batch statistics run.

use std::fs;
#[cfg(unix)]
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use pg_assess::model::{model_fingerprint, source_sha256, SourceKind};
use pg_snapshot::Snapshot;
use serde::Serialize;

pub(crate) const FORMAT: &str = "pangloss-batch-stats-manifest";
pub(crate) const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SourceIdentity {
    pub(crate) kind: &'static str,
    pub(crate) source_sha256: String,
    pub(crate) grammar_hash: String,
    pub(crate) model_fingerprint: String,
    pub(crate) compile_options_json: Option<String>,
}

impl SourceIdentity {
    pub(crate) fn from_snapshot_source(source: &str, snapshot: &Snapshot) -> Result<Self, String> {
        let fingerprint =
            model_fingerprint(SourceKind::Snapshot, source, env!("CARGO_PKG_VERSION"))
                .map_err(|error| format!("fingerprint Snapshot source: {error}"))?;
        Ok(Self {
            kind: SourceKind::Snapshot.as_str(),
            source_sha256: source_sha256(source.as_bytes()),
            grammar_hash: snapshot.grammar_hash(),
            model_fingerprint: fingerprint,
            compile_options_json: Some(production_compile_options_json()),
        })
    }

    pub(crate) fn from_imported_snapshot(
        source_bytes: &[u8],
        snapshot: &Snapshot,
    ) -> Result<Self, String> {
        let fingerprint_source = snapshot.to_json();
        let fingerprint = model_fingerprint(
            SourceKind::Snapshot,
            &fingerprint_source,
            env!("CARGO_PKG_VERSION"),
        )
        .map_err(|error| format!("fingerprint imported Snapshot: {error}"))?;
        Ok(Self {
            kind: SourceKind::Snapshot.as_str(),
            source_sha256: source_sha256(source_bytes),
            grammar_hash: snapshot.grammar_hash(),
            model_fingerprint: fingerprint,
            compile_options_json: Some(production_compile_options_json()),
        })
    }

    pub(crate) fn from_hc_xml(source: &str) -> Result<Self, String> {
        let fingerprint = model_fingerprint(SourceKind::HcXml, source, env!("CARGO_PKG_VERSION"))
            .map_err(|error| format!("fingerprint HC XML source: {error}"))?;
        let source_digest = source_sha256(source.as_bytes());
        let grammar_hash = source_digest
            .strip_prefix("sha256:")
            .expect("source digest has the registered SHA-256 prefix")
            .to_string();
        Ok(Self {
            kind: SourceKind::HcXml.as_str(),
            source_sha256: source_digest,
            grammar_hash,
            model_fingerprint: fingerprint,
            compile_options_json: None,
        })
    }
}

fn production_compile_options_json() -> String {
    let options = pg_grammar::compile::CompileOptions::default();
    options.canonical_projection_json()
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CompilerIdentity {
    pub(crate) version: &'static str,
    pub(crate) build_identity: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CacheIdentity {
    pub(crate) sha256: String,
    pub(crate) bytes: u64,
    pub(crate) schema_version: i64,
    pub(crate) counter_semantics_version: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RunIdentity {
    pub(crate) id: i64,
    pub(crate) engine: String,
    pub(crate) grammar_hash: String,
    pub(crate) options_hash: String,
    pub(crate) options_json: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BatchOptions {
    pub(crate) engine: &'static str,
    pub(crate) threads: usize,
    pub(crate) step_cap: pg_stats::StepCap,
    pub(crate) work_cap: usize,
    pub(crate) search_budget_semantics: u32,
    pub(crate) word_timeout_ms: Option<u64>,
    pub(crate) guess: bool,
    pub(crate) always_enforce_final_templates: bool,
    pub(crate) start: usize,
    pub(crate) analyses_requested: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct InputIdentity {
    pub(crate) word_count: usize,
    pub(crate) word_list_sha256: String,
    pub(crate) words: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WordCompletion {
    pub(crate) index: usize,
    pub(crate) form: String,
    pub(crate) status: &'static str,
    pub(crate) capped: bool,
    pub(crate) timed_out: bool,
    pub(crate) invalid_shape: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CompletionCensus {
    pub(crate) requested: usize,
    pub(crate) complete: usize,
    pub(crate) incomplete: usize,
    pub(crate) invalid_shape: usize,
    pub(crate) missing: usize,
    pub(crate) words: Vec<WordCompletion>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StatsRunManifest {
    pub(crate) format: &'static str,
    pub(crate) version: u32,
    pub(crate) source: SourceIdentity,
    pub(crate) compiler: CompilerIdentity,
    pub(crate) cache: CacheIdentity,
    pub(crate) run: RunIdentity,
    pub(crate) batch: BatchOptions,
    pub(crate) input: InputIdentity,
    pub(crate) completion: CompletionCensus,
}

pub(crate) struct PublishedManifest {
    pub(crate) path: PathBuf,
    pub(crate) bytes: u64,
    pub(crate) sha256: String,
}

pub(crate) fn input_identity(words: &[String]) -> Result<InputIdentity, String> {
    let bytes =
        serde_json::to_vec(words).map_err(|error| format!("serialize word list: {error}"))?;
    Ok(InputIdentity {
        word_count: words.len(),
        word_list_sha256: sha256_digest(&bytes),
        words: words.to_vec(),
    })
}

pub(crate) fn publish_noclobber(
    path: &Path,
    manifest: &StatsRunManifest,
) -> Result<PublishedManifest, String> {
    if path.exists() {
        return Err(format!(
            "output_exists: stats manifest already exists at {}",
            path.display()
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(format!(
            "stats manifest output directory does not exist: {}",
            parent.display()
        ));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("create temporary stats manifest: {error}"))?;
    serde_json::to_writer_pretty(&mut temporary, manifest)
        .map_err(|error| format!("serialize stats manifest: {error}"))?;
    temporary
        .write_all(b"\n")
        .map_err(|error| format!("write stats manifest: {error}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("sync temporary stats manifest: {error}"))?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| format!("publish stats manifest without clobbering: {}", error.error))?;
    sync_parent(parent)?;

    let bytes =
        fs::read(path).map_err(|error| format!("read published stats manifest: {error}"))?;
    Ok(PublishedManifest {
        path: path.to_path_buf(),
        bytes: bytes.len() as u64,
        sha256: sha256_digest(&bytes),
    })
}

fn sync_parent(parent: &Path) -> Result<(), String> {
    #[cfg(unix)]
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("sync stats manifest directory: {error}"))?;
    #[cfg(not(unix))]
    let _ = parent;
    Ok(())
}

pub(crate) fn sha256_digest(bytes: &[u8]) -> String {
    source_sha256(bytes)
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    sha256_digest(bytes)
        .strip_prefix("sha256:")
        .expect("source digest has the registered SHA-256 prefix")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_identity_records_resolved_production_compile_options() {
        let snapshot = Snapshot::new(
            pg_snapshot::Project::default(),
            pg_snapshot::FeatureSystems::default(),
            pg_snapshot::Phonology::default(),
            pg_snapshot::Morphology::default(),
            pg_snapshot::Lexicon::default(),
        );
        let identity = SourceIdentity::from_snapshot_source("{}", &snapshot).unwrap();
        let options = pg_grammar::compile::CompileOptions::default();

        assert_eq!(
            identity.compile_options_json.as_deref(),
            Some(options.canonical_projection_json().as_str())
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                identity.compile_options_json.as_ref().unwrap()
            )
            .unwrap()["semanticLossPolicy"],
            "refuse"
        );
    }

    #[test]
    fn hc_xml_identity_has_no_snapshot_compile_options() {
        let identity = SourceIdentity::from_hc_xml("<HC/> ").unwrap();
        assert_eq!(identity.compile_options_json, None);
    }
}
