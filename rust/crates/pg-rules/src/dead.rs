//! Static report of root allomorphs no affix can follow: every affix allomorph rejects the entry's rule features (inflection class) or the allomorph's stem name. It reads the grammar and never changes a parse.
//!
//! The first affix a synthesis applies to a root sees the entry's own features, so an affix allomorph that fails its required or excluded feature gate there can never be that first affix. If every one fails, no word has an affix after this root.

use std::collections::BTreeMap;

use pg_grammar_model::model::{
    mpr_excluded_ok, mpr_required_ok, AffixAllomorphDef, AllomorphId, Grammar, LexEntryId, MRuleId,
    MprSet, NaturalClassKind, OutputAction, PatternNode, PhonRuleDef, RootAllomorphDef, StemNameId,
};

/// What follows the root allomorph.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Continuation {
    /// Some affix allomorph accepts the root's features and stem name.
    Live,
    /// No affix can follow and the allomorph cannot stand as a bare word: no word selects it.
    Proven,
    /// No affix can follow, but the allomorph may still stand as a bare stem or in a compound.
    Maybe,
}

impl Continuation {
    pub fn code(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Proven => "proven",
            Self::Maybe => "maybe",
        }
    }
}

/// `Maybe` because the allomorph can stand bare.
pub const MAY_STAND_BARE: &str = "may_stand_bare";
/// `Maybe` because a compounding rule could use the root without an affix.
pub const COMPOUNDING_PRESENT: &str = "compounding_present";
/// `Proven` because the allomorph is bound.
pub const BOUND_ROOT: &str = "bound_root";
/// `Proven` because every environment needs a neighbouring segment a bare word lacks.
pub const NEEDS_NEIGHBOUR: &str = "environment_needs_neighbour";
/// `Maybe` because the allomorph needs a following segment that only class-rejected affixes can supply.
pub const NEEDS_REJECTED_SUPPLIER: &str = "environment_needs_class_rejected_neighbour";
/// Every reason code, with the meaning a host shows.
pub const REASONS: [(&str, &str); 5] = [
    (
        MAY_STAND_BARE,
        "no affix accepts it, but nothing stops the allomorph standing as a bare word",
    ),
    (
        COMPOUNDING_PRESENT,
        "the grammar has a compounding rule, which can use a root without an affix",
    ),
    (
        BOUND_ROOT,
        "the allomorph is bound, so a word needs more than it",
    ),
    (
        NEEDS_NEIGHBOUR,
        "every environment of the allomorph requires a segment beside it, which a bare word lacks",
    ),
    (
        NEEDS_REJECTED_SUPPLIER,
        "affixes attach, but every environment of the allomorph needs a following segment and each suffix that could start with it rejects the entry's features or stem name; text-based, so only maybe",
    ),
];

/// One distinct requirement that turned affix allomorphs away.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Blocking {
    /// `required_mpr`, `excluded_mpr` or `stem_name`.
    pub kind: &'static str,
    /// Feature names, or the stem name's id for `stem_name`.
    pub names: Vec<String>,
}

/// A requirement and how many affix allomorphs and rules it stopped.
#[derive(Clone, Debug)]
pub struct BlockingCount {
    pub blocking: Blocking,
    pub allomorphs: usize,
    pub rules: usize,
}

/// One root allomorph's result.
#[derive(Clone, Debug)]
pub struct DeadRow {
    pub entry: LexEntryId,
    pub allomorph: AllomorphId,
    pub index: usize,
    pub verdict: Continuation,
    pub reasons: Vec<&'static str>,
    /// The entry's own feature names, which every affix gate is tested against.
    pub features: Vec<String>,
    /// How many affix rules there are, and how many accept it.
    pub rules: usize,
    pub accepting_rules: usize,
    /// The most common blocking requirements, most frequent first; empty when `Live`.
    pub blocked_by: Vec<BlockingCount>,
    /// For an allomorph that needs a following segment: the requirements that turn away affixes that could supply it, even when other affixes accept the entry.
    pub neighbour_rejections: Vec<BlockingCount>,
    /// Whether an accepting null affix changes the entry's rule features, so the rejecting affixes may be reached after it.
    pub via_converter: bool,
}

fn names(g: &Grammar, set: &MprSet) -> Vec<String> {
    set.iter()
        .map(|id| g.mpr_names[id.0 as usize].clone())
        .collect()
}

fn needs_segment(nodes: &[PatternNode]) -> bool {
    nodes.iter().any(|n| match n {
        PatternNode::Context(_) => true,
        PatternNode::Quantifier { min, children, .. } => *min > 0 && needs_segment(children),
        _ => false,
    })
}

/// Whether every environment requires a segment beside the morph, so a lone root fails it.
fn envs_need_neighbour(a: &RootAllomorphDef) -> bool {
    !a.environments.is_empty()
        && a.environments.iter().all(|e| {
            e.require
                && (e.left.as_ref().is_some_and(|p| needs_segment(&p.nodes))
                    || e.right.as_ref().is_some_and(|p| needs_segment(&p.nodes)))
        })
}

