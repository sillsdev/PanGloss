//! The in-flight parse state for the morphological-rule engine.
//!
//! This is the `Word` the affix-process and compounding primitives (un)apply rules to. It is
//! deliberately **owned** (no arena lifetime); only its rule trail is shared between clones.
//!
//! `syn_fs`/`real_fs` are owned `FeatureStruct`s, not `FsId`s: rule application mints new
//! feature-structure values (`unify` then `priority_union` on synthesis; `Add`/`Clear` on
//! analysis), and those values cannot be interned into the immutable grammar interner
//! (`grammar.fs_interner`, a frozen contract). Grammar-tier requirement/output FSs stay `FsId` and
//! are resolved through `grammar.fs_interner` at use. `mpr: MprSet` and `obligatory: Vec<FeatId>`
//! are also owned here, since MPR gating/accumulation and obligatory-feature accumulation both
//! read/write the word's own state as rules apply.

use std::ops::Range;
use std::rc::Rc;

use pg_featstruct::{FeatId, FeatureStruct};
use pg_grammar_model::model::{
    AllomorphId, LexEntryId, MRuleId, MorphemeId, MprSet, PRuleId, StratumId,
};
use pg_shape::{NodeKind, Shape};

use crate::alternative::{Alternative, Alternatives};
use crate::extras::{Extras, SpanMarks};
use crate::trail::{SlotView, Trail};

/// `size_of::<Word>()` when the byte estimates were calibrated.
pub(crate) const LEGACY_WORD_BYTES: usize = 320;

/// State used by the final-template interleaving prune. `None` means the most recently applied
/// rule was a template/realizational rule (or that no ordinary rule has established a poisoned
/// state); `NonTemplate` means an ordinary affix or compounding rule was applied.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum FinalTemplateState {
    #[default]
    None,
    NonTemplate,
}

/// Per-word gating flags (subset of C# `Word`'s flags that this milestone can compute without the
/// deferred rule-count/trail machinery).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WordFlags {
    /// Bit 0 is C# `Word.IsPartial`; bit 1 enables diagnostics without growing Word.
    pub(crate) partial: u8,
    /// C# `Word.IsLastAppliedRuleFinal` (`bool?`): `None` = no template applied yet.
    pub is_last_applied_rule_final: Option<bool>,
    /// Whether the latest successful rule application was an ordinary non-template rule.
    pub final_template_state: FinalTemplateState,
}

impl WordFlags {
    #[inline]
    pub fn is_partial(&self) -> bool {
        self.partial & 1 != 0
    }

    #[inline]
    pub fn set_is_partial(&mut self, value: bool) {
        self.partial = (self.partial & !1) | u8::from(value);
    }
}

/// One applied allomorph, recorded in **morph order** (the batch signature is
/// `join("+", morpheme.Id)` over the word's morphs in surface order).
///
/// `Real.order` is a positioned run's start; singleton statuses use their node anchor. The vector
/// stores top-level range order with each genuine child block immediately before its host.
// C layout keeps defined fields at bytes 8..16: with passed_over there, None left that word undefined and the compiler filled it from a register holding glibc's tcache key, so every later free took the double-free scan.
#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct MorphRecord {
    pub allomorph: AllomorphId,
    pub morpheme: MorphemeId,
    pub order: u32,
    /// Diagnostic application identity within the word. The primary root is zero, and
    /// separate positioned runs of one application keep the same identity.
    pub observation_occurrence: u16,
    /// How this record is anchored — see `MorphStatus` and `pg_rules::morph::attribute_morphs`'s
    /// doc comment for the C# mechanism each variant ports. `Real` for root records.
    pub status: MorphStatus,
    /// Runtime root payload attached to this morph (never word-wide), for supplied/guessed roots.
    pub runtime_root: Option<Rc<RuntimeRoot>>,
    /// The same-rule allomorph indices that successfully
    /// applied *before* this one within the same `SynthesisAffixProcessRule.Apply` loop — C#'s
    /// per-morphID `Word._disjunctiveAllomorphIndices` entry (`appliedAllomorphIndices`,
    /// `SynthesisAffixProcessRule.cs:138,201-202`), stored on the record itself since this port's
    /// records have no morphID key. `Some` (possibly empty) on every affix record minted by
    /// `synth_affix`; `None` on root records (C# `GetDisjunctiveAllomorphApplications` returns
    /// null for them — morphID `"ROOT"` is never a rule-application key — and the validity gate
    /// then falls back to `Enumerable.Range(0, Index)`, `Allomorph.cs:127`). Carried across later
    /// rule applications by `attribute_morphs`' record inheritance, mirroring how C#'s dictionary
    /// rides along in `Word`'s copy constructor (Word.cs:112-115).
    pub passed_over: Option<Box<[u16]>>,
}

/// The current annotation role used by the flat postorder representation.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum MorphStatus {
    /// Positioned run envelope; a later singleton overlay can own its first node instead.
    Real,
    /// Empty top-level singleton at `order`.
    Floating,
    /// Genuine descendant block; records precede their host and share its range.
    SubsumedChild,
    /// Top-level singleton that currently owns the node at `order`.
    SubsumedFirst,
}

/// One morph record as `Word::span_attribution` reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpanRecord {
    pub morpheme: MorphemeId,
    pub allomorph: AllomorphId,
    /// The application identity runs of one rule application share.
    pub occurrence: u16,
    /// The record shares its range with another record, so no segment is its alone.
    pub nested: bool,
    /// A phonological rule made this run's segments ambiguous.
    pub lost: Option<PRuleId>,
    /// A reduplication made which copy is the base arbitrary.
    pub ambiguous_copy: bool,
}

/// Who owns one interior node of a word's shape.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SegmentOwner {
    /// The index into `Word::morphs` of the record owning the node.
    Record(usize),
    /// A phonological rewrite inserted the node.
    Inserted(PRuleId),
    /// No morph range holds the node.
    Unowned,
}

/// The engine's own answer to which morph owns each segment of the final shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpanAttribution {
    /// Parallel to `Word::morphs`.
    pub records: Vec<SpanRecord>,
    /// One per interior node of the shape (anchors excluded).
    pub owners: Vec<SegmentOwner>,
    /// Every attribution is unreliable; the rule is the first one that left a mark.
    pub poisoned: Option<PRuleId>,
    /// An affix that inserts nothing left a positional mark on a real segment, so no morph's spans hold.
    pub positional: bool,
    /// The word has compounding non-heads, whose segments are folded into one root morph.
    pub compound: bool,
}

/// A top-level annotation and its contiguous postorder child block.
#[derive(Clone, Debug)]
pub(crate) struct MorphGroup {
    pub host: usize,
    pub children: Range<usize>,
    pub range: (u32, u32),
}

/// Transient ranges and segment owners decoded from the existing flat records.
#[derive(Clone, Debug)]
pub(crate) struct MorphLayout {
    pub groups: Vec<MorphGroup>,
    pub record_ranges: Vec<(u32, u32)>,
    pub node_owners: Vec<Option<usize>>,
}

