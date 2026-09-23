//! A grammar author has no way to see how their
//! language is actually handled. `crate::plan`/`crate::enumerate::enumerate_default` already turned
//! compilation into an explicit, content-addressed AND-OR DAG;
//! this module renders THAT real `Plan` — never a parallel, hand-maintained description that could
//! drift — first as a versioned JSON document, then as a mermaid diagram over that same document.
//!
//! # Two steps, on purpose (mirrors `crate::health`'s convention)
//! `build_plan_document` projects a `Plan` (plus its grammar and the real capability evaluation)
//! into `PlanDocument` — canonical, versioned JSON, independently useful for diffing two grammar
//! revisions or machine checks. `render_mermaid` is a **pure function over that documented JSON
//! shape**, never over the `Plan`/`Grammar` again — exactly `crate::health`'s "canonical JSON is the
//! source artifact; the rendering is a view" split.
//!
//! # Node identity IS the plan's content address
//! `PlanDocumentNode::id` is `crate::plan::NodeId`'s own `Display` (16 lowercase hex digits) —
//! not a separately-invented diagram-local id. Two runs over an unchanged grammar therefore produce
//! byte-identical JSON (this module's own `plan_diagram_determinism` test pins that), and a diff
//! between two grammar revisions highlights exactly the subtrees whose *meaning* changed (this
//! module's own `plan_diagram_content_address_property` test pins that too, rather than assuming
//! it).
//!
//! # Linguistic labelling (never a second source of truth)
//! Every `PlanDocumentNode::label` names the linguistic work that node performs (a stratum, a
//! rewrite/metathesis rule and its mode/direction, a gate partition and what it gates on, a rewrite
//! cascade and its member rules) with the plan node kind carried separately, as secondary detail, in
//! `PlanDocumentNode::kind`. Every label is derived from the SAME `Plan` node's own payload
//! (`FragmentSpec`/`Provenance`/`GatePartitionSpec`/`ReplaceCascadeSpec`) plus read-only lookups
//! against the SAME `Grammar` the plan was built from (stratum names, rule names/xml-ids) — never a
//! parallel description invented here. See `leaf_label`/`gate_label`/`replace_label`.
//!
//! # Capability verdicts, read from the real evaluation
//! `PlanDocumentNode::verdict` is **not** inferred from a node merely existing in the plan (a node
//! exists whether or not it was admitted — `crate::capability::compose_envelope`'s own module doc).
//! `per_node_verdicts` computes each node's real, bottom-up `crate::capability::CompileDecision`
//! by mirroring `crate::capability`'s own private `node_decision` walk — same registry, same
//! `crate::capability::CharacteristicsProfile`, same `crate::capability::meet` — over ONLY that
//! module's own public API (this file never modifies
//! `capability.rs`). `PlanDocument::overall_verdict` is, separately, the literal, unmodified return
//! value of `crate::capability::compose_envelope` itself, so a reader always has the ONE
//! authoritative whole-grammar answer available even where a characteristic has no distinct
//! `crate::plan::PlanNodeKind` to hang a node-local verdict on (`compose_envelope`'s own doc names
//! several: `Compounding`, `UnorderedMorphRuleApplication`, `MprGroupAppend`, `MprGroupOverwrite`,
//! `CircumfixOutputAction`, `Reduplication`). This module's own `plan_diagram_root_verdict_matches_
//! compose_envelope` test pins that the two agree for `crate::capability::default_registry`, whose
//! own test already proves it leaves no `ConfigPredicate` characteristic undischarged.
//!
//! **A grammar-wide characteristic with no distinct plan node legitimately marks EVERY node
//! refused** (because the predicate that discharges it ignores which node it is asked about and
//! answers the same way everywhere — exactly `compose_envelope`'s own documented "representative
//! node" shape). This is not a rendering bug: it is what the real bottom-up algorithm computes, and
//! it is exactly what happens for all three reference grammars today (`mpr-group.overwrite-output`,
//! a permanent carve-out) — see this module's own tests for both that whole-plan-refusal shape AND
//! the contrasting node-LOCAL shape (`simultaneous.subrule-overlap`, which refuses one specific
//! rewrite-rule leaf while an unrelated sibling rule leaf stays `Admit`).
//!
//! # Honest summarization (mermaid only — the JSON is always the complete plan)
//! A plan over a realistic lexicon can have far more sibling rewrite-rule leaves under one
//! `crate::plan::PlanNodeKind::Replace` node than mermaid can draw at all (mermaid fails outright
//! above a size, rather than degrading). `render_mermaid` collapses sibling LEAF children above a
//! caller-chosen threshold (default `DEFAULT_LEAF_COLLAPSE_THRESHOLD`) into one labelled summary
//! node carrying a count, and the rendered text ALWAYS states the threshold, whether summarization
//! actually happened, and the node count actually emitted (`MermaidRender`'s own fields, plus the
//! same facts as `%%` comment lines in the rendered text itself) — a reader must never be left
//! guessing whether they are looking at the whole plan. `RenderMode::Full` opts out entirely.
//! Collapsing decisions are made per PARENT EDGE, not globally per node: a leaf shared by several
//! parents (real, since `crate::plan::Plan` dedups identical subtrees) can be drawn in full under one
//! parent while folded into a summary node under another whose own child count crossed the
//! threshold — both are faithful to that parent's own fan-out.

