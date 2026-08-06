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
use crate::stratum::NonHeadRootFilter;
use crate::trace::{FailureReason, TraceHandle, TraceSink};
use crate::word::{MorphRecord, MorphStatus, Word};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

// Table zero is never an implicit default. Every function here that resolves a char-def or
// natural-class identity takes an explicit `table: TableId`, resolved ONCE per rule application at
// the entry point and threaded down — never re-derived by a low-level helper.
//
// One table per application is complete, not an approximation, even though a word's shape can carry
// material from an earlier different-table stratum: that material is already frozen into concrete
// char-def/lane values, and every helper below resolves table-relative identities only for material
// THIS rule's own declaration introduces fresh.

// =================================================================================================
// Public API (the surface M4b/M5 compose over).
// =================================================================================================

/// Apply `rule` forward to `word` (synthesis). Empty if the rule does not apply — gating failed, or
/// no allomorph matched.
///
/// Recompiles every allomorph/subrule LHS FST on every call, deliberately: this entry point is also
/// called on standalone, non-grammar-resident rule fixtures that have no stable index into a
/// `crate::cache::RuleCache`. The real per-word pipeline calls `synthesize_cached` instead.
pub fn synthesize(g: &Grammar, word: &Word, rule: &MorphRuleDef) -> Vec<Word> {
    let out = match rule {
        MorphRuleDef::AffixProcess(def) => synth_affix(g, word, def),
        MorphRuleDef::Compounding(def) => synth_compound(g, word, def),
        MorphRuleDef::Realizational(def) => synth_realizational(g, word, def),
    };
    apply_blocking(g, out, rule.blockable())
}

