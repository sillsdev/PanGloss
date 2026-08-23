//! Morphological-rule application: affix-process, realizational, and compounding rules, in both
//! directions (**synthesis** = apply, **analysis** = unapply). Ports
//! `SIL.Machine.Morphology.HermitCrab/MorphologicalRules/` at the rule level; the strata/template
//! cascade around it is `crate::stratum`.
//!
//! Synthesis matches an allomorph's LHS parts against the word's shape (anchored, one capture group
//! per part) and builds a new shape by executing the RHS output actions. Analysis builds the
//! *analysis LHS* by inverting those actions — a copy becomes a capture, an insert becomes
//! match-and-consume, a modify captures the modified form and remembers to underspecify it — matches
//! nondeterministically with all submatches, then re-emits the captured parts while dropping the
//! inserted material. That is how an affix's material is "removed" on unapply.
//!
//! Two scope limits a caller has to know, both consequences of this module deliberately having no
//! lexicon dependency and no per-candidate history:
//!
//! - **Compounding analysis here prunes nothing.** It produces the head/non-head split but never
//!   runs C#'s root-allomorph search over the non-head, so every `Compounding` output still needs
//!   the caller's `crate::stratum::NonHeadRootFilter` applied. A bare `analyze` call discards
//!   nothing at all.
//! - **`max_apps` is not enforced here** either; `crate::stratum::StratumAnalyzer::apply_one_mrule`
//!   owns that gate, since it is the layer holding the word's unapplication counts.
//!
//! `ModifyFromInput` inversion widens the changed feature lanes back to `full_mask` and no further:
//! the general nested/variable anti-feature-structure cases are not ported.

use pg_featstruct::{add, is_unifiable, priority_union, unify, FeatureStruct};
use pg_fst::{CompileInput, CompileNode, Direction, Fst, FstResult, Segment, Transduce};
use pg_grammar::chardef::CharDefId;
use pg_grammar::featsys::FlatIndex;
use pg_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AllomorphId, AllomorphOwner, CompoundingRuleDef,
    CompoundingSubruleDef, Grammar, LexEntryId, MRuleId, MorphRuleDef, MorphemeId,
    NaturalClassKind, OutputAction, PartRef, Pattern, PatternNode, RealizationalRuleDef,
    ReduplicationHint, SimpleContext, StratumId, TableId,
};
use pg_shape::{CdBits, CdSet, EffectiveCdSet, NodeKind, Shape, ShapeBuilder, NO_CHAR_DEF};

use crate::bridge::{BridgeError, PatternBridge};
use crate::stats::{AnalysisPhase, MRuleStatsCtx};
use crate::stratum::NonHeadRootFilter;
use crate::trace::{FailureReason, TraceHandle, TraceSink};
use crate::word::{MorphRecord, MorphStatus, Word};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

// Analysis-side allomorph/subrule stats attribution; see `crate::stats::MRuleStatsCtx`.

/// Ticks the rule's own `attempts` once on the invocation's first-reached allomorph/subrule (`index`, 0-based) and always records that allomorph's own work/outputs; shifts by one so index 0 never collides with `ALLOMORPH_NONE`.
fn record_mrule_reach(
    mstats: Option<MRuleStatsCtx>,
    index: u32,
    segments: u64,
    outputs: u64,
    reached: &mut u32,
) {
    let Some(ctx) = mstats else { return };
    let allomorph = index + 1;
    if *reached == 0 {
        ctx.stats
            .record_mrule_reach_attempt(ctx.stratum, ctx.id, ctx.direction);
    }
    *reached += 1;
    ctx.stats.record_mrule_allomorph_try(
        ctx.stratum,
        ctx.id,
        allomorph,
        ctx.direction,
        segments,
        outputs,
    );
}

/// Attributes a whole rule invocation (gated before the loop, or zero allomorphs reached) to `ALLOMORPH_NONE`.
fn record_mrule_none_residual(mstats: Option<MRuleStatsCtx>, segments: u64) {
    let Some(ctx) = mstats else { return };
    ctx.stats
        .record_mrule_attempt(ctx.stratum, ctx.id, ctx.direction, segments);
    ctx.stats
        .record_mrule_outcome(ctx.stratum, ctx.id, ctx.direction, 0);
}

/// Post-loop dispatch: zero-reached takes the full `record_mrule_none_residual` residual, reached-but-empty ticks one rule-level "not applied" instead.
fn record_mrule_invocation_end(
    mstats: Option<MRuleStatsCtx>,
    reached: u32,
    total_outputs: u64,
    segments: u64,
) {
    let Some(ctx) = mstats else { return };
    if reached == 0 {
        record_mrule_none_residual(Some(ctx), segments);
    } else if total_outputs == 0 {
        ctx.stats
            .record_mrule_invocation_not_applied(ctx.stratum, ctx.id, ctx.direction);
    }
}

// Table is resolved once per rule application and threaded down explicitly; a word's shape may carry frozen material from an earlier different-table stratum, but nothing here re-derives an identity for it.

// Public API.

/// Apply `rule` forward to `word` (synthesis). Empty if the rule does not apply — gating failed, or
/// no allomorph matched.
///
/// Recompiles every allomorph/subrule LHS FST on every call, deliberately: this entry point is also
/// called on standalone, non-grammar-resident rule fixtures that have no stable index into a
/// `crate::cache::RuleCache`. The real per-word pipeline calls `synthesize_cached` instead.
pub fn synthesize(g: &Grammar, word: &Word, rule: &MorphRuleDef) -> Vec<Word> {
    synthesize_stats(g, word, rule, None)
}

/// `synthesize`'s `--stats`-carrying sibling; `pub(crate)` since only `crate::stratum` needs the ctx.
pub(crate) fn synthesize_stats(
    g: &Grammar,
    word: &Word,
    rule: &MorphRuleDef,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    let out = match rule {
        MorphRuleDef::AffixProcess(def) => synth_affix(g, word, def, mstats),
        MorphRuleDef::Compounding(def) => synth_compound(g, word, def, mstats),
        MorphRuleDef::Realizational(def) => synth_realizational(g, word, def, mstats),
    };
    apply_blocking(g, out, rule.blockable())
}

/// The `crate::cache::RuleCache`-aware sibling of `synthesize`, used by the real per-word pipeline.
/// `mrid` must identify `rule` — every production call site already holds both. Pass
/// `&NoopSink`/`TraceHandle::DUMMY` for an untraced call.
#[allow(clippy::too_many_arguments)]
pub(crate) fn synthesize_cached_traced(
    g: &Grammar,
    mrid: MRuleId,
    word: &Word,
    rule: &MorphRuleDef,
    cache: &crate::cache::RuleCache,
    mstats: Option<MRuleStatsCtx>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    let out = match rule {
        MorphRuleDef::AffixProcess(def) => {
            synth_affix_cached(g, word, def, mrid, cache, mstats, trace, parent)
        }
        MorphRuleDef::Compounding(def) => {
            synth_compound_cached(g, word, def, mrid, cache, mstats, trace, parent)
        }
        MorphRuleDef::Realizational(def) => {
            synth_realizational_cached(g, word, def, mrid, cache, mstats, trace, parent)
        }
    };
    apply_blocking_traced(g, out, rule.blockable(), mrid, trace, parent)
}

/// Untraced sibling of `synthesize_cached_traced`, for callers outside this crate that hold a
/// `&RuleCache` but no trace sink. Returns exactly what `synthesize` returns for the same inputs,
/// provided `mrid` identifies `rule`; only where the LHS FST comes from differs.
pub fn synthesize_cached(
    g: &Grammar,
    mrid: MRuleId,
    word: &Word,
    rule: &MorphRuleDef,
    cache: &crate::cache::RuleCache,
) -> Vec<Word> {
    synthesize_cached_traced(
        g,
        mrid,
        word,
        rule,
        cache,
        None,
        &crate::trace::NoopSink,
        crate::trace::TraceHandle::DUMMY,
    )
}

/// Reports which of `g.mpr_group_ok`'s required/excluded MPR gates actually failed, checked required-then-excluded.
fn mpr_gate_reason(
    g: &Grammar,
    required: pg_grammar::model::MprSet,
    excluded: pg_grammar::model::MprSet,
    have: pg_grammar::model::MprSet,
) -> Option<FailureReason> {
    if !pg_grammar::model::mpr_required_ok(&g.mpr_groups, required, have) {
        return Some(FailureReason::RequiredMprFeatures);
    }
    if !pg_grammar::model::mpr_excluded_ok(&g.mpr_groups, excluded, have) {
        return Some(FailureReason::ExcludedMprFeatures);
    }
    None
}

/// Un-apply `rule` to `word` (analysis); empty if it cannot be un-applied. Recompiles on every
/// call — see `synthesize`'s doc for why. The real pipeline calls `analyze_cached`.
pub fn analyze(g: &Grammar, word: &Word, rule: &MorphRuleDef) -> Vec<Word> {
    analyze_stats(g, word, rule, None)
}

/// `analyze`'s `--stats`-carrying sibling; `pub(crate)` since only `crate::stratum` needs the ctx.
pub(crate) fn analyze_stats(
    g: &Grammar,
    word: &Word,
    rule: &MorphRuleDef,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    match rule {
        MorphRuleDef::AffixProcess(def) => ana_affix(g, word, def, mstats),
        MorphRuleDef::Compounding(def) => ana_compound(g, word, def, None, mstats),
        MorphRuleDef::Realizational(def) => ana_realizational(g, word, def, mstats),
    }
}

/// The `crate::cache::RuleCache`-aware sibling of `analyze`. See `synthesize_cached`'s doc
/// for the `mrid`/`rule` correspondence contract.
pub(crate) fn analyze_cached(
    g: &Grammar,
    mrid: MRuleId,
    word: &Word,
    rule: &MorphRuleDef,
    cache: &crate::cache::RuleCache,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    match rule {
        MorphRuleDef::AffixProcess(def) => ana_affix_cached(g, word, def, cache, mstats),
        MorphRuleDef::Compounding(def) => {
            ana_compound_cached(g, word, def, mrid, cache, None, mstats)
        }
        MorphRuleDef::Realizational(def) => ana_realizational_cached(g, word, def, cache, mstats),
    }
}

/// `analyze_cached`'s sibling for the one call site that also holds the non-head lexicon filter.
/// Only a `Compounding` rule consumes it. Threading the filter in here rather than post-filtering
/// the returned words is what lets root resolution join C#'s **per-subrule** dedup scope — see
/// `ana_compound_subrule`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn analyze_cached_with_root_filter(
    g: &Grammar,
    mrid: MRuleId,
    word: &Word,
    rule: &MorphRuleDef,
    cache: &crate::cache::RuleCache,
    root_filter: NonHeadRootFilter,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    match rule {
        MorphRuleDef::AffixProcess(def) => ana_affix_cached(g, word, def, cache, mstats),
        MorphRuleDef::Compounding(def) => {
            ana_compound_cached(g, word, def, mrid, cache, Some(root_filter), mstats)
        }
        MorphRuleDef::Realizational(def) => ana_realizational_cached(g, word, def, cache, mstats),
    }
}

/// Uncached sibling of `analyze_cached_with_root_filter` (the `cache: None` production fallback —
/// see `analyze`'s doc for why that path still exists).
pub fn analyze_with_root_filter(
    g: &Grammar,
    word: &Word,
    rule: &MorphRuleDef,
    root_filter: NonHeadRootFilter,
) -> Vec<Word> {
    analyze_with_root_filter_stats(g, word, rule, root_filter, None)
}

/// `analyze_with_root_filter`'s `--stats`-carrying sibling; `pub(crate)` since only `crate::stratum` needs the ctx.
pub(crate) fn analyze_with_root_filter_stats(
    g: &Grammar,
    word: &Word,
    rule: &MorphRuleDef,
    root_filter: NonHeadRootFilter,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    match rule {
        MorphRuleDef::AffixProcess(def) => ana_affix(g, word, def, mstats),
        MorphRuleDef::Compounding(def) => ana_compound(g, word, def, Some(root_filter), mstats),
        MorphRuleDef::Realizational(def) => ana_realizational(g, word, def, mstats),
    }
}

// Traced analysis: thin event-emitting shells around the untraced matchers, reimplementing no logic; for `Compounding`, the `Pattern` reason also covers "matched but `resolve_non_head_roots` found no lexicon entry", the closest existing bucket.

/// `analyze_cached`'s traced sibling, sharing this section's fast-path and reason-mapping contract.
pub(crate) fn analyze_cached_traced(
    g: &Grammar,
    mrid: MRuleId,
    word: &Word,
    rule: &MorphRuleDef,
    cache: &crate::cache::RuleCache,
    mstats: Option<MRuleStatsCtx>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    if !trace.is_tracing() {
        return analyze_cached(g, mrid, word, rule, cache, mstats);
    }
    match rule {
        MorphRuleDef::AffixProcess(def) => {
            ana_affix_cached_traced(g, word, def, mrid, cache, mstats, trace, parent)
        }
        MorphRuleDef::Compounding(def) => {
            ana_compound_cached_traced(g, word, def, mrid, cache, None, mstats, trace, parent)
        }
        MorphRuleDef::Realizational(def) => {
            ana_realizational_cached_traced(g, word, def, mrid, cache, mstats, trace, parent)
        }
    }
}

