//! Generic construct-id mapping and kind-level projections, alongside shared coverage classification.

use std::collections::HashSet;

use crate::capability::{CharacteristicKind, Disposition};
use pg_grammar::model::{Grammar, MorphRuleOrder, MprGroupOutput, PhonRuleDef};

/// Generic upstream construct tags for a kind, matched exhaustively.
/// Concrete predicate variants use their own identities rather than inheriting these tags.
pub fn construct_ids_for(kind: CharacteristicKind) -> &'static [&'static str] {
    use CharacteristicKind::*;
    match kind {
        Affixation => &[
            "AffixProcessRule: prefix/suffix/circumfix/infix",
            "AffixProcessRule: subtraction/truncation",
        ],
        RealizationalMorphology => &["RealizationalAffixProcessRule"],
        Compounding => &["CompoundingRule"],
        OrderedMorphRuleApplication => &["Stratum (Linear/Unordered rule order)"],
        UnorderedMorphRuleApplication => &["Stratum (Linear/Unordered rule order)"],
        MprGroupAppend => &["MPR features/groups"],
        MprGroupOverwrite => &["MPR features/groups"],
        IterativeRewrite => {
            &["RewriteRule Iterative (epenthesis/deletion/feature/expansion/merge)"]
        }
        SimultaneousRewrite => &["RewriteRule Simultaneous"],
        LeftToRightRewrite => &["RewriteRule direction (Dir): left-to-right"],
        RightToLeftRewrite => &["RewriteRule direction (Dir): right-to-left"],
        Metathesis => &["MetathesisRule"],
        Epenthesis => &["RewriteRule Iterative (epenthesis/deletion/feature/expansion/merge)"],
        SubruleGating => {
            &["RewriteSubruleDef gating: required/excluded POS or MPR at the subrule level"]
        }
        CircumfixOutputAction => &["AffixProcessRule: prefix/suffix/circumfix/infix"],
        Reduplication => &["AffixProcessRule: reduplication (ReduplicationHint)"],
        CoOccurrenceConstraint => &["MorphemeCoOccurrenceRule/AllomorphCoOccurrenceRule"],
        NaturalClassDefinition => {
            &["NaturalClass: Segments vs FeatureNaturalClass/SegmentNaturalClass precision"]
        }
        MultiTable => &["CharacterDefinitionTable: more than one table, one per stratum"],
        // Its own row, not MultiTable's: a multi-table fixture whose inner roots both tables spell alike would otherwise inherit coverage for a construct it never exhibits.
        CrossTableRespelling => &[
            "CharacterDefinitionTable: cross-table respelling (an inner-stratum root surfaces spelled by the final stratum's table)",
        ],
        QuantifierPattern => {
            &["CharacterDefinitionTable pattern shapes: optional group / Kleene star"]
        }
        StemName => &["Stem names"],
        FreeFluctuation => &["Disjunctive allomorphs / free-fluctuation"],
        ProcessMorphology => &["MorphologicalOutputAction: ModifyFromInput/InsertSimpleContext"],
    }
}