/// The `crate::cache::RuleCache`-aware sibling of `synthesize`, used by the real per-word pipeline.
/// `mrid` must identify `rule` — every production call site already holds both. Pass
/// `&NoopSink`/`TraceHandle::DUMMY` for an untraced call.
pub(crate) fn synthesize_cached_traced(
    g: &Grammar,
    mrid: MRuleId,
    word: &Word,
    rule: &MorphRuleDef,
    cache: &crate::cache::RuleCache,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    let out = match rule {
        MorphRuleDef::AffixProcess(def) => {
            synth_affix_cached(g, word, def, mrid, cache, trace, parent)
        }
        MorphRuleDef::Compounding(def) => {
            synth_compound_cached(g, word, def, mrid, cache, trace, parent)
        }
        MorphRuleDef::Realizational(def) => {
            synth_realizational_cached(g, word, def, mrid, cache, trace, parent)
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
        &crate::trace::NoopSink,
        crate::trace::TraceHandle::DUMMY,
    )
}

/// `g.mpr_group_ok` folds C#'s required and excluded MPR gates into one bool; this reports which of
/// the two actually failed, in C#'s own required-then-excluded order.
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
    match rule {
        MorphRuleDef::AffixProcess(def) => ana_affix(g, word, def),
        MorphRuleDef::Compounding(def) => ana_compound(g, word, def, None),
        MorphRuleDef::Realizational(def) => ana_realizational(g, word, def),
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
) -> Vec<Word> {
    match rule {
        MorphRuleDef::AffixProcess(def) => ana_affix_cached(g, word, def, cache),
        MorphRuleDef::Compounding(def) => ana_compound_cached(g, word, def, mrid, cache, None),
        MorphRuleDef::Realizational(def) => ana_realizational_cached(g, word, def, cache),
    }
}

/// `analyze_cached`'s sibling for the one call site that also holds the non-head lexicon filter.
/// Only a `Compounding` rule consumes it. Threading the filter in here rather than post-filtering
/// the returned words is what lets root resolution join C#'s **per-subrule** dedup scope — see
/// `ana_compound_subrule`.
pub(crate) fn analyze_cached_with_root_filter(
    g: &Grammar,
    mrid: MRuleId,
    word: &Word,
    rule: &MorphRuleDef,
    cache: &crate::cache::RuleCache,
    root_filter: NonHeadRootFilter,
) -> Vec<Word> {
    match rule {
        MorphRuleDef::AffixProcess(def) => ana_affix_cached(g, word, def, cache),
        MorphRuleDef::Compounding(def) => {
            ana_compound_cached(g, word, def, mrid, cache, Some(root_filter))
        }
        MorphRuleDef::Realizational(def) => ana_realizational_cached(g, word, def, cache),
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
    match rule {
        MorphRuleDef::AffixProcess(def) => ana_affix(g, word, def),
        MorphRuleDef::Compounding(def) => ana_compound(g, word, def, Some(root_filter)),
        MorphRuleDef::Realizational(def) => ana_realizational(g, word, def),
    }
}

// =================================================================================================
// Traced analysis — the analysis-side mirror of `synthesize_cached_traced`.
//
// Each function below is a thin trace-emitting shell around the existing per-allomorph/subrule
// matcher; none reimplements any matching logic, so the returned words are exactly what the
// untraced sibling returns. Only trace events and each output's `.trace` cursor are added, and an
// untracing sink short-circuits straight back to that sibling.
//
// Reason-mapping, stated once rather than per call site. A rule-level `ana_syn_fs` failure is
// reported with the SAME variant the synthesis-side twin gate uses, even though `ana_syn_fs`
// literally checks its `out` parameter rather than `req`: both are feature-structure unify failures
// of the same kind, which is what the consuming census bucket measures. A per-allomorph FST match
// producing nothing is reported as `Pattern`. For `Compounding`, `Pattern` ALSO absorbs "the FST
// matched but `resolve_non_head_roots` found no lexicon entry" — an approximation, flagged here
// rather than smoothed over, on the grounds that a non-head lexical miss is closer to a shape
// mismatch than to any of the other buckets.
// =================================================================================================

/// `analyze_cached`'s traced sibling. See this section's header for the fast-path and reason-mapping
/// contract every function below shares.
pub(crate) fn analyze_cached_traced(
    g: &Grammar,
    mrid: MRuleId,
    word: &Word,
    rule: &MorphRuleDef,
    cache: &crate::cache::RuleCache,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    if !trace.is_tracing() {
        return analyze_cached(g, mrid, word, rule, cache);
    }
    match rule {
        MorphRuleDef::AffixProcess(def) => {
            ana_affix_cached_traced(g, word, def, mrid, cache, trace, parent)
        }
        MorphRuleDef::Compounding(def) => {
            ana_compound_cached_traced(g, word, def, mrid, cache, None, trace, parent)
        }
        MorphRuleDef::Realizational(def) => {
            ana_realizational_cached_traced(g, word, def, mrid, cache, trace, parent)
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
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    if !trace.is_tracing() {
        return analyze_cached_with_root_filter(g, mrid, word, rule, cache, root_filter);
    }
    match rule {
        MorphRuleDef::AffixProcess(def) => {
            ana_affix_cached_traced(g, word, def, mrid, cache, trace, parent)
        }
        MorphRuleDef::Compounding(def) => {
            ana_compound_cached_traced(g, word, def, mrid, cache, Some(root_filter), trace, parent)
        }
        MorphRuleDef::Realizational(def) => {
            ana_realizational_cached_traced(g, word, def, mrid, cache, trace, parent)
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
        return Vec::new();
    };
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    let mut output = Vec::new();
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        let Some((fst, lhs)) = cache.allomorph(allo.id).ana_lhs.as_ref() else {
            continue;
        };
        let before = output.len();
        for mut w in ana_affix_allomorph(g, table, word, allo, lhs, fst, &segs, &node_of, &new_syn)
        {
            w.trace = Some(trace.morphological_rule_unapplied(parent, mrid, i as i32, &w));
            output.push(w);
        }
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
    output
}

#[allow(clippy::too_many_arguments)]
fn ana_realizational_cached_traced(
    g: &Grammar,
    word: &Word,
    rule: &RealizationalRuleDef,
    mrid: MRuleId,
    cache: &crate::cache::RuleCache,
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
        return Vec::new();
    };
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    let mut output = Vec::new();
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        let Some((fst, lhs)) = cache.allomorph(allo.id).ana_lhs.as_ref() else {
            continue;
        };
        let before = output.len();
        for mut w in
            ana_realizational_allomorph(g, table, word, allo, lhs, fst, &segs, &node_of, &real_fs)
        {
            w.trace = Some(trace.morphological_rule_unapplied(parent, mrid, i as i32, &w));
            output.push(w);
        }
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
        return Vec::new();
    };
    let table = crate::cache::owning_table_for_mrule(g, mrid).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    let cc = cache.compound(mrid);
    let mut output = Vec::new();
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
    output
}

// =================================================================================================
// Lexical-family blocking (W5) — `Word.CheckBlocking` / the `ChooseInflectionalStem` seed helper.
// =================================================================================================

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

/// Runs `check_blocking` once over a whole rule application's output, where C# does it inline in
/// each of its three per-allomorph loops. Observably equivalent: blocking only substitutes one
/// already-produced word for another, and the loop-continuation condition it would have to
/// influence is allomorph-static, never a function of the word `check_blocking` just replaced.
fn apply_blocking(g: &Grammar, words: Vec<Word>, blockable: bool) -> Vec<Word> {
    if !blockable {
        return words;
    }
    words
        .into_iter()
        .map(|w| check_blocking(g, &w).unwrap_or(w))
        .collect()
}

/// `apply_blocking`'s traced sibling. C# fires `Blocked` BEFORE the rule's own `Applied` event, but
/// this port's blocking runs as a post-pass, so `Applied` was already minted from the PRE-block
/// word. Flagged as an accepted approximation of C#'s interleaving: node counts and reasons match,
/// emission order does not, and the replacement inherits the pre-block word's cursor either way.
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

// =================================================================================================
// Feature / lane helpers.
// =================================================================================================

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

/// Driver lanes for a char-def (width `W`, `full_mask` for unmentioned/boundary lanes). `table` is
/// the rule/allomorph's own owning table (see this module's top-of-file note), never an implicit
/// default.
fn cd_lanes(g: &Grammar, table: TableId, cd_raw: u32) -> Vec<u64> {
    if cd_raw == NO_CHAR_DEF {
        return full_lanes(g);
    }
    let t = &g.char_tables[table.0 as usize];
    fit(g, t.get(CharDefId(cd_raw)).feature_lanes())
}

/// The `(feature, symbol-bits)` a `SimpleContext` pins (alpha-variable features left unconstrained).
/// `table` is the rule/allomorph's own owning table -- see this module's top-of-file note.
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

/// The char-def-set a `SimpleContext`'s natural class carries — this port's `StrRep` analog. A
/// `Segments`-kind class is exactly its member list; a `Feature`-kind class is every char-def whose
/// lanes unify with `ctx_pins`. `Unrestricted` when that set is the whole table, so a class meaning
/// "any segment" never materializes a full-table bitset.
fn ctx_cd_set(g: &Grammar, table: TableId, ctx: &SimpleContext) -> CdSet {
    let nc = &g.natural_classes[ctx.nat_class.0 as usize];
    match &nc.kind {
        NaturalClassKind::Segments(segs) => {
            CdSet::Members(CdBits::from_ids(segs.iter().map(|cd| cd.0)))
        }
        NaturalClassKind::Feature(_) => {
            let pins = ctx_pins(g, table, ctx);
            if pins.is_empty() {
                // Nothing pinned (e.g. every feature is alpha-variable-governed): the class matches
                // every segment, same as an all-unconstrained lane row.
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

/// The owned `CdSet` to carry onto a new `OutNode` copying an existing shape node `p`. A concrete
/// source yields `Unrestricted`, harmlessly: the copy keeps the same real `char_def` and
/// `Shape::node_cd_set` derives the singleton from that, never reading this field. Only a source
/// that was itself `NO_CHAR_DEF` needs its real membership set propagated.
fn cd_set_of(shape: &Shape, p: usize) -> CdSet {
    match shape.node_cd_set(p) {
        EffectiveCdSet::Singleton(_) | EffectiveCdSet::Unrestricted => CdSet::Unrestricted,
        EffectiveCdSet::Members(b) => CdSet::Members(b.clone()),
    }
}

/// Convert driver full-mask lanes to FST-facing lanes (`full_mask` → `u64::MAX`), so the compiled
/// constraint canonicalizes identically to `bridge`/`rewrite`.
fn to_fst(g: &Grammar, lanes: &[u64]) -> Vec<u64> {
    lanes
        .iter()
        .enumerate()
        .map(|(f, &l)| if l == full_mask(g, f) { u64::MAX } else { l })
        .collect()
}

// =================================================================================================
// Segment sequences + shape freezing.
// =================================================================================================

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
    // The `StrRep` identity lane (see `PatternBridge::id_lane`): every input node carries its
    // char-def identity as a membership bitset. `Unrestricted` nodes, and tables too wide for the
    // lane, omit it entirely — absent means all-ones, so an underspecified node matches any
    // identity, exactly as a C# node with no `StrRep` value does.
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

/// Provenance of an output node, used both for morph attribution and (for existing morphs) span
/// remapping.
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
    /// Char-def-set identity, consulted only when `char_def == NO_CHAR_DEF`. A producer keeping a
    /// real `char_def` leaves this `Unrestricted` and it is never read; only
    /// `InsertSimpleContext`-originated nodes set a real `CdSet`.
    cd_set: CdSet,
}

/// Freeze interior `OutNode`s into a bracketed `Shape`. Optional segments use the
/// delete-then-reinsert workaround (as `rewrite.rs`), since `ShapeBuilder` has no set-flags-in-place.
fn freeze_out(g: &Grammar, nodes: &[OutNode]) -> Shape {
    let w = feat_width(g) as u32;
    let mut b = ShapeBuilder::with_features_capacity(w, nodes.len());
    for n in nodes {
        let lanes = fit(g, &n.lanes);
        match n.kind {
            // Class insertions carry their real cd_set; a concrete segment's own char_def already
            // is the identity, so `n.cd_set` is never consulted for one.
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

// =================================================================================================
// Part-group matching (synthesis + compounding head/non-head).
// =================================================================================================

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
    // Morphological-LHS FSTs carry the `StrRep` identity lane; their inputs all come from
    // `segs_of`, which emits the same lane.
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

/// Per-part captured `(start, end)` seg-position ranges of a match result (`None` = part not
/// captured / matched zero segments).
fn part_ranges(fst: &Fst, names: &[String], result: &FstResult) -> Vec<Option<(usize, usize)>> {
    names
        .iter()
        .map(|name| {
            fst.get_offsets(name, &result.registers)
                .map(|(a, b)| (a as usize, b as usize))
        })
        .collect()
}

// =================================================================================================
// Morph attribution.
// =================================================================================================

/// Which input morph owns a source interior node `idx` — a contiguous partition of `word.morphs`
/// by ascending `order`. Only `Real` records own nodes; the other statuses are markers riding at a
/// position. `Real` orders never tie, each owning a distinct leftmost node, so `max_by_key` is
/// unambiguous.
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

/// Build the output word's `MorphRecord`s from the constructed output nodes: existing morphs are
/// remapped to where their copied material landed, keeping their `passed_over` sets; new affix
/// material becomes records carrying `affix`'s own passed-over set.
///
/// One record per **contiguous run** of a morph's output positions, mirroring C# `MarkMorphs`'
/// split — a circumfix's two pieces, or a root split by an infix, are separate records sharing
/// allomorph, morpheme, and passed-over set. That is what keeps `crate::validity`'s span
/// derivation exact for discontinuous morphs: each run is checked at its own span.
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

    // Pass 2: build the output records, walking the input words' record vecs IN ORDER. The fallback
    // model for a record that owns no output positions this hop:
    //
    // * A `Real` record with runs is a normal positioned morph.
    // * A `Real` record with none (a later rule deleted all its material) subsumes: onto the
    //   affix's longest run as a `SubsumedChild` when the rule inserted new material, else onto
    //   order 0 as a `SubsumedFirst`. The two differ in placement because C#'s postorder traversal
    //   renders a subsumed child before its host, while its interval sort renders the containing
    //   annotation first.
    // * A `SubsumedChild`/`SubsumedFirst` never owns nodes; each hop it re-anchors to its host, the
    //   unique `Real` record sharing its order. If the host also dropped, both ride the host's own
    //   fallback — except that C#'s pure-truncation branch does NOT recurse into children, so a
    //   `SubsumedChild` is dropped there (bug-compatible) while a `SubsumedFirst` re-anchors at 0.
    // * A `Floating` record rides at `FLOATING_ORDER` until a hop with new material resolves it.
    // * A no-run record whose allomorph was already recorded this hop is skipped; first wins.
    //
    // Flagged approximation: C#'s fallback annotations own the actual first/last NODE, stolen from
    // the morph that had it, so a later hop can split ownership inside one run. This flat
    // order-partition model cannot, so markers here own no nodes and re-anchor by host-following.
    // Diverging would need a grammar that further affixes onto the stolen position AND renders
    // per-node attribution; no conformance surface does.
    //
    // Compounding (`affix: None`) has no fallbacks at all: an input morph with no copied material
    // is simply dropped, as in C#.
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
                    // Host dropped, pure truncation: C#'s Shape.First branch does not recurse into
                    // children (SubsumedChild is lost, bug-compatible); a top-level SubsumedFirst
                    // re-anchors at the new first node.
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

    // Floating markers: ride, resolve onto this hop's new material, and/or mint this rule's own.
    // Pushed after the subsumed-input records and before the affix runs, approximating C#'s
    // input-morph-order attachment.
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
            // Pure truncation: mint this rule's own floating marker. An entirely empty `out` has no
            // last node for C# either, so it is guarded rather than assumed away.
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
        // The affix's own runs, last — so every same-order subsumed/resolved record above renders
        // before its host (stable sort keeps insertion order at ties).
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

/// Sentinel `order` for a still-unresolved floating marker (see `attribute_morphs`): larger than
/// any real `out` position could ever be, so `owning_morph`'s `order <= idx` filter never selects
/// it, and it always sorts after every genuinely-positioned record in the word's own signature.
const FLOATING_ORDER: u32 = u32::MAX;

// =================================================================================================
// RHS execution (synthesis) — shared by affix and compounding.
// =================================================================================================

/// Resolve a `PartRef` to the matched source (segments + node map + captured range + origin tag).
struct PartSource<'a> {
    node_of: &'a [usize],
    shape: &'a Shape,
    range: Option<(usize, usize)>,
    head: bool, // true = Origin::Head, false = Origin::NonHead
}

/// Copy the captured nodes of `src` into `out`, tagging their origin. `force_origin` overrides the
/// default Copy/Modify-based choice: `Some(true)` pins the origin to the existing input morph even
/// for a modify, `Some(false)` pins it to `Origin::Affix` even for a plain copy. That is how
/// `classify_redup`'s output is threaded in — a repeated copy of one LHS part is not uniformly
/// "existing" the way a single occurrence is.
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
    // C# `GetSkippedOptionalNodes`: a run of Optional nodes immediately LEFT of the captured range
    // that reaches all the way back to the left anchor is folded into the copy, in surface order,
    // through the same body as the captured nodes. Boundaries are always Optional; Optional
    // segments additionally arise from the epenthesis/narrow analysis markers, hence the
    // two-pronged predicate.
    let mut positions: Vec<usize> = Vec::new();
    if s < e {
        let first_node = src.node_of[s];
        let skippable =
            |i: usize| src.shape.kind(i) == NodeKind::Boundary || src.shape.flags(i).is_optional();
        let mut i = first_node;
        while i > 0 && skippable(i - 1) {
            i -= 1;
        }
        // The walk must have stopped AT the left anchor for the fold to apply; stopping at any
        // non-optional interior node folds nothing at all.
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
                // A modified node must NOT keep the source node's literal identity: C# has no such
                // concept here at all — it re-derives candidate string representations from the
                // CURRENT feature structure — so retaining `char_def` would make a modified "p"
                // still render and match only as "p". Clearing to `NO_CHAR_DEF` plus the ctx's own
                // set cannot under-restrict, since a char-def unifying with the full pinned lanes
                // is always inside `ctx_cd_set`'s pins-only membership.
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
                // ModifyFromInput material is "new" (affix) for an affix rule; for compounding it
                // stays with its source morph. Callers building compounding pass modify=None on
                // head/non-head copies, so this Affix tag only fires on affix rules.
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

/// Append an `InsertSegments` shape's interior nodes to `out`. These always reference a concrete
/// literal `char_def`, never `NO_CHAR_DEF`, so `cd_set` stays `Unrestricted` and is never read.
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

// =================================================================================================
// Syntactic-FS gating.
// =================================================================================================

/// C# synthesis: `required.Unify(word.syn, useDefaults=true)`; on success priority-union `out`.
/// Returns the post-application syn FS, or `None` if the required FS does not unify.
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

/// The C# analysis guard and adjust. Note the guard is `out.IsUnifiable(word.syn)` — the rule's
/// OUTPUT FS against the input word, not `req`. On unapply every output's syntactic FS starts equal
/// to the input's, which is why it can be hoisted here, and then `req` (if non-empty) is `Add`ed —
/// a **widening union**, never a narrowing unify. An empty `out` with empty `req` clears it.
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

// MPR-group-aware required/excluded gating lives on `Grammar`, the only owner of `mpr_groups`. Do
// not reintroduce a flat overlap check here: it is correct only for singleton groups, and for
// `required` with 2+ ungrouped members it inverts C#'s semantics (which ANDs, not ORs). The
// compounding prod-restriction gates are the deliberate exception — C# checks those through the
// always-group-unaware `CompoundMprFeaturesMatch`, i.e. `MprSet::compound_match`.

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

/// `Allomorph.FreeFluctuatesWith`. Every call site passes *adjacent* allomorphs of one rule, so
/// C#'s index-range walk over intervening allomorphs collapses to a single `constraints_equal`
/// check, and its same-object/same-morpheme guards are vacuous here.
fn free_fluctuates_with(g: &Grammar, cur: &AffixAllomorphDef, next: &AffixAllomorphDef) -> bool {
    constraints_equal(g, cur, next)
}

// =================================================================================================
// Affix process — synthesis.
// =================================================================================================

/// Resolve `word`'s root allomorph to its own stem name. `None` when the word has no root allomorph
/// (defensive — a standalone fixture may lack one) or the allomorph carries no stem name.
///
/// A guessed root has no `allomorph_owners` row, so it is guarded like `check_blocking`. `None` is
/// conservative rather than exact: delegating to the guess's pattern, as the final validity check
/// does, is deliberately not attempted at this synthesis-time gate.
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

fn synth_affix(g: &Grammar, word: &Word, rule: &AffixProcessRuleDef) -> Vec<Word> {
    // Gate order matches C#: the two template prohibitions, then `RequiredStemName`, and the
    // required-syntactic-FS unify LAST. Every gate is independent, so the order decides only which
    // `FailureReason` the traced sibling reports first, never which words are produced.
    //
    // Both template checks are guarded on `!is_template_rule`: a rule that is itself a template
    // slot member is never subject to either, whatever the word's final-rule state. They exist to
    // gate an ordinary rule applied AFTER a template finished, not the template's own slot rules.
    // (a) After a *final* template, prohibit a non-partial rule.
    if !rule.is_template_rule
        && matches!(word.flags.is_last_applied_rule_final, Some(true))
        && !word.flags.is_partial
        && !rule.partial
    {
        return Vec::new();
    }
    // (b) After a *non-final* template, prohibit a partial rule unless the input is itself partial.
    if !rule.is_template_rule
        && matches!(word.flags.is_last_applied_rule_final, Some(false))
        && !word.flags.is_partial
        && rule.partial
    {
        return Vec::new();
    }

    // `requiredStemName` is a reference-equality gate on the WORD's root allomorph's stem name, not
    // on this rule's allomorphs'. `None` on both sides passes, as does an exact match.
    if rule.required_stem_name.is_some() && rule.required_stem_name != root_stem_name(g, word) {
        return Vec::new();
    }

    let Some(new_syn) = synth_syn_fs(g, rule.required_syn_fs, rule.out_syn_fs, word) else {
        return Vec::new();
    };

    // Resolved once per call against the rule's OWN owning stratum, never an implicit `TableId(0)`.
    // The fallback only fires for a non-grammar-resident fixture, this uncached path's audience.
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, true);
    let mut output = Vec::new();
    // Indices that already applied in THIS loop, recorded on each output morph before the
    // producing index itself is added.
    let mut applied: Vec<u16> = Vec::new();
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        if !g.mpr_group_ok(allo.required_mpr, allo.excluded_mpr, word.mpr) {
            continue;
        }
        // Recompiled per call, deliberately: see `synthesize`'s doc.
        let Ok((fst, names)) = compile_parts(g, table, &allo.lhs, "p", true) else {
            continue;
        };
        if let Some(w) = synth_process_allomorph(
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
        ) {
            output.push(w);
            applied.push(i as u16);
            // Disjunctive-allomorph break: stop after the first match unless this allomorph is
            // environment- or syn-constrained, or free-fluctuates with the next one — in which case
            // C# keeps going so the next allomorph's word is produced too.
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
    output
}

/// `crate::cache::RuleCache`-aware sibling of `synth_affix`, used by the real per-word pipeline.
/// Every early return reports its own `FailureReason` with subrule index `-1`, as C# does at the
/// same rule-level gates; a successful allomorph reports its real index and reassigns the output
/// word's trace cursor.
#[allow(clippy::too_many_arguments)]
fn synth_affix_cached(
    g: &Grammar,
    word: &Word,
    rule: &AffixProcessRuleDef,
    mrid: MRuleId,
    cache: &crate::cache::RuleCache,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    macro_rules! not_applied {
        ($reason:expr) => {{
            if trace.is_tracing() {
                trace.morphological_rule_not_applied(parent, mrid, -1, word, $reason);
            }
            return Vec::new();
        }};
    }
    // Gate order and the `!is_template_rule` guards mirror `synth_affix` exactly; see its doc.
    // Order matters here only because the FIRST failing gate is the reason reported.
    // (a) Final-template prohibition.
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
        match synth_process_allomorph(
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
        ) {
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
    output
}

// =================================================================================================
// Realizational affix process (W5) — synthesis.
// =================================================================================================

/// C# `SynthesisRealizationalAffixProcessRule.IsBlocked`: every feature key `real_fs` declares must
/// also be a key `syn_fs` declares, recursing into nested complex values. A symbolic leaf need only
/// be *present*, never value-compared. All present ⇒ blocked. No cycle guard is needed: this
/// syntactic-FS model is a tree, never a DAG, so C#'s `visited` set can never revisit a pair.
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

/// C# `SynthesisRealizationalAffixProcessRule.Apply`. Three rule-level gates precede the loop, in
/// C#'s order: `real_fs` subsumption against the word's *current* `real_fs`; `IsBlocked`, only when
/// the rule's `real_fs` is non-empty and checked against the syn FS *before* the unify below; then
/// the required-syn-FS unify, which is `synth_syn_fs`'s shape with `real_fs` standing in for
/// `out_syn_fs` and so reuses it verbatim.
///
/// This class has NO partial/final/obligatory/max-application gates at all. The per-allomorph loop
/// is otherwise identical to `synth_affix`'s, via the shared `synth_process_allomorph`.
fn synth_realizational(g: &Grammar, word: &Word, rule: &RealizationalRuleDef) -> Vec<Word> {
    let real_fs = g.fs_interner.get(rule.real_fs);
    if !pg_featstruct::subsumes(real_fs, &word.real_fs) {
        return Vec::new();
    }
    if !real_fs.is_empty() && realizational_is_blocked(real_fs, &word.syn_fs) {
        return Vec::new();
    }
    let Some(new_syn) = synth_syn_fs(g, rule.required_syn_fs, rule.real_fs, word) else {
        return Vec::new();
    };

    // Resolved once per call — see `synth_affix`'s twin site.
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, true);
    let mut output = Vec::new();
    let mut applied: Vec<u16> = Vec::new();
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        if !g.mpr_group_ok(allo.required_mpr, allo.excluded_mpr, word.mpr) {
            continue;
        }
        let Ok((fst, names)) = compile_parts(g, table, &allo.lhs, "p", true) else {
            continue;
        };
        if let Some(w) = synth_process_allomorph(
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
        ) {
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
    output
}

/// `crate::cache::RuleCache`-aware sibling of `synth_realizational`. The first two gates stay
/// UNTRACED on purpose: C# fires no trace call at either site, so tracing them would fabricate
/// events C# never produces. Only the required-syn-FS unify and the allomorph loop are traced.
#[allow(clippy::too_many_arguments)]
fn synth_realizational_cached(
    g: &Grammar,
    word: &Word,
    rule: &RealizationalRuleDef,
    mrid: MRuleId,
    cache: &crate::cache::RuleCache,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    let real_fs = g.fs_interner.get(rule.real_fs);
    if !pg_featstruct::subsumes(real_fs, &word.real_fs) {
        return Vec::new();
    }
    if !real_fs.is_empty() && realizational_is_blocked(real_fs, &word.syn_fs) {
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
        return Vec::new();
    };

    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, true);
    let mut output = Vec::new();
    let mut applied: Vec<u16> = Vec::new();
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
        match synth_process_allomorph(
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
        ) {
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
    output
}

/// The `PartRef::Input` index an RHS action references — only copy and modify carry one; the two
/// insert kinds reference no part. A `PartRef::Input(i)` corresponds to C#'s `lhs[i].Name`, so
/// grouping by this index is equivalent to C#'s grouping by part name.
fn redup_part_ref(action: &OutputAction) -> Option<u16> {
    match action {
        OutputAction::Copy(PartRef::Input(i)) | OutputAction::Modify(PartRef::Input(i), _) => {
            Some(*i)
        }
        _ => None,
    }
}

/// Reduplication morph attribution — C#'s `_nonAllomorphActions`, ported with part names replaced
/// by `PartRef::Input` indices. For every RHS index inside a "true" reduplication group (an `Input`
/// part referenced twice or more), reports whether that occurrence is the *existing* echo of the
/// input morph or genuinely new affix material. Indices outside any repeated group are absent, so
/// callers keep their default attribution: a lone copy is existing, a lone modify is new.
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
    // Deterministic order for the loop below (matters only for tie-free readability; each group is
    // classified independently so iteration order cannot change the result).
    redup_parts.sort_by_key(|v| v[0]);

    // `start`: the RHS index at which a `lhs_len`-long run echoes every LHS part exactly once, in
    // original order — the plain, non-reduplicating repetition of the whole allomorph input.
    // `None` when no such contiguous run exists. Signed throughout, as in C#, and widened to `i64`
    // so a literal translation of its subtractions cannot underflow.
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

/// One allomorph's synthesis: LHS match plus RHS build, shared by the regular affix-process and
/// realizational paths. C# builds both from the same per-allomorph rule spec, so the
/// match-then-emit mechanics are identical; only the rule-level bookkeeping differs, which is why
/// those fields arrive individually rather than as a `&AffixProcessRuleDef`. The realizational
/// caller passes `obligatory: &[]`, `partial: None`, and `apply_out_mpr: false`, because that C#
/// class touches none of the three. `partial: None` means "leave `word.flags` exactly as cloned",
/// NOT "treat as non-partial".
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

    // Empty unless this allomorph's RHS actually repeats an `Input` part, so a non-reduplicating
    // allomorph pays only for building an empty map.
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

// =================================================================================================
// Affix process — analysis.
// =================================================================================================

/// The analysis LHS built from RHS actions, plus capture bookkeeping. `pub(crate)` so
/// `crate::cache::RuleCache` can store the compiled `(Fst, AnalysisLhs)` pair per allomorph/subrule.
pub(crate) struct AnalysisLhs {
    nodes: Vec<CompileNode>,
    /// part name → number of capture groups generated for it.
    captured: HashMap<String, usize>,
    /// part name → (capture-group index, ctx) for a `ModifyFromInput` (its material is
    /// underspecified on `GenerateShape`).
    modify: HashMap<String, (usize, SimpleContext)>,
}

/// Strip boundary constraints from a pattern (C# `DeepCloneExceptBoundaries`): boundary char-defs
/// are dropped, and a quantifier whose children all vanish is dropped too.
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

/// Apply ctx pins to every `Constraint` node (recursively) of a compiled part — the analysis form
/// of `ModifyFromInput` matches the *modified* surface (`PriorityUnion` onto the pattern).
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
                        // The analysis-side consumer must find and consume *this* inserted segment,
                        // as C# does by matching the full char-def FS, not any unifiable one.
                        if let (Some(w), true) = (id_width, char_def != NO_CHAR_DEF) {
                            crate::bridge::push_id_lane(&mut lanes, w, 1u64 << char_def);
                        }
                        lhs.nodes.push(CompileNode::Constraint(lanes));
                    }
                }
            }
            OutputAction::InsertContext(ctx) => {
                // Known residual: this is an FST *match* constraint, not an output node, so there
                // is no shape to hang a `cd_set` on and `pg_fst::Segment` carries lanes only. The
                // id lane below closes it for `Segments`-kind classes on tables narrow enough to
                // have one; wider tables still over-match, accepting any segment unifiable with the
                // member lane-union rather than only real members.
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

/// `GenerateShape`: re-emit the captured original LHS parts into output nodes (dropping the inserted
/// material). Modify parts get their changed features underspecified.
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
            // Not captured: untruncate the part, materializing its segment constraints as
            // optional beyond a quantifier's min.
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
                            // The analysis-side counterpart of `copy_part`'s modify handling: a
                            // lexical root storing the PRE-modification segment can never be found
                            // by a char-def-equality lookup while this node still claims to BE the
                            // post-modification one. Clearing it makes lookup fall back to lane
                            // unification against the lanes just widened above.
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

/// Materialize a part's segment and context constraints as (optional) output nodes — C#
/// `AnalysisMorphologicalTransform.Untruncate`. Boundaries are skipped.
///
/// Quantifier semantics are C#'s exactly, and the unbounded case is the trap: an **unbounded**
/// quantifier emits NOTHING, because C#'s loop runs to `MaxOccur` and infinity is encoded as -1. A
/// bounded one emits `max` copies, optional beyond `min`. Emitting `max(min, 1)` instead fabricates
/// a phantom optional wildcard for every uncaptured `[Seg]*` part, and on narrowing-flooded
/// analysis shapes unrelated affix rules then "unapply" straight through those phantoms.
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

fn ana_affix(g: &Grammar, word: &Word, rule: &AffixProcessRuleDef) -> Vec<Word> {
    let Some(new_syn) = ana_syn_fs(g, rule.required_syn_fs, rule.out_syn_fs, word) else {
        return Vec::new();
    };
    // Resolved once per call — see `synth_affix`'s twin site.
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    let mut output = Vec::new();
    for allo in &rule.allomorphs {
        let Ok((fst, lhs)) = build_ana_affix_lhs(g, table, allo) else {
            continue;
        };
        output.extend(ana_affix_allomorph(
            g, table, word, allo, &lhs, &fst, &segs, &node_of, &new_syn,
        ));
    }
    output
}

/// `crate::cache::RuleCache`-aware sibling of `ana_affix`.
fn ana_affix_cached(
    g: &Grammar,
    word: &Word,
    rule: &AffixProcessRuleDef,
    cache: &crate::cache::RuleCache,
) -> Vec<Word> {
    let Some(new_syn) = ana_syn_fs(g, rule.required_syn_fs, rule.out_syn_fs, word) else {
        return Vec::new();
    };
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    let mut output = Vec::new();
    for allo in &rule.allomorphs {
        let Some((fst, lhs)) = cache.allomorph(allo.id).ana_lhs.as_ref() else {
            continue;
        };
        output.extend(ana_affix_allomorph(
            g, table, word, allo, lhs, fst, &segs, &node_of, &new_syn,
        ));
    }
    output
}

/// One allomorph's analysis-side match, `GenerateShape`, and dedup. `carry` writes whichever
/// feature structure the rule kind propagates onto each surviving candidate.
///
/// The dedup scope is freshly reset per allomorph, never shared across the whole rule — a candidate
/// from one allomorph must not suppress an identical-looking one from another.
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
    carry: impl Fn(&mut Word),
) -> Vec<Word> {
    let parts: Vec<(String, &Pattern)> = allo
        .lhs
        .iter()
        .enumerate()
        .map(|(i, p)| (format!("p{i}"), p))
        .collect();
    let mut allo_out: Vec<Word> = Vec::new();
    for result in Transduce::new(fst, segs.to_vec())
        .anchored(true, true)
        .all_matches()
    {
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
) -> Vec<Word> {
    ana_allomorph_matches(g, table, word, allo, lhs, fst, segs, node_of, |w| {
        w.syn_fs = new_syn.clone()
    })
}

// =================================================================================================
// Realizational affix process (W5) — analysis.
// =================================================================================================

/// C# `AnalysisRealizationalAffixProcessRule.Apply`: one rule-level gate, the realizational-FS
/// unify, after which every allomorph's matches all carry the SAME unified value. No
/// max-application or syntactic-FS gate exists on this class, unlike `ana_affix`.
fn ana_realizational(g: &Grammar, word: &Word, rule: &RealizationalRuleDef) -> Vec<Word> {
    let Some(real_fs) = unify(g.fs_interner.get(rule.real_fs), &word.real_fs) else {
        return Vec::new();
    };
    // Resolved once per call — see `synth_affix`'s twin site.
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    let mut output = Vec::new();
    for allo in &rule.allomorphs {
        let Ok((fst, lhs)) = build_ana_affix_lhs(g, table, allo) else {
            continue;
        };
        output.extend(ana_realizational_allomorph(
            g, table, word, allo, &lhs, &fst, &segs, &node_of, &real_fs,
        ));
    }
    output
}

/// `crate::cache::RuleCache`-aware sibling of `ana_realizational`.
fn ana_realizational_cached(
    g: &Grammar,
    word: &Word,
    rule: &RealizationalRuleDef,
    cache: &crate::cache::RuleCache,
) -> Vec<Word> {
    let Some(real_fs) = unify(g.fs_interner.get(rule.real_fs), &word.real_fs) else {
        return Vec::new();
    };
    let table = crate::cache::owning_table_for_morpheme(g, rule.morpheme).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    let mut output = Vec::new();
    for allo in &rule.allomorphs {
        let Some((fst, lhs)) = cache.allomorph(allo.id).ana_lhs.as_ref() else {
            continue;
        };
        output.extend(ana_realizational_allomorph(
            g, table, word, allo, lhs, fst, &segs, &node_of, &real_fs,
        ));
    }
    output
}

/// Unlike `ana_affix_allomorph`, the syntactic FS is left completely untouched: C#'s realizational
/// analysis never assigns it, only the realizational FS, so the clone's own value passes through.
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
) -> Vec<Word> {
    ana_allomorph_matches(g, table, word, allo, lhs, fst, segs, node_of, |w| {
        w.real_fs = real_fs.clone()
    })
}

/// C# `HermitCrabExtensions.RemoveDuplicates`: insert `w`, unless `out` already holds a candidate
/// whose **non-Optional** nodes form the identical sequence. Optional nodes are deliberately
/// ignored — they are exactly the reconstructed material narrowing and deletion analysis mark. A
/// duplicate keeps whichever shape is **longer**, preferring the candidate carrying more
/// reconstructed material; a strict tie keeps the earlier one.
///
/// This is not cosmetic. Once a phonological analysis rule has scattered Optional segments through
/// a shape, an affix rule's all-submatches matching yields many distinct *exact* shapes differing
/// only in which of those segments fell inside the matched parts. Nothing downstream ever unifies
/// them again, since `WordKey` compares the full shape, so keeping them all is a combinatorial
/// blow-up whose survivor, once the step budget trims it, is arbitrary rather than the longest.
fn push_remove_duplicates(out: &mut Vec<Word>, w: Word) {
    // `web_time::Instant`: this timestamp is unconditional even though the profiling read is gated,
    // and std's panics on wasm32-unknown-unknown, which this crate is built for.
    let start = web_time::Instant::now();
    let out_len = out.len();
    push_keep_longer(out, w, |a, b| shape_duplicates(&a.shape, &b.shape));
    dedup_profile::record(start.elapsed().as_nanos(), out_len);
}

/// The shared body of the three dedup passes: replace the first `dup`-matching candidate when `w`'s
/// shape is strictly longer, else keep what is there; append when nothing matches.
fn push_keep_longer(out: &mut Vec<Word>, w: Word, dup: impl Fn(&Word, &Word) -> bool) {
    if let Some(existing) = out.iter_mut().find(|o| dup(&w, o)) {
        if w.shape.len() > existing.shape.len() {
            *existing = w;
        }
        return;
    }
    out.push(w);
}

// Measures `push_remove_duplicates`'s own cost separately from pg-fst's `Transduce` dedup: both are
// linear scans over candidate lists that can explode on Optional-flooded shapes, and only a split
// measurement says which dominates. A permanent diagnostic — near-zero cost when unread.
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

/// C# `HermitCrabExtensions.Duplicates`: two shapes duplicate each other iff their **non-Optional**
/// nodes, in order, carry an identical feature structure. That structure has two dimensions this
/// port stores separately — the phonological features and type, which are the node's lanes, and
/// `StrRep`, which is the node's effective char-def-set.
///
/// `StrRep` is load-bearing, not belt-and-braces. On a zero-phonological-feature grammar, and on
/// every boundary of every grammar, it is the node's ONLY identity: comparing lanes alone treats
/// any two same-length candidate sequences as duplicates and longer-wins-collapses genuinely
/// distinct analyses. It compares as a value SET, and a natural-class-inserted node carries the
/// member union, which is why `Singleton(x)` must equal `Members({x})`.
///
/// **Deliberate residual, finer than C# in one direction only.** On feature-bearing grammars C#'s
/// segment FS carries no `StrRep` at all, so two same-lane nodes with different char-defs compare
/// EQUAL there and unequal here. Being finer only keeps extra candidates C# would prune, and C#
/// itself documents this dedup as a search-space optimization rather than a correctness step.
/// Being coarser would delete real analyses, so err in this direction if you touch it.
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

/// Set-equality over `EffectiveCdSet` — `shape_duplicates`'s `StrRep` dimension. `Unrestricted`
/// equals only `Unrestricted`: a node whose FS carries `StrRep` and one whose FS does not are
/// different feature structures.
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

// =================================================================================================
// Compounding — synthesis.
// =================================================================================================

fn synth_compound(g: &Grammar, word: &Word, rule: &CompoundingRuleDef) -> Vec<Word> {
    let Some(nh) = word.current_non_head().cloned() else {
        return Vec::new();
    };
    // Gating.
    if !is_unifiable(g.fs_interner.get(rule.non_head_required_syn_fs), &nh.syn_fs) {
        return Vec::new();
    }
    let Some(new_syn) = synth_syn_fs(g, rule.head_required_syn_fs, rule.out_syn_fs, word) else {
        return Vec::new();
    };
    if matches!(word.flags.is_last_applied_rule_final, Some(true)) && !word.flags.is_partial {
        return Vec::new();
    }
    if !rule.head_prod_restrictions_mpr.compound_match(word.mpr) {
        return Vec::new();
    }

    // A compounding rule has no `MorphemeId` to resolve through, and this uncached entry point has
    // no `MRuleId` in scope either, so the table is resolved by the rule's own xml id.
    let table = crate::cache::owning_table_for_compounding_rule(g, rule).unwrap_or(TableId(0));
    let (head_segs, head_node_of) = segs_of(g, table, &word.shape, true);
    let (nh_segs, nh_node_of) = segs_of(g, table, &nh.shape, true);
    let mut output = Vec::new();
    for sr in &rule.subrules {
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
        if let Ok(w) = synth_compound_subrule(
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
        ) {
            output.push(w);
            break; // C# breaks after the first matching subrule
        }
    }
    output
}

/// `crate::cache::RuleCache`-aware sibling of `synth_compound`. Gate ORDER differs from C#'s — the
/// two syntactic-FS gates run before the partial-template one — which is harmless because every
/// gate is independent and boolean: only the reported reason changes when two would fail at once.
///
/// `HeadProdRestrictMprFeatures` alone routes through `compounding_rule_not_applied`; every other
/// gate here, including the loop's, uses the generic event. That is C#'s own split — it uses the
/// compounding-specific event at exactly one site.
#[allow(clippy::too_many_arguments)]
fn synth_compound_cached(
    g: &Grammar,
    word: &Word,
    rule: &CompoundingRuleDef,
    mrid: MRuleId,
    cache: &crate::cache::RuleCache,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    let Some(nh) = word.current_non_head().cloned() else {
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
        return Vec::new();
    }

    let cc = cache.compound(mrid);
    let table = crate::cache::owning_table_for_mrule(g, mrid).unwrap_or(TableId(0));
    let (head_segs, head_node_of) = segs_of(g, table, &word.shape, true);
    let (nh_segs, nh_node_of) = segs_of(g, table, &nh.shape, true);
    let mut output = Vec::new();
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
        match synth_compound_subrule(
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
        ) {
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
    output
}

/// Returns which side's pattern failed rather than a bare `None`. The head is tried first, so the
/// non-head is only attempted — and `NonHeadPattern` only reported — once the head has matched.
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
    // The consumed non-head is deliberately NOT popped off `w.non_heads`, matching C#: only
    // confirmation moves the index backward, leaving the entry behind as history. `WordKey`'s
    // recursion needs that history — two compounds built from surface-homophone but distinct
    // non-head entries are otherwise indistinguishable once the shared shape is synthesized.
    //
    // Safe for any number of accumulated non-heads because `Word::current_non_head` is
    // index-based, not `last()`, so a stale consumed entry never shadows the one the index points
    // at. Do not "simplify" it back to `last()`: generation seeds can push several non-heads,
    // bypassing analysis and its `max_stem_count` gate entirely.
    w.flags.is_last_applied_rule_final = None;
    Ok(w)
}

// =================================================================================================
// Compounding — analysis.
// =================================================================================================

/// The combined head+non-head part list an `ana_compound` subrule matches against (head parts
/// named `h{i}`, non-head parts named `n{i}`, concatenated — the analysis LHS spans both).
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

/// Build the analysis LHS and its compiled FST for one compounding subrule — `build_ana_affix_lhs`'s
/// counterpart, and cached once per (rule, subrule) pair for the same reason.
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

fn ana_compound(
    g: &Grammar,
    word: &Word,
    rule: &CompoundingRuleDef,
    root_filter: Option<NonHeadRootFilter>,
) -> Vec<Word> {
    // Same guard and adjust as `ana_affix`; the head-required/out pair plays the affix rule's
    // required/out role exactly. See `ana_syn_fs`.
    let Some(new_syn) = ana_syn_fs(g, rule.head_required_syn_fs, rule.out_syn_fs, word) else {
        return Vec::new();
    };
    let table = crate::cache::owning_table_for_compounding_rule(g, rule).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    let mut output = Vec::new();
    for sr in &rule.subrules {
        let Ok((fst, lhs)) = build_ana_compound_lhs(g, table, sr) else {
            continue;
        };
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
    }
    output
}

/// `crate::cache::RuleCache`-aware sibling of `ana_compound`.
fn ana_compound_cached(
    g: &Grammar,
    word: &Word,
    rule: &CompoundingRuleDef,
    mrid: MRuleId,
    cache: &crate::cache::RuleCache,
    root_filter: Option<NonHeadRootFilter>,
) -> Vec<Word> {
    let Some(new_syn) = ana_syn_fs(g, rule.head_required_syn_fs, rule.out_syn_fs, word) else {
        return Vec::new();
    };
    let table = crate::cache::owning_table_for_mrule(g, mrid).unwrap_or(TableId(0));
    let (segs, node_of) = segs_of(g, table, &word.shape, false);
    let cc = cache.compound(mrid);
    let mut output = Vec::new();
    for (i, sr) in rule.subrules.iter().enumerate() {
        let Some((fst, lhs)) = cc.subrules[i].ana.as_ref() else {
            continue;
        };
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
    }
    output
}

/// One subrule's analysis-side match, head and non-head `GenerateShape`, dedup, and non-head
/// root-allomorph resolution — all in one per-subrule scope, as C# does. The dedup scope resets per
/// subrule: a candidate from one subrule must never suppress an identical-looking one from another.
///
/// With `root_filter` `None` each match yields one raw, unresolved split. With `Some`, each split
/// multiplies into **one candidate per surviving root allomorph**, the non-head's shape, syntactic
/// FS, MPR, root allomorph and morph record all replaced by the matched entry's canonical values. A
/// split whose non-head matches no root, or whose matches all fail the rule's non-head gates, is
/// discarded entirely — C# assumes it is not a valid analysis.
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
                // Must push the split-off non-head AND advance the index in lock-step:
                // `current_non_head()` is index-based, so a raw `non_heads.push` leaving the index
                // stale makes the just-split non-head invisible to `synth_compound`'s own gate.
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

/// C#'s root-allomorph search, gates, and pin: look up root allomorphs matching the just-split-off
/// non-head's raw shape, keep those whose entry unifies with the rule's non-head required syntactic
/// FS and satisfies its non-head prod restrictions, and build a resolved non-head `Word` per
/// survivor — shape re-segmented from the allomorph's own stored text, syntactic FS, MPR, partial
/// flag and stratum from the entry, root allomorph pinned, one order-0 morph record.
///
/// That resolution is what lets `attribute_morphs`'s non-head branch and `synth_compound`'s
/// non-head gate see real data rather than an empty FS and empty morph list. An empty result is
/// meaningful: it discards the whole split.
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

/// `push_remove_duplicates` extended to the (head, non-head) shape pair, for the lexicon-free path.
/// "Longer" is judged on the head shape alone, mirroring C#'s use of the head word's own count.
fn push_remove_duplicates_compound(out: &mut Vec<Word>, w: Word) {
    push_keep_longer(out, w, |a, b| {
        shape_duplicates(&a.shape, &b.shape)
            && shape_duplicates(
                &a.non_heads.last().unwrap().shape,
                &b.non_heads.last().unwrap().shape,
            )
    });
}

/// The root-allomorph-pinned sibling, used once the non-head is resolved. C#'s duplicate key is the
/// HEAD shape plus the *same pinned allomorph id*, not the non-head shape — two candidates pinned to
/// one allomorph already share a non-head shape by construction. "Longer" is again the head shape.
fn push_remove_duplicates_compound_pinned(out: &mut Vec<Word>, w: Word) {
    let allo = w.current_non_head().and_then(|nh| nh.root_allomorph);
    push_keep_longer(out, w, |a, b| {
        shape_duplicates(&a.shape, &b.shape)
            && b.current_non_head().and_then(|nh| nh.root_allomorph) == allo
    });
}

// =================================================================================================
// Compile-once cache (plan §13.2 step 5; `crate::cache::RuleCache`'s allomorph/compounding slices).
// =================================================================================================

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
