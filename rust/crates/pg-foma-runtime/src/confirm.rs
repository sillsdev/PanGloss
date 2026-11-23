//! Fresh port of `hc-hybrid/src/replay.rs`'s confirm half (module doc there, `replay.rs:1-49`,
//! esp. the quirk-8 `RuleRef` mapping) — attribution comments throughout, NO dependency on
//! `hc-hybrid` (that crate is being sunset, plan D8). Adapted to this crate's own
//! `crate::tags::Candidate` (the FST-tag-decoded candidate shape, plan D2 — `MorphemeId`
//! sequence + `root_index`, not `hc-hybrid/src/walk.rs`'s own candidate type) and to plan D4's
//! multiplicity recovery: `confirm_all` collects EVERY matching analysis the pinned
//! `parse_word_selected` outcome contains, not just the first the way the original's `confirm`
//! (`replay.rs:118-192`, `.find()`) did — the engine returns a genuine multiset (Sena `mbali`: 8),
//! and D4 requires restoring it rather than silently collapsing to one hit per candidate.
//!
//! Each collected match is paired with its own `(morpheme-join, surface)` display-string pair —
//! `pg_parse::ParseOutcome::analyses[i]` is, by that struct's own doc (`pg-parse/src/morpher.rs:
//! 79-120`), built from the exact same traversal as `structured[i]` and shares its index, so
//! zipping the two `Vec`s together before filtering (rather than re-deriving the strings some other
//! way afterward) is what keeps a matched analysis's numeric ids and display strings describing the
//! same thing.

use std::time::Duration;

use rustc_hash::FxHashSet as HashSet;

use pg_grammar::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef, MorphemeId};
use pg_parse::{Morpher, ParseOptions, ParseOutcome, WordAnalysis as EngineAnalysis};
use pg_rules::stratum::RuleRef;
use pg_rules::trace::TraceSink;

use crate::tags::Candidate;

/// Per-candidate buckets of confirmed matches; outer index parallels the candidate slice. See `confirm_batch`'s doc for the collection rationale.
type ConfirmedBuckets = Vec<Vec<(EngineAnalysis, String, String)>>;

/// How many rules beyond a chunk's largest member's own rule set the chunk's union may admit
/// (see `confirm_batch`'s doc). 0 = exact-filter grouping (never merges rule-diverse
/// candidates); large = per-root-set full union (merges everything, risks near-cross-product
/// searches on rule-diverse words). 3 measured best on the Sena 40-word set.
pub const RULE_UNION_SLACK: usize = 3;

/// True execution topology for one `confirm_batch_with_diagnostics` call. `confirmation_groups`
/// counts fused internal work groups; each group currently performs exactly one restricted
/// `Morpher::parse_word_selected` call, counted separately in `confirmation_calls`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConfirmBatchDiagnostics {
    pub confirmation_groups: usize,
    pub confirmation_calls: usize,
    /// Total work steps the full-HC oracle actually consumed across every call in this batch
    /// (`pg_parse::ParseOutcome::work_steps`).
    ///
    /// A finer unit than `confirmation_calls`, and the one worth ranking on: a call is not a constant
    /// amount of work, because a long word costs far more to adjudicate than a short one, so counting
    /// calls under-weights exactly the expensive words that dominate real cost. It is also the unit
    /// HC work is bounded in (`backend_runtime::DEFAULT_ORACLE_WORK_CAP` caps the same total), so the
    /// objective and the cap speak the same language. Deterministic, like the counts beside it.
    pub confirmation_work: usize,
    pub incomplete: bool,
    /// Which limits stopped a confirming parse; `incomplete` is true iff any is set.
    pub stop: pg_parse::StopReasons,
}

