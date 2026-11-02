use std::fs;
use std::path::Path;

use pg_grammar::compile::{compile_project_with, CompileOptions};
use pg_snapshot::{ConversionIssue, InventoryDelta, LoadDecision, Snapshot};
use rusqlite::{params, Connection, TransactionBehavior};
use tempfile::NamedTempFile;

use crate::context::FactsContext;
use crate::metadata::{fixed_sections, ProducerIdentity, SectionStatus};
use crate::stats::{StatsExpectation, StatsInput, StatsProjection};
use crate::{FactsError, FactsResult};

struct Publication<'a> {
    output_path: &'a Path,
    snapshot: &'a Snapshot,
    context: &'a FactsContext,
    producer: ProducerIdentity<'a>,
    source_sha256: &'a str,
    grammar_hash: &'a str,
    model_fingerprint: &'a str,
    options_json: &'a str,
    options_sha256: &'a str,
    compile_status: &'static str,
    inventory: &'a InventoryDelta,
    decisions: &'a [LoadDecision],
    compile_issues: &'a [ConversionIssue],
    environment_resolutions: &'a [pg_grammar::compile::EnvironmentResolution],
    compiled_outputs: &'a [pg_grammar::compile::CompiledOutput],
    allomorph_output_ids: &'a [Option<u32>],
    compiled_mappings: &'a [pg_grammar::compile::CompiledMapping],
    compiled_allomorph_order: &'a [pg_grammar::compile::CompiledAllomorphOrder],
    allomorph_gates: &'a [pg_grammar::compile::AllomorphGateOutcome],
    grammar: Option<&'a pg_grammar::model::Grammar>,
    stats: Option<StatsProjection>,
    sections: Vec<SectionStatus>,
}

struct MetadataInput<'a> {
    snapshot: &'a Snapshot,
    context: &'a FactsContext,
    compiler_version: &'a str,
    source_revision: &'a str,
    build_identity: &'a str,
    source_sha256: &'a str,
    grammar_hash: &'a str,
    model_fingerprint: &'a str,
    options_json: &'a str,
    options_sha256: &'a str,
    compile_status: &'a str,
    run_manifest_sha256: Option<&'a str>,
}

pub fn write_facts(
    source_bytes: &[u8],
    context_bytes: &[u8],
    output_path: impl AsRef<Path>,
    producer: ProducerIdentity<'_>,
) -> Result<FactsResult, FactsError> {
    write_facts_inner(
        source_bytes,
        context_bytes,
        output_path,
        producer,
        None,
        |_| {},
    )
}

/// Builds a facts file and attaches exactly one manifest-bound frozen stats run.
pub fn write_facts_with_stats(
    source_bytes: &[u8],
    context_bytes: &[u8],
    output_path: impl AsRef<Path>,
    producer: ProducerIdentity<'_>,
    stats_input: StatsInput<'_>,
) -> Result<FactsResult, FactsError> {
    write_facts_inner(
        source_bytes,
        context_bytes,
        output_path,
        producer,
        Some(stats_input),
        |_| {},
    )
}

