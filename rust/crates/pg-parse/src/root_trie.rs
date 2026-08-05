//! Root-allomorph lexical-lookup trie (plan §5.1 lexical lookup; M5a).
//!
//! Faithful behavioral port of C# `RootAllomorphTrie` (`RootAllomorphTrie.cs`) and
//! `Morpher.SearchRootAllomorphs` (`Morpher.cs:343-347`) + the per-stratum trie construction
//! (`Morpher.cs:35-48`). Each stratum indexes all of its root allomorphs' segment shapes into a
//! trie; [`RootAllomorphTrie::search`] walks an input shape's segment nodes through the trie by
//! **feature unification** (not string identity) and yields every matching root allomorph.
//!
//! ## Why a direct trie and not `pg-fst`
//! Two independent facts steer the design onto a hand-built trie rather than the frozen `pg-fst`
//! engine:
//!
//! 1. **Multi-accept-id.** The C# trie is one FSA with *many* accepting states, each carrying its
//!    own allomorph id (`RootAllomorphTrie.cs:31,50-52,59`). `pg_fst::CompileInput` compiles one
//!    pattern to one FSA with a *single* `accept_id` (`compile.rs:58`); it cannot express a
//!    1371-path trie whose every leaf carries a distinct root id, and `pg-fst` is a frozen contract
//!    (no edits).
//! 2. **Segment discrimination without phonological features.** In C# the trie arc condition is a
//!    shape node's *full* `FeatureStruct`. `CharacterDefinitionTable.Add` (`CharacterDefinitionTable
//!    .cs:68-81`) attaches `StrRep` **only on the `fs == null` branch** — i.e. only when the segment
//!    authors zero phonological `<FeatureValue>`s (Sena has no `<PhonologicalFeatureSystem>` at all;
//!    `XmlLanguageLoader.cs:670-673` passes a non-null fs whenever the feature system is non-empty).
//!    Sena's every segment lands in that branch, so its phonological lanes are empty and
//!    [`pg_featstruct::flat_unifiable`] on two empty rows is trivially `true`. A lane-only trie would
//!    therefore merge *all* Sena roots onto one path and return every equal-length root. The Rust
//!    engine does not model `StrRep`; its faithful analog for a zero-feature grammar is the node's
//!    `char_def` (a [`pg_grammar::chardef::CharDefId`], whose representations are unique per table).
//!    For a feature-bearing grammar (Indonesian, Amharic, the C# test fixtures) C# attaches **no**
//!    `StrRep` at all, so two distinct char-defs whose feature structs unify legitimately cross-match
//!    lexical lookup in C#. `edge_matches`'s build-time
//!    [`pg_grammar::chardef::CharDefTable::unifiable_cds`] closure (Design A) is the equality-miss
//!    fallback that restores this: a bitset probe, consulted only when `char_def` equality itself
//!    misses, and entirely absent (`None`) for a zero-feature table so Sena/en/sp pay nothing and
//!    keep the pre-P5 identity-only behavior bit-for-bit.
//!
//! So the match predicate is the exact port of C#'s full-`FeatureStruct` unification for concrete
//! segments: **`char_def` equality, OR closure membership when the table has one (the `StrRep`
//! analog), AND [`pg_featstruct::flat_unifiable`] on the phonological lanes.** For a zero-phon-feature
//! grammar (Sena) this reduces to char-def identity (no closure exists); for a phon-feature grammar
//! (Indonesian, Amharic) identity is still the fast path but a closure-eligible cross-table/cross-
//! char-def pair is no longer rejected outright. The trie is keyed on `char_def`, mirroring C#'s
//! `ValueEquals` arc-grouping over the full FS (`RootAllomorphTrie.cs:39-40`).
//!
//! ## Filter
//! Build and search both range over **`Segment` nodes only** (skip boundaries and anchors),
//! matching the C# filter `ann.Type() == HCFeatureSystem.Segment` (`Morpher.cs:40`).
//!
//! ## M5b invariants introduced by the `char_def` key (C#'s `StrRep` did not have these)
//! `char_def` ids are **per-table** ([`pg_grammar::chardef::CharDefId`] is a dense per-table
//! identity), whereas C#'s `StrRep` is a table-independent string. Two consequences the pipeline
//! must uphold:
//! - **Same-table segmentation.** [`RootAllomorphTrie::search`] requires the input shape to be
//!   segmented against the *same stratum's character-definition table* the trie was built from.
//!   (For Sena and Indonesian all strata share `table1`, so this holds trivially; a
//!   multi-table grammar must route each shape to its stratum's trie.)
//! - **Stale `char_def` after feature-change.** `pg_shape::ShapeBuilder::modify` deliberately leaves
//!   a node's `char_def` as the *as-segmented* identity even when a phonological rule mutated its
//!   lanes. The as-segmented `char_def` is the correct `StrRep` analog for lexical lookup (lookup
//!   happens on the unapplied stem whose segments retain their surface char-defs); this is called
//!   out so M5b does not mistake it for a bug.
//!
//! ## Capability gap flagged (do not hack)
//! `pg-fst` arcs match purely on lane unifiability with no `StrRep`/char-def dimension
//! (`fst.rs` `MatchInput::matches`). It cannot express the root trie's matching for a
//! zero-phon-feature grammar. This is recorded here as the reason the trie is built directly rather
//! than on the frozen FSA engine; `pg-fst` itself is left untouched.