/// What one fused confirmation chunk cost, and which candidates shared that cost.
///
/// This is the finest granularity confirmation has: `confirm_batch` groups candidates and runs one
/// restricted reparse per group, so the cost of a group is a single measurement no matter how many
/// candidates are in it. A candidate absent from every chunk cost nothing at all — pin resolution
/// turned it away before any parse — and a candidate that is a chunk's only member owns that
/// chunk's whole figure. Anything in between is shared and not divisible by this record.
///
/// `work_steps` is the deterministic work spent in this chunk; `elapsed` measures the same call.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConfirmChunkCost {
    /// Indices into the candidate slice that was confirmed, ascending.
    pub members: Vec<usize>,
    pub work_steps: usize,
    pub elapsed: Duration,
    /// Whether the word deadline fired during this chunk's reparse. When it did, `work_steps` is a
    /// truncated figure and the chunk's empty buckets are not evidence that anything was rejected —
    /// a reader that ignores this flag will count abandoned work as removable work.
    pub timed_out: bool,
}

/// Which grammar object owns a given `MorphemeId` — ported from `hc-hybrid/src/replay.rs`'s
/// `MorphemeOwner` (`replay.rs:70-74`) verbatim. See that module's doc for the full quirk-8
/// rationale (why a `CompoundingRule` never owns a morpheme and so is never this enum's `MRule`
/// variant).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MorphemeOwner {
    LexEntry(LexEntryId),
    MRule(MRuleId),
}

/// `replay.rs::build_morpheme_owners` (`replay.rs:82-98`), ported verbatim onto this crate's own
/// `Grammar`/`MorphemeId` types (same crate, `pg-grammar`, as the original — no adaptation needed
/// beyond the module it lives in).
pub fn build_morpheme_owners(g: &Grammar) -> Vec<Option<MorphemeOwner>> {
    let mut owners = vec![None; g.morphemes.len()];
    for (i, e) in g.entries.iter().enumerate() {
        owners[e.morpheme.0 as usize] = Some(MorphemeOwner::LexEntry(LexEntryId(i as u32)));
    }
    for (i, r) in g.mrules.iter().enumerate() {
        let morpheme = match r {
            MorphRuleDef::AffixProcess(def) => Some(def.morpheme),
            MorphRuleDef::Realizational(def) => Some(def.morpheme),
            MorphRuleDef::Compounding(_) => None,
        };
        if let Some(m) = morpheme {
            owners[m.0 as usize] = Some(MorphemeOwner::MRule(MRuleId(i as u32)));
        }
    }
    owners
}

fn owner_of(owners: &[Option<MorphemeOwner>], m: MorphemeId) -> Option<MorphemeOwner> {
    owners.get(m.0 as usize).copied().flatten()
}

/// Positional identity comparison, element-wise not set-wise: morphemes in the wrong order or wrong `root_index` is a silent loss, never a false negative match.
fn analyses_match(wa: &EngineAnalysis, candidate: &Candidate) -> bool {
    wa.root_morpheme_index == candidate.root_index
        && wa.morpheme_ids.len() == candidate.morphemes.len()
        && wa
            .morpheme_ids
            .iter()
            .zip(candidate.morphemes.iter())
            .all(|(&a, &b)| a == b.0)
}

/// Confirms one candidate with the same lexical pins and exact identity routing as `confirm_batch`.
/// The morpher must be uncapped so confirmation cannot silently discard an accepted derivation.
pub fn confirm_all(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidate: &Candidate,
    word: &str,
) -> Vec<(EngineAnalysis, String, String)> {
    confirm_batch(g, owners, morpher, std::slice::from_ref(candidate), word)
        .pop()
        .unwrap_or_default()
}

/// One candidate's resolved pins: root entry, non-root rule set, extra compound roots; `None` when the root isn't a `LexEntry` or a non-root morpheme is unowned.
pub(crate) struct CandidatePins {
    pub(crate) root_entry: LexEntryId,
    pub(crate) rules: HashSet<MRuleId>,
    pub(crate) extra_roots: HashSet<LexEntryId>,
}

