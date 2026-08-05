//! Shapes and annotations (plan §5.2).
//!
//! A **frozen shape** is a contiguous struct-of-arrays block (no node objects, no in-array
//! linked list, no annotation tree) interned per parse to a `ShapeId`. This is the Rust-native
//! form of C#'s flat `Shape`: C# kept a doubly-linked list inside its flat arrays only to
//! preserve `ShapeNode` reference identity and O(1) splice under the existing API; Rust owes
//! nothing to that API, so a shape is just parallel `Box<[_]>` columns.
//!
//! A C# `Shape` is always bracketed by a left and a right anchor node (`LeftSideAnchor` /
//! `RightSideAnchor`), with the segmented interior between them; we mirror that exactly so node
//! indices and counts line up with the managed engine for the §8 layer-1 segmentation gate.
//!
//! ## Scope note
//! M1 implemented the segmentation-relevant core: node kind + char-definition reference + flags,
//! an append `ShapeBuilder`, and the `ShapeId` interner. **M3 (this file) adds** the per-node
//! **feature matrix** (`W` symbolic `u64` lanes per node, plan §5.2/§5.3) and the positional
//! copy-on-write mutation ops (`insert`/`delete`/`modify`) that phonological rewrite rules need.
//!
//! ## Feature lanes (plan §5.2/§5.3)
//! Each node carries `W` inline `u64` lanes, stored SoA in one flat `feat_lanes` block: node `i`'s
//! lanes are `feat_lanes[i*W .. (i+1)*W]`. One lane is one symbolic feature's
//! `pg_featstruct::SymbolBits` set
//! (raw `u64`, so [`node_lanes`](Shape::node_lanes) feeds `pg_featstruct::flat_unifiable` with no
//! newtype friction). `W` (`feat_width`) is fixed per shape.
//!
//! These lanes are **inline-mutable storage, not derived from `char_def`**: feature-change
//! rewrite rules mutate a node's features away from any character definition, so a `char_def`-only
//! representation cannot express post-rule state. After a feature-change (`modify`), `char_def` is
//! deliberately **left stale** (it is the as-segmented character identity used for the display
//! signature); the live phonological state is the lanes, and the two intentionally diverge.
//!
//! Feature lanes are part of a shape's **identity**: two shapes with identical structure but
//! differing lanes intern to different `ShapeId`s.
//!
//! The default lane fill for anchors / no-lane pushes is `u64::MAX` ("unconstrained", matching
//! `flat_unifiable`'s treatment of an absent lane); real segment lanes come from callers. Do not
//! later split the notion of "unconstrained" between `u64::MAX` and `full_mask(count)`.
#![forbid(unsafe_code)]

use pg_featstruct::Interner;
use smallvec::SmallVec;

/// Sentinel `char_def` for anchor nodes (they reference no character definition).
pub const NO_CHAR_DEF: u32 = u32::MAX;

/// Per-table-sized bitset of character-definition ids (plan §13.1 Tier-1 #3's "char-def-set"
/// dimension, the port's analog of C#'s `StrRep` disjunction). **Not** `pg_featstruct::SymbolBits`:
/// that type is a fixed 8-byte/64-bit set for a different domain (feature-symbol values, verified
/// ≤37 across these grammars) with its own documented size invariant. A char-def table can be much
/// larger — Amharic has 418 `<SegmentDefinition>`s in one table — so this is a variable-length word
/// array, `SmallVec`-inlined for the common (≤64-member) case: Indonesian (30) and Sena (41) never
/// spill to the heap; only Amharic-scale tables (≥65 members) allocate.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CdBits(SmallVec<[u64; 1]>);

impl CdBits {
    pub fn empty() -> Self {
        CdBits(SmallVec::new())
    }

    pub fn insert(&mut self, id: u32) {
        let word = id as usize / 64;
        let bit = id as usize % 64;
        if self.0.len() <= word {
            self.0.resize(word + 1, 0);
        }
        self.0[word] |= 1u64 << bit;
    }

    pub fn contains(&self, id: u32) -> bool {
        let word = id as usize / 64;
        let bit = id as usize % 64;
        self.0.get(word).is_some_and(|w| w & (1u64 << bit) != 0)
    }