use std::collections::{BTreeMap, HashMap, HashSet};

use pg_grammar::model::{Grammar, LexEntryId, PRuleId, PhonRuleDef};
use serde::{Deserialize, Serialize};

use crate::capability::{
    compose_envelope_with_semantics, default_registry, CapabilityDiagnostic, CharacteristicKind,
    CharacteristicsProfile, CompileDecision, Disposition, PredicateRegistry, PredicateVerdict,
};
use crate::grammar_semantics::GrammarSemantics;
use crate::plan::{
    ComposeStrategy, FragmentSpec, GatedSubruleRef, NodeId, Plan, PlanNodeKind, Provenance,
};
use crate::plan_interaction_coverage::plan_for_semantics;

/// This document's own schema version (same discipline as `crate::coverage_ledger::
/// COVERAGE_LEDGER_SCHEMA_VERSION`/`crate::health::HEALTH_SCHEMA_VERSION`) — bump only on a
/// wire-incompatible change to `PlanDocument`'s shape.
pub const PLAN_DIAGRAM_SCHEMA_VERSION: u32 = 1;

// Capability verdicts: DiagnosticView + NodeVerdict, the real evaluation's serializable projection.

/// An owned, serializable projection of `CapabilityDiagnostic` (which does not itself derive
/// `serde` traits) — same fields, `predicate` widened from `&'static str` to `String` only because
/// `serde` needs an owned type to (de)serialize into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticView {
    pub predicate: String,
    pub construct: String,
    pub witness: String,
}

impl From<&CapabilityDiagnostic> for DiagnosticView {
    fn from(d: &CapabilityDiagnostic) -> Self {
        DiagnosticView {
            predicate: d.predicate.to_string(),
            construct: d.construct.clone(),
            witness: d.witness.clone(),
        }
    }
}

/// A serializable projection of `CompileDecision` (which does not itself derive `serde` traits) —
/// same three outcomes, `Refuse`'s diagnostics widened to `DiagnosticView`. Never constructed from
/// a node's mere presence in the plan — see this module's own top-doc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum NodeVerdict {
    /// Proven faithful; admission-filtering licensed.
    Admit,
    /// Propose the superset; no no-false-negative proof, but a first-class non-failure.
    ConfirmOnly,
    /// Refused, carrying every diagnostic collected for this node (or, for the whole-plan
    /// `PlanDocument::overall_verdict`, for the whole plan).
    Refuse { diagnostics: Vec<DiagnosticView> },
}

impl NodeVerdict {
    fn from_decision(d: &CompileDecision) -> Self {
        match d {
            CompileDecision::Admit => NodeVerdict::Admit,
            CompileDecision::ConfirmOnly => NodeVerdict::ConfirmOnly,
            CompileDecision::Refuse(diags) => NodeVerdict::Refuse {
                diagnostics: diags.iter().map(DiagnosticView::from).collect(),
            },
        }
    }

