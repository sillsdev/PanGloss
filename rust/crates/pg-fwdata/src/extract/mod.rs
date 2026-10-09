//! Object-graph → `pg_snapshot::Snapshot` extraction, split by snapshot section (`project`, `features`, `phonology`, `morphology`, `lexicon`), sharing one `Ctx`; most guid fields pass through unresolved (validated later), and this crate only dereferences a guid where the snapshot embeds the target's own data inline, warning and skipping rather than panicking when the target is missing.

mod features;
mod inventory;
mod lexicon;
mod morphology;
mod phonology;
mod project;
mod stored_analyses;

pub(crate) mod codes;

pub(crate) use inventory::tracked_kind;
pub(crate) use stored_analyses::extract as stored_analyses;

use pg_snapshot::{
    ConversionProvenance, FwClass, FwObjectRef, ImportWarningCode, InventoryKey, IssueClass,
    LoadDecisionDraft, LoadDisposition, LoadPipelineStage, LoadReasonCode, SelectionRecorder,
    Snapshot, SourceInventoryStatus, SourceRef, Warning, CONVERSION_PROVENANCE_SCHEMA_VERSION,
};

use crate::{
    xml::{RawGraph, Record},
    ImportError,
};

/// Shared extraction context: the raw object graph, accumulating warnings, and writing-system priority lists that only become known once the `project` section has been read.
pub struct Ctx<'a> {
    pub graph: &'a RawGraph,
    /// Semantic, never-fatal tolerances the extractor noticed; see `RawGraph.issues` for structural, fatal-capable graph problems.
    pub warnings: Vec<Warning>,
    /// Analysis writing systems, default first — `project.analysisWritingSystems`.
    pub analysis_ws: Vec<String>,
    /// Vernacular writing systems, default first — `project.vernacularWritingSystems`.
    pub vernacular_ws: Vec<String>,
    pub(crate) recorder: SelectionRecorder,
}

#[derive(Clone, Copy)]
pub(crate) enum ReferenceResolution<'a> {
    Found(&'a Record),
    WrongClass(&'a Record),
    Missing,
}

impl ReferenceResolution<'_> {
    pub(crate) fn failure_code(self) -> ImportWarningCode {
        match self {
            Self::Found(_) => panic!("a resolved reference has no failure code"),
            Self::WrongClass(_) => codes::UNEXPECTED_CLASS,
            Self::Missing => codes::DANGLING_REFERENCE,
        }
    }
}

impl<'a> Ctx<'a> {
    fn new(graph: &'a RawGraph) -> Self {
        let mut recorder = SelectionRecorder::for_stage(LoadPipelineStage::Import);
        inventory::seed_authored_from_graph(&mut recorder, graph);
        Ctx {
            graph,
            warnings: Vec::new(),
            analysis_ws: Vec::new(),
            vernacular_ws: Vec::new(),
            recorder,
        }
    }

    pub(crate) fn warn_with_subjects(
        &mut self,
        code: ImportWarningCode,
        msg: impl Into<String>,
        subjects: impl IntoIterator<Item = FwObjectRef>,
    ) {
        let mut warning = Warning::new(code, msg);
        warning.subjects.extend(subjects);
        self.warnings.push(warning);
    }

    pub(crate) fn subject_for_record(&self, record: &Record, field: Option<&str>) -> FwObjectRef {
        let name = record_name(record, self);
        let mut subject = FwObjectRef::new(FwClass::from_wire(&record.class))
            .guid(record.guid.clone())
            .source_class(record.class.clone());
        if let Some(name) = name.filter(|name| !name.trim().is_empty()) {
            subject = subject.name(name);
        }
        if let Some(field) = field {
            subject = subject.field(field);
        }
        subject
    }

    pub(crate) fn unresolved_subject(
        &self,
        guid: &str,
        want_class: &str,
        field: Option<&str>,
    ) -> FwObjectRef {
        let mut subject = FwObjectRef::new(FwClass::from_wire(want_class))
            .guid(guid)
            .unresolved_reference()
            .source_class(want_class);
        if let Some(field) = field {
            subject = subject.field(field);
        }
        subject
    }

