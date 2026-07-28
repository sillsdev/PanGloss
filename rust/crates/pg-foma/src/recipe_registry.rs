//! Validated, extensible recipe-family registry backed by executable grammar predicates and
//! semantics-preserving transforms of a real compilation Plan.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use pg_grammar::model::{Grammar, MorphRuleDef, OutputAction, PhonRuleDef, ReduplicationHint};
use serde::{Deserialize, Serialize};

use crate::enumerate::CandidatePlan;
use crate::oracle::{permute_gate_groups, permute_union_children};
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
    HasGatedExceptions,
    HasTemplates,
    HasMorphology,
    HasReduplication,
    HasMetathesis,
    HasMultipleStrata,
}

impl Applicability {
    pub fn matches(&self, grammar: &Grammar) -> bool {
        match self {
            Self::Always => true,
            Self::HasGatedExceptions => {
                !grammar.mpr_features.is_empty()
                    && grammar.prules.iter().any(|rule| match rule {
                        PhonRuleDef::Rewrite(rewrite) => rewrite.subrules.iter().any(|subrule| {
                            subrule.required_pos.is_some()
                                || !subrule.required_mpr.is_empty()
                                || !subrule.excluded_mpr.is_empty()
                        }),
                        PhonRuleDef::Metathesis(_) => false,
                    })
            }
            Self::HasTemplates => !grammar.templates.is_empty(),
            Self::HasMorphology => !grammar.mrules.is_empty(),
            Self::HasReduplication => grammar.mrules.iter().any(rule_has_reduplication),
            Self::HasMetathesis => grammar
                .prules
                .iter()
                .any(|rule| matches!(rule, PhonRuleDef::Metathesis(_))),
            Self::HasMultipleStrata => grammar.strata.len() > 1,
        }
    }
}

fn rule_has_reduplication(rule: &MorphRuleDef) -> bool {
    let allomorphs = match rule {
        MorphRuleDef::AffixProcess(def) => &def.allomorphs,
        MorphRuleDef::Realizational(def) => &def.allomorphs,
        MorphRuleDef::Compounding(_) => return false,
    };
    allomorphs.iter().any(|allomorph| {
        !matches!(allomorph.redup_hint, ReduplicationHint::Implicit) || {
            let copies = allomorph
                .rhs
                .iter()
                .filter(|action| matches!(action, OutputAction::Copy(_)))
                .count();
            copies > allomorph.lhs.len()
        }
    })
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeFamily {
    pub id: String,
    pub version: u16,
    pub parameters: Vec<Parameter>,
    pub applicability: Applicability,
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
    ) -> Result<CandidatePlan, MaterializeError>;
}

impl<F> Materializer for F
where
    F: for<'a> Fn(
            &RecipeInstance,
            &MaterializerContext<'a>,
        ) -> Result<CandidatePlan, MaterializeError>
        + Send
        + Sync,
{
    fn materialize(
        &self,
        instance: &RecipeInstance,
        context: &MaterializerContext<'_>,
    ) -> Result<CandidatePlan, MaterializeError> {
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
        self.instances_matching(|family| family.applicability.matches(grammar))
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
    ) -> Result<CandidatePlan, MaterializeError> {
        self.validate_instance(instance)
            .map_err(|error| MaterializeError::Invalid(error.to_string()))?;
        let family = self
            .family(&instance.family_id)
            .expect("validated instance has a family");
        if !family.applicability.matches(context.grammar) {
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
    pub fn materialize_distinct(
        &self,
        context: &MaterializerContext<'_>,
    ) -> Result<Vec<(RecipeInstance, CandidatePlan)>, MaterializeError> {
        let mut roots = BTreeSet::<NodeId>::new();
        let mut candidates = Vec::new();
        for instance in self.instances_for_grammar(context.grammar) {
            let candidate = self.materialize(&instance, context)?;
            let root = candidate
                .plan
                .root()
                .ok_or_else(|| MaterializeError::RootlessPlan(instance.family_id.clone()))?;
            if roots.insert(root) {
                candidates.push((instance, candidate));
            }
        }
        Ok(candidates)
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

#[derive(Debug, Clone, Copy)]
enum SafeTransform {
    Identity,
    GatePermutation,
    UnionPermutation,
}

#[derive(Debug, Clone, Copy)]
struct SeededFamily {
    id: &'static str,
    applicability: Applicability,
    transform: SafeTransform,
    ordering: &'static [(&'static str, &'static str)],
}

impl SeededFamily {
    fn family(self) -> RecipeFamily {
        RecipeFamily {
            id: self.id.to_owned(),
            version: REGISTRY_SCHEMA_VERSION,
            parameters: vec![Parameter {
                name: "topology".to_owned(),
                domain: vec![match self.transform {
                    SafeTransform::Identity => "baseline",
                    SafeTransform::GatePermutation => "gate-permutation",
                    SafeTransform::UnionPermutation => "union-permutation",
                }
                .to_owned()],
                depends_on: Vec::new(),
            }],
            applicability: self.applicability,
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
    ) -> Result<CandidatePlan, MaterializeError> {
        let plan = match self.transform {
            SafeTransform::Identity => context.baseline.clone(),
            SafeTransform::GatePermutation => permute_gate_groups(context.baseline),
            SafeTransform::UnionPermutation => permute_union_children(context.baseline),
        };
        Ok(CandidatePlan {
            label: self.id,
            plan,
        })
    }
}

const SEEDS: &[SeededFamily] = &[
    SeededFamily {
        id: "ordered-morphophonology",
        applicability: Applicability::Always,
        transform: SafeTransform::Identity,
        ordering: &[("morphology", "phonology")],
    },
    SeededFamily {
        id: "class-exception-cascade",
        applicability: Applicability::HasGatedExceptions,
        transform: SafeTransform::GatePermutation,
        ordering: &[("class-partition", "exception-cascade")],
    },
    SeededFamily {
        id: "complete-template",
        applicability: Applicability::HasTemplates,
        transform: SafeTransform::UnionPermutation,
        ordering: &[("template-selection", "phonology")],
    },
    SeededFamily {
        id: "specialized-branch",
        applicability: Applicability::HasMorphology,
        transform: SafeTransform::UnionPermutation,
        ordering: &[("branch-selection", "shared-cascade")],
    },
    SeededFamily {
        id: "copy-branch",
        applicability: Applicability::HasReduplication,
        transform: SafeTransform::UnionPermutation,
        ordering: &[("copy", "repair")],
    },
    SeededFamily {
        id: "bounded-metathesis",
        applicability: Applicability::HasMetathesis,
        transform: SafeTransform::Identity,
        ordering: &[("match", "switch")],
    },
    SeededFamily {
        id: "layered-morphology",
        applicability: Applicability::HasMultipleStrata,
        transform: SafeTransform::UnionPermutation,
        ordering: &[("lower-stratum", "upper-stratum")],
    },
];

pub const SEEDED_FAMILIES: &[&str] = &[
    "ordered-morphophonology",
    "class-exception-cascade",
    "complete-template",
    "specialized-branch",
    "copy-branch",
    "bounded-metathesis",
    "layered-morphology",
];

#[cfg(test)]
mod tests {
    use super::*;
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
                Ok(CandidatePlan {
                    label,
                    plan: c.baseline.clone(),
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
}
