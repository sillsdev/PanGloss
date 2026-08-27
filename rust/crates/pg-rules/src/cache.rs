//! Compile-once FST pattern cache.
//!
//! Every matcher the phonological/morphological rule engines use is a pure function of
//! grammar-static data — a rule/subrule/allomorph pattern, a direction, and a char-def table — never
//! of the runtime word/shape being parsed. C# compiles each one exactly once, at `Morpher`
//! construction (`XmlLanguageLoader`/the `Matcher`/`PatternRuleSpec` build pass). This Rust engine
//! historically recompiled the same Thompson-NFA-then-determinize pipeline inside the hot per-word
//! loop instead:
//! - `crate::rewrite`'s `lhs_fst`/`compile_env`/`compile_env_analysis`/`compile_lane_fst`, called
//!   once per phonological-rule-invocation (i.e. once per subrule per `synthesize`/`analyze` call —
//!   see `rewrite::synthesize_with_mpr`/`analyze`'s per-`Kind` compile calls before this cache);
//! - `crate::morph`'s `compile_parts`/`build_analysis_lhs`, called once per allomorph (or
//!   compounding subrule) per morphological-rule application;
//! - `crate::validity`'s allomorph-environment `compile_env` calls, once per environment per morph
//!   per word-validity check.
//!
//! `RuleCache::build` walks `Grammar` once (mirroring the C# constructor's compile pass) and stores
//! every one of those FSTs, indexed by the same identity the grammar already assigns them
//! (`PRuleId` + subrule position, `AllomorphId`, `MRuleId` + subrule position) — no pointer-identity
//! hashing, no mutex. The cache is built once (at `pg-parse::Morpher` construction) and is
//! thereafter read-only, so `pangloss batch --threads=N` shares one `&RuleCache` across every worker
//! with zero contention and no thread-count sensitivity.
//!

use pg_fst::Fst;
use pg_grammar::model::{
    AllomorphId, AllomorphOwner, CompoundingRuleDef, Grammar, MRuleId, MetathesisRuleDef,
    MorphRuleDef, MorphemeId, PRuleId, PhonRuleDef, TableId,
};

use crate::metathesis::{self, MetaCache};
use crate::morph::{self, AnalysisLhs, CompoundCache};
use crate::rewrite::{self, EnvFst, PruleCache};

// Owning-table resolution: table zero is never an implicit default, mirroring `pg_foma::replace::owning_table`'s precedent on the compiled-net side.

/// Resolve the table that owns a grammar-resident phonological rule (`g.prules[pid.0]`), by finding
/// which stratum's own `prules` cascade contains it -- mirrors `pg_foma::replace::
/// owning_table_id_for_prule_position`'s identical contract on the compiled-net side. `None` (never
/// a guess) when no stratum's own `prules` list contains `pid`: a real, reachable shape under the
/// XML loader's "unknown/unwired ids are silently skipped" convention (`pg_grammar::load`), not
/// merely a defensive case -- confirmed empirically (see this crate's own sweep notes).
pub(crate) fn owning_table_for_prule(g: &Grammar, pid: PRuleId) -> Option<TableId> {
    g.strata
        .iter()
        .find(|s| s.prules.contains(&pid))
        .map(|s| s.table)
}

/// `owning_table_for_prule`'s sibling for a caller that holds a `MetathesisRuleDef` value but not
/// its `PRuleId` (a standalone, non-`_cached` call site -- `metathesis::synthesize`/`analyze`,
/// this crate's own "recompiles every call, also exercised by non-grammar-resident test fixtures"
/// convention, `crate::cache`'s own module doc). Finds the rule's own position in `g.prules` by
/// `xml_id`, then resolves as above. `None` both when the rule isn't grammar-resident at all (never
/// registered into any `Grammar`'s `prules` -- a hand-built fixture, this crate's well-established
/// "standalone rule" pattern) and when it is resident but orphaned (same caveat as
/// `owning_table_for_prule`); callers fall back to `TableId(0)` only in that non-resident case,
/// matching every other standalone entry point in this crate (see call sites' own doc).
pub(crate) fn owning_table_for_metathesis_rule(
    g: &Grammar,
    rule: &MetathesisRuleDef,
) -> Option<TableId> {
    let idx = g
        .prules
        .iter()
        .position(|pr| matches!(pr, PhonRuleDef::Metathesis(r) if r.xml_id == rule.xml_id))?;
    owning_table_for_prule(g, PRuleId(idx as u32))
}