    /// `true` iff this verdict is `NodeVerdict::Refuse` — the one fact `render_mermaid` must
    /// show unmistakably: a refused construct must be visible in the picture, not only in a
    /// diagnostic.
    pub fn is_refused(&self) -> bool {
        matches!(self, NodeVerdict::Refuse { .. })
    }
}

/// Restates `crate::capability`'s private `verdict_to_decision` mapping, since that helper is not importable.
fn predicate_verdict_to_decision(v: PredicateVerdict) -> CompileDecision {
    match v {
        PredicateVerdict::Admit => CompileDecision::Admit,
        PredicateVerdict::ConfirmOnly => CompileDecision::ConfirmOnly,
        PredicateVerdict::Refuse(diag) => CompileDecision::Refuse(vec![diag]),
    }
}

/// Mirrors `crate::capability`'s private `node_decision` exactly, over only that module's public API, since this module must not modify `capability.rs`; pinned against the real `compose_envelope` by `plan_diagram_root_verdict_matches_compose_envelope`.
fn node_decision_mirror(
    grammar: &Grammar,
    plan: &Plan,
    profile: &CharacteristicsProfile,
    registry: &PredicateRegistry,
    relevant_kinds: &HashSet<CharacteristicKind>,
    node_id: NodeId,
    cache: &mut HashMap<NodeId, CompileDecision>,
) -> CompileDecision {
    if let Some(cached) = cache.get(&node_id) {
        return cached.clone();
    }
    let Some(kind) = plan.get(node_id) else {
        // Dangling id: not a capability judgment, vacuously Admit (same defensive choice as `capability::node_decision`).
        return CompileDecision::Admit;
    };

    let mut decision = CompileDecision::Admit;
    for &child in kind.children() {
        decision = crate::capability::meet(
            decision,
            node_decision_mirror(
                grammar,
                plan,
                profile,
                registry,
                relevant_kinds,
                child,
                cache,
            ),
        );
    }
    for predicate in registry.predicates() {
        if predicate
            .discharges()
            .iter()
            .any(|k| relevant_kinds.contains(k))
        {
            decision = crate::capability::meet(
                decision,
                predicate_verdict_to_decision(predicate.evaluate(grammar, profile, kind)),
            );
        }
    }

    cache.insert(node_id, decision.clone());
    decision
}

/// Every node's own `CompileDecision`, keyed by `NodeId`; `compose_envelope` itself only ever returns the single whole-plan decision at the root.
fn per_node_verdicts(
    grammar: &Grammar,
    plan: &Plan,
    profile: &CharacteristicsProfile,
    registry: &PredicateRegistry,
) -> HashMap<NodeId, CompileDecision> {
    let relevant_kinds: HashSet<CharacteristicKind> = profile
        .observations()
        .iter()
        .filter(|o| o.disposition != Disposition::Proven)
        .map(|o| o.kind)
        .collect();

    let mut cache = HashMap::new();
    for (id, _) in plan.iter() {
        // `cache` is this loop's output; the returned decision exists for the recursive case.
        let _ = node_decision_mirror(
            grammar,
            plan,
            profile,
            registry,
            &relevant_kinds,
            id,
            &mut cache,
        );
    }
    cache
}

// Linguistic labelling: derived from the plan's own payload + the same Grammar, nothing else.

fn stratum_label_at(g: &Grammar, index: usize) -> String {
    g.strata
        .get(index)
        .and_then(|s| s.name.clone())
        .unwrap_or_else(|| format!("stratum#{index}"))
}

fn stratum_name_for_prule(g: &Grammar, rule: PRuleId) -> Option<String> {
    g.strata
        .iter()
        .enumerate()
        .find(|(_, s)| s.prules.contains(&rule))
        .map(|(i, _)| stratum_label_at(g, i))
}

fn stratum_names_for_entries(g: &Grammar, entries: &[LexEntryId]) -> Vec<String> {
    let entry_set: HashSet<LexEntryId> = entries.iter().copied().collect();
    let mut names: Vec<String> = g
        .strata
        .iter()
        .enumerate()
        .filter(|(_, s)| s.entries.iter().any(|e| entry_set.contains(e)))
        .map(|(i, _)| stratum_label_at(g, i))
        .collect();
    names.sort();
    names.dedup();
    names
}

