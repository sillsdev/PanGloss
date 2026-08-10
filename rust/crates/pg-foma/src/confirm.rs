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
    /// Total `pg_rules::stratum::StepBudget` ticks the full-HC oracle actually consumed across every
    /// call in this batch (`pg_parse::ParseOutcome::steps`).
    ///
    /// A finer unit than `confirmation_calls`, and the one worth ranking on: a call is not a constant
    /// amount of work, because a long word costs far more to adjudicate than a short one, so counting
    /// calls under-weights exactly the expensive words that dominate real cost. It is also the unit
    /// HC work is already BOUNDED in (`backend_runtime::DEFAULT_ORACLE_STEP_CAP` caps these same
    /// ticks), so measuring in it means the objective and the cap finally speak the same language.
    /// Deterministic, like the counts beside it -- no wall-clock anywhere.
    pub confirmation_steps: usize,
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

/// D4 multiplicity recovery over `replay.rs::confirm`'s lex_entry_filter/rule_filter construction
/// (`replay.rs:118-192`; quirk-8 mapping in that module's own doc — `Stratum`/`Template` always
/// admitted, an `MRule` admitted iff it's one of the candidate's own rules or a `Compounding` rule
/// with extra roots present). `morpher` MUST be built uncapped (`Morpher::new(g, usize::MAX)`,
/// `replay.rs:106-110`'s rationale carries over unchanged: a Rust-side cap here could silently drop
/// a result the full engine would find, which would look like a parity bug rather than the
/// deliberate absence of a work budget it actually is).
///
/// Returns every matching `(engine analysis, morpheme-join string, surface string)` triple in the
/// pinned outcome's own order — empty (never panics) when the candidate's root position isn't a
/// `LexEntry`, when any non-root morpheme resolves to neither a `LexEntry` nor an `MRule`, or when
/// the restricted re-analysis simply confirms nothing.
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
struct CandidatePins {
    root_entry: LexEntryId,
    rules: HashSet<MRuleId>,
    extra_roots: HashSet<LexEntryId>,
}