/// A rewrite rule that can put a segment where none was: an empty target or a longer output.
fn can_insert(g: &Grammar) -> bool {
    g.prules.iter().any(|r| match r {
        PhonRuleDef::Rewrite(rule) => {
            rule.lhs.nodes.is_empty()
                || rule
                    .subrules
                    .iter()
                    .any(|s| s.rhs.nodes.len() > rule.lhs.nodes.len())
        }
        PhonRuleDef::Metathesis(_) => false,
    })
}

/// Why a rule's affix allomorphs all turn this root away, or `None` when one accepts it.
fn rejection(
    g: &Grammar,
    rule: MRuleId,
    have: &MprSet,
    stem: Option<StemNameId>,
) -> Option<Vec<Blocking>> {
    let def = &g.mrules[rule.0 as usize];
    let allos = def.affix_allomorphs()?;
    if let pg_grammar_model::model::MorphRuleDef::AffixProcess(p) = def {
        if p.required_stem_name.is_some() && p.required_stem_name != stem {
            let id = p.required_stem_name.map_or(0, |s| s.0);
            return Some(vec![Blocking {
                kind: "stem_name",
                names: vec![g.stem_names[id as usize].xml_id.clone()],
            }]);
        }
    }
    let mut out = Vec::new();
    for a in allos {
        if !mpr_required_ok(&g.mpr_groups, &a.required_mpr, have) {
            out.push(Blocking {
                kind: "required_mpr",
                names: names(g, &a.required_mpr),
            });
        } else if !mpr_excluded_ok(&g.mpr_groups, &a.excluded_mpr, have) {
            out.push(Blocking {
                kind: "excluded_mpr",
                names: names(g, &a.excluded_mpr),
            });
        } else {
            return None;
        }
    }
    Some(out)
}

/// The text a first right-context node asks for, or `None` when it is not a literal segment set.
fn first_right_texts(g: &Grammar, a: &RootAllomorphDef) -> Option<Vec<String>> {
    if a.environments.is_empty() || g.char_tables.len() != 1 {
        return None;
    }
    let table = &g.char_tables[0];
    let mut out = Vec::new();
    for e in &a.environments {
        let first = e.right.as_ref().filter(|_| e.require)?.nodes.first()?;
        match first {
            PatternNode::CharDef(id) => out.extend(table.get(*id).representations().to_vec()),
            PatternNode::Segments { shape, .. } => out.push(shape.text.clone()),
            PatternNode::Context(c) => match &g.natural_classes.get(c.nat_class.0 as usize)?.kind {
                NaturalClassKind::Segments(ids) => {
                    for id in ids {
                        out.extend(table.get(*id).representations().to_vec());
                    }
                }
                NaturalClassKind::Feature(_) => return None,
            },
            _ => return None,
        }
    }
    Some(out)
}

/// Whether an affix allomorph may put material next to the stem that starts one of `texts`; unknown shapes and any that change rule features may.
fn may_supply(a: &AffixAllomorphDef, texts: &[String]) -> bool {
    if !a.out_mpr.is_empty() {
        return true;
    }
    match a.rhs.as_slice() {
        [OutputAction::Copy(_), OutputAction::InsertSegments { shape, .. }] => {
            let ins = shape.text.trim_start_matches('+');
            !ins.is_empty()
                && texts
                    .iter()
                    .any(|t| ins.starts_with(t.as_str()) || t.starts_with(ins))
        }
        [OutputAction::InsertSegments { .. }, OutputAction::Copy(_)] | [OutputAction::Copy(_)] => {
            false
        }
        _ => true,
    }
}

/// The requirement that turns one affix allomorph away from an entry, or `None` when it accepts.
fn turned_away(
    g: &Grammar,
    rule: MRuleId,
    a: &AffixAllomorphDef,
    have: &MprSet,
    stem: Option<StemNameId>,
) -> Option<Blocking> {
    if let pg_grammar_model::model::MorphRuleDef::AffixProcess(p) = &g.mrules[rule.0 as usize] {
        if p.required_stem_name.is_some() && p.required_stem_name != stem {
            let id = p.required_stem_name.map_or(0, |s| s.0);
            return Some(Blocking {
                kind: "stem_name",
                names: vec![g.stem_names[id as usize].xml_id.clone()],
            });
        }
    }
    if !mpr_required_ok(&g.mpr_groups, &a.required_mpr, have) {
        Some(Blocking {
            kind: "required_mpr",
            names: names(g, &a.required_mpr),
        })
    } else if !mpr_excluded_ok(&g.mpr_groups, &a.excluded_mpr, have) {
        Some(Blocking {
            kind: "excluded_mpr",
            names: names(g, &a.excluded_mpr),
        })
    } else {
        None
    }
}

/// What the affixes beside an allomorph that needs a following segment do with the entry.
struct Neighbour {
    /// Requirements that turned away affix allomorphs which could supply the segment.
    rejected: Tally,
    /// Affix allomorphs that could supply it and accept the entry.
    accepted: usize,
    /// An accepting null affix changes the rule features, so a rejecting affix may still be reached after it.
    converter: bool,
}

