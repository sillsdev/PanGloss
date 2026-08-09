//! ONE immutable, typed owner for the semantic facts this crate derives from a `Grammar`, and
//! the projections its consumers read instead of walking the grammar themselves.
//!
//! # The problem this replaces
//! Four consumer areas -- the capability gate (`crate::capability`/`crate::capability_entry`/
//! `crate::preflight`), registry applicability (`crate::recipe_registry::Applicability`),
//! recipe-space accounting (`crate::recipe_space::GrammarFacts`) and the phonology/morphotactics
//! existence gates -- each independently re-walked `Grammar` for the SAME handful of facts. Nothing
//! memoized anything: `crate::capability::compose_envelope` recomputed
//! `crate::capability::characterize` unconditionally on every call, and
//! `crate::selection::select_plan` called it ONCE PER CANDIDATE PLAN -- re-running the whole
//! grammar walk, including the real `foma::types::Fsm` construction `characterize`'s
//! `lower_subrule_span` performs for every `Simultaneous`-mode subrule. `pangloss make-report`
//! re-derived the whole capability verdict three times in one process; `pangloss fst-health` twice
//! -- duplication `crate::preflight`'s own module doc called "an acceptable duplication" and this
//! type now removes by memoizing the derivation once.
//!
//! # What this owns, and what it deliberately does not
//! `GrammarSemantics::derive` is the single derivation point. It is a pure function of `&Grammar`,
//! it is immutable once built (no `&mut self` method exists), and every field it exposes is either
//! computed eagerly in `derive` or memoized exactly once behind a `OnceLock` -- so the second read
//! is free and can never disagree with the first.
//!
//! It does NOT own:
//! - `conformance_coverage.rs`'s four `grammar_has_*` structural witnesses. Those are an
//!   INTENTIONALLY independent second derivation, cross-checked against the characteristics profile
//!   by `tests/structural_witness_gate.rs`. Making them projections would destroy exactly the
//!   independence that gate exists to exploit. Left alone on purpose.
//! - The compile paths themselves (`crate::gate::compile_gated_grammar_with_budget`,
//!   `crate::emit`, `crate::replace`). Those take `prules_in_order` as an explicit parameter
//!   from their caller and build live networks; this type owns the FACTS those paths are described
//!   by, not the compilation.
//!
//! # Authored order is preserved, hash order is not
//! Stratum order and template-slot order are load-bearing for reachability/completability, and
//! `crate::enumerate::prules_in_order`'s borrow-from-`g.prules` identity is load-bearing for
//! `crate::enumerate::rule_id_of`'s pointer-identity `PRuleId` recovery. Neither is canonicalized,
//! sorted, or rebuilt here -- `GrammarSemantics::prules_in_order()` hands back the exact slice every
//! compile seam already takes.
//!
//! The ONE thing this type does canonicalize is the entry partition's group order.
//! `crate::gate::partition_entries` collects into a `HashMap` and returns its iteration order,
//! which is not authored order and is not stable across runs -- it is hash order. Sorting the groups
//! by their own gate key makes `GrammarSemantics::derive` byte-identical on a fresh load, and is
//! safe because every consumer of the partition in this type's scope reads only `len()` (the
//! `partitions` fact) or `is_empty()`. `gate.rs`'s own compile path is untouched and keeps calling
//! `partition_entries` directly.
//!
//! # Two facts that look like one: declared vs. cascade phonology
//! `Applicability::HasPhonology` tests the grammar-wide `g.prules`, while
//! `crate::junctions::PhonologyProbe::new`'s existence gate scans each stratum's own `sd.prules`.
//! These are NOT the same predicate, and they genuinely disagree: a `<PhonologicalRule>` declared in
//! the global `<PhonologicalRuleDefinitions>` block but named by no stratum's `phonologicalRules`
//! attribute is loaded into `g.prules` and referenced by no `sd.prules`
//! (`pg_grammar::load`'s stratum loader skips unknown ids and only ever pushes ids the attribute
//! names). `tests/grammar_semantics_owner_gate.rs` pins a fixture where they differ.
//!
//! So this type owns BOTH, under names that say which is which --
//! `GrammarSemantics::declared_phonology()` and `GrammarSemantics::cascade_phonology()` -- and each
//! consumer projects the one it already meant. This is deliberately NOT a unification: collapsing
//! them would change which recipe families a grammar is offered, and this type's split is a
//! consolidation of existing facts, not a behavior change.
//!
//! Templates have no such split, and that is a reasoned negative, not an unchecked assumption. Both
//! grammar loaders keep `g.templates` and the owning stratum's `sd.templates` in lockstep by
//! construction: `pg_grammar::load`'s stratum loader pushes each `<AffixTemplate>` into
//! `acc.templates` and that stratum's own `templates` list in the same statement, and
//! `pg_grammar::compile` hands `templates::build`'s entire output to the morphology stratum. There
//! is no path that can produce a template in `g.templates` that no stratum references, so
//! `declared_templates` and a hypothetical `stratum_templates` cannot differ. Only the declared form
//! is exposed.

