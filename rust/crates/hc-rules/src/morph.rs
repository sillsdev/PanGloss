//! Part 3 — morphological-rule application (plan §5.5, M4a): affix-process rules and compounding
//! rules, both directions (**synthesis** = apply, **analysis** = unapply).
//!
//! Ports `SIL.Machine.Morphology.HermitCrab/MorphologicalRules/` at the **rule level** (the full
//! `Morpher.ParseWord` cascade — strata, templates, lexical lookup — is M4b/M5). `rewrite.rs` (the
//! verified phonological engine) is the studied pattern; this module is its morphological sibling
//! and does not touch it.
//!
//! ## What each direction does
//! - **Synthesis** (`SynthesisAffixProcessRule` / `SynthesisCompoundingRule`): match the allomorph
//!   LHS parts against the input word's shape (anchored, per-part capture groups via the frozen
//!   `hc-fst`), then build a *new* shape by executing the RHS [`OutputAction`]s
//!   (`CopyFromInput`/`InsertSegments`/`ModifyFromInput`/`InsertSimpleContext`). Records the applied
//!   allomorph as a [`MorphRecord`] and priority-unions the rule's `out_syn_fs` onto the word.
//! - **Analysis** (`AnalysisAffixProcessRule` / `AnalysisCompoundingRule` via
//!   `AnalysisMorphologicalTransform`): build the *analysis LHS* by inverting the RHS actions
//!   (`AnalysisMorphologicalTransform` ctor: `CopyFromInput`→capture the part; `InsertSegments`→
//!   match-and-consume those segments; `ModifyFromInput`→capture the *modified* form and remember to
//!   underspecify it; `InsertSimpleContext`→match-and-consume one node). Matching is
//!   nondeterministic with all-submatches; `GenerateShape` re-emits the captured LHS parts (dropping
//!   the inserted material) — that is how an affix's material is "removed" on unapply.
//!
//! ## Faithful-but-bounded scope (flagged; the acceptance gate is Copy+InsertSegments on Sena)
//! - **`ModifyFromInput` analysis inversion** = set the changed feature lane(s) back to `full_mask`
//!   (underspecified). This is exactly what C# computes: `AntiFeatureStruct` negates the modify
//!   ctx's symbolic value (`SymbolicFeatureValue.Negation` → `_flags.Not()`), and `GenerateShape`
//!   does `node.FeatureStruct.Add(anti)` (`Union`), so `S ∪ ¬S = full mask`
//!   (`AnalysisMorphologicalTransform.cs:33` + `HermitCrabExtensions.AntiFeatureStruct`). Stale-claim
//!   correction (plan §W1.5): `ModifyFromInput` is **not** zero-occurrence — Amharic has 1
//!   occurrence (confirmed independently by this audit and the phase-1 audit); Indonesian/Sena have
//!   zero. The general nested/variable `AntiFeatureStruct` cases remain unexercised even on that one
//!   occurrence and are not ported beyond this lane-underspecify rule.
//! - **Reduplication** (`_nonAllomorphActions` / `ReduplicationHint`) **is now modeled**
//!   (`classify_redup`, plan §13.1 Tier-2 #8): a lone `CopyFromInput` per part is stem material and
//!   a lone `InsertSegments`/`InsertSimpleContext`/`ModifyFromInput` is new (affix) material, as
//!   before — the only change is when an `Input` part is referenced ≥2 times by Copy/Modify actions
//!   in one allomorph's RHS (a "true" reduplication subrule), where exactly one occurrence (per
//!   `ReduplicationHint`) stays attributed to the existing input morph and the rest become new affix
//!   material, mirroring `SynthesisAffixProcessAllomorphRuleSpec.cs:23-124`. Grammar census (verified
//!   by direct XML scan, not just the `redupMorphType` attribute's presence): exactly 3 subrules
//!   across all three reference grammars actually repeat a part — all Indonesian
//!   (`msubrule5`/`msubrule11`/`msubrule13`, the `-Cont`/`-Pl`/`REDUP-meN` families). Amharic's five
//!   `redupMorphType`-tagged subrules each reference their `Input` part exactly once, so C#'s own
//!   `redupParts.Count > 0` gate never fires for them either — the attribute is present but inert on
//!   that grammar, both in C# and here.
//! - **`blockable` / `RequiredStemName` / free-fluctuation** are not gated: blocking needs the M5
//!   lexicon; stem names lint unsupported. `NonFinal`/partial gating that *is* computable from
//!   [`WordFlags`] is applied. **`max_apps`** *is* gated, but — like the compounding root-allomorph
//!   search noted just below — one layer up, in
//!   [`crate::stratum::StratumAnalyzer::apply_one_mrule`] (the `Word::unapplied_rule_counts`
//!   multiset built for the M6 memo key doubles as this gate's input), not inside this module: this
//!   module stays a pure semantics function of `(Grammar, Word, MorphRuleDef)` with no per-candidate
//!   history to consult.
//! - **MPR gating** uses plain set overlap/subset (no `MprFeatureGroup` All/Any resolution) and MPR
//!   output is a union (no `Overwrite` groups); the reference grammars don't exercise groups here.
//! - **Compounding analysis** produces the head/non-head split; the module itself still does
//!   **not** run the C# root allomorph search over the non-head (`SearchRootAllomorphs`) — this
//!   module stays free of the lexicon dependency. That gate is now closed one layer up, in
//!   [`crate::stratum::StratumAnalyzer::apply_one_mrule`] (M5c: `non_head_root_matches` +
//!   [`crate::stratum::NonHeadRootFilter`]), wired from `hc-parse::Morpher` (which owns the
//!   `RootAllomorphIndex`). Every output this module returns for a `Compounding` rule still needs
//!   that filter applied by the caller — a bare call to [`analyze`] here does not prune anything.
//! - **`GetSkippedOptionalNodes`** (optional nodes just left of a captured range folded into a copy)
//!   **is now modeled** (P10): [`copy_part`] folds a word-initial run of optional (boundary) nodes
//!   into the copied range, mirroring `MorphologicalOutputAction.cs:41-55` — first exercised by a
//!   zero allomorph's `^0+` insertion feeding a later prefix rule's stem copy
//!   (`rust/conformance/allomorphy/strrep-identity/`).
//! - **Copy-only affixes** (a purely feature/syntactic affix whose RHS is only `CopyFromInput`, so
//!   no `Origin::Affix` node exists) **now** record their morpheme (wave-4, W9.1, history row
//!   `dfbb754b`/#264/LT-21939) — `attribute_morphs` mirrors C#'s `outputNewMorph == null` fallback
//!   (`SynthesisAffixProcessAllomorphRuleSpec.ApplyRhs`, marks the allomorph on the last output
//!   node) by minting a synthetic tail-ordered [`MorphRecord`] when no `Origin::Affix` position was
//!   produced. Fixture: `rust/conformance/affix-shapes/truncate/`. Zero occurrences in Sena's 132
//!   affix rules (all Copy + InsertSegments), so the reference corpora never exercised this path
//!   before the fixture; the fix is otherwise unconditional (any grammar, not corpus-gated).

use hc_featstruct::{add, is_unifiable, priority_union, unify, FeatureStruct};
use hc_fst::{CompileInput, CompileNode, Direction, Fst, FstResult, Segment, Transduce};
use hc_grammar::chardef::CharDefId;
use hc_grammar::featsys::FlatIndex;
use hc_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AllomorphId, AllomorphOwner, CompoundingRuleDef,
    CompoundingSubruleDef, Grammar, LexEntryId, MRuleId, MorphRuleDef, MorphemeId,
    NaturalClassKind, OutputAction, PartRef, Pattern, PatternNode, RealizationalRuleDef,
    ReduplicationHint, SimpleContext, StratumId, TableId,
};
use hc_shape::{CdBits, CdSet, EffectiveCdSet, NodeKind, Shape, ShapeBuilder, NO_CHAR_DEF};

use crate::bridge::{BridgeError, PatternBridge};
use crate::stratum::NonHeadRootFilter;
use crate::trace::{FailureReason, TraceHandle, TraceSink};
use crate::word::{MorphRecord, MorphStatus, Word};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

/// Phonological rules default to `TableId(0)` in every reference grammar; morphological rules
/// resolve char-defs/natural-classes against the same table.
const TABLE: TableId = TableId(0);

// =================================================================================================
// Public API (the surface M4b/M5 compose over).
// =================================================================================================

/// Apply `rule` forward to `word` (synthesis). Returns the resulting word(s); empty if the rule
/// does not apply (gating fails or no allomorph matches), mirroring C# `Apply` returning
/// `Enumerable.Empty`.
///
/// Recompiles every allomorph/subrule LHS FST on every call — kept as-is (not cached) because this
/// function is also called directly on standalone, non-grammar-resident rule fixtures throughout
/// the test suite (e.g. `hc-parse/tests/cd_set_gate.rs`), which have no stable index into a
/// [`crate::cache::RuleCache`]. The real per-word pipeline (`crate::stratum`) calls
/// [`synthesize_cached`] instead. See `crate::cache`'s module doc for the full rationale.
pub fn synthesize(g: &Grammar, word: &Word, rule: &MorphRuleDef) -> Vec<Word> {
    let out = match rule {
        MorphRuleDef::AffixProcess(def) => synth_affix(g, word, def),
        MorphRuleDef::Compounding(def) => synth_compound(g, word, def),
        MorphRuleDef::Realizational(def) => synth_realizational(g, word, def),
    };
    apply_blocking(g, out, rule.blockable())
}

/// The [`crate::cache::RuleCache`]-aware sibling of [`synthesize`], used by the real per-word
/// pipeline. `mrid` must identify `rule` (`rule as *const _ == &g.mrules[mrid.0 as usize] as *const
/// _`) — every production call site already has both in hand.
///
/// P12 chunk 4: this is now ALWAYS the traced form (`trace`/`parent` params) — unlike this crate's
/// other `*_traced` siblings (`allomorphs_valid_cached_traced`, `synthesize_stratum_traced`), there
/// is no separate untraced `synthesize_cached` wrapper: its only call site (`guided_synth`) already
/// carries a `trace`/`parent` pair unconditionally (threaded from `hc-parse::Morpher::parse_word`'s
/// entry, defaulting to [`crate::trace::NoopSink`]/[`TraceHandle::DUMMY`] on the untraced path), so
/// a second thin wrapper here would be dead code with no caller. Pass `&NoopSink`/`TraceHandle::DUMMY`
/// directly for an untraced call, exactly as `guided_synth`'s own untraced callers already do one
/// level up.
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

/// `RequiredMprFeatures`/`ExcludedMprFeatures` (§3.2): `g.mpr_group_ok` folds both C# gates into one
/// bool; this reports which of the two actually failed, matching C#'s two separate `if` checks
/// (e.g. `SynthesisAffixProcessRule.cs:145-172`) in the same required-then-excluded order.
fn mpr_gate_reason(
    g: &Grammar,
    required: hc_grammar::model::MprSet,
    excluded: hc_grammar::model::MprSet,
    have: hc_grammar::model::MprSet,
) -> Option<FailureReason> {
    if !hc_grammar::model::mpr_required_ok(&g.mpr_groups, required, have) {
        return Some(FailureReason::RequiredMprFeatures);
    }
    if !hc_grammar::model::mpr_excluded_ok(&g.mpr_groups, excluded, have) {
        return Some(FailureReason::ExcludedMprFeatures);
    }
    None
}

/// Un-apply `rule` to `word` (analysis). Returns the un-applied word(s); empty if the rule cannot
/// be un-applied.
///
/// Recompiles on every call — see [`synthesize`]'s doc for why. The real pipeline calls
/// [`analyze_cached`].
pub fn analyze(g: &Grammar, word: &Word, rule: &MorphRuleDef) -> Vec<Word> {
    match rule {
        MorphRuleDef::AffixProcess(def) => ana_affix(g, word, def),
        MorphRuleDef::Compounding(def) => ana_compound(g, word, def, None),
        MorphRuleDef::Realizational(def) => ana_realizational(g, word, def),
    }
}

/// The [`crate::cache::RuleCache`]-aware sibling of [`analyze`]. See [`synthesize_cached`]'s doc
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

/// [`analyze_cached`]'s sibling for the one production call site that also has the M5c non-head
/// lexicon filter in hand (`crate::stratum::StratumAnalyzer::apply_one_mrule`, `Compounding` rules
/// only — an `AffixProcess` rule never consumes a filter, so callers route those through
/// [`analyze_cached`] instead). Threading the filter in here (rather than post-filtering the
/// returned `Vec<Word>`, as M5c originally did) is what lets the root-allomorph resolution join the
/// **per-subrule** duplicate-elimination scope C# uses (`AnalysisCompoundingRule.cs:99-117`) instead
/// of a coarser whole-rule scope — see [`ana_compound_subrule`]'s doc.
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