/// The pure fact both confirmation and the candidate filter's structural passes read: a candidate
/// whose pins do not resolve is one this module's own restricted reparse never runs, so its
/// confirmation bucket stays empty.
pub(crate) fn resolve_pins(
    owners: &[Option<MorphemeOwner>],
    candidate: &Candidate,
) -> Option<CandidatePins> {
    if candidate.root_index < 0 || candidate.root_index as usize >= candidate.morphemes.len() {
        return None;
    }
    let root_index = candidate.root_index as usize;
    let root_entry = match owner_of(owners, candidate.morphemes[root_index]) {
        Some(MorphemeOwner::LexEntry(le)) => le,
        _ => return None, // replay.rs:38-41 — the designated root must be a LexEntry.
    };
    let mut rules: HashSet<MRuleId> = HashSet::default();
    let mut extra_roots: HashSet<LexEntryId> = HashSet::default();
    for (i, &m) in candidate.morphemes.iter().enumerate() {
        if i == root_index {
            continue;
        }
        match owner_of(owners, m) {
            Some(MorphemeOwner::LexEntry(le)) => {
                extra_roots.insert(le);
            }
            Some(MorphemeOwner::MRule(mid)) => {
                rules.insert(mid);
            }
            None => return None, // replay.rs:56-59 — neither a LexEntry nor a rule -> None.
        }
    }
    Some(CandidatePins {
        root_entry,
        rules,
        extra_roots,
    })
}

/// Census helper for candidate-failure attribution: run exactly the same
/// restricted reparse `confirm_all` would for ONE candidate (same pin resolution via
/// `resolve_pins`, same tight per-candidate filter — root set + exact rule set, no slack) but
/// return the raw `ParseOutcome` instead of routing matches into a bucket, and accept a
/// caller-supplied `TraceSink` so the census can classify *why* a failing candidate failed
/// (validity-gate `FailureReason`, via the trace tree, vs. `candidates_generated == 0` meaning
/// the unapply/synthesis cascade never produced a single candidate to test) — WITHOUT touching
/// the timed paths (`confirm_batch`/`confirm_all`, both still `NoopSink`-only, unchanged).
/// `None` when the candidate's pins don't resolve (mirrors `confirm_all`'s empty-result case for
/// the same inputs — `resolve_pins`'s doc explains the two rejection cases).
///
/// Census-only instrumentation: traces one candidate's confirm outcome without touching the timed
/// batch/all paths above.
pub fn confirm_one_traced(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidate: &Candidate,
    word: &str,
    trace: &dyn TraceSink,
) -> Option<ParseOutcome> {
    let pins = resolve_pins(owners, candidate)?;
    let lex_entry_filter =
        move |le: LexEntryId| le == pins.root_entry || pins.extra_roots.contains(&le);
    let rule_filter = move |r: RuleRef| rule_is_allowed(g, &pins.rules, r);
    Some(morpher.parse_word_selected_traced(
        word,
        &ParseOptions::default(),
        trace,
        Some(&lex_entry_filter),
        Some(&rule_filter),
    ))
}

/// Batched confirm ("one reparse for the union of candidates" — predicted to go from 122
/// reparses to 4 sets of around 30, borne out by measurement): candidates are grouped by their
/// ROOT SET (designated root + extra compound
/// roots), and each group gets ONE `parse_word_selected` run whose filters admit the union of
/// that group's rules; returned analyses are routed to the candidate they positionally match.
/// Returns one bucket per candidate, preserving exact morpheme order and root position.
/// Candidates share a reparse when their rule sets match, with lexical pins unioned across roots.
/// Compounding remains available even for a single-root identity: repeated use of one allomorph
/// collapses to one identity in the full parser. Exact identity routing rejects unrelated results.
///
/// Discards whether a confirming parse stopped on a limit; use `confirm_batch_checked` wherever a limit can be armed.
pub fn confirm_batch(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidates: &[Candidate],
    word: &str,
) -> ConfirmedBuckets {
    confirm_batch_impl(g, owners, morpher, candidates, word, None, None).0
}

