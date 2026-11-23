//! The refusal function: every construct outside W5's feature set is refused by name.
//!
//! A refusal text always starts `not modelled: <construct>` so a caller (and the fuzz gate) can
//! check that the named construct is the one found. Nothing is admitted on a guess: a construct
//! this module cannot classify is refused as `unclassified`.

use pg_foma_runtime::cascade::BuildRefused;
use pg_grammar::model::{
    Grammar, MorphRuleDef, OutputAction, PartRef, PatternNode, PhonRuleDef, RewriteMode,
};

use super::zone::{classify, AlloShape, InteriorKind};

/// Constructs the refusal function names. The first eight are the plan's refused constructs.
pub mod construct {
    pub const FAMILIES: &str = "families with a blockable rule";
    pub const SUPPLIED_ROOTS: &str = "supplied roots";
    pub const CHARACTER_TABLES: &str = "several character tables";
    pub const STEM_EDITING_COMPOUNDING: &str = "stem-editing compounding";
    pub const METATHESIS_MIDDLE_NODE: &str = "metathesis with a middle node";
    pub const OVERLAPPING_FOCUS: &str = "overlapping simultaneous multi-token focus";
    pub const CLASS_MODIFY: &str = "class Modify without feature lanes";
    pub const UNBOUNDED_TRUNCATION: &str = "unbounded truncation";
    // Outside W5's feature set, not refused for good: later work items build these.
    pub const PHONOLOGY: &str = "phonology rules";
    pub const COMPOUNDING: &str = "compounding";
    pub const INTERIOR: &str = "interior affix allomorph";
    pub const MODIFY: &str = "Modify or InsertContext allomorph";
    pub const PATTERN_ROOT: &str = "pattern root allomorph";
    pub const LAYERS: &str = "layer count";
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub construct: &'static str,
    pub detail: String,
}

impl Refusal {
    fn new(construct: &'static str, detail: impl Into<String>) -> Self {
        Refusal {
            construct,
            detail: detail.into(),
        }
    }

    pub fn text(&self) -> String {
        format!("not modelled: {}: {}", self.construct, self.detail)
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text())
    }
}

#[derive(Debug)]
pub enum BuildError {
    Refused(Refusal),
    /// `Refused { ConstructionBudget }` naming the layer or step.
    Budget(BuildRefused),
    /// The accounting check failed: part of the grammar has no layer and no named refusal.
    Accounting(String),
}

impl From<Refusal> for BuildError {
    fn from(r: Refusal) -> Self {
        BuildError::Refused(r)
    }
}

impl From<BuildRefused> for BuildError {
    fn from(r: BuildRefused) -> Self {
        BuildError::Budget(r)
    }
}

impl BuildError {
    pub fn text(&self) -> String {
        match self {
            BuildError::Refused(r) => r.text(),
            BuildError::Budget(b) => {
                format!("construction budget: {:?} while building {}", b.dim, b.what)
            }
            BuildError::Accounting(s) => format!("accounting check failed: {s}"),
        }
    }
}

/// The maximum number of layers of one grammar (plan 3.5 `K_layers`).
pub const MAX_LAYERS: usize = 256;

fn rule_label(rule: &MorphRuleDef, index: usize) -> String {
    rule.authored_name()
        .map_or_else(|| format!("mrule {index}"), str::to_string)
}

fn unbounded(nodes: &[PatternNode]) -> bool {
    nodes.iter().any(|n| match n {
        PatternNode::Quantifier { max, children, .. } => max.is_none() || unbounded(children),
        _ => false,
    })
}

/// Whether every subrule copies both members whole.
fn compounding_copies_whole(rule: &pg_grammar::model::CompoundingRuleDef) -> bool {
    rule.subrules.iter().all(|sub| {
        let run = |n: usize, make: fn(u16) -> PartRef| {
            n == 0
                || sub.rhs.windows(n).any(|w| {
                    w.iter()
                        .enumerate()
                        .all(|(i, a)| *a == OutputAction::Copy(make(i as u16)))
                })
        };
        run(sub.head_lhs.len(), PartRef::Head) && run(sub.non_head_lhs.len(), PartRef::NonHead)
    })
}

