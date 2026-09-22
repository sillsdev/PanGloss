//! The `hc-*` grammar-authoring health checks, ported from C#
//! `SIL.Machine.Morphology.HermitCrab.GrammarHealthChecker`/`GrammarHealthCheckFinding`
//! (`sillsdev/machine` PR 475, branch `feature/grammar-health-checker`). Diagnostic only: running
//! these checks never fails, never refuses, and never changes how a [`Grammar`] compiles or
//! parses -- it reports, and the caller decides.
//!
//! This answers a *different* question from `pg_health::health`'s FST-compilation vocabulary
//! (`Severity`/`FindingCode`/`FindingClass`): that module asks "can this grammar be compiled to an
//! FST and published" (gated on `FindingClass`: Representability/Readiness/Containment/Process).
//! These checks ask "is this grammar well-formed for its author" -- an authoring-correctness
//! question with no publication-blocking tier and no representability/containment axis, so a
//! separate, smaller vocabulary lives here rather than contorting the FST schema's severity/class
//! pair to fit it. Wire codes are the stable C# strings (`hc-undeclared-segment`,
//! `hc-duplicate-feature-bundle`, `hc-partial-morpheme`) so the two implementations' output can be
//! compared directly.
//!
//! Lives in `pg-grammar`, not `pg-health`, because the checks read [`Grammar`] directly.
//! `pg-health` is a leaf crate whose only dependencies are `serde`/`serde_json`, kept that way so
//! it builds for `wasm32-unknown-unknown` with no compiler/model machinery in its graph
//! (`pg-wasm/tests/wasm_excludes_compiler.rs`); adding a `pg-grammar` dependency there would be a
//! new, unnecessary edge. `pg-grammar` is already an unconditional dependency of `pg-wasm` (see
//! that crate's `Cargo.toml`), so this module adds no new wasm32 exposure of its own.
//!
//! # What is NOT ported
//! C#'s `CheckUndeclaredSegments` also flags a `SegmentNaturalClass` member whose
//! `CharacterDefinition.CharacterDefinitionTable` is not one of the language's declared tables.
//! [`crate::model::NaturalClassKind::Segments`] stores only a per-table
//! [`crate::chardef::CharDefId`] with no owning-table reference, and `load.rs`'s natural-class pass
//! resolves every `<Segment segment="...">` reference through an index built ONLY from
//! already-declared tables -- so a member naming an undeclared table cannot exist once a grammar
//! has loaded, and the model carries no field this check could read even by hand construction.
//! Porting it would require adding a table reference to every `Segments` member, a model-shape
//! change out of scope for this port.

use crate::chardef::{CharDef, CharDefId, CharDefKind, CharDefTable};
use crate::grammar_health_presentation::{morph_rule_link, prepare_report, subject_link};
use crate::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef, OutputAction, TableId};
use crate::stats_identity;
use pg_shape::{NodeKind, Shape, NO_CHAR_DEF};

/// How serious a [`GrammarHealthCheckFinding`] is. Mirrors C# `GrammarHealthSeverity`: `Error` means
/// the engine behaves incorrectly (or refuses the word outright) whenever the offending construct
/// is exercised; `Warning` means the construct is a genuine reliability risk whose actual impact
/// depends on how the grammar's rules use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrammarHealthSeverity {
    Warning,
    Error,
}

impl GrammarHealthSeverity {
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// The stable finding codes this module reports (C# `GrammarHealthCodes`). Treat
/// [`GrammarHealthCode::wire`], not [`GrammarHealthCheckFinding::message`], as the identifier a host
/// filters/suppresses/tests on -- the message text is free to change. Serializes as the bare wire
/// string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum GrammarHealthCode {
    #[serde(rename = "hc-undeclared-segment")]
    UndeclaredSegment,
    #[serde(rename = "hc-duplicate-feature-bundle")]
    DuplicateFeatureBundle,
    #[serde(rename = "hc-partial-morpheme")]
    PartialMorpheme,
}

impl GrammarHealthCode {
    /// Every grammar-health code in the stable contract. The exhaustive `group_name` match below
    /// makes adding a code without a sidebar label a compile error.
    pub const ALL: &'static [Self] = &[
        Self::UndeclaredSegment,
        Self::DuplicateFeatureBundle,
        Self::PartialMorpheme,
    ];

    /// The stable C# wire string this code shares with `GrammarHealthCodes`.
    pub const fn wire(self) -> &'static str {
        match self {
            Self::UndeclaredSegment => "hc-undeclared-segment",
            Self::DuplicateFeatureBundle => "hc-duplicate-feature-bundle",
            Self::PartialMorpheme => "hc-partial-morpheme",
        }
    }

    /// Stable plain-language sidebar/log grouping label. Keep these short and linguist-facing.
    pub const fn group_name(self) -> &'static str {
        match self {
            Self::UndeclaredSegment => "Missing segment definition",
            Self::DuplicateFeatureBundle => "Duplicate segment features",
            Self::PartialMorpheme => "Partial morpheme analysis",
        }
    }
}

/// The subject kind used by every grammar-health renderer. This is deliberately separate from
/// the finding code: a finding can name several kinds of model object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrammarHealthSubjectKind {
    Table,
    CharDef,
    LexEntry,
    MorphRule,
}

impl GrammarHealthSubjectKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Table => "character-definition table",
            Self::CharDef => "character definition",
            Self::LexEntry => "lexical entry",
            Self::MorphRule => "morphological rule",
        }
    }
}

/// FieldWorks navigation data. The tool is selected by PanGloss from the subject kind; an absent
/// URL is explicit and never a guessed or malformed link.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FieldWorksLink {
    pub guid: Option<String>,
    pub tool: String,
    pub url: Option<String>,
    pub url_unavailable: Option<String>,
}

/// One structured item a grammar-health finding references. `title` is the only identity a human
/// report should display; `internal_id` is retained solely for tooling and navigation joins.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GrammarHealthSubject {
    pub kind: GrammarHealthSubjectKind,
    pub title: String,
    pub subtitle: Option<String>,
    pub internal_id: String,
    pub fieldworks: FieldWorksLink,
}

