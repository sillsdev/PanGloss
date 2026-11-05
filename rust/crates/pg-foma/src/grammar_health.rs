//! Projects compiler-owned refusals and emission gaps into grammar-health's author vocabulary.

use pg_grammar::grammar_health::{GrammarHealthCode, GrammarHealthDiagnostic};
use pg_grammar::model::{Grammar, LexEntryId, MRuleId};
use pg_grammar::GrammarError;
use pg_snapshot::{FwClass, FwObjectRef};

use crate::capability::{CapabilityRefusal, CompileDecision};
use crate::emit::{EmissionHealth, FomaTier};
use crate::enumerate::EmissionStrategy;

/// Adds FST findings without repeating an existing undeclared-segment or identical finding.
/// Returns the reason if the finite inspection could not complete; existing findings survive.
pub fn extend_diagnostics(
    grammar: &Grammar,
    diagnostics: &mut Vec<GrammarHealthDiagnostic>,
) -> Result<Option<String>, GrammarError> {
    let selection = crate::backend_selection::select_backends_for_grammar(grammar);
    if let CompileDecision::Refuse(refusals) =
        selection.decision_for(EmissionStrategy::TunedSurfaceProbed)
    {
        let mut errors = Vec::new();
        for refusal in refusals {
            let message = format!("{}: {}", refusal.construct, refusal.witness);
            let diagnostic = match refusal.refusal {
                CapabilityRefusal::CircumfixNoRoute { rule: Some(rule) } => {
                    GrammarHealthDiagnostic::for_morph_rule(
                        grammar,
                        MRuleId(rule),
                        GrammarHealthCode::FstCircumfixNoRoute,
                        message,
                    )?
                }
                CapabilityRefusal::CircumfixNoRoute { rule: None } => {
                    GrammarHealthDiagnostic::from_check(
                        GrammarHealthCode::FstCircumfixNoRoute,
                        message,
                        vec![FwObjectRef::new(FwClass::Project)
                            .name("FST allomorph routing")
                            .project_settings()],
                    )
                }
                CapabilityRefusal::ReduplicationNoRoute { rule } => {
                    GrammarHealthDiagnostic::for_morph_rule(
                        grammar,
                        MRuleId(rule),
                        GrammarHealthCode::FstReduplicationNoRoute,
                        message,
                    )?
                }
                CapabilityRefusal::PatternRootShape { entry } => {
                    GrammarHealthDiagnostic::for_lexical_entry(
                        grammar,
                        LexEntryId(entry),
                        GrammarHealthCode::FstPatternRootShape,
                        message,
                    )?
                }
                CapabilityRefusal::AlphaEnvironment => {
                    errors.push(format!(
                        "FST inspection deferred for alpha environments pending #177: {message}"
                    ));
                    continue;
                }
                CapabilityRefusal::RootSpellingBudget { .. } => {
                    errors.push(message);
                    continue;
                }
                CapabilityRefusal::StrategyCoverage => {
                    return Err(GrammarError::Semantic(format!(
                        "FST refusal has no grammar-health code: {} ({message})",
                        refusal.predicate
                    )));
                }
            };
            push_finding(diagnostics, diagnostic);
        }
        return Ok((!errors.is_empty()).then(|| errors.join("; ")));
    }

    let emitted = crate::emit::emit_tuned_surface(grammar);
    let mut errors = Vec::new();
    let mut grammar_gaps = 0;
    for item in emitted.report.uncovered {
        match item.grammar_health {
            EmissionHealth::Grammar(diagnostic) => {
                grammar_gaps += 1;
                push_finding(diagnostics, *diagnostic);
            }
            EmissionHealth::Budget { .. } => errors.push(item.reason),
            EmissionHealth::Advisory => {}
        }
    }
    match emitted.report.tier {
        FomaTier::Unsupported { reason } => errors.push(reason),
        FomaTier::CannotRepresent { reason } if grammar_gaps == 0 => errors.push(reason),
        FomaTier::CannotRepresent { .. } | FomaTier::Full => {}
    }
    Ok((!errors.is_empty()).then(|| errors.join("; ")))
}

/// Returns only additional FST findings, failing explicitly if inspection was incomplete.
pub fn check_fst_diagnostics(
    grammar: &Grammar,
) -> Result<Vec<GrammarHealthDiagnostic>, GrammarError> {
    let mut diagnostics = pg_grammar::grammar_health::check_grammar_health_diagnostics(grammar)?;
    let existing_count = diagnostics.len();
    if let Some(reason) = extend_diagnostics(grammar, &mut diagnostics)? {
        return Err(GrammarError::Semantic(format!(
            "FST grammar-health inspection incomplete: {reason}"
        )));
    }
    Ok(diagnostics.split_off(existing_count))
}

fn push_finding(
    diagnostics: &mut Vec<GrammarHealthDiagnostic>,
    diagnostic: GrammarHealthDiagnostic,
) {
    let same_owner = |existing: &GrammarHealthDiagnostic| {
        existing.subjects.iter().any(|a| {
            diagnostic
                .subjects
                .iter()
                .any(|b| a.internal_id.is_some() && a.internal_id == b.internal_id)
        })
    };
    if diagnostics.iter().any(|existing| {
        existing.code == diagnostic.code && existing.subjects == diagnostic.subjects
            || diagnostic.code == GrammarHealthCode::FstShapeNotInSurfaceTable
                && existing.code == GrammarHealthCode::UndeclaredSegment
                && same_owner(existing)
    }) {
        return;
    }
    diagnostics.push(diagnostic);
}