fn write_facts_inner<F>(
    source_bytes: &[u8],
    context_bytes: &[u8],
    output_path: impl AsRef<Path>,
    producer: ProducerIdentity<'_>,
    stats_input: Option<StatsInput<'_>>,
    before_schema: F,
) -> Result<FactsResult, FactsError>
where
    F: FnOnce(&Connection),
{
    let context = FactsContext::parse(context_bytes)?;
    let source = std::str::from_utf8(source_bytes)
        .map_err(|error| FactsError::InvalidSnapshot(error.to_string()))?;
    let strict = pg_assess::jcs::parse_strict_json(source)
        .map_err(|error| FactsError::InvalidSnapshot(error.to_string()))?;
    // The strict parse is the only one the fingerprint needs; its error surfaces where it always did.
    let fingerprint = pg_assess::snapshot_value_fingerprint(&strict, producer.compiler_version);
    drop(strict);
    let snapshot = Snapshot::from_json(source).map_err(|error| match error {
        pg_snapshot::SnapshotError::UnsupportedVersion { found } => {
            FactsError::UnsupportedSnapshotVersion(found)
        }
        pg_snapshot::SnapshotError::UnknownFormat { found } => {
            FactsError::UnsupportedFactsSource(found)
        }
        pg_snapshot::SnapshotError::Json(error) => FactsError::InvalidSnapshot(error.to_string()),
    })?;
    snapshot
        .conversion_provenance
        .validate()
        .map_err(|error| FactsError::InvalidSnapshot(error.to_string()))?;
    crate::morphology::validate_authored_guids(&snapshot)?;
    crate::phonology::validate_authored_guids(&snapshot)?;

    let source_sha256 = pg_assess::source_sha256(source_bytes);
    let grammar_hash = snapshot.grammar_hash();
    let model_fingerprint =
        fingerprint.map_err(|error| FactsError::InvalidSnapshot(error.to_string()))?;
    if let Some(expected) = &context.expected_model_fingerprint {
        if expected != &model_fingerprint {
            return Err(FactsError::ModelMismatch {
                expected: expected.clone(),
                actual: model_fingerprint,
            });
        }
    }

    let options = CompileOptions::default();
    let options_json = options.canonical_projection_json();
    let options_sha256 = pg_assess::sha256_bytes(options_json.as_bytes());
    let (
        compile_status,
        inventory,
        decisions,
        compile_issues,
        environment_resolutions,
        compiled_outputs,
        compiled_mappings,
        compiled_allomorph_order,
        allomorph_gates,
        grammar,
        allomorph_output_ids,
    ) = match compile_project_with(&snapshot, options) {
        Ok(output) => (
            "completed",
            output.inventory,
            output.load_decisions,
            output.issues,
            output.environment_resolutions,
            output.compiled_outputs,
            output.compiled_mappings,
            output.compiled_allomorph_order,
            output.allomorph_gates,
            Some(output.grammar),
            output.allomorph_output_ids,
        ),
        Err(pg_grammar::GrammarError::Conversion(error)) => (
            "refused",
            error.inventory,
            error.load_decisions,
            error.issues,
            error.environment_resolutions,
            error.compiled_outputs,
            error.compiled_mappings,
            error.compiled_allomorph_order,
            error.allomorph_gates,
            None,
            Vec::new(),
        ),
        Err(error) => return Err(FactsError::Compile(error.to_string())),
    };
    let stats = match stats_input {
        Some(input) => {
            let Some(grammar) = grammar.as_ref() else {
                return Err(FactsError::StatsRunUnavailable(
                    "stats projection requires a completed production compile".into(),
                ));
            };
            Some(crate::stats::read_projection(
                input,
                StatsExpectation {
                    source_sha256: &source_sha256,
                    grammar_hash: &grammar_hash,
                    model_fingerprint: &model_fingerprint,
                    compile_options_json: &options_json,
                    producer,
                },
                &snapshot,
                grammar,
            )?)
        }
        None => None,
    };

    let source_inventory_complete = snapshot.conversion_provenance.schema_version
        == pg_snapshot::CONVERSION_PROVENANCE_SCHEMA_VERSION
        && matches!(
            snapshot.conversion_provenance.source_inventory_status,
            pg_snapshot::SourceInventoryStatus::ImportedComplete
                | pg_snapshot::SourceInventoryStatus::ImportedWithFatalIssues
        );
    let load_accounting_complete = crate::load::accounting_is_complete(
        &snapshot.conversion_provenance,
        &inventory,
        &decisions,
    );
    let sections = fixed_sections(
        compile_status == "completed",
        source_inventory_complete,
        snapshot.morphology.adhoc_prohibition_groups.is_some(),
        load_accounting_complete,
        stats.is_some(),
    );
    let result = publish(
        Publication {
            output_path: output_path.as_ref(),
            snapshot: &snapshot,
            context: &context,
            producer,
            source_sha256: &source_sha256,
            grammar_hash: &grammar_hash,
            model_fingerprint: &model_fingerprint,
            options_json: &options_json,
            options_sha256: &options_sha256,
            compile_status,
            inventory: &inventory,
            decisions: &decisions,
            compile_issues: &compile_issues,
            environment_resolutions: &environment_resolutions,
            compiled_outputs: &compiled_outputs,
            allomorph_output_ids: &allomorph_output_ids,
            compiled_mappings: &compiled_mappings,
            compiled_allomorph_order: &compiled_allomorph_order,
            allomorph_gates: &allomorph_gates,
            grammar: grammar.as_ref(),
            stats,
            sections,
        },
        before_schema,
    )?;
    Ok(result)
}