    /// Number of member ids in the set.
    pub fn count(&self) -> usize {
        self.0.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Build from an iterator of member ids (order-independent; duplicate ids fold harmlessly).
    pub fn from_ids(ids: impl IntoIterator<Item = u32>) -> Self {
        let mut s = CdBits::empty();
        for id in ids {
            s.insert(id);
        }
        s
    }

    /// The set as a single `u64` word, when every member id is `< 64` — the P10 identity-lane
    /// consumer (`pg_rules::morph::segs_of`), which only engages on tables of ≤ 64 char-defs.
    /// `None` when any id ≥ 64 is present (Amharic-scale tables), telling the caller to fall back
    /// to the unrestricted (pre-P10) behavior rather than silently truncate membership.
    pub fn as_u64(&self) -> Option<u64> {
        if self.0.iter().skip(1).any(|&w| w != 0) {
            return None;
        }
        Some(self.0.first().copied().unwrap_or(0))
    }

    /// Bitwise-AND (set intersection) with `other`. P11 §4.3: the guess matcher's
    /// `MatchNodesWithPattern` port unifies two abstract (`NO_CHAR_DEF`) nodes' `CdSet`s when
    /// both the analysis word's node and the lexical-pattern node are class-derived — the
    /// identity-dimension analog of `FeatureStruct.Unify` narrowing two symbolic lanes to their
    /// common values.
    pub fn intersect(&self, other: &CdBits) -> CdBits {
        let n = self.0.len().max(other.0.len());
        let mut out = SmallVec::with_capacity(n);
        for i in 0..n {
            let a = self.0.get(i).copied().unwrap_or(0);
            let b = other.0.get(i).copied().unwrap_or(0);
            out.push(a & b);
        }
        CdBits(out)
    }
}

/// The char-def-set identity of an underspecified (`char_def == NO_CHAR_DEF`) shape node —
/// the port's analog of C#'s `StrRep` disjunction (plan §13.1 Tier-1 #3). Concrete/segmented nodes
/// never need this stored explicitly: they derive an implicit singleton from their own `char_def`
/// (see `Shape::node_cd_set`), matching the convention `root_trie.rs` already documents for
/// lexical lookup. This type stores the explicit set only for nodes born from a natural-class
/// insertion (`InsertSimpleContext`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum CdSet {
    /// No membership restriction beyond phonological-lane unifiability — matches any table entry
    /// whose lanes unify. This is the historical (pre-fix) behavior for every `NO_CHAR_DEF` node,
    /// kept as the safe default for node producers this milestone did not touch (anchors; the
    /// zero-occurrence epenthesis/reduplication insertion paths) and as the "class truly means any
    /// segment" fast path (avoids materializing a full-table bitset for that common case).
    #[default]
    Unrestricted,
    /// Exactly these char-defs (`NaturalClassKind::Segments`' explicit member list, or a
    /// `NaturalClassKind::Feature` class's precomputed unifying set when it is a proper subset).
    Members(CdBits),
}

/// The *effective* char-def-set of a node, resolved at query time: concrete nodes (`char_def !=
/// NO_CHAR_DEF`) are an implicit singleton (their own identity, never stored); underspecified
/// nodes use their stored `CdSet`. Borrowed, not owned — built fresh by `Shape::node_cd_set`.
#[derive(Copy, Clone, Debug)]
pub enum EffectiveCdSet<'a> {
    Singleton(u32),
    Unrestricted,
    Members(&'a CdBits),
}

impl EffectiveCdSet<'_> {
    /// Is char-def `id` a member of this set?
    #[inline]
    pub fn contains(&self, id: u32) -> bool {
        match self {
            EffectiveCdSet::Singleton(s) => *s == id,
            EffectiveCdSet::Unrestricted => true,
            EffectiveCdSet::Members(b) => b.contains(id),
        }
    }
}

/// The kind of a shape node. `repr(u8)` so it packs into the SoA `kinds` column.
///
/// Mirrors the C# node "type" symbols (`HCFeatureSystem.LeftSideAnchor/RightSideAnchor`,
/// `Segment`, `Boundary`). Anchors bracket every shape; the interior is segments and boundaries.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
#[repr(u8)]
pub enum NodeKind {
    LeftAnchor = 0,
    RightAnchor = 1,
    Segment = 2,
    Boundary = 3,
}

/// Per-node flag bits (C# `Annotation.Optional` and the Kleene-star `Iterative` marker). Boundary
/// nodes are `OPTIONAL` after word segmentation; `ITERATIVE` is set only by pattern parsing.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default, PartialOrd, Ord)]
pub struct NodeFlags(pub u8);

impl NodeFlags {
    pub const EMPTY: NodeFlags = NodeFlags(0);
    pub const OPTIONAL: u8 = 0b0000_0001;
    pub const ITERATIVE: u8 = 0b0000_0010;

    #[inline]
    pub const fn is_optional(self) -> bool {
        self.0 & Self::OPTIONAL != 0
    }
    #[inline]
    pub const fn is_iterative(self) -> bool {
        self.0 & Self::ITERATIVE != 0
    }
    #[inline]
    pub fn set(&mut self, bit: u8) {
        self.0 |= bit;
    }
}

