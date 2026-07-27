//! Final per-allomorph word-validity gates (plan §13.1 Tier-1 #5): C# `Allomorph.IsWordValid`
//! (Allomorph.cs:105-156), called once per *distinct* allomorph used in the final word from
//! `Morpher.IsWordValid` (Morpher.cs:555-582, specifically the closing
//! `word.Allomorphs.All(allo => allo.IsWordValid(this, word))` at cs:581 — the clause this port was
//! missing entirely; the partial-parse + obligatory-feature clauses above it are already ported in
//! `pg-parse::Morpher::is_word_valid`).
//!
//! Ports the sub-gates the reference grammars actually exercise, plus (W6) co-occurrence rules and
//! (W5) `StemName` — both ported despite zero occurrence in Indonesian/Amharic/Sena, per the
//! phase-2 "no reference-grammar gate may move" floor and, for W5,
//! `rust/conformance/realizational/*`'s oracle-verified fixtures (`stem_name_gates_ok`/
//! `stem_name_required_match`/`stem_name_excluded_match` below port `StemName.IsRequiredMatch`/
//! `IsExcludedMatch`, StemName.cs:31-44, and `RootAllomorph.CheckAllomorphConstraints`'s clause
//! for them, RootAllomorph.cs:65-91):
//!  - **Environments** (`RootAllomorph`/`AffixProcessAllomorph` via `Allomorph.Environments`,
//!    Allomorph.cs:110-125): every morph span this allomorph produced must satisfy at least one of
//!    its declared environments (Required *or* Excluded — C#'s `Environments` collection holds both
//!    kinds together, each tagged with its own `ConstraintType`; `EnvironmentDef.require` mirrors
//!    that tag).
//!  - **Bound roots** (`RootAllomorph.CheckAllomorphConstraints`, RootAllomorph.cs:56-63): a root
//!    allomorph flagged `isBound` cannot be the word's *only* distinct allomorph
//!    (`word.Allomorphs.Count == 1`, i.e. `Word._allomorphs.Values` — a dict keyed by allomorph id,
//!    so "distinct allomorphs used", not morph-occurrence count; ported as `distinct_count` below).
//!  - **Required syntactic FS** (`AffixProcessAllomorph.CheckAllomorphConstraints`,
//!    AffixProcessAllomorph.cs:87-105): `RequiredSyntacticFeatureStruct.Subsumes(word.syn)`,
//!    re-checked at final-validity time against the word's *accumulated* syntactic FS — not just at
//!    the moment the rule applied (this port's `synth_affix`/`ana_affix` in `morph.rs` never gate on
//!    this per-allomorph FS at apply time; only the rule-level `required_syn_fs` is enforced there).
//!
//! W3.2 (plan #5d, history row `987be2fd`, formerly deferred): the **disjunctive-allomorph /
//! free-fluctuation re-check** (`Allomorph.IsWordValid`'s second loop, Allomorph.cs:127-152) is now
//! ported. Per morph occurrence, every "passed-over" disjunctive alternative — an earlier-indexed
//! allomorph of the same morpheme recorded by synthesis ([`MorphRecord::passed_over`], C#'s
//! `appliedAllomorphIndices` / `Word.GetDisjunctiveAllomorphApplications`), or ALL earlier-indexed
//! allomorphs (`Enumerable.Range(0, Index)`) when nothing was recorded (root morphs) — REJECTS the
//! word if it does not free-fluctuate with the used allomorph (`Allomorph.FreeFluctuatesWith`'s
//! adjacent-pair `ConstraintsEqual` walk over the index range, Allomorph.cs:80-98), its
//! environments are absent or satisfied at this morph's span, and its other allomorph constraints
//! hold (root: the bound-root gate; affix: the required-syntactic-FS subsumption; the StemName
//! clause is loader-linted `Unsupported`, as above; W6's co-occurrence rules are now also checked
//! here — see that section's own doc for the exact candidate-vs-key subtlety). Pinned by
//! `rust/conformance/allomorphy/disjunctive-recheck/` (oracle-diffed) and
//! `pg-parse/tests/disjunctive_recheck_gate.rs`.
//!
//! ## Morph-span derivation
//! [`crate::word::MorphRecord`] stores only the leftmost interior position (`order`); this module
//! derives each record's span as `[order_i, order_{i+1} - 1]` (sorted ascending; the last record
//! runs to the shape's last interior index).
//!
//! W3.3 (formerly the KNOWN RESIDUAL here): this derivation is exact for **discontinuous** morphs
//! too, because `attribute_morphs` (`morph.rs`) now emits one `MorphRecord` per **contiguous run**
//! of a morph's material (C# `MarkMorphs`' split, `97fa7721`) — a circumfix's two pieces, or a
//! root split by a later infixing rule, are separate records tiling the shape, so each run's env
//! check anchors at its own span, exactly C#'s per-annotation `word.GetMorphs(allomorph)` loop.
//! The old single-merged-record approximation mis-anchored exactly as this module's previous
//! scope note predicted — proven live by the oracle-diffed probing fixture
//! `rust/conformance/allomorphy/discontinuous-env/` (pre-fix: Rust accepted `xpitz`/`muat`, C#
//! rejects both because the environment fails at the *second* piece) and pinned by
//! `pg-parse/tests/discontinuous_env_gate.rs`.