use pg_featstruct::flat_unifiable;
use pg_grammar::chardef::{CharDefId, CharDefTable};
use pg_grammar::model::{AllomorphId, Grammar, LexEntryId, StratumId, TableId};
use pg_shape::{CdBits, CdSet, EffectiveCdSet, NodeKind, Shape, NO_CHAR_DEF};

/// A trie node: outgoing edges (keyed by `char_def`) and the root allomorphs that accept here.
#[derive(Debug, Default)]
struct TrieNode {
    edges: Vec<TrieEdge>,
    /// `(allomorph, owning entry)` pairs accepted at this node (homographs accumulate here, exactly
    /// as C# `AcceptInfos` accumulate on a shared accepting state, `RootAllomorphTrie.cs:50-52`).
    accepts: Vec<(AllomorphId, LexEntryId)>,
}

/// A trie edge: the segment's `char_def` (the `StrRep` analog) plus its phonological feature lanes
/// (for the unifiability refinement), and the target node.
///
/// Wave-4 (loader N3 end-to-end): a **pattern-derived** root-allomorph node (`[NatClass]` in a
/// `<PhoneticShape>`, loaded by `pg_grammar::segment::segment_with_patterns`) has
/// `char_def == NO_CHAR_DEF` and carries its natural class's member set as a [`CdSet`] instead.
/// Such an edge stores that set here; matching a concrete query segment against it goes by **set
/// membership** (the `StrRep`-compatibility analog: the set was precomputed as exactly the table
/// entries the class unifies with — `pg_grammar::segment`'s `nat_class_cd_set`) plus lane
/// unifiability, which together are this port's rendering of C#'s arc-condition `FeatureStruct`
/// unification against a class-only (no-`StrRep`) condition (`RootAllomorphTrie.cs:39-40,61-63` +
/// the FSA's `UseUnification = true`).
///
/// Stored-node `OPTIONAL`/`ITERATIVE` flags (`([NatClass])` / `[NatClass]*`) are deliberately NOT
/// modeled: C#'s own `AddNode` creates one unconditional arc per shape node — the node's
/// `Annotation.Optional`/iterative marks never reach the arc condition (a frozen `FeatureStruct`
/// clone, `RootAllomorphTrie.cs:61-63`) — so a stored optional class node is mandatory in the C#
/// trie too. Matching that (mis)behavior exactly is the parity-faithful choice. P11 chunk 2: this
/// scenario cannot actually arise for a trie-indexed allomorph any more — any node carrying
/// `ITERATIVE`, or `OPTIONAL` on a non-boundary node, makes the *whole allomorph* `IsPattern`
/// (`RootAllomorphDef::is_pattern`), and `RootAllomorphTrie::build` now excludes pattern allomorphs
/// from the trie entirely (see that function's doc). A `Segment` node with these flags reaching
/// this edge type is therefore unreachable post-fix; the paragraph above records the historical
/// C# behavior this port would replicate *if* such a node ever did reach here (defense in depth,
/// not a live path).
#[derive(Debug)]
struct TrieEdge {
    char_def: u32,
    lanes: Vec<u64>,
    /// The stored node's char-def-set — consulted only when `char_def == NO_CHAR_DEF` (a concrete
    /// edge is an implicit singleton of its own `char_def`, mirroring `Shape::node_cd_set`).
    cd_set: CdSet,
    target: usize,
}

/// One stratum's root-allomorph trie (C# `RootAllomorphTrie`). Built once (mirroring C# `Morpher`'s
/// `_allomorphTries` dictionary, `Morpher.cs:35-48`) and searched many times.
#[derive(Debug)]
pub struct RootAllomorphTrie {
    nodes: Vec<TrieNode>,
    /// The stratum's character-definition table (used to resolve input `char_def`s to lanes).
    table: TableId,
    /// Phonological feature width (`grammar.phon_features.len()`).
    feat_width: usize,
    /// Number of root allomorphs indexed (C# `_shapeCount`).
    allomorph_count: usize,
}