/// `analyze_cached_with_root_filter`'s traced sibling — see this section's header.
#[allow(clippy::too_many_arguments)]
pub(crate) fn analyze_cached_with_root_filter_traced(
    g: &Grammar,
    mrid: MRuleId,
    word: &Word,
    rule: &MorphRuleDef,
    cache: &crate::cache::RuleCache,
    root_filter: NonHeadRootFilter,
    mstats: Option<MRuleStatsCtx>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    if !trace.is_tracing() {
        return analyze_cached_with_root_filter(g, mrid, word, rule, cache, root_filter, mstats);
    }
    match rule {
        MorphRuleDef::AffixProcess(def) => {
            ana_affix_cached_traced(g, word, def, mrid, cache, mstats, trace, parent)
        }
        MorphRuleDef::Compounding(def) => ana_compound_cached_traced(
            g,
            word,
            def,
            mrid,
            cache,
            Some(root_filter),
            mstats,
            trace,
            parent,
        ),
        MorphRuleDef::Realizational(def) => {
            ana_realizational_cached_traced(g, word, def, mrid, cache, mstats, trace, parent)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn ana_affix_cached_traced(
    g: &Grammar,
    word: &Word,
    rule: &AffixProcessRuleDef,
    mrid: MRuleId,
    cache: &crate::cache::RuleCache,
    mstats: Option<MRuleStatsCtx>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    let Some(new_syn) = ana_syn_fs(g, rule.required_syn_fs, rule.out_syn_fs, word) else {
        trace.morphological_rule_not_unapplied(
            parent,
            mrid,
            -1,
            word,
            FailureReason::RequiredSyntacticFeatureStruct,
        );
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    let mut output = Vec::new();
    let mut reached: u32 = 0;
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        let Some((fst, lhs)) = cache.allomorph(allo.id).ana_lhs.as_ref() else {
            continue;
        };
        let before = output.len();
        for mut w in ana_affix_allomorph(
            g, table, word, allo, lhs, fst, &segs, &node_of, &new_syn, mstats,
        ) {
            w.trace = Some(trace.morphological_rule_unapplied(parent, mrid, i as i32, &w));
            output.push(w);
        }
        let n = (output.len() - before) as u64;
        record_mrule_reach(mstats, i as u32, segs.len() as u64, n, &mut reached);
        if output.len() == before {
            trace.morphological_rule_not_unapplied(
                parent,
                mrid,
                i as i32,
                word,
                FailureReason::Pattern,
            );
        }
    }
    record_mrule_invocation_end(mstats, reached, output.len() as u64, segs.len() as u64);
    output
}

#[allow(clippy::too_many_arguments)]
fn ana_realizational_cached_traced(
    g: &Grammar,
    word: &Word,
    rule: &RealizationalRuleDef,
    mrid: MRuleId,
    cache: &crate::cache::RuleCache,
    mstats: Option<MRuleStatsCtx>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    let Some(real_fs) = unify(g.fs_interner.get(rule.real_fs), &word.real_fs) else {
        trace.morphological_rule_not_unapplied(
            parent,
            mrid,
            -1,
            word,
            FailureReason::RequiredSyntacticFeatureStruct,
        );
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    let mut output = Vec::new();
    let mut reached: u32 = 0;
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        let Some((fst, lhs)) = cache.allomorph(allo.id).ana_lhs.as_ref() else {
            continue;
        };
        let before = output.len();
        for mut w in ana_realizational_allomorph(
            g, table, word, allo, lhs, fst, &segs, &node_of, &real_fs, mstats,
        ) {
            w.trace = Some(trace.morphological_rule_unapplied(parent, mrid, i as i32, &w));
            output.push(w);
        }
        let n = (output.len() - before) as u64;
        record_mrule_reach(mstats, i as u32, segs.len() as u64, n, &mut reached);
        if output.len() == before {
            trace.morphological_rule_not_unapplied(
                parent,
                mrid,
                i as i32,
                word,
                FailureReason::Pattern,
            );
        }
    }
    record_mrule_invocation_end(mstats, reached, output.len() as u64, segs.len() as u64);
    output
}

#[allow(clippy::too_many_arguments)]
fn ana_compound_cached_traced(
    g: &Grammar,
    word: &Word,
    rule: &CompoundingRuleDef,
    mrid: MRuleId,
    cache: &crate::cache::RuleCache,
    root_filter: Option<NonHeadRootFilter>,
    mstats: Option<MRuleStatsCtx>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    let Some(new_syn) = ana_syn_fs(g, rule.head_required_syn_fs, rule.out_syn_fs, word) else {
        trace.morphological_rule_not_unapplied(
            parent,
            mrid,
            -1,
            word,
            FailureReason::HeadRequiredSyntacticFeatureStruct,
        );
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };
    let table = crate::cache::owning_table_for_mrule(g, mrid).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    let cc = cache.compound(mrid);
    let mut output = Vec::new();
    let mut reached: u32 = 0;
    for (i, sr) in rule.subrules.iter().enumerate() {
        let Some((fst, lhs)) = cc.subrules[i].ana.as_ref() else {
            continue;
        };
        let before = output.len();
        for mut w in ana_compound_subrule(
            g,
            table,
            word,
            rule,
            sr,
            lhs,
            fst,
            &segs,
            &node_of,
            &new_syn,
            root_filter,
        ) {
            w.trace = Some(trace.morphological_rule_unapplied(parent, mrid, i as i32, &w));
            output.push(w);
        }
        let n = (output.len() - before) as u64;
        record_mrule_reach(mstats, i as u32, segs.len() as u64, n, &mut reached);
        if output.len() == before {
            trace.morphological_rule_not_unapplied(
                parent,
                mrid,
                i as i32,
                word,
                FailureReason::Pattern,
            );
        }
    }
    record_mrule_invocation_end(mstats, reached, output.len() as u64, segs.len() as u64);
    output
}

// Lexical-family blocking — `Word.CheckBlocking` / the `ChooseInflectionalStem` seed helper.

/// `Word.CheckBlocking`: if the word's root morpheme belongs to a family, search the family's other
/// entries in document order for one in the SAME stratum whose lexical syntactic FS is subsumed by
/// this word's accumulated syntactic FS. The first match wins and the word is replaced by a fresh
/// root-level word seeded from that entry's primary allomorph, discarding every rule applied so
/// far. `None` when not blocked. Compounding outputs carry the head's root allomorph forward, so no
/// rule-kind branch is needed.
///
/// The guessed-root arm is load-bearing, not defensive: blocking runs on the output of any
/// blockable rule, including one applied over a guessed root, and indexing `allomorph_owners` with
/// the sentinel panics. `None` is also the faithful answer — a guessed root has no family.
pub(crate) fn check_blocking(g: &Grammar, w: &Word) -> Option<Word> {
    let root_id = w.root_allomorph?;
    if root_id == AllomorphId::GUESSED {
        return None;
    }
    let AllomorphOwner::Root(le, _) = g.allomorph_owners[root_id.0 as usize] else {
        return None;
    };
    let family = g.entries[le.0 as usize].family?;
    for &other in &g.families[family.0 as usize].entries {
        if other == le {
            continue;
        }
        let entry = &g.entries[other.0 as usize];
        if g.morphemes[entry.morpheme.0 as usize].stratum != w.stratum {
            continue;
        }
        if pg_featstruct::subsumes(&w.syn_fs, g.fs_interner.get(entry.syn_fs)) {
            return Some(seed_from_entry(g, other, w.real_fs.clone()));
        }
    }
    None
}

/// Runs `check_blocking` once over the whole output rather than inline per C#'s three loops — observably equivalent, since blocking only substitutes one already-produced word and never affects loop continuation.
fn apply_blocking(g: &Grammar, words: Vec<Word>, blockable: bool) -> Vec<Word> {
    if !blockable {
        return words;
    }
    words
        .into_iter()
        .map(|w| check_blocking(g, &w).unwrap_or(w))
        .collect()
}

/// `apply_blocking`'s traced sibling: blocking runs as a post-pass, so `Applied` is emitted for the pre-block word — an accepted approximation of C#'s event ordering (counts and reasons still match).
fn apply_blocking_traced(
    g: &Grammar,
    words: Vec<Word>,
    blockable: bool,
    mrid: MRuleId,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    if !blockable {
        return words;
    }
    words
        .into_iter()
        .map(|w| match check_blocking(g, &w) {
            Some(mut new_word) => {
                if trace.is_tracing() {
                    trace.blocked(parent, mrid, &new_word);
                }
                new_word.trace = w.trace;
                new_word
            }
            None => w,
        })
        .collect()
}

/// Build a fresh root-level word from `le`'s primary allomorph — C#'s `Word(RootAllomorph,
/// FeatureStruct)` ctor. The primary allomorph is index 0; the loader never reorders them. Every
/// field starts fresh from the entry, with no rule-application history, EXCEPT the realizational
/// FS, which the caller supplies from the current word — a bare `LexEntry` has no such concept.
pub(crate) fn seed_from_entry(g: &Grammar, le: LexEntryId, real_fs: FeatureStruct) -> Word {
    let entry = &g.entries[le.0 as usize];
    let allo = &entry.allomorphs[0];
    let stratum = g.morphemes[entry.morpheme.0 as usize].stratum;
    let table = &g.char_tables[g.strata[stratum.0 as usize].table.0 as usize];
    let shape = crate::shape_feat::segment_with_features(g, table, &allo.shape.text)
        .unwrap_or_else(|_| allo.shape.shape.clone());
    let mut w = Word::new(shape, stratum);
    w.syn_fs = g.fs_interner.get(entry.syn_fs).clone();
    w.mpr = entry.mpr;
    w.flags.is_partial = entry.partial;
    w.root_allomorph = Some(allo.id);
    w.real_fs = real_fs;
    w.morphs = vec![MorphRecord::new(allo.id, entry.morpheme, 0)];
    w
}

// Feature / lane helpers.

fn feat_width(g: &Grammar) -> usize {
    g.phon_features.len()
}

fn full_mask(g: &Grammar, f: usize) -> u64 {
    g.phon_features.mask(FlatIndex(f as u32))
}

/// Driver full-mask lane vector (width `W`, unconstrained everywhere).
fn full_lanes(g: &Grammar) -> Vec<u64> {
    (0..feat_width(g)).map(|f| full_mask(g, f)).collect()
}

/// Fit a lane row to width `W`: truncate extra, pad missing with `full_mask` (unconstrained).
fn fit(g: &Grammar, lanes: &[u64]) -> Vec<u64> {
    let w = feat_width(g);
    let mut out = full_lanes(g);
    for (i, slot) in out.iter_mut().enumerate().take(w) {
        if let Some(&l) = lanes.get(i) {
            *slot = l;
        }
    }
    out
}

/// Driver lanes for a char-def, `full_mask` for unmentioned/boundary lanes; `table` is the rule's own owning table, never an implicit default.
fn cd_lanes(g: &Grammar, table: TableId, cd_raw: u32) -> Vec<u64> {
    if cd_raw == NO_CHAR_DEF {
        return full_lanes(g);
    }
    let t = &g.char_tables[table.0 as usize];
    fit(g, t.get(CharDefId(cd_raw)).feature_lanes())
}

/// The `(feature, symbol-bits)` a `SimpleContext` pins; alpha-variable features are left unconstrained.
fn ctx_pins(g: &Grammar, table: TableId, ctx: &SimpleContext) -> Vec<(usize, u64)> {
    let w = feat_width(g);
    let t = &g.char_tables[table.0 as usize];
    let nc = &g.natural_classes[ctx.nat_class.0 as usize];
    let alpha: HashSet<usize> = ctx.vars.iter().map(|v| v.feature.0 as usize).collect();
    match &nc.kind {
        NaturalClassKind::Feature(pairs) => pairs
            .iter()
            .filter(|(f, _)| !alpha.contains(&(f.0 as usize)))
            .map(|(f, b)| (f.0 as usize, b.0))
            .collect(),
        NaturalClassKind::Segments(segs) => (0..w)
            .filter_map(|f| {
                let bits = segs
                    .iter()
                    .fold(0u64, |acc, cd| acc | fit(g, t.get(*cd).feature_lanes())[f]);
                (bits != full_mask(g, f)).then_some((f, bits))
            })
            .collect(),
    }
}

/// Driver lanes for a `SimpleContext` (width `W`).
fn ctx_lanes(g: &Grammar, table: TableId, ctx: &SimpleContext) -> Vec<u64> {
    let mut lanes = full_lanes(g);
    for (f, bits) in ctx_pins(g, table, ctx) {
        lanes[f] = bits;
    }
    lanes
}

/// The char-def-set a `SimpleContext`'s natural class carries; `Unrestricted` rather than a full-table bitset when the class means "any segment".
fn ctx_cd_set(g: &Grammar, table: TableId, ctx: &SimpleContext) -> CdSet {
    let nc = &g.natural_classes[ctx.nat_class.0 as usize];
    match &nc.kind {
        NaturalClassKind::Segments(segs) => {
            CdSet::Members(CdBits::from_ids(segs.iter().map(|cd| cd.0)))
        }
        NaturalClassKind::Feature(_) => {
            let pins = ctx_pins(g, table, ctx);
            if pins.is_empty() {
                // Nothing pinned means every feature is alpha-variable-governed, so the class matches every segment.
                return CdSet::Unrestricted;
            }
            let t = &g.char_tables[table.0 as usize];
            let mut members = Vec::new();
            let mut all = true;
            for (id, cd) in t.iter() {
                if cd.kind() != pg_grammar::chardef::CharDefKind::Segment {
                    continue;
                }
                let lanes = fit(g, cd.feature_lanes());
                if pins.iter().all(|&(f, bits)| lanes[f] & bits != 0) {
                    members.push(id.0);
                } else {
                    all = false;
                }
            }
            if all {
                CdSet::Unrestricted
            } else {
                CdSet::Members(CdBits::from_ids(members))
            }
        }
    }
}

/// The owned `CdSet` for a copied `OutNode`: harmlessly `Unrestricted` for a concrete source (its `char_def` already carries identity), real propagation only when the source was itself `NO_CHAR_DEF`.
fn cd_set_of(shape: &Shape, p: usize) -> CdSet {
    match shape.node_cd_set(p) {
        EffectiveCdSet::Singleton(_) | EffectiveCdSet::Unrestricted => CdSet::Unrestricted,
        EffectiveCdSet::Members(b) => CdSet::Members(b.clone()),
    }
}

/// Converts driver full-mask lanes to FST-facing lanes (`full_mask` -> `u64::MAX`) so constraints canonicalize identically to `bridge`/`rewrite`.
fn to_fst(g: &Grammar, lanes: &[u64]) -> Vec<u64> {
    lanes
        .iter()
        .enumerate()
        .map(|(f, &l)| if l == full_mask(g, f) { u64::MAX } else { l })
        .collect()
}

// Segment sequences + shape freezing.

/// Build the FST segment sequence for a shape under the matcher filter, plus a `seg-pos → shape
/// node index` map. Synthesis includes boundaries as optional segments; analysis is `Segment`-only.
///
/// A `Segment`-kind node carrying its own Optional flag must also be passed through as optional,
/// not just boundaries. Phonological analysis marks re-inserted deleted material and unapplied
/// epenthesis Optional precisely so a later morphological analysis LHS can explore both "present"
/// and "skipped"; passing such a node as mandatory silently discards that signal and makes any
/// affix rule sitting above such a phonological rule unable to skip it.
pub(crate) fn segs_of(
    g: &Grammar,
    table: TableId,
    shape: &Shape,
    include_boundaries: bool,
) -> (Vec<Segment>, Vec<usize>) {
    // The `StrRep` identity lane: an `Unrestricted` node or a too-wide table omits it, and absent means all-ones (matches any identity), matching a C# node with no `StrRep`.
    let id_width = crate::bridge::id_lane_width(g, table);
    let id_bits = |i: usize| -> Option<u64> {
        match shape.node_cd_set(i) {
            pg_shape::EffectiveCdSet::Singleton(cd) => Some(1u64 << cd),
            pg_shape::EffectiveCdSet::Members(b) => b.as_u64(),
            pg_shape::EffectiveCdSet::Unrestricted => None,
        }
    };
    let with_id = |i: usize, mut lanes: Vec<u64>| -> Vec<u64> {
        if let Some(w) = id_width {
            // Only reached on ≤64-def tables, so a `Singleton` shift can never overflow.
            if let Some(bits) = id_bits(i) {
                crate::bridge::push_id_lane(&mut lanes, w, bits);
            }
        }
        lanes
    };
    let mut segs = Vec::new();
    let mut node_of = Vec::new();
    for i in 0..shape.len() {
        match shape.kind(i) {
            NodeKind::Segment => {
                let lanes = with_id(i, shape.node_lanes(i).to_vec());
                segs.push(if shape.flags(i).is_optional() {
                    Segment::optional(lanes)
                } else {
                    Segment::new(lanes)
                });
                node_of.push(i);
            }
            NodeKind::Boundary if include_boundaries => {
                segs.push(Segment::optional(with_id(i, shape.node_lanes(i).to_vec())));
                node_of.push(i);
            }
            _ => {}
        }
    }
    (segs, node_of)
}

/// Provenance of an output node, used for morph attribution and span remapping.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Origin {
    /// Copied from the head/input word's interior node `idx` (0-based interior index).
    Head(usize),
    /// Copied from the non-head word's interior node `idx`.
    NonHead(usize),
    /// New affix material (InsertSegments/InsertContext/ModifyFromInput on an affix rule).
    Affix,
    /// Inserted linker material carrying no morpheme (e.g. a compounding "+" boundary).
    Insert,
}

#[derive(Clone, Debug)]
struct OutNode {
    kind: NodeKind,
    char_def: u32,
    lanes: Vec<u64>,
    optional: bool,
    origin: Origin,
    /// Char-def-set identity, consulted only when `char_def == NO_CHAR_DEF`; a real `char_def` leaves this `Unrestricted` and unread.
    cd_set: CdSet,
}

/// Freezes interior `OutNode`s into a bracketed `Shape`; optional segments use a delete-then-reinsert workaround since `ShapeBuilder` has no set-flags-in-place.
fn freeze_out(g: &Grammar, nodes: &[OutNode]) -> Shape {
    let w = feat_width(g) as u32;
    let mut b = ShapeBuilder::with_features_capacity(w, nodes.len());
    for n in nodes {
        let lanes = fit(g, &n.lanes);
        match n.kind {
            // Class insertions carry their real cd_set; a concrete segment's own char_def is already the identity, so `n.cd_set` goes unread there.
            NodeKind::Segment if n.char_def == NO_CHAR_DEF => {
                b.push_segment_with_lanes_and_set(&lanes, n.cd_set.clone())
            }
            NodeKind::Segment => b.push_segment_with_lanes(n.char_def, &lanes),
            NodeKind::Boundary => b.push_boundary_with_lanes(n.char_def, &lanes),
            _ => {}
        }
    }
    let mut shape = b.finish();
    let optional_positions: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.optional && n.kind == NodeKind::Segment)
        .map(|(i, _)| i + 1) // +1 for the left anchor
        .collect();
    if !optional_positions.is_empty() {
        let mut m = ShapeBuilder::from_shape(&shape);
        for idx in optional_positions {
            let n = &nodes[idx - 1];
            let lanes = fit(g, &n.lanes);
            m.delete(idx);
            if n.char_def == NO_CHAR_DEF {
                m.insert_with_set(
                    idx,
                    pg_shape::NodeFlags(pg_shape::NodeFlags::OPTIONAL),
                    &lanes,
                    n.cd_set.clone(),
                );
            } else {
                m.insert(
                    idx,
                    NodeKind::Segment,
                    n.char_def,
                    pg_shape::NodeFlags(pg_shape::NodeFlags::OPTIONAL),
                    &lanes,
                );
            }
        }
        shape = m.freeze();
    }
    shape
}

// Part-group matching (synthesis + compounding head/non-head).

/// Compile a list of LHS `parts` into one FST whose parts are wrapped in named capture groups
/// (`{prefix}{i}`), returning the FST and the group names in order. `pub(crate)` so
/// `crate::cache::RuleCache::build` can call it once per allomorph/subrule instead of recompiling.
pub(crate) fn compile_parts(
    g: &Grammar,
    table: TableId,
    parts: &[Pattern],
    prefix: &str,
    deterministic: bool,
) -> Result<(Fst, Vec<String>), BridgeError> {
    // Morphological-LHS FSTs carry the `StrRep` identity lane, matching what `segs_of` emits.
    let bridge = PatternBridge::new(g)
        .with_table(table)
        .deterministic(deterministic)
        .id_lane(true);
    let mut nodes = Vec::new();
    let mut names = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let compiled = bridge.compile_pattern(part)?;
        let name = format!("{prefix}{i}");
        nodes.push(CompileNode::Group {
            name: name.clone(),
            children: compiled.input.nodes,
        });
        names.push(name);
    }
    let fst = CompileInput::new(nodes)
        .deterministic(deterministic)
        .compile_with_direction(Direction::LeftToRight);
    Ok((fst, names))
}