/// A frozen phonetic shape: parallel SoA columns, bracketed by anchor nodes. Cheap to clone as an
/// interned `ShapeId`; direct `Clone` here is a column copy (used only by the builder).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Shape {
    kinds: Box<[NodeKind]>,
    /// Index into the grammar's character-definition table, or `NO_CHAR_DEF` for anchors.
    char_defs: Box<[u32]>,
    flags: Box<[NodeFlags]>,
    /// Lanes-per-node (`W`). The `feat_lanes` block is `feat_width * len()` long.
    feat_width: u32,
    /// SoA feature matrix: node `i`'s lanes are `feat_lanes[i*feat_width .. (i+1)*feat_width]`.
    feat_lanes: Box<[u64]>,
    /// Explicit char-def-set per node (plan §13.1 Tier-1 #3), consulted only when `char_defs[i] ==
    /// NO_CHAR_DEF` — see `Shape::node_cd_set`. `CdSet::Unrestricted` for every node whose
    /// producer doesn't set one (the overwhelming majority: concrete nodes ignore this column
    /// entirely in favor of their own `char_def`).
    cd_sets: Box<[CdSet]>,
}

impl Shape {
    /// Number of nodes, **including the two anchors**.
    #[inline]
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    #[inline]
    pub fn kind(&self, i: usize) -> NodeKind {
        self.kinds[i]
    }

    #[inline]
    pub fn char_def(&self, i: usize) -> u32 {
        self.char_defs[i]
    }

    #[inline]
    pub fn flags(&self, i: usize) -> NodeFlags {
        self.flags[i]
    }

    /// Lanes-per-node (`W`). Zero for a shape built without a feature matrix.
    #[inline]
    pub fn feat_width(&self) -> u32 {
        self.feat_width
    }

    /// Node `i`'s feature lanes (`W` `u64`s), ready to pass to `pg_featstruct::flat_unifiable`.
    /// Empty slice when the shape has no feature matrix (`feat_width == 0`).
    #[inline]
    pub fn node_lanes(&self, i: usize) -> &[u64] {
        let w = self.feat_width as usize;
        &self.feat_lanes[i * w..i * w + w]
    }

    /// The effective char-def-set identity of node `i` (plan §13.1 Tier-1 #3): a concrete node
    /// (`char_def != NO_CHAR_DEF`) is an implicit singleton of its own identity; an underspecified
    /// node (natural-class insertion) uses its stored `CdSet`. See `EffectiveCdSet`.
    #[inline]
    pub fn node_cd_set(&self, i: usize) -> EffectiveCdSet<'_> {
        let cd = self.char_defs[i];
        if cd != NO_CHAR_DEF {
            return EffectiveCdSet::Singleton(cd);
        }
        match &self.cd_sets[i] {
            CdSet::Unrestricted => EffectiveCdSet::Unrestricted,
            CdSet::Members(b) => EffectiveCdSet::Members(b),
        }
    }

    /// The interior nodes (segments and boundaries), i.e. everything but the two anchors, as
    /// `(index, NodeKind, char_def, NodeFlags)`. This is the projection the segmentation gate
    /// compares against the managed engine.
    pub fn interior(&self) -> impl Iterator<Item = (usize, NodeKind, u32, NodeFlags)> + '_ {
        (0..self.len())
            .map(move |i| (i, self.kinds[i], self.char_defs[i], self.flags[i]))
            .filter(|&(_, k, _, _)| k != NodeKind::LeftAnchor && k != NodeKind::RightAnchor)
    }
}

/// Builds a `Shape`, either by **appending** interior nodes between the anchors then
/// [`finish`](Self::finish)ing, or by **copy-on-write mutation** of a frozen shape
/// ([`from_shape`](Self::from_shape) → `insert`/`delete`/`modify` → [`freeze`](Self::freeze)).
///
/// ## Two entry points, two finishers
/// - **Append path** (segmentation): [`new`](Self::new)/[`with_features`](Self::with_features)
///   start with the left anchor present; `push_*` append interior nodes; [`finish`](Self::finish)
///   appends the right anchor and freezes.
/// - **Mutation path** (rule RHS application, plan §5.2): [`from_shape`](Self::from_shape) memcpys
///   a frozen shape's SoA columns (**including both anchors**) into scratch `Vec`s, positional
///   `insert`/`delete`/`modify` edit them, and [`freeze`](Self::freeze) boxes them as-is. The copy
///   makes the builder fully independent of the source frozen shape (COW).
///
/// Both paths maintain the SoA invariant `feat_lanes.len() == feat_width * node_count`.
///
/// ## Positional-index contract
/// `insert`/`delete`/`modify` operate on the builder's **current** node positions, so indices
/// shift as you mutate (a delete at `i` moves everything after `i` down by one; an insert at `i`
/// moves everything from `i` up by one). A caller performing several deletes must therefore work
/// in descending index order or pre-collect the target positions — exactly as C#
/// `NarrowSynthesisRewriteSubruleSpec` snapshots `GetNodes(range).ToArray()` before deleting.
#[derive(Debug)]
pub struct ShapeBuilder {
    kinds: Vec<NodeKind>,
    char_defs: Vec<u32>,
    flags: Vec<NodeFlags>,
    feat_width: u32,
    feat_lanes: Vec<u64>,
    cd_sets: Vec<CdSet>,
}

