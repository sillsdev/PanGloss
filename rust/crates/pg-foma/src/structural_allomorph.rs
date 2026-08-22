//! Bounded structural-allomorph classification and legacy lowering for templated morphology.
//!
//! The classifier proves source ownership, finite input members, and supported rewrite shapes.
//! Caller-zoned recipes retain their typed binding input; only the legacy staged marker path
//! allocates a marker here. Circumfix text remains a direct, marker-free lowering.

use foma::constructions::{fsm_compose, fsm_union, fsm_universal};
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::types::Fsm;
use pg_grammar::chardef::{CharDef, CharDefId, CharDefKind, CharDefTable};
use pg_grammar::model::{
    AffixAllomorphDef, AllomorphId, AllomorphOwner, Grammar, MorphRuleDef, NaturalClassKind,
    OutputAction, PartRef, Pattern, PatternNode, PhonRuleDef, SimpleContext, TableId,
};
use pg_shape::{NodeKind, Shape};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::replace::SegAlphabet;

const PUA_A_BASE: u32 = 0xF0000;
const PUA_A_LAST: u32 = 0xFFFFD;
const PUA_B_BASE: u32 = 0x100000;
const PUA_B_LAST: u32 = 0x10FFFD;
const MARKER_PAIRS_PER_RANGE: u64 = ((PUA_A_LAST - PUA_A_BASE + 1) / 2) as u64;

// Bounds semantic probing; this path does not construct production FSTs.
const RELATION_PROBE_WORK_CAP: usize = 1_000_000;
const RELATION_PROBE_OUTPUT_CAP: usize = 10_000;
const RELATION_PROBE_OUTPUT_BYTES_CAP: usize = 1_000_000;

