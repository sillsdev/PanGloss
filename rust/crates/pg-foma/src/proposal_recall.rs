//! Grammar-only representability limits shared by emission and the capability envelope.

use pg_foma_runtime::emission_support::{reduplication_rule_is_peelable, rhs_drops_lhs_material};
use pg_grammar::model::{
    Grammar, MorphRuleDef, NaturalClassKind, OutputAction, PatternNode, PhonRuleDef,
};
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
    let spelling_gap = crate::spelling_route::SpellingRoutePlan::for_surface(g).err();
    refusals_with_spelling(g, spelling_gap)
}

pub(crate) fn refusals_with_spelling(
    g: &Grammar,
    spelling_gap: Option<ProposalRecallGap>,
) -> RefusalReport {
    let mut gaps = Vec::new();
    if let Some(gap) = spelling_gap {
        let (construct, witness) = match gap {
            ProposalRecallGap::AmbiguousSegmentSpelling => (
                "referenced character table lacks a singleton spelling closure",
                "literal root and affix emission do not preserve every spelling accepted by the character table's feature-unification closure",
            ),
            ProposalRecallGap::RecursiveCompoundDepth => (
                "recursive compound beyond the proved stem bound",
                "the spelling and identity proof covers at most four stems",
            ),
            ProposalRecallGap::RecursiveCompoundSpelling => (
                "recursive compound without a spelling proof",
                "the proof requires one table, no phonology, literal roots, and two intact stem blocks with only a boundary insertion at the junction",
            ),
            ProposalRecallGap::CompoundBoundarySpelling => (
                "written compound junction without a spelling proof",
                "the singleton route omits junction insertions and the spelling route could not certify this grammar",
            ),
            ProposalRecallGap::CompoundJunctionTokenization => (
                "compound spelling with unproved junction tokenization",
                "literal pieces must retain their foma symbols after the query is joined and normalized",
            ),
            ProposalRecallGap::SingletonRootBoundary => (
                "singleton-route grammar whose lexical entry spells a boundary inside its root",
                "the singleton route drops boundaries from root spellings, so a typed boundary inside a root is not proposed",
            ),
            ProposalRecallGap::CompoundJunctionPhonology => (
                "compound grammar with a phonological rule that can span a stem junction",
                "a deletion, epenthesis, metathesis or context-sensitive change can rewrite material on either side of the compound junction, and the singleton route does not certify that",
            ),
            ProposalRecallGap::RecursiveCompoundAsymmetricLicense => (
                "recursive compound with asymmetric stem licenses",
                "left growth can move a root from the first physical slot to a later slot; every subrule must license the same entries in both slots",
            ),
            ProposalRecallGap::RecursiveCompoundMprRestriction => (
                "recursive compound with MPR restrictions",
                "the restrictions inspect the compound head's MPR set, which the physical root path cannot certify",
            ),
            ProposalRecallGap::RecursiveCompoundMprOutput => (
                "recursive compound with MPR output",
                "the spelling proof does not cover changes to the compound head's MPR set",
            ),
            ProposalRecallGap::RecursiveCompoundAffixPlacement => (
                "recursive compound with unproved affix placement",
                "the recursive chain covers free prefixes on intact hosts at each stem; it cannot certify affixes that can move to an unsupported position between stems",
            ),
            _ => unreachable!("spelling proof returned an unrelated proposal recall gap"),
        };
        gaps.push((gap, construct, witness));
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
    if crate::emptied_morph::deletion_can_empty_morph_before_later_affix(g)
        || crate::emptied_morph::shape_edit_can_reorder_morphs_before_later_affix(g)
    {
        gaps.push((ProposalRecallGap::EmptiedMorphReorder,
            "deletion or insertion that can change which morph a later stratum's affix orders first",
            "Rust HC, like C#, orders a morph that lost every node, or sits beside a deleted or inserted node, by the later affix's copy, and the surface skeleton tags morphs in their lexical order"));
    }
    if partial_rule_deletion_or_reduplication(g) {
        gaps.push((ProposalRecallGap::PartialRuleDeletionOrReduplication,
            "partial morphological rule in a grammar that also has a deleting rewrite (from the first partial rule's stratum on) or a peelable reduplication",
            "stopgap refusal: the proposer loses analyses when a deletion removes affix material at a junction before a template-slot affix, or when a peelable reduplication precedes a later affix; neither needs a partial rule (#415, #416), but the two upstream partial-rule fixtures reach them and the predicate keys on a partial rule to keep every other admission unchanged"));
    }
    if deletion_with_environment_beside_affixes(g) {
        gaps.push((ProposalRecallGap::JunctionDeletion,
            "deleting or narrowing rewrite with an environment in a grammar that has affix rules",
            "a deletion whose left or right context reaches into the neighbouring morph removes material across a morph junction, and the proposer spells each morph on its own, so it misses the analysis (#415); a context-free deletion is not affected, but a rewrite with an environment is refused as a whole"));
    }
    if reduplication_before_later_affix(g) {
        gaps.push((ProposalRecallGap::ReduplicationBeforeLaterAffix,
            "peelable whole-stem reduplication in a grammar with an affix in a template slot or in a later stratum",
            "the reduplication peeler does not follow the reduplicant through a template-slot or later-stratum affix, so an analysis such as root, reduplication, suffix is not proposed (#416)"));
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
            if matches!(&g.prules[id.0 as usize], PhonRuleDef::Metathesis(rule)
                if crate::metathesis_span::metathesis_can_swap_across_morphs(g, *id, rule))
            {
                gaps.push((ProposalRecallGap::MetathesisAcrossMorphs,
                    "metathesis in a grammar whose morphology can place the switches in different morphs",
                    "a swap across a morph boundary reorders the morphs in the analysis, and the surface skeleton tags morphs in their order before phonology"));
            }
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

/// Stopgap: a partial affix rule plus a peelable reduplication or a deleting rewrite; TODO replace with fixes for the junction-deletion and reduplication-before-later-affix holes, tracked in #415 (deletion across a morph junction) and #416 (whole-stem reduplication before a template-slot or later-stratum affix).
fn partial_rule_deletion_or_reduplication(g: &Grammar) -> bool {
    let Some(first_partial) = g.strata.iter().position(|stratum| {
        stratum.mrules.iter().any(|&mid| {
            matches!(&g.mrules[mid.0 as usize], MorphRuleDef::AffixProcess(rule) if rule.is_partial())
        })
    }) else {
        return false;
    };
    let redup = g.strata.iter().any(|stratum| {
        stratum
            .mrules
            .iter()
            .any(|&mid| reduplication_rule_is_peelable(g, mid))
    });
    redup
        || g.strata[first_partial..].iter().any(|stratum| {
            stratum.prules.iter().any(|id| {
                matches!(&g.prules[id.0 as usize], PhonRuleDef::Rewrite(rule)
                    if crate::emptied_morph::rewrite_deletes(rule))
            })
        })
}

/// Whether a rewrite that writes fewer nodes than it matches has a non-empty left or right environment while the grammar has an affix rule (#415).
fn deletion_with_environment_beside_affixes(g: &Grammar) -> bool {
    let has_affix = g
        .mrules
        .iter()
        .any(|rule| rule.affix_allomorphs().is_some());
    has_affix
        && g.strata.iter().any(|stratum| {
            stratum.prules.iter().any(|id| {
                matches!(&g.prules[id.0 as usize], PhonRuleDef::Rewrite(rule)
                if crate::emptied_morph::rewrite_deletes(rule)
                    && rule.subrules.iter().any(|sub| {
                        [&sub.left_env, &sub.right_env]
                            .into_iter()
                            .flatten()
                            .any(|env| !env.nodes.is_empty())
                    }))
            })
        })
}

/// Whether a peelable reduplication sits in a grammar with an affix in some template slot, or in a stratum after the reduplication's own (#416).
fn reduplication_before_later_affix(g: &Grammar) -> bool {
    let is_affix = |mid: &pg_grammar::model::MRuleId| {
        g.mrules[mid.0 as usize].affix_allomorphs().is_some()
            && !reduplication_rule_is_peelable(g, *mid)
    };
    let template_affix = g
        .strata
        .iter()
        .flat_map(|stratum| &stratum.templates)
        .flat_map(|id| &g.templates[id.0 as usize].slots)
        .flat_map(|slot| &slot.rules)
        .any(is_affix);
    g.strata.iter().enumerate().any(|(index, stratum)| {
        stratum
            .mrules
            .iter()
            .any(|&mid| reduplication_rule_is_peelable(g, mid))
            && (template_affix
                || g.strata[index + 1..]
                    .iter()
                    .any(|later| later.mrules.iter().any(is_affix)))
    })
}