impl ShapeBuilder {
    /// A new feature-less (`feat_width == 0`) builder containing just the left anchor.
    pub fn new() -> Self {
        Self::with_features(0)
    }

    /// A new feature-less builder with interior capacity reserved (anchors included).
    pub fn with_interior_capacity(cap: usize) -> Self {
        Self::with_features_capacity(0, cap)
    }

    /// A new builder with a `feat_width`-lane feature matrix, containing just the left anchor.
    pub fn with_features(feat_width: u32) -> Self {
        Self::with_features_capacity(feat_width, 0)
    }

    /// A new feature-matrix builder with interior capacity reserved (anchors included).
    pub fn with_features_capacity(feat_width: u32, cap: usize) -> Self {
        let w = feat_width as usize;
        let mut b = ShapeBuilder {
            kinds: Vec::with_capacity(cap + 2),
            char_defs: Vec::with_capacity(cap + 2),
            flags: Vec::with_capacity(cap + 2),
            feat_width,
            feat_lanes: Vec::with_capacity((cap + 2) * w),
            cd_sets: Vec::with_capacity(cap + 2),
        };
        b.push(NodeKind::LeftAnchor, NO_CHAR_DEF, NodeFlags::EMPTY);
        b
    }

    /// Start a copy-on-write mutation builder from a frozen shape: memcpy its SoA columns and
    /// feature lanes (both anchors included) into scratch. Independent of `src` after this call.
    pub fn from_shape(src: &Shape) -> Self {
        ShapeBuilder {
            kinds: src.kinds.to_vec(),
            char_defs: src.char_defs.to_vec(),
            flags: src.flags.to_vec(),
            feat_width: src.feat_width,
            feat_lanes: src.feat_lanes.to_vec(),
            cd_sets: src.cd_sets.to_vec(),
        }
    }

    /// Append a node with an explicit `feat_width`-long lane row.
    #[inline]
    fn push_with_lanes(&mut self, kind: NodeKind, char_def: u32, flags: NodeFlags, lanes: &[u64]) {
        assert_eq!(
            lanes.len(),
            self.feat_width as usize,
            "lane row width must equal feat_width"
        );
        self.kinds.push(kind);
        self.char_defs.push(char_def);
        self.flags.push(flags);
        self.feat_lanes.extend_from_slice(lanes);
        self.cd_sets.push(CdSet::Unrestricted);
    }

    /// Append a node, defaulting its `feat_width` lanes to `u64::MAX` ("unconstrained").
    #[inline]
    fn push(&mut self, kind: NodeKind, char_def: u32, flags: NodeFlags) {
        self.kinds.push(kind);
        self.char_defs.push(char_def);
        self.flags.push(flags);
        self.feat_lanes
            .resize(self.feat_lanes.len() + self.feat_width as usize, u64::MAX);
        self.cd_sets.push(CdSet::Unrestricted);
    }

    /// Append a segment node referencing character definition `char_def`, with default
    /// (unconstrained) lanes.
    pub fn push_segment(&mut self, char_def: u32) {
        self.push(NodeKind::Segment, char_def, NodeFlags::EMPTY);
    }

    /// Append a segment node with an explicit feature-lane row (`lanes.len()` must be `feat_width`).
    pub fn push_segment_with_lanes(&mut self, char_def: u32, lanes: &[u64]) {
        self.push_with_lanes(NodeKind::Segment, char_def, NodeFlags::EMPTY, lanes);
    }

    /// Append a boundary node. Boundaries are `OPTIONAL` after word segmentation (C#
    /// `node.Annotation.Optional = node.Type() == Boundary`). Lanes default to unconstrained.
    pub fn push_boundary(&mut self, char_def: u32) {
        self.push(NodeKind::Boundary, char_def, NodeFlags(NodeFlags::OPTIONAL));
    }

    /// Append a boundary node with an explicit feature-lane row.
    pub fn push_boundary_with_lanes(&mut self, char_def: u32, lanes: &[u64]) {
        self.push_with_lanes(
            NodeKind::Boundary,
            char_def,
            NodeFlags(NodeFlags::OPTIONAL),
            lanes,
        );
    }

    /// Append a `NO_CHAR_DEF` segment node with explicit lanes **and** an explicit char-def-set
    /// (plan §13.1 Tier-1 #3): the `InsertSimpleContext` insertion path, which must carry the
    /// natural class's real membership instead of the default `CdSet::Unrestricted`.
    pub fn push_segment_with_lanes_and_set(&mut self, lanes: &[u64], cd_set: CdSet) {
        self.push_with_lanes(NodeKind::Segment, NO_CHAR_DEF, NodeFlags::EMPTY, lanes);
        *self.cd_sets.last_mut().expect("just pushed") = cd_set;
    }