fn is_technical_marker(ch: char) -> bool {
    let code = ch as u32;
    (PUA_A_BASE..=PUA_A_LAST).contains(&code) || (PUA_B_BASE..=PUA_B_LAST).contains(&code)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkerZone {
    Prefix,
    Suffix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneRequirement {
    Caller,
    Intrinsic(MarkerZone),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerKey {
    pub allomorph: AllomorphId,
    pub zone: MarkerZone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerBinding {
    pub key: MarkerKey,
    pub symbol: char,
    /// The placement is exposed directly for callers which do not need to unpack `key`.
    /// `key` remains the identity of a binding; this is deliberately not a second namespace.
    pub zone: MarkerZone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewriteProvenance {
    pub allomorph: AllomorphId,
    pub source_table: TableId,
    pub active_table: TableId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerBindingError {
    IntrinsicZoneMismatch {
        required: MarkerZone,
        actual: MarkerZone,
    },
    InvalidScalar,
}

/// Allocate a marker only inside the two closed supplementary PUA ranges. Prefix and suffix
/// bindings intentionally occupy different slots for one allomorph; the Unicode noncharacter
/// gaps at the end of each range are never allocated.
pub fn marker_binding_for(
    key: MarkerKey,
    requirement: ZoneRequirement,
) -> Result<MarkerBinding, MarkerBindingError> {
    if let ZoneRequirement::Intrinsic(required) = requirement {
        if required != key.zone {
            return Err(MarkerBindingError::IntrinsicZoneMismatch {
                required,
                actual: key.zone,
            });
        }
    }
    let pair = key.allomorph.0 as u64;
    let (base, pair) = if pair < MARKER_PAIRS_PER_RANGE {
        (PUA_A_BASE as u64, pair)
    } else if pair < MARKER_PAIRS_PER_RANGE * 2 {
        (PUA_B_BASE as u64, pair - MARKER_PAIRS_PER_RANGE)
    } else {
        return Err(MarkerBindingError::InvalidScalar);
    };
    let zone_offset = match key.zone {
        MarkerZone::Prefix => 0,
        MarkerZone::Suffix => 1,
    };
    let code = base
        .checked_add(
            pair.checked_mul(2)
                .ok_or(MarkerBindingError::InvalidScalar)?,
        )
        .and_then(|value| value.checked_add(zone_offset))
        .ok_or(MarkerBindingError::InvalidScalar)?;
    let symbol =
        char::from_u32(u32::try_from(code).map_err(|_| MarkerBindingError::InvalidScalar)?)
            .ok_or(MarkerBindingError::InvalidScalar)?;
    Ok(MarkerBinding {
        key,
        symbol,
        zone: key.zone,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MorphologyRewrite {
    OrdinaryLiteral {
        variants: Vec<String>,
        provenance: RewriteProvenance,
    },
    DirectWholeRootWrapper {
        prefix_variants: Vec<String>,
        suffix_variants: Vec<String>,
        provenance: RewriteProvenance,
    },
    MarkedStructural {
        shape_id: &'static str,
        recipe: MorphologyRecipe,
        zone_requirement: ZoneRequirement,
        provenance: RewriteProvenance,
    },
    Unsupported {
        shape_id: &'static str,
        reason_id: &'static str,
        allomorph: AllomorphId,
        source_table: TableId,
        active_table: TableId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorphologyRecipe {
    refs: Vec<u16>,
    literal_runs: Vec<Vec<String>>,
    output_segments: Vec<String>,
    translated_input_members: Vec<Vec<String>>,
}

impl MorphologyRecipe {
    pub fn input_refs(&self) -> Vec<u16> {
        self.refs.clone()
    }

    pub fn literal_runs(&self) -> Vec<Vec<String>> {
        self.literal_runs.clone()
    }

    pub fn output_segments(&self) -> Vec<String> {
        self.output_segments.clone()
    }

    pub fn translated_input_members(&self) -> Vec<Vec<String>> {
        self.translated_input_members.clone()
    }
}

/// The result of probing the classifier-owned morphology relation.
///
/// This is intentionally a small, deterministic relation model.  It is used while the
/// morphology relation is being assembled and tested; it does not claim that a later Foma
/// network has been emitted.  In particular, marker-free input has one explicit identity branch,
/// while every marked input must select exactly one known recipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MorphologyRelationResult {
    Identity {
        outputs: BTreeSet<String>,
        consumed_markers: usize,
    },
    Recipe {
        allomorph: AllomorphId,
        zone: MarkerZone,
        shape_id: &'static str,
        outputs: BTreeSet<String>,
        consumed_markers: usize,
    },
    Rejected {
        reason_id: &'static str,
        consumed_markers: usize,
    },
    ResourceRejected {
        reason_id: &'static str,
        consumed_markers: usize,
        work: usize,
        outputs: usize,
    },
}

/// A classifier result together with the chain zone chosen by the caller.
///
/// The tuple form `(MorphologyRewrite, MarkerZone)` is accepted by the construction API as
/// well.  Keeping this value separate from `MorphologyRewrite` makes it impossible for the
/// relation to infer a zone from a role or from the first allomorph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedMorphologyInput {
    pub rewrite: MorphologyRewrite,
    pub zone: MarkerZone,
}

pub trait IntoClassifiedMorphologyInput {
    fn into_classified_morphology_input(self) -> ClassifiedMorphologyInput;
}

impl IntoClassifiedMorphologyInput for ClassifiedMorphologyInput {
    fn into_classified_morphology_input(self) -> ClassifiedMorphologyInput {
        self
    }
}

impl IntoClassifiedMorphologyInput for (MorphologyRewrite, MarkerZone) {
    fn into_classified_morphology_input(self) -> ClassifiedMorphologyInput {
        ClassifiedMorphologyInput {
            rewrite: self.0,
            zone: self.1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MorphologyRelationError {
    UnsupportedRewrite {
        allomorph: AllomorphId,
        shape_id: &'static str,
        reason_id: &'static str,
    },
    ZoneMismatch {
        allomorph: AllomorphId,
        required: MarkerZone,
        actual: MarkerZone,
    },
    DuplicateBinding {
        allomorph: AllomorphId,
        zone: MarkerZone,
    },
    InvalidMarker(MarkerBindingError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerInputError {
    MissingBinding {
        allomorph: AllomorphId,
    },
    AmbiguousBinding {
        allomorph: AllomorphId,
    },
    MissingZoneBinding {
        allomorph: AllomorphId,
        zone: MarkerZone,
    },
}

impl fmt::Display for MarkerInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBinding { allomorph } => {
                write!(f, "no marker binding for {allomorph:?}")
            }
            Self::AmbiguousBinding { allomorph } => {
                write!(f, "ambiguous marker binding for {allomorph:?}")
            }
            Self::MissingZoneBinding { allomorph, zone } => {
                write!(f, "no marker binding for {allomorph:?}/{zone:?}")
            }
        }
    }
}

impl std::error::Error for MarkerInputError {}

impl fmt::Display for MorphologyRelationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedRewrite {
                allomorph,
                shape_id,
                reason_id,
            } => write!(
                f,
                "unsupported morphology rewrite for {allomorph:?}: {shape_id}/{reason_id}"
            ),
            Self::ZoneMismatch {
                allomorph,
                required,
                actual,
            } => write!(
                f,
                "morphology marker zone mismatch for {allomorph:?}: required {required:?}, got {actual:?}"
            ),
            Self::DuplicateBinding { allomorph, zone } => write!(
                f,
                "duplicate morphology marker binding for {allomorph:?}/{zone:?}"
            ),
            Self::InvalidMarker(error) => write!(f, "invalid morphology marker: {error:?}"),
        }
    }
}

impl std::error::Error for MorphologyRelationError {}

#[derive(Debug)]
struct RelationRecipe {
    binding: MarkerBinding,
    shape_id: &'static str,
    recipe: MorphologyRecipe,
    fired: Arc<AtomicUsize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProbeBudgetReached {
    reason_id: &'static str,
    work: usize,
    outputs: usize,
}

#[derive(Debug, Clone, Copy)]
struct ProbeBudget {
    work: usize,
    outputs: usize,
    output_bytes: usize,
}

impl ProbeBudget {
    fn new() -> Self {
        Self {
            work: 0,
            outputs: 0,
            output_bytes: 0,
        }
    }

    fn charge_work(&mut self, units: usize) -> Result<(), ProbeBudgetReached> {
        let units = units.max(1);
        if units > RELATION_PROBE_WORK_CAP.saturating_sub(self.work) {
            self.work = RELATION_PROBE_WORK_CAP;
            return Err(ProbeBudgetReached {
                reason_id: "probe-work-budget",
                work: self.work,
                outputs: self.outputs,
            });
        }
        self.work += units;
        Ok(())
    }

    fn work(&mut self) -> Result<(), ProbeBudgetReached> {
        self.charge_work(1)
    }

    fn output(&mut self) -> Result<(), ProbeBudgetReached> {
        if self.outputs >= RELATION_PROBE_OUTPUT_CAP {
            return Err(ProbeBudgetReached {
                reason_id: "probe-output-budget",
                work: self.work,
                outputs: self.outputs,
            });
        }
        self.outputs += 1;
        Ok(())
    }

    fn charge_output_bytes(&mut self, units: usize) -> Result<(), ProbeBudgetReached> {
        let units = units.max(1);
        if units > RELATION_PROBE_OUTPUT_BYTES_CAP.saturating_sub(self.output_bytes) {
            self.output_bytes = RELATION_PROBE_OUTPUT_BYTES_CAP;
            return Err(ProbeBudgetReached {
                reason_id: "probe-output-bytes",
                work: self.work,
                outputs: self.outputs,
            });
        }
        self.output_bytes += units;
        Ok(())
    }
}

fn resource_rejected(reached: ProbeBudgetReached) -> MorphologyRelationResult {
    MorphologyRelationResult::ResourceRejected {
        reason_id: reached.reason_id,
        consumed_markers: 0,
        work: reached.work,
        outputs: reached.outputs,
    }
}

impl RelationRecipe {
    fn clone_for_relation(&self) -> Self {
        Self {
            binding: self.binding,
            shape_id: self.shape_id,
            recipe: self.recipe.clone(),
            fired: Arc::clone(&self.fired),
        }
    }
}

/// The classifier-owned, marker-dispatched morphology relation.
///
/// Construction consumes already-classified allomorphs and their caller-selected zones.  It
/// allocates one checked technical marker per unique `(AllomorphId, MarkerZone)` pair and refuses
/// duplicate allocation.  No Foma objects are created here: this relation is the semantic
/// foundation and deterministic probe used by the later compiler integration.
#[derive(Debug)]
pub struct CompiledMorphologyRelation {
    recipes: Vec<RelationRecipe>,
    by_binding: HashMap<(AllomorphId, MarkerZone), usize>,
    by_allomorph: HashMap<AllomorphId, Vec<usize>>,
}

impl Clone for CompiledMorphologyRelation {
    fn clone(&self) -> Self {
        let recipes = self
            .recipes
            .iter()
            .map(RelationRecipe::clone_for_relation)
            .collect();
        Self {
            recipes,
            by_binding: self.by_binding.clone(),
            by_allomorph: self.by_allomorph.clone(),
        }
    }
}

impl CompiledMorphologyRelation {
    pub fn from_classified<I, T>(inputs: I) -> Result<Self, MorphologyRelationError>
    where
        I: IntoIterator<Item = T>,
        T: IntoClassifiedMorphologyInput,
    {
        let mut relation = Self {
            recipes: Vec::new(),
            by_binding: HashMap::new(),
            by_allomorph: HashMap::new(),
        };
        for input in inputs {
            let ClassifiedMorphologyInput { rewrite, zone } =
                input.into_classified_morphology_input();
            let MorphologyRewrite::MarkedStructural {
                shape_id,
                recipe,
                zone_requirement,
                provenance,
            } = rewrite
            else {
                // Keep ordinary literals and direct wrappers on the marker-free identity branch.
                if matches!(rewrite, MorphologyRewrite::Unsupported { .. }) {
                    if let MorphologyRewrite::Unsupported {
                        shape_id,
                        reason_id,
                        allomorph,
                        ..
                    } = rewrite
                    {
                        return Err(MorphologyRelationError::UnsupportedRewrite {
                            allomorph,
                            shape_id,
                            reason_id,
                        });
                    }
                }
                continue;
            };
            let required = match zone_requirement {
                ZoneRequirement::Caller => None,
                ZoneRequirement::Intrinsic(required) => Some(required),
            };
            if let Some(required) = required {
                if required != zone {
                    return Err(MorphologyRelationError::ZoneMismatch {
                        allomorph: provenance.allomorph,
                        required,
                        actual: zone,
                    });
                }
            }
            let key = (provenance.allomorph, zone);
            if relation.by_binding.contains_key(&key) {
                return Err(MorphologyRelationError::DuplicateBinding {
                    allomorph: provenance.allomorph,
                    zone,
                });
            }
            let binding = marker_binding_for(
                MarkerKey {
                    allomorph: provenance.allomorph,
                    zone,
                },
                zone_requirement,
            )
            .map_err(MorphologyRelationError::InvalidMarker)?;
            let index = relation.recipes.len();
            relation.recipes.push(RelationRecipe {
                binding,
                shape_id,
                recipe,
                fired: Arc::new(AtomicUsize::new(0)),
            });
            relation.by_binding.insert(key, index);
            relation
                .by_allomorph
                .entry(provenance.allomorph)
                .or_default()
                .push(index);
        }
        Ok(relation)
    }

    pub fn new<I, T>(inputs: I) -> Result<Self, MorphologyRelationError>
    where
        I: IntoIterator<Item = T>,
        T: IntoClassifiedMorphologyInput,
    {
        Self::from_classified(inputs)
    }

    pub fn from_classified_rewrites<I, T>(inputs: I) -> Result<Self, MorphologyRelationError>
    where
        I: IntoIterator<Item = T>,
        T: IntoClassifiedMorphologyInput,
    {
        Self::from_classified(inputs)
    }

    pub fn marker_binding_for(&self, allomorph: AllomorphId) -> Option<MarkerBinding> {
        let indices = self.by_allomorph.get(&allomorph)?;
        if indices.len() != 1 {
            return None;
        }
        self.recipes.get(indices[0]).map(|recipe| recipe.binding)
    }

    pub fn marker_binding_for_zone(
        &self,
        allomorph: AllomorphId,
        zone: MarkerZone,
    ) -> Option<MarkerBinding> {
        self.by_binding
            .get(&(allomorph, zone))
            .and_then(|index| self.recipes.get(*index))
            .map(|recipe| recipe.binding)
    }

    pub fn marked_input(
        &self,
        allomorph: AllomorphId,
        base_tokens: &str,
    ) -> Result<String, MarkerInputError> {
        let Some(indices) = self.by_allomorph.get(&allomorph) else {
            return Err(MarkerInputError::MissingBinding { allomorph });
        };
        if indices.len() != 1 {
            return Err(MarkerInputError::AmbiguousBinding { allomorph });
        }
        Ok(self.marked_input_with_binding(self.recipes[indices[0]].binding, base_tokens))
    }

    pub fn marked_input_for_zone(
        &self,
        allomorph: AllomorphId,
        zone: MarkerZone,
        base_tokens: &str,
    ) -> Result<String, MarkerInputError> {
        let Some(binding) = self.marker_binding_for_zone(allomorph, zone) else {
            return Err(MarkerInputError::MissingZoneBinding { allomorph, zone });
        };
        Ok(self.marked_input_with_binding(binding, base_tokens))
    }

    fn marked_input_with_binding(&self, binding: MarkerBinding, base_tokens: &str) -> String {
        match binding.zone {
            MarkerZone::Prefix => format!("{}{}", binding.symbol, base_tokens),
            MarkerZone::Suffix => format!("{}{}", base_tokens, binding.symbol),
        }
    }

    pub fn fired_recipe_count(&self) -> usize {
        self.recipes
            .iter()
            .map(|recipe| recipe.fired.load(Ordering::Relaxed))
            .sum()
    }

    pub fn fired_recipe_count_for(&self, allomorph: AllomorphId, zone: MarkerZone) -> usize {
        self.marker_binding_for_zone(allomorph, zone)
            .and_then(|binding| {
                self.by_binding
                    .get(&(binding.key.allomorph, binding.key.zone))
                    .and_then(|index| self.recipes.get(*index))
            })
            .map_or(0, |recipe| recipe.fired.load(Ordering::Relaxed))
    }

    /// Probe the semantic relation without constructing a Foma network.
    ///
    /// This method uses closed, nonconfigurable 1,000,000-work/10,000-output and 1,000,000-byte
    /// probe budgets; callers cannot supply raw limits. If a budget is reached,
    /// `ResourceRejected` reports the observed counters and no partial output or recipe fire is
    /// returned. A resource refusal has `consumed_markers: 0` because recipe application did not
    /// commit.
    pub fn apply(&self, input: &str) -> MorphologyRelationResult {
        let mut budget = ProbeBudget::new();
        let mut marker_symbols = Vec::new();
        let mut marker_positions = Vec::new();
        let mut base = String::new();
        let mut input_char_count = 0;
        for (position, ch) in input.chars().enumerate() {
            input_char_count = position + 1;
            if let Err(reached) = budget.charge_work(ch.len_utf8()) {
                return resource_rejected(reached);
            }
            if is_technical_marker(ch) {
                marker_symbols.push(ch);
                marker_positions.push(position);
            } else {
                base.push(ch);
            }
        }
        match marker_symbols.as_slice() {
            [] => match budget.charge_output_bytes(input.len()) {
                Ok(()) => MorphologyRelationResult::Identity {
                    outputs: BTreeSet::from([input.to_owned()]),
                    consumed_markers: 0,
                },
                Err(reached) => resource_rejected(reached),
            },
            [symbol] => {
                let mut recipe = None;
                for candidate in &self.recipes {
                    if let Err(reached) = budget.work() {
                        return resource_rejected(reached);
                    }
                    if candidate.binding.symbol == *symbol {
                        recipe = Some(candidate);
                        break;
                    }
                }
                let Some(recipe) = recipe else {
                    return MorphologyRelationResult::Rejected {
                        reason_id: "foreign-marker",
                        consumed_markers: 0,
                    };
                };
                let marker_position = marker_positions[0];
                let expected_position = match recipe.binding.zone {
                    MarkerZone::Prefix => 0,
                    MarkerZone::Suffix => input_char_count.saturating_sub(1),
                };
                if marker_position != expected_position {
                    return MorphologyRelationResult::Rejected {
                        reason_id: "zone-mismatch",
                        consumed_markers: 0,
                    };
                }
                let outputs = match apply_recipe(recipe, &base, &mut budget) {
                    Ok(outputs) if !outputs.is_empty() => outputs,
                    Ok(_) => {
                        return MorphologyRelationResult::Rejected {
                            reason_id: "recipe-input-mismatch",
                            consumed_markers: 0,
                        };
                    }
                    Err(reached) => {
                        return resource_rejected(reached);
                    }
                };
                recipe.fired.fetch_add(1, Ordering::Relaxed);
                MorphologyRelationResult::Recipe {
                    allomorph: recipe.binding.key.allomorph,
                    zone: recipe.binding.zone,
                    shape_id: recipe.shape_id,
                    outputs,
                    consumed_markers: 1,
                }
            }
            symbols
                if {
                    let mut seen = HashSet::new();
                    symbols.iter().any(|symbol| !seen.insert(*symbol))
                } =>
            {
                MorphologyRelationResult::Rejected {
                    reason_id: "duplicate-marker",
                    consumed_markers: 0,
                }
            }
            _ => MorphologyRelationResult::Rejected {
                reason_id: "multiple-markers",
                consumed_markers: 0,
            },
        }
    }
}

fn apply_recipe(
    recipe: &RelationRecipe,
    input: &str,
    budget: &mut ProbeBudget,
) -> Result<BTreeSet<String>, ProbeBudgetReached> {
    let mut outputs = BTreeSet::new();
    let member_sets = &recipe.recipe.translated_input_members;
    match recipe.shape_id {
        "AdjacentTerminalDrop" => {
            let Some(final_members) = member_sets.get(1) else {
                return Ok(outputs);
            };
            for member in final_members {
                budget.charge_work(member.len().max(1))?;
                if member.is_empty() || !input.ends_with(member) || input.len() == member.len() {
                    continue;
                }
                let prefix = &input[..input.len() - member.len()];
                for literal in recipe.recipe.literal_runs.first().into_iter().flatten() {
                    let output = build_probe_text(&[prefix, literal], budget)?;
                    insert_probe_output(&mut outputs, output, budget)?;
                }
            }
        }
        "AdjacentInitialDrop" => {
            let Some(initial_members) = member_sets.first() else {
                return Ok(outputs);
            };
            for member in initial_members {
                budget.charge_work(member.len().max(1))?;
                if member.is_empty() || !input.starts_with(member) || input.len() == member.len() {
                    continue;
                }
                let output = build_probe_text(&[&input[member.len()..]], budget)?;
                insert_probe_output(&mut outputs, output, budget)?;
            }
        }
        "AmharicInitialVowelReplacement" => {
            let Some(initial_members) = member_sets.first() else {
                return Ok(outputs);
            };
            for member in initial_members {
                budget.charge_work(member.len().max(1))?;
                if member.is_empty() || !input.starts_with(member) || input.len() == member.len() {
                    continue;
                }
                let remainder = &input[member.len()..];
                for literal in recipe.recipe.literal_runs.first().into_iter().flatten() {
                    let output = build_probe_text(&[literal, remainder], budget)?;
                    insert_probe_output(&mut outputs, output, budget)?;
                }
            }
        }
        "AmharicTerminalModify" => {
            let Some(final_members) = member_sets.last() else {
                return Ok(outputs);
            };
            for (start, _) in input.char_indices() {
                budget.work()?;
                for member in final_members {
                    budget.charge_work(member.len().max(1))?;
                    if member.is_empty() || !input[start..].starts_with(member) {
                        continue;
                    }
                    let end = start + member.len();
                    let prefix = &input[..start];
                    let suffix = &input[end..];
                    for replacement in &recipe.recipe.output_segments {
                        let output = build_probe_text(&[prefix, replacement, suffix], budget)?;
                        insert_probe_output(&mut outputs, output, budget)?;
                    }
                }
            }
        }
        "AmharicInteriorInsertion" => {
            let refs = recipe.recipe.refs.len();
            visit_segmentations(input, member_sets, budget, &mut |tokens, budget| {
                visit_partitions(
                    tokens,
                    refs,
                    budget,
                    &mut Vec::new(),
                    &mut |parts, budget| {
                        let mut variants = vec![String::new()];
                        for (position, part) in parts.iter().enumerate() {
                            let joined = join_tokens_bounded(part, budget)?;
                            for variant in &mut variants {
                                budget.charge_output_bytes(joined.len())?;
                                variant.push_str(&joined);
                            }
                            if let Some(run) = recipe.recipe.literal_runs.get(position) {
                                if !run.is_empty() {
                                    let mut next = Vec::new();
                                    for variant in &variants {
                                        for literal in run {
                                            budget.work()?;
                                            next.push(build_probe_text(
                                                &[variant, literal],
                                                budget,
                                            )?);
                                        }
                                    }
                                    variants = next;
                                }
                            }
                        }
                        for variant in variants {
                            insert_probe_output(&mut outputs, variant, budget)?;
                        }
                        Ok(())
                    },
                )
            })?;
        }
        _ => {}
    }
    Ok(outputs)
}

fn insert_probe_output(
    outputs: &mut BTreeSet<String>,
    output: String,
    budget: &mut ProbeBudget,
) -> Result<(), ProbeBudgetReached> {
    let lookup_cost = ceil_log2(outputs.len().saturating_add(1));
    budget.charge_work(lookup_cost)?;
    if outputs.contains(&output) {
        budget.work()?;
    } else {
        budget.output()?;
        outputs.insert(output);
    }
    Ok(())
}

fn ceil_log2(value: usize) -> usize {
    let value = value.max(2);
    (usize::BITS - (value - 1).leading_zeros()) as usize
}

fn build_probe_text(
    parts: &[&str],
    budget: &mut ProbeBudget,
) -> Result<String, ProbeBudgetReached> {
    let byte_len = parts
        .iter()
        .fold(0usize, |total, part| total.saturating_add(part.len()));
    budget.charge_output_bytes(byte_len)?;
    let mut output = String::with_capacity(byte_len);
    for part in parts {
        output.push_str(part);
    }
    Ok(output)
}

fn join_tokens_bounded(
    tokens: &[String],
    budget: &mut ProbeBudget,
) -> Result<String, ProbeBudgetReached> {
    let byte_len = tokens
        .iter()
        .fold(0usize, |total, token| total.saturating_add(token.len()));
    budget.charge_output_bytes(byte_len)?;
    let mut output = String::with_capacity(byte_len);
    for token in tokens {
        output.push_str(token);
    }
    Ok(output)
}

// Enumerate scalar fallback and member paths under the closed probe budget.
fn visit_segmentations<F>(
    input: &str,
    member_sets: &[Vec<String>],
    budget: &mut ProbeBudget,
    callback: &mut F,
) -> Result<(), ProbeBudgetReached>
where
    F: FnMut(&[String], &mut ProbeBudget) -> Result<(), ProbeBudgetReached>,
{
    let mut all_members = Vec::new();
    for member in member_sets.iter().flat_map(|members| members.iter()) {
        budget.charge_work(member.len().max(1))?;
        if !member.is_empty() {
            all_members.push(member.clone());
        }
    }
    budget.charge_work(all_members.len().max(1))?;
    let sort_cost = all_members
        .len()
        .saturating_mul(ceil_log2(all_members.len()))
        .saturating_add(all_members.len())
        .max(1);
    budget.charge_work(sort_cost)?;
    all_members.sort();
    all_members.dedup();

    fn visit<F>(
        input: &str,
        offset: usize,
        members: &[String],
        current: &mut Vec<String>,
        budget: &mut ProbeBudget,
        callback: &mut F,
    ) -> Result<(), ProbeBudgetReached>
    where
        F: FnMut(&[String], &mut ProbeBudget) -> Result<(), ProbeBudgetReached>,
    {
        budget.work()?;
        if offset == input.len() {
            return callback(current, budget);
        }
        for member in members {
            budget.charge_work(member.len().max(1))?;
            if input[offset..].starts_with(member) {
                current.push(member.clone());
                visit(
                    input,
                    offset + member.len(),
                    members,
                    current,
                    budget,
                    callback,
                )?;
                current.pop();
            }
        }
        if let Some(ch) = input[offset..].chars().next() {
            let end = offset + ch.len_utf8();
            let scalar = &input[offset..end];
            budget.charge_work(scalar.len().max(1))?;
            current.push(scalar.to_owned());
            visit(input, end, members, current, budget, callback)?;
            current.pop();
        }
        Ok(())
    }

    visit(input, 0, &all_members, &mut Vec::new(), budget, callback)
}

fn visit_partitions<F>(
    tokens: &[String],
    parts: usize,
    budget: &mut ProbeBudget,
    current: &mut Vec<Vec<String>>,
    callback: &mut F,
) -> Result<(), ProbeBudgetReached>
where
    F: FnMut(&[Vec<String>], &mut ProbeBudget) -> Result<(), ProbeBudgetReached>,
{
    if parts == 0 || tokens.len() < parts {
        return Ok(());
    }
    fn visit<F>(
        tokens: &[String],
        parts_left: usize,
        offset: usize,
        current: &mut Vec<Vec<String>>,
        budget: &mut ProbeBudget,
        callback: &mut F,
    ) -> Result<(), ProbeBudgetReached>
    where
        F: FnMut(&[Vec<String>], &mut ProbeBudget) -> Result<(), ProbeBudgetReached>,
    {
        budget.work()?;
        if parts_left == 1 {
            charge_token_slice_copy(&tokens[offset..], budget)?;
            current.push(tokens[offset..].to_vec());
            callback(current, budget)?;
            current.pop();
            return Ok(());
        }
        let max_end = tokens.len() - (parts_left - 1);
        for end in (offset + 1)..=max_end {
            charge_token_slice_copy(&tokens[offset..end], budget)?;
            current.push(tokens[offset..end].to_vec());
            visit(tokens, parts_left - 1, end, current, budget, callback)?;
            current.pop();
        }
        Ok(())
    }

    visit(tokens, parts, 0, current, budget, callback)
}

fn charge_token_slice_copy(
    tokens: &[String],
    budget: &mut ProbeBudget,
) -> Result<(), ProbeBudgetReached> {
    let bytes = tokens
        .iter()
        .fold(0usize, |total, token| total.saturating_add(token.len()));
    budget.charge_work(tokens.len().saturating_add(bytes).max(1))
}

pub struct MorphologyRewriteClassifier;

impl MorphologyRewriteClassifier {
    pub fn classify(
        grammar: &Grammar,
        allomorph: &AffixAllomorphDef,
        active_table: TableId,
    ) -> MorphologyRewrite {
        let Some(source_table) = owning_source_table(grammar, allomorph).ok() else {
            return MorphologyRewrite::Unsupported {
                shape_id: "InvalidReferences",
                reason_id: "invalid-allomorph-owner",
                allomorph: allomorph.id,
                source_table: active_table,
                active_table,
            };
        };
        Self::classify_with_tables(grammar, allomorph, source_table, active_table)
    }

    pub fn classify_with_tables(
        grammar: &Grammar,
        allomorph: &AffixAllomorphDef,
        source_table: TableId,
        active_table: TableId,
    ) -> MorphologyRewrite {
        let owner_source = match owning_source_table(grammar, allomorph) {
            Ok(table) => table,
            Err(reason_id) => {
                return MorphologyRewrite::Unsupported {
                    shape_id: "InvalidReferences",
                    reason_id,
                    allomorph: allomorph.id,
                    source_table,
                    active_table,
                }
            }
        };
        if owner_source != source_table {
            return MorphologyRewrite::Unsupported {
                shape_id: "InvalidReferences",
                reason_id: "source-table-mismatch",
                allomorph: allomorph.id,
                source_table,
                active_table,
            };
        }
        match classify_rewrite(grammar, allomorph, source_table, active_table) {
            Ok(result) => result,
            Err((shape_id, reason_id)) => MorphologyRewrite::Unsupported {
                shape_id,
                reason_id,
                allomorph: allomorph.id,
                source_table,
                active_table,
            },
        }
    }
}

fn owning_source_table(
    g: &Grammar,
    allomorph: &AffixAllomorphDef,
) -> Result<TableId, &'static str> {
    let Some(AllomorphOwner::Affix(mrule, index)) = g.allomorph_owners.get(allomorph.id.0 as usize)
    else {
        return Err("invalid-allomorph-owner");
    };
    let Some(rule) = g.mrules.get(mrule.0 as usize) else {
        return Err("invalid-allomorph-owner");
    };
    let Some(candidate) = rule
        .affix_allomorphs()
        .and_then(|allomorphs| allomorphs.get(*index as usize))
    else {
        return Err("invalid-allomorph-owner");
    };
    if candidate.id != allomorph.id || !std::ptr::eq(candidate, allomorph) {
        return Err("invalid-allomorph-owner");
    }
    let morpheme = match rule {
        MorphRuleDef::AffixProcess(def) => def.morpheme,
        MorphRuleDef::Realizational(def) => def.morpheme,
        MorphRuleDef::Compounding(_) => return Err("invalid-allomorph-owner"),
    };
    let stratum = g
        .morphemes
        .get(morpheme.0 as usize)
        .and_then(|info| g.strata.get(info.stratum.0 as usize))
        .ok_or("invalid-allomorph-owner")?;
    Ok(stratum.table)
}

type ClassifierResult<T> = Result<T, (&'static str, &'static str)>;

fn classify_rewrite(
    g: &Grammar,
    a: &AffixAllomorphDef,
    source_table: TableId,
    active_table: TableId,
) -> ClassifierResult<MorphologyRewrite> {
    if g.char_tables.get(source_table.0 as usize).is_none()
        || g.char_tables.get(active_table.0 as usize).is_none()
    {
        return Err(("InvalidReferences", "invalid-table-reference"));
    }
    if a.lhs.is_empty() {
        return Err(("UnlistedTopology", "missing-input-copy"));
    }
    for part in &a.lhs {
        if part.nodes.is_empty() {
            return Err(("UnlistedTopology", "empty-input-part"));
        }
        validate_pattern(g, source_table, part).map_err(|reason| ("InvalidReferences", reason))?;
    }

    if a.rhs
        .iter()
        .any(|action| matches!(action, OutputAction::InsertContext(_)))
    {
        return Err(("InsertContext", "insert-context"));
    }
    for action in &a.rhs {
        if let OutputAction::Modify(_, context) = action {
            validate_context_features(g, context)
                .map_err(|reason| ("InvalidReferences", reason))?;
        }
    }

    let provenance = RewriteProvenance {
        allomorph: a.id,
        source_table,
        active_table,
    };

    if a.rhs.is_empty()
        || a.rhs
            .iter()
            .all(|action| matches!(action, OutputAction::InsertSegments { .. }))
    {
        let variants = translated_literal_variants(g, &a.rhs, active_table)
            .map_err(|reason| ("OrdinaryLiteral", reason))?;
        return Ok(MorphologyRewrite::OrdinaryLiteral {
            variants,
            provenance,
        });
    }

    let refs = referenced_inputs(&a.rhs).map_err(|reason| ("InvalidReferences", reason))?;
    if refs.iter().any(|index| (*index as usize) >= a.lhs.len()) {
        return Err(("InvalidReferences", "invalid-input-reference"));
    }
    if refs.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(("UnlistedTopology", "repeated-input-reference"));
    }
    if refs.windows(2).any(|pair| pair[0] > pair[1]) {
        return Err(("UnlistedTopology", "reordered-input-reference"));
    }

    for index in &refs {
        let part = a
            .lhs
            .get(*index as usize)
            .ok_or(("InvalidReferences", "invalid-input-reference"))?;
        if minimum_consumed_segments(g, source_table, part)
            .map_err(|reason| ("InvalidReferences", reason))?
            == 0
        {
            return Err((shape_for_action(a, &refs), "non-consuming-input-part"));
        }
    }

    if a.rhs
        .iter()
        .any(|action| matches!(action, OutputAction::Modify(..)))
    {
        return classify_terminal_modify(g, a, source_table, active_table, &refs);
    }

    let expected = (0..a.lhs.len() as u16).collect::<Vec<_>>();
    if refs == expected {
        if let Some((prefix, suffix)) = wrapper_runs(g, &a.rhs, active_table, a.lhs.len())
            .map_err(|reason| ("DirectWholeRootWrapper", reason))?
        {
            return Ok(MorphologyRewrite::DirectWholeRootWrapper {
                prefix_variants: prefix,
                suffix_variants: suffix,
                provenance,
            });
        }
        if a.lhs.len() >= 2 {
            if a.lhs
                .iter()
                .any(|part| pattern_has_boundary(g, source_table, part))
            {
                return Err(("AmharicInteriorInsertion", "non-segment-input-atom"));
            }
            if let Some(runs) = interior_runs(g, &a.rhs, active_table, a.lhs.len())
                .map_err(|reason| ("AmharicInteriorInsertion", reason))?
            {
                return Ok(marked(
                    "AmharicInteriorInsertion",
                    refs,
                    runs,
                    Vec::new(),
                    input_members(g, source_table, active_table, &a.lhs)
                        .map_err(|reason| ("AmharicInteriorInsertion", reason))?,
                    ZoneRequirement::Caller,
                    provenance,
                ));
            }
        }
    }

    if a.lhs.len() == 2
        && a.rhs.len() >= 2
        && a.rhs.last() == Some(&OutputAction::Copy(PartRef::Input(1)))
        && is_fixed_segment_atom(g, source_table, a.lhs.first())
        && a.rhs[..a.rhs.len() - 1]
            .iter()
            .all(|action| matches!(action, OutputAction::InsertSegments { .. }))
    {
        if pattern_has_boundary(g, source_table, &a.lhs[0]) {
            return Err(("AmharicInitialVowelReplacement", "non-segment-input-atom"));
        }
        let variants = translated_literal_variants(g, &a.rhs[..a.rhs.len() - 1], active_table)
            .map_err(|reason| ("AmharicInitialVowelReplacement", reason))?;
        if variants != vec![String::new()] {
            return Ok(marked(
                "AmharicInitialVowelReplacement",
                vec![1],
                vec![variants],
                Vec::new(),
                input_members(g, source_table, active_table, &a.lhs)
                    .map_err(|reason| ("AmharicInitialVowelReplacement", reason))?,
                ZoneRequirement::Intrinsic(MarkerZone::Prefix),
                provenance,
            ));
        }
    }

    if a.lhs.len() == 2
        && refs == [0]
        && matches!(a.rhs.first(), Some(OutputAction::Copy(PartRef::Input(0))))
        && a.rhs[1..]
            .iter()
            .all(|action| matches!(action, OutputAction::InsertSegments { .. }))
    {
        if !is_segment_only_atom(g, source_table, a.lhs.get(1)) {
            return Err(("AdjacentTerminalDrop", "non-segment-input-atom"));
        }
        let variants = translated_literal_variants(g, &a.rhs[1..], active_table)
            .map_err(|reason| ("AdjacentTerminalDrop", reason))?;
        return Ok(marked(
            "AdjacentTerminalDrop",
            vec![0],
            vec![variants],
            Vec::new(),
            input_members(g, source_table, active_table, &a.lhs)
                .map_err(|reason| ("AdjacentTerminalDrop", reason))?,
            ZoneRequirement::Intrinsic(MarkerZone::Suffix),
            provenance,
        ));
    }
    if a.lhs.len() == 2 && refs == [1] && a.rhs == [OutputAction::Copy(PartRef::Input(1))] {
        if !is_segment_only_atom(g, source_table, a.lhs.first()) {
            return Err(("AdjacentInitialDrop", "non-segment-input-atom"));
        }
        return Ok(marked(
            "AdjacentInitialDrop",
            vec![1],
            Vec::new(),
            Vec::new(),
            input_members(g, source_table, active_table, &a.lhs)
                .map_err(|reason| ("AdjacentInitialDrop", reason))?,
            ZoneRequirement::Intrinsic(MarkerZone::Prefix),
            provenance,
        ));
    }

    if refs.len() != a.lhs.len() {
        return Err(("UnlistedTopology", "missing-input-copy"));
    }
    Err(("UnlistedTopology", "unlisted-topology"))
}

fn marked(
    shape_id: &'static str,
    refs: Vec<u16>,
    literal_runs: Vec<Vec<String>>,
    output_segments: Vec<String>,
    translated_input_members: Vec<Vec<String>>,
    zone_requirement: ZoneRequirement,
    provenance: RewriteProvenance,
) -> MorphologyRewrite {
    MorphologyRewrite::MarkedStructural {
        shape_id,
        recipe: MorphologyRecipe {
            refs,
            literal_runs,
            output_segments,
            translated_input_members,
        },
        zone_requirement,
        provenance,
    }
}

fn shape_for_action(a: &AffixAllomorphDef, refs: &[u16]) -> &'static str {
    if a.rhs
        .iter()
        .any(|action| matches!(action, OutputAction::Modify(..)))
    {
        "ModifyFromInput"
    } else if a.lhs.len() == 2 && refs == [1] {
        "AdjacentInitialDrop"
    } else if a.lhs.len() == 2 && refs == [0] {
        "AdjacentTerminalDrop"
    } else if refs.len() == a.lhs.len() && a.lhs.len() >= 2 {
        "AmharicInteriorInsertion"
    } else {
        "UnlistedTopology"
    }
}

fn referenced_inputs(rhs: &[OutputAction]) -> Result<Vec<u16>, &'static str> {
    let mut refs = Vec::new();
    for action in rhs {
        let part = match action {
            OutputAction::Copy(PartRef::Input(index))
            | OutputAction::Modify(PartRef::Input(index), _) => *index,
            OutputAction::Copy(_) | OutputAction::Modify(_, _) => {
                return Err("invalid-part-reference-kind");
            }
            OutputAction::InsertSegments { .. } => continue,
            OutputAction::InsertContext(_) => continue,
        };
        refs.push(part);
    }
    Ok(refs)
}

fn lookup_char_def(table: &CharDefTable, id: CharDefId) -> Option<&CharDef> {
    table
        .iter()
        .find_map(|(candidate, definition)| (candidate == id).then_some(definition))
}

fn validate_pattern(g: &Grammar, table: TableId, pattern: &Pattern) -> Result<(), &'static str> {
    for node in &pattern.nodes {
        validate_node(g, table, node)?;
    }
    Ok(())
}

fn validate_node(g: &Grammar, table: TableId, node: &PatternNode) -> Result<(), &'static str> {
    match node {
        PatternNode::CharDef(id) => {
            let Some(_def) = lookup_char_def(&g.char_tables[table.0 as usize], *id) else {
                return Err("invalid-source-char-def");
            };
        }
        PatternNode::Context(context) => {
            validate_context_features(g, context)?;
            if !context_members(g, table, context).is_some_and(|members| !members.is_empty()) {
                return Err("invalid-source-context");
            }
        }
        PatternNode::Quantifier { children, .. } => {
            for child in children {
                validate_node(g, table, child)?;
            }
        }
        PatternNode::Segments {
            table: node_table,
            shape,
        } => {
            if *node_table != table {
                return Err("invalid-source-table");
            }
            for (_, kind, id, _) in shape.shape.interior() {
                if kind != NodeKind::Segment {
                    return Err("invalid-source-segment-node");
                }
                let Some(_def) = lookup_char_def(&g.char_tables[table.0 as usize], CharDefId(id))
                else {
                    return Err("invalid-source-char-def");
                };
            }
        }
        PatternNode::Anchor(_) => {}
    }
    Ok(())
}

fn validate_context_features(g: &Grammar, context: &SimpleContext) -> Result<(), &'static str> {
    let Some(class) = g.natural_classes.get(context.nat_class.0 as usize) else {
        return Err("invalid-source-context");
    };
    if let NaturalClassKind::Feature(pairs) = &class.kind {
        for (feature, values) in pairs {
            if feature.0 as usize >= g.phon_features.len() {
                return Err("invalid-source-feature-reference");
            }
            if values.0 & !g.phon_features.mask(*feature) != 0 {
                return Err("invalid-source-feature-mask");
            }
        }
    }
    Ok(())
}

fn minimum_consumed_segments(
    g: &Grammar,
    table: TableId,
    pattern: &Pattern,
) -> Result<usize, &'static str> {
    pattern.nodes.iter().try_fold(0usize, |total, node| {
        Ok(total + minimum_consumed_node(g, table, node)?)
    })
}

fn minimum_consumed_node(
    g: &Grammar,
    table: TableId,
    node: &PatternNode,
) -> Result<usize, &'static str> {
    Ok(match node {
        PatternNode::CharDef(id) => usize::from(
            lookup_char_def(
                g.char_tables
                    .get(table.0 as usize)
                    .ok_or("invalid-table-reference")?,
                *id,
            )
            .is_some_and(|def| def.kind() == CharDefKind::Segment),
        ),
        PatternNode::Context(context) => {
            validate_context_features(g, context)?;
            usize::from(
                context_members(g, table, context).is_some_and(|members| !members.is_empty()),
            )
        }
        PatternNode::Quantifier { min, children, .. } => {
            if *min == 0 {
                0
            } else {
                children.iter().try_fold(0usize, |total, child| {
                    Ok(total + minimum_consumed_node(g, table, child)?)
                })?
            }
        }
        PatternNode::Segments {
            table: node_table,
            shape,
        } => {
            if *node_table != table {
                return Err("invalid-source-table");
            }
            shape
                .shape
                .interior()
                .try_fold(0usize, |count, (_, kind, id, _)| {
                    let def = lookup_char_def(
                        g.char_tables
                            .get(table.0 as usize)
                            .ok_or("invalid-table-reference")?,
                        CharDefId(id),
                    )
                    .ok_or("invalid-source-char-def")?;
                    if kind == NodeKind::Segment && def.kind() != CharDefKind::Segment {
                        return Err("invalid-source-char-def");
                    }
                    Ok(count + usize::from(kind == NodeKind::Segment))
                })?
        }
        PatternNode::Anchor(_) => 0,
    })
}