use std::collections::HashSet;
use std::sync::OnceLock;

use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::{
    Grammar, LexEntryId, MorphRuleDef, PRuleId, PhonRuleDef, TableId, TemplateId,
};

use crate::capability::{characterize, rhs_has_true_reduplication, CharacteristicsProfile};
use crate::enumerate::prules_in_order;
use crate::gate::{find_gated_subrules, partition_entries, GatedSubrule};

/// One entry partition, owned by `GrammarSemantics` in a deterministic order (module doc). Shaped
/// like `crate::gate::EntryGroup` but held separately so `gate.rs`'s own compile path keeps its
/// existing, unchanged return type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticEntryGroup {
    pub key: Vec<bool>,
    pub entries: HashSet<LexEntryId>,
}

/// The single immutable owner of this crate's grammar-derived semantic facts (module doc).
///
/// Built once with `GrammarSemantics::derive` and shared by reference. Every accessor is `&self`
/// and every one of them is deterministic: the eager fields are computed in `derive`, the two
/// genuinely expensive ones (`Self::entry_partition` and `Self::characteristics`) are memoized
/// behind a `OnceLock` so the first caller pays and every later caller reads the same value.
///
/// # Why those two are lazy and the rest are not
/// `derive` has to be cheap enough that every consumer can afford to call it -- an owner nobody can
/// afford to build gets bypassed, and then it owns nothing. The eager facts are all O(rules + strata
/// + entries) scans over already-loaded vectors. The two lazy ones are not:
/// `crate::gate::partition_entries` is O(entries x gated subrules) and evaluates the engine's own
/// `pg_rules::rewrite::subrule_applicable` predicate for each pair, and
/// `crate::capability::characterize` builds real `foma` networks for `Simultaneous`-mode subrules.
/// A registry applicability check that only needs "does this grammar have templates" must not pay
/// for either.
pub struct GrammarSemantics<'g> {
    grammar: &'g Grammar,
    prules_in_order: Vec<&'g PhonRuleDef>,
    gated_subrules: Vec<GatedSubrule>,
    declared_phonology: bool,
    cascade_phonology: bool,
    declared_templates: bool,
    template_count: u64,
    has_morphology: bool,
    mrule_count: u64,
    reduplicative_allomorph_count: u64,
    metathesis_rule_count: u64,
    entry_count: usize,
    stratum_count: usize,
    ordered_operations: u64,
    ordering_dependencies: u64,
    prule_ids_in_order: Vec<PRuleId>,
    template_ids: Vec<TemplateId>,
    char_table_count: usize,
    primary_table: Option<TableId>,
    primary_table_boundary_symbols: Vec<String>,
    entry_partition: OnceLock<Vec<SemanticEntryGroup>>,
    characteristics: OnceLock<CharacteristicsProfile>,
}

impl std::fmt::Debug for GrammarSemantics<'_> {
    /// Deliberately shallow: memoized fields print as "computed / not yet computed" rather than being forced, so a `{:?}` in a log can never turn a cheap derive into an expensive one.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GrammarSemantics")
            .field("gated_subrules", &self.gated_subrules.len())
            .field("declared_phonology", &self.declared_phonology)
            .field("cascade_phonology", &self.cascade_phonology)
            .field("declared_templates", &self.declared_templates)
            .field("has_morphology", &self.has_morphology)
            .field(
                "reduplicative_allomorph_count",
                &self.reduplicative_allomorph_count,
            )
            .field("metathesis_rule_count", &self.metathesis_rule_count)
            .field("entry_count", &self.entry_count)
            .field("stratum_count", &self.stratum_count)
            .field("entry_partition", &self.entry_partition.get().map(Vec::len))
            .field("characteristics", &self.characteristics.get().is_some())
            .finish()
    }
}

