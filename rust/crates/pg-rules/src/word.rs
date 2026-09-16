//! The in-flight parse state for the morphological-rule engine.
//!
//! This is the `Word` the affix-process and compounding primitives (un)apply rules to. It is
//! deliberately **owned** (no arena lifetime, no persistent trail, no `rule_counts`).
//!
//! `syn_fs`/`real_fs` are owned `FeatureStruct`s, not `FsId`s: rule application mints new
//! feature-structure values (`unify` then `priority_union` on synthesis; `Add`/`Clear` on
//! analysis), and those values cannot be interned into the immutable grammar interner
//! (`grammar.fs_interner`, a frozen contract). Grammar-tier requirement/output FSs stay `FsId` and
//! are resolved through `grammar.fs_interner` at use. `mpr: MprSet` and `obligatory: Vec<FeatId>`
//! are also owned here, since MPR gating/accumulation and obligatory-feature accumulation both
//! read/write the word's own state as rules apply.

use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use pg_featstruct::{FeatId, FeatureStruct};
use pg_grammar::model::{AllomorphId, LexEntryId, MRuleId, MorphemeId, MprSet, StratumId};
use pg_shape::Shape;

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
    /// C# `Word.IsPartial`.
    pub is_partial: bool,
    /// C# `Word.IsLastAppliedRuleFinal` (`bool?`): `None` = no template applied yet.
    pub is_last_applied_rule_final: Option<bool>,
    /// Whether the latest successful rule application was an ordinary non-template rule.
    pub final_template_state: FinalTemplateState,
}

/// One applied allomorph, recorded in **morph order** (the batch signature is
/// `join("+", morpheme.Id)` over the word's morphs in surface order).
///
/// `order` is the leftmost interior node position (0-based, anchors excluded) of the morph's
/// material in *this* word's shape at the time it was recorded; morph order = ascending `order`.
/// Recomputed on every (un)application because the shape is rebuilt each time (mirroring C#'s
/// annotation-parent morph records, which move with their nodes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MorphRecord {
    pub allomorph: AllomorphId,
    pub morpheme: MorphemeId,
    pub order: u32,
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
    /// How this record is anchored — see `MorphStatus` and `pg_rules::morph::attribute_morphs`'s
    /// doc comment for the C# mechanism each variant ports. `Real` for root records.
    pub status: MorphStatus,
    /// Runtime root payload attached to this morph (never word-wide), for supplied/guessed roots.
    pub runtime_root: Option<Rc<RuntimeRoot>>,
}

/// How a `MorphRecord` is anchored to the word's shape. Only `MorphStatus::Real`
/// records own output nodes (`pg_rules::morph`'s `owning_morph` skips the rest); the other three
/// variants port the C# annotation-tree states a morph can be in after
/// `SynthesisAffixProcessAllomorphRuleSpec.ApplyRhs`'s fallback branches (cs:162-207):
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum MorphStatus {
    /// Owns the output nodes starting at `order` (a normal, positioned morph annotation).
    Real,
    /// A pure-truncation rule's fallback marker, not yet resolved onto real material (`order` is the `FLOATING_ORDER` sentinel while in this state).
    Floating,
    /// An input morph whose material was entirely dropped by a rule that inserted new material: attached as a child of the new morph's annotation, rendering before the host.
    SubsumedChild,
    /// An input morph whose material was entirely dropped by a rule with no new material: re-marked on the output's first node, rendering after the containing morph.
    SubsumedFirst,
}