/// Resolve the table a stratum-resident `MorphemeId` belongs to (`morpheme -> its stratum -> that
/// stratum's table`) -- the common tail `owning_table_for_allomorph`'s two `AllomorphOwner` arms
/// both reduce to, factored out so `pg_rules::morph`'s own per-rule (not per-allomorph) call sites
/// can resolve the SAME way directly from an `AffixProcessRuleDef`'s/`RealizationalRuleDef`'s own
/// `morpheme` field, without needing to first reconstruct an `AllomorphId`/`AllomorphOwner` they
/// don't have in hand (`morph::synth_affix`/`ana_affix`/`synth_realizational`/`ana_realizational`
/// and their `_cached` siblings all receive the rule def directly, never an `AllomorphOwner`).
/// `.get()`-based, never a raw index -- see `owning_table_for_allomorph`'s doc for why (hand-built
/// test `Grammar`s with a bare opaque `morpheme` tag and no backing `g.morphemes` entry).
pub(crate) fn owning_table_for_morpheme(g: &Grammar, morpheme: MorphemeId) -> Option<TableId> {
    let stratum = g.morphemes.get(morpheme.0 as usize)?.stratum;
    g.strata.get(stratum.0 as usize).map(|s| s.table)
}

/// Resolve the table that owns an allomorph (root or affix): a `LexEntryDef`'s/
/// `AffixProcessRuleDef`'s/`RealizationalRuleDef`'s own `MorphemeId` names a stratum-resident
/// morpheme for every grammar `pg_grammar::load` produces (that loader's stratum-index-minting
/// discipline never lets this dangle -- confirmed by direct investigation, not merely assumed;
/// mirrors `morph::seed_from_entry`'s identical `entry.morpheme -> stratum -> table` derivation for
/// a root entry). `AllomorphOwner::Affix` never legitimately names a `Compounding` rule (compounding
/// rules mint no `AllomorphId` at all -- `MorphRuleDef::affix_allomorphs`'s own doc), so that arm is
/// `unreachable!()`, a real invariant rather than a test-fixture concern.
pub(crate) fn owning_table_for_allomorph(g: &Grammar, owner: AllomorphOwner) -> Option<TableId> {
    let morpheme = match owner {
        AllomorphOwner::Root(le, _) => g.entries.get(le.0 as usize)?.morpheme,
        AllomorphOwner::Affix(mr, _) => match g.mrules.get(mr.0 as usize)? {
            MorphRuleDef::AffixProcess(def) => def.morpheme,
            MorphRuleDef::Realizational(def) => def.morpheme,
            MorphRuleDef::Compounding(_) => unreachable!(
                "AllomorphOwner::Affix never names a Compounding rule (mints no AllomorphId)"
            ),
        },
    };
    owning_table_for_morpheme(g, morpheme)
}

/// `owning_table_for_prule`'s sibling over `g.strata[..].mrules` instead of `.prules` --
/// `owning_table_for_allomorph` cannot resolve a `MorphRuleDef::Compounding` rule at all (it
/// mints no `AllomorphOwner`, unlike `AffixProcess`/`Realizational`, both of which carry their own
/// `MorphemeId` and so resolve via `owning_table_for_morpheme` instead of needing this). Not a
/// fourth independent resolution strategy: it is `owning_table_for_prule`'s own "which stratum's
/// own cascade contains this id" algorithm, applied to the one `Grammar` list (`mrules`) that
/// algorithm doesn't already cover. `None` under the identical two conditions
/// `owning_table_for_prule`'s doc names (an orphaned-but-resident rule, or a non-loader test
/// fixture with no strata wiring at all).
pub(crate) fn owning_table_for_mrule(g: &Grammar, mrid: MRuleId) -> Option<TableId> {
    g.strata
        .iter()
        .find(|s| s.mrules.contains(&mrid))
        .map(|s| s.table)
}

