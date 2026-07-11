//! `walk.rs` (F4/F7, HYBRID_FST_RUST_PLAN.md §8) — C#'s `FstTemplateAnalyzer` walker, both halves:
//! `AnalyzeShape`/`EpsilonClosure`/`BeamBudget`/`ToWordAnalyses` (F4, `FstTemplateAnalyzer.cs:
//! 441-729,552-637`) and the CHAIN walker `AnalyzeChain`/`ChainClosure`/`CascadeSymbol`, state-vector
//! `PConfig`s/`PConfigKey`, boundary insertion (F7, `:759-1197`).
//!
//! ## Two walkers, one trie, ONE mode selector — not unified into one code path
//! Per this milestone's brief: "read how C# structures the choice between them ... don't try to
//! unify them into one code path if C# doesn't." C# doesn't: `AnalyzeWord(string)` always calls the
//! bare `AnalyzeShape`; a caller wanting the chain calls `AnalyzeChain`/`AnalyzeComposed` directly —
//! there is no shared dispatch function, no flag threaded through one entry point, just two
//! independently-callable methods on the same `FstTemplateAnalyzer` instance (both read the same
//! `_start`/`_tokenOnEntry`/`Enter`/`Key`-equivalent trie plumbing, but their closure/frontier/
//! candidate-emission logic is entirely separate). This module mirrors that exactly: [`analyze_shape`]
//! (bare) and [`analyze_chain`] (chain) are two separate public functions over the same [`Trie`]; the
//! CALLER (a sibling proposer — `walk::analyze_word` for the bare FST, F7's `ChainPhonologyProposer`
//! for the chain) picks which one to invoke, exactly mirroring `LockstepPhonologyProposer`/
//! `ChainPhonologyProposer` each calling their own C# method (`AnalyzeComposed`/`AnalyzeChain`) rather
//! than a shared dispatcher.
//!
//! ## Chain walk overview (C# `AnalyzeChain`/`CascadeSymbol`/`ChainClosure`)
//! `chain: &[InversePhonology]` is stored in "reverse application order" (index 0 = inverse of the
//! LAST rule HC applied during synthesis, closest to the surface; the last index sits closest to the
//! lexicon trie). [`PConfig`] carries one rule-state PER chain level plus the lexicon [`Config`] plus
//! a global boundary-insertion counter. [`cascade_symbol`] feeds one symbol into `chain[rank]`: a
//! matching arc either stops (ε-output, epenthesis-inverse), cascades its emission into `rank+1`
//! (recursion), or — at the last rank — must unify a REAL (non-ε) trie arc, advancing the lexicon walk.
//! [`chain_closure`] floods: (a) trie ε-arcs (unchanged from the bare walk), (b) per-level structural
//! epsilons, (c) per-level ε-input restoration arcs (cascading down via [`cascade_symbol`]), (d) the
//! global "insert boundary" move (offering each `trie.boundary_alphabet` member at rank 0, bounded by
//! `max_boundary_insertions`). A boundary-conditioned trie arc is treated as a REAL symbol here
//! (contrast the bare walk's [`is_free_arc`], which crosses it for free) — only (d) can ever cross one,
//! matching `F1_QUIRK_AUDIT.md`'s framing of the boundary tape.
//!
//! ## Beam-debit points: the bare walk has ONE axis; the chain walk has BOTH (quirk 7, verified)
//! `F1_QUIRK_AUDIT.md` quirk 7 documents two debit axes in the C# source: (a) once per genuinely-new
//! post-dedup frontier admission, and (b) once per matching arc INSIDE `CascadeSymbol`, before
//! recursing to the next rank. Reading `AnalyzeShape`/`EpsilonClosure` directly (not assuming the
//! quirk's "shared by every walker in this file" framing extends identically to both halves):
//! **axis (b) does not exist on the bare path.** The bare walker has no recursive rank-by-rank
//! cascade at all — a matching arc moves in a single hop from one trie state directly to the next
//! (`AnalyzeShape`'s per-segment loop, `:507-524`; `EpsilonClosure`'s closure loop, `:667-685`), so
//! every admission passes through exactly one `HashSet.Add`-then-`TryDebit` pair. Axis (b) is a
//! property of `CascadeSymbol`'s recursive per-RANK fan-out over a CHAIN of inverse-phonology
//! transducers stacked in front of the trie — a mechanism the bare walker has no analog of, because
//! it walks the trie alone with no phonology-inverse layer underneath it. This module therefore
//! ports exactly the one debit point the bare source actually has; inventing a second one here would
//! be a departure from the C# source, not a missed quirk.
//!
//! ## Determinism (plan §4.2)
//! Candidate emission order matters (later milestones' composite ordering depends on each
//! proposer's own order being stable): [`analyze_shape`]'s output order is exactly the order its
//! final frontier (`current`) was populated in — itself deterministic because every closure/frontier
//! step walks a `Vec`-backed worklist (never a `HashMap`/`HashSet` iteration; the `HashSet`s here are
//! consulted only via `insert`/membership-check, never iterated, satisfying §4.2 the same way
//! `trie.rs`'s own memoization maps do). Results are never re-sorted by this module — sorting (where
//! required at all) is the golden-comparison boundary's job, not the walker's.

use std::rc::Rc;

use rustc_hash::FxHashSet as HashSet;

use hc_grammar::model::{Grammar, MorphemeId};
use hc_shape::{CdBits, NodeKind, Shape};

use crate::inverse::InversePhonology;
use crate::token::{self, MorphOp};
use crate::trie::{surface_table, ArcLabel, StateId, Trie};

/// C# `maxBeamWork`'s constructor default (`FstTemplateAnalyzer.cs:153,193`): a MEASURED
/// three-point calibration on the Sena guarded slice (feasibility report §8.1) — 10,000 clipped
/// 58 of 60 HEALTHY words; 10,000,000 covered 60/60 but allocated ~1.9 GB on the pathological tail
/// and crashed a test host; 1,000,000 covers 58/60 with 0 unsound, stopping exactly the 2
/// pathological-tail words the cap exists to stop. Per-grammar calibration is out of scope here
/// (complexity-cap plan, feasibility §9 item 4).
pub const DEFAULT_MAX_BEAM_WORK: i64 = 1_000_000;