impl RootAllomorphTrie {
    /// Build the trie for one stratum: index every **non-pattern** root allomorph of every
    /// lexical entry the stratum owns (`Morpher.cs:39-47`). P11 chunk 2: `IsPattern` allomorphs
    /// (`Morpher.cs:43-44`) are diverted into `Morpher.lexical_patterns`
    /// ([`collect_lexical_patterns`]) instead — mirroring C#'s partition exactly, rather than the
    /// prior (wrong) "index everything" placeholder this module's doc used to carry. Before this
    /// fix, a lexical-pattern entry (e.g. a bare `[Any]*` root) fell through to a single
    /// mandatory unrestricted trie edge and could match any one-segment word in ordinary
    /// (guess-off) lexical lookup — a real divergence from C#, which never trie-indexes a pattern
    /// allomorph at all.
    pub fn build(grammar: &Grammar, stratum: StratumId) -> RootAllomorphTrie {
        let sd = &grammar.strata[stratum.0 as usize];
        let table = sd.table;
        let table_ref = &grammar.char_tables[table.0 as usize];
        let feat_width = grammar.phon_features.len();

        let mut trie = RootAllomorphTrie {
            nodes: vec![TrieNode::default()], // root = index 0
            table,
            feat_width,
            allomorph_count: 0,
        };

        for &le_id in &sd.entries {
            let entry = &grammar.entries[le_id.0 as usize];
            for allo in &entry.allomorphs {
                if allo.is_pattern {
                    continue;
                }
                let segs = shape_segments(&allo.shape.shape, table_ref, feat_width);
                trie.add_path(&segs, allo.id, le_id);
            }
        }
        trie
    }

    /// Number of root allomorphs indexed (C# `_shapeCount`).
    #[inline]
    pub fn allomorph_count(&self) -> usize {
        self.allomorph_count
    }

    /// Number of trie nodes (structural introspection for tests/reports).
    #[inline]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Add one root allomorph path. `segs` is the pre-resolved `(char_def, lanes, cd_set)` sequence
    /// of the allomorph's `Segment` nodes. Edges are grouped by `char_def` so shared prefixes share
    /// a path (C# `AddNode` arc reuse via `ValueEquals`, `RootAllomorphTrie.cs:39-63`); a
    /// pattern-derived (`NO_CHAR_DEF`) node additionally requires `cd_set` + lane equality to reuse
    /// an edge — the analog of `ValueEquals` distinguishing two different class conditions (two
    /// concrete nodes with the same `char_def` always resolve identical lanes from the table, so
    /// the `char_def`-only key remains exact for them).
    fn add_path(&mut self, segs: &[(u32, Vec<u64>, CdSet)], allo: AllomorphId, entry: LexEntryId) {
        let mut cur = 0usize;
        for (cd, lanes, cd_set) in segs {
            let found = self.nodes[cur].edges.iter().position(|e| {
                e.char_def == *cd
                    && (*cd != NO_CHAR_DEF || (e.cd_set == *cd_set && e.lanes == *lanes))
            });
            cur = match found {
                Some(ei) => self.nodes[cur].edges[ei].target,
                None => {
                    let target = self.nodes.len();
                    self.nodes.push(TrieNode::default());
                    self.nodes[cur].edges.push(TrieEdge {
                        char_def: *cd,
                        lanes: lanes.clone(),
                        cd_set: cd_set.clone(),
                        target,
                    });
                    target
                }
            };
        }
        self.nodes[cur].accepts.push((allo, entry));
        self.allomorph_count += 1;
    }

    /// Search an input shape for matching root allomorphs (C# `RootAllomorphTrie.Search` +
    /// `Morpher.SearchRootAllomorphs`, anchored to start *and* end so the whole segment sequence is
    /// consumed — `Transduce(.., startAnchor: true, endAnchor: true, ..)`, `RootAllomorphTrie.cs:74`).
    /// Returns the distinct `(allomorph, owning entry)` pairs (C# `.Distinct()`, `Morpher.cs:346`).
    pub fn search(&self, grammar: &Grammar, shape: &Shape) -> Vec<(AllomorphId, LexEntryId)> {
        let table_ref = &grammar.char_tables[self.table.0 as usize];
        let segs = shape_search_segments(shape, table_ref, self.feat_width);
        self.search_segs_opt(&segs, table_ref.unif_closure_rows())
    }

    /// Core search over a pre-resolved input `(char_def, lanes)` segment sequence, **all segments
    /// mandatory** (the internal-unit-test entry point). Delegates to [`Self::search_segs_opt`]
    /// with `closure = None` — every existing unit test exercises the Sena-regime (no closure)
    /// behavior; the P5 closure-aware unit tests below pass a closure explicitly instead.
    #[cfg(test)]
    fn search_segs(&self, segs: &[(u32, Vec<u64>)]) -> Vec<(AllomorphId, LexEntryId)> {
        let with_opt: Vec<(u32, Vec<u64>, bool)> =
            segs.iter().map(|(cd, l)| (*cd, l.clone(), false)).collect();
        self.search_segs_opt(&with_opt, None)
    }

    /// Same as [`Self::search_segs`], but threading an explicit closure (P5, §7.1's unit tests) —
    /// lets a test exercise the equality-miss fallback without needing a real `CharDefTable`.
    #[cfg(test)]
    fn search_segs_with_closure(
        &self,
        segs: &[(u32, Vec<u64>)],
        closure: Option<&[CdBits]>,
    ) -> Vec<(AllomorphId, LexEntryId)> {
        let with_opt: Vec<(u32, Vec<u64>, bool)> =
            segs.iter().map(|(cd, l)| (*cd, l.clone(), false)).collect();
        self.search_segs_opt(&with_opt, closure)
    }