    pub(crate) fn reject_with_warning(
        &mut self,
        key: InventoryKey,
        class: IssueClass,
        fatal: bool,
        source: Option<SourceRef>,
        warning: Warning,
    ) {
        let reason_code = match warning.code.as_str() {
            code if code == codes::DANGLING_REFERENCE.wire() => LoadReasonCode::MissingReference,
            code if code == codes::UNEXPECTED_CLASS.wire() => LoadReasonCode::WrongKindReference,
            _ => LoadReasonCode::ConversionIssue(warning.code.clone()),
        };
        self.warnings.push(warning.clone());
        self.recorder.rejected_with_reason(
            key,
            pg_snapshot::ConversionIssue {
                code: ImportWarningCode::from_wire_or_unregistered(&warning.code),
                class,
                source,
                fatal,
                message: warning.message,
            },
            reason_code,
        );
    }

    pub fn get(&self, guid: &str) -> Option<&'a Record> {
        self.graph.get(guid)
    }

    /// Records an attachment/expansion/setting `key` as sourced (object identities are seeded once in `Ctx::new` instead).
    pub(crate) fn authored(&mut self, key: InventoryKey) {
        self.recorder.authored(key);
    }

    pub(crate) fn considered(&mut self, key: InventoryKey) {
        self.recorder.considered(key);
    }

    pub(crate) fn not_considered(&mut self, key: InventoryKey, reason_code: LoadReasonCode) {
        self.recorder
            .not_considered(key, String::new(), reason_code);
    }

    pub(crate) fn has_load_decision(&self, key: &InventoryKey, stage: LoadPipelineStage) -> bool {
        self.recorder.has_load_decision(key, stage)
    }

    pub(crate) fn record_source_decision(
        &mut self,
        subject: InventoryKey,
        disposition: LoadDisposition,
        loaded: Option<bool>,
        reason_code: LoadReasonCode,
        issue_code: Option<String>,
    ) {
        self.recorder.record_load_decision(LoadDecisionDraft {
            subject,
            pipeline_stage: LoadPipelineStage::Import,
            context_key: String::new(),
            disposition,
            loaded,
            reason_code,
            effective_value_json: None,
            issue_code,
        });
    }

    pub(crate) fn selected(&mut self, key: InventoryKey) {
        self.recorder.selected(key);
    }

    pub(crate) fn represented(&mut self, key: InventoryKey) {
        self.recorder.represented(key);
    }

    pub(crate) fn synthesized(&mut self, key: InventoryKey) {
        self.recorder.synthesized(key);
    }

    pub(crate) fn is_represented(&self, key: &InventoryKey) -> bool {
        self.recorder.is_represented(key)
    }

    /// Records `key` rejected with no new warning, for a failure a caller has already warned about through another path (e.g. `Ctx::require` or `parser_params`'s own issues).
    pub(crate) fn record_rejected(
        &mut self,
        key: InventoryKey,
        issue: pg_snapshot::ConversionIssue,
    ) {
        let reason_code = if issue.code == codes::DANGLING_REFERENCE {
            LoadReasonCode::MissingReference
        } else if issue.code == codes::UNEXPECTED_CLASS {
            LoadReasonCode::WrongKindReference
        } else {
            LoadReasonCode::ConversionIssue(issue.code.wire().to_string())
        };
        self.recorder.rejected_with_reason(key, issue, reason_code);
    }

    pub(crate) fn require_from(
        &mut self,
        guid: &str,
        want_class: &str,
        context: &str,
        owner: &Record,
        field: &str,
    ) -> Option<&'a Record> {
        let resolution = self.resolve_reference(guid, want_class);
        self.require_resolution(guid, want_class, context, resolution, (owner, field))
    }

    /// Shares one missing/wrong-class decision between diagnostics and extraction.
    pub(crate) fn resolve_reference(
        &self,
        guid: &str,
        want_class: &str,
    ) -> ReferenceResolution<'a> {
        match self.get(guid) {
            Some(record) if record.class == want_class => ReferenceResolution::Found(record),
            Some(record) => ReferenceResolution::WrongClass(record),
            None => ReferenceResolution::Missing,
        }
    }

    pub(crate) fn subjects_for_reference(
        &self,
        guid: &str,
        want_class: &str,
        resolution: ReferenceResolution<'a>,
        owner_field: (&Record, &str),
    ) -> Vec<FwObjectRef> {
        let (owner, field) = owner_field;
        let mut subjects = vec![self.subject_for_record(owner, Some(field))];
        match resolution {
            ReferenceResolution::Found(record) => {
                subjects.push(self.subject_for_record(record, Some(field)));
            }
            ReferenceResolution::WrongClass(record) => {
                subjects.push(self.unresolved_subject(guid, want_class, Some(field)));
                subjects.push(self.subject_for_record(record, Some(field)));
            }
            ReferenceResolution::Missing => {
                subjects.push(self.unresolved_subject(guid, want_class, Some(field)));
            }
        }
        subjects
    }

    pub(crate) fn require_resolution(
        &mut self,
        guid: &str,
        want_class: &str,
        context: &str,
        resolution: ReferenceResolution<'a>,
        owner_field: (&Record, &str),
    ) -> Option<&'a Record> {
        match resolution {
            ReferenceResolution::Found(record) => Some(record),
            ReferenceResolution::WrongClass(record) => {
                let subjects =
                    self.subjects_for_reference(guid, want_class, resolution, owner_field);
                self.warn_with_subjects(
                    codes::UNEXPECTED_CLASS,
                    format!(
                        "{context}: expected {want_class} but {guid} is {}",
                        record.class
                    ),
                    subjects,
                );
                None
            }
            ReferenceResolution::Missing => {
                let subjects =
                    self.subjects_for_reference(guid, want_class, resolution, owner_field);
                self.warn_with_subjects(
                    codes::DANGLING_REFERENCE,
                    format!("{context}: dangling reference to {want_class} {guid}"),
                    subjects,
                );
                None
            }
        }
    }

    /// The best-analysis-alternative string out of a multilingual field's forms: earliest in `Ctx::analysis_ws`, falling back to the first form present (or `""`).
    pub fn best_analysis(&self, forms: &[pg_snapshot::WsForm]) -> String {
        best_alt(forms, &self.analysis_ws)
    }

    /// As `Ctx::best_analysis` but preferring `Ctx::vernacular_ws`, used for boundary-marker representations.
    pub fn best_vernacular(&self, forms: &[pg_snapshot::WsForm]) -> String {
        best_alt(forms, &self.vernacular_ws)
    }
}