use pg_grammar::model::{
    AllomorphCoOccurrenceRuleDef, AllomorphId, AllomorphOwner, CoOccurrenceAdjacency,
    EnvironmentDef, Grammar, MorphemeCoOccurrenceRuleDef, MorphemeId, RootAllomorphDef, StemNameId,
    TableId,
};
use pg_shape::Shape;

use crate::cache::RuleCache;
use crate::morph::segs_of;
use crate::rewrite::{compile_env_allomorph, left_env_ok, right_env_ok, EnvFst};
use crate::trace::{FailureReason, NoopSink, TraceHandle, TraceSink};
use crate::word::{MorphRecord, Word};

/// Morphological rules and lexical entries all resolve char-defs/patterns against table 0 in every
/// reference grammar (same convention as `morph.rs`/`rewrite.rs`).
const TABLE: TableId = TableId(0);

/// C# `Allomorph.IsWordValid`'s environment clause (Allomorph.cs:110-125) for one morph occurrence:
/// the span `[start, end]` (inclusive, 0-based interior indices, anchors excluded) must satisfy at
/// least one of `envs`, if any are declared. `start`/`end` double as segment-*positions* in the
/// `include_boundaries=true` sequence [`segs_of`] builds: every interior node is a Segment or a
/// Boundary, so with boundaries included nothing is filtered out and segment-position `k` is
/// exactly interior index `k` (the same identity `morph.rs`'s `owning_morph`/`MorphRecord.order`
/// convention already relies on).
///
/// Left/right anchoring matches C# exactly: `left_env_ok`/`right_env_ok` (`rewrite.rs`, shared
/// verbatim with the phonological-rule environment matcher) test a suffix of `segs[..start]` ending
/// adjacent to the morph and a prefix of `segs[end+1..]` starting adjacent to it — the Rust
/// equivalent of C#'s `morph.Range.Start.Prev` (right-to-left anchored-to-start) and
/// `morph.Range.End.Next` (left-to-right anchored-to-end) matchers.
///
/// Recompiles every environment's matcher on every call — kept as-is (not cached) because this
/// function is also called directly, in tests, against standalone `EnvironmentDef`s that are never
/// grammar-resident (no stable `AllomorphId` to cache against). The real per-word pipeline
/// (`pg-parse::Morpher::is_word_valid`) calls [`allomorphs_valid_cached`] instead, which reads each
/// environment's matcher from `crate::cache::RuleCache` via [`environments_ok_cached`]. See
/// `crate::cache`'s module doc for the full rationale.
pub fn environments_ok(
    g: &Grammar,
    envs: &[EnvironmentDef],
    shape: &Shape,
    start: u32,
    end: u32,
) -> bool {
    if envs.is_empty() {
        return true;
    }
    let (segs, _node_of) = segs_of(g, TABLE, shape, true);
    envs.iter().any(|env| {
        let left = compile_env_allomorph(g, TABLE, env.left.as_ref());
        let right = compile_env_allomorph(g, TABLE, env.right.as_ref());
        env_side_ok(env, &left, &right, &segs, start, end)
    })
}

/// The [`crate::cache::RuleCache`]-aware sibling of [`environments_ok`]: `envs` and `env_cache` must
/// correspond positionally (`env_cache[i]` is `(left, right)` for `envs[i]`) — every production call
/// site gets both from the same `cache.allomorph(id).envs`/owning def's `environments`, so they are
/// always in lockstep.
fn environments_ok_cached(
    g: &Grammar,
    envs: &[EnvironmentDef],
    env_cache: &[(Option<EnvFst>, Option<EnvFst>)],
    shape: &Shape,
    start: u32,
    end: u32,
) -> bool {
    if envs.is_empty() {
        return true;
    }
    let (segs, _node_of) = segs_of(g, TABLE, shape, true);
    envs.iter()
        .zip(env_cache)
        .any(|(env, (left, right))| env_side_ok(env, left, right, &segs, start, end))
}