/// C# `BeamBudget` (`FstTemplateAnalyzer.cs:597-628`): a per-word work-unit budget, debited once per
/// genuinely-new post-dedup frontier admission, LATCHING on exhaustion — once [`overflowed`] is
/// true, every subsequent [`try_debit`] call returns `false` immediately, which is what keeps a
/// worklist walk hang-proof regardless of how explosive the underlying graph is: at most
/// `max_beam_work` admissions ever happen, full stop.
///
/// [`overflowed`]: BeamBudget::overflowed
/// [`try_debit`]: BeamBudget::try_debit
pub struct BeamBudget {
    remaining: i64,
    overflowed: bool,
}

impl BeamBudget {
    pub fn new(max_beam_work: i64) -> Self {
        BeamBudget {
            remaining: max_beam_work,
            overflowed: false,
        }
    }

    /// True once [`try_debit`](Self::try_debit) has returned `false` once — the word this budget
    /// belongs to must be treated as unparsed.
    pub fn overflowed(&self) -> bool {
        self.overflowed
    }

    /// C# `TryDebit` (`:614-627`): debit one unit of work; returns `false` (latching
    /// [`overflowed`](Self::overflowed)) once the budget is exhausted.
    pub fn try_debit(&mut self) -> bool {
        if self.overflowed {
            return false;
        }
        if self.remaining <= 0 {
            self.overflowed = true;
            return false;
        }
        self.remaining -= 1;
        true
    }
}

/// One live walk configuration: the trie state plus the accumulated token history along this path
/// (C#'s `Config` struct, `:1858-1868`). `tokens` is `Rc<[u32]>` rather than C#'s bare `uint[]` so
/// [`enter`] can cheaply share an unchanged history across many configs exactly when C#'s `Enter`
/// does (no token on the entered state ⇒ the SAME array reference, no allocation — C#'s `Enter`
/// returns `new Config(state, tokens)` with the very same `tokens` reference in that branch), while
/// `Rc<[u32]>`'s `Hash`/`Eq` impls forward to the SLICE's value (not the pointer), so [`ConfigKey`]
/// still reproduces C#'s by-VALUE struct equality exactly. A representation change, not a behavior
/// change (nothing observable depends on whether/when an allocation happens).
#[derive(Clone)]
pub struct Config {
    pub state: StateId,
    pub tokens: Rc<[u32]>,
}

/// Dedup key for a [`Config`] (C#'s `ConfigKey`, `:1917-1939`): state id + token history, by value
/// (see [`Config`]'s doc for why `Rc<[u32]>` still gives value semantics here).
#[derive(Clone, PartialEq, Eq, Hash)]
struct ConfigKey {
    state: StateId,
    tokens: Rc<[u32]>,
}

fn key_of(c: &Config) -> ConfigKey {
    ConfigKey {
        state: c.state,
        tokens: Rc::clone(&c.tokens),
    }
}

/// C# `Enter` (`FstTemplateAnalyzer.cs:689-694`): move into `state`, appending its token (if any,
/// via [`Trie::token`]) to the path's history.
fn enter(trie: &Trie, state: StateId, tokens: &Rc<[u32]>) -> Config {
    match trie.token(state) {
        Some(t) => {
            let mut v: Vec<u32> = tokens.iter().copied().collect();
            v.push(t);
            Config {
                state,
                tokens: Rc::from(v.into_boxed_slice()),
            }
        }
        None => Config {
            state,
            tokens: Rc::clone(tokens),
        },
    }
}

/// True for an arc the BARE walker crosses for free (C#'s `EpsilonClosure` admission test,
/// `arc.Input.IsEpsilon || IsBoundaryArc(arc)`, `:670`): a true epsilon arc, OR a boundary-
/// conditioned arc (I4 "boundary tape" — a real surface word never contains a literal junction
/// marker, so the bare walk can never consume one via the per-segment matching loop below; treating
/// it as free here is what keeps every arc built from a boundary segment reachable). Contrast the
/// (out-of-scope, F7) chain walker, which treats a boundary arc as a real symbol only its own
/// "insert boundary" move may cross.
fn is_free_arc(label: &ArcLabel) -> bool {
    matches!(label, ArcLabel::Epsilon | ArcLabel::Boundary { .. })
}

/// C# `EpsilonClosure` (`FstTemplateAnalyzer.cs:648-687`): flood-fill every free arc (epsilon or
/// boundary) reachable from `configs`, deduped by [`ConfigKey`], debiting `budget` once per
/// genuinely-new admission — the bare walk's ONLY debit point (see this module's own doc on why
/// axis (b) does not apply here). Loop structure mirrors the C# `while (stack.Count > 0 &&
/// !budget.Overflowed)` check exactly (checked BEFORE popping, not after) so a config already on the
/// stack when the budget latches is simply never processed, matching the C# control flow bit-for-bit
/// rather than merely producing an equivalent final answer.
fn epsilon_closure(trie: &Trie, configs: Vec<Config>, budget: &mut BeamBudget) -> Vec<Config> {
    let mut result = Vec::new();
    let mut seen: HashSet<ConfigKey> = HashSet::default();
    let mut stack: Vec<Config> = Vec::new();
    for config in configs {
        if seen.insert(key_of(&config)) {
            result.push(config.clone());
            stack.push(config);
        }
    }
    while !stack.is_empty() && !budget.overflowed() {
        let config = stack.pop().expect("stack non-empty per loop guard");
        for arc in trie.arcs(config.state) {
            if is_free_arc(&arc.label) {
                let nc = enter(trie, arc.target, &config.tokens);
                if seen.insert(key_of(&nc)) {
                    // Frontier-axis debit (the bare walk's only one).
                    if !budget.try_debit() {
                        break;
                    }
                    result.push(nc.clone());
                    stack.push(nc);
                }
            }
        }
    }
    result
}

/// One candidate analysis surviving the walk (C#'s `WordAnalysis`, restricted to the fields the bare
/// walker ever populates — the bare proposer never sets a `Category`, matching `ToWordAnalyses`'s
/// own 2-arg-plus-null `WordAnalysis` construction, `:722`/`:727`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordAnalysis {
    pub morphemes: Vec<MorphemeId>,
    /// Index into `morphemes` of the head root, or `-1` if none (C#'s `RootMorphemeIndex`).
    pub root_index: i32,
}