/// Upstream construct tags outside this capability inventory, with their scope rationale.
pub const ORPHAN_CONSTRUCT_ROWS: &[(&str, &str)] = &[
    (
        "MorphologicalOutputAction: CopyFromInput/InsertSegments",
        "an ordinary output-action primitive already exercised as part of Affixation/\
         CircumfixOutputAction coverage; the capability inventory has no distinct admission-gated characteristic \
         for individual OutputAction variants (they collapse into whichever MorphRuleDef/\
         AffixAllomorphDef characteristic uses them).",
    ),
    (
        "Affix template slots (obligatory/disjunctive/ordering)",
        "a morphotactic templating detail already covered generically by Affixation/\
         OrderedMorphRuleApplication proposing; no distinct CharacteristicKind carves out \
         slot-level obligatoriness/ordering as its own admission-filtered phenomenon.",
    ),
    (
        "Boundary markers (CharacterDefinitionTable)",
        "segmentation/tokenization detail within one CharacterDefinitionTable -- purely \
         representational (how raw input is chunked into segments), not itself a proposer/confirm \
         capability axis the capability inventory gates on (same standing as NaturalClassDefinition's own \
         \"representational only, no capability implication either way\" framing, just not given \
         its own variant).",
    ),
    (
        "Guesser/LexicalGuess",
        "a runtime unknown-word heuristic (HC's guesser subsystem), not a compiled-FST capability; \
         no model.rs construct this crate's characterize() walks corresponds to it at all -- \
         outside this crate's characterization scope entirely.",
    ),
    (
        "Syntactic feature agreement (RequiredHeadFeatures/OutputHeadFeatures/RequiredFootFeatures/\
         OutputFootFeatures)",
        "ordinary confirm-time feature-unification, already assumed faithful per \
         IMPLEMENTATION-READINESS.md R1 (\"HermitCrab and the Rust model are assumed complete \
         apart from bug fixes\"); not a distinct FST-compilation capability axis this crate \
         characterizes.",
    ),
    (
        "Alpha-variable phonological environments (VariableFeature/AlphaVariables)",
        "alpha-variable (feature-agreement) environment matching is a rewrite-rule environment \
         detail, already covered generically by IterativeRewrite/SimultaneousRewrite fixtures; no \
         distinct CharacteristicKind carves out alpha-variable binding as its own admission-gated \
         phenomenon.",
    ),
    (
        "CompoundingRule constraints (MaxApplicationCount/Blockable/head-nonhead syntactic \
         features)",
        "a more granular constraint-configuration variant of CharacteristicKind::Compounding's own \
         construct (the plain \"CompoundingRule\" row); CompoundingRecursionSafePredicate's \
         admit/confirm/refuse split doesn't distinguish these sub-configurations as their own \
         shape, so no separate characteristic was invented for them.",
    ),
    (
        "Ordinary/realizational rule constraints (MaxApplicationCount/RequiredStemName/Blockable)",
        "a constraint-configuration variant of Affixation/RealizationalMorphology's own construct; \
         MaxApplicationCount/RequiredStemName/Blockable gate WHETHER an already-characterized rule \
         applies, they don't introduce a new FST-compilation shape of their own.",
    ),
    (
        "Tracing (TraceType)",
        "an engine debug/diagnostic feature (HC's trace output for developers), not a \
         grammar-compilation capability -- the typology research backing the capability inventory found no \
         capability basis for tracing as its own characteristic.",
    ),
];

/// Every `CharacteristicKind` the cross-check reasons over — literally `CharacteristicKind::ALL`,
/// not the narrower `Disposition::Proven` subset, which made the cross-check vacuous for most of
/// the ledger. Kept as a named function (rather than every caller reaching
/// for `CharacteristicKind::ALL` directly) so call sites read as making a deliberate scope choice,
/// matching this file's own established naming.
pub fn supported_kinds() -> Vec<CharacteristicKind> {
    CharacteristicKind::ALL.to_vec()
}

/// One `CharacteristicKind`'s cross-check outcome (deliverable 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageStatus {
    /// At least one of `construct_ids_for`'s ids is in the caller's passing-covered set.
    Covered,
    /// `construct_ids_for` names at least one construct id, but none is in the caller's passing-covered set: supported without a covering, passing fixture.
    Uncovered,
    /// `construct_ids_for` returns an empty slice for this kind: no `constructs.txt` identifier corresponds to it, a gap in the mapping contract itself, distinct from `Uncovered`.
    Unmappable,
}

/// Generic kind coverage, independent of any concrete predicate variant's completeness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageReportRow {
    pub kind: CharacteristicKind,
    pub disposition: Disposition,
    pub status: CoverageStatus,
    pub construct_ids: &'static [&'static str],
}