impl GrammarHealthSubject {
    pub fn where_text(&self) -> String {
        let mut text = self.title.clone();
        if let Some(subtitle) = self.subtitle.as_deref().filter(|text| !text.is_empty()) {
            text.push_str(" (");
            text.push_str(subtitle);
            text.push(')');
        }
        if let Some(url) = self.fieldworks.url.as_deref() {
            text.push_str(" [");
            text.push_str(url);
            text.push(']');
        } else if let Some(reason) = self.fieldworks.url_unavailable.as_deref() {
            text.push_str(" [FieldWorks link unavailable: ");
            text.push_str(reason);
            text.push(']');
        }
        text
    }

    fn log_title(&self, include_guid: bool) -> String {
        if !include_guid {
            return self.title.clone();
        }
        match self.fieldworks.guid.as_deref() {
            Some(guid) => format!("{} [guid {guid}]", self.title),
            None => format!("{} [guid unavailable]", self.title),
        }
    }
}

/// One problem found by check_grammar_health. JSON uses `problem` for the human message while
/// the Rust field remains `message` for source compatibility with the existing API.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GrammarHealthCheckFinding {
    pub severity: GrammarHealthSeverity,
    pub code: GrammarHealthCode,
    /// Stable plain-language grouping label, duplicated per finding for simple JSON consumers.
    pub group_name: String,
    #[serde(rename = "problem")]
    pub message: String,
    pub subjects: Vec<GrammarHealthSubject>,
}

impl GrammarHealthCheckFinding {
    fn validation_message(&self, finding_index: usize) -> Option<String> {
        let prefix = format!(
            "grammar-health finding {finding_index} ({})",
            self.code.wire()
        );
        if self.group_name.trim().is_empty() {
            return Some(format!("{prefix}: missing group_name"));
        }
        if self.group_name != self.code.group_name() {
            return Some(format!(
                "{prefix}: group_name does not match the code-owned label"
            ));
        }
        if self.message.trim().is_empty() {
            return Some(format!("{prefix}: missing message"));
        }
        if self.subjects.is_empty() {
            return Some(format!("{prefix}: missing subjects"));
        }
        for (subject_index, subject) in self.subjects.iter().enumerate() {
            let subject_prefix = format!("{prefix} subject {subject_index}");
            if subject.title.trim().is_empty() {
                return Some(format!("{subject_prefix}: missing title"));
            }
            if subject.internal_id.trim().is_empty() {
                return Some(format!("{subject_prefix}: missing internal_id"));
            }
            if subject.fieldworks.tool.trim().is_empty() {
                return Some(format!("{subject_prefix}: missing fieldworks.tool"));
            }
            match (
                subject.fieldworks.url.as_deref(),
                subject.fieldworks.url_unavailable.as_deref(),
            ) {
                (Some(url), None) if !url.trim().is_empty() => {}
                (None, Some(reason)) if !reason.trim().is_empty() => {}
                (Some(_), Some(_)) => {
                    return Some(format!(
                        "{subject_prefix}: fieldworks link must have either url or url_unavailable, not both"
                    ));
                }
                (Some(_), None) => {
                    return Some(format!(
                        "{subject_prefix}: fieldworks.url must be nonblank"
                    ));
                }
                (None, Some(_)) => {
                    return Some(format!(
                        "{subject_prefix}: fieldworks.url_unavailable must be nonblank"
                    ));
                }
                (None, None) => {
                    return Some(format!(
                        "{subject_prefix}: missing fieldworks.url or fieldworks.url_unavailable"
                    ));
                }
            }
        }
        None
    }

    pub fn is_complete(&self) -> bool {
        self.validation_message(0).is_none()
    }

    fn log_line(&self, include_guids: bool) -> String {
        let kinds = self
            .subjects
            .iter()
            .map(|subject| subject.kind.label())
            .collect::<Vec<_>>();
        let titles = self
            .subjects
            .iter()
            .map(|subject| subject.log_title(include_guids))
            .collect::<Vec<_>>();
        format!(
            "{} [{}] {}: {} - {}",
            self.severity.wire(),
            self.group_name,
            kinds.join(", "),
            titles.join(", "),
            self.message.trim()
        )
    }
}

/// Validate the complete report input before any presentation adapter runs.
pub fn validate_findings(findings: &[GrammarHealthCheckFinding]) -> serde_json::Result<()> {
    for (finding_index, finding) in findings.iter().enumerate() {
        if let Some(message) = finding.validation_message(finding_index) {
            return Err(<serde_json::Error as serde::de::Error>::custom(message));
        }
    }
    Ok(())
}

/// Versioned JSON contract for Motif and other structured consumers. Version 1 is the first
/// contract containing structured subjects and FieldWorks navigation data; the previous bare array
/// is intentionally not reused after this shape change.
///
/// The wire shape is { schema_version: 1, findings: [{ severity, code, group_name, problem,
/// subjects: [{ kind, title, subtitle, internal_id, fieldworks: { guid, tool, url,
/// url_unavailable } }] }] }. 	itle is the human identity; internal_id is secondary
/// tooling data, and group_name is a stable linguist-facing grouping label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarHealthJsonReport {
    pub schema_version: u32,
    pub findings: Vec<GrammarHealthCheckFinding>,
}

pub const GRAMMAR_HEALTH_SCHEMA_VERSION: u32 = 1;

fn validate_report(
    schema_version: u32,
    findings: &[GrammarHealthCheckFinding],
) -> serde_json::Result<()> {
    if schema_version != GRAMMAR_HEALTH_SCHEMA_VERSION {
        return Err(<serde_json::Error as serde::de::Error>::custom(format!(
            "unsupported grammar-health schema version {schema_version}; expected {GRAMMAR_HEALTH_SCHEMA_VERSION}"
        )));
    }
    validate_findings(findings)
}

impl GrammarHealthJsonReport {
    /// Encode the canonical versioned grammar-health report.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Decode the canonical versioned grammar-health report.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        let value: serde_json::Value = serde_json::from_str(json)?;
        if !value.is_object() {
            return Err(<serde_json::Error as serde::de::Error>::custom(
                "grammar-health report must be an object with schema_version and findings; bare findings arrays are unsupported",
            ));
        }
        serde_json::from_value(value)
    }
}

impl serde::Serialize for GrammarHealthJsonReport {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        validate_report(self.schema_version, &self.findings).map_err(|error| {
            <S::Error as serde::ser::Error>::custom(error.to_string())
        })?;