fn input_members(
    g: &Grammar,
    source_table: TableId,
    active_table: TableId,
    parts: &[Pattern],
) -> Result<Vec<Vec<String>>, &'static str> {
    parts
        .iter()
        .map(|part| {
            let ids = pattern_members(g, source_table, part)?;
            if ids.is_empty() {
                return Ok(Vec::new());
            }
            translated_ids(g, source_table, active_table, &ids).ok_or("untranslatable-input-table")
        })
        .collect()
}

fn pattern_members(
    g: &Grammar,
    table: TableId,
    pattern: &Pattern,
) -> Result<Vec<CharDefId>, &'static str> {
    let mut ids = Vec::new();
    for node in &pattern.nodes {
        collect_node_members(g, table, node, &mut ids)?;
    }
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

fn collect_node_members(
    g: &Grammar,
    table: TableId,
    node: &PatternNode,
    out: &mut Vec<CharDefId>,
) -> Result<(), &'static str> {
    match node {
        PatternNode::CharDef(id) => out.push(*id),
        PatternNode::Context(context) => {
            validate_context_features(g, context)?;
            out.extend(context_members(g, table, context).ok_or("invalid-source-context")?);
        }
        PatternNode::Quantifier { children, .. } => {
            for child in children {
                collect_node_members(g, table, child, out)?;
            }
        }
        PatternNode::Segments {
            table: node_table,
            shape,
        } => {
            if *node_table != table {
                return Err("invalid-source-table");
            }
            for (_, kind, id, _) in shape.shape.interior() {
                if kind != NodeKind::Segment {
                    return Err("invalid-source-segment-node");
                }
                out.push(CharDefId(id));
            }
        }
        PatternNode::Anchor(_) => {}
    }
    Ok(())
}