fn rule_name(g: &Grammar, rule: PRuleId) -> String {
    match g.prules.get(rule.0 as usize) {
        Some(PhonRuleDef::Rewrite(def)) => def.name.clone().unwrap_or_else(|| def.xml_id.clone()),
        Some(PhonRuleDef::Metathesis(def)) => {
            def.name.clone().unwrap_or_else(|| def.xml_id.clone())
        }
        None => format!("rule#{}", rule.0),
    }
}

fn rewrite_rule_label(g: &Grammar, rule: PRuleId) -> String {
    let stratum = stratum_name_for_prule(g, rule).unwrap_or_else(|| "unmapped stratum".to_string());
    let name = rule_name(g, rule);
    match g.prules.get(rule.0 as usize) {
        Some(PhonRuleDef::Rewrite(def)) => format!(
            "Rewrite rule '{name}' -- stratum {stratum} ({:?}, {:?})",
            def.mode, def.dir
        ),
        Some(PhonRuleDef::Metathesis(def)) => {
            format!(
                "Metathesis rule '{name}' -- stratum {stratum} ({:?})",
                def.dir
            )
        }
        None => format!("Rewrite rule '{name}' -- stratum {stratum} (unresolved)"),
    }
}

fn lexicon_label(g: &Grammar, entries: &Option<Vec<LexEntryId>>) -> String {
    match entries {
        None => format!("Lexicon (whole grammar, {} entries)", g.entries.len()),
        Some(ids) => {
            let strata = stratum_names_for_entries(g, ids);
            let strata_desc = if strata.is_empty() {
                "no owning stratum found".to_string()
            } else {
                strata.join(", ")
            };
            format!(
                "Lexicon fragment: {} entries (stratum {strata_desc})",
                ids.len()
            )
        }
    }
}

fn leaf_label(g: &Grammar, fragment: &FragmentSpec) -> String {
    match fragment {
        FragmentSpec::LexiconFragment { entries } => lexicon_label(g, entries),
        FragmentSpec::RewriteRule { rule } => rewrite_rule_label(g, *rule),
        FragmentSpec::GuardAutomaton { group_key } => {
            format!("Guard automaton (gate key {group_key:?})")
        }
        FragmentSpec::CompositeEmissionMarker => {
            "Composite-emission subtree (multi-tag composite entries)".to_string()
        }
        FragmentSpec::StructuralCompositeMarker => {
            "Structural-composite subtree (circumfix / dropped-material rules)".to_string()
        }
    }
}

fn compose_strategy_name(s: ComposeStrategy) -> &'static str {
    match s {
        ComposeStrategy::Static => "Static",
    }
}

/// Stable, exhaustively-matched tags for `FragmentSpec` variants: the JSON payload's `fragment` field, and the mermaid summarizer's collapsing group key.
fn fragment_tag(fragment: &FragmentSpec) -> &'static str {
    match fragment {
        FragmentSpec::LexiconFragment { .. } => "lexicon_fragment",
        FragmentSpec::RewriteRule { .. } => "rewrite_rule",
        FragmentSpec::GuardAutomaton { .. } => "guard_automaton",
        FragmentSpec::CompositeEmissionMarker => "composite_emission_marker",
        FragmentSpec::StructuralCompositeMarker => "structural_composite_marker",
    }
}

fn provenance_tag(p: &Provenance) -> &'static str {
    match p {
        Provenance::Lexicon => "lexicon",
        Provenance::RewriteRule(_) => "rewrite_rule",
        Provenance::MorphRule(_) => "morph_rule",
        Provenance::Template(_) => "template",
        Provenance::Gate => "gate",
        Provenance::Replace => "replace",
        Provenance::CompositeEmission => "composite_emission",
        Provenance::StructuralComposite => "structural_composite",
    }
}

fn gate_label(group_count: usize, gated: &[GatedSubruleRef]) -> String {
    if gated.is_empty() {
        format!("Gate: {group_count} partition group(s) (ungated)")
    } else {
        let subrules: Vec<String> = gated
            .iter()
            .map(|g| format!("(rule_pos={}, sub_idx={})", g.rule_pos, g.sub_idx))
            .collect();
        format!(
            "Gate: {group_count} partition group(s) over subrule(s) {}",
            subrules.join(", ")
        )
    }
}