fn publish<F>(publication: Publication<'_>, before_schema: F) -> Result<FactsResult, FactsError>
where
    F: FnOnce(&Connection),
{
    let Publication {
        output_path,
        snapshot,
        context,
        producer,
        source_sha256,
        grammar_hash,
        model_fingerprint,
        options_json,
        options_sha256,
        compile_status,
        inventory,
        decisions,
        compile_issues,
        environment_resolutions,
        compiled_outputs,
        allomorph_output_ids,
        compiled_mappings,
        compiled_allomorph_order,
        allomorph_gates,
        grammar,
        stats,
        sections,
    } = publication;
    if output_path.exists() {
        return Err(FactsError::OutputExists);
    }
    let parent = output_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err(FactsError::Io(format!(
            "output directory does not exist: {}",
            parent.display()
        )));
    }
    let temporary =
        NamedTempFile::new_in(parent).map_err(|error| FactsError::Io(error.to_string()))?;
    let temporary_path = temporary.path().to_path_buf();
    {
        let form_segments = match grammar {
            Some(grammar) => {
                crate::compiled::output_segments(snapshot, grammar, allomorph_output_ids)?
            }
            None => Vec::new(),
        };
        let mut connection = Connection::open(temporary_path)?;
        configure_database(&connection)?;
        before_schema(&connection);
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(include_str!("schema.sql"))?;
        // Authored and compiled rows may spell one GUID in two cases until the canonical pass runs.
        transaction.execute_batch("PRAGMA defer_foreign_keys = ON;")?;
        insert_metadata(
            &transaction,
            MetadataInput {
                snapshot,
                context,
                compiler_version: producer.compiler_version,
                source_revision: producer.source_revision,
                build_identity: producer.build_identity,
                source_sha256,
                grammar_hash,
                model_fingerprint,
                options_json,
                options_sha256,
                compile_status,
                run_manifest_sha256: stats
                    .as_ref()
                    .map(|projection| projection.manifest_sha256.as_str()),
            },
        )?;
        for section in &sections {
            crate::sql::execute(
                &transaction,
                "INSERT INTO artifact_section(section, status, source_scope, reason_code) VALUES (?1, ?2, ?3, ?4)",
                params![section.section, section.status, section.source_scope, section.reason_code],
            )?;
        }
        crate::stats::insert_catalog(
            &transaction,
            stats
                .as_ref()
                .map_or(pg_stats::COUNTER_SEMANTICS_VERSION, |run| {
                    run.counter_semantics
                }),
        )?;
        crate::morphology::insert_authored(
            &transaction,
            snapshot,
            decisions,
            compile_status == "completed",
        )?;
        let feature_structures = crate::features::insert_authored(&transaction, snapshot)?;
        crate::morphology::insert_definitions(&transaction, snapshot, &feature_structures)?;
        crate::morphology::insert_closures(&transaction)?;
        crate::lexicon::insert_entry_variants(&transaction, snapshot)?;
        crate::lexicon::insert_gates(&transaction, snapshot, allomorph_gates, &feature_structures)?;
        crate::load::insert_inventory_and_issues(
            &transaction,
            snapshot,
            inventory,
            compile_issues,
            decisions,
        )?;
        // One pass over every decision here; the phonology and reference writers look owners up in it.
        let owner_loads = crate::owner_loads::OwnerLoads::new(decisions);
        crate::phonology::insert_authored(
            &transaction,
            snapshot,
            &feature_structures,
            environment_resolutions,
            decisions,
            &owner_loads,
            grammar,
        )?;
        crate::compiled::insert(
            &transaction,
            compiled_outputs,
            compiled_mappings,
            compiled_allomorph_order,
            &form_segments,
        )?;
        crate::settings::insert_parser_config(&transaction, snapshot, grammar)?;
        crate::references::insert(
            &transaction,
            snapshot,
            decisions,
            &owner_loads,
            environment_resolutions,
            allomorph_gates,
            crate::references::CompiledInputs {
                grammar,
                mappings: compiled_mappings,
                form_segments: &form_segments,
            },
        )?;
        if let Some(stats) = &stats {
            crate::stats::insert(&transaction, stats)?;
        }
        crate::guid::canonicalize_stored_guids(&transaction)?;
        crate::sql::execute(
            &transaction,
            "UPDATE artifact_meta SET complete=1 WHERE singleton=1",
            [],
        )?;
        transaction.commit()?;
        verify_database(&connection)?;
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| FactsError::Io(error.to_string()))?;
    let published_path = match temporary.persist_noclobber(output_path) {
        Ok(file) => file,
        Err(error) => {
            if output_path.exists() {
                return Err(FactsError::OutputExists);
            }
            return Err(FactsError::Io(error.error.to_string()));
        }
    };
    drop(published_path);
    sync_parent(parent).map_err(|error| {
        FactsError::Io(format!(
            "published complete facts file at {} but could not sync its directory: {error}",
            output_path.display()
        ))
    })?;
    let bytes = fs::read(output_path).map_err(|error| {
        FactsError::Io(format!(
            "published complete facts file at {} but could not read it for the response digest: {error}",
            output_path.display()
        ))
    })?;
    Ok(FactsResult {
        application_id: crate::APPLICATION_ID,
        schema_version: crate::FACT_SCHEMA_VERSION,
        format: crate::FACT_FORMAT.into(),
        source_sha256: source_sha256.to_string(),
        grammar_hash: grammar_hash.to_string(),
        model_fingerprint: model_fingerprint.to_string(),
        baseline_key: context.baseline_key.clone(),
        input_kind: context.input_kind.into(),
        dry_run_digest: context.dry_run_digest.clone(),
        compile_status: compile_status.into(),
        sections,
        output_bytes: bytes.len() as u64,
        output_sha256: pg_assess::sha256_bytes(&bytes),
    })
}

