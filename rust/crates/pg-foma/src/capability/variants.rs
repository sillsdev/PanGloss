//! Predicate-owned identities for reachable construct variants and their observed verdicts.

use super::*;

/// A concrete admission outcome, independent of the kind's default predicate requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariantDisposition {
    Admit,
    ConfirmOnly,
    Refuse,
}

impl From<&PredicateVerdict> for VariantDisposition {
    fn from(verdict: &PredicateVerdict) -> Self {
        match verdict {
            PredicateVerdict::Admit => Self::Admit,
            PredicateVerdict::ConfirmOnly => Self::ConfirmOnly,
            PredicateVerdict::Refuse(_) => Self::Refuse,
        }
    }
}

/// Concrete forms owned by admission predicates; vacuous absence supplies no witness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConstructVariant {
    CompoundingNonRecursive,
    CompoundingRecursive,
    UnorderedOrderUnion,
    MprOverwrite,
    EpenthesisStructural,
    CircumfixStructural,
    CircumfixUnrouted,
    ReduplicationPeel,
    ReduplicationStructural,
    ReduplicationUnrouted,
    MultiTableDisjoint,
    MultiTableShared,
}

impl ConstructVariant {
    /// Stable fixture tag and ledger identity.
    pub fn id(self) -> &'static str {
        match self {
            Self::CompoundingNonRecursive => "compounding.non-recursive",
            Self::CompoundingRecursive => "compounding.recursive",
            Self::UnorderedOrderUnion => "unordered-application.order-union",
            Self::MprOverwrite => "mpr-group.overwrite",
            Self::EpenthesisStructural => "epenthesis.structural-route",
            Self::CircumfixStructural => "circumfix.structural-route",
            Self::CircumfixUnrouted => "circumfix.no-structural-route",
            Self::ReduplicationPeel => "reduplication.peel-route",
            Self::ReduplicationStructural => "reduplication.structural-route",
            Self::ReduplicationUnrouted => "reduplication.no-proposal-route",
            Self::MultiTableDisjoint => "multi-table.disjoint-representations",
            Self::MultiTableShared => "multi-table.shared-representations",
        }
    }

    pub fn kind(self) -> CharacteristicKind {
        match self {
            Self::CompoundingNonRecursive => CharacteristicKind::Compounding,
            Self::CompoundingRecursive => CharacteristicKind::Compounding,
            Self::UnorderedOrderUnion => CharacteristicKind::UnorderedMorphRuleApplication,
            Self::MprOverwrite => CharacteristicKind::MprGroupOverwrite,
            Self::EpenthesisStructural => CharacteristicKind::Epenthesis,
            Self::CircumfixStructural => CharacteristicKind::CircumfixOutputAction,
            Self::CircumfixUnrouted => CharacteristicKind::CircumfixOutputAction,
            Self::ReduplicationPeel => CharacteristicKind::Reduplication,
            Self::ReduplicationStructural => CharacteristicKind::Reduplication,
            Self::ReduplicationUnrouted => CharacteristicKind::Reduplication,
            Self::MultiTableDisjoint => CharacteristicKind::MultiTable,
            Self::MultiTableShared => CharacteristicKind::MultiTable,
        }
    }

    /// The predicate outcome this variant's evidence must demonstrate.
    pub fn disposition(self) -> VariantDisposition {
        match self {
            Self::CompoundingNonRecursive => VariantDisposition::ConfirmOnly,
            Self::CompoundingRecursive => VariantDisposition::ConfirmOnly,
            Self::UnorderedOrderUnion => VariantDisposition::ConfirmOnly,
            Self::MprOverwrite => VariantDisposition::ConfirmOnly,
            Self::EpenthesisStructural => VariantDisposition::ConfirmOnly,
            Self::CircumfixStructural => VariantDisposition::ConfirmOnly,
            Self::CircumfixUnrouted => VariantDisposition::Refuse,
            Self::ReduplicationPeel => VariantDisposition::ConfirmOnly,
            Self::ReduplicationStructural => VariantDisposition::ConfirmOnly,
            Self::ReduplicationUnrouted => VariantDisposition::Refuse,
            Self::MultiTableDisjoint => VariantDisposition::ConfirmOnly,
            Self::MultiTableShared => VariantDisposition::ConfirmOnly,
        }
    }

    /// Permanent authoring-based refusal, when one has been established.
    pub fn permanent_refusal_reason(self) -> Option<&'static str> {
        match self {
            Self::CircumfixUnrouted | Self::ReduplicationUnrouted => Some(
                "not authorable in FieldWorks; HC-XML only \
                 (docs/divergences/059-fieldworks-unauthorable-variants.md)",
            ),
            _ => None,
        }
    }
}

/// Calls each occurrence's predicate, including lowering; missing or inconsistent inventories panic.
pub fn observed_variants(
    grammar: &Grammar,
    registry: &PredicateRegistry,
) -> HashSet<ConstructVariant> {
    let profile = characterize(grammar);
    let mut observed = HashSet::new();
    for observation in profile
        .observations()
        .iter()
        .filter(|o| o.disposition == Disposition::ConfigPredicate)
    {
        let owners: Vec<_> = registry
            .predicates()
            .iter()
            .filter(|p| p.discharges().contains(&observation.kind))
            .collect();
        assert!(
            !owners.is_empty(),
            "no predicate owns observed variant of {:?}",
            observation.kind
        );
        let local = CharacteristicsProfile {
            observations: vec![observation.clone()],
            cardinality: profile.cardinality,
        };
        let fragment = match observation.location {
            ModelLocation::PhonRule(rule) | ModelLocation::RewriteSubrule { rule, .. } => {
                FragmentSpec::RewriteRule { rule }
            }
            _ => FragmentSpec::StructuralCompositeMarker,
        };
        let node = PlanNodeKind::Leaf {
            fragment,
            provenance: crate::plan::Provenance::StructuralComposite,
        };
        for owner in owners {
            let verdict = owner.evaluate(grammar, &local, &node);
            let variant = owner.variant_for(grammar, observation, &verdict);
            assert!(
                owner.variants().contains(&variant),
                "{} returned an unregistered variant",
                owner.id()
            );
            assert_eq!(variant.kind(), observation.kind);
            assert_eq!(
                variant.disposition(),
                VariantDisposition::from(&verdict),
                "{} variant disagrees with its admission verdict",
                variant.id()
            );
            observed.insert(variant);
        }
    }
    observed
}