fn resolve_pins(owners: &[Option<MorphemeOwner>], candidate: &Candidate) -> Option<CandidatePins> {
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
    let any_extra_roots = !pins.extra_roots.is_empty();
    let lex_entry_filter =
        move |le: LexEntryId| le == pins.root_entry || pins.extra_roots.contains(&le);
    let rule_filter = move |r: RuleRef| match r {
        RuleRef::Stratum(_) | RuleRef::Template(_) => true,
        RuleRef::MRule(id) => {
            pins.rules.contains(&id)
                || (any_extra_roots
                    && matches!(g.mrules[id.0 as usize], MorphRuleDef::Compounding(_)))
        }
    };
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
/// Returns one bucket per input candidate (parallel by index), each bucket in its group
/// outcome's own order — content-identical to calling `confirm_all` per candidate.
///
/// Grouping granularity — four strategies measured on the Sena 40-word set (confirm totals):
/// per-candidate 400ms; one global union 339ms but REDISTRIBUTED cost (`kutongera` 5x slower —
/// every homograph root + 28 candidates' rules in one parse is a near-cross-product search);
/// per-root-set groups with full rule union 233ms but `kutongera` still 3x slower; exact
/// (root set, rule set) groups 356ms — never regresses but rarely merges, because real FST
/// candidates mostly differ in rule SETS, not just morpheme order.
///
/// **What this implements — root-set groups, sub-chunked by bounded rule-union slack, THEN
/// fused across root sets on an exact rule-filter match:**
/// 1. candidates are grouped by root set (designated root + extra compound roots — the lexicon
///    pin stays exactly as tight as per-candidate confirm), then greedily sub-chunked so that a
///    chunk's rule-set UNION never exceeds its largest member's own rule set by more than
///    `RULE_UNION_SLACK` rules. Homogeneous candidate families (shared rule core, the antumira/
///    kakamwe shape) merge into a few parses; rule-diverse families (the kutongera shape) fall
///    back toward tight per-candidate parses automatically.
/// 2. **Cross-root-set fusion** (the "identical morpheme-derivation, different
///    proposed root" redundancy a tracer found costing ~1.8s of one Amharic word's *unbatched*
///    confirm, and — measured after step 1 above already existed — still ~54% of the *batched*
///    Sena confirm total): the analysis-phase mrule/template unapplication cascade
///    (`pg_parse::Morpher::parse_word_core_selected`'s step 2) never reads `lex_entry_filter` —
///    that closure is threaded into exactly one place, the step-3 `lexical_lookup_filtered` call
///    — so two chunks from *different* root-set groups that happen to need the exact same
///    `rule_filter` predicate (same admitted-`MRuleId` SET, same `Compounding`-admission flag)
///    are provably computing the byte-identical analysis-phase result; they differ only in which
///    root entries get admitted at the cheap step-3/4 lexical-lookup+synthesis stage. Such chunks
///    are fused into ONE `parse_word_selected` call whose `lex_entry_filter` is the union of the
///    constituent root keys and whose `rule_filter` set is left untouched (carried through
///    explicitly as `any_extra_roots`, never re-derived from the fused root key's length — see
///    the loop below). Measured on Sena's `cinagumanika`: 4 chunks across 4 distinct root keys
///    shared `rule_ids=[15,21,51,54,58,120,134]` at ~50-66ms apiece; fusing them into one call
///    keeps one ~50ms analysis pass instead of four.
///
///    Deliberately EXACT rule-SET equality only, no slack: approximating here (the way step 1's
///    RULE_UNION_SLACK does within one root set) would re-admit the broadened-rule-filter search
///    blowup that step 1's slack bound exists to avoid, and would demote this fusion from "the
///    analysis phase is provably identical" to merely "safe by downstream positional routing" —
///    the weaker argument the unfused case already relies on for its own correctness.
///
/// **Why a chunk's union parse preserves its members' per-candidate results exactly** (both the
/// within-root-set union of step 1 and the cross-root-set fusion of step 2 — the argument is the
/// same shape for both, since fusion only ever *widens* `lex_entry_filter`, never the rule set):
/// - *No loss:* each member's own filters are a subset of the chunk's (wider admits more, the
///   morpher is uncapped so nothing truncates).
/// - *No spurious gain:* a derivation admitted by the chunk but not by some member's own filters
///   uses a rule or root outside that member's pins — and every such rule/root contributes its
///   own morpheme to the analysis's sequence, so the analysis fails that member's positional
///   match and routes elsewhere (or nowhere). The one morpheme-less rule kind, `Compounding`, is
///   gated per root set: it is admitted iff the root set has extra roots, identical to every
///   member's own flag, and a compound derivation carries the extra root's MORPHEME in its
///   sequence anyway. Fusion carries `any_extra_roots` through unchanged (never recomputed from
///   the fused/union root key), so this gate is untouched by fusion.
/// - *At most one bucket per analysis:* buckets are keyed by exact `(morpheme sequence,
///   root_index)`, distinct after the caller's dedup, so routing is a map lookup. Two fused
///   members always come from *different* root sets, hence different designated root
///   `LexEntryId`s, hence different root `MorphemeId`s at their own `root_index` (`owner_of` is a
///   pure function of morpheme id) — so fused members can never collide on this key either.
pub fn confirm_batch(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidates: &[Candidate],
    word: &str,
) -> ConfirmedBuckets {
    confirm_batch_impl(g, owners, morpher, candidates, word, None)
}

pub(crate) fn confirm_batch_with_diagnostics(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidates: &[Candidate],
    word: &str,
) -> (ConfirmedBuckets, ConfirmBatchDiagnostics) {
    let mut diagnostics = ConfirmBatchDiagnostics::default();
    let buckets = confirm_batch_impl(g, owners, morpher, candidates, word, Some(&mut diagnostics));
    (buckets, diagnostics)
}

fn confirm_batch_impl(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidates: &[Candidate],
    word: &str,
    mut diagnostics: Option<&mut ConfirmBatchDiagnostics>,
) -> ConfirmedBuckets {
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

    // 3) Cross-root-set fusion: merge `work` entries with byte-identical rule_filter predicates (same rule set + any_extra_roots flag), keyed exactly, never slack-based.
    struct FusedChunk {
        /// Union of every fused member's own root set, sorted+deduped; widens only `lex_entry_filter`.
        root_keys: Vec<u32>,
        members: Vec<usize>,
        union_rules: HashSet<MRuleId>,
        /// Carried through from the original chunk, never recomputed from `root_keys` post-fusion, or `Compounding` admission would flip for members that never asked for it.
        any_extra_roots: bool,
    }
    let mut fused: rustc_hash::FxHashMap<(Vec<u32>, bool), FusedChunk> =
        rustc_hash::FxHashMap::default();
    for (root_key, chunk) in work {
        let any_extra_roots = root_key.len() > 1;
        let mut rule_ids: Vec<u32> = chunk.union_rules.iter().map(|r| r.0).collect();
        rule_ids.sort_unstable();
        fused
            .entry((rule_ids, any_extra_roots))
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
                any_extra_roots,
            });
    }

    if let Some(diagnostics) = diagnostics.as_deref_mut() {
        diagnostics.confirmation_groups = fused.len();
    }

    for chunk in fused.values() {
        let members = &chunk.members;
        let union_rules = &chunk.union_rules;
        let any_extra_roots = chunk.any_extra_roots;
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
        let rule_filter = |r: RuleRef| match r {
            RuleRef::Stratum(_) | RuleRef::Template(_) => true,
            RuleRef::MRule(id) => {
                union_rules.contains(&id)
                    || (any_extra_roots
                        && matches!(g.mrules[id.0 as usize], MorphRuleDef::Compounding(_)))
            }
        };

        if let Some(diagnostics) = diagnostics.as_deref_mut() {
            diagnostics.confirmation_calls += 1;
        }
        let outcome = morpher.parse_word_selected(
            word,
            &ParseOptions::default(),
            Some(&lex_entry_filter),
            Some(&rule_filter),
        );
        // Read BEFORE the loop below consumes `outcome`.
        if let Some(diagnostics) = diagnostics.as_deref_mut() {
            diagnostics.confirmation_steps =
                diagnostics.confirmation_steps.saturating_add(outcome.steps);
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
    buckets
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_grammar::model::MorphemeId as Mid;

    fn sample_path(name: &str) -> Option<std::path::PathBuf> {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("../../../samples/data").join(name);
        path.exists().then_some(path)
    }

    fn load_indonesian() -> Option<Grammar> {
        let path = sample_path("indonesian-hc.xml")?;
        let xml = std::fs::read_to_string(&path).expect("read grammar");
        Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
    }

    /// A bare-root word confirms to a non-empty set of matches, all sharing the expected root entry.
    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
    fn confirm_bare_root_word_verifies() {
        let Some(g) = load_indonesian() else {
            eprintln!("skipping: indonesian-hc.xml not present on disk");
            return;
        };
        let morpher = Morpher::new(&g, usize::MAX);
        let owners = build_morpheme_owners(&g);
        // Finds "ajar"'s morpheme id from the grammar itself (entry25/entry26 homograph) rather than hard-coding one that might drift.
        let entry = g
            .entries
            .iter()
            .enumerate()
            .find(|(_, e)| {
                g.morphemes[e.morpheme.0 as usize].xml_key == "entry25"
                    || g.morphemes[e.morpheme.0 as usize].xml_key == "entry26"
            })
            .map(|(i, e)| (i, e.morpheme));
        let Some((_idx, morpheme)) = entry else {
            eprintln!("skipping: entry25/entry26 not found in indonesian-hc.xml");
            return;
        };
        let candidate = Candidate {
            morphemes: vec![morpheme],
            root_index: 0,
        };
        let matches = confirm_all(&g, &owners, &morpher, &candidate, "ajar");
        assert!(
            !matches.is_empty(),
            "\"ajar\" must confirm to at least one analysis"
        );
        for (wa, _, _) in &matches {
            assert_eq!(wa.root_morpheme_index, 0);
            assert_eq!(wa.morpheme_ids, vec![morpheme.0]);
        }
    }

    /// A candidate whose root position is out of range (or empty) must confirm to nothing, never panic.
    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
    fn confirm_rejects_out_of_range_root_index() {
        let Some(g) = load_indonesian() else {
            eprintln!("skipping: indonesian-hc.xml not present on disk");
            return;
        };
        let morpher = Morpher::new(&g, usize::MAX);
        let owners = build_morpheme_owners(&g);
        let bogus = Candidate {
            morphemes: vec![],
            root_index: 0,
        };
        assert!(confirm_all(&g, &owners, &morpher, &bogus, "ajar").is_empty());
    }

    /// A non-root morpheme id owned by neither a `LexEntry` nor an `MRule` must confirm to nothing.
    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
    fn confirm_rejects_unowned_non_root_morpheme() {
        let Some(g) = load_indonesian() else {
            eprintln!("skipping: indonesian-hc.xml not present on disk");
            return;
        };
        let morpher = Morpher::new(&g, usize::MAX);
        let owners = build_morpheme_owners(&g);
        let root = g.entries[0].morpheme;
        let candidate = Candidate {
            morphemes: vec![root, Mid(u32::MAX - 5)],
            root_index: 0,
        };
        assert!(confirm_all(&g, &owners, &morpher, &candidate, "ajar").is_empty());
    }
}
