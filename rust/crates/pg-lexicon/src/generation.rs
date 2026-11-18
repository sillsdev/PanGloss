//! Revision-bound generation from complete structured analyses, sharing runtime root authority.

use crate::{LexiconSnapshot, Revision, StructuredError, SuppliedLexiconRuntime};
use pg_grammar::model::{MprIds, MprSet};
use pg_parse::{AnalysisProvenance, WordAnalysis};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenerationRequest<Mpr = MprSet> {
    pub revision: Revision,
    pub analysis: WordAnalysis<Mpr>,
}

/// Host decoding must retain sparse IDs until revision and grammar inventory validation.
pub type HostGenerationRequest = GenerationRequest<MprIds>;

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
    /// Decodes host MPR IDs against the loaded grammar before any dense bitmap expansion.
    /// Revision validation and synthesis use the same immutable runtime snapshot.
    pub fn generate_host_analysis(
        &self,
        request: HostGenerationRequest,
    ) -> Result<GenerationResult, StructuredError> {
        let snapshot = self.generation_snapshot(&request.revision)?;
        let analysis = request.analysis.try_map_mpr(|ids| {
            ids.into_set(self.grammar.mpr_features.len()).map_err(|id| {
                error(
                    "invalid_mpr_id",
                    &format!("MPR ID {} is outside the grammar inventory", id.0),
                )
            })
        })?;
        self.generate_analysis_in_snapshot(analysis, &snapshot)
    }

    /// Regenerates using one immutable snapshot for revision, active identities and synthesis.
    /// Supplied payloads must exactly match the canonical roots already published by the import
    /// owner; numeric sentinel slots cannot recreate missing or forged runtime identities.
    /// The existing generation overload's synthesis semantics remain unchanged. Completion is
    /// explicitly not assessed, and guessed roots are unsupported by this overload.
    pub fn generate_analysis(
        &self,
        request: GenerationRequest,
    ) -> Result<GenerationResult, StructuredError> {
        let snapshot = self.generation_snapshot(&request.revision)?;
        self.generate_analysis_in_snapshot(request.analysis, &snapshot)
    }

    fn generation_snapshot(
        &self,
        revision: &Revision,
    ) -> Result<std::sync::Arc<LexiconSnapshot>, StructuredError> {
        let snapshot = self.snapshot();
        if revision != snapshot.revision() {
            return Err(error(
                "revision_conflict",
                "generation revision does not match the runtime snapshot",
            ));
        }
        Ok(snapshot)
    }

    fn generate_analysis_in_snapshot(
        &self,
        mut analysis: WordAnalysis,
        snapshot: &LexiconSnapshot,
    ) -> Result<GenerationResult, StructuredError> {
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
            .morpher(snapshot)
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

#[cfg(test)]
mod tests {
    #[test]
    fn sparse_host_ids_preserve_wire_validation_and_inventory_boundaries() {
        use pg_grammar::model::{MprId, MprIds, MprSet};
        let decode = |value| serde_json::from_value::<MprIds>(value);
        let high = decode(serde_json::json!({"version":1,"ids":[302,64,302]})).unwrap();
        assert_eq!(high.clone().into_set(302), Err(MprId(302)));
        assert_eq!(
            high.into_set(303)
                .unwrap()
                .iter()
                .map(|id| id.0)
                .collect::<Vec<_>>(),
            vec![64, 302]
        );
        assert_eq!(
            decode(serde_json::json!(0)).unwrap().into_set(0).unwrap(),
            MprSet::EMPTY
        );
        assert_eq!(
            decode(serde_json::json!(1u64 << 63)).unwrap().into_set(63),
            Err(MprId(63))
        );
        assert_eq!(
            decode(serde_json::json!(1u64 << 63))
                .unwrap()
                .into_set(64)
                .unwrap(),
            MprSet::from_bits(1u64 << 63)
        );
        for invalid in [
            serde_json::json!({"version":2,"ids":[0]}),
            serde_json::json!({"version":1,"ids":[4294967296u64]}),
            serde_json::json!({"version":1,"ids":[-1]}),
            serde_json::json!({"version":1,"ids":[0],"extra":0}),
        ] {
            assert!(decode(invalid).is_err());
        }
    }
}
