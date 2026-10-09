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
    SimultaneousDisjoint,
    SimultaneousUnproven,
    RtlReversal,
    RtlUnlowerable,
    MetathesisLtrSwap,
    MetathesisRtlSwap,
    MetathesisLtrUnlowerable,
    MetathesisRtlUnlowerable,
    EpenthesisStructural,
    CircumfixStructural,
    CircumfixUnrouted,
    ReduplicationPeel,
    ReduplicationStructural,
    ReduplicationUnrouted,
    MultiTableDisjoint,
    MultiTableShared,
    QuantifierBounded,
    QuantifierUnbounded,
    QuantifierBoundedUnlowerable,
    QuantifierUnboundedUnlowerable,
}

impl ConstructVariant {
    /// Stable fixture tag and ledger identity.
    pub fn id(self) -> &'static str {
        match self {
            Self::CompoundingNonRecursive => "compounding.non-recursive",
            Self::CompoundingRecursive => "compounding.recursive",
            Self::UnorderedOrderUnion => "unordered-application.order-union",
            Self::MprOverwrite => "mpr-group.overwrite",
            Self::SimultaneousDisjoint => "simultaneous.proven-disjoint",
            Self::SimultaneousUnproven => "simultaneous.unproven-pair",
            Self::RtlReversal => "right-to-left-rewrite.reversal",
            Self::RtlUnlowerable => "right-to-left-rewrite.unlowerable",
            Self::MetathesisLtrSwap => "metathesis.left-to-right-swap",
            Self::MetathesisRtlSwap => "metathesis.right-to-left-swap",
            Self::MetathesisLtrUnlowerable => "metathesis.left-to-right-unlowerable",
            Self::MetathesisRtlUnlowerable => "metathesis.right-to-left-unlowerable",
            Self::EpenthesisStructural => "epenthesis.structural-route",
            Self::CircumfixStructural => "circumfix.structural-route",
            Self::CircumfixUnrouted => "circumfix.no-structural-route",
            Self::ReduplicationPeel => "reduplication.peel-route",
            Self::ReduplicationStructural => "reduplication.structural-route",
            Self::ReduplicationUnrouted => "reduplication.no-proposal-route",
            Self::MultiTableDisjoint => "multi-table.disjoint-representations",
            Self::MultiTableShared => "multi-table.shared-representations",
            Self::QuantifierBounded => "quantifier.bounded-compilable",
            Self::QuantifierUnbounded => "quantifier.unbounded-compilable",
            Self::QuantifierBoundedUnlowerable => "quantifier.bounded-unlowerable",
            Self::QuantifierUnboundedUnlowerable => "quantifier.unbounded-unlowerable",
        }
    }

    pub fn kind(self) -> CharacteristicKind {
        match self {
            Self::CompoundingNonRecursive => CharacteristicKind::Compounding,
            Self::CompoundingRecursive => CharacteristicKind::Compounding,
            Self::UnorderedOrderUnion => CharacteristicKind::UnorderedMorphRuleApplication,
            Self::MprOverwrite => CharacteristicKind::MprGroupOverwrite,
            Self::SimultaneousDisjoint => CharacteristicKind::SimultaneousRewrite,
            Self::SimultaneousUnproven => CharacteristicKind::SimultaneousRewrite,
            Self::RtlReversal => CharacteristicKind::RightToLeftRewrite,
            Self::RtlUnlowerable => CharacteristicKind::RightToLeftRewrite,
            Self::MetathesisLtrSwap => CharacteristicKind::Metathesis,
            Self::MetathesisRtlSwap => CharacteristicKind::Metathesis,
            Self::MetathesisLtrUnlowerable => CharacteristicKind::Metathesis,
            Self::MetathesisRtlUnlowerable => CharacteristicKind::Metathesis,
            Self::EpenthesisStructural => CharacteristicKind::Epenthesis,
            Self::CircumfixStructural => CharacteristicKind::CircumfixOutputAction,
            Self::CircumfixUnrouted => CharacteristicKind::CircumfixOutputAction,
            Self::ReduplicationPeel => CharacteristicKind::Reduplication,
            Self::ReduplicationStructural => CharacteristicKind::Reduplication,
            Self::ReduplicationUnrouted => CharacteristicKind::Reduplication,
            Self::MultiTableDisjoint => CharacteristicKind::MultiTable,
            Self::MultiTableShared => CharacteristicKind::MultiTable,
            Self::QuantifierBounded => CharacteristicKind::QuantifierPattern,
            Self::QuantifierUnbounded => CharacteristicKind::QuantifierPattern,
            Self::QuantifierBoundedUnlowerable => CharacteristicKind::QuantifierPattern,
            Self::QuantifierUnboundedUnlowerable => CharacteristicKind::QuantifierPattern,
        }
    }

    /// The predicate outcome this variant's evidence must demonstrate.
    pub fn disposition(self) -> VariantDisposition {
        match self {
            Self::CompoundingNonRecursive => VariantDisposition::ConfirmOnly,
            Self::CompoundingRecursive => VariantDisposition::ConfirmOnly,
            Self::UnorderedOrderUnion => VariantDisposition::ConfirmOnly,
            Self::MprOverwrite => VariantDisposition::ConfirmOnly,
            Self::SimultaneousDisjoint => VariantDisposition::Admit,
            Self::SimultaneousUnproven => VariantDisposition::Refuse,
            Self::RtlReversal => VariantDisposition::ConfirmOnly,
            Self::RtlUnlowerable => VariantDisposition::Refuse,
            Self::MetathesisLtrSwap => VariantDisposition::ConfirmOnly,
            Self::MetathesisRtlSwap => VariantDisposition::ConfirmOnly,
            Self::MetathesisLtrUnlowerable => VariantDisposition::Refuse,
            Self::MetathesisRtlUnlowerable => VariantDisposition::Refuse,
            Self::EpenthesisStructural => VariantDisposition::ConfirmOnly,
            Self::CircumfixStructural => VariantDisposition::ConfirmOnly,
            Self::CircumfixUnrouted => VariantDisposition::Refuse,
            Self::ReduplicationPeel => VariantDisposition::ConfirmOnly,
            Self::ReduplicationStructural => VariantDisposition::ConfirmOnly,
            Self::ReduplicationUnrouted => VariantDisposition::Refuse,
            Self::MultiTableDisjoint => VariantDisposition::ConfirmOnly,
            Self::MultiTableShared => VariantDisposition::ConfirmOnly,
            Self::QuantifierBounded => VariantDisposition::ConfirmOnly,
            Self::QuantifierUnbounded => VariantDisposition::ConfirmOnly,
            Self::QuantifierBoundedUnlowerable => VariantDisposition::Refuse,
            Self::QuantifierUnboundedUnlowerable => VariantDisposition::Refuse,
        }
    }

    /// Permanent authoring-based refusal, when one has been established.
    pub fn permanent_refusal_reason(self) -> Option<&'static str> {
        match self {
            Self::CircumfixUnrouted => Some(
                "not authorable in FieldWorks; HC-XML only (HCLoader.cs:1273-1311,1334-1420; \
                 emission_support.rs:248-251,273-335,357-370,424-471; \
                 capability.rs:970-982,1135-1147,2700-2719; emit.rs:2958-2986)",
            ),
            Self::ReduplicationUnrouted => {
                Some("not authorable in FieldWorks; HC-XML only (HCLoader.cs:976-979)")
            }
            Self::MetathesisLtrUnlowerable => Some(
                "not authorable in FieldWorks; HC-XML only (FieldWorks/Src/LexText/Morphology/\
                 MetaRuleFormulaControl.cs:56-62,402-427; \
                 FieldWorks/Src/LexText/ParserCore/HCLoader.cs:2103-2150; \
                 LT-22826 (alpha-variable HCLoader null-reference crash, \
                 https://jira.sil.org/browse/LT-22826); LT-22827 (out-of-range switch index emits \
                 malformed HC XML, https://jira.sil.org/browse/LT-22827); \
                 machine/src/SIL.Machine.Morphology.HermitCrab/PhonologicalRules/\
                 AnalysisMetathesisRuleSpec.cs:20-52; \
                 /tmp/pangloss-lanes/metathesis-check/report.md, cases/final/optional-ltr and \
                 repeated-ltr; rust/crates/pg-foma/src/replace.rs:1175-1240,1310-1330; \
                 rust/crates/pg-foma/src/capability.rs:822-849)",
            ),
            Self::MetathesisRtlUnlowerable => Some(
                "not authorable in FieldWorks; HC-XML only (FieldWorks/Src/LexText/Morphology/\
                 MetaRuleFormulaControl.cs:56-62,402-427; \
                 FieldWorks/Src/LexText/ParserCore/HCLoader.cs:2103-2150; \
                 LT-22826 (alpha-variable HCLoader null-reference crash, \
                 https://jira.sil.org/browse/LT-22826); LT-22827 (out-of-range switch index emits \
                 malformed HC XML, https://jira.sil.org/browse/LT-22827); \
                 machine/src/SIL.Machine.Morphology.HermitCrab/PhonologicalRules/\
                 AnalysisMetathesisRuleSpec.cs:20-52; \
                 /tmp/pangloss-lanes/metathesis-check/report.md, cases/final/optional-rtl and \
                 repeated-rtl; rust/crates/pg-foma/src/replace.rs:1175-1240,1310-1330; \
                 rust/crates/pg-foma/src/capability.rs:822-849)",
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