/// C# `ToWordAnalyses` (`FstTemplateAnalyzer.cs:708-729`): decode one accepted token array into one
/// candidate PER compound-root choice — the trie doesn't statically know which root a compounding
/// rule treats as head (see `trie.rs`'s `build_compound_loop` doc), so every `Root`-tagged token
/// position yields its own candidate; F5's verify is what actually confirms headedness. 0 or 1 Root
/// tokens yields exactly one candidate (`root_index = -1` if none), in ascending token-position order
/// for the multi-root case (matching C#'s left-to-right `rootIndices` scan).
pub fn to_word_analyses(codec: &token::MorphTokenCodec, tokens: &[u32]) -> Vec<WordAnalysis> {
    let morphemes: Vec<MorphemeId> = tokens
        .iter()
        .map(|&t| codec.get_morpheme(token::get_morpheme_id(t)))
        .collect();
    let root_indices: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|&(_, &t)| token::get_op(t) == MorphOp::Root)
        .map(|(i, _)| i)
        .collect();
    if root_indices.len() <= 1 {
        let root_index = root_indices.first().map(|&i| i as i32).unwrap_or(-1);
        return vec![WordAnalysis { morphemes, root_index }];
    }
    root_indices
        .into_iter()
        .map(|i| WordAnalysis {
            morphemes: morphemes.clone(),
            root_index: i as i32,
        })
        .collect()
}

/// Outcome of one bare walk (C#'s `AnalyzeShape` return value, plus the overflow signal C# exposes
/// via the shared instance-level `BeamOverflowCount`/`RecordOverflow` — surfaced here per call
/// instead, since `analyze_shape` is a stateless function rather than a method on a long-lived
/// shared analyzer object; a caller wanting an aggregate count across many words accumulates
/// `overflowed` itself, e.g. for a future `fst-stats`-style diagnostic, F8's job).
pub struct WalkOutcome {
    pub analyses: Vec<WordAnalysis>,
    pub overflowed: bool,
}

/// One already-segmented input position: its char-def identity plus phonological feature lanes
/// (C#'s full per-node `FeatureStruct`, split the same way [`crate::trie::ArcLabel`] splits an
/// arc's condition — see that type's doc for why `char_def` must travel alongside `lanes`).
#[derive(Clone, Debug)]
pub struct InputSegment {
    pub char_def: u32,
    pub lanes: Vec<u64>,
}

/// C# `Arc.Input.FeatureStruct.IsUnifiable(segment)` for THIS trie's arcs — mirrors
/// `hc_parse::root_trie`'s `edge_matches` predicate exactly (see [`crate::trie::ArcLabel`]'s doc):
/// `char_def` equality, OR closure membership when the table has one, AND `flat_unifiable` on the
/// lanes. `closure` is `None` for a zero-phon-feature table (Sena), reducing this to plain
/// `char_def` identity.
fn arc_matches_segment(arc_char_def: u32, arc_lanes: &[u64], seg: &InputSegment, closure: Option<&[CdBits]>) -> bool {
    let cd_ok = arc_char_def == seg.char_def
        || closure.is_some_and(|c| {
            (arc_char_def as usize) < c.len() && c[arc_char_def as usize].contains(seg.char_def)
        });
    cd_ok && hc_featstruct::flat_unifiable(arc_lanes, &seg.lanes)
}

/// C# `AnalyzeShape` (`FstTemplateAnalyzer.cs:470-550`): walk `trie` over `segments` (already-
/// extracted per-segment char-def + feature lanes — see [`word_segments`] for how a surface word's
/// own segments are produced), returning every accepted, deduped-by-RAW-TOKEN-ARRAY candidate (C#'s
/// `emitted.Add(new TokenArrayKey(config.Tokens))`, `:544`) decoded via [`to_word_analyses`], in the
/// SAME order the walk's own final frontier produced them. `closure` is the table's
/// `unifiable_cds` closure (`None` for a zero-phon-feature grammar) — see [`arc_matches_segment`].
pub fn analyze_shape(
    trie: &Trie,
    segments: &[InputSegment],
    closure: Option<&[CdBits]>,
    max_beam_work: i64,
) -> WalkOutcome {
    let mut budget = BeamBudget::new(max_beam_work);
    let empty_tokens: Rc<[u32]> = Rc::from(Vec::<u32>::new().into_boxed_slice());
    let start = enter(trie, trie.start(), &empty_tokens);
    let mut current = epsilon_closure(trie, vec![start], &mut budget);

    for seg in segments {
        if budget.overflowed() {
            break;
        }
        let mut next: Vec<Config> = Vec::new();
        let mut seen: HashSet<ConfigKey> = HashSet::default();
        // C# checks `if (budget.Overflowed) break;` at the TOP of the per-config loop (`:501-506`)
        // and, separately, breaks only the INNER per-arc loop on a failed debit (`:516-519`) — the
        // outer loop's very next iteration then sees `Overflowed == true` (it latches) and breaks
        // there. Mirrored literally rather than collapsed to a single flag check, so a partial scan
        // of one config's own arcs (some admitted before the debit that overflows) matches exactly.
        for config in &current {
            if budget.overflowed() {
                break;
            }
            for arc in trie.arcs(config.state) {
                if let ArcLabel::Segment { lanes, char_def, .. } = &arc.label {
                    if arc_matches_segment(*char_def, lanes, seg, closure) {
                        let nc = enter(trie, arc.target, &config.tokens);
                        if seen.insert(key_of(&nc)) {
                            if !budget.try_debit() {
                                break;
                            }
                            next.push(nc);
                        }
                    }
                }
            }
        }
        current = epsilon_closure(trie, next, &mut budget);
        if current.is_empty() {
            break;
        }
    }

    if budget.overflowed() {
        // C# `RecordOverflow`: never throw, never hang — this word falls to unparsed and is
        // COUNTED, not silently dropped. The counting itself is the caller's job here (see
        // `WalkOutcome`'s doc).
        return WalkOutcome {
            analyses: Vec::new(),
            overflowed: true,
        };
    }

    let mut results = Vec::new();
    let mut emitted: HashSet<Rc<[u32]>> = HashSet::default();
    for config in &current {
        if trie.is_accepting(config.state) && emitted.insert(Rc::clone(&config.tokens)) {
            results.extend(to_word_analyses(trie.codec(), &config.tokens));
        }
    }
    WalkOutcome {
        analyses: results,
        overflowed: false,
    }
}