impl MorphLayout {
    /// Infer envelopes from positioned starts, then overlay owning singletons. Unowned gaps inside
    /// those inferred envelopes (such as compound linkers) cannot be reconstructed by this decoder.
    pub(crate) fn decode(morphs: &[MorphRecord], shape_len: usize, extras: &Extras) -> Self {
        let bare_nodes = extras.bare_nodes();
        let last = shape_len.saturating_sub(3) as u32;
        let mut groups = Vec::with_capacity(morphs.len());
        let mut index = 0;
        while index < morphs.len() {
            if morphs[index].status == MorphStatus::SubsumedChild {
                let first_child = index;
                let anchor = morphs[index].order;
                while index < morphs.len()
                    && morphs[index].status == MorphStatus::SubsumedChild
                    && morphs[index].order == anchor
                {
                    index += 1;
                }
                assert!(
                    index < morphs.len() && morphs[index].order == anchor,
                    "SubsumedChild block at {anchor} must immediately precede its host"
                );
                let host = index;
                groups.push(MorphGroup {
                    host,
                    children: first_child..host,
                    range: (anchor, anchor),
                });
                index += 1;
            } else {
                let host = index;
                groups.push(MorphGroup {
                    host,
                    children: host..host,
                    range: (morphs[host].order, morphs[host].order),
                });
                index += 1;
            }
        }

        for group_index in 0..groups.len() {
            let host = groups[group_index].host;
            let morph = &morphs[host];
            if morph.status == MorphStatus::Real {
                let end = groups[group_index + 1..]
                    .iter()
                    .map(|group| &morphs[group.host])
                    .find(|next| next.status == MorphStatus::Real && next.order > morph.order)
                    .map_or(last, |next| next.order.saturating_sub(1).max(morph.order));
                // C# AllomorphEnvironment.cs:88-97 reads `morph.Range.End.Next`, and an annotation's range never holds a node inserted at its edge (AnnotationList.cs:131-138).
                let mut end = end;
                while end > morph.order && bare_nodes.contains(&end) {
                    end -= 1;
                }
                // A swap that wrapped this morph around another left its true end here, which no later morph's start implies.
                let stretched = extras.stretched_ends();
                if !stretched.is_empty() {
                    let run = morphs[..host]
                        .iter()
                        .filter(|r| {
                            r.allomorph == morph.allomorph
                                && r.observation_occurrence == morph.observation_occurrence
                        })
                        .count() as u16;
                    let key = (morph.allomorph, morph.observation_occurrence, run);
                    if let Some(&(_, stored)) = stretched.iter().find(|(k, _)| *k == key) {
                        end = stored.min(last).max(morph.order);
                    }
                }
                groups[group_index].range = (morph.order, end);
            }
        }

        let mut record_ranges = vec![(0, 0); morphs.len()];
        for group in &groups {
            for record_range in record_ranges
                .iter_mut()
                .take(group.host + 1)
                .skip(group.children.start)
            {
                *record_range = group.range;
            }
        }

        let mut node_owners = vec![None; shape_len.saturating_sub(2)];
        for (group_index, group) in groups.iter().enumerate() {
            if morphs[group.host].status == MorphStatus::Real {
                for order in group.range.0..=group.range.1 {
                    if let Some(owner) = node_owners.get_mut(order as usize) {
                        *owner = Some(group_index);
                    }
                }
            }
        }
        for &bare in bare_nodes {
            if let Some(owner) = node_owners.get_mut(bare as usize) {
                *owner = None;
            }
        }
        for (group_index, group) in groups.iter().enumerate() {
            if morphs[group.host].status == MorphStatus::SubsumedFirst {
                if let Some(owner) = node_owners.get_mut(group.range.0 as usize) {
                    *owner = Some(group_index);
                }
            }
        }

        Self {
            groups,
            record_ranges,
            node_owners,
        }
    }
}

impl MorphRecord {
    /// A record with no passed-over set (root records and hand-built test records).
    pub fn new(allomorph: AllomorphId, morpheme: MorphemeId, order: u32) -> Self {
        MorphRecord {
            allomorph,
            morpheme,
            order,
            observation_occurrence: 0,
            passed_over: None,
            status: MorphStatus::Real,
            runtime_root: None,
        }
    }

    pub fn with_runtime_root(mut self, root: RuntimeRoot) -> Self {
        self.runtime_root = Some(Rc::new(root));
        self
    }
}

/// The fabricated root's payload. `Grammar` is immutable and shared across parses/threads, so a
/// guessed allomorph/entry/morpheme — which C# fabricates as fresh `RootAllomorph`/`LexEntry`
/// objects with no place in any grammar table (`Morpher.LexicalGuess`, `Morpher.cs:522-590`) —
/// cannot be appended to it. Instead `Word::root_allomorph` and the root `MorphRecord` carry the
/// sentinels `AllomorphId::GUESSED` / `MorphemeId::GUESSED`, and this payload is looked up on the
/// word itself wherever the real content is needed.
///
/// Every id-resolution site that reads `allomorph_owners`/`entries`/`morphemes` with a word's
/// morph ids must special-case the sentinel and delegate to `pattern_allo`/`pattern_entry` here —
/// which are REAL, ordinary grammar ids (the lexical-pattern allomorph the guess matched, e.g.
/// `[Any]*`; only the fabricated root ITSELF has no table row, never the pattern it came from).
/// See `pg-parse/src/guess.rs`'s module doc for how this is fabricated, and
/// `pg-rules/src/validity.rs::allomorphs_valid_impl`'s sentinel branch for the first, and so far
/// only, resolution site this port needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuessedRoot {
    /// The lexical-pattern root allomorph the guess matched against (C#'s `patternAllomorph`) —
    /// resolvable through `Grammar::allomorph_owners`/`entries` exactly like any other allomorph
    /// id; only the fabricated root carries a sentinel, never this.
    pub pattern_allo: AllomorphId,
    /// The lexical entry that owns `pattern_allo` (redundant with
    /// `Grammar::allomorph_owners[pattern_allo]`'s `Root(entry, _)`, stored directly since every
    /// resolution site needs the owning entry without re-deriving it).
    pub pattern_entry: LexEntryId,
    /// The rendered guess string (`pg-parse/src/guess.rs::render_match`'s output). Doubles as C#'s
    /// fabricated `Id`/`Gloss`/root-`MorphemeId` string — C# sets all three to this same
    /// `shapeString` (`Morpher.cs:564-579`).
    pub text: String,
}

/// Provenance and stable identity for roots that do not live in the immutable grammar tables.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SuppliedAuthorityData {
    Supplied,
    Override { official_entry_id: String },
}

/// Self-contained supplied root data used by both ordinary and compound lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuppliedRootData {
    pub entry_id: String,
    pub realization_id: String,
    pub authority: SuppliedAuthorityData,
    pub lexical_spelling: String,
    pub gloss: String,
    pub syn_fs: FeatureStruct,
    pub mpr: MprSet,
    pub stratum: StratumId,
}

/// Generalized side channel for roots that have no grammar allomorph/entry row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeRoot {
    Guessed(GuessedRoot),
    Supplied(SuppliedRootData),
}

/// Composite result crossing the pg-parse → pg-rules compounding lookup boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedRoot {
    Grammar(AllomorphId, LexEntryId),
    Supplied(SuppliedRootData),
}

pub fn runtime_id(root: Option<&RuntimeRoot>) -> Option<&str> {
    match root {
        Some(RuntimeRoot::Guessed(root)) => Some(&root.text),
        Some(RuntimeRoot::Supplied(root)) => Some(&root.realization_id),
        None => None,
    }
}

