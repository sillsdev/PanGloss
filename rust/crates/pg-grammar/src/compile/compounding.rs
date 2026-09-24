//! Compound rules: authored, or — when the snapshot declares none and `NoDefaultCompounding` is not set — the two synthesized `DefaultCompoundingRules` defaults.

use pg_snapshot::morphology::{CompoundConstituentRequirement, CompoundOutcome, CompoundRule};
use pg_snapshot::{InventoryKey, InventoryKind, IssueClass, Snapshot};

use crate::model::{
    CompoundingRuleDef, CompoundingSubruleDef, MRuleId, MorphRuleDef, OutputAction, PartRef,
    Pattern,
};
use crate::GrammarError;

use super::inventory::LineageTarget;
use super::{environment, issue_codes, roles, Acc, Ctx};

pub(crate) fn build(
    snapshot: &Snapshot,
    ctx: &Ctx,
    acc: &mut Acc,
    morphology_mrules: &mut Vec<MRuleId>,
) -> Result<(), GrammarError> {
    for r in &snapshot.morphology.compound_rules {
        ctx.considered(InventoryKey::object(
            InventoryKind::CompoundRule,
            r.guid().to_string(),
        ));
    }
    let rules: Vec<&CompoundRule> = snapshot
        .morphology
        .compound_rules
        .iter()
        .filter(|r| !r.disabled())
        .collect();

    if snapshot.morphology.compound_rules.is_empty()
        && !snapshot.morphology.parser_parameters.no_default_compounding
    {
        for id in default_compounding_rules(ctx, acc)? {
            morphology_mrules.push(id);
        }
        return Ok(());
    }

    for rule in rules {
        let key = InventoryKey::object(InventoryKind::CompoundRule, rule.guid().to_string());
        ctx.selected(key.clone());
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
                match build_endo(
                    rule.guid(),
                    name,
                    *head_last,
                    left,
                    right,
                    overriding,
                    max_apps,
                    ctx,
                    acc,
                )? {
                    Some(id) => {
                        morphology_mrules.push(id);
                        ctx.represent_via(LineageTarget::MRule(id.0), key);
                    }
                    None => {}
                }
            }
            CompoundRule::Exocentric {
                name,
                left,
                right,
                to,
                ..
            } => {
                let ids = build_exo(rule.guid(), name, left, right, to, max_apps, ctx, acc)?;
                if ids.is_empty() {
                    // `build_exo` records the issue at the failed requirement.
                } else {
                    // Both ids are always pushed to the same stratum list, so they survive/die together; the shared key rides on either one's lineage.
                    ctx.represent_via(LineageTarget::MRule(ids[0].0), key.clone());
                    for (&member_id, role) in ids.iter().zip([roles::EXO_RIGHT, roles::EXO_LEFT]) {
                        let expansion = InventoryKey::expansion(
                            InventoryKind::CompoundRule,
                            rule.guid().to_string(),
                            Vec::new(),
                            role,
                        );
                        ctx.synthesized(expansion.clone());
                        ctx.considered(expansion.clone());
                        ctx.selected(expansion.clone());
                        ctx.represent_via(LineageTarget::MRule(member_id.0), expansion);
                    }
                    for id in ids {
                        morphology_mrules.push(id);
                    }
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

/// The two synthesized defaults, head-first and head-second, both with no POS/MPR requirements.
fn default_compounding_rules(ctx: &Ctx, acc: &mut Acc) -> Result<Vec<MRuleId>, GrammarError> {
    let mut out = Vec::new();
    for (name, head_first) in [
        ("Default Left Head Compounding", true),
        ("Default Right Head Compounding", false),
    ] {
        let key = InventoryKey::object(InventoryKind::CompoundRule, name.to_string());
        ctx.synthesized(key.clone());
        ctx.considered(key.clone());
        ctx.selected(key.clone());
        let (head_lhs, non_head_lhs) = head_nonhead_patterns(ctx);
        let rhs = plus_join(head_first, ctx)?;
        let empty = acc.fs_interner.intern(pg_featstruct::FeatureStruct::EMPTY);
        let mrule_id = MRuleId(acc.mrules.len() as u32);
        ctx.represent_via(LineageTarget::MRule(mrule_id.0), key);
        acc.mrules
            .push(MorphRuleDef::Compounding(CompoundingRuleDef {
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
    Ok(out)
}

/// `Copy(head), "+", Copy(nonhead)` or the reverse, depending on which constituent comes first.
fn plus_join(head_first: bool, ctx: &Ctx) -> Result<Vec<OutputAction>, GrammarError> {
    let plus = crate::segment::segment(ctx.table, "+").map_err(|e| {
        GrammarError::UnsegmentableBoundary(format!(
            "table {:?} cannot segment the morph boundary \"+\" ({e}); a table needs at least \
             its boundary/phoneme definitions",
            ctx.table.xml_id()
        ))
    })?;
    let insert = OutputAction::InsertSegments {
        table: ctx.table_id,
        shape: crate::model::SegmentedText {
            text: "+".to_string(),
            shape: plus,
        },
    };
    Ok(if head_first {
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
    })
}

#[allow(clippy::too_many_arguments)]
fn build_endo(
    rule_guid: &str,
    name: &str,
    head_last: bool,
    left: &CompoundConstituentRequirement,
    right: &CompoundConstituentRequirement,
    overriding: &CompoundOutcome,
    max_apps: u16,
    ctx: &Ctx,
    acc: &mut Acc,
) -> Result<Option<MRuleId>, GrammarError> {
    let (head_side, non_head_side, head_role, non_head_role) = if head_last {
        (right, left, roles::RIGHT, roles::LEFT)
    } else {
        (left, right, roles::LEFT, roles::RIGHT)
    };
    let Some(head_required_syn_fs) =
        side_required_fs(rule_guid, name, head_role, head_side, ctx, acc)
    else {
        return Ok(None);
    };
    let Some(non_head_required_syn_fs) =
        side_required_fs(rule_guid, name, non_head_role, non_head_side, ctx, acc)
    else {
        return Ok(None);
    };
    let out_pos = overriding.part_of_speech.as_deref().and_then(|p| {
        let attachment = InventoryKey::attachment(
            InventoryKind::PartOfSpeech,
            rule_guid.to_string(),
            p.to_string(),
            roles::OUTPUT,
        );
        let bits = ctx.pos.bits_single(p);
        ctx.record_attachment(
            attachment,
            bits.is_some(),
            issue_codes::COMPOUND_SIDE_POS_UNRESOLVED,
            IssueClass::InvalidSource,
            "compound rule output: part of speech does not resolve",
        );
        bits
    });
    let out_syn_fs = match super::features::build_syn_fs(ctx.syn, out_pos, None) {
        Ok(fs) => acc.fs_interner.intern(fs),
        Err(_) => {
            ctx.reject(
                InventoryKey::object(InventoryKind::CompoundRule, rule_guid.to_string()),
                issue_codes::COMPOUND_RULE_BUILD_FAILED,
                IssueClass::UnrepresentableForHc,
                format!(
                    "Compound rule '{name}' could not be built; check its constituent categories."
                ),
            );
            return Ok(None);
        }
    };
    let out_mpr = overriding
        .inflection_class
        .as_deref()
        .and_then(|ic| ctx.mpr.infl_class_single(ic))
        .unwrap_or(crate::model::MprSet::EMPTY);

    let (head_lhs, non_head_lhs) = head_nonhead_patterns(ctx);
    let rhs = plus_join(!head_last, ctx)?;

    let mrule_id = MRuleId(acc.mrules.len() as u32);
    acc.mrules
        .push(MorphRuleDef::Compounding(CompoundingRuleDef {
            xml_id: format!("endo#{name}"),
            name: Some(name.to_string()),
            blockable: true,
            max_apps,
            head_required_syn_fs,
            non_head_required_syn_fs,
            out_syn_fs,
            head_prod_restrictions_mpr: side_mpr(rule_guid, head_role, head_side, ctx),
            non_head_prod_restrictions_mpr: side_mpr(rule_guid, non_head_role, non_head_side, ctx),
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
    Ok(Some(mrule_id))
}

/// Produces *two* rules, one per output-head order, since an exocentric compound's morphosyntax is stipulated rather than inherited.
#[allow(clippy::too_many_arguments)]
fn build_exo(
    rule_guid: &str,
    name: &str,
    left: &CompoundConstituentRequirement,
    right: &CompoundConstituentRequirement,
    to: &CompoundOutcome,
    max_apps: u16,
    ctx: &Ctx,
    acc: &mut Acc,
) -> Result<Vec<MRuleId>, GrammarError> {
    let Some(left_fs) = side_required_fs(rule_guid, name, roles::LEFT, left, ctx, acc) else {
        return Ok(Vec::new());
    };
    let Some(right_fs) = side_required_fs(rule_guid, name, roles::RIGHT, right, ctx, acc) else {
        return Ok(Vec::new());
    };
    let out_pos = to.part_of_speech.as_deref().and_then(|p| {
        let attachment = InventoryKey::attachment(
            InventoryKind::PartOfSpeech,
            rule_guid.to_string(),
            p.to_string(),
            roles::OUTPUT,
        );
        let bits = ctx.pos.bits_single(p);
        ctx.record_attachment(
            attachment,
            bits.is_some(),
            issue_codes::COMPOUND_SIDE_POS_UNRESOLVED,
            IssueClass::InvalidSource,
            "compound rule output: part of speech does not resolve",
        );
        bits
    });
    let out_syn_fs = match super::features::build_syn_fs(ctx.syn, out_pos, None) {
        Ok(fs) => acc.fs_interner.intern(fs),
        Err(_) => {
            ctx.reject(
                InventoryKey::object(InventoryKind::CompoundRule, rule_guid.to_string()),
                issue_codes::COMPOUND_RULE_BUILD_FAILED,
                IssueClass::UnrepresentableForHc,
                format!(
                    "Compound rule '{name}' could not be built; check its constituent categories."
                ),
            );
            return Ok(Vec::new());
        }
    };
    let out_mpr = to
        .inflection_class
        .as_deref()
        .and_then(|ic| ctx.mpr.infl_class_single(ic))
        .unwrap_or(crate::model::MprSet::EMPTY);
    let left_mpr = side_mpr(rule_guid, roles::LEFT, left, ctx);
    let right_mpr = side_mpr(rule_guid, roles::RIGHT, right, ctx);

    let mut out = Vec::new();
    // "right compound rule": head = right, non-head = left, output = nonhead+"+"+head.
    {
        let (head_lhs, non_head_lhs) = head_nonhead_patterns(ctx);
        let rhs = plus_join(false, ctx)?;
        let mrule_id = MRuleId(acc.mrules.len() as u32);
        acc.mrules
            .push(MorphRuleDef::Compounding(CompoundingRuleDef {
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
        let rhs = plus_join(true, ctx)?;
        let mrule_id = MRuleId(acc.mrules.len() as u32);
        acc.mrules
            .push(MorphRuleDef::Compounding(CompoundingRuleDef {
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
    Ok(out)
}

fn side_required_fs(
    rule_guid: &str,
    rule_name: &str,
    role: &str,
    side: &CompoundConstituentRequirement,
    ctx: &Ctx,
    acc: &mut Acc,
) -> Option<pg_featstruct::FsId> {
    let pos_bits = side.part_of_speech.as_deref().map(|p| {
        let attachment = InventoryKey::attachment(
            InventoryKind::PartOfSpeech,
            rule_guid.to_string(),
            p.to_string(),
            role.to_string(),
        );
        let resolved = ctx.pos.bits_single(p);
        ctx.record_attachment(
            attachment,
            resolved.is_some(),
            issue_codes::COMPOUND_SIDE_POS_UNRESOLVED,
            IssueClass::InvalidSource,
            "compound rule side: part of speech does not resolve",
        );
        ctx.pos.bits_with_descendants(std::iter::once(p))
    });
    match super::features::build_syn_fs(ctx.syn, pos_bits, None) {
        Ok(fs) => Some(acc.fs_interner.intern(fs)),
        Err(_) => {
            ctx.reject(
                InventoryKey::object(InventoryKind::CompoundRule, rule_guid.to_string()),
                issue_codes::COMPOUND_RULE_BUILD_FAILED,
                IssueClass::UnrepresentableForHc,
                format!("Compound rule '{rule_name}' could not be built; check its constituent categories."),
            );
            None
        }
    }
}

fn side_mpr(
    rule_guid: &str,
    role: &str,
    side: &CompoundConstituentRequirement,
    ctx: &Ctx,
) -> crate::model::MprSet {
    let mut set = crate::model::MprSet::EMPTY;
    for f in &side.exception_features {
        let attachment = InventoryKey::attachment(
            InventoryKind::RuleFeature,
            rule_guid.to_string(),
            f.clone(),
            role.to_string(),
        );
        let resolved = ctx.mpr.exception_feature(f);
        ctx.record_attachment(
            attachment,
            resolved.is_some(),
            issue_codes::COMPOUND_SIDE_EXCEPTION_FEATURE_UNRESOLVED,
            IssueClass::InvalidSource,
            format!("compound rule: exception feature {f:?} does not resolve"),
        );
        if let Some(s) = resolved {
            set = set.union(s);
        }
    }
    set
}
