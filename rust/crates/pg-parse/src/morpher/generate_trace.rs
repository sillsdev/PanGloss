//! Bounded generation keeping each seed's allomorph choices and rule trace, for `pangloss serve`'s `generate` op.

use std::collections::BTreeMap;
use std::time::Duration;

use pg_featstruct::FeatureStruct;
use pg_grammar_model::model::{
    AllomorphId, AllomorphOwner, Grammar, LexEntryId, MRuleId, MorphRuleDef, PRuleId, PhonRuleDef,
    StratumId, TemplateId,
};
use pg_rules::trace::{NoopSink, TraceHandle, TraceSink, TraceSource, TraceType, TreeTraceSink};
use pg_rules::word::Word;

use super::{
    generation_seed_product, visit_permute_rules, FinalTemplateSynthesisPolicy, GenMorpheme,
    Morpher, PermItem,
};
use crate::surface;

/// The caller's bounds on one traced generation; every field is explicit so a serve process's parse cap can never become the generation cap by accident.
#[derive(Debug, Clone, Copy)]
pub struct TracedGenerateLimits {
    /// Work units shared by seed selection and synthesis, as `Morpher::with_generation_cap`.
    pub work_limit: usize,
    /// Wall-clock deadline for the whole call; `None` is unbounded.
    pub timeout: Option<Duration>,
    /// Most seeds that may be synthesized.
    pub max_seeds: usize,
    /// Most trace nodes retained across every seed; `None` records no trace.
    pub max_trace_nodes: Option<usize>,
}

/// Why a traced generation returned before every seed was examined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracedStopReason {
    SeedProductExceedsBudget { limit: usize },
    WorkLimit { limit: usize },
    Deadline,
    TraceLimit { limit: usize },
    SeedLimit { limit: usize },
}

impl TracedStopReason {
    /// The wire name `pangloss serve` reports.
    pub fn code(self) -> &'static str {
        match self {
            Self::SeedProductExceedsBudget { .. } => "seed_product_exceeds_budget",
            Self::WorkLimit { .. } => "work_limit",
            Self::Deadline => "deadline",
            Self::TraceLimit { .. } => "trace_limit",
            Self::SeedLimit { .. } => "seed_limit",
        }
    }
}

/// Which part of a seed an allomorph record describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedItemRole {
    Root,
    /// A compounding non-head root entry, one of whose allomorphs the seed picked.
    NonHead,
    /// An affix or realizational rule; it has no allomorph choice.
    Rule,
}

impl SeedItemRole {
    pub fn code(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::NonHead => "nonHead",
            Self::Rule => "rule",
        }
    }
}

/// One morpheme of a seed and the allomorph the seed chose for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedItem {
    pub role: SeedItemRole,
    /// Authored lexical entry GUID, when the grammar carries one.
    pub entry_guid: Option<String>,
    /// Authored MSA GUID, when the grammar carries one.
    pub msa_guid: Option<String>,
    /// The compiled allomorph id; `None` for a rule.
    pub allomorph_id: Option<u32>,
    /// Authored MoForm GUID of the chosen allomorph, when known.
    pub form_guid: Option<String>,
    /// The chosen allomorph's lexical spelling; `None` for a rule.
    pub shape: Option<String>,
    /// The rule's compiled name; `None` for an entry.
    pub rule_name: Option<String>,
}

/// The rule behind one trace step.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StepRule {
    /// `phon`, `morph` or `template`.
    pub kind: &'static str,
    /// The compiled rule's grammar identifier.
    pub id: String,
    pub name: Option<String>,
    /// The authored FieldWorks GUID, when the grammar carries one.
    pub guid: Option<String>,
}

/// One rule application on the path from a seed to a surface.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GenerateStep {
    pub stratum: String,
    pub rule: StepRule,
    /// The word before the rule, boundaries shown as `+`.
    pub input: String,
    /// The word after the rule.
    pub output: String,
    /// Which subrule fired, when the rule has subrules.
    pub subrule: Option<i32>,
}