impl<'g> GrammarSemantics<'g> {
    /// The single derivation point (module doc). Pure, deterministic, and byte-identical on a fresh
    /// load of the same grammar.
    pub fn derive(grammar: &'g Grammar) -> Self {
        let prules_in_order = prules_in_order(grammar);
        let gated_subrules = find_gated_subrules(grammar, &prules_in_order);

        // Two distinct facts, not one -- see the module doc's "declared vs. cascade phonology".
        let declared_phonology = !grammar.prules.is_empty();
        let cascade_phonology = grammar.strata.iter().any(|s| !s.prules.is_empty());

        let reduplicative_allomorph_count = grammar
            .mrules
            .iter()
            .map(reduplicative_allomorphs_in)
            .sum::<u64>();

        let metathesis_rule_count = grammar
            .prules
            .iter()
            .filter(|rule| matches!(rule, PhonRuleDef::Metathesis(_)))
            .count() as u64;

        // Authored per-stratum operation counts (module doc: never sorted or canonicalized).
        let ordered_operations = grammar
            .strata
            .iter()
            .map(|stratum| {
                (stratum.prules.len() + stratum.mrules.len() + stratum.templates.len()) as u64
            })
            .sum();
        let ordering_dependencies = grammar
            .strata
            .iter()
            .map(|stratum| {
                let n = stratum.prules.len() + stratum.mrules.len() + stratum.templates.len();
                n.saturating_sub(1) as u64
            })
            .sum();

        // The same stratum-cascade walk `prules_in_order` performs, kept as ids since `prules_in_order` must stay a borrow (its pointer identity is load-bearing for `enumerate::rule_id_of`).
        let prule_ids_in_order: Vec<PRuleId> = grammar
            .strata
            .iter()
            .flat_map(|s| s.prules.iter())
            .copied()
            .collect();

        // `TemplateId` is a dense index into `g.templates`, so the declared set is exactly `0..len`.
        let template_ids: Vec<TemplateId> = (0..grammar.templates.len() as u32)
            .map(TemplateId)
            .collect();

        // The grammar's first character-definition table only, deliberately: a union across tables would silently claim a symbol inventory no single table has.
        let primary_table = (!grammar.char_tables.is_empty()).then_some(TableId(0));
        let primary_table_boundary_symbols = grammar
            .char_tables
            .first()
            .map(|table| {
                let mut symbols: Vec<String> = table
                    .iter()
                    .filter(|(_, def)| def.kind() == CharDefKind::Boundary)
                    .flat_map(|(_, def)| def.representations().iter().cloned())
                    .collect();
                symbols.sort();
                symbols.dedup();
                symbols
            })
            .unwrap_or_default();

        Self {
            grammar,
            prules_in_order,
            gated_subrules,
            declared_phonology,
            cascade_phonology,
            declared_templates: !grammar.templates.is_empty(),
            template_count: grammar.templates.len() as u64,
            has_morphology: !grammar.mrules.is_empty(),
            mrule_count: grammar.mrules.len() as u64,
            reduplicative_allomorph_count,
            metathesis_rule_count,
            entry_count: grammar.entries.len(),
            stratum_count: grammar.strata.len(),
            ordered_operations,
            ordering_dependencies,
            prule_ids_in_order,
            template_ids,
            char_table_count: grammar.char_tables.len(),
            primary_table,
            primary_table_boundary_symbols,
            entry_partition: OnceLock::new(),
            characteristics: OnceLock::new(),
        }
    }

