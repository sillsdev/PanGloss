//! Validated, extensible recipe-family registry backed by executable grammar predicates and
//! semantics-preserving transforms of a real compilation Plan.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use pg_grammar::model::Grammar;
use serde::{Deserialize, Serialize};

use crate::enumerate::{CandidateRole, LoweredCandidate};
use crate::executable_candidate::LoweringAdapter;
use crate::executable_candidate::{CandidateConstructionError, ExecutableCandidate};
use crate::grammar_semantics::GrammarSemantics;
use crate::mechanism_provider::derive_mechanism_graph;
use crate::oracle::{
    permute_gate_groups, permute_union_children, refine_gate_partition, PartitionGranularity,
};
use crate::plan::{NodeId, Plan};

pub const REGISTRY_SCHEMA_VERSION: u16 = 1;

/// The witness that an [`ExecutableCandidate`] was built by the Registry (task 7.5).
///
/// Its only field is a private unit, declared in THIS module. That single fact is the whole
/// enforcement: no other module -- in this crate or any other -- can name the field, so no other
/// module can produce a value of this type, so no other module can call
/// [`crate::executable_candidate::seal`]. It is deliberately neither `Copy` nor `Clone`, so an
/// authority obtained for one candidate cannot be kept and reused to seal a second, unvalidated
/// one.
///
/// This is the same shape task 7.3 used for [`crate::recipe_mechanism::MechanismBinding`] -- private
/// fields plus a single constructor -- scaled up to a type whose validation lives in a different
/// module from the type itself, where field privacy alone would not have reached.
pub struct RegistryAuthority(());

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
    /// ([`crate::gate::find_gated_subrules`], over [`crate::enumerate::prules_in_order`]) rather
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
    /// At least one phonological rule. Required by
    /// `EmissionStrategy::TemplatedUnderlyingTokens`, whose whole premise is that a compiled rewrite
    /// cascade does the phonological work the surface probe would otherwise bake into the lexc: with
    /// no rules, `compile_templated_morphotactics` has no cascade to compose and fails with
    /// `NoCompiledRules`. Gating here turns that from a guaranteed build failure in the report into
    /// an honest "this family does not apply to this grammar".
    ///
    /// Kept as its own variant (rather than folded into `HasPhonologyOrTemplates` below) because it
    /// still names a real, narrower structural fact on its own.
    HasPhonology,
    /// `HasPhonology` OR `HasTemplates`, evaluated structurally over the same two Grammar fields
    /// those variants already read. `token-cascade-morphology`'s compiler
    /// (`compile_templated_morphotactics`) has two independent reasons to be worth offering: a real
    /// rewrite cascade to compose (the `HasPhonology` case), or template-aware morphotactic
    /// structure -- slot ordering and bounded slot occupancy -- that the plan-composed baseline's
    /// self-looping `uflexc` emitter does not generalize to (`uflexc`'s own module doc). A grammar
    /// can have either without the other: the measured Sena shape has templates and zero
    /// phonological rules, so gating on `HasPhonology` alone left it with `uflexc` as its only
    /// underlying model, never comparing it against the template-aware candidate at all. Fixed as a
    /// single widened predicate rather than a second seeded family with the same strategy label,
    /// because `materialize_distinct` dedups on `(plan root, strategy label)` and a whole-grammar
    /// strategy carries the baseline plan verbatim -- two families here would just be two
    /// applicability checks racing to be first, collapsing to the same one candidate either way.
    HasPhonologyOrTemplates,
}

impl Applicability {
    /// `&Grammar` front end onto [`Self::matches_semantics`] — derives a
    /// [`GrammarSemantics`] and delegates. A caller checking several families against one grammar
    /// (every `Registry` entry point below) derives the owner once and calls
    /// [`Self::matches_semantics`] instead.
    pub fn matches(&self, grammar: &Grammar) -> bool {
        self.matches_semantics(&GrammarSemantics::derive(grammar))
    }