/// Uncached sibling of [`analyze_cached_with_root_filter`] (the `cache: None` production fallback —
/// see [`analyze`]'s doc for why that path still exists).
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
// Lexical-family blocking (W5) — `Word.CheckBlocking` / the `ChooseInflectionalStem` seed helper.
// =================================================================================================

/// `Word.CheckBlocking` (Word.cs:605-630): if the word's root morpheme belongs to a `<Family>`,
/// search the family's OTHER entries (document order) for one in the SAME stratum whose own
/// lexical syntactic FS is subsumed by this word's accumulated syntactic FS; the first match wins
/// and the word is replaced by a fresh root-level word seeded from that entry's primary allomorph
/// (`new Word(entry.PrimaryAllomorph, RealizationalFeatureStruct.Clone())`), discarding every rule
/// applied so far. `None` if not blocked (no family, or no matching relative). Compounding output
/// words carry the HEAD's root allomorph forward (`SynthesisCompoundingRule.cs`'s `ApplySubrule`
/// clones the head match, `AllomorphOwner::Root` by construction — see [`crate::cache`]'s doc for
/// why `AllomorphOwner::Affix` never appears there), so this needs no rule-kind branch of its own.
///
/// P11 correction (§4.4-1's audit claimed this site "unreachable for a guessed root, which is
/// never a lexicon entry" — confirmed WRONG empirically: `apply_blocking` runs on the OUTPUT of
/// any successfully-applied blockable rule, including one applied on top of a guessed root, whose
/// `root_allomorph` stays `Some(AllomorphId::GUESSED)` — `g.allomorph_owners[u32::MAX]` panics
/// without this guard, reproduced by `AnalyzeWord_CanGuess_ReturnsCorrectAnalysis`'s own "gagd"
/// case before this fix). A guessed root can never belong to a `<Family>` (that's real-lexicon-only
/// metadata `Word::guessed_root` doesn't carry), so `None` is the correct, faithful answer.
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
        if hc_featstruct::subsumes(&w.syn_fs, g.fs_interner.get(entry.syn_fs)) {
            return Some(seed_from_entry(g, other, w.real_fs.clone()));
        }
    }
    None
}

/// `apply_blocking`'s post-pass, run once for the whole `Vec<Word>` a `synthesize`/
/// `synthesize_cached` call produces rather than duplicated inline in each of `synth_affix`/
/// `synth_compound`/`synth_realizational`'s per-allomorph loops (C#'s three call sites:
/// `SynthesisAffixProcessRule.cs:204`, `SynthesisCompoundingRule.cs:198`,
/// `SynthesisRealizationalAffixProcessRule.cs:134`). Observably equivalent: blocking only
/// substitutes one already-produced word for another and never feeds back into whether a LATER
/// allomorph in the SAME rule application gets tried — that loop-continuation condition
/// (environments / required-syntactic-FS / free-fluctuation with the next allomorph) is entirely
/// allomorph-static, never a function of the word `CheckBlocking` just replaced.
fn apply_blocking(g: &Grammar, words: Vec<Word>, blockable: bool) -> Vec<Word> {
    if !blockable {
        return words;
    }
    words
        .into_iter()
        .map(|w| check_blocking(g, &w).unwrap_or(w))
        .collect()
}

/// [`apply_blocking`]'s traced sibling (`synthesize_cached_traced`'s only caller). C# fires
/// `Blocked(rule, newWord)` (a sibling trace node, still under the ambient `parent` — C#'s
/// `output.CurrentTrace` at that point has not yet been reassigned to the rule's own `Applied` node)
/// BEFORE `MorphologicalRuleApplied` reassigns the cursor to the (possibly blocked-replaced) output
/// word. This port's `synth_*_cached` functions already emit `Applied` per successful allomorph
/// (setting `w.trace`) before this post-pass runs, so — unlike C# — the `Applied` node here is
/// already minted using the PRE-block word as its `Output` snapshot; `Blocked`'s own snapshot is
/// still the correct post-block word, and the replacement inherits the pre-block word's already-
/// minted `.trace` handle (the cursor a later event should nest under stays the `Applied` node
/// either way, matching C#'s final `outWord.CurrentTrace = trace` using the post-block `outWord`
/// variable). Flagged, not silently smoothed: this is an accepted approximation of C#'s exact
/// Blocked-then-Applied interleaving, a consequence of Rust's blocking-as-separate-post-pass
/// architecture (see this fn's non-traced sibling's own doc) — `Blocking` is corpus-rare, so the
/// node-count/reason fidelity (not the exact emission order) is what matters for chunk 9's diff.
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
/// FeatureStruct)` ctor, as called by both `Word.CheckBlocking` (Word.cs:620) and
/// `SynthesisAffixTemplatesRule.ChooseInflectionalStem` (cs:103). `le`'s primary allomorph is
/// `Allomorphs[0]` (C# `LexEntry.PrimaryAllomorph`, LexEntry.cs:57-64 — the loader never reorders
/// `Vec<RootAllomorphDef>`, so index 0 is exactly that). Every field starts fresh (fresh shape/
/// stratum/syn FS/MPR/partial-flag from the entry, a single root [`MorphRecord`] at order 0, no
/// rule-application history) except the realizational FS, which the caller supplies (both call
/// sites pass `Clone()` of the CURRENT word's `real_fs`, not the new entry's — there is no such
/// concept on a bare `LexEntry`).
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

/// Driver lanes for a char-def (width `W`, `full_mask` for unmentioned/boundary lanes).
fn cd_lanes(g: &Grammar, cd_raw: u32) -> Vec<u64> {
    if cd_raw == NO_CHAR_DEF {
        return full_lanes(g);
    }
    let t = &g.char_tables[TABLE.0 as usize];
    fit(g, t.get(CharDefId(cd_raw)).feature_lanes())
}