/// Extract the Segment-kind (never Boundary) per-segment [`InputSegment`]s (char-def + feature
/// lanes) of `shape` against `g`'s surface-stratum table (C#'s `AnalyzeShape`'s own segment
/// extraction, `:472-480`, filtered by `_filter = ann => ann.Type() == HCFeatureSystem.Segment` — a
/// real surface word never contains a literal boundary node, so this is never observably different
/// from also including boundaries, but mirroring the exact filter keeps this function honest
/// against the C# source rather than assuming the difference is moot).
pub fn word_segments(g: &Grammar, shape: &Shape) -> Vec<InputSegment> {
    let (table, w) = surface_table(g);
    shape
        .interior()
        .filter(|(_, kind, _cd, _flags)| *kind == NodeKind::Segment)
        .map(|(_, _kind, cd, _flags)| InputSegment {
            char_def: cd,
            lanes: crate::trie::lanes_for(table, hc_grammar::chardef::CharDefId(cd), w),
        })
        .collect()
}

/// C# `AnalyzeWord` (`FstTemplateAnalyzer.cs:441-454`): segment `word` against `g`'s surface table
/// and walk it. If `word` contains a phoneme outside the table, C#'s OWN `AnalyzeWord` catches
/// `InvalidShapeException` internally and returns an EMPTY sequence (`:448-452`) — it does NOT
/// propagate the exception to its caller. PARITY (found empirically building this milestone's
/// candidate-parity gate): this means the Indonesian corpus word `write-CONTpijit` (index 118,
/// containing characters — uppercase `C`/`O`/`N`/`T` — outside the grammar's char-def table)
/// produces ZERO candidate lines and, critically, NOT a `SKIPPED` line in `fst-candidates --bare`'s
/// output — `FstCandidatesCommand`'s own `catch (InvalidShapeException)` around the whole proposer
/// loop is dead code for the `--bare` proposer specifically, since the exception never reaches it.
/// Mirrored here by folding a segmentation failure into an ordinary empty, non-overflowed
/// [`WalkOutcome`] rather than a distinguishable `None`/error variant.
pub fn analyze_word(g: &Grammar, trie: &Trie, word: &str, max_beam_work: i64) -> WalkOutcome {
    let (table, _w) = surface_table(g);
    let Ok(shape) = hc_grammar::segment::segment(table, word) else {
        return WalkOutcome {
            analyses: Vec::new(),
            overflowed: false,
        };
    };
    let segments = word_segments(g, &shape);
    analyze_shape(trie, &segments, table.unif_closure_rows(), max_beam_work)
}

// =================================================================================================
// The CHAIN walker (F7): `AnalyzeChain`/`ChainClosure`/`CascadeSymbol`.
// =================================================================================================

/// C# `AnalyzeChain`'s default (`FstTemplateAnalyzer.cs:816`): generous default, real grammars need
/// at most one inserted boundary per morpheme junction the word actually has; only a pathological
/// grammar/word combination hits this, converting a would-be hang into "unparsed".
pub const DEFAULT_MAX_BOUNDARY_INSERTIONS: i32 = 8;

/// C# `PConfig` (`FstTemplateAnalyzer.cs:731-756`): one state PER chain level, plus the lexicon
/// [`Config`], plus how many global "insert boundary" ε-moves this config has already taken.
#[derive(Clone)]
pub struct PConfig {
    pub rule_states: Vec<u32>,
    pub lex: Config,
    pub insertions_used: i32,
}

/// Dedup key for a [`PConfig`] (C# `PConfigKey`): the chain's state vector + the lexicon state id +
/// lexicon token history + insertions-used, by value.
#[derive(Clone, PartialEq, Eq, Hash)]
struct PConfigKey {
    rule_states: Vec<u32>,
    lex_state: StateId,
    lex_tokens: Rc<[u32]>,
    insertions_used: i32,
}

fn pkey(pc: &PConfig) -> PConfigKey {
    PConfigKey {
        rule_states: pc.rule_states.clone(),
        lex_state: pc.lex.state,
        lex_tokens: Rc::clone(&pc.lex.tokens),
        insertions_used: pc.insertions_used,
    }
}

fn all_accepting(chain: &[InversePhonology], rule_states: &[u32]) -> bool {
    chain.iter().zip(rule_states).all(|(pinv, &s)| pinv.is_accepting(s))
}

/// A trie arc's own (lanes, target) if it is NOT a pure epsilon arc (C#'s `!larc.Input.IsEpsilon`
/// guard at both the `CascadeSymbol` bottom join and `ChainClosure`'s ε-input-restoration join) --
/// covers BOTH `ArcLabel::Segment` and `ArcLabel::Boundary` (the chain's lexicon join has no notion
/// of "this rank's emission must be a literal surface segment vs. a boundary"; that discrimination
/// is carried entirely by the lane rows themselves via the synthetic Type feature -- see `inverse.rs`'s
/// module doc).
fn trie_arc_lanes(label: &ArcLabel) -> Option<&[u64]> {
    match label {
        ArcLabel::Epsilon => None,
        ArcLabel::Segment { lanes, .. } | ArcLabel::Boundary { lanes, .. } => Some(lanes),
    }
}