fn configure_database(connection: &Connection) -> Result<(), FactsError> {
    let pragmas = format!(
        concat!(
            "PRAGMA page_size=4096;\n",
            "PRAGMA encoding='UTF-8';\n",
            "PRAGMA journal_mode=DELETE;\n",
            "PRAGMA synchronous=FULL;\n",
            "PRAGMA foreign_keys=ON;\n",
            "PRAGMA ignore_check_constraints=ON;\n",
            "PRAGMA application_id={};\n",
            "PRAGMA user_version={};"
        ),
        crate::APPLICATION_ID,
        crate::FACT_SCHEMA_VERSION
    );
    connection.execute_batch(&pragmas)?;
    connection.set_prepared_statement_cache_capacity(crate::sql::STATEMENT_CACHE_CAPACITY);
    Ok(())
}

fn insert_metadata(
    tx: &rusqlite::Transaction<'_>,
    metadata: MetadataInput<'_>,
) -> Result<(), FactsError> {
    crate::sql::execute(
        tx,
        "INSERT INTO artifact_meta(singleton, application_id, schema_version, format, writer_version, compiler_version, source_revision, build_identity, snapshot_format, snapshot_version, provenance_schema_version, source_inventory_status, source_sha256, grammar_hash, model_fingerprint, baseline_token_json, baseline_key, input_kind, dry_run_digest, compile_options_json, compile_options_sha256, compile_status, complete, run_manifest_sha256) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, 0, ?22)",
        params![
            crate::APPLICATION_ID,
            i64::from(crate::FACT_SCHEMA_VERSION),
            crate::FACT_FORMAT,
            env!("CARGO_PKG_VERSION"),
            metadata.compiler_version,
            metadata.source_revision,
            metadata.build_identity,
            metadata.snapshot.format,
            i64::from(metadata.snapshot.version),
            i64::from(metadata.snapshot.conversion_provenance.schema_version),
            source_inventory_status(metadata.snapshot.conversion_provenance.source_inventory_status),
            metadata.source_sha256,
            metadata.grammar_hash,
            metadata.model_fingerprint,
            metadata.context.baseline_token_json,
            metadata.context.baseline_key,
            metadata.context.input_kind,
            metadata.context.dry_run_digest,
            metadata.options_json,
            metadata.options_sha256,
            metadata.compile_status,
            metadata.run_manifest_sha256,
        ],
    )?;
    Ok(())
}