/// `confirm_batch` that also reports which limits stopped a confirming parse; when any did, the buckets are a prefix of the true analyses and the caller must treat the word as incomplete.
pub fn confirm_batch_checked(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidates: &[Candidate],
    word: &str,
) -> (ConfirmedBuckets, pg_parse::StopReasons) {
    confirm_batch_impl(g, owners, morpher, candidates, word, None, None)
}

/// `confirm_batch_attributed` that also reports which limits stopped a confirming parse; see `confirm_batch_checked`.
pub fn confirm_batch_attributed_checked(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidates: &[Candidate],
    word: &str,
) -> (
    ConfirmedBuckets,
    pg_parse::StopReasons,
    Vec<ConfirmChunkCost>,
) {
    let mut chunks = Vec::new();
    let (buckets, stop) = confirm_batch_impl(
        g,
        owners,
        morpher,
        candidates,
        word,
        None,
        Some(&mut chunks),
    );
    (buckets, stop, chunks)
}

/// `confirm_batch`, plus a record of what each fused chunk cost and who was in it.
///
/// The buckets are byte-identical to `confirm_batch`'s; the second return value is pure
/// instrumentation, and reading it changes nothing about what is confirmed.
pub fn confirm_batch_attributed(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidates: &[Candidate],
    word: &str,
) -> (ConfirmedBuckets, Vec<ConfirmChunkCost>) {
    let (buckets, _, chunks) =
        confirm_batch_attributed_checked(g, owners, morpher, candidates, word);
    (buckets, chunks)
}

/// The plain-path confirmation: no topology accounting, only the stop flags every consumer needs.
pub(crate) fn confirm_batch_stopping(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidates: &[Candidate],
    word: &str,
) -> (ConfirmedBuckets, ConfirmBatchDiagnostics) {
    let (buckets, stop) = confirm_batch_checked(g, owners, morpher, candidates, word);
    let diagnostics = ConfirmBatchDiagnostics {
        stop,
        incomplete: stop.any(),
        ..ConfirmBatchDiagnostics::default()
    };
    (buckets, diagnostics)
}

pub(crate) fn confirm_batch_with_diagnostics(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidates: &[Candidate],
    word: &str,
) -> (ConfirmedBuckets, ConfirmBatchDiagnostics) {
    let mut diagnostics = ConfirmBatchDiagnostics::default();
    let (buckets, stop) = confirm_batch_impl(
        g,
        owners,
        morpher,
        candidates,
        word,
        Some(&mut diagnostics),
        None,
    );
    diagnostics.stop = stop;
    diagnostics.incomplete = stop.any();
    (buckets, diagnostics)
}

pub(crate) fn confirm_batch_attributed_with_diagnostics(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidates: &[Candidate],
    word: &str,
) -> (
    ConfirmedBuckets,
    ConfirmBatchDiagnostics,
    Vec<ConfirmChunkCost>,
) {
    let mut diagnostics = ConfirmBatchDiagnostics::default();
    let mut chunks = Vec::new();
    let (buckets, stop) = confirm_batch_impl(
        g,
        owners,
        morpher,
        candidates,
        word,
        Some(&mut diagnostics),
        Some(&mut chunks),
    );
    diagnostics.stop = stop;
    diagnostics.incomplete = stop.any();
    (buckets, diagnostics, chunks)
}

fn rule_is_allowed(g: &Grammar, rules: &HashSet<MRuleId>, rule: RuleRef) -> bool {
    match rule {
        RuleRef::Stratum(_) | RuleRef::Template(_) => true,
        RuleRef::MRule(id) => {
            rules.contains(&id) || matches!(g.mrules[id.0 as usize], MorphRuleDef::Compounding(_))
        }
    }
}