    /// The `Grammar` these facts were derived from. Exposed so a consumer that already holds a
    /// `&GrammarSemantics` never has to be handed the grammar separately -- NOT an invitation to
    /// re-derive a fact this type already owns.
    pub fn grammar(&self) -> &'g Grammar {
        self.grammar
    }

    /// `g`'s phonological rules in stratum-cascade (authored) order, as literal borrows of
    /// `g.prules` -- the exact slice `crate::enumerate::enumerate_default`,
    /// `crate::gate::compile_gated_grammar_with_budget` and `crate::replace`'s cascade builders
    /// take. The borrow identity is load-bearing (see `crate::enumerate::prules_in_order`).
    pub fn prules_in_order(&self) -> &[&'g PhonRuleDef] {
        &self.prules_in_order
    }

    /// Every gated `RewriteSubruleDef` `crate::gate::find_gated_subrules` finds over
    /// `Self::prules_in_order` -- the real mechanism's own answer, computed once.
    pub fn gated_subrules(&self) -> &[GatedSubrule] {
        &self.gated_subrules
    }

    /// `true` iff at least one subrule is gated. The projection
    /// `crate::recipe_registry::Applicability::HasGatedExceptions` reads.
    pub fn has_gated_exceptions(&self) -> bool {
        !self.gated_subrules.is_empty()
    }

    /// `crate::gate::partition_entries`'s groups, sorted by gate key so this is deterministic
    /// (module doc: `partition_entries` itself returns `HashMap` iteration order). Memoized.
    pub fn entry_partition(&self) -> &[SemanticEntryGroup] {
        self.entry_partition.get_or_init(|| {
            let mut groups: Vec<SemanticEntryGroup> =
                partition_entries(self.grammar, &self.gated_subrules, &self.prules_in_order)
                    .into_iter()
                    .map(|group| SemanticEntryGroup {
                        key: group.key,
                        entries: group.entries,
                    })
                    .collect();
            groups.sort_by(|a, b| a.key.cmp(&b.key));
            groups
        })
    }

    /// How many distinct gate-key partitions this grammar's entries realize -- `GrammarFacts`'s
    /// `partitions` magnitude.
    pub fn partition_count(&self) -> u64 {
        self.entry_partition().len() as u64
    }

    /// `true` iff the grammar declares at least one phonological rule ANYWHERE
    /// (`!g.prules.is_empty()`), whether or not a stratum names it. What
    /// `crate::recipe_registry::Applicability::HasPhonology` means -- see the module doc for why
    /// this is not the same question as `Self::cascade_phonology`.
    pub fn declared_phonology(&self) -> bool {
        self.declared_phonology
    }

    /// `true` iff at least one stratum's own `phonologicalRules` list is non-empty, i.e. at least
    /// one rule actually reaches the trailing per-stratum rewrite cascade. What
    /// `crate::junctions::PhonologyProbe::new`'s existence gate means.
    pub fn cascade_phonology(&self) -> bool {
        self.cascade_phonology
    }

    /// `true` iff the grammar declares at least one affix template. Both loaders keep this in
    /// lockstep with per-stratum template membership (module doc), so there is only one form of this
    /// fact.
    pub fn declared_templates(&self) -> bool {
        self.declared_templates
    }

    /// How many affix templates the grammar declares -- `GrammarFacts`'s `templates` magnitude.
    pub fn template_count(&self) -> u64 {
        self.template_count
    }

    /// `true` iff the grammar declares at least one morphological rule.
    pub fn has_morphology(&self) -> bool {
        self.has_morphology
    }

    /// How many morphological rules the grammar declares -- `GrammarFacts`'s `branches` magnitude.
    pub fn mrule_count(&self) -> u64 {
        self.mrule_count
    }

    /// `true` iff any morphological-rule allomorph genuinely reduplicates, per
    /// `crate::capability::rhs_has_true_reduplication` -- the single authority for that fact.
    pub fn has_reduplication(&self) -> bool {
        self.reduplicative_allomorph_count > 0
    }

    /// How many morphological-rule allomorphs genuinely reduplicate (a magnitude, not a bool --
    /// `GrammarFacts` reports it as a count).
    pub fn reduplicative_allomorph_count(&self) -> u64 {
        self.reduplicative_allomorph_count
    }

    /// `true` iff any grammar-wide phonological rule is a `PhonRuleDef::Metathesis`.
    pub fn has_metathesis(&self) -> bool {
        self.metathesis_rule_count > 0
    }

    /// How many grammar-wide phonological rules are `PhonRuleDef::Metathesis`.
    pub fn metathesis_rule_count(&self) -> u64 {
        self.metathesis_rule_count
    }

    /// The grammar's lexical-entry count.
    pub fn entry_count(&self) -> usize {
        self.entry_count
    }

    /// The grammar's stratum count.
    pub fn stratum_count(&self) -> usize {
        self.stratum_count
    }

    /// Total authored per-stratum operations (`prules + mrules + templates`, summed over strata).
    pub fn ordered_operations(&self) -> u64 {
        self.ordered_operations
    }

    /// Total authored per-stratum ordering dependencies (`n - 1` per stratum, summed).
    pub fn ordering_dependencies(&self) -> u64 {
        self.ordering_dependencies
    }

    /// The same stratum-cascade order as `Self::prules_in_order`, as `PRuleId`s. Authored order,
    /// never canonicalized -- what a mechanism's ordered-phonology body records.
    pub fn prule_ids_in_order(&self) -> &[PRuleId] {
        &self.prule_ids_in_order
    }

    /// Every declared affix template, in authored order.
    pub fn template_ids(&self) -> &[TemplateId] {
        &self.template_ids
    }

    /// How many character-definition tables the grammar declares.
    pub fn char_table_count(&self) -> usize {
        self.char_table_count
    }

    /// The grammar's first character-definition table, or `None` if it declares none.
    pub fn primary_table(&self) -> Option<TableId> {
        self.primary_table
    }

    /// The primary table's `CharDefKind::Boundary` representations, sorted and deduplicated. Note
    /// the scope: the PRIMARY table's inventory, not a union across tables (see `derive`).
    pub fn primary_table_boundary_symbols(&self) -> &[String] {
        &self.primary_table_boundary_symbols
    }

    /// `crate::capability::characterize`'s exhaustive construct profile, computed AT MOST ONCE per
    /// `GrammarSemantics` (see the module doc: this is the memoization this type exists to provide).
    pub fn characteristics(&self) -> &CharacteristicsProfile {
        self.characteristics
            .get_or_init(|| characterize(self.grammar))
    }
}

