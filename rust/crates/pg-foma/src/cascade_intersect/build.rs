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
use pg_foma_runtime::cascade::{Layer, LayerScratch, NfaLayer, TagKind};
use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::{Grammar, MRuleId, MorphRuleDef, MorphRuleOrder};

use super::lex::build_lex;
use super::recipe::{recipe_t, RecipeCtx};
use super::refusal::{check, construct, BuildError, Refusal, MAX_LAYERS};
use super::repr::build_repr;
use super::schedule::{build_h, HRule};
use super::spell::{spell_table, Spelling};
use super::zone::{build_zone, classify, AlloShape, Alt, Position, ZoneKind};

/// What was mapped to a layer, for the accounting check and the report.
#[derive(Clone, Debug, Default)]
pub struct Accounting {
    pub root_allomorphs: BTreeSet<(u32, usize)>,
    pub placed_allomorphs: BTreeSet<(MRuleId, usize)>,
    pub unreached_rules: BTreeSet<MRuleId>,
    pub slots: BTreeSet<(usize, usize)>,
    /// Rules whose tag appears on some arc of a built zone layer.
    pub emitted_rules: BTreeSet<MRuleId>,
    pub zones: BTreeSet<usize>,
    /// Allomorphs placed in a zone layer.
    pub zone_allomorphs: BTreeSet<(MRuleId, usize)>,
    /// Allomorphs placed in an interior (`H`) layer.
    pub h_allomorphs: BTreeSet<(MRuleId, usize)>,
    /// Interior rules of Linear strata: every allomorph of these is in the rule's own layers.
    pub linear_interior_rules: BTreeSet<MRuleId>,
    /// Per Unordered stratum with interior rules: `A_s` read from the grammar and from the layers.
    pub a_s: Vec<(usize, u32, u32)>,
    /// Number of interior (`H`) layers, repeats included.
    pub h_layers: usize,
    pub layers: usize,
    /// Tokens whose only spelling is epsilon.
    pub epsilon_only_tokens: usize,
}

fn rule_parts(
    g: &Grammar,
    id: MRuleId,
) -> Result<(&[pg_grammar::model::AffixAllomorphDef], u32), BuildError> {
    match &g.mrules[id.0 as usize] {
        MorphRuleDef::AffixProcess(r) => Ok((&r.allomorphs, u32::from(r.max_apps))),
        // A realizational rule applies at most once per derivation, wherever it is listed.
        MorphRuleDef::Realizational(r) => Ok((&r.allomorphs, 1)),
        MorphRuleDef::Compounding(_) => Err(BuildError::Accounting(format!(
            "compounding rule {} reached the zone builder",
            id.0
        ))),
    }
}

/// The bound of a rule: `max_apps`, and 1 for a realizational rule.
fn rule_bound(g: &Grammar, id: MRuleId) -> Result<u32, BuildError> {
    Ok(rule_parts(g, id)?.1.max(1))
}

/// A rule is interior iff at least one allomorph is (plan 2.4.2).
fn rule_interior(g: &Grammar, id: MRuleId) -> Result<bool, BuildError> {
    Ok(rule_parts(g, id)?
        .0
        .iter()
        .any(|a| matches!(classify(a), AlloShape::Interior(_))))
}

/// The edge-only allomorphs of a rule as zone alternatives.
fn edge_alts(g: &Grammar, id: MRuleId) -> Result<(Vec<Alt>, u32), BuildError> {
    let (allomorphs, max_apps) = rule_parts(g, id)?;
    let mut alts = Vec::new();
    for (k, a) in allomorphs.iter().enumerate() {
        if let AlloShape::Edge { pre, suf } = classify(a) {
            alts.push(Alt {
                rule: id,
                allomorph: k,
                pre,
                suf,
            });
        }
    }
    Ok((alts, max_apps))
}