fn pattern_has_boundary(g: &Grammar, table: TableId, pattern: &Pattern) -> bool {
    pattern
        .nodes
        .iter()
        .any(|node| pattern_has_boundary_node(g, table, node))
}

fn pattern_has_boundary_node(g: &Grammar, table: TableId, node: &PatternNode) -> bool {
    match node {
        PatternNode::CharDef(id) => lookup_char_def(&g.char_tables[table.0 as usize], *id)
            .is_some_and(|definition| definition.kind() == CharDefKind::Boundary),
        PatternNode::Context(_) | PatternNode::Anchor(_) => false,
        PatternNode::Quantifier { children, .. } => children
            .iter()
            .any(|child| pattern_has_boundary_node(g, table, child)),
        PatternNode::Segments { shape, .. } => shape
            .shape
            .interior()
            .any(|(_, kind, _, _)| kind == NodeKind::Boundary),
    }
}

fn context_members(g: &Grammar, table: TableId, context: &SimpleContext) -> Option<Vec<CharDefId>> {
    validate_context_features(g, context).ok()?;
    let class = g.natural_classes.get(context.nat_class.0 as usize)?;
    let table_ref = g.char_tables.get(table.0 as usize)?;
    let members = match &class.kind {
        NaturalClassKind::Segments(ids) => ids.clone(),
        NaturalClassKind::Feature(pairs) => table_ref
            .iter()
            .filter(|(_, def)| def.kind() == CharDefKind::Segment)
            .filter(|(_, def)| {
                pairs.iter().all(|(feature, values)| {
                    def.feature_lanes()
                        .get(feature.0 as usize)
                        .is_some_and(|lane| lane & values.0 != 0)
                })
            })
            .map(|(id, _)| id)
            .collect(),
    };
    let mut out = Vec::new();
    for id in members {
        let def = lookup_char_def(table_ref, id)?;
        if def.kind() != CharDefKind::Segment {
            return None;
        }
        out.push(id);
    }
    out.sort_unstable();
    out.dedup();
    Some(out)
}