/// One surface a seed generated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedSurface {
    pub surface: String,
    /// The derivation, outermost application first; empty when no trace was requested.
    pub steps: Vec<GenerateStep>,
    /// How many distinct derivations of this seed reached the surface; `steps` is the least.
    pub derivations: usize,
}

/// One seed (a root allomorph plus one selection of the others) and what it generated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedSeed {
    /// `r<root allomorph index>/p<selection index>`, in the order the seeds were visited.
    pub seed_key: String,
    pub items: Vec<SeedItem>,
    /// Sorted by surface.
    pub surfaces: Vec<SeedSurface>,
    /// The trace cap cut this seed's trace short, so its steps may be missing.
    pub trace_truncated: bool,
}

/// Everything a traced generation found, and whether it examined every seed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TracedGenerateOutcome {
    pub seeds: Vec<GeneratedSeed>,
    pub stop_reason: Option<TracedStopReason>,
    pub work_units: usize,
    /// Seeds synthesized, whether or not they produced a surface.
    pub seed_choices: usize,
    pub trace_nodes: usize,
}

impl<'g> Morpher<'g> {
    /// `Self::generate_words_bounded` kept per seed; a stop returns the seeds finished so far with a typed reason.
    pub fn generate_words_traced(
        &self,
        root: LexEntryId,
        others: &[GenMorpheme],
        real_fs: FeatureStruct,
        limits: &TracedGenerateLimits,
    ) -> TracedGenerateOutcome {
        let g = self.g;
        let work_limit = limits.work_limit;
        let mut outcome = TracedGenerateOutcome {
            seeds: Vec::new(),
            stop_reason: None,
            work_units: 0,
            seed_choices: 0,
            trace_nodes: 0,
        };
        let seed_product = match generation_seed_product(g, root, others) {
            Some(product) if product <= work_limit => product,
            Some(_) | None => {
                outcome.stop_reason =
                    Some(TracedStopReason::SeedProductExceedsBudget { limit: work_limit });
                return outcome;
            }
        };
        if seed_product == 0 {
            return outcome;
        }

        let budget =
            pg_rules::stratum::WordBudget::without_reserve(work_limit).with_timeout(limits.timeout);
        let _work_scope = budget.enter();
        let mut trace_left = limits.max_trace_nodes;
        let mut selected = Vec::with_capacity(others.len());
        let mut stop: Option<TracedStopReason> = None;
        for allo_idx in 0..g.entries[root.0 as usize].allomorphs.len() {
            let mut permutation_index = 0usize;
            let keep_going = visit_permute_rules(g, others, &mut selected, &mut |perm| {
                if !pg_fst::work::consume() {
                    return false;
                }
                if outcome.seed_choices >= limits.max_seeds {
                    stop = Some(TracedStopReason::SeedLimit {
                        limit: limits.max_seeds,
                    });
                    return false;
                }
                if trace_left == Some(0) {
                    stop = Some(TracedStopReason::TraceLimit {
                        limit: limits.max_trace_nodes.unwrap_or(0),
                    });
                    return false;
                }
                outcome.seed_choices = outcome.seed_choices.saturating_add(1);
                let seed_key = format!("r{allo_idx}/p{permutation_index}");
                permutation_index += 1;
                let seed = self.build_generation_seed(root, allo_idx, perm, real_fs.clone());
                let items = self.seed_items(root, allo_idx, perm);
                let policy = FinalTemplateSynthesisPolicy {
                    always_enforce: self.always_enforce_final_templates,
                };

                let (generated_seed, truncated) = match trace_left {
                    None => {
                        let generated = self.synthesis_pipeline_selected_with_policy(
                            seed,
                            &NoopSink,
                            TraceHandle::DUMMY,
                            None,
                            &budget,
                            None,
                            policy,
                            None,
                        );
                        let mut found: BTreeMap<String, usize> = BTreeMap::new();
                        for word in &generated {
                            if self.is_word_valid(word) {
                                *found.entry(self.generated_surface_of(word)).or_default() += 1;
                            }
                        }
                        let surfaces = found
                            .into_iter()
                            .map(|(surface, derivations)| SeedSurface {
                                surface,
                                steps: Vec::new(),
                                derivations,
                            })
                            .collect();
                        (surfaces, false)
                    }
                    Some(left) => {
                        let sink = TreeTraceSink::with_node_cap(left);
                        let root_handle = sink.generate_words();
                        let generated = self.synthesis_pipeline_selected_with_policy(
                            seed,
                            &sink,
                            root_handle,
                            None,
                            &budget,
                            None,
                            policy,
                            None,
                        );
                        let surfaces = self.traced_seed_surfaces(generated, &sink);
                        trace_left = Some(left.saturating_sub(sink.len()));
                        outcome.trace_nodes = outcome.trace_nodes.saturating_add(sink.len());
                        (surfaces, sink.truncated())
                    }
                };
                outcome.seeds.push(GeneratedSeed {
                    seed_key,
                    items,
                    surfaces: generated_seed,
                    trace_truncated: truncated,
                });
                if truncated {
                    stop = Some(TracedStopReason::TraceLimit {
                        limit: limits.max_trace_nodes.unwrap_or(0),
                    });
                    return false;
                }
                !budget.capped() && !budget.timed_out()
            });
            if !keep_going {
                break;
            }
        }
        outcome.stop_reason = stop.or(if budget.timed_out() {
            Some(TracedStopReason::Deadline)
        } else if budget.capped() {
            Some(TracedStopReason::WorkLimit { limit: work_limit })
        } else {
            None
        });
        outcome.work_units = budget.work_steps();
        outcome
    }