/// Advisory generic kind coverage; concrete variant obligations are enumerated by the ledger.
pub fn supported_coverage_report(
    passing_covered_constructs: &HashSet<&str>,
) -> Vec<CoverageReportRow> {
    supported_kinds()
        .into_iter()
        .map(|kind| {
            let construct_ids = construct_ids_for(kind);
            let status = coverage_status_for_ids(construct_ids, passing_covered_constructs);
            CoverageReportRow {
                kind,
                disposition: kind.default_disposition(),
                status,
                construct_ids,
            }
        })
        .collect()
}

/// Classifies one obligation against tags collected from passing fixture words.
pub fn coverage_status_for_ids(ids: &[&str], passing: &HashSet<&str>) -> CoverageStatus {
    if ids.is_empty() {
        CoverageStatus::Unmappable
    } else if ids.iter().any(|id| passing.contains(id)) {
        CoverageStatus::Covered
    } else {
        CoverageStatus::Uncovered
    }
}

/// Convenience projection of `supported_coverage_report`: every `CharacteristicKind` whose
/// status is NOT `CoverageStatus::Covered` (i.e. `Uncovered` or `Unmappable` — both are "not
/// demonstrably covered by the evidence its own disposition demands today", the task's "gaps").
/// `tests/conformance_coverage_gate.rs` asserts this is empty against the real conformance corpus
/// and prints the full report either way.
pub fn supported_uncovered(passing_covered_constructs: &HashSet<&str>) -> Vec<CharacteristicKind> {
    supported_coverage_report(passing_covered_constructs)
        .into_iter()
        .filter(|row| row.status != CoverageStatus::Covered)
        .map(|row| row.kind)
        .collect()
}

/// Generic tags shared by multiple kinds, derived from the mapping in stable tag order.
pub fn shared_construct_ids() -> Vec<(&'static str, Vec<CharacteristicKind>)> {
    let mut by_id: std::collections::BTreeMap<&'static str, Vec<CharacteristicKind>> =
        std::collections::BTreeMap::new();
    for &kind in CharacteristicKind::ALL {
        for &id in construct_ids_for(kind) {
            by_id.entry(id).or_default().push(kind);
        }
    }
    by_id
        .into_iter()
        .filter(|(_, kinds)| kinds.len() > 1)
        .collect()
}

/// One at-risk shared id's structural witness: a predicate over the LOADED grammar model that
/// decides whether a grammar structurally exhibits `finer_kind` — independent of any
/// `exercises:` tag. See `registered_structural_witnesses` for today's three, and each
/// predicate function's own doc for exactly where it reads its facts from.
pub struct StructuralWitness {
    pub construct_id: &'static str,
    /// The finer characteristic sharing `construct_id` that this predicate specifically pins
    /// structural evidence for. The coarser sibling needs no predicate of its own — an ordinary
    /// passing fixture already proves it, with no inheritance risk in that direction.
    pub finer_kind: CharacteristicKind,
    pub predicate: fn(&Grammar) -> bool,
}

impl std::fmt::Debug for StructuralWitness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StructuralWitness")
            .field("construct_id", &self.construct_id)
            .field("finer_kind", &self.finer_kind)
            .finish()
    }
}

/// `CharacteristicKind::UnorderedMorphRuleApplication`'s structural predicate: does any stratum
/// in the loaded `Grammar` declare `morphologicalRuleOrder="unordered"`
/// (`MorphRuleOrder::Unordered`, `pg_grammar::load`'s own parse of that attribute)? Reads the
/// LOADED model (`StratumDef::mrule_order`), never the raw XML — the same field
/// `tests/cover_unordered_morph_rules.rs` asserts against directly.
pub fn grammar_has_unordered_stratum(g: &Grammar) -> bool {
    g.strata
        .iter()
        .any(|s| s.mrule_order == MorphRuleOrder::Unordered)
}

