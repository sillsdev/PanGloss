//! The edge environment test of serve op `effects`: whether a rule's environment can be met across a morph's left or right edge by any segment the grammar lets stand there.
//! Every unproven route is an `unknown` reason and never a ruling out; see `docs/formats/serve-protocol.md`.

use std::collections::{BTreeSet, HashMap};

use pg_grammar::model::{Grammar, MorphRuleDef, OutputAction, PhonRuleDef, TableId};
use pg_rules::edge_env::{flatten, inventory, isolate, placements, FlatPattern, NodeTests};
use pg_rules::edge_env::{Neighbours, Seg};

use super::{segments_of, AllomorphForm, LhsVerdict, RuleItem};

/// The `status` of one edge entry.
pub(super) const EDGE_STATUSES: [&str; 3] = ["notAtEdge", "notRuledOut", "unknown"];
/// The reason of a `notAtEdge` entry whose environment no neighbour satisfies.
pub(super) const NO_NEIGHBOUR: &str = "no_neighbour_satisfies_environment";
/// The reason of a `notRuledOut` entry: some neighbour segment passes, which proves nothing more.
pub(super) const SATISFIABLE: &str = "environment_satisfiable";
/// Every reason an edge entry carries; the last four repeat the row's own verdict reasons.
pub(super) const EDGE_REASONS: [&str; 20] = [
    NO_NEIGHBOUR,
    SATISFIABLE,
    "multiple_strata",
    "inventory_not_modelled",
    "neighbour_may_change",
    "non_local",
    "metathesis",
    "null_form",
    "rule_not_compiled",
    "alpha_variable",
    "optional_or_repeated_group",
    "boundary_in_pattern",
    "pattern_not_compilable",
    "form_not_segmentable_in_stratum",
    "may_be_fed",
    "affix_modifies_stem",
    "empty_lhs_epenthesis",
    "stratum_precedes_morpheme",
    "lhs_match_budget",
    "form_partly_unmodelled",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Status {
    NotAtEdge,
    NotRuledOut,
    Unknown,
}

impl Status {
    pub(super) fn name(self) -> &'static str {
        EDGE_STATUSES[match self {
            Status::NotAtEdge => 0,
            Status::NotRuledOut => 1,
            Status::Unknown => 2,
        }]
    }
}

/// What the edge test says about one row, for each requested edge.
pub(super) struct EdgeAnswer {
    pub edges: Vec<(&'static str, Status, &'static str)>,
    /// True when no placement wholly inside the form is possible; `None` when unproven.
    pub not_inside: Option<bool>,
}

/// One rule row against one allomorph form.
#[derive(Clone, Copy)]
pub(super) struct Row<'a> {
    pub form: &'a AllomorphForm,
    pub segments: Option<&'a [Seg]>,
    pub item_index: usize,
    pub item: &'a RuleItem,
    /// The row's left-hand-side verdict.
    pub verdict: &'a LhsVerdict,
    /// An earlier rule or an affix may have changed the form.
    pub fed: Option<&'static str>,
}

type Compiled = Result<(FlatPattern, NodeTests), &'static str>;

pub(super) struct EdgeContext {
    table: TableId,
    /// A reason that makes every answer unknown.
    global: Option<&'static str>,
    pool: Vec<Seg>,
    /// The text of each pool segment, for the engine oracle.
    #[cfg(test)]
    pool_text: Vec<String>,
    before: Vec<usize>,
    after: Vec<usize>,
    /// For each item, whether an earlier rule may change a neighbour's edge segment.
    unstable: Vec<bool>,
    compiled: HashMap<(usize, usize), Compiled>,
    /// The target of each rewrite rule as node tests; `None` when it is not modelled.
    targets: HashMap<usize, Option<NodeTests>>,
}

/// Whether the form's edges are the edges of one contiguous run, with the stem (if any) wholly on one side.
pub(super) fn usable(form: &AllomorphForm) -> bool {
    !form.opaque
        && form.runs.len() <= 1
        && form.lhs_parts <= 1
        && !(form.copy_before && form.copy_after)
}

