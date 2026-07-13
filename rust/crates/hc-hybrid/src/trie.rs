//! `trie.rs` (F3, HYBRID_FST_RUST_PLAN.md §8) — port of the trie-CONSTRUCTION half of C#
//! `FstTemplateAnalyzer.cs` (`SIL.Machine.Morphology.HermitCrab`, `fst-oracle`/`fst-advisor`
//! branch): the shared root trie + checkpoints, affix arcs (wired with `SurfacePhonology`'s
//! junction variants and deletion skips), templates/slots, derivation BFS with the compounding
//! edge (`DerivableToCategory`), the bounded compound loop, and boundary arcs. This module does
//! NOT implement either walker (`AnalyzeShape`/`EpsilonClosure` or `AnalyzeChain`/`ChainClosure`)
//! — that is `walk.rs`'s job (F4/F7), per the plan's own module sketch (§7).
//!
//! ## Representation (a deliberate, documented deviation from C#'s object graph)
//! C#'s `FstTemplateAnalyzer` builds a `Fst<Shape,ShapeNode>` (a general graph library shared with
//! the engine's FSA machinery) with `State`/`Arc` objects and a side `Dictionary<State,uint>` for
//! per-state tokens. Rust represents the same graph as a flat arena: [`Trie::states`] indexed by
//! [`StateId`] (a plain `u32`), each [`StateData`] owning its own `arcs: Vec<ArcData>` and an
//! `Option<u32>` token — no separate token side-table, no object identity, no `Dictionary`
//! anywhere in the hot construction path (plan §4.2's determinism rule: no `HashMap`/`HashSet`
//! iteration order may reach observable output). [`ArcLabel`] distinguishes an epsilon arc from a
//! segment-consuming arc from a boundary-consuming arc BY CONSTRUCTION (an enum variant) rather
//! than C#'s runtime `IsBoundary(FeatureStruct)` predicate check — the two walkers (F4/F7) match on
//! the variant instead of re-deriving it. Neither change alters observable behavior: it changes
//! only how the identical graph is stored.
//!
//! ## Determinism (plan §4.2)
//! Every iteration that can affect the graph's topology or the structural dump walks `Vec`s in
//! grammar-document order (`g.strata`, `stratum.entries`, `stratum.mrules`, `stratum.templates`,
//! `template.slots`, `allomorph.rhs`) — never a `HashMap`/`HashSet`. The two per-allomorph/per-root
//! memoization maps ([`TrieBuilder::root_chains`], [`TrieBuilder::root_checkpoints`]) ARE
//! `HashMap`s, but only ever consulted via `get`/`insert`/`contains_key`, never iterated — their
//! internal bucket order cannot reach any output, satisfying §4.2 by the same argument
//! `token.rs`'s `MorphTokenCodec` already documents for its own lookup map.
//!
//! ## Canonical structural dump (`canon.rs`)
//! State-id NUMBERS themselves are an internal allocation-order artifact with no cross-language
//! meaning (Rust's arena order has no reason to match C#'s `Fst.CreateState` call order, and — as
//! documented in the C# `FstStructuralDump.cs` — C#'s OWN arc storage order is not even plain
//! insertion order, thanks to `ArcCollection`'s `List<T>.BinarySearch`-based insert). The F3 gate's
//! byte-identical structural dump is produced by `canon.rs`'s color refinement over this module's
//! graph, not by comparing raw [`StateId`]s.

use rustc_hash::FxHashMap as HashMap;

use hc_featstruct::FsId;
use hc_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use hc_grammar::model::{
    AffixAllomorphDef, AffixTemplateDef, AllomorphId, Grammar, LexEntryId, MRuleId, MorphRuleDef,
    MorphemeId, OutputAction, SlotDef,
};
use hc_parse::Morpher;
use hc_shape::{NodeKind, Shape};
use unicode_normalization::UnicodeNormalization;

use crate::surface::SurfacePhonology;
use crate::token::{self, MorphOp};

/// A dense index into [`Trie::states`] — the arena's own allocation order, meaningful only within
/// one build (see the module doc's canonical-dump note for why it is never compared directly
/// across languages).
pub type StateId = u32;

/// C#'s `language.SurfaceStratum.CharacterDefinitionTable` lookup, factored out of
/// [`TrieBuilder::new`] so `walk.rs` (F4) can segment a surface WORD against the identical table
/// (same "one stratum, one table" convention every reference grammar satisfies — see that
/// constructor's own doc) without duplicating the lookup.
pub(crate) fn surface_table(g: &Grammar) -> (&CharDefTable, usize) {
    let surface_stratum = g.strata.last().expect("a grammar has at least one stratum");
    let table = &g.char_tables[surface_stratum.table.0 as usize];
    (table, g.phon_features.len())
}

