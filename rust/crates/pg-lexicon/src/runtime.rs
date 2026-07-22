use crate::{
    AddRequest, ClassCatalog, ClassSignature, Clock, EntryAuthority, EntryId, ExpectedRevision,
    IdSource, MutationResult, OsIdSource, RemoveRequest, Revision, SetAuthorityRequest,
    SetGlossLanguageRequest, StructuredError, SuppliedEntry, UpdateRequest, UtcClock,
    ValidationState,
};
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOutcome, RootAuthority, SuppliedRoot, SuppliedRootOverlay};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, RwLock};

pub const LEXICON_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LexiconDocument {
    pub schema_version: u32,
    pub grammar_name: String,
    pub source_grammar_fingerprint: String,
    pub gloss_language: Option<String>,
    pub signatures: Vec<ClassSignature>,
    pub entries: Vec<SuppliedEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationReport {
    pub exact_match: bool,
    pub compatible_migration: bool,
    pub inactive_entries: Vec<EntryId>,
    pub superseded_entries: Vec<EntryId>,
    pub revision: Revision,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub document: LexiconDocument,
    pub expected_revision: Option<Revision>,
}

#[derive(Debug)]
pub struct LexiconSnapshot {
    revision: Revision,
    entries: Vec<SuppliedEntry>,
    mappings: BTreeMap<crate::SignatureId, ClassSignature>,
    gloss_language: Option<String>,
    overlay: SuppliedRootOverlay,
}

impl LexiconSnapshot {
    pub fn revision(&self) -> &Revision {
        &self.revision
    }
    pub fn entries(&self) -> &[SuppliedEntry] {
        &self.entries
    }
    pub fn overlay(&self) -> &SuppliedRootOverlay {
        &self.overlay
    }
}

pub struct SuppliedLexiconRuntime {
    grammar: Arc<Grammar>,
    catalog: ClassCatalog,
    grammar_name: String,
    source_fingerprint: String,
    mutation: Mutex<()>,
    ids: Mutex<Box<dyn IdSource + Send>>,
    clock: Mutex<Box<dyn Clock + Send>>,
    state: RwLock<Arc<LexiconSnapshot>>,
}

impl SuppliedLexiconRuntime {
    pub fn new(grammar: Arc<Grammar>, grammar_source: &str) -> Result<Self, StructuredError> {
        Self::with_sources(grammar, grammar_source, OsIdSource, UtcClock)
    }

    pub fn with_sources<I, C>(
        grammar: Arc<Grammar>,
        grammar_source: &str,
        ids: I,
        clock: C,
    ) -> Result<Self, StructuredError>
    where
        I: IdSource + Send + 'static,
        C: Clock + Send + 'static,
    {
        let catalog = ClassCatalog::from_grammar(&grammar)
            .map_err(|message| error("invalid_catalog", message))?;
        let grammar_name = grammar
            .name
            .clone()
            .ok_or_else(|| error("missing_grammar_name", "grammar has no name"))?;
        let grammar_name = grammar_name.trim().to_string();
        if grammar_name.is_empty() {
            return Err(error("missing_grammar_name", "grammar name is blank"));
        }
        let snapshot = LexiconSnapshot {
            revision: Revision::new(0),
            entries: Vec::new(),
            mappings: BTreeMap::new(),
            gloss_language: None,
            overlay: SuppliedRootOverlay::empty(&grammar),
        };
        Ok(Self {
            grammar,
            catalog,
            grammar_name,
            source_fingerprint: grammar_source_fingerprint(grammar_source),
            mutation: Mutex::new(()),
            ids: Mutex::new(Box::new(ids)),
            clock: Mutex::new(Box::new(clock)),
            state: RwLock::new(Arc::new(snapshot)),
        })
    }

    pub fn catalog(&self) -> &ClassCatalog {
        &self.catalog
    }

    pub fn source_fingerprint(&self) -> &str {
        &self.source_fingerprint
    }

    pub fn snapshot(&self) -> Arc<LexiconSnapshot> {
        self.state
            .read()
            .expect("lexicon snapshot lock poisoned")
            .clone()
    }

    pub fn parse_word(&self, word: &str) -> ParseOutcome {
        let snapshot = self.snapshot();
        Morpher::new_with_overlay(&self.grammar, 100_000, snapshot.overlay()).parse_word(word)
    }

    pub fn export_document(&self) -> LexiconDocument {
        let snapshot = self.snapshot();
        let referenced: BTreeSet<_> = snapshot
            .entries
            .iter()
            .flat_map(|entry| entry.signatures.iter().cloned())
            .collect();
        LexiconDocument {
            schema_version: LEXICON_SCHEMA_VERSION,
            grammar_name: self.grammar_name.clone(),
            source_grammar_fingerprint: self.source_fingerprint.clone(),
            gloss_language: snapshot.gloss_language.clone(),
            signatures: referenced
                .iter()
                .filter_map(|id| snapshot.mappings.get(id).cloned())
                .collect(),
            entries: snapshot.entries.clone(),
        }
    }

    pub fn add(
        &self,
        request: AddRequest,
    ) -> Result<MutationResult<SuppliedEntry>, StructuredError> {
        let _mutation = self
            .mutation
            .lock()
            .expect("lexicon mutation lock poisoned");
        self.check_revision(&request.expected_revision)?;
        let mut signatures = request.signatures;
        self.validate_active_entry(&request.stem, &request.gloss, &mut signatures)?;
        let id = EntryId::from_bytes(
            self.ids
                .lock()
                .expect("lexicon ID source lock poisoned")
                .next_128()?,
        );
        let mut document = self.export_document();
        if document.entries.iter().any(|entry| entry.id == id) {
            return Err(error("duplicate_entry_id", "generated duplicate entry ID"));
        }
        let now = self
            .clock
            .lock()
            .expect("lexicon clock lock poisoned")
            .now();
        let entry = SuppliedEntry {
            id,
            stem: request.stem,
            gloss: request.gloss,
            signatures,
            date_created: now.clone(),
            date_modified: now,
            authority: EntryAuthority::Supplied,
            state: ValidationState::Active,
        };
        document.entries.push(entry.clone());
        self.ensure_current_mappings(&mut document);
        self.import_document_locked(document)?;
        let snapshot = self.snapshot();
        let value = snapshot
            .entries()
            .iter()
            .find(|candidate| candidate.id == entry.id)
            .expect("published entry is present")
            .clone();
        Ok(MutationResult {
            value,
            revision: snapshot.revision().clone(),
            changed: true,
        })
    }

    pub fn update(
        &self,
        request: UpdateRequest,
    ) -> Result<MutationResult<SuppliedEntry>, StructuredError> {
        let _mutation = self
            .mutation
            .lock()
            .expect("lexicon mutation lock poisoned");
        self.check_revision(&request.expected_revision)?;
        let mut signatures = request.signatures;
        self.validate_active_entry(&request.stem, &request.gloss, &mut signatures)?;
        let mut document = self.export_document();
        let entry = document
            .entries
            .iter_mut()
            .find(|entry| entry.id == request.id)
            .ok_or_else(|| error("entry_not_found", "entry not found"))?;
        if entry.stem == request.stem
            && entry.gloss == request.gloss
            && entry.signatures == signatures
        {
            return Ok(MutationResult {
                value: entry.clone(),
                revision: self.snapshot().revision().clone(),
                changed: false,
            });
        }
        entry.stem = request.stem;
        entry.gloss = request.gloss;
        entry.signatures = signatures;
        entry.date_modified = self
            .clock
            .lock()
            .expect("lexicon clock lock poisoned")
            .now();
        let id = entry.id.clone();
        self.ensure_current_mappings(&mut document);
        self.import_document_locked(document)?;
        let snapshot = self.snapshot();
        let value = snapshot
            .entries()
            .iter()
            .find(|entry| entry.id == id)
            .expect("published entry is present")
            .clone();
        Ok(MutationResult {
            value,
            revision: snapshot.revision().clone(),
            changed: true,
        })
    }

    pub fn remove(&self, request: RemoveRequest) -> Result<MutationResult<bool>, StructuredError> {
        let _mutation = self
            .mutation
            .lock()
            .expect("lexicon mutation lock poisoned");
        self.check_revision(&request.expected_revision)?;
        let mut document = self.export_document();
        let before = document.entries.len();
        document.entries.retain(|entry| entry.id != request.id);
        let changed = before != document.entries.len();
        if changed {
            self.ensure_current_mappings(&mut document);
            self.import_document_locked(document)?;
        }
        Ok(MutationResult {
            value: changed,
            revision: self.snapshot().revision().clone(),
            changed,
        })
    }

    pub fn clear(
        &self,
        request: ExpectedRevision,
    ) -> Result<MutationResult<usize>, StructuredError> {
        let _mutation = self
            .mutation
            .lock()
            .expect("lexicon mutation lock poisoned");
        self.check_revision(&request.expected_revision)?;
        let mut document = self.export_document();
        let count = document.entries.len();
        if count > 0 {
            document.entries.clear();
            document.signatures.clear();
            self.import_document_locked(document)?;
        }
        Ok(MutationResult {
            value: count,
            revision: self.snapshot().revision().clone(),
            changed: count > 0,
        })
    }

    pub fn set_gloss_language(
        &self,
        request: SetGlossLanguageRequest,
    ) -> Result<MutationResult<Option<String>>, StructuredError> {
        let _mutation = self
            .mutation
            .lock()
            .expect("lexicon mutation lock poisoned");
        self.check_revision(&request.expected_revision)?;
        if request
            .gloss_language
            .as_ref()
            .is_some_and(|language| language.trim().is_empty())
        {
            return Err(error("invalid_gloss_language", "language cannot be blank"));
        }
        let mut document = self.export_document();
        if request.gloss_language.is_none()
            && document.entries.iter().any(|entry| !entry.gloss.is_empty())
        {
            return Err(error(
                "gloss_language_required",
                "cannot clear language while glosses exist",
            ));
        }
        let changed = document.gloss_language != request.gloss_language;
        if changed {
            document.gloss_language = request.gloss_language.clone();
            self.import_document_locked(document)?;
        }
        Ok(MutationResult {
            value: request.gloss_language,
            revision: self.snapshot().revision().clone(),
            changed,
        })
    }

    pub fn set_authority(
        &self,
        request: SetAuthorityRequest,
    ) -> Result<MutationResult<SuppliedEntry>, StructuredError> {
        let _mutation = self
            .mutation
            .lock()
            .expect("lexicon mutation lock poisoned");
        self.check_revision(&request.expected_revision)?;
        let mut document = self.export_document();
        let entry = document
            .entries
            .iter_mut()
            .find(|entry| entry.id == request.id)
            .ok_or_else(|| error("entry_not_found", "entry not found"))?;
        if entry.authority == request.authority {
            return Ok(MutationResult {
                value: entry.clone(),
                revision: self.snapshot().revision().clone(),
                changed: false,
            });
        }
        entry.authority = request.authority;
        entry.date_modified = self
            .clock
            .lock()
            .expect("lexicon clock lock poisoned")
            .now();
        let id = entry.id.clone();
        self.import_document_locked(document)?;
        let snapshot = self.snapshot();
        let value = snapshot
            .entries()
            .iter()
            .find(|entry| entry.id == id)
            .expect("published entry is present")
            .clone();
        Ok(MutationResult {
            value,
            revision: snapshot.revision().clone(),
            changed: true,
        })
    }

    fn check_revision(&self, expected: &Option<Revision>) -> Result<(), StructuredError> {
        let current = self.snapshot();
        if expected
            .as_ref()
            .is_some_and(|revision| revision != current.revision())
        {
            Err(error(
                "revision_conflict",
                "expected revision does not match",
            ))
        } else {
            Ok(())
        }
    }

    fn validate_active_entry(
        &self,
        stem: &str,
        gloss: &str,
        signatures: &mut Vec<crate::SignatureId>,
    ) -> Result<(), StructuredError> {
        if stem.is_empty() {
            return Err(error("invalid_stem", "stem cannot be empty"));
        }
        if !gloss.is_empty() && self.snapshot().gloss_language.is_none() {
            return Err(error(
                "gloss_language_required",
                "set gloss language before adding a gloss",
            ));
        }
        crate::validate_shape(&self.grammar, stem)
            .map_err(|message| error("invalid_shape", message))?;
        signatures.sort();
        signatures.dedup();
        if signatures.is_empty() {
            return Err(error(
                "invalid_signatures",
                "at least one signature is required",
            ));
        }
        if signatures
            .iter()
            .any(|signature| self.catalog.resolved(signature).is_none())
        {
            return Err(error("unknown_signature", "unknown signature ID"));
        }
        Ok(())
    }

    fn ensure_current_mappings(&self, document: &mut LexiconDocument) {
        let retained: BTreeMap<_, _> = document
            .signatures
            .iter()
            .map(|mapping| (mapping.id.clone(), mapping.clone()))
            .collect();
        let referenced: BTreeSet<_> = document
            .entries
            .iter()
            .flat_map(|entry| entry.signatures.iter().cloned())
            .collect();
        document.signatures = referenced
            .iter()
            .filter_map(|id| {
                self.catalog
                    .resolved(id)
                    .map(|resolved| resolved.signature.clone())
                    .or_else(|| retained.get(id).cloned())
            })
            .collect();
    }

    pub fn import_document(
        &self,
        document: LexiconDocument,
    ) -> Result<ReconciliationReport, StructuredError> {
        self.import(ImportRequest {
            document,
            expected_revision: None,
        })
    }

    pub fn import(&self, request: ImportRequest) -> Result<ReconciliationReport, StructuredError> {
        let _mutation = self
            .mutation
            .lock()
            .expect("lexicon mutation lock poisoned");
        self.check_revision(&request.expected_revision)?;
        self.import_document_locked(request.document)
    }

    pub fn import_json(
        &self,
        json: &str,
        expected_revision: Option<Revision>,
    ) -> Result<ReconciliationReport, StructuredError> {
        let document = serde_json::from_str(json).map_err(|json_error| StructuredError {
            code: "invalid_import_json".into(),
            message: "invalid lexicon JSON".into(),
            details: serde_json::json!({"error": json_error.to_string()}),
        })?;
        self.import(ImportRequest {
            document,
            expected_revision,
        })
    }

    fn import_document_locked(
        &self,
        document: LexiconDocument,
    ) -> Result<ReconciliationReport, StructuredError> {
        if document.schema_version != LEXICON_SCHEMA_VERSION {
            return Err(error(
                "unsupported_schema",
                "unsupported lexicon schema version",
            ));
        }
        if document.grammar_name != self.grammar_name {
            return Err(error(
                "grammar_name_mismatch",
                "lexicon belongs to another grammar",
            ));
        }
        let mut mappings = BTreeMap::new();
        for mapping in document.signatures {
            if let Some(previous) = mappings.insert(mapping.id.clone(), mapping.clone()) {
                if previous != mapping {
                    return Err(error(
                        "conflicting_signature_mapping",
                        "signature ID has conflicting mappings",
                    ));
                }
                return Err(error(
                    "duplicate_signature_mapping",
                    "duplicate signature mapping",
                ));
            }
        }
        let mut ids = BTreeSet::new();
        for entry in &document.entries {
            if !ids.insert(entry.id.clone()) {
                return Err(error("duplicate_entry_id", "duplicate supplied entry ID"));
            }
            if entry.signatures.is_empty() {
                return Err(error(
                    "invalid_signatures",
                    "at least one signature is required",
                ));
            }
            let unique_signatures: BTreeSet<_> = entry.signatures.iter().collect();
            if unique_signatures.len() != entry.signatures.len() {
                return Err(error(
                    "duplicate_signature_reference",
                    "entry contains a duplicate signature reference",
                ));
            }
            if entry.signatures.iter().any(|id| !mappings.contains_key(id)) {
                return Err(error(
                    "missing_signature_mapping",
                    "entry references an unmapped signature",
                ));
            }
            if !entry.gloss.is_empty() && document.gloss_language.is_none() {
                return Err(error(
                    "gloss_language_required",
                    "a nonblank gloss requires a gloss language",
                ));
            }
            if let EntryAuthority::SuppliedOverride {
                official_entry_id, ..
            } = &entry.authority
            {
                let promoted_id = entry.id.to_dotnet_guid_string()?;
                if !official_entry_id.eq_ignore_ascii_case(&promoted_id)
                    || !self.grammar.entries.iter().any(|official| {
                        official.authored_id.eq_ignore_ascii_case(official_entry_id)
                    })
                {
                    return Err(error(
                        "invalid_override",
                        "override must identify the official entry with the same 128-bit identity",
                    ));
                }
            }
        }

        let exact_match = document.source_grammar_fingerprint == self.source_fingerprint;
        let mut entries = document.entries;
        for entry in &mut entries {
            entry.signatures.sort();
        }
        entries.sort_by(|left, right| left.id.cmp(&right.id));
        let mut inactive_entries = Vec::new();
        let mut superseded_entries = Vec::new();
        let mut roots = Vec::new();
        for entry in &mut entries {
            let mut diagnostics = Vec::new();
            if crate::validate_shape(&self.grammar, &entry.stem).is_err() {
                diagnostics.push("invalidShape".to_string());
            }
            for signature in &entry.signatures {
                if self.catalog.resolved(signature).is_none() {
                    diagnostics.push(format!("missingSignature:{}", signature.as_str()));
                }
            }
            let promoted_official = entry.id.to_dotnet_guid_string().ok().and_then(|guid| {
                self.grammar
                    .entries
                    .iter()
                    .find(|official| official.authored_id.eq_ignore_ascii_case(&guid))
                    .map(|official| official.authored_id.clone())
            });
            let is_override = matches!(entry.authority, EntryAuthority::SuppliedOverride { .. });
            if let (true, Some(official_entry_id), false) =
                (diagnostics.is_empty(), promoted_official, is_override)
            {
                entry.state = ValidationState::Superseded { official_entry_id };
                superseded_entries.push(entry.id.clone());
                for signature in &entry.signatures {
                    let resolved = self.catalog.resolved(signature).expect("checked above");
                    mappings.insert(signature.clone(), resolved.signature.clone());
                }
            } else if diagnostics.is_empty() {
                entry.state = ValidationState::Active;
                for signature in &entry.signatures {
                    let resolved = self.catalog.resolved(signature).expect("checked above");
                    mappings.insert(signature.clone(), resolved.signature.clone());
                    let authority = match &entry.authority {
                        EntryAuthority::Supplied => RootAuthority::Supplied,
                        EntryAuthority::SuppliedOverride {
                            official_entry_id, ..
                        } => RootAuthority::SuppliedOverride {
                            official_entry_id: official_entry_id.clone(),
                        },
                    };
                    roots.push(SuppliedRoot {
                        entry_id: entry.id.as_str().to_string(),
                        realization_id: format!("{}:{}", entry.id.as_str(), signature.as_str()),
                        lexical_spelling: entry.stem.clone(),
                        gloss: entry.gloss.clone(),
                        syn_fs: self.grammar.fs_interner.get(resolved.syn_fs).clone(),
                        mpr: resolved.mpr,
                        stratum: resolved.stratum,
                        authority,
                    });
                }
            } else {
                entry.state = ValidationState::Inactive { diagnostics };
                inactive_entries.push(entry.id.clone());
            }
        }
        let old = self.snapshot();
        let referenced: BTreeSet<_> = entries
            .iter()
            .flat_map(|entry| entry.signatures.iter().cloned())
            .collect();
        mappings.retain(|id, _| referenced.contains(id));
        let compatible_migration = !exact_match && inactive_entries.is_empty();
        if old.entries == entries
            && old.mappings == mappings
            && old.gloss_language == document.gloss_language
        {
            return Ok(ReconciliationReport {
                exact_match,
                compatible_migration,
                inactive_entries,
                superseded_entries,
                revision: old.revision().clone(),
                changed: false,
            });
        }
        let overlay = SuppliedRootOverlay::build(&self.grammar, roots)
            .map_err(|message| error("invalid_overlay", message))?;
        let snapshot = Arc::new(LexiconSnapshot {
            revision: Revision::new(old.revision().number() + 1),
            entries,
            mappings,
            gloss_language: document.gloss_language,
            overlay,
        });
        let revision = snapshot.revision().clone();
        *self.state.write().expect("lexicon snapshot lock poisoned") = snapshot;
        Ok(ReconciliationReport {
            exact_match,
            compatible_migration,
            inactive_entries,
            superseded_entries,
            revision,
            changed: true,
        })
    }
}

pub fn grammar_source_fingerprint(source: &str) -> String {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let digest = Sha256::digest(normalized.as_bytes());
    let mut out = String::from("sha256_");
    for byte in digest {
        use std::fmt::Write;
        write!(&mut out, "{byte:02x}").expect("writing to String cannot fail");
    }
    out
}

fn error(code: &str, message: impl Into<String>) -> StructuredError {
    StructuredError {
        code: code.into(),
        message: message.into(),
        details: serde_json::Value::Null,
    }
}