        #[derive(serde::Serialize)]
        struct Wire<'a> {
            schema_version: u32,
            findings: &'a [GrammarHealthCheckFinding],
        }

        Wire {
            schema_version: self.schema_version,
            findings: &self.findings,
        }
        .serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for GrammarHealthJsonReport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <serde_json::Value as serde::Deserialize>::deserialize(deserializer)?;
        let Some(object) = value.as_object() else {
            return Err(<D::Error as serde::de::Error>::custom(
                "grammar-health report must be an object with schema_version and findings; bare findings arrays are unsupported",
            ));
        };
        let Some(schema_value) = object.get("schema_version") else {
            return Err(<D::Error as serde::de::Error>::custom(
                "grammar-health report is missing schema_version",
            ));
        };
        let Some(schema_version) = schema_value.as_u64() else {
            return Err(<D::Error as serde::de::Error>::custom(
                "grammar-health schema_version must be an unsigned integer",
            ));
        };
        let schema_version = u32::try_from(schema_version).map_err(|_| {
            <D::Error as serde::de::Error>::custom(
                "grammar-health schema_version is outside the supported unsigned range",
            )
        })?;
        let Some(findings_value) = object.get("findings") else {
            return Err(<D::Error as serde::de::Error>::custom(
                "grammar-health report is missing findings",
            ));
        };
        let findings = serde_json::from_value::<Vec<GrammarHealthCheckFinding>>(
            findings_value.clone(),
        )
        .map_err(|error| <D::Error as serde::de::Error>::custom(error.to_string()))?;
        validate_report(schema_version, &findings)
            .map_err(|error| <D::Error as serde::de::Error>::custom(error.to_string()))?;
        Ok(Self {
            schema_version,
            findings,
        })
    }
}