/// One trie arc's condition. `Segment`/`Boundary` carry the SAME phonological feature lanes
/// `hc_rules::shape_feat::segment_with_features` attaches to a live parse shape (`Vec<u64>`, width
/// `grammar.phon_features.len()`), so a later walker (F4/F7) matches them with
/// [`hc_featstruct::flat_unifiable`] exactly like every other lane-based match in this port. Both
/// variants ALSO carry the char-def's own (NFD-normalized) string representation, needed for the
/// structural dump's label-repr (see [`label_repr`]) in two real, empirically-found cases (not
/// hypothetical — both bit while building the F3 golden comparison, recorded in `MANIFEST.txt`'s
/// F3 format-extension entry):
/// - A `Boundary` char-def's C# `FeatureStruct` carries NO phonological lanes at all
///   (`CharacterDefinitionTable.Add`'s `fs == null` branch stamps only `{Type, StrRep}` from
///   `HCFeatureSystem.Instance`) — UNCONDITIONALLY, for every grammar.
/// - A `Segment` char-def hits the SAME `fs == null` branch too, but only when the WHOLE
///   grammar's declared `<PhonologicalFeatureSystem>` is empty (`XmlLanguageLoader.
///   LoadCharacterDefinitionTable`: `fs = null; if (_language.PhonologicalFeatureSystem.Count > 0)
///   fs = LoadFeatureStruct(...)` — a grammar-wide gate, not a per-segment one). Sena has zero
///   phonological rules and, empirically, zero declared phonological features either
///   (`g.phon_features.is_empty()`, i.e. `len() == 1`: only the synthetic `Type` feature — see
///   `hc_grammar::featsys::PhonFeatureSystem::is_empty`'s own doc, which independently documents
///   this exact Sena fact) — so EVERY Sena segment's C# label is `{StrRep, Type}` only, and
///   rendering it via `render_feature_struct` alone (which only knows `phon_features` lanes)
///   collapsed every segment to the indistinguishable `[Type:segment]`, catastrophically merging
///   the structural dump (caught immediately by the F3 gate's byte-comparison against the
///   regenerated Sena golden).
///
/// `char_def` (found building F4's walker): the SAME zero-phon-feature fact above means
/// `flat_unifiable` on a Sena arc's `lanes` is TRIVIALLY TRUE against any other Sena segment's lanes
/// (every non-`Type` lane defaults to the full/wildcard mask when no `<FeatureValue>` narrows it —
/// see `hc_grammar::chardef::CharDefTable`'s own `unif_closure` field doc and
/// `hc_parse::root_trie`'s module doc, "Segment discrimination without phonological features",
/// which already solved this EXACT problem for lexical lookup: C#'s real per-segment identity for a
/// zero-feature grammar lives in `StrRep`, which this port does not model as a lane — its faithful
/// analog is the segment's own [`CharDefId`]). A lane-only arc match therefore lets ANY Sena segment
/// cross an arc built for any OTHER Sena segment, which — caught empirically building this
/// milestone's Sena candidate-parity gate — overflows the beam budget on effectively every corpus
/// word (the first real segment already matches almost the whole shared trie). Match predicate
/// mirrors `root_trie.rs`'s `edge_matches` exactly: `char_def` equality, OR closure membership when
/// the table has one (`CharDefTable::unifiable_cds`), AND `flat_unifiable` on the lanes — which
/// reduces to plain `char_def` identity for a zero-feature table (no closure exists) and adds
/// nothing spurious for a feature-bearing one (identity implies unifiable lanes trivially; the
/// closure only ever WIDENS a feature-bearing table's cross-char-def matches, never narrows).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArcLabel {
    Epsilon,
    Segment {
        lanes: Vec<u64>,
        reprs: Vec<String>,
        char_def: u32,
    },
    Boundary {
        lanes: Vec<u64>,
        reprs: Vec<String>,
        char_def: u32,
    },
}

#[derive(Clone, Debug)]
pub struct ArcData {
    pub label: ArcLabel,
    pub target: StateId,
}

#[derive(Default)]
pub struct StateData {
    pub arcs: Vec<ArcData>,
    pub accepting: bool,
    /// C#'s `_tokenOnEntry[state]`: the packed [`token::MorphOp`]+morpheme-index token emitted on
    /// ENTERING this state (not a per-arc token — see `token.rs`'s module doc for why the trie
    /// tags states, not arcs).
    pub token: Option<u32>,
}

/// The built morphotactic trie: states + arcs (this module's whole product), the codec that
/// assigned every emitted token's morpheme index, the coverage diagnostic (which [`MorphOp`]s the
/// build had to skip because the FST cannot model them — reduplication/infix/circumfix/process),
/// and the boundary alphabet (every `Boundary`-kind char-def's lanes) walk.rs (F4/F7) will need for
/// `ChainClosure`'s "insert boundary" move — computed here since it is a build-time, grammar-only
/// fact, but not consumed by anything in this module.
pub struct Trie {
    states: Vec<StateData>,
    start: StateId,
    codec: token::MorphTokenCodec,
    uncovered: [bool; 12],
    pub boundary_alphabet: Vec<Vec<u64>>,
}

impl Trie {
    /// Test-support constructor: assemble a [`Trie`] directly from already-built states, bypassing
    /// [`Trie::build`]'s grammar-driven [`TrieBuilder`] entirely. Mirrors C# `BeamCapTests`'s own
    /// pattern of hand-building an `InversePhonology` chain (bypassing `RuleInverseCompiler`) so a
    /// pathological shape is fully engineered rather than emergent from a real grammar — the bare-
    /// walker analog needed by `walk.rs`'s beam-cap port (F4), since the bare walk has no chain to
    /// hand-build instead. Every [`StateData`]/[`ArcData`] field is already `pub`; this constructor
    /// exists only because [`Trie`]'s own fields are not.
    pub fn from_states(
        states: Vec<StateData>,
        start: StateId,
        codec: token::MorphTokenCodec,
    ) -> Trie {
        Trie {
            states,
            start,
            codec,
            uncovered: [false; 12],
            boundary_alphabet: Vec::new(),
        }
    }

    pub fn state_count(&self) -> usize {
        self.states.len()
    }

    pub fn start(&self) -> StateId {
        self.start
    }

    pub fn is_accepting(&self, s: StateId) -> bool {
        self.states[s as usize].accepting
    }

    pub fn token(&self, s: StateId) -> Option<u32> {
        self.states[s as usize].token
    }

    pub fn arcs(&self, s: StateId) -> &[ArcData] {
        &self.states[s as usize].arcs
    }

    pub fn codec(&self) -> &token::MorphTokenCodec {
        &self.codec
    }

    /// C# `CoversAllConstructs`.
    pub fn covers_all_constructs(&self) -> bool {
        self.uncovered.iter().all(|&b| !b)
    }

    /// C# `UncoveredOps`, in [`MorphOp::ALL`] order (deterministic; no `HashSet` iteration).
    pub fn uncovered_ops(&self) -> Vec<MorphOp> {
        MorphOp::ALL
            .iter()
            .copied()
            .filter(|&op| self.uncovered[op as usize])
            .collect()
    }

