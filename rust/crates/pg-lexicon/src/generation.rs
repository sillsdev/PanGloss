//! Revision-bound generation from complete structured analyses, sharing runtime root authority.

use crate::{Revision, StructuredError, SuppliedLexiconRuntime};
use pg_parse::{AnalysisProvenance, WordAnalysis};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenerationRequest {
    pub revision: Revision,
    pub analysis: WordAnalysis,
}

/// The synthesis owner currently returns surfaces without publishing its independent cap stops.
/// This explicitly prevents callers from treating an empty or partial result as complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GenerationCompletion {
    NotAssessed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationResult {
    pub revision: Revision,
    pub analysis: WordAnalysis,
    pub words: Vec<String>,
    pub completion: GenerationCompletion,
}

impl SuppliedLexiconRuntime {
    /// Regenerates using one immutable snapshot for revision, active identities and synthesis.
    /// Supplied payloads must exactly match the canonical roots already published by the import
    /// owner; numeric sentinel slots cannot recreate missing or forged runtime identities.
    /// The existing generation overload's synthesis semantics remain unchanged. Completion is
    /// explicitly not assessed, and guessed roots are unsupported by this overload.
    pub fn generate_analysis(
        &self,
        request: GenerationRequest,
    ) -> Result<GenerationResult, StructuredError> {
        let snapshot = self.snapshot();
        if &request.revision != snapshot.revision() {
            return Err(error(
                "revision_conflict",
                "generation revision does not match the runtime snapshot",
            ));
        }
        let mut analysis = request.analysis;
        if analysis.guessed
            || analysis.guessed_string.is_some()
            || matches!(analysis.provenance, AnalysisProvenance::Guessed)
        {
            return Err(error(
                "unsupported_guessed_generation",
                "this generation overload cannot regenerate guessed roots",
            ));
        }
        let root_index = usize::try_from(analysis.root_morpheme_index)
            .ok()
            .filter(|&index| index < analysis.morpheme_ids.len())
            .ok_or_else(|| {
                error(
                    "invalid_analysis",
                    "root index is outside the morpheme sequence",
                )
            })?;
        if analysis.morpheme_roots.len() != analysis.morpheme_ids.len() {
            return Err(error(
                "invalid_analysis",
                "morphemeRoots must align with morphemeIds",
            ));
        }
        for (&id, root) in analysis
            .morpheme_ids
            .iter()
            .zip(&mut analysis.morpheme_roots)
        {
            match root {
                Some(payload) => {
                    if id != u32::MAX {
                        return Err(error(
                            "invalid_analysis",
                            "supplied payload requires a runtime sentinel morpheme",
                        ));
                    }
                    let canonical = snapshot.canonical_root(payload).ok_or_else(|| {
                        error(
                            "invalid_supplied_root",
                            "supplied root is not an active canonical realization in this snapshot",
                        )
                    })?;
                    *payload = canonical.clone();
                }
                None if id == u32::MAX => {
                    return Err(error(
                        "invalid_analysis",
                        "runtime sentinel morpheme has no supplied root payload",
                    ));
                }
                None => {}
            }
        }
        let head = &analysis.morpheme_roots[root_index];
        if &analysis.supplied_root != head {
            return Err(error(
                "invalid_analysis",
                "suppliedRoot disagrees with the head morpheme payload",
            ));
        }
        let provenance = head.as_ref().map_or(
            AnalysisProvenance::Grammar,
            pg_parse::SuppliedRoot::provenance,
        );
        if analysis.provenance != provenance {
            return Err(error(
                "invalid_analysis",
                "head provenance disagrees with the canonical root authority",
            ));
        }
        analysis.supplied_root = head.clone();
        let words = self
            .morpher(&snapshot)
            .generate_words_from_analysis(&analysis);
        Ok(GenerationResult {
            revision: snapshot.revision().clone(),
            analysis,
            words,
            completion: GenerationCompletion::NotAssessed,
        })
    }
}

fn error(code: &str, message: &str) -> StructuredError {
    StructuredError {
        code: code.into(),
        message: message.into(),
        details: serde_json::Value::Null,
    }
}