/// C# `CascadeSymbol` (`FstTemplateAnalyzer.cs:948-1009`): feed `symbol` (lanes only -- see
/// `inverse.rs`'s PARITY note) into `chain[rank]` at `rule_states[rank]`'s current state. Eagerly
/// collects what C#'s `IEnumerable<PConfig>` `yield return`s -- a failed [`BeamBudget::try_debit`]
/// stops the loop immediately (C#'s `yield break`), discarding no already-collected config, exactly
/// matching the generator's "stop enumerating right here" semantics.
#[allow(clippy::too_many_arguments)]
fn cascade_symbol(
    trie: &Trie,
    chain: &[InversePhonology],
    rule_states: &[u32],
    rank: usize,
    symbol: &[u64],
    lex: &Config,
    budget: &mut BeamBudget,
    insertions_used: i32,
) -> Vec<PConfig> {
    let mut out = Vec::new();
    for arc in chain[rank].arcs_from(rule_states[rank]) {
        if arc.is_epsilon_input() || !arc.surface_unifiable(symbol) {
            continue; // ε-input arcs are taken in the closure; this arc must consume a real symbol
        }
        // I6 enumeration-axis debit: BEFORE cloning the state vector / recursing / scanning the lexicon.
        if !budget.try_debit() {
            return out;
        }
        let mut new_states = rule_states.to_vec();
        new_states[rank] = arc.target;
        if arc.is_epsilon_output() {
            // I0/I3 plumbing: consumes the incoming symbol but emits nothing further down the
            // chain -- the cascade for THIS symbol stops here; the lexicon does not advance.
            out.push(PConfig { rule_states: new_states, lex: lex.clone(), insertions_used });
            continue;
        }
        let underlying = arc.underlying.as_ref().expect("non-epsilon-output arc has Some(underlying)");
        if rank == chain.len() - 1 {
            // Bottom of the chain: the emission must unify a (non-ε) lexicon arc.
            for larc in trie.arcs(lex.state) {
                if let Some(lanes) = trie_arc_lanes(&larc.label) {
                    if hc_featstruct::flat_unifiable(lanes, underlying) {
                        out.push(PConfig {
                            rule_states: new_states.clone(),
                            lex: enter(trie, larc.target, &lex.tokens),
                            insertions_used,
                        });
                    }
                }
            }
        } else {
            out.extend(cascade_symbol(trie, chain, &new_states, rank + 1, underlying, lex, budget, insertions_used));
        }
    }
    out
}

/// C# `ChainClosure` (`FstTemplateAnalyzer.cs:1036-1194`): closure over (a) lexicon ε-arcs (unchanged
/// from the bare walk), (b) per-level structural-epsilon arcs, (c) per-level ε-input restoration arcs
/// (cascading down via [`cascade_symbol`] before they can unify a lexicon arc), (d) the global "insert
/// boundary" move, bounded by `max_boundary_insertions` via [`PConfig::insertions_used`] (part of
/// [`PConfigKey`], so the dedup set is finite regardless of chain/trie topology).
fn chain_closure(
    trie: &Trie,
    chain: &[InversePhonology],
    configs: Vec<PConfig>,
    max_boundary_insertions: i32,
    budget: &mut BeamBudget,
) -> Vec<PConfig> {
    let mut result = Vec::new();
    let mut seen: HashSet<PConfigKey> = HashSet::default();
    let mut stack: Vec<PConfig> = Vec::new();
    for pc in configs {
        if seen.insert(pkey(&pc)) {
            result.push(pc.clone());
            stack.push(pc);
        }
    }

    while !stack.is_empty() && !budget.overflowed() {
        let pc = stack.pop().expect("stack non-empty per loop guard");

        // (a) lexicon ε-arcs: the morphotactic network's slot-entry/skip transitions.
        for larc in trie.arcs(pc.lex.state) {
            if matches!(larc.label, ArcLabel::Epsilon) {
                let nc =
                    PConfig { rule_states: pc.rule_states.clone(), lex: enter(trie, larc.target, &pc.lex.tokens), insertions_used: pc.insertions_used };
                if seen.insert(pkey(&nc)) {
                    if !budget.try_debit() {
                        break;
                    }
                    result.push(nc.clone());
                    stack.push(nc);
                }
            }
        }

        // (b) + (c) per-level epsilon arcs.
        for rank in 0..chain.len() {
            if budget.overflowed() {
                break;
            }
            for arc in chain[rank].arcs_from(pc.rule_states[rank]) {
                if !arc.is_epsilon_input() {
                    continue; // must not consume a real surface symbol to be taken in closure
                }
                // I6 enumeration-axis debit: mirrors `cascade_symbol`'s per-matching-arc debit.
                if !budget.try_debit() {
                    break;
                }
                let mut new_states = pc.rule_states.clone();
                new_states[rank] = arc.target;
                if arc.is_epsilon_output() {
                    // (b) structural epsilon: pure state move at this rank only.
                    let nc = PConfig { rule_states: new_states, lex: pc.lex.clone(), insertions_used: pc.insertions_used };
                    if seen.insert(pkey(&nc)) {
                        if !budget.try_debit() {
                            break;
                        }
                        result.push(nc.clone());
                        stack.push(nc);
                    }
                    continue;
                }
                // (c) real ε-input restoration: its emission must cascade down through rank+1..end
                // (or, if this is already the last rank, unify a lexicon arc directly).
                let underlying = arc.underlying.clone().expect("ε-input, non-ε-output arc has Some(underlying)");
                if rank == chain.len() - 1 {
                    for larc in trie.arcs(pc.lex.state) {
                        if let Some(lanes) = trie_arc_lanes(&larc.label) {
                            if hc_featstruct::flat_unifiable(lanes, &underlying) {
                                let nc = PConfig {
                                    rule_states: new_states.clone(),
                                    lex: enter(trie, larc.target, &pc.lex.tokens),
                                    insertions_used: pc.insertions_used,
                                };
                                if seen.insert(pkey(&nc)) {
                                    if !budget.try_debit() {
                                        break;
                                    }
                                    result.push(nc.clone());
                                    stack.push(nc);
                                }
                            }
                        }
                    }
                } else {
                    for nc in cascade_symbol(trie, chain, &new_states, rank + 1, &underlying, &pc.lex, budget, pc.insertions_used) {
                        if seen.insert(pkey(&nc)) {
                            if !budget.try_debit() {
                                break;
                            }
                            result.push(nc.clone());
                            stack.push(nc);
                        }
                    }
                }
            }
        }

        // (d) I4: global "insert boundary" move -- offered while under the per-word insertion
        // budget; the shared I6 work budget is also checked between boundary symbols.
        if !budget.overflowed() && pc.insertions_used < max_boundary_insertions {
            for boundary_lanes in &trie.boundary_alphabet {
                if budget.overflowed() {
                    break;
                }
                for nc0 in cascade_symbol(trie, chain, &pc.rule_states, 0, boundary_lanes, &pc.lex, budget, 0) {
                    let nc = PConfig { rule_states: nc0.rule_states, lex: nc0.lex, insertions_used: pc.insertions_used + 1 };
                    if seen.insert(pkey(&nc)) {
                        if !budget.try_debit() {
                            break;
                        }
                        result.push(nc.clone());
                        stack.push(nc);
                    }
                }
            }
        }
    }
    result
}