fn record_name(record: &Record, ctx: &Ctx<'_>) -> Option<String> {
    if record.class == "LexEntry" {
        let citation = ctx.best_analysis(&record.node.ws_forms("CitationForm"));
        if !citation.trim().is_empty() {
            return Some(citation);
        }
        if let Some(name) = record
            .node
            .objsur_one("LexemeForm")
            .and_then(|guid| ctx.get(&guid))
            .map(|lexeme_form| ctx.best_vernacular(&lexeme_form.node.ws_forms("Form")))
            .filter(|name| !name.trim().is_empty())
        {
            return Some(name);
        }
    }
    let fields = match record.class.as_str() {
        "LexEntry" => ["CitationForm", "Name", "Abbreviation"],
        "MoStemAllomorph" | "MoAffixAllomorph" | "MoAffixProcess" | "MoForm" => {
            ["Form", "Name", "CitationForm"]
        }
        _ => ["Name", "Abbreviation", "Form"],
    };
    for field in fields {
        let forms = record.node.ws_forms(field);
        let name = ctx.best_analysis(&forms);
        if !name.trim().is_empty() {
            return Some(name);
        }
    }
    None
}

fn best_alt(forms: &[pg_snapshot::WsForm], priority: &[String]) -> String {
    for ws in priority {
        if let Some(f) = forms.iter().find(|f| &f.ws == ws) {
            return f.form.clone();
        }
    }
    forms.first().map(|f| f.form.clone()).unwrap_or_default()
}

/// Extract a whole `Snapshot` from a parsed object graph. `filename_stem` is the `.fwdata` file's stem, used as `project.name` since FieldWorks derives the project name from the file, never from the XML.
pub fn extract(
    graph: &RawGraph,
    filename_stem: &str,
) -> Result<(Snapshot, Vec<Warning>), ImportError> {
    let (snapshot, warnings, _recorder) = extract_recording(graph, filename_stem)?;
    Ok((snapshot, warnings))
}

