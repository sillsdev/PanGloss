//! Compile-once FST pattern cache (plan §13.2 step 5; rust-conversion.md §13.1.1's "Disproven"
//! paragraph, measured-impact item (1)).
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
//! hashing, no mutex. The cache is built once (at `hc-parse::Morpher` construction) and is
//! thereafter read-only, so `hc-rs batch --threads=N` shares one `&RuleCache` across every worker
//! with zero contention and no thread-count sensitivity.
//!
//! **The engine's own uncached entry points are unchanged**: `rewrite::synthesize`/
//! `synthesize_with_mpr`/`analyze`, `morph::synthesize`/`analyze`, and `validity::allomorphs_valid`/
//! `environments_ok` still recompile on every call, exactly as before this milestone. This is
//! deliberate, not a missed spot: a large fraction of the test suite builds a standalone
//! `RewriteRuleDef`/`EnvironmentDef`/allomorph fixture that is never grammar-resident (never lives at
//! a stable index inside some `Grammar`'s `prules`/`mrules`/`allomorph_owners`), so there is no index
//! to cache against, and pointer-identity caching was rejected (a `HashMap` behind a lock in the hot
//! path is exactly the contention pattern the design brief asked to avoid). Only the real per-word
//! pipeline (`hc-parse::Morpher`, threaded through `crate::stratum`) uses the `_cached` siblings this
//! module and its per-file counterparts (`rewrite::synthesize_with_mpr_cached`/`analyze_cached`,
//! `morph::synthesize_cached`/`analyze_cached`, `validity::allomorphs_valid_cached`) add.

use hc_fst::Fst;
use hc_grammar::model::{
    AllomorphId, AllomorphOwner, Grammar, MRuleId, MorphRuleDef, PRuleId, PhonRuleDef, TableId,
};

use crate::metathesis::{self, MetaCache};
use crate::morph::{self, AnalysisLhs, CompoundCache};
use crate::rewrite::{self, EnvFst, PruleCache};

/// Phonological/morphological rules and lexical entries all resolve char-defs/patterns against
/// table 0 in every reference grammar (the same convention `morph.rs`/`rewrite.rs`/`validity.rs`
/// each independently document).
const TABLE: TableId = TableId(0);

/// Per-[`AllomorphId`] precompiled matchers, shared by root and affix allomorphs (both draw from the
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
pub(crate) enum PruleCacheEntry {
    Rewrite(PruleCache),
    Metathesis(MetaCache),
}

/// The whole-grammar compile-once cache (plan §13.2 step 5). Build once, at `Morpher` construction;
/// share read-only across every `hc-rs batch --threads=N` worker.
pub struct RuleCache {
    /// Indexed by `PRuleId.0` (one slot per `g.prules` entry, in order).
    prules: Vec<PruleCacheEntry>,
    /// Indexed by `AllomorphId.0` (one slot per `g.allomorph_owners` entry, in order — root and
    /// affix allomorphs share this global id space).
    allomorphs: Vec<AllomorphCache>,
    /// Indexed by `MRuleId.0` (one slot per `g.mrules` entry, in order); `Some` only where that
    /// entry is a `MorphRuleDef::Compounding` (an `AffixProcess` entry's matchers live in
    /// `allomorphs` instead, keyed by each of its allomorphs' own `AllomorphId`).
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
            .map(|rule| match rule {
                PhonRuleDef::Rewrite(r) => {
                    PruleCacheEntry::Rewrite(rewrite::build_prule_cache(g, TABLE, r))
                }
                PhonRuleDef::Metathesis(r) => {
                    PruleCacheEntry::Metathesis(metathesis::build_meta_cache(g, TABLE, r))
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
            .map(|mr| match mr {
                MorphRuleDef::Compounding(def) => Some(morph::build_compound_cache(g, def)),
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
    match *owner {
        AllomorphOwner::Root(le, idx) => {
            let def = &g.entries[le.0 as usize].allomorphs[idx as usize];
            AllomorphCache {
                envs: build_env_cache(g, &def.environments),
                synth_lhs: None,
                ana_lhs: None,
            }
        }
        // `MorphRuleDef::affix_allomorphs` centralizes the AffixProcess/Realizational-both-own-
        // AffixAllomorphDef fact (see that method's doc) so this site doesn't need its own
        // three-way match.
        AllomorphOwner::Affix(mr, idx) => {
            let allos = g.mrules[mr.0 as usize]
                .affix_allomorphs()
                .expect("compounding rules mint no AllomorphId (no per-allomorph registry entry)");
            let def = &allos[idx as usize];
            let lhs = morph::build_allomorph_lhs_cache(g, def);
            AllomorphCache {
                envs: build_env_cache(g, &def.environments),
                synth_lhs: lhs.synth_lhs,
                ana_lhs: lhs.ana_lhs,
            }
        }
    }
}

fn build_env_cache(
    g: &Grammar,
    envs: &[hc_grammar::model::EnvironmentDef],
) -> Vec<(Option<EnvFst>, Option<EnvFst>)> {
    envs.iter()
        .map(|env| {
            (
                rewrite::compile_env_allomorph(g, TABLE, env.left.as_ref()),
                rewrite::compile_env_allomorph(g, TABLE, env.right.as_ref()),
            )
        })
        .collect()
}