/// Recipe T machines of a rule: every allomorph, or only the interior ones.
fn rule_h(
    ctx: &RecipeCtx,
    g: &Grammar,
    id: MRuleId,
    interior_only: bool,
) -> Result<HRule, BuildError> {
    let mut machines = Vec::new();
    for (k, a) in rule_parts(g, id)?.0.iter().enumerate() {
        if interior_only && matches!(classify(a), AlloShape::Edge { .. }) {
            continue;
        }
        machines.push((k, recipe_t(ctx, a)?));
    }
    Ok(HRule { rule: id, machines })
}

fn too_many_layers(n: usize) -> BuildError {
    BuildError::Refused(Refusal::new(
        construct::LAYERS,
        format!("grammar needs {n} layers"),
    ))
}

struct Stratum<'a> {
    names: &'a mut Vec<String>,
    layers: &'a mut Vec<NfaLayer>,
}

impl Stratum<'_> {
    fn push(&mut self, name: String, layer: NfaLayer) -> Result<(), BuildError> {
        if self.layers.len() + 1 >= MAX_LAYERS {
            return Err(too_many_layers(self.layers.len() + 2));
        }
        self.names.push(name);
        self.layers.push(layer);
        Ok(())
    }
}

/// Compiles `g`. `supplied_roots` is the reference morpher's `has_supplied_roots()`.
pub fn build_cascade(
    g: &Grammar,
    supplied_roots: bool,
    meter: &mut BuildMeter,
) -> Result<(CascadeLayers, Accounting), BuildError> {
    check(g, supplied_roots)?;
    let table = &g.char_tables[0];
    let spelling = spell_table(table, meter)?;
    let char_ids: BTreeMap<char, u32> = Spelling::alphabet(table)
        .into_iter()
        .enumerate()
        .map(|(i, c)| (c, i as u32))
        .collect();
    // One alphabet for every layer: the tokens and the NFD characters share the id space.
    let universe = table.len().max(char_ids.len()) as u32;
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

    let ctx = RecipeCtx::new(g).map_err(BuildError::Refused)?;
    for (si, stratum) in g.strata.iter().enumerate() {
        let mut slot_rules: Vec<Vec<MRuleId>> = Vec::new();
        for &tid in &stratum.templates {
            for (k, slot) in g.templates[tid.0 as usize].slots.iter().enumerate() {
                acct.slots.insert((tid.0 as usize, k));
                slot_rules.push(slot.rules.clone());
            }
        }
        let mut st = Stratum {
            names: &mut names,
            layers: &mut layers,
        };
        let mut zone_no = 0usize;
        let mut zone = |st: &mut Stratum,
                        acct: &mut Accounting,
                        kind: ZoneKind,
                        positions: &[Position],
                        meter: &mut BuildMeter|
         -> Result<(), BuildError> {
            let name = format!("Zone {si}.{zone_no}");
            zone_no += 1;
            let z = build_zone(&name, universe, &boundaries, kind, positions, meter)?;
            if !z.layer.identity_test(meter)? {
                return Err(BuildError::Accounting(format!(
                    "{name} does not contain the identity from its start state"
                )));
            }
            acct.emitted_rules.extend(rule_tags(&z.layer));
            acct.zone_allomorphs.extend(z.placed.iter().copied());
            st.push(name, z.layer)
        };
        match stratum.mrule_order {
            MorphRuleOrder::Linear => {
                let mut run: Vec<Position> = Vec::new();
                let mut hno = 0usize;
                for &id in &stratum.mrules {
                    if rule_interior(g, id)? {
                        if !run.is_empty() {
                            zone(&mut st, &mut acct, ZoneKind::Ordered, &run, meter)?;
                            run.clear();
                        }
                        acct.linear_interior_rules.insert(id);
                        let bound = rule_bound(g, id)? as usize;
                        if st.layers.len() + bound + 1 >= MAX_LAYERS {
                            return Err(too_many_layers(st.layers.len() + bound + 2));
                        }
                        let h = build_h(
                            &format!("H {si}.{hno}"),
                            universe,
                            &boundaries,
                            &[rule_h(&ctx, g, id, false)?],
                            meter,
                        )?;
                        hno += 1;
                        acct.emitted_rules.extend(rule_tags(&h.layer));
                        acct.h_allomorphs.extend(h.placed.iter().copied());
                        for c in 0..bound {
                            acct.h_layers += 1;
                            st.push(
                                format!("H {si}.{hno} rule {} copy {c}", id.0),
                                h.layer.clone(),
                            )?;
                        }
                    } else {
                        let (alts, max_apps) = edge_alts(g, id)?;
                        run.push(Position::new(max_apps, alts));
                    }
                }
                for rules in &slot_rules {
                    if rules
                        .iter()
                        .map(|&id| rule_interior(g, id))
                        .collect::<Result<Vec<_>, _>>()?
                        .contains(&true)
                    {
                        if !run.is_empty() {
                            zone(&mut st, &mut acct, ZoneKind::Ordered, &run, meter)?;
                            run.clear();
                        }
                        let mut hrules = Vec::new();
                        for &id in rules {
                            if rule_interior(g, id)? {
                                acct.linear_interior_rules.insert(id);
                            }
                            hrules.push(rule_h(&ctx, g, id, false)?);
                        }
                        let h = build_h(
                            &format!("H {si}.{hno}"),
                            universe,
                            &boundaries,
                            &hrules,
                            meter,
                        )?;
                        hno += 1;
                        acct.emitted_rules.extend(rule_tags(&h.layer));
                        acct.h_allomorphs.extend(h.placed.iter().copied());
                        acct.h_layers += 1;
                        st.push(format!("H {si}.{hno} slot"), h.layer)?;
                    } else {
                        let mut alts = Vec::new();
                        for &id in rules {
                            alts.extend(edge_alts(g, id)?.0);
                        }
                        run.push(Position::new(1, alts));
                    }
                }
                zone(&mut st, &mut acct, ZoneKind::Ordered, &run, meter)?;
            }
            MorphRuleOrder::Unordered => {
                let mut positions = Vec::new();
                let mut interior: Vec<MRuleId> = Vec::new();
                let mut roles: Vec<MRuleId> = stratum.mrules.clone();
                roles.extend(slot_rules.iter().flatten().copied());
                for &id in &roles {
                    let (alts, max_apps) = edge_alts(g, id)?;
                    positions.push(Position::new(max_apps, alts));
                    if rule_interior(g, id)? && !interior.contains(&id) {
                        interior.push(id);
                    }
                }
                let mut a_s = 0u32;
                for &id in &interior {
                    a_s += rule_bound(g, id)?;
                }
                zone(&mut st, &mut acct, ZoneKind::Pool, &positions, meter)?;
                if a_s > 0 {
                    if st.layers.len() + 2 * a_s as usize + 1 >= MAX_LAYERS {
                        return Err(too_many_layers(st.layers.len() + 2 * a_s as usize + 2));
                    }
                    let mut hrules = Vec::new();
                    for &id in &interior {
                        hrules.push(rule_h(&ctx, g, id, true)?);
                    }
                    let h = build_h(&format!("H {si}"), universe, &boundaries, &hrules, meter)?;
                    // The build assertion: A_s recomputed from the layer must equal the repeat count.
                    let mut from_layer = 0u32;
                    for &id in &h.members {
                        from_layer += rule_bound(g, id)?;
                    }
                    acct.a_s.push((si, a_s, from_layer));
                    if from_layer != a_s {
                        return Err(BuildError::Accounting(format!(
                            "stratum {si}: A_s is {a_s} from the grammar but {from_layer} from the interior layer"
                        )));
                    }
                    acct.emitted_rules.extend(rule_tags(&h.layer));
                    acct.h_allomorphs.extend(h.placed.iter().copied());
                    // Z_s is built once per copy; the layers are immutable and cloned.
                    let z = st.layers.pop().expect("the pool zone was just pushed");
                    let zname = st.names.pop().expect("the pool zone was just pushed");
                    st.layers.push(z.clone());
                    st.names.push(zname);
                    for c in 0..a_s {
                        acct.h_layers += 1;
                        st.push(format!("H {si} copy {c}"), h.layer.clone())?;
                        st.push(format!("Zone {si}.{}", c + 1), z.clone())?;
                    }
                }
            }
        }
        acct.zones.insert(si);
    }
    acct.placed_allomorphs
        .extend(acct.zone_allomorphs.iter().copied());
    acct.placed_allomorphs
        .extend(acct.h_allomorphs.iter().copied());
    layers.push(build_repr(&spelling, &char_ids, universe, meter)?);
    names.push("Repr".to_string());
    if layers.len() > MAX_LAYERS {
        return Err(too_many_layers(layers.len()));
    }
    acct.layers = layers.len();

    let (reachable, _) = reachable_parts(g);
    for (i, rule) in g.mrules.iter().enumerate() {
        let id = MRuleId(i as u32);
        if !reachable.contains(&id) && rule.affix_allomorphs().is_some() {
            acct.unreached_rules.insert(id);
        }
    }
    verify(g, &acct)?;
    Ok((
        CascadeLayers {
            layers,
            names,
            char_ids,
            supplied_roots,
        },
        acct,
    ))
}