/// Per-part captured `(start, end)` seg-position ranges (`None` = not captured / zero segments).
fn part_ranges(fst: &Fst, names: &[String], result: &FstResult) -> Vec<Option<(usize, usize)>> {
    names
        .iter()
        .map(|name| {
            fst.get_offsets(name, &result.registers)
                .map(|(a, b)| (a as usize, b as usize))
        })
        .collect()
}

// Morph attribution.

/// Which input morph owns source node `idx`: only `Real` records own nodes (others are non-owning markers), and `Real` orders never tie, so `max_by_key` is unambiguous.
fn owning_morph(word: &Word, idx: usize) -> Option<usize> {
    word.morphs
        .iter()
        .enumerate()
        .filter(|(_, m)| m.status == MorphStatus::Real && (m.order as usize) <= idx)
        .max_by_key(|(_, m)| m.order)
        .map(|(i, _)| i)
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
enum MorphKey {
    Head(usize),
    NonHead(usize),
    Affix,
}

/// Builds the output word's `MorphRecord`s, one per **contiguous run** of a morph's output positions (mirrors C# `MarkMorphs`), so a discontinuous morph's two pieces get separate records checked at their own spans.
fn attribute_morphs(
    out: &[OutNode],
    head: &Word,
    non_head: Option<&Word>,
    affix: Option<(
        pg_grammar::model::AllomorphId,
        pg_grammar::model::MorphemeId,
        &[u16],
    )>,
) -> Vec<MorphRecord> {
    // Pass 1: output positions per input morph / affix (Real records only, via `owning_morph`).
    let mut by_morph: HashMap<MorphKey, Vec<u32>> = HashMap::default();
    for (pos, n) in out.iter().enumerate() {
        let key = match n.origin {
            Origin::Head(idx) => match owning_morph(head, idx) {
                Some(mi) => MorphKey::Head(mi),
                None => continue,
            },
            Origin::NonHead(idx) => {
                let Some(nh) = non_head else { continue };
                match owning_morph(nh, idx) {
                    Some(mi) => MorphKey::NonHead(mi),
                    None => continue,
                }
            }
            Origin::Affix => {
                if affix.is_none() {
                    continue;
                }
                MorphKey::Affix
            }
            Origin::Insert => continue,
        };
        by_morph.entry(key).or_default().push(pos as u32);
    }

    // Contiguous runs per key, as `(order, len)` pairs — see this function's doc.
    fn runs_of(positions: &[u32]) -> Vec<(u32, u32)> {
        let mut runs = Vec::new();
        let mut run_start = 0usize;
        for i in 0..positions.len() {
            if i + 1 == positions.len() || positions[i + 1] != positions[i] + 1 {
                runs.push((positions[run_start], (i - run_start + 1) as u32));
                run_start = i + 1;
            }
        }
        runs
    }
    let key_runs: HashMap<MorphKey, Vec<(u32, u32)>> =
        by_morph.iter().map(|(k, ps)| (*k, runs_of(ps))).collect();
    // First-longest run wins at ties, matching C# `MarkMorphs`' strict `>`.
    let longest_run_order = |key: &MorphKey| -> Option<u32> {
        key_runs.get(key).map(|rs| {
            let mut best = rs[0];
            for &r in &rs[1..] {
                if r.1 > best.1 {
                    best = r;
                }
            }
            best.0
        })
    };
    // The affix's "primary" annotation — the attachment point for subsumption.
    let affix_host_order: Option<u32> = longest_run_order(&MorphKey::Affix);

    // Pass 2: walks each input word's morphs in order; an unpositioned record subsumes onto the affix's new material or order 0, a dropped marker re-anchors to its host, and pure truncation drops a `SubsumedChild` but not a `SubsumedFirst` (bug-compatible with C#'s non-recursing truncation branch); compounding has no fallbacks at all.
    let mut records: Vec<MorphRecord> = Vec::new();
    let mut marked: Vec<pg_grammar::model::AllomorphId> = Vec::new();

    let push_runs = |records: &mut Vec<MorphRecord>, key: &MorphKey, m: &MorphRecord| {
        for &(order, _) in &key_runs[key] {
            records.push(MorphRecord {
                allomorph: m.allomorph,
                morpheme: m.morpheme,
                order,
                passed_over: m.passed_over.clone(),
                status: MorphStatus::Real,
                runtime_root: m.runtime_root.clone(),
            });
        }
    };

    // Head-word records (in stored order). Non-head records handled after (compounding only).
    for (mi, m) in head.morphs.iter().enumerate() {
        let key = MorphKey::Head(mi);
        if key_runs.contains_key(&key) {
            push_runs(&mut records, &key, m);
            marked.push(m.allomorph);
            continue;
        }
        if affix.is_none() {
            continue; // compounding: untouched input morphs are dropped (no fallback in C#)
        }
        match m.status {
            MorphStatus::Floating => continue, // handled by the floater block below
            MorphStatus::Real => {
                if marked.contains(&m.allomorph) {
                    continue;
                }
                if let Some(host_order) = affix_host_order {
                    records.push(MorphRecord {
                        order: host_order,
                        status: MorphStatus::SubsumedChild,
                        ..m.clone()
                    });
                } else if !out.is_empty() {
                    records.push(MorphRecord {
                        order: 0,
                        status: MorphStatus::SubsumedFirst,
                        ..m.clone()
                    });
                }
                marked.push(m.allomorph);
            }
            MorphStatus::SubsumedChild | MorphStatus::SubsumedFirst => {
                if marked.contains(&m.allomorph) {
                    continue;
                }
                // Host = the unique Real input record sharing this order.
                let host = head
                    .morphs
                    .iter()
                    .position(|h| h.status == MorphStatus::Real && h.order == m.order);
                let host_runs = host.and_then(|hi| longest_run_order(&MorphKey::Head(hi)));
                let new_anchor = match (host_runs, affix_host_order) {
                    // Host still has material: follow it (keep the variant's placement semantics).
                    (Some(o), _) => Some((o, m.status)),
                    // Host dropped too, rule has new material: both subsume onto the new morph.
                    (None, Some(o)) => Some((o, MorphStatus::SubsumedChild)),
                    // Host dropped, pure truncation: C#'s Shape.First branch doesn't recurse into children (SubsumedChild lost, bug-compatible); SubsumedFirst re-anchors at the new first node.
                    (None, None) => match m.status {
                        MorphStatus::SubsumedFirst if !out.is_empty() => {
                            Some((0, MorphStatus::SubsumedFirst))
                        }
                        _ => None,
                    },
                };
                if let Some((order, status)) = new_anchor {
                    records.push(MorphRecord {
                        order,
                        status,
                        ..m.clone()
                    });
                    marked.push(m.allomorph);
                }
            }
        }
    }
    if let Some(nh) = non_head {
        for (mi, m) in nh.morphs.iter().enumerate() {
            let key = MorphKey::NonHead(mi);
            if key_runs.contains_key(&key) {
                push_runs(&mut records, &key, m);
            }
        }
    }

    // Floating markers ride, resolve onto this hop's new material, or mint a new one; pushed after subsumed-input records and before affix runs, approximating C#'s input-morph-order attachment.
    if let Some((a, mo, p)) = affix {
        let floaters = head
            .morphs
            .iter()
            .filter(|m| m.status == MorphStatus::Floating);
        if let Some(host_order) = affix_host_order {
            for f in floaters {
                records.push(MorphRecord {
                    order: host_order,
                    status: MorphStatus::SubsumedChild,
                    ..f.clone()
                });
            }
        } else {
            records.extend(floaters.cloned());
            // Pure truncation mints this rule's own floating marker, guarded since an entirely empty `out` has no last node for C# either.
            if !out.is_empty() {
                records.push(MorphRecord {
                    allomorph: a,
                    morpheme: mo,
                    order: FLOATING_ORDER,
                    passed_over: Some(p.into()),
                    status: MorphStatus::Floating,
                    runtime_root: None,
                });
            }
        }
        // The affix's own runs go last, so same-order subsumed/resolved records above render before their host (stable sort at ties).
        if key_runs.contains_key(&MorphKey::Affix) {
            for &(order, _) in &key_runs[&MorphKey::Affix] {
                records.push(MorphRecord {
                    allomorph: a,
                    morpheme: mo,
                    order,
                    passed_over: Some(p.into()),
                    status: MorphStatus::Real,
                    runtime_root: None,
                });
            }
        }
    }

    records.sort_by_key(|m| m.order);
    records
}

/// Sentinel `order` for an unresolved floating marker: larger than any real position, so it never matches `owning_morph`'s filter and always sorts last.
const FLOATING_ORDER: u32 = u32::MAX;

// RHS execution (synthesis) — shared by affix and compounding.

/// Resolve a `PartRef` to the matched source (segments + node map + captured range + origin tag).
struct PartSource<'a> {
    node_of: &'a [usize],
    shape: &'a Shape,
    range: Option<(usize, usize)>,
    head: bool, // true = Origin::Head, false = Origin::NonHead
}