fn replace_label(
    g: &Grammar,
    rules: &[PRuleId],
    gated_subrules: &[GatedSubruleRef],
    group_key: &[bool],
) -> String {
    let names: Vec<String> = rules.iter().map(|r| rule_name(g, *r)).collect();
    let base = format!(
        "Rewrite cascade: {} rule(s) [{}]",
        rules.len(),
        names.join(", ")
    );
    if gated_subrules.is_empty() {
        base
    } else {
        format!("{base} -- gated (key={group_key:?})")
    }
}

// The JSON document.

/// An owned, serializable projection of `GatedSubruleRef` (which does not derive `serde` traits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatedSubruleRefView {
    pub rule_pos: usize,
    pub sub_idx: usize,
}

impl From<&GatedSubruleRef> for GatedSubruleRefView {
    fn from(r: &GatedSubruleRef) -> Self {
        GatedSubruleRefView {
            rule_pos: r.rule_pos,
            sub_idx: r.sub_idx,
        }
    }
}

/// A structured, machine-checkable projection of one `PlanNodeKind`'s own config (excluding
/// `children`, which `PlanDocumentNode::children` already carries) — independently
/// usable for machine checks: a caller diffing two
/// grammar revisions can compare `payload` fields directly, not just the human-readable `label`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodePayload {
    Leaf {
        /// `fragment_tag` — also the mermaid summarizer's collapsing group key.
        fragment: String,
        /// `provenance_tag`.
        provenance: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        rule_id: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        entry_count: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        group_key: Option<Vec<bool>>,
    },
    Compose {
        strategy: String,
    },
    Union,
    Gate {
        gated_subrules: Vec<GatedSubruleRefView>,
        group_keys: Vec<Vec<bool>>,
    },
    Replace {
        rules: Vec<u32>,
        gated_subrules: Vec<GatedSubruleRefView>,
        group_key: Vec<bool>,
    },
}

/// One node in a `PlanDocument`. `id` is `NodeId`'s own content address (`Display`, 16 lowercase
/// hex digits) — never a diagram-local counter: `NodeId` is the diagram's node
/// identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanDocumentNode {
    pub id: String,
    /// The plan node kind (`Leaf`/`Compose`/`Union`/`Gate`/`Replace`) — secondary detail; see this
    /// module's top-doc "Linguistic labelling" section for why `label` carries the primary meaning.
    pub kind: String,
    pub children: Vec<String>,
    /// The linguistic work this node performs, derived from its own payload plus the grammar it was
    /// built from — never a second, independently-invented description.
    pub label: String,
    pub payload: NodePayload,
    /// This node's own real, bottom-up capability verdict — see `per_node_verdicts`'s doc.
    pub verdict: NodeVerdict,
}

/// The versioned JSON projection of a `Plan`: a documented, versioned JSON shape.
/// See this module's top-doc for the full contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanDocument {
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// The real, unmodified `crate::capability::compose_envelope` verdict for the whole grammar —
    /// see this module's top-doc for why this is kept alongside (not instead of) per-node verdicts.
    pub overall_verdict: NodeVerdict,
    /// Every node, in `Plan::iter`'s own content-address order (deterministic — `Plan`'s own doc)
    /// — this is what makes two runs over an unchanged grammar serialize byte-identically.
    pub nodes: Vec<PlanDocumentNode>,
}

impl PlanDocument {
    /// Canonical machine-readable form — pretty-printed, two-space indent, fields in Rust
    /// declaration order (serde's unmodified default), mirroring `crate::health`/`crate::
    /// coverage_ledger`'s own determinism convention.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    /// Looks up one node by its content-address id (the same string `PlanDocumentNode::id`/
    /// `PlanDocument::root` carry).
    pub fn node(&self, id: &str) -> Option<&PlanDocumentNode> {
        self.nodes.iter().find(|n| n.id == id)
    }
}