/// One environment's require/exclude check, shared by [`environments_ok`] and
/// [`environments_ok_cached`].
fn env_side_ok(
    env: &EnvironmentDef,
    left: &Option<EnvFst>,
    right: &Option<EnvFst>,
    segs: &[pg_fst::Segment],
    start: u32,
    end: u32,
) -> bool {
    let is_match =
        left_env_ok(left, segs, start as usize) && right_env_ok(right, segs, end as usize + 1);
    if env.require {
        is_match
    } else {
        !is_match
    }
}

/// How an allomorph's environments are matched by [`allomorphs_valid_impl`]: freshly compiled per
/// call ([`environments_ok`], standalone-fixture tests) or read from the [`RuleCache`] (the real
/// per-word pipeline). Threaded so the W3.2 disjunctive loop's *candidate* environment checks go
/// through the same machinery as the used allomorph's own check.
enum EnvCheck<'a> {
    Fresh,
    Cached(&'a RuleCache),
}

impl EnvCheck<'_> {
    fn envs_ok(
        &self,
        g: &Grammar,
        id: AllomorphId,
        envs: &[EnvironmentDef],
        shape: &Shape,
        start: u32,
        end: u32,
    ) -> bool {
        match self {
            EnvCheck::Fresh => environments_ok(g, envs, shape, start, end),
            EnvCheck::Cached(cache) => {
                environments_ok_cached(g, envs, &cache.allomorph(id).envs, shape, start, end)
            }
        }
    }
}

/// C# `RootAllomorph.ConstraintsEqual` (RootAllomorph.cs:106-112): environments as a set +
/// `IsBound` equality. `StemName` is genuinely NOT compared here even in C# (verified against the
/// override itself, which calls only `base.ConstraintsEqual(other) && IsBound == otherAllo.
/// IsBound` — no `StemName` reference at all), so [`stem_name_gates_ok`]'s checks stay entirely
/// separate from `FreeFluctuatesWith`.
fn root_constraints_equal(a: &RootAllomorphDef, b: &RootAllomorphDef) -> bool {
    crate::morph::env_set_equal(&a.environments, &b.environments) && a.is_bound == b.is_bound
}

/// C# `StemName.IsRequiredMatch` (StemName.cs:31-34): `fs` (a word's accumulated syntactic FS)
/// matches iff at least one region subsumes it.
fn stem_name_required_match(
    g: &Grammar,
    sn: StemNameId,
    fs: &pg_featstruct::FeatureStruct,
) -> bool {
    g.stem_names[sn.0 as usize]
        .regions
        .iter()
        .any(|&r| pg_featstruct::subsumes(g.fs_interner.get(r), fs))
}

/// C# `StemName.IsExcludedMatch` (StemName.cs:36-44): every region THIS stem name declares that
/// `exclude_from`'s stem name does NOT also declare (the set difference — shared regions are
/// exempted, since both stem names agree those describe a valid match) must fail to subsume `fs`.
/// `exclude_from = None` mirrors C#'s `stemName == null` branch (`Except(Enumerable.Empty<...>())`
/// — nothing is exempted, i.e. every region of `sn` is checked).
fn stem_name_excluded_match(
    g: &Grammar,
    sn: StemNameId,
    fs: &pg_featstruct::FeatureStruct,
    exclude_from: Option<StemNameId>,
) -> bool {
    let sn_regions = &g.stem_names[sn.0 as usize].regions;
    let exempt: &[pg_featstruct::FsId] = match exclude_from {
        Some(other) => &g.stem_names[other.0 as usize].regions,
        None => &[],
    };
    sn_regions
        .iter()
        .filter(|r| !exempt.contains(r))
        .all(|&r| !pg_featstruct::subsumes(g.fs_interner.get(r), fs))
}

/// C# `RootAllomorph.CheckAllomorphConstraints`'s `StemName` clause (RootAllomorph.cs:65-91, W5):
/// `allo`'s own stem name (if any) must be a required match against `fs`, AND every OTHER
/// allomorph of the same entry that carries its own stem name must find `fs` an excluded match
/// (relative to `allo`'s stem name, so shared regions between the two don't conflict). Runs on
/// both the primary allomorph being validated (`fs` = the word's current syntactic FS) and, in the
/// W3.2 disjunctive re-check, a passed-over candidate (RootAllomorph.cs:56 delegates the SAME
/// method to the disjunctive check via `disjunctiveAllomorph.CheckAllomorphConstraints`).
fn stem_name_gates_ok(
    g: &Grammar,
    allos: &[RootAllomorphDef],
    idx: usize,
    fs: &pg_featstruct::FeatureStruct,
) -> bool {
    stem_name_gate_reason(g, allos, idx, fs).is_none()
}