fn is_segment_only_atom(g: &Grammar, table: TableId, pattern: Option<&Pattern>) -> bool {
    let Some(pattern) = pattern else { return false };
    let [node] = pattern.nodes.as_slice() else {
        return false;
    };
    match node {
        PatternNode::CharDef(id) => g
            .char_tables
            .get(table.0 as usize)
            .and_then(|table| lookup_char_def(table, *id))
            .is_some_and(|def| def.kind() == CharDefKind::Segment),
        PatternNode::Context(context) => {
            context_members(g, table, context).is_some_and(|m| !m.is_empty())
        }
        PatternNode::Segments {
            table: node_table,
            shape,
        } if *node_table == table => {
            let mut ids = shape.shape.interior();
            let Some((_, kind, id, _)) = ids.next() else {
                return false;
            };
            kind == NodeKind::Segment
                && ids.next().is_none()
                && lookup_char_def(&g.char_tables[table.0 as usize], CharDefId(id))
                    .is_some_and(|def| def.kind() == CharDefKind::Segment)
        }
        _ => false,
    }
}

fn is_fixed_segment_atom(g: &Grammar, table: TableId, pattern: Option<&Pattern>) -> bool {
    let Some(pattern) = pattern else { return false };
    let [PatternNode::CharDef(id)] = pattern.nodes.as_slice() else {
        return false;
    };
    g.char_tables
        .get(table.0 as usize)
        .and_then(|table| lookup_char_def(table, *id))
        .is_some_and(|def| def.kind() == CharDefKind::Segment)
}

fn translated_literal_variants(
    g: &Grammar,
    actions: &[OutputAction],
    active_table: TableId,
) -> Result<Vec<String>, &'static str> {
    let mut variants = vec![String::new()];
    for action in actions {
        let OutputAction::InsertSegments {
            table: source_table,
            shape,
        } = action
        else {
            return Err("nonliteral-output-action");
        };
        let Some(source) = g.char_tables.get(source_table.0 as usize) else {
            return Err("untranslatable-output-table");
        };
        if shape.shape.interior().any(|(_, _, id, _)| {
            lookup_char_def(source, CharDefId(id))
                .is_some_and(|definition| definition.kind() != CharDefKind::Segment)
        }) {
            return Err("non-segment-output-atom");
        }
        let pieces = translated_shape_variants(g, *source_table, active_table, &shape.shape)
            .ok_or("untranslatable-output-table")?;
        let mut next = Vec::new();
        for prefix in &variants {
            for piece in &pieces {
                let value = format!("{prefix}{piece}");
                if !next.contains(&value) {
                    next.push(value);
                }
            }
        }
        variants = next;
    }
    Ok(variants)
}

fn translated_shape_variants(
    g: &Grammar,
    source_table: TableId,
    active_table: TableId,
    shape: &Shape,
) -> Option<Vec<String>> {
    let source = g.char_tables.get(source_table.0 as usize)?;
    let active = g.char_tables.get(active_table.0 as usize)?;
    let mut variants = vec![String::new()];
    for (_, _, raw_id, _) in shape.interior() {
        let source_id = CharDefId(raw_id);
        let source_def = lookup_char_def(source, source_id)?;
        if source_def.kind() != CharDefKind::Segment {
            return None;
        }
        let mut pieces = Vec::new();
        for representation in source_def.representations_nfd() {
            let Some(active_id) = active.lookup_nfd(representation) else {
                continue;
            };
            let active_def = lookup_char_def(active, active_id)?;
            if active_def.kind() != CharDefKind::Segment {
                return None;
            }
            pieces.extend(active_def.representations_nfd().iter().cloned());
        }
        pieces.dedup();
        if pieces.is_empty() {
            return None;
        }
        let mut next = Vec::new();
        for prefix in &variants {
            for piece in &pieces {
                next.push(format!("{prefix}{piece}"));
            }
        }
        variants = next;
    }
    Some(variants)
}

fn wrapper_runs(
    g: &Grammar,
    actions: &[OutputAction],
    active_table: TableId,
    parts: usize,
) -> Result<Option<(Vec<String>, Vec<String>)>, &'static str> {
    let first_copy = actions
        .iter()
        .position(|action| matches!(action, OutputAction::Copy(PartRef::Input(_))));
    let Some(first_copy) = first_copy else {
        return Ok(None);
    };
    let mut cursor = first_copy;
    for expected in 0..parts as u16 {
        if !matches!(actions.get(cursor), Some(OutputAction::Copy(PartRef::Input(index))) if *index == expected)
        {
            return Ok(None);
        }
        cursor += 1;
    }
    if actions[first_copy + parts..]
        .iter()
        .any(|action| !matches!(action, OutputAction::InsertSegments { .. }))
        || actions[..first_copy]
            .iter()
            .any(|action| !matches!(action, OutputAction::InsertSegments { .. }))
    {
        return Ok(None);
    }
    let prefix = translated_literal_variants(g, &actions[..first_copy], active_table)?;
    let suffix = translated_literal_variants(g, &actions[cursor..], active_table)?;
    Ok(Some((prefix, suffix)))
}

fn interior_runs(
    g: &Grammar,
    actions: &[OutputAction],
    active_table: TableId,
    parts: usize,
) -> Result<Option<Vec<Vec<String>>>, &'static str> {
    let mut cursor = 0;
    let mut runs = Vec::with_capacity(parts.saturating_sub(1));
    for expected in 0..parts as u16 {
        if !matches!(actions.get(cursor), Some(OutputAction::Copy(PartRef::Input(index))) if *index == expected)
        {
            return Ok(None);
        }
        cursor += 1;
        if expected + 1 == parts as u16 {
            break;
        }
        let start = cursor;
        while matches!(
            actions.get(cursor),
            Some(OutputAction::InsertSegments { .. })
        ) {
            cursor += 1;
        }
        let values = translated_literal_variants(g, &actions[start..cursor], active_table)?;
        runs.push(if values == vec![String::new()] {
            Vec::new()
        } else {
            values
        });
    }
    if cursor != actions.len() || runs.iter().all(Vec::is_empty) {
        return Ok(None);
    }
    Ok(Some(runs))
}

fn classify_terminal_modify(
    g: &Grammar,
    a: &AffixAllomorphDef,
    source_table: TableId,
    active_table: TableId,
    refs: &[u16],
) -> ClassifierResult<MorphologyRewrite> {
    let n = a.lhs.len();
    let Some(OutputAction::Modify(PartRef::Input(index), context)) = a.rhs.last() else {
        return Err(("ModifyFromInput", "modify-nonterminal"));
    };
    if n < 2 || *index as usize != n - 1 || a.rhs.len() != n {
        return Err(("ModifyFromInput", "modify-nonterminal"));
    }
    if !a.rhs[..n - 1].iter().enumerate().all(|(expected, action)| {
        matches!(action, OutputAction::Copy(PartRef::Input(index)) if *index as usize == expected)
    }) {
        return Err(("ModifyFromInput", "modify-nonterminal"));
    }
    if !context.vars.is_empty() {
        return Err(("ModifyFromInput", "terminal-modify-variable"));
    }
    let Some(last) = a.lhs.get(n - 1) else {
        return Err(("ModifyFromInput", "terminal-modify-multi-segment"));
    };
    let [node] = last.nodes.as_slice() else {
        if last
            .nodes
            .iter()
            .any(|node| matches!(node, PatternNode::Quantifier { .. }))
        {
            return Err(("ModifyFromInput", "terminal-modify-quantified"));
        }
        return Err(("ModifyFromInput", "terminal-modify-multi-segment"));
    };
    if matches!(node, PatternNode::Quantifier { .. }) {
        return Err(("ModifyFromInput", "terminal-modify-quantified"));
    }
    if !matches!(
        node,
        PatternNode::CharDef(_) | PatternNode::Context(_) | PatternNode::Segments { .. }
    ) {
        return Err(("ModifyFromInput", "terminal-modify-multi-segment"));
    }
    if !is_segment_only_atom(g, source_table, Some(last)) {
        return Err(("ModifyFromInput", "terminal-modify-source-atom"));
    }
    let members = context_members(g, source_table, context)
        .ok_or(("ModifyFromInput", "terminal-modify-empty-output"))?;
    if members.is_empty() {
        return Err(("ModifyFromInput", "terminal-modify-empty-output"));
    }
    let output_segments = translated_ids(g, source_table, active_table, &members)
        .ok_or(("AmharicTerminalModify", "untranslatable-output-table"))?;
    if output_segments.is_empty() {
        return Err(("ModifyFromInput", "terminal-modify-empty-output"));
    }
    let translated_input_members = input_members(g, source_table, active_table, &a.lhs)
        .map_err(|reason| ("AmharicTerminalModify", reason))?;
    Ok(marked(
        "AmharicTerminalModify",
        refs.to_vec(),
        Vec::new(),
        output_segments,
        translated_input_members,
        ZoneRequirement::Caller,
        RewriteProvenance {
            allomorph: a.id,
            source_table,
            active_table,
        },
    ))
}

fn translated_ids(
    g: &Grammar,
    source_table: TableId,
    active_table: TableId,
    ids: &[CharDefId],
) -> Option<Vec<String>> {
    let source = g.char_tables.get(source_table.0 as usize)?;
    let active = g.char_tables.get(active_table.0 as usize)?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for id in ids {
        let source_def = lookup_char_def(source, *id)?;
        if source_def.kind() != CharDefKind::Segment {
            return None;
        }
        for rep in source_def.representations_nfd() {
            let Some(active_id) = active.lookup_nfd(rep) else {
                continue;
            };
            let active_def = lookup_char_def(active, active_id)?;
            if active_def.kind() != CharDefKind::Segment {
                return None;
            }
            for active_rep in active_def.representations_nfd() {
                if seen.insert(active_rep.clone()) {
                    out.push(active_rep.clone());
                }
            }
        }
        // Unmapped source representations are skipped; at least one aggregate mapping is required.
    }
    (!out.is_empty()).then_some(out)
}

#[derive(Debug, Clone)]
struct LocalRecipe {
    allomorph: AllomorphId,
    table: TableId,
    tail_members: Vec<CharDefId>,
    inserted: String,
    leading: bool,
}

fn encode_insert_actions(
    g: &Grammar,
    alphabet: &SegAlphabet,
    actions: &[OutputAction],
) -> Option<Vec<String>> {
    if actions.is_empty() {
        return None;
    }
    let mut variants = vec![String::new()];
    for action in actions {
        let OutputAction::InsertSegments { table, shape } = action else {
            return None;
        };
        let origin = g.char_tables.get(table.0 as usize)?;
        let pieces = crate::emit::underlying_shape_variants(alphabet, origin, &shape.shape);
        if pieces.is_empty() {
            return None;
        }
        let mut next = Vec::with_capacity(variants.len() * pieces.len());
        for prefix in &variants {
            for piece in &pieces {
                let mut encoded = prefix.clone();
                encoded.push_str(piece);
                next.push(encoded);
            }
        }
        variants = next;
    }
    Some(variants)
}

