//! Compound rules: authored (`LoadEndoCompoundingRule`/`LoadExoCompoundingRule`,
//! HCLoader.cs:1842-2001) or, when the snapshot declares none and `NoDefaultCompounding` is not
//! set, the two synthesized defaults (`DefaultCompoundingRules`, HCLoader.cs:1808-1840).

use pg_snapshot::morphology::{CompoundConstituentRequirement, CompoundOutcome, CompoundRule};
use pg_snapshot::Snapshot;

use crate::model::{CompoundingRuleDef, CompoundingSubruleDef, MRuleId, MorphRuleDef, OutputAction, PartRef, Pattern};
use crate::GrammarError;

use super::{environment, Acc, Ctx};

pub(crate) fn build(
    snapshot: &Snapshot,
    ctx: &Ctx,
    acc: &mut Acc,
    morphology_mrules: &mut Vec<MRuleId>,
    warnings: &mut Vec<String>,
) -> Result<(), GrammarError> {
    let rules: Vec<&CompoundRule> = snapshot
        .morphology
        .compound_rules
        .iter()
        .filter(|r| !r.disabled())
        .collect();

    if snapshot.morphology.compound_rules.is_empty()
        && !snapshot.morphology.parser_parameters.no_default_compounding
    {
        for id in default_compounding_rules(ctx, acc) {
            morphology_mrules.push(id);
        }
        return Ok(());
    }

    for rule in rules {
        let max_apps = snapshot
            .morphology
            .parser_parameters
            .compound_rule_max_applications
            .iter()
            .find(|m| m.compound_rule == rule.guid())
            .map(|m| m.max_applications as u16)
            .unwrap_or(1);
        match rule {
            CompoundRule::Endocentric {
                name,
                head_last,
                left,
                right,
                overriding,
                ..
            } => {
                if let Some(id) =
                    build_endo(name, *head_last, left, right, overriding, max_apps, ctx, acc, warnings)
                {
                    morphology_mrules.push(id);
                }
            }
            CompoundRule::Exocentric { name, left, right, to, .. } => {
                for id in build_exo(name, left, right, to, max_apps, ctx, acc, warnings) {
                    morphology_mrules.push(id);
                }
            }
        }
    }
    Ok(())
}

fn head_nonhead_patterns(ctx: &Ctx) -> (Vec<Pattern>, Vec<Pattern>) {
    (
        vec![Pattern {
            nodes: environment::any_plus(ctx),
        }],
        vec![Pattern {
            nodes: environment::any_plus(ctx),
        }],
    )
}

/// `DefaultCompoundingRules` (HCLoader.cs:1808-1840): "Default Left Head Compounding" (head first)
/// and "Default Right Head Compounding" (head second), both with no POS/MPR requirements.
fn default_compounding_rules(ctx: &Ctx, acc: &mut Acc) -> Vec<MRuleId> {
    let mut out = Vec::new();
    for (name, head_first) in [
        ("Default Left Head Compounding", true),
        ("Default Right Head Compounding", false),
    ] {
        let (head_lhs, non_head_lhs) = head_nonhead_patterns(ctx);
        let rhs = plus_join(head_first, ctx);
        let empty = acc.fs_interner.intern(pg_featstruct::FeatureStruct::EMPTY);
        let mrule_id = MRuleId(acc.mrules.len() as u32);
        acc.mrules.push(MorphRuleDef::Compounding(CompoundingRuleDef {
            xml_id: name.to_string(),
            name: Some(name.to_string()),
            blockable: true,
            max_apps: 1,
            head_required_syn_fs: empty,
            non_head_required_syn_fs: empty,
            out_syn_fs: empty,
            head_prod_restrictions_mpr: crate::model::MprSet::EMPTY,
            non_head_prod_restrictions_mpr: crate::model::MprSet::EMPTY,
            output_prod_restrictions_mpr: crate::model::MprSet::EMPTY,
            obligatory_features: Vec::new(),
            subrules: vec![CompoundingSubruleDef {
                vars: crate::model::VarTable::default(),
                required_mpr: crate::model::MprSet::EMPTY,
                excluded_mpr: crate::model::MprSet::EMPTY,
                out_mpr: crate::model::MprSet::EMPTY,
                head_lhs,
                non_head_lhs,
                rhs,
            }],
        }));
        out.push(mrule_id);
    }
    out
}