/// P12 chunk 3: [`stem_name_gates_ok`]'s reason-reporting sibling (§3.2: Rust already distinguishes
/// required-vs-excluded internally -- this just reports which one failed instead of folding both
/// into one bool). `stem_name_gates_ok` is a thin wrapper over this, so the traced and untraced
/// paths cannot disagree on which allomorphs pass.
fn stem_name_gate_reason(
    g: &Grammar,
    allos: &[RootAllomorphDef],
    idx: usize,
    fs: &pg_featstruct::FeatureStruct,
) -> Option<FailureReason> {
    let allo = &allos[idx];
    if let Some(sn) = allo.stem_name {
        if !stem_name_required_match(g, sn, fs) {
            return Some(FailureReason::RequiredStemName);
        }
    }
    let excluded_ok = allos
        .iter()
        .enumerate()
        .filter(|&(i, other)| i != idx && other.stem_name.is_some())
        .all(|(_, other)| {
            stem_name_excluded_match(g, other.stem_name.unwrap(), fs, allo.stem_name)
        });
    if excluded_ok {
        None
    } else {
        Some(FailureReason::ExcludedStemName)
    }
}

/// C# `Allomorph.FreeFluctuatesWith` (Allomorph.cs:80-98): the full `minIndex..maxIndex`
/// adjacent-pair `ConstraintsEqual` walk — unlike `morph.rs::free_fluctuates_with` (whose call
/// sites only ever compare adjacent allomorphs), the validity gate compares arbitrary index pairs,
/// so the general walk is required. `i == j` returns true vacuously (C#'s `this == other` guard).
fn free_fluctuates<T>(allos: &[T], i: usize, j: usize, eq: impl Fn(&T, &T) -> bool) -> bool {
    let (lo, hi) = if i < j { (i, j) } else { (j, i) };
    (lo..hi).all(|k| eq(&allos[k], &allos[k + 1]))
}

/// The candidate index set for the W3.2 disjunctive re-check: the recorded passed-over indices
/// (C# `Word.GetDisjunctiveAllomorphApplications`), falling back to every earlier index
/// (`?? Enumerable.Range(0, Index)`, Allomorph.cs:127) when nothing was recorded (root morphs).
fn disjunctive_candidates(m: &MorphRecord, own_index: usize) -> Vec<usize> {
    match &m.passed_over {
        Some(list) => list.iter().map(|&x| x as usize).collect(),
        None => (0..own_index).collect(),
    }
}

// --- W6: co-occurrence rules (`MorphemeCoOccurrenceRule`/`AllomorphCoOccurrenceRule`) ------------
//
// C# `Allomorph.CheckAllomorphConstraints` (Allomorph.cs:158-204), the tail every override
// (`RootAllomorph`/`AffixProcessAllomorph`) delegates to via `base.CheckAllomorphConstraints(...)`
// AFTER its own extra check (bound-root / required-syntactic-FS) and BEFORE `Allomorph.IsWordValid`
// checks `Environments` (Allomorph.cs:105-125) — hence these gates sit between the bound-root/
// required-syn-fs check and the environments check in `allomorphs_valid_impl` below, matching C#'s
// call order exactly. History row `90dcee64` (#311/LT-22156, `rust/conformance/HISTORY-MATRIX.md`):
// post-fix, EVERY attached rule must pass (`foreach ... return false` on first failure) — never
// "at least one passes" (the pre-fix `.Any(...)`, the bug this row fixed). `Vec::iter().all(...)`
// below is exactly that AND-across-rules fold.