    /// The selection's morphemes in selector order: the root first, then each requested morpheme.
    fn seed_items(&self, root: LexEntryId, allo_idx: usize, perm: &[PermItem]) -> Vec<SeedItem> {
        let g = self.g;
        let mut items = Vec::with_capacity(perm.len() + 1);
        items.push(self.entry_item(
            SeedItemRole::Root,
            root,
            g.entries[root.0 as usize].allomorphs[allo_idx].id,
            allo_idx,
        ));
        for item in perm {
            match *item {
                PermItem::Rule(id) => items.push(self.rule_item(id)),
                PermItem::NonHead(allomorph) => {
                    if let AllomorphOwner::Root(entry, index) =
                        g.allomorph_owners[allomorph.0 as usize]
                    {
                        items.push(self.entry_item(
                            SeedItemRole::NonHead,
                            entry,
                            allomorph,
                            index as usize,
                        ));
                    }
                }
            }
        }
        items
    }

    fn entry_item(
        &self,
        role: SeedItemRole,
        entry: LexEntryId,
        allomorph: AllomorphId,
        index: usize,
    ) -> SeedItem {
        let g = self.g;
        let def = &g.entries[entry.0 as usize];
        let info = g.morphemes.get(def.morpheme.0 as usize);
        SeedItem {
            role,
            entry_guid: def.source_guid.clone(),
            msa_guid: info.and_then(|info| info.source_msa_guid.clone()),
            allomorph_id: Some(allomorph.0),
            form_guid: g
                .allomorph_sources
                .get(allomorph.0 as usize)
                .and_then(|source| source.form_guids.first().cloned().flatten()),
            shape: def.allomorphs.get(index).map(|a| a.shape.text.clone()),
            rule_name: None,
        }
    }

    fn rule_item(&self, id: MRuleId) -> SeedItem {
        let g = self.g;
        let rule = &g.mrules[id.0 as usize];
        let info = rule
            .morpheme()
            .and_then(|morpheme| g.morphemes.get(morpheme.0 as usize));
        SeedItem {
            role: SeedItemRole::Rule,
            entry_guid: info
                .and_then(|info| info.owner.as_ref())
                .map(|owner| owner.entry_guid.clone()),
            msa_guid: info.and_then(|info| info.source_msa_guid.clone()),
            allomorph_id: None,
            form_guid: None,
            shape: None,
            rule_name: mrule_display(rule),
        }
    }