/// As [`extract`], but also returns the unfinished [`SelectionRecorder`] `import_file_measured` reads into an `InventoryDelta`; `conversion_provenance` itself comes from an independently finished clone, so this changes nothing about `extract`'s own behaviour.
pub(crate) fn extract_recording(
    graph: &RawGraph,
    filename_stem: &str,
) -> Result<(Snapshot, Vec<Warning>, SelectionRecorder), ImportError> {
    let mut ctx = Ctx::new(graph);

    let lang_project = project::find_lang_project(&mut ctx, filename_stem);
    let project = project::extract_project(&mut ctx, lang_project, filename_stem);
    ctx.analysis_ws = project.analysis_writing_systems.clone();
    ctx.vernacular_ws = project.vernacular_writing_systems.clone();

    let feature_systems = features::extract_feature_systems(&mut ctx, lang_project);
    let phonology =
        phonology::extract_phonology(&mut ctx, lang_project, &feature_systems, &project.name);
    let morphology =
        morphology::extract_morphology(&mut ctx, lang_project, &feature_systems, &project.name)?;
    let lexicon = lexicon::extract_lexicon(&mut ctx, &feature_systems, &morphology);

    morphology::check_stale_adhoc_morpheme_rules(&mut ctx, &morphology, &lexicon);
    finalize_unrecorded_tracked_objects(&mut ctx);

    let (graph_to_snapshot, recorder_issues, _) = ctx.recorder.clone().finish_with_load_decisions();
    let mut snapshot = Snapshot::new(project, feature_systems, phonology, morphology, lexicon);
    let mut import_issues = graph.issues.clone();
    import_issues.extend(recorder_issues);
    let source_inventory_status = if import_issues.iter().any(|issue| issue.fatal) {
        SourceInventoryStatus::ImportedWithFatalIssues
    } else {
        SourceInventoryStatus::ImportedComplete
    };
    for source_object in graph.source_objects() {
        let decision = if !source_object.handled {
            Some((LoadReasonCode::UnknownClass, None))
        } else if source_object.raw_guid.is_empty() {
            Some((LoadReasonCode::MissingGuid, Some(codes::MISSING_GUID)))
        } else if source_object.duplicate && !source_object.retained {
            Some((LoadReasonCode::DuplicateHeader, Some(codes::DUPLICATE_GUID)))
        } else {
            None
        };
        if let Some((reason_code, issue_code)) = decision {
            ctx.record_source_decision(
                source_object.key(),
                if source_object.handled {
                    LoadDisposition::Rejected
                } else {
                    LoadDisposition::NotConsidered
                },
                if source_object.handled {
                    Some(false)
                } else {
                    None
                },
                reason_code,
                issue_code.map(|code| code.wire().to_string()),
            );
        }
    }
    let (_, _, import_load_decisions) = ctx.recorder.clone().finish_with_load_decisions();
    snapshot.conversion_provenance = ConversionProvenance {
        schema_version: CONVERSION_PROVENANCE_SCHEMA_VERSION,
        source_inventory_status,
        source_census: graph.census(),
        source_objects: graph.source_objects(),
        graph_to_snapshot,
        import_load_decisions,
        import_issues,
    };
    Ok((snapshot, ctx.warnings, ctx.recorder))
}