/// Copies `src`'s captured nodes into `out`, tagging origin; `force_origin` overrides the default Copy/Modify-based choice, which is how `classify_redup` marks a repeated copy as new rather than existing.
fn copy_part(
    g: &Grammar,
    table: TableId,
    out: &mut Vec<OutNode>,
    src: &PartSource,
    modify: Option<&SimpleContext>,
    force_origin: Option<bool>,
) {
    let Some((s, e)) = src.range else { return };
    let pins = modify.map(|c| ctx_pins(g, table, c)).unwrap_or_default();
    // C# `GetSkippedOptionalNodes`: a run of Optional nodes immediately left of the capture, reaching back to the left anchor, folds into the copy — hence the two-pronged boundary-or-optional predicate.
    let mut positions: Vec<usize> = Vec::new();
    if s < e {
        let first_node = src.node_of[s];
        let skippable =
            |i: usize| src.shape.kind(i) == NodeKind::Boundary || src.shape.flags(i).is_optional();
        let mut i = first_node;
        while i > 0 && skippable(i - 1) {
            i -= 1;
        }
        // The walk must stop AT the left anchor for the fold to apply; stopping at any non-optional interior node folds nothing.
        if i == 1 {
            positions.extend(1..first_node);
        }
    }
    positions.extend(src.node_of[s..e].iter().copied());
    for &p in &positions {
        let mut lanes = fit(g, src.shape.node_lanes(p));
        let kind = src.shape.kind(p);
        let mut char_def = src.shape.char_def(p);
        let mut cd_set = cd_set_of(src.shape, p);
        if kind == NodeKind::Segment {
            for &(f, bits) in &pins {
                lanes[f] = bits; // priority-union: the ctx value wins
            }
            if let Some(ctx) = modify {
                // A modified node must not keep the source's literal char_def, or a modified "p" would still render/match only as "p"; clearing to `NO_CHAR_DEF` plus `ctx_cd_set` cannot under-restrict.
                char_def = NO_CHAR_DEF;
                cd_set = ctx_cd_set(g, table, ctx);
            }
        }
        let interior = p - 1; // anchor at index 0
        let existing_origin = if src.head {
            Origin::Head(interior)
        } else {
            Origin::NonHead(interior)
        };
        let origin = match force_origin {
            Some(true) => existing_origin,
            Some(false) => Origin::Affix,
            None if modify.is_some() => {
                // ModifyFromInput material is "new" (affix) for an affix rule but stays with its source morph for compounding, since compounding callers always pass modify=None.
                Origin::Affix
            }
            None => existing_origin,
        };
        out.push(OutNode {
            kind,
            char_def,
            lanes,
            optional: src.shape.flags(p).is_optional(),
            origin,
            cd_set,
        });
    }
}

/// Appends an `InsertSegments` shape's interior nodes to `out`; these always carry a concrete `char_def`, so `cd_set` stays `Unrestricted` and unread.
fn insert_segments(
    g: &Grammar,
    table: TableId,
    out: &mut Vec<OutNode>,
    seg_shape: &Shape,
    origin: Origin,
) {
    for (idx, kind, char_def, _flags) in seg_shape.interior() {
        let _ = idx;
        out.push(OutNode {
            kind,
            char_def,
            lanes: cd_lanes(g, table, char_def),
            optional: false,
            origin,
            cd_set: CdSet::Unrestricted,
        });
    }
}

// Syntactic-FS gating.

/// C# synthesis gate: unify `required` with the word's syn FS, then priority-union `out`; `None` if the unify fails.
fn synth_syn_fs(
    g: &Grammar,
    req: pg_featstruct::FsId,
    out: pg_featstruct::FsId,
    word: &Word,
) -> Option<FeatureStruct> {
    let req_fs = g.fs_interner.get(req);
    if !is_unifiable(req_fs, &word.syn_fs) {
        return None;
    }
    let unified = unify(req_fs, &word.syn_fs)?;
    Some(priority_union(&unified, g.fs_interner.get(out)))
}

/// C# analysis guard: gates on `out.IsUnifiable(word.syn)` (the rule's OUTPUT against the input, not `req`), then widens with `Add`, never a narrowing unify.
fn ana_syn_fs(
    g: &Grammar,
    req: pg_featstruct::FsId,
    out: pg_featstruct::FsId,
    word: &Word,
) -> Option<FeatureStruct> {
    let out_fs = g.fs_interner.get(out);
    if !is_unifiable(out_fs, &word.syn_fs) {
        return None;
    }
    let req_fs = g.fs_interner.get(req);
    if !req_fs.is_empty() {
        Some(add(&word.syn_fs, req_fs, &|f| g.syn_features.mask(f)))
    } else if out_fs.is_empty() {
        Some(FeatureStruct::EMPTY)
    } else {
        Some(word.syn_fs.clone())
    }
}

// MPR-group-aware gating lives on `Grammar` only: a flat overlap check here would invert C#'s AND semantics for 2+ ungrouped `required` members. The compounding prod-restriction gates are the deliberate exception, via `MprSet::compound_match`.

/// C# `HashSet<AllomorphEnvironment>.SetEquals` — environment lists compared as sets (shared by
/// `constraints_equal` and `crate::validity`'s root-allomorph `ConstraintsEqual` port).
pub(crate) fn env_set_equal(
    a: &[pg_grammar::model::EnvironmentDef],
    b: &[pg_grammar::model::EnvironmentDef],
) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut used = vec![false; b.len()];
    'outer: for x in a {
        for (i, y) in b.iter().enumerate() {
            if !used[i] && x == y {
                used[i] = true;
                continue 'outer;
            }
        }
        return false;
    }
    true
}

/// `Allomorph.ConstraintsEqual` as `AffixProcessAllomorph` overrides it: same environments as a
/// set, same required/excluded MPR sets, structurally identical LHS pattern list (order matters),
/// and a value-equal required syntactic FS.
pub(crate) fn constraints_equal(g: &Grammar, a: &AffixAllomorphDef, b: &AffixAllomorphDef) -> bool {
    env_set_equal(&a.environments, &b.environments)
        && a.required_mpr == b.required_mpr
        && a.excluded_mpr == b.excluded_mpr
        && a.lhs == b.lhs
        && g.fs_interner.get(a.required_syn_fs) == g.fs_interner.get(b.required_syn_fs)
}

/// `Allomorph.FreeFluctuatesWith`: callers always pass adjacent allomorphs, so C#'s index-range walk collapses to one `constraints_equal` check.
fn free_fluctuates_with(g: &Grammar, cur: &AffixAllomorphDef, next: &AffixAllomorphDef) -> bool {
    constraints_equal(g, cur, next)
}

// Affix process — synthesis.

/// Resolves `word`'s root allomorph to its stem name; `None` for a missing root, no stem name, or a guessed root (conservative — this synthesis-time gate does not consult the guess's pattern).
fn root_stem_name(g: &Grammar, word: &Word) -> Option<pg_grammar::model::StemNameId> {
    let root_id = word.root_allomorph?;
    if root_id == AllomorphId::GUESSED {
        return None;
    }
    let AllomorphOwner::Root(le, idx) = g.allomorph_owners[root_id.0 as usize] else {
        return None;
    };
    g.entries[le.0 as usize].allomorphs[idx as usize].stem_name
}

fn synth_affix(
    g: &Grammar,
    word: &Word,
    rule: &AffixProcessRuleDef,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    // Gate order matches C#: template prohibitions, then `RequiredStemName`, then the syn-FS unify; independent gates, so order only picks which `FailureReason` is reported first. Both template checks are guarded on `!is_template_rule`, since a template's own slot rules are never subject to them.

    // After a *final* template, prohibit a non-partial rule.
    if !rule.is_template_rule
        && matches!(word.flags.is_last_applied_rule_final, Some(true))
        && !word.flags.is_partial
        && !rule.partial
    {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    }
    // (b) After a *non-final* template, prohibit a partial rule unless the input is itself partial.
    if !rule.is_template_rule
        && matches!(word.flags.is_last_applied_rule_final, Some(false))
        && !word.flags.is_partial
        && rule.partial
    {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    }

    // `requiredStemName` is a reference-equality gate on the WORD's root stem name, not the rule's allomorphs'; `None` on both sides passes.
    if rule.required_stem_name.is_some() && rule.required_stem_name != root_stem_name(g, word) {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    }

    let Some(new_syn) = synth_syn_fs(g, rule.required_syn_fs, rule.out_syn_fs, word) else {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };

    // Resolved once per call against the rule's own owning stratum; the `TableId(0)` fallback fires only for a non-grammar-resident fixture.
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, true);
    let mut output = Vec::new();
    // Indices already applied in this loop, recorded on each output morph before the producing index is added.
    let mut applied: Vec<u16> = Vec::new();
    let mut reached: u32 = 0;
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        if !g.mpr_group_ok(allo.required_mpr, allo.excluded_mpr, word.mpr) {
            continue;
        }
        // Recompiled per call, deliberately: see `synthesize`'s doc.
        let Ok((fst, names)) = compile_parts(g, table, &allo.lhs, "p", true) else {
            continue;
        };
        let matched = synth_process_allomorph(
            g,
            table,
            word,
            rule.morpheme,
            &rule.obligatory_features,
            Some(rule.partial),
            true,
            allo,
            &segs,
            &node_of,
            &new_syn,
            &fst,
            &names,
            &applied,
        );
        let is_match = matched.is_some();
        record_mrule_reach(
            mstats,
            i as u32,
            segs.len() as u64,
            u64::from(is_match),
            &mut reached,
        );
        if let Some(w) = matched {
            output.push(w);
            applied.push(i as u16);
            // Disjunctive-allomorph break: stop after the first match unless this allomorph is environment/syn-constrained or free-fluctuates with the next one.
            let next_free_fluctuates = rule
                .allomorphs
                .get(i + 1)
                .is_some_and(|next| free_fluctuates_with(g, allo, next));
            if !next_free_fluctuates
                && allo.environments.is_empty()
                && g.fs_interner.get(allo.required_syn_fs).is_empty()
            {
                break;
            }
        }
    }
    record_mrule_invocation_end(mstats, reached, output.len() as u64, segs.len() as u64);
    output
}

/// `RuleCache`-aware sibling of `synth_affix`: every early return reports its own `FailureReason` at index `-1`, matching C#'s rule-level gates.
#[allow(clippy::too_many_arguments)]
fn synth_affix_cached(
    g: &Grammar,
    word: &Word,
    rule: &AffixProcessRuleDef,
    mrid: MRuleId,
    cache: &crate::cache::RuleCache,
    mstats: Option<MRuleStatsCtx>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    macro_rules! not_applied {
        ($reason:expr) => {{
            if trace.is_tracing() {
                trace.morphological_rule_not_applied(parent, mrid, -1, word, $reason);
            }
            record_mrule_none_residual(mstats, word.shape.len() as u64);
            return Vec::new();
        }};
    }
    // Gate order and `!is_template_rule` guards mirror `synth_affix`; order matters only because the first failing gate is the reported reason.

    // Final-template prohibition.
    if !rule.is_template_rule
        && matches!(word.flags.is_last_applied_rule_final, Some(true))
        && !word.flags.is_partial
        && !rule.partial
    {
        not_applied!(FailureReason::NonPartialRuleProhibitedAfterFinalTemplate);
    }
    // (b) Non-final-template prohibition.
    if !rule.is_template_rule
        && matches!(word.flags.is_last_applied_rule_final, Some(false))
        && !word.flags.is_partial
        && rule.partial
    {
        not_applied!(FailureReason::NonPartialRuleRequiredAfterNonFinalTemplate);
    }

    if rule.required_stem_name.is_some() && rule.required_stem_name != root_stem_name(g, word) {
        not_applied!(FailureReason::RequiredStemName);
    }

    let Some(new_syn) = synth_syn_fs(g, rule.required_syn_fs, rule.out_syn_fs, word) else {
        not_applied!(FailureReason::RequiredSyntacticFeatureStruct);
    };

    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, true);
    let mut output = Vec::new();
    let mut applied: Vec<u16> = Vec::new();
    let mut reached: u32 = 0;
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        if let Some(reason) = mpr_gate_reason(g, allo.required_mpr, allo.excluded_mpr, word.mpr) {
            if trace.is_tracing() {
                trace.morphological_rule_not_applied(parent, mrid, i as i32, word, reason);
            }
            continue;
        }
        let Some((fst, names)) = cache.allomorph(allo.id).synth_lhs.as_ref() else {
            continue;
        };
        let matched = synth_process_allomorph(
            g,
            table,
            word,
            rule.morpheme,
            &rule.obligatory_features,
            Some(rule.partial),
            true,
            allo,
            &segs,
            &node_of,
            &new_syn,
            fst,
            names,
            &applied,
        );
        record_mrule_reach(
            mstats,
            i as u32,
            segs.len() as u64,
            u64::from(matched.is_some()),
            &mut reached,
        );
        match matched {
            Some(mut w) => {
                if trace.is_tracing() {
                    w.trace = Some(trace.morphological_rule_applied(parent, mrid, i as i32, &w));
                }
                output.push(w);
                applied.push(i as u16);
                // Disjunctive-allomorph break — see `synth_affix`'s twin site.
                let next_free_fluctuates = rule
                    .allomorphs
                    .get(i + 1)
                    .is_some_and(|next| free_fluctuates_with(g, allo, next));
                if !next_free_fluctuates
                    && allo.environments.is_empty()
                    && g.fs_interner.get(allo.required_syn_fs).is_empty()
                {
                    break;
                }
            }
            None => {
                if trace.is_tracing() {
                    trace.morphological_rule_not_applied(
                        parent,
                        mrid,
                        i as i32,
                        word,
                        FailureReason::Pattern,
                    );
                }
            }
        }
    }
    record_mrule_invocation_end(mstats, reached, output.len() as u64, segs.len() as u64);
    output
}

// Realizational affix process — synthesis.

/// C# `IsBlocked`: blocked iff every feature key `real_fs` declares is also present in `syn_fs` (recursing into complex values); no cycle guard needed since this FS model is a tree, never a DAG.
fn realizational_is_blocked(real_fs: &FeatureStruct, syn_fs: &FeatureStruct) -> bool {
    for (feat, rval) in real_fs.entries() {
        let Some(sval) = syn_fs.get(*feat) else {
            return false;
        };
        if let (
            pg_featstruct::FeatureValue::Complex(rfs),
            pg_featstruct::FeatureValue::Complex(sfs),
        ) = (rval, sval)
        {
            if !realizational_is_blocked(rfs, sfs) {
                return false;
            }
        }
    }
    true
}