/// `owning_table_for_mrule`'s sibling for a caller that holds a `CompoundingRuleDef` value but
/// not its `MRuleId` -- `morph::synth_compound`/`ana_compound`, the standalone (uncached,
/// non-grammar-resident-fixture-friendly) entry points reached via `morph::synthesize`/`analyze`
/// with no `MRuleId` in scope at all, exactly mirroring `owning_table_for_metathesis_rule`'s own
/// "resolve by `xml_id`, then delegate" shape for the identical reason (`metathesis::synthesize`/
/// `analyze` have the same no-id-in-scope shape one crate module over). `None` both when `rule`
/// isn't grammar-resident (a hand-built fixture) and when it is resident but orphaned; callers fall
/// back to `TableId(0)` only in that non-resident case, matching every other standalone entry point
/// in this crate.
pub(crate) fn owning_table_for_compounding_rule(
    g: &Grammar,
    rule: &CompoundingRuleDef,
) -> Option<TableId> {
    let idx = g
        .mrules
        .iter()
        .position(|mr| matches!(mr, MorphRuleDef::Compounding(def) if def.xml_id == rule.xml_id))?;
    owning_table_for_mrule(g, MRuleId(idx as u32))
}

/// Per-`AllomorphId` precompiled matchers, shared by root and affix allomorphs (both draw from the
/// same global registry, `Grammar::allomorph_owners`). `envs` backs `crate::validity`'s
/// per-environment gate (populated for both owner kinds); `synth_lhs`/`ana_lhs` are
/// `AffixAllomorphDef`-only (a root allomorph has no `MorphologicalInput`/RHS to compile, so both
/// stay `None` for a `AllomorphOwner::Root` entry).
pub(crate) struct AllomorphCache {
    pub(crate) envs: Vec<(Option<EnvFst>, Option<EnvFst>)>,
    pub(crate) synth_lhs: Option<(Fst, Vec<String>)>,
    pub(crate) ana_lhs: Option<(Fst, AnalysisLhs)>,
}

/// One `RuleCache::prules` slot: which kind of compiled cache depends on the corresponding
/// `g.prules[pid.0]`'s own `PhonRuleDef` variant — always the matching one, by construction
/// (`RuleCache::build` maps `g.prules` 1:1, in order).
// `clippy::large_enum_variant` is unsatisfiable here in both directions, so it is allowed
// deliberately rather than papered over: `Metathesis` IS boxed (below), which already shrank this
// enum from ~440 to 232 bytes per `Vec` slot; what remains is the lint complaining about the
// *difference* between a 232-byte `Rewrite` and an 8-byte boxed `Metathesis`. Boxing `Rewrite` too
// would silence it only by adding a pointer chase to the hot confirm path (nearly every rule in a
// real grammar is a rewrite rule, and `prule_rewrite` is called per rule per word), which is a worse
// trade than a lint warning. Revisit only if `PruleCache` itself grows materially.
#[allow(clippy::large_enum_variant)]
pub(crate) enum PruleCacheEntry {
    Rewrite(PruleCache),
    /// Boxed so a mostly-`Rewrite` `Vec` doesn't pay `MetaCache`'s size on every slot; metathesis rules are rare.
    Metathesis(Box<MetaCache>),
}

/// The whole-grammar compile-once cache. Build once, at `Morpher` construction;
/// share read-only across every `pangloss batch --threads=N` worker.
pub struct RuleCache {
    /// Indexed by `PRuleId.0` (one slot per `g.prules` entry, in order).
    prules: Vec<PruleCacheEntry>,
    /// Indexed by `AllomorphId.0` (root and affix allomorphs share this global id space).
    allomorphs: Vec<AllomorphCache>,
    /// Indexed by `MRuleId.0`; `Some` only for a `MorphRuleDef::Compounding` entry (an `AffixProcess` entry's matchers live in `allomorphs` instead).
    compounds: Vec<Option<CompoundCache>>,
}

