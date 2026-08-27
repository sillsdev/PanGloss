//! Stateless grammatical classification and the optional in-process answer guide.

use crate::{
    AddRequest, CanonicalFeature, CanonicalFeatureValue, ClassCatalog, SignatureId,
    StructuredError, SuppliedEntry, SuppliedLexiconRuntime,
};
use pg_grammar::model::{Grammar, MRuleId, MorphRuleDef};
use pg_parse::{Morpher, SynthesisBudget};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownFacts {
    pub pos_id: Option<String>,
    pub features: Vec<KnownFeature>,
    pub mpr_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownFeature {
    pub feature_id: String,
    pub value_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationRequest {
    pub stem: String,
    #[serde(default)]
    pub known: KnownFacts,
    #[serde(default)]
    pub budgets: ClassificationBudgets,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationBudgets {
    pub max_forms: usize,
    pub max_derivations: usize,
    pub max_candidates: usize,
    pub max_steps: usize,
    pub max_time_ms: u64,
}

impl Default for ClassificationBudgets {
    fn default() -> Self {
        Self {
            max_forms: 128,
            max_derivations: 16_384,
            max_candidates: 65_536,
            max_steps: 65_536,
            max_time_ms: 2_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TruncationReason {
    FormLimit,
    DerivationLimit,
    CandidateLimit,
    StepLimit,
    TimeLimit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleMetadata {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormPrediction {
    pub signature_id: SignatureId,
    pub derivations: Vec<Vec<RuleMetadata>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticForm {
    pub id: String,
    pub surface: String,
    pub predictions: Vec<FormPrediction>,
}

impl DiagnosticForm {
    fn predicts(&self, signature: &SignatureId) -> bool {
        self.predictions
            .iter()
            .any(|prediction| &prediction.signature_id == signature)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationMatrix {
    pub stem: String,
    pub candidates: Vec<crate::ClassSignature>,
    pub forms: Vec<DiagnosticForm>,
    pub exhaustive: bool,
    pub truncation_reason: Option<TruncationReason>,
}

#[derive(Debug, Clone)]
struct Work {
    signature: SignatureId,
    rules: Vec<MRuleId>,
}

pub fn classify(
    grammar: &Grammar,
    catalog: &ClassCatalog,
    request: ClassificationRequest,
) -> Result<ClassificationMatrix, StructuredError> {
    crate::validate_shape(grammar, &request.stem).map_err(|message| StructuredError {
        code: "invalid_shape".into(),
        message,
        details: serde_json::Value::Null,
    })?;
    let mut candidates: Vec<_> = catalog
        .signatures()
        .iter()
        .filter(|signature| matches_known(signature, &request.known))
        .cloned()
        .collect();
    candidates.sort_by(|a, b| a.id.cmp(&b.id));
    let budgets = request.budgets;
    if budgets.max_forms > 128 {
        return Err(budget_error(
            "maxForms cannot exceed the safety ceiling of 128",
        ));
    }
    if [
        budgets.max_forms,
        budgets.max_derivations,
        budgets.max_candidates,
        budgets.max_steps,
    ]
    .contains(&0)
    {
        return Err(budget_error("classification count budgets must be nonzero"));
    }

    let morpher = Morpher::new(grammar, usize::MAX);
    let mut queue = VecDeque::new();
    for signature in &candidates {
        queue.push_back(Work {
            signature: signature.id.clone(),
            rules: Vec::new(),
        });
    }
    let candidate_ids: Vec<_> = candidates.iter().map(|s| s.id.clone()).collect();
    let mut surfaces: BTreeMap<String, BTreeMap<SignatureId, BTreeSet<Vec<u32>>>> = BTreeMap::new();
    let engine_budget = SynthesisBudget::new(
        budgets.max_steps,
        budgets.max_candidates,
        Duration::from_millis(budgets.max_time_ms),
    );
    let mut derivations = 0usize;
    let mut truncation = None;
    let mut completed_depth = 0usize;

    while let Some(work) = queue.pop_front() {
        if work.rules.len() > completed_depth {
            if useful_surface_count(&surfaces, candidates.len()) > budgets.max_forms {
                truncation = Some(TruncationReason::FormLimit);
                break;
            }
            if all_pairs_separated(&candidate_ids, &surfaces) {
                break;
            }
            completed_depth = work.rules.len();
        }
        if derivations >= budgets.max_derivations {
            truncation = Some(TruncationReason::DerivationLimit);
            break;
        }
        let resolved = catalog
            .resolved(&work.signature)
            .expect("catalog is coherent");
        derivations += 1;
        let produced = morpher.synthesize_resolved_stem_bounded(
            &request.stem,
            resolved.syn_fs,
            resolved.mpr,
            resolved.stratum,
            &work.rules,
            &engine_budget,
        );
        if let Some(reason) = engine_truncation(&engine_budget) {
            truncation = Some(reason);
            break;
        }
        let path: Vec<u32> = work.rules.iter().map(|id| id.0).collect();
        for surface in produced {
            surfaces
                .entry(surface)
                .or_default()
                .entry(work.signature.clone())
                .or_default()
                .insert(path.clone());
        }
        let stratum = &grammar.strata[resolved.stratum.0 as usize];
        for &rule in &stratum.mrules {
            let count = work.rules.iter().filter(|&&used| used == rule).count();
            let max = grammar.mrules[rule.0 as usize].max_apps() as usize;
            if count < max {
                let mut rules = work.rules.clone();
                rules.push(rule);
                queue.push_back(Work {
                    signature: work.signature.clone(),
                    rules,
                });
            }
        }
    }

    let separated = all_pairs_separated(&candidate_ids, &surfaces);
    if truncation.is_none() && useful_surface_count(&surfaces, candidates.len()) > budgets.max_forms
    {
        truncation = Some(TruncationReason::FormLimit);
    }
    let naturally_done = queue.is_empty();
    let exhaustive = truncation.is_none() && (separated || naturally_done);
    let mut useful: Vec<_> = surfaces
        .into_iter()
        .filter(|(_, predictions)| !predictions.is_empty() && predictions.len() < candidates.len())
        .map(|(surface, predictions)| build_form(grammar, surface, predictions))
        .collect();
    useful.sort_by(|a, b| {
        information_score(b, candidates.len())
            .cmp(&information_score(a, candidates.len()))
            .then_with(|| a.surface.cmp(&b.surface))
    });
    if useful.len() > budgets.max_forms {
        useful.truncate(budgets.max_forms);
        truncation = Some(TruncationReason::FormLimit);
    }
    Ok(ClassificationMatrix {
        stem: request.stem,
        candidates,
        forms: useful,
        exhaustive: exhaustive && truncation.is_none(),
        truncation_reason: truncation,
    })
}

fn engine_truncation(budget: &SynthesisBudget) -> Option<TruncationReason> {
    if budget.timed_out() {
        Some(TruncationReason::TimeLimit)
    } else if budget.step_capped() {
        Some(TruncationReason::StepLimit)
    } else if budget.candidate_capped() {
        Some(TruncationReason::CandidateLimit)
    } else {
        None
    }
}

fn useful_surface_count(
    surfaces: &BTreeMap<String, BTreeMap<SignatureId, BTreeSet<Vec<u32>>>>,
    total: usize,
) -> usize {
    surfaces
        .values()
        .filter(|predictions| !predictions.is_empty() && predictions.len() < total)
        .count()
}

fn budget_error(message: &str) -> StructuredError {
    StructuredError {
        code: "invalid_classification_budget".into(),
        message: message.into(),
        details: serde_json::Value::Null,
    }
}

fn matches_known(signature: &crate::ClassSignature, known: &KnownFacts) -> bool {
    if known
        .pos_id
        .as_ref()
        .is_some_and(|id| signature.pos.as_ref().map(|p| &p.id) != Some(id))
    {
        return false;
    }
    let mpr: BTreeSet<_> = signature.mpr.iter().map(|item| item.id.as_str()).collect();
    if known.mpr_ids.iter().any(|id| !mpr.contains(id.as_str())) {
        return false;
    }
    known.features.iter().all(|fact| {
        find_feature(&signature.features, &fact.feature_id).is_some_and(|feature| {
            let values = symbolic_ids(&feature.value);
            fact.value_ids.iter().all(|id| values.contains(id.as_str()))
        })
    })
}

fn find_feature<'a>(features: &'a [CanonicalFeature], id: &str) -> Option<&'a CanonicalFeature> {
    features.iter().find_map(|feature| {
        if feature.feature.id == id {
            Some(feature)
        } else if let CanonicalFeatureValue::Complex(nested) = &feature.value {
            find_feature(nested, id)
        } else {
            None
        }
    })
}

fn symbolic_ids(value: &CanonicalFeatureValue) -> BTreeSet<&str> {
    match value {
        CanonicalFeatureValue::Symbolic(values) => values.iter().map(|v| v.id.as_str()).collect(),
        CanonicalFeatureValue::Complex(_) => BTreeSet::new(),
    }
}

fn all_pairs_separated(
    ids: &[SignatureId],
    surfaces: &BTreeMap<String, BTreeMap<SignatureId, BTreeSet<Vec<u32>>>>,
) -> bool {
    (0..ids.len()).all(|left| {
        (left + 1..ids.len()).all(|right| {
            surfaces.values().any(|predictions| {
                predictions.contains_key(&ids[left]) != predictions.contains_key(&ids[right])
            })
        })
    })
}

fn build_form(
    grammar: &Grammar,
    surface: String,
    predictions: BTreeMap<SignatureId, BTreeSet<Vec<u32>>>,
) -> DiagnosticForm {
    let predictions: Vec<_> = predictions
        .into_iter()
        .map(|(signature_id, paths)| FormPrediction {
            signature_id,
            derivations: paths
                .into_iter()
                .map(|path| {
                    path.into_iter()
                        .rev()
                        .map(|id| rule_metadata(grammar, MRuleId(id)))
                        .collect()
                })
                .collect(),
        })
        .collect();
    let mut hash = Sha256::new();
    hash.update(surface.as_bytes());
    for prediction in &predictions {
        hash.update(prediction.signature_id.as_str().as_bytes());
    }
    let digest = hash.finalize();
    let id = digest
        .iter()
        .fold(String::from("form_"), |mut value, byte| {
            use std::fmt::Write;
            write!(&mut value, "{byte:02x}").expect("writing to String cannot fail");
            value
        });
    DiagnosticForm {
        id,
        surface,
        predictions,
    }
}

fn rule_metadata(grammar: &Grammar, id: MRuleId) -> RuleMetadata {
    let (identity, label) = match &grammar.mrules[id.0 as usize] {
        MorphRuleDef::AffixProcess(rule) => {
            let m = &grammar.morphemes[rule.morpheme.0 as usize];
            (
                m.xml_key.clone(),
                rule.name.clone().or_else(|| m.morph_id.clone()),
            )
        }
        MorphRuleDef::Realizational(rule) => {
            let m = &grammar.morphemes[rule.morpheme.0 as usize];
            (
                m.xml_key.clone(),
                rule.name.clone().or_else(|| m.morph_id.clone()),
            )
        }
        MorphRuleDef::Compounding(rule) => (rule.xml_id.clone(), rule.name.clone()),
    };
    RuleMetadata {
        label: label.unwrap_or_else(|| identity.clone()),
        id: identity,
    }
}

fn information_score(form: &DiagnosticForm, total: usize) -> usize {
    let yes = form.predictions.len();
    yes.min(total.saturating_sub(yes))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Judgment {
    Yes,
    No,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideSelection {
    pub signatures: Vec<SignatureId>,
    pub exhaustive: bool,
}

#[derive(Debug, Clone)]
pub struct ClassificationGuide {
    matrix: ClassificationMatrix,
    answers: BTreeMap<String, Judgment>,
    history: Vec<(String, Option<Judgment>)>,
}

impl ClassificationGuide {
    pub fn new(matrix: ClassificationMatrix) -> Self {
        Self {
            matrix,
            answers: BTreeMap::new(),
            history: Vec::new(),
        }
    }
    pub fn answer(&mut self, form_id: &str, judgment: Judgment) -> Result<(), StructuredError> {
        if !self.matrix.forms.iter().any(|form| form.id == form_id) {
            return Err(StructuredError {
                code: "unknown_form".into(),
                message: "unknown diagnostic form".into(),
                details: serde_json::json!({"formId":form_id}),
            });
        }
        let previous = self.answers.insert(form_id.to_string(), judgment);
        self.history.push((form_id.to_string(), previous));
        Ok(())
    }
    pub fn undo(&mut self) -> bool {
        let Some((id, previous)) = self.history.pop() else {
            return false;
        };
        if let Some(value) = previous {
            self.answers.insert(id, value);
        } else {
            self.answers.remove(&id);
        }
        true
    }
    pub fn remaining_signatures(&self) -> Vec<SignatureId> {
        self.matrix
            .candidates
            .iter()
            .filter(|candidate| {
                self.answers.iter().all(|(id, judgment)| {
                    let form = self
                        .matrix
                        .forms
                        .iter()
                        .find(|form| &form.id == id)
                        .expect("answers validated");
                    match judgment {
                        Judgment::Yes => form.predicts(&candidate.id),
                        Judgment::No => !form.predicts(&candidate.id),
                        Judgment::Unknown => true,
                    }
                })
            })
            .map(|candidate| candidate.id.clone())
            .collect()
    }
    pub fn all_useful_forms(&self) -> Vec<DiagnosticForm> {
        let remaining = self.remaining_signatures();
        self.matrix
            .forms
            .iter()
            .filter(|form| {
                if self.answers.contains_key(&form.id) {
                    return false;
                }
                let yes = remaining.iter().filter(|id| form.predicts(id)).count();
                yes > 0 && yes < remaining.len()
            })
            .cloned()
            .collect()
    }
    pub fn next_form(&self) -> Option<DiagnosticForm> {
        let remaining = self.remaining_signatures();
        self.all_useful_forms().into_iter().max_by(|a, b| {
            information_score(a, remaining.len())
                .cmp(&information_score(b, remaining.len()))
                .then_with(|| b.surface.cmp(&a.surface))
        })
    }
    pub fn final_selection(&self) -> GuideSelection {
        GuideSelection {
            signatures: self.remaining_signatures(),
            exhaustive: self.matrix.exhaustive,
        }
    }
    pub fn matrix(&self) -> &ClassificationMatrix {
        &self.matrix
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AuthoredRef;

    fn sid(n: char) -> SignatureId {
        SignatureId::parse(&format!("sig_{}", n.to_string().repeat(64))).unwrap()
    }
    fn signature(id: SignatureId) -> crate::ClassSignature {
        crate::ClassSignature {
            id,
            pos: Some(AuthoredRef {
                id: "p".into(),
                label: "P".into(),
            }),
            features: vec![],
            mpr: vec![],
            canonical_encoding: "{}".into(),
            entry_count: 1,
        }
    }
    fn form(id: &str, surface: &str, predictions: Vec<SignatureId>) -> DiagnosticForm {
        DiagnosticForm {
            id: id.into(),
            surface: surface.into(),
            predictions: predictions
                .into_iter()
                .map(|signature_id| FormPrediction {
                    signature_id,
                    derivations: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn guide_applies_strict_logic_and_supports_replace_and_undo() {
        let (a, b, c) = (sid('a'), sid('b'), sid('c'));
        let matrix = ClassificationMatrix {
            stem: "x".into(),
            candidates: vec![
                signature(a.clone()),
                signature(b.clone()),
                signature(c.clone()),
            ],
            forms: vec![
                form("one", "xa", vec![a.clone(), b.clone()]),
                form("two", "xb", vec![b.clone()]),
            ],
            exhaustive: true,
            truncation_reason: None,
        };
        let mut guide = ClassificationGuide::new(matrix);
        guide.answer("one", Judgment::No).unwrap();
        assert_eq!(guide.remaining_signatures(), vec![c.clone()]);
        guide.answer("one", Judgment::Yes).unwrap();
        assert_eq!(guide.remaining_signatures(), vec![a.clone(), b.clone()]);
        assert!(guide.undo());
        assert_eq!(guide.remaining_signatures(), vec![c]);
        guide.answer("one", Judgment::Unknown).unwrap();
        assert_eq!(guide.remaining_signatures().len(), 3);
        assert!(guide.undo());
    }

    #[test]
    fn adaptive_form_splits_the_remaining_set_and_unknown_forms_are_rejected() {
        let (a, b, c) = (sid('a'), sid('b'), sid('c'));
        let matrix = ClassificationMatrix {
            stem: "x".into(),
            candidates: vec![
                signature(a.clone()),
                signature(b.clone()),
                signature(c.clone()),
            ],
            forms: vec![
                form("weak", "z", vec![a.clone(), b.clone()]),
                form("strong", "a", vec![b]),
            ],
            exhaustive: false,
            truncation_reason: Some(TruncationReason::DerivationLimit),
        };
        let mut guide = ClassificationGuide::new(matrix);
        assert_eq!(guide.next_form().unwrap().surface, "a");
        let err = guide.answer("missing", Judgment::Yes).unwrap_err();
        assert_eq!(err.code, "unknown_form");
        assert!(!guide.final_selection().exhaustive);
    }
}