    /// Does this edge accept a query segment `(cd, lanes)`? The match predicate of the whole trie
    /// (see module doc): for a **concrete** edge, `char_def` equality (the `StrRep` analog) OR —
    /// Design A, P5 — membership of `cd` in the edge char-def's build-time unifiability closure
    /// (`closure`, `None` for a zero-feature table: Sena/en/sp keep the pre-P5 identity-only
    /// behavior bit-for-bit); for a **pattern-derived** edge (`char_def == NO_CHAR_DEF`, wave-4 /
    /// loader N3 end-to-end), membership of the query's `char_def` in the edge's stored [`CdSet`]
    /// (precomputed as exactly the table entries the class unifies with). A `NO_CHAR_DEF` **query**
    /// segment has no literal identity to compare and passes the char-def gate against *any* edge
    /// (its features still gate below) — including a pattern edge, where C# would unify the
    /// reinserted node's class-features against the stored class-features; this port's pattern
    /// edges carry their class identity in `cd_set` (their `lanes` are empty ⇒ trivially unifiable),
    /// so this is an over-approximation for that (query-pattern × edge-pattern) corner only — safe,
    /// because every trie hit is re-verified by synthesis-confirm downstream. In every case the
    /// phonological-lane unifiability refinement also applies, so a closure hit that fails the lane
    /// conjunct (a query carrying divergent shape lanes) still correctly rejects.
    fn edge_matches(e: &TrieEdge, cd: u32, lanes: &[u64], closure: Option<&[CdBits]>) -> bool {
        let cd_ok = cd == NO_CHAR_DEF
            || e.char_def == cd
            || (e.char_def != NO_CHAR_DEF
                && closure.is_some_and(|c| c[e.char_def as usize].contains(cd)))
            || (e.char_def == NO_CHAR_DEF
                && match &e.cd_set {
                    CdSet::Unrestricted => true,
                    CdSet::Members(b) => b.contains(cd),
                });
        cd_ok && flat_unifiable(lanes, &e.lanes)
    }

    /// Core search over a pre-resolved input `(char_def, lanes, optional)` segment sequence. Follows
    /// **all** edges that match the current input segment (`char_def` equal *and* lanes unifiable),
    /// preserving C#'s nondeterministic unification traversal; in practice the char-def key makes at
    /// most one edge per node match a concrete segment.
    ///
    /// A query segment whose `char_def` is [`NO_CHAR_DEF`] has no known literal identity — this is
    /// this port's analog of a C# node whose `StrRep` feature is left unspecified, which happens for
    /// material an analysis-side phonological rule re-inserted from a **natural-class-only** LHS
    /// (e.g. Indonesian `prule5`'s "Voiceless obstruent deletion" reinstates a deleted segment typed
    /// only as `nc13`/"voiceless obstruent", not as any specific literal phoneme —
    /// `pg_rules::rewrite::new_seg_node`'s `_ => u32::MAX` fallback). C# unification treats an
    /// unspecified `StrRep` as compatible with *any* value, so such a node must match every trie edge
    /// whose phonological lanes unify, regardless of the edge's own `char_def` — the module doc's
    /// "match = `char_def` equality AND `flat_unifiable`" shortcut is only valid when the query
    /// segment itself carries a concrete literal identity. Without this, a root like Indonesian
    /// `pakai` (whose first phoneme, "p", is exactly this kind of reinstated-but-unidentified
    /// segment during analysis of memakai) can never be found: the char_def-equality gate rejects
    /// every edge outright, and the segment is not `optional` (it must be consumed, not skipped), so
    /// the whole-word search always fails.
    ///
    /// **Optional segments are skippable** — the faithful analog of C#'s transducer treating an
    /// `Optional` shape annotation as matchable-or-skippable (`Transduce` over the shape's Segment
    /// annotations, `RootAllomorphTrie.Search`). An optional input segment (e.g. the epenthetic node
    /// a deletion prule's *analysis* re-inserts, marked `OPTIONAL` by `rewrite::analyze`) may be
    /// consumed by a trie edge *or* left unconsumed, so the underlying root `/ajar/` still matches the
    /// analysis shape `aj[?]ar`. Without this, prule-bearing grammars (Indonesian) find no root.
    fn search_segs_opt(
        &self,
        segs: &[(u32, Vec<u64>, bool)],
        closure: Option<&[CdBits]>,
    ) -> Vec<(AllomorphId, LexEntryId)> {
        let mut active: Vec<usize> = vec![0];
        for (cd, lanes, optional) in segs {
            let mut next: Vec<usize> = Vec::new();
            // Consume branch: follow matching edges — see [`Self::edge_matches`] for the full
            // concrete/pattern/wildcard predicate.
            for &node in &active {
                for e in &self.nodes[node].edges {
                    if Self::edge_matches(e, *cd, lanes, closure) && !next.contains(&e.target) {
                        next.push(e.target);
                    }
                }
            }
            // Skip branch (optional only): the segment is absent in the underlying form, so the trie
            // position is carried forward unchanged.
            if *optional {
                for &node in &active {
                    if !next.contains(&node) {
                        next.push(node);
                    }
                }
            }
            if next.is_empty() {
                return Vec::new(); // no continuation consumes this segment ⇒ end-anchored fail
            }
            active = next;
        }
        // Whole input consumed; collect accepts at the reached nodes (end anchor).
        let mut out: Vec<(AllomorphId, LexEntryId)> = Vec::new();
        for &node in &active {
            for &pair in &self.nodes[node].accepts {
                if !out.contains(&pair) {
                    out.push(pair);
                }
            }
        }
        out
    }
}