    /// Every valid word of one seed, keyed by surface, each with the least derivation that reached it.
    fn traced_seed_surfaces(&self, generated: Vec<Word>, sink: &TreeTraceSink) -> Vec<SeedSurface> {
        let parents = sink.parent_index();
        let log = event_log(sink);
        let mut found: BTreeMap<String, (Vec<GenerateStep>, usize)> = BTreeMap::new();
        for word in generated {
            if !self.is_word_valid(&word) {
                continue;
            }
            let chain: Vec<TraceHandle> = word
                .trace
                .filter(|handle| *handle != TraceHandle::DUMMY)
                .map(|handle| sink.path_from_root(&parents, handle))
                .unwrap_or_default();
            let steps = self.steps_for(&log, &chain, &word);
            let entry = found
                .entry(self.generated_surface_of(&word))
                .or_insert_with(|| (steps.clone(), 0));
            entry.1 += 1;
            if steps < entry.0 {
                entry.0 = steps;
            }
        }
        found
            .into_iter()
            .map(|(surface, (steps, derivations))| SeedSurface {
                surface,
                steps,
                derivations,
            })
            .collect()
    }

    /// The rule applications that turned the seed into `word`, found by walking the event log backward by shape because only morphological nodes chain through the word's cursor.
    fn steps_for(&self, log: &[LogEvent], chain: &[TraceHandle], word: &Word) -> Vec<GenerateStep> {
        let g = self.g;
        let chained: Vec<usize> = chain.iter().map(|handle| handle.index() as usize).collect();
        let mut target = word.shape.clone();
        let mut bound = log.len();
        let mut reversed = Vec::new();
        while let Some((index, event)) =
            log[..bound]
                .iter()
                .enumerate()
                .rev()
                .find(|(index, event)| {
                    event
                        .output
                        .as_ref()
                        .is_some_and(|(_, shape)| *shape == target)
                        && match event.kind {
                            EventKind::Phon(_) => event
                                .input
                                .as_ref()
                                .is_some_and(|(_, shape)| shape != &target),
                            EventKind::Morph(_) => chained.contains(index),
                            EventKind::Stratum => false,
                        }
                })
        {
            let (out_stratum, out_shape) = event.output.as_ref().expect("matched on its output");
            let (before_stratum, before) = match event.kind {
                EventKind::Phon(_) => event.input.clone().expect("matched on its input"),
                _ => morph_input(log, &chained, index, *out_stratum),
            };
            reversed.push(GenerateStep {
                stratum: stratum_display(g, *out_stratum),
                rule: step_rule(g, event.kind.step_kind().expect("matched a rule event")),
                input: render_shape(g, before_stratum, &before),
                output: render_shape(g, *out_stratum, out_shape),
                subrule: event.subrule,
            });
            target = before;
            bound = index;
        }
        reversed.reverse();
        reversed
    }
}

/// The word entering the morphological rule at `index`: the output of the previous chained morphological rule in its stratum, else that stratum's input.
fn morph_input(
    log: &[LogEvent],
    chained: &[usize],
    index: usize,
    stratum: StratumId,
) -> (StratumId, pg_shape::Shape) {
    for earlier in (0..index).rev() {
        let event = &log[earlier];
        match event.kind {
            EventKind::Morph(_) if chained.contains(&earlier) => {
                if let Some(output) = &event.output {
                    return output.clone();
                }
            }
            EventKind::Stratum if event.is_input => {
                if let Some(input) = &event.input {
                    return input.clone();
                }
            }
            _ => {}
        }
    }
    (stratum, pg_shape::Shape::default())
}

#[derive(Clone, Copy)]
enum EventKind {
    Phon(PRuleId),
    Morph(StepKind),
    Stratum,
}

impl EventKind {
    fn step_kind(self) -> Option<StepKind> {
        match self {
            Self::Phon(id) => Some(StepKind::Phon(id)),
            Self::Morph(kind) => Some(kind),
            Self::Stratum => None,
        }
    }
}

/// One node of a seed's trace, reduced to what the backward walk reads.
struct LogEvent {
    kind: EventKind,
    is_input: bool,
    subrule: Option<i32>,
    input: Option<(StratumId, pg_shape::Shape)>,
    output: Option<(StratumId, pg_shape::Shape)>,
}

