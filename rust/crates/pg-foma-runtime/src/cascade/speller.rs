//! `propose_corrections` over a layer stack (plan S.1, S.5, S.6, S.8): limits, the `Best(n)`
//! procedure, groups and variants, three-valued membership, the confirm loop with one budget per
//! group, and `confirm_complete`.
//!
//! The layers are whatever the caller built (`Lex .. Repr | Units | Fold | Err1 | Err2` once WS1 is
//! in); this module sees them through [`SpellModel`] and the product's labels only. No
//! probability ranking happens here (S.12 is WR1): `g` is the error weight and nothing else.

use std::collections::{BTreeSet, HashSet};

use pg_grammar::model::{LexEntryId, MRuleId};

use super::budget::{CascadeStop, WordBudget, WordMeter};
use super::layer::{BadTag, Layer, Tag, TagKind, Token};
use super::product::{
    Control, Observe, Outcome, PrepassPlan, Product, ProductError, ProductScratch, Report, Request,
    Run,
};
use super::spell_read::{
    string_info, Charge, Cost, GroupCore, GroupIter, Label, Reader, SpellModel, VariantCore,
    VariantIter,
};
use super::trim::TrimmedGraph;

/// `weight` is fixed-point thousandths everywhere.
pub type Weight = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    All,
    Best(usize),
}

#[derive(Clone, Debug)]
pub struct SpellOptions {
    pub max_edits: u8,
    pub weight_ceiling: Option<u32>,
    pub mode: Mode,
    pub confirm_top: usize,
    pub stack_cap: usize,
    pub casing_cap: usize,
    /// Strings to test for membership inside the request.
    pub probe: Vec<String>,
}

impl SpellOptions {
    pub fn new(max_edits: u8, mode: Mode) -> Self {
        SpellOptions {
            max_edits,
            weight_ceiling: None,
            mode,
            confirm_top: 0,
            stack_cap: 4,
            casing_cap: 4,
            probe: Vec::new(),
        }
    }
}

/// Every limit of S.8 that is not a product budget, plus the product budget itself.
#[derive(Clone, Debug)]
pub struct SpellLimits {
    pub word: WordBudget,
    pub prepass: PrepassPlan,
    /// Groups plus listed variants (`Strings`).
    pub strings: u64,
    /// The work one group's parses may spend together (`WordBudget::new(confirm_group_work)`).
    pub confirm_group_work: u64,
    /// `F_c`: groups tried per wanted suggestion (Q10).
    pub f_c: u64,
    /// Total confirm work for the word; `None` = `F_c * confirm_top * 17/16 * confirm_group_work`.
    pub confirm_word_work: Option<u64>,
    /// Groups tried at most; `None` = `F_c * confirm_top`.
    pub groups_tried: Option<u64>,
}

impl SpellLimits {
    pub const STRINGS: u64 = 65_536;

    pub fn for_request(max_edits: u8, confirm_group_work: u64) -> Self {
        SpellLimits {
            word: WordBudget::for_edits(max_edits),
            prepass: PrepassPlan::none(),
            strings: Self::STRINGS,
            confirm_group_work,
            f_c: 4,
            confirm_word_work: None,
            groups_tried: None,
        }
    }

    fn word_work(&self, top: usize) -> u64 {
        self.confirm_word_work.unwrap_or_else(|| {
            (self.f_c * top as u64)
                .saturating_mul(17)
                .saturating_div(16)
                .saturating_mul(self.confirm_group_work)
        })
    }