impl MorphRecord {
    /// A record with no passed-over set (root records and hand-built test records).
    pub fn new(allomorph: AllomorphId, morpheme: MorphemeId, order: u32) -> Self {
        MorphRecord {
            allomorph,
            morpheme,
            order,
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
    /// Compounding non-head children (C# `Word._nonHeadApps`; `CurrentNonHead` = the element at
    /// `Word::non_head_app_index`).
    pub non_heads: Vec<Word>,
    /// C# `Word._nonHeadAppIndex` (init `-1` = none). During analysis this only ever grows
    /// (`NonHeadUnapplied` pushes + increments; nothing pops), so it invariably equals
    /// `non_heads.len() as i32 - 1`. It is a **dedup-key** component (Word.cs:520,540), not a
    /// cascade driver — the cascades track their own rule position via `AnalysisStratumRule`.
    pub non_head_app_index: i32,
    /// The morphological rules unapplied so far, in unapplication order (C# `Word._mruleApps`;
    /// `MorphologicalRuleUnapplied` appends). Analysis always records the *known* `Some(`[`MRuleId`]`)`
    /// for every (un)application (affix + compounding). `None` is C#'s `null` "unknown compounding
    /// rule" entry (Word.cs:317-327's doc: "used when generating a compound word, because the
    /// compounding rule is usually not known, just the non-head allomorph") — it never arises from
    /// analysis, only from `pg_parse`'s `generate_words` seeding a bare `LexEntry` non-head
    /// directly into the unapplication trail; the synthesis confirmation gate
    /// (`pg_rules::stratum::guided_synth`) matches a `None` slot against **any** `CompoundingRule`,
    /// exactly as C#'s `IsMorphologicalRuleApplicable`'s `curRule == null && rule is CompoundingRule`
    /// clause (Word.cs:237-244) does. A dedup-key component (Word.cs:523,543): two words with the
    /// same shape but different unapplication histories are **not** equal — which is also why the
    /// cascade self-loop guard (`key(in) != key(out)`) is always true here (every unapplication grows
    /// this list), so termination rests on rules ceasing to apply or the step cap, never on key
    /// equality.
    pub mrule_apps: Vec<Option<MRuleId>>,
    /// C# `Word._mruleAppIndex` (init `-1`). Grows in lock-step with `mrule_apps` during analysis,
    /// so it always equals `mrule_apps.len() as i32 - 1`. A dedup-key component
    /// (Word.cs:524,544), not a cascade driver.
    pub mrule_app_index: i32,
    /// The matched root allomorph (C# `Word._rootAllomorph`). Set by lexical lookup; stays
    /// `None` throughout analysis (the surface word starts from `Word(Stratum, Shape)`, which
    /// leaves it null). A dedup-key component (Word.cs:522,542).
    pub root_allomorph: Option<AllomorphId>,
    /// Stable identity of a runtime-backed head root; payload remains on its MorphRecord.
    pub root_runtime_id: Option<String>,
    /// Obligatory syntactic features accumulated by applied rules (C#
    /// `Word.ObligatorySyntacticFeatures`).
    pub obligatory: Vec<FeatId>,
    /// Per-rule **unapplication** count multiset (C# `Word._mrulesUnapplied` /
    /// `Word.UnappliedRuleCounts`, Word.cs:376-406) — how many times each morphological rule has been
    /// unapplied on this word. Incremented alongside every `mrule_apps.push`. Its sole consumer is
    /// `pg_memo::AnalysisStateKey`, which needs an
    /// order-independent view of the trail; it is **not** part of `WordKey` (C# `ValueEquals`
    /// ignores it), so adding it does not perturb dedup. A `BTreeMap` for the same canonical-order
    /// reason the key uses one. Unmodified by `Word::replay_onto` — see that method.
    pub unapplied_rule_counts: BTreeMap<MRuleId, u32>,
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
    /// `pg_memo::AnalysisStateKey`, or an equal `WordKey` differing only in syntactic FS, does not
    /// enter the output set; instead `canonicalWord.Alternatives.Add(mruleOutWord)`). They differ
    /// from the canonical in rule/non-head history and (widened into the canonical via
    /// `pg_featstruct::union` on the fold — see `crate::stratum`'s merge) syntactic FS, and are
    /// re-expanded at synthesis by `Word::expand_alternatives`. Empty on almost every word. **Not**
    /// part of `WordKey`; **not** copied by `Word::clone_without_alternatives` (C#'s copy ctor
    /// leaves `_alternatives` fresh-empty, Word.cs:87), so only the two sites that build a canonical
    /// (the stratum merge) ever populate it.
    ///
    /// `Rc` (not owned `Word`, and not `Arc`): the canonical this field lives on is cloned heavily
    /// once merged (the memo store at `stratum.rs:1135-1155`/`:1311-1318`, `replay_onto`, dedup),
    /// and each clone used to deep-copy this whole subtree even though the payload is immutable
    /// from the moment the fold attaches it — the same rationale as `Word::source`. Plain `Rc`
    /// suffices: `Word` is already `!Send` via `source`, and `pg-parse/src/batch.rs` parallelizes
    /// across words, not within one word's single-threaded cascade.
    pub alternatives: Vec<Rc<Word>>,
    /// Mirrors C# `Word.CurrentTrace` (`object`), the live cursor a `TraceManager`
    /// call reassigns as the parse progresses (`Trace.cs`/`TraceManager.cs`; see
    /// `crate::trace`'s module doc for why this port carries the cursor as an explicit
    /// `crate::trace::TraceHandle` value alongside the word instead of a mutated field the sink
    /// owns). `None` when tracing is off (the overwhelming common case — adds one `Option<u32>`-sized
    /// field, no allocation) or on any `Word` never touched by a traced call. Threaded through every
    /// existing clone point (the derived `Clone`, `Word::clone_without_alternatives`,
    /// `Word::replay_onto`) exactly as C#'s `CurrentTrace` rides along `Word.Clone()` (Word.cs:110)
    /// — **not** part of `WordKey` (C# `ValueEquals`/`FreezeImpl` never reads `CurrentTrace` either).
    pub trace: Option<crate::trace::TraceHandle>,
}

/// Canonical dedup key for `Word`, retaining the source-bearing morph trail as well as the engine
/// state. The cascades and the stratum orchestrator dedup on this key; `pg_memo::AnalysisStateKey`
/// remains the separate rule-state memo key.
///
/// The state fields follow C# `Word.ValueEquals` (Word.cs:537-545), while `morphs` is retained so
/// two candidates with the same rendered shape but different selected source allomorphs, MSA,
/// inflection type, or annotation order cannot disappear before structured projection.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WordKey {
    shape: Shape,
    real_fs: FeatureStruct,
    morphs: Vec<MorphKey>,
    non_heads: Vec<WordKey>,
    non_head_app_index: i32,
    stratum: StratumId,
    root_allomorph: Option<AllomorphId>,
    root_realization: Option<String>,
    mrule_apps: Vec<Option<MRuleId>>,
    mrule_app_index: i32,
    is_last_applied_rule_final: Option<bool>,
    final_template_state: FinalTemplateState,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct MorphKey {
    allomorph: AllomorphId,
    morpheme: MorphemeId,
    order: u32,
    status: MorphStatus,
    runtime_identity: Option<String>,
}

impl Word {
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
            non_heads: Vec::new(),
            non_head_app_index: -1,
            mrule_apps: Vec::new(),
            mrule_app_index: -1,
            root_allomorph: None,
            root_runtime_id: None,
            obligatory: Vec::new(),
            unapplied_rule_counts: BTreeMap::new(),
            flags: WordFlags::default(),
            source: None,
            alternatives: Vec::new(),
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

    /// Record one morphological-rule unapplication in the count multiset (C#
    /// `Word.MorphologicalRuleUnapplied`'s `_mrulesUnapplied.UpdateValue`, Word.cs:376-379). Called
    /// from the stratum orchestrator's per-unapplication closure, paired with `mrule_apps.push(id)`.
    pub fn record_unapplication(&mut self, id: MRuleId) {
        *self.unapplied_rule_counts.entry(id).or_insert(0) += 1;
    }

    /// C# `Word.MorphologicalRuleUnapplied` (Word.cs:317-327), the direct-generation counterpart to
    /// the stratum orchestrator's inline bookkeeping (`apply_one_mrule`, which only ever pushes a
    /// known analysis-confirmed `Some(id)`): `mrule` is `None` for C#'s "unknown compounding rule"
    /// null (only reachable from `pg_parse`'s `generate_words`, never from analysis). If `mrule` is
    /// known, its unapplication count is recorded first regardless of rule kind (cs:319-320); then,
    /// unless `is_realizational` (a `RealizationalAffixProcessRule` never occupies an `mrule_apps`
    /// slot — cs:321 — because its confirmation gate is the realizational-FS subsumption check in
    /// `pg_rules::morph::synth_realizational`, not the ordinary trail), the slot is pushed and the
    /// index advanced.
    pub fn morphological_rule_unapplied(&mut self, is_realizational: bool, mrule: Option<MRuleId>) {
        if let Some(id) = mrule {
            self.record_unapplication(id);
        }
        if !is_realizational {
            self.mrule_apps.push(mrule);
            self.mrule_app_index = self.mrule_apps.len() as i32 - 1;
        }
    }

    /// C# `Word.NonHeadUnapplied` (Word.cs:395-399): push a compounding non-head candidate and
    /// advance `Word::non_head_app_index` to point at it. Generation-only counterpart to the
    /// stratum orchestrator's own non-head push inside `morph::ana_compound` (which builds the
    /// non-head from the analysis split, not from a caller-supplied `Word`).
    pub fn non_head_unapplied(&mut self, non_head: Word) {
        self.non_heads.push(non_head);
        self.non_head_app_index = self.non_heads.len() as i32 - 1;
    }

    /// Re-parent a word computed while exploring the analysis-cascade subtree below some node `N`
    /// onto `query` — a different word that reached the same `pg_memo::AnalysisStateKey` as `N` via
    /// a different rule-unapplication *order* (the #451 positive memo; C# `Word.ReplayOnto`,
    /// Word.cs:554-581).
    ///
    /// Everything computed strictly *within* the subtree (deeper shape/FS edits, and any rules or
    /// non-heads unapplied below `N`) is a deterministic function of `N`'s content alone — analysis
    /// rules read shape, syntactic FS, rule multiset, non-head count, and source morph history, all
    /// equal between `N` and `query` by definition of an equal key — so it is kept as-is from `self`. Only the two
    /// *ordered* structures the key summarizes as counts/multisets — the mrule trail (`mrule_apps`)
    /// and the non-head list (`non_heads`) — have their **prefix** (whatever accumulated before
    /// reaching `N`) replaced with `query`'s own prefix. `mrule_trail_prefix_length` /
    /// `non_head_prefix_length` are `N`'s lengths at store time; everything from those indices on in
    /// `self` is the subtree-local suffix to keep.
    ///
    /// `unapplied_rule_counts` is intentionally **not** rebuilt: an equal key guarantees `query` and
    /// `N` have identical per-rule prefix counts, so `self`'s multiset (= `N`-prefix + suffix) already
    /// equals the correct (`query`-prefix + suffix) multiset. The prefix lengths are likewise equal
    /// (equal multiset ⇒ equal total count), so the graft is length-consistent.
    pub fn replay_onto(
        &self,
        query: &Word,
        mrule_trail_prefix_length: usize,
        non_head_prefix_length: usize,
    ) -> Word {
        let mut clone = self.clone();

        // Graft the mrule trail: query's prefix + self's subtree-local suffix (Word.cs:561-568).
        let mrule_suffix = clone.mrule_apps.split_off(mrule_trail_prefix_length);
        clone.mrule_apps.clear();
        clone.mrule_apps.extend_from_slice(&query.mrule_apps);
        clone.mrule_apps.extend(mrule_suffix);
        clone.mrule_app_index = clone.mrule_apps.len() as i32 - 1;

        // Graft the non-head list the same way (Word.cs:570-577).
        let non_head_suffix = clone.non_heads.split_off(non_head_prefix_length);
        clone.non_heads.clear();
        clone.non_heads.extend_from_slice(&query.non_heads);
        clone.non_heads.extend(non_head_suffix);
        clone.non_head_app_index = clone.non_heads.len() as i32 - 1;

        clone
    }

    /// The canonical dedup key (see `WordKey`). Cloned per lookup; no per-parse interning is done.
    pub fn dedup_key(&self) -> WordKey {
        WordKey {
            shape: self.shape.clone(),
            real_fs: self.real_fs.clone(),
            morphs: self
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
            non_heads: self.non_heads.iter().map(Word::dedup_key).collect(),
            non_head_app_index: self.non_head_app_index,
            stratum: self.stratum,
            root_allomorph: self.root_allomorph,
            root_realization: self.root_runtime_id.clone(),
            mrule_apps: self.mrule_apps.clone(),
            mrule_app_index: self.mrule_app_index,
            is_last_applied_rule_final: self.flags.is_last_applied_rule_final,
            final_template_state: self.flags.final_template_state,
        }
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
        self.non_heads.get(self.non_head_app_index as usize)
    }

    /// Reconstruct the full set of analysis candidates that `MergeEquivalentAnalyses` folded into
    /// this word — a faithful port of C# `Word.ExpandAlternatives` (Word.cs:491-533).
    ///
    /// The per-stratum merge (`AnalysisStratumRule.Apply`) keeps only one word per
    /// `pg_memo::AnalysisStateKey` (or `WordKey`-fallback match) flowing into deeper strata,
    /// stashing the folded repeats in `Word::alternatives` and every word's stratum-input in
    /// `Word::source`. This walks that `source` spine: it expands the
    /// source first, and — whenever the source itself expanded to two or more words (i.e. a merge
    /// happened upstream) — replays the delta this word accumulated since its source (the extra
    /// `mrule_apps`, the extra `non_heads`, and, at the synthesis boundary, the root allomorph) onto
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
        let mut out: Vec<Word> = Vec::new();
        let originals: Option<Vec<Word>> = self.source.as_ref().map(|s| s.expand_alternatives());
        match &originals {
            // The general case: the source fanned out to >= 2 originals, so this word's delta must be replayed onto each of them.
            Some(o) if o.len() >= 2 => {
                let src = self
                    .source
                    .as_ref()
                    .expect("originals is Some => source is Some");
                for original in o {
                    let mut alt = original.clone();
                    alt.shape = self.shape.clone();
                    // Rules unapplied since the source (cs:509-511 `MorphologicalRuleUnapplied`).
                    for i in src.mrule_apps.len()..self.mrule_apps.len() {
                        let id = self.mrule_apps[i];
                        alt.mrule_apps.push(id);
                        // C#'s `mrule != null` guard; never actually `None` on this path, kept for type-level faithfulness.
                        if let Some(id) = id {
                            *alt.unapplied_rule_counts.entry(id).or_insert(0) += 1;
                        }
                    }
                    alt.mrule_app_index = alt.mrule_apps.len() as i32 - 1;
                    // Non-heads unapplied since the source (cs:512-514 `NonHeadUnapplied`).
                    for i in src.non_heads.len()..self.non_heads.len() {
                        alt.non_heads.push(self.non_heads[i].clone());
                    }
                    alt.non_head_app_index = alt.non_heads.len() as i32 - 1;
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
                        alt.mpr = self.mpr;
                        alt.morphs = self.morphs.clone();
                        alt.flags.is_partial = self.flags.is_partial;
                        alt.root_allomorph = self.root_allomorph;
                        alt.root_runtime_id = self.root_runtime_id.clone();
                        // The guessed-root payload rides along with root_allomorph: both change together at the exact same site, never observable in isolation.
                    }
                    out.push(alt);
                }
            }
            // Special case (cs:499-503): no source, or a single-word source — this word stands alone.
            _ => out.push(self.clone()),
        }
        // Local alternatives (cs:530-531): every word folded into this one by the merge expands too.
        for alt in &self.alternatives {
            out.extend(alt.expand_alternatives());
        }
        out
    }

