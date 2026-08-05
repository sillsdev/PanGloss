//! Phonological rules: rewrite rules (`LoadRewriteRule`, HCLoader.cs:2003-2101) placed on the
//! stratum `NotOnClitics` selects (HCLoader.cs:313-317). Metathesis rules (HCLoader.cs:2103-2161)
//! are not implemented — each produces a warning, not a rule.

use pg_snapshot::phonology::{PhonContext, PhonologicalRule, RewriteRhs, RewriteRule};
use pg_snapshot::Snapshot;

use crate::model::{
    AlphaVar, AnchorSide, Dir, PRuleId, Pattern, PatternNode, PhonRuleDef, RewriteMode,
    RewriteRuleDef, RewriteSubruleDef, SimpleContext, VarTable,
};
use crate::GrammarError;

use super::Ctx;

/// Greek-letter alpha-variable names, in assignment order (`HCLoader.VariableNames`,
/// HCLoader.cs:37-41).
const VAR_NAMES: [&str; 24] = [
    "\u{3b1}", "\u{3b2}", "\u{3b3}", "\u{3b4}", "\u{3b5}", "\u{3b6}", "\u{3b7}", "\u{3b8}",
    "\u{3b9}", "\u{3ba}", "\u{3bb}", "\u{3bc}", "\u{3bd}", "\u{3be}", "\u{3bf}", "\u{3c0}",
    "\u{3c1}", "\u{3c3}", "\u{3c4}", "\u{3c5}", "\u{3c6}", "\u{3c7}", "\u{3c8}", "\u{3c9}",
];

/// All phonological-rule definitions, plus which of them run on the Morphology vs. Clitics
/// stratum (`NotOnClitics`, HCLoader.cs:313-317).
type RuleBuild = (Vec<PhonRuleDef>, Vec<PRuleId>, Vec<PRuleId>);

pub(crate) fn build(
    snapshot: &Snapshot,
    ctx: &Ctx,
    warnings: &mut Vec<String>,
) -> Result<RuleBuild, GrammarError> {
    let mut prules = Vec::new();
    let mut morphology_prules = Vec::new();
    let mut clitic_prules = Vec::new();

    // `m_notOnClitics` (default true) -> rules run on the morphophonemic (Morphology) stratum;
    // `false` -> the clitic stratum (HCLoader.cs:313-317).
    let on_morphology = snapshot.morphology.parser_parameters.not_on_clitics;

    for rule in &snapshot.phonology.rules {
        match rule {
            PhonologicalRule::Rewrite(r) => match build_rewrite_rule(r, snapshot, ctx, warnings) {
                Ok(def) => {
                    let id = PRuleId(prules.len() as u32);
                    prules.push(PhonRuleDef::Rewrite(def));
                    if on_morphology {
                        morphology_prules.push(id);
                    } else {
                        clitic_prules.push(id);
                    }
                }
                Err(e) => warnings.push(format!("phonological rule {:?}: {e}; skipped", r.guid)),
            },
            PhonologicalRule::Metathesis(r) => {
                warnings.push(format!(
                    "unsupported: metathesis rule {:?} not implemented; skipped",
                    r.guid
                ));
            }
        }
    }

    Ok((prules, morphology_prules, clitic_prules))
}

fn dir_mode(d: pg_snapshot::phonology::RuleDirection) -> (Dir, RewriteMode) {
    use pg_snapshot::phonology::RuleDirection as D;
    match d {
        D::LeftToRight => (Dir::LeftToRight, RewriteMode::Iterative),
        D::RightToLeft => (Dir::RightToLeft, RewriteMode::Iterative),
        D::Simultaneous => (Dir::LeftToRight, RewriteMode::Simultaneous),
    }
}

