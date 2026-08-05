//! Derives the [`crate::recipe_mechanism::MechanismGraph`] from the shared [`GrammarSemantics`]
//! and from nothing else.
//!
//! # The one rule, and how the signature enforces it
//! [`derive_mechanism_graph`] takes `&GrammarSemantics` and no `&Grammar`. That is not a
//! convention this module promises to keep; it is the whole surface. There is no second entry
//! point, no `&Grammar` front end (unlike
//! [`crate::recipe_registry::Applicability::matches`], which deliberately keeps one), and
//! [`GrammarSemantics::grammar`] is never called anywhere in this file -- so a provider CANNOT
//! decide applicability, membership, or ordering from a fresh grammar walk. Everything below is a
//! projection of a fact `GrammarSemantics` already owns.
//!
//! # Why observations are the right input
//! A mechanism exists because a construct was OBSERVED, and
//! [`crate::capability::CharacteristicObservation`] carries exactly the two things needed to place
//! it: a [`crate::capability::CharacteristicKind`] (which mechanism owns it, via
//! [`mechanism_kind_for`]; and, unchanged, which compilers can represent it, via
//! [`crate::strategy_coverage`]) and a [`crate::capability::ModelLocation`] (where it came from,
//! convertible to a typed [`MechanismSource`] by an impl that already existed). The join is
//! total and needs no grammar access.
//!
//! **Inert hints create nothing, for free.** The repo's standing trap here is the
//! `ReduplicationHint` shortcut: an allomorph may carry a non-`Implicit` hint and not reduplicate
//! at all. [`crate::capability::characterize`] already refuses to raise
//! `CharacteristicKind::Reduplication` for such an allomorph (it uses the structural
//! [`crate::capability::rhs_has_true_reduplication`] test, the single authority for the fact), so
//! an inert hint produces no observation, therefore no source, therefore no `CopyProcess` node.
//! This module does not re-decide the question, which is precisely why it cannot get it wrong.
//! `tests/mechanism_provider_gate.rs` pins both halves: inert hint -> zero copy nodes, real
//! reduplication -> exactly one.
//!
//! # Canonical identity
//! Nodes are emitted in [`MechanismKind::COMPOSITION_ORDER`] and edges chain the present nodes in
//! that same order, so two derivations of the same grammar are identical as data and
//! byte-identical under [`MechanismGraph::canonical_projection`]. Every collection inside a node is
//! deterministically ordered at its own source: requirements are a `BTreeSet`, sources a
//! `BTreeSet` flattened to a `Vec`, partition groups come from
//! [`GrammarSemantics::entry_partition`] (which sorts by gate key precisely because
//! `gate::partition_entries` returns `HashMap` order) with members sorted, and rule order is the
//! authored cascade order.
//!
//! The single chain is NOT a plan-shape choice. Wave 3 measured plan-shape permutation to vary
//! nothing observable, so there is exactly one order and no permutation of it is representable.
//!
//! # What this module does NOT do
//! It does not construct an `ExecutableCandidate`, register anything, or feed selection -- the
//! recipe registry owns that, and nothing here is called from any routing, applicability or
//! candidate path. Deriving a graph changes no outcome; it only makes one describable.
//!
//! It also does not split mechanisms per stratum. Placing a rule-located observation in a stratum
//! needs a rule -> stratum map that `GrammarSemantics` does not own, and inventing one here would
//! mean re-walking `Grammar` -- exactly what this module's one rule (no `&Grammar` access) forbids.
//! Every derived node is therefore grammar-wide (`stratum: None`), which the vocabulary already
//! models and which the edge check
//! treats as compatible with anything. A later slice that teaches `GrammarSemantics` the map can
//! split them without changing this file's contract.

use std::collections::{BTreeMap, BTreeSet};

use crate::capability::CharacteristicKind;
use crate::grammar_semantics::GrammarSemantics;
use crate::recipe_mechanism::{
    mechanism_kind_for, BoundaryCleanupSpec, MechanismBody, MechanismEdge, MechanismGraph,
    MechanismId, MechanismKind, MechanismNode, MechanismSource, MorphotacticsSpec,
    OrderedPhonologySpec, PartitionGroupSpec, SymbolSpace,
};

/// One mechanism's accumulated evidence, before it becomes a node.
#[derive(Default)]
struct Accumulator {
    requirements: BTreeSet<CharacteristicKind>,
    sources: BTreeSet<MechanismSource>,
}