    pub fn root_runtime(&self) -> Option<&RuntimeRoot> {
        self.morphs
            .iter()
            .find(|m| {
                Some(m.allomorph) == self.root_allomorph
                    && runtime_id(m.runtime_root.as_deref()) == self.root_runtime_id.as_deref()
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
pub(crate) fn estimate_shape_bytes(shape: &Shape) -> usize {
    const PER_NODE_FIXED: usize = 16; // kind + char_def(u32) + flags + cd_set, rounded up
    let n = shape.len();
    n * PER_NODE_FIXED + n * shape.feat_width() as usize * std::mem::size_of::<u64>()
}

/// Approximate heap-byte cost of one `FeatureStruct`, recursing into `Complex` values.
pub(crate) fn estimate_fs_bytes(fs: &FeatureStruct) -> usize {
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
/// stratum-merge's folded-candidate list — see `WordByteBreakdown::alternatives`'s doc). `source`'s
/// `Rc<Word>` chain is deliberately **not** recursed into: it is shared across many stratum outputs
/// (see the field's doc), so counting its pointee per clone would wildly overstate distinct memory —
/// only the pointer's own bytes are counted here, same as any other `Option<Rc<_>>` field.
///
/// Dedup scope: a single top-level `Word`'s own subtree, keyed on `Rc::as_ptr`, so an `alternatives`
/// entry reachable twice from *this one* `w` (e.g. via two different `non_heads`) is charged once.
/// It does **not** dedup against any other `Word` passed to a separate call — see
/// `estimate_words_breakdown` for the pass-level scope that needs (and `docs/research/
/// memory-measurement-repair.md` for why each scope was picked for its caller). Before this pass-
/// level scope existed, EVERY call here (including this single-word one) was already known and
/// documented to double-count a shared `alternatives` entry per referrer -- a deliberate
/// conservative-by-construction trade-off, not a latent bug (it only ever charged *more* than the
/// true retained set, biasing the byte budget toward evicting sooner). This function's own
/// undocumented-until-now claim was fixed too, since a per-word walk can still see the same `Rc`
/// twice via two different `non_heads`.
pub fn estimate_word_bytes_breakdown(w: &Word) -> WordByteBreakdown {
    let mut seen = HashSet::new();
    estimate_word_bytes_breakdown_seen(w, &mut seen)
}

/// Shared recursion for both public entry points: `seen` charges each `alternatives` pointer once per call, not once per referrer.
fn estimate_word_bytes_breakdown_seen(
    w: &Word,
    seen: &mut HashSet<*const Word>,
) -> WordByteBreakdown {
    let mut b = WordByteBreakdown {
        base: std::mem::size_of::<Word>(),
        shape: estimate_shape_bytes(&w.shape),
        syn_fs: estimate_fs_bytes(&w.syn_fs),
        real_fs: estimate_fs_bytes(&w.real_fs),
        morphs: w.morphs.len() * std::mem::size_of::<MorphRecord>(),
        mrule_apps: w.mrule_apps.len() * std::mem::size_of::<Option<MRuleId>>(),
        obligatory: w.obligatory.len() * std::mem::size_of::<FeatId>(),
        unapplied_rule_counts: w.unapplied_rule_counts.len()
            * (std::mem::size_of::<MRuleId>() + std::mem::size_of::<u32>()),
        root_runtime_id: w.root_runtime_id.as_ref().map_or(0, String::len),
        non_heads: 0,
        alternatives: 0,
    };
    for nh in &w.non_heads {
        b.non_heads += estimate_word_bytes_breakdown_seen(nh, seen).total();
    }
    for alt in &w.alternatives {
        if seen.insert(Rc::as_ptr(alt)) {
            b.alternatives += estimate_word_bytes_breakdown_seen(alt, seen).total();
        }
    }
    b
}

/// Approximate heap-byte cost of one `Word` (see `estimate_word_bytes_breakdown` for the
/// per-field decomposition this sums).
pub fn estimate_word_bytes(w: &Word) -> usize {
    estimate_word_bytes_breakdown(w).total()
}

/// Sum of each `Word` in `words`'s byte breakdown, sharing ONE `alternatives` seen-set across the
/// whole slice: an `Rc<Word>` alternative retained by more than one word in `words` is still one
/// heap allocation, so it is charged once for the group, not once per referrer. This is the scope
/// `pg_rules::word_stats::record_live_words` ("live peak", one stratum pass's whole `words`
/// accumulator) and the memo byte budget (`AnalysisScope::has_byte_capacity`, charged per
/// `MemoEntry::results` at insert time) both need. It does **not** dedup across separate calls —
/// two different memo entries, or this pass against a previous one — so a shared allocation already
/// paid for elsewhere can still be recharged in a later call; that residual is documented in
/// `docs/research/memory-measurement-repair.md` rather than silently claimed fixed.
/// Generic over any borrowed source (`&[Word]`, `&Vec<Word>`, a `HashMap` `.values()` iterator, ...)
/// so a caller with only a borrowed collection -- `pg-parse`'s synthesis-loop `HC_CLOCK_SAMPLE=1`
/// sample point, in particular -- never has to clone a whole `Word` set just to measure it; cloning
/// there would inflate the very allocator peak the sample exists to read.
pub fn estimate_words_breakdown<'w, I>(words: I) -> WordByteBreakdown
where
    I: IntoIterator<Item = &'w Word>,
{
    let mut seen = HashSet::new();
    let mut b = WordByteBreakdown::default();
    for w in words {
        b.add_assign(&estimate_word_bytes_breakdown_seen(w, &mut seen));
    }
    b
}

/// Approximate heap-byte cost of a `MemoEntry`'s `results` (or any other `Word` group charged
/// together), the payload the memo byte budget accounts against. See `estimate_words_breakdown` for
/// the dedup scope and the borrowed-iterator rationale.
pub fn estimate_words_bytes(words: &[Word]) -> usize {
    estimate_words_breakdown(words).total()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_shape::ShapeBuilder;

    fn w() -> Word {
        Word::new(ShapeBuilder::new().finish(), StratumId(0))
    }

    /// T1: one pass-level call charges a shared alternative once; two separate per-word calls still double-charge it.
    #[test]
    fn shared_alternative_across_two_words_is_charged_once_by_the_pass_level_walk() {
        let mut shared = w();
        shared.morphs = vec![MorphRecord::new(AllomorphId(1), MorphemeId(2), 0)];
        let shared = Rc::new(shared);

        let mut a = w();
        a.alternatives.push(Rc::clone(&shared));
        let mut b = w();
        b.alternatives.push(Rc::clone(&shared));

        let alt_cost = estimate_word_bytes_breakdown(&shared).total();
        assert!(
            alt_cost > 0,
            "the shared alternative needs nonzero measured cost for this test to mean anything"
        );

        // Two independent per-word calls: each pays for the shared alternative in full.
        let per_word_sum =
            estimate_word_bytes_breakdown(&a).total() + estimate_word_bytes_breakdown(&b).total();

        // One pass-level call over both words: the shared alternative is charged exactly once.
        let one_pass = estimate_words_breakdown(&[a, b]).total();
        assert_eq!(
            one_pass,
            per_word_sum - alt_cost,
            "the pass-level walk must drop exactly one duplicate charge of the shared alternative"
        );
    }

    #[test]
    fn final_template_state_is_a_two_value_copyable_key_component() {
        let mut a = w();
        let mut b = w();
        a.flags.final_template_state = FinalTemplateState::None;
        b.flags.final_template_state = FinalTemplateState::NonTemplate;
        assert_ne!(a.dedup_key(), b.dedup_key());
        assert_eq!(FinalTemplateState::default(), FinalTemplateState::None);
        let copied = FinalTemplateState::NonTemplate;
        assert_eq!(copied, FinalTemplateState::NonTemplate);
    }

    #[test]
    fn dedup_key_keeps_selected_allomorph_and_annotation_order() {
        let mut first = w();
        let mut second = w();
        first.morphs = vec![MorphRecord::new(AllomorphId(1), MorphemeId(2), 0)];
        second.morphs = vec![MorphRecord::new(AllomorphId(3), MorphemeId(2), 0)];
        assert_ne!(first.dedup_key(), second.dedup_key());

        second.morphs[0].allomorph = AllomorphId(1);
        second.morphs[0].order = 1;
        assert_ne!(first.dedup_key(), second.dedup_key());
    }

    #[test]
    fn dedup_key_ignores_procedural_passed_over_state() {
        let mut first = w();
        let mut second = w();
        first.morphs = vec![MorphRecord::new(AllomorphId(1), MorphemeId(2), 0)];
        second.morphs = first.morphs.clone();
        first.morphs[0].passed_over = Some(vec![2, 4].into_boxed_slice());
        assert_eq!(first.dedup_key(), second.dedup_key());
    }

    #[test]
    fn replay_keeps_subtree_final_template_state() {
        let mut stored = w();
        stored.flags.final_template_state = FinalTemplateState::NonTemplate;
        let query = w();
        let replayed = stored.replay_onto(&query, 0, 0);
        assert_eq!(
            replayed.flags.final_template_state,
            FinalTemplateState::NonTemplate
        );
    }

    #[test]
    fn replay_grafts_mrule_prefix_keeps_suffix() {
        // Stored subtree result: reached node N after unapplying [r0, r1], then the subtree unapplied [r2].
        let mut stored = w();
        stored.mrule_apps = vec![Some(MRuleId(0)), Some(MRuleId(1)), Some(MRuleId(2))];
        stored.mrule_app_index = 2;

        // Query reached the same state N via a different order [r1, r0] (same multiset, same length -- the equal-key guarantee).
        let mut query = w();
        query.mrule_apps = vec![Some(MRuleId(1)), Some(MRuleId(0))];
        query.mrule_app_index = 1;

        let r = stored.replay_onto(&query, 2, 0);
        // Result trail = query's prefix ++ stored's subtree-local suffix.
        assert_eq!(
            r.mrule_apps,
            vec![Some(MRuleId(1)), Some(MRuleId(0)), Some(MRuleId(2))]
        );
        assert_eq!(r.mrule_app_index, 2);
    }

    #[test]
    fn replay_grafts_non_head_prefix() {
        // stored non_heads: [nh_s1, nh_s2]; prefix len 1 → subtree suffix = [nh_s2].
        let mut stored = w();
        let mut nh1 = w();
        nh1.stratum = StratumId(1);
        let mut nh2 = w();
        nh2.stratum = StratumId(2);
        stored.non_heads = vec![nh1, nh2];
        stored.non_head_app_index = 1;

        // query non_heads: [nh_s3].
        let mut query = w();
        let mut qnh = w();
        qnh.stratum = StratumId(3);
        query.non_heads = vec![qnh];
        query.non_head_app_index = 0;

        let r = stored.replay_onto(&query, 0, 1);
        // query's non-heads ++ stored's subtree-local suffix.
        assert_eq!(r.non_heads.len(), 2);
        assert_eq!(r.non_heads[0].stratum, StratumId(3));
        assert_eq!(r.non_heads[1].stratum, StratumId(2));
        assert_eq!(r.non_head_app_index, 1);
    }

    #[test]
    fn replay_leaves_count_multiset_untouched() {
        // The multiset is carried from `stored` unchanged (equal-key invariant makes it correct).
        let mut stored = w();
        stored.mrule_apps = vec![Some(MRuleId(0)), Some(MRuleId(1))];
        stored.record_unapplication(MRuleId(0));
        stored.record_unapplication(MRuleId(1));
        let before = stored.unapplied_rule_counts.clone();

        let mut query = w();
        query.mrule_apps = vec![Some(MRuleId(1)), Some(MRuleId(0))];
        let r = stored.replay_onto(&query, 2, 0);
        assert_eq!(r.unapplied_rule_counts, before);
    }
}