/// C# `MorphCoOccurrenceRule<T>.CoOccurs` (`MorphCoOccurrenceRule.cs:92-170`), generic over
/// whichever id space the rule cares about (`MorphemeId` for `MorphemeCoOccurrenceRule`,
/// `AllomorphId` for `AllomorphCoOccurrenceRule` — C#'s own `MorphCoOccurrenceRule<T>` base is
/// generic the same way). `morph_list` is `Word.AllomorphsInMorphOrder` projected to `T` (`key_of`
/// applied to each entry, C#'s `GetMorphObject`); `others` is the rule's declared list, consumed
/// left-to-right (`Anywhere` aside, order matters: `others[0]`/`others[last]` are tested
/// positionally, not as a set). A line-for-line port, including the two `Adjacent*` branches'
/// single-other special case (`else if key != next/prevMorphObj`): with exactly one declared
/// `other`, "adjacent" means directly next to the KEY itself, not to another `other`.
fn co_occurs<T: Copy + PartialEq>(
    key: T,
    others: &[T],
    adjacency: CoOccurrenceAdjacency,
    morph_list: &[T],
) -> bool {
    let mut rest: Vec<T> = others.to_vec();
    match adjacency {
        CoOccurrenceAdjacency::Anywhere => {
            for &cur in morph_list {
                if let Some(pos) = rest.iter().position(|&o| o == cur) {
                    rest.remove(pos);
                }
            }
        }
        CoOccurrenceAdjacency::SomewhereToLeft | CoOccurrenceAdjacency::AdjacentToLeft => {
            for i in 0..morph_list.len() {
                let cur = morph_list[i];
                if key == cur {
                    break;
                }
                if !rest.is_empty() && rest[0] == cur {
                    if adjacency == CoOccurrenceAdjacency::AdjacentToLeft {
                        if i == morph_list.len() - 1 {
                            return false;
                        }
                        let next = morph_list[i + 1];
                        if rest.len() > 1 {
                            if rest[1] != next {
                                return false;
                            }
                        } else if key != next {
                            return false;
                        }
                    }
                    rest.remove(0);
                }
            }
        }
        CoOccurrenceAdjacency::SomewhereToRight | CoOccurrenceAdjacency::AdjacentToRight => {
            for i in (0..morph_list.len()).rev() {
                let cur = morph_list[i];
                if key == cur {
                    break;
                }
                if !rest.is_empty() && *rest.last().unwrap() == cur {
                    if adjacency == CoOccurrenceAdjacency::AdjacentToRight {
                        if i == 0 {
                            return false;
                        }
                        let prev = morph_list[i - 1];
                        if rest.len() > 1 {
                            if rest[rest.len() - 2] != prev {
                                return false;
                            }
                        } else if key != prev {
                            return false;
                        }
                    }
                    rest.pop();
                }
            }
        }
    }
    rest.is_empty()
}

/// C# `MorphCoOccurrenceRule<T>.IsWordValid` (`MorphCoOccurrenceRule.cs:82-87`): `require` (C#
/// `ConstraintType.Require`) passes iff `co_occurs`; `exclude` (the DTD default) passes iff NOT.
fn co_occurrence_rule_ok<T: Copy + PartialEq>(
    require: bool,
    key: T,
    others: &[T],
    adjacency: CoOccurrenceAdjacency,
    morph_list: &[T],
) -> bool {
    let co = co_occurs(key, others, adjacency, morph_list);
    if require {
        co
    } else {
        !co
    }
}

/// Every rule in `rules` must pass against `key` — the AND-across-rules fold `90dcee64` requires.
/// Factored out of [`morpheme_co_occurrence_ok`] so the P11 §4.4-1 guessed-root branch of
/// [`allomorphs_valid_impl`] can evaluate the PATTERN's rule list keyed on the GUESSED (sentinel)
/// morpheme id, exactly as [`allomorph_co_occurrence_ok`] already lets its caller separate "whose
/// rules" from "which id is the primary key".
fn morpheme_co_occurrence_rules_ok(
    rules: &[MorphemeCoOccurrenceRuleDef],
    key: MorphemeId,
    morph_list_morphemes: &[MorphemeId],
) -> bool {
    rules.iter().all(|rule| {
        co_occurrence_rule_ok(
            rule.require,
            key,
            &rule.others,
            rule.adjacency,
            morph_list_morphemes,
        )
    })
}

/// Every `MorphemeCoOccurrenceRule` attached to `morpheme` (C# `Morpheme.
/// MorphemeCoOccurrenceRules`) must pass — the AND-across-rules fold `90dcee64` requires.
fn morpheme_co_occurrence_ok(
    g: &Grammar,
    morpheme: MorphemeId,
    morph_list_morphemes: &[MorphemeId],
) -> bool {
    morpheme_co_occurrence_rules_ok(
        &g.morphemes[morpheme.0 as usize].co_occurrence,
        morpheme,
        morph_list_morphemes,
    )
}

/// Every `AllomorphCoOccurrenceRule` in `rules` (an allomorph's own `co_occurrence` field, C#
/// `Allomorph.AllomorphCoOccurrenceRules`) must pass, tested against `key` — same AND-across-rules
/// fold as [`morpheme_co_occurrence_ok`]. `key` is a parameter (not always `rules`'s own owning
/// allomorph id) because the W3.2 disjunctive re-check calls this with the candidate's OWN rules
/// but the ORIGINALLY USED allomorph as the co-occurrence key — see that call site's comment.
fn allomorph_co_occurrence_ok(
    rules: &[AllomorphCoOccurrenceRuleDef],
    key: AllomorphId,
    morph_list_allomorphs: &[AllomorphId],
) -> bool {
    rules.iter().all(|rule| {
        co_occurrence_rule_ok(
            rule.require,
            key,
            &rule.others,
            rule.adjacency,
            morph_list_allomorphs,
        )
    })
}

