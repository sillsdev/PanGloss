//! Root-allomorph lexical-lookup trie (plan §5.1 lexical lookup; M5a).
//!
//! Faithful behavioral port of C# `RootAllomorphTrie` (`RootAllomorphTrie.cs`) and
//! `Morpher.SearchRootAllomorphs` (`Morpher.cs:343-347`) + the per-stratum trie construction
//! (`Morpher.cs:35-48`). Each stratum indexes all of its root allomorphs' segment shapes into a
//! trie; `RootAllomorphTrie::search` walks an input shape's segment nodes through the trie by
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
//!    `pg_featstruct::flat_unifiable` on two empty rows is trivially `true`. A lane-only trie would
//!    therefore merge *all* Sena roots onto one path and return every equal-length root. The Rust
//!    engine does not model `StrRep`; its faithful analog for a zero-feature grammar is the node's
//!    `char_def` (a `pg_grammar_model::chardef::CharDefId`, whose representations are unique per table).
//!    For a feature-bearing grammar (Indonesian, Amharic, the C# test fixtures) C# attaches **no**
//!    `StrRep` at all, so two distinct char-defs whose feature structs unify legitimately cross-match
//!    lexical lookup in C#. `edge_matches`'s build-time
//!    `pg_grammar_model::chardef::CharDefTable::unifiable_cds` closure (Design A) is the equality-miss
//!    fallback that restores this: a bitset probe, consulted only when `char_def` equality itself
//!    misses, and entirely absent (`None`) for a zero-feature table so Sena/en/sp pay nothing and
//!    keep the pre-P5 identity-only behavior bit-for-bit.
//!
//! So the match predicate is the exact port of C#'s full-`FeatureStruct` unification for concrete
//! segments: **`char_def` equality, OR closure membership when the table has one (the `StrRep`
//! analog), AND `pg_featstruct::flat_unifiable` on the phonological lanes.** For a zero-phon-feature
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
//! `char_def` ids are **per-table** (`pg_grammar_model::chardef::CharDefId` is a dense per-table
//! identity), whereas C#'s `StrRep` is a table-independent string. Two consequences the pipeline
//! must uphold:
//! - **Same-table segmentation.** `RootAllomorphTrie::search` requires the input shape to be
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
use pg_grammar_model::chardef::{CharDefId, CharDefTable};
use pg_grammar_model::model::{AllomorphId, Grammar, LexEntryId, StratumId, TableId};
use pg_shape::{CdBits, CdSet, EffectiveCdSet, NodeKind, Shape, NO_CHAR_DEF};

/// A trie node: outgoing edges (keyed by `char_def`) and the root allomorphs that accept here.
#[derive(Debug, Default)]
struct TrieNode {
    edges: Vec<TrieEdge>,
    /// `(allomorph, owning entry)` pairs accepted at this node; homographs accumulate here.
    accepts: Vec<(AllomorphId, LexEntryId)>,
}

/// A trie edge: the segment's `char_def` plus lanes and the target node. A pattern-derived edge (wave-4)
/// stores a class member set instead -- see docs/research/pg-parse-root-trie-design-notes.md.
#[derive(Debug)]
struct TrieEdge {
    char_def: u32,
    lanes: Vec<u64>,
    /// The stored node's char-def-set, consulted only when `char_def == NO_CHAR_DEF`; a concrete edge is an implicit singleton.
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
    /// (`collect_lexical_patterns`) instead — mirroring C#'s partition exactly, rather than the
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

    /// Adds one root allomorph path, grouping edges by `char_def` so shared prefixes share a path; a pattern-derived edge also requires matching `cd_set`/lanes to reuse an edge.
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

    /// Test-only entry point: all segments mandatory, delegates to `Self::search_segs_opt` with `closure = None`.
    #[cfg(test)]
    fn search_segs(&self, segs: &[(u32, Vec<u64>)]) -> Vec<(AllomorphId, LexEntryId)> {
        let with_opt: Vec<(u32, Vec<u64>, bool)> =
            segs.iter().map(|(cd, l)| (*cd, l.clone(), false)).collect();
        self.search_segs_opt(&with_opt, None)
    }

    /// Same as `Self::search_segs`, but threading an explicit closure to exercise the equality-miss fallback without a real `CharDefTable`.
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

    /// Does this edge accept a query segment `(cd, lanes)`? Concrete edges match by `char_def` equality or build-time closure
    /// membership; pattern edges by `CdSet` membership. See docs/research/pg-parse-root-trie-design-notes.md for the `NO_CHAR_DEF` case.
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

    /// Follows every edge that matches an input segment (preserving C#'s nondeterministic traversal) and skips optional segments.
    /// See docs/research/pg-parse-root-trie-design-notes.md for why a `NO_CHAR_DEF` query is a wildcard and optional segments skippable.
    fn search_segs_opt(
        &self,
        segs: &[(u32, Vec<u64>, bool)],
        closure: Option<&[CdBits]>,
    ) -> Vec<(AllomorphId, LexEntryId)> {
        let mut active: Vec<usize> = vec![0];
        for (cd, lanes, optional) in segs {
            let mut next: Vec<usize> = Vec::new();
            // Consume branch: follow every matching edge (see `Self::edge_matches`).
            for &node in &active {
                for e in &self.nodes[node].edges {
                    if Self::edge_matches(e, *cd, lanes, closure) && !next.contains(&e.target) {
                        next.push(e.target);
                    }
                }
            }
            // Skip branch (optional only): the trie position carries forward unchanged.
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

/// The `(char_def, phon-lanes, cd_set)` sequence of a shape's `Segment` nodes (boundaries/anchors skipped); a pattern-derived node's `CdSet` is its class member set, a concrete node's is `Unrestricted`.
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

/// The `(char_def, phon-lanes)` sequence of an input shape's `Segment` nodes; prefers the shape's own lanes when width matches (post feature-change), else resolves from the char-def table.
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
/// `RootAllomorphIndex::search` is the M5b pipeline entry — the Rust analog of
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
/// counterpart of the exclusion `RootAllomorphTrie::build` now applies, mirroring C#'s single
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
mod tests;