fn source_inventory_status(status: pg_snapshot::SourceInventoryStatus) -> &'static str {
    match status {
        pg_snapshot::SourceInventoryStatus::ImportedComplete => "importedComplete",
        pg_snapshot::SourceInventoryStatus::ImportedWithFatalIssues => "importedWithFatalIssues",
        pg_snapshot::SourceInventoryStatus::Synthetic => "synthetic",
        pg_snapshot::SourceInventoryStatus::Unknown => "unknown",
    }
}

/// Turns CHECK enforcement back on first: `integrity_check` skips CHECK constraints while it is off.
fn verify_database(connection: &Connection) -> Result<(), FactsError> {
    connection.execute_batch("PRAGMA ignore_check_constraints=OFF;")?;
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(FactsError::Integrity(integrity));
    }
    let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
    let mut rows = statement.query([])?;
    if rows.next()?.is_some() {
        return Err(FactsError::Integrity(
            "foreign_key_check returned a row".into(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), FactsError> {
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| FactsError::Io(error.to_string()))
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> Result<(), FactsError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use pg_snapshot::{
        BoundaryMarker, FeatureSystems, Lexicon, Morphology, Phoneme, Phonology, Project, Snapshot,
        WsForm,
    };
    use tempfile::tempdir;

    use rusqlite::Connection;

    use super::{configure_database, verify_database, write_facts_inner};
    use crate::metadata::ProducerIdentity;
    use crate::FactsError;

    const CONTEXT: &[u8] = br#"{"format":"pangloss-facts-context","version":1,"baselineToken":{"id":"commit-count"},"inputKind":"baseline","dryRunDigest":null}"#;

    fn fixture_source() -> Vec<u8> {
        let snapshot = Snapshot::new(
            Project {
                name: "Commit-count fixture".into(),
                vernacular_writing_systems: vec!["qaa".into()],
                analysis_writing_systems: vec!["en".into()],
                ..Project::default()
            },
            FeatureSystems::default(),
            Phonology {
                phonemes: vec![Phoneme {
                    guid: "00000000-0000-0000-0000-000000000001".into(),
                    name: "a".into(),
                    representations: vec![WsForm {
                        ws: "qaa".into(),
                        form: "a".into(),
                    }],
                    features: None,
                    basic_ipa_symbol: None,
                }],
                boundary_markers: vec![BoundaryMarker {
                    guid: "00000000-0000-0000-0000-000000000002".into(),
                    name: "Morpheme boundary".into(),
                    representations: vec![WsForm {
                        ws: "qaa".into(),
                        form: "+".into(),
                    }],
                }],
                ..Phonology::default()
            },
            Morphology::default(),
            Lexicon::default(),
        );
        snapshot.to_json().into_bytes()
    }

    const PRODUCER: ProducerIdentity<'static> = ProducerIdentity {
        compiler_version: "0.8.3",
        source_revision: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        build_identity: "pangloss/0.8.3+aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    };

    #[test]
    fn facts_write_commits_schema_and_rows_once() {
        let source = fixture_source();
        let temporary = tempdir().unwrap();
        let output = temporary.path().join("facts.sqlite");
        let commits = Arc::new(AtomicUsize::new(0));
        let hook_commits = Arc::clone(&commits);

        write_facts_inner(
            &source,
            CONTEXT,
            &output,
            PRODUCER,
            None,
            move |connection| {
                connection.commit_hook(Some(move || {
                    hook_commits.fetch_add(1, Ordering::SeqCst);
                    false
                }));
            },
        )
        .unwrap();

        assert_eq!(
            commits.load(Ordering::SeqCst),
            1,
            "schema and facts rows should share one write transaction"
        );
    }

    /// A facts file built with CHECKs ignored holds a bad row only if verify fails to find it.
    #[test]
    fn verify_reports_a_row_that_violates_a_check_planted_during_the_build() {
        let temporary = tempdir().unwrap();
        let connection = Connection::open(temporary.path().join("facts.sqlite")).unwrap();
        configure_database(&connection).unwrap();
        connection
            .execute_batch(include_str!("schema.sql"))
            .unwrap();
        connection
            .execute(
                "INSERT INTO artifact_section(section, status, source_scope) VALUES ('project', 'not-a-status', 'x')",
                [],
            )
            .unwrap();
        let during_build: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            during_build, "ok",
            "the planted row is invisible while CHECKs are ignored"
        );

        match verify_database(&connection) {
            Err(FactsError::Integrity(message)) => assert!(
                message.contains("CHECK constraint failed"),
                "unexpected message: {message}"
            ),
            other => panic!("verify must report the CHECK violation, got {other:?}"),
        }
    }

    #[test]
    fn a_clean_database_passes_verify_with_checks_enforced_again() {
        let temporary = tempdir().unwrap();
        let connection = Connection::open(temporary.path().join("facts.sqlite")).unwrap();
        configure_database(&connection).unwrap();
        connection
            .execute_batch(include_str!("schema.sql"))
            .unwrap();
        connection
            .execute(
                "INSERT INTO artifact_section(section, status, source_scope) VALUES ('project', 'complete', 'x')",
                [],
            )
            .unwrap();
        verify_database(&connection).unwrap();
        let ignoring: i64 = connection
            .query_row("PRAGMA ignore_check_constraints", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ignoring, 0);
    }

    fn names_in(directory: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_output_name_appears_only_after_the_file_is_built_and_verified() {
        let source = fixture_source();
        let temporary = tempdir().unwrap();
        let directory = temporary.path().to_path_buf();
        let output = directory.join("facts.sqlite");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let hook_seen = Arc::clone(&seen);
        let hook_output = output.clone();
        let hook_directory = directory.clone();

        write_facts_inner(
            &source,
            CONTEXT,
            &output,
            PRODUCER,
            None,
            move |connection| {
                connection.commit_hook(Some(move || {
                    hook_seen
                        .lock()
                        .unwrap()
                        .push((hook_output.exists(), names_in(&hook_directory)));
                    false
                }));
            },
        )
        .unwrap();

        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        let (output_existed, names) = &seen[0];
        assert!(
            !output_existed,
            "the final name must not exist while rows are written"
        );
        assert!(
            !names.is_empty() && names.iter().all(|name| name.starts_with(".tmp")),
            "only temporary files exist mid-write: {names:?}"
        );
        assert_eq!(names_in(&directory), ["facts.sqlite"]);
    }

    #[test]
    fn an_aborted_write_leaves_neither_the_output_nor_a_temporary_file() {
        let source = fixture_source();
        let temporary = tempdir().unwrap();
        let output = temporary.path().join("facts.sqlite");

        let result = write_facts_inner(&source, CONTEXT, &output, PRODUCER, None, |connection| {
            connection.commit_hook(Some(|| true));
        });

        assert!(result.is_err());
        assert_eq!(names_in(temporary.path()), Vec::<String>::new());
    }

    #[test]
    fn the_stored_model_fingerprint_is_that_of_the_source_text() {
        let source = fixture_source();
        let temporary = tempdir().unwrap();
        let output = temporary.path().join("facts.sqlite");

        let result = write_facts_inner(&source, CONTEXT, &output, PRODUCER, None, |_| {}).unwrap();

        let expected = pg_assess::model_fingerprint(
            pg_assess::SourceKind::Snapshot,
            std::str::from_utf8(&source).unwrap(),
            PRODUCER.compiler_version,
        )
        .unwrap();
        assert_eq!(result.model_fingerprint, expected);
    }
}
