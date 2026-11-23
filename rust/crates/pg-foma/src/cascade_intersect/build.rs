//! Compiles a grammar to layers and checks the accounting (plan 3.1, 7 row W5).
//!
//! Layer list: `Lex | zone of stratum 0 | … | zone of the last stratum | Repr`. `Spell` is the
//! identity here (see [`super::spell`]). The build ends with the **accounting check**: every
//! root allomorph, every allomorph of every placed affix or realizational rule, every template
//! slot and every stratum is mapped to a layer, or the rule is named unreached; a part with no
//! layer fails the build (mutations C23, C46).

use std::collections::{BTreeMap, BTreeSet};

use pg_foma_runtime::cascade::analyzer::CascadeLayers;
use pg_foma_runtime::cascade::BuildMeter;
use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::{Grammar, MRuleId, MorphRuleDef, MorphRuleOrder};

use super::lex::build_lex;
use super::refusal::{check, BuildError, MAX_LAYERS};
use super::repr::build_repr;
use super::spell::{spell_table, Spelling};
use super::zone::{build_zone, classify, AlloShape, Alt, Position, ZoneKind};

/// What was mapped to a layer, for the accounting check and the report.
#[derive(Clone, Debug, Default)]
pub struct Accounting {
    pub root_allomorphs: BTreeSet<(u32, usize)>,
    pub placed_allomorphs: BTreeSet<(MRuleId, usize)>,
    pub unreached_rules: BTreeSet<MRuleId>,
    pub slots: BTreeSet<(usize, usize)>,
    pub zones: BTreeSet<usize>,
    pub layers: usize,
    /// Tokens whose only spelling is epsilon.
    pub epsilon_only_tokens: usize,
}

fn rule_alts(g: &Grammar, id: MRuleId) -> Result<(Vec<Alt>, u32), BuildError> {
    let rule = &g.mrules[id.0 as usize];
    let (allomorphs, max_apps) = match rule {
        MorphRuleDef::AffixProcess(r) => (&r.allomorphs, u32::from(r.max_apps)),
        // A realizational rule applies at most once per derivation, wherever it is listed.
        MorphRuleDef::Realizational(r) => (&r.allomorphs, 1),
        MorphRuleDef::Compounding(_) => {
            return Err(BuildError::Accounting(format!(
                "compounding rule {} reached the zone builder",
                id.0
            )))
        }
    };
    let mut alts = Vec::new();
    for (k, a) in allomorphs.iter().enumerate() {
        let AlloShape::Edge { pre, suf } = classify(a) else {
            return Err(BuildError::Accounting(format!(
                "interior allomorph {k} of rule {} reached the zone builder",
                id.0
            )));
        };
        alts.push(Alt {
            rule: id,
            allomorph: k,
            pre,
            suf,
        });
    }
    Ok((alts, max_apps))
}