fn build_var_table(
    guids: &[String],
    snapshot: &Snapshot,
    ctx: &Ctx,
    warnings: &mut Vec<String>,
) -> VarTable {
    let mut vars = Vec::new();
    for (i, g) in guids.iter().enumerate() {
        let Some(fc) = snapshot
            .phonology
            .feature_constraints
            .iter()
            .find(|c| &c.guid == g)
        else {
            warnings.push(format!("feature constraint {g:?} does not resolve"));
            continue;
        };
        let Some(flat) = ctx.phon.flat_index(&fc.feature) else {
            warnings.push(format!(
                "feature constraint {g:?}: unknown phonological feature {:?}",
                fc.feature
            ));
            continue;
        };
        let name = VAR_NAMES.get(i).copied().unwrap_or("?").to_string();
        vars.push((g.clone(), name, flat));
    }
    VarTable { vars }
}

fn build_rewrite_rule(
    r: &RewriteRule,
    snapshot: &Snapshot,
    ctx: &Ctx,
    warnings: &mut Vec<String>,
) -> Result<RewriteRuleDef, String> {
    let (dir, mode) = dir_mode(r.direction);
    let vars = build_var_table(&r.feature_constraint_variables, snapshot, ctx, warnings);

    let mut lhs_nodes = Vec::new();
    for c in &r.structural_description {
        lhs_nodes.extend(phon_context_nodes(c, ctx, &vars)?);
    }
    let lhs = Pattern { nodes: lhs_nodes };

    let mut subrules = Vec::new();
    for rhs in &r.right_hand_sides {
        subrules.push(build_subrule(rhs, &lhs, mode, ctx, &vars, warnings)?);
    }

    Ok(RewriteRuleDef {
        xml_id: r.guid.clone(),
        name: Some(r.name.clone()),
        mode,
        dir,
        vars,
        lhs,
        subrules,
    })
}

fn build_subrule(
    rhs: &RewriteRhs,
    lhs: &Pattern,
    mode: RewriteMode,
    ctx: &Ctx,
    vars: &VarTable,
    warnings: &mut Vec<String>,
) -> Result<RewriteSubruleDef, String> {
    let required_pos = if rhs.required_parts_of_speech.is_empty() {
        None
    } else {
        Some(
            ctx.pos
                .bits_with_descendants(rhs.required_parts_of_speech.iter().map(String::as_str)),
        )
    };

    let mut required_mpr = crate::model::MprSet::EMPTY;
    for f in &rhs.required_rule_features {
        match ctx.mpr.rule_feature(f) {
            Some(s) => required_mpr = required_mpr.union(s),
            None => warnings.push(format!("rule feature {f:?} does not resolve")),
        }
    }
    let mut excluded_mpr = crate::model::MprSet::EMPTY;
    for f in &rhs.excluded_rule_features {
        match ctx.mpr.rule_feature(f) {
            Some(s) => excluded_mpr = excluded_mpr.union(s),
            None => warnings.push(format!("rule feature {f:?} does not resolve")),
        }
    }

    let mut rhs_nodes = Vec::new();
    for c in &rhs.structural_change {
        rhs_nodes.extend(phon_context_nodes(c, ctx, vars)?);
    }
    let rhs_pattern = Pattern { nodes: rhs_nodes };

    let left_env = match &rhs.left_context {
        None => None,
        Some(c) => {
            let mut nodes = Vec::new();
            if left_is_word_boundary(c) {
                nodes.push(PatternNode::Anchor(AnchorSide::Left));
            }
            nodes.extend(phon_context_nodes(c, ctx, vars)?);
            Some(Pattern { nodes })
        }
    };
    let right_env = match &rhs.right_context {
        None => None,
        Some(c) => {
            let mut nodes = phon_context_nodes(c, ctx, vars)?;
            if right_is_word_boundary(c) {
                nodes.push(PatternNode::Anchor(AnchorSide::Right));
            }
            Some(Pattern { nodes })
        }
    };

    // Self-opaquing: a simplified, conservative port of `crate::load`'s `compute_self_opaquing`
    // (not reusable here — it is a private `load.rs` function and this crate's own module-privacy
    // keeps that loader frozen). Exact for
    // Iterative mode (always `false`) and epenthesis (`lhs` empty -> unconditionally `true` when
    // Simultaneous); for a feature-changing Simultaneous subrule this conservatively reports
    // `false` (no forced fixpoint repeat) rather than replicating the RHS/environment
    // feature-unifiability precheck — a documented gap, not a crash risk (affects only analysis of
    // Simultaneous-mode rules whose structural change and environment happen to pin the same
    // phonological feature to conflicting values).
    let self_opaquing = mode == RewriteMode::Simultaneous && lhs.nodes.is_empty();

    Ok(RewriteSubruleDef {
        required_pos,
        required_mpr,
        excluded_mpr,
        rhs: rhs_pattern,
        left_env,
        right_env,
        self_opaquing,
    })
}

