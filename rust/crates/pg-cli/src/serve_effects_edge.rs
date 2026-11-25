//! The edge environment test of serve op `effects`: whether a rule's environment can be met across a morph's left or right edge by any segment the grammar lets stand there.
//! Every unproven route is an `unknown` reason and never a ruling out; see `docs/formats/serve-protocol.md`.

use std::collections::{BTreeSet, HashMap, HashSet};

use pg_grammar::model::{Grammar, MorphRuleDef, OutputAction, PhonRuleDef, TableId};
use pg_rules::edge_env::{
    flatten, flatten_all, inventory, isolate, placements, written_at, FlatPattern, NodeTests,
    Placements,
};
use pg_rules::edge_env::{Neighbours, Seg, Shape};

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
    pub item: &'a RuleItem,
    /// The row's left-hand-side verdict.
    pub verdict: &'a LhsVerdict,
    /// Some affix modifies stem material before the phonology runs.
    pub modifies_stem: bool,
}

type Compiled = Result<Vec<(FlatPattern, NodeTests)>, &'static str>;

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
    compiled: HashMap<(usize, usize), Compiled>,
    morpheme_stratum: usize,
    /// The distinct phonological rules in application order.
    steps: Vec<Step>,
    step_of: HashMap<usize, usize>,
    /// What each neighbour set holds before each step: before, after, anywhere; `None` once a step may do more than rewrite one segment to another.
    neighbour_sets: Vec<[Option<Vec<Seg>>; 3]>,
    written_memo: HashMap<(usize, usize, Vec<Shape>), Option<Vec<Seg>>>,
    /// The segments one form segment may hold before a step.
    form_memo: HashMap<(String, usize), Option<Vec<Seg>>>,
}

/// One phonological rule as the edge test sees it.
struct Step {
    prule: usize,
    stratum: usize,
    /// The target as node tests; `None` when the rule is not a modelled rewrite rule.
    tests: Option<NodeTests>,
}

/// Whether the form's edges are the edges of one contiguous run, with the stem (if any) wholly on one side.
pub(super) fn usable(form: &AllomorphForm) -> bool {
    !form.opaque
        && form.runs.len() <= 1
        && form.lhs_parts <= 1
        && !(form.copy_before && form.copy_after)
}

impl EdgeContext {
    pub(super) fn new(g: &Grammar, items: &[RuleItem], morpheme_stratum: usize) -> EdgeContext {
        let table = g.strata.first().map_or(TableId(0), |s| s.table);
        let mut ctx = EdgeContext {
            table,
            global: None,
            pool: Vec::new(),
            #[cfg(test)]
            pool_text: Vec::new(),
            before: Vec::new(),
            after: Vec::new(),
            compiled: HashMap::new(),
            morpheme_stratum,
            steps: Vec::new(),
            step_of: HashMap::new(),
            neighbour_sets: Vec::new(),
            written_memo: HashMap::new(),
            form_memo: HashMap::new(),
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
        let guess = g.guess_roots;
        let (mut before_open, mut after_open) = (guess, guess);
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
            if ctx.step_of.contains_key(&item.prule) {
                continue;
            }
            let modelled = match &g.prules[item.prule] {
                PhonRuleDef::Rewrite(rule) => flatten(g, table, None, &rule.lhs, None)
                    .ok()
                    .and_then(|flat| flat.compile(g, table).ok()),
                _ => None,
            };
            ctx.step_of.insert(item.prule, ctx.steps.len());
            ctx.steps.push(Step {
                prule: item.prule,
                stratum: item.stratum,
                tests: modelled,
            });
        }
        let pick = |ctx: &EdgeContext, ids: &[usize]| -> Vec<Seg> {
            ids.iter().map(|i| ctx.pool[*i].clone()).collect()
        };
        let mut sets = [
            Some(pick(&ctx, &ctx.before)),
            Some(pick(&ctx, &ctx.after)),
            Some(ctx.pool.clone()),
        ];
        for j in 0..ctx.steps.len() {
            ctx.neighbour_sets.push(sets.clone());
            for set in &mut sets {
                *set = set.take().and_then(|s| ctx.advance(g, j, &s, false));
            }
        }
        ctx.neighbour_sets.push(sets);
        ctx
    }