/// Render complete findings as one nonblank plain-text line per finding.
///
/// include_guids is opt-in because the title remains the human identity in the default log.
pub fn render_log(
    findings: &[GrammarHealthCheckFinding],
    include_guids: bool,
) -> serde_json::Result<String> {
    validate_findings(findings)?;
    Ok(findings
        .iter()
        .map(|finding| finding.log_line(include_guids))
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Render the full Motif-facing JSON shape, including FieldWorks links and explicit link reasons.
pub fn render_json(findings: &[GrammarHealthCheckFinding]) -> Result<String, serde_json::Error> {
    validate_findings(findings)?;
    GrammarHealthJsonReport {
        schema_version: GRAMMAR_HEALTH_SCHEMA_VERSION,
        findings: findings.to_vec(),
    }
    .to_json()
}

/// Runs every registered check against grammar. `fieldworks_project` is caller-supplied because
/// the project/database name is not part of the compiled grammar.
pub fn check_grammar_health(
    grammar: &Grammar,
    fieldworks_project: Option<&str>,
) -> Vec<GrammarHealthCheckFinding> {
    let mut findings = Vec::new();
    check_duplicate_feature_bundles(grammar, &mut findings);
    check_undeclared_segments(grammar, &mut findings);
    check_partial_morphemes(grammar, &mut findings);
    prepare_report(&mut findings, fieldworks_project);
    validate_findings(&findings).expect("grammar-health emitted an incomplete finding");
    findings
}

// --- hc-duplicate-feature-bundle -------------------------------------------------------------

/// Distinct-bundle segments only; skipped for a zero-feature grammar, where every bundle is the same empty struct by construction.
fn check_duplicate_feature_bundles(
    grammar: &Grammar,
    findings: &mut Vec<GrammarHealthCheckFinding>,
) {
    if grammar.phon_features.is_empty() {
        return;
    }
    for (table_idx, table) in grammar.char_tables.iter().enumerate() {
        let table_id = TableId(table_idx as u16);
        let mut segs: Vec<(CharDefId, &CharDef)> = table
            .iter()
            .filter(|(_, cd)| cd.kind() == CharDefKind::Segment)
            .collect();
        segs.sort_by(|(_, a), (_, b)| first_representation(a).cmp(first_representation(b)));

        let mut groups: Vec<Vec<(CharDefId, &CharDef)>> = Vec::new();
        for entry in segs {
            match groups
                .iter_mut()
                .find(|g| stripped_lanes(g[0].1) == stripped_lanes(entry.1))
            {
                Some(group) => group.push(entry),
                None => groups.push(vec![entry]),
            }
        }

        for group in groups {
            if group.len() < 2 {
                continue;
            }
            let names: Vec<&str> = group
                .iter()
                .map(|(_, cd)| first_representation(cd))
                .collect();
            let mut subjects = vec![table_subject(table_id, table)];
            subjects.extend(
                group
                    .iter()
                    .map(|(id, cd)| char_def_subject(table_id, *id, cd, table)),
            );
            findings.push(GrammarHealthCheckFinding {
                severity: GrammarHealthSeverity::Warning,
                code: GrammarHealthCode::DuplicateFeatureBundle,
                group_name: GrammarHealthCode::DuplicateFeatureBundle.group_name().to_string(),
                message: format!(
                    "Character definition table '{}' has {} segments with an identical \
                     phonological feature bundle, so a segment-changing rule cannot reliably \
                     tell them apart: {}.",
                    table_display_name(table),
                    group.len(),
                    names.join(", ")
                ),
                subjects,
            });
        }
    }
}

// The `Type` lane is always appended last (`PhonFeatureSystem::from_raw`), so slicing it off is a bare truncation, not a search.
fn stripped_lanes(cd: &CharDef) -> &[u64] {
    let lanes = cd.feature_lanes();
    &lanes[..lanes.len() - 1]
}

fn first_representation(cd: &CharDef) -> &str {
    cd.representations()
        .iter()
        .map(String::as_str)
        .find(|representation| !representation.trim().is_empty())
        .unwrap_or("unnamed character definition")
}

fn table_display_name(table: &CharDefTable) -> &str {
    table
        .name()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("unnamed character-definition table")
}

fn make_subject(
    kind: GrammarHealthSubjectKind,
    title: String,
    subtitle: Option<String>,
    internal_id: String,
    fieldworks: FieldWorksLink,
) -> GrammarHealthSubject {
    GrammarHealthSubject {
        kind,
        title,
        subtitle,
        internal_id,
        fieldworks,
    }
}

fn table_subject(id: TableId, table: &CharDefTable) -> GrammarHealthSubject {
    make_subject(
        GrammarHealthSubjectKind::Table,
        table_display_name(table).to_string(),
        None,
        format!("table#{}:{}", id.0, table.xml_id()),
        subject_link(GrammarHealthSubjectKind::Table, table.xml_id()),
    )
}

fn char_def_subject(
    table_id: TableId,
    id: CharDefId,
    cd: &CharDef,
    table: &CharDefTable,
) -> GrammarHealthSubject {
    make_subject(
        GrammarHealthSubjectKind::CharDef,
        first_representation(cd).to_string(),
        Some(format!("in {}", table_display_name(table))),
        format!("table#{}:char_def#{}:{}", table_id.0, id.0, cd.xml_id()),
        subject_link(GrammarHealthSubjectKind::CharDef, cd.xml_id()),
    )
}

fn lex_entry_subject(grammar: &Grammar, id: LexEntryId) -> GrammarHealthSubject {
    let entry = &grammar.entries[id.0 as usize];
    make_subject(
        GrammarHealthSubjectKind::LexEntry,
        stats_identity::lex_entry_identity(grammar, id).label,
        None,
        format!("lex_entry#{}:{}", id.0, entry.authored_id),
        subject_link(GrammarHealthSubjectKind::LexEntry, &entry.authored_id),
    )
}

fn morph_rule_source_id(grammar: &Grammar, id: MRuleId) -> String {
    match &grammar.mrules[id.0 as usize] {
        MorphRuleDef::Compounding(def) => def.xml_id.clone(),
        MorphRuleDef::AffixProcess(def) => grammar
            .morphemes
            .get(def.morpheme.0 as usize)
            .and_then(|info| {
                info.source_msa_guid
                    .as_deref()
                    .or(info.source_infl_type_guid.as_deref())
                    .or(Some(info.xml_key.as_str()))
            })
            .unwrap_or_default()
            .to_string(),
        MorphRuleDef::Realizational(def) => grammar
            .morphemes
            .get(def.morpheme.0 as usize)
            .and_then(|info| {
                info.source_msa_guid
                    .as_deref()
                    .or(info.source_infl_type_guid.as_deref())
                    .or(Some(info.xml_key.as_str()))
            })
            .unwrap_or_default()
            .to_string(),
    }
}

fn morph_rule_subject(grammar: &Grammar, id: MRuleId) -> GrammarHealthSubject {
    let source_id = morph_rule_source_id(grammar, id);
    let kind = match &grammar.mrules[id.0 as usize] {
        MorphRuleDef::Compounding(_) => "compounding rule",
        MorphRuleDef::AffixProcess(_) => "affix-process rule",
        MorphRuleDef::Realizational(_) => "realizational rule",
    };
    make_subject(
        GrammarHealthSubjectKind::MorphRule,
        stats_identity::morph_rule_identity(grammar, id).label,
        Some(kind.to_string()),
        format!("morph_rule#{}:{}", id.0, source_id),
        morph_rule_link(grammar, id, &source_id),
    )
}

// --- hc-undeclared-segment ---------------------------------------------------------------------

/// Lex-entry allomorph segments plus rule InsertSegments, walking only a stratum's ordinary
/// mrules -- matches C#'s own scope, so a template-slot-only rule is outside this check too.
fn check_undeclared_segments(
    grammar: &Grammar,
    findings: &mut Vec<GrammarHealthCheckFinding>,
) {
    for stratum in &grammar.strata {
        let Some(table) = grammar.char_tables.get(stratum.table.0 as usize) else {
            continue;
        };
        for &entry_id in &stratum.entries {
            let Some(entry) = grammar.entries.get(entry_id.0 as usize) else {
                continue;
            };
            let name = stats_identity::lex_entry_identity(grammar, entry_id).label;
            for allomorph in &entry.allomorphs {
                check_segments_declared(
                    table,
                    stratum.table,
                    &allomorph.shape.shape,
                    &format!("Lexical entry '{name}' allomorph '{}'", allomorph.shape.text),
                    lex_entry_subject(grammar, entry_id),
                    findings,
                );
            }
        }

        for &rule_id in &stratum.mrules {
            let Some(rule) = grammar.mrules.get(rule_id.0 as usize) else {
                continue;
            };
            let name = stats_identity::morph_rule_identity(grammar, rule_id).label;
            let subject = morph_rule_subject(grammar, rule_id);
            match rule {
                MorphRuleDef::AffixProcess(def) => {
                    for allomorph in &def.allomorphs {
                        for action in &allomorph.rhs {
                            check_insert_segments(
                                grammar,
                                action,
                                "Morphological rule",
                                &name,
                                subject.clone(),
                                findings,
                            );
                        }
                    }
                }
                MorphRuleDef::Compounding(def) => {
                    for subrule in &def.subrules {
                        for action in &subrule.rhs {
                            check_insert_segments(
                                grammar,
                                action,
                                "Compounding rule",
                                &name,
                                subject.clone(),
                                findings,
                            );
                        }
                    }
                }
                MorphRuleDef::Realizational(_) => {}
            }
        }
    }
}

fn check_insert_segments(
    grammar: &Grammar,
    action: &OutputAction,
    kind_label: &str,
    rule_name: &str,
    owner_subject: GrammarHealthSubject,
    findings: &mut Vec<GrammarHealthCheckFinding>,
) {
    let OutputAction::InsertSegments {
        table: table_id,
        shape,
    } = action
    else {
        return;
    };
    let Some(table) = grammar.char_tables.get(table_id.0 as usize) else {
        return;
    };
    check_segments_declared(
        table,
        *table_id,
        &shape.shape,
        &format!("{kind_label} '{rule_name}' inserted segments '{}'", shape.text),
        owner_subject,
        findings,
    );
}

/// Boundary/anchor nodes are structural and an abstract natural-class node is already declared,
/// so both are skipped -- mirrors C#'s Segment-type-only filter.
fn check_segments_declared(
    table: &CharDefTable,
    table_id: TableId,
    shape: &Shape,
    where_desc: &str,
    owner_subject: GrammarHealthSubject,
    findings: &mut Vec<GrammarHealthCheckFinding>,
) {
    for (_, kind, cd, _) in shape.interior() {
        if kind != NodeKind::Segment || cd == NO_CHAR_DEF {
            continue;
        }
        let declared =
            (cd as usize) < table.len() && table.get(CharDefId(cd)).kind() == CharDefKind::Segment;
        if declared {
            continue;
        }
        findings.push(GrammarHealthCheckFinding {
            severity: GrammarHealthSeverity::Error,
            code: GrammarHealthCode::UndeclaredSegment,
            group_name: GrammarHealthCode::UndeclaredSegment.group_name().to_string(),
            message: format!(
                "{where_desc} contains an undeclared segment; character definition table '{}' does not declare it.",
                table_display_name(table)
            ),
            subjects: vec![
                table_subject(table_id, table),
                owner_subject.clone(),
            ],
        });
    }
}

// --- hc-partial-morpheme -------------------------------------------------------------------

/// Reads partial directly (the model's own published fact); every rule lives once in Grammar::mrules
/// regardless of slot references, so one pass already dedups without a seen-set.
fn check_partial_morphemes(
    grammar: &Grammar,
    findings: &mut Vec<GrammarHealthCheckFinding>,
) {
    for (i, entry) in grammar.entries.iter().enumerate() {
        if !entry.partial {
            continue;
        }
        let id = LexEntryId(i as u32);
        let name = stats_identity::lex_entry_identity(grammar, id).label;
        findings.push(partial_finding(
            "Lexical entry",
            &name,
            lex_entry_subject(grammar, id),
        ));
    }
    for (i, rule) in grammar.mrules.iter().enumerate() {
        let MorphRuleDef::AffixProcess(def) = rule else {
            continue;
        };
        if !def.partial {
            continue;
        }
        let id = MRuleId(i as u32);
        let name = stats_identity::morph_rule_identity(grammar, id).label;
        findings.push(partial_finding(
            "Morphological rule",
            &name,
            morph_rule_subject(grammar, id),
        ));
    }
}

fn partial_finding(
    kind_label: &str,
    name: &str,
    subject: GrammarHealthSubject,
) -> GrammarHealthCheckFinding {
    GrammarHealthCheckFinding {
        severity: GrammarHealthSeverity::Warning,
        code: GrammarHealthCode::PartialMorpheme,
        group_name: GrammarHealthCode::PartialMorpheme.group_name().to_string(),
        message: format!(
            "{kind_label} '{name}' is partially analyzed. Supply its missing category or \
             template/slot analysis; leaving it partial can broaden analysis and disable safe \
             final-template pruning."
        ),
        subjects: vec![subject],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_shape::ShapeBuilder;

    fn grammar(xml: &str) -> Grammar {
        crate::load(xml).unwrap_or_else(|e| panic!("fixture grammar failed to load: {e}"))
    }

    fn codes(findings: &[GrammarHealthCheckFinding]) -> Vec<GrammarHealthCode> {
        findings.iter().map(|f| f.code).collect()
    }

    #[test]
    fn every_code_has_a_distinct_stable_linguist_group_name() {
        let mut names = Vec::new();
        for &code in GrammarHealthCode::ALL {
            let name = code.group_name();
            assert!(!name.trim().is_empty(), "{code:?} has no group name");
            assert!(name.chars().count() <= 30, "{code:?} group name is too long: {name}");
            assert!(name.chars().count() <= 45, "{code:?} group name exceeds the hard limit: {name}");
            assert!(!names.contains(&name), "duplicate group name: {name}");
            names.push(name);
        }
    }
    // --- hc-duplicate-feature-bundle ------------------------------------------------------

    const TWO_SEGMENTS_SHARE_BUNDLE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>DuplicateBundle</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_voc"><Name>voc</Name>
        <Symbols><Symbol id="sym_p">+</Symbol><Symbol id="sym_m">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_p" />
        </SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_p" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn two_segments_share_feature_bundle_reports_both_by_name() {
        let g = grammar(TWO_SEGMENTS_SHARE_BUNDLE_XML);
        let findings = check_grammar_health(&g, None);
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.code, GrammarHealthCode::DuplicateFeatureBundle);
        assert_eq!(finding.severity, GrammarHealthSeverity::Warning);
        assert!(finding.message.contains('a'));
        assert!(finding.message.contains('b'));
        assert!(matches!(
            &finding.subjects[0],
            GrammarHealthSubject { kind: GrammarHealthSubjectKind::Table, title, .. } if title == "table1"
        ));
    }

    const DISTINCT_BUNDLES_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>DistinctBundles</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_voc"><Name>voc</Name>
        <Symbols><Symbol id="sym_p">+</Symbol><Symbol id="sym_m">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_p" />
        </SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_m" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn every_segment_has_distinct_feature_bundle_no_findings() {
        let g = grammar(DISTINCT_BUNDLES_XML);
        assert!(check_grammar_health(&g, None).is_empty());
    }

    const ZERO_FEATURE_SYSTEM_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>ZeroFeatureSystem</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_c"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn no_phonological_feature_system_does_not_flag_trivially_identical_bundles() {
        // No `<PhonologicalFeatureSystem>` at all (the real Sena shape) -- every bundle is the same empty struct, so this must not report a duplicate.
        let g = grammar(ZERO_FEATURE_SYSTEM_XML);
        assert!(check_grammar_health(&g, None).is_empty());
    }

    // --- hc-undeclared-segment -------------------------------------------------------------

    const CLEAN_LEXICON_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CleanLexicon</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>Surface</Name>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn clean_grammar_no_findings_at_all() {
        let g = grammar(CLEAN_LEXICON_XML);
        assert!(check_grammar_health(&g, None).is_empty());
    }

    /// A hand-built `Shape` bypassing the table's own validated segmentation -- mirrors C#'s own test note that direct object-model construction need not go through it.
    fn undeclared_shape() -> Shape {
        let mut b = ShapeBuilder::new();
        b.push_segment(9_999);
        b.finish()
    }

    #[test]
    fn lexical_entry_uses_segment_no_table_declares_reports_finding() {
        let mut g = grammar(CLEAN_LEXICON_XML);
        g.entries[0].allomorphs[0].shape.shape = undeclared_shape();

        let findings = check_grammar_health(&g, None);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, GrammarHealthCode::UndeclaredSegment);
        assert_eq!(findings[0].severity, GrammarHealthSeverity::Error);
        assert!(findings[0].message.contains("e1"));
    }

    const AFFIX_INSERT_SEGMENTS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>AffixInsertSegments</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1" morphologicalRules="mr1">
        <Name>Surface</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>plural</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub1">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    /// Finds the sole `InsertSegments` action inside `grammar.mrules[0]`'s single allomorph.
    fn insert_segments_shape_mut(g: &mut Grammar) -> &mut Shape {
        let MorphRuleDef::AffixProcess(def) = &mut g.mrules[0] else {
            panic!("expected an AffixProcess rule");
        };
        for action in &mut def.allomorphs[0].rhs {
            if let OutputAction::InsertSegments { shape, .. } = action {
                return &mut shape.shape;
            }
        }
        panic!("expected an InsertSegments action");
    }

    #[test]
    fn affix_process_rule_insert_segments_undeclared_reports_finding() {
        let mut g = grammar(AFFIX_INSERT_SEGMENTS_XML);
        *insert_segments_shape_mut(&mut g) = undeclared_shape();

        let findings = check_grammar_health(&g, None);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, GrammarHealthCode::UndeclaredSegment);
        assert!(findings[0].message.contains("plural"));
    }

    const COMPOUNDING_INSERT_SEGMENTS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CompoundingInsertSegments</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="char_bnd"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1" morphologicalRules="mrC">
        <Name>Surface</Name>
        <MorphologicalRuleDefinitions>
          <CompoundingRule id="mrC">
            <Name>compound1</Name>
            <CompoundingSubrules><CompoundingSubrule>
              <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
              <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
            </CompoundingSubrule></CompoundingSubrules>
          </CompoundingRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn compounding_rule_insert_segments_undeclared_reports_finding() {
        let mut g = grammar(COMPOUNDING_INSERT_SEGMENTS_XML);
        let MorphRuleDef::Compounding(def) = &mut g.mrules[0] else {
            panic!("expected a Compounding rule");
        };
        let mut replaced = false;
        for action in &mut def.subrules[0].rhs {
            if let OutputAction::InsertSegments { shape, .. } = action {
                shape.shape = undeclared_shape();
                replaced = true;
            }
        }
        assert!(replaced, "fixture must contain an InsertSegments action");

        let findings = check_grammar_health(&g, None);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, GrammarHealthCode::UndeclaredSegment);
        assert!(findings[0].message.contains("compound1"));
    }

    // --- hc-partial-morpheme -----------------------------------------------------------------

    const PARTIAL_LEX_ENTRY_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PartialLexEntry</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>Surface</Name>
        <LexicalEntries>
          <LexicalEntry id="entry1" partial="true">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn partial_lexical_entry_reports_actionable_warning() {
        let g = grammar(PARTIAL_LEX_ENTRY_XML);
        let findings = check_grammar_health(&g, None);
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.code, GrammarHealthCode::PartialMorpheme);
        assert_eq!(finding.severity, GrammarHealthSeverity::Warning);
        assert!(finding.message.contains("Lexical entry 'a'"));
        assert!(finding.message.contains("partially analyzed"));
        assert!(finding.message.contains("final-template pruning"));
        assert!(matches!(
            &finding.subjects[..],
            [GrammarHealthSubject { kind: GrammarHealthSubjectKind::LexEntry, title, .. }] if title == "a"
        ));
    }

    const PARTIAL_TEMPLATE_RULE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PartialTemplateRule</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>Surface</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV" partial="true">
            <Name>subject</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub1">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stem" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate>
            <Name>verb1</Name>
            <Slot morphologicalRules="mr1"><Name>Sl1</Name></Slot>
          </AffixTemplate>
          <AffixTemplate>
            <Name>verb2</Name>
            <Slot morphologicalRules="mr1"><Name>Sl2</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn partial_ordinary_rule_reports_rule() {
        // Distinct from the template-only fixture below: this one lists `mr1` in the stratum's own `morphologicalRules`, exercising the ordinary-rule path.
        const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PartialOrdinaryRule</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1" morphologicalRules="mr1">
        <Name>Surface</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV" partial="true">
            <Name>plural</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub1">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stem" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
        let g = grammar(XML);
        let findings = check_grammar_health(&g, None);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, GrammarHealthCode::PartialMorpheme);
        assert!(findings[0].message.contains("plural"));
        assert!(matches!(
            &findings[0].subjects[..],
            [GrammarHealthSubject { kind: GrammarHealthSubjectKind::MorphRule, title, .. }] if title == "plural"
        ));
    }

    #[test]
    fn partial_template_rule_referenced_twice_reports_once() {
        let g = grammar(PARTIAL_TEMPLATE_RULE_XML);
        let findings = check_grammar_health(&g, None);
        assert_eq!(findings.len(), 1, "referenced by two slots, reported once");
        assert_eq!(findings[0].code, GrammarHealthCode::PartialMorpheme);
        assert!(findings[0].message.contains("subject"));
    }

    const PARTIAL_MORPHEME_AND_EXISTING_PROBLEM_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PartialAndDuplicate</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_voc"><Name>voc</Name>
        <Symbols><Symbol id="sym_p">+</Symbol><Symbol id="sym_m">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_p" />
        </SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations>
          <FeatureValue feature="feat_voc" symbolValues="sym_p" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>Surface</Name>
        <LexicalEntries>
          <LexicalEntry id="entry1" partial="true">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn partial_morpheme_and_existing_problem_reports_both() {
        let g = grammar(PARTIAL_MORPHEME_AND_EXISTING_PROBLEM_XML);
        let mut found = codes(&check_grammar_health(&g, None));
        found.sort_by_key(|c| c.wire());
        let mut expected = vec![
            GrammarHealthCode::DuplicateFeatureBundle,
            GrammarHealthCode::PartialMorpheme,
        ];
        expected.sort_by_key(|c| c.wire());
        assert_eq!(found, expected);
    }

    // --- serialization ------------------------------------------------------------------------

    #[test]
    fn findings_are_serializable() {
        let g = grammar(PARTIAL_LEX_ENTRY_XML);
        let findings = check_grammar_health(&g, None);
        let json = serde_json::to_string(&findings).expect("findings must serialize");
        assert!(json.contains("hc-partial-morpheme"));
        let round_tripped: Vec<GrammarHealthCheckFinding> =
            serde_json::from_str(&json).expect("findings must deserialize");
        assert_eq!(round_tripped, findings);
    }

    #[test]
    fn every_code_serializes_to_its_stable_wire_string() {
        for (code, wire) in [
            (
                GrammarHealthCode::UndeclaredSegment,
                "hc-undeclared-segment",
            ),
            (
                GrammarHealthCode::DuplicateFeatureBundle,
                "hc-duplicate-feature-bundle",
            ),
            (GrammarHealthCode::PartialMorpheme, "hc-partial-morpheme"),
        ] {
            assert_eq!(code.wire(), wire);
            assert_eq!(serde_json::to_string(&code).unwrap(), format!("{wire:?}"));
        }
    }

    #[test]
    fn group_names_describe_their_codes() {
        assert_eq!(
            GrammarHealthCode::UndeclaredSegment.group_name(),
            "Missing segment definition"
        );
        assert_eq!(
            GrammarHealthCode::DuplicateFeatureBundle.group_name(),
            "Duplicate segment features"
        );
        assert_eq!(
            GrammarHealthCode::PartialMorpheme.group_name(),
            "Partial morpheme analysis"
        );
    }

    const FIELDWORKS_GUID_PARTIAL_XML: &str = r#"<HermitCrabInput><Language>