/// C# `Apply`: gates in order are `real_fs` subsumption, `IsBlocked` (when `real_fs` non-empty), then the required-syn-FS unify via `synth_syn_fs`; unlike `synth_affix` this class has no partial/final/obligatory/max-application gates.
fn synth_realizational(
    g: &Grammar,
    word: &Word,
    rule: &RealizationalRuleDef,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    let real_fs = g.fs_interner.get(rule.real_fs);
    if !pg_featstruct::subsumes(real_fs, &word.real_fs) {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    }
    if !real_fs.is_empty() && realizational_is_blocked(real_fs, &word.syn_fs) {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    }
    let Some(new_syn) = synth_syn_fs(g, rule.required_syn_fs, rule.real_fs, word) else {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };

    // Resolved once per call — see `synth_affix`'s twin site.
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, true);
    let mut output = Vec::new();
    let mut applied: Vec<u16> = Vec::new();
    let mut reached: u32 = 0;
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        if !g.mpr_group_ok(allo.required_mpr, allo.excluded_mpr, word.mpr) {
            continue;
        }
        let Ok((fst, names)) = compile_parts(g, table, &allo.lhs, "p", true) else {
            continue;
        };
        let matched = synth_process_allomorph(
            g,
            table,
            word,
            rule.morpheme,
            &[],
            None,
            false,
            allo,
            &segs,
            &node_of,
            &new_syn,
            &fst,
            &names,
            &applied,
        );
        record_mrule_reach(
            mstats,
            i as u32,
            segs.len() as u64,
            u64::from(matched.is_some()),
            &mut reached,
        );
        if let Some(w) = matched {
            output.push(w);
            applied.push(i as u16);
            let next_free_fluctuates = rule
                .allomorphs
                .get(i + 1)
                .is_some_and(|next| free_fluctuates_with(g, allo, next));
            if !next_free_fluctuates
                && allo.environments.is_empty()
                && g.fs_interner.get(allo.required_syn_fs).is_empty()
            {
                break;
            }
        }
    }
    record_mrule_invocation_end(mstats, reached, output.len() as u64, segs.len() as u64);
    output
}

/// `RuleCache`-aware sibling of `synth_realizational`: the first two gates stay untraced, since C# fires no trace event at either site.
#[allow(clippy::too_many_arguments)]
fn synth_realizational_cached(
    g: &Grammar,
    word: &Word,
    rule: &RealizationalRuleDef,
    mrid: MRuleId,
    cache: &crate::cache::RuleCache,
    mstats: Option<MRuleStatsCtx>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    let real_fs = g.fs_interner.get(rule.real_fs);
    if !pg_featstruct::subsumes(real_fs, &word.real_fs) {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    }
    if !real_fs.is_empty() && realizational_is_blocked(real_fs, &word.syn_fs) {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    }
    let Some(new_syn) = synth_syn_fs(g, rule.required_syn_fs, rule.real_fs, word) else {
        if trace.is_tracing() {
            trace.morphological_rule_not_applied(
                parent,
                mrid,
                -1,
                word,
                FailureReason::RequiredSyntacticFeatureStruct,
            );
        }
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };

    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, true);
    let mut output = Vec::new();
    let mut applied: Vec<u16> = Vec::new();
    let mut reached: u32 = 0;
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        if let Some(reason) = mpr_gate_reason(g, allo.required_mpr, allo.excluded_mpr, word.mpr) {
            if trace.is_tracing() {
                trace.morphological_rule_not_applied(parent, mrid, i as i32, word, reason);
            }
            continue;
        }
        let Some((fst, names)) = cache.allomorph(allo.id).synth_lhs.as_ref() else {
            continue;
        };
        let matched = synth_process_allomorph(
            g,
            table,
            word,
            rule.morpheme,
            &[],
            None,
            false,
            allo,
            &segs,
            &node_of,
            &new_syn,
            fst,
            names,
            &applied,
        );
        record_mrule_reach(
            mstats,
            i as u32,
            segs.len() as u64,
            u64::from(matched.is_some()),
            &mut reached,
        );
        match matched {
            Some(mut w) => {
                if trace.is_tracing() {
                    w.trace = Some(trace.morphological_rule_applied(parent, mrid, i as i32, &w));
                }
                output.push(w);
                applied.push(i as u16);
                let next_free_fluctuates = rule
                    .allomorphs
                    .get(i + 1)
                    .is_some_and(|next| free_fluctuates_with(g, allo, next));
                if !next_free_fluctuates
                    && allo.environments.is_empty()
                    && g.fs_interner.get(allo.required_syn_fs).is_empty()
                {
                    break;
                }
            }
            None => {
                if trace.is_tracing() {
                    trace.morphological_rule_not_applied(
                        parent,
                        mrid,
                        i as i32,
                        word,
                        FailureReason::Pattern,
                    );
                }
            }
        }
    }
    record_mrule_invocation_end(mstats, reached, output.len() as u64, segs.len() as u64);
    output
}

/// The `PartRef::Input` index an RHS action references (copy/modify only); equivalent to C#'s grouping by part name since `Input(i)` corresponds to `lhs[i].Name`.
fn redup_part_ref(action: &OutputAction) -> Option<u16> {
    match action {
        OutputAction::Copy(PartRef::Input(i)) | OutputAction::Modify(PartRef::Input(i), _) => {
            Some(*i)
        }
        _ => None,
    }
}

/// For every RHS index inside a reduplication group (an `Input` part referenced 2+ times), reports whether that occurrence is the existing echo or new affix material; indices outside any group are absent, keeping default attribution.
fn classify_redup(
    lhs_len: u16,
    rhs: &[OutputAction],
    hint: ReduplicationHint,
) -> HashMap<usize, bool> {
    // Group RHS indices by referenced `Input` part.
    let mut groups: HashMap<u16, Vec<usize>> = HashMap::default();
    for (i, action) in rhs.iter().enumerate() {
        if let Some(p) = redup_part_ref(action) {
            groups.entry(p).or_default().push(i);
        }
    }
    let mut redup_parts: Vec<&Vec<usize>> = groups.values().filter(|v| v.len() > 1).collect();
    if redup_parts.is_empty() {
        return HashMap::default();
    }
    // Deterministic order below only for readability; each group is classified independently, so order cannot change the result.
    redup_parts.sort_by_key(|v| v[0]);

    // `start`: the RHS index where a `lhs_len`-long run echoes every LHS part once in order (the non-reduplicating whole-input repeat); widened to `i64` so C#'s subtractions cannot underflow.
    let mut start: Option<i64> = None;
    match hint {
        ReduplicationHint::Prefix => {
            let mut prefix_part_index: i64 = lhs_len as i64 - 1;
            for i in (0..rhs.len()).rev() {
                let pr = redup_part_ref(&rhs[i]).map(i64::from);
                if pr == Some(prefix_part_index) || pr == Some(lhs_len as i64 - 1) {
                    if pr == Some(0) {
                        start = Some(i as i64);
                        break;
                    }
                    if pr != Some(prefix_part_index) {
                        prefix_part_index = lhs_len as i64 - 1;
                    }
                    prefix_part_index -= 1;
                } else {
                    prefix_part_index = lhs_len as i64 - 1;
                }
            }
        }
        ReduplicationHint::Suffix | ReduplicationHint::Implicit => {
            // Suffix and Implicit share one branch in C# too.
            let mut suffix_part_index: i64 = 0;
            for (i, action) in rhs.iter().enumerate() {
                let pr = redup_part_ref(action).map(i64::from);
                if pr == Some(suffix_part_index) || pr == Some(0) {
                    if pr == Some(lhs_len as i64 - 1) {
                        start = Some(i as i64 - (lhs_len as i64 - 1));
                        break;
                    }
                    if pr != Some(suffix_part_index) {
                        suffix_part_index = 0;
                    }
                    suffix_part_index += 1;
                } else {
                    suffix_part_index = 0;
                }
            }
        }
    }

    // Classify each occurrence of each redup group.
    let mut existing: HashMap<usize, bool> = HashMap::default();
    for part_actions in &redup_parts {
        for (j, &rhs_idx) in part_actions.iter().enumerate() {
            let is_existing = match start {
                None => {
                    j == if hint == ReduplicationHint::Prefix {
                        part_actions.len() - 1
                    } else {
                        0
                    }
                }
                Some(s) => {
                    let idx = rhs_idx as i64;
                    idx >= s && idx < s + lhs_len as i64
                }
            };
            existing.insert(rhs_idx, is_existing);
        }
    }
    existing
}

/// One allomorph's synthesis (LHS match + RHS build), shared by affix-process and realizational paths; the realizational caller passes `obligatory: &[]`, `partial: None` (leave flags as cloned, not "non-partial"), `apply_out_mpr: false`.
#[allow(clippy::too_many_arguments)]
fn synth_process_allomorph(
    g: &Grammar,
    table: TableId,
    word: &Word,
    morpheme: MorphemeId,
    obligatory: &[pg_featstruct::FeatId],
    partial: Option<bool>,
    apply_out_mpr: bool,
    allo: &AffixAllomorphDef,
    segs: &[Segment],
    node_of: &[usize],
    new_syn: &FeatureStruct,
    fst: &Fst,
    names: &[String],
    passed: &[u16],
) -> Option<Word> {
    let result = Transduce::new(fst, segs.to_vec())
        .anchored(true, true)
        .first_match()?;
    let ranges = part_ranges(fst, names, &result);

    // Empty unless the RHS actually repeats an `Input` part, so a non-reduplicating allomorph pays only for an empty map.
    let redup = classify_redup(allo.lhs.len() as u16, &allo.rhs, allo.redup_hint);

    let mut out: Vec<OutNode> = Vec::new();
    for (rhs_idx, action) in allo.rhs.iter().enumerate() {
        match action {
            OutputAction::Copy(PartRef::Input(i)) => {
                let src = PartSource {
                    node_of,
                    shape: &word.shape,
                    range: ranges[*i as usize],
                    head: true,
                };
                copy_part(g, table, &mut out, &src, None, redup.get(&rhs_idx).copied());
            }
            OutputAction::Modify(PartRef::Input(i), ctx) => {
                let src = PartSource {
                    node_of,
                    shape: &word.shape,
                    range: ranges[*i as usize],
                    head: true,
                };
                copy_part(
                    g,
                    table,
                    &mut out,
                    &src,
                    Some(ctx),
                    redup.get(&rhs_idx).copied(),
                );
            }
            OutputAction::InsertSegments { shape, .. } => {
                insert_segments(g, table, &mut out, &shape.shape, Origin::Affix);
            }
            OutputAction::InsertContext(ctx) => {
                out.push(OutNode {
                    kind: NodeKind::Segment,
                    char_def: NO_CHAR_DEF,
                    lanes: ctx_lanes(g, table, ctx),
                    optional: false,
                    origin: Origin::Affix,
                    cd_set: ctx_cd_set(g, table, ctx),
                });
            }
            // Cross-list part refs never appear on an affix rule (loader invariant).
            OutputAction::Copy(_) | OutputAction::Modify(_, _) => {}
        }
    }

    let morphs = attribute_morphs(&out, word, None, Some((allo.id, morpheme, passed)));
    let mut w = word.clone();
    w.shape = freeze_out(g, &out);
    w.syn_fs = new_syn.clone();
    if apply_out_mpr {
        w.mpr = g.mpr_add_output(word.mpr, allo.out_mpr);
    }
    w.morphs = morphs;
    w.obligatory.extend_from_slice(obligatory);
    if let Some(is_partial_rule) = partial {
        if !is_partial_rule {
            w.flags.is_last_applied_rule_final = None;
        } else {
            w.flags.is_partial = true;
        }
    }
    Some(w)
}

// Affix process — analysis.

/// The analysis LHS built from RHS actions, plus capture bookkeeping. `pub(crate)` so
/// `crate::cache::RuleCache` can store the compiled `(Fst, AnalysisLhs)` pair per allomorph/subrule.
pub(crate) struct AnalysisLhs {
    nodes: Vec<CompileNode>,
    /// part name → number of capture groups generated for it.
    captured: HashMap<String, usize>,
    /// part name -> (capture-group index, ctx) for a `ModifyFromInput`, underspecified on `GenerateShape`.
    modify: HashMap<String, (usize, SimpleContext)>,
}

/// Strips boundary char-defs from a pattern (C# `DeepCloneExceptBoundaries`); a quantifier whose children all vanish is dropped too.
fn strip_boundaries(g: &Grammar, table: TableId, part: &Pattern) -> Pattern {
    fn is_boundary(g: &Grammar, table: TableId, cd: CharDefId) -> bool {
        let t = &g.char_tables[table.0 as usize];
        (cd.0 as usize) < t.len() && t.get(cd).kind() == pg_grammar::chardef::CharDefKind::Boundary
    }
    fn strip(g: &Grammar, table: TableId, nodes: &[PatternNode]) -> Vec<PatternNode> {
        let mut out = Vec::new();
        for n in nodes {
            match n {
                PatternNode::CharDef(cd) if is_boundary(g, table, *cd) => {}
                PatternNode::Quantifier { min, max, children } => {
                    let kids = strip(g, table, children);
                    if !kids.is_empty() {
                        out.push(PatternNode::Quantifier {
                            min: *min,
                            max: *max,
                            children: kids,
                        });
                    }
                }
                other => out.push(other.clone()),
            }
        }
        out
    }
    Pattern {
        nodes: strip(g, table, &part.nodes),
    }
}

/// Applies ctx pins recursively to every `Constraint` node, so the analysis-side `ModifyFromInput` matches the modified surface.
fn apply_ctx_to_nodes(nodes: &mut [CompileNode], pins: &[(usize, u64)]) {
    for n in nodes {
        match n {
            CompileNode::Constraint(lanes) => {
                for &(f, bits) in pins {
                    if f < lanes.len() {
                        lanes[f] = bits;
                    }
                }
            }
            CompileNode::Group { children, .. } => apply_ctx_to_nodes(children, pins),
            CompileNode::Quantifier { children, .. } => apply_ctx_to_nodes(children, pins),
            CompileNode::Alternation(alts) => {
                for alt in alts {
                    apply_ctx_to_nodes(alt, pins);
                }
            }
        }
    }
}