/// `CharacteristicKind::Epenthesis`'s structural predicate: does any phonological rewrite rule
/// in the loaded `Grammar` have an EMPTY `PhoneticInput` — `pg_grammar::model::RewriteRuleDef`'s
/// own doc: "empty pattern if absent (epenthesis rules)"? Reads the LOADED model
/// (`RewriteRuleDef::lhs.nodes.is_empty()`), not a regex over `grammar.xml`'s `<PhoneticInput/>`
/// tag — deliberately: the DTD's insertion-only convention is that an ABSENT/empty
/// `<PhoneticInput>` element compiles to an empty `pg_grammar::model::Pattern` (zero
/// `pg_grammar::model::PatternNode`s), and reading the compiled model is the same fact
/// `tests/epenthesis_structural_route_containment.rs::fixture_has_epenthesis_and_composes_to_confirm_only`
/// already asserts on directly, rather than this gate re-deriving its own independent (and
/// possibly drifting) idea of what "empty" looks like in the source XML — e.g. a
/// self-closing `<PhoneticInput/>` vs. an empty `<PhoneticInput><PhoneticSequence/></PhoneticInput>`
/// are two different XML spellings of the identical loaded fact, and only the loaded model
/// treats them identically for free.
pub fn grammar_has_empty_lhs_rewrite_rule(g: &Grammar) -> bool {
    g.prules
        .iter()
        .any(|pr| matches!(pr, PhonRuleDef::Rewrite(r) if r.lhs.nodes.is_empty()))
}

/// `CharacteristicKind::CircumfixOutputAction`'s structural predicate: does ANY allomorph of ANY
/// morphological rule in the loaded `Grammar` classify as `Role::CircumfixPrefix` under
/// `crate::emit::classify_affix` — the COMPILER'S OWN classifier, called here directly rather
/// than re-implemented, so this gate and the compiler cannot drift apart. Deliberately scans
/// EVERY allomorph of every rule (`MorphRuleDef::affix_allomorphs`), not just allomorph 0 —
/// `crate::emit::rule_role` (the compiler's OWN candidate-selection path) classifies a rule by its
/// FIRST allomorph only, so a rule whose non-first allomorph is circumfix-shaped is a real,
/// order-of-declaration-dependent gap on the COMPILE side — this predicate must not repeat that
/// gap on the GATE side too, or a circumfix-shaped allomorph hiding behind a non-circumfix
/// allomorph 0 would make this witness silently fail to find a fixture that visibly has one.
pub fn grammar_has_circumfix_shaped_allomorph(g: &Grammar) -> bool {
    g.mrules.iter().any(|def| {
        def.affix_allomorphs().is_some_and(|allomorphs| {
            allomorphs
                .iter()
                .any(|a| crate::emit::classify_affix(&a.rhs) == crate::emit::Role::CircumfixPrefix)
        })
    })
}

pub fn grammar_has_overwrite_mpr_group(g: &Grammar) -> bool {
    g.mpr_groups
        .iter()
        .any(|group| group.output == MprGroupOutput::Overwrite)
}

/// The live `StructuralWitness`es — one per `shared_construct_ids` entry.
pub fn registered_structural_witnesses() -> Vec<StructuralWitness> {
    vec![
        StructuralWitness {
            construct_id: construct_ids_for(CharacteristicKind::UnorderedMorphRuleApplication)[0],
            finer_kind: CharacteristicKind::UnorderedMorphRuleApplication,
            predicate: grammar_has_unordered_stratum,
        },
        StructuralWitness {
            construct_id: construct_ids_for(CharacteristicKind::Epenthesis)[0],
            finer_kind: CharacteristicKind::Epenthesis,
            predicate: grammar_has_empty_lhs_rewrite_rule,
        },
        StructuralWitness {
            construct_id: construct_ids_for(CharacteristicKind::CircumfixOutputAction)[0],
            finer_kind: CharacteristicKind::CircumfixOutputAction,
            predicate: grammar_has_circumfix_shaped_allomorph,
        },
        StructuralWitness {
            construct_id: construct_ids_for(CharacteristicKind::MprGroupOverwrite)[0],
            finer_kind: CharacteristicKind::MprGroupOverwrite,
            predicate: grammar_has_overwrite_mpr_group,
        },
    ]
}

#[cfg(test)]
mod tests;