/// The `(char_def, phon-lanes, cd_set)` sequence of a shape's `Segment` nodes, resolving lanes from
/// the char-def table (root allomorph shapes are stored feature-less, `feat_width == 0`).
/// Boundaries and anchors are skipped (the C# `Segment`-only filter, `Morpher.cs:40`). A
/// pattern-derived node (`char_def == NO_CHAR_DEF`, loader N3) contributes its stored [`CdSet`]
/// (the class's member set); a concrete node's set is never consulted (implicit singleton), stored
/// as the free [`CdSet::Unrestricted`].
fn shape_segments(
    shape: &Shape,
    table: &CharDefTable,
    feat_width: usize,
) -> Vec<(u32, Vec<u64>, CdSet)> {
    let mut out = Vec::new();
    for i in 0..shape.len() {
        if shape.kind(i) == NodeKind::Segment {
            let cd = shape.char_def(i);
            let cd_set = match shape.node_cd_set(i) {
                EffectiveCdSet::Members(b) => CdSet::Members(b.clone()),
                EffectiveCdSet::Singleton(_) | EffectiveCdSet::Unrestricted => CdSet::Unrestricted,
            };
            out.push((cd, char_def_lanes(table, cd, feat_width), cd_set));
        }
    }
    out
}

/// The `(char_def, phon-lanes)` sequence of an *input* shape's `Segment` nodes. Prefers the shape's
/// own feature lanes when it carries a matching-width feature matrix (a feature-bearing shape from
/// the rewrite bridge, whose lanes may diverge from the char-def after a feature-change rule); else
/// resolves lanes from the char-def table, matching the build side.
fn shape_search_segments(
    shape: &Shape,
    table: &CharDefTable,
    feat_width: usize,
) -> Vec<(u32, Vec<u64>, bool)> {
    let use_shape_lanes = feat_width > 0 && shape.feat_width() as usize == feat_width;
    let mut out = Vec::new();
    for i in 0..shape.len() {
        if shape.kind(i) == NodeKind::Segment {
            let cd = shape.char_def(i);
            let lanes = if use_shape_lanes {
                shape.node_lanes(i).to_vec()
            } else {
                char_def_lanes(table, cd, feat_width)
            };
            out.push((cd, lanes, shape.flags(i).is_optional()));
        }
    }
    out
}

/// A char-def's phonological feature lanes (empty for anchors / zero-phon-feature grammars).
fn char_def_lanes(table: &CharDefTable, cd: u32, feat_width: usize) -> Vec<u64> {
    if cd == NO_CHAR_DEF || feat_width == 0 {
        return Vec::new();
    }
    table.get(CharDefId(cd)).feature_lanes().to_vec()
}

/// All strata's root-allomorph tries, built once (C# `Morpher._allomorphTries`, `Morpher.cs:35-48`).
/// [`RootAllomorphIndex::search`] is the M5b pipeline entry — the Rust analog of
/// `Morpher.SearchRootAllomorphs(stratum, shape)` (`Morpher.cs:343-347`).
#[derive(Debug)]
pub struct RootAllomorphIndex {
    tries: Vec<RootAllomorphTrie>,
}

impl RootAllomorphIndex {
    /// Build one trie per stratum (document order).
    pub fn build(grammar: &Grammar) -> RootAllomorphIndex {
        let tries = (0..grammar.strata.len())
            .map(|s| RootAllomorphTrie::build(grammar, StratumId(s as u8)))
            .collect();
        RootAllomorphIndex { tries }
    }

    /// The M5b entry: search `stratum`'s trie for root allomorphs matching `shape`
    /// (C# `Morpher.SearchRootAllomorphs`). Each returned `LexEntryId` feeds lexical lookup.
    pub fn search(
        &self,
        grammar: &Grammar,
        stratum: StratumId,
        shape: &Shape,
    ) -> Vec<(AllomorphId, LexEntryId)> {
        self.tries[stratum.0 as usize].search(grammar, shape)
    }

    /// The per-stratum trie (introspection for reports/tests).
    #[inline]
    pub fn trie(&self, stratum: StratumId) -> &RootAllomorphTrie {
        &self.tries[stratum.0 as usize]
    }