/// The in-flight parse state. See the module docs for the owned-vs-interned tradeoffs.
#[derive(Clone, Debug)]
pub struct Word {
    /// The phonetic shape (owned; per-parse interning is not done).
    pub shape: Shape,
    pub stratum: StratumId,
    /// Syntactic feature structure (owned — see module docs).
    pub syn_fs: FeatureStruct,
    /// Realizational FS: `FeatureStruct::EMPTY` except while a `RealizationalAffixProcessRule` is
    /// threading its own realizational value through `ana_realizational`/`synth_realizational`
    /// (`pg_rules::morph`), or on a generation seed built by `pg_parse::morpher`'s
    /// `generate_words` with a caller-supplied non-empty value (C#
    /// `Morpher.GenerateWords`'s `realizationalFS` parameter, Morpher.cs:169-182).
    pub real_fs: FeatureStruct,
    /// Morphological/phonological rule (MPR) feature set (see module docs).
    pub mpr: MprSet,
    /// Applied allomorphs in morph order (see `MorphRecord`).
    pub morphs: Vec<MorphRecord>,
    /// C# `Word._nonHeadAppIndex` (init `-1` = none). During analysis this only ever grows
    /// (`NonHeadUnapplied` pushes + increments; nothing pops), so it invariably equals
    /// `non_heads().len() as i32 - 1`. It is a **dedup-key** component (Word.cs:520,540), not a
    /// cascade driver — the cascades track their own rule position via `AnalysisStratumRule`.
    pub non_head_app_index: i32,
    /// The rule and allomorph of every unapplication so far, with per-rule unapplication counts (C# `Word._mruleApps` and `_mrulesUnapplied`); a `None` rule is C#'s unknown compounding rule, matched by any compounding rule in synthesis.
    /// Its slots are a dedup-key component (Word.cs:523,543): every unapplication grows them, so a rule output never equals its input.
    pub trail: Trail,
    /// Allomorph selected by the current analysis call, before its rule is added to the trail.
    pub analysis_allomorph: Option<AllomorphId>,
    /// Compounding non-heads, realizational and phonological rules applied, and the runtime root id: empty on almost every word.
    pub(crate) extras: Extras,
    /// C# `Word._mruleAppIndex` (init `-1`). Grows in lock-step with `trail` during analysis, so it always equals `trail.len() as i32 - 1`. A dedup-key component (Word.cs:524,544), not a cascade driver.
    pub mrule_app_index: i32,
    /// The matched root allomorph (C# `Word._rootAllomorph`). Set by lexical lookup; stays
    /// `None` throughout analysis (the surface word starts from `Word(Stratum, Shape)`, which
    /// leaves it null). A dedup-key component (Word.cs:522,542).
    pub root_allomorph: Option<AllomorphId>,
    /// Obligatory syntactic features accumulated by applied rules (C#
    /// `Word.ObligatorySyntacticFeatures`).
    pub obligatory: Vec<FeatId>,
    pub flags: WordFlags,
    /// C# `Word.Source` (Word.cs:86,491-533): the word this one was derived from at a **stratum
    /// boundary**. `AnalysisStratumRule.Apply` sets every output word's `Source` to the stratum's
    /// input (`mruleOutWord.Source = origInput`, AnalysisStratumRule.cs:165), and the seed's `Source`
    /// is that same input (from `input.Clone()`, cs:106). So the chain of `source` links is exactly
    /// the per-stratum input words back to the surface form — the spine `Word::expand_alternatives`
    /// walks to distribute a merged word's rule/non-head deltas across its alternatives. `None` on the
    /// surface input word (chain root) and on any word never passed through a stratum boundary.
    ///
    /// `Rc` (not `Box`): many stratum outputs share one input, and the chain is walked, never mutated,
    /// so sharing is both cheaper and identity-correct. **Not** part of `WordKey` / dedup (C#
    /// `ValueEquals` never reads `Source`), and cleared-not-copied on the seed/lexical-lookup clones
    /// (see those sites) so it can never inflate the key or the candidate set.
    pub source: Option<Rc<Word>>,
    /// `Some` iff this word's root is a guessed (fabricated) one — equivalently iff
    /// `root_allomorph == Some(AllomorphId::GUESSED)` — carrying the payload every sentinel-id
    /// resolution site needs (see `GuessedRoot`'s doc). `None` for every ordinary (real-lexicon)
    /// word, the overwhelming majority. `Rc` for the same reason `source` is: words are cloned
    /// heavily, the payload is immutable once fabricated, and sharing is both cheaper and
    /// identity-correct. **Not** part of `WordKey` (C# has no notion of it in `ValueEquals` —
    /// `Word._rootAllomorph` alone, which already includes the fabricated `RootAllomorph`'s
    /// object identity, is what C#'s dedup keys on; `root_allomorph`'s `AllomorphId::GUESSED`
    /// sentinel is the Rust analog of that, so this payload would be redundant in the key even if
    /// added).
    /// C# `Word.Alternatives` (Word.cs:485-489): the analysis candidates folded into this word by
    /// `MergeEquivalentAnalyses` (AnalysisStratumRule.cs:161-171 — a candidate reaching an equal
    /// `AnalysisStateKey`, or an equal `WordKey` differing only in syntactic FS, does not
    /// enter the output set; instead `canonicalWord.Alternatives.Add(mruleOutWord)`). They differ
    /// from the canonical in rule/non-head history and (widened into the canonical via
    /// `pg_featstruct::union` on the fold — see `crate::stratum`'s merge) syntactic FS, and are
    /// re-expanded at synthesis by `Word::expand_alternatives`. Empty on almost every word. **Not**
    /// part of `WordKey`; **not** copied by `Word::clone_without_alternatives` (C#'s copy ctor
    /// leaves `_alternatives` fresh-empty, Word.cs:87), so only the two sites that build a canonical
    /// (the stratum merge) ever populate it.
    ///
    /// Held in the stored form `crate::alternative` describes; clones of the canonical share one list.
    pub alternatives: Alternatives,
    /// Mirrors C# `Word.CurrentTrace` (`object`), the live cursor a `TraceManager`
    /// call reassigns as the parse progresses (`Trace.cs`/`TraceManager.cs`; see
    /// `crate::trace`'s module doc for why this port carries the cursor as an explicit
    /// `crate::trace::TraceHandle` value alongside the word instead of a mutated field the sink
    /// owns). `None` when tracing is off (the overwhelming common case — adds one `Option<u32>`-sized
    /// field, no allocation) or on any `Word` never touched by a traced call. Threaded through every
    /// existing clone point (the derived `Clone`, `Word::clone_without_alternatives`)
    /// exactly as C#'s `CurrentTrace` rides along `Word.Clone()` (Word.cs:110)
    /// — **not** part of `WordKey` (C# `ValueEquals`/`FreezeImpl` never reads `CurrentTrace` either).
    pub trace: Option<crate::trace::TraceHandle>,
}

/// Canonical dedup key for `Word`, retaining the source-bearing morph trail as well as the engine
/// state. The cascades and the stratum orchestrator dedup on this key; `AnalysisStateKey`
/// remains the separate rule-state identity the merge fold uses.
///
/// The state fields follow C# `Word.ValueEquals` (Word.cs:537-545), while `morphs` is retained so
/// two candidates with the same rendered shape but different selected source allomorphs, MSA,
/// inflection type, or annotation order cannot disappear before structured projection.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WordKey {
    shape: Shape,
    real_fs: FeatureStruct,
    morphs: Vec<MorphKey>,
    stretched_ends: Vec<((AllomorphId, u16, u16), u32)>,
    non_heads: Vec<WordKey>,
    non_head_app_index: i32,
    stratum: StratumId,
    root_allomorph: Option<AllomorphId>,
    root_realization: Option<String>,
    trail: SlotView,
    applied_realizational_rules: Vec<MorphemeId>,
    mrule_app_index: i32,
    is_last_applied_rule_final: Option<bool>,
    final_template_state: FinalTemplateState,
}

// C layout keeps defined fields at bytes 8..16, see MorphRecord.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
struct MorphKey {
    allomorph: AllomorphId,
    morpheme: MorphemeId,
    order: u32,
    status: MorphStatus,
    runtime_identity: Option<String>,
}

/// The fields a `WordKey` is taken from, borrowed from wherever they are held: a stored word that repeats most of another word keeps only its own fields, and its key is still read in place.
#[derive(Clone, Copy)]
pub(crate) struct KeyFields<'a> {
    pub shape: &'a Shape,
    pub trail: &'a Trail,
    pub mrule_app_index: i32,
    pub final_template_state: FinalTemplateState,
    /// The word every other key field is read from.
    pub rest: &'a Word,
}

impl<'a> KeyFields<'a> {
    pub(crate) fn of(w: &'a Word) -> Self {
        KeyFields {
            shape: &w.shape,
            trail: &w.trail,
            mrule_app_index: w.mrule_app_index,
            final_template_state: w.flags.final_template_state,
            rest: w,
        }
    }
}

impl WordKey {
    /// Whether `w.dedup_key()` would equal this key, answered by reading `w` in place so no key is built. This is what lets a dedup index hold hash and position only instead of a second copy of every word's key; `tests::matches_agrees_with_key_equality` holds it to `==`.
    pub fn matches(&self, w: &Word) -> bool {
        self.matches_fields(&KeyFields::of(w))
    }