fn build_node(
    g: &Grammar,
    id: NodeId,
    kind: &PlanNodeKind,
    verdict: &CompileDecision,
) -> PlanDocumentNode {
    let children: Vec<String> = kind.children().iter().map(NodeId::to_string).collect();

    let (label, payload) = match kind {
        PlanNodeKind::Leaf {
            fragment,
            provenance,
        } => {
            let label = leaf_label(g, fragment);
            let (rule_id, entry_count, group_key) = match fragment {
                FragmentSpec::RewriteRule { rule } => (Some(rule.0), None, None),
                FragmentSpec::LexiconFragment { entries } => (
                    None,
                    Some(entries.as_ref().map_or_else(|| g.entries.len(), Vec::len)),
                    None,
                ),
                FragmentSpec::GuardAutomaton { group_key } => (None, None, Some(group_key.clone())),
                FragmentSpec::CompositeEmissionMarker | FragmentSpec::StructuralCompositeMarker => {
                    (None, None, None)
                }
            };
            (
                label,
                NodePayload::Leaf {
                    fragment: fragment_tag(fragment).to_string(),
                    provenance: provenance_tag(provenance).to_string(),
                    rule_id,
                    entry_count,
                    group_key,
                },
            )
        }
        PlanNodeKind::Compose { strategy, .. } => {
            let s = compose_strategy_name(*strategy);
            (
                format!("Composition ({s})"),
                NodePayload::Compose {
                    strategy: s.to_string(),
                },
            )
        }
        PlanNodeKind::Union { children } => (
            format!("Union of {} branch(es)", children.len()),
            NodePayload::Union,
        ),
        PlanNodeKind::Gate { partition, .. } => {
            let label = gate_label(partition.groups.len(), &partition.gated_subrules);
            (
                label,
                NodePayload::Gate {
                    gated_subrules: partition
                        .gated_subrules
                        .iter()
                        .map(GatedSubruleRefView::from)
                        .collect(),
                    group_keys: partition.groups.iter().map(|gr| gr.key.clone()).collect(),
                },
            )
        }
        PlanNodeKind::Replace { cascade, .. } => {
            let label = replace_label(
                g,
                &cascade.rules,
                &cascade.gated_subrules,
                &cascade.group_key,
            );
            (
                label,
                NodePayload::Replace {
                    rules: cascade.rules.iter().map(|r| r.0).collect(),
                    gated_subrules: cascade
                        .gated_subrules
                        .iter()
                        .map(GatedSubruleRefView::from)
                        .collect(),
                    group_key: cascade.group_key.clone(),
                },
            )
        }
    };

    PlanDocumentNode {
        id: id.to_string(),
        kind: kind.kind_name().to_string(),
        children,
        label,
        payload,
        verdict: NodeVerdict::from_decision(verdict),
    }
}

/// Builds `g`'s `PlanDocument`: `plan_for_semantics` assembles the real `Plan`/
/// `CharacteristicsProfile` the way `crate::backend_selection::best_case_across_backends` does; [`crate::
/// capability::compose_envelope`] supplies the real whole-grammar verdict; `per_node_verdicts`
/// supplies the real per-node verdicts (mirroring the same algorithm, see its own doc). Every label
/// is derived from each node's own payload plus `g` — see this module's top-doc.
///
/// Shares ONE `crate::grammar_semantics::GrammarSemantics` for the whole document:
/// `build_plan_document` derives it once, then `build_plan_document_with_semantics` passes it to
/// `plan_for_semantics` and `build_plan_document_for_plan_with_semantics`.
pub fn build_plan_document(g: &Grammar) -> PlanDocument {
    build_plan_document_with_semantics(&GrammarSemantics::derive(g))
}

/// `build_plan_document` over an already-derived `GrammarSemantics`.
pub fn build_plan_document_with_semantics(semantics: &GrammarSemantics<'_>) -> PlanDocument {
    let plan = plan_for_semantics(semantics);
    build_plan_document_for_plan_with_semantics(semantics, &plan)
}