/// Derive the mechanism graph for the grammar `semantics` describes.
///
/// Pure, deterministic, and a function of `semantics` alone (module doc). Returns an empty graph
/// for a grammar whose profile observes nothing -- an empty graph is the honest answer, not an
/// error, and it validates.
pub fn derive_mechanism_graph(semantics: &GrammarSemantics<'_>) -> MechanismGraph {
    let Some(table) = semantics.primary_table() else {
        // No character-definition table means no symbol space, so no mechanism can be placed.
        return MechanismGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
        };
    };
    let symbol_space = SymbolSpace::Surface(table.into());

    // Attribute every observed construct to a mechanism. Both halves come from the observation
    // itself: `kind` decides which mechanism owns it, `location` becomes the typed source.
    let mut accumulated: BTreeMap<MechanismKind, Accumulator> = BTreeMap::new();
    for observation in semantics.characteristics().observations() {
        let entry = accumulated
            .entry(mechanism_kind_for(observation.kind))
            .or_default();
        entry.requirements.insert(observation.kind);
        entry.sources.insert(MechanismSource::from(&observation.location));
    }

    // A terminal cleanup exists whenever any other mechanism does: it is what consumes the boundary
    // symbols those mechanisms needed to see (the cleanup dossier's scope). Its source is the
    // character table it cleans, which is why `MechanismSourceKind::CharacterTable` exists -- a
    // cleanup is not derived from an observed construct, and a source-less node is rejected.
    if !accumulated.is_empty() {
        accumulated
            .entry(MechanismKind::BoundaryCleanup)
            .or_default()
            .sources
            .insert(MechanismSource::character_table(table));
    }

    let mut nodes = Vec::new();
    for &kind in MechanismKind::COMPOSITION_ORDER {
        let Some(accumulator) = accumulated.remove(&kind) else {
            continue;
        };
        nodes.push(MechanismNode {
            id: MechanismId(kind.label().to_owned()),
            sources: accumulator.sources.into_iter().collect(),
            symbol_space: symbol_space.clone(),
            stratum: None,
            construct_requirements: accumulator.requirements,
            body: body_for(kind, semantics),
        });
    }
    debug_assert!(
        accumulated.is_empty(),
        "COMPOSITION_ORDER must list every MechanismKind"
    );

    // The canonical spine: consecutive present mechanisms, in composition order.
    let edges = nodes
        .windows(2)
        .map(|pair| MechanismEdge {
            producer: pair[0].id.clone(),
            consumer: pair[1].id.clone(),
        })
        .collect();

    MechanismGraph { nodes, edges }
}

/// The per-kind body, projected from `semantics`. Every field is a fact the owner already holds;
/// nothing here inspects a `Grammar`.
fn body_for(kind: MechanismKind, semantics: &GrammarSemantics<'_>) -> MechanismBody {
    match kind {
        MechanismKind::StaticPartition => MechanismBody::StaticPartition(
            semantics
                .entry_partition()
                .iter()
                .map(|group| {
                    let mut members: Vec<crate::recipe_mechanism::WireModelId> =
                        group.entries.iter().copied().map(Into::into).collect();
                    // `SemanticEntryGroup::entries` is a `HashSet`; the groups themselves are
                    // already sorted by gate key by the owner, but membership order is not.
                    members.sort();
                    PartitionGroupSpec {
                        key: group.key.clone(),
                        members,
                    }
                })
                .collect(),
        ),
        MechanismKind::Morphotactics => MechanismBody::Morphotactics(MorphotacticsSpec {
            templates: semantics
                .template_ids()
                .iter()
                .copied()
                .map(Into::into)
                .collect(),
            // Honestly `None` today: `GrammarCardinality::max_derivation_chain_depth`'s own doc
            // requires a documented absence over an invented estimate.
            max_depth: semantics.characteristics().cardinality.max_derivation_chain_depth,
        }),
        MechanismKind::OrderedPhonology => MechanismBody::OrderedPhonology(OrderedPhonologySpec {
            rule_order: semantics
                .prule_ids_in_order()
                .iter()
                .copied()
                .map(Into::into)
                .collect(),
        }),
        MechanismKind::BoundaryCleanup => MechanismBody::BoundaryCleanup(BoundaryCleanupSpec {
            boundary_symbols: semantics.primary_table_boundary_symbols().to_vec(),
        }),
        MechanismKind::StructuralAllomorph => MechanismBody::StructuralAllomorph,
        MechanismKind::CopyProcess => MechanismBody::CopyProcess,
    }
}