fn neighbour(
    g: &Grammar,
    affix_rules: &[MRuleId],
    entry: &pg_grammar_model::model::LexEntryDef,
    a: &RootAllomorphDef,
) -> Option<Neighbour> {
    let texts = first_right_texts(g, a)?;
    let mut n = Neighbour {
        rejected: Tally::new(),
        accepted: 0,
        converter: false,
    };
    for &rule in affix_rules {
        let mut seen = Vec::new();
        for x in g.mrules[rule.0 as usize].affix_allomorphs()? {
            let blocked = turned_away(g, rule, x, &entry.mpr, a.stem_name);
            if blocked.is_none() && !x.out_mpr.is_empty() && is_null_shape(x) {
                n.converter = true;
            }
            if !may_supply(x, &texts) {
                continue;
            }
            match blocked {
                None => n.accepted += 1,
                Some(b) => {
                    let t = n.rejected.entry(b.clone()).or_default();
                    t.0 += 1;
                    if !seen.contains(&b) {
                        t.1 += 1;
                        seen.push(b);
                    }
                }
            }
        }
    }
    Some(n)
}

/// An affix allomorph that adds no segments.
fn is_null_shape(a: &AffixAllomorphDef) -> bool {
    match a.rhs.as_slice() {
        [OutputAction::Copy(_)] => true,
        [OutputAction::Copy(_), OutputAction::InsertSegments { shape, .. }] => {
            shape.text.trim_start_matches('+').is_empty()
        }
        _ => false,
    }
}

fn counts(t: &Tally) -> Vec<BlockingCount> {
    let mut v: Vec<BlockingCount> = t
        .iter()
        .map(|(b, (allomorphs, rules))| BlockingCount {
            blocking: b.clone(),
            allomorphs: *allomorphs,
            rules: *rules,
        })
        .collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.allomorphs));
    v.truncate(5);
    v
}

/// Blocking requirement to (allomorphs stopped, rules stopped).
type Tally = BTreeMap<Blocking, (usize, usize)>;

/// Every root allomorph of every entry, typed. Read-only.
pub fn dead_continuations(g: &Grammar) -> Vec<DeadRow> {
    let affix_rules: Vec<MRuleId> = g
        .mrules
        .iter()
        .enumerate()
        .filter(|(_, r)| r.affix_allomorphs().is_some())
        .map(|(i, _)| MRuleId(i as u32))
        .collect();
    let compounding = g.mrules.iter().any(|r| r.affix_allomorphs().is_none());
    let inserts = can_insert(g);
    let mut out = Vec::new();
    for (e, entry) in g.entries.iter().enumerate() {
        let features = names(g, &entry.mpr);
        // Rejections depend on the entry's features and the stem name only; cache per stem name.
        let mut cache: BTreeMap<Option<u32>, (usize, Tally)> = BTreeMap::new();
        for (index, a) in entry.allomorphs.iter().enumerate() {
            let key = a.stem_name.map(|s| s.0);
            let (accepting, tally) = cache.entry(key).or_insert_with(|| {
                let mut accepting = 0;
                let mut tally = Tally::new();
                for &rule in &affix_rules {
                    match rejection(g, rule, &entry.mpr, a.stem_name) {
                        None => accepting += 1,
                        Some(blocks) => {
                            let mut seen = Vec::new();
                            for b in blocks {
                                let t = tally.entry(b.clone()).or_default();
                                t.0 += 1;
                                if !seen.contains(&b) {
                                    t.1 += 1;
                                    seen.push(b);
                                }
                            }
                        }
                    }
                }
                (accepting, tally)
            });
            let near = if *accepting > 0 && !compounding {
                neighbour(g, &affix_rules, entry, a)
            } else {
                None
            };
            let dead_by_supplier = near.as_ref().is_some_and(|n| n.accepted == 0);
            let tally = match &near {
                Some(n) if dead_by_supplier => &n.rejected,
                _ => tally,
            };
            let (verdict, reasons) = if dead_by_supplier {
                (Continuation::Maybe, vec![NEEDS_REJECTED_SUPPLIER])
            } else if *accepting > 0 || affix_rules.is_empty() {
                (Continuation::Live, Vec::new())
            } else if compounding {
                (Continuation::Maybe, vec![COMPOUNDING_PRESENT])
            } else if a.is_bound {
                (Continuation::Proven, vec![BOUND_ROOT])
            } else if envs_need_neighbour(a) && !inserts {
                (Continuation::Proven, vec![NEEDS_NEIGHBOUR])
            } else {
                (Continuation::Maybe, vec![MAY_STAND_BARE])
            };
            let blocked_by = if verdict == Continuation::Live {
                Vec::new()
            } else {
                counts(tally)
            };
            let neighbour_rejections = near.as_ref().map_or_else(Vec::new, |n| counts(&n.rejected));
            let via_converter = near.as_ref().is_some_and(|n| n.converter);
            out.push(DeadRow {
                entry: LexEntryId(e as u32),
                allomorph: a.id,
                index,
                verdict,
                reasons,
                features: features.clone(),
                rules: affix_rules.len(),
                accepting_rules: *accepting,
                blocked_by,
                neighbour_rejections,
                via_converter,
            });
        }
    }
    out
}