    /// Build the trie for `g`, per the plan's F3 milestone. `surface` is F2's product (junction
    /// variants + deletion skips); `morpher` is only used for [`SurfacePhonology::bare_root_surfaces`]
    /// (the obligatoriness/surface-allomorph check — C#'s ctor takes the same `Morpher`).
    /// `enable_junction_probing` mirrors C#'s ctor knob of the same name (§8.3/I4): `false`
    /// suppresses [`SurfacePhonology::deletion_junctions`] wiring entirely (every root's pending-skip
    /// list stays empty), leaving segment/boundary arcs otherwise identical.
    pub fn build<'g>(
        g: &'g Grammar,
        surface: &SurfacePhonology<'g>,
        morpher: &Morpher<'g>,
        max_states: usize,
        deriv_depth: usize,
        enable_junction_probing: bool,
    ) -> Trie {
        Trie::build_ex(
            g,
            surface,
            morpher,
            max_states,
            deriv_depth,
            enable_junction_probing,
            true,
        )
    }

    /// [`Trie::build`] with an additional `enable_variants` knob (F7, HYBRID_FST_RUST_PLAN.md §8):
    /// C#'s `LockstepPhonologyProposer`/`ChainPhonologyProposer` each build their own "underlying-only
    /// acceptor" via `new FstTemplateAnalyzer(language)` -- a DIFFERENT, simpler constructor
    /// (`FstTemplateAnalyzer.cs:150-164`) than the one every other caller uses, whose
    /// `affixSurfaces` closure is `s => new[] { s }` (the identity function -- literally no surface
    /// probing, not merely "probing suppressed"): `BuildAffixArcs`'s variant loop
    /// (`variant == underlying` always true) therefore builds ONLY the underlying affix arc, no
    /// assimilated-surface variant arcs at all. `enable_junction_probing` alone (`Trie::build`'s
    /// existing knob) does NOT achieve this -- it only gates `DeletionJunctions` (confirmed by
    /// reading `FstTemplateAnalyzer.cs:205-226`'s private ctor directly: the `enableJunctionProbing`
    /// ternary picks only between `surfacePhonology.DeletionJunctions` and a no-op, while
    /// `affixSurfaces` is unconditionally `surfacePhonology.Variants` on THAT ctor path) -- so the
    /// chain/lockstep proposers' trie needs this SEPARATE, additional suppression.
    /// `enable_variants = false` skips the `surface.variants(underlying)` loop entirely (equivalent
    /// output to C#'s identity closure, since that loop already `continue`s on `variant == underlying`
    /// -- not iterating it at all changes nothing observable, just skips calling into
    /// [`SurfacePhonology`] for no reason). `bare_root_surfaces` is left on its existing
    /// synthesis-aware path regardless (C#'s simple ctor's `root => new[] { UnderlyingForm(root) }`
    /// vs. the full ctor's obligatoriness-aware `BareRootSurfaces` — a difference this port does not
    /// yet distinguish; see this crate's F7 commit message for why this is scoped-safe on the three
    /// reference grammars, verified by the chain-on gate rather than assumed).
    pub fn build_ex<'g>(
        g: &'g Grammar,
        surface: &SurfacePhonology<'g>,
        morpher: &Morpher<'g>,
        max_states: usize,
        deriv_depth: usize,
        enable_junction_probing: bool,
        enable_variants: bool,
    ) -> Trie {
        let mut b = TrieBuilder::new(
            g,
            max_states,
            deriv_depth,
            enable_junction_probing,
            enable_variants,
        );
        b.run(surface, morpher);
        b.finish()
    }
}

/// C# `FeatureStruct.ToString()`-rendered label for one arc — "EPS" for an epsilon arc, else
/// "SEG "/"BOUND " + the rendered feature structure (see [`ArcLabel`]'s doc for the boundary
/// special case). Shared by `canon.rs`'s color refinement (label is part of every refinement key)
/// and the final structural-dump line.
pub fn label_repr(g: &Grammar, label: &ArcLabel) -> String {
    match label {
        ArcLabel::Epsilon => "EPS".to_string(),
        ArcLabel::Segment { lanes, reprs, .. } => {
            if g.phon_features.is_empty() {
                // No declared phonological features anywhere in this grammar (Sena) -- C#'s
                // loader never calls `LoadFeatureStruct` for ANY segment in that case, so every
                // segment's real FeatureStruct is `{StrRep, Type}` only, same as a boundary's.
                format!("SEG [StrRep:{}, Type:segment]", render_strrep(reprs))
            } else {
                format!("SEG {}", crate::surface::render_feature_struct(g, lanes))
            }
        }
        ArcLabel::Boundary { reprs, .. } => {
            format!("BOUND [StrRep:{}, Type:boundary]", render_strrep(reprs))
        }
    }
}

/// C# `StringFeatureValue.ToString()`'s rendering of a `StrRep` feature value: `"x"` for a single
/// representation, `{"a", "b", ...}` for multiple. PARITY DECISION (found building Sena's dump,
/// which has real multi-representation char-defs): C#'s own multi-value sort
/// (`_values.OrderBy(v => v)`, no comparer — CULTURE-AWARE) is impractical to reproduce bit-for-
/// bit in Rust (it would mean re-implementing .NET's ICU/NLS collation for values like
/// "*0"/"&0"/"^0"/"∅" that sort differently under culture rules than ordinally). Since this dump
/// format is invented fresh in this milestone, both sides instead sort ORDINALLY — the C# side's
/// `FstStructuralDump.LabelRepr` post-processes `FeatureStruct.ToString()`'s output
/// (`SortBraceListsOrdinal`) to match. Single-representation char-defs (the common case) are
/// unaffected either way.
fn render_strrep(reprs: &[String]) -> String {
    if reprs.len() == 1 {
        format!("\"{}\"", reprs[0])
    } else {
        let mut sorted = reprs.to_vec();
        sorted.sort_unstable();
        let parts: Vec<String> = sorted.iter().map(|r| format!("\"{r}\"")).collect();
        format!("{{{}}}", parts.join(", "))
    }
}

/// One root/compound-attachment reference: an allomorph plus the bookkeeping needed to reach it
/// (owning entry + its index within that entry's own allomorph list, so
/// [`TrieBuilder::get_or_build_root_chain`] can re-fetch the `RootAllomorphDef` without a linear
/// search) and its category/stratum (C#'s `RootRef` struct, `FstTemplateAnalyzer.cs:2001-2013`).
#[derive(Clone, Copy)]
struct RootRef {
    allomorph: AllomorphId,
    entry: LexEntryId,
    allo_idx: u16,
    category: FsId,
    stratum_index: u8,
}

struct TrieBuilder<'g> {
    g: &'g Grammar,
    table: &'g CharDefTable,
    w: usize,
    states: Vec<StateData>,
    start: StateId,
    codec: token::MorphTokenCodec,
    root_chains: HashMap<AllomorphId, (StateId, StateId)>,
    root_checkpoints: HashMap<AllomorphId, Vec<StateId>>,
    uncovered: [bool; 12],
    deriv_prefix_rules: Vec<MRuleId>,
    deriv_suffix_rules: Vec<MRuleId>,
    compounding_rules: Vec<MRuleId>,
    max_states: usize,
    deriv_depth: usize,
    enable_junction_probing: bool,
    enable_variants: bool,
    boundary_alphabet: Vec<Vec<u64>>,
}