/// The `(feature, symbol-bits)` a `SimpleContext` pins (alpha-variable features left unconstrained).
fn ctx_pins(g: &Grammar, ctx: &SimpleContext) -> Vec<(usize, u64)> {
    let w = feat_width(g);
    let t = &g.char_tables[TABLE.0 as usize];
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
fn ctx_lanes(g: &Grammar, ctx: &SimpleContext) -> Vec<u64> {
    let mut lanes = full_lanes(g);
    for (f, bits) in ctx_pins(g, ctx) {
        lanes[f] = bits;
    }
    lanes
}

/// The char-def-set a `SimpleContext`'s natural class carries (plan §13.1 Tier-1 #3 — the port's
/// `StrRep` analog for `InsertSimpleContext`, C# `CharacterDefinitionTable.Add`'s `fs==null` branch
/// cs:68-76): a `Segments`-kind class is exactly its explicit member list; a `Feature`-kind class is
/// the set of char-defs whose lanes unify with the pins `ctx_pins` computes (alpha-variable-governed
/// features already excluded there), computed once per call rather than cached per node — these
/// tables are small (≤418 entries) and this only runs per rule application, not per shape. Falls back
/// to [`CdSet::Unrestricted`] when that set is the whole table (a class that really does mean "any
/// segment"), avoiding materializing a full-table bitset for that common case.
fn ctx_cd_set(g: &Grammar, ctx: &SimpleContext) -> CdSet {
    let nc = &g.natural_classes[ctx.nat_class.0 as usize];
    match &nc.kind {
        NaturalClassKind::Segments(segs) => {
            CdSet::Members(CdBits::from_ids(segs.iter().map(|cd| cd.0)))
        }
        NaturalClassKind::Feature(_) => {
            let pins = ctx_pins(g, ctx);
            if pins.is_empty() {
                // Nothing is actually pinned (e.g. every feature is alpha-variable-governed) -- the
                // class matches every segment, same as an all-unconstrained lane row.
                return CdSet::Unrestricted;
            }
            let t = &g.char_tables[TABLE.0 as usize];
            let mut members = Vec::new();
            let mut all = true;
            for (id, cd) in t.iter() {
                if cd.kind() != hc_grammar::chardef::CharDefKind::Segment {
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

/// The owned [`CdSet`] to carry onto a new [`OutNode`] copying an existing shape node `p` (plan
/// §13.1 Tier-1 #3: "feature-modified copies of an existing node carry forward the source node's
/// set unchanged"). For a concrete source (`char_def != NO_CHAR_DEF`) this is `Unrestricted` —
/// harmless, since the copy keeps that same real `char_def` and `Shape::node_cd_set` will derive
/// the singleton from it directly, never consulting this field. Only a source that was itself
/// `NO_CHAR_DEF` (an earlier rule's class insertion, now being copied/modified by a later rule)
/// needs its real membership set propagated here.
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
/// node index` map. Synthesis includes boundaries (as optional segments, matching C#
/// `Filter = Segment|Boundary`); analysis is `Segment`-only.
///
/// A `Segment`-kind node whose own `Optional` flag is set (C# `Annotation.Optional`) must also be
/// passed through as `Segment::optional`, not just boundaries: phonological analysis marks
/// re-inserted deleted material (`rewrite::ana_narrow`) and unapplied epenthesis
/// (`rewrite::ana_epenthesis`) Optional specifically so that *later* matching — here, a
/// morphological rule's own analysis LHS pattern, e.g. an affix-insertion rule un-matching the
/// inserted affix material plus the (now Optional-bearing) stem — can explore both "present" and
/// "skipped" as alternative paths (`hc_fst::traverse`'s `Advance`, traverse.rs:277, spawns the
/// skip-path exactly when `Segment.optional` is set). Before this fix every `Segment`-kind node
/// was passed as non-optional regardless of its shape flag, silently discarding that "may be
/// absent" signal and forcing every candidate segment — including ones a prior phonological rule
/// could not determine were actually present — to be treated as mandatory, breaking analysis of
/// e.g. Indonesian's `meN-` prefix (whose nasal-assimilation/deletion phonological rules leave
/// Optional candidate segments that the prefix's own un-insertion rule must be able to skip).
pub(crate) fn segs_of(
    g: &Grammar,
    shape: &Shape,
    include_boundaries: bool,
) -> (Vec<Segment>, Vec<usize>) {
    // P10 `StrRep` identity lane (see `PatternBridge::id_lane`): every input node carries its
    // char-def identity as a membership bitset at index `id_width` — a concrete node is a
    // singleton, a class-born (`NO_CHAR_DEF`) node its stored `CdSet`. `Unrestricted` nodes (and
    // >64-def tables, where `id_lane_width` is `None`) omit the lane entirely: absent = all-ones,
    // exactly C#'s "no `StrRep` value on this FS" (an underspecified node matches any identity).
    let id_width = crate::bridge::id_lane_width(g, TABLE);
    let id_bits = |i: usize| -> Option<u64> {
        match shape.node_cd_set(i) {
            hc_shape::EffectiveCdSet::Singleton(cd) => Some(1u64 << cd),
            hc_shape::EffectiveCdSet::Members(b) => b.as_u64(),
            hc_shape::EffectiveCdSet::Unrestricted => None,
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
    /// Char-def-set identity (plan §13.1 Tier-1 #3), consulted only when `char_def == NO_CHAR_DEF`.
    /// Every producer that copies/keeps a real `char_def` defaults this to `Unrestricted` — it is
    /// never read for those nodes (`Shape::node_cd_set` derives their singleton from `char_def`
    /// itself). Only `InsertSimpleContext`-originated nodes set a real [`CdSet`].
    cd_set: CdSet,
}

/// Freeze interior [`OutNode`]s into a bracketed [`Shape`]. Optional segments use the
/// delete-then-reinsert workaround (as `rewrite.rs`), since `ShapeBuilder` has no set-flags-in-place.
fn freeze_out(g: &Grammar, nodes: &[OutNode]) -> Shape {
    let w = feat_width(g) as u32;
    let mut b = ShapeBuilder::with_features_capacity(w, nodes.len());
    for n in nodes {
        let lanes = fit(g, &n.lanes);
        match n.kind {
            // NO_CHAR_DEF segments (class insertions) carry their real cd_set (plan §13.1 Tier-1
            // #3); concrete segments keep the existing plain path (their own char_def already is
            // the identity -- `n.cd_set` is `Unrestricted` and never consulted for them).
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
                    hc_shape::NodeFlags(hc_shape::NodeFlags::OPTIONAL),
                    &lanes,
                    n.cd_set.clone(),
                );
            } else {
                m.insert(
                    idx,
                    NodeKind::Segment,
                    n.char_def,
                    hc_shape::NodeFlags(hc_shape::NodeFlags::OPTIONAL),
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
/// `crate::cache::RuleCache::build` can call it once per allomorph/subrule at cache-construction
/// time instead of leaving it to be recompiled on every application.
pub(crate) fn compile_parts(
    g: &Grammar,
    parts: &[Pattern],
    prefix: &str,
    deterministic: bool,
) -> Result<(Fst, Vec<String>), BridgeError> {
    // P10: morphological-LHS FSTs carry the `StrRep` identity lane (see `PatternBridge::id_lane`);
    // their inputs all come from [`segs_of`], which emits the same lane.
    let bridge = PatternBridge::new(g)
        .with_table(TABLE)
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

/// Which input morph owns a source interior node `idx` (contiguous partition of `word.morphs` by
/// ascending `order` — exact for concatenative morphology; see module scope note). Only
/// [`MorphStatus::Real`] records own nodes: `Floating`/`SubsumedChild`/`SubsumedFirst` records are
/// markers riding at (or sharing) a position, never material owners (wave-4; and `Real` records
/// never tie on `order` — each owns a distinct leftmost node — so `max_by_key` is unambiguous).
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

/// Build the output word's [`MorphRecord`]s from the constructed output nodes: existing morphs are
/// remapped to where their copied material landed (keeping their `passed_over` sets — C#'s
/// `_disjunctiveAllomorphIndices` dictionary rides along in `Word`'s copy constructor); new affix
/// material becomes records whose `passed_over` is `affix`'s recorded passed-over-index set
/// (W3.2 — see [`MorphRecord::passed_over`]).
///
/// W3.3 (`97fa7721`, "do not allow non-contiguous morph annotations"): one record per **contiguous
/// run** of a morph's output positions, mirroring C# `MarkMorphs`' split (`SynthesisAffixProcess
/// AllomorphRuleSpec.cs:237-259`, `nodes[i].Next != nodes[i+1]` ⇒ new annotation) — a circumfix's
/// two pieces, or a root split by an infix, are separate records sharing allomorph/morpheme/
/// passed-over. This is what makes `crate::validity`'s span derivation (`[order_i, order_{i+1}-1]`)
/// exact for discontinuous morphs: each run is checked at its own span, exactly C#'s
/// per-annotation `word.GetMorphs(allomorph)` loop. (C#'s "longest run is the primary" clause only
/// feeds `MarkSubsumedMorph` attachment, which has no port yet — history row `a5a8239f`.)
fn attribute_morphs(
    out: &[OutNode],
    head: &Word,
    non_head: Option<&Word>,
    affix: Option<(
        hc_grammar::model::AllomorphId,
        hc_grammar::model::MorphemeId,
        &[u16],
    )>,
) -> Vec<MorphRecord> {
    // ---- Pass 1: output positions per input morph / affix (Real records only, via owning_morph).
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

    // Contiguous runs per key: `(order, len)` pairs (W3.3, `97fa7721` — one record per run,
    // mirroring C# `MarkMorphs`' `nodes[i].Next != nodes[i+1]` split; a circumfix's two pieces or
    // a root split by an infix are separate records sharing allomorph/morpheme/passed-over, which
    // is what keeps `crate::validity`'s span derivation exact for discontinuous morphs).
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
    // The affix's "primary" annotation = its longest run, FIRST longest at ties (C# `MarkMorphs`'
    // strict `>` comparison, cs:253) — the attachment point (`outputNewMorph`) for subsumption.
    let affix_host_order: Option<u32> = key_runs.get(&MorphKey::Affix).map(|rs| {
        let mut best = rs[0];
        for &r in &rs[1..] {
            if r.1 > best.1 {
                best = r;
            }
        }
        best.0
    });
    // A `Real` input record's longest run (same first-longest rule), for host-following.
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

    // ---- Pass 2: build the output records, walking the input words' record vecs IN ORDER (the
    // analog of C#'s `foreach inputMorph in match.Input.Morphs` — `Word.morphs` is kept sorted by
    // `order` with the tie-order invariants below, matching the annotation tree's traversal order).
    //
    // Wave-4 unified fallback model (C# `SynthesisAffixProcessAllomorphRuleSpec.ApplyRhs`,
    // cs:137-207; fixture `rust/conformance/affix-shapes/truncate/` + the `subsumed_affix` C# port):
    //
    // * A [`MorphStatus::Real`] record with runs is a normal positioned morph (`MarkMorphs`).
    // * A `Real` record with NO runs this hop (a later rule deleted all of its material — e.g.
    //   `s_suffix` capturing-but-not-copying the `u` that was 3SG's whole realization) is C#'s
    //   `else if (inputMorph.Parent == null && !markedAllomorphs.Contains(...))` branch:
    //   - rule inserted new material (`outputNewMorph != null`) ⇒ `MarkSubsumedMorph`: the record
    //     becomes [`MorphStatus::SubsumedChild`] at the affix's longest-run order, pushed BEFORE
    //     the affix runs (C#'s postorder `Word.Morphs` traversal renders a subsumed child before
    //     its host — "47 3SG PAST", not "47 PAST 3SG").
    //   - pure truncation (`outputNewMorph == null`) ⇒ `MarkMorph(Shape.First)`: the record becomes
    //     [`MorphStatus::SubsumedFirst`] at order 0, pushed AFTER the node-0 owner's records (C#'s
    //     interval sort renders the longer/containing annotation first — "47 3SG PRES").
    // * A `SubsumedChild`/`SubsumedFirst` record never owns nodes; each hop it re-anchors to its
    //   HOST — the unique `Real` record sharing its `order` (unique because `Real` orders are
    //   distinct leftmost positions, and subsumption only ever mints order-sharing markers):
    //   - host has runs ⇒ follow to the host's longest run's order (C# `MarkSubsumedMorphs`
    //     re-attaching children under the host's new primary annotation), keeping the
    //     before/after-host placement its variant dictates;
    //   - host also dropped ⇒ ride the host's own fallback: with new material both subsume onto it
    //     (C# recursion), as `SubsumedChild`s; on pure truncation C#'s `MarkMorph(Shape.First)`
    //     branch does NOT recurse into children — a `SubsumedChild` is dropped (bug-compatible)
    //     while a `SubsumedFirst` re-anchors at 0 (it was a top-level annotation in C#).
    // * A [`MorphStatus::Floating`] record (a previous pure-truncation rule's own marker,
    //   W9.1/`dfbb754b`) rides at the `FLOATING_ORDER` sentinel until a hop with new material
    //   resolves it to a `SubsumedChild` of that hop's affix run.
    // * `markedAllomorphs` (cs:175,185,206): a no-run record whose allomorph was already recorded
    //   earlier this hop is skipped entirely (first occurrence wins).
    //
    // Flat-model approximation, flagged: C#'s fallback annotations own the actual `Shape.First`/
    // `Shape.Last` NODE (stealing it from the morph that had it), so a subsequent hop can attribute
    // that one node to the marker while the rest of the run stays with the original morph. The
    // flat `order`-partition model cannot split ownership inside one run, so markers here never
    // own nodes and re-anchor by host-following instead. Observable difference: none in any ported
    // test/fixture (the signature's per-allomorph count and order are preserved); the divergence
    // would need a grammar that *further affixes onto* the stolen node's position and renders
    // per-node morph attribution, which no conformance surface does today.
    //
    // Compounding (`affix: None`) has none of this: C# `SynthesisCompoundingRule.ApplySubrule` has
    // no fallback branches at all — an input morph with no copied material is simply dropped.
    let mut records: Vec<MorphRecord> = Vec::new();
    let mut marked: Vec<hc_grammar::model::AllomorphId> = Vec::new();

    let push_runs = |records: &mut Vec<MorphRecord>, key: &MorphKey, m: &MorphRecord| {
        for &(order, _) in &key_runs[key] {
            records.push(MorphRecord {
                allomorph: m.allomorph,
                morpheme: m.morpheme,
                order,
                passed_over: m.passed_over.clone(),
                status: MorphStatus::Real,
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

    // Floating markers (W9.1): ride, resolve onto this hop's new material, and/or mint this rule's
    // own (see the doc block above). Pushed after the subsumed-input records and before the affix
    // runs, approximating C#'s input-`Morphs`-order attachment.
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
            // Pure truncation: mint this rule's own floating marker (C# `MarkMorph(Shape.Last)`,
            // cs:168-174). Guarded: an entirely empty `out` has no `Shape.Last` for C# either —
            // unreachable for any real grammar, guarded rather than assumed.
            if !out.is_empty() {
                records.push(MorphRecord {
                    allomorph: a,
                    morpheme: mo,
                    order: FLOATING_ORDER,
                    passed_over: Some(p.into()),
                    status: MorphStatus::Floating,
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
                });
            }
        }
    }

    records.sort_by_key(|m| m.order);
    records
}

/// Sentinel `order` for a still-unresolved floating marker (see [`attribute_morphs`]): larger than
/// any real `out` position could ever be, so [`owning_morph`]'s `order <= idx` filter never selects
/// it, and it always sorts after every genuinely-positioned record in the word's own signature.
const FLOATING_ORDER: u32 = u32::MAX;

// =================================================================================================
// RHS execution (synthesis) — shared by affix and compounding.
// =================================================================================================

/// Resolve a [`PartRef`] to the matched source (segments + node map + captured range + origin tag).
struct PartSource<'a> {
    node_of: &'a [usize],
    shape: &'a Shape,
    range: Option<(usize, usize)>,
    head: bool, // true = Origin::Head, false = Origin::NonHead
}

/// Copy the captured nodes of `src` into `out`, tagging their origin. `force_origin` overrides the
/// default Copy/Modify-based origin choice below: `Some(true)` pins the origin to the *existing*
/// input morph (`Origin::Head`/`Origin::NonHead`) even for a `ModifyFromInput`; `Some(false)` pins it
/// to `Origin::Affix` (new material) even for a plain `CopyFromInput`. This is how reduplication's
/// `_nonAllomorphActions` classification (`classify_redup`, below) is threaded through: a repeated
/// `CopyFromInput` of the same LHS part is *not* uniformly "existing" the way a single occurrence is
/// (`SynthesisAffixProcessAllomorphRuleSpec.cs:137-159`).
fn copy_part(
    g: &Grammar,
    out: &mut Vec<OutNode>,
    src: &PartSource,
    modify: Option<&SimpleContext>,
    force_origin: Option<bool>,
) {
    let Some((s, e)) = src.range else { return };
    let pins = modify.map(|c| ctx_pins(g, c)).unwrap_or_default();
    // C# `GetSkippedOptionalNodes` (`MorphologicalOutputAction.cs:41-55`, called by both
    // `CopyFromInput.Apply` and `ModifyFromInput.Apply`): a run of Optional nodes immediately LEFT
    // of the captured range that extends all the way back to the left anchor is folded into the
    // copy, in surface order, through the same clone-and-maybe-modify body as the captured nodes
    // (the modify arm only touches `Segment`-typed nodes in C# too, which the `kind` guard below
    // already mirrors). Boundary annotations are always Optional in C#
    // (`CharacterDefinitionTable.cs:126`); Optional *segments* additionally arise from the
    // epenthesis/narrow analysis markers, hence the two-pronged predicate. First actually
    // exercised by P10's null-allomorph work: a zero allomorph's `^0+` insertion leaves
    // word-initial optional boundary nodes that the NEXT prefix rule's stem copy must carry
    // forward (oracle: `nd+^0+pat`, not `nd+pat` — see
    // `rust/conformance/allomorphy/strrep-identity/`).
    let mut positions: Vec<usize> = Vec::new();
    if s < e {
        let first_node = src.node_of[s];
        let skippable =
            |i: usize| src.shape.kind(i) == NodeKind::Boundary || src.shape.flags(i).is_optional();
        let mut i = first_node;
        while i > 0 && skippable(i - 1) {
            i -= 1;
        }
        // C#'s `node == shape.Begin` test: the walk must have stopped AT the left anchor
        // (index 0) for the fold to apply; stopping at any non-optional interior node folds
        // nothing at all.
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
                // Finding (this file's sibling test-port doc, `simulfix_rules`/
                // `modify_from_input_rules` in `csharp_port_affix_process.rs`): a `ModifyFromInput`
                // output node kept the SOURCE node's own `char_def`/`cd_set` forever, so
                // `hc_parse::surface::matching_str_reps`/`RootAllomorphIndex::search` would only
                // ever consider the pre-modification character's own representations — a modified
                // "p" always rendered/matched as "p", never "b", regardless of how the lanes above
                // changed. C# has no such literal-identity concept for this step at all:
                // `ModifyFromInput.Apply` (`ModifyFromInput.cs:56-80`) clones the input node then
                // `PriorityUnion`s the ctx's `FeatureStruct` onto it, and rendering always re-derives
                // candidate string representations from the CURRENT `FeatureStruct`
                // (`CharacterDefinitionTable.GetMatchingStrReps`'s per-call `IsUnifiable`) — no
                // identity lock. Mirrors `OutputAction::InsertContext`'s handling immediately below
                // in `synth_affix_allomorph`'s match arm: `NO_CHAR_DEF` + the ctx's own `cd_set`
                // (`ctx_cd_set`), refined by `matching_str_reps`'s `flat_unifiable(node_lanes, ..)`
                // gate against the FULL (pinned + retained) lanes above — a cd unifying with the
                // full lanes is always a subset of `ctx_cd_set`'s pins-only membership (pins is a
                // relaxation of the full constraint set), so this cannot under-restrict.
                char_def = NO_CHAR_DEF;
                cd_set = ctx_cd_set(g, ctx);
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
/// literal `char_def` (from an authored `<PhoneticShape>`), never `NO_CHAR_DEF`, so `cd_set`
/// defaults to `Unrestricted` (never consulted — `Shape::node_cd_set` derives the singleton from
/// `char_def` itself).
fn insert_segments(g: &Grammar, out: &mut Vec<OutNode>, seg_shape: &Shape, origin: Origin) {
    for (idx, kind, char_def, _flags) in seg_shape.interior() {
        let _ = idx;
        out.push(OutNode {
            kind,
            char_def,
            lanes: cd_lanes(g, char_def),
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
    req: hc_featstruct::FsId,
    out: hc_featstruct::FsId,
    word: &Word,
) -> Option<FeatureStruct> {
    let req_fs = g.fs_interner.get(req);
    if !is_unifiable(req_fs, &word.syn_fs) {
        return None;
    }
    let unified = unify(req_fs, &word.syn_fs)?;
    Some(priority_union(&unified, g.fs_interner.get(out)))
}

/// C# analysis guard + adjust (`AnalysisAffixProcessRule.cs:46-68`, `AnalysisCompoundingRule.cs:
/// 46-53,133-138`): `out.IsUnifiable(word.syn)` gate on the rule's *input*; on unapply, each
/// output word's syntactic FS starts equal to the input's (`AnalysisAffixProcessAllomorphRuleSpec
/// .ApplyRhs`: `output = match.Input.Clone()` never touches `SyntacticFeatureStruct`, so this is
/// the same value for every allomorph/subrule output and can be hoisted once here) and then, if
/// `required` is non-empty, C# `Add`s it — a **widening union**, not a narrowing unify — onto
/// that clone (`sfs.Add(_rule.RequiredSyntacticFeatureStruct)`, never fails); else if `out` is
/// empty the result is cleared to empty.
fn ana_syn_fs(
    g: &Grammar,
    req: hc_featstruct::FsId,
    out: hc_featstruct::FsId,
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

// W3.1: MPR-group-aware required/excluded gating now lives on `Grammar` itself
// (`Grammar::mpr_group_ok`/`mpr_required_ok`/`mpr_excluded_ok`/`mpr_add_output`,
// `hc-grammar/src/model.rs`) since it's the only owner of `mpr_groups`. The former `mpr_ok` here
// was a group-**unaware** flat overlap check — correct only for singleton groups (all 3 reference
// corpora) and, for `required` with 2+ ungrouped members, actually WRONG vs. C#'s
// `IsMatchRequired` (which ANDs ungrouped members, not ORs — see `Grammar::mpr_required_ok`'s doc).
// `head_prod_restrictions_mpr`/`non_head_prod_restrictions_mpr` gates are unaffected: C# checks
// those via the always-group-unaware `CompoundMprFeaturesMatch`, i.e. `MprSet::compound_match`.

/// C# `HashSet<AllomorphEnvironment>.SetEquals` — environment lists compared as sets (shared by
/// [`constraints_equal`] and `crate::validity`'s root-allomorph `ConstraintsEqual` port).
pub(crate) fn env_set_equal(
    a: &[hc_grammar::model::EnvironmentDef],
    b: &[hc_grammar::model::EnvironmentDef],
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

/// R3 (plan §13.1.1): `Allomorph.ConstraintsEqual` as overridden by `AffixProcessAllomorph`
/// (`AffixProcessAllomorph.cs:75-85`) — same environments (as a set), same required/excluded MPR
/// feature sets, structurally identical LHS pattern list (`SequenceEqual`, order matters), and
/// value-equal `RequiredSyntacticFeatureStruct`. `pub(crate)` for `crate::validity`'s W3.2
/// free-fluctuation walk.
pub(crate) fn constraints_equal(g: &Grammar, a: &AffixAllomorphDef, b: &AffixAllomorphDef) -> bool {
    env_set_equal(&a.environments, &b.environments)
        && a.required_mpr == b.required_mpr
        && a.excluded_mpr == b.excluded_mpr
        && a.lhs == b.lhs
        && g.fs_interner.get(a.required_syn_fs) == g.fs_interner.get(b.required_syn_fs)
}

/// R3: `Allomorph.FreeFluctuatesWith` (`Allomorph.cs:80-98`). `cur`/`next` are always *adjacent*
/// allomorphs of the same rule (hence the same morpheme) at every call site this port has, so the
/// general `Index`-range walk over intervening allomorphs collapses to the single-step
/// `ConstraintsEqual` check (`minIndex..maxIndex` has exactly one pair when the two indices differ
/// by 1). The `this == other`/`Morpheme != other.Morpheme` guards are vacuous here for the same
/// reason (distinct objects, same morpheme) and are omitted.
fn free_fluctuates_with(g: &Grammar, cur: &AffixAllomorphDef, next: &AffixAllomorphDef) -> bool {
    constraints_equal(g, cur, next)
}

// =================================================================================================
// Affix process — synthesis.
// =================================================================================================

/// `input.RootAllomorph.StemName` (W5): resolve `word`'s root allomorph, if any, to its own
/// `StemName` field. `None` if the word has no root allomorph yet (shouldn't happen on the real
/// synthesis pipeline — every synthesized word descends from a lexical-lookup seed — but a
/// standalone `synth_affix` fixture in the test suite may lack one, so this stays defensive) or if
/// the root allomorph carries no stem name.
///
/// P11: `word.root_allomorph == Some(AllomorphId::GUESSED)` (a fabricated root, §4.4) has no
/// `allomorph_owners` row; not exercised by any oracle-verified fixture today (no
/// `requiredStemName`-declaring rule combines with the guesser yet), but guarded the same way as
/// [`check_blocking`] for consistency and defense-in-depth — a guessed root's own stem name would
/// need explicit delegation to its pattern (as `hc_rules::validity`'s sentinel branch already does
/// for the FINAL validity check), which this synthesis-time rule-application gate does not
/// attempt; `None` (no stem name) is the safe, conservative answer.
fn root_stem_name(g: &Grammar, word: &Word) -> Option<hc_grammar::model::StemNameId> {
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
    // Rule-level required syntactic FS gate + output FS.
    let Some(new_syn) = synth_syn_fs(g, rule.required_syn_fs, rule.out_syn_fs, word) else {
        return Vec::new();
    };

    // NonFinal / partial gating that is computable without the deferred rule-count machinery.
    // Plan §6 item 6 / W1.6: both checks are C# `!_rule.IsTemplateRule && ...`
    // (`SynthesisAffixProcessRule.cs:64,86`) — a rule that is itself a template-slot member is
    // never subject to either, regardless of the word's `IsLastAppliedRuleFinal` state (these
    // checks exist to gate an ordinary rule applied *after* a template finished, not the
    // template's own slot rules). See `AffixProcessRuleDef::is_template_rule`'s doc.
    // (a) SynthesisAffixProcessRule.cs:64-82: after a *final* template, prohibit a non-partial rule.
    if !rule.is_template_rule
        && matches!(word.flags.is_last_applied_rule_final, Some(true))
        && !word.flags.is_partial
        && !rule.partial
    {
        return Vec::new();
    }
    // (b) Tier-2 #13, gate 3 — SynthesisAffixProcessRule.cs:86-105: after a *non-final* template,
    // require a non-partial rule, i.e. prohibit a partial rule (unless the input itself is partial).
    if !rule.is_template_rule
        && matches!(word.flags.is_last_applied_rule_final, Some(false))
        && !word.flags.is_partial
        && rule.partial
    {
        return Vec::new();
    }

    // W5 `requiredStemName` (`SynthesisAffixProcessRule.cs:107-120`): reference-equality gate on
    // the WORD's root allomorph's own stem name (not this rule's allomorphs') — `None` for both
    // sides (no `requiredStemName` attribute and an unrestricted root allomorph) passes, as does
    // an exact `Some(x) == Some(x)` match; anything else fails.
    if rule.required_stem_name.is_some() && rule.required_stem_name != root_stem_name(g, word) {
        return Vec::new();
    }

    let (segs, node_of) = segs_of(g, &word.shape, true);
    let mut output = Vec::new();
    // W3.2: C# appliedAllomorphIndices (SynthesisAffixProcessRule.cs:138,201-202) -- the indices
    // that successfully applied earlier in THIS loop, recorded on each output morph (before the
    // producing index itself is added).
    let mut applied: Vec<u16> = Vec::new();
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        if !g.mpr_group_ok(allo.required_mpr, allo.excluded_mpr, word.mpr) {
            continue;
        }
        // Recompiles the allomorph LHS FST on every call — kept as-is (not cached) because this
        // uncached path is also exercised directly by the test suite against standalone,
        // non-grammar-resident rule fixtures with no stable `AllomorphId`. The real pipeline calls
        // `synth_affix_cached`, which reads this from `crate::cache::RuleCache` instead. See
        // `crate::cache`'s module doc.
        let Ok((fst, names)) = compile_parts(g, &allo.lhs, "p", true) else {
            continue;
        };
        if let Some(w) = synth_process_allomorph(
            g,
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
            // Disjunctive-allomorph break (SynthesisAffixProcessRule.cs:235-242): stop after the
            // first match unless this allomorph is environment- or syn-constrained, or R3 (plan
            // §13.1.1) — it free-fluctuates with the next allomorph (`Allomorph.cs:80-98`), in
            // which case C# keeps going so the next allomorph's word is produced too.
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

/// [`crate::cache::RuleCache`]-aware sibling of [`synth_affix`], used by the real per-word pipeline.
/// P12 chunk 4: `mrid`/`trace`/`parent` close gap #1 (dead-end nodes with no `Failed` sibling) --
/// every early return now reports the matching `FailureReason` via `MorphologicalRuleNotApplied`
/// (`subrule_index = -1` for the four rule-level gates before the loop, matching C#'s `-1` at
/// those same call sites, `SynthesisAffixProcessRule.cs:46-131`), and a successful allomorph fires
/// `MorphologicalRuleApplied` with its REAL subrule index `i` (closing the spine's `-1` placeholder,
/// `SynthesisAffixProcessRule.cs:210`), reassigning the output word's trace cursor.
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
    let Some(new_syn) = synth_syn_fs(g, rule.required_syn_fs, rule.out_syn_fs, word) else {
        not_applied!(FailureReason::RequiredSyntacticFeatureStruct);
    };
    // Plan §6 item 6 / W1.6: `!rule.is_template_rule &&` guard on both — see the twin site in
    // `synth_affix` above for the full citation.
    // (a) SynthesisAffixProcessRule.cs:64-82 (final-template prohibition).
    if !rule.is_template_rule
        && matches!(word.flags.is_last_applied_rule_final, Some(true))
        && !word.flags.is_partial
        && !rule.partial
    {
        not_applied!(FailureReason::NonPartialRuleProhibitedAfterFinalTemplate);
    }
    // (b) Tier-2 #13, gate 3 — SynthesisAffixProcessRule.cs:86-105 (non-final-template prohibition).
    if !rule.is_template_rule
        && matches!(word.flags.is_last_applied_rule_final, Some(false))
        && !word.flags.is_partial
        && rule.partial
    {
        not_applied!(FailureReason::NonPartialRuleRequiredAfterNonFinalTemplate);
    }

    // W5 `requiredStemName` — see the twin site in `synth_affix` above for the full citation.
    if rule.required_stem_name.is_some() && rule.required_stem_name != root_stem_name(g, word) {
        not_applied!(FailureReason::RequiredStemName);
    }

    let (segs, node_of) = segs_of(g, &word.shape, true);
    let mut output = Vec::new();
    // W3.2: C# appliedAllomorphIndices (SynthesisAffixProcessRule.cs:138,201-202) -- the indices
    // that successfully applied earlier in THIS loop, recorded on each output morph (before the
    // producing index itself is added).
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
                // R3 (plan §13.1.1) — see the twin site in `synth_affix` for the full citation.
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

/// C# `SynthesisRealizationalAffixProcessRule.IsBlocked` (cs:185-212): every feature key
/// `real_fs` declares must also be a key `syn_fs` declares (`!syntacticFS.ContainsFeature(f) =>
/// false`, i.e. NOT blocked) — recursing into nested `Complex` values, but a `Symbolic` leaf needs
/// only be *present*, never value-compared (C#'s loop body has no `else` branch checking anything
/// about a `SimpleFeatureValue`, only the `ComplexFeature` branch recurses). All features present
/// (recursively) ⇒ blocked. No cycle guard: `hc_featstruct::tree`'s syntactic-FS model is a tree,
/// never a DAG (see that module's doc — the same reasoning `hc_featstruct::ops` gives for skipping
/// re-entrancy generally), so C#'s `visited` set (needed there only to guard the general re-entrant
/// `FeatureStruct` type against infinite recursion) can never actually revisit a pair here.
fn realizational_is_blocked(real_fs: &FeatureStruct, syn_fs: &FeatureStruct) -> bool {
    for (feat, rval) in real_fs.entries() {
        let Some(sval) = syn_fs.get(*feat) else {
            return false;
        };
        if let (
            hc_featstruct::FeatureValue::Complex(rfs),
            hc_featstruct::FeatureValue::Complex(sfs),
        ) = (rval, sval)
        {
            if !realizational_is_blocked(rfs, sfs) {
                return false;
            }
        }
    }
    true
}

/// C# `SynthesisRealizationalAffixProcessRule.Apply` (cs:42-183), gates through the per-allomorph
/// loop (cs:80-179). Three rule-level gates precede the loop, in C#'s exact order:
/// 1. `RealizationalFeatureStruct.Subsumes(input.RealizationalFeatureStruct)` (cs:47) — gates on
///    the word's *current* `real_fs`, not the syntactic one.
/// 2. `IsBlocked` (cs:50-60), only when the rule's `real_fs` is non-empty — checked against
///    `input.SyntacticFeatureStruct` (the word's syn FS *before* this rule's own unify below).
/// 3. `RequiredSyntacticFeatureStruct.Unify(input.SyntacticFeatureStruct, true, out syntacticFS)`
///    then, per successful allomorph, `sfs.PriorityUnion(_rule.RealizationalFeatureStruct)`
///    (cs:62-76,127-129) — exactly [`synth_syn_fs`]'s `unify-then-priority_union` shape with
///    `real_fs` standing in for the regular path's `out_syn_fs`, so it is reused verbatim.
///
/// No `MaxApplicationCount`/`IsPartial`/`IsLastAppliedRuleFinal`/`ObligatorySyntacticFeatures`
/// gates exist on this class (see [`RealizationalRuleDef`]'s doc) — the per-allomorph loop itself
/// (MPR gating, disjunctive-break condition, `appliedAllomorphIndices`) is otherwise identical to
/// [`synth_affix`], via the shared [`synth_process_allomorph`].
fn synth_realizational(g: &Grammar, word: &Word, rule: &RealizationalRuleDef) -> Vec<Word> {
    let real_fs = g.fs_interner.get(rule.real_fs);
    if !hc_featstruct::subsumes(real_fs, &word.real_fs) {
        return Vec::new();
    }
    if !real_fs.is_empty() && realizational_is_blocked(real_fs, &word.syn_fs) {
        return Vec::new();
    }
    let Some(new_syn) = synth_syn_fs(g, rule.required_syn_fs, rule.real_fs, word) else {
        return Vec::new();
    };

    let (segs, node_of) = segs_of(g, &word.shape, true);
    let mut output = Vec::new();
    let mut applied: Vec<u16> = Vec::new();
    for (i, allo) in rule.allomorphs.iter().enumerate() {
        if !g.mpr_group_ok(allo.required_mpr, allo.excluded_mpr, word.mpr) {
            continue;
        }
        let Ok((fst, names)) = compile_parts(g, &allo.lhs, "p", true) else {
            continue;
        };
        if let Some(w) = synth_process_allomorph(
            g,
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

/// [`crate::cache::RuleCache`]-aware sibling of [`synth_realizational`]. P12 chunk 4: the first two
/// gates (`real_fs` subsumption, `IsBlocked`) stay UNTRACED — verified against
/// `SynthesisRealizationalAffixProcessRule.cs:42-56`, C# itself fires no `TraceManager` call at
/// either site (a bare `return Enumerable.Empty<Word>()`), so tracing them would fabricate an event
/// C# never produces. Only the rule-level `RequiredSyntacticFeatureStruct` unify (cs:62-76) and the
/// per-allomorph loop are traced, matching C#'s own call sites exactly.
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
    if !hc_featstruct::subsumes(real_fs, &word.real_fs) {
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

    let (segs, node_of) = segs_of(g, &word.shape, true);
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

/// `PartRef::Input` index an RHS action references, for the two action kinds that carry a
/// C# `PartName` (`SynthesisAffixProcessAllomorphRuleSpec.cs:26-28`: the `Where` filter over
/// `!string.IsNullOrEmpty(action.PartName)` selects exactly `CopyFromInput`/`ModifyFromInput` --
/// `InsertSegments`/`InsertSimpleContext` have no referenced part). A `PartRef::Input(i)`'s C#
/// `PartName` is by construction `lhs[i].Name`, so grouping/comparing by this index is equivalent
/// to C#'s `GroupBy(PartName)`/`==` on part names.
fn redup_part_ref(action: &OutputAction) -> Option<u16> {
    match action {
        OutputAction::Copy(PartRef::Input(i)) | OutputAction::Modify(PartRef::Input(i), _) => {
            Some(*i)
        }
        _ => None,
    }
}

/// Tier-2 #8 (reduplication morph attribution): C#'s `_nonAllomorphActions`
/// (`SynthesisAffixProcessAllomorphRuleSpec.cs:23-120`). Ported verbatim (the `PartName` string
/// comparisons become `PartRef::Input` index comparisons, per [`redup_part_ref`]'s doc). Returns,
/// for every RHS index that is part of a "true" reduplication group (some `Input` part referenced
/// ≥2 times by Copy/Modify actions), whether that occurrence is the *existing* echo of the input
/// morph (`true`, C#'s `_nonAllomorphActions.Contains == true`) or genuinely new affix material
/// (`false`). RHS indices outside any repeated-part group are absent from the map -- callers keep
/// their unmodified default attribution for those (a lone `CopyFromInput` is always existing, a lone
/// `ModifyFromInput` is always new, matching C#'s singleton-group branch, cs:33-37).
fn classify_redup(
    lhs_len: u16,
    rhs: &[OutputAction],
    hint: ReduplicationHint,
) -> HashMap<usize, bool> {
    // Group RHS indices by referenced `Input` part (cs:25-31: `GroupBy(action => action.PartName)`).
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

    // Find `start`: the RHS index at which a `lhs_len`-long run `Input(0), Input(1), ...,
    // Input(lhs_len-1)` echoes every LHS part exactly once, in original order -- the "plain" (no
    // reduplication) repetition of the whole allomorph input. `None` (C#'s `start == -1`) when no
    // such contiguous run exists (cs:97 falls through the `switch` with `start` unset in that case
    // too, once the loop below exhausts `rhs` without finding one). Ported verbatim from
    // cs:45-97, `i32` throughout there; kept as `i64` here to avoid any subtraction-underflow UB
    // ambiguity when translating literally.
    let mut start: Option<i64> = None;
    match hint {
        ReduplicationHint::Prefix => {
            // cs:48-72.
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
            // cs:74-96 (`Suffix` and `Implicit` share one branch in C#).
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

    // cs:99-119: classify each occurrence of each redup group.
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

/// One allomorph's synthesis: LHS match + RHS build, shared by the regular affix-process path
/// (`synth_affix`/`synth_affix_cached`) and the W5 realizational path (`synth_realizational`/
/// `synth_realizational_cached`) — C#'s `SynthesisAffixProcessAllomorphRuleSpec` is the one class
/// both `SynthesisAffixProcessRule` and `SynthesisRealizationalAffixProcessRule` build their
/// per-allomorph `PatternRule` from, so the pattern-match-then-emit mechanics are identical; only
/// the handful of **rule-level** bookkeeping fields differ, which is why this takes them
/// individually instead of a `&AffixProcessRuleDef`:
/// - `morpheme` tags the [`MorphRecord`] `attribute_morphs` mints (both rule kinds have one).
/// - `obligatory` is `&[]` for realizational (C# `RealizationalAffixProcessRule` never touches
///   `Word.ObligatorySyntacticFeatures` — no such field on that class at all).
/// - `partial` is `None` for realizational: C#'s `SynthesisRealizationalAffixProcessRule.Apply`
///   never reads or writes `IsPartial`/`IsLastAppliedRuleFinal`, unlike the regular affix path's
///   final/non-final-template interaction (`SynthesisAffixProcessRule.cs:181-186`) — `None` means
///   "leave `word.flags` exactly as cloned", not "treat as non-partial".
/// - `apply_out_mpr` is `false` for realizational: `SynthesisRealizationalAffixProcessRule.Apply`
///   never calls `outWord.MprFeatures.AddOutput` at all (verified against the whole class — only
///   `RequiredMprFeatures`/`ExcludedMprFeatures` are read, for gating, never `OutMprFeatures` for
///   output), unlike `SynthesisAffixProcessRule`/`SynthesisCompoundingRule`, which both do.
#[allow(clippy::too_many_arguments)]
fn synth_process_allomorph(
    g: &Grammar,
    word: &Word,
    morpheme: MorphemeId,
    obligatory: &[hc_featstruct::FeatId],
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

    // Tier-2 #8 (reduplication morph attribution): classify which RHS occurrences of a repeated
    // `Input` part are the *existing* echo vs genuinely new (affix) material. Empty unless this
    // allomorph's RHS actually repeats some `Input` part (`classify_redup`'s own early return) --
    // every non-reduplicating allomorph pays only the cost of building an empty map.
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
                copy_part(g, &mut out, &src, None, redup.get(&rhs_idx).copied());
            }
            OutputAction::Modify(PartRef::Input(i), ctx) => {
                let src = PartSource {
                    node_of,
                    shape: &word.shape,
                    range: ranges[*i as usize],
                    head: true,
                };
                copy_part(g, &mut out, &src, Some(ctx), redup.get(&rhs_idx).copied());
            }
            OutputAction::InsertSegments { shape, .. } => {
                insert_segments(g, &mut out, &shape.shape, Origin::Affix);
            }
            OutputAction::InsertContext(ctx) => {
                out.push(OutNode {
                    kind: NodeKind::Segment,
                    char_def: NO_CHAR_DEF,
                    lanes: ctx_lanes(g, ctx),
                    optional: false,
                    origin: Origin::Affix,
                    cd_set: ctx_cd_set(g, ctx),
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
fn strip_boundaries(g: &Grammar, part: &Pattern) -> Pattern {
    fn is_boundary(g: &Grammar, cd: CharDefId) -> bool {
        let t = &g.char_tables[TABLE.0 as usize];
        (cd.0 as usize) < t.len() && t.get(cd).kind() == hc_grammar::chardef::CharDefKind::Boundary
    }
    fn strip(g: &Grammar, nodes: &[PatternNode]) -> Vec<PatternNode> {
        let mut out = Vec::new();
        for n in nodes {
            match n {
                PatternNode::CharDef(cd) if is_boundary(g, *cd) => {}
                PatternNode::Quantifier { min, max, children } => {
                    let kids = strip(g, children);
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
        nodes: strip(g, &part.nodes),
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
    lhs_parts: &[(String, &Pattern)],
    rhs: &[OutputAction],
) -> Result<AnalysisLhs, BridgeError> {
    // P10: same `StrRep` identity lane as `compile_parts` (inputs come from `segs_of`).
    let bridge = PatternBridge::new(g)
        .with_table(TABLE)
        .deterministic(false)
        .id_lane(true);
    let id_width = crate::bridge::id_lane_width(g, TABLE);
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
                let part = strip_boundaries(g, lookup[name.as_str()]);
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
                let part = strip_boundaries(g, lookup[name.as_str()]);
                let mut children = bridge.compile_pattern(&part)?.input.nodes;
                let pins: Vec<(usize, u64)> = ctx_pins(g, ctx)
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
                        let mut lanes = to_fst(g, &cd_lanes(g, char_def));
                        // P10 `StrRep` identity: the analysis-side consumer must find and consume
                        // *this* inserted segment (C# matches its full char-def FS incl. `StrRep`),
                        // not any feature-unifiable one.
                        if let (Some(w), true) = (id_width, char_def != NO_CHAR_DEF) {
                            crate::bridge::push_id_lane(&mut lanes, w, 1u64 << char_def);
                        }
                        lhs.nodes.push(CompileNode::Constraint(lanes));
                    }
                }
            }
            OutputAction::InsertContext(ctx) => {
                // KNOWN RESIDUAL (plan §13.1 Tier-1 #3): this builds an FST *match* constraint (the
                // analysis-direction consumer must still find and consume the inserted class's
                // material to unapply it), not an output node -- there is no `Shape` here to attach
                // a `cd_set` to. `hc_fst::Segment` carries only lanes, so a `Segments`-kind class
                // still over-matches on this path (accepts any segment unifiable with the member
                // lane-union, not just real members) exactly as the audit's bridge.rs finding
                // describes. Zero real-grammar occurrences today (no `InsertSimpleContext` in any
                // of the three reference grammars' XML), so this is unexercised, not silently wrong
                // on a word that matters -- flagged rather than hacked around, matching this
                // module's existing scope-note convention. A full fix needs `hc_fst::Segment` (a
                // frozen contract elsewhere in this port) to carry a char-def-set dimension too.
                // [P10 addendum: the id lane now closes this for `Segments`-kind classes on ≤64-def
                // tables — member-set bits below — leaving only the >64-def fallback over-matching.]
                let mut lanes = to_fst(g, &ctx_lanes(g, ctx));
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
fn generate_shape(
    g: &Grammar,
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
            // Not captured → untruncate the part (materialize its segment constraints, optional
            // beyond a quantifier's min). Stale-claim correction (plan §W1.5): NOT zero in the
            // reference grammars — Amharic has 4 occurrences (confirmed independently by this
            // audit and the phase-1 audit); Indonesian/Sena have zero.
            untruncate(g, &mut out, part);
            continue;
        };
        // ModifyFromInput: the underspecify set = the features the ctx pinned.
        let modify_pins: Option<Vec<usize>> = lhs
            .modify
            .get(name)
            .map(|(_, ctx)| ctx_pins(g, ctx).into_iter().map(|(f, _)| f).collect());
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
                            // Analysis-side counterpart of `copy_part`'s synthesis fix above (same
                            // finding, `modify_from_input_rules`): the surface node's own `char_def`
                            // (e.g. the synthesized "b") is exactly as stale here as
                            // `ana_feature`'s widened-lane node is (rewrite.rs's module-doc "major
                            // finding") — a lexical root whose stored allomorph has the
                            // PRE-modification segment (e.g. "p") can never be found by
                            // `RootAllomorphIndex::search`'s char_def-equality gate while this node
                            // still claims to literally BE "b". Clear to `NO_CHAR_DEF` so lookup
                            // falls back to lane unification against the just-widened (underspecified
                            // at the pinned features) lanes above — `cd_set` stays `Unrestricted`
                            // (`cd_set_of` already maps a concrete singleton source to `Unrestricted`,
                            // so this is a no-op there; written explicitly for clarity since the
                            // `char_def` it would have kept the singleton view of is gone).
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
            untruncate(g, &mut out, part);
        }
    }
    out
}

/// Materialize a part's `Segment`/`Context` constraints as (optional) output nodes (C#
/// `AnalysisMorphologicalTransform.Untruncate`). Boundaries are skipped.
///
/// Quantifier semantics are C#'s exactly (`AnalysisMorphologicalTransform.cs:125-129`):
/// `for (int i = 0; i < quantifier.MaxOccur; i++) Untruncate(..., i >= MinOccur, ...)` — with
/// `Quantifier.Infinite == -1`, an **unbounded** quantifier runs ZERO iterations and emits
/// nothing; a bounded one emits `max` copies, optional beyond `min`. This port originally emitted
/// `max(min, 1)` copies instead, which fabricated a phantom OPTIONAL wildcard segment for every
/// uncaptured `[Seg]*`-style part (`min=0, max=∞`). On W8's narrowing-flooded analysis shapes
/// those phantoms let unrelated affix rules "unapply" through them (e.g. Amharic's ለ=/በ=/ከ=
/// proclitics matched their proclitic segment against the phantom wildcard: 44 spurious
/// unapplications each on መረዘ where C# has 0), inflating the stratum-0 combination walk ~27x
/// (268 interior nodes vs C#'s ~10) — the "residual efficiency divergence" the W8 BoundaryMarker
/// commit left open.
fn untruncate(g: &Grammar, out: &mut Vec<OutNode>, part: &Pattern) {
    fn emit(g: &Grammar, out: &mut Vec<OutNode>, nodes: &[PatternNode], optional: bool) {
        for n in nodes {
            match n {
                PatternNode::Context(sc) => out.push(OutNode {
                    kind: NodeKind::Segment,
                    char_def: NO_CHAR_DEF,
                    lanes: ctx_lanes(g, sc),
                    optional,
                    origin: Origin::Affix,
                    cd_set: ctx_cd_set(g, sc),
                }),
                PatternNode::CharDef(cd) => {
                    let t = &g.char_tables[TABLE.0 as usize];
                    if (cd.0 as usize) < t.len()
                        && t.get(*cd).kind() == hc_grammar::chardef::CharDefKind::Segment
                    {
                        out.push(OutNode {
                            kind: NodeKind::Segment,
                            char_def: cd.0,
                            lanes: cd_lanes(g, cd.0),
                            optional,
                            origin: Origin::Affix,
                            cd_set: CdSet::Unrestricted,
                        });
                    }
                }
                PatternNode::Quantifier { min, max, children } => {
                    // C# `for (int i = 0; i < quantifier.MaxOccur; i++)`, Infinite == -1: an
                    // unbounded quantifier emits nothing (see fn doc).
                    if let Some(max) = max {
                        for r in 0..*max {
                            emit(g, out, children, optional || r >= *min);
                        }
                    }
                }
                PatternNode::Segments { .. } | PatternNode::Anchor(_) => {}
            }
        }
    }
    emit(g, out, &part.nodes, false);
}

/// Build the analysis LHS + its compiled FST for one affix allomorph (C# `AnalysisMorphologicalTransform`
/// applied to `allo.rhs`). Pure function of `allo` (grammar-static) — factored out so
/// `crate::cache::RuleCache::build` can call it once per [`AllomorphId`](hc_grammar::model::AllomorphId)
/// instead of leaving it to be recompiled on every application (the uncached [`ana_affix`] still calls
/// this itself, once per call, for the standalone-fixture test callers that have no grammar-resident
/// index to cache against).
pub(crate) fn build_ana_affix_lhs(
    g: &Grammar,
    allo: &AffixAllomorphDef,
) -> Result<(Fst, AnalysisLhs), BridgeError> {
    let parts: Vec<(String, &Pattern)> = allo
        .lhs
        .iter()
        .enumerate()
        .map(|(i, p)| (format!("p{i}"), p))
        .collect();
    let lhs = build_analysis_lhs(g, &parts, &allo.rhs)?;
    let fst = CompileInput::new(lhs.nodes.clone())
        .deterministic(false)
        .compile_with_direction(Direction::LeftToRight);
    Ok((fst, lhs))
}

fn ana_affix(g: &Grammar, word: &Word, rule: &AffixProcessRuleDef) -> Vec<Word> {
    let Some(new_syn) = ana_syn_fs(g, rule.required_syn_fs, rule.out_syn_fs, word) else {
        return Vec::new();
    };
    let (segs, node_of) = segs_of(g, &word.shape, false);
    let mut output = Vec::new();
    for allo in &rule.allomorphs {
        let Ok((fst, lhs)) = build_ana_affix_lhs(g, allo) else {
            continue;
        };
        output.extend(ana_affix_allomorph(
            g, word, allo, &lhs, &fst, &segs, &node_of, &new_syn,
        ));
    }
    output
}

/// [`crate::cache::RuleCache`]-aware sibling of [`ana_affix`].
fn ana_affix_cached(
    g: &Grammar,
    word: &Word,
    rule: &AffixProcessRuleDef,
    cache: &crate::cache::RuleCache,
) -> Vec<Word> {
    let Some(new_syn) = ana_syn_fs(g, rule.required_syn_fs, rule.out_syn_fs, word) else {
        return Vec::new();
    };
    let (segs, node_of) = segs_of(g, &word.shape, false);
    let mut output = Vec::new();
    for allo in &rule.allomorphs {
        let Some((fst, lhs)) = cache.allomorph(allo.id).ana_lhs.as_ref() else {
            continue;
        };
        output.extend(ana_affix_allomorph(
            g, word, allo, lhs, fst, &segs, &node_of, &new_syn,
        ));
    }
    output
}

/// One allomorph's analysis-side match + `GenerateShape` + per-allomorph dedup (C#
/// `_rules[i].Apply(input).RemoveDuplicates()`, `AnalysisAffixProcessRule.cs:58` — the dedup scope
/// is freshly reset per allomorph, NOT a single set shared across the whole rule).
#[allow(clippy::too_many_arguments)]
fn ana_affix_allomorph(
    g: &Grammar,
    word: &Word,
    allo: &AffixAllomorphDef,
    lhs: &AnalysisLhs,
    fst: &Fst,
    segs: &[Segment],
    node_of: &[usize],
    new_syn: &FeatureStruct,
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
        let out = generate_shape(g, &parts, lhs, fst, &result, node_of, &word.shape);
        let shape = freeze_out(g, &out);
        let mut w = word.clone();
        w.shape = shape;
        w.syn_fs = new_syn.clone();
        push_remove_duplicates(&mut allo_out, w);
    }
    allo_out
}

// =================================================================================================
// Realizational affix process (W5) — analysis.
// =================================================================================================

/// C# `AnalysisRealizationalAffixProcessRule.Apply` (cs:41-80): one rule-level gate —
/// `RealizationalFeatureStruct.Unify(input.RealizationalFeatureStruct, out realFS)` — then every
/// allomorph's matches all get the SAME unified `realFS` written onto `real_fs` (cs:56). No
/// `MaxApplicationCount`/syntactic-FS gate exists on this class (contrast [`ana_affix`]'s
/// [`ana_syn_fs`] — see [`RealizationalRuleDef`]'s doc for the full field-by-field diff).
fn ana_realizational(g: &Grammar, word: &Word, rule: &RealizationalRuleDef) -> Vec<Word> {
    let Some(real_fs) = unify(g.fs_interner.get(rule.real_fs), &word.real_fs) else {
        return Vec::new();
    };
    let (segs, node_of) = segs_of(g, &word.shape, false);
    let mut output = Vec::new();
    for allo in &rule.allomorphs {
        let Ok((fst, lhs)) = build_ana_affix_lhs(g, allo) else {
            continue;
        };
        output.extend(ana_realizational_allomorph(
            g, word, allo, &lhs, &fst, &segs, &node_of, &real_fs,
        ));
    }
    output
}

/// [`crate::cache::RuleCache`]-aware sibling of [`ana_realizational`].
fn ana_realizational_cached(
    g: &Grammar,
    word: &Word,
    rule: &RealizationalRuleDef,
    cache: &crate::cache::RuleCache,
) -> Vec<Word> {
    let Some(real_fs) = unify(g.fs_interner.get(rule.real_fs), &word.real_fs) else {
        return Vec::new();
    };
    let (segs, node_of) = segs_of(g, &word.shape, false);
    let mut output = Vec::new();
    for allo in &rule.allomorphs {
        let Some((fst, lhs)) = cache.allomorph(allo.id).ana_lhs.as_ref() else {
            continue;
        };
        output.extend(ana_realizational_allomorph(
            g, word, allo, lhs, fst, &segs, &node_of, &real_fs,
        ));
    }
    output
}

/// One realizational allomorph's analysis-side match + `GenerateShape` + per-allomorph dedup.
/// Unlike [`ana_affix_allomorph`], the syntactic FS is left completely untouched — C#'s
/// `AnalysisRealizationalAffixProcessRule.Apply` never assigns `outWord.SyntacticFeatureStruct` at
/// all (only `RealizationalFeatureStruct`, cs:56), so `word.clone()`'s syn FS (identical to the
/// matched span's, since the pattern spec never touches it either) passes through verbatim.
#[allow(clippy::too_many_arguments)]
fn ana_realizational_allomorph(
    g: &Grammar,
    word: &Word,
    allo: &AffixAllomorphDef,
    lhs: &AnalysisLhs,
    fst: &Fst,
    segs: &[Segment],
    node_of: &[usize],
    real_fs: &FeatureStruct,
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
        let out = generate_shape(g, &parts, lhs, fst, &result, node_of, &word.shape);
        let shape = freeze_out(g, &out);
        let mut w = word.clone();
        w.shape = shape;
        w.real_fs = real_fs.clone();
        push_remove_duplicates(&mut allo_out, w);
    }
    allo_out
}

/// C# `HermitCrabExtensions.RemoveDuplicates`/`Duplicates` (HermitCrabExtensions.cs:180-207):
/// insert `w` into `out`, but first scan `out` for another candidate whose **non-Optional** nodes
/// form the identical sequence (`Duplicates` deliberately ignores `Optional` nodes — the very
/// reconstructed/re-inserted material narrowing and deletion analysis subrules mark `Optional`).
/// A duplicate match keeps whichever shape is **longer** (`Shape.Count`) — i.e. prefers the
/// candidate carrying MORE of that reconstructed optional material (more lexical-lookup
/// opportunities downstream), discarding the shorter one; a strict tie keeps the earlier one
/// (`add=false` when not strictly longer, mirroring C#'s `>` comparison exactly).
///
/// This is not just a cosmetic difference from a naive exact-shape keep-first dedup: once a
/// phonological narrowing/deletion analysis rule has scattered many `Optional` segments through a
/// shape (a legitimate part of un-applying e.g. Amharic's CV-merger rules), an affix rule's
/// nondeterministic `AllSubmatches` matching produces many distinct *exact* shapes that differ
/// only in which of those already-optional segments happen to be captured inside vs. outside the
/// matched affix parts. C# collapses all of those down to one core analysis per rule application;
/// without this, Rust kept every one of them as a permanently-distinct candidate (no other dedup
/// stage ever unifies them, since the outer `WordKey` compares the full shape), causing a
/// combinatorial blow-up in downstream candidate counts — and, when the step budget then trims the
/// resulting oversized search, an arbitrary (often shorter/wrong) survivor rather than C#'s
/// deliberately-preferred longer one.
fn push_remove_duplicates(out: &mut Vec<Word>, w: Word) {
    // web_time::Instant, not std's: this call is unconditional on every invocation (the profiling
    // read is gated, this timestamp isn't), and std::time::Instant panics on wasm32-unknown-unknown
    // (hc-wasm builds this crate for the browser demo). web_time re-exports std elsewhere.
    let __o2_start = web_time::Instant::now();
    let __o2_out_len = out.len();
    push_remove_duplicates_inner(out, w);
    dedup_profile::record(__o2_start.elapsed().as_nanos(), __o2_out_len);
}

// --- O2 profiling instrumentation (rust-optimizations-phase2.md O2) -------------------------
// Measures `push_remove_duplicates`'s own cost (the plan's ranked lead #2, "keep-longer dedup
// preferring Optional-flooded shapes") separately from hc-fst's `Transduce::run`/`distinct()`
// (lead #1) -- both are linear-scan dedup passes over candidate lists that can explode on
// Optional-flooded shapes, and this distinguishes which one actually dominates wall-clock.
// Kept as a permanent diagnostic (HC_STEP_STATS-style, near-zero cost when unread) -- see
// `docs/o2-profile-findings.md`; the Fable follow-up will want it to confirm lead #2 stays
// negligible after whatever fix it lands for lead #1.
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

fn push_remove_duplicates_inner(out: &mut Vec<Word>, w: Word) {
    if let Some(existing) = out
        .iter_mut()
        .find(|o| shape_duplicates(&w.shape, &o.shape))
    {
        if w.shape.len() > existing.shape.len() {
            *existing = w;
        }
        return;
    }
    out.push(w);
}

/// C# `HermitCrabExtensions.Duplicates` (HermitCrabExtensions.cs:180-184): two shapes "duplicate"
/// each other iff their **non-Optional** nodes, in order, carry an identical `FeatureStruct`
/// (`NodeComparer`, HermitCrabExtensions.cs:175-178, projects onto `Annotation.FeatureStruct`).
/// That `FeatureStruct` has TWO dimensions this port stores separately:
///
/// 1. **Phonological features + `Type`** → the node's lanes (the `Type` lane per plan Tier-1 #1).
/// 2. **`StrRep`** → the node's effective char-def-set (`Shape::node_cd_set`, plan Tier-1 #3).
///    `StrRep` is a real string feature *inside* the node FS whenever the char-def was built via
///    `CharacterDefinitionTable.Add`'s `fs == null` branch (`CharacterDefinitionTable.cs:68-76`) —
///    which `XmlLanguageLoader.cs:670-673` takes for **every segment of a zero-phonological-feature
///    grammar** (Sena) and `AddBoundary` (cs:43-46) takes for **every boundary of every grammar**.
///    On such nodes `StrRep` is the *only* identity: a lanes-only comparison (this function's
///    original, corrected form) treated any two same-length Sena candidate sequences as duplicates
///    and longer-wins-collapsed genuinely distinct analyses. `StrRep` compares as a value **set**
///    (`StringFeatureValue.ValueEquals`, `SM/FeatureModel/StringFeatureValue.cs:219-224`, uses
///    `SetEquals`), and a `SegmentNaturalClass`-inserted node carries the member-FS **union**
///    (`SegmentNaturalClass.cs:16-26`) — hence [`effective_cd_sets_eq`]'s set semantics, including
///    `Singleton(x) == Members({x})` (a one-member class's unioned `StrRep` is byte-identical to
///    the member's own).
///
/// Node-touching analysis paths verified against C# for what the compared FS actually carries:
/// - deletion re-insertion (`NarrowAnalysisRewriteRuleSpec.cs:41-58`): inserted via
///   `AddAfter(.., fs, true)` → **Optional**, so excluded from `Duplicates` on both engines;
/// - feature un-application (`FeatureAnalysisRewriteRuleSpec.cs:101-115`): `PriorityUnion` then
///   `Union` never remove `StrRep` — the node keeps its identity, matching Rust's `ana_feature`
///   keeping `char_def`.
///
/// **Deliberate residual, finer than C# in one direction only**: on feature-bearing grammars C#'s
/// segment FS carries *no* `StrRep` (`XmlLanguageLoader.cs:670-673` passes a real `fs`, and the
/// `fs != null` branch adds only `Type`, `CharacterDefinitionTable.cs:77-81`), so two same-lane
/// nodes with different char-defs (or one reset to `NO_CHAR_DEF` by `syn_feature`) compare EQUAL
/// in C# but unequal here. Being finer merely keeps extra candidates C# would prune — safe, since
/// C# documents this dedup as "not strictly necessary, but it helps to reduce the search space"
/// (AnalysisCompoundingRule.cs:99-100) — whereas being coarser deletes real analyses.
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

/// Set-equality over [`EffectiveCdSet`] — the `StrRep` value-set comparison of
/// [`shape_duplicates`]'s dimension 2. `Unrestricted` (a node whose producer recorded no
/// restriction) only equals `Unrestricted`: in C# terms, a node whose FS carries `StrRep` and one
/// whose FS does not are different `FeatureStruct`s.
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

    let (head_segs, head_node_of) = segs_of(g, &word.shape, true);
    let (nh_segs, nh_node_of) = segs_of(g, &nh.shape, true);
    let mut output = Vec::new();
    for sr in &rule.subrules {
        if !g.mpr_group_ok(sr.required_mpr, sr.excluded_mpr, word.mpr) {
            continue;
        }
        // Recompiles both part-group FSTs on every call — kept as-is (not cached) for the same
        // standalone-fixture reason as `synth_affix`; the real pipeline calls `synth_compound_cached`.
        let Ok((head_fst, head_names)) = compile_parts(g, &sr.head_lhs, "h", true) else {
            continue;
        };
        let Ok((nh_fst, nh_names)) = compile_parts(g, &sr.non_head_lhs, "n", true) else {
            continue;
        };
        if let Ok(w) = synth_compound_subrule(
            g,
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

/// [`crate::cache::RuleCache`]-aware sibling of [`synth_compound`]. P12 chunk 4: gate order here
/// differs slightly from `SynthesisCompoundingRule.cs`'s exact sequence (C#: MaxApplicationCount →
/// NonPartialRuleProhibitedAfterFinalTemplate → NonHeadRequiredSyntacticFeatureStruct →
/// HeadRequiredSyntacticFeatureStruct → HeadProdRestrictMprFeatures; this port has no
/// `MaxApplicationCount` gate at all here — see `guided_synth`'s doc — and checks the two
/// syntactic-FS gates before the partial-template one) — every gate is independent and boolean, so
/// this cannot change WHICH words are produced, only which single reason is reported when more than
/// one gate would have failed simultaneously on the same word (rare in practice). `HeadProdRestrictMprFeatures`
/// alone routes through `CompoundingRuleNotApplied` (→ `TraceType::CompoundingRuleSynthesis`),
/// matching C#'s cs:119-131 exactly — every OTHER gate here (including MPR/pattern in the loop)
/// stays on the generic `MorphologicalRuleNotApplied` (→ `MorphologicalRuleSynthesis`), because
/// that is what C# itself does at every one of those other call sites (verified against
/// `SynthesisCompoundingRule.cs` in full — `CompoundingRuleNotApplied` is used at exactly ONE site).
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
    let (head_segs, head_node_of) = segs_of(g, &word.shape, true);
    let (nh_segs, nh_node_of) = segs_of(g, &nh.shape, true);
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

/// Returns `Err(FailureReason::HeadPattern)`/`Err(FailureReason::NonHeadPattern)` instead of a bare
/// `None` (P12 chunk 4) so the caller can report which side's pattern failed to match, mirroring
/// `SynthesisCompoundingRule.cs:226-240`'s `else`/final-`else` pair exactly (head tried first; the
/// non-head is only attempted, and only `NonHeadPattern` reported, once the head already matched).
#[allow(clippy::too_many_arguments)]
fn synth_compound_subrule(
    g: &Grammar,
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
                copy_part(g, &mut out, &src, None, None);
            }
            OutputAction::Copy(PartRef::NonHead(i)) => {
                let src = PartSource {
                    node_of: nh_node_of,
                    shape: &nh.shape,
                    range: nh_ranges[*i as usize],
                    head: false,
                };
                copy_part(g, &mut out, &src, None, None);
            }
            OutputAction::InsertSegments { shape, .. } => {
                insert_segments(g, &mut out, &shape.shape, Origin::Insert);
            }
            OutputAction::InsertContext(ctx) => out.push(OutNode {
                kind: NodeKind::Segment,
                char_def: NO_CHAR_DEF,
                lanes: ctx_lanes(g, ctx),
                optional: false,
                origin: Origin::Insert,
                cd_set: ctx_cd_set(g, ctx),
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
    // Deliberately NOT popped. C#'s `SynthesisCompoundingRule.Apply` (`ApplySubrule`, cs:248-291)
    // clones `headMatch.Input` -- which carries `_nonHeadApps` forward unchanged (`Word`'s copy
    // ctor, Word.cs:105, clones the list) -- and never removes an entry from it; only
    // `MorphologicalRuleApplied` (Word.cs:411-429, decrement at 417-418) moves `_nonHeadAppIndex`
    // *backward* on confirmation (ported below, in the caller's `guided_synth`, as
    // `non_head_app_index -= 1`), leaving the consumed non-head permanently in the list as history.
    // That history is exactly what `WordKey`'s `non_heads` recursion needs: two compounds built
    // from surface-homophone but distinct non-head lexical entries (different `root_allomorph`)
    // otherwise become indistinguishable once the shared `shape` is synthesized. Previously this
    // popped the just-consumed non-head off `w.non_heads`, which erased that disambiguating history
    // and collapsed such pairs into one dedup-key bucket -- see
    // `hc-parse/tests/csharp_port_compounding.rs`'s `simple_rules_1_homophone_disjunction_finding`.
    // Correct regardless of how many non-heads accumulate: `Word::current_non_head()` is
    // index-based (`non_heads.get(non_head_app_index as usize)`, matching C#'s
    // `_nonHeadApps[_nonHeadAppIndex]`, Word.cs:453-461) rather than `non_heads.last()`, so leaving
    // stale, already-consumed entries in the list never shadows the one the index actually points
    // at. (An earlier draft of this comment argued un-popping was only safe because
    // `AnalyzerConfig::max_stem_count`'s analysis-side gate caps `non_heads.len()` at 1 -- that
    // argument was wrong: `hc_parse::Morpher::generate_words` builds synthesis seeds directly,
    // bypassing analysis and its `max_stem_count` gate entirely, and can push two or more
    // non-heads via `GenMorpheme::NonHead` today. `current_non_head()` was fixed to be index-based
    // for exactly this reason, rather than relying on that now-refuted length cap.)
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

/// Build the analysis LHS + its compiled FST for one compounding subrule. Pure function of `sr`
/// (grammar-static) — factored out so `crate::cache::RuleCache::build` can call it once per
/// (rule, subrule) pair instead of leaving it to be recompiled on every application. Mirrors
/// [`build_ana_affix_lhs`]'s role for affix-process allomorphs.
fn build_ana_compound_lhs(
    g: &Grammar,
    sr: &CompoundingSubruleDef,
) -> Result<(Fst, AnalysisLhs), BridgeError> {
    let parts = ana_compound_parts(sr);
    let lhs = build_analysis_lhs(g, &parts, &sr.rhs)?;
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
    // Same guard + adjust as `ana_affix` (`ana_syn_fs`'s doc comment): C#'s
    // `AnalysisCompoundingRule.Apply` gates on `out.IsUnifiable(word.syn)`
    // (AnalysisCompoundingRule.cs:46-53) then, on unapply, `Add`s `HeadRequiredSyntacticFeature
    // Struct` onto each output's (input-cloned) syntactic FS (cs:133-138) -- identical shape to
    // the affix-process rule's `RequiredSyntacticFeatureStruct`/`OutSyntacticFeatureStruct` pair.
    let Some(new_syn) = ana_syn_fs(g, rule.head_required_syn_fs, rule.out_syn_fs, word) else {
        return Vec::new();
    };
    let (segs, node_of) = segs_of(g, &word.shape, false);
    let mut output = Vec::new();
    for sr in &rule.subrules {
        let Ok((fst, lhs)) = build_ana_compound_lhs(g, sr) else {
            continue;
        };
        output.extend(ana_compound_subrule(
            g,
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

/// [`crate::cache::RuleCache`]-aware sibling of [`ana_compound`].
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
    let (segs, node_of) = segs_of(g, &word.shape, false);
    let cc = cache.compound(mrid);
    let mut output = Vec::new();
    for (i, sr) in rule.subrules.iter().enumerate() {
        let Some((fst, lhs)) = cc.subrules[i].ana.as_ref() else {
            continue;
        };
        output.extend(ana_compound_subrule(
            g,
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

/// One subrule's analysis-side match + `GenerateShape` (head + non-head) + per-subrule dedup +
/// (plan Tier-2 #7) the non-head root-allomorph resolution, all in the same per-subrule scope C#
/// uses (`AnalysisCompoundingRule.Apply`'s `srOutput`, AnalysisCompoundingRule.cs:56-126: the dedup
/// scope is reset for each subrule `i`, not shared across the rule's subrules — a candidate from one
/// subrule must never suppress an identical-looking candidate from a different subrule).
///
/// `root_filter` is `None` for `hc-rules`'s own lexicon-free tests (preserves the pre-#7 shape-pair
/// dedup, one raw-split candidate per match, unresolved) and `Some` on the one production path
/// (`hc-parse::Morpher`, wired through `crate::stratum::StratumAnalyzer::apply_one_mrule`). When
/// `Some`, each raw split multiplies into **one candidate per surviving root allomorph** — mirroring
/// C#'s `foreach (RootAllomorph allo in _morpher.SearchRootAllomorphs(...))` loop
/// (AnalysisCompoundingRule.cs:63-124) — with the non-head's shape/syntactic-FS/MPR/root-allomorph/
/// morph record replaced by the matched `LexEntry`'s own canonical values (`Word.RootAllomorph`
/// setter via `SetRootAllomorph`, Word.cs:148-169), exactly as `hc-parse::Morpher::set_root_allomorph`
/// does for the head-level root (Morpher.cs:349-371/`hc-parse/src/morpher.rs`). A split whose
/// non-head matches no root, or whose only matches fail the rule's
/// `NonHeadRequiredSyntacticFeatureStruct`/`NonHeadProdRestrictionsMprFeatures` gates, is thrown away
/// entirely — "we assume it is not a valid analysis" (cs:61-62).
#[allow(clippy::too_many_arguments)]
fn ana_compound_subrule(
    g: &Grammar,
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
        // Acceptable: at least one head part captured (AnalysisCompoundingSubruleRuleSpec).
        let head_captured = head_parts.iter().any(|(name, _)| {
            (0..*lhs.captured.get(name).unwrap_or(&0)).any(|idx| {
                fst.get_offsets(&group_name(name, idx), &result.registers)
                    .is_some()
            })
        });
        if !head_captured {
            continue;
        }
        let head_out = generate_shape(g, &head_parts, lhs, fst, &result, node_of, &word.shape);
        let nh_out = generate_shape(g, &nh_parts, lhs, fst, &result, node_of, &word.shape);
        let head_shape = freeze_out(g, &head_out);
        let nh_shape = freeze_out(g, &nh_out);
        match root_filter {
            None => {
                let mut w = word.clone();
                w.shape = head_shape;
                w.syn_fs = new_syn.clone();
                // NonHeadUnapplied (Word.cs:477-482): push the split-off non-head word AND advance
                // `non_head_app_index` to point at it -- P4 (2026-07-09) made `current_non_head()`
                // index-based (matching C#'s `CurrentNonHead`, Word.cs:453-461), so a raw
                // `non_heads.push` here (leaving the index stale) would make the just-split non-head
                // invisible to `synth_compound`'s `word.current_non_head()` gate. Use the helper that
                // keeps both in lock-step, exactly like `Word::non_head_unapplied`'s own callers.
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

/// (Plan Tier-2 #7) C# `AnalysisCompoundingRule.Apply`'s root-allomorph-search + gates + pin
/// (AnalysisCompoundingRule.cs:63-124): search the lexicon (via `filter`, `hc-parse`'s
/// `RootAllomorphIndex::search`) for root allomorphs matching the just-split-off non-head's raw
/// shape, keep only those whose owning `LexEntry` unifies with `rule.non_head_required_syn_fs`
/// (cs:67-75) and whose MPR features satisfy `rule.non_head_prod_restrictions_mpr`
/// (cs:77-97), and for each survivor build the resolved non-head `Word`: shape re-segmented (with
/// phonological features, like `hc-parse`'s `set_root_allomorph`) from the matched allomorph's own
/// stored text, syntactic FS / MPR / partial flag / stratum taken from the entry, `root_allomorph`
/// pinned, and a single order-0 `MorphRecord` for the whole shape (`Word.SetRootAllomorph`'s
/// `MarkMorph(_shape, _rootAllomorph, RootMorphID)`, Word.cs:159-169) — this is what lets
/// `attribute_morphs`'s `Origin::NonHead` branch (this module) and `SynthesisCompoundingRule`'s
/// non-head syntactic-FS gate (`morph::synth_compound`'s `is_unifiable` check) see real data instead
/// of an empty FS / empty morph list. Returns one `Word` per surviving allomorph (never zero-or-more
/// "maybe"; an empty result correctly discards the whole split, matching cs:61-62's "assume it is
/// not a valid analysis and throw it away").
fn resolve_non_head_roots(
    g: &Grammar,
    rule: &CompoundingRuleDef,
    filter: NonHeadRootFilter,
    nh_shape: &Shape,
    stratum: StratumId,
) -> Vec<Word> {
    let req = g.fs_interner.get(rule.non_head_required_syn_fs);
    let mut out = Vec::new();
    for (allo_id, le_id) in filter(stratum, nh_shape) {
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

/// [`push_remove_duplicates`] extended to the (head, non-head) shape pair a compounding analysis
/// candidate carries, for the lexicon-free (`root_filter = None`) path — `hc-rules`'s own tests that
/// call `ana_compound`/`analyze` directly, unfiltered. "Longer" is judged on the head shape alone,
/// exactly mirroring C#'s `outWord.Shape.Count` (the head word's own shape, not the non-head's).
fn push_remove_duplicates_compound(out: &mut Vec<Word>, w: Word) {
    if let Some(existing) = out.iter_mut().find(|o| {
        shape_duplicates(&w.shape, &o.shape)
            && shape_duplicates(
                &w.non_heads.last().unwrap().shape,
                &o.non_heads.last().unwrap().shape,
            )
    }) {
        if w.shape.len() > existing.shape.len() {
            *existing = w;
        }
        return;
    }
    out.push(w);
}

/// (Plan Tier-2 #7) The root-allomorph-pinned sibling of [`push_remove_duplicates_compound`], used
/// once the non-head has been resolved: C#'s duplicate key is `outWord.Shape.Duplicates(...) && allo
/// == srOutput[j].CurrentNonHead.RootAllomorph` (AnalysisCompoundingRule.cs:104-107) — the HEAD
/// shape (optional-blind `Duplicates`) plus the *same pinned allomorph id*, not the non-head shape
/// (which is now the resolved root's own canonical shape, so two candidates pinned to the same
/// allomorph already have identical non-head shapes by construction). "Longer" is judged on the head
/// shape alone (`outWord.Shape.Count`, cs:109), matching [`push_remove_duplicates_compound`].
fn push_remove_duplicates_compound_pinned(out: &mut Vec<Word>, w: Word) {
    let allo = w.current_non_head().and_then(|nh| nh.root_allomorph);
    if let Some(existing) = out.iter_mut().find(|o| {
        shape_duplicates(&w.shape, &o.shape)
            && o.current_non_head().and_then(|nh| nh.root_allomorph) == allo
    }) {
        if w.shape.len() > existing.shape.len() {
            *existing = w;
        }
        return;
    }
    out.push(w);
}

// =================================================================================================
// Compile-once cache (plan §13.2 step 5; `crate::cache::RuleCache`'s allomorph/compounding slices).
// =================================================================================================

/// One compounding subrule's precompiled matchers. `synth_head`/`synth_non_head` are
/// [`compile_parts`]' output for `sr.head_lhs`/`sr.non_head_lhs` (used by
/// [`synth_compound_subrule`]); `ana` is [`build_ana_compound_lhs`]'s output (used by
/// [`ana_compound_subrule`]). `None` iff the underlying pattern failed to compile (a loader
/// invariant violation in practice) — the runtime functions already treat a compile failure as "this
/// subrule cannot apply," so a cached `None` reproduces that exactly.
pub(crate) struct CompoundSubruleCache {
    pub(crate) synth_head: Option<(Fst, Vec<String>)>,
    pub(crate) synth_non_head: Option<(Fst, Vec<String>)>,
    pub(crate) ana: Option<(Fst, AnalysisLhs)>,
}

/// One compounding rule's precompiled matchers, one [`CompoundSubruleCache`] per subrule.
pub(crate) struct CompoundCache {
    pub(crate) subrules: Vec<CompoundSubruleCache>,
}

/// Build the compile-once cache for one compounding rule (`crate::cache::RuleCache::build` calls
/// this once per `g.mrules` entry that is a [`CompoundingRuleDef`]).
pub(crate) fn build_compound_cache(g: &Grammar, rule: &CompoundingRuleDef) -> CompoundCache {
    let subrules = rule
        .subrules
        .iter()
        .map(|sr| CompoundSubruleCache {
            synth_head: compile_parts(g, &sr.head_lhs, "h", true).ok(),
            synth_non_head: compile_parts(g, &sr.non_head_lhs, "n", true).ok(),
            ana: build_ana_compound_lhs(g, sr).ok(),
        })
        .collect();
    CompoundCache { subrules }
}

/// One allomorph's precompiled matchers (`crate::cache::RuleCache`'s per-[`AllomorphId`]
/// (hc_grammar::model::AllomorphId) slice; root allomorphs never populate these — only
/// `AffixAllomorphDef`s have an `lhs`/`rhs` to compile). `synth_lhs` is [`compile_parts`]'s output
/// for `allo.lhs` (used by [`synth_affix_allomorph`]); `ana_lhs` is [`build_ana_affix_lhs`]'s output
/// (used by [`ana_affix_allomorph`]).
pub(crate) struct AllomorphLhsCache {
    pub(crate) synth_lhs: Option<(Fst, Vec<String>)>,
    pub(crate) ana_lhs: Option<(Fst, AnalysisLhs)>,
}

/// Build the LHS/RHS half of one affix allomorph's cache entry (`crate::cache::RuleCache::build`
/// pairs this with the environment-gate half it builds itself via `crate::rewrite::compile_env`).
pub(crate) fn build_allomorph_lhs_cache(
    g: &Grammar,
    allo: &AffixAllomorphDef,
) -> AllomorphLhsCache {
    AllomorphLhsCache {
        synth_lhs: compile_parts(g, &allo.lhs, "p", true).ok(),
        ana_lhs: build_ana_affix_lhs(g, allo).ok(),
    }
}