impl RuleCache {
    /// Compile every matcher this grammar's rules/allomorphs/environments need, exactly once —
    /// the Rust analog of C#'s `Morpher`/`XmlLanguageLoader` compile pass. See the module doc for
    /// the full per-site rationale.
    pub fn build(g: &Grammar) -> RuleCache {
        let prules = g
            .prules
            .iter()
            .enumerate()
            .map(|(idx, rule)| {
                let pid = PRuleId(idx as u32);
                // `None` means either an orphaned prule (provably dead code, never walked by `crate::stratum`) or a no-strata test fixture, where `TableId(0)` is the correct, not-a-guess answer.
                let table = owning_table_for_prule(g, pid).unwrap_or(TableId(0));
                match rule {
                    PhonRuleDef::Rewrite(r) => {
                        PruleCacheEntry::Rewrite(rewrite::build_prule_cache(g, table, r))
                    }
                    PhonRuleDef::Metathesis(r) => PruleCacheEntry::Metathesis(Box::new(
                        metathesis::build_meta_cache(g, table, r),
                    )),
                }
            })
            .collect();
        let allomorphs = g
            .allomorph_owners
            .iter()
            .map(|owner| build_allomorph_cache(g, owner))
            .collect();
        let compounds = g
            .mrules
            .iter()
            .enumerate()
            .map(|(idx, mr)| match mr {
                MorphRuleDef::Compounding(def) => {
                    // Compounding mints no `AllomorphOwner` for `owning_table_for_allomorph` to resolve through.
                    let table =
                        owning_table_for_mrule(g, MRuleId(idx as u32)).unwrap_or(TableId(0));
                    Some(morph::build_compound_cache(g, table, def))
                }
                MorphRuleDef::AffixProcess(_) | MorphRuleDef::Realizational(_) => None,
            })
            .collect();
        RuleCache {
            prules,
            allomorphs,
            compounds,
        }
    }

    pub(crate) fn prule_rewrite(&self, pid: PRuleId) -> &PruleCache {
        match &self.prules[pid.0 as usize] {
            PruleCacheEntry::Rewrite(c) => c,
            PruleCacheEntry::Metathesis(_) => {
                unreachable!("pid must identify a PhonRuleDef::Rewrite entry")
            }
        }
    }

    pub(crate) fn prule_metathesis(&self, pid: PRuleId) -> &MetaCache {
        match &self.prules[pid.0 as usize] {
            PruleCacheEntry::Metathesis(c) => c,
            PruleCacheEntry::Rewrite(_) => {
                unreachable!("pid must identify a PhonRuleDef::Metathesis entry")
            }
        }
    }

    pub(crate) fn allomorph(&self, id: AllomorphId) -> &AllomorphCache {
        &self.allomorphs[id.0 as usize]
    }

    pub(crate) fn compound(&self, mrid: MRuleId) -> &CompoundCache {
        self.compounds[mrid.0 as usize]
            .as_ref()
            .expect("mrid must identify a MorphRuleDef::Compounding entry")
    }
}

fn build_allomorph_cache(g: &Grammar, owner: &AllomorphOwner) -> AllomorphCache {
    // An allomorph's environments are compiled against its own owning stratum's table, never an implicit table-zero default.
    let table = owning_table_for_allomorph(g, *owner).unwrap_or(TableId(0));
    match *owner {
        AllomorphOwner::Root(le, idx) => {
            let def = &g.entries[le.0 as usize].allomorphs[idx as usize];
            AllomorphCache {
                envs: build_env_cache(g, table, &def.environments),
                synth_lhs: None,
                ana_lhs: None,
            }
        }
        // `MorphRuleDef::affix_allomorphs` centralizes the AffixProcess/Realizational-both-own-AffixAllomorphDef fact, so this site doesn't need its own three-way match.
        AllomorphOwner::Affix(mr, idx) => {
            let allos = g.mrules[mr.0 as usize]
                .affix_allomorphs()
                .expect("compounding rules mint no AllomorphId (no per-allomorph registry entry)");
            let def = &allos[idx as usize];
            let lhs = morph::build_allomorph_lhs_cache(g, table, def);
            AllomorphCache {
                envs: build_env_cache(g, table, &def.environments),
                synth_lhs: lhs.synth_lhs,
                ana_lhs: lhs.ana_lhs,
            }
        }
    }
}