impl<'g> TrieBuilder<'g> {
    fn new(
        g: &'g Grammar,
        max_states: usize,
        deriv_depth: usize,
        enable_junction_probing: bool,
        enable_variants: bool,
    ) -> Self {
        // C#'s single `_table` field: `language.SurfaceStratum.CharacterDefinitionTable` — the
        // LAST stratum's table, used for every segmentation this class does (roots, affixes,
        // surface variants alike). Matches `SurfacePhonology::new`'s identical convention.
        let (table, w) = surface_table(g);

        let mut boundary_alphabet = Vec::new();
        for (_, cd) in table.iter() {
            if cd.kind() == CharDefKind::Boundary {
                boundary_alphabet.push(cd.feature_lanes().to_vec());
            }
        }

        let mut b = TrieBuilder {
            g,
            table,
            w,
            states: Vec::new(),
            start: 0,
            codec: token::MorphTokenCodec::new(),
            root_chains: HashMap::default(),
            root_checkpoints: HashMap::default(),
            uncovered: [false; 12],
            deriv_prefix_rules: Vec::new(),
            deriv_suffix_rules: Vec::new(),
            compounding_rules: Vec::new(),
            max_states,
            deriv_depth,
            enable_junction_probing,
            enable_variants,
            boundary_alphabet,
        };
        b.start = b.new_state();
        b
    }

    fn finish(self) -> Trie {
        Trie {
            states: self.states,
            start: self.start,
            codec: self.codec,
            uncovered: self.uncovered,
            boundary_alphabet: self.boundary_alphabet,
        }
    }

    // ---- primitive graph ops -------------------------------------------------------------------

    fn new_state(&mut self) -> StateId {
        let id = self.states.len();
        assert!(
            id < self.max_states,
            "FstTemplateAnalyzer/Trie exceeded the state budget ({}); this grammar needs the \
             lazy/on-the-fly partition rather than an eager build (HERMITCRAB_FST_PLAN.md §10)",
            self.max_states
        );
        self.states.push(StateData::default());
        id as StateId
    }

    /// PARITY (quirk #9, found building F4's candidate-order gate — not in the original 8-item
    /// `F1_QUIRK_AUDIT.md` list; added here): C#'s `State.Arcs` is `ArcCollection`
    /// (`src/SIL.Machine/FiniteState/ArcCollection.cs`), which stores arcs in a `List<Arc>` kept
    /// "sorted" by `_arcs.BinarySearch(arc, _arcComparer)` on every `Add` (`:136-143`), where
    /// `_arcComparer` projects each arc's `PriorityType` (`:24`). `FstTemplateAnalyzer` never sets a
    /// non-default priority anywhere (grepped: zero hits for `ArcPriorityType`/`priorityType` in
    /// that file), so EVERY comparison the comparer ever makes here ties (`Compare` returns `0`).
    /// .NET's `List<T>.BinarySearch` (`lo=0, hi=count-1; i=lo+((hi-lo)>>1); if Compare(arr[i],v)==0
    /// return i;`) returns the FIRST probed midpoint the moment a tie is found — with an
    /// all-tied comparer that is the very first comparison, so the insertion index for the
    /// `k`-th arc added to a state (`k` = arcs already present) is the deterministic, closed-form
    /// `0` if `k==0` else `(k-1)/2` (`AddInternal`'s `_arcs.Insert(index, arc)`, `:141` — this
    /// reduces to plain insertion-order-preserving `push` for `k∈{0,1}` but reorders non-trivially
    /// from the 4th arc onward: NOT a bug in this port, a faithful mirror of a real, previously
    /// undocumented ArcCollection quirk that determines candidate EMISSION order, not merely trie
    /// topology (`trie.rs`'s F3-era module doc and `canon.rs`'s doc both already flagged that C#'s
    /// raw arc order is not plain insertion order and predicted a later order-sensitive gate would
    /// need to confront it — this is that gate). `debug_assert!` below documents the "one shared
    /// priority" precondition this closed form depends on, so a future construct that ever varies
    /// arc priority fails loudly here rather than silently reordering wrong.
    fn arc_insert_index(current_len: usize) -> usize {
        if current_len == 0 {
            0
        } else {
            (current_len - 1) / 2
        }
    }

    // NOTE: `arc_insert_index` assumes every arc at a state shares one priority (C#'s implicit
    // constant `ArcPriorityType.Medium` -- see that function's doc). `ArcData` has no priority
    // field at all, so there is nothing to assert structurally; this is a marker comment for
    // whoever adds a priority-varying construct later; that person must revisit this function.
    fn insert_arc(&mut self, from: StateId, arc: ArcData) {
        let arcs = &mut self.states[from as usize].arcs;
        let idx = Self::arc_insert_index(arcs.len());
        arcs.insert(idx, arc);
    }

    fn add_epsilon(&mut self, from: StateId, to: StateId) {
        self.insert_arc(
            from,
            ArcData {
                label: ArcLabel::Epsilon,
                target: to,
            },
        );
    }

    fn add_labeled(&mut self, from: StateId, label: ArcLabel) -> StateId {
        let to = self.new_state();
        self.insert_arc(from, ArcData { label, target: to });
        to
    }

    fn set_token(&mut self, s: StateId, token: u32) {
        self.states[s as usize].token = Some(token);
    }

    fn set_accepting(&mut self, s: StateId) {
        self.states[s as usize].accepting = true;
    }

    /// C# `GetSegments` (`FstTemplateAnalyzer.cs:1812-1826`): a root/affix chain's segment list,
    /// INCLUDING boundary nodes, read straight off `shape`'s own char-def ids against this
    /// analyzer's single table (see [`TrieBuilder::new`]'s doc on why one table suffices — every
    /// reference grammar has exactly one stratum).
    fn get_segments(&self, shape: &Shape) -> Vec<ArcLabel> {
        shape
            .interior()
            .filter_map(|(_, kind, cd, _flags)| {
                let cd_id = CharDefId(cd);
                let lanes = lanes_for(self.table, cd_id, self.w);
                let reprs = || self.table.get(cd_id).representations_nfd().to_vec();
                match kind {
                    NodeKind::Segment => Some(ArcLabel::Segment {
                        lanes,
                        reprs: reprs(),
                        char_def: cd_id.0,
                    }),
                    NodeKind::Boundary => Some(ArcLabel::Boundary {
                        lanes,
                        reprs: reprs(),
                        char_def: cd_id.0,
                    }),
                    _ => None,
                }
            })
            .collect()
    }