/// Every construct in `g` outside this backend's feature set, each named; empty when admitted.
pub fn check_all(g: &Grammar, supplied_roots: bool) -> Vec<Refusal> {
    let mut out = Vec::new();
    if supplied_roots {
        out.push(Refusal::new(
            construct::SUPPLIED_ROOTS,
            "the reference morpher has a supplied-root overlay",
        ));
    }
    if g.char_tables.len() != 1 {
        out.push(Refusal::new(
            construct::CHARACTER_TABLES,
            format!("the grammar has {} character tables", g.char_tables.len()),
        ));
        return out;
    }
    if !g.families.is_empty() {
        out.push(Refusal::new(
            construct::FAMILIES,
            "any grammar with families is refused: blocking replaces the word by another family member",
        ));
    }
    for (i, rule) in g.mrules.iter().enumerate() {
        if let MorphRuleDef::Compounding(c) = rule {
            out.push(if compounding_copies_whole(c) {
                Refusal::new(
                    construct::COMPOUNDING,
                    format!(
                        "{}: block compounding is not built yet",
                        rule_label(rule, i)
                    ),
                )
            } else {
                Refusal::new(
                    construct::STEM_EDITING_COMPOUNDING,
                    format!(
                        "compounding rule {} edits stem material",
                        rule_label(rule, i)
                    ),
                )
            });
        }
    }
    for pr in &g.prules {
        out.push(match pr {
            PhonRuleDef::Metathesis(m) => {
                let (lo, hi) = (
                    m.left_switch.min(m.right_switch),
                    m.left_switch.max(m.right_switch),
                );
                if hi > lo + 1 {
                    Refusal::new(
                        construct::METATHESIS_MIDDLE_NODE,
                        format!("rule {} has nodes between its switches", m.xml_id),
                    )
                } else {
                    Refusal::new(
                        construct::PHONOLOGY,
                        format!("metathesis rule {}", m.xml_id),
                    )
                }
            }
            PhonRuleDef::Rewrite(r) => {
                if r.mode == RewriteMode::Simultaneous && r.lhs.nodes.len() >= 2 {
                    Refusal::new(
                        construct::OVERLAPPING_FOCUS,
                        format!(
                            "rule {}: simultaneous multi-token focus (sites may overlap)",
                            r.xml_id
                        ),
                    )
                } else {
                    Refusal::new(construct::PHONOLOGY, format!("rewrite rule {}", r.xml_id))
                }
            }
        });
    }
    let featureless = g.char_tables[0]
        .iter()
        .all(|(_, cd)| !cd.has_authored_features());
    for (i, rule) in g.mrules.iter().enumerate() {
        let Some(allomorphs) = rule.affix_allomorphs() else {
            continue;
        };
        for (k, a) in allomorphs.iter().enumerate() {
            let AlloShape::Interior(kind) = classify(a) else {
                continue;
            };
            let at = format!("{}, allomorph {k}", rule_label(rule, i));
            out.push(match kind {
                InteriorKind::Modify | InteriorKind::InsertContext if featureless => {
                    Refusal::new(construct::CLASS_MODIFY, at)
                }
                InteriorKind::Modify | InteriorKind::InsertContext => {
                    Refusal::new(construct::MODIFY, at)
                }
                InteriorKind::Truncation => {
                    let copied: Vec<u16> = a
                        .rhs
                        .iter()
                        .filter_map(|x| match x {
                            OutputAction::Copy(PartRef::Input(k)) => Some(*k),
                            _ => None,
                        })
                        .collect();
                    let drops_unbounded = a
                        .lhs
                        .iter()
                        .enumerate()
                        .any(|(p, pat)| !copied.contains(&(p as u16)) && unbounded(&pat.nodes));
                    if drops_unbounded {
                        Refusal::new(construct::UNBOUNDED_TRUNCATION, at)
                    } else {
                        Refusal::new(construct::INTERIOR, format!("{at}: bounded truncation"))
                    }
                }
                InteriorKind::Other => Refusal::new(
                    construct::INTERIOR,
                    format!("{at}: infix, reduplication or reordering"),
                ),
            });
        }
    }
    for entry in &g.entries {
        if entry.allomorphs.iter().any(|a| a.is_pattern) {
            out.push(Refusal::new(
                construct::PATTERN_ROOT,
                format!("entry {}", entry.authored_id),
            ));
        }
    }
    if g.strata.len() + 2 > MAX_LAYERS {
        out.push(Refusal::new(
            construct::LAYERS,
            format!("grammar needs {} layers", g.strata.len() + 2),
        ));
    }
    out
}

/// The first refusal of [`check_all`].
pub fn check(g: &Grammar, supplied_roots: bool) -> Result<(), Refusal> {
    check_all(g, supplied_roots)
        .into_iter()
        .next()
        .map_or(Ok(()), Err)
}