    /// OR `bits` into the most-recently-pushed node's flags (audit C finding N3: the
    /// `PhoneticShape` pattern language's optional-group `([Seg])` and Kleene-star `[Seg]*`
    /// syntax retroactively mark the *already-pushed* natural-class node, mirroring C#
    /// `nodesList[nodesList.Count - 1].Annotation.Optional`/`.SetIterative(true)`
    /// (`CharacterDefinitionTable.GetShapeNodes`, `CharacterDefinitionTable.cs:174-195`). No-op on
    /// an empty builder (defensive; callers only ever invoke this right after a successful push).
    pub fn set_last_flags(&mut self, bits: u8) {
        if let Some(f) = self.flags.last_mut() {
            f.set(bits);
        }
    }

    /// Number of nodes currently in the builder (**including anchors present**).
    #[inline]
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    /// Number of interior nodes appended so far (excludes the left anchor; append-path helper).
    pub fn interior_len(&self) -> usize {
        self.kinds.len() - 1
    }

    /// Insert a node at position `index`, shifting nodes at `index..` up by one (models C#
    /// `Shape.AddAfter`, the segment-insertion primitive of the epenthesis/narrow rewrite specs).
    ///
    /// `index` must land the new node strictly between the anchors (`1..=len-1`); `lanes.len()`
    /// must equal `feat_width`.
    pub fn insert(
        &mut self,
        index: usize,
        kind: NodeKind,
        char_def: u32,
        flags: NodeFlags,
        lanes: &[u64],
    ) {
        assert_eq!(
            lanes.len(),
            self.feat_width as usize,
            "lane row width must equal feat_width"
        );
        assert!(
            index >= 1 && index < self.kinds.len(),
            "insert index must be interior (between the anchors)"
        );
        self.kinds.insert(index, kind);
        self.char_defs.insert(index, char_def);
        self.flags.insert(index, flags);
        let w = self.feat_width as usize;
        for (k, &v) in lanes.iter().enumerate() {
            self.feat_lanes.insert(index * w + k, v);
        }
        self.cd_sets.insert(index, CdSet::Unrestricted);
    }

    /// [`insert`](Self::insert) with an explicit char-def-set on the new (always `NO_CHAR_DEF`,
    /// `Segment`) node — the counterpart of
    /// [`push_segment_with_lanes_and_set`](Self::push_segment_with_lanes_and_set) for the
    /// positional-mutation path (plan §13.1 Tier-1 #3).
    pub fn insert_with_set(
        &mut self,
        index: usize,
        flags: NodeFlags,
        lanes: &[u64],
        cd_set: CdSet,
    ) {
        self.insert(index, NodeKind::Segment, NO_CHAR_DEF, flags, lanes);
        self.cd_sets[index] = cd_set;
    }

    /// Delete the node at position `index`, shifting nodes after it down by one (models C#
    /// `Shape.Remove` / the narrow rewrite spec's target deletion). `index` must be interior
    /// (not an anchor).
    pub fn delete(&mut self, index: usize) {
        assert!(
            self.kinds[index] != NodeKind::LeftAnchor && self.kinds[index] != NodeKind::RightAnchor,
            "cannot delete an anchor node"
        );
        self.kinds.remove(index);
        self.char_defs.remove(index);
        self.flags.remove(index);
        let w = self.feat_width as usize;
        // Draining from the tail forward keeps the remaining indices valid across removals.
        for k in (0..w).rev() {
            self.feat_lanes.remove(index * w + k);
        }
        self.cd_sets.remove(index);
    }

    /// Feature-change: replace node `index`'s lanes with `lanes` (`lanes.len()` must be
    /// `feat_width`). Models the priority-union RHS of C# `FeatureSynthesisRewriteSubruleSpec`:
    /// pg-shape is pure storage, so the **caller** computes the post-union lanes (reading
    /// `Shape::node_lanes`, unifying with `pg_featstruct` ops + the grammar's per-feature masks)
    /// and hands in the full resulting row. `char_def` is deliberately **left unchanged** — it
    /// remains the as-segmented display identity even though the live features now diverge from it.
    /// The node's `CdSet` (plan §13.1 Tier-1 #3) is likewise **left untouched** by design: a
    /// feature-changing rule narrows which lanes are pinned, not which literal characters/segments
    /// are being talked about (C#'s `StrRep` is a distinct FS slot a plain feature `PriorityUnion`
    /// never touches) — this method simply never writes `cd_sets[index]`, so the caller gets
    /// carry-forward for free.
    pub fn modify(&mut self, index: usize, lanes: &[u64]) {
        let w = self.feat_width as usize;
        assert_eq!(lanes.len(), w, "lane row width must equal feat_width");
        assert!(
            self.kinds[index] != NodeKind::LeftAnchor && self.kinds[index] != NodeKind::RightAnchor,
            "cannot modify features of an anchor node"
        );
        self.feat_lanes[index * w..index * w + w].copy_from_slice(lanes);
    }