impl EdgeContext {
    pub(super) fn new(g: &Grammar, items: &[RuleItem]) -> EdgeContext {
        let table = g.strata.first().map_or(TableId(0), |s| s.table);
        let mut ctx = EdgeContext {
            table,
            global: None,
            pool: Vec::new(),
            #[cfg(test)]
            pool_text: Vec::new(),
            before: Vec::new(),
            after: Vec::new(),
            unstable: vec![false; items.len()],
            compiled: HashMap::new(),
            targets: HashMap::new(),
        };
        // Morphemes added after a stratum's rules ran would interleave with them; FieldWorks keeps its rules in the last stratum that has morphology.
        let last_morphology = g.strata.iter().rposition(|s| {
            !(s.entries.is_empty() && s.mrules.is_empty() && s.templates.is_empty())
        });
        let interleaved = last_morphology
            .is_some_and(|last| g.strata[..last].iter().any(|s| !s.prules.is_empty()));
        ctx.global = if interleaved || g.strata.iter().any(|s| s.table != table) {
            Some("multiple_strata")
        } else {
            None
        };
        if ctx.global.is_some() {
            return ctx;
        }
        let (inv, complete) = inventory(g, table);
        if !complete {
            ctx.global = Some("inventory_not_modelled");
            return ctx;
        }
        let mut index: HashMap<String, usize> = HashMap::new();
        for (text, seg) in inv {
            index.insert(text.clone(), ctx.pool.len());
            #[cfg(test)]
            ctx.pool_text.push(text);
            ctx.pool.push(seg);
        }
        let all: Vec<usize> = (0..ctx.pool.len()).collect();
        let (mut before, mut after) = (BTreeSet::new(), BTreeSet::new());
        // Any end the grammar cannot name, and guessed roots, may be any segment of the inventory.
        let (mut before_open, mut after_open) = (g.guess_roots, g.guess_roots);
        let mut copy_open = false;
        let mut lexicon: BTreeSet<usize> = BTreeSet::new();
        let mut add =
            |ctx: &mut EdgeContext, set: &mut BTreeSet<usize>, open: &mut bool, text: &str| {
                let at = match index.get(text) {
                    Some(at) => Some(*at),
                    None => isolate(g, table, text).map(|seg| {
                        #[cfg(test)]
                        ctx.pool_text.push(text.to_string());
                        ctx.pool.push(seg);
                        index.insert(text.to_string(), ctx.pool.len() - 1);
                        ctx.pool.len() - 1
                    }),
                };
                match at {
                    Some(at) => {
                        set.insert(at);
                    }
                    None => *open = true,
                }
            };
        for entry in &g.entries {
            for a in &entry.allomorphs {
                if a.shape.text.is_empty() {
                    continue;
                }
                match segments_of(g, table, &a.shape.text) {
                    Ok(segs) => {
                        for seg in &segs {
                            let mut open = false;
                            let mut scratch = BTreeSet::new();
                            add(&mut ctx, &mut scratch, &mut open, seg);
                            lexicon.extend(scratch);
                            before_open |= open;
                            after_open |= open;
                        }
                        if let (Some(first), Some(last)) = (segs.first(), segs.last()) {
                            add(&mut ctx, &mut before, &mut before_open, last);
                            add(&mut ctx, &mut after, &mut after_open, first);
                        }
                    }
                    Err(_) => {
                        before_open = true;
                        after_open = true;
                    }
                }
            }
        }
        // A form that is not one contiguous run beside its whole stem copies or splits stem material, so any segment of the lexicon may stand beside it.
        for rule in &g.mrules {
            for a in rule.affix_allomorphs().unwrap_or(&[]) {
                let Ok(form) = super::affix_form(g, a) else {
                    before_open = true;
                    after_open = true;
                    continue;
                };
                let mut each = |ctx: &mut EdgeContext, set: &mut BTreeSet<usize>, text: &str| {
                    let mut open = false;
                    add(ctx, set, &mut open, text);
                    open
                };
                let (mut lex_open, mut scratch) = (false, BTreeSet::new());
                for seg in &form.segments {
                    lex_open |= each(&mut ctx, &mut scratch, seg);
                }
                lexicon.extend(scratch);
                if lex_open {
                    before_open = true;
                    after_open = true;
                }
                if usable(&form) {
                    if let (Some(first), Some(last)) = (form.segments.first(), form.segments.last())
                    {
                        add(&mut ctx, &mut before, &mut before_open, last);
                        add(&mut ctx, &mut after, &mut after_open, first);
                    }
                } else {
                    copy_open = true;
                }
            }
        }
        if copy_open {
            before.extend(&lexicon);
            after.extend(&lexicon);
        }
        // A compound puts two whole words side by side, so their ends are already in the sets; what a compounding subrule writes or changes itself adds to them.
        for rule in &g.mrules {
            let MorphRuleDef::Compounding(def) = rule else {
                continue;
            };
            for sub in &def.subrules {
                if sub.head_lhs.len() != 1 || sub.non_head_lhs.len() != 1 {
                    before_open = true;
                    after_open = true;
                }
                for action in &sub.rhs {
                    match action {
                        OutputAction::Copy(_) => {}
                        OutputAction::InsertSegments { table, shape } => {
                            match segments_of(g, *table, &shape.text) {
                                Ok(segs) => {
                                    if let (Some(first), Some(last)) = (segs.first(), segs.last()) {
                                        add(&mut ctx, &mut before, &mut before_open, last);
                                        add(&mut ctx, &mut after, &mut after_open, first);
                                    }
                                }
                                Err(_) => {
                                    before_open = true;
                                    after_open = true;
                                }
                            }
                        }
                        _ => {
                            before_open = true;
                            after_open = true;
                        }
                    }
                }
            }
        }
        ctx.before = if before_open {
            all.clone()
        } else {
            before.into_iter().collect()
        };
        ctx.after = if after_open {
            all
        } else {
            after.into_iter().collect()
        };
        for item in items.iter().filter(|i| i.unlisted.is_none()) {
            ctx.targets.entry(item.prule).or_insert_with(|| {
                let PhonRuleDef::Rewrite(rule) = &g.prules[item.prule] else {
                    return None;
                };
                let flat = flatten(g, table, None, &rule.lhs, None).ok()?;
                flat.compile(g, table).ok()
            });
        }
        ctx.unstable = ctx.unstable_prefixes(g, items);
        ctx
    }