    /// Chain `labels` from `from`, returning the final state (C#'s repeated `s = AddArc(s, fs)` loop).
    fn add_segments(&mut self, from: StateId, labels: &[ArcLabel]) -> StateId {
        let mut s = from;
        for label in labels {
            s = self.add_labeled(s, label.clone());
        }
        s
    }

    // ---- rule/allomorph helpers -----------------------------------------------------------------

    fn allomorphs_of(&self, mid: MRuleId) -> &'g [AffixAllomorphDef] {
        match &self.g.mrules[mid.0 as usize] {
            MorphRuleDef::AffixProcess(def) => &def.allomorphs,
            MorphRuleDef::Realizational(def) => &def.allomorphs,
            MorphRuleDef::Compounding(_) => &[],
        }
    }

    fn owning_morpheme_of_mrule(g: &Grammar, mid: MRuleId) -> MorphemeId {
        match &g.mrules[mid.0 as usize] {
            MorphRuleDef::AffixProcess(def) => def.morpheme,
            MorphRuleDef::Realizational(def) => def.morpheme,
            MorphRuleDef::Compounding(_) => {
                unreachable!("a slot/derivation rule reference is never a CompoundingRule")
            }
        }
    }

    /// `required_syn_fs`/`out_syn_fs` fall back to `FsId(0)` (the grammar's interned EMPTY feature
    /// structure — always id 0, per `Grammar::fs_interner`'s own doc) wherever C#'s
    /// `RequiredCategory`/`OutCategory` return `null` (a `RealizationalAffixProcessRule` has no
    /// output category; nothing here is ever asked for a `CompoundingRule`'s). `FsId(0)`'s
    /// `is_empty()` is `true`, so every "null-or-empty" check downstream is unaffected by folding
    /// the two cases together — a deliberate simplification the Rust `FsId` model affords for free.
    fn required_category(g: &Grammar, mid: MRuleId) -> FsId {
        match &g.mrules[mid.0 as usize] {
            MorphRuleDef::AffixProcess(def) => def.required_syn_fs,
            MorphRuleDef::Realizational(def) => def.required_syn_fs,
            MorphRuleDef::Compounding(_) => FsId(0),
        }
    }

    fn out_category(g: &Grammar, mid: MRuleId) -> FsId {
        match &g.mrules[mid.0 as usize] {
            MorphRuleDef::AffixProcess(def) => def.out_syn_fs,
            _ => FsId(0),
        }
    }

    /// C# `RuleOp`/first-allomorph classification (`FstTemplateAnalyzer.cs:1257-1264`).
    fn rule_op(allomorphs: &[AffixAllomorphDef]) -> MorphOp {
        allomorphs
            .first()
            .map(|a| token::classify_affix(&a.rhs))
            .unwrap_or(MorphOp::None)
    }

    fn category_matches(&self, root_category: FsId, required: FsId) -> bool {
        let req = self.g.fs_interner.get(required);
        if req.is_empty() {
            return true;
        }
        let cat = self.g.fs_interner.get(root_category);
        hc_featstruct::is_unifiable(cat, req)
    }

    /// C# `DerivableToCategory` (`FstTemplateAnalyzer.cs:1567-1622`), including the Phase-G2
    /// compounding edge.
    fn derivable_to_category(&self, root_category: FsId, template_category: FsId) -> bool {
        let templ = self.g.fs_interner.get(template_category);
        if templ.is_empty() {
            return false;
        }
        let mut frontier: Vec<FsId> = vec![root_category];
        for _ in 0..self.deriv_depth {
            if frontier.is_empty() {
                break;
            }
            let mut next: Vec<FsId> = Vec::new();
            for &cat_id in &frontier {
                let cat = self.g.fs_interner.get(cat_id);
                for &mid in self
                    .deriv_suffix_rules
                    .iter()
                    .chain(self.deriv_prefix_rules.iter())
                {
                    let out_cat_id = Self::out_category(self.g, mid);
                    let out_cat = self.g.fs_interner.get(out_cat_id);
                    if out_cat.is_empty() {
                        continue; // not a category-changing derivation
                    }
                    let in_cat_id = Self::required_category(self.g, mid);
                    let in_cat = self.g.fs_interner.get(in_cat_id);
                    if !in_cat.is_empty() && !hc_featstruct::is_unifiable(cat, in_cat) {
                        continue;
                    }
                    if hc_featstruct::is_unifiable(out_cat, templ) {
                        return true;
                    }
                    next.push(out_cat_id);
                }
                for &mid in &self.compounding_rules {
                    let MorphRuleDef::Compounding(def) = &self.g.mrules[mid.0 as usize] else {
                        unreachable!("compounding_rules only ever holds Compounding rule ids")
                    };
                    let out_cat = self.g.fs_interner.get(def.out_syn_fs);
                    if out_cat.is_empty() {
                        continue;
                    }
                    let head_req = self.g.fs_interner.get(def.head_required_syn_fs);
                    let non_head_req = self.g.fs_interner.get(def.non_head_required_syn_fs);
                    let can_head =
                        head_req.is_empty() || hc_featstruct::is_unifiable(cat, head_req);
                    let can_non_head =
                        non_head_req.is_empty() || hc_featstruct::is_unifiable(cat, non_head_req);
                    if !can_head && !can_non_head {
                        continue; // this category fits neither role of this compounding rule
                    }
                    if hc_featstruct::is_unifiable(out_cat, templ) {
                        return true;
                    }
                    next.push(def.out_syn_fs);
                }
            }
            frontier = next;
        }
        false
    }

    // ---- root chains ----------------------------------------------------------------------------

    fn get_or_build_root_chain(&mut self, r: &RootRef) -> (StateId, StateId) {
        if let Some(&pair) = self.root_chains.get(&r.allomorph) {
            return pair;
        }
        let g = self.g;
        let entry_def = &g.entries[r.entry.0 as usize];
        let root = &entry_def.allomorphs[r.allo_idx as usize];
        let morpheme = entry_def.morpheme;

        let entry_state = self.new_state();
        let mut checkpoints = vec![entry_state];
        let labels = self.get_segments(&root.shape.shape);
        let mut state = entry_state;
        for label in &labels {
            state = self.add_labeled(state, label.clone());
            checkpoints.push(state);
        }
        let tok = token::encode(MorphOp::Root, self.codec.get_or_add_index(morpheme));
        self.set_token(state, tok);

        self.root_chains.insert(r.allomorph, (entry_state, state));
        self.root_checkpoints.insert(r.allomorph, checkpoints);
        (entry_state, state)
    }

    /// C# `RootChainAfterSkip` (`FstTemplateAnalyzer.cs:1725-1730`).
    fn root_chain_after_skip(&mut self, r: &RootRef, skip_count: usize) -> Option<StateId> {
        self.get_or_build_root_chain(r); // ensures root_checkpoints is populated
        self.root_checkpoints
            .get(&r.allomorph)
            .and_then(|cps| cps.get(skip_count).copied())
    }

    /// C# `BuildRootChainFromSurface` (`FstTemplateAnalyzer.cs:1751-1773`).
    fn build_root_chain_from_surface(
        &mut self,
        from: StateId,
        surface_str: &str,
        morpheme: MorphemeId,
    ) -> Option<StateId> {
        let shape = hc_grammar::segment::segment(self.table, surface_str).ok()?;
        let labels = self.get_segments(&shape);
        let end = self.add_segments(from, &labels);
        let tok = token::encode(MorphOp::Root, self.codec.get_or_add_index(morpheme));
        self.set_token(end, tok);
        Some(end)
    }

    // ---- affix arcs + junction wiring -----------------------------------------------------------

    /// C# `BuildAffixArcs` (`FstTemplateAnalyzer.cs:1380-1421`). `insert` is `None` for a
    /// zero/empty-segment affix (token only). PARITY (quirk #3, F1_QUIRK_AUDIT.md item 3):
    /// `SurfacePhonology::variants` already dedups by RENDERED STRING (a `BTreeSet<String>`), not
    /// by `FeatureStruct` sequence — this method adds no further dedup, matching C# exactly (state
    /// counts encode this).
    fn build_affix_arcs(
        &mut self,
        token_state: StateId,
        after: StateId,
        insert: Option<(&str, &Shape)>,
        surface: &SurfacePhonology<'g>,
    ) {
        let Some((underlying, shape)) = insert else {
            self.add_epsilon(token_state, after);
            return;
        };
        let labels = self.get_segments(shape);
        let end = self.add_segments(token_state, &labels);
        self.add_epsilon(end, after);

        // F7: `enable_variants = false` mirrors C#'s "underlying-only acceptor" ctor (see
        // `Trie::build_ex`'s doc) -- skip probing entirely rather than probe-then-discard.
        if self.enable_variants {
            for variant in surface.variants(underlying) {
                if variant == underlying {
                    continue; // underlying path already built
                }
                let Ok(vshape) = hc_grammar::segment::segment(self.table, &variant) else {
                    continue;
                };
                let vlabels = self.get_segments(&vshape);
                let vend = self.add_segments(token_state, &vlabels);
                self.add_epsilon(vend, after);
            }
        }
    }

    /// C# `BuildDeletionJunctionArcs` (`FstTemplateAnalyzer.cs:1435-1465`). Distinct outcome
    /// strings share one exit state (`exit_by_string`), matching C#'s dedup exactly.
    fn build_deletion_junction_arcs(
        &mut self,
        token_state: StateId,
        underlying: &str,
        pending_skips: &mut Vec<(StateId, Vec<u64>)>,
        surface: &SurfacePhonology<'g>,
    ) {
        if !self.enable_junction_probing {
            return;
        }
        let mut exit_by_string: HashMap<String, StateId> = HashMap::default();
        for junction in surface.deletion_junctions(underlying) {
            let exit_state = if let Some(&s) = exit_by_string.get(&junction.affix_surface) {
                s
            } else {
                let Ok(shape) = hc_grammar::segment::segment(self.table, &junction.affix_surface)
                else {
                    continue;
                };
                let labels = self.get_segments(&shape);
                let s = self.add_segments(token_state, &labels);
                exit_by_string.insert(junction.affix_surface.clone(), s);
                s
            };
            pending_skips.push((exit_state, junction.deleted_neighbor_lanes));
        }
    }

    /// C# `WireDeletionSkips` (`FstTemplateAnalyzer.cs:1474-1500`): gated by a REAL feature-lane
    /// unification test (`flat_unifiable`), not a string compare — the F3-review fix this milestone
    /// made to `SurfacePhonology::DeletionJunction` (see that struct's doc).
    fn wire_deletion_skips(&mut self, pending_skips: &[(StateId, Vec<u64>)], r: &RootRef) {
        if pending_skips.is_empty() {
            return;
        }
        let g = self.g;
        let root = &g.entries[r.entry.0 as usize].allomorphs[r.allo_idx as usize];
        let labels = self.get_segments(&root.shape.shape);
        let Some(first) = labels.first() else {
            return; // a root with no segments is never gated (nothing to skip into)
        };
        let first_lanes: &[u64] = match first {
            ArcLabel::Segment { lanes, .. } | ArcLabel::Boundary { lanes, .. } => lanes,
            ArcLabel::Epsilon => unreachable!("get_segments never emits an Epsilon label"),
        };
        for (exit_state, onset_class_lanes) in pending_skips {
            if !hc_featstruct::flat_unifiable(first_lanes, onset_class_lanes) {
                continue;
            }
            if let Some(after_skip) = self.root_chain_after_skip(r, 1) {
                self.add_epsilon(*exit_state, after_skip);
            }
        }
    }

    // ---- derivation layers + compound loop -------------------------------------------------------

    /// C# `BuildDerivationLayer` (`FstTemplateAnalyzer.cs:1334-1369`), shared by the prefix and
    /// suffix wrappers below. `collect_skips` mirrors C#'s `pendingSkips != null` gate (prefix
    /// layer only).
    fn build_derivation_layer(
        &mut self,
        entry: StateId,
        rules: &[MRuleId],
        op: MorphOp,
        collect_skips: bool,
        surface: &SurfacePhonology<'g>,
    ) -> (StateId, Vec<(StateId, Vec<u64>)>) {
        let mut current = entry;
        let mut pending_skips = Vec::new();
        for _ in 0..self.deriv_depth {
            let after = self.new_state();
            self.add_epsilon(current, after);
            for &mid in rules {
                for allo in self.allomorphs_of(mid) {
                    if token::classify_affix(&allo.rhs) != op {
                        continue;
                    }
                    let morpheme = Self::owning_morpheme_of_mrule(self.g, mid);
                    let tok = token::encode(op, self.codec.get_or_add_index(morpheme));
                    let token_state = self.new_state();
                    self.set_token(token_state, tok);
                    self.add_epsilon(current, token_state);
                    let insert = first_insert(&allo.rhs);
                    self.build_affix_arcs(token_state, after, insert, surface);
                    if collect_skips {
                        if let Some((underlying, _)) = insert {
                            self.build_deletion_junction_arcs(
                                token_state,
                                underlying,
                                &mut pending_skips,
                                surface,
                            );
                        }
                    }
                }
            }
            current = after;
        }
        (current, pending_skips)
    }

    fn build_derivation_suffix_layer(
        &mut self,
        entry: StateId,
        surface: &SurfacePhonology<'g>,
    ) -> StateId {
        let rules = self.deriv_suffix_rules.clone();
        self.build_derivation_layer(entry, &rules, MorphOp::Suffix, false, surface)
            .0
    }

    fn build_derivation_prefix_layer(
        &mut self,
        entry: StateId,
        surface: &SurfacePhonology<'g>,
    ) -> (StateId, Vec<(StateId, Vec<u64>)>) {
        let rules = self.deriv_prefix_rules.clone();
        self.build_derivation_layer(entry, &rules, MorphOp::Prefix, true, surface)
    }

    /// C# `BuildCompoundLoop` (`FstTemplateAnalyzer.cs:1294-1304`). Bounded at exactly one extra
    /// root (feasibility report §8.5's compound-bound residual).
    fn build_compound_loop(&mut self, roots: &[RootRef], continuation: StateId) -> StateId {
        let join = self.new_state();
        for r in roots {
            let (entry, end) = self.get_or_build_root_chain(r);
            self.add_epsilon(join, entry);
            self.add_epsilon(end, continuation);
        }
        join
    }

    // ---- templates --------------------------------------------------------------------------------

    /// C# `ClassifyTemplate` (`FstTemplateAnalyzer.cs:1200-1233`): split a template's slots into
    /// prefix/suffix lists (prefix REVERSED to surface order), recording any uncovered op for a
    /// slot the FST can't build.
    fn classify_template(
        &mut self,
        template: &'g AffixTemplateDef,
    ) -> (Vec<&'g SlotDef>, Vec<&'g SlotDef>) {
        let mut prefix = Vec::new();
        let mut suffix = Vec::new();
        for slot in &template.slots {
            match self.slot_op(slot) {
                MorphOp::Prefix => prefix.push(slot),
                MorphOp::Suffix => suffix.push(slot),
                _ => {
                    for &mid in &slot.rules {
                        let op = Self::rule_op(self.allomorphs_of(mid));
                        if op != MorphOp::Prefix && op != MorphOp::Suffix && op != MorphOp::None {
                            self.uncovered[op as usize] = true;
                        }
                    }
                }
            }
        }
        prefix.reverse();
        (prefix, suffix)
    }

    /// C# `SlotOp` (`FstTemplateAnalyzer.cs:1238-1254`).
    fn slot_op(&self, slot: &SlotDef) -> MorphOp {
        let mut has_zero = false;
        for &mid in &slot.rules {
            let op = Self::rule_op(self.allomorphs_of(mid));
            if op == MorphOp::Prefix || op == MorphOp::Suffix {
                return op;
            }
            if op == MorphOp::None {
                has_zero = true;
            }
        }
        if has_zero {
            MorphOp::Suffix
        } else {
            MorphOp::None
        }
    }

    /// C# `AppendSlots` (`FstTemplateAnalyzer.cs:1638-1696`).
    fn append_slots(
        &mut self,
        start: StateId,
        slots: &[&'g SlotDef],
        op: MorphOp,
        template_category: FsId,
        surface: &SurfacePhonology<'g>,
    ) -> StateId {
        let mut current = start;
        for slot in slots {
            let after = self.new_state();
            if slot.optional {
                self.add_epsilon(current, after);
            }
            for &mid in &slot.rules {
                let required = Self::required_category(self.g, mid);
                let req_fs = self.g.fs_interner.get(required);
                if !req_fs.is_empty() {
                    let tmpl_fs = self.g.fs_interner.get(template_category);
                    if !hc_featstruct::is_unifiable(tmpl_fs, req_fs) {
                        continue;
                    }
                }
                for allo in self.allomorphs_of(mid) {
                    let aop = token::classify_affix(&allo.rhs);
                    if aop != op && aop != MorphOp::None {
                        self.uncovered[aop as usize] = true;
                        continue;
                    }
                    let morpheme = Self::owning_morpheme_of_mrule(self.g, mid);
                    let affix_token = token::encode(op, self.codec.get_or_add_index(morpheme));
                    let token_state = self.new_state();
                    self.set_token(token_state, affix_token);
                    self.add_epsilon(current, token_state);
                    let insert = first_insert(&allo.rhs);
                    self.build_affix_arcs(token_state, after, insert, surface);
                }
            }
            current = after;
        }
        current
    }

    // ---- top-level orchestration (C#'s private ctor body) ----------------------------------------

    fn run(&mut self, surface: &SurfacePhonology<'g>, morpher: &Morpher<'g>) {
        let g = self.g;

        // Every root, with the stratum index it is introduced at.
        let mut roots: Vec<RootRef> = Vec::new();
        for (si, sd) in g.strata.iter().enumerate() {
            for &entry_id in &sd.entries {
                let entry = &g.entries[entry_id.0 as usize];
                for (allo_idx, allo) in entry.allomorphs.iter().enumerate() {
                    roots.push(RootRef {
                        allomorph: allo.id,
                        entry: entry_id,
                        allo_idx: allo_idx as u16,
                        category: entry.syn_fs,
                        stratum_index: si as u8,
                    });
                }
            }
        }

        // Phase G2: compounding rules, grammar-wide.
        for sd in &g.strata {
            for &mid in &sd.mrules {
                if matches!(g.mrules[mid.0 as usize], MorphRuleDef::Compounding(_)) {
                    self.compounding_rules.push(mid);
                }
            }
        }
        let has_compounding_rules = !self.compounding_rules.is_empty();

        // Standalone derivational affix rules.
        for sd in &g.strata {
            for &mid in &sd.mrules {
                let allomorphs: &[AffixAllomorphDef] = match &g.mrules[mid.0 as usize] {
                    MorphRuleDef::AffixProcess(def) => &def.allomorphs,
                    MorphRuleDef::Realizational(def) => &def.allomorphs,
                    MorphRuleDef::Compounding(_) => continue,
                };
                match Self::rule_op(allomorphs) {
                    MorphOp::Suffix => self.deriv_suffix_rules.push(mid),
                    MorphOp::Prefix => self.deriv_prefix_rules.push(mid),
                    MorphOp::None => {}
                    other => self.uncovered[other as usize] = true,
                }
            }
        }

        // Bare-root paths.
        for r in &roots {
            let surfaces = surface.bare_root_surfaces(morpher, r.entry);
            if surfaces.is_empty() {
                continue; // bare root not valid (obligatory inflection)
            }
            let (entry_state, end_state) = self.get_or_build_root_chain(r);
            let start = self.start;
            self.add_epsilon(start, entry_state);
            self.set_accepting(end_state);

            let root_def = &g.entries[r.entry.0 as usize].allomorphs[r.allo_idx as usize];
            let underlying = underlying_form(root_def);
            let morpheme = g.entries[r.entry.0 as usize].morpheme;
            for s in &surfaces {
                if *s == underlying {
                    continue; // already built from the underlying shape
                }
                let start = self.start;
                if let Some(surface_end) = self.build_root_chain_from_surface(start, s, morpheme) {
                    self.set_accepting(surface_end);
                }
            }
        }

        // Template-less derivational stems (+ the Phase G2 compound loop's home when a grammar
        // compounds but has no other standalone derivational rule).
        if !self.deriv_prefix_rules.is_empty()
            || !self.deriv_suffix_rules.is_empty()
            || has_compounding_rules
        {
            let tl_prefix_entry = self.new_state();
            let (tl_root_start, tl_pending_skips) =
                self.build_derivation_prefix_layer(tl_prefix_entry, surface);
            let start = self.start;
            self.add_epsilon(start, tl_prefix_entry);
            let tl_suffix_entry = self.new_state();
            let tl_suffix_exit = self.build_derivation_suffix_layer(tl_suffix_entry, surface);
            self.set_accepting(tl_suffix_exit);
            let tl_compound_join = if has_compounding_rules {
                Some(self.build_compound_loop(&roots, tl_suffix_entry))
            } else {
                None
            };
            for r in &roots {
                let (entry_state, end_state) = self.get_or_build_root_chain(r);
                self.add_epsilon(tl_root_start, entry_state);
                self.add_epsilon(end_state, tl_suffix_entry);
                if let Some(join) = tl_compound_join {
                    self.add_epsilon(end_state, join);
                }
                self.wire_deletion_skips(&tl_pending_skips, r);
            }
        }

        // Each template: prefix automaton -> (gated roots) -> suffix automaton.
        for (ti, sd) in g.strata.iter().enumerate() {
            for &template_id in &sd.templates {
                let template = &g.templates[template_id.0 as usize];
                let (prefix_slots, suffix_slots) = self.classify_template(template);

                let prefix_entry = self.new_state();
                let prefix_exit = self.append_slots(
                    prefix_entry,
                    &prefix_slots,
                    MorphOp::Prefix,
                    template.required_syn_fs,
                    surface,
                );
                let (root_start, template_pending_skips) =
                    self.build_derivation_prefix_layer(prefix_exit, surface);
                let suffix_entry = self.new_state();
                let suffix_exit = self.append_slots(
                    suffix_entry,
                    &suffix_slots,
                    MorphOp::Suffix,
                    template.required_syn_fs,
                    surface,
                );
                self.set_accepting(suffix_exit);

                let deriv_entry = self.new_state();
                let deriv_exit = self.build_derivation_suffix_layer(deriv_entry, surface);
                self.add_epsilon(deriv_exit, suffix_entry);

                let template_compound_join = if has_compounding_rules {
                    Some(self.build_compound_loop(&roots, deriv_entry))
                } else {
                    None
                };

                let start = self.start;
                self.add_epsilon(start, prefix_entry);

                for r in &roots {
                    if r.stratum_index as usize <= ti
                        && (self.category_matches(r.category, template.required_syn_fs)
                            || self.derivable_to_category(r.category, template.required_syn_fs))
                    {
                        let (entry_state, end_state) = self.get_or_build_root_chain(r);
                        self.add_epsilon(root_start, entry_state);
                        self.add_epsilon(end_state, deriv_entry);
                        if let Some(join) = template_compound_join {
                            self.add_epsilon(end_state, join);
                        }
                        self.wire_deletion_skips(&template_pending_skips, r);
                    }
                }
            }
        }
    }
}