/// The one checked restricted reparse, shared by `confirm_batch_impl` and `confirm_pin_groups` so the two entries cannot disagree on whether a word stopped. The stop expression is `ParseOutcome::stop_reasons`, exactly as `confirm_batch_impl` read it before this helper existed; `None` filters mean an unfiltered parse.
fn checked_reparse(
    morpher: &Morpher,
    word: &str,
    lex_entry_filter: Option<&dyn Fn(LexEntryId) -> bool>,
    rule_filter: Option<pg_rules::stratum::RuleFilter>,
    budget: &pg_rules::stratum::WordBudget,
) -> (ParseOutcome, pg_parse::StopReasons) {
    let outcome = morpher.parse_word_selected_with_budget(
        word,
        &ParseOptions::default(),
        lex_entry_filter,
        rule_filter,
        budget,
    );
    let stopped = outcome.stop_reasons();
    (outcome, stopped)
}

/// One pin group: the head roots and rule ids a restricted reparse admits. Strata, templates and compounding rules are always allowed (`rule_is_allowed`); non-head roots are not filtered by HC.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PinGroup {
    /// Head roots only. A head root must be in exactly one group of one `confirm_pin_groups` call.
    pub roots: HashSet<LexEntryId>,
    pub rules: HashSet<MRuleId>,
}

/// Result of `confirm_pin_groups` or `confirm_unfiltered`.
pub struct PinnedConfirmation {
    /// Every analysis the reparses returned, concatenated in group order, never de-duplicated: HC's multiset is kept.
    pub analyses: Vec<(EngineAnalysis, String, String)>,
    /// The union of the stop flags of every reparse, always computed; `stop.any()` means `analyses` may be a prefix.
    pub stop: pg_parse::StopReasons,
    /// The allowance's final cumulative work count (`WordBudget::work_steps`), not a sum over groups.
    pub work_steps: u64,
    /// How many reparses ran.
    pub reparses: usize,
}

impl PinnedConfirmation {
    pub const fn incomplete(&self) -> bool {
        self.stop.any()
    }
}

/// Restricted reparses of `word`, one per pin group, sharing the caller's `budget`.
///
/// Groups run in ascending order of their smallest root id (a group without roots sorts last, ties by position), so which groups ran before a cap never depends on hashing. Head roots must be disjoint across groups (asserted): the lexical-entry filter acts only at head lookup, so an analysis headed by root `r` comes from exactly the one reparse whose group holds `r`, with its full multiplicity, and the results are concatenated. Every analysis returned is one plain HC returns for `word`; recall rests on the caller's groups covering a derivation's head root and rules.
///
/// Analyses from a group whose reparse stopped are kept, not dropped: when `incomplete()` they may be a prefix and the caller must discard them. An empty `groups` slice runs no reparse and returns no analyses and no stop; that is not a rejection unless the proposal that produced the groups was itself complete.
///
/// Additive to `confirm_batch`: nothing there changes behaviour.
pub fn confirm_pin_groups(
    g: &Grammar,
    morpher: &Morpher,
    word: &str,
    groups: &[PinGroup],
    budget: &pg_rules::stratum::WordBudget,
) -> PinnedConfirmation {
    let mut result = reparse_groups(g, morpher, word, groups, &mut |_| budget);
    result.work_steps = budget.work_steps() as u64;
    result
}

/// The group loop behind `confirm_pin_groups`; `budget_for(k)` names the allowance of the k-th reparse in run order, so a test can give groups separate allowances. `work_steps` is left 0 for the caller.
fn reparse_groups<'b>(
    g: &Grammar,
    morpher: &Morpher,
    word: &str,
    groups: &[PinGroup],
    budget_for: &mut dyn FnMut(usize) -> &'b pg_rules::stratum::WordBudget,
) -> PinnedConfirmation {
    let mut seen: HashSet<LexEntryId> = HashSet::default();
    for group in groups {
        for root in &group.roots {
            assert!(
                seen.insert(*root),
                "confirm_pin_groups: head root {root:?} is in more than one pin group"
            );
        }
    }
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_by_key(|&i| {
        (
            groups[i]
                .roots
                .iter()
                .map(|r| r.0)
                .min()
                .unwrap_or(u32::MAX),
            i,
        )
    });

    let mut result = PinnedConfirmation {
        analyses: Vec::new(),
        stop: pg_parse::StopReasons::default(),
        work_steps: 0,
        reparses: 0,
    };
    for (k, i) in order.into_iter().enumerate() {
        let group = &groups[i];
        let lex_entry_filter = |le: LexEntryId| group.roots.contains(&le);
        let rule_filter = |r: RuleRef| rule_is_allowed(g, &group.rules, r);
        let (outcome, stopped) = checked_reparse(
            morpher,
            word,
            Some(&lex_entry_filter),
            Some(&rule_filter),
            budget_for(k),
        );
        result.reparses += 1;
        result.stop = result.stop.union(stopped);
        result.analyses.extend(
            outcome
                .structured
                .into_iter()
                .zip(outcome.analyses)
                .map(|(wa, (join, surface))| (wa, join, surface)),
        );
    }
    result
}