    /// For each item, whether a rule before it could change a segment that may stand at a morph's edge: a rule not provably unable to touch one (its target accepts one, is empty, is a metathesis, or is not modelled).
    fn unstable_prefixes(&self, g: &Grammar, items: &[RuleItem]) -> Vec<bool> {
        let neighbours: Vec<&Seg> = self
            .before
            .iter()
            .chain(&self.after)
            .map(|i| &self.pool[*i])
            .collect();
        let mut memo: HashMap<usize, bool> = HashMap::new();
        // The subrules of one rule are alternatives of a single application, so a rule counts only for the rules after it.
        let (mut running, mut pending, mut last) = (false, false, None);
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            if item.unlisted.is_some() {
                out.push(running);
                continue;
            }
            if last != Some(item.prule) {
                running |= pending;
                pending = false;
                last = Some(item.prule);
            }
            out.push(running);
            let inert = *memo.entry(item.prule).or_insert_with(|| {
                self.targets.get(&item.prule).is_some_and(|tests| {
                    tests.as_ref().is_some_and(|tests| {
                        !neighbours.iter().any(|seg| tests.any_node_accepts(g, seg))
                    })
                })
            });
            pending |= !inert;
        }
        out
    }

    /// Whether the rule provably cannot change any of the form's segments: it is a modelled rewrite rule whose target accepts none of them.
    pub(super) fn inert_on_form(
        &self,
        g: &Grammar,
        item: &RuleItem,
        segments: Option<&[Seg]>,
    ) -> bool {
        let Some(segments) = segments else {
            return false;
        };
        self.targets.get(&item.prule).is_some_and(|tests| {
            tests
                .as_ref()
                .is_some_and(|tests| !segments.iter().any(|seg| tests.any_node_accepts(g, seg)))
        })
    }

    /// The texts of the segments that may stand before and after a morph.
    #[cfg(test)]
    pub(super) fn neighbour_texts(&self) -> (Vec<String>, Vec<String>) {
        let texts = |set: &[usize]| set.iter().map(|i| self.pool_text[*i].clone()).collect();
        (texts(&self.before), texts(&self.after))
    }

    /// The form's segments as isolated shapes, or `None` when one does not stand alone.
    pub(super) fn form_segments(&self, g: &Grammar, form: &AllomorphForm) -> Option<Vec<Seg>> {
        form.segments
            .iter()
            .map(|s| isolate(g, self.table, s))
            .collect()
    }

    /// The edge answer for one row, for each of `edges`.
    pub(super) fn answer(
        &mut self,
        g: &Grammar,
        row: &Row<'_>,
        edges: &[&'static str],
    ) -> EdgeAnswer {
        let Row {
            form,
            segments,
            item_index,
            item,
            verdict,
            fed,
        } = *row;
        let unknown = |reason: &'static str| EdgeAnswer {
            edges: edges
                .iter()
                .map(|e| (*e, Status::Unknown, reason))
                .collect(),
            not_inside: None,
        };
        if let Some(reason) = self.global {
            return unknown(reason);
        }
        if item.unlisted.is_some() {
            return unknown("rule_not_compiled");
        }
        match verdict {
            LhsVerdict::NotOnForm => {
                return EdgeAnswer {
                    edges: edges
                        .iter()
                        .map(|e| (*e, Status::NotAtEdge, super::NO_LHS_MATCH))
                        .collect(),
                    not_inside: Some(true),
                };
            }
            LhsVerdict::Unknown(
                reason @ ("stratum_precedes_morpheme"
                | "empty_lhs_epenthesis"
                | "lhs_match_budget"
                | "form_not_segmentable_in_stratum"
                | "form_partly_unmodelled"
                | "may_be_fed"),
            ) => return unknown(reason),
            LhsVerdict::Listed(_) | LhsVerdict::Deadline => return unknown("rule_not_compiled"),
            _ => {}
        }
        if let Some(reason) = fed {
            return unknown(reason);
        }
        let PhonRuleDef::Rewrite(rule) = &g.prules[item.prule] else {
            return unknown("metathesis");
        };
        if item.text.as_ref().is_none_or(|t| !t.non_local.is_empty()) {
            return unknown("non_local");
        }
        if !usable(form) {
            return unknown("form_partly_unmodelled");
        }
        let Some(form_segs) = segments else {
            return unknown("form_not_segmentable_in_stratum");
        };
        if form_segs.is_empty() {
            return unknown("null_form");
        }
        if self.unstable[item_index] {
            return unknown("neighbour_may_change");
        }
        let table = self.table;
        let compiled = self
            .compiled
            .entry((item.prule, item.sub_index))
            .or_insert_with(|| {
                let sub = rule.subrules.get(item.sub_index);
                let flat = flatten(
                    g,
                    table,
                    sub.and_then(|s| s.left_env.as_ref()),
                    &rule.lhs,
                    sub.and_then(|s| s.right_env.as_ref()),
                )?;
                let tests = flat.compile(g, table)?;
                Ok((flat, tests))
            });
        let (flat, tests) = match compiled {
            Ok(pair) => pair,
            Err(reason) => return unknown(reason),
        };
        let before: Vec<&Seg> = self.before.iter().map(|i| &self.pool[*i]).collect();
        let after: Vec<&Seg> = self.after.iter().map(|i| &self.pool[*i]).collect();
        let anywhere: Vec<&Seg> = self.pool.iter().collect();
        let found = placements(
            g,
            flat,
            tests,
            form_segs,
            &Neighbours {
                before: &before,
                after: &after,
                anywhere: &anywhere,
            },
        );
        EdgeAnswer {
            edges: edges
                .iter()
                .map(|edge| {
                    let possible = if *edge == "left" {
                        found.left
                    } else {
                        found.right
                    };
                    if possible {
                        (*edge, Status::NotRuledOut, SATISFIABLE)
                    } else {
                        (*edge, Status::NotAtEdge, NO_NEIGHBOUR)
                    }
                })
                .collect(),
            not_inside: Some(!found.inside),
        }
    }
}
