//! Static report of alternates that can never be selected because an earlier alternate of the same morpheme covers them; it reads the grammar and never changes a parse.
//!
//! The rule mirrors the engine. A root alternate is rejected at final validity when any earlier alternate is eligible and does not free-fluctuate with it (`validity.rs`, the root arm). An affix alternate is rejected the same way against the earlier alternates synthesis applied (`passed_over`), and synthesis stops at an unconstrained applied alternate. So alternate `j` is `shadowed` by `i < j` when every condition `j` needs implies the condition `i` needs, proven here only from structural identity or emptiness; anything else is `maybe_shadowed` with a reason, or not a cover at all.

use pg_grammar_model::model::{
    AffixAllomorphDef, AllomorphId, EnvironmentDef, Grammar, LexEntryId, MRuleId, NaturalClassKind,
    Pattern, PatternNode, RootAllomorphDef,
};

use crate::morph::constraints_equal;
use crate::validity::{free_fluctuates, root_constraints_equal};

/// Typed result for one alternate.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Proven: no word can select this alternate.
    Shadowed,
    /// An earlier alternate may cover it; the reasons say what was not proven.
    MaybeShadowed,
    /// No earlier alternate covers it, or none was left undecided.
    Selectable,
}

impl Verdict {
    /// The wire code.
    pub fn code(self) -> &'static str {
        match self {
            Self::Shadowed => "shadowed",
            Self::MaybeShadowed => "maybe_shadowed",
            Self::Selectable => "selectable",
        }
    }
}

/// The morpheme whose alternates are compared.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    Entry(LexEntryId),
    Rule(MRuleId),
}

/// Why an alternate is shadowed (`UNCONSTRAINED_EARLIER`, `CONDITIONS_COVER`) or only maybe (the rest).
pub const UNCONSTRAINED_EARLIER: &str = "unconstrained_earlier";
pub const CONDITIONS_COVER: &str = "conditions_cover";
pub const ENVIRONMENT_UNDECIDED: &str = "environment_subsumption_undecided";
pub const LHS_UNDECIDED: &str = "lhs_subsumption_undecided";
pub const MPR_UNDECIDED: &str = "mpr_subsumption_undecided";
pub const SYN_UNDECIDED: &str = "required_features_subsumption_undecided";
pub const STEM_NAME_UNDECIDED: &str = "stem_name_overlap_undecided";
pub const CO_OCCURRENCE_UNMODELED: &str = "blocker_co_occurrence_unmodeled";

/// `MaybeShadowed` because another copy of the same FieldWorks allomorph is not shadowed.
pub const SOURCE_ELSEWHERE: &str = "source_allomorph_selectable_elsewhere";

/// Every reason code, with the meaning a host shows.
pub const REASONS: [(&str, &str); 9] = [
    (UNCONSTRAINED_EARLIER, "an earlier alternate has no environment and no condition, so it is eligible wherever this one is"),
    (CONDITIONS_COVER, "every condition of this alternate implies the earlier alternate's conditions"),
    (ENVIRONMENT_UNDECIDED, "the earlier alternate's environments are not shown to follow from this one's"),
    (LHS_UNDECIDED, "the input patterns differ and neither is shown to cover the other"),
    (MPR_UNDECIDED, "the rule-feature requirements differ and the cover is not shown"),
    (SYN_UNDECIDED, "the required syntactic features differ and the cover is not shown"),
    (STEM_NAME_UNDECIDED, "the stem names share a region, so which words pass each is not decided"),
    (SOURCE_ELSEWHERE, "another copy of the same FieldWorks allomorph (another environment pass or MSA) is not shadowed, so removing the allomorph would lose words"),
    (CO_OCCURRENCE_UNMODELED, "the earlier alternate has co-occurrence rules, which this version does not model"),
];

/// One alternate's result.
#[derive(Clone, Debug)]
pub struct Alternate {
    pub allomorph: AllomorphId,
    /// Position in the morpheme's allomorph order, the order the engine tries them.
    pub index: usize,
    pub verdict: Verdict,
    /// The earlier alternates responsible (all of them proven for `Shadowed`, all possible ones for `MaybeShadowed`); empty for `Selectable`.
    pub blockers: Vec<AllomorphId>,
    /// Reason codes: why shadowed, or what is undecided.
    pub reasons: Vec<&'static str>,
}

/// One morpheme's alternates, in engine order.
#[derive(Clone, Debug)]
pub struct MorphemeShadow {
    pub owner: Owner,
    pub alternates: Vec<Alternate>,
}