/// C# `PhonologyRuleCompiler`-adjacent `AddArc`/lane padding — mirrors
/// `hc_rules::shape_feat::lanes_for` (private to that crate) for this module's own char-def-id ->
/// lane lookups; every real char-def is already `w`-wide (`hc_grammar`'s loader stamps every
/// char-def, segment or boundary, with a full `feat_sys.len()`-wide row including its own `Type`
/// pin), so the pad branch is defensive, not a live path.
pub(crate) fn lanes_for(table: &CharDefTable, cd: CharDefId, w: usize) -> Vec<u64> {
    let raw = table.get(cd).feature_lanes();
    if raw.len() == w {
        raw.to_vec()
    } else {
        let mut v = vec![u64::MAX; w];
        let n = raw.len().min(w);
        v[..n].copy_from_slice(&raw[..n]);
        v
    }
}

/// C# `UnderlyingForm` (`FstTemplateAnalyzer.cs:1508-1511`).
fn underlying_form(root: &hc_grammar::model::RootAllomorphDef) -> String {
    root.shape.text.nfd().collect()
}

/// The first `InsertSegments` action in an allomorph's RHS (C#'s
/// `allomorph.Rhs.OfType<InsertSegments>().FirstOrDefault()`), as `(underlying text, shape)`.
fn first_insert(rhs: &[OutputAction]) -> Option<(&str, &Shape)> {
    rhs.iter().find_map(|a| match a {
        OutputAction::InsertSegments { shape, .. } => Some((shape.text.as_str(), &shape.shape)),
        _ => None,
    })
}