    /// `matches` against the word these fields describe.
    pub(crate) fn matches_fields(&self, f: &KeyFields<'_>) -> bool {
        let w = f.rest;
        self.trail.matches(f.trail)
            && self.non_head_app_index == w.non_head_app_index
            && self.stratum == w.stratum
            && self.root_allomorph == w.root_allomorph
            && self.mrule_app_index == f.mrule_app_index
            && self.is_last_applied_rule_final == w.flags.is_last_applied_rule_final
            && self.final_template_state == f.final_template_state
            && self.root_realization.as_deref() == w.root_runtime_id()
            && self.applied_realizational_rules == w.applied_realizational_rules()
            && self.morphs.len() == w.morphs.len()
            && self.morphs.iter().zip(&w.morphs).all(|(k, m)| {
                k.allomorph == m.allomorph
                    && k.morpheme == m.morpheme
                    && k.order == m.order
                    && k.status == m.status
                    && k.runtime_identity.as_deref() == runtime_id(m.runtime_root.as_deref())
            })
            && self.stretched_ends == w.extras.stretched_ends()
            && self.non_heads.len() == w.non_heads().len()
            && self
                .non_heads
                .iter()
                .zip(w.non_heads())
                .all(|(k, nh)| k.matches(nh))
            && self.real_fs == w.real_fs
            && self.shape == *f.shape
    }

    /// The key of the word these fields describe.
    pub(crate) fn of(f: &KeyFields<'_>) -> WordKey {
        let w = f.rest;
        WordKey {
            shape: f.shape.clone(),
            real_fs: w.real_fs.clone(),
            morphs: w
                .morphs
                .iter()
                .map(|morph| MorphKey {
                    allomorph: morph.allomorph,
                    morpheme: morph.morpheme,
                    order: morph.order,
                    status: morph.status,
                    runtime_identity: runtime_id(morph.runtime_root.as_deref()).map(str::to_owned),
                })
                .collect(),
            stretched_ends: w.extras.stretched_ends().to_vec(),
            non_heads: w.non_heads().iter().map(Word::dedup_key).collect(),
            non_head_app_index: w.non_head_app_index,
            stratum: w.stratum,
            root_allomorph: w.root_allomorph,
            root_realization: w.root_runtime_id().map(str::to_owned),
            trail: f.trail.slot_view(),
            applied_realizational_rules: w.applied_realizational_rules().to_vec(),
            mrule_app_index: f.mrule_app_index,
            is_last_applied_rule_final: w.flags.is_last_applied_rule_final,
            final_template_state: f.final_template_state,
        }
    }
}

impl Word {
    /// Re-anchor morph records after a phonological rule moved nodes, as C# `SynthesisMetathesisRuleSpec.ApplyRhs` re-adds each morph annotation from its children's new positions; `origin[i]` is the old index (anchors included) of the node now at `i`.
    pub(crate) fn reorder_morphs(&mut self, origin: &[usize]) {
        let shape_len = self.shape.len();
        if origin.len() != shape_len {
            return;
        }
        let layout = MorphLayout::decode(&self.morphs, shape_len, &self.extras);
        let interior = shape_len.saturating_sub(2);
        let mut new_of_old = vec![0u32; interior];
        for new in 0..interior {
            new_of_old[origin[new + 1] - 1] = new as u32;
        }
        let bare: Vec<u32> = self
            .extras
            .bare_nodes()
            .iter()
            .filter_map(|&b| new_of_old.get(b as usize).copied())
            .collect();
        // C# SynthesisMetathesisRuleSpec.ApplyRhs moves only the two switch groups and a deleted node is in neither, so the gaps keep their positions.
        let gaps = self.extras.deleted_gaps().to_vec();
        self.extras.set_shape_marks(gaps, bare);
        let mut starts: Vec<u32> = layout.groups.iter().map(|g| g.range.0).collect();
        let mut pinned: Vec<bool> = layout
            .groups
            .iter()
            .map(|g| {
                self.morphs[g.host].status == MorphStatus::Real
                    && self.extras.pinned_starts().contains(&self.run_key(g.host))
            })
            .collect();
        for (group_index, group) in layout.groups.iter().enumerate() {
            let host = &self.morphs[group.host];
            starts[group_index] = match host.status {
                MorphStatus::Real => {
                    let first_node = layout
                        .node_owners
                        .iter()
                        .enumerate()
                        .filter(|&(_, owner)| *owner == Some(group_index))
                        .map(|(old, _)| new_of_old[old])
                        .min()
                        .unwrap_or(group.range.0);
                    // A deleted first node stays where it was, just before the node now at its old start.
                    pinned[group_index] &= group.range.0 <= first_node;
                    if pinned[group_index] {
                        group.range.0
                    } else {
                        first_node
                    }
                }
                MorphStatus::SubsumedFirst => new_of_old
                    .get(host.order as usize)
                    .copied()
                    .unwrap_or(host.order),
                MorphStatus::Floating | MorphStatus::SubsumedChild => group.range.0,
            };
        }
        // Where each group's last node went: C# re-creates every overlapped morph as its leftmost child to its rightmost, which can wrap another morph.
        let wanted_ends: Vec<Option<u32>> = layout
            .groups
            .iter()
            .enumerate()
            .map(|(group_index, group)| {
                if self.morphs[group.host].status != MorphStatus::Real {
                    return None;
                }
                layout
                    .node_owners
                    .iter()
                    .enumerate()
                    .filter(|&(_, owner)| *owner == Some(group_index))
                    .map(|(old, _)| new_of_old[old])
                    .max()
            })
            .collect();
        let mut new_host: Vec<usize> = layout.groups.iter().map(|g| g.host).collect();
        if !layout
            .groups
            .iter()
            .zip(&starts)
            .all(|(group, &start)| group.range.0 == start)
        {
            let mut sequence: Vec<usize> = (0..layout.groups.len()).collect();
            sequence.sort_by_key(|&g| (starts[g], !pinned[g]));
            let mut morphs = Vec::with_capacity(self.morphs.len());
            for g in sequence {
                let group = &layout.groups[g];
                for record in &self.morphs[group.children.start..=group.host] {
                    let mut record = record.clone();
                    record.order = starts[g];
                    morphs.push(record);
                }
                new_host[g] = morphs.len() - 1;
            }
            self.morphs = morphs;
        }
        // Keep an end only where the next morph's start would not imply it.
        self.extras.set_stretched_ends(Vec::new());
        let inferred = MorphLayout::decode(&self.morphs, shape_len, &self.extras);
        let mut ends = Vec::new();
        for (group_index, wanted) in wanted_ends.into_iter().enumerate() {
            let Some(wanted) = wanted else { continue };
            let host = new_host[group_index];
            let implied = inferred
                .groups
                .iter()
                .find(|group| group.host == host)
                .map(|group| group.range.1);
            if implied != Some(wanted) {
                ends.push((self.run_key(host), wanted));
            }
        }
        self.extras.set_stretched_ends(ends);
    }

    /// Whether this word records what phonological rules do to morph identity, for `Word::span_attribution`.
    #[inline]
    pub fn track_spans(&self) -> bool {
        self.flags.partial & 4 != 0
    }

    #[inline]
    pub fn set_track_spans(&mut self, enabled: bool) {
        self.flags.partial = (self.flags.partial & !4) | (u8::from(enabled) << 2);
    }

    /// The run key of the morph owning old interior node `old`, unless a rule inserted that node.
    fn node_run(
        &self,
        layout: &MorphLayout,
        marks: &SpanMarks,
        old: usize,
    ) -> Option<(AllomorphId, u16, u16)> {
        if marks.inserted.iter().any(|&(node, _)| node as usize == old) {
            return None;
        }
        let group = layout.node_owners.get(old).copied().flatten()?;
        Some(self.run_key(layout.groups[group].host))
    }