/// The already-encoded `(prefix, suffix)` token text for `allomorph`, if it matches a single,
/// non-reduplicated `Copy(Input(0))` wrapped by leading and trailing `InsertSegments` over a
/// 1-part LHS (`Role::CircumfixPrefix`'s shape); an interior insert or a repeated copy of the same
/// part needs root-internal splitting or duplication this cannot represent, so stays uncovered.
pub(crate) fn circumfix_texts(
    g: &Grammar,
    alphabet: &SegAlphabet,
    allomorph: &AffixAllomorphDef,
) -> Option<Vec<(String, String)>> {
    if allomorph.lhs.len() != 1 {
        return None;
    }
    let rhs = allomorph.rhs.as_slice();
    let mut copy_positions = rhs
        .iter()
        .enumerate()
        .filter_map(|(i, a)| matches!(a, OutputAction::Copy(PartRef::Input(0))).then_some(i));
    let copy_pos = copy_positions.next()?;
    if copy_positions.next().is_some() {
        return None;
    }
    if copy_pos == 0 || copy_pos == rhs.len() - 1 {
        return None;
    }
    let prefixes = encode_insert_actions(g, alphabet, &rhs[..copy_pos])?;
    let suffixes = encode_insert_actions(g, alphabet, &rhs[copy_pos + 1..])?;
    let mut pairs = Vec::with_capacity(prefixes.len() * suffixes.len());
    for prefix in prefixes {
        for suffix in &suffixes {
            pairs.push((prefix.clone(), suffix.clone()));
        }
    }
    Some(pairs)
}

#[cfg(test)]
mod circumfix_text_tests {
    use super::*;
    use pg_featstruct::SymbolBits;
    use pg_grammar::featsys::FlatIndex;
    use pg_grammar::model::{NatClassId, SegmentedText};

    const XML: &str = r#"<HermitCrabInput><Language><Name>CrossTableCircumfix</Name>
      <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="inner"><Name>Inner</Name><SegmentDefinitions>
        <SegmentDefinition id="ix"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="iz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="iq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions></CharacterDefinitionTable>
      <CharacterDefinitionTable id="outer"><Name>Outer</Name><SegmentDefinitions>
        <SegmentDefinition id="ow"><Representations><Representation>w</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="oq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="oz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ox"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions></CharacterDefinitionTable>
      <NaturalClasses><FeatureNaturalClass id="any"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
      <Strata><Stratum characterDefinitionTable="inner" morphologicalRuleOrder="unordered" morphologicalRules="m">
        <Name>Inner</Name><MorphologicalRuleDefinitions><MorphologicalRule id="m" requiredPartsOfSpeech="p" outputPartOfSpeech="p">
          <Name>M</Name><MorphologicalSubrules><MorphologicalSubrule id="a">
            <MorphologicalInput><PhoneticSequence id="s"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments><CopyFromInput index="s" /><InsertSegments><PhoneticShape>z</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule></MorphologicalSubrules><MorphemeId>M</MorphemeId>
        </MorphologicalRule></MorphologicalRuleDefinitions></Stratum>
        <Stratum characterDefinitionTable="outer" morphologicalRuleOrder="unordered"><Name>Outer</Name></Stratum>
      </Strata></Language></HermitCrabInput>"#;

    #[test]
    fn circumfix_text_uses_surface_table_tokens_for_foreign_insertions() {
        let g = pg_grammar::load(XML).expect("fixture must load");
        let allomorph = match &g.mrules[0] {
            MorphRuleDef::AffixProcess(def) => &def.allomorphs[0],
            other => panic!("m must be affix-process, got {other:?}"),
        };
        let surface = &g.char_tables[1];
        let alphabet = SegAlphabet::new(surface);
        let texts = circumfix_texts(&g, &alphabet, allomorph).expect("circumfix shape");
        assert_eq!(
            texts,
            vec![(
                alphabet.token(surface.lookup_nfd("x").unwrap()).to_string(),
                alphabet.token(surface.lookup_nfd("z").unwrap()).to_string(),
            )]
        );
    }

    #[test]
    fn feature_context_rejects_values_outside_feature_mask() {
        let mut g = pg_grammar::load(XML).expect("fixture must load");
        g.natural_classes[0].kind =
            NaturalClassKind::Feature(vec![(FlatIndex(0), SymbolBits(1u64 << 63))]);
        let context = SimpleContext {
            nat_class: NatClassId(0),
            vars: Vec::new(),
        };
        assert_eq!(
            validate_context_features(&g, &context),
            Err("invalid-source-feature-mask")
        );
    }

    #[test]
    fn shape_member_extraction_rejects_non_segment_nodes() {
        let g = pg_grammar::load(XML).expect("fixture must load");
        let mut builder = pg_shape::ShapeBuilder::new();
        builder.push_boundary(0);
        let node = PatternNode::Segments {
            table: TableId(0),
            shape: SegmentedText {
                text: "x".into(),
                shape: builder.finish(),
            },
        };
        let mut members = Vec::new();
        assert_eq!(
            collect_node_members(&g, TableId(0), &node, &mut members),
            Err("invalid-source-segment-node")
        );
    }
}

fn class_members(g: &Grammar, table: TableId, node: &PatternNode) -> Option<Vec<CharDefId>> {
    let table_ref = g.char_tables.get(table.0 as usize)?;
    let mut members = match node {
        PatternNode::CharDef(id) => vec![*id],
        PatternNode::Context(context) if context.vars.is_empty() => {
            validate_context_features(g, context).ok()?;
            match &g.natural_classes.get(context.nat_class.0 as usize)?.kind {
                NaturalClassKind::Segments(ids) => ids.clone(),
                NaturalClassKind::Feature(pairs) => table_ref
                    .iter()
                    .filter(|(_, definition)| definition.kind() == CharDefKind::Segment)
                    .filter(|(_, definition)| {
                        pairs.iter().all(|(feature, values)| {
                            definition.feature_lanes()[feature.0 as usize] & values.0 != 0
                        })
                    })
                    .map(|(id, _)| id)
                    .collect(),
            }
        }
        PatternNode::Segments {
            table: node_table,
            shape,
        } if *node_table == table => shape
            .shape
            .interior()
            .map(|(_, _, char_def, _)| CharDefId(char_def))
            .collect(),
        _ => return None,
    };
    members.sort_by_key(|id| id.0);
    members.dedup();
    (!members.is_empty()).then_some(members)
}

fn recipe_for(
    g: &Grammar,
    allomorph: &AffixAllomorphDef,
    table_hint: &CharDefTable,
) -> Option<LocalRecipe> {
    if allomorph.lhs.len() != 2 {
        return None;
    }
    let (leading, dropped_node, rest) = match allomorph.rhs.as_slice() {
        [OutputAction::Copy(PartRef::Input(0)), rest @ ..] => (false, &allomorph.lhs[1], rest),
        [OutputAction::Copy(PartRef::Input(1))] => {
            (true, &allomorph.lhs[0], &[] as &[OutputAction])
        }
        _ => return None,
    };
    let [dropped_node] = dropped_node.nodes.as_slice() else {
        return None;
    };
    let mut table = None;
    let mut inserted_shapes = Vec::new();
    for action in rest {
        let OutputAction::InsertSegments {
            table: action_table,
            shape,
        } = action
        else {
            return None;
        };
        if table.is_some_and(|existing| existing != *action_table) {
            return None;
        }
        table = Some(*action_table);
        inserted_shapes.push(shape);
    }
    let table = match table {
        Some(table) => table,
        None => g
            .char_tables
            .iter()
            .position(|candidate| std::ptr::eq(candidate, table_hint))
            .and_then(|index| u16::try_from(index).ok())
            .map(TableId)?,
    };
    let alphabet = SegAlphabet::new(g.char_tables.get(table.0 as usize)?);
    let tail_members = class_members(g, table, dropped_node)?;
    let inserted = inserted_shapes
        .into_iter()
        .map(|shape| alphabet.encode_shape(&shape.shape))
        .collect();
    Some(LocalRecipe {
        allomorph: allomorph.id,
        table,
        tail_members,
        inserted,
        leading,
    })
}

pub(crate) fn structural_marker_for_zone(
    g: &Grammar,
    allomorph: &AffixAllomorphDef,
    table_hint: &CharDefTable,
    prefix_zone: bool,
) -> Option<char> {
    recipe_for(g, allomorph, table_hint)
        .filter(|_| !prefix_zone)
        .and_then(|recipe| {
            let zone = if recipe.leading {
                MarkerZone::Prefix
            } else {
                MarkerZone::Suffix
            };
            marker_binding_for(
                MarkerKey {
                    allomorph: recipe.allomorph,
                    zone,
                },
                ZoneRequirement::Intrinsic(zone),
            )
            .ok()
            .map(|binding| binding.symbol)
        })
}

fn atom(tokens: &[char]) -> String {
    match tokens {
        [only] => only.to_string(),
        many => format!(
            "[{}]",
            many.iter()
                .map(char::to_string)
                .collect::<Vec<_>>()
                .join(" | ")
        ),
    }
}