/// `Copy(head), "+", Copy(nonhead)` or the reverse, depending on which constituent comes first in
/// the output surface form.
fn plus_join(head_first: bool, ctx: &Ctx) -> Vec<OutputAction> {
    let plus = crate::segment::segment(ctx.table, "+").expect("'+' always segments (morph boundary)");
    let insert = OutputAction::InsertSegments {
        table: ctx.table_id,
        shape: crate::model::SegmentedText {
            text: "+".to_string(),
            shape: plus,
        },
    };
    if head_first {
        vec![
            OutputAction::Copy(PartRef::Head(0)),
            insert,
            OutputAction::Copy(PartRef::NonHead(0)),
        ]
    } else {
        vec![
            OutputAction::Copy(PartRef::NonHead(0)),
            insert,
            OutputAction::Copy(PartRef::Head(0)),
        ]
    }
}

#[allow(clippy::too_many_arguments)]
fn build_endo(
    name: &str,
    head_last: bool,
    left: &CompoundConstituentRequirement,
    right: &CompoundConstituentRequirement,
    overriding: &CompoundOutcome,
    max_apps: u16,
    ctx: &Ctx,
    acc: &mut Acc,
    warnings: &mut Vec<String>,
) -> Option<MRuleId> {
    let (head_side, non_head_side) = if head_last { (right, left) } else { (left, right) };
    let head_required_syn_fs = side_required_fs(head_side, ctx, acc, warnings)?;
    let non_head_required_syn_fs = side_required_fs(non_head_side, ctx, acc, warnings)?;
    let out_pos = overriding.part_of_speech.as_deref().and_then(|p| ctx.pos.bits_single(p));
    let out_syn_fs = match super::features::build_syn_fs(ctx.syn, out_pos, None) {
        Ok(fs) => acc.fs_interner.intern(fs),
        Err(e) => {
            warnings.push(format!("compound rule {name:?}: {e}; skipped"));
            return None;
        }
    };
    let out_mpr = overriding
        .inflection_class
        .as_deref()
        .and_then(|ic| ctx.mpr.infl_class_single(ic))
        .unwrap_or(crate::model::MprSet::EMPTY);

    let (head_lhs, non_head_lhs) = head_nonhead_patterns(ctx);
    let rhs = plus_join(!head_last, ctx);

    let mrule_id = MRuleId(acc.mrules.len() as u32);
    acc.mrules.push(MorphRuleDef::Compounding(CompoundingRuleDef {
        xml_id: format!("endo#{name}"),
        name: Some(name.to_string()),
        blockable: true,
        max_apps,
        head_required_syn_fs,
        non_head_required_syn_fs,
        out_syn_fs,
        head_prod_restrictions_mpr: side_mpr(head_side, ctx, warnings),
        non_head_prod_restrictions_mpr: side_mpr(non_head_side, ctx, warnings),
        output_prod_restrictions_mpr: crate::model::MprSet::EMPTY,
        obligatory_features: Vec::new(),
        subrules: vec![CompoundingSubruleDef {
            vars: crate::model::VarTable::default(),
            required_mpr: crate::model::MprSet::EMPTY,
            excluded_mpr: crate::model::MprSet::EMPTY,
            out_mpr,
            head_lhs,
            non_head_lhs,
            rhs,
        }],
    }));
    Some(mrule_id)
}