fn build_analysis_lhs(
    g: &Grammar,
    table: TableId,
    lhs_parts: &[(String, &Pattern)],
    rhs: &[OutputAction],
) -> Result<AnalysisLhs, BridgeError> {
    // Same `StrRep` identity lane as `compile_parts`; inputs come from `segs_of` either way.
    let bridge = PatternBridge::new(g)
        .with_table(table)
        .deterministic(false)
        .id_lane(true);
    let id_width = crate::bridge::id_lane_width(g, table);
    let lookup: HashMap<&str, &Pattern> = lhs_parts.iter().map(|(n, p)| (n.as_str(), *p)).collect();
    let mut lhs = AnalysisLhs {
        nodes: Vec::new(),
        captured: HashMap::default(),
        modify: HashMap::default(),
    };
    for action in rhs {
        match action {
            OutputAction::Copy(pr) => {
                let name = part_name(pr);
                let part = strip_boundaries(g, table, lookup[name.as_str()]);
                let children = bridge.compile_pattern(&part)?.input.nodes;
                let count = *lhs.captured.get(&name).unwrap_or(&0);
                lhs.nodes.push(CompileNode::Group {
                    name: group_name(&name, count),
                    children,
                });
                lhs.captured.insert(name, count + 1);
            }
            OutputAction::Modify(pr, ctx) => {
                let name = part_name(pr);
                let part = strip_boundaries(g, table, lookup[name.as_str()]);
                let mut children = bridge.compile_pattern(&part)?.input.nodes;
                let pins: Vec<(usize, u64)> = ctx_pins(g, table, ctx)
                    .into_iter()
                    .map(|(f, b)| (f, to_fst_lane(g, f, b)))
                    .collect();
                apply_ctx_to_nodes(&mut children, &pins);
                let count = *lhs.captured.get(&name).unwrap_or(&0);
                lhs.nodes.push(CompileNode::Group {
                    name: group_name(&name, count),
                    children,
                });
                lhs.modify.insert(name.clone(), (count, ctx.clone()));
                lhs.captured.insert(name, count + 1);
            }
            OutputAction::InsertSegments { shape, .. } => {
                for (_, kind, char_def, _) in shape.shape.interior() {
                    if kind == NodeKind::Segment {
                        let mut lanes = to_fst(g, &cd_lanes(g, table, char_def));
                        // The analysis-side consumer must find and consume this exact inserted segment, matching the full char-def FS rather than any unifiable one, as C# does.
                        if let (Some(w), true) = (id_width, char_def != NO_CHAR_DEF) {
                            crate::bridge::push_id_lane(&mut lanes, w, 1u64 << char_def);
                        }
                        lhs.nodes.push(CompileNode::Constraint(lanes));
                    }
                }
            }
            OutputAction::InsertContext(ctx) => {
                // Known residual: this is an FST match constraint, not an output node, so there is no shape for a `cd_set`; the id lane below closes it for narrow tables, but wider tables still over-match on lane-union alone.
                let mut lanes = to_fst(g, &ctx_lanes(g, table, ctx));
                if let Some(w) = id_width {
                    if let NaturalClassKind::Segments(segs) =
                        &g.natural_classes[ctx.nat_class.0 as usize].kind
                    {
                        let bits = segs.iter().fold(0u64, |acc, cd| acc | (1u64 << cd.0));
                        crate::bridge::push_id_lane(&mut lanes, w, bits);
                    }
                }
                lhs.nodes.push(CompileNode::Constraint(lanes));
            }
        }
    }
    Ok(lhs)
}

fn to_fst_lane(g: &Grammar, f: usize, bits: u64) -> u64 {
    if bits == full_mask(g, f) {
        u64::MAX
    } else {
        bits
    }
}

fn part_name(pr: &PartRef) -> String {
    match pr {
        PartRef::Input(i) => format!("p{i}"),
        PartRef::Head(i) => format!("h{i}"),
        PartRef::NonHead(i) => format!("n{i}"),
    }
}

fn group_name(part: &str, idx: usize) -> String {
    format!("{part}_{idx}")
}

/// `GenerateShape`: re-emits captured original LHS parts as output nodes, dropping inserted material; modify parts get their changed features underspecified.
#[allow(clippy::too_many_arguments)]
fn generate_shape(
    g: &Grammar,
    table: TableId,
    lhs_parts: &[(String, &Pattern)],
    lhs: &AnalysisLhs,
    fst: &Fst,
    result: &FstResult,
    node_of: &[usize],
    shape: &Shape,
) -> Vec<OutNode> {
    let mut out = Vec::new();
    for (name, part) in lhs_parts {
        let Some(&count) = lhs.captured.get(name) else {
            // Not captured: untruncate the part, materializing its segment constraints as optional beyond a quantifier's min.
            untruncate(g, table, &mut out, part);
            continue;
        };
        // ModifyFromInput: the underspecify set = the features the ctx pinned.
        let modify_pins: Option<Vec<usize>> = lhs.modify.get(name).map(|(_, ctx)| {
            ctx_pins(g, table, ctx)
                .into_iter()
                .map(|(f, _)| f)
                .collect()
        });
        let mut emitted = false;
        for idx in 0..count {
            if let Some((s, e)) = fst.get_offsets(&group_name(name, idx), &result.registers) {
                for &p in &node_of[s as usize..e as usize] {
                    let mut lanes = fit(g, shape.node_lanes(p));
                    let mut char_def = shape.char_def(p);
                    if shape.kind(p) == NodeKind::Segment {
                        if let Some(feats) = &modify_pins {
                            for &f in feats {
                                lanes[f] = full_mask(g, f); // underspecify (undo the change)
                            }
                            // The analysis-side counterpart of `copy_part`'s modify handling: clearing char_def makes lookup fall back to lane unification, since a pre-modification lexical root can't be found while this node still claims to be the post-modification one.
                            char_def = NO_CHAR_DEF;
                        }
                    }
                    out.push(OutNode {
                        kind: shape.kind(p),
                        char_def,
                        lanes,
                        optional: shape.flags(p).is_optional(),
                        origin: Origin::Head(p - 1),
                        cd_set: cd_set_of(shape, p),
                    });
                }
                emitted = true;
                break;
            }
        }
        if !emitted {
            untruncate(g, table, &mut out, part);
        }
    }
    out
}

/// Materializes a part's constraints as output nodes (C# `Untruncate`); an **unbounded** quantifier emits NOTHING (infinity is encoded as -1), never a fabricated `max(min, 1)` phantom wildcard.
fn untruncate(g: &Grammar, table: TableId, out: &mut Vec<OutNode>, part: &Pattern) {
    fn emit(
        g: &Grammar,
        table: TableId,
        out: &mut Vec<OutNode>,
        nodes: &[PatternNode],
        optional: bool,
    ) {
        for n in nodes {
            match n {
                PatternNode::Context(sc) => out.push(OutNode {
                    kind: NodeKind::Segment,
                    char_def: NO_CHAR_DEF,
                    lanes: ctx_lanes(g, table, sc),
                    optional,
                    origin: Origin::Affix,
                    cd_set: ctx_cd_set(g, table, sc),
                }),
                PatternNode::CharDef(cd) => {
                    let t = &g.char_tables[table.0 as usize];
                    if (cd.0 as usize) < t.len()
                        && t.get(*cd).kind() == pg_grammar::chardef::CharDefKind::Segment
                    {
                        out.push(OutNode {
                            kind: NodeKind::Segment,
                            char_def: cd.0,
                            lanes: cd_lanes(g, table, cd.0),
                            optional,
                            origin: Origin::Affix,
                            cd_set: CdSet::Unrestricted,
                        });
                    }
                }
                PatternNode::Quantifier { min, max, children } => {
                    // An unbounded quantifier emits nothing — see this function's doc.
                    if let Some(max) = max {
                        for r in 0..*max {
                            emit(g, table, out, children, optional || r >= *min);
                        }
                    }
                }
                PatternNode::Segments { .. } | PatternNode::Anchor(_) => {}
            }
        }
    }
    emit(g, table, out, &part.nodes, false);
}

/// Build the analysis LHS and its compiled FST for one affix allomorph — C#'s
/// `AnalysisMorphologicalTransform` applied to `allo.rhs`. A pure function of grammar-static data,
/// so `crate::cache::RuleCache::build` calls it once per allomorph; the uncached `ana_affix` calls
/// it per application for standalone fixtures that have no grammar-resident index.
pub(crate) fn build_ana_affix_lhs(
    g: &Grammar,
    table: TableId,
    allo: &AffixAllomorphDef,
) -> Result<(Fst, AnalysisLhs), BridgeError> {
    let parts: Vec<(String, &Pattern)> = allo
        .lhs
        .iter()
        .enumerate()
        .map(|(i, p)| (format!("p{i}"), p))
        .collect();
    let lhs = build_analysis_lhs(g, table, &parts, &allo.rhs)?;
    let fst = CompileInput::new(lhs.nodes.clone())
        .deterministic(false)
        .compile_with_direction(Direction::LeftToRight);
    Ok((fst, lhs))
}

fn ana_affix(
    g: &Grammar,
    word: &Word,
    rule: &AffixProcessRuleDef,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    let Some(new_syn) = ana_syn_fs(g, rule.required_syn_fs, rule.out_syn_fs, word) else {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };
    // Resolved once per call — see `synth_affix`'s twin site.
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    let mut output = Vec::new();
    let mut reached: u32 = 0;
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        let Ok((fst, lhs)) = build_ana_affix_lhs(g, table, allo) else {
            continue;
        };
        let before = output.len();
        output.extend(ana_affix_allomorph(
            g, table, word, allo, &lhs, &fst, &segs, &node_of, &new_syn, mstats,
        ));
        let n = (output.len() - before) as u64;
        record_mrule_reach(mstats, i as u32, segs.len() as u64, n, &mut reached);
    }
    record_mrule_invocation_end(mstats, reached, output.len() as u64, segs.len() as u64);
    output
}

/// `crate::cache::RuleCache`-aware sibling of `ana_affix`, also driving the `AnalysisPhase` breakdown.
fn ana_affix_cached(
    g: &Grammar,
    word: &Word,
    rule: &AffixProcessRuleDef,
    cache: &crate::cache::RuleCache,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    let _synfs_phase = mstats.map(|m| m.stats.phase_enter(AnalysisPhase::AnaSynFs, 1));
    let Some(new_syn) = ana_syn_fs(g, rule.required_syn_fs, rule.out_syn_fs, word) else {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };
    drop(_synfs_phase);
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let _segs_phase = mstats.map(|m| m.stats.phase_enter(AnalysisPhase::SegsOf, 1));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    drop(_segs_phase);
    let mut output = Vec::new();
    let mut reached: u32 = 0;
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        let Some((fst, lhs)) = cache.allomorph(allo.id).ana_lhs.as_ref() else {
            continue;
        };
        let before = output.len();
        let _allo_phase = mstats.map(|m| m.stats.phase_enter(AnalysisPhase::AnaAffixAllomorph, 1));
        output.extend(ana_affix_allomorph(
            g, table, word, allo, lhs, fst, &segs, &node_of, &new_syn, mstats,
        ));
        drop(_allo_phase);
        let n = (output.len() - before) as u64;
        record_mrule_reach(mstats, i as u32, segs.len() as u64, n, &mut reached);
    }
    record_mrule_invocation_end(mstats, reached, output.len() as u64, segs.len() as u64);
    output
}

/// One allomorph's analysis-side match, `GenerateShape`, and dedup; `carry` writes whichever feature structure the rule kind propagates, and the dedup scope resets per allomorph, never shared across the rule. `mstats` times the FST search itself (`all_matches`, `AnalysisPhase::FstTraversal`) apart from the `generate_shape`/dedup remainder, which is charged to whichever phase the caller already entered.
#[allow(clippy::too_many_arguments)]
fn ana_allomorph_matches(
    g: &Grammar,
    table: TableId,
    word: &Word,
    allo: &AffixAllomorphDef,
    lhs: &AnalysisLhs,
    fst: &Fst,
    segs: &[Segment],
    node_of: &[usize],
    mstats: Option<MRuleStatsCtx>,
    carry: impl Fn(&mut Word),
) -> Vec<Word> {
    let parts: Vec<(String, &Pattern)> = allo
        .lhs
        .iter()
        .enumerate()
        .map(|(i, p)| (format!("p{i}"), p))
        .collect();
    let mut allo_out: Vec<Word> = Vec::new();
    let matches = {
        let _fst_phase = mstats.map(|m| m.stats.phase_enter(AnalysisPhase::FstTraversal, 1));
        Transduce::new(fst, segs.to_vec())
            .anchored(true, true)
            .all_matches()
    };
    for result in matches {
        let out = generate_shape(g, table, &parts, lhs, fst, &result, node_of, &word.shape);
        let mut w = word.clone();
        w.shape = freeze_out(g, &out);
        carry(&mut w);
        push_remove_duplicates(&mut allo_out, w);
    }
    allo_out
}

#[allow(clippy::too_many_arguments)]
fn ana_affix_allomorph(
    g: &Grammar,
    table: TableId,
    word: &Word,
    allo: &AffixAllomorphDef,
    lhs: &AnalysisLhs,
    fst: &Fst,
    segs: &[Segment],
    node_of: &[usize],
    new_syn: &FeatureStruct,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    ana_allomorph_matches(g, table, word, allo, lhs, fst, segs, node_of, mstats, |w| {
        w.syn_fs = new_syn.clone()
    })
}

// Realizational affix process — analysis.

/// C# `Apply`: one rule-level gate (the realizational-FS unify), after which every allomorph's matches carry that same unified value; unlike `ana_affix`, no max-application or syn-FS gate exists here.
fn ana_realizational(
    g: &Grammar,
    word: &Word,
    rule: &RealizationalRuleDef,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    let Some(real_fs) = unify(g.fs_interner.get(rule.real_fs), &word.real_fs) else {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };
    // Resolved once per call — see `synth_affix`'s twin site.
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    let mut output = Vec::new();
    let mut reached: u32 = 0;
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        let Ok((fst, lhs)) = build_ana_affix_lhs(g, table, allo) else {
            continue;
        };
        let before = output.len();
        output.extend(ana_realizational_allomorph(
            g, table, word, allo, &lhs, &fst, &segs, &node_of, &real_fs, mstats,
        ));
        let n = (output.len() - before) as u64;
        record_mrule_reach(mstats, i as u32, segs.len() as u64, n, &mut reached);
    }
    record_mrule_invocation_end(mstats, reached, output.len() as u64, segs.len() as u64);
    output
}

