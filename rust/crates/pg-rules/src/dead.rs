//! Static report of root allomorphs no affix can follow: every affix allomorph rejects the entry's rule features (inflection class) or the allomorph's stem name. It reads the grammar and never changes a parse.
//!
//! The first affix a synthesis applies to a root sees the entry's own features, so an affix allomorph that fails its required or excluded feature gate there can never be that first affix. If every one fails, no word has an affix after this root.

use std::collections::BTreeMap;

use pg_grammar_model::model::{
    mpr_excluded_ok, mpr_required_ok, AllomorphId, Grammar, LexEntryId, MRuleId, MprSet,
    PatternNode, PhonRuleDef, RootAllomorphDef, StemNameId,
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
/// Every reason code, with the meaning a host shows.
pub const REASONS: [(&str, &str); 4] = [
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
            let (verdict, reasons) = if *accepting > 0 || affix_rules.is_empty() {
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
            let mut blocked_by: Vec<BlockingCount> = if verdict == Continuation::Live {
                Vec::new()
            } else {
                tally
                    .iter()
                    .map(|(b, (allomorphs, rules))| BlockingCount {
                        blocking: b.clone(),
                        allomorphs: *allomorphs,
                        rules: *rules,
                    })
                    .collect()
            };
            blocked_by.sort_by_key(|x| std::cmp::Reverse(x.allomorphs));
            blocked_by.truncate(5);
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
            });
        }
    }
    out
}