/// The rules and template slots a grammar's strata can reach, read from the grammar alone.
fn reachable_parts(g: &Grammar) -> (BTreeSet<MRuleId>, BTreeSet<(usize, usize)>) {
    let (mut rules, mut slots) = (BTreeSet::new(), BTreeSet::new());
    for stratum in &g.strata {
        rules.extend(stratum.mrules.iter().copied());
        for &tid in &stratum.templates {
            for (k, slot) in g.templates[tid.0 as usize].slots.iter().enumerate() {
                slots.insert((tid.0 as usize, k));
                rules.extend(slot.rules.iter().copied());
            }
        }
    }
    (rules, slots)
}

/// The rule ids tagged on any arc of `layer`, read back from the built arcs.
fn rule_tags(layer: &NfaLayer) -> BTreeSet<MRuleId> {
    let mut scratch = LayerScratch::default();
    let mut out = BTreeSet::new();
    for s in 0..layer.state_count() as u32 {
        layer.arcs_free(&mut scratch, s, &mut |a| {
            for t in a.tags {
                if let Ok(TagKind::Rule(id)) = t.kind() {
                    out.insert(MRuleId(id));
                }
            }
        });
    }
    out
}

/// The accounting check: the grammar's parts, read from the grammar, against what was built.
fn verify(g: &Grammar, acct: &Accounting) -> Result<(), BuildError> {
    let (reachable, expected_slots) = reachable_parts(g);
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
        if reachable.contains(&id) {
            for k in 0..allomorphs.len() {
                if !acct.placed_allomorphs.contains(&(id, k)) {
                    missing.push(format!("allomorph {k} of rule {i}"));
                }
            }
            if !acct.emitted_rules.contains(&id) {
                missing.push(format!("rule {i} has no tagged arc in any zone layer"));
            }
            if acct.unreached_rules.contains(&id) {
                missing.push(format!("rule {i} is reachable but named unreached"));
            }
        } else if !acct.unreached_rules.contains(&id) {
            missing.push(format!("rule {i} (neither reachable nor named unreached)"));
        }
    }
    for (id, k) in &acct.zone_allomorphs {
        if acct.linear_interior_rules.contains(id) {
            missing.push(format!(
                "allomorph {k} of interior Linear rule {} is placed in a zone",
                id.0
            ));
        }
    }
    for slot in &expected_slots {
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

/// Runs the accounting check against `acct`, for tests.
pub fn verify_for_test(g: &Grammar, acct: &Accounting) -> Result<(), BuildError> {
    verify(g, acct)
}