/// `crate::cache::RuleCache`-aware sibling of `ana_realizational`, also driving the `AnalysisPhase` breakdown.
fn ana_realizational_cached(
    g: &Grammar,
    word: &Word,
    rule: &RealizationalRuleDef,
    cache: &crate::cache::RuleCache,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    let _synfs_phase = mstats.map(|m| m.stats.phase_enter(AnalysisPhase::AnaSynFs, 1));
    let Some(real_fs) = unify(g.fs_interner.get(rule.real_fs), &word.real_fs) else {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };
    drop(_synfs_phase);
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let _segs_phase = mstats.map(|m| m.stats.phase_enter(AnalysisPhase::SegsOf, 1));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    drop(_segs_phase);
    let mut output = Vec::new();
    let mut reached: u32 = 0;
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        let Some((fst, lhs)) = cache.allomorph(allo.id).ana_lhs.as_ref() else {
            continue;
        };
        let before = output.len();
        let _allo_phase = mstats.map(|m| m.stats.phase_enter(AnalysisPhase::AnaRealizational, 1));
        output.extend(ana_realizational_allomorph(
            g, table, word, allo, lhs, fst, &segs, &node_of, &real_fs, mstats,
        ));
        drop(_allo_phase);
        let n = (output.len() - before) as u64;
        record_mrule_reach(mstats, i as u32, segs.len() as u64, n, &mut reached);
    }
    record_mrule_invocation_end(mstats, reached, output.len() as u64, segs.len() as u64);
    output
}

/// Unlike `ana_affix_allomorph`, the syntactic FS is left untouched: C#'s realizational analysis never assigns it, only the realizational FS.
#[allow(clippy::too_many_arguments)]
fn ana_realizational_allomorph(
    g: &Grammar,
    table: TableId,
    word: &Word,
    allo: &AffixAllomorphDef,
    lhs: &AnalysisLhs,
    fst: &Fst,
    segs: &[Segment],
    node_of: &[usize],
    real_fs: &FeatureStruct,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    ana_allomorph_matches(g, table, word, allo, lhs, fst, segs, node_of, mstats, |w| {
        w.real_fs = real_fs.clone()
    })
}

/// C# `RemoveDuplicates`: inserts `w` unless `out` holds a candidate with an identical **non-Optional** sequence, keeping the longer shape — not cosmetic, since Optional-segment proliferation is otherwise a combinatorial blow-up nothing downstream ever unifies away.
fn push_remove_duplicates(out: &mut Vec<Word>, w: Word) {
    // `web_time::Instant`, since std's `Instant` panics on wasm32-unknown-unknown, which this crate is built for.
    let start = web_time::Instant::now();
    let out_len = out.len();
    push_keep_longer(out, w, |a, b| shape_duplicates(&a.shape, &b.shape));
    dedup_profile::record(start.elapsed().as_nanos(), out_len);
}

/// The shared body of the three dedup passes: replace the first `dup`-matching candidate only when `w`'s shape is strictly longer, else append.
fn push_keep_longer(out: &mut Vec<Word>, w: Word, dup: impl Fn(&Word, &Word) -> bool) {
    if let Some(existing) = out.iter_mut().find(|o| dup(&w, o)) {
        if w.shape.len() > existing.shape.len() {
            *existing = w;
        }
        return;
    }
    out.push(w);
}

// Measures `push_remove_duplicates`'s own cost separately from pg-fst's `Transduce` dedup, since both are linear scans that can explode on Optional-flooded shapes and only a split measurement says which dominates.
pub mod dedup_profile {
    use std::cell::Cell;

    thread_local! {
        static CALLS: Cell<u64> = const { Cell::new(0) };
        static NANOS: Cell<u128> = const { Cell::new(0) };
        static MAX_OUT_LEN: Cell<usize> = const { Cell::new(0) };
        static TOTAL_OUT_LEN: Cell<u64> = const { Cell::new(0) };
    }

    pub(super) fn record(elapsed_nanos: u128, out_len_before: usize) {
        CALLS.with(|c| c.set(c.get() + 1));
        NANOS.with(|c| c.set(c.get() + elapsed_nanos));
        MAX_OUT_LEN.with(|c| c.set(c.get().max(out_len_before)));
        TOTAL_OUT_LEN.with(|c| c.set(c.get() + out_len_before as u64));
    }

    /// (calls, total_ns, max_out_len_seen, total_out_len_seen) -- snapshot only, never reset.
    pub fn snapshot() -> (u64, u128, usize, u64) {
        (
            CALLS.with(|c| c.get()),
            NANOS.with(|c| c.get()),
            MAX_OUT_LEN.with(|c| c.get()),
            TOTAL_OUT_LEN.with(|c| c.get()),
        )
    }
}

/// C# `Duplicates`: two shapes duplicate each other iff their **non-Optional** nodes carry an identical feature structure (lanes AND `StrRep`, load-bearing since it is a boundary's only identity); this port is deliberately finer than C# here, so err toward finer, never coarser, if you touch it.
fn shape_duplicates(a: &Shape, b: &Shape) -> bool {
    let idx = |s: &Shape| -> Vec<usize> {
        (0..s.len())
            .filter(|&i| !s.flags(i).is_optional())
            .collect()
    };
    let ia = idx(a);
    let ib = idx(b);
    ia.len() == ib.len()
        && ia.iter().zip(&ib).all(|(&x, &y)| {
            a.node_lanes(x) == b.node_lanes(y)
                && effective_cd_sets_eq(a.node_cd_set(x), b.node_cd_set(y))
        })
}

/// Set-equality over `EffectiveCdSet` — `shape_duplicates`'s `StrRep` dimension; `Unrestricted` equals only `Unrestricted`, since a node with `StrRep` and one without are different feature structures.
fn effective_cd_sets_eq(a: EffectiveCdSet, b: EffectiveCdSet) -> bool {
    match (a, b) {
        (EffectiveCdSet::Singleton(x), EffectiveCdSet::Singleton(y)) => x == y,
        (EffectiveCdSet::Unrestricted, EffectiveCdSet::Unrestricted) => true,
        (EffectiveCdSet::Members(x), EffectiveCdSet::Members(y)) => x == y,
        (EffectiveCdSet::Singleton(x), EffectiveCdSet::Members(m))
        | (EffectiveCdSet::Members(m), EffectiveCdSet::Singleton(x)) => {
            m.count() == 1 && m.contains(x)
        }
        _ => false,
    }
}

// Compounding — synthesis.

fn synth_compound(
    g: &Grammar,
    word: &Word,
    rule: &CompoundingRuleDef,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    let Some(nh) = word.current_non_head().cloned() else {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };
    // Gating.
    if !is_unifiable(g.fs_interner.get(rule.non_head_required_syn_fs), &nh.syn_fs) {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    }
    let Some(new_syn) = synth_syn_fs(g, rule.head_required_syn_fs, rule.out_syn_fs, word) else {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };
    if matches!(word.flags.is_last_applied_rule_final, Some(true)) && !word.flags.is_partial {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    }
    if !rule.head_prod_restrictions_mpr.compound_match(word.mpr) {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    }

    // A compounding rule has no `MorphemeId` and this uncached entry point has no `MRuleId` in scope, so the table is resolved by the rule's own xml id.
    let table = crate::cache::owning_table_for_compounding_rule(g, rule).unwrap_or(TableId(0));
    let (head_segs, head_node_of) = segs_of(g, table, &word.shape, true);
    let (nh_segs, nh_node_of) = segs_of(g, table, &nh.shape, true);
    let mut output = Vec::new();
    let mut reached: u32 = 0;
    for (i, sr) in rule.subrules.iter().enumerate() {
        if !g.mpr_group_ok(sr.required_mpr, sr.excluded_mpr, word.mpr) {
            continue;
        }
        // Recompiled per call for the same standalone-fixture reason as `synth_affix`.
        let Ok((head_fst, head_names)) = compile_parts(g, table, &sr.head_lhs, "h", true) else {
            continue;
        };
        let Ok((nh_fst, nh_names)) = compile_parts(g, table, &sr.non_head_lhs, "n", true) else {
            continue;
        };
        let matched = synth_compound_subrule(
            g,
            table,
            word,
            &nh,
            rule,
            sr,
            &head_segs,
            &head_node_of,
            &nh_segs,
            &nh_node_of,
            &new_syn,
            &head_fst,
            &head_names,
            &nh_fst,
            &nh_names,
        );
        record_mrule_reach(
            mstats,
            i as u32,
            (head_segs.len() + nh_segs.len()) as u64,
            u64::from(matched.is_ok()),
            &mut reached,
        );
        if let Ok(w) = matched {
            output.push(w);
            break; // C# breaks after the first matching subrule
        }
    }
    record_mrule_invocation_end(
        mstats,
        reached,
        output.len() as u64,
        (head_segs.len() + nh_segs.len()) as u64,
    );
    output
}

/// `RuleCache`-aware sibling of `synth_compound`: gate order differs harmlessly from C#'s (independent boolean gates), and only `HeadProdRestrictMprFeatures` routes through `compounding_rule_not_applied`, matching C#'s own one-site split.
#[allow(clippy::too_many_arguments)]
fn synth_compound_cached(
    g: &Grammar,
    word: &Word,
    rule: &CompoundingRuleDef,
    mrid: MRuleId,
    cache: &crate::cache::RuleCache,
    mstats: Option<MRuleStatsCtx>,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    let Some(nh) = word.current_non_head().cloned() else {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };
    if !is_unifiable(g.fs_interner.get(rule.non_head_required_syn_fs), &nh.syn_fs) {
        if trace.is_tracing() {
            trace.morphological_rule_not_applied(
                parent,
                mrid,
                -1,
                word,
                FailureReason::NonHeadRequiredSyntacticFeatureStruct,
            );
        }
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    }
    let Some(new_syn) = synth_syn_fs(g, rule.head_required_syn_fs, rule.out_syn_fs, word) else {
        if trace.is_tracing() {
            trace.morphological_rule_not_applied(
                parent,
                mrid,
                -1,
                word,
                FailureReason::HeadRequiredSyntacticFeatureStruct,
            );
        }
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };
    if matches!(word.flags.is_last_applied_rule_final, Some(true)) && !word.flags.is_partial {
        if trace.is_tracing() {
            trace.morphological_rule_not_applied(
                parent,
                mrid,
                -1,
                word,
                FailureReason::NonPartialRuleProhibitedAfterFinalTemplate,
            );
        }
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    }
    if !rule.head_prod_restrictions_mpr.compound_match(word.mpr) {
        if trace.is_tracing() {
            trace.compounding_rule_not_applied(
                parent,
                mrid,
                word,
                FailureReason::HeadProdRestrictMprFeatures,
            );
        }
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    }

    let cc = cache.compound(mrid);
    let table = crate::cache::owning_table_for_mrule(g, mrid).unwrap_or(TableId(0));
    let (head_segs, head_node_of) = segs_of(g, table, &word.shape, true);
    let (nh_segs, nh_node_of) = segs_of(g, table, &nh.shape, true);
    let mut output = Vec::new();
    let mut reached: u32 = 0;
    for (i, sr) in rule.subrules.iter().enumerate() {
        if let Some(reason) = mpr_gate_reason(g, sr.required_mpr, sr.excluded_mpr, word.mpr) {
            if trace.is_tracing() {
                trace.morphological_rule_not_applied(parent, mrid, i as i32, word, reason);
            }
            continue;
        }
        let src = &cc.subrules[i];
        let (Some((head_fst, head_names)), Some((nh_fst, nh_names))) =
            (src.synth_head.as_ref(), src.synth_non_head.as_ref())
        else {
            continue;
        };
        let matched = synth_compound_subrule(
            g,
            table,
            word,
            &nh,
            rule,
            sr,
            &head_segs,
            &head_node_of,
            &nh_segs,
            &nh_node_of,
            &new_syn,
            head_fst,
            head_names,
            nh_fst,
            nh_names,
        );
        record_mrule_reach(
            mstats,
            i as u32,
            (head_segs.len() + nh_segs.len()) as u64,
            u64::from(matched.is_ok()),
            &mut reached,
        );
        match matched {
            Ok(mut w) => {
                if trace.is_tracing() {
                    w.trace = Some(trace.morphological_rule_applied(parent, mrid, i as i32, &w));
                }
                output.push(w);
                break;
            }
            Err(reason) => {
                if trace.is_tracing() {
                    trace.morphological_rule_not_applied(parent, mrid, i as i32, word, reason);
                }
            }
        }
    }
    record_mrule_invocation_end(
        mstats,
        reached,
        output.len() as u64,
        (head_segs.len() + nh_segs.len()) as u64,
    );
    output
}

/// Returns which side's pattern failed rather than a bare `None`; the head is tried first, so `NonHeadPattern` is only reported once the head has matched.
#[allow(clippy::too_many_arguments)]
fn synth_compound_subrule(
    g: &Grammar,
    table: TableId,
    word: &Word,
    nh: &Word,
    rule: &CompoundingRuleDef,
    sr: &CompoundingSubruleDef,
    head_segs: &[Segment],
    head_node_of: &[usize],
    nh_segs: &[Segment],
    nh_node_of: &[usize],
    new_syn: &FeatureStruct,
    head_fst: &Fst,
    head_names: &[String],
    nh_fst: &Fst,
    nh_names: &[String],
) -> Result<Word, FailureReason> {
    let head_res = Transduce::new(head_fst, head_segs.to_vec())
        .anchored(true, true)
        .first_match()
        .ok_or(FailureReason::HeadPattern)?;
    let nh_res = Transduce::new(nh_fst, nh_segs.to_vec())
        .anchored(true, true)
        .first_match()
        .ok_or(FailureReason::NonHeadPattern)?;
    let head_ranges = part_ranges(head_fst, head_names, &head_res);
    let nh_ranges = part_ranges(nh_fst, nh_names, &nh_res);

    let mut out: Vec<OutNode> = Vec::new();
    for action in &sr.rhs {
        match action {
            OutputAction::Copy(PartRef::Head(i)) => {
                let src = PartSource {
                    node_of: head_node_of,
                    shape: &word.shape,
                    range: head_ranges[*i as usize],
                    head: true,
                };
                copy_part(g, table, &mut out, &src, None, None);
            }
            OutputAction::Copy(PartRef::NonHead(i)) => {
                let src = PartSource {
                    node_of: nh_node_of,
                    shape: &nh.shape,
                    range: nh_ranges[*i as usize],
                    head: false,
                };
                copy_part(g, table, &mut out, &src, None, None);
            }
            OutputAction::InsertSegments { shape, .. } => {
                insert_segments(g, table, &mut out, &shape.shape, Origin::Insert);
            }
            OutputAction::InsertContext(ctx) => out.push(OutNode {
                kind: NodeKind::Segment,
                char_def: NO_CHAR_DEF,
                lanes: ctx_lanes(g, table, ctx),
                optional: false,
                origin: Origin::Insert,
                cd_set: ctx_cd_set(g, table, ctx),
            }),
            // Modify / Input refs are not used by the reference compounding rules.
            _ => {}
        }
    }

    let morphs = attribute_morphs(&out, word, Some(nh), None);
    let mut w = word.clone();
    w.shape = freeze_out(g, &out);
    w.syn_fs = new_syn.clone();
    w.mpr = g.mpr_add_output(
        g.mpr_add_output(word.mpr, sr.out_mpr),
        rule.output_prod_restrictions_mpr,
    );
    w.morphs = morphs;
    w.obligatory.extend_from_slice(&rule.obligatory_features);
    // The consumed non-head stays in `w.non_heads` as history (matching C#) since `WordKey` needs it to distinguish surface-homophone compounds; `current_non_head` is index-based, not `last()` — do not "simplify" it back, since generation seeds can push several non-heads at once.
    w.flags.is_last_applied_rule_final = None;
    Ok(w)
}