    /// What `step` leaves of `set`: each segment a target node accepts also may become what the rule writes there, with the rest of the target filled by any segment that can stand there (environments ignored); `None` when the rule writes a different length or is not modelled.
    fn advance(&mut self, g: &Grammar, step: usize, set: &[Seg], form: bool) -> Option<Vec<Seg>> {
        if form && self.steps[step].stratum < self.morpheme_stratum {
            return Some(set.to_vec());
        }
        let prule = self.steps[step].prule;
        let n = self.steps[step].tests.as_ref()?.len();
        let far: Vec<Seg> = self.neighbour_sets[step][2].clone()?;
        let mut out = set.to_vec();
        let mut seen: HashSet<Shape> = out.iter().map(|s| s.shape().clone()).collect();
        let mut frontier = set.to_vec();
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for seg in &frontier {
                for at in 0..n {
                    let tests = self.steps[step].tests.as_ref()?;
                    if !tests.accepts(g, at, seg) {
                        continue;
                    }
                    // The other target nodes may sit on any segment that fits them.
                    let fits: Vec<Vec<&Seg>> = (0..n)
                        .map(|node| {
                            if node == at {
                                vec![seg]
                            } else {
                                far.iter().filter(|s| tests.accepts(g, node, s)).collect()
                            }
                        })
                        .collect();
                    let total = fits
                        .iter()
                        .try_fold(1usize, |acc, f| acc.checked_mul(f.len()))?;
                    if total > 4096 {
                        return None;
                    }
                    let mut index = vec![0usize; n];
                    'combos: for _ in 0..total {
                        let window: Vec<&Seg> = (0..n).map(|k| fits[k][index[k]]).collect();
                        for new in self.written_by(g, prule, &window, at)? {
                            if seen.insert(new.shape().clone()) {
                                out.push(new.clone());
                                next.push(new);
                            }
                        }
                        for k in 0..n {
                            index[k] += 1;
                            if index[k] < fits[k].len() {
                                continue 'combos;
                            }
                            index[k] = 0;
                        }
                    }
                }
            }
            if out.len() > 2000 {
                return None;
            }
            frontier = next;
        }
        Some(out)
    }

    fn written_by(
        &mut self,
        g: &Grammar,
        prule: usize,
        window: &[&Seg],
        at: usize,
    ) -> Option<Vec<Seg>> {
        let key = (
            prule,
            at,
            window.iter().map(|s| s.shape().clone()).collect::<Vec<_>>(),
        );
        if let Some(hit) = self.written_memo.get(&key) {
            return hit.clone();
        }
        let PhonRuleDef::Rewrite(rule) = &g.prules[prule] else {
            return None;
        };
        let out = written_at(g, self.table, rule, window, at);
        self.written_memo.insert(key, out.clone());
        out
    }

    /// What a form segment may hold before `step`, after the earlier rules of the morpheme's strata.
    fn form_closure(&mut self, g: &Grammar, text: &str, step: usize) -> Option<Vec<Seg>> {
        let key = (text.to_string(), step);
        if let Some(hit) = self.form_memo.get(&key) {
            return hit.clone();
        }
        let before = if step == 0 {
            isolate(g, self.table, text).map(|s| vec![s])
        } else {
            let prior = self.form_closure(g, text, step - 1)?;
            self.advance(g, step - 1, &prior, true)
        };
        self.form_memo.insert(key, before.clone());
        before
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
            item,
            verdict,
            modifies_stem,
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
                | "form_partly_unmodelled"),
            ) => return unknown(reason),
            LhsVerdict::Listed(_) | LhsVerdict::Deadline => return unknown("rule_not_compiled"),
            _ => {}
        }
        if modifies_stem {
            return unknown("affix_modifies_stem");
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
        let step = self.step_of[&item.prule];
        let mut variants: Vec<Vec<Seg>> = Vec::with_capacity(form.segments.len());
        for text in &form.segments {
            match self.form_closure(g, text, step) {
                Some(v) => variants.push(v),
                None => return unknown("may_be_fed"),
            }
        }
        let [Some(before_set), Some(after_set), Some(far_set)] = self.neighbour_sets[step].clone()
        else {
            return unknown("neighbour_may_change");
        };
        let table = self.table;
        let compiled = self
            .compiled
            .entry((item.prule, item.sub_index))
            .or_insert_with(|| {
                let sub = rule.subrules.get(item.sub_index);
                flatten_all(
                    g,
                    table,
                    sub.and_then(|s| s.left_env.as_ref()),
                    &rule.lhs,
                    sub.and_then(|s| s.right_env.as_ref()),
                )?
                .into_iter()
                .map(|flat| flat.compile(g, table).map(|tests| (flat, tests)))
                .collect()
            });
        let patterns = match compiled {
            Ok(patterns) => patterns,
            Err(reason) => return unknown(reason),
        };
        let before: Vec<&Seg> = before_set.iter().collect();
        let after: Vec<&Seg> = after_set.iter().collect();
        let anywhere: Vec<&Seg> = far_set.iter().collect();
        let form_variants: Vec<Vec<&Seg>> = variants.iter().map(|v| v.iter().collect()).collect();
        let mut found = Placements {
            inside: false,
            left: false,
            right: false,
        };
        for (flat, tests) in patterns.iter() {
            let one = placements(
                g,
                flat,
                tests,
                &form_variants,
                &Neighbours {
                    before: &before,
                    after: &after,
                    anywhere: &anywhere,
                },
            );
            found.inside |= one.inside;
            found.left |= one.left;
            found.right |= one.right;
        }
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