    /// Before a synthesis rewrite replaces the shape (`self` still holds the old one and its morphs): records the nodes it inserts and the morphs whose segments it merges across an edge.
    pub(crate) fn note_rewrite_spans(&mut self, new_shape: &Shape, keys: &[u32], rule: PRuleId) {
        if !self.track_spans() {
            return;
        }
        let old_len = self.shape.len();
        let interior = old_len.saturating_sub(2);
        let layout = MorphLayout::decode(&self.morphs, old_len, &self.extras);
        let mut marks = self.extras.span_marks().cloned().unwrap_or_default();
        let kept = |old: usize| keys.iter().position(|&key| key == 2 * (old as u32 + 1));
        let mut inserted: Vec<(u32, PRuleId)> = marks
            .inserted
            .iter()
            .filter_map(|&(old, by)| kept(old as usize).map(|new| (new as u32, by)))
            .collect();
        inserted.extend(
            keys.iter()
                .enumerate()
                .filter(|(_, key)| *key % 2 == 1)
                .map(|(new, _)| (new as u32, rule)),
        );
        inserted.sort_unstable();
        let old_marks = marks.clone();
        marks.inserted = inserted;
        let is_boundary = |old: usize| self.shape.kind(old + 1) == NodeKind::Boundary;
        let differs = |old: usize, new: usize| {
            let (a, b) = (old + 1, new + 1);
            self.shape.kind(a) != new_shape.kind(b)
                || self.shape.char_def(a) != new_shape.char_def(b)
                || self.shape.node_lanes(a) != new_shape.node_lanes(b)
        };
        for old in 0..interior {
            if kept(old).is_some() || self.shape.flags(old + 1).is_optional() || is_boundary(old) {
                continue;
            }
            let Some(deleted) = self.node_run(&layout, &old_marks, old) else {
                continue;
            };
            // The nearest kept segment on each side, past boundaries and other deleted nodes.
            let left = (0..old)
                .rev()
                .find(|&i| kept(i).is_some() && !is_boundary(i));
            let right = (old + 1..interior).find(|&i| kept(i).is_some() && !is_boundary(i));
            for neighbour in left.into_iter().chain(right) {
                let (Some(new), Some(owner)) = (
                    kept(neighbour),
                    self.node_run(&layout, &old_marks, neighbour),
                ) else {
                    continue;
                };
                if owner != deleted && differs(neighbour, new) {
                    marks.lose(deleted, rule);
                    marks.lose(owner, rule);
                }
            }
        }
        *self.extras.span_marks_mut() = marks;
    }

    /// Before a metathesis re-orders the shape (`origin[i]` is the old index of the node now at `i`): records the morphs whose segments it moved across an edge.
    pub(crate) fn note_metathesis_spans(&mut self, origin: &[usize], rule: PRuleId) {
        let shape_len = self.shape.len();
        if !self.track_spans() || origin.len() != shape_len {
            return;
        }
        let interior = shape_len.saturating_sub(2);
        let layout = MorphLayout::decode(&self.morphs, shape_len, &self.extras);
        let mut marks = self.extras.span_marks().cloned().unwrap_or_default();
        let mut new_of_old = vec![0u32; interior];
        for new in 0..interior {
            new_of_old[origin[new + 1] - 1] = new as u32;
        }
        let mut owners: Vec<(AllomorphId, u16, u16)> = Vec::new();
        let mut unowned_moved = false;
        for new in 0..interior {
            let old = origin[new + 1] - 1;
            if old == new {
                continue;
            }
            match self.node_run(&layout, &marks, old) {
                Some(run) => {
                    if !owners.contains(&run) {
                        owners.push(run);
                    }
                }
                None => {
                    unowned_moved |= !marks.inserted.iter().any(|&(node, _)| node as usize == old);
                }
            }
        }
        if owners.len() > 1 || (unowned_moved && !owners.is_empty()) {
            for run in owners {
                marks.lose(run, rule);
            }
        }
        for (node, _) in &mut marks.inserted {
            *node = new_of_old[*node as usize];
        }
        marks.inserted.sort_unstable();
        *self.extras.span_marks_mut() = marks;
    }

    /// Which morph record owns each interior node, as the engine's own morph ranges say, with the doubts phonological rules left behind (see `SpanMarks`).
    pub fn span_attribution(&self) -> SpanAttribution {
        let layout = MorphLayout::decode(&self.morphs, self.shape.len(), &self.extras);
        let empty = SpanMarks::default();
        let marks = self.extras.span_marks().unwrap_or(&empty);
        let mut records: Vec<SpanRecord> = self
            .morphs
            .iter()
            .map(|record| SpanRecord {
                morpheme: record.morpheme,
                allomorph: record.allomorph,
                occurrence: record.observation_occurrence,
                nested: false,
                lost: None,
                ambiguous_copy: marks
                    .ambiguous_copy
                    .contains(&(record.allomorph, record.observation_occurrence)),
            })
            .collect();
        for group in &layout.groups {
            if !group.children.is_empty() {
                for record in &mut records[group.children.start..=group.host] {
                    record.nested = true;
                }
            }
            if self.morphs[group.host].status == MorphStatus::Real {
                let run = (
                    self.morphs[group.host].allomorph,
                    self.morphs[group.host].observation_occurrence,
                    0,
                );
                records[group.host].lost = marks
                    .lost
                    .iter()
                    .find(|(lost, _)| *lost == run)
                    .map(|(_, rule)| *rule);
            }
        }
        let owners = layout
            .node_owners
            .iter()
            .enumerate()
            .map(|(node, owner)| {
                if let Some(&(_, rule)) = marks.inserted.iter().find(|&&(n, _)| n as usize == node)
                {
                    SegmentOwner::Inserted(rule)
                } else {
                    match owner {
                        Some(group) => SegmentOwner::Record(layout.groups[*group].host),
                        None => SegmentOwner::Unowned,
                    }
                }
            })
            .collect();
        SpanAttribution {
            records,
            owners,
            poisoned: marks.poisoned,
            positional: !marks.positional.is_empty(),
            compound: !self.non_heads().is_empty(),
        }
    }

    /// A record's allomorph, application and how many earlier records share both, so each run of a circumfix has its own key.
    fn run_key(&self, index: usize) -> (AllomorphId, u16, u16) {
        let record = &self.morphs[index];
        let run = self.morphs[..index]
            .iter()
            .filter(|r| {
                r.allomorph == record.allomorph
                    && r.observation_occurrence == record.observation_occurrence
            })
            .count();
        (record.allomorph, record.observation_occurrence, run as u16)
    }

    /// Re-anchor morph records after a synthesis rewrite deleted or inserted nodes. `old_len` is the shape's length before the rewrite, and `keys` holds each kept node's `MutShape::anchor_keys` entry: twice its old shape index, or an odd key for an inserted node. C# keeps a deleted node in the shape, so its morph annotations keep their places; here each record starts at the first kept node at or after its old start, a positioned morph that kept no node becomes an empty singleton there, so it neither owns its neighbour's segments nor leaves the morph order, and a morph that lost its first node keeps that start through a later swap.
    pub(crate) fn remap_morph_orders(&mut self, old_len: usize, keys: &[u32]) {
        let bare_before = self.extras.bare_nodes().to_vec();
        let layout = MorphLayout::decode(&self.morphs, old_len, &self.extras);
        let new_start = |old: u32| keys.partition_point(|&key| key < 2 * (old + 1)) as u32;
        let is_bare = |key: u32| key.is_multiple_of(2) && bare_before.contains(&(key / 2 - 1));
        for group in &layout.groups {
            let host = &self.morphs[group.host];
            let emptied = host.status == MorphStatus::Real && {
                let (start, end) = group.range;
                // C# AnnotationList.cs:131-138 nests an inserted node only inside a morph's span, so an insertion past its last node or a bare node does not keep it.
                let (low, high) = (2 * (start + 1), 2 * (end + 1));
                !keys
                    .iter()
                    .any(|&key| (low..=high).contains(&key) && !is_bare(key))
            };
            if emptied {
                self.morphs[group.host].status = MorphStatus::Floating;
            } else if host.status == MorphStatus::Real && !keys.contains(&(2 * (group.range.0 + 1)))
            {
                let pinned = self.run_key(group.host);
                self.extras.pin_start(pinned);
            }
        }
        let interior_old = old_len.saturating_sub(2) as u32;
        let mut gaps: Vec<u32> = self
            .extras
            .deleted_gaps()
            .iter()
            .map(|&gap| new_start(gap))
            .collect();
        for old in 0..interior_old {
            // C# `NarrowSynthesisRewriteSubruleSpec.ApplyRhs` marks a node deleted but leaves it in the shape; an Optional one is walked over by `CollectSkippedOptionalNodes`, so only others block a fold.
            if !keys.contains(&(2 * (old + 1))) && !self.shape.flags(old as usize + 1).is_optional()
            {
                gaps.push(new_start(old));
            }
        }
        gaps.sort_unstable();
        gaps.dedup();
        let mut bare: Vec<u32> = bare_before
            .iter()
            .filter_map(|&b| keys.iter().position(|&key| key == 2 * (b + 1)))
            .map(|index| index as u32)
            .collect();
        for (index, &key) in keys.iter().enumerate() {
            // C# AnnotationList.cs:131-138 nests an inserted node only inside a range that contains it, so one on a morph's edge stays bare.
            let inside = key % 2 == 1
                && layout.groups.iter().any(|group| {
                    let (start, end) = group.range;
                    self.morphs[group.host].status == MorphStatus::Real
                        && 2 * (start + 1) < key
                        && key < 2 * (end + 1)
                });
            if key % 2 == 1 && !inside {
                bare.push(index as u32);
            }
        }
        bare.sort_unstable();
        // A stretched end follows the last of its node that the rewrite kept.
        let ends: Vec<_> = self
            .extras
            .stretched_ends()
            .iter()
            .filter_map(|&(run, end)| {
                keys.iter()
                    .rposition(|&key| key.is_multiple_of(2) && key <= 2 * (end + 1))
                    .map(|kept| (run, kept as u32))
            })
            .collect();
        self.extras.set_shape_marks(gaps, bare);
        self.extras.set_stretched_ends(ends);
        for record in &mut self.morphs {
            record.order = new_start(record.order);
        }
    }