    fn groups(&self, top: usize) -> u64 {
        self.groups_tried.unwrap_or(self.f_c * top as u64)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmLimit {
    WordWork,
    GroupsTried,
}

/// The dimension that ended a request as `Incomplete`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dimension {
    ProductStates,
    ProductArcs,
    ProductSteps,
    ProductBytes,
    Strings,
    Confirmation(ConfirmLimit),
}

impl From<CascadeStop> for Dimension {
    fn from(s: CascadeStop) -> Self {
        match s {
            CascadeStop::ProductStates => Dimension::ProductStates,
            CascadeStop::ProductArcs => Dimension::ProductArcs,
            CascadeStop::ProductSteps => Dimension::ProductSteps,
            CascadeStop::ProductBytes => Dimension::ProductBytes,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Membership {
    Yes(Features),
    No,
    Unknown { reason: UnknownReason },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownReason {
    /// The graph is not complete.
    GraphIncomplete,
    /// `Best(n)` graph: the string is not below the stop weight.
    AtOrAboveStopWeight,
    /// The probe has a character the cascade has no token for, on a graph that is not complete.
    NoSuchCharacter,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Features {
    pub roots: HashSet<LexEntryId>,
    pub rules: HashSet<MRuleId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HcLabel {
    Unchecked,
    Confirmed(Vec<(String, String)>),
    Rejected,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupConfirm {
    NotTried,
    Confirmed,
    AllRejected,
    Open,
}

#[derive(Clone, Debug)]
pub struct Variant {
    pub surface: String,
    pub edits: u8,
    pub weight: Weight,
    pub stack_distance: u32,
    pub case_distance: u32,
    pub features: Features,
    /// The listed features come from an explored part only; confirm by a plain parse.
    pub features_partial: bool,
    pub hc: HcLabel,
    tokens: Vec<Token>,
}

#[derive(Clone, Debug)]
pub struct Group {
    pub key: Vec<Token>,
    pub edits: u8,
    pub weight: Weight,
    pub variants: Vec<Variant>,
    pub more_variants: bool,
    pub confirm: GroupConfirm,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub states: u64,
    pub arcs: u64,
    pub steps: u64,
    pub bytes: u64,
    pub groups: u64,
    pub variants_listed: u64,
    pub parses: u64,
    pub parses_stopped: u64,
    pub confirm_work: u64,
    /// `Best(n)`: how many times the queue was snapshotted.
    pub checkpoints: u64,
}

#[derive(Clone, Debug)]
pub enum SpellOutcome {
    Complete {
        max_edits: u8,
        weight_ceiling: Option<u32>,
        groups: Vec<Group>,
        probes: Vec<(String, Membership)>,
        /// True iff a confirm loop ran to its end and no tried group is `Open` and no variant is
        /// `Stopped`. `Complete` alone never says that (S.1; review r10 F10-P3-3): a `Complete`
        /// list can hold `Open`, `Stopped` and `NotTried` labels. False when nothing was confirmed.
        confirm_complete: bool,
        counters: Counters,
    },
    Incomplete {
        dimension: Dimension,
        value: u64,
        limit: u64,
        graph_complete: bool,
        partial: Vec<Group>,
        probes: Vec<(String, Membership)>,
        counters: Counters,
    },
    Refused {
        construct: String,
        rule: String,
    },
}

// ---------------------------------------------------------------- confirmation

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseResult {
    Confirmed(Vec<(String, String)>),
    Rejected,
    Stopped,
}

/// The one confirm entry the loop needs; `HcConfirmer` implements it over `confirm_pin_groups`.
pub trait Confirmer {
    /// A fresh allowance for one group (`WordBudget::new(confirm_group_work)`).
    fn begin_group(&mut self);
    /// One restricted parse of the variant's exact surface under the group's budget.
    fn parse(&mut self, surface: &str, features: &Features) -> ParseResult;
    /// Work the current group's allowance has spent.
    fn group_used(&self) -> u64;
}

/// The confirm loop (S.5): groups in list order, one budget per group, a stopped variant ends its
/// group and not the word. Returns the limit that ended the loop, if one did.
pub fn confirm_loop(
    groups: &mut [Group],
    confirmer: &mut dyn Confirmer,
    top: usize,
    limits: &SpellLimits,
    counters: &mut Counters,
) -> Option<(ConfirmLimit, u64, u64)> {
    let work_limit = limits.word_work(top);
    let tried_limit = limits.groups(top);
    let (mut confirmed, mut tried, mut work) = (0usize, 0u64, 0u64);
    for g in groups.iter_mut() {
        if confirmed >= top {
            break;
        }
        if tried >= tried_limit {
            return Some((ConfirmLimit::GroupsTried, tried, tried_limit));
        }
        tried += 1;
        confirmer.begin_group();
        let mut stopped = false;
        for v in g.variants.iter_mut() {
            if stopped {
                break;
            }
            counters.parses += 1;
            match confirmer.parse(&v.surface, &v.features) {
                ParseResult::Confirmed(a) => v.hc = HcLabel::Confirmed(a),
                ParseResult::Rejected => v.hc = HcLabel::Rejected,
                ParseResult::Stopped => {
                    v.hc = HcLabel::Stopped;
                    counters.parses_stopped += 1;
                    stopped = true;
                }
            }
        }
        work += confirmer.group_used();
        counters.confirm_work = work;
        g.confirm = group_label(g);
        if g.confirm == GroupConfirm::Confirmed {
            confirmed += 1;
        }
        if work > work_limit {
            return Some((ConfirmLimit::WordWork, work, work_limit));
        }
    }
    None
}

/// A group is `Confirmed` if a variant is; `AllRejected` only when no variant is unlisted and
/// every one was rejected (never while one is unchecked: C142); otherwise `Open`.
pub fn group_label(g: &Group) -> GroupConfirm {
    if g.variants
        .iter()
        .all(|v| matches!(v.hc, HcLabel::Unchecked))
    {
        return GroupConfirm::NotTried;
    }
    if g.variants
        .iter()
        .any(|v| matches!(v.hc, HcLabel::Confirmed(_)))
    {
        GroupConfirm::Confirmed
    } else if !g.more_variants && g.variants.iter().all(|v| v.hc == HcLabel::Rejected) {
        GroupConfirm::AllRejected
    } else {
        GroupConfirm::Open
    }
}

fn confirm_complete(groups: &[Group], ran: bool, limit_hit: bool) -> bool {
    ran && !limit_hit
        && !groups.iter().any(|g| {
            g.confirm == GroupConfirm::Open || g.variants.iter().any(|v| v.hc == HcLabel::Stopped)
        })
}

// ---------------------------------------------------------------- the speller

/// A cascade stack plus the model that reads its labels.
pub struct Speller<'a> {
    pub layers: &'a [&'a dyn Layer],
    pub model: &'a dyn SpellModel,
    pub limits: SpellLimits,
}

fn features_of(tags: &BTreeSet<Tag>) -> Result<Features, BadTag> {
    let mut f = Features::default();
    for t in tags {
        match t.kind()? {
            TagKind::Root(i) => {
                f.roots.insert(LexEntryId(i));
            }
            TagKind::Rule(i) => {
                f.rules.insert(MRuleId(i));
            }
            TagKind::NonHead(_) => {}
        }
    }
    Ok(f)
}

/// What the graph of a request is good for.
#[derive(Clone, Copy, Debug)]
struct GraphKind {
    complete: bool,
    /// Weights below this are whole; `None` = every weight.
    stop: Option<u64>,
}

enum Internal {
    Stop(CascadeStop),
    Bad(BadTag),
}

impl From<CascadeStop> for Internal {
    fn from(s: CascadeStop) -> Self {
        Internal::Stop(s)
    }
}

impl From<BadTag> for Internal {
    fn from(b: BadTag) -> Self {
        Internal::Bad(b)
    }
}

impl Speller<'_> {
    /// One request. `confirm` is the HC entry; `None` skips confirmation.
    pub fn propose(
        &self,
        word: &[Token],
        opts: &SpellOptions,
        confirm: Option<&mut dyn Confirmer>,
    ) -> SpellOutcome {
        if opts.max_edits > 2 {
            return SpellOutcome::Refused {
                construct: format!("max_edits {}", opts.max_edits),
                rule: "the edit limit is 0, 1 or 2".into(),
            };
        }
        let product = match Product::new(self.layers) {
            Ok(p) => p,
            Err(e) => {
                return SpellOutcome::Refused {
                    construct: "layer stack".into(),
                    rule: match e {
                        ProductError::NoLayers => "no layers".into(),
                        ProductError::UniverseMismatch { layer, .. } => {
                            format!("layer {layer} has another alphabet")
                        }
                    },
                }
            }
        };
        let model = self.model;
        let observe = move |mv: &super::product::MoveView| model.observe(mv).pack();
        let mut req = Request::new(word, opts.max_edits);
        req.theta = opts.weight_ceiling;
        req.budget = self.limits.word;
        req.prepass = self.limits.prepass.clone();
        req.observe = Some(Observe(&observe));
        let mut scratch = ProductScratch::new(product.layer_count());
        let mut counters = Counters::default();
        let unknown = |reason| {
            opts.probe
                .iter()
                .map(|p| (p.clone(), Membership::Unknown { reason }))
                .collect::<Vec<_>>()
        };
        let incomplete = |stop: CascadeStop, report: &Report, counters: &mut Counters| {
            fill(counters, report);
            let (value, limit) = dimension_figures(stop, report, &self.limits.word);
            SpellOutcome::Incomplete {
                dimension: stop.into(),
                value,
                limit,
                graph_complete: false,
                partial: Vec::new(),
                probes: unknown(UnknownReason::GraphIncomplete),
                counters: counters.clone(),
            }
        };

        let mut checkpoints = 0u64;
        let (graph, kind, report) = match opts.mode {
            Mode::All => match product.run(&mut scratch, &req) {
                Outcome::Complete { graph, report } => (
                    graph,
                    GraphKind {
                        complete: true,
                        stop: None,
                    },
                    report,
                ),
                Outcome::Incomplete { stop, report } => {
                    return incomplete(stop, &report, &mut counters)
                }
            },
            Mode::Best(n) => {
                let mut found: Option<(TrimmedGraph, u64)> = None;
                let mut hook =
                    |cp: &mut super::product::Checkpoint<'_>| -> Result<Control, CascadeStop> {
                        checkpoints += 1;
                        let f_min = cp.f_min;
                        let (g, bytes) = cp.snapshot()?;
                        let complete = {
                            let mut ch = |c: Cost| match c {
                                Cost::Steps(n) => cp.charge_steps(n),
                                Cost::Bytes(n) => cp.charge_bytes(n),
                            };
                            let r = Reader::new(&g, &mut ch)?;
                            let mut it = GroupIter::new(&r);
                            let mut complete = 0usize;
                            while complete < n {
                                match it.next_group(&mut ch)? {
                                    Some(c) if c.weight < f_min => complete += 1,
                                    _ => break,
                                }
                            }
                            complete
                        };
                        if complete >= n {
                            found = Some((g, f_min));
                            return Ok(Control::Stop);
                        }
                        cp.release(bytes);
                        Ok(Control::Continue)
                    };
                match product.run_hooked(&mut scratch, &req, Some(&mut hook)) {
                    Run::Finished(Outcome::Complete { graph, report }) => (
                        graph,
                        GraphKind {
                            complete: true,
                            stop: None,
                        },
                        report,
                    ),
                    Run::Finished(Outcome::Incomplete { stop, report }) => {
                        return incomplete(stop, &report, &mut counters)
                    }
                    Run::Stopped { f_min, report } => {
                        let (graph, _) = found.expect("the hook keeps the snapshot it stops on");
                        (
                            graph,
                            GraphKind {
                                complete: false,
                                stop: Some(f_min),
                            },
                            report,
                        )
                    }
                }
            }
        };
        fill(&mut counters, &report);
        counters.checkpoints = checkpoints;

        // the read-out is charged to the product's counters, against the same limits
        let mut meter = WordMeter::new(self.limits.word);
        meter.states = report.states;
        meter.arcs = report.arcs;
        meter.steps = report.steps;
        meter.bytes = report.bytes;
        let mut charge = |c: Cost| match c {
            Cost::Steps(n) => meter.charge_steps(n),
            Cost::Bytes(n) => meter.charge_bytes(n),
        };
        let mut groups: Vec<Group> = Vec::new();
        let mut probes: Vec<(String, Membership)> = Vec::new();
        let read = self.read_out(&graph, kind, opts, &mut charge, &mut groups, &mut probes);
        counters.steps = meter.steps;
        counters.bytes = meter.bytes;
        counters.groups = groups.len() as u64;
        counters.variants_listed = groups.iter().map(|g| g.variants.len() as u64).sum();
        match read {
            Ok(()) => {}
            Err(Internal::Stop(s)) => {
                return SpellOutcome::Incomplete {
                    dimension: s.into(),
                    value: meter.steps,
                    limit: self.limits.word.steps,
                    graph_complete: kind.complete,
                    partial: groups,
                    probes: unknown(UnknownReason::GraphIncomplete),
                    counters,
                }
            }
            Err(Internal::Bad(b)) => {
                return SpellOutcome::Refused {
                    construct: "unknown tag word".into(),
                    rule: format!("{b:?}"),
                }
            }
        }
        if counters.groups + counters.variants_listed > self.limits.strings {
            return SpellOutcome::Incomplete {
                dimension: Dimension::Strings,
                value: counters.groups + counters.variants_listed,
                limit: self.limits.strings,
                graph_complete: kind.complete,
                partial: groups,
                probes,
                counters,
            };
        }
        let mut ran = false;
        let mut limit_hit = None;
        if let Some(c) = confirm {
            if opts.confirm_top > 0 {
                ran = true;
                limit_hit = confirm_loop(
                    &mut groups,
                    c,
                    opts.confirm_top,
                    &self.limits,
                    &mut counters,
                );
            }
        }
        if let Some((which, value, limit)) = limit_hit {
            return SpellOutcome::Incomplete {
                dimension: Dimension::Confirmation(which),
                value,
                limit,
                graph_complete: kind.complete,
                partial: groups,
                probes,
                counters,
            };
        }
        let cc = confirm_complete(&groups, ran, false);
        SpellOutcome::Complete {
            max_edits: opts.max_edits,
            weight_ceiling: opts.weight_ceiling,
            groups,
            probes,
            confirm_complete: cc,
            counters,
        }
    }

    fn read_out(
        &self,
        graph: &TrimmedGraph,
        kind: GraphKind,
        opts: &SpellOptions,
        charge: Charge,
        groups: &mut Vec<Group>,
        probes: &mut Vec<(String, Membership)>,
    ) -> Result<(), Internal> {
        let reader = Reader::new(graph, charge)?;
        let limit = match opts.mode {
            Mode::All => usize::MAX,
            Mode::Best(n) => n,
        };
        let mut it = GroupIter::new(&reader);
        let mut listed = 0u64;
        while groups.len() < limit {
            let Some(core) = it.next_group(charge)? else {
                break;
            };
            if kind.stop.is_some_and(|s| core.weight >= s) {
                break;
            }
            let g = self.group(&reader, core, kind, opts, charge)?;
            listed += 1 + g.variants.len() as u64;
            groups.push(g);
            if listed > self.limits.strings {
                // over the cap: the caller reports `Strings` with this partial list
                return Ok(());
            }
        }
        for p in &opts.probe {
            probes.push((p.clone(), self.probe(&reader, p, kind, charge)?));
        }
        Ok(())
    }

    fn group(
        &self,
        reader: &Reader,
        core: GroupCore,
        kind: GraphKind,
        opts: &SpellOptions,
        charge: Charge,
    ) -> Result<Group, Internal> {
        let mut it = VariantIter::new(reader, self.model, &core.key, kind.stop);
        let mut variants = Vec::new();
        let mut assignments: Vec<(Vec<Vec<Token>>, usize)> = Vec::new();
        let mut more = self.model.stack_loop();
        let mut rejected = 0u64;
        while let Some(v) = it.next_variant(charge)? {
            let slot = assignments.iter().position(|(a, _)| *a == v.stacks);
            let ok = match slot {
                Some(i) => assignments[i].1 < opts.casing_cap,
                None => assignments.len() < opts.stack_cap,
            };
            if ok {
                match slot {
                    Some(i) => assignments[i].1 += 1,
                    None => assignments.push((v.stacks.clone(), 1)),
                }
                variants.push(self.variant(&v)?);
            } else {
                more = true;
                rejected += 1;
                let full = assignments.len() >= opts.stack_cap
                    && assignments.iter().all(|(_, c)| *c >= opts.casing_cap);
                let scan = (4 * opts.stack_cap * opts.casing_cap).max(64) as u64;
                if full || rejected >= scan.min(self.limits.strings) {
                    break;
                }
            }
        }
        more |= it.beyond_limit;
        Ok(Group {
            key: core.key,
            edits: core.edits,
            weight: core.weight,
            variants,
            more_variants: more,
            confirm: GroupConfirm::NotTried,
        })
    }

    fn variant(&self, v: &VariantCore) -> Result<Variant, BadTag> {
        Ok(Variant {
            surface: v.chars.iter().collect(),
            edits: v.edits,
            weight: v.weight,
            stack_distance: v.stack_distance,
            case_distance: v.case_distance,
            features: features_of(&v.tags)?,
            features_partial: false,
            hc: HcLabel::Unchecked,
            tokens: v.tokens.clone(),
        })
    }

    fn probe(
        &self,
        reader: &Reader,
        text: &str,
        kind: GraphKind,
        charge: Charge,
    ) -> Result<Membership, Internal> {
        let toks: Option<Vec<Token>> = text.chars().map(|c| self.model.written_token(c)).collect();
        let unknown = |reason| Ok(Membership::Unknown { reason });
        let info = match &toks {
            None => None,
            Some(t) => string_info(reader, t, charge)?,
        };
        match (info, kind.complete) {
            (Some(i), _) if kind.stop.is_none_or(|s| i.weight < s) => {
                Ok(Membership::Yes(features_of(&i.tags)?))
            }
            (Some(_), _) => unknown(UnknownReason::AtOrAboveStopWeight),
            (None, true) => Ok(Membership::No),
            (None, false) if toks.is_none() => unknown(UnknownReason::NoSuchCharacter),
            (None, false) => unknown(UnknownReason::AtOrAboveStopWeight),
        }
    }
}

fn fill(c: &mut Counters, r: &Report) {
    c.states = r.states;
    c.arcs = r.arcs;
    c.steps = r.steps;
    c.bytes = r.bytes;
}

fn dimension_figures(stop: CascadeStop, r: &Report, b: &WordBudget) -> (u64, u64) {
    match stop {
        CascadeStop::ProductStates => (r.states, b.states),
        CascadeStop::ProductArcs => (r.arcs, b.arcs),
        CascadeStop::ProductSteps => (r.steps, b.steps),
        CascadeStop::ProductBytes => (r.bytes, b.bytes),
    }
}

impl Variant {
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }
}

impl Label {
    /// The label of a move with nothing observed.
    pub const NONE: Label = Label {
        written: None,
        folded: None,
        case: false,
    };
}