/// Finalizes tracked source objects that normal extraction did not reach.
fn finalize_unrecorded_tracked_objects(ctx: &mut Ctx<'_>) {
    use std::collections::BTreeMap;

    use pg_snapshot::{InventoryKind, LoadDecisionDraft};

    fn collect_refs(node: &crate::node::Node, out: &mut Vec<String>) {
        if node.tag == "objsur" {
            if let Some(guid) = node.attr("guid") {
                out.push(guid.to_string());
            }
        }
        for child in &node.children {
            collect_refs(child, out);
        }
    }

    let mut incoming = BTreeMap::<String, Vec<(String, String)>>::new();
    for owner in ctx.graph.records.values() {
        let mut refs = Vec::new();
        collect_refs(&owner.node, &mut refs);
        refs.sort();
        refs.dedup();
        for target in refs {
            incoming
                .entry(target)
                .or_default()
                .push((owner.class.clone(), owner.guid.clone()));
        }
    }
    for owners in incoming.values_mut() {
        owners.sort();
        owners.dedup();
    }

    let mut disabled_rule_targets = BTreeMap::<String, Vec<String>>::new();
    for rule in ctx.graph.records.values().filter(|record| {
        matches!(record.class.as_str(), "PhRegularRule" | "PhMetathesisRule")
            && record.node.val_bool("Disabled").unwrap_or(false)
    }) {
        let mut pending = Vec::new();
        collect_refs(&rule.node, &mut pending);
        let mut seen = std::collections::BTreeSet::new();
        while let Some(target) = pending.pop() {
            if !seen.insert(target.clone()) {
                continue;
            }
            if ctx.graph.get(&target).is_some_and(|record| {
                inventory::tracked_kind(&record.class).is_some_and(|kind| {
                    matches!(
                        kind,
                        InventoryKind::PhonologicalContext | InventoryKind::RuleFeature
                    )
                })
            }) {
                disabled_rule_targets
                    .entry(target.clone())
                    .or_default()
                    .push(rule.guid.clone());
            }
            if ctx.graph.get(&target).is_some_and(|record| {
                inventory::tracked_kind(&record.class) == Some(InventoryKind::PhonologicalContext)
            }) {
                if let Some(record) = ctx.graph.get(&target) {
                    collect_refs(&record.node, &mut pending);
                }
            }
        }
    }
    for rules in disabled_rule_targets.values_mut() {
        rules.sort();
        rules.dedup();
    }

    let mut records: Vec<_> = ctx
        .graph
        .records
        .values()
        .filter_map(|record| inventory::tracked_kind(&record.class).map(|kind| (kind, record)))
        .collect();
    records.sort_by(|(left_kind, left), (right_kind, right)| {
        (left_kind, left.guid.as_str()).cmp(&(right_kind, right.guid.as_str()))
    });

    for (guid, rule_guids) in &disabled_rule_targets {
        let Some(record) = ctx.graph.get(guid) else {
            continue;
        };
        let Some(kind) = inventory::tracked_kind(&record.class) else {
            continue;
        };
        ctx.considered(InventoryKey::object(kind, guid.clone()));
        for rule_guid in rule_guids {
            ctx.recorder.not_considered(
                InventoryKey::object(kind, guid.clone()),
                format!("phonologicalRule:{rule_guid}"),
                LoadReasonCode::Disabled,
            );
        }
    }

    for (kind, record) in records {
        if !matches!(
            kind,
            InventoryKind::FeatureStructure
                | InventoryKind::Msa
                | InventoryKind::PhonologicalContext
                | InventoryKind::RuleFeature
        ) {
            continue;
        }
        let key = InventoryKey::object(kind, record.guid.clone());
        if ctx.has_load_decision(&key, LoadPipelineStage::Import) {
            continue;
        }

        let owners = incoming.get(&record.guid).cloned().unwrap_or_default();
        match kind {
            InventoryKind::FeatureStructure if !owners.is_empty() => {
                ctx.recorder.record_load_decision(LoadDecisionDraft {
                    subject: key,
                    pipeline_stage: LoadPipelineStage::Import,
                    context_key: String::new(),
                    disposition: LoadDisposition::MetadataOnly,
                    loaded: None,
                    reason_code: LoadReasonCode::MetadataOnly,
                    effective_value_json: None,
                    issue_code: None,
                });
            }
            InventoryKind::Msa if !owners.is_empty() => {
                for (owner_class, owner_guid) in owners {
                    ctx.recorder.record_load_decision(LoadDecisionDraft {
                        subject: key.clone(),
                        pipeline_stage: LoadPipelineStage::Import,
                        context_key: format!("owner:{owner_class}:{owner_guid}"),
                        disposition: LoadDisposition::NotConsidered,
                        loaded: None,
                        reason_code: LoadReasonCode::OwnerNotLoaded,
                        effective_value_json: None,
                        issue_code: None,
                    });
                }
            }
            InventoryKind::PhonologicalContext | InventoryKind::RuleFeature => {
                if !owners.is_empty() {
                    for (owner_class, owner_guid) in owners {
                        ctx.recorder.record_load_decision(LoadDecisionDraft {
                            subject: key.clone(),
                            pipeline_stage: LoadPipelineStage::Import,
                            context_key: format!("owner:{owner_class}:{owner_guid}"),
                            disposition: LoadDisposition::Rejected,
                            loaded: Some(false),
                            reason_code: LoadReasonCode::ConversionIssue(
                                "sourceObjectNotResolved".into(),
                            ),
                            effective_value_json: None,
                            issue_code: None,
                        });
                    }
                } else {
                    ctx.recorder
                        .not_considered(key, String::new(), LoadReasonCode::Unreferenced);
                }
            }
            InventoryKind::FeatureStructure => {
                ctx.recorder
                    .not_considered(key, String::new(), LoadReasonCode::Unreferenced)
            }
            InventoryKind::Msa => {
                ctx.recorder
                    .not_considered(key, String::new(), LoadReasonCode::Unreferenced)
            }
            _ => unreachable!("tracked-object finalizer filters its supported kinds"),
        }
    }
}