/// C# `Morpher.IsWordValid`'s final clause (Morpher.cs:581): every *distinct* allomorph used
/// anywhere in the word (compounding synthesis already flattens non-head morphs into `w.morphs`
/// before this runs — see `morph.rs::synth_compound_subrule`'s `attribute_morphs` call) passes its
/// own `Allomorph.IsWordValid`.
///
/// Recompiles every checked allomorph's environment matchers on every call — see
/// [`environments_ok`]'s doc for why (standalone test fixtures). The real per-word pipeline
/// (`pg-parse::Morpher::is_word_valid`) calls [`allomorphs_valid_cached`] instead.
pub fn allomorphs_valid(g: &Grammar, w: &Word) -> bool {
    let sink = NoopSink;
    allomorphs_valid_impl(g, w, EnvCheck::Fresh, &sink, TraceHandle::DUMMY)
}

/// The [`RuleCache`]-aware sibling of [`allomorphs_valid`], used by the real per-word pipeline
/// (`pg-parse::Morpher::is_word_valid`): every environment matcher is read from
/// `cache.allomorph(id).envs` instead of being recompiled.
pub fn allomorphs_valid_cached(g: &Grammar, w: &Word, cache: &RuleCache) -> bool {
    let sink = NoopSink;
    allomorphs_valid_impl(g, w, EnvCheck::Cached(cache), &sink, TraceHandle::DUMMY)
}