/// Does an earlier alternate cover a later one?
enum Judge {
    Yes,
    Maybe(Vec<&'static str>),
    No,
}

fn and(a: Judge, b: Judge) -> Judge {
    match (a, b) {
        (Judge::No, _) | (_, Judge::No) => Judge::No,
        (Judge::Maybe(mut x), Judge::Maybe(y)) => {
            x.extend(y);
            Judge::Maybe(x)
        }
        (Judge::Maybe(x), Judge::Yes) | (Judge::Yes, Judge::Maybe(x)) => Judge::Maybe(x),
        (Judge::Yes, Judge::Yes) => Judge::Yes,
    }
}

/// An environment accepted by nothing but a matching context: required with no side at all is always accepted.
fn always_accepted(env: &EnvironmentDef) -> bool {
    env.require && env.left.is_none() && env.right.is_none()
}

/// Whether `later` accepted implies `earlier` accepted, from identical or absent sides only.
fn env_implies(later: &EnvironmentDef, earlier: &EnvironmentDef) -> bool {
    if later == earlier || always_accepted(earlier) {
        return true;
    }
    let weaker = |l: &Option<Pattern>, e: &Option<Pattern>| e.is_none() || l == e;
    if later.require && earlier.require {
        return weaker(&later.left, &earlier.left) && weaker(&later.right, &earlier.right);
    }
    // An excluded environment is accepted when its sides do not match; a stronger match in `earlier` implies the weaker one in `later`.
    if !later.require && !earlier.require {
        return weaker(&earlier.left, &later.left) && weaker(&earlier.right, &later.right);
    }
    false
}

/// Whether `j`'s environment test passing guarantees `i`'s passes at the same span.
fn env_cover(later: &[EnvironmentDef], earlier: &[EnvironmentDef]) -> Judge {
    if earlier.is_empty() {
        return Judge::Yes;
    }
    if later.is_empty() {
        return if earlier.iter().any(always_accepted) {
            Judge::Yes
        } else {
            Judge::No
        };
    }
    if later
        .iter()
        .all(|l| earlier.iter().any(|e| env_implies(l, e)))
    {
        Judge::Yes
    } else {
        Judge::Maybe(vec![ENVIRONMENT_UNDECIDED])
    }
}

/// Whether the earlier alternate's stem-name gate passes whenever the later one's does.
fn stem_cover(g: &Grammar, i: &RootAllomorphDef, j: &RootAllomorphDef) -> Judge {
    match (i.stem_name, j.stem_name) {
        (None, None) => Judge::Yes,
        (Some(a), Some(b)) if a == b => Judge::Yes,
        (Some(a), Some(b)) => {
            let ra = &g.stem_names[a.0 as usize].regions;
            let rb = &g.stem_names[b.0 as usize].regions;
            if ra.iter().any(|r| rb.contains(r)) {
                Judge::Maybe(vec![STEM_NAME_UNDECIDED])
            } else {
                Judge::No
            }
        }
        // One side names a stem and the other does not: the unnamed side's excluded-region check refuses the named one's regions.
        _ => Judge::No,
    }
}

fn root_pair(g: &Grammar, allos: &[RootAllomorphDef], i: usize, j: usize) -> Judge {
    if free_fluctuates(allos, i, j, root_constraints_equal) {
        return Judge::No;
    }
    let (ai, aj) = (&allos[i], &allos[j]);
    if ai.is_bound && !aj.is_bound {
        return Judge::No;
    }
    let mut out = and(
        env_cover(&aj.environments, &ai.environments),
        stem_cover(g, ai, aj),
    );
    if !ai.co_occurrence.is_empty() {
        out = and(out, Judge::Maybe(vec![CO_OCCURRENCE_UNMODELED]));
    }
    out
}

/// A single-part pattern that matches any nonempty segment run: one unbounded quantifier over a class constrained only by the synthetic segment type.
fn universal(g: &Grammar, lhs: &[Pattern]) -> bool {
    let [part] = lhs else { return false };
    let [PatternNode::Quantifier {
        max: None,
        children,
        min,
    }] = part.nodes.as_slice()
    else {
        return false;
    };
    let [PatternNode::Context(ctx)] = children.as_slice() else {
        return false;
    };
    *min <= 1
        && ctx.vars.is_empty()
        && matches!(&g.natural_classes[ctx.nat_class.0 as usize].kind, NaturalClassKind::Feature(f) if f.iter().all(|(lane, _)| lane.0 as usize + 1 == g.phon_features.len()))
}

/// Parts built only of class contexts, so a match needs at least one segment and names no boundary.
fn contexts_only(lhs: &[Pattern]) -> bool {
    fn ok(node: &PatternNode) -> bool {
        match node {
            PatternNode::Context(_) => true,
            PatternNode::Quantifier { children, .. } => children.iter().all(ok),
            PatternNode::CharDef(_) | PatternNode::Segments { .. } | PatternNode::Anchor(_) => {
                false
            }
        }
    }
    !lhs.is_empty() && lhs.iter().all(|p| p.nodes.iter().all(ok))
}

fn lhs_cover(g: &Grammar, i: &AffixAllomorphDef, j: &AffixAllomorphDef) -> Judge {
    if i.lhs == j.lhs {
        return Judge::Yes;
    }
    if universal(g, &i.lhs) && contexts_only(&j.lhs) && j.lhs.iter().any(has_required_segment) {
        return Judge::Yes;
    }
    if universal(g, &j.lhs) && !universal(g, &i.lhs) {
        return Judge::No;
    }
    Judge::Maybe(vec![LHS_UNDECIDED])
}

/// A part with a non-optional context somewhere, so any match consumes a segment.
fn has_required_segment(p: &Pattern) -> bool {
    fn req(node: &PatternNode) -> bool {
        match node {
            PatternNode::Context(_) => true,
            PatternNode::Quantifier { min, children, .. } => *min > 0 && children.iter().any(req),
            _ => false,
        }
    }
    p.nodes.iter().any(req)
}

fn affix_pair(g: &Grammar, allos: &[AffixAllomorphDef], i: usize, j: usize) -> Judge {
    if free_fluctuates(allos, i, j, |a, b| constraints_equal(g, a, b)) {
        return Judge::No;
    }
    let (ai, aj) = (&allos[i], &allos[j]);
    let mut out = lhs_cover(g, ai, aj);
    let mpr_free = |a: &AffixAllomorphDef| a.required_mpr.is_empty() && a.excluded_mpr.is_empty();
    out = and(
        out,
        if mpr_free(ai)
            || (ai.required_mpr == aj.required_mpr && ai.excluded_mpr == aj.excluded_mpr)
        {
            Judge::Yes
        } else if mpr_free(aj) {
            Judge::No
        } else {
            Judge::Maybe(vec![MPR_UNDECIDED])
        },
    );
    out = and(out, env_cover(&aj.environments, &ai.environments));
    let fi = g.fs_interner.get(ai.required_syn_fs);
    let fj = g.fs_interner.get(aj.required_syn_fs);
    out = and(
        out,
        if fi.is_empty() || pg_featstruct::subsumes(fi, fj) {
            Judge::Yes
        } else if fj.is_empty() {
            Judge::No
        } else {
            Judge::Maybe(vec![SYN_UNDECIDED])
        },
    );
    if !ai.co_occurrence.is_empty() {
        out = and(out, Judge::Maybe(vec![CO_OCCURRENCE_UNMODELED]));
    }
    out
}

fn unconstrained_root(a: &RootAllomorphDef) -> bool {
    a.environments.is_empty() && !a.is_bound && a.stem_name.is_none() && a.co_occurrence.is_empty()
}

fn unconstrained_affix(g: &Grammar, a: &AffixAllomorphDef) -> bool {
    a.environments.is_empty()
        && g.fs_interner.get(a.required_syn_fs).is_empty()
        && a.required_mpr.is_empty()
        && a.excluded_mpr.is_empty()
        && a.co_occurrence.is_empty()
}

/// Folds each earlier alternate's judgement into `j`'s result.
fn classify(
    count: usize,
    ids: impl Fn(usize) -> AllomorphId,
    judge: impl Fn(usize, usize) -> Judge,
    unconstrained: impl Fn(usize) -> bool,
) -> Vec<Alternate> {
    let mut out = Vec::with_capacity(count);
    for j in 0..count {
        let mut yes = Vec::new();
        let mut maybe = Vec::new();
        let mut maybe_reasons: Vec<&'static str> = Vec::new();
        let mut proof = UNCONSTRAINED_EARLIER;
        for i in 0..j {
            match judge(i, j) {
                Judge::Yes => {
                    if !unconstrained(i) {
                        proof = CONDITIONS_COVER;
                    }
                    yes.push(ids(i));
                }
                Judge::Maybe(reasons) => {
                    maybe.push(ids(i));
                    for r in reasons {
                        if !maybe_reasons.contains(&r) {
                            maybe_reasons.push(r);
                        }
                    }
                }
                Judge::No => {}
            }
        }
        let (verdict, blockers, reasons) = if !yes.is_empty() {
            (Verdict::Shadowed, yes, vec![proof])
        } else if !maybe.is_empty() {
            (Verdict::MaybeShadowed, maybe, maybe_reasons)
        } else {
            (Verdict::Selectable, Vec::new(), Vec::new())
        };
        out.push(Alternate {
            allomorph: ids(j),
            index: j,
            verdict,
            blockers,
            reasons,
        });
    }
    out
}

/// Every morpheme with two or more alternates, each alternate typed. Read-only.
pub fn shadowed_allomorphs(g: &Grammar) -> Vec<MorphemeShadow> {
    let mut out = Vec::new();
    for (e, entry) in g.entries.iter().enumerate() {
        let allos = &entry.allomorphs;
        if allos.len() < 2 {
            continue;
        }
        out.push(MorphemeShadow {
            owner: Owner::Entry(LexEntryId(e as u32)),
            alternates: classify(
                allos.len(),
                |k| allos[k].id,
                |i, j| root_pair(g, allos, i, j),
                |i| unconstrained_root(&allos[i]),
            ),
        });
    }
    for (r, rule) in g.mrules.iter().enumerate() {
        let Some(allos) = rule.affix_allomorphs() else {
            continue;
        };
        if allos.len() < 2 {
            continue;
        }
        out.push(MorphemeShadow {
            owner: Owner::Rule(MRuleId(r as u32)),
            alternates: classify(
                allos.len(),
                |k| allos[k].id,
                |i, j| affix_pair(g, allos, i, j),
                |i| unconstrained_affix(g, &allos[i]),
            ),
        });
    }
    out
}