    /// The authoritative applicability predicate (task 7.11,
    /// `openspec/changes/cleanup-and-recipe-parity`): every arm is a PROJECTION of a fact
    /// [`GrammarSemantics`] already owns, never a fresh grammar walk. In particular
    /// `HasGatedExceptions` no longer re-runs `prules_in_order` + `find_gated_subrules` per family
    /// per instance — those ran up to `families x instances` times through
    /// `Registry::materialize_distinct` alone.
    pub fn matches_semantics(&self, semantics: &GrammarSemantics<'_>) -> bool {
        match self {
            Self::Always => true,
            // A PROJECTION of the real mechanism, not a third reimplementation of it. The
            // gated-subrule universe is whatever `gate::find_gated_subrules` says it is -- the same
            // call `gate::compile_gated_grammar_with_budget`, `enumerate::enumerate_default` and
            // `recipe_space::GrammarFacts::from_grammar` all make -- so this predicate cannot drift
            // away from the compile path it is supposed to be describing.
            //
            // The re-derivation this replaces disagreed with the mechanism in three ways, one of
            // them a real bug:
            //  1. It required `!grammar.mpr_features.is_empty()`, a precondition NO other
            //     derivation of this fact has (`gate::is_gated`, `capability::characterize`'s
            //     `SubruleGating`). A grammar gating purely on `required_pos` and declaring no MPR
            //     features at all is gated by every other measure -- `recipe_space` reports its
            //     gated subrules -- yet was never offered `FAMILY_CLASS_EXCEPTION_CASCADE`.
            //  2. It counted `required_mpr`/`excluded_mpr` bits belonging to an `Any`-type
            //     `MprGroup`, which `gate::is_gated` deliberately masks out (that module's caveat:
            //     `Any`-type restrictions are not partitioned on in this prototype), so the
            //     registry could offer a gate-permutation family over a partition the gate
            //     mechanism would then refuse to split.
            //  3. It scanned `grammar.prules` wholesale rather than the stratum-cascade slice
            //     everything downstream actually compiles, so a rule no stratum references counted.
            Self::HasGatedExceptions => semantics.has_gated_exceptions(),
            Self::HasTemplates => semantics.declared_templates(),
            Self::HasMorphology => semantics.has_morphology(),
            Self::HasReduplication => semantics.has_reduplication(),
            Self::HasMetathesis => semantics.has_metathesis(),
            Self::HasMultipleStrata => semantics.stratum_count() > 1,
            // Deliberately an OVER-approximation, and sound because of it. Whether a `Gate` group is
            // genuinely splittable is a property of the built Plan (a group needs >=2 entries), and
            // this predicate only sees the Grammar. Two or more lexical entries is the necessary
            // condition; when it holds but no group actually turns out splittable,
            // `oracle::refine_gate_partition` is a documented no-op whose rebuilt nodes
            // content-address straight back to the originals, so `materialize_distinct` dedups the
            // candidate away. Over-approximating therefore costs one wasted materialization at worst;
            // under-approximating would silently drop a real candidate, which is the error that
            // matters.
            Self::HasSplittableGateGroup => semantics.entry_count() >= 2,
            // `declared_phonology`, NOT `cascade_phonology`. These two are different questions and
            // genuinely disagree on a rule declared globally but named by no stratum
            // (`grammar_semantics`'s module doc). This arm keeps the grammar-wide reading it always
            // had -- task 7.11 is a consolidation, and switching predicates here would change which
            // families a grammar is offered.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FamilySearchPolicy {
    AlwaysSearch,
    SkipOnCompositionalTopology,
}

impl Default for FamilySearchPolicy {
    fn default() -> Self {
        Self::AlwaysSearch
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeFamily {
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
        instance: &RecipeInstance,
        context: &MaterializerContext<'_>,
    ) -> Result<LoweredCandidate, MaterializeError>;
}

impl<F> Materializer for F
where
    F: for<'a> Fn(
            &RecipeInstance,
            &MaterializerContext<'a>,
        ) -> Result<LoweredCandidate, MaterializeError>
        + Send
        + Sync,
{
    fn materialize(
        &self,
        instance: &RecipeInstance,
        context: &MaterializerContext<'_>,
    ) -> Result<LoweredCandidate, MaterializeError> {
        self(instance, context)
    }
}

pub type MaterializerFn = Box<dyn Materializer>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RecipeInstance {
    pub family_id: String,
    pub parameters: BTreeMap<String, String>,
}

impl RecipeInstance {
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
    families: BTreeMap<String, RecipeFamily>,
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
            families: BTreeMap<String, RecipeFamily>,
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
        family: RecipeFamily,
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

    pub fn validate_family(&self, family: &RecipeFamily) -> Result<(), RegistryError> {
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

    pub fn validate_instance(&self, instance: &RecipeInstance) -> Result<(), RegistryError> {
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

    pub fn family(&self, id: &str) -> Option<&RecipeFamily> {
        self.families.get(id)
    }

    pub fn families(&self) -> impl Iterator<Item = &RecipeFamily> {
        self.families.values()
    }

    pub fn instances(&self) -> Vec<RecipeInstance> {
        self.instances_matching(|_| true)
    }

    pub fn instances_for_grammar(&self, grammar: &Grammar) -> Vec<RecipeInstance> {
        self.instances_for_semantics(&GrammarSemantics::derive(grammar))
    }

    /// [`Self::instances_for_grammar`] over an already-derived [`GrammarSemantics`] (task 7.11).
    pub fn instances_for_semantics(&self, semantics: &GrammarSemantics<'_>) -> Vec<RecipeInstance> {
        self.instances_matching(|family| family.applicability.matches_semantics(semantics))
    }

    /// Return the applicable instances that are licensed for this run. The exclusion count is
    /// intentionally separate from syntactic deduplication and optimizer budget pruning.
    pub fn instances_for_search(
        &self,
        grammar: &Grammar,
        compositional_topology: bool,
        search_all_families: bool,
    ) -> (Vec<RecipeInstance>, u64) {
        self.instances_for_search_with_semantics(
            &GrammarSemantics::derive(grammar),
            compositional_topology,
            search_all_families,
        )
    }

    /// [`Self::instances_for_search`] over an already-derived [`GrammarSemantics`] (task 7.11).
    pub fn instances_for_search_with_semantics(
        &self,
        semantics: &GrammarSemantics<'_>,
        compositional_topology: bool,
        search_all_families: bool,
    ) -> (Vec<RecipeInstance>, u64) {
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

    fn instances_matching(&self, predicate: impl Fn(&RecipeFamily) -> bool) -> Vec<RecipeInstance> {
        self.families
            .values()
            .filter(|family| predicate(family))
            .flat_map(expand_family)
            .collect()
    }

    pub fn materialize(
        &self,
        instance: &RecipeInstance,
        context: &MaterializerContext<'_>,
    ) -> Result<LoweredCandidate, MaterializeError> {
        self.materialize_with_semantics(
            instance,
            context,
            &GrammarSemantics::derive(context.grammar),
        )
    }

    /// [`Self::materialize`] over an already-derived [`GrammarSemantics`] (task 7.11). The
    /// applicability re-check is unchanged; what changes is that a batch materializer
    /// ([`Self::materialize_distinct`]) no longer re-derives the grammar's semantic facts once per
    /// instance on top of the one derivation its own instance enumeration already made.
    pub fn materialize_with_semantics(
        &self,
        instance: &RecipeInstance,
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
    ) -> Result<Vec<(RecipeInstance, LoweredCandidate)>, MaterializeError> {
        // ONE derivation for the whole batch (task 7.11): shared by the instance enumeration below
        // AND by every per-instance applicability re-check inside `materialize_with_semantics`.
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

    /// Task 7.5: the SOLE constructor of a validated [`ExecutableCandidate`].
    ///
    /// Materializes `instance` exactly as [`Self::materialize_with_semantics`] does -- same
    /// validation, same applicability re-check, same typed [`MaterializeError`], so nothing about
    /// WHICH candidates a grammar is offered changes here -- and then binds the parts a
    /// a `LoweredCandidate` does not carry: a stable semantic digest, a portable round-trippable Plan
    /// document and its digest, the exact lowering adapter, the runtime requirements the evaluator
    /// already enforces, the mechanism graph with its per-adapter bindings, and the certification
    /// scope those bindings license.
    ///
    /// The mechanism graph is derived from `semantics` alone, through
    /// [`crate::mechanism_provider::derive_mechanism_graph`] -- task 7.4's rule that no provider may
    /// reread the `Grammar` to decide applicability is inherited here rather than restated, because
    /// this function has no other way to obtain a graph.
    ///
    /// # Refuses, never substitutes
    /// Every failure is a [`CandidateConstructionError`]. If the named adapter cannot represent a
    /// construct this grammar's mechanisms require, that is
    /// [`CandidateConstructionError::MechanismRefused`] naming the mechanism and the adapter -- not
    /// a quiet switch to a compiler that happens to work. A cheaper candidate can be a WRONG
    /// candidate (Amharic's 2.2x-cheaper `identity-mismatch`), so a substitution made here would be
    /// indistinguishable, downstream, from a measurement of the candidate that was asked for.
    ///
    /// # Builds and verifies data only, like [`crate::mechanism_provider`]
    /// Constructing an `ExecutableCandidate` changes no outcome and makes nothing selectable that
    /// was not selectable before -- no applicability predicate, dispatch, or evaluation in
    /// [`crate::recipe_runtime`] is required to consume one.
    pub fn executable_candidate(
        &self,
        instance: &RecipeInstance,
        context: &MaterializerContext<'_>,
        semantics: &GrammarSemantics<'_>,
    ) -> Result<ExecutableCandidate, CandidateConstructionError> {
        let candidate = self
            .materialize_with_semantics(instance, context, semantics)
            .map_err(CandidateConstructionError::Materialize)?;
        crate::executable_candidate::seal(
            RegistryAuthority(()),
            &instance.family_id,
            instance,
            &candidate.plan,
            candidate.adapter,
            derive_mechanism_graph(semantics),
        )
    }

    pub fn canonical_json(&self) -> String {
        #[derive(Serialize)]
        struct View<'a> {
            schema_version: u16,
            families: &'a BTreeMap<String, RecipeFamily>,
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

fn expand_family(family: &RecipeFamily) -> Vec<RecipeInstance> {
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
        .map(|parameters| RecipeInstance {
            family_id: family.id.clone(),
            parameters,
        })
        .collect()
}

/// The plan rewrites a seeded family may apply. Every one must be SEMANTICS-PRESERVING: it may
/// change a Plan's shape and therefore its content address, never the relation the compiled network
/// accepts. Each variant below cites the argument that makes it safe -- none is safe by assumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SafeTransform {
    Identity,
    /// Reorders a `Gate` node's partition groups. Safe because `build_controllable` folds the groups
    /// with `union_checked` (commutative) and always finishes with `minimize_checked`, so only group
    /// MEMBERSHIP reaches the accepted relation (`oracle::permute_gate_groups`' own doc).
    GatePermutation,
    /// Reorders a root `Union`'s children. Same commutativity argument, different node kind.
    UnionPermutation,
    /// Splits each eligible `Gate` group's entry set into at most 2 contiguous sub-groups, each still
    /// composed with that group's OWN unchanged `Replace` node. Safe because composition distributes
    /// over union -- `(A ∪ B) .o. R == (A .o. R) ∪ (B .o. R)` -- so re-unioning the pieces reproduces
    /// the original group's net exactly (`oracle::refine_gate_partition`'s own doc). This is a
    /// genuinely different axis from the two above: it changes partition CARDINALITY, not order.
    PartitionBisect,
    /// The same refinement taken to its limit: one singleton sub-group per entry. Distinct from
    /// `PartitionBisect` whenever a group holds more than two entries, and it stresses the
    /// many-small-unions shape rather than the few-large-unions one.
    PartitionFanOut,
}

#[derive(Debug, Clone, Copy)]
struct SeededFamily {
    id: &'static str,
    applicability: Applicability,
    transform: SafeTransform,
    /// Which compiler realizes this family. `PlanComposed` for every plan-rewrite family (the
    /// `transform` is then what varies); a whole-grammar strategy for a family whose whole point is
    /// that it compiles the grammar a DIFFERENT way, in which case `transform` is `Identity` because
    /// that compiler does not interpret a plan at all.
    adapter: LoweringAdapter,
    ordering: &'static [(&'static str, &'static str)],
}

impl SeededFamily {
    fn family(self) -> RecipeFamily {
        RecipeFamily {
            id: self.id.to_owned(),
            version: REGISTRY_SCHEMA_VERSION,
            parameters: vec![Parameter {
                name: "topology".to_owned(),
                // For a whole-grammar strategy the varying axis is the COMPILER, not the plan
                // rewrite, so name that instead — otherwise every such family would report
                // `topology=baseline` and read as a relabelled duplicate of the baseline.
                domain: vec![if !self.adapter.interprets_plan() {
                    self.adapter.strategy().label().to_owned()
                } else {
                    match self.transform {
                        SafeTransform::Identity => "baseline",
                        SafeTransform::GatePermutation => "gate-permutation",
                        SafeTransform::UnionPermutation => "union-permutation",
                        SafeTransform::PartitionBisect => "partition-bisect",
                        SafeTransform::PartitionFanOut => "partition-fan-out",
                    }
                    .to_owned()
                }],
                depends_on: Vec::new(),
            }],
            applicability: self.applicability,
            // Registry minimization evidence records that these exact plan-rewrite transforms
            // preserve the compositional relation. The production gate is structural and runs
            // before pilot/evaluation; it never builds a candidate to discover a tie.
            search_policy: match self.transform {
                SafeTransform::Identity => FamilySearchPolicy::AlwaysSearch,
                SafeTransform::GatePermutation
                | SafeTransform::UnionPermutation
                | SafeTransform::PartitionBisect
                | SafeTransform::PartitionFanOut => {
                    FamilySearchPolicy::SkipOnCompositionalTopology
                }
            },
            ordering: self
                .ordering
                .iter()
                .map(|(before, after)| OrderingConstraint {
                    before: (*before).to_owned(),
                    after: (*after).to_owned(),
                })
                .collect(),
            provenance: Provenance {
                source: "docs/fst-plan/linguistic-recipe-harvest.md".to_owned(),
                note: "Attested construction prior only; grammar facts and full-HC confirmation remain authoritative".to_owned(),
                attested: true,
            },
        }
    }
}

impl Materializer for SeededFamily {
    fn materialize(
        &self,
        _instance: &RecipeInstance,
        context: &MaterializerContext<'_>,
    ) -> Result<LoweredCandidate, MaterializeError> {
        let plan = match self.transform {
            SafeTransform::Identity => context.baseline.clone(),
            SafeTransform::GatePermutation => permute_gate_groups(context.baseline),
            SafeTransform::UnionPermutation => permute_union_children(context.baseline),
            SafeTransform::PartitionBisect => {
                refine_gate_partition(context.baseline, PartitionGranularity::Bisect)
            }
            SafeTransform::PartitionFanOut => {
                refine_gate_partition(context.baseline, PartitionGranularity::FanOut)
            }
        };
        Ok(LoweredCandidate {
            label: self.id,
            plan,
            adapter: self.adapter,
            // DERIVED, never declared -- the same discipline task 7.3 imposed on `MechanismEdge` and
            // 7.5 on `RuntimeRequirement`. A family whose transform is `Identity` hands back
            // `context.baseline` VERBATIM, so under the one adapter that interprets a plan that
            // candidate IS this grammar's default compilation and nothing else can be. Every other
            // family rewrites the assembly tree (an alternative by construction), and a whole-grammar
            // adapter never reads the plan at all, so calling it "the baseline plan's compilation"
            // would be a category error -- it is a different compiler, which is exactly the axis
            // Wave 3 measured as decisive.
            //
            // At most one such candidate survives a batch: `materialize_distinct` dedups on
            // `(plan root, strategy label)`, and every `Identity` + plan-composing family produces
            // the identical root under the identical label.
            role: if self.transform == SafeTransform::Identity && self.adapter.interprets_plan() {
                CandidateRole::Baseline
            } else {
                CandidateRole::Alternative
            },
        })
    }
}

// Family ids: the single source of truth for every seeded family's identity string. Defined here,
// used IN `SEEDS` below (not duplicated as literals), so decision sites elsewhere (`pg-cli`'s
// `recipe_optimize.rs` baseline detection, any test asserting on family identity) reference these
// constants instead of comparing strings -- a rename then fails the build at every use site rather
// than silently changing behavior (recipe-pipeline-hygiene D7 / spec.md "Family identities are
// compiler-checked at decision sites").
pub const FAMILY_ORDERED_MORPHOPHONOLOGY: &str = "ordered-morphophonology";
pub const FAMILY_CLASS_EXCEPTION_CASCADE: &str = "class-exception-cascade";
pub const FAMILY_COMPLETE_TEMPLATE: &str = "complete-template";
pub const FAMILY_SPECIALIZED_BRANCH: &str = "specialized-branch";
pub const FAMILY_COPY_BRANCH: &str = "copy-branch";
pub const FAMILY_BOUNDED_METATHESIS: &str = "bounded-metathesis";
pub const FAMILY_LAYERED_MORPHOLOGY: &str = "layered-morphology";
pub const FAMILY_SURFACE_PROBE_MORPHOLOGY: &str = "surface-probe-morphology";
pub const FAMILY_TOKEN_CASCADE_MORPHOLOGY: &str = "token-cascade-morphology";

const SEEDS: &[SeededFamily] = &[
    SeededFamily {
        id: FAMILY_ORDERED_MORPHOPHONOLOGY,
        applicability: Applicability::Always,
        transform: SafeTransform::Identity,
        adapter: LoweringAdapter::ControllablePlanCompose,
        ordering: &[("morphology", "phonology")],
    },
    SeededFamily {
        id: FAMILY_CLASS_EXCEPTION_CASCADE,
        applicability: Applicability::HasGatedExceptions,
        transform: SafeTransform::GatePermutation,
        adapter: LoweringAdapter::ControllablePlanCompose,
        ordering: &[("class-partition", "exception-cascade")],
    },
    SeededFamily {
        id: FAMILY_COMPLETE_TEMPLATE,
        applicability: Applicability::HasTemplates,
        transform: SafeTransform::UnionPermutation,
        adapter: LoweringAdapter::ControllablePlanCompose,
        ordering: &[("template-selection", "phonology")],
    },
    SeededFamily {
        id: FAMILY_SPECIALIZED_BRANCH,
        // A "specialized branch" IS a narrower partition of the same entries over the same cascade,
        // so bisection is what this family actually names -- it was previously a fourth relabelled
        // copy of UnionPermutation, contributing nothing the baseline did not already contribute.
        applicability: Applicability::HasSplittableGateGroup,
        transform: SafeTransform::PartitionBisect,
        adapter: LoweringAdapter::ControllablePlanCompose,
        ordering: &[("branch-selection", "shared-cascade")],
    },
    SeededFamily {
        id: FAMILY_COPY_BRANCH,
        applicability: Applicability::HasReduplication,
        transform: SafeTransform::UnionPermutation,
        adapter: LoweringAdapter::ControllablePlanCompose,
        ordering: &[("copy", "repair")],
    },
    SeededFamily {
        id: FAMILY_BOUNDED_METATHESIS,
        applicability: Applicability::HasMetathesis,
        transform: SafeTransform::Identity,
        adapter: LoweringAdapter::ControllablePlanCompose,
        ordering: &[("match", "switch")],
    },
    SeededFamily {
        id: FAMILY_LAYERED_MORPHOLOGY,
        // Maximal refinement: one sub-group per entry, the many-small-unions shape. Applicability
        // moves from HasMultipleStrata to HasSplittableGateGroup because what this transform needs is
        // a splittable partition, not multiple strata -- the old predicate gated it on a property it
        // never used, which is part of why it silently reduced to the baseline everywhere.
        applicability: Applicability::HasSplittableGateGroup,
        transform: SafeTransform::PartitionFanOut,
        adapter: LoweringAdapter::ControllablePlanCompose,
        ordering: &[("lower-stratum", "upper-stratum")],
    },
    SeededFamily {
        // The OTHER whole-grammar compiler, offered explicitly rather than only reachable as a
        // post-failure rescue. On a marker-carrying grammar the baseline already falls back here, so
        // this adds nothing there; on a marker-free grammar the baseline composes its plan instead,
        // and this is the only way the surface-probed compiler ever gets compared against it.
        id: FAMILY_SURFACE_PROBE_MORPHOLOGY,
        applicability: Applicability::Always,
        transform: SafeTransform::Identity,
        adapter: LoweringAdapter::TunedSurfaceEmit,
        ordering: &[("morphology", "phonology")],
    },
    SeededFamily {
        // The first family that varies the COMPILER rather than the plan shape. Every family above
        // rewrites the assembly tree, and measurement says that cannot change the compiled network:
        // on eight marker-free fixtures all of them produced bit-identical states/arcs/proposals and
        // differed only in build time, upward. This one instead compiles the grammar to a different
        // lexc entirely -- plain char-def tokens plus a real rewrite cascade, rather than phonology
        // baked in by the surface probe and its expressive gaps patched with synthesized composite
        // entries. `transform` is `Identity` because that compiler does not interpret a plan at all.
        id: FAMILY_TOKEN_CASCADE_MORPHOLOGY,
        // Widened from `HasPhonology`: a template-bearing, phonology-free grammar (the measured
        // Sena shape) has no rewrite cascade to justify this family on `HasPhonology` alone, but its
        // morphotactics are exactly what this compiler represents faithfully and `uflexc` does not.
        // See `Applicability::HasPhonologyOrTemplates`'s doc for the full argument and why this
        // stays one family rather than two.
        applicability: Applicability::HasPhonologyOrTemplates,
        transform: SafeTransform::Identity,
        adapter: LoweringAdapter::TemplatedUnderlyingEmit,
        ordering: &[("morphotactics", "phonology")],
    },
];

pub const SEEDED_FAMILIES: &[&str] = &[
    FAMILY_ORDERED_MORPHOPHONOLOGY,
    FAMILY_CLASS_EXCEPTION_CASCADE,
    FAMILY_COMPLETE_TEMPLATE,
    FAMILY_SPECIALIZED_BRANCH,
    FAMILY_COPY_BRANCH,
    FAMILY_BOUNDED_METATHESIS,
    FAMILY_LAYERED_MORPHOLOGY,
    FAMILY_TOKEN_CASCADE_MORPHOLOGY,
    FAMILY_SURFACE_PROBE_MORPHOLOGY,
];

#[cfg(test)]
mod tests {
    use pg_grammar::model::MorphRuleDef;

    use super::*;
    use crate::capability::rhs_has_true_reduplication;
    use crate::plan::{FragmentSpec, PlanNodeKind, Provenance as PlanProvenance};

    fn family(id: &str) -> RecipeFamily {
        RecipeFamily {
            id: id.to_owned(),
            version: REGISTRY_SCHEMA_VERSION,
            parameters: vec![Parameter {
                name: "mode".to_owned(),
                domain: vec!["a".to_owned()],
                depends_on: Vec::new(),
            }],
            applicability: Applicability::Always,
            search_policy: FamilySearchPolicy::AlwaysSearch,
            ordering: Vec::new(),
            provenance: Provenance {
                source: "synthetic".to_owned(),
                note: "test".to_owned(),
                attested: false,
            },
        }
    }

    fn minimal_grammar() -> Grammar {
        pg_grammar::load(
            r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
<PhonologicalFeatureSystem><PhoneticFeature id="f"><Name>f</Name><PossibleValues><PhoneticValue id="v"><Name>v</Name></PhoneticValue></PossibleValues></PhoneticFeature></PhonologicalFeatureSystem>
<CharacterDefinitionTable id="t"><Name>T</Name><Encoding>IPA</Encoding><SegmentDefinitions><SegmentDefinition id="s"><Representation>a</Representation><FeatureValuePairs><FeatureValuePair feature="f" value="v"/></FeatureValuePairs></SegmentDefinition></SegmentDefinitions></CharacterDefinitionTable>
<Strata><Stratum characterDefinitionTable="t"><Name>S</Name><Lexicon><LexicalEntry id="e" partOfSpeech="p"><Name>e</Name><Allomorphs><Allomorph id="a"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></Lexicon></Stratum></Strata>
</Language></HermitCrabInput>"#,
        )
        .expect("synthetic grammar")
    }

    fn baseline() -> Plan {
        let mut plan = Plan::new();
        let root = plan.add_node(PlanNodeKind::Leaf {
            fragment: FragmentSpec::CompositeEmissionMarker,
            provenance: PlanProvenance::CompositeEmission,
        });
        plan.set_root(root);
        plan
    }

    /// **Task 7.13: the baseline fact is DERIVED and lives ON the candidate, and it is not position.**
    ///
    /// The two deleted shapes were a positional `i == 0` rule and a caller-supplied parallel
    /// `&[bool]`. This asserts what replaced them, and it is not vacuous: it checks that the ONE
    /// candidate whose plan is the baseline plan verbatim is the one marked `Baseline`, and that a
    /// plan-REWRITING family is marked `Alternative` even though its plan is equally applicable — a
    /// distinction position cannot express, since `materialize_distinct` orders candidates by FAMILY ID
    /// and `ordered-morphophonology` sorts after `class-exception-cascade`, `complete-template`,
    /// `copy-branch` and `bounded-metathesis`. Element zero was therefore NOT the default compilation
    /// on any grammar those families apply to, which is exactly how a permutation came to be measured
    /// on the baseline's own network and reported as confirmed.
    #[test]
    fn the_baseline_role_follows_the_baseline_plan_and_never_the_position() {
        let grammar = minimal_grammar();
        let base = baseline();
        let registry = Registry::seeded();
        let materialized = registry
            .materialize_distinct(&MaterializerContext {
                grammar: &grammar,
                baseline: &base,
            })
            .expect("the seeded registry must materialize for this grammar");
        assert!(
            materialized.len() >= 2,
            "this assertion needs at least a baseline and one non-baseline candidate to distinguish"
        );

        let baselines: Vec<&RecipeInstance> = materialized
            .iter()
            .filter(|(_, candidate)| candidate.is_baseline())
            .map(|(instance, _)| instance)
            .collect();
        assert_eq!(
            baselines.len(),
            1,
            "exactly one candidate may be this grammar's default compilation; got {baselines:?}"
        );

        for (instance, candidate) in &materialized {
            if candidate.is_baseline() {
                assert_eq!(
                    candidate.plan.root(),
                    base.root(),
                    "{}: a candidate marked Baseline must carry the baseline plan VERBATIM, or the \
                     role is a label rather than a derived fact",
                    instance.family_id
                );
                assert!(
                    candidate.adapter.interprets_plan(),
                    "{}: only the plan-interpreting adapter can be a plan's own compilation",
                    instance.family_id
                );
            } else {
                let rewrites_the_plan = candidate.plan.root() != base.root();
                let different_compiler = !candidate.adapter.interprets_plan();
                assert!(
                    rewrites_the_plan || different_compiler,
                    "{}: an Alternative must differ from the baseline in its PLAN or its COMPILER; a \
                     candidate that differs in neither is the baseline wearing a second label",
                    instance.family_id
                );
            }
        }
    }

    #[test]
    fn seeded_registry_has_stable_ids_and_metadata() {
        let registry = Registry::seeded();
        assert_eq!(
            registry
                .families()
                .map(|f| f.id.as_str())
                .collect::<BTreeSet<_>>(),
            SEEDED_FAMILIES.iter().copied().collect()
        );
        assert_eq!(
            registry.canonical_json(),
            Registry::seeded().canonical_json()
        );
        assert!(registry
            .families()
            .all(|family| !family.ordering.is_empty()));
    }

    #[test]
    fn rejects_schema_version_cycle_domain_and_unknown_instance() {
        assert!(matches!(
            Registry::new(9),
            Err(RegistryError::UnsupportedSchema(9))
        ));
        let mut cyclic = family("cycle");
        cyclic.parameters = vec![
            Parameter {
                name: "a".to_owned(),
                domain: vec!["x".to_owned()],
                depends_on: vec!["b".to_owned()],
            },
            Parameter {
                name: "b".to_owned(),
                domain: vec!["x".to_owned()],
                depends_on: vec!["a".to_owned()],
            },
        ];
        assert!(matches!(
            Registry::new(1).unwrap().validate_family(&cyclic),
            Err(RegistryError::CyclicDependency(_))
        ));
        let mut empty = family("empty");
        empty.parameters[0].domain.clear();
        assert!(matches!(
            Registry::new(1).unwrap().validate_family(&empty),
            Err(RegistryError::EmptyDomain { .. })
        ));
        assert!(matches!(
            Registry::new(1)
                .unwrap()
                .validate_instance(&RecipeInstance {
                    family_id: "missing".to_owned(),
                    parameters: BTreeMap::new()
                }),
            Err(RegistryError::UnknownFamily(_))
        ));
    }

    #[test]
    fn loaded_metadata_requires_explicit_executable_materializer() {
        let seeded = Registry::seeded();
        let loaded = Registry::load_json(&seeded.canonical_json()).expect("valid metadata");
        assert!(matches!(
            loaded.validate_ready(),
            Err(RegistryError::MissingMaterializer(_))
        ));
        let grammar = minimal_grammar();
        let base = baseline();
        let instance = loaded.instances_for_grammar(&grammar).remove(0);
        assert!(matches!(
            loaded.materialize(
                &instance,
                &MaterializerContext {
                    grammar: &grammar,
                    baseline: &base
                }
            ),
            Err(MaterializeError::MissingMaterializer(_))
        ));
    }

    #[test]
    fn extension_and_content_address_dedup_do_not_change_registry_core() {
        let grammar = minimal_grammar();
        let base = baseline();
        let mut registry = Registry::new(1).unwrap();
        let make = |label: &'static str| {
            Box::new(move |_i: &RecipeInstance, c: &MaterializerContext<'_>| {
                Ok(LoweredCandidate {
                    label,
                    plan: c.baseline.clone(),
                    adapter: LoweringAdapter::ControllablePlanCompose,
                    role: CandidateRole::Alternative,
                })
            }) as MaterializerFn
        };
        registry
            .register_family(family("extension-a"), make("extension-a"))
            .unwrap();
        registry
            .register_family(family("extension-b"), make("extension-b"))
            .unwrap();
        let materialized = registry
            .materialize_distinct(&MaterializerContext {
                grammar: &grammar,
                baseline: &base,
            })
            .unwrap();
        assert_eq!(
            materialized.len(),
            1,
            "identical Plans from different families deduplicate by root NodeId"
        );
    }
    #[test]
    fn policy_exclusions_happen_before_materialization_and_opt_in_restores_instances() {
        let grammar = minimal_grammar();
        let mut registry = Registry::new(REGISTRY_SCHEMA_VERSION).unwrap();
        let mut tie_family = family("synthetic-tie");
        tie_family.search_policy = FamilySearchPolicy::SkipOnCompositionalTopology;
        registry
            .register_family(
                tie_family,
                Box::new(
                    |_instance: &RecipeInstance, context: &MaterializerContext<'_>| {
                        Ok(LoweredCandidate {
                            label: "synthetic-tie",
                            plan: context.baseline.clone(),
                            adapter: LoweringAdapter::ControllablePlanCompose,
                            role: CandidateRole::Alternative,
                        })
                    },
                ),
            )
            .unwrap();

        let (default_instances, declared_not_searched) =
            registry.instances_for_search(&grammar, true, false);
        assert!(default_instances.is_empty());
        assert_eq!(declared_not_searched, 1);

        let (all_instances, opt_in_declared_not_searched) =
            registry.instances_for_search(&grammar, true, true);
        assert_eq!(all_instances.len(), 1);
        assert_eq!(opt_in_declared_not_searched, 0);
    }

    #[test]
    fn seeded_tie_policy_names_exactly_the_recorded_plan_rewrite_families() {
        let registry = Registry::seeded();
        let actual = registry
            .families()
            .filter(|family| {
                family.search_policy == FamilySearchPolicy::SkipOnCompositionalTopology
            })
            .map(|family| family.id.as_str())
            .collect::<BTreeSet<_>>();
        let expected = [
            FAMILY_CLASS_EXCEPTION_CASCADE,
            FAMILY_COMPLETE_TEMPLATE,
            FAMILY_SPECIALIZED_BRANCH,
            FAMILY_COPY_BRANCH,
            FAMILY_LAYERED_MORPHOLOGY,
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected);
    }

    // =============================================================================================
    // Cross-derivation agreement: the registry's grammar predicates vs. the REAL mechanisms
    //
    // Each fact below has more than one derivation in this crate, and a disagreement between them
    // silently changes which candidates a grammar is offered — a measurement-space bug that never
    // surfaces as a failure, only as a different number. These tests assert every derivation
    // against the SAME synthetic grammar so they cannot drift apart again.
    // =============================================================================================

    /// A `PhonologicalSubrule` gated purely on `requiredPartsOfSpeech`, in a grammar that declares
    /// NO `MorphologicalPhonologicalRuleFeature`s at all. Synthetic and delanguaged.
    fn pos_gated_no_mpr_features_grammar() -> Grammar {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PosGatedNoMprFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
      <PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prule1">
        <Name>posGate</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule requiredPartsOfSpeech="posV">
            <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prule1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>E1</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e2" partOfSpeech="posN">
            <Allomorphs><Allomorph id="a2"><PhoneticShape>q</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>E2</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    /// The bug this pins: `HasGatedExceptions` used to require `!grammar.mpr_features.is_empty()`,
    /// a precondition NO other derivation of the same fact carries. On this grammar
    /// `gate::find_gated_subrules` (the real mechanism every compile path uses) and
    /// `recipe_space::GrammarFacts` both report a gated subrule, while the registry refused to
    /// offer `FAMILY_CLASS_EXCEPTION_CASCADE` — two parts of the system disagreeing about the same
    /// grammar. All four assertions are made together so no single one can be "fixed" in isolation.
    #[test]
    fn pos_only_gating_without_mpr_features_agrees_across_every_derivation() {
        let g = pos_gated_no_mpr_features_grammar();
        assert!(
            g.mpr_features.is_empty(),
            "fixture premise: this grammar declares no MPR features at all"
        );

        // (a) the real mechanism
        let gated = crate::gate::find_gated_subrules(&g, &crate::enumerate::prules_in_order(&g));
        assert_eq!(
            gated.len(),
            1,
            "gate::find_gated_subrules must see the requiredPartsOfSpeech-gated subrule"
        );
        assert_eq!(
            crate::recipe_space::GrammarFacts::from_grammar(&g).gated_subrules,
            1,
            "recipe_space projects the same mechanism and must report the same count"
        );

        // (b) the registry predicate
        assert!(
            Applicability::HasGatedExceptions.matches(&g),
            "HasGatedExceptions must be a projection of gate::find_gated_subrules, not a \
             reimplementation with an mpr_features precondition of its own"
        );

        // (c) the family the predicate gates
        let offered = Registry::seeded()
            .instances_for_grammar(&g)
            .into_iter()
            .map(|instance| instance.family_id)
            .collect::<BTreeSet<_>>();
        assert!(
            offered.contains(FAMILY_CLASS_EXCEPTION_CASCADE),
            "a genuinely gated grammar must be offered {FAMILY_CLASS_EXCEPTION_CASCADE}; offered: \
             {offered:?}"
        );
    }

    /// One `MorphologicalRule` whose output carries a non-default `redupMorphType`, but whose RHS
    /// copies the single input part exactly ONCE — no part is echoed, so nothing here reduplicates.
    /// `echoed` switches the RHS to copy that part twice, which IS reduplication.
    fn redup_hint_grammar(echoed: bool) -> Grammar {
        let copies = if echoed {
            r#"<CopyFromInput index="stem" /><CopyFromInput index="stem" />"#
        } else {
            r#"<CopyFromInput index="stem" /><InsertSegments><PhoneticShape>q</PhoneticShape></InsertSegments>"#
        };
        let xml = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>RedupHintFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="c1" /><Segment segment="c2" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mr1">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>redupish</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub1">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput redupMorphType="prefix">{copies}</MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <Gloss>RED</Gloss>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>E1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
        );
        pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    /// The bug this pins: `Applicability::HasReduplication` and
    /// `recipe_space::reduplication_count` both used to fire on ANY non-`Implicit` `redup_hint`,
    /// the exact trap `capability::rhs_has_true_reduplication`'s doc names — `Implicit` is the
    /// DTD's default for every ordinary affix, so a merely non-default hint says nothing about
    /// whether a part is actually echoed. `FAMILY_COPY_BRANCH` was therefore offered to grammars
    /// with no reduplication in them, and `GrammarFacts.reduplicative_allomorphs` counted them.
    #[test]
    fn non_default_redup_hint_without_an_echoed_part_is_reduplication_free_everywhere() {
        let g = redup_hint_grammar(false);
        let allomorph = match &g.mrules[0] {
            MorphRuleDef::AffixProcess(def) => &def.allomorphs[0],
            other => panic!("fixture premise: expected an AffixProcess rule, got {other:?}"),
        };
        assert_ne!(
            allomorph.redup_hint,
            pg_grammar::model::ReduplicationHint::Implicit,
            "fixture premise: the hint must be non-default"
        );

        // (a) the authority
        assert!(
            !rhs_has_true_reduplication(&allomorph.rhs),
            "no input part is echoed twice, so this is not reduplication"
        );
        // (b) the registry predicate
        assert!(
            !Applicability::HasReduplication.matches(&g),
            "HasReduplication must consume rhs_has_true_reduplication, not the redup_hint"
        );
        // (c) the recipe-space count
        assert_eq!(
            crate::recipe_space::GrammarFacts::from_grammar(&g).reduplicative_allomorphs,
            0,
            "reduplication_count must consume rhs_has_true_reduplication, not the redup_hint"
        );
        // (d) the family the predicate gates
        let offered = Registry::seeded()
            .instances_for_grammar(&g)
            .into_iter()
            .map(|instance| instance.family_id)
            .collect::<BTreeSet<_>>();
        assert!(
            !offered.contains(FAMILY_COPY_BRANCH),
            "a reduplication-free grammar must not be offered {FAMILY_COPY_BRANCH}; offered: \
             {offered:?}"
        );
    }

    /// The other half of the same contract: a genuinely reduplicating grammar (one input part
    /// echoed twice) must still be detected by all three derivations, so the fix above is a
    /// narrowing of a false positive and not a loss of the real signal.
    #[test]
    fn a_genuinely_echoed_part_is_reduplication_everywhere() {
        let g = redup_hint_grammar(true);
        let allomorph = match &g.mrules[0] {
            MorphRuleDef::AffixProcess(def) => &def.allomorphs[0],
            other => panic!("fixture premise: expected an AffixProcess rule, got {other:?}"),
        };

        assert!(rhs_has_true_reduplication(&allomorph.rhs));
        assert!(Applicability::HasReduplication.matches(&g));
        assert_eq!(
            crate::recipe_space::GrammarFacts::from_grammar(&g).reduplicative_allomorphs,
            1
        );
        let offered = Registry::seeded()
            .instances_for_grammar(&g)
            .into_iter()
            .map(|instance| instance.family_id)
            .collect::<BTreeSet<_>>();
        assert!(
            offered.contains(FAMILY_COPY_BRANCH),
            "a genuinely reduplicating grammar must be offered {FAMILY_COPY_BRANCH}; offered: \
             {offered:?}"
        );
    }
}