    /// Read node `index`'s current lanes (mutation-path inspection helper).
    #[inline]
    pub fn node_lanes(&self, index: usize) -> &[u64] {
        let w = self.feat_width as usize;
        &self.feat_lanes[index * w..index * w + w]
    }

    /// Append the right anchor and freeze into an immutable `Shape` (append path).
    pub fn finish(mut self) -> Shape {
        self.push(NodeKind::RightAnchor, NO_CHAR_DEF, NodeFlags::EMPTY);
        self.into_shape()
    }

    /// Freeze the builder's current nodes as-is into an immutable `Shape` (mutation path — the
    /// anchors are already present from [`from_shape`](Self::from_shape)).
    pub fn freeze(self) -> Shape {
        debug_assert_eq!(self.kinds.first(), Some(&NodeKind::LeftAnchor));
        debug_assert_eq!(self.kinds.last(), Some(&NodeKind::RightAnchor));
        self.into_shape()
    }

    fn into_shape(self) -> Shape {
        debug_assert_eq!(
            self.feat_lanes.len(),
            self.kinds.len() * self.feat_width as usize
        );
        debug_assert_eq!(self.cd_sets.len(), self.kinds.len());
        Shape {
            kinds: self.kinds.into_boxed_slice(),
            char_defs: self.char_defs.into_boxed_slice(),
            flags: self.flags.into_boxed_slice(),
            feat_width: self.feat_width,
            feat_lanes: self.feat_lanes.into_boxed_slice(),
            cd_sets: self.cd_sets.into_boxed_slice(),
        }
    }
}

impl Default for ShapeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-parse identity of a frozen, interned shape (plan §5.2). Clone = copy this id; memo-key
/// shape equality = integer compare.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct ShapeId(pub u32);

/// Hash-cons interner for shapes (per-parse scope; lives in the parse arena, plan §6.2). Reuses
/// the generic `Interner` from `pg-featstruct` but hands back a distinct `ShapeId` type.
#[derive(Debug, Default, Clone)]
pub struct ShapeInterner {
    inner: Interner<Shape>,
}

impl ShapeInterner {
    pub fn new() -> Self {
        ShapeInterner {
            inner: Interner::new(),
        }
    }

    /// Intern a frozen shape; structurally-equal shapes get the same id.
    pub fn intern(&mut self, shape: Shape) -> ShapeId {
        ShapeId(self.inner.intern(shape).0)
    }