    /// Compounding non-head children (C# `Word._nonHeadApps`; `CurrentNonHead` = the element at `Word::non_head_app_index`).
    pub fn non_heads(&self) -> &[Word] {
        self.extras.non_heads()
    }

    pub fn non_heads_mut(&mut self) -> &mut Vec<Word> {
        self.extras.non_heads_mut()
    }

    /// Owners of realizational rules already applied in synthesis; unlike morph records this survives deletion of their segments.
    pub fn applied_realizational_rules(&self) -> &[MorphemeId] {
        self.extras.applied_realizational_rules()
    }

    pub fn applied_realizational_rules_mut(&mut self) -> &mut Vec<MorphemeId> {
        self.extras.applied_realizational_rules_mut()
    }

    /// Analysis-side phonological rules that applied, populated only when stats are collected.
    pub fn applied_phonological_rules(&self) -> &[(StratumId, PRuleId)] {
        self.extras.applied_phonological_rules()
    }

    pub fn applied_phonological_rules_mut(&mut self) -> &mut Vec<(StratumId, PRuleId)> {
        self.extras.applied_phonological_rules_mut()
    }

    /// Stable identity of a runtime-backed head root; the payload remains on its `MorphRecord`.
    pub fn root_runtime_id(&self) -> Option<&str> {
        self.extras.root_runtime_id()
    }

    pub fn set_root_runtime_id(&mut self, id: Option<String>) {
        self.extras.set_root_runtime_id(id);
    }

    /// Diagnostic enablement shares the partial-flag byte, preserving Word's layout and accounting.
    #[inline]
    pub fn collect_partials(&self) -> bool {
        self.flags.partial & 2 != 0
    }

    #[inline]
    pub fn set_collect_partials(&mut self, enabled: bool) {
        self.flags.partial = (self.flags.partial & !2) | (u8::from(enabled) << 1);
    }

    /// A fresh word over `shape` in `stratum` with empty FSs / MPR / morphs (the starting state a
    /// root lookup or a test harness builds on).
    pub fn new(shape: Shape, stratum: StratumId) -> Self {
        Word {
            shape,
            stratum,
            syn_fs: FeatureStruct::EMPTY,
            real_fs: FeatureStruct::EMPTY,
            mpr: MprSet::EMPTY,
            morphs: Vec::new(),
            non_head_app_index: -1,
            trail: Trail::default(),
            analysis_allomorph: None,
            extras: Extras::default(),
            mrule_app_index: -1,
            root_allomorph: None,
            obligatory: Vec::new(),
            flags: WordFlags::default(),
            source: None,
            alternatives: Alternatives::default(),
            trace: None,
        }
    }

    /// A clone that drops `Word::alternatives` (leaves it empty), mirroring C#'s `Word(Word)` copy
    /// constructor which never copies `_alternatives` ("Don't copy Alternatives", Word.cs:87). The
    /// plain `#[derive(Clone)]` copies every field, so this is used at the two boundary sites where
    /// C# starts a fresh candidate whose alternatives must NOT ride along — the per-stratum analysis
    /// seed (`AnalysisStratumRule.cs:106`) and the lexical-lookup clone
    /// (`Morpher.LexicalLookup`/`Word.SetRootAllomorph`) — so a canonical's stashed alternatives are
    /// never expanded twice. `source` **is** carried (C# repoints it, we set it explicitly at those
    /// sites right after).
    pub fn clone_without_alternatives(&self) -> Word {
        let mut c = self.clone();
        c.alternatives.clear();
        c
    }

    /// Folds `w` into this canonical word as an alternative.
    pub fn fold_alternative(&mut self, w: Word) {
        let alternative = Alternative::of(w, self);
        self.alternatives.push(alternative);
    }

    /// C# `Word.MorphologicalRuleUnapplied` (Word.cs:317-327): `mrule` is `None` for the unknown compounding rule, reachable only from generation seeding a bare non-head; a realizational rule is counted but never occupies a slot (cs:321).
    pub fn morphological_rule_unapplied(&mut self, is_realizational: bool, mrule: Option<MRuleId>) {
        match (is_realizational, mrule) {
            (false, mrule) => {
                self.trail.push(mrule, None);
                self.mrule_app_index = self.trail.len() as i32 - 1;
            }
            (true, Some(id)) => self.trail.push_count_only(id),
            (true, None) => {}
        }
    }

    /// The analysis-selected allomorph for the rule currently being confirmed.
    pub fn current_recorded_allomorph(&self) -> Option<AllomorphId> {
        let Ok(i) = usize::try_from(self.mrule_app_index) else {
            return None;
        };
        self.trail
            .allomorph(i)
            .expect("pending morphological rule has no allomorph trail slot")
    }

    /// C# `Word.NonHeadUnapplied` (Word.cs:395-399): push a compounding non-head candidate and
    /// advance `Word::non_head_app_index` to point at it. Generation-only counterpart to the
    /// stratum orchestrator's own non-head push inside `morph::ana_compound` (which builds the
    /// non-head from the analysis split, not from a caller-supplied `Word`).
    pub fn non_head_unapplied(&mut self, non_head: Word) {
        self.non_heads_mut().push(non_head);
        self.non_head_app_index = self.non_heads().len() as i32 - 1;
    }

    /// The canonical dedup key (see `WordKey`). Cloned per lookup; no per-parse interning is done.
    pub fn dedup_key(&self) -> WordKey {
        WordKey::of(&KeyFields::of(self))
    }

    /// The current non-head — a faithful, **index-based** port of C# `Word.CurrentNonHead`
    /// (Word.cs:453-461: `_nonHeadAppIndex == -1 ? null : _nonHeadApps[_nonHeadAppIndex]`), not
    /// `non_heads.last()`. These only coincide when `non_head_app_index` always equals
    /// `non_heads.len() - 1`, which holds while the trail only ever grows (fresh analysis splits,
    /// `Word::non_head_unapplied`) but stops holding once a compounding rule confirms and
    /// `guided_synth` decrements `non_head_app_index` **without** removing anything from
    /// `non_heads` (see `pg-rules/src/morph.rs`'s `synth_compound_subrule` doc for
    /// why `non_heads` is deliberately never popped). With two or more non-heads on the trail
    /// (reachable today via `pg_parse::Morpher::generate_words`'s direct API, which pushes one
    /// non-head per `GenMorpheme::NonHead` with no `max_stem_count` gate — that gate is analysis-
    /// side only, `pg-rules/src/stratum.rs`'s `apply_one_mrule`, and generation never goes through
    /// analysis), after the first compounding rule confirms, `.last()` would incorrectly re-return
    /// the same (already-consumed) non-head instead of the next one down the index — this method
    /// must read by index to match C#.
    pub fn current_non_head(&self) -> Option<&Word> {
        if self.non_head_app_index < 0 {
            return None;
        }
        self.non_heads().get(self.non_head_app_index as usize)
    }