/// The unfiltered call: one reparse with no lexical or rule filter (plain HC through the same checked helper), for a caller that decides a restricted parse would prune nothing worth a filter.
pub fn confirm_unfiltered(
    morpher: &Morpher,
    word: &str,
    budget: &pg_rules::stratum::WordBudget,
) -> PinnedConfirmation {
    let (outcome, stop) = checked_reparse(morpher, word, None, None, budget);
    PinnedConfirmation {
        analyses: outcome
            .structured
            .into_iter()
            .zip(outcome.analyses)
            .map(|(wa, (join, surface))| (wa, join, surface))
            .collect(),
        stop,
        work_steps: budget.work_steps() as u64,
        reparses: 1,
    }
}

fn confirm_batch_impl(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidates: &[Candidate],
    word: &str,
    mut diagnostics: Option<&mut ConfirmBatchDiagnostics>,
    mut costs: Option<&mut Vec<ConfirmChunkCost>>,
) -> (ConfirmedBuckets, pg_parse::StopReasons) {
    let mut stop = pg_parse::StopReasons::default();
    let mut buckets: ConfirmedBuckets = (0..candidates.len()).map(|_| Vec::new()).collect();

    let pins: Vec<Option<CandidatePins>> =
        candidates.iter().map(|c| resolve_pins(owners, c)).collect();

    // 1) Group candidate indices by root set (designated root + extra roots), first-seen order.
    let mut root_groups: Vec<(Vec<u32>, Vec<usize>)> = Vec::new();
    for (i, p) in pins.iter().enumerate() {
        let Some(p) = p else { continue };
        let mut roots: Vec<u32> = std::iter::once(p.root_entry.0)
            .chain(p.extra_roots.iter().map(|le| le.0))
            .collect();
        roots.sort_unstable();
        match root_groups.iter_mut().find(|(k, _)| *k == roots) {
            Some((_, members)) => members.push(i),
            None => root_groups.push((roots, vec![i])),
        }
    }

    // 2) Sub-chunk each root group: a member joins only if the chunk's rule union stays within RULE_UNION_SLACK of the largest member's own set.
    struct Chunk {
        members: Vec<usize>,
        union_rules: HashSet<MRuleId>,
        max_member_rules: usize,
    }
    let mut work: Vec<(Vec<u32>, Chunk)> = Vec::new();
    for (root_key, members) in root_groups {
        let mut chunks: Vec<Chunk> = Vec::new();
        for &i in &members {
            let p = pins[i].as_ref().expect("grouped members always have pins");
            let placed = chunks.iter_mut().any(|ch| {
                let would_union = ch.union_rules.union(&p.rules).count();
                let would_max = ch.max_member_rules.max(p.rules.len());
                if would_union <= would_max + RULE_UNION_SLACK {
                    ch.union_rules.extend(p.rules.iter().copied());
                    ch.max_member_rules = would_max;
                    ch.members.push(i);
                    true
                } else {
                    false
                }
            });
            if !placed {
                chunks.push(Chunk {
                    members: vec![i],
                    union_rules: p.rules.clone(),
                    max_member_rules: p.rules.len(),
                });
            }
        }
        for ch in chunks {
            work.push((root_key.clone(), ch));
        }
    }

    // Exact rule-set keys keep fused analysis searches identical.
    struct FusedChunk {
        /// Union of every fused member's own root set, sorted+deduped; widens only `lex_entry_filter`.
        root_keys: Vec<u32>,
        members: Vec<usize>,
        union_rules: HashSet<MRuleId>,
    }
    let mut fused: rustc_hash::FxHashMap<Vec<u32>, FusedChunk> = rustc_hash::FxHashMap::default();
    for (root_key, chunk) in work {
        let mut rule_ids: Vec<u32> = chunk.union_rules.iter().map(|r| r.0).collect();
        rule_ids.sort_unstable();
        fused
            .entry(rule_ids)
            .and_modify(|f| {
                for r in &root_key {
                    if let Err(pos) = f.root_keys.binary_search(r) {
                        f.root_keys.insert(pos, *r);
                    }
                }
                f.members.extend(&chunk.members);
            })
            .or_insert_with(|| FusedChunk {
                root_keys: root_key,
                members: chunk.members,
                union_rules: chunk.union_rules,
            });
    }

    if let Some(diagnostics) = diagnostics.as_deref_mut() {
        diagnostics.confirmation_groups = fused.len();
    }

    let budget = morpher.word_budget();
    for chunk in fused.values() {
        let members = &chunk.members;
        let union_rules = &chunk.union_rules;
        let root_key = &chunk.root_keys;

        // Routes each outcome analysis to the group member it positionally matches; `entry().or_insert()` keeps first-wins semantics for any duplicate key.
        let mut by_key: rustc_hash::FxHashMap<(Vec<u32>, i32), usize> =
            rustc_hash::FxHashMap::default();
        for &i in members {
            by_key
                .entry((
                    candidates[i].morphemes.iter().map(|m| m.0).collect(),
                    candidates[i].root_index,
                ))
                .or_insert(i);
        }

        let lex_entry_filter = |le: LexEntryId| root_key.binary_search(&le.0).is_ok();
        let rule_filter = |r: RuleRef| rule_is_allowed(g, union_rules, r);

        if let Some(diagnostics) = diagnostics.as_deref_mut() {
            diagnostics.confirmation_calls += 1;
        }
        let timer = crate::word_timer::start();
        let work_before = budget.work_steps();
        let (outcome, stopped) = checked_reparse(
            morpher,
            word,
            Some(&lex_entry_filter),
            Some(&rule_filter),
            &budget,
        );
        let elapsed = timer.elapsed();
        // Read BEFORE the loop below consumes `outcome`.
        if let Some(diagnostics) = diagnostics.as_deref_mut() {
            diagnostics.confirmation_work = outcome.work_steps;
        }
        stop = stop.union(stopped);
        if let Some(costs) = costs.as_deref_mut() {
            let mut members = members.clone();
            members.sort_unstable();
            costs.push(ConfirmChunkCost {
                members,
                work_steps: outcome.work_steps.saturating_sub(work_before),
                elapsed,
                timed_out: outcome.timed_out,
            });
        }

        // `outcome.analyses[i]` and `outcome.structured[i]` describe the same analysis; zip so a routed match keeps both.
        for (wa, (join, surface)) in outcome.structured.into_iter().zip(outcome.analyses) {
            let key = (wa.morpheme_ids.clone(), wa.root_morpheme_index);
            if let Some(&i) = by_key.get(&key) {
                debug_assert!(analyses_match(&wa, &candidates[i]));
                buckets[i].push((wa, join, surface));
            }
        }
    }
    // Chunk order follows a hash map's iteration, so a report keyed on it would not be reproducible.
    if let Some(costs) = costs {
        costs.sort_by(|left, right| left.members.cmp(&right.members));
    }
    (buckets, stop)
}

#[cfg(test)]
mod tests;