    /// Number of strata indexed.
    #[inline]
    pub fn stratum_count(&self) -> usize {
        self.tries.len()
    }
}

/// All lexical-pattern root allomorphs, flat across every stratum, in document order — the exact
/// counterpart of the exclusion [`RootAllomorphTrie::build`] now applies, mirroring C#'s single
/// `_lexicalPatterns` list built across all strata (`Morpher.cs:74-85`). Consumed by the guess
/// subsystem (P11 chunks 3-5, `Morpher.lexical_patterns`); inert until then.
pub fn collect_lexical_patterns(grammar: &Grammar) -> Vec<(AllomorphId, LexEntryId)> {
    let mut out = Vec::new();
    for sd in &grammar.strata {
        for &le_id in &sd.entries {
            let entry = &grammar.entries[le_id.0 as usize];
            for allo in &entry.allomorphs {
                if allo.is_pattern {
                    out.push((allo.id, le_id));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: a concrete-node path (the pre-wave-4 2-tuple form) — `cd_set` never consulted
    /// for a concrete edge, stored as the free `Unrestricted`.
    fn concrete(segs: &[(u32, Vec<u64>)]) -> Vec<(u32, Vec<u64>, CdSet)> {
        segs.iter()
            .map(|(cd, l)| (*cd, l.clone(), CdSet::Unrestricted))
            .collect()
    }

    // A tiny hand-built trie over a 1-lane "feature system". Segments are `(char_def, lanes)`:
    //   cd 10 = "p", concrete lanes [0b01]
    //   cd 11 = "a", concrete lanes [0b10]
    //   cd 12 = "b", concrete lanes [0b01]
    // Roots: A = /p a/  (allo 100, entry 0);  B = /p a b/ (allo 101, entry 1);  C = /a/ (allo 102, entry 2)
    fn tiny_trie() -> RootAllomorphTrie {
        let mut t = RootAllomorphTrie {
            nodes: vec![TrieNode::default()],
            table: TableId(0),
            feat_width: 1,
            allomorph_count: 0,
        };
        t.add_path(
            &concrete(&[(10, vec![0b01]), (11, vec![0b10])]),
            AllomorphId(100),
            LexEntryId(0),
        );
        t.add_path(
            &concrete(&[(10, vec![0b01]), (11, vec![0b10]), (12, vec![0b01])]),
            AllomorphId(101),
            LexEntryId(1),
        );
        t.add_path(
            &concrete(&[(11, vec![0b10])]),
            AllomorphId(102),
            LexEntryId(2),
        );
        t
    }

    #[test]
    fn exact_match_returns_the_root() {
        let t = tiny_trie();
        // Search /p a/ (exact same char_defs + lanes as root A) ⇒ {allo 100}.
        let got = t.search_segs(&[(10, vec![0b01]), (11, vec![0b10])]);
        assert_eq!(got, vec![(AllomorphId(100), LexEntryId(0))]);
    }

    #[test]
    fn prefix_of_a_longer_root_does_not_accept() {
        let t = tiny_trie();
        // /p a b/ is root B, not A: end-anchored, so /p a/ must NOT return B, and /p a b/ returns B only.
        let got = t.search_segs(&[(10, vec![0b01]), (11, vec![0b10]), (12, vec![0b01])]);
        assert_eq!(got, vec![(AllomorphId(101), LexEntryId(1))]);
    }

    #[test]
    fn feature_unification_matches_underspecified_input() {
        let t = tiny_trie();
        // Input first segment carries char_def 10 but a *superset* lane [0b11]; it unifies with the
        // stored [0b01] (0b11 & 0b01 = 0b01 ≠ 0). Root A is found — this is unification, not string
        // identity (C# UseUnification=true).
        let got = t.search_segs(&[(10, vec![0b11]), (11, vec![0b10])]);
        assert_eq!(got, vec![(AllomorphId(100), LexEntryId(0))]);
    }

    #[test]
    fn feature_conflict_rejects_despite_char_def_match() {
        let t = tiny_trie();
        // Same char_def 10, but lanes [0b10] conflict with the stored [0b01] (AND = 0). C#'s full-FS
        // unification would reject: char_def/StrRep matches but the phonological features clash.
        let got = t.search_segs(&[(10, vec![0b10]), (11, vec![0b10])]);
        assert!(got.is_empty(), "feature conflict must reject, got {got:?}");
    }

    #[test]
    fn char_def_mismatch_rejects() {
        let t = tiny_trie();
        // First segment char_def 99 has no edge at the root node ⇒ no match, even though lanes could
        // unify with anything. This is the StrRep discriminator (crucial for zero-phon-feature Sena).
        let got = t.search_segs(&[(99, vec![0b01]), (11, vec![0b10])]);
        assert!(got.is_empty(), "char_def mismatch must reject, got {got:?}");
    }

    // ============================================================================================
    // The build-time unifiability closure (Design A) as an equality-miss fallback in
    // `edge_matches`.
    // ============================================================================================

    #[test]
    fn closure_cross_matches_a_distinct_unifiable_char_def_only_when_provided() {
        let t = tiny_trie();
        // Declare cd 10 ("p", stored lanes [0b01]) and cd 99 (a distinct char_def, simulating a
        // second table's "a̘"-analog) as P5 closure siblings.
        let mut closure = vec![CdBits::empty(); 100];
        closure[10].insert(10);
        closure[10].insert(99);
        closure[99].insert(99);
        closure[99].insert(10);

        // With Some(closure) and lane-compatible input (query cd 99, lanes [0b01] unify with the
        // stored edge's [0b01]), the equality-miss fallback lets root A's edge match.
        let got = t.search_segs_with_closure(&[(99, vec![0b01]), (11, vec![0b10])], Some(&closure));
        assert_eq!(
            got,
            vec![(AllomorphId(100), LexEntryId(0))],
            "closure hit must cross-match"
        );

        // The exact same query with closure = None (the Sena/zero-feature regime) must NOT match --
        // this is the "closure absent ⇒ bit-for-bit today's behavior" invariant (design §3).
        let got_none = t.search_segs_with_closure(&[(99, vec![0b01]), (11, vec![0b10])], None);
        assert!(
            got_none.is_empty(),
            "closure disabled must still reject a distinct char_def"
        );
    }

    #[test]
    fn closure_membership_does_not_bypass_the_lane_conjunct() {
        let t = tiny_trie();
        // cd 10 and cd 99 are still declared closure siblings...
        let mut closure = vec![CdBits::empty(); 100];
        closure[10].insert(99);
        closure[99].insert(10);
        // ...but this query's lanes for cd 99 are [0b10], which conflicts with the stored edge's
        // [0b01] (AND = 0). Design A's soundness argument (§3): the closure hit is REFINED by the
        // existing `flat_unifiable` conjunct, never a substitute for it.
        let got = t.search_segs_with_closure(&[(99, vec![0b10]), (11, vec![0b10])], Some(&closure));
        assert!(
            got.is_empty(),
            "closure membership must not bypass the phonological-lane conjunct"
        );
    }

    #[test]
    fn closure_present_but_unrelated_char_defs_still_reject() {
        let t = tiny_trie();
        // A closure exists (Some), but declares no relation at all for cd 99 (an all-empty row) --
        // must behave exactly like the no-closure case for this cd.
        let closure = vec![CdBits::empty(); 100];
        let got = t.search_segs_with_closure(&[(99, vec![0b01]), (11, vec![0b10])], Some(&closure));
        assert!(
            got.is_empty(),
            "an empty closure row must not manufacture a match"
        );
    }

    #[test]
    fn single_segment_root_matches_and_is_distinct_from_prefixes() {
        let t = tiny_trie();
        // /a/ is root C. It must NOT collide with the /p .../ paths.
        let got = t.search_segs(&[(11, vec![0b10])]);
        assert_eq!(got, vec![(AllomorphId(102), LexEntryId(2))]);
    }

    #[test]
    fn homographs_accumulate_at_one_accepting_node() {
        // Two entries sharing the identical surface /p a/ both accept at the same node.
        let mut t = tiny_trie();
        t.add_path(
            &concrete(&[(10, vec![0b01]), (11, vec![0b10])]),
            AllomorphId(200),
            LexEntryId(7),
        );
        let got = t.search_segs(&[(10, vec![0b01]), (11, vec![0b10])]);
        assert_eq!(
            got,
            vec![
                (AllomorphId(100), LexEntryId(0)),
                (AllomorphId(200), LexEntryId(7))
            ],
        );
    }

    #[test]
    fn empty_and_too_long_inputs_do_not_match() {
        let t = tiny_trie();
        // Empty input: root node has no accepts ⇒ nothing.
        assert!(t.search_segs(&[]).is_empty());
        // /p a b b/: after /p a b/ there is no further edge ⇒ end-anchored fail.
        let got = t.search_segs(&[
            (10, vec![0b01]),
            (11, vec![0b10]),
            (12, vec![0b01]),
            (12, vec![0b01]),
        ]);
        assert!(got.is_empty());
    }

    #[test]
    fn zero_phon_feature_discrimination_is_by_char_def() {
        // A synthetic feat_width-0 stratum (all lanes empty), exercising that width directly.
        // NOTE: post plan §13.1 Tier-1 #1, no *real* grammar (including Sena) ever actually
        // constructs a `RootAllomorphTrie` at `feat_width == 0` any more — `grammar.phon_features
        // .len()` is always >= 1 now (the always-appended synthetic `Type` feature), so this test
        // is a decoupled unit-level exercise of the width-0 codepath, not a live "Sena" analog.
        // Discrimination is purely by char_def; two distinct roots of equal length must not
        // cross-match.
        let mut t = RootAllomorphTrie {
            nodes: vec![TrieNode::default()],
            table: TableId(0),
            feat_width: 0,
            allomorph_count: 0,
        };
        t.add_path(
            &concrete(&[(1, vec![]), (2, vec![])]),
            AllomorphId(1),
            LexEntryId(0),
        ); // /b a/
        t.add_path(
            &concrete(&[(3, vec![]), (4, vec![])]),
            AllomorphId(2),
            LexEntryId(1),
        ); // /m u/
        assert_eq!(
            t.search_segs(&[(1, vec![]), (2, vec![])]),
            vec![(AllomorphId(1), LexEntryId(0))]
        );
        assert_eq!(
            t.search_segs(&[(3, vec![]), (4, vec![])]),
            vec![(AllomorphId(2), LexEntryId(1))]
        );
        // A different length or a swapped char_def must not match either root.
        assert!(t.search_segs(&[(1, vec![]), (4, vec![])]).is_empty());
        assert_eq!(t.allomorph_count(), 2);
    }

    // ============================================================================================
    // Wave-4: pattern-derived (NO_CHAR_DEF + CdSet) edges — loader N3 end-to-end.
    // ============================================================================================

    /// A root "b[Vowel]t" (cd 20="b", cd 22="t"; Vowel = {21, 23}) — the loader-N3 fixture shape.
    fn pattern_trie() -> RootAllomorphTrie {
        let mut t = RootAllomorphTrie {
            nodes: vec![TrieNode::default()],
            table: TableId(0),
            feat_width: 0,
            allomorph_count: 0,
        };
        t.add_path(
            &[
                (20, vec![], CdSet::Unrestricted),
                (
                    NO_CHAR_DEF,
                    vec![],
                    CdSet::Members(CdBits::from_ids([21, 23])),
                ),
                (22, vec![], CdSet::Unrestricted),
            ],
            AllomorphId(300),
            LexEntryId(9),
        );
        t
    }

    #[test]
    fn pattern_edge_matches_each_class_member() {
        let t = pattern_trie();
        // Both "bat" (21) and "bet" (23) reach the accept through the class edge.
        assert_eq!(
            t.search_segs(&[(20, vec![]), (21, vec![]), (22, vec![])]),
            vec![(AllomorphId(300), LexEntryId(9))],
        );
        assert_eq!(
            t.search_segs(&[(20, vec![]), (23, vec![]), (22, vec![])]),
            vec![(AllomorphId(300), LexEntryId(9))],
        );
    }

    #[test]
    fn pattern_edge_rejects_a_non_member() {
        let t = pattern_trie();
        // "bit" (24 not in {21, 23}): the membership gate must reject even though a NO_CHAR_DEF
        // edge exists at that position and the (empty) lanes trivially unify.
        assert!(t
            .search_segs(&[(20, vec![]), (24, vec![]), (22, vec![])])
            .is_empty());
    }

    #[test]
    fn no_char_def_query_still_passes_a_pattern_edge() {
        let t = pattern_trie();
        // A reinserted/unidentified query segment (NO_CHAR_DEF) keeps its wildcard behavior against
        // pattern edges too (the documented over-approximation in `edge_matches`).
        assert_eq!(
            t.search_segs(&[(20, vec![]), (NO_CHAR_DEF, vec![]), (22, vec![])]),
            vec![(AllomorphId(300), LexEntryId(9))],
        );
    }

    #[test]
    fn distinct_class_edges_do_not_merge() {
        // Two pattern roots whose classes differ must get separate edges (ValueEquals-analog
        // grouping): "x[A]" with A={1} and "x[B]" with B={2}.
        let mut t = RootAllomorphTrie {
            nodes: vec![TrieNode::default()],
            table: TableId(0),
            feat_width: 0,
            allomorph_count: 0,
        };
        t.add_path(
            &[
                (5, vec![], CdSet::Unrestricted),
                (NO_CHAR_DEF, vec![], CdSet::Members(CdBits::from_ids([1]))),
            ],
            AllomorphId(1),
            LexEntryId(0),
        );
        t.add_path(
            &[
                (5, vec![], CdSet::Unrestricted),
                (NO_CHAR_DEF, vec![], CdSet::Members(CdBits::from_ids([2]))),
            ],
            AllomorphId(2),
            LexEntryId(1),
        );
        assert_eq!(
            t.search_segs(&[(5, vec![]), (1, vec![])]),
            vec![(AllomorphId(1), LexEntryId(0))]
        );
        assert_eq!(
            t.search_segs(&[(5, vec![]), (2, vec![])]),
            vec![(AllomorphId(2), LexEntryId(1))]
        );
        // Identical classes DO share an edge (prefix sharing still works for patterns).
        t.add_path(
            &[
                (5, vec![], CdSet::Unrestricted),
                (NO_CHAR_DEF, vec![], CdSet::Members(CdBits::from_ids([1]))),
            ],
            AllomorphId(3),
            LexEntryId(2),
        );
        assert_eq!(
            t.search_segs(&[(5, vec![]), (1, vec![])]),
            vec![
                (AllomorphId(1), LexEntryId(0)),
                (AllomorphId(3), LexEntryId(2))
            ],
        );
    }
}