/// P12 chunk 3: [`allomorphs_valid_cached`]'s traced sibling -- the single source of truth both
/// share (`allomorphs_valid_cached` calls this with a [`NoopSink`]). Closes the gap chunk 2 left
/// open: `pg-parse::Morpher::is_word_valid_traced`'s final gate now reports exactly which of the 11
/// `FailureReason`s in this function's cross-reference table (design doc §3.1/§3.2) rejected the
/// word, at the first morph occurrence that fails.
pub fn allomorphs_valid_cached_traced(
    g: &Grammar,
    w: &Word,
    cache: &RuleCache,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> bool {
    allomorphs_valid_impl(g, w, EnvCheck::Cached(cache), trace, parent)
}

/// Emit `Failed(parent, w, reason)` (guarded by `is_tracing()`) and return `false` -- every early
/// return in [`allomorphs_valid_impl`] goes through this so the trace call and the `false` result
/// can never drift apart.
fn fail(trace: &dyn TraceSink, parent: TraceHandle, w: &Word, reason: FailureReason) -> bool {
    if trace.is_tracing() {
        trace.failed(parent, w, reason);
    }
    false
}

fn allomorphs_valid_impl(
    g: &Grammar,
    w: &Word,
    check: EnvCheck,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> bool {
    let parent = w.trace.unwrap_or(parent);
    if w.morphs.is_empty() {
        return true;
    }
    let mut sorted: Vec<&MorphRecord> = w.morphs.iter().collect();
    sorted.sort_by_key(|m| m.order);

    // C# `Word.Allomorphs.Count` (Word.cs:121-124): `_allomorphs` is a dict keyed by allomorph id,
    // so this is the count of *distinct* allomorphs used, not the number of morph records/pieces.
    let distinct_count = {
        let mut ids: Vec<u32> = sorted.iter().map(|m| m.allomorph.0).collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    };

    // Last interior index of the final shape (anchors excluded): `shape.len()` always counts the
    // two anchors (the `p - 1` convention `morph.rs::copy_part` and friends already rely on).
    let last_interior = w.shape.len().saturating_sub(3) as u32;

    // W6: `Word.AllomorphsInMorphOrder` projected to each id space the co-occurrence rules key on
    // (C# `MorphCoOccurrenceRule<T>.GetMorphObject`) — computed once, since it's the same list for
    // every morph checked below (the word itself doesn't change across this loop).
    let morph_list_allomorphs: Vec<AllomorphId> = sorted.iter().map(|m| m.allomorph).collect();
    let morph_list_morphemes: Vec<MorphemeId> = sorted.iter().map(|m| m.morpheme).collect();

    for (i, m) in sorted.iter().enumerate() {
        let start = m.order;
        let end = sorted
            .get(i + 1)
            .map_or(last_interior, |n| n.order.saturating_sub(1));

        // P11 §4.4-1: the fabricated (guessed) root has no `allomorph_owners` row at all — the
        // one place this indexing would otherwise panic. Delegate every check to the REAL
        // lexical-pattern allomorph the guess was matched against (`Word::guessed_root`), never
        // to `g.allomorph_owners[AllomorphId::GUESSED]`.
        if m.allomorph == AllomorphId::GUESSED {
            let Some(gr) = m
                .runtime_root
                .as_ref()
                .and_then(|root| match root.as_ref() {
                    crate::word::RuntimeRoot::Guessed(root) => Some(root),
                    crate::word::RuntimeRoot::Supplied(_) => None,
                })
            else {
                // A supplied ordinary root, or a supplied compound non-head whose record has
                // been folded into the head word, has no grammar allomorph restrictions.
                continue;
            };
            let allos = &g.entries[gr.pattern_entry.0 as usize].allomorphs;
            let idx = allos
                .iter()
                .position(|a| a.id == gr.pattern_allo)
                .expect("guessed_root.pattern_allo must be one of its owning entry's allomorphs");
            let def = &allos[idx];
            if def.is_bound && distinct_count == 1 {
                return fail(trace, parent, w, FailureReason::BoundRoot);
            }
            // W5 stem-name gate, PRIMARY clause only: the "exclude sibling stem names" loop
            // iterates the FABRICATED entry's allomorphs in C# — exactly one, itself — so it is a
            // no-op here; do NOT iterate the pattern's real siblings (§4.4-1).
            if let Some(sn) = def.stem_name {
                if !stem_name_required_match(g, sn, &w.syn_fs) {
                    return fail(trace, parent, w, FailureReason::RequiredStemName);
                }
            }
            // W6: the pattern's own rule lists, keyed on the GUESSED (sentinel) ids as the
            // primary — mirrors C#'s fabricated-object identity; the sentinel correctly never
            // equals any real id in `morph_list_allomorphs`/`morph_list_morphemes`.
            if !allomorph_co_occurrence_ok(&def.co_occurrence, m.allomorph, &morph_list_allomorphs)
            {
                return fail(trace, parent, w, FailureReason::AllomorphCoOccurrenceRules);
            }
            let pattern_morpheme = g.entries[gr.pattern_entry.0 as usize].morpheme;
            if !morpheme_co_occurrence_rules_ok(
                &g.morphemes[pattern_morpheme.0 as usize].co_occurrence,
                m.morpheme,
                &morph_list_morphemes,
            ) {
                return fail(trace, parent, w, FailureReason::MorphemeCoOccurrenceRules);
            }
            // Environments are identical objects to the pattern's in C# — reuse the pattern
            // allomorph's own (cached) matcher, no per-guess compilation.
            if !check.envs_ok(g, gr.pattern_allo, &def.environments, &w.shape, start, end) {
                return fail(trace, parent, w, FailureReason::Environments);
            }
            // No W3.2 disjunctive re-check: the fabricated entry has exactly one allomorph
            // (itself), so C#'s `Enumerable.Range(0, Index)`/passed-over candidate set is empty.
            continue;
        }

        match g.allomorph_owners[m.allomorph.0 as usize] {
            AllomorphOwner::Root(le, idx) => {
                let allos = &g.entries[le.0 as usize].allomorphs;
                let def = &allos[idx as usize];
                if def.is_bound && distinct_count == 1 {
                    return fail(trace, parent, w, FailureReason::BoundRoot);
                }
                // W5 (`RootAllomorph.CheckAllomorphConstraints`'s `StemName` clause,
                // RootAllomorph.cs:65-91): between the bound-root gate and the W6 co-occurrence
                // gates, matching C#'s exact order.
                if let Some(reason) = stem_name_gate_reason(g, allos, idx as usize, &w.syn_fs) {
                    return fail(trace, parent, w, reason);
                }
                // W6 (`RootAllomorph.CheckAllomorphConstraints` -> `base.CheckAllomorphConstraints`,
                // Allomorph.cs:93,158-179): allomorph-level rules, then morpheme-level rules, both
                // BEFORE the environments check below (C#'s exact `CheckAllomorphConstraints`-then-
                // `Environments` order in `Allomorph.IsWordValid`).
                if !allomorph_co_occurrence_ok(
                    &def.co_occurrence,
                    m.allomorph,
                    &morph_list_allomorphs,
                ) {
                    return fail(trace, parent, w, FailureReason::AllomorphCoOccurrenceRules);
                }
                if !morpheme_co_occurrence_ok(g, m.morpheme, &morph_list_morphemes) {
                    return fail(trace, parent, w, FailureReason::MorphemeCoOccurrenceRules);
                }
                if !check.envs_ok(g, m.allomorph, &def.environments, &w.shape, start, end) {
                    return fail(trace, parent, w, FailureReason::Environments);
                }
                // W3.2 disjunctive re-check (Allomorph.cs:127-152), root arm: an earlier-indexed
                // allomorph of the same lexical entry that doesn't free-fluctuate with the used
                // one, whose environments are absent-or-satisfied at this morph's span, and whose
                // own constraints hold (`RootAllomorph.CheckAllomorphConstraints` = the bound-root
                // gate plus, per W6, the candidate's OWN allomorph-co-occurrence rules — Allomorph.
                // cs:137's `disjunctiveAllomorph.CheckAllomorphConstraints(null, this, word)` reads
                // `disjunctiveAllomorph.AllomorphCoOccurrenceRules` but keys `IsWordValid` on the
                // ORIGINALLY USED allomorph, i.e. `this` = `m.allomorph`, not `cand.id`; the
                // morpheme-level rules are the SAME set/key as the primary check just above, since
                // disjunctive alternatives share one morpheme, so re-checking them here would be a
                // provable no-op and is omitted) rejects the word — the first-listed matching
                // allomorph wins.
                for ci in disjunctive_candidates(m, idx as usize) {
                    let cand = &allos[ci];
                    if free_fluctuates(allos, ci, idx as usize, root_constraints_equal) {
                        continue;
                    }
                    if check.envs_ok(g, cand.id, &cand.environments, &w.shape, start, end)
                        && !(cand.is_bound && distinct_count == 1)
                        && stem_name_gates_ok(g, allos, ci, &w.syn_fs)
                        && allomorph_co_occurrence_ok(
                            &cand.co_occurrence,
                            m.allomorph,
                            &morph_list_allomorphs,
                        )
                    {
                        return fail(trace, parent, w, FailureReason::DisjunctiveAllomorph);
                    }
                }
            }
            AllomorphOwner::Affix(mr, idx) => {
                // C# `AffixProcessAllomorph.CheckAllomorphConstraints` is shared verbatim by
                // `AffixProcessRule` and `RealizationalAffixProcessRule` allomorphs (both are the
                // same C# class — `MorphRuleDef::affix_allomorphs` centralizes the lookup, see its
                // doc); `Morpher.IsWordValid`'s `word.Allomorphs.All(...)` walk doesn't care which
                // rule kind owns the allomorph it's validating.
                let allos = g.mrules[mr.0 as usize].affix_allomorphs().expect(
                    "compounding rules mint no AllomorphId (no per-allomorph registry entry)",
                );
                let def = &allos[idx as usize];
                if !pg_featstruct::subsumes(g.fs_interner.get(def.required_syn_fs), &w.syn_fs) {
                    return fail(
                        trace,
                        parent,
                        w,
                        FailureReason::RequiredSyntacticFeatureStruct,
                    );
                }
                // W6 (`AffixProcessAllomorph.CheckAllomorphConstraints` -> `base.
                // CheckAllomorphConstraints`, AffixProcessAllomorph.cs:104): same ordering as the
                // root arm above.
                if !allomorph_co_occurrence_ok(
                    &def.co_occurrence,
                    m.allomorph,
                    &morph_list_allomorphs,
                ) {
                    return fail(trace, parent, w, FailureReason::AllomorphCoOccurrenceRules);
                }
                if !morpheme_co_occurrence_ok(g, m.morpheme, &morph_list_morphemes) {
                    return fail(trace, parent, w, FailureReason::MorphemeCoOccurrenceRules);
                }
                if !check.envs_ok(g, m.allomorph, &def.environments, &w.shape, start, end) {
                    return fail(trace, parent, w, FailureReason::Environments);
                }
                // W3.2 disjunctive re-check, affix arm: candidates are the passed-over subrule
                // indices recorded during synthesis ([`MorphRecord::passed_over`]); the candidate's
                // `AffixProcessAllomorph.CheckAllomorphConstraints` is its required-syntactic-FS
                // subsumption against the word's accumulated syn FS, plus (W6) its own allomorph-
                // co-occurrence rules keyed on the originally used allomorph — same rationale as
                // the root arm's disjunctive loop above.
                for ci in disjunctive_candidates(m, idx as usize) {
                    let cand = &allos[ci];
                    if free_fluctuates(allos, ci, idx as usize, |a, b| {
                        crate::morph::constraints_equal(g, a, b)
                    }) {
                        continue;
                    }
                    if check.envs_ok(g, cand.id, &cand.environments, &w.shape, start, end)
                        && pg_featstruct::subsumes(
                            g.fs_interner.get(cand.required_syn_fs),
                            &w.syn_fs,
                        )
                        && allomorph_co_occurrence_ok(
                            &cand.co_occurrence,
                            m.allomorph,
                            &morph_list_allomorphs,
                        )
                    {
                        return fail(trace, parent, w, FailureReason::DisjunctiveAllomorph);
                    }
                }
            }
        }
    }
    true
}