fn left_is_word_boundary(pc: &PhonContext) -> bool {
    match pc {
        PhonContext::WordBoundary => true,
        PhonContext::Sequence { members } => {
            matches!(members.first(), Some(PhonContext::WordBoundary))
        }
        _ => false,
    }
}

fn right_is_word_boundary(pc: &PhonContext) -> bool {
    match pc {
        PhonContext::WordBoundary => true,
        PhonContext::Sequence { members } => {
            matches!(members.last(), Some(PhonContext::WordBoundary))
        }
        _ => false,
    }
}

/// `LoadPatternNode`'s recursive dispatch (HCLoader.cs:2313-2389), alpha-variable-aware version
/// used by rewrite rules (unlike `super::affixes::phon_context_nodes`, which rejects alpha variables —
/// `MoAffixProcess` input carries no variable scope in LCM).
fn phon_context_nodes(
    pc: &PhonContext,
    ctx: &Ctx,
    vars: &VarTable,
) -> Result<Vec<PatternNode>, String> {
    match pc {
        PhonContext::Sequence { members } => {
            let mut out = Vec::new();
            for m in members {
                out.extend(phon_context_nodes(m, ctx, vars)?);
            }
            Ok(out)
        }
        PhonContext::Iteration { min, max, member } => {
            let children = phon_context_nodes(member, ctx, vars)?;
            Ok(vec![PatternNode::Quantifier {
                min: (*min).max(0) as u32,
                max: if *max < 0 { None } else { Some(*max as u32) },
                children,
            }])
        }
        PhonContext::Segment { phoneme } => {
            let cd = ctx
                .phoneme_of
                .get(phoneme)
                .copied()
                .ok_or_else(|| format!("unknown phoneme {phoneme:?}"))?;
            Ok(vec![PatternNode::CharDef(cd)])
        }
        PhonContext::NaturalClass {
            natural_class,
            plus_variables,
            minus_variables,
        } => {
            let nc = ctx
                .natclass_by_guid
                .get(natural_class)
                .copied()
                .ok_or_else(|| format!("unknown natural class {natural_class:?}"))?;
            let mut alpha = Vec::new();
            for g in plus_variables {
                alpha.push(resolve_alpha(g, true, vars)?);
            }
            for g in minus_variables {
                alpha.push(resolve_alpha(g, false, vars)?);
            }
            Ok(vec![PatternNode::Context(SimpleContext {
                nat_class: nc,
                vars: alpha,
            })])
        }
        PhonContext::Boundary { marker } => {
            let cd = ctx
                .boundary_of
                .get(marker)
                .copied()
                .ok_or_else(|| format!("unknown boundary marker {marker:?}"))?;
            Ok(vec![PatternNode::CharDef(cd)])
        }
        PhonContext::WordBoundary => Ok(Vec::new()),
        PhonContext::Variable => Err(
            "PhVariable is only valid inside a MoAffixProcess input, not a rewrite-rule pattern"
                .to_string(),
        ),
    }
}

fn resolve_alpha(guid: &str, plus: bool, vars: &VarTable) -> Result<AlphaVar, String> {
    let var = vars
        .by_xml_id(guid)
        .ok_or_else(|| format!("feature constraint {guid:?} not in this rule's variable scope"))?;
    let feature = vars.vars[var.0 as usize].2;
    Ok(AlphaVar { feature, var, plus })
}