fn spaced(text: &str) -> String {
    text.chars()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Modifier letters and the degree-style placeholder count as technical realization symbols when they occur as singleton rewrite inputs.
fn is_floating_marker_representation(representation: &str) -> bool {
    !representation.is_empty()
        && representation
            .chars()
            .all(|ch| matches!(ch as u32, 0x02B0..=0x02FF | 0x1D2C..=0x1DFF) || ch == '°')
}

/// Compile a recall-safe identity-or-cleanup relation for technical floating markers that the
/// grammar itself uses as singleton rewrite inputs. The normal environment-sensitive cascade gets
/// first chance to realize them; this final fallback permits an unrealized marker to disappear.
/// Ordinary IPA segments and multi-member natural classes are deliberately excluded.
pub fn compile_authored_deletion_fallback(
    opts: &FomaOptions,
    g: &Grammar,
    pipeline_alphabet: &SegAlphabet,
) -> Option<Fsm> {
    let pipeline_table = g
        .char_tables
        .iter()
        .position(|table| std::ptr::eq(table, pipeline_alphabet.table()))?;
    let pipeline_table = TableId(pipeline_table as u16);
    let mut targets = Vec::new();
    for stratum in &g.strata {
        if stratum.table != pipeline_table {
            continue;
        }
        for rule_id in &stratum.prules {
            let PhonRuleDef::Rewrite(rule) = g.prules.get(rule_id.0 as usize)? else {
                continue;
            };
            let [node] = rule.lhs.nodes.as_slice() else {
                continue;
            };
            let Some(ids) = class_members(g, stratum.table, node) else {
                continue;
            };
            let [id] = ids.as_slice() else {
                continue;
            };
            let definition = pipeline_alphabet.table().get(*id);
            if definition.kind() != CharDefKind::Segment
                || !definition
                    .representations()
                    .iter()
                    .any(|text| is_floating_marker_representation(text))
            {
                continue;
            }
            targets.push(pipeline_alphabet.token(*id));
        }
    }
    targets.sort_unstable();
    targets.dedup();
    if targets.is_empty() {
        return None;
    }
    let regex = targets
        .iter()
        .map(|token| format!("{token} -> 0"))
        .collect::<Vec<_>>()
        .join(", ");
    let delete = fsm_parse_regex(opts, &regex, None, None)?;
    Some(fsm_union(opts, delete, fsm_universal()))
}
/// Compile every supported local recipe. Returns `None` when the grammar contains no supported
/// shape, so existing grammars remain byte-for-byte on the old pipeline.
pub fn compile_layer(
    opts: &FomaOptions,
    g: &Grammar,
    pipeline_alphabet: &SegAlphabet,
) -> Option<Fsm> {
    let pipeline_table = g
        .char_tables
        .iter()
        .position(|table| std::ptr::eq(table, pipeline_alphabet.table()))?;
    let pipeline_table = TableId(pipeline_table as u16);
    let mut net = None;
    for rule in &g.mrules {
        let MorphRuleDef::AffixProcess(definition) = rule else {
            continue;
        };
        for allomorph in &definition.allomorphs {
            if std::env::var_os("PANGLOSS_TRACE_STRUCTURAL_RECIPES").is_some() {
                eprintln!(
                    "structural-shape\t{:?}\t{:?}\tmatched={}",
                    allomorph.lhs,
                    allomorph.rhs,
                    recipe_for(g, allomorph, pipeline_alphabet.table()).is_some()
                );
            }
            let Some(recipe) = recipe_for(g, allomorph, pipeline_alphabet.table()) else {
                continue;
            };
            if recipe.table != pipeline_table {
                continue;
            }
            let zone = if recipe.leading {
                MarkerZone::Prefix
            } else {
                MarkerZone::Suffix
            };
            let marker = marker_binding_for(
                MarkerKey {
                    allomorph: recipe.allomorph,
                    zone,
                },
                ZoneRequirement::Intrinsic(zone),
            )
            .ok()?
            .symbol;
            let tails: Vec<char> = recipe
                .tail_members
                .iter()
                .map(|id| pipeline_alphabet.token(*id))
                .collect();
            let output = if recipe.inserted.is_empty() {
                "0".to_string()
            } else {
                spaced(&recipe.inserted)
            };
            let regex = if recipe.leading {
                let segments: Vec<char> = g
                    .char_tables
                    .get(recipe.table.0 as usize)
                    .into_iter()
                    .flat_map(|table| table.iter())
                    .filter(|(_, definition)| definition.kind() == CharDefKind::Segment)
                    .map(|(id, _)| pipeline_alphabet.token(id))
                    .collect();
                let right_context = format!("{}* {}", atom(&segments), marker);
                let drop = fsm_parse_regex(
                    opts,
                    &format!("{} -> 0 || .#. _ {}", atom(&tails), right_context),
                    None,
                    None,
                )
                .unwrap_or_else(|| panic!("foma rejected structural allomorph regex"));
                let marker_delete = fsm_parse_regex(opts, &format!("{} -> 0", marker), None, None)
                    .unwrap_or_else(|| panic!("foma rejected structural allomorph marker cleanup"));
                fsm_compose(opts, drop, marker_delete)
            } else {
                let regex = format!("{} {} -> {}", atom(&tails), marker, output);
                fsm_parse_regex(opts, &regex, None, None)
                    .unwrap_or_else(|| panic!("foma rejected structural allomorph regex {regex:?}"))
            };
            net = Some(match net {
                None => regex,
                Some(previous) => fsm_compose(opts, previous, regex),
            });
        }
    }
    net
}

#[cfg(test)]
mod relation_tests {
    use super::*;

    fn marked(
        id: u32,
        shape_id: &'static str,
        members: Vec<Vec<&str>>,
        runs: Vec<Vec<&str>>,
        output_segments: Vec<&str>,
        zone: MarkerZone,
    ) -> (MorphologyRewrite, MarkerZone) {
        (
            MorphologyRewrite::MarkedStructural {
                shape_id,
                recipe: MorphologyRecipe {
                    refs: match shape_id {
                        "AmharicInteriorInsertion" => vec![0, 1, 2],
                        "AmharicInitialVowelReplacement" | "AdjacentInitialDrop" => vec![1],
                        "AdjacentTerminalDrop" => vec![0],
                        "AmharicTerminalModify" => vec![0, 1],
                        _ => vec![],
                    },
                    literal_runs: runs
                        .into_iter()
                        .map(|run| run.into_iter().map(str::to_owned).collect())
                        .collect(),
                    output_segments: output_segments.into_iter().map(str::to_owned).collect(),
                    translated_input_members: members
                        .into_iter()
                        .map(|part| part.into_iter().map(str::to_owned).collect())
                        .collect(),
                },
                zone_requirement: ZoneRequirement::Caller,
                provenance: RewriteProvenance {
                    allomorph: AllomorphId(id),
                    source_table: TableId(0),
                    active_table: TableId(0),
                },
            },
            zone,
        )
    }

    #[test]
    fn technical_marker_predicate_accepts_high_plane_and_rejects_foreign_inputs() {
        let relation = CompiledMorphologyRelation::from_classified([marked(
            0x8000,
            "AdjacentInitialDrop",
            vec![vec!["a"], vec!["b"]],
            vec![],
            vec![],
            MarkerZone::Prefix,
        )])
        .unwrap();
        let known = relation.marked_input(AllomorphId(0x8000), "ab").unwrap();
        assert!(matches!(
            relation.apply(&known),
            MorphologyRelationResult::Recipe { .. }
        ));
        let foreign = format!("a{}", char::from_u32(0x100001).unwrap());
        assert!(matches!(
            relation.apply(&foreign),
            MorphologyRelationResult::Rejected {
                reason_id: "foreign-marker",
                ..
            }
        ));
    }

    #[test]
    fn marker_namespace_boundaries_are_closed_and_overflow_is_typed() {
        let first_a = marker_binding_for(
            MarkerKey {
                allomorph: AllomorphId(0),
                zone: MarkerZone::Prefix,
            },
            ZoneRequirement::Caller,
        )
        .unwrap();
        let last_a = marker_binding_for(
            MarkerKey {
                allomorph: AllomorphId(0x7FFE),
                zone: MarkerZone::Suffix,
            },
            ZoneRequirement::Caller,
        )
        .unwrap();
        let first_b = marker_binding_for(
            MarkerKey {
                allomorph: AllomorphId(0x7FFF),
                zone: MarkerZone::Prefix,
            },
            ZoneRequirement::Caller,
        )
        .unwrap();
        let last_b = marker_binding_for(
            MarkerKey {
                allomorph: AllomorphId(0xFFFD),
                zone: MarkerZone::Suffix,
            },
            ZoneRequirement::Caller,
        )
        .unwrap();

        assert_eq!(first_a.symbol as u32, 0xF0000);
        assert_eq!(last_a.symbol as u32, 0xFFFFD);
        assert_eq!(first_b.symbol as u32, 0x100000);
        assert_eq!(last_b.symbol as u32, 0x10FFFD);
        assert!(!is_technical_marker(char::from_u32(0xFFFFE).unwrap()));
        assert!(!is_technical_marker(char::from_u32(0xFFFFF).unwrap()));
        assert!(!is_technical_marker(char::from_u32(0x10FFFE).unwrap()));
        assert!(!is_technical_marker(char::from_u32(0x10FFFF).unwrap()));
        assert!(matches!(
            marker_binding_for(
                MarkerKey {
                    allomorph: AllomorphId(0xFFFE),
                    zone: MarkerZone::Prefix,
                },
                ZoneRequirement::Caller,
            ),
            Err(MarkerBindingError::InvalidScalar)
        ));
    }

    #[test]
    fn probe_shapes_preserve_arbitrary_multi_segment_sequences() {
        let cases = [
            (
                1,
                marked(
                    1,
                    "AdjacentTerminalDrop",
                    vec![vec!["a"], vec!["b"]],
                    vec![vec!["x"]],
                    vec![],
                    MarkerZone::Suffix,
                ),
                "acb",
                "acx",
                MarkerZone::Suffix,
            ),
            (
                2,
                marked(
                    2,
                    "AdjacentInitialDrop",
                    vec![vec!["a"], vec!["b"]],
                    vec![],
                    vec![],
                    MarkerZone::Prefix,
                ),
                "abc",
                "bc",
                MarkerZone::Prefix,
            ),
            (
                3,
                marked(
                    3,
                    "AmharicInitialVowelReplacement",
                    vec![vec!["a"], vec!["b"]],
                    vec![vec!["p"]],
                    vec![],
                    MarkerZone::Prefix,
                ),
                "abc",
                "pbc",
                MarkerZone::Prefix,
            ),
            (
                4,
                marked(
                    4,
                    "AmharicTerminalModify",
                    vec![vec!["a"], vec!["c"]],
                    vec![],
                    vec!["x", "y"],
                    MarkerZone::Suffix,
                ),
                "abc",
                "abx",
                MarkerZone::Suffix,
            ),
        ];
        for (id, input, base, expected, zone) in cases {
            let relation = CompiledMorphologyRelation::from_classified([input]).unwrap();
            let marked = relation
                .marked_input_for_zone(AllomorphId(id), zone, base)
                .unwrap();
            let result = relation.apply(&marked);
            let outputs = match result {
                MorphologyRelationResult::Recipe { outputs, .. } => outputs,
                other => panic!("expected recipe result, got {other:?}"),
            };
            assert!(outputs.contains(expected));
        }
    }

    #[test]
    fn relation_enumerates_multicodepoint_active_members() {
        let cases = [
            (
                12,
                marked(
                    12,
                    "AdjacentInitialDrop",
                    vec![vec!["sy"], vec!["a"]],
                    vec![],
                    vec![],
                    MarkerZone::Prefix,
                ),
                "sya",
                BTreeSet::from(["a".to_owned()]),
            ),
            (
                13,
                marked(
                    13,
                    "AdjacentTerminalDrop",
                    vec![vec!["a"], vec!["sy"]],
                    vec![vec!["x"]],
                    vec![],
                    MarkerZone::Suffix,
                ),
                "asy",
                BTreeSet::from(["ax".to_owned()]),
            ),
            (
                14,
                marked(
                    14,
                    "AmharicInitialVowelReplacement",
                    vec![vec!["sy"], vec!["a"]],
                    vec![vec!["p"]],
                    vec![],
                    MarkerZone::Prefix,
                ),
                "sya",
                BTreeSet::from(["pa".to_owned()]),
            ),
            (
                15,
                marked(
                    15,
                    "AmharicTerminalModify",
                    vec![vec!["a"], vec!["sy"]],
                    vec![],
                    vec!["x"],
                    MarkerZone::Suffix,
                ),
                "asyb",
                BTreeSet::from(["axb".to_owned()]),
            ),
        ];
        for (id, classified, base, expected) in cases {
            let relation = CompiledMorphologyRelation::from_classified([classified]).unwrap();
            let input = relation
                .marked_input_for_zone(AllomorphId(id), MarkerZone::Prefix, base)
                .or_else(|_| {
                    relation.marked_input_for_zone(AllomorphId(id), MarkerZone::Suffix, base)
                })
                .unwrap();
            let MorphologyRelationResult::Recipe { outputs, .. } = relation.apply(&input) else {
                panic!("multi-codepoint member must produce a recipe");
            };
            assert_eq!(outputs, expected);
        }
    }

    #[test]
    fn interior_relation_partitions_multicodepoint_members_as_tokens() {
        let relation = CompiledMorphologyRelation::from_classified([marked(
            16,
            "AmharicInteriorInsertion",
            vec![vec!["a"], vec!["sy"], vec!["b"]],
            vec![vec!["x"], vec!["y"]],
            vec![],
            MarkerZone::Suffix,
        )])
        .unwrap();
        let input = relation.marked_input(AllomorphId(16), "asyb").unwrap();
        let MorphologyRelationResult::Recipe { outputs, .. } = relation.apply(&input) else {
            panic!("multi-codepoint interior member must produce a recipe");
        };
        assert!(outputs.contains("axsyyb"));
    }

    #[test]
    fn interior_probe_inserts_runs_across_arbitrary_partitions() {
        let relation = CompiledMorphologyRelation::from_classified([marked(
            5,
            "AmharicInteriorInsertion",
            vec![vec!["a"], vec!["b"], vec!["c"]],
            vec![vec!["x"], vec!["y"]],
            vec![],
            MarkerZone::Suffix,
        )])
        .unwrap();
        let input = relation.marked_input(AllomorphId(5), "abcd").unwrap();
        let outputs = match relation.apply(&input) {
            MorphologyRelationResult::Recipe { outputs, .. } => outputs,
            other => panic!("expected recipe result, got {other:?}"),
        };
        assert!(outputs.contains("axbycd"));
    }

    #[test]
    fn interior_recall_keeps_scalar_partition_with_overlapping_members() {
        let relation = CompiledMorphologyRelation::from_classified([marked(
            19,
            "AmharicInteriorInsertion",
            vec![vec!["a", "aa"], vec!["b"], vec!["c"]],
            vec![vec!["x"], vec!["y"]],
            vec![],
            MarkerZone::Suffix,
        )])
        .unwrap();
        let input = relation.marked_input(AllomorphId(19), "aab").unwrap();
        let MorphologyRelationResult::Recipe { outputs, .. } = relation.apply(&input) else {
            panic!("overlapping members must retain scalar fallback recall");
        };
        assert!(outputs.contains("axayb"));
    }

    #[test]
    fn overlapping_interior_probe_rejects_before_unbounded_enumeration() {
        let relation = CompiledMorphologyRelation::from_classified([marked(
            17,
            "AmharicInteriorInsertion",
            vec![vec!["a", "aa"], vec!["a", "aa"], vec!["a", "aa"]],
            vec![vec!["x"], vec!["y"]],
            vec![],
            MarkerZone::Suffix,
        )])
        .unwrap();
        let input = relation
            .marked_input(AllomorphId(17), &"a".repeat(64))
            .unwrap();
        assert!(matches!(
            relation.apply(&input),
            MorphologyRelationResult::ResourceRejected {
                reason_id: "probe-work-budget" | "probe-output-bytes",
                consumed_markers: 0,
                work,
                ..
            } if work > 0
        ));
        assert_eq!(
            relation.fired_recipe_count_for(AllomorphId(17), MarkerZone::Suffix),
            0
        );
    }

    #[test]
    fn oversized_no_match_input_is_resource_rejected_without_fire() {
        let relation = CompiledMorphologyRelation::from_classified([marked(
            20,
            "AmharicTerminalModify",
            vec![vec!["a"], vec!["needle"]],
            vec![],
            vec!["x"],
            MarkerZone::Suffix,
        )])
        .unwrap();
        let input = relation
            .marked_input(AllomorphId(20), &"z".repeat(RELATION_PROBE_WORK_CAP + 1))
            .unwrap();
        assert!(matches!(
            relation.apply(&input),
            MorphologyRelationResult::ResourceRejected {
                reason_id: "probe-work-budget",
                consumed_markers: 0,
                ..
            }
        ));
        assert_eq!(
            relation.fired_recipe_count_for(AllomorphId(20), MarkerZone::Suffix),
            0
        );
    }

    #[test]
    fn oversized_no_match_member_table_is_resource_rejected_without_fire() {
        let (
            MorphologyRewrite::MarkedStructural {
                shape_id,
                mut recipe,
                zone_requirement,
                provenance,
            },
            zone,
        ) = marked(
            21,
            "AmharicTerminalModify",
            vec![vec!["a"], vec!["needle"]],
            vec![],
            vec!["x"],
            MarkerZone::Suffix,
        )
        else {
            unreachable!();
        };
        recipe.translated_input_members[1] = (0..=RELATION_PROBE_WORK_CAP)
            .map(|index| format!("member-{index}"))
            .collect();
        let relation = CompiledMorphologyRelation::from_classified([(
            MorphologyRewrite::MarkedStructural {
                shape_id,
                recipe,
                zone_requirement,
                provenance,
            },
            zone,
        )])
        .unwrap();
        let input = relation.marked_input(AllomorphId(21), "z").unwrap();
        assert!(matches!(
            relation.apply(&input),
            MorphologyRelationResult::ResourceRejected {
                reason_id: "probe-work-budget",
                consumed_markers: 0,
                ..
            }
        ));
        assert_eq!(
            relation.fired_recipe_count_for(AllomorphId(21), MarkerZone::Suffix),
            0
        );
    }

    #[test]
    fn member_sort_is_refused_before_sorting_when_its_bound_exceeds_budget() {
        let member_sets = vec![vec!["a".to_owned(), "b".to_owned()]];
        let mut budget = ProbeBudget {
            work: RELATION_PROBE_WORK_CAP - 4,
            outputs: 0,
            output_bytes: 0,
        };
        let mut callbacks = 0;
        let result = visit_segmentations("", &member_sets, &mut budget, &mut |_, _| {
            callbacks += 1;
            Ok(())
        });
        assert!(matches!(
            result,
            Err(ProbeBudgetReached {
                reason_id: "probe-work-budget",
                ..
            })
        ));
        assert_eq!(callbacks, 0);
    }

    #[test]
    fn empty_member_entries_are_charged_before_segmentation() {
        let member_sets = vec![vec![String::new(), String::new()]];
        let mut budget = ProbeBudget {
            work: RELATION_PROBE_WORK_CAP - 2,
            outputs: 0,
            output_bytes: 0,
        };
        let mut callbacks = 0;
        let result = visit_segmentations("", &member_sets, &mut budget, &mut |_, _| {
            callbacks += 1;
            Ok(())
        });
        assert!(matches!(
            result,
            Err(ProbeBudgetReached {
                reason_id: "probe-work-budget",
                ..
            })
        ));
        assert_eq!(callbacks, 0);
    }

    #[test]
    fn partition_clone_is_refused_before_slice_copy() {
        let tokens = vec!["a".to_owned(), "b".to_owned()];
        let mut budget = ProbeBudget {
            work: RELATION_PROBE_WORK_CAP - 2,
            outputs: 0,
            output_bytes: 0,
        };
        let mut current = Vec::new();
        let mut callbacks = 0;
        let result = visit_partitions(&tokens, 1, &mut budget, &mut current, &mut |_, _| {
            callbacks += 1;
            Ok(())
        });
        assert!(matches!(
            result,
            Err(ProbeBudgetReached {
                reason_id: "probe-work-budget",
                ..
            })
        ));
        assert_eq!(callbacks, 0);
        assert!(current.is_empty());
    }

    #[test]
    fn duplicate_output_lookup_is_budgeted_before_contains() {
        let outputs = (0..1024)
            .map(|index| format!("output-{index}"))
            .collect::<BTreeSet<_>>();
        let mut budget = ProbeBudget {
            work: RELATION_PROBE_WORK_CAP - 1,
            outputs: 0,
            output_bytes: 0,
        };
        let mut outputs = outputs;
        let result = insert_probe_output(&mut outputs, "output-0".to_owned(), &mut budget);
        assert!(matches!(
            result,
            Err(ProbeBudgetReached {
                reason_id: "probe-work-budget",
                ..
            })
        ));
        assert_eq!(outputs.len(), 1024);
    }

    #[test]
    fn giant_direct_output_is_refused_before_formatting() {
        let mut replacement = marked(
            22,
            "AmharicTerminalModify",
            vec![vec!["a"], vec!["a"]],
            vec![],
            vec!["x"],
            MarkerZone::Suffix,
        );
        if let MorphologyRewrite::MarkedStructural { recipe, .. } = &mut replacement.0 {
            recipe.output_segments = vec!["x".repeat(RELATION_PROBE_OUTPUT_BYTES_CAP + 1)];
        }
        let relation = CompiledMorphologyRelation::from_classified([replacement]).unwrap();
        let input = relation.marked_input(AllomorphId(22), "a").unwrap();
        assert!(matches!(
            relation.apply(&input),
            MorphologyRelationResult::ResourceRejected {
                reason_id: "probe-output-bytes",
                consumed_markers: 0,
                ..
            }
        ));
        assert_eq!(
            relation.fired_recipe_count_for(AllomorphId(22), MarkerZone::Suffix),
            0
        );
    }

    #[test]
    fn giant_direct_literal_is_refused_before_formatting() {
        let mut literal = marked(
            23,
            "AdjacentTerminalDrop",
            vec![vec!["a"], vec!["b"]],
            vec![vec!["x"]],
            vec![],
            MarkerZone::Suffix,
        );
        if let MorphologyRewrite::MarkedStructural { recipe, .. } = &mut literal.0 {
            recipe.literal_runs[0] = vec!["x".repeat(RELATION_PROBE_OUTPUT_BYTES_CAP + 1)];
        }
        let relation = CompiledMorphologyRelation::from_classified([literal]).unwrap();
        let input = relation.marked_input(AllomorphId(23), "ab").unwrap();
        assert!(matches!(
            relation.apply(&input),
            MorphologyRelationResult::ResourceRejected {
                reason_id: "probe-output-bytes",
                consumed_markers: 0,
                ..
            }
        ));
        assert_eq!(
            relation.fired_recipe_count_for(AllomorphId(23), MarkerZone::Suffix),
            0
        );
    }

    #[test]
    fn output_budget_rejects_unique_direct_outputs_without_partial_fire() {
        let (
            MorphologyRewrite::MarkedStructural {
                shape_id,
                mut recipe,
                zone_requirement,
                provenance,
            },
            zone,
        ) = marked(
            18,
            "AmharicTerminalModify",
            vec![vec!["a"], vec!["a"]],
            vec![],
            vec!["placeholder"],
            MarkerZone::Suffix,
        )
        else {
            unreachable!();
        };
        recipe.output_segments = (0..=RELATION_PROBE_OUTPUT_CAP)
            .map(|index| format!("output-{index}"))
            .collect();
        let relation = CompiledMorphologyRelation::from_classified([(
            MorphologyRewrite::MarkedStructural {
                shape_id,
                recipe,
                zone_requirement,
                provenance,
            },
            zone,
        )])
        .unwrap();
        let input = relation.marked_input(AllomorphId(18), "a").unwrap();
        assert!(matches!(
            relation.apply(&input),
            MorphologyRelationResult::ResourceRejected {
                reason_id: "probe-output-budget",
                consumed_markers: 0,
                work,
                outputs,
            } if work < RELATION_PROBE_WORK_CAP && outputs == RELATION_PROBE_OUTPUT_CAP
        ));
        assert_eq!(
            relation.fired_recipe_count_for(AllomorphId(18), MarkerZone::Suffix),
            0
        );
    }

    #[test]
    fn terminal_modify_replaces_a_matching_middle_position_and_preserves_suffix() {
        let relation = CompiledMorphologyRelation::from_classified([marked(
            6,
            "AmharicTerminalModify",
            vec![vec!["a"], vec!["b"]],
            vec![],
            vec!["x", "y"],
            MarkerZone::Suffix,
        )])
        .unwrap();
        let input = relation.marked_input(AllomorphId(6), "abc").unwrap();
        let outputs = match relation.apply(&input) {
            MorphologyRelationResult::Recipe { outputs, .. } => outputs,
            other => panic!("expected recipe result, got {other:?}"),
        };
        assert_eq!(
            outputs,
            BTreeSet::from(["axc".to_owned(), "ayc".to_owned()])
        );
    }

    #[test]
    fn misplaced_markers_are_rejected_by_zone() {
        let prefix = CompiledMorphologyRelation::from_classified([marked(
            7,
            "AdjacentInitialDrop",
            vec![vec!["a"], vec!["b"]],
            vec![],
            vec![],
            MarkerZone::Prefix,
        )])
        .unwrap();
        let suffix = CompiledMorphologyRelation::from_classified([marked(
            8,
            "AdjacentTerminalDrop",
            vec![vec!["a"], vec!["b"]],
            vec![vec!["x"]],
            vec![],
            MarkerZone::Suffix,
        )])
        .unwrap();
        let prefix_marker = prefix.marker_binding_for(AllomorphId(7)).unwrap().symbol;
        let suffix_marker = suffix.marker_binding_for(AllomorphId(8)).unwrap().symbol;
        assert!(matches!(
            prefix.apply(&format!("a{}b", prefix_marker)),
            MorphologyRelationResult::Rejected {
                reason_id: "zone-mismatch",
                ..
            }
        ));
        assert!(matches!(
            suffix.apply(&format!("{}ab", suffix_marker)),
            MorphologyRelationResult::Rejected {
                reason_id: "zone-mismatch",
                ..
            }
        ));
    }

    #[test]
    fn cloned_relations_share_recipe_fire_observations() {
        let relation = CompiledMorphologyRelation::from_classified([marked(
            11,
            "AdjacentInitialDrop",
            vec![vec!["a"], vec!["b"]],
            vec![],
            vec![],
            MarkerZone::Prefix,
        )])
        .unwrap();
        let clone = relation.clone();
        let input = clone.marked_input(AllomorphId(11), "ab").unwrap();
        assert!(matches!(
            clone.apply(&input),
            MorphologyRelationResult::Recipe { .. }
        ));
        assert_eq!(relation.fired_recipe_count(), 1);
    }

    #[test]
    fn duplicate_binding_is_rejected_by_relation_construction() {
        let first = marked(
            9,
            "AdjacentTerminalDrop",
            vec![vec!["a"], vec!["b"]],
            vec![vec!["x"]],
            vec![],
            MarkerZone::Suffix,
        );
        let second = marked(
            10,
            "AdjacentInitialDrop",
            vec![vec!["a"], vec!["b"]],
            vec![],
            vec![],
            MarkerZone::Prefix,
        );
        let duplicate = marked(
            9,
            "AdjacentTerminalDrop",
            vec![vec!["a"], vec!["b"]],
            vec![vec!["x"]],
            vec![],
            MarkerZone::Suffix,
        );
        assert!(matches!(
            CompiledMorphologyRelation::from_classified([first, second, duplicate]),
            Err(MorphologyRelationError::DuplicateBinding { .. })
        ));
    }
}