/// Projects an already materialized backend plan over an already-derived `GrammarSemantics`, using
/// the same capability evidence and labels as the default grammar-derived plan.
pub fn build_plan_document_for_plan_with_semantics(
    semantics: &GrammarSemantics<'_>,
    plan: &Plan,
) -> PlanDocument {
    let g = semantics.grammar();
    let profile = semantics.characteristics();
    let registry = default_registry();
    let verdicts = per_node_verdicts(g, plan, profile, &registry);
    let overall = compose_envelope_with_semantics(semantics, plan, &registry);

    // `plan.iter()` yields deterministic content-address order already, so no additional sort is needed here.
    let nodes: Vec<PlanDocumentNode> = plan
        .iter()
        .map(|(id, kind)| {
            let decision = verdicts.get(&id).cloned().unwrap_or(CompileDecision::Admit);
            build_node(g, id, kind, &decision)
        })
        .collect();

    PlanDocument {
        schema_version: PLAN_DIAGRAM_SCHEMA_VERSION,
        root: plan.root().map(|r| r.to_string()),
        overall_verdict: NodeVerdict::from_decision(&overall),
        nodes,
    }
}

// Mermaid rendering: a pure function over PlanDocument.

/// The default readability threshold above which `render_mermaid` collapses sibling leaf children
/// into a summary node (see this module's top-doc "Honest summarization" section). Chosen well
/// above any node count a small synthetic test fixture produces (so ordinary tests exercise the
/// uncollapsed path by default) and well below the count at which mermaid's own renderer degrades.
pub const DEFAULT_LEAF_COLLAPSE_THRESHOLD: usize = 24;

/// Whether `render_mermaid` summarizes large sibling-leaf groups or draws every node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// Collapse sibling leaf groups whose count exceeds `threshold` under any one parent.
    Summarized { threshold: usize },
    /// Opt-in full rendering: draw every node, regardless of size (an explicit non-default
    /// escape hatch).
    Full,
}

impl Default for RenderMode {
    fn default() -> Self {
        RenderMode::Summarized {
            threshold: DEFAULT_LEAF_COLLAPSE_THRESHOLD,
        }
    }
}

/// `render_mermaid`'s result: the rendered text plus the honesty facts a reader needs regardless
/// of whether they read the text closely -- the renderer reports the node count it
/// emitted so a failed render is diagnosable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MermaidRender {
    pub mermaid: String,
    /// `None` for `RenderMode::Full`; `Some(threshold)` for `RenderMode::Summarized`, regardless
    /// of whether any group actually exceeded it.
    pub threshold: Option<usize>,
    /// `true` iff at least one sibling-leaf group was actually collapsed.
    pub summarized: bool,
    /// The number of distinct node-definition lines actually written (individual nodes + summary
    /// nodes) -- what a reader actually sees.
    pub emitted_node_count: usize,
    /// The number of distinct nodes reachable from the root in the FULL, uncollapsed plan --
    /// independent of `mode`, so a caller can always tell how much was folded away.
    pub total_node_count: usize,
}

fn mermaid_escape(s: &str) -> String {
    s.replace('"', "'").replace(['\n', '\r'], " ")
}

fn verdict_class(v: &NodeVerdict) -> &'static str {
    match v {
        NodeVerdict::Admit => "pgAdmit",
        NodeVerdict::ConfirmOnly => "pgConfirm",
        NodeVerdict::Refuse { .. } => "pgRefuse",
    }
}

fn verdict_suffix(v: &NodeVerdict) -> String {
    match v {
        NodeVerdict::Admit => "Admit".to_string(),
        NodeVerdict::ConfirmOnly => "ConfirmOnly".to_string(),
        NodeVerdict::Refuse { diagnostics } => {
            let first = diagnostics
                .first()
                .map(|d| d.predicate.as_str())
                .unwrap_or("?");
            format!("REFUSED ({first})")
        }
    }
}

fn mid(id: &str) -> String {
    format!("n{id}")
}

fn leaf_group_key(node: &PlanDocumentNode) -> String {
    match &node.payload {
        NodePayload::Leaf { fragment, .. } => fragment.clone(),
        _ => "leaf".to_string(),
    }
}

/// The number of distinct nodes reachable from `root`, independent of any collapsing decision.
fn reachable_node_count(by_id: &HashMap<&str, &PlanDocumentNode>, root: &str) -> usize {
    let mut seen: HashSet<String> = HashSet::new();
    let mut stack = vec![root.to_string()];
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(n) = by_id.get(id.as_str()) {
            stack.extend(n.children.iter().cloned());
        }
    }
    seen.len()
}