fn build_env_cache(
    g: &Grammar,
    table: TableId,
    envs: &[pg_grammar::model::EnvironmentDef],
) -> Vec<(Option<EnvFst>, Option<EnvFst>)> {
    envs.iter()
        .map(|env| {
            (
                rewrite::compile_env_allomorph(g, table, env.left.as_ref()),
                rewrite::compile_env_allomorph(g, table, env.right.as_ref()),
            )
        })
        .collect()
}

// Regression gate: a phonological rule on a non-zero-table stratum must resolve natural classes against its own table, never table 0.

#[cfg(test)]
mod owning_table_tests {
    use super::*;
    use pg_featstruct::FeatureStruct;
    use pg_grammar::model::MprSet;

    /// Two tables/strata with deliberately misaligned raw indices: `t0`'s segment "z" and `t1`'s "q" both sit at index 0 but carry opposite feature values, so a wrongly-table-0-resolved `ncQ` can never match a real `t1` "q".
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>OwningTableProbe</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featF">
        <Name>f</Name>
        <Symbols><Symbol id="fp">+</Symbol><Symbol id="fm">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t0">
      <Name>T0</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c0z">
          <Representations><Representation>z</Representation></Representations>
          <FeatureValue feature="featF" symbolValues="fp" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <CharacterDefinitionTable id="t1">
      <Name>T1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1q">
          <Representations><Representation>q</Representation></Representations>
          <FeatureValue feature="featF" symbolValues="fm" />
        </SegmentDefinition>
        <SegmentDefinition id="c1p">
          <Representations><Representation>p</Representation></Representations>
          <FeatureValue feature="featF" symbolValues="fp" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncQ"><Name>Q</Name><Segment segment="c1q" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncP"><Name>P</Name><Segment segment="c1p" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prQtoP">
        <Name>qtop</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncQ" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncP" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t0" morphologicalRuleOrder="unordered">
        <Name>S0</Name>
      </Stratum>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prQtoP">
        <Name>S1</Name>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    fn load() -> Grammar {
        pg_grammar::load(XML).unwrap_or_else(|e| panic!("owning-table probe grammar loads: {e}"))
    }

    #[test]
    fn owning_table_for_prule_resolves_the_rules_own_stratum_not_table_zero() {
        let g = load();
        assert_eq!(
            g.char_tables.len(),
            2,
            "fixture must declare exactly 2 tables"
        );
        assert_eq!(g.strata.len(), 2, "fixture must declare exactly 2 strata");
        assert_eq!(
            g.prules.len(),
            1,
            "fixture declares exactly 1 phonological rule"
        );

        let table = owning_table_for_prule(&g, PRuleId(0))
            .expect("prQtoP is wired into stratum S1's own phonologicalRules cascade");
        assert_eq!(
            table,
            TableId(1),
            "prQtoP belongs to stratum S1 (table 1) -- owning_table_for_prule must NOT return \
             table 0"
        );
    }

    /// Runs the rule through the real cached production path on a `t1`-native "q" segment: if `ncQ` is ever wrongly compiled against table 0 again, synthesis finds nothing and this test fails.
    #[test]
    fn cached_synthesis_resolves_natural_classes_against_the_rules_own_table_not_table_zero() {
        let g = load();
        let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
            panic!("prQtoP must load as a PhonRuleDef::Rewrite");
        };
        let cache = RuleCache::build(&g);

        let t1 = &g.char_tables[1];
        let input = crate::shape_feat::segment_with_features(&g, t1, "q")
            .expect("\"q\" segments against t1");

        let out = rewrite::synthesize_with_mpr_cached(
            &g,
            PRuleId(0),
            rule,
            &input,
            &FeatureStruct::EMPTY,
            MprSet::EMPTY,
            &cache,
        );
        assert_eq!(
            out.len(),
            1,
            "prQtoP must fire on a genuine t1 \"q\" segment when its own natural classes are \
             resolved against t1 (its real owning table); an empty result here means ncQ/ncP were \
             wrongly compiled against table 0 instead"
        );