// Compounding — analysis.

/// The combined head+non-head part list an `ana_compound` subrule matches against (`h{i}`/`n{i}` names, concatenated).
fn ana_compound_parts(sr: &CompoundingSubruleDef) -> Vec<(String, &Pattern)> {
    let mut parts: Vec<(String, &Pattern)> = Vec::new();
    for (i, p) in sr.head_lhs.iter().enumerate() {
        parts.push((format!("h{i}"), p));
    }
    for (i, p) in sr.non_head_lhs.iter().enumerate() {
        parts.push((format!("n{i}"), p));
    }
    parts
}

/// Build the analysis LHS and its compiled FST for one compounding subrule — `build_ana_affix_lhs`'s counterpart, cached per (rule, subrule) pair for the same reason.
fn build_ana_compound_lhs(
    g: &Grammar,
    table: TableId,
    sr: &CompoundingSubruleDef,
) -> Result<(Fst, AnalysisLhs), BridgeError> {
    let parts = ana_compound_parts(sr);
    let lhs = build_analysis_lhs(g, table, &parts, &sr.rhs)?;
    let fst = CompileInput::new(lhs.nodes.clone())
        .deterministic(false)
        .compile_with_direction(Direction::LeftToRight);
    Ok((fst, lhs))
}

#[allow(clippy::too_many_arguments)]
fn ana_compound(
    g: &Grammar,
    word: &Word,
    rule: &CompoundingRuleDef,
    root_filter: Option<NonHeadRootFilter>,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    // Same guard and adjust as `ana_affix`; the head-required/out pair plays the affix rule's required/out role exactly.
    let Some(new_syn) = ana_syn_fs(g, rule.head_required_syn_fs, rule.out_syn_fs, word) else {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };
    let table = crate::cache::owning_table_for_compounding_rule(g, rule).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    let mut output = Vec::new();
    let mut reached: u32 = 0;
    for (i, sr) in rule.subrules.iter().enumerate() {
        let Ok((fst, lhs)) = build_ana_compound_lhs(g, table, sr) else {
            continue;
        };
        let before = output.len();
        output.extend(ana_compound_subrule(
            g,
            table,
            word,
            rule,
            sr,
            &lhs,
            &fst,
            &segs,
            &node_of,
            &new_syn,
            root_filter,
        ));
        let n = (output.len() - before) as u64;
        record_mrule_reach(mstats, i as u32, segs.len() as u64, n, &mut reached);
    }
    record_mrule_invocation_end(mstats, reached, output.len() as u64, segs.len() as u64);
    output
}

/// `crate::cache::RuleCache`-aware sibling of `ana_compound`, also driving the `AnalysisPhase` breakdown.
#[allow(clippy::too_many_arguments)]
fn ana_compound_cached(
    g: &Grammar,
    word: &Word,
    rule: &CompoundingRuleDef,
    mrid: MRuleId,
    cache: &crate::cache::RuleCache,
    root_filter: Option<NonHeadRootFilter>,
    mstats: Option<MRuleStatsCtx>,
) -> Vec<Word> {
    let _synfs_phase = mstats.map(|m| m.stats.phase_enter(AnalysisPhase::AnaSynFs, 1));
    let Some(new_syn) = ana_syn_fs(g, rule.head_required_syn_fs, rule.out_syn_fs, word) else {
        record_mrule_none_residual(mstats, word.shape.len() as u64);
        return Vec::new();
    };
    drop(_synfs_phase);
    let table = crate::cache::owning_table_for_mrule(g, mrid).unwrap_or(TableId(0));
    let _segs_phase = mstats.map(|m| m.stats.phase_enter(AnalysisPhase::SegsOf, 1));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    drop(_segs_phase);
    let cc = cache.compound(mrid);
    let mut output = Vec::new();
    let mut reached: u32 = 0;
    for (i, sr) in rule.subrules.iter().enumerate() {
        let Some((fst, lhs)) = cc.subrules[i].ana.as_ref() else {
            continue;
        };
        let before = output.len();
        let _sr_phase = mstats.map(|m| m.stats.phase_enter(AnalysisPhase::AnaCompound, 1));
        output.extend(ana_compound_subrule(
            g,
            table,
            word,
            rule,
            sr,
            lhs,
            fst,
            &segs,
            &node_of,
            &new_syn,
            root_filter,
        ));
        drop(_sr_phase);
        let n = (output.len() - before) as u64;
        record_mrule_reach(mstats, i as u32, segs.len() as u64, n, &mut reached);
    }
    record_mrule_invocation_end(mstats, reached, output.len() as u64, segs.len() as u64);
    output
}

/// One subrule's analysis-side match, `GenerateShape`, and dedup (scope resets per subrule); with `root_filter` `Some`, each raw split multiplies into one candidate per surviving root allomorph, and a split matching no root is discarded entirely.
#[allow(clippy::too_many_arguments)]
fn ana_compound_subrule(
    g: &Grammar,
    table: TableId,
    word: &Word,
    rule: &CompoundingRuleDef,
    sr: &CompoundingSubruleDef,
    lhs: &AnalysisLhs,
    fst: &Fst,
    segs: &[Segment],
    node_of: &[usize],
    new_syn: &FeatureStruct,
    root_filter: Option<NonHeadRootFilter>,
) -> Vec<Word> {
    let parts = ana_compound_parts(sr);
    let head_parts: Vec<(String, &Pattern)> = parts
        .iter()
        .filter(|(n, _)| n.starts_with('h'))
        .map(|(n, p)| (n.clone(), *p))
        .collect();
    let nh_parts: Vec<(String, &Pattern)> = parts
        .iter()
        .filter(|(n, _)| n.starts_with('n'))
        .map(|(n, p)| (n.clone(), *p))
        .collect();
    let mut sr_out: Vec<Word> = Vec::new();
    for result in Transduce::new(fst, segs.to_vec())
        .anchored(true, true)
        .all_matches()
    {
        // Acceptable only if at least one head part was captured.
        let head_captured = head_parts.iter().any(|(name, _)| {
            (0..*lhs.captured.get(name).unwrap_or(&0)).any(|idx| {
                fst.get_offsets(&group_name(name, idx), &result.registers)
                    .is_some()
            })
        });
        if !head_captured {
            continue;
        }
        let head_out = generate_shape(
            g,
            table,
            &head_parts,
            lhs,
            fst,
            &result,
            node_of,
            &word.shape,
        );
        let nh_out = generate_shape(g, table, &nh_parts, lhs, fst, &result, node_of, &word.shape);
        let head_shape = freeze_out(g, &head_out);
        let nh_shape = freeze_out(g, &nh_out);
        match root_filter {
            None => {
                let mut w = word.clone();
                w.shape = head_shape;
                w.syn_fs = new_syn.clone();
                // Must push the split-off non-head AND advance the index in lock-step, or `current_non_head()`'s index-based lookup leaves the just-split non-head invisible to `synth_compound`'s gate.
                w.non_head_unapplied(Word::new(nh_shape, word.stratum));
                push_remove_duplicates_compound(&mut sr_out, w);
            }
            Some(filter) => {
                for resolved_nh in resolve_non_head_roots(g, rule, filter, &nh_shape, word.stratum)
                {
                    let mut w = word.clone();
                    w.shape = head_shape.clone();
                    w.syn_fs = new_syn.clone();
                    w.non_head_unapplied(resolved_nh);
                    push_remove_duplicates_compound_pinned(&mut sr_out, w);
                }
            }
        }
    }
    sr_out
}

/// C#'s root-allomorph search, gates, and pin: builds a resolved non-head `Word` per surviving root allomorph so `attribute_morphs` and `synth_compound`'s non-head gate see real data; an empty result meaningfully discards the whole split.
fn resolve_non_head_roots(
    g: &Grammar,
    rule: &CompoundingRuleDef,
    filter: NonHeadRootFilter,
    nh_shape: &Shape,
    stratum: StratumId,
) -> Vec<Word> {
    let req = g.fs_interner.get(rule.non_head_required_syn_fs);
    let mut out = Vec::new();
    for resolved in filter(stratum, nh_shape) {
        let crate::word::ResolvedRoot::Grammar(allo_id, le_id) = resolved else {
            let crate::word::ResolvedRoot::Supplied(root) = resolved else {
                unreachable!()
            };
            if !is_unifiable(req, &root.syn_fs)
                || !rule.non_head_prod_restrictions_mpr.compound_match(root.mpr)
            {
                continue;
            }
            let table = &g.char_tables[g.strata[root.stratum.0 as usize].table.0 as usize];
            let Ok(shape) =
                crate::shape_feat::segment_with_features(g, table, &root.lexical_spelling)
            else {
                continue;
            };
            let mut nh = Word::new(shape, root.stratum);
            nh.syn_fs = root.syn_fs.clone();
            nh.mpr = root.mpr;
            nh.root_allomorph = Some(AllomorphId::GUESSED);
            nh.root_runtime_id = Some(root.realization_id.clone());
            nh.morphs = vec![
                MorphRecord::new(AllomorphId::GUESSED, MorphemeId::GUESSED, 0)
                    .with_runtime_root(crate::word::RuntimeRoot::Supplied(root)),
            ];
            out.push(nh);
            continue;
        };
        let entry = &g.entries[le_id.0 as usize];
        if !is_unifiable(req, g.fs_interner.get(entry.syn_fs)) {
            continue;
        }
        if !rule
            .non_head_prod_restrictions_mpr
            .compound_match(entry.mpr)
        {
            continue;
        }
        let Some(allo) = entry.allomorphs.iter().find(|a| a.id == allo_id) else {
            continue;
        };
        let root_stratum = g.morphemes[entry.morpheme.0 as usize].stratum;
        let table = &g.char_tables[g.strata[root_stratum.0 as usize].table.0 as usize];
        let shape = crate::shape_feat::segment_with_features(g, table, &allo.shape.text)
            .unwrap_or_else(|_| allo.shape.shape.clone());
        let mut nh = Word::new(shape, root_stratum);
        nh.syn_fs = g.fs_interner.get(entry.syn_fs).clone();
        nh.mpr = entry.mpr;
        nh.flags.is_partial = entry.partial;
        nh.root_allomorph = Some(allo_id);
        nh.morphs = vec![MorphRecord::new(allo_id, entry.morpheme, 0)];
        out.push(nh);
    }
    out
}

/// `push_remove_duplicates` extended to the (head, non-head) shape pair, for the lexicon-free path; "longer" is judged on the head shape alone, mirroring C#.
fn push_remove_duplicates_compound(out: &mut Vec<Word>, w: Word) {
    push_keep_longer(out, w, |a, b| {
        shape_duplicates(&a.shape, &b.shape)
            && shape_duplicates(
                &a.non_heads.last().unwrap().shape,
                &b.non_heads.last().unwrap().shape,
            )
    });
}

/// The root-allomorph-pinned sibling, used once the non-head is resolved: C#'s duplicate key is the head shape plus the same pinned allomorph id, not the non-head shape (which is already shared by construction).
fn push_remove_duplicates_compound_pinned(out: &mut Vec<Word>, w: Word) {
    let allo = w.current_non_head().and_then(|nh| nh.root_allomorph);
    push_keep_longer(out, w, |a, b| {
        shape_duplicates(&a.shape, &b.shape)
            && b.current_non_head().and_then(|nh| nh.root_allomorph) == allo
    });
}

// Compile-once cache — `crate::cache::RuleCache`'s allomorph/compounding slices.

/// One compounding subrule's precompiled matchers. A field is `None` iff its pattern failed to
/// compile; the runtime functions already treat a compile failure as "this subrule cannot apply",
/// so a cached `None` reproduces that exactly.
pub(crate) struct CompoundSubruleCache {
    pub(crate) synth_head: Option<(Fst, Vec<String>)>,
    pub(crate) synth_non_head: Option<(Fst, Vec<String>)>,
    pub(crate) ana: Option<(Fst, AnalysisLhs)>,
}

/// One compounding rule's precompiled matchers, one `CompoundSubruleCache` per subrule.
pub(crate) struct CompoundCache {
    pub(crate) subrules: Vec<CompoundSubruleCache>,
}

/// Build the compile-once cache for one compounding rule. `table` is the rule's own owning table,
/// already resolved once by the caller.
pub(crate) fn build_compound_cache(
    g: &Grammar,
    table: TableId,
    rule: &CompoundingRuleDef,
) -> CompoundCache {
    let subrules = rule
        .subrules
        .iter()
        .map(|sr| CompoundSubruleCache {
            synth_head: compile_parts(g, table, &sr.head_lhs, "h", true).ok(),
            synth_non_head: compile_parts(g, table, &sr.non_head_lhs, "n", true).ok(),
            ana: build_ana_compound_lhs(g, table, sr).ok(),
        })
        .collect();
    CompoundCache { subrules }
}

/// One allomorph's precompiled matchers. Root allomorphs never populate these — only an
/// `AffixAllomorphDef` has an LHS and RHS to compile.
pub(crate) struct AllomorphLhsCache {
    pub(crate) synth_lhs: Option<(Fst, Vec<String>)>,
    pub(crate) ana_lhs: Option<(Fst, AnalysisLhs)>,
}

/// Build the LHS/RHS half of one affix allomorph's cache entry; the caller pairs it with the
/// environment-gate half. `table` must be the allomorph's own owning table — compiling against a
/// `TableId(0)` default here leaves the allomorph's pattern table-blind even when the environment
/// half is correct.
pub(crate) fn build_allomorph_lhs_cache(
    g: &Grammar,
    table: TableId,
    allo: &AffixAllomorphDef,
) -> AllomorphLhsCache {
    AllomorphLhsCache {
        synth_lhs: compile_parts(g, table, &allo.lhs, "p", true).ok(),
        ana_lhs: build_ana_affix_lhs(g, table, allo).ok(),
    }
}