/// C# `AnalyzeChain` (`FstTemplateAnalyzer.cs:812-915`): the general chain walker over
/// already-extracted `segments` -- the chain analog of [`analyze_shape`], factored out the same way
/// so hand-built-trie unit tests can exercise the walk without a real [`Grammar`]/char-def table
/// (mirroring [`analyze_shape`]/[`analyze_word`]'s own split). `chain` is in "reverse application
/// order" (see this module's doc).
pub fn analyze_chain_segments(
    trie: &Trie,
    chain: &[InversePhonology],
    segments: &[InputSegment],
    max_beam_work: i64,
    max_boundary_insertions: i32,
) -> WalkOutcome {
    let start_states: Vec<u32> = chain.iter().map(|p| p.start_state).collect();
    let mut budget = BeamBudget::new(max_beam_work);
    let empty_tokens: Rc<[u32]> = Rc::from(Vec::<u32>::new().into_boxed_slice());
    let start_lex = enter(trie, trie.start(), &empty_tokens);
    let mut current = chain_closure(
        trie,
        chain,
        vec![PConfig { rule_states: start_states, lex: start_lex, insertions_used: 0 }],
        max_boundary_insertions,
        &mut budget,
    );

    for seg in segments {
        if budget.overflowed() {
            break;
        }
        let mut next: Vec<PConfig> = Vec::new();
        let mut seen: HashSet<PConfigKey> = HashSet::default();
        for pc in &current {
            if budget.overflowed() {
                break;
            }
            for nc in cascade_symbol(trie, chain, &pc.rule_states, 0, &seg.lanes, &pc.lex, &mut budget, pc.insertions_used) {
                if seen.insert(pkey(&nc)) {
                    if !budget.try_debit() {
                        break;
                    }
                    next.push(nc);
                }
            }
        }
        current = chain_closure(trie, chain, next, max_boundary_insertions, &mut budget);
        if current.is_empty() {
            break;
        }
    }

    if budget.overflowed() {
        return WalkOutcome { analyses: Vec::new(), overflowed: true };
    }

    let mut results = Vec::new();
    let mut emitted: HashSet<Rc<[u32]>> = HashSet::default();
    for pc in &current {
        if all_accepting(chain, &pc.rule_states) && trie.is_accepting(pc.lex.state) && emitted.insert(Rc::clone(&pc.lex.tokens)) {
            results.extend(to_word_analyses(trie.codec(), &pc.lex.tokens));
        }
    }
    WalkOutcome { analyses: results, overflowed: false }
}