/// Compiles `g`. `supplied_roots` is the reference morpher's `has_supplied_roots()`.
pub fn build_cascade(
    g: &Grammar,
    supplied_roots: bool,
    meter: &mut BuildMeter,
) -> Result<(CascadeLayers, Accounting), BuildError> {
    check(g, supplied_roots)?;
    let table = &g.char_tables[0];
    let universe = table.len() as u32;
    let spelling = spell_table(table, meter)?;
    let char_ids: BTreeMap<char, u32> = Spelling::alphabet(table)
        .into_iter()
        .enumerate()
        .map(|(i, c)| (c, i as u32))
        .collect();
    let boundaries: Vec<u32> = table
        .iter()
        .filter(|(_, cd)| cd.kind() == CharDefKind::Boundary)
        .map(|(id, _)| id.0)
        .collect();

    let mut acct = Accounting {
        epsilon_only_tokens: spelling.epsilon_only,
        ..Accounting::default()
    };
    let lex = build_lex(g, universe, meter)?;
    acct.root_allomorphs = lex.allomorphs;
    let mut layers = vec![lex.layer];
    let mut names = vec!["Lex".to_string()];

    let mut placed_rules: BTreeSet<MRuleId> = BTreeSet::new();
    let mut expected_slots = BTreeSet::new();
    let mut zone_placements: Vec<BTreeSet<(MRuleId, usize)>> = Vec::new();
    for (si, stratum) in g.strata.iter().enumerate() {
        let mut positions = Vec::new();
        for &id in &stratum.mrules {
            let (alts, max_apps) = rule_alts(g, id)?;
            placed_rules.insert(id);
            positions.push(Position::new(max_apps, alts));
        }
        for &tid in &stratum.templates {
            for (k, slot) in g.templates[tid.0 as usize].slots.iter().enumerate() {
                let mut alts = Vec::new();
                for &id in &slot.rules {
                    alts.extend(rule_alts(g, id)?.0);
                    placed_rules.insert(id);
                }
                expected_slots.insert((tid.0 as usize, k));
                acct.slots.insert((tid.0 as usize, k));
                positions.push(Position::new(1, alts));
            }
        }
        let kind = match stratum.mrule_order {
            MorphRuleOrder::Linear => ZoneKind::Ordered,
            MorphRuleOrder::Unordered => ZoneKind::Pool,
        };
        let name = format!("Zone {si}");
        let zone = build_zone(&name, universe, &boundaries, kind, &positions, meter)?;
        if !zone.layer.identity_test(meter)? {
            return Err(BuildError::Accounting(format!(
                "{name} does not contain the identity from its start state"
            )));
        }
        zone_placements.push(zone.placed);
        acct.zones.insert(si);
        layers.push(zone.layer);
        names.push(name);
    }
    for placed in &zone_placements {
        acct.placed_allomorphs.extend(placed.iter().copied());
    }
    layers.push(build_repr(&spelling, &char_ids, meter)?);
    names.push("Repr".to_string());
    if layers.len() > MAX_LAYERS {
        return Err(BuildError::Accounting(format!(
            "{} layers above the limit {MAX_LAYERS}",
            layers.len()
        )));
    }
    acct.layers = layers.len();

    for (i, rule) in g.mrules.iter().enumerate() {
        let id = MRuleId(i as u32);
        if !placed_rules.contains(&id) && rule.affix_allomorphs().is_some() {
            acct.unreached_rules.insert(id);
        }
    }
    verify(g, &acct, &placed_rules, &expected_slots)?;
    Ok((
        CascadeLayers {
            layers,
            names,
            char_ids,
        },
        acct,
    ))
}

/// The accounting check: the grammar's parts, enumerated afresh, against what was mapped.
fn verify(
    g: &Grammar,
    acct: &Accounting,
    placed_rules: &BTreeSet<MRuleId>,
    expected_slots: &BTreeSet<(usize, usize)>,
) -> Result<(), BuildError> {
    let mut missing = Vec::new();
    for (ei, entry) in g.entries.iter().enumerate() {
        for ai in 0..entry.allomorphs.len() {
            if !acct.root_allomorphs.contains(&(ei as u32, ai)) {
                missing.push(format!("root allomorph {ai} of entry {ei}"));
            }
        }
    }
    for (i, rule) in g.mrules.iter().enumerate() {
        let id = MRuleId(i as u32);
        let Some(allomorphs) = rule.affix_allomorphs() else {
            continue;
        };
        if placed_rules.contains(&id) {
            for k in 0..allomorphs.len() {
                if !acct.placed_allomorphs.contains(&(id, k)) {
                    missing.push(format!("allomorph {k} of rule {i}"));
                }
            }
        } else if !acct.unreached_rules.contains(&id) {
            missing.push(format!("rule {i} (neither placed nor named unreached)"));
        }
    }
    for slot in expected_slots {
        if !acct.slots.contains(slot) {
            missing.push(format!("slot {} of template {}", slot.1, slot.0));
        }
    }
    for si in 0..g.strata.len() {
        if !acct.zones.contains(&si) {
            missing.push(format!("stratum {si}"));
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(BuildError::Accounting(missing.join("; ")))
    }
}