/// How many of `rule`'s allomorphs genuinely reduplicate, per `crate::capability::rhs_has_true_reduplication`, the single authority for this fact.
fn reduplicative_allomorphs_in(rule: &MorphRuleDef) -> u64 {
    let allomorphs = match rule {
        MorphRuleDef::AffixProcess(def) => &def.allomorphs,
        MorphRuleDef::Realizational(def) => &def.allomorphs,
        MorphRuleDef::Compounding(_) => return 0,
    };
    allomorphs
        .iter()
        .filter(|allomorph| rhs_has_true_reduplication(&allomorph.rhs))
        .count() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    /// A grammar whose single `<PhonologicalRule>` is declared globally but named by no stratum's `phonologicalRules` attribute: the declared-vs-cascade split, made concrete.
    const ORPHANED_PRULE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>OrphanedPruleFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prule1">
        <Name>orphan</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e0">
            <Allomorphs><Allomorph id="allo0"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e0</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    /// The two phonology facts are genuinely distinct: without the split, whichever single predicate survived would change one of the two consumers' answers on this shape.
    #[test]
    fn declared_and_cascade_phonology_are_distinct_facts() {
        let g = load(ORPHANED_PRULE_XML);
        let sem = GrammarSemantics::derive(&g);

        assert!(
            sem.declared_phonology(),
            "the grammar declares a <PhonologicalRule>, so declared_phonology must be true"
        );
        assert!(
            !sem.cascade_phonology(),
            "no stratum names that rule in its phonologicalRules list, so it never reaches the \
             per-stratum cascade and cascade_phonology must be false"
        );
        assert!(
            sem.prules_in_order().is_empty(),
            "prules_in_order walks strata, so an unreferenced rule contributes nothing"
        );
    }

    /// `derive` is pure: two derivations from the same load, and one from a second independent load, agree on every eager and memoized fact.
    #[test]
    fn derivation_is_deterministic_across_independent_loads() {
        let g1 = load(ORPHANED_PRULE_XML);
        let g2 = load(ORPHANED_PRULE_XML);
        let a = GrammarSemantics::derive(&g1);
        let b = GrammarSemantics::derive(&g2);

        assert_eq!(a.declared_phonology(), b.declared_phonology());
        assert_eq!(a.cascade_phonology(), b.cascade_phonology());
        assert_eq!(a.declared_templates(), b.declared_templates());
        assert_eq!(a.has_morphology(), b.has_morphology());
        assert_eq!(a.has_reduplication(), b.has_reduplication());
        assert_eq!(a.has_metathesis(), b.has_metathesis());
        assert_eq!(a.entry_count(), b.entry_count());
        assert_eq!(a.stratum_count(), b.stratum_count());
        assert_eq!(a.ordered_operations(), b.ordered_operations());
        assert_eq!(a.ordering_dependencies(), b.ordering_dependencies());
        assert_eq!(a.gated_subrules(), b.gated_subrules());
        assert_eq!(a.entry_partition(), b.entry_partition());
        // Reading twice must give the same memoized answer, not a recomputed one.
        assert_eq!(a.entry_partition(), a.entry_partition());
        assert_eq!(a.partition_count(), 1);
    }
}
