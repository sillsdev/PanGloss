//! Structured compile refusals shared by parsing commands and grammar-health.

use std::fmt;

use pg_grammar::grammar_health::GrammarHealthDiagnostic;
use pg_snapshot::{DiagnosticLevel, FwClass, ImportWarningCode, IssueClass, Snapshot, Warning};
use serde::Serialize;

#[derive(Serialize)]
#[serde(untagged)]
enum CompileIssueKind {
    Conversion(IssueClass),
    Load(&'static str),
}

#[derive(Serialize)]
pub(crate) struct CompileIssue {
    code: String,
    kind: CompileIssueKind,
    object_guid: Option<String>,
    object_kind: Option<FwClass>,
    field: Option<String>,
    text: String,
    advice: String,
    fatal: bool,
}

#[derive(Serialize)]
pub(crate) struct CompileFailure {
    schema_version: u16,
    status: &'static str,
    path: String,
    message: String,
    issues: Vec<CompileIssue>,
    #[serde(skip)]
    pub(crate) diagnostics: Vec<GrammarHealthDiagnostic>,
}

impl CompileFailure {
    pub(crate) fn new(
        path: &str,
        error: pg_grammar::GrammarError,
        snapshot: Option<&Snapshot>,
    ) -> Self {
        let mut issues = Vec::new();
        let mut diagnostics = Vec::new();
        let message = error.to_string();
        if let pg_grammar::GrammarError::Conversion(ref conversion) = error {
            diagnostics.extend(
                conversion
                    .warnings
                    .iter()
                    .map(GrammarHealthDiagnostic::from_import_warning),
            );
            for issue in &conversion.issues {
                let mut warning = conversion
                    .warnings
                    .iter()
                    .find(|warning| {
                        warning.code == issue.code.wire()
                            && issue.source.as_ref().is_none_or(|source| {
                                warning.subjects.iter().any(|subject| {
                                    subject.class == source.kind
                                        && subject.guid.as_deref()
                                            == Some(
                                                pg_snapshot::canonical_guid(&source.id)
                                                    .as_deref()
                                                    .unwrap_or(&source.id),
                                            )
                                })
                            })
                    })
                    .cloned()
                    .unwrap_or_else(|| Warning::from_conversion_issue(issue));
                if warning.subjects.is_empty() {
                    warning.subjects.push(
                        pg_snapshot::FwObjectRef::new(FwClass::Project)
                            .project_settings()
                            .name(path),
                    );
                }
                let diagnostic = GrammarHealthDiagnostic::from_import_warning(&warning);
                if !diagnostics.iter().any(|existing| {
                    existing.code == diagnostic.code && existing.subjects == diagnostic.subjects
                }) {
                    diagnostics.push(diagnostic.clone());
                }
                let subject = warning.subjects.first();
                let text = issue
                    .source
                    .as_ref()
                    .and_then(|source| {
                        snapshot.and_then(|snapshot| {
                            snapshot.phonology.environments.iter().find(|env| {
                                source.kind == FwClass::PhEnvironment && env.guid == source.id
                            })
                        })
                    })
                    .map_or_else(|| issue.message.clone(), |env| env.representation.clone());
                issues.push(CompileIssue {
                    code: issue.code.wire().into(),
                    kind: CompileIssueKind::Conversion(issue.class),
                    object_guid: issue.source.as_ref().map(|source| source.id.clone()),
                    object_kind: issue.source.as_ref().map(|source| source.kind),
                    field: subject.and_then(|subject| subject.field.clone()),
                    text,
                    advice: diagnostic.guidance.clone().unwrap_or_else(|| "Use the diagnostic description to locate the source item; repair malformed data or report a valid construct that cannot be imported.".into()),
                    fatal: issue.fatal,
                });
            }
            for diagnostic in &mut diagnostics {
                if conversion
                    .issues
                    .iter()
                    .any(|issue| issue.fatal && issue.code.wire() == diagnostic.code.wire())
                {
                    diagnostic.level = DiagnosticLevel::Error;
                }
            }
        } else {
            let kind = match error {
                pg_grammar::GrammarError::Xml(_) => "xml",
                pg_grammar::GrammarError::Unsupported(_) => "unsupported",
                pg_grammar::GrammarError::Semantic(_) => "semantic",
                pg_grammar::GrammarError::DuplicateRepresentation(_) => "duplicate_representation",
                pg_grammar::GrammarError::UnsegmentableBoundary(_) => "unsegmentable_boundary",
                pg_grammar::GrammarError::Conversion(_) => unreachable!(),
            };
            let warning = Warning::new(ImportWarningCode::CompileFailed, &message).with_subject(
                pg_snapshot::FwObjectRef::new(FwClass::Project)
                    .project_settings()
                    .name(path),
            );
            let mut diagnostic = GrammarHealthDiagnostic::from_import_warning(&warning);
            diagnostic.level = DiagnosticLevel::Error;
            let advice = diagnostic.guidance.clone().unwrap_or_else(|| "Correct the grammar input using the error description; report valid grammar input that cannot be loaded.".into());
            issues.push(CompileIssue {
                code: warning.code,
                kind: CompileIssueKind::Load(kind),
                object_guid: None,
                object_kind: None,
                field: None,
                text: message.clone(),
                advice,
                fatal: true,
            });
            diagnostics.push(diagnostic);
        }
        Self {
            schema_version: 1,
            status: "compile_error",
            path: path.into(),
            message,
            issues,
            diagnostics,
        }
    }
}

pub(crate) enum GrammarLoadError {
    Input(String),
    Compile(Box<CompileFailure>),
}

impl From<String> for GrammarLoadError {
    fn from(message: String) -> Self {
        Self::Input(message)
    }
}

impl fmt::Display for GrammarLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(message) => f.write_str(message),
            Self::Compile(report) => {
                f.write_str(&serde_json::to_string(report).map_err(|_| fmt::Error)?)
            }
        }
    }
}

pub(crate) fn command_error(name: &str, message: &str) -> String {
    if serde_json::from_str::<serde_json::Value>(message).is_ok_and(|value| {
        value["status"] == "compile_error"
            && value["schema_version"] == 1
            && value["issues"].is_array()
    }) {
        message.into()
    } else {
        format!("pangloss {name}: {message}")
    }
}

#[cfg(test)]
mod tests;