/// Applied synthesis events in the order they ran; failed or not-applied nodes carry no usable pair and are dropped by their kind.
fn event_log(sink: &TreeTraceSink) -> Vec<LogEvent> {
    (0..sink.len())
        .map(|index| {
            sink.with_node(TraceHandle::from_index(index as u32), |node| {
                let kind = match (node.type_, node.source) {
                    (TraceType::PhonologicalRuleSynthesis, TraceSource::PhonRule(id)) => {
                        EventKind::Phon(id)
                    }
                    (
                        TraceType::MorphologicalRuleSynthesis | TraceType::CompoundingRuleSynthesis,
                        TraceSource::MorphRule(id),
                    ) if node.failure_reason.is_none() => EventKind::Morph(StepKind::Morph(id)),
                    (TraceType::TemplateSynthesisOutput, TraceSource::Template(id))
                        if node.failure_reason.is_none() =>
                    {
                        EventKind::Morph(StepKind::Template(id))
                    }
                    _ => EventKind::Stratum,
                };
                LogEvent {
                    kind,
                    is_input: node.type_ == TraceType::StratumSynthesisInput,
                    subrule: node.subrule_index.filter(|index| *index >= 0),
                    input: node.input.as_ref().map(|w| (w.stratum, w.shape.clone())),
                    output: node.output.as_ref().map(|w| (w.stratum, w.shape.clone())),
                }
            })
        })
        .collect()
}

#[derive(Clone, Copy)]
enum StepKind {
    Phon(PRuleId),
    Morph(MRuleId),
    Template(TemplateId),
}

fn step_rule(g: &Grammar, kind: StepKind) -> StepRule {
    match kind {
        StepKind::Phon(id) => {
            let (rule_id, name) = match g.prules.get(id.0 as usize) {
                Some(PhonRuleDef::Rewrite(rule)) => (rule.xml_id.clone(), rule.name.clone()),
                Some(PhonRuleDef::Metathesis(rule)) => (rule.xml_id.clone(), rule.name.clone()),
                None => (format!("prule#{}", id.0), None),
            };
            StepRule {
                kind: "phon",
                guid: Some(rule_id.clone()).filter(|text| looks_like_guid(text)),
                id: rule_id,
                name,
            }
        }
        StepKind::Morph(id) => {
            let rule = g.mrules.get(id.0 as usize);
            let info = rule
                .and_then(MorphRuleDef::morpheme)
                .and_then(|morpheme| g.morphemes.get(morpheme.0 as usize));
            let (rule_id, guid) = match (rule, info) {
                (Some(MorphRuleDef::Compounding(def)), _) => {
                    (def.xml_id.clone(), def.source_guid.clone())
                }
                (_, Some(info)) => (info.xml_key.clone(), info.source_msa_guid.clone()),
                _ => (format!("mrule#{}", id.0), None),
            };
            StepRule {
                kind: "morph",
                id: rule_id,
                name: rule.and_then(mrule_display),
                guid,
            }
        }
        StepKind::Template(id) => {
            let def = g.templates.get(id.0 as usize);
            StepRule {
                kind: "template",
                id: format!("template#{}", id.0),
                name: def.and_then(|def| def.name.clone()),
                guid: def.and_then(|def| def.source_guid.clone()),
            }
        }
    }
}

fn mrule_display(rule: &MorphRuleDef) -> Option<String> {
    match rule {
        MorphRuleDef::AffixProcess(def) => def.name.clone(),
        MorphRuleDef::Realizational(def) => def.name.clone(),
        MorphRuleDef::Compounding(def) => def.name.clone(),
    }
}

fn stratum_display(g: &Grammar, id: StratumId) -> String {
    g.strata
        .get(id.0 as usize)
        .and_then(|stratum| stratum.name.clone())
        .unwrap_or_else(|| format!("stratum#{}", id.0))
}

/// A mid-derivation shape in its own stratum's table, morpheme boundaries kept so a reader can see where each affix sits.
fn render_shape(g: &Grammar, stratum: StratumId, shape: &pg_shape::Shape) -> String {
    let table = &g.char_tables[g.strata[stratum.0 as usize].table.0 as usize];
    surface::to_plain_string(table, shape, true)
}

fn looks_like_guid(text: &str) -> bool {
    text.len() == 36
        && text.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}
