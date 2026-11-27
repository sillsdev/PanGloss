//! Validated, extensible backend-family registry backed by executable grammar predicates and
//! semantics-preserving transforms of a real compilation Plan.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use pg_grammar::model::Grammar;
use serde::{Deserialize, Serialize};

use crate::enumerate::{CandidateRole, EmissionStrategy, LoweredCandidate};
use crate::grammar_semantics::GrammarSemantics;
use crate::plan::{NodeId, Plan};

pub const REGISTRY_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub domain: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Applicability {
    Always,
    /// At least one gated `RewriteSubruleDef`, decided by PROJECTING the real mechanism
    /// (`crate::gate::find_gated_subrules`, over `crate::enumerate::prules_in_order`) rather
    /// than re-deriving it. See `matches`'s own arm for why the previous re-derivation was wrong.
    HasGatedExceptions,
    HasTemplates,
    HasMorphology,
    HasReduplication,
    HasMetathesis,
    HasMultipleStrata,
    /// At least two lexical entries, i.e. a `Gate` partition that a refinement transform could
    /// actually split. See `matches`'s own arm for why this is an over-approximation on purpose.
    HasSplittableGateGroup,
    /// At least one phonological rule.
    HasPhonology,
    /// `HasPhonology` OR `HasTemplates`, evaluated structurally over the same two Grammar fields
    /// those variants already read.
    HasPhonologyOrTemplates,
}

impl Applicability {
    /// `&Grammar` front end onto `Self::matches_semantics` — derives a
    /// `GrammarSemantics` and delegates. A caller checking several families against one grammar
    /// (every `Registry` entry point below) derives the owner once and calls
    /// `Self::matches_semantics` instead.
    pub fn matches(&self, grammar: &Grammar) -> bool {
        self.matches_semantics(&GrammarSemantics::derive(grammar))
    }