/// `LoadExoCompoundingRule` (HCLoader.cs:1922-2001): produces *two* rules, one per output-head
/// order, since an exocentric compound's own morphosyntax is stipulated rather than inherited.
#[allow(clippy::too_many_arguments)]
fn build_exo(
    name: &str,
    left: &CompoundConstituentRequirement,
    right: &CompoundConstituentRequirement,
    to: &CompoundOutcome,
    max_apps: u16,
    ctx: &Ctx,
    acc: &mut Acc,
    warnings: &mut Vec<String>,
) -> Vec<MRuleId> {
    let Some(left_fs) = side_required_fs(left, ctx, acc, warnings) else {
        return Vec::new();
    };
    let Some(right_fs) = side_required_fs(right, ctx, acc, warnings) else {
        return Vec::new();
    };
    let out_pos = to.part_of_speech.as_deref().and_then(|p| ctx.pos.bits_single(p));
    let out_syn_fs = match super::features::build_syn_fs(ctx.syn, out_pos, None) {
        Ok(fs) => acc.fs_interner.intern(fs),
        Err(e) => {
            warnings.push(format!("compound rule {name:?}: {e}; skipped"));
            return Vec::new();
        }
    };
    let out_mpr = to
        .inflection_class
        .as_deref()
        .and_then(|ic| ctx.mpr.infl_class_single(ic))
        .unwrap_or(crate::model::MprSet::EMPTY);
    let left_mpr = side_mpr(left, ctx, warnings);
    let right_mpr = side_mpr(right, ctx, warnings);

    let mut out = Vec::new();
    // "right compound rule": head = right, non-head = left, output = nonhead+"+"+head.
    {
        let (head_lhs, non_head_lhs) = head_nonhead_patterns(ctx);
        let rhs = plus_join(false, ctx);
        let mrule_id = MRuleId(acc.mrules.len() as u32);
        acc.mrules.push(MorphRuleDef::Compounding(CompoundingRuleDef {
            xml_id: format!("exo-right#{name}"),
            name: Some(name.to_string()),
            blockable: true,
            max_apps,
            head_required_syn_fs: right_fs,
            non_head_required_syn_fs: left_fs,
            out_syn_fs,
            head_prod_restrictions_mpr: right_mpr,
            non_head_prod_restrictions_mpr: left_mpr,
            output_prod_restrictions_mpr: crate::model::MprSet::EMPTY,
            obligatory_features: Vec::new(),
            subrules: vec![CompoundingSubruleDef {
                vars: crate::model::VarTable::default(),
                required_mpr: crate::model::MprSet::EMPTY,
                excluded_mpr: crate::model::MprSet::EMPTY,
                out_mpr,
                head_lhs,
                non_head_lhs,
                rhs,
            }],
        }));
        out.push(mrule_id);
    }
    // "left compound rule": head = left, non-head = right, output = head+"+"+nonhead.
    {
        let (head_lhs, non_head_lhs) = head_nonhead_patterns(ctx);
        let rhs = plus_join(true, ctx);
        let mrule_id = MRuleId(acc.mrules.len() as u32);
        acc.mrules.push(MorphRuleDef::Compounding(CompoundingRuleDef {
            xml_id: format!("exo-left#{name}"),
            name: Some(name.to_string()),
            blockable: true,
            max_apps,
            head_required_syn_fs: left_fs,
            non_head_required_syn_fs: right_fs,
            out_syn_fs,
            head_prod_restrictions_mpr: left_mpr,
            non_head_prod_restrictions_mpr: right_mpr,
            output_prod_restrictions_mpr: crate::model::MprSet::EMPTY,
            obligatory_features: Vec::new(),
            subrules: vec![CompoundingSubruleDef {
                vars: crate::model::VarTable::default(),
                required_mpr: crate::model::MprSet::EMPTY,
                excluded_mpr: crate::model::MprSet::EMPTY,
                out_mpr,
                head_lhs,
                non_head_lhs,
                rhs,
            }],
        }));
        out.push(mrule_id);
    }
    out
}

fn side_required_fs(
    side: &CompoundConstituentRequirement,
    ctx: &Ctx,
    acc: &mut Acc,
    warnings: &mut Vec<String>,
) -> Option<pg_featstruct::FsId> {
    let pos_bits = side
        .part_of_speech
        .as_deref()
        .map(|p| ctx.pos.bits_with_descendants(std::iter::once(p)));
    match super::features::build_syn_fs(ctx.syn, pos_bits, None) {
        Ok(fs) => Some(acc.fs_interner.intern(fs)),
        Err(e) => {
            warnings.push(format!("compound rule: {e}; skipped"));
            None
        }
    }
}

fn side_mpr(side: &CompoundConstituentRequirement, ctx: &Ctx, warnings: &mut Vec<String>) -> crate::model::MprSet {
    let mut set = crate::model::MprSet::EMPTY;
    for f in &side.exception_features {
        match ctx.mpr.exception_feature(f) {
            Some(s) => set = set.union(s),
            None => warnings.push(format!("compound rule: exception feature {f:?} does not resolve")),
        }
    }
    set
}