<Name>FieldWorks Demo</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="table1"><Name>Orthography</Name>
<SegmentDefinitions><SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
</CharacterDefinitionTable>
<Strata><Stratum characterDefinitionTable="table1"><Name>main</Name><LexicalEntries>
<LexicalEntry id="f4e4b416-5a15-41e3-9039-c3cca7093153" partial="true">
<MorphemeId>walk</MorphemeId><Gloss>walk</Gloss>
<Allomorphs><Allomorph id="allo1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
</LexicalEntry>
</LexicalEntries></Stratum></Strata>
</Language></HermitCrabInput>"#;

    #[test]
    fn fieldworks_guid_partial_entry_uses_its_readable_name() {
        let g = grammar(FIELDWORKS_GUID_PARTIAL_XML);
        let findings = check_grammar_health(&g, None);
        assert_eq!(findings.len(), 1);
        assert!(matches!(
            &findings[0].subjects[..],
            [GrammarHealthSubject { kind: GrammarHealthSubjectKind::LexEntry, title, .. }] if title == "a - walk"
        ));
        assert!(!findings[0]
            .message
            .contains("f4e4b416-5a15-41e3-9039-c3cca7093153"));
    }

    fn looks_like_internal_subject_label(title: &str) -> bool {
        let lower = title.trim().to_ascii_lowercase();
        ["mrule", "entry", "rule", "slot", "template"].iter().any(|prefix| {
            lower
                .strip_prefix(prefix)
                .is_some_and(|rest| {
                    let rest = rest.trim();
                    !rest.is_empty() && rest.chars().all(|character| character.is_ascii_digit())
                })
        }) || crate::grammar_health_presentation::canonical_guid(title).is_some()
    }

    fn assert_no_blank_or_internal_subjects(findings: &[GrammarHealthCheckFinding]) {
        for finding in findings {
            assert!(finding.is_complete(), "incomplete finding: {finding:?}");
            assert!(!finding.group_name.trim().is_empty());
            assert_eq!(finding.group_name, finding.code.group_name());
            assert!(!finding.message.trim().is_empty());
            assert!(!finding.code.wire().is_empty());
            for subject in &finding.subjects {
                assert!(!subject.kind.label().is_empty());
                assert!(!subject.title.trim().is_empty());
                assert!(!subject.where_text().trim().is_empty());
                assert!(!looks_like_internal_subject_label(&subject.title));
            }
            let json = serde_json::to_value(finding).expect("finding serializes");
            assert!(json["severity"].as_str().is_some_and(|value| !value.is_empty()));
            assert!(json["group_name"].as_str().is_some_and(|value| !value.trim().is_empty()));
            assert!(json["problem"].as_str().is_some_and(|value| !value.is_empty()));
            let subjects = json["subjects"].as_array().expect("subjects serialize as an array");
            assert!(!subjects.is_empty());
            for subject in subjects {
                assert!(subject["kind"].as_str().is_some_and(|value| !value.is_empty()));
                assert!(subject["title"].as_str().is_some_and(|value| !value.trim().is_empty()));
            }
        }
    }

    #[test]
    fn report_rejects_an_incomplete_finding_instead_of_dropping_it() {
        let incomplete = GrammarHealthCheckFinding {
            severity: GrammarHealthSeverity::Warning,
            code: GrammarHealthCode::PartialMorpheme,
            group_name: GrammarHealthCode::PartialMorpheme.group_name().to_string(),
            message: String::new(),
            subjects: Vec::new(),
        };
        let mut findings = check_grammar_health(&grammar(PARTIAL_LEX_ENTRY_XML), None);
        findings.push(incomplete);

        let error = render_json(&findings).expect_err("incomplete findings must fail the report");
        let error = error.to_string();
        assert!(error.contains("hc-partial-morpheme"), "{error}");
        assert!(error.contains("finding 1"), "{error}");
        assert!(error.contains("message"), "{error}");
    }

    #[test]
    fn log_renderer_rejects_the_same_incomplete_finding() {
        let incomplete = GrammarHealthCheckFinding {
            severity: GrammarHealthSeverity::Warning,
            code: GrammarHealthCode::PartialMorpheme,
            group_name: GrammarHealthCode::PartialMorpheme.group_name().to_string(),
            message: String::new(),
            subjects: Vec::new(),
        };
        let error =
            render_log(&[incomplete], false).expect_err("incomplete findings must fail the log");
        assert!(error.to_string().contains("hc-partial-morpheme"));
        assert!(error.to_string().contains("message"));
    }

    #[test]
    fn an_empty_report_remains_valid() {
        assert_eq!(render_log(&[], false).expect("empty log renders"), "");
        let json = render_json(&[]).expect("empty report serializes");
        assert!(json.contains("\"findings\": []"));
    }

    #[test]
    fn direct_report_decode_rejects_an_unsupported_schema_version() {
        let json = serde_json::json!({
            "schema_version": 99,
            "findings": [],
        });
        let error = serde_json::from_value::<GrammarHealthJsonReport>(json)
            .expect_err("unsupported schema versions must be rejected");
        assert!(error.to_string().contains("schema version 99"));
        assert!(error.to_string().contains("expected 1"));
    }

    #[test]
    fn direct_report_decode_rejects_both_fieldworks_link_states() {
        let grammar = grammar(FIELDWORKS_GUID_PARTIAL_XML);
        let finding = check_grammar_health(&grammar, None)
            .into_iter()
            .next()
            .expect("fixture has a finding");
        let report = GrammarHealthJsonReport {
            schema_version: GRAMMAR_HEALTH_SCHEMA_VERSION,
            findings: vec![finding],
        };
        let mut json = serde_json::to_value(&report).expect("report serializes");
        json["findings"][0]["subjects"][0]["fieldworks"]["url"] =
            serde_json::Value::String("silfw://invalid".to_string());
        let error = serde_json::from_value::<GrammarHealthJsonReport>(json)
            .expect_err("both link states must be rejected");
        assert!(error.to_string().contains("fieldworks"));
        assert!(error.to_string().contains("either url or url_unavailable"));
    }

    #[test]
    fn canonical_decoder_rejects_the_previous_bare_array_shape() {
        let error = GrammarHealthJsonReport::from_json("[]")
            .expect_err("bare findings arrays are not the versioned report");
        assert!(error.to_string().contains("bare findings arrays"));
    }

    #[test]
    fn canonical_report_round_trips_without_changing_consumer_values() {
        let findings = check_grammar_health(&grammar(FIELDWORKS_GUID_PARTIAL_XML), None);
        let original = GrammarHealthJsonReport {
            schema_version: GRAMMAR_HEALTH_SCHEMA_VERSION,
            findings,
        };
        let json = original.to_json().expect("canonical report serializes");
        let decoded =
            GrammarHealthJsonReport::from_json(&json).expect("canonical report deserializes");
        assert_eq!(decoded, original);
    }

    #[test]
    fn every_existing_grammar_fixture_has_complete_human_findings() {
        let mut compounding_grammar = grammar(COMPOUNDING_INSERT_SEGMENTS_XML);
        let MorphRuleDef::Compounding(def) = &mut compounding_grammar.mrules[0] else {
            panic!("expected a Compounding rule");
        };
        for action in &mut def.subrules[0].rhs {
            if let OutputAction::InsertSegments { shape, .. } = action {
                shape.shape = undeclared_shape();
            }
        }
        let compounding_findings = check_grammar_health(&compounding_grammar, None);
        let compounding_subject = compounding_findings
            .iter()
            .flat_map(|finding| &finding.subjects)
            .find(|subject| subject.kind == GrammarHealthSubjectKind::MorphRule)
            .expect("compounding finding names its rule");
        assert_eq!(compounding_subject.fieldworks.tool, "compoundRuleAdvancedEdit");
        for xml in [
            TWO_SEGMENTS_SHARE_BUNDLE_XML,
            DISTINCT_BUNDLES_XML,
            ZERO_FEATURE_SYSTEM_XML,
            CLEAN_LEXICON_XML,
            AFFIX_INSERT_SEGMENTS_XML,
            COMPOUNDING_INSERT_SEGMENTS_XML,
            PARTIAL_LEX_ENTRY_XML,
            PARTIAL_TEMPLATE_RULE_XML,
            PARTIAL_MORPHEME_AND_EXISTING_PROBLEM_XML,
            FIELDWORKS_GUID_PARTIAL_XML,
        ] {
            let findings = check_grammar_health(&grammar(xml), None);
            assert_no_blank_or_internal_subjects(&findings);
        }
    }

    #[test]
    fn log_and_json_render_the_same_guid_fixture_with_explicit_link_state() {
        let g = grammar(FIELDWORKS_GUID_PARTIAL_XML);
        let with_project = check_grammar_health(&g, Some("FieldWorks Demo"));
        assert_no_blank_or_internal_subjects(&with_project);
        let subject = &with_project[0].subjects[0];
        assert_eq!(subject.title, "a - walk");
        assert_eq!(subject.fieldworks.guid.as_deref(), Some("f4e4b416-5a15-41e3-9039-c3cca7093153"));
        assert_eq!(subject.fieldworks.tool, "lexiconEdit");
        let url = subject.fieldworks.url.as_deref().expect("project makes a link");
        assert!(url.contains("database%3DFieldWorks+Demo%26tool%3DlexiconEdit%26guid%3Df4e4b416-5a15-41e3-9039-c3cca7093153%26tag%3D"));
        assert!(!url.contains("&tool="));
        assert!(!url.contains("&guid="));

        let log = render_log(&with_project, false).expect("complete findings render");
        assert_eq!(log.lines().count(), with_project.len());
        assert!(log.lines().all(|line| !line.trim().is_empty()));
        assert!(log.contains("a - walk"));
        assert!(log.contains("Partial morpheme analysis"));
        assert!(!log.contains("f4e4b416-5a15-41e3-9039-c3cca7093153"));
        assert!(!log.contains("entry0"));
        let log_with_guids = render_log(&with_project, true).expect("complete findings render");
        assert!(log_with_guids.contains("a - walk [guid f4e4b416-5a15-41e3-9039-c3cca7093153]"));
        assert!(!log_with_guids.contains("entry0"));

        let json = render_json(&with_project).expect("structured findings serialize");
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"group_name\": \"Partial morpheme analysis\""));
        assert!(json.contains("silfw://localhost/link?database%3DFieldWorks+Demo%26tool%3DlexiconEdit"));
        assert!(json.contains("\"internal_id\""));

        let without_project = check_grammar_health(&g, None);
        let no_link = &without_project[0].subjects[0].fieldworks;
        assert!(no_link.url.is_none());
        assert_eq!(
            no_link.url_unavailable.as_deref(),
            Some("no FieldWorks project name supplied")
        );
        let no_project_json = render_json(&without_project).expect("structured findings serialize");
        assert!(no_project_json.contains("no FieldWorks project name supplied"));
        assert!(!no_project_json.contains("silfw://localhost/link?database="));

        let no_guid_findings = check_grammar_health(&grammar(TWO_SEGMENTS_SHARE_BUNDLE_XML), Some("FieldWorks Demo"));
        let no_guid_log = render_log(&no_guid_findings, true).expect("complete findings render");
        assert!(no_guid_log.contains("[guid unavailable]"));
        assert!(!no_guid_log.contains("[guid ]"));
    }
}
