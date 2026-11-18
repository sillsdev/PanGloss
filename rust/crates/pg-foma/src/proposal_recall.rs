//! Grammar-only representability limits shared by emission and the capability envelope.

use pg_foma_runtime::emission_support::{reduplication_rule_is_peelable, rhs_drops_lhs_material};
use pg_grammar::model::{Grammar, NaturalClassKind, OutputAction, PatternNode, PhonRuleDef};
use pg_health::capability::{
    CapabilityDiagnostic, CapabilityRefusal, ProposalRecallGap, RefusalOccurrenceCount,
};
use pg_health::strategy::EmissionStrategy;

pub(crate) const PREDICATE: &str = "surface-probe.proposal-recall";

#[derive(Debug, Clone, Default)]
pub(crate) struct RefusalReport {
    pub diagnostics: Vec<CapabilityDiagnostic>,
    pub occurrences: Vec<RefusalOccurrenceCount>,
}

fn has_class(g: &Grammar, nodes: &[PatternNode], segment_list: bool) -> bool {
    nodes.iter().any(|node| match node {
        PatternNode::Context(context) => {
            match &g.natural_classes[context.nat_class.0 as usize].kind {
                NaturalClassKind::Segments(members) => segment_list && members.len() > 1,
                NaturalClassKind::Feature(_) => !segment_list,
            }
        }
        PatternNode::Quantifier { children, .. } => has_class(g, children, segment_list),
        _ => false,
    })
}

/// These checks never parse words or infer capability from emitted diagnostic text.
pub(crate) fn refusals(g: &Grammar) -> RefusalReport {
    let spelling_covered = crate::emit::eager_route_has_single_segment_spellings(g)
        || crate::spelling_route::SpellingRoutePlan::prepare(g).is_some();
    refusals_with_spelling(g, spelling_covered)
}

pub(crate) fn refusals_with_spelling(g: &Grammar, spelling_covered: bool) -> RefusalReport {
    let mut gaps = Vec::new();
    if !spelling_covered {
        gaps.push((ProposalRecallGap::AmbiguousSegmentSpelling,
            "referenced character table lacks a singleton spelling closure",
            "literal root and affix emission do not preserve every spelling accepted by the character table's feature-unification closure"));
    }
    let template_count: usize = g.strata.iter().map(|s| s.templates.len()).sum();
    if template_count > 1
        && g.strata
            .iter()
            .flat_map(|s| &s.templates)
            .any(|id| !g.templates[id.0 as usize].is_final)
    {
        gaps.push((ProposalRecallGap::TemplateReentry,
            "non-final template followed by another template",
            "the surface skeleton represents one template application, so template reentry or cross-stratum template completion can lose analyses"));
    }
    for (stratum_index, stratum) in g.strata.iter().enumerate() {
        if stratum
            .mrules
            .iter()
            .any(|&mid| reduplication_rule_is_peelable(g, mid))
            && g.strata[stratum_index..]
                .iter()
                .any(|s| !s.prules.is_empty())
        {
            gaps.push((ProposalRecallGap::PhonologicalReduplication,
                "surface-peeled reduplication followed by phonology",
                "the peeler matches equal surface copies and cannot invert phonology that changes copied material"));
        }
        if stratum.mrules.len() > 1
            && stratum.mrules.iter().any(|mid| {
                g.mrules[mid.0 as usize]
                    .affix_allomorphs()
                    .is_some_and(|allomorphs| {
                        allomorphs.iter().any(|a| {
                            rhs_drops_lhs_material(a)
                                && a.rhs.iter().all(|action| {
                                    matches!(
                                        action,
                                        OutputAction::Copy(_) | OutputAction::InsertSegments { .. }
                                    )
                                })
                        })
                    })
            })
        {
            gaps.push((ProposalRecallGap::TruncationComposition,
                "material-dropping morphology composed with another rule",
                "structural truncation and ordinary copied-host affix routes do not preserve every composed derivation"));
        }
        for id in &stratum.prules {
            let PhonRuleDef::Rewrite(rule) = &g.prules[id.0 as usize] else {
                continue;
            };
            let table = &g.char_tables[stratum.table.0 as usize];
            if !pg_rules::rewrite::pattern_has_single_spellings(g, table, &rule.lhs.nodes)
                || rule.subrules.iter().any(|subrule| {
                    !pg_rules::rewrite::pattern_has_single_spellings(g, table, &subrule.rhs.nodes)
                })
            {
                gaps.push((ProposalRecallGap::AmbiguousSegmentSpelling,
                    "rewrite input or output lacks a singleton spelling proof",
                    "every unresolved segment bundle must resolve to exactly one concrete spelling, including all representations and feature-unification alternatives; variables and structural pattern forms lack this proof"));
            }
            for subrule in &rule.subrules {
                if rule.lhs.nodes.is_empty() && has_class(g, &subrule.rhs.nodes, false) {
                    gaps.push((ProposalRecallGap::SymbolicEpenthesis,
                        "epenthesis with a feature-class output",
                        "the inserted underspecified feature bundle is not faithfully represented by concrete segment spellings"));
                }
                if rule.lhs.nodes.len() > subrule.rhs.nodes.len()
                    && has_class(g, &subrule.rhs.nodes, true)
                {
                    gaps.push((ProposalRecallGap::NarrowingClassOutput,
                        "narrowing rewrite with a segment-class output",
                        "narrowing can generate an ambiguous class bundle whose later rule behavior is lost by concrete surface probing"));
                }
            }
        }
    }
    let mut diagnostics = Vec::new();
    let mut occurrences: Vec<RefusalOccurrenceCount> = Vec::new();
    for (gap, construct, witness) in gaps {
        let cause = CapabilityRefusal::ProposalRecall(gap);
        if let Some(count) = occurrences.iter_mut().find(|count| count.cause == cause) {
            count.occurrences += 1;
        } else {
            occurrences.push(RefusalOccurrenceCount {
                strategy: EmissionStrategy::TunedSurfaceProbed,
                cause,
                occurrences: 1,
            });
        }
        if diagnostics
            .iter()
            .any(|d: &CapabilityDiagnostic| d.refusal == CapabilityRefusal::ProposalRecall(gap))
        {
            continue;
        }
        diagnostics.push(CapabilityDiagnostic {
            refusal: CapabilityRefusal::ProposalRecall(gap),
            predicate: PREDICATE,
            construct: construct.to_string(),
            witness: witness.to_string(),
        });
    }
    RefusalReport {
        diagnostics,
        occurrences,
    }
}