        // The interior node's lanes must match "p"'s (f=+), not "z"'s -- an independent check from the match-at-all proof above, since both happen to share f=+.
        let p_lanes = t1
            .get(pg_grammar::chardef::CharDefId(1))
            .feature_lanes()
            .to_vec();
        let interior: Vec<usize> = (0..out[0].len())
            .filter(|&i| matches!(out[0].kind(i), pg_shape::NodeKind::Segment))
            .collect();
        assert_eq!(interior.len(), 1, "exactly one segment node");
        assert_eq!(
            out[0].node_lanes(interior[0]).to_vec(),
            p_lanes,
            "the rewritten node must carry ncP's (t1's \"p\") own lanes"
        );
    }

    /// Same regression as the two tests above, but for an affix allomorph's own LHS/RHS pattern rather than a phonological rule's environment.
    #[test]
    fn cached_affix_synthesis_resolves_the_allomorphs_own_lhs_rhs_pattern_against_its_own_table_not_table_zero(
    ) {
        const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>OwningTableAffixProbe</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <HeadFeatures />
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featF">
        <Name>f</Name>
        <Symbols><Symbol id="fp">+</Symbol><Symbol id="fm">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t0">
      <Name>T0</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c0z">
          <Representations><Representation>z</Representation></Representations>
          <FeatureValue feature="featF" symbolValues="fp" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <CharacterDefinitionTable id="t1">
      <Name>T1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1q">
          <Representations><Representation>q</Representation></Representations>
          <FeatureValue feature="featF" symbolValues="fm" />
        </SegmentDefinition>
        <SegmentDefinition id="c1p">
          <Representations><Representation>p</Representation></Representations>
          <FeatureValue feature="featF" symbolValues="fp" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncQ"><Name>Q</Name><Segment segment="c1q" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t0" morphologicalRuleOrder="unordered">
        <Name>S0</Name>
      </Stratum>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrQtoQP">
        <Name>S1</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrQtoQP" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>plus-p</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subQP">
                <MorphologicalInput>
                  <PhoneticSequence id="stem"><SimpleContext naturalClass="ncQ" /></PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eQ" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aQ"><PhoneticShape>q</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>root</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
        let g = pg_grammar::load(XML)
            .unwrap_or_else(|e| panic!("owning-table affix probe grammar loads: {e}"));
        assert_eq!(
            g.char_tables.len(),
            2,
            "fixture must declare exactly 2 tables"
        );
        assert_eq!(g.strata.len(), 2, "fixture must declare exactly 2 strata");
        assert_eq!(
            g.mrules.len(),
            1,
            "fixture declares exactly 1 morphological rule"
        );
        assert_eq!(
            g.entries.len(),
            1,
            "fixture declares exactly 1 lexical entry"
        );

        let cache = RuleCache::build(&g);
        let mrid = MRuleId(0);
        let rule = &g.mrules[0];

        let word =
            morph::seed_from_entry(&g, pg_grammar::model::LexEntryId(0), FeatureStruct::EMPTY);
        assert_eq!(
            word.stratum,
            pg_grammar::model::StratumId(1),
            "the root entry must load onto S1 (table t1), not S0"
        );

        let out = morph::synthesize_cached(&g, mrid, &word, rule, &cache);
        assert_eq!(
            out.len(),
            1,
            "mrQtoQP must fire on a genuine t1 \"q\" root when ncQ (its allomorph's own LHS \
             pattern) is resolved against t1 (its real owning table); an empty result here means \
             ncQ was wrongly compiled against table 0 instead"
        );

        let w = &out[0];
        let interior: Vec<usize> = (0..w.shape.len())
            .filter(|&i| matches!(w.shape.kind(i), pg_shape::NodeKind::Segment))
            .collect();
        assert_eq!(interior.len(), 2, "the root \"q\" plus the inserted \"p\"");

        let t1 = &g.char_tables[1];
        let q_lanes = t1
            .get(pg_grammar::chardef::CharDefId(0))
            .feature_lanes()
            .to_vec();
        let p_lanes = t1
            .get(pg_grammar::chardef::CharDefId(1))
            .feature_lanes()
            .to_vec();
        assert_eq!(
            w.shape.node_lanes(interior[0]).to_vec(),
            q_lanes,
            "the copied root node must keep t1's own \"q\" lanes"
        );
        assert_eq!(
            w.shape.node_lanes(interior[1]).to_vec(),
            p_lanes,
            "the inserted node must carry t1's own \"p\" lanes (cd_lanes resolved against t1), \
             not t0's"
        );
    }
}