    /// The authoritative applicability predicate: every arm is a PROJECTION of a fact
    /// `GrammarSemantics` already owns, never a fresh grammar walk. In particular
    /// `HasGatedExceptions` no longer re-runs `prules_in_order` + `find_gated_subrules` per family
    /// per instance — those ran up to `families x instances` times through
    /// `Registry::materialize_distinct` alone.
    pub fn matches_semantics(&self, semantics: &GrammarSemantics<'_>) -> bool {
        match self {
            Self::Always => true,
            // A projection of `gate::find_gated_subrules`, the same call every compile-path consumer makes, so this predicate cannot drift from the mechanism it describes.
            Self::HasGatedExceptions => semantics.has_gated_exceptions(),
            Self::HasTemplates => semantics.declared_templates(),
            Self::HasMorphology => semantics.has_morphology(),
            Self::HasReduplication => semantics.has_reduplication(),
            Self::HasMetathesis => semantics.has_metathesis(),
            Self::HasMultipleStrata => semantics.stratum_count() > 1,
            // Deliberately over-approximates splittability: a false positive wastes one materialization (deduped away by `materialize_distinct`), a false negative would silently drop a real candidate.
            Self::HasSplittableGateGroup => semantics.entry_count() >= 2,
            // `declared_phonology`, not `cascade_phonology`: they disagree on a rule declared globally but named by no stratum, and switching here would change which families a grammar is offered.
            Self::HasPhonology => semantics.declared_phonology(),
            Self::HasPhonologyOrTemplates => {
                semantics.declared_phonology() || semantics.declared_templates()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderingConstraint {
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub source: String,
    pub note: String,
    pub attested: bool,
}

/// Recorded registry evidence that a family is a plan rewrite whose relation is already
/// represented by the compositional topology. This is policy metadata, not a runtime tie
/// detector: the optimizer can exclude the family before materializing or evaluating a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FamilySearchPolicy {
    #[default]
    AlwaysSearch,
    SkipOnCompositionalTopology,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendFamily {
    pub id: String,
    pub version: u16,
    pub parameters: Vec<Parameter>,
    pub applicability: Applicability,
    #[serde(default)]
    pub search_policy: FamilySearchPolicy,
    #[serde(default)]
    pub ordering: Vec<OrderingConstraint>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone)]
pub struct MaterializerContext<'a> {
    pub grammar: &'a Grammar,
    pub baseline: &'a Plan,
}

pub trait Materializer: Send + Sync {
    fn materialize(
        &self,
        instance: &BackendInstance,
        context: &MaterializerContext<'_>,
    ) -> Result<LoweredCandidate, MaterializeError>;
}

impl<F> Materializer for F
where
    F: for<'a> Fn(
            &BackendInstance,
            &MaterializerContext<'a>,
        ) -> Result<LoweredCandidate, MaterializeError>
        + Send
        + Sync,
{
    fn materialize(
        &self,
        instance: &BackendInstance,
        context: &MaterializerContext<'_>,
    ) -> Result<LoweredCandidate, MaterializeError> {
        self(instance, context)
    }
}

pub type MaterializerFn = Box<dyn Materializer>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BackendInstance {
    pub family_id: String,
    pub parameters: BTreeMap<String, String>,
}

impl BackendInstance {
    pub fn canonical_key(&self) -> String {
        let parameters = self
            .parameters
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join(",");
        format!("{}|{parameters}", self.family_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterializeError {
    Inapplicable(String),
    Invalid(String),
    MissingMaterializer(String),
    RootlessPlan(String),
}

impl fmt::Display for MaterializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for MaterializeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    UnsupportedSchema(u16),
    UnsupportedFamilyVersion {
        family: String,
        version: u16,
    },
    DuplicateFamily(String),
    EmptyId,
    DuplicateParameter {
        family: String,
        name: String,
    },
    EmptyDomain {
        family: String,
        parameter: String,
    },
    DanglingDependency {
        family: String,
        parameter: String,
        dependency: String,
    },
    CyclicDependency(String),
    MissingMaterializer(String),
    UnknownFamily(String),
    InvalidInstance(String),
    InvalidWireFormat(String),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RegistryError {}

#[derive(Default)]
pub struct Registry {
    schema_version: u16,
    families: BTreeMap<String, BackendFamily>,
    materializers: BTreeMap<String, MaterializerFn>,
}

impl Registry {
    pub fn new(schema_version: u16) -> Result<Self, RegistryError> {
        if schema_version != REGISTRY_SCHEMA_VERSION {
            return Err(RegistryError::UnsupportedSchema(schema_version));
        }
        Ok(Self {
            schema_version,
            ..Self::default()
        })
    }

    /// Loads declarative family metadata. Materializers are executable code and therefore must be
    /// registered separately; attempting to materialize before that produces a typed error.
    pub fn load_json(json: &str) -> Result<Self, RegistryError> {
        #[derive(Deserialize)]
        struct Wire {
            schema_version: u16,
            families: BTreeMap<String, BackendFamily>,
        }
        let wire: Wire = serde_json::from_str(json)
            .map_err(|error| RegistryError::InvalidWireFormat(error.to_string()))?;
        let mut registry = Self::new(wire.schema_version)?;
        for (key, family) in wire.families {
            if key != family.id {
                return Err(RegistryError::InvalidWireFormat(format!(
                    "family map key {key:?} does not match id {:?}",
                    family.id
                )));
            }
            registry.validate_family(&family)?;
            registry.families.insert(family.id.clone(), family);
        }
        Ok(registry)
    }

    pub fn seeded() -> Self {
        let mut registry = Self::new(REGISTRY_SCHEMA_VERSION).expect("supported schema");
        for seed in SEEDS {
            registry
                .register_family(seed.family(), Box::new(*seed))
                .expect("seeded family is valid and unique");
        }
        registry
    }

    pub fn register_family(
        &mut self,
        family: BackendFamily,
        materializer: MaterializerFn,
    ) -> Result<(), RegistryError> {
        self.validate_family(&family)?;
        if self.families.contains_key(&family.id) {
            return Err(RegistryError::DuplicateFamily(family.id));
        }
        self.materializers.insert(family.id.clone(), materializer);
        self.families.insert(family.id.clone(), family);
        Ok(())
    }

    pub fn register_materializer(
        &mut self,
        family_id: &str,
        materializer: MaterializerFn,
    ) -> Result<(), RegistryError> {
        if !self.families.contains_key(family_id) {
            return Err(RegistryError::UnknownFamily(family_id.to_owned()));
        }
        self.materializers
            .insert(family_id.to_owned(), materializer);
        Ok(())
    }

    pub fn validate_family(&self, family: &BackendFamily) -> Result<(), RegistryError> {
        if family.id.trim().is_empty() {
            return Err(RegistryError::EmptyId);
        }
        if family.version != REGISTRY_SCHEMA_VERSION {
            return Err(RegistryError::UnsupportedFamilyVersion {
                family: family.id.clone(),
                version: family.version,
            });
        }
        let names: BTreeSet<_> = family
            .parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect();
        if names.len() != family.parameters.len() {
            return Err(RegistryError::DuplicateParameter {
                family: family.id.clone(),
                name: "duplicate".to_owned(),
            });
        }
        for parameter in &family.parameters {
            if parameter.domain.is_empty() {
                return Err(RegistryError::EmptyDomain {
                    family: family.id.clone(),
                    parameter: parameter.name.clone(),
                });
            }
            for dependency in &parameter.depends_on {
                if !names.contains(dependency.as_str()) {
                    return Err(RegistryError::DanglingDependency {
                        family: family.id.clone(),
                        parameter: parameter.name.clone(),
                        dependency: dependency.clone(),
                    });
                }
            }
        }
        if has_dependency_cycle(&family.parameters) {
            return Err(RegistryError::CyclicDependency(family.id.clone()));
        }
        Ok(())
    }

    /// Validates that declarative metadata has a registered executable materializer for every
    /// family. Call this once before characterization/search; `load_json` intentionally permits a
    /// two-phase load so applications can register trusted code after parsing metadata.
    pub fn validate_ready(&self) -> Result<(), RegistryError> {
        for family_id in self.families.keys() {
            if !self.materializers.contains_key(family_id) {
                return Err(RegistryError::MissingMaterializer(family_id.clone()));
            }
        }
        Ok(())
    }

    pub fn validate_instance(&self, instance: &BackendInstance) -> Result<(), RegistryError> {
        let family = self
            .family(&instance.family_id)
            .ok_or_else(|| RegistryError::UnknownFamily(instance.family_id.clone()))?;
        let declared: BTreeSet<_> = family
            .parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect();
        if declared != instance.parameters.keys().cloned().collect() {
            return Err(RegistryError::InvalidInstance(instance.canonical_key()));
        }
        for parameter in &family.parameters {
            let value = instance
                .parameters
                .get(&parameter.name)
                .expect("key set checked above");
            if !parameter.domain.contains(value) {
                return Err(RegistryError::InvalidInstance(instance.canonical_key()));
            }
        }
        Ok(())
    }

    pub fn family(&self, id: &str) -> Option<&BackendFamily> {
        self.families.get(id)
    }

    pub fn families(&self) -> impl Iterator<Item = &BackendFamily> {
        self.families.values()
    }

    pub fn instances(&self) -> Vec<BackendInstance> {
        self.instances_matching(|_| true)
    }

    pub fn instances_for_grammar(&self, grammar: &Grammar) -> Vec<BackendInstance> {
        self.instances_for_semantics(&GrammarSemantics::derive(grammar))
    }

    /// `Self::instances_for_grammar` over an already-derived `GrammarSemantics`.
    pub fn instances_for_semantics(
        &self,
        semantics: &GrammarSemantics<'_>,
    ) -> Vec<BackendInstance> {
        self.instances_matching(|family| family.applicability.matches_semantics(semantics))
    }

    /// Return the applicable instances that are licensed for this run. The exclusion count is
    /// intentionally separate from syntactic deduplication and optimizer budget pruning.
    pub fn instances_for_search(
        &self,
        grammar: &Grammar,
        compositional_topology: bool,
        search_all_families: bool,
    ) -> (Vec<BackendInstance>, u64) {
        self.instances_for_search_with_semantics(
            &GrammarSemantics::derive(grammar),
            compositional_topology,
            search_all_families,
        )
    }

    /// `Self::instances_for_search` over an already-derived `GrammarSemantics`.
    pub fn instances_for_search_with_semantics(
        &self,
        semantics: &GrammarSemantics<'_>,
        compositional_topology: bool,
        search_all_families: bool,
    ) -> (Vec<BackendInstance>, u64) {
        let mut declared_not_searched = 0u64;
        let instances = self
            .families
            .values()
            .filter(|family| family.applicability.matches_semantics(semantics))
            .flat_map(expand_family)
            .filter(|instance| {
                let skip = compositional_topology
                    && !search_all_families
                    && self.family(&instance.family_id).is_some_and(|family| {
                        family.search_policy == FamilySearchPolicy::SkipOnCompositionalTopology
                    });
                if skip {
                    declared_not_searched += 1;
                }
                !skip
            })
            .collect();
        (instances, declared_not_searched)
    }

    fn instances_matching(
        &self,
        predicate: impl Fn(&BackendFamily) -> bool,
    ) -> Vec<BackendInstance> {
        self.families
            .values()
            .filter(|family| predicate(family))
            .flat_map(expand_family)
            .collect()
    }

    pub fn materialize(
        &self,
        instance: &BackendInstance,
        context: &MaterializerContext<'_>,
    ) -> Result<LoweredCandidate, MaterializeError> {
        self.materialize_with_semantics(
            instance,
            context,
            &GrammarSemantics::derive(context.grammar),
        )
    }

    /// `Self::materialize` over an already-derived `GrammarSemantics`. The
    /// applicability re-check is unchanged; what changes is that a batch materializer
    /// (`Self::materialize_distinct`) no longer re-derives the grammar's semantic facts once per
    /// instance on top of the one derivation its own instance enumeration already made.
    pub fn materialize_with_semantics(
        &self,
        instance: &BackendInstance,
        context: &MaterializerContext<'_>,
        semantics: &GrammarSemantics<'_>,
    ) -> Result<LoweredCandidate, MaterializeError> {
        self.validate_instance(instance)
            .map_err(|error| MaterializeError::Invalid(error.to_string()))?;
        let family = self
            .family(&instance.family_id)
            .expect("validated instance has a family");
        if !family.applicability.matches_semantics(semantics) {
            return Err(MaterializeError::Inapplicable(instance.family_id.clone()));
        }
        let candidate = self
            .materializers
            .get(&instance.family_id)
            .ok_or_else(|| MaterializeError::MissingMaterializer(instance.family_id.clone()))?
            .materialize(instance, context)?;
        candidate
            .plan
            .root()
            .ok_or_else(|| MaterializeError::RootlessPlan(instance.family_id.clone()))?;
        Ok(candidate)
    }

    /// Materializes all applicable instances and deduplicates equal executable Plans by root
    /// content address. The first family in stable registry order owns the retained provenance.
    /// Deduplicates on `(plan root, EmissionStrategy)`, NOT on the plan root alone.
    ///
    /// The strategy has to be part of the key. A whole-grammar strategy family carries the baseline
    /// PLAN (that compiler derives its own topology and never interprets one), so keying on the root
    /// alone would classify it as a duplicate of the baseline and silently drop the only candidate in
    /// the registry whose network can differ for a reason minimization cannot erase — the exact
    /// failure this dedup is meant to prevent, inverted.
    pub fn materialize_distinct(
        &self,
        context: &MaterializerContext<'_>,
    ) -> Result<Vec<(BackendInstance, LoweredCandidate)>, MaterializeError> {
        // One derivation shared by the instance enumeration below and every per-instance applicability re-check inside `materialize_with_semantics`.
        let semantics = GrammarSemantics::derive(context.grammar);
        let mut seen = BTreeSet::<(NodeId, &'static str)>::new();
        let mut candidates = Vec::new();
        for instance in self.instances_for_semantics(&semantics) {
            let candidate = self.materialize_with_semantics(&instance, context, &semantics)?;
            let root = candidate
                .plan
                .root()
                .ok_or_else(|| MaterializeError::RootlessPlan(instance.family_id.clone()))?;
            if seen.insert((root, candidate.strategy().label())) {
                candidates.push((instance, candidate));
            }
        }
        Ok(candidates)
    }

    pub fn canonical_json(&self) -> String {
        #[derive(Serialize)]
        struct View<'a> {
            schema_version: u16,
            families: &'a BTreeMap<String, BackendFamily>,
        }
        serde_json::to_string(&View {
            schema_version: self.schema_version,
            families: &self.families,
        })
        .expect("registry metadata is serializable")
    }
}

fn has_dependency_cycle(parameters: &[Parameter]) -> bool {
    fn visit(
        name: &str,
        parameters: &[Parameter],
        complete: &mut BTreeSet<String>,
        active: &mut BTreeSet<String>,
    ) -> bool {
        if active.contains(name) {
            return true;
        }
        if complete.contains(name) {
            return false;
        }
        active.insert(name.to_owned());
        let cyclic = parameters
            .iter()
            .find(|parameter| parameter.name == name)
            .is_some_and(|parameter| {
                parameter
                    .depends_on
                    .iter()
                    .any(|dependency| visit(dependency, parameters, complete, active))
            });
        active.remove(name);
        complete.insert(name.to_owned());
        cyclic
    }

    let mut complete = BTreeSet::new();
    parameters.iter().any(|parameter| {
        visit(
            &parameter.name,
            parameters,
            &mut complete,
            &mut BTreeSet::new(),
        )
    })
}

fn expand_family(family: &BackendFamily) -> Vec<BackendInstance> {
    let mut assignments = vec![BTreeMap::new()];
    for parameter in &family.parameters {
        assignments = assignments
            .into_iter()
            .flat_map(|assignment| {
                parameter.domain.iter().map(move |value| {
                    let mut next = assignment.clone();
                    next.insert(parameter.name.clone(), value.clone());
                    next
                })
            })
            .collect();
    }
    assignments
        .into_iter()
        .map(|parameters| BackendInstance {
            family_id: family.id.clone(),
            parameters,
        })
        .collect()
}

/// One seeded family: a compiler offered by name over the grammar's baseline plan.
#[derive(Debug, Clone, Copy)]
struct SeededFamily {
    id: &'static str,
    applicability: Applicability,
    /// Which compiler realizes this family.
    adapter: EmissionStrategy,
    ordering: &'static [(&'static str, &'static str)],
}

impl SeededFamily {
    fn family(self) -> BackendFamily {
        BackendFamily {
            id: self.id.to_owned(),
            version: REGISTRY_SCHEMA_VERSION,
            parameters: vec![Parameter {
                name: "topology".to_owned(),
                // The varying axis is the compiler, so name that instead of reporting a relabelled `topology=baseline`.
                domain: vec![self.adapter.label().to_owned()],
                depends_on: Vec::new(),
            }],
            applicability: self.applicability,
            search_policy: FamilySearchPolicy::AlwaysSearch,
            ordering: self
                .ordering
                .iter()
                .map(|(before, after)| OrderingConstraint {
                    before: (*before).to_owned(),
                    after: (*after).to_owned(),
                })
                .collect(),
            provenance: Provenance {
                source: "docs/fst-plan/linguistic-backend-harvest.md".to_owned(),
                note: "Attested construction prior only; grammar facts and full-HC confirmation remain authoritative".to_owned(),
                attested: true,
            },
        }
    }
}

impl Materializer for SeededFamily {
    fn materialize(
        &self,
        _instance: &BackendInstance,
        context: &MaterializerContext<'_>,
    ) -> Result<LoweredCandidate, MaterializeError> {
        Ok(LoweredCandidate {
            label: self.id,
            plan: context.baseline.clone(),
            adapter: self.adapter,
            role: CandidateRole::Baseline,
        })
    }
}

// Family ids are defined once here and used by `SEEDS` below (not duplicated as literals), so a rename fails the build at every decision site instead of silently changing behavior.
pub const FAMILY_SURFACE_PROBE_MORPHOLOGY: &str = "surface-probe-morphology";

const SEEDS: &[SeededFamily] = &[SeededFamily {
    id: FAMILY_SURFACE_PROBE_MORPHOLOGY,
    applicability: Applicability::Always,
    adapter: EmissionStrategy::TunedSurfaceProbed,
    ordering: &[("morphology", "phonology")],
}];

pub const SEEDED_FAMILIES: &[&str] = &[FAMILY_SURFACE_PROBE_MORPHOLOGY];

#[cfg(test)]
mod tests;