    /// Reconstruct the full set of analysis candidates that `MergeEquivalentAnalyses` folded into
    /// this word — a faithful port of C# `Word.ExpandAlternatives` (Word.cs:491-533).
    ///
    /// The per-stratum merge (`AnalysisStratumRule.Apply`) keeps only one word per
    /// `AnalysisStateKey` (or `WordKey`-fallback match) flowing into deeper strata,
    /// stashing the folded repeats in `Word::alternatives` and every word's stratum-input in
    /// `Word::source`. This walks that `source` spine: it expands the
    /// source first, and — whenever the source itself expanded to two or more words (i.e. a merge
    /// happened upstream) — replays the delta this word accumulated since its source (the extra
    /// trail slots, the extra `non_heads`, and, at the synthesis boundary, the root allomorph) onto
    /// each of those expanded originals. The word's own `alternatives` are each expanded and appended.
    /// The net effect is that a candidate merged away at a shallow stratum is recovered with the
    /// deeper strata's rules grafted on, so synthesis sees exactly the candidate set the unmerged
    /// engine would have — this is what makes the merge a de-duplication rather than a loss.
    ///
    /// Deviations from C#, documented here rather than silently dropped:
    /// - the realizational-FS diff (Word.cs:515-524) is ported — see the `real_fs` block
    ///   below — but C#'s `Unify(diff, out newFS)` call **discards its own success flag**, so a
    ///   diff that fails to unify with `alt.real_fs` sets `alternative._realizationalFS` to a C#
    ///   `null` (`FeatureStruct.Unify`'s failure branch, FeatureStruct.cs:1043-1047) with no
    ///   downstream guard — a latent C# null-crash on whatever consumes it next. This port instead
    ///   leaves `alt.real_fs` unchanged on that failure (`Option::None` from `unify`), which is
    ///   strictly safer than reproducing a crash and is never exercised by any oracle-verified
    ///   fixture (a real family's shape-equivalent alternatives never carry genuinely conflicting
    ///   realizational feature diffs — flagged here rather than silently assumed);
    /// - `alternative.RootAllomorph = RootAllomorph` (Word.cs:525-526) is realized by copying the
    ///   root-derived fields straight off `self` (which already holds them, having had the identical
    ///   root applied to the identical shape) instead of re-running the lexicon, so this stays a pure
    ///   `Word` method with no grammar handle.
    pub fn expand_alternatives(&self) -> Vec<Word> {
        self.expand_alternatives_impl(None)
    }

    pub fn expand_alternatives_observed(
        &self,
        recorder: &dyn crate::observations::Recorder,
    ) -> Vec<Word> {
        self.expand_alternatives_impl(Some(recorder))
    }

    fn expand_alternatives_impl(
        &self,
        recorder: Option<&dyn crate::observations::Recorder>,
    ) -> Vec<Word> {
        if !pg_fst::work::consume() {
            return Vec::new();
        }
        let mut out: Vec<Word> = Vec::new();
        let originals: Option<Vec<Word>> = self
            .source
            .as_ref()
            .map(|s| s.expand_alternatives_impl(recorder));
        match &originals {
            // The general case: the source fanned out to >= 2 originals, so this word's delta must be replayed onto each of them.
            Some(o) if o.len() >= 2 => {
                let src = self
                    .source
                    .as_ref()
                    .expect("originals is Some => source is Some");
                let mut delta: Vec<_> = self
                    .trail
                    .slots_rev()
                    .take(self.trail.len().saturating_sub(src.trail.len()))
                    .collect();
                delta.reverse();
                for original in o {
                    if !pg_fst::work::consume() {
                        return Vec::new();
                    }
                    let mut alt = original.clone();
                    alt.shape = self.shape.clone();
                    // Rules unapplied since the source (cs:509-511 `MorphologicalRuleUnapplied`).
                    for &(rule, allomorph) in &delta {
                        alt.trail.push(rule, allomorph);
                    }
                    alt.mrule_app_index = alt.trail.len() as i32 - 1;
                    // Non-heads unapplied since the source (cs:512-514 `NonHeadUnapplied`).
                    for non_head in self.non_heads().iter().skip(src.non_heads().len()) {
                        alt.non_heads_mut().push(non_head.clone());
                    }
                    alt.non_head_app_index = alt.non_heads().len() as i32 - 1;
                    // Realizational-FS diff -- see the doc comment for the one documented divergence (C#'s discarded-success-flag null path).
                    if self.real_fs != src.real_fs {
                        let diff = pg_featstruct::subtract(&self.real_fs, &src.real_fs);
                        if let Some(new_fs) = pg_featstruct::unify(&alt.real_fs, &diff) {
                            alt.real_fs = new_fs;
                        }
                    }
                    // Root allomorph change (cs:525-526) — see the doc comment.
                    if self.root_allomorph != src.root_allomorph {
                        alt.shape = self.shape.clone();
                        alt.stratum = self.stratum;
                        alt.syn_fs = self.syn_fs.clone();
                        alt.mpr = self.mpr.clone();
                        alt.morphs = self.morphs.clone();
                        alt.flags.set_is_partial(self.flags.is_partial());
                        alt.root_allomorph = self.root_allomorph;
                        alt.set_root_runtime_id(self.root_runtime_id().map(str::to_owned));
                        // The guessed-root payload rides along with root_allomorph: both change together at the exact same site, never observable in isolation.
                    }
                    if let Some(recorder) = recorder {
                        alt.trace = recorder.advance(original.trace);
                    }
                    out.push(alt);
                }
            }
            // Special case (cs:499-503): no source, or a single-word source — this word stands alone.
            _ => out.push(self.clone()),
        }
        // Local alternatives (cs:530-531): every word folded into this one by the merge expands too.
        for alt in self.alternatives.iter() {
            out.extend(alt.word(self).expand_alternatives_impl(recorder));
            if pg_fst::work::stopped() {
                return Vec::new();
            }
        }
        if pg_fst::work::stopped() {
            Vec::new()
        } else {
            out
        }
    }

    pub fn root_runtime(&self) -> Option<&RuntimeRoot> {
        self.morphs
            .iter()
            .find(|m| {
                Some(m.allomorph) == self.root_allomorph
                    && runtime_id(m.runtime_root.as_deref()) == self.root_runtime_id()
            })
            .and_then(|m| m.runtime_root.as_deref())
    }

    /// Morpheme ids in morph (surface) order — the sequence the batch signature joins with `+`.
    /// Deduped by allomorph and runtime realization in first-occurrence order. The runtime
    /// identity is load-bearing because supplied roots share a sentinel allomorph while still
    /// representing distinct morphemes. A discontinuous morph also splits into one `MorphRecord`
    /// per contiguous run, all sharing both identities.
    pub fn morpheme_sequence(&self) -> Vec<MorphemeId> {
        let mut ms = self.morphs.clone();
        ms.sort_by_key(|m| m.order);
        let mut seen: Vec<(AllomorphId, Option<String>)> = Vec::new();
        ms.into_iter()
            .filter(|m| {
                let key = (
                    m.allomorph,
                    runtime_id(m.runtime_root.as_deref()).map(str::to_owned),
                );
                if seen.contains(&key) {
                    false
                } else {
                    seen.push(key);
                    true
                }
            })
            .map(|m| m.morpheme)
            .collect()
    }
}

/// Approximate heap-byte cost of one `Shape` (per-node overhead plus `feat_width` `u64` lanes); a rough estimate is sufficient since this only sizes the memo byte budget, so no allocator introspection.
fn estimate_shape_bytes(shape: &Shape) -> usize {
    const PER_NODE_FIXED: usize = 16; // kind + char_def(u32) + flags + cd_set, rounded up
    let n = shape.len();
    n * PER_NODE_FIXED + n * shape.feat_width() as usize * std::mem::size_of::<u64>()
}