    /// The shape behind an id.
    pub fn get(&self, id: ShapeId) -> &Shape {
        self.inner.get(pg_featstruct::FsId(id.0))
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

// Hot-struct size discipline (plan §9): ids are copied/compared in the memo and traversal hot
// paths. `NodeKind`/`NodeFlags` pack to one byte each so the SoA columns stay cache-dense.
const _: () = assert!(std::mem::size_of::<ShapeId>() == 4);
const _: () = assert!(std::mem::size_of::<NodeKind>() == 1);
const _: () = assert!(std::mem::size_of::<NodeFlags>() == 1);

#[cfg(test)]
mod tests {
    use super::*;

    fn build_cvc() -> Shape {
        // A tiny "shape" of three segments (char-def ids 10, 11, 10) with no boundaries.
        let mut b = ShapeBuilder::with_interior_capacity(3);
        b.push_segment(10);
        b.push_segment(11);
        b.push_segment(10);
        b.finish()
    }

    #[test]
    fn cdbits_intersect_keeps_only_shared_members() {
        let a = CdBits::from_ids([1, 2, 3, 70]); // 70 forces the SmallVec to spill to a 2nd word.
        let b = CdBits::from_ids([2, 3, 4, 70]);
        let i = a.intersect(&b);
        assert_eq!(i.count(), 3);
        assert!(i.contains(2) && i.contains(3) && i.contains(70));
        assert!(!i.contains(1) && !i.contains(4));
    }

    #[test]
    fn cdbits_intersect_of_disjoint_sets_is_empty() {
        let a = CdBits::from_ids([1, 2]);
        let b = CdBits::from_ids([3, 4]);
        assert_eq!(a.intersect(&b).count(), 0);
    }

    #[test]
    fn anchors_bracket_the_interior() {
        let s = build_cvc();
        assert_eq!(s.len(), 5); // left anchor + 3 segments + right anchor
        assert_eq!(s.kind(0), NodeKind::LeftAnchor);
        assert_eq!(s.kind(4), NodeKind::RightAnchor);
        assert_eq!(s.char_def(0), NO_CHAR_DEF);
        assert_eq!(s.char_def(4), NO_CHAR_DEF);
        let interior: Vec<_> = s.interior().map(|(_, k, cd, _)| (k, cd)).collect();
        assert_eq!(
            interior,
            vec![
                (NodeKind::Segment, 10),
                (NodeKind::Segment, 11),
                (NodeKind::Segment, 10),
            ]
        );
    }

    #[test]
    fn boundaries_are_optional() {
        let mut b = ShapeBuilder::new();
        b.push_segment(1);
        b.push_boundary(99);
        b.push_segment(2);
        let s = b.finish();
        let flags: Vec<_> = s
            .interior()
            .map(|(_, k, _, f)| (k, f.is_optional()))
            .collect();
        assert_eq!(
            flags,
            vec![
                (NodeKind::Segment, false),
                (NodeKind::Boundary, true),
                (NodeKind::Segment, false),
            ]
        );
    }

    #[test]
    fn interner_dedups_equal_shapes() {
        let mut it = ShapeInterner::new();
        let a = it.intern(build_cvc());
        let b = it.intern(build_cvc());
        assert_eq!(a, b);
        assert_eq!(it.len(), 1);
        assert_eq!(it.get(a).len(), 5);

        let mut bld = ShapeBuilder::new();
        bld.push_segment(10);
        let c = it.intern(bld.finish());
        assert_ne!(a, c);
        assert_eq!(it.len(), 2);
    }

    // --- M3: feature lanes + positional COW mutation -------------------------------------------

    /// Build a two-segment shape with a `W`-lane feature matrix from explicit lane rows.
    fn build_with_lanes(w: u32, s0: &[u64], s1: &[u64]) -> Shape {
        let mut b = ShapeBuilder::with_features_capacity(w, 2);
        b.push_segment_with_lanes(10, s0);
        b.push_segment_with_lanes(11, s1);
        b.finish()
    }

    #[test]
    fn feature_lanes_round_trip_and_default_fill() {
        let s = build_with_lanes(2, &[0b0011, 0b0101], &[0b1000, 0b0001]);
        // len = LA + 2 segments + RA; feat_lanes = 4 nodes * 2 lanes.
        assert_eq!(s.len(), 4);
        assert_eq!(s.feat_width(), 2);
        assert_eq!(s.node_lanes(1), &[0b0011, 0b0101]); // segment 0
        assert_eq!(s.node_lanes(2), &[0b1000, 0b0001]); // segment 1
                                                        // Anchors got the default unconstrained fill.
        assert_eq!(s.node_lanes(0), &[u64::MAX, u64::MAX]);
        assert_eq!(s.node_lanes(3), &[u64::MAX, u64::MAX]);
        // A feature-less shape has width 0 and empty lane rows (back-compat with the append path).
        assert_eq!(build_cvc().feat_width(), 0);
        assert_eq!(build_cvc().node_lanes(1), &[] as &[u64]);
    }

    #[test]
    fn feature_lanes_are_part_of_identity() {
        let mut it = ShapeInterner::new();
        let a = it.intern(build_with_lanes(2, &[0b0011, 0b0101], &[0b1000, 0b0001]));
        // Identical structure AND lanes -> same id.
        let b = it.intern(build_with_lanes(2, &[0b0011, 0b0101], &[0b1000, 0b0001]));
        assert_eq!(a, b);
        assert_eq!(it.len(), 1);
        // Same structure/char_defs but one differing lane -> different id.
        let c = it.intern(build_with_lanes(2, &[0b0011, 0b0101], &[0b1000, 0b0010]));
        assert_ne!(a, c);
        assert_eq!(it.len(), 2);
    }

    #[test]
    fn insert_segment_mid_shape() {
        // Start: LA, seg(10)[lanes a], seg(11)[lanes b], RA  (indices 0..=3)
        let base = build_with_lanes(2, &[0b0001, 0b0001], &[0b0010, 0b0010]);
        let mut m = ShapeBuilder::from_shape(&base);
        // Insert a new segment at index 2 (between the two segments; epenthesis-style AddAfter).
        m.insert(
            2,
            NodeKind::Segment,
            99,
            NodeFlags::EMPTY,
            &[0b0100, 0b1000],
        );
        let s = m.freeze();
        assert_eq!(s.len(), 5); // one more node
        let interior: Vec<_> = s.interior().map(|(_, k, cd, _)| (k, cd)).collect();
        assert_eq!(
            interior,
            vec![
                (NodeKind::Segment, 10),
                (NodeKind::Segment, 99),
                (NodeKind::Segment, 11),
            ]
        );
        assert_eq!(s.node_lanes(1), &[0b0001, 0b0001]); // original seg 10
        assert_eq!(s.node_lanes(2), &[0b0100, 0b1000]); // inserted seg 99
        assert_eq!(s.node_lanes(3), &[0b0010, 0b0010]); // shifted seg 11
        assert_eq!(s.kind(4), NodeKind::RightAnchor); // anchor shifted to the tail
    }

    #[test]
    fn delete_node_shifts_positions() {
        // LA, seg10, seg11, seg12, RA
        let mut b = ShapeBuilder::with_features_capacity(1, 3);
        b.push_segment_with_lanes(10, &[0b001]);
        b.push_segment_with_lanes(11, &[0b010]);
        b.push_segment_with_lanes(12, &[0b100]);
        let base = b.finish();
        let mut m = ShapeBuilder::from_shape(&base);
        // Delete the middle segment (index 2). Everything after shifts down by one.
        m.delete(2);
        let s = m.freeze();
        assert_eq!(s.len(), 4);
        let interior: Vec<_> = s.interior().map(|(_, _, cd, _)| cd).collect();
        assert_eq!(interior, vec![10, 12]);
        assert_eq!(s.node_lanes(1), &[0b001]);
        assert_eq!(s.node_lanes(2), &[0b100]); // seg12's lanes moved down with it
    }

    #[test]
    fn delete_two_nodes_descending_index() {
        // Deleting several nodes must go descending-index (the documented shift contract).
        let mut b = ShapeBuilder::with_features_capacity(1, 4);
        for (cd, lane) in [(10, 0b0001), (11, 0b0010), (12, 0b0100), (13, 0b1000)] {
            b.push_segment_with_lanes(cd, &[lane]);
        }
        let base = b.finish(); // LA,10,11,12,13,RA  (indices 1..=4)
        let mut m = ShapeBuilder::from_shape(&base);
        // Remove seg11 (idx 2) and seg13 (idx 4): process the higher index first.
        m.delete(4);
        m.delete(2);
        let s = m.freeze();
        let interior: Vec<_> = s.interior().map(|(_, _, cd, _)| cd).collect();
        assert_eq!(interior, vec![10, 12]);
        assert_eq!(s.node_lanes(1), &[0b0001]);
        assert_eq!(s.node_lanes(2), &[0b0100]);
    }

    #[test]
    fn modify_changes_lanes_but_preserves_char_def() {
        let base = build_with_lanes(2, &[0b0011, 0b0101], &[0b1000, 0b0001]);
        let mut m = ShapeBuilder::from_shape(&base);
        // Feature-change on segment 0 (index 1): caller hands in the post-union lane row.
        m.modify(1, &[0b0001, 0b0100]);
        let s = m.freeze();
        assert_eq!(s.node_lanes(1), &[0b0001, 0b0100]); // lanes changed
        assert_eq!(s.char_def(1), 10); // char_def deliberately unchanged (stale display identity)
        assert_eq!(s.node_lanes(2), &[0b1000, 0b0001]); // other node untouched
    }

    #[test]
    fn mutation_is_copy_on_write_independent_of_source() {
        let base = build_with_lanes(2, &[0b0011, 0b0101], &[0b1000, 0b0001]);
        let mut m = ShapeBuilder::from_shape(&base);
        m.modify(1, &[0b0001, 0b0001]);
        m.delete(2);
        m.insert(
            2,
            NodeKind::Segment,
            77,
            NodeFlags::EMPTY,
            &[0b1111, 0b1111],
        );
        let mutated = m.freeze();
        // The original frozen shape is completely unaffected by the builder's edits.
        assert_eq!(base.len(), 4);
        assert_eq!(base.node_lanes(1), &[0b0011, 0b0101]);
        assert_eq!(base.node_lanes(2), &[0b1000, 0b0001]);
        assert_eq!(base.char_def(2), 11);
        // ...and the mutated shape reflects them.
        assert_ne!(mutated, base);
        assert_eq!(mutated.node_lanes(1), &[0b0001, 0b0001]);
        assert_eq!(mutated.char_def(2), 77);
    }

    #[test]
    fn frozen_build_mutate_freeze_columns_round_trip() {
        // Full round trip: frozen -> from_shape -> mutate -> freeze -> assert every column.
        let base = build_with_lanes(1, &[0b01], &[0b10]);
        let mut m = ShapeBuilder::from_shape(&base);
        m.insert(
            2,
            NodeKind::Boundary,
            5,
            NodeFlags(NodeFlags::OPTIONAL),
            &[u64::MAX],
        );
        let s = m.freeze();
        let cols: Vec<_> = (0..s.len())
            .map(|i| {
                (
                    s.kind(i),
                    s.char_def(i),
                    s.flags(i).is_optional(),
                    s.node_lanes(i)[0],
                )
            })
            .collect();
        assert_eq!(
            cols,
            vec![
                (NodeKind::LeftAnchor, NO_CHAR_DEF, false, u64::MAX),
                (NodeKind::Segment, 10, false, 0b01),
                (NodeKind::Boundary, 5, true, u64::MAX),
                (NodeKind::Segment, 11, false, 0b10),
                (NodeKind::RightAnchor, NO_CHAR_DEF, false, u64::MAX),
            ]
        );
    }
}