/// C# `AnalyzeChain(string, ...)`: segment `word` against `g`'s surface table then walk it (thin
/// wrapper over [`analyze_chain_segments`], mirroring [`analyze_word`]'s own wrapper over
/// [`analyze_shape`]). Same empty-on-segmentation-failure behavior as [`analyze_word`] (PARITY: C#'s
/// `AnalyzeChain` catches `InvalidShapeException` internally too, `:819-827`).
pub fn analyze_chain(
    g: &Grammar,
    trie: &Trie,
    chain: &[InversePhonology],
    word: &str,
    max_beam_work: i64,
    max_boundary_insertions: i32,
) -> WalkOutcome {
    let (table, _w) = surface_table(g);
    let Ok(shape) = hc_grammar::segment::segment(table, word) else {
        return WalkOutcome { analyses: Vec::new(), overflowed: false };
    };
    let segments = word_segments(g, &shape);
    analyze_chain_segments(trie, chain, &segments, max_beam_work, max_boundary_insertions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trie::{ArcData, StateData};

    /// TEMP DEBUG (investigating Sena full-corpus overflow): print the INITIAL epsilon-closure
    /// size (before any word segment is consumed -- word-INDEPENDENT) for a real grammar's trie,
    /// to see whether the closure itself, not the per-word walk, is what exhausts the budget.
    #[test]
    #[ignore = "debug probe, not a gate"]
    fn debug_initial_closure_size() {
        for name in ["indonesian-hc.xml", "sena-hc.xml"] {
            let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let path = manifest_dir.join("../../../samples/data").join(name);
            if !path.exists() {
                eprintln!("skip {name}: not present");
                continue;
            }
            let xml = std::fs::read_to_string(&path).expect("read grammar");
            let g = hc_grammar::load(&xml).unwrap_or_else(|e| panic!("load {name}: {e}"));
            let morpher = hc_parse::Morpher::new(&g, usize::MAX);
            let surface = crate::surface::SurfacePhonology::new(&g);
            let t0 = std::time::Instant::now();
            let trie = Trie::build(&g, &surface, &morpher, 1_000_000, 2, true);
            eprintln!("{name}: trie built in {:?}, {} states", t0.elapsed(), trie.state_count());

            let mut budget = BeamBudget::new(DEFAULT_MAX_BEAM_WORK);
            let empty_tokens: Rc<[u32]> = Rc::from(Vec::<u32>::new().into_boxed_slice());
            let start = enter(&trie, trie.start(), &empty_tokens);
            let t1 = std::time::Instant::now();
            let current = epsilon_closure(&trie, vec![start], &mut budget);
            eprintln!(
                "{name}: initial closure = {} configs in {:?}, overflowed={}",
                current.len(),
                t1.elapsed(),
                budget.overflowed()
            );
        }
    }

    /// A single-state trie accepting the empty word — the minimal well-formed fixture for exercising
    /// [`analyze_shape`]/[`epsilon_closure`] without a real grammar.
    fn empty_word_trie() -> Trie {
        let mut states = vec![StateData::default()];
        states[0].accepting = true;
        Trie::from_states(states, 0, token::MorphTokenCodec::new())
    }

    #[test]
    fn empty_word_accepts_with_no_candidates_when_no_token() {
        let trie = empty_word_trie();
        let out = analyze_shape(&trie, &[], None, DEFAULT_MAX_BEAM_WORK);
        assert!(!out.overflowed);
        // Accepting with an empty token array decodes to exactly one (empty-morpheme) analysis.
        assert_eq!(out.analyses.len(), 1);
        assert_eq!(out.analyses[0].root_index, -1);
        assert!(out.analyses[0].morphemes.is_empty());
    }

    /// Build a "comb" trie with `depth` layers of `branch` parallel TOKEN-BEARING epsilon arcs each
    /// (every arc's target is a fresh state carrying a distinct token, converging back into one
    /// shared per-layer entry state) — `branch^depth` DISTINCT token histories reach the final
    /// state via a SMALL graph (`depth*branch + depth` states), exactly the shape C#'s
    /// `BeamCapTests.BuildExplosiveChain` engineers for the chain walker (many candidate paths
    /// enumerated before any dedup collapses them), ported to the bare walker's own axis: here the
    /// explosion is in `EpsilonClosure`'s frontier (many DISTINCT `(state, tokens)` keys converging
    /// on the SAME final state), not in a per-rank recursive cascade (the bare walk has none — see
    /// this module's own doc). No input segment is needed: the whole explosion happens during the
    /// walk's initial closure, before any segment is consumed.
    fn build_comb_trie(depth: usize, branch: usize) -> Trie {
        let mut states = vec![StateData::default()]; // state 0: overall entry
        let mut codec = token::MorphTokenCodec::new();
        let mut layer_entry: StateId = 0;
        for layer in 0..depth {
            let mut branch_states = Vec::with_capacity(branch);
            for b in 0..branch {
                // Register a genuine morpheme index via the codec (not a raw literal) so a
                // completed walk's `to_word_analyses` can decode this token without panicking --
                // `MorphTokenCodec::get_morpheme` indexes into ITS OWN first-seen table, which must
                // actually contain every morpheme a token here can reference.
                let morpheme = MorphemeId((layer * branch + b) as u32);
                let idx = codec.get_or_add_index(morpheme);
                let s = StateData {
                    token: Some(token::encode(MorphOp::Prefix, idx)),
                    ..StateData::default()
                };
                states.push(s);
                branch_states.push((states.len() - 1) as StateId);
            }
            let next_entry = if layer + 1 == depth {
                let s = StateData {
                    accepting: true,
                    ..StateData::default()
                };
                states.push(s);
                (states.len() - 1) as StateId
            } else {
                states.push(StateData::default());
                (states.len() - 1) as StateId
            };
            for &bs in &branch_states {
                states[layer_entry as usize].arcs.push(ArcData {
                    label: ArcLabel::Epsilon,
                    target: bs,
                });
                states[bs as usize].arcs.push(ArcData {
                    label: ArcLabel::Epsilon,
                    target: next_entry,
                });
            }
            layer_entry = next_entry;
        }
        Trie::from_states(states, 0, codec)
    }

    /// Port of C# `BeamCapTests.BeamCap_PathologicalChain_FallsToUnparsed_Bounded_...`'s pathological
    /// half, adapted to the BARE walker (the C# original engineers the explosion in
    /// `CascadeSymbol`'s chain-only recursive cascade, out of scope for F4 — see this module's own
    /// doc): `branch^depth` candidate paths must overflow a small budget, fall to unparsed (never a
    /// partial/wrong guess), and do so FAST (bounded by the budget, not by how explosive the graph
    /// is) — the same "hang-proof by construction" property, exercised on the axis the bare walker
    /// actually has.
    #[test]
    fn beam_cap_pathological_comb_falls_to_unparsed_bounded() {
        let trie = build_comb_trie(12, 8); // 8^12 ~= 6.9e10 distinct token histories
        let start = std::time::Instant::now();
        let out = analyze_shape(&trie, &[], None, 10_000);
        let elapsed = start.elapsed();

        assert!(out.overflowed, "an explosive comb must overflow a small budget");
        assert!(
            out.analyses.is_empty(),
            "an overflowed word must fall to unparsed, never a partial guess"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "a pathological comb must never hang the walk (took {elapsed:?})"
        );
    }

    /// Port of C# `BeamCapTests.BeamCap_CtorKnob_LowersOverflowThreshold`: the SAME healthy walk
    /// must complete under the generous default and overflow a deliberately tiny custom budget.
    #[test]
    fn beam_cap_ctor_knob_lowers_overflow_threshold() {
        // A modest, non-explosive comb (3^4 = 81 distinct histories) -- healthy under the default,
        // but easily overflowed by a tiny custom budget.
        let trie = build_comb_trie(4, 3);

        let default_out = analyze_shape(&trie, &[], None, DEFAULT_MAX_BEAM_WORK);
        assert!(
            !default_out.overflowed,
            "the generous default must not be reached by a small healthy comb walk"
        );

        let tiny_out = analyze_shape(&trie, &[], None, 5);
        assert!(
            tiny_out.overflowed,
            "the SAME walk must overflow a deliberately tiny custom budget"
        );
        assert!(tiny_out.analyses.is_empty());
    }

    /// The beam cap must never clip a healthy walk: a normal (non-explosive) comb, immediately
    /// after a fresh `BeamBudget` per call (the design's own "no mutable state crosses calls"
    /// guarantee -- trivially true here since `analyze_shape` is a stateless function, C#'s
    /// equivalent guarantee for a SHARED, long-lived analyzer instance), still succeeds.
    #[test]
    fn healthy_walk_after_overflow_elsewhere_is_unaffected() {
        let explosive = build_comb_trie(12, 8);
        let overflowed = analyze_shape(&explosive, &[], None, 10_000);
        assert!(overflowed.overflowed);

        let healthy = build_comb_trie(2, 2);
        let ok = analyze_shape(&healthy, &[], None, DEFAULT_MAX_BEAM_WORK);
        assert!(!ok.overflowed, "a fresh call's budget must be unaffected by a prior overflow");
    }

    // =============================================================================================
    // Chain walker tests (F7).
    // =============================================================================================

    /// A 3-state trie: state0 --Segment(B)--> state1 --Segment(A)--> state2 (accepting, token=Root).
    fn deletion_trie() -> Trie {
        const A: [u64; 1] = [0b01];
        const B: [u64; 1] = [0b10];
        let mut codec = token::MorphTokenCodec::new();
        let idx = codec.get_or_add_index(MorphemeId(0));
        let mut states = vec![StateData::default(), StateData::default(), StateData {
            accepting: true,
            token: Some(token::encode(MorphOp::Root, idx)),
            ..StateData::default()
        }];
        states[0].arcs.push(ArcData {
            label: ArcLabel::Segment { lanes: B.to_vec(), reprs: vec!["b".into()], char_def: 0 },
            target: 1,
        });
        states[1].arcs.push(ArcData {
            label: ArcLabel::Segment { lanes: A.to_vec(), reprs: vec!["a".into()], char_def: 1 },
            target: 2,
        });
        Trie::from_states(states, 0, codec)
    }

    /// The core new capability the chain brings over the bare walk: a deletion the surface never
    /// shows, recovered ONLY because the lexicon has a matching arc for the restored underlying
    /// segment at the exact position it's needed (the LEVER_2 spike shape,
    /// `LeverTwoSpikeTests`/`ChainDeletionEpenthesisTests`'s C# analogue, minimized). The bare walk
    /// on word "A" (one real segment) finds NOTHING at all -- state0 has no arc for A, only B. A
    /// length-1 chain whose `InversePhonology` restores B (ε-input) and passes A through as
    /// identity, both self-looping at its one accepting state, finds exactly the one candidate the
    /// lexicon actually licenses.
    #[test]
    fn chain_recovers_a_deletion_the_bare_walk_cannot_see() {
        const A: [u64; 1] = [0b01];
        const B: [u64; 1] = [0b10];
        let trie = deletion_trie();

        let seg_a = InputSegment { char_def: 1, lanes: A.to_vec() };
        let bare = analyze_shape(&trie, std::slice::from_ref(&seg_a), None, DEFAULT_MAX_BEAM_WORK);
        assert!(bare.analyses.is_empty(), "bare walk cannot see the deleted B at all");

        let mut pinv = InversePhonology::new();
        pinv.start_state = 0;
        pinv.set_accepting(0);
        pinv.add_arc(0, Some(A.to_vec()), Some(A.to_vec()), 0); // identity: A passes through
        pinv.add_arc(0, None, Some(B.to_vec()), 0); // ε-input restoration: B was deleted here
        let chain = vec![pinv];

        let chained = analyze_chain_segments(&trie, &chain, &[seg_a], DEFAULT_MAX_BEAM_WORK, DEFAULT_MAX_BOUNDARY_INSERTIONS);
        assert!(!chained.overflowed);
        assert_eq!(chained.analyses.len(), 1, "the chain recovers exactly the one lexicon-licensed candidate");
        assert_eq!(chained.analyses[0].morphemes, vec![MorphemeId(0)]);
        assert_eq!(chained.analyses[0].root_index, 0);
    }

    /// Without the restoration arc, an otherwise-identical chain must NOT manufacture the deletion
    /// out of nothing -- a length-1 chain that is pure identity behaves exactly like the bare walk
    /// (finds nothing), confirming the previous test's positive result comes from the restoration
    /// arc specifically, not from some other difference between the two walkers.
    #[test]
    fn identity_only_chain_matches_bare_walk_on_the_same_deletion_trie() {
        const A: [u64; 1] = [0b01];
        let trie = deletion_trie();
        let seg_a = InputSegment { char_def: 1, lanes: A.to_vec() };

        let mut pinv = InversePhonology::new();
        pinv.start_state = 0;
        pinv.set_accepting(0);
        pinv.add_arc(0, Some(A.to_vec()), Some(A.to_vec()), 0);
        let chain = vec![pinv];

        let chained =
            analyze_chain_segments(&trie, &chain, std::slice::from_ref(&seg_a), DEFAULT_MAX_BEAM_WORK, DEFAULT_MAX_BOUNDARY_INSERTIONS);
        let bare = analyze_shape(&trie, &[seg_a], None, DEFAULT_MAX_BEAM_WORK);
        assert_eq!(chained.analyses, bare.analyses);
        assert!(chained.analyses.is_empty());
    }

    /// Pathological chain: an unconditioned restoration self-loop offered at every closure step
    /// must not hang or explode -- the shared I6 beam budget stops it (never a throw, never a
    /// silent wrong answer), exercising the chain's OWN enumeration-axis debit inside
    /// `chain_closure`'s ε-input-restoration branch (quirk 7's two-axis accounting, chain-specific).
    #[test]
    fn chain_beam_budget_stops_an_unconditioned_restoration_loop() {
        const A: [u64; 1] = [0b01];
        const B: [u64; 1] = [0b10];
        // A trie with a genuine ε-cycle at state0 (B self-loop as an EPSILON arc) so the
        // restoration's lexicon join can keep re-admitting the same lexicon state over and over
        // via distinct rule-state histories is not even needed -- the chain's OWN closure
        // recursion (cascade_symbol's rank fan-out) is what must be capped; a single-level chain
        // with the restoration targeting a FRESH state each admission is unnecessary to engineer
        // since PConfigKey dedup already collapses same-state repeats. Instead: a small comb-style
        // trie already proven explosive for the bare walker's frontier, walked through a
        // permissive identity chain, confirms the chain's budget is independently enforced.
        let trie = deletion_trie();
        let seg_a = InputSegment { char_def: 1, lanes: A.to_vec() };
        let seg_b = InputSegment { char_def: 0, lanes: B.to_vec() };

        let mut pinv = InversePhonology::new();
        pinv.start_state = 0;
        pinv.set_accepting(0);
        pinv.add_arc(0, Some(A.to_vec()), Some(A.to_vec()), 0);
        pinv.add_arc(0, Some(B.to_vec()), Some(B.to_vec()), 0);
        pinv.add_arc(0, None, Some(B.to_vec()), 0);
        let chain = vec![pinv];

        let start = std::time::Instant::now();
        let out = analyze_chain_segments(&trie, &chain, &[seg_b, seg_a], 5, DEFAULT_MAX_BOUNDARY_INSERTIONS);
        let elapsed = start.elapsed();
        assert!(out.overflowed, "a 5-unit budget must overflow well before finishing");
        assert!(out.analyses.is_empty());
        assert!(elapsed < std::time::Duration::from_secs(5), "must never hang (took {elapsed:?})");
    }
}