/// Approximate heap-byte cost of one `FeatureStruct`, recursing into `Complex` values.
fn estimate_fs_bytes(fs: &FeatureStruct) -> usize {
    const ENTRY_FIXED: usize = 24; // FeatId + enum discriminant + Vec slot overhead, rounded up
    fs.entries()
        .iter()
        .map(|(_, v)| {
            ENTRY_FIXED
                + match v {
                    pg_featstruct::FeatureValue::Symbolic(_) => {
                        std::mem::size_of::<pg_featstruct::SymbolBits>()
                    }
                    pg_featstruct::FeatureValue::Complex(inner) => estimate_fs_bytes(inner),
                }
        })
        .sum()
}

/// Per-field decomposition of `estimate_word_bytes`'s total, for the `HC_WORD_STATS=1` diagnostic
/// (`crate::word_stats`) — see `docs/research/word-memory-trace.md` for what each field's measured
/// share turned out to be. `non_heads`/`alternatives` are each the *recursive total* of the nested
/// `Word`s they hold, not a per-node count, matching `estimate_word_bytes`'s own recursion.
#[derive(Clone, Copy, Debug, Default)]
pub struct WordByteBreakdown {
    /// `size_of::<Word>()` — the struct's own inline fields (pointers, small enums, indices).
    pub base: usize,
    pub shape: usize,
    pub syn_fs: usize,
    pub real_fs: usize,
    /// Wide words and supplied-root MPR field growth, charged per retained owner.
    pub mpr: usize,
    pub morphs: usize,
    pub mrule_apps: usize,
    pub obligatory: usize,
    pub unapplied_rule_counts: usize,
    pub root_runtime_id: usize,
    /// Recursive total over every `Word` in `non_heads`.
    pub non_heads: usize,
    /// Recursive total over every `Rc<Word>` in `alternatives` — omitted by `estimate_word_bytes`
    /// before this diagnostic (`docs/research/word-memory-trace.md` §2); alternatives are
    /// "empty on almost every word" (`Word::alternatives`'s doc) but a full merged-candidate
    /// subtree on every one that is not, so a single pathological entry can dominate.
    pub alternatives: usize,
}

impl WordByteBreakdown {
    pub fn total(&self) -> usize {
        self.base
            + self.shape
            + self.syn_fs
            + self.real_fs
            + self.mpr
            + self.morphs
            + self.mrule_apps
            + self.obligatory
            + self.unapplied_rule_counts
            + self.root_runtime_id
            + self.non_heads
            + self.alternatives
    }

    /// Component-wise accumulate `other` into `self`, for summing a breakdown over many `Word`s.
    pub fn add_assign(&mut self, other: &WordByteBreakdown) {
        self.base += other.base;
        self.shape += other.shape;
        self.syn_fs += other.syn_fs;
        self.real_fs += other.real_fs;
        self.mpr += other.mpr;
        self.morphs += other.morphs;
        self.mrule_apps += other.mrule_apps;
        self.obligatory += other.obligatory;
        self.unapplied_rule_counts += other.unapplied_rule_counts;
        self.root_runtime_id += other.root_runtime_id;
        self.non_heads += other.non_heads;
        self.alternatives += other.alternatives;
    }
}

/// Approximate heap-byte cost of one `Word`, broken down by field. Recurses into `non_heads` (the
/// nested-`Word` growth the memo byte budget exists to bound) and into `alternatives` (the
/// stratum-merge's folded-candidate list — see `WordByteBreakdown::alternatives`'s doc for why
/// this recursion was added). `source`'s `Rc<Word>` chain is deliberately **not** recursed into:
/// it is shared across many stratum outputs (see the field's doc), so counting its pointee per
/// clone would wildly overstate distinct memory: only the pointer's own bytes are counted here,
/// same as any other `Option<Rc<_>>` field.
///
/// `alternatives` is now `Vec<Rc<Word>>` too, but is still recursed into (unlike `source`) because
/// the per-entry budget this feeds wants each owner's full attributable cost. That does mean two
/// owners that both hold a clone of the same canonical `Word` — the memo store and the live
/// frontier, most commonly, since a `Vec<Word>` clone now clones `Rc` pointers instead of the
/// subtree — will each count that shared subtree in full: a real double-count of one allocation
/// across two totals, not merely a hypothetical one. It biases the byte budget conservative
/// (evicts sooner than the true retained set requires) rather than under; it is not a live-memory
/// bound (see the work-cap contract).
pub fn estimate_word_bytes_breakdown(w: &Word) -> WordByteBreakdown {
    let mut b = WordByteBreakdown {
        // Fixed at the size `Word` had when the capture budgets were calibrated, so shrinking the struct does not move any estimate.
        base: LEGACY_WORD_BYTES - std::mem::size_of::<Shape>()
            + pg_shape::INLINE_COLUMNS_BYTES
            // `Word` holds two inline feature structures (`syn_fs`, `real_fs`); each keeps its old 24-byte weight.
            + 2 * (pg_featstruct::FEATURE_STRUCT_INLINE_BYTES
                - std::mem::size_of::<FeatureStruct>())
            + std::mem::size_of::<MprSet>()
            - std::mem::size_of::<u64>(),
        shape: estimate_shape_bytes(&w.shape),
        syn_fs: estimate_fs_bytes(&w.syn_fs),
        real_fs: estimate_fs_bytes(&w.real_fs),
        mpr: mpr_extra_bytes(w),
        morphs: w.morphs.len() * std::mem::size_of::<MorphRecord>(),
        mrule_apps: w.trail.len() * std::mem::size_of::<Option<MRuleId>>()
            + w.trail.len() * std::mem::size_of::<Option<AllomorphId>>()
            + std::mem::size_of_val(w.applied_realizational_rules()),
        obligatory: w.obligatory.len() * std::mem::size_of::<FeatId>(),
        unapplied_rule_counts: w.trail.rule_counts().len()
            * (std::mem::size_of::<MRuleId>() + std::mem::size_of::<u32>()),
        root_runtime_id: w.root_runtime_id().map_or(0, str::len),
        non_heads: 0,
        alternatives: 0,
    };
    for nh in w.non_heads() {
        b.non_heads += estimate_word_bytes_breakdown(nh).total();
    }
    for alt in w.alternatives.iter() {
        b.alternatives += alt.estimated_bytes();
    }
    b
}

/// Charges every retained supplied payload, including non-head roots flattened into morph records.
fn mpr_extra_bytes(w: &Word) -> usize {
    w.mpr.heap_bytes()
        + w.morphs
            .iter()
            .map(|m| match m.runtime_root.as_deref() {
                Some(RuntimeRoot::Supplied(root)) => {
                    std::mem::size_of::<MprSet>() - std::mem::size_of::<u64>()
                        + root.mpr.heap_bytes()
                }
                _ => 0,
            })
            .sum::<usize>()
}

/// Approximate heap-byte cost of one `Word` (see `estimate_word_bytes_breakdown` for the
/// per-field decomposition this sums).
pub fn estimate_word_bytes(w: &Word) -> usize {
    estimate_word_bytes_breakdown(w).total()
}

/// Approximate heap-byte cost of a `MemoEntry`'s `results`, the payload the memo byte budget accounts against.
pub fn estimate_words_bytes(words: &[Word]) -> usize {
    words.iter().map(estimate_word_bytes).sum()
}

/// Platform-independent logical charge for a retained search word and its indexes.
/// Fixed weights account for spare collection capacity; the trail slab is charged separately.
pub(crate) fn retained_word_bytes(w: &Word) -> usize {
    512usize
        .saturating_add(mpr_extra_bytes(w))
        .saturating_add(w.shape.len().saturating_mul(8))
        .saturating_add(w.morphs.len().saturating_mul(64))
        .saturating_add(w.obligatory.len().saturating_mul(16))
}

#[cfg(test)]
mod tests;