/// Renders `doc` as a mermaid `flowchart` — a pure function over the documented `PlanDocument`
/// shape, never over the `Plan`/`Grammar` again. See
/// this module's top-doc "Honest summarization" section for the collapsing contract.
pub fn render_mermaid(doc: &PlanDocument, mode: RenderMode) -> MermaidRender {
    let by_id: HashMap<&str, &PlanDocumentNode> =
        doc.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let threshold = match mode {
        RenderMode::Summarized { threshold } => Some(threshold),
        RenderMode::Full => None,
    };

    let Some(root) = doc.root.clone() else {
        return MermaidRender {
            mermaid: "%% empty plan: no root node\nflowchart TD\n".to_string(),
            threshold,
            summarized: false,
            emitted_node_count: 0,
            total_node_count: 0,
        };
    };
    let total_node_count = reachable_node_count(&by_id, &root);

    let mut body: Vec<String> = Vec::new();
    let mut defined: HashSet<String> = HashSet::new();
    let mut seen_nodes: HashSet<String> = HashSet::new();
    let mut summarized_any = false;

    let mut stack = vec![root.clone()];
    while let Some(node_id) = stack.pop() {
        if !seen_nodes.insert(node_id.clone()) {
            continue;
        }
        let Some(node) = by_id.get(node_id.as_str()) else {
            continue;
        };

        if defined.insert(node_id.clone()) {
            body.push(format!(
                "  {}[\"{}\\n({} . {})\"]:::{}",
                mid(&node_id),
                mermaid_escape(&node.label),
                node.kind,
                verdict_suffix(&node.verdict),
                verdict_class(&node.verdict)
            ));
        }

        // Partition this node's children into leaf-kind groups vs. everything else; a per-parent-edge decision, never a whole-plan blanket rule.
        let mut leaf_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut other_children: Vec<String> = Vec::new();
        for child_id in &node.children {
            match by_id.get(child_id.as_str()) {
                Some(child) if child.kind == "Leaf" => {
                    leaf_groups
                        .entry(leaf_group_key(child))
                        .or_default()
                        .push(child_id.clone());
                }
                _ => other_children.push(child_id.clone()),
            }
        }

        for child_id in &other_children {
            body.push(format!("  {} --> {}", mid(&node_id), mid(child_id)));
            stack.push(child_id.clone());
        }

        for (frag_kind, members) in &leaf_groups {
            if threshold.is_some_and(|t| members.len() > t) {
                summarized_any = true;
                let summary_id = format!("{}_summary_{frag_kind}", mid(&node_id));
                if defined.insert(summary_id.clone()) {
                    body.push(format!(
                        "  {summary_id}[\"{} x {frag_kind} leaves collapsed (> {} threshold)\"]",
                        members.len(),
                        threshold.unwrap()
                    ));
                }
                body.push(format!("  {} --> {summary_id}", mid(&node_id)));
            } else {
                for child_id in members {
                    body.push(format!("  {} --> {}", mid(&node_id), mid(child_id)));
                    stack.push(child_id.clone());
                }
            }
        }
    }

    let emitted_node_count = defined.len();
    let threshold_desc = match threshold {
        Some(t) => t.to_string(),
        None => "none (full rendering requested)".to_string(),
    };
    let mut lines = vec![
        "%% pg-foma compilation-plan diagram".to_string(),
        format!(
            "%% summarization: {} (threshold={threshold_desc})",
            if summarized_any {
                "applied"
            } else {
                "not applied"
            }
        ),
        format!("%% nodes emitted: {emitted_node_count} of {total_node_count} reachable"),
        "flowchart TD".to_string(),
        "classDef pgAdmit fill:#dcffe4,stroke:#1a7f37,color:#0a3622;".to_string(),
        "classDef pgConfirm fill:#fff8c5,stroke:#9a6700,color:#4d3800;".to_string(),
        "classDef pgRefuse fill:#ffebe9,stroke:#cf222e,color:#82071e;".to_string(),
    ];
    lines.extend(body);

    MermaidRender {
        mermaid: lines.join("\n") + "\n",
        threshold,
        summarized: summarized_any,
        emitted_node_count,
        total_node_count,
    }
}

#[cfg(test)]
mod tests;
