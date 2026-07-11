//! The guesser's node matcher (P11 §4.3): a faithful, literal port of C#
//! `Morpher.MatchNodesWithPattern` (`Morpher.cs:597-625`) + the rendering half of
//! `Morpher.LexicalGuess` (`Morpher.cs:522-590` step 3, `match.ToString(table, false)`,
//! `HermitCrabExtensions.cs:317-335`).
//!
//! **Why not `hc-fst`.** C# itself refuses to route this through its `Matcher`/`Pattern` engine
//! — "the Matcher doesn't preserve the unifications of the nodes" (`Morpher.cs:138-140`) — and
//! this port follows: [`match_nodes_with_pattern`] is a direct recursive walk over a small
//! resolved node view ([`GuessNode`]), never through `hc_fst`'s FSA/pattern-compile machinery.
//! (The design doc's §3 blesses reuse of pure lane-arithmetic — this module keeps its own tiny
//! `unify_lanes`/`unify_cd_set` helpers rather than importing `hc_fst::lanes`, so there is no
//! dependency edge onto the FST engine from this file at all, not even an arithmetic one.)
//!
//! [`GuessNode`] is built from a [`Shape`]'s INTERIOR nodes — segments AND boundaries (anchors
//! excluded; they are not `ShapeNode`s in C#) — via [`nodes_of`]. This is the crucial difference
//! from `root_trie.rs`'s trie-edge builders, which filter to `Segment` nodes only: §1.3 step 2
//! is explicit that `MatchNodesWithPattern`'s `nodes` parameter is **every** node of the analysis
//! shape, boundaries included (they only drop out later, at [`render_match`] time, mirroring
//! `match.ToString(table, false)`'s `includeBdry = false`).
//!
//! Unification of an input node against a pattern node ([`unify_shape_nodes`]) is `root_trie.rs::
//! edge_matches`'s same shape of predicate — kind equality (the `Type` feature, which the trie
//! never needed since it pre-filters to segments), identity (concrete `char_def` equality /
//! `CdSet` membership / `NO_CHAR_DEF`-query wildcard), and phonological-lane unifiability — but
//! genuinely UNIFIES (narrows) rather than just checking compatibility: the returned node's
//! identity and lanes are the intersection of both sides, because a narrowed node is exactly what
//! [`render_match`] needs to pick the right candidate representations later (§4.3's "the
//! narrowing matters for rendering").

use std::rc::Rc;

use hc_grammar::chardef::{CharDefId, CharDefTable};
use hc_grammar::model::{AllomorphId, Grammar, LexEntryId, MorphemeId};
use hc_rules::shape_feat::segment_with_features;
use hc_rules::word::{GuessedRoot, MorphRecord, Word};
use hc_shape::{CdSet, NodeKind, Shape, NO_CHAR_DEF};
use rustc_hash::FxHashSet as HashSet;

use crate::surface::matching_reps_for_node;

/// A resolved, table-independent view of one shape node — the `(kind, char_def, lanes, cd_set,
/// optional, iterative, deleted)` tuple §4.3 calls for. Built from a [`Shape`]'s interior nodes by
/// [`nodes_of`]; also the type [`match_nodes_with_pattern`] both consumes and produces (a matched
/// node may be a freshly unified value, not one of the original shape's own nodes).
#[derive(Clone, Debug, PartialEq)]
pub struct GuessNode {
    pub kind: NodeKind,
    /// `NO_CHAR_DEF` for an abstract (class-derived) node; a concrete char-def id otherwise. The
    /// `StrRep` analog, exactly `root_trie.rs`'s convention.
    pub char_def: u32,
    pub lanes: Vec<u64>,
    /// Consulted only when `char_def == NO_CHAR_DEF` (a concrete node is an implicit singleton of
    /// its own `char_def`) — same convention as `Shape::node_cd_set`/`root_trie.rs::TrieEdge`.
    pub cd_set: CdSet,
    pub optional: bool,
    pub iterative: bool,
    /// Always `false` in this port: frozen `Shape`s never carry deleted nodes (repeatedly noted
    /// elsewhere in this crate/`hc-rules`). Kept as a field for fidelity with C#'s
    /// `ShapeNode.IsDeleted()` check at the one call site that reads it ([`render_match`]) and so
    /// a future deletion-bearing node source needs no signature change here.
    pub deleted: bool,
}

/// Table-derived lanes for a concrete `char_def` (empty for `NO_CHAR_DEF` or a zero-phon-feature
/// table) — the same small resolution `root_trie.rs::char_def_lanes` performs for root-allomorph
/// shapes, which (like a lexical-pattern shape) are stored feature-less at load time.
fn table_lanes(table: &CharDefTable, cd: u32, feat_width: usize) -> Vec<u64> {
    if cd == NO_CHAR_DEF || feat_width == 0 {
        return Vec::new();
    }
    table.get(CharDefId(cd)).feature_lanes().to_vec()
}

/// Build the full node-view sequence of `shape`'s interior (§1.3 step 2: "inputNodes = ALL of the
/// analysis shape's nodes"). Mirrors `root_trie.rs::shape_search_segments`'s prefer-the-shape's-
/// own-lanes-when-feature-bearing convention (`shape.feat_width() == phon_features.len()`):
/// a live, feature-bearing analysis shape uses its own per-node lanes; a feature-less shape (every
/// lexical-pattern shape, loaded by `segment_with_patterns`, which never attaches phonological
/// lanes) falls back to resolving lanes from `table` by `char_def` — exactly like a root
/// allomorph's stored shape.
pub fn nodes_of(shape: &Shape, table: &CharDefTable, feat_width: usize) -> Vec<GuessNode> {
    let use_shape_lanes = feat_width > 0 && shape.feat_width() as usize == feat_width;
    let mut out = Vec::with_capacity(shape.len());
    for i in 0..shape.len() {
        let kind = shape.kind(i);
        if kind != NodeKind::Segment && kind != NodeKind::Boundary {
            continue; // anchors are not ShapeNodes in C#
        }
        let cd = shape.char_def(i);
        let lanes = if use_shape_lanes {
            shape.node_lanes(i).to_vec()
        } else {
            table_lanes(table, cd, feat_width)
        };
        let cd_set = match shape.node_cd_set(i) {
            hc_shape::EffectiveCdSet::Members(b) => CdSet::Members(b.clone()),
            hc_shape::EffectiveCdSet::Singleton(_) | hc_shape::EffectiveCdSet::Unrestricted => CdSet::Unrestricted,
        };
        let flags = shape.flags(i);
        out.push(GuessNode {
            kind,
            char_def: cd,
            lanes,
            cd_set,
            optional: flags.is_optional(),
            iterative: flags.is_iterative(),
            deleted: false,
        });
    }
    out
}

/// Does `cd_set` admit char-def `id`?
fn cd_set_contains(cd_set: &CdSet, id: u32) -> bool {
    match cd_set {
        CdSet::Unrestricted => true,
        CdSet::Members(b) => b.contains(id),
    }
}

/// Unify two identity dimensions (a `(char_def, cd_set)` pair each), producing the NARROWED
/// identity or `None` if incompatible. The four cases mirror `root_trie.rs::edge_matches`'s
/// concrete/pattern/wildcard shape, but symmetrically (either side may be concrete or abstract,
/// unlike the trie's query-vs-stored-edge asymmetry) and PRODUCING a result rather than a bool.
fn unify_identity(a_cd: u32, a_set: &CdSet, b_cd: u32, b_set: &CdSet) -> Option<(u32, CdSet)> {
    match (a_cd != NO_CHAR_DEF, b_cd != NO_CHAR_DEF) {
        (true, true) => (a_cd == b_cd).then(|| (a_cd, CdSet::Unrestricted)),
        (true, false) => cd_set_contains(b_set, a_cd).then(|| (a_cd, CdSet::Unrestricted)),
        (false, true) => cd_set_contains(a_set, b_cd).then(|| (b_cd, CdSet::Unrestricted)),
        (false, false) => unify_cd_set(a_set, b_set).map(|s| (NO_CHAR_DEF, s)),
    }
}

/// Intersect two abstract identity sets — `FeatureStruct.Unify`'s narrowing semantics applied to
/// the identity dimension (§3's "class membership" refinement) rather than a phonological lane.
fn unify_cd_set(a: &CdSet, b: &CdSet) -> Option<CdSet> {
    match (a, b) {
        (CdSet::Unrestricted, CdSet::Unrestricted) => Some(CdSet::Unrestricted),
        (CdSet::Unrestricted, CdSet::Members(m)) | (CdSet::Members(m), CdSet::Unrestricted) => {
            Some(CdSet::Members(m.clone()))
        }
        (CdSet::Members(m1), CdSet::Members(m2)) => {
            let inter = m1.intersect(m2);
            if inter.count() == 0 {
                None
            } else {
                Some(CdSet::Members(inter))
            }
        }
    }
}

/// Lane-wise unify (intersect) two flat symbolic-feature constraints — `FeatureStruct.Unify` on
/// the phonological-lane dimension (a lane AND; an absent lane on either side is unconstrained,
/// i.e. all-ones, matching `hc_featstruct::flat_unifiable`'s convention). `None` if any lane's
/// intersection is empty. Kept local to this module (rather than reusing `hc_fst::lanes::
/// flat_unify`, which computes the identical arithmetic) so this file has no dependency edge onto
/// the FST engine at all — see the module doc's "why not hc-fst" note.
fn unify_lanes(a: &[u64], b: &[u64]) -> Option<Vec<u64>> {
    let n = a.len().max(b.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let av = a.get(i).copied().unwrap_or(u64::MAX);
        let bv = b.get(i).copied().unwrap_or(u64::MAX);
        let v = av & bv;
        if v == 0 {
            return None;
        }
        out.push(v);
    }
    Some(out)
}

/// `Morpher.UnifyShapeNodes` (`Morpher.cs:640-647`): unify one input node against one pattern
/// node, producing the unified node or `None` on failure. `Type` (kind) mismatch — a Segment node
/// against a Boundary pattern node or vice versa — always fails, exactly as it would fail C#'s
/// full-`FeatureStruct` unify (the type symbol is part of that FS). C#'s `fs.ValueEquals(node.FS)
/// ? node : new ShapeNode(fs)` object-identity optimization has no analog for a value type — this
/// always returns a freshly narrowed [`GuessNode`], which is behaviorally identical content
/// either way.
fn unify_shape_nodes(node: &GuessNode, pattern: &GuessNode) -> Option<GuessNode> {
    if node.kind != pattern.kind {
        return None;
    }
    let (char_def, cd_set) = unify_identity(node.char_def, &node.cd_set, pattern.char_def, &pattern.cd_set)?;
    let lanes = unify_lanes(&node.lanes, &pattern.lanes)?;
    Some(GuessNode {
        kind: node.kind,
        char_def,
        lanes,
        cd_set,
        // Inert for `match_nodes_with_pattern`'s own control flow: the recursion only ever reads
        // `optional`/`iterative` off the ORIGINAL `pattern` slice (`pattern[p]`), never off an
        // already-unified node in `prefix` — matching C#, whose `MatchNodesWithPattern` likewise
        // never re-reads a freshly unified `ShapeNode`'s own (default, unset) annotation flags.
        optional: false,
        iterative: false,
        deleted: false,
    })
}

/// `Morpher.MatchNodesWithPattern` (`Morpher.cs:597-625`): match `nodes` against `pattern` in
/// full — the whole `nodes` sequence must be consumed (`pattern.Count == p` only accepts when
/// `nodes.Count == n`; this is a whole-shape match, not a substring search). Input-side
/// `optional`/`iterative` flags are ignored (every input node must be consumed); only
/// pattern-side flags drive skip/repeat. Returns every matched node-list path (there can be more
/// than one, e.g. `([Seg])([Seg])`'s two ways to place a single real segment) — deduping the
/// rendered strings is the caller's job (`hc-parse::guess::lexical_guess`, P11 chunk 5), exactly
/// as the C# doc comment says: "It is up to the caller to eliminate duplicates."
pub fn match_nodes_with_pattern(nodes: &[GuessNode], pattern: &[GuessNode]) -> Vec<Vec<GuessNode>> {
    match_rec(nodes, pattern, 0, 0, false, &[])
}

/// The recursion `match_nodes_with_pattern` seeds: `n`/`p` are cursors into `nodes`/`pattern`;
/// `obligatory` suppresses skip-of-`p` immediately after consuming at `p` via the iterative-reuse
/// branch (derivation dedup — skip-after-iterate would duplicate the consume-then-advance path);
/// `prefix` is the matched-so-far node list. Literal port of `Morpher.cs:597-625`, branch order
/// preserved exactly (skip, then end-of-nodes check, then consume, then iterate-reuse, then
/// advance) since C#'s own `results.AddRange` order is what an unstable-sort-independent test
/// (like this module's ported `TestMatchNodesWithPattern` cases) can still pin the *count* of.
fn match_rec(
    nodes: &[GuessNode],
    pattern: &[GuessNode],
    n: usize,
    p: usize,
    obligatory: bool,
    prefix: &[GuessNode],
) -> Vec<Vec<GuessNode>> {
    let mut results = Vec::new();
    if p == pattern.len() {
        if n == nodes.len() {
            // We match because we are at the end of both the pattern and the nodes.
            results.push(prefix.to_vec());
        }
        return results;
    }
    if pattern[p].optional && !obligatory {
        // Try skipping this item in the pattern.
        results.extend(match_rec(nodes, pattern, n, p + 1, false, prefix));
    }
    if n == nodes.len() {
        // We fail to match because we are at the end of the nodes but not the pattern.
        return results;
    }
    let Some(new_node) = unify_shape_nodes(&nodes[n], &pattern[p]) else {
        // We fail because the pattern didn't match the node here.
        return results;
    };
    let mut new_prefix = prefix.to_vec();
    new_prefix.push(new_node);
    if pattern[p].iterative {
        // Try using this item in the pattern again.
        results.extend(match_rec(nodes, pattern, n + 1, p, true, &new_prefix));
    }
    // Try the remainder of the nodes against the remainder of the pattern.
    results.extend(match_rec(nodes, pattern, n + 1, p + 1, false, &new_prefix));
    results
}

/// §1.3 step 3: `match.ToString(table, false)` (`HermitCrabExtensions.cs:317-335`, `includeBdry =
/// false`) — skip boundary and deleted nodes; per remaining node, the FIRST representation of the
/// FIRST table char-def whose kind/identity/lanes match (table document order). Delegates to
/// `surface::matching_reps_for_node`, the node-view-generalized core of `surface::
/// matching_str_reps` (P11 §4.3's refactor), so this is *exactly* the same rendering rule the
/// batch signature's surface column uses — no separate/duplicated predicate.
pub fn render_match(table: &CharDefTable, matched: &[GuessNode]) -> String {
    let mut out = String::new();
    for node in matched {
        if node.kind == NodeKind::Boundary || node.deleted {
            continue;
        }
        let reps = matching_reps_for_node(table, node.kind, node.char_def, &node.cd_set, &node.lanes, false);
        if let Some(first) = reps.into_iter().next() {
            out.push_str(&first);
        }
    }
    out
}

/// C# `Morpher.LexicalGuess` (`Morpher.cs:522-590`), §1.3 steps 1-7. Per lexical pattern (flat
/// across all strata, `Morpher::lexical_patterns`), match the analysis word `aw`'s shape against
/// the pattern's own shape, render each surviving match to a literal string (deduped PER PATTERN
/// — §1.3 step 4, C#'s `shapeSet`), and fabricate a concrete guessed root + owning entry for each
/// surviving string, cloning `aw` onto it exactly as `hc-parse::morpher::Morpher::lexical_lookup`
/// does for a real root.
///
/// **Table choice (§1.3 step 1, literal from C#):** `table` = `aw`'s OWN stratum's character-
/// definition table — used for BOTH matching ([`nodes_of`] on both the analysis shape and the
/// pattern's stored shape) and rendering ([`render_match`]), even though `lexical_patterns` draws
/// from every stratum. This is C#'s own literal behavior (`CharacterDefinitionTable table =
/// input.Stratum.CharacterDefinitionTable;`, declared once and reused for every pattern
/// regardless of which stratum it came from) — correct for every grammar where strata share one
/// table (true of all three reference grammars; `root_trie.rs`'s M5b module doc already flags a
/// same-table assumption elsewhere in this pipeline for the identical reason).
///
/// **The fabrication's re-segmentation table is different, deliberately** (design doc §4.3): the
/// FABRICATED root's shape is re-segmented via `segment_with_features` against the PATTERN
/// ENTRY's own stratum table — mirroring `Morpher::set_root_allomorph`'s treatment of an ordinary
/// root exactly (a fresh text→shape conversion keyed to the entry that will own it, independent
/// of whichever stratum's char-def ids the matching step above used). For every grammar in scope
/// this is the identical table object to the one used above, so the two choices are
/// observationally indistinguishable on any fixture this port targets; documented here as the
/// deliberate (not overlooked) simplification the design doc calls for.
///
/// **Simplification (documented, not a gap):** C# takes the `if (lexicalPattern.Morpheme != null)`
/// branch to overwrite the fabricated entry's stratum/syn-FS/MPR/is_partial from the pattern's
/// OWNING entry (`Morpher.cs:564-579`); every Rust lexical pattern has a real owning `LexEntryId`
/// via `Grammar::allomorph_owners` (the loader never produces an ownerless allomorph), so this
/// port always takes that branch — the "else" branch (leave the fabricated entry's fields
/// initialized straight from `aw`) is unreachable here and not implemented.
///
/// No dedup across patterns or across this function's own output (§1.2: C#'s `.Distinct()` on the
/// call site is a documented no-op — every yielded `Word` is a fresh clone with no `Equals`
/// override — the real dedup is the per-pattern `shape_set` here); duplicates across different
/// patterns are real and survive, matching C#.
pub fn lexical_guess(g: &Grammar, lexical_patterns: &[(AllomorphId, LexEntryId)], aw: &Word) -> Vec<Word> {
    let table = &g.char_tables[g.strata[aw.stratum.0 as usize].table.0 as usize];
    let feat_width = g.phon_features.len();
    let input_nodes = nodes_of(&aw.shape, table, feat_width);

    let mut out = Vec::new();
    for &(pattern_allo, pattern_entry) in lexical_patterns {
        let pattern_def = g.entries[pattern_entry.0 as usize]
            .allomorphs
            .iter()
            .find(|a| a.id == pattern_allo)
            .expect("lexical_patterns pair must resolve to a real allomorph on its owning entry");
        let pattern_nodes = nodes_of(&pattern_def.shape.shape, table, feat_width);

        // Per-pattern dedup by rendered string (§1.3 step 4 / HISTORY row 12): two DIFFERENT
        // patterns producing the same string must each still yield a guess (cross-pattern
        // homographs are real) — this set resets for every pattern, never shared across them.
        let mut shape_set: HashSet<String> = HashSet::default();
        for m in match_nodes_with_pattern(&input_nodes, &pattern_nodes) {
            let shape_string = render_match(table, &m);
            if !shape_set.insert(shape_string.clone()) {
                continue;
            }

            // §1.3 step 6: fabricate the owning entry — net effect ≡ the pattern's owning entry,
            // except Id/Gloss (= shape_string) and the allomorph list (the fabricated root alone).
            let owning_entry = &g.entries[pattern_entry.0 as usize];
            let fab_stratum = g.morphemes[owning_entry.morpheme.0 as usize].stratum;
            let fab_table = &g.char_tables[g.strata[fab_stratum.0 as usize].table.0 as usize];
            let Ok(fab_shape) = segment_with_features(g, fab_table, &shape_string) else {
                // The rendered text doesn't literally segment against the pattern's own stratum
                // table (no stored fallback shape exists for a freshly rendered guess, unlike
                // `set_root_allomorph`'s real-allomorph fallback) — drop this candidate silently,
                // the same way a real word's own segmentation failure yields no analyses.
                continue;
            };

            // §1.3 step 7: `newWord = input.Clone(); newWord.RootAllomorph = root` — mirrors
            // `Morpher::lexical_lookup`'s clone-and-reroot exactly.
            let mut nw = aw.clone_without_alternatives();
            nw.source = Some(Rc::new(aw.clone()));
            nw.shape = fab_shape;
            nw.stratum = fab_stratum;
            nw.syn_fs = g.fs_interner.get(owning_entry.syn_fs).clone();
            nw.mpr = owning_entry.mpr;
            nw.flags.is_partial = owning_entry.partial;
            nw.root_allomorph = Some(AllomorphId::GUESSED);
            nw.guessed_root = Some(Rc::new(GuessedRoot {
                pattern_allo,
                pattern_entry,
                text: shape_string,
            }));
            nw.morphs = vec![MorphRecord::new(AllomorphId::GUESSED, MorphemeId::GUESSED, 0)];
            out.push(nw);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_shape::CdBits;

    // --- Hand-built node constructors (no Shape/Grammar needed -- match_nodes_with_pattern is a
    // pure function over `&[GuessNode]`) -----------------------------------------------------

    /// A concrete segment node (a real char-def, e.g. table id 1 = "a").
    fn seg(cd: u32) -> GuessNode {
        GuessNode { kind: NodeKind::Segment, char_def: cd, lanes: vec![], cd_set: CdSet::Unrestricted, optional: false, iterative: false, deleted: false }
    }

    /// A mandatory `[Any]` class-reference node: abstract, matches every segment (the "Any"
    /// class's empty-FS / all-members convention, `nat_class_cd_set`'s `Unrestricted` fallback).
    fn any_class(optional: bool, iterative: bool) -> GuessNode {
        GuessNode { kind: NodeKind::Segment, char_def: NO_CHAR_DEF, lanes: vec![], cd_set: CdSet::Unrestricted, optional, iterative, deleted: false }
    }

    /// A boundary node ("+"), optional (as every boundary is after segmentation).
    fn boundary(cd: u32) -> GuessNode {
        GuessNode { kind: NodeKind::Boundary, char_def: cd, lanes: vec![], cd_set: CdSet::Unrestricted, optional: true, iterative: false, deleted: false }
    }

    const A: u32 = 1; // the "a" char-def id used throughout these hand-built fixtures.
    const I: u32 = 2; // a different literal char-def ("i").
    const PLUS: u32 = 9; // the boundary char-def id ("+").

    fn nodes(n: usize) -> Vec<GuessNode> {
        (0..n).map(|_| seg(A)).collect()
    }

    // =============================================================================================
    // Ported from `MorpherTests.TestMatchNodesWithPattern` (MorpherTests.cs:349-449).
    // =============================================================================================

    /// C#'s "test feature matching" block: unifying two nodes that each constrain a DIFFERENT
    /// lane must produce a node carrying BOTH constraints (a genuine merge, not just a
    /// compatibility check) -- ported onto this port's lane model (symbolic bit-lanes) rather
    /// than C#'s arbitrary `StringFeature`s, which this engine does not model at all. Lane 0 =
    /// "feat1" (only the input constrains it, to bit 0b01 = "A"); lane 1 = "feat2" (only the
    /// pattern constrains it, to bit 0b01 = "B"); each side leaves the other lane unconstrained
    /// (all-ones), so the unified node's lanes show both narrowed values.
    #[test]
    fn unify_merges_disjoint_lane_constraints_from_both_sides() {
        let a_is_valuea = 0b01u64;
        let b_is_valueb = 0b01u64;
        let input = GuessNode { lanes: vec![a_is_valuea, u64::MAX], ..seg(A) };
        let pattern = GuessNode { lanes: vec![u64::MAX, b_is_valueb], ..seg(A) };
        let unified = unify_shape_nodes(&input, &pattern).expect("compatible: same char_def, unifiable lanes");
        assert_eq!(unified.lanes, vec![a_is_valuea, b_is_valueb], "both constraints must survive the unify");
    }

    /// "Test sequences": a single 'a' node against a single 'i' pattern node -- different
    /// concrete char-defs never unify.
    #[test]
    fn one_node_against_a_different_literal_fails() {
        let got = match_nodes_with_pattern(&nodes(1), &[seg(I)]);
        assert!(got.is_empty());
    }

    #[test]
    fn one_node_against_itself_matches_exactly_once() {
        let got = match_nodes_with_pattern(&nodes(1), &nodes(1));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0], nodes(1));
    }

    #[test]
    fn two_and_three_nodes_against_themselves_match_exactly_once() {
        for n in [2usize, 3] {
            let got = match_nodes_with_pattern(&nodes(n), &nodes(n));
            assert_eq!(got.len(), 1, "n={n}");
            assert_eq!(got[0], nodes(n), "n={n}");
        }
    }

    /// "Test optionality": `([Any])` -- zero or one real node.
    #[test]
    fn optional_class_matches_zero_or_one_node_but_not_two() {
        let pattern = vec![any_class(true, false)];
        assert_eq!(match_nodes_with_pattern(&nodes(0), &pattern).len(), 1, "zero nodes: skip");
        let one = match_nodes_with_pattern(&nodes(1), &pattern);
        assert_eq!(one.len(), 1, "one node: consume");
        assert_eq!(one[0], nodes(1));
        assert!(match_nodes_with_pattern(&nodes(2), &pattern).is_empty(), "two nodes: no path consumes both");
    }

    /// "Test ambiguity": `([Any])([Any])` -- two independently optional (non-iterative) slots.
    /// One real node matches via EITHER slot -- two distinct paths, both rendering the same
    /// one-node list ("it is up to the caller to eliminate duplicates").
    #[test]
    fn two_independent_optionals_are_ambiguous_for_one_node_but_not_zero_or_two() {
        let pattern = vec![any_class(true, false), any_class(true, false)];
        let zero = match_nodes_with_pattern(&nodes(0), &pattern);
        assert_eq!(zero.len(), 1, "zero nodes: skip both, one path");
        assert_eq!(zero[0], nodes(0));

        let one = match_nodes_with_pattern(&nodes(1), &pattern);
        assert_eq!(one.len(), 2, "one node: two ways to place it (first slot or second slot)");
        assert!(one.iter().all(|m| *m == nodes(1)));

        let two = match_nodes_with_pattern(&nodes(2), &pattern);
        assert_eq!(two.len(), 1, "two nodes: both slots consumed, exactly one path");
        assert_eq!(two[0], nodes(2));

        assert!(match_nodes_with_pattern(&nodes(3), &pattern).is_empty(), "three nodes: only two slots, no path");
    }

    /// "Test Kleene star": `[Any]*` -- zero, one, or more real nodes, always exactly one path
    /// (the iterative-reuse `obligatory` guard suppresses the duplicate skip-after-consume path).
    #[test]
    fn kleene_star_matches_any_count_with_exactly_one_path() {
        let pattern = vec![any_class(true, true)];
        for n in [0usize, 1, 2] {
            let got = match_nodes_with_pattern(&nodes(n), &pattern);
            assert_eq!(got.len(), 1, "n={n}");
            assert_eq!(got[0], nodes(n), "n={n}");
        }
    }

    /// "Test Kleene plus look alike": `[Any]+` -- '+' is a BOUNDARY marker, not a Kleene-plus
    /// operator, so this segments to [mandatory class node, optional boundary node]. Zero real
    /// nodes fails outright (the class node is mandatory); one node matches (boundary skipped,
    /// since there's no boundary in the input); two nodes fails (nothing left to match the
    /// optional boundary against but a second real segment, a kind mismatch).
    #[test]
    fn plus_after_class_is_a_boundary_not_kleene_plus() {
        let pattern = vec![any_class(false, false), boundary(PLUS)];
        assert!(match_nodes_with_pattern(&nodes(0), &pattern).is_empty(), "mandatory class needs >=1 node");

        let one = match_nodes_with_pattern(&nodes(1), &pattern);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0], nodes(1), "the boundary pattern slot is skipped (no boundary in the input)");

        assert!(match_nodes_with_pattern(&nodes(2), &pattern).is_empty(), "second real node can't match a boundary");
    }

    // =============================================================================================
    // Additional unify_shape_nodes coverage (identity narrowing -- not in the C# test, but
    // directly exercised by §4.3's "the narrowing matters for rendering" claim).
    // =============================================================================================

    #[test]
    fn kind_mismatch_never_unifies() {
        assert!(match_nodes_with_pattern(&[seg(A)], &[boundary(PLUS)]).is_empty());
    }

    #[test]
    fn concrete_input_against_a_restricted_class_pattern_narrows_to_the_concrete_id() {
        let pattern_in_class = GuessNode { cd_set: CdSet::Members(CdBits::from_ids([A, I])), ..any_class(false, false) };
        let got = match_nodes_with_pattern(&[seg(A)], std::slice::from_ref(&pattern_in_class));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0][0].char_def, A, "the unified node keeps the concrete identity");

        let pattern_excludes = GuessNode { cd_set: CdSet::Members(CdBits::from_ids([I])), ..any_class(false, false) };
        assert!(
            match_nodes_with_pattern(&[seg(A)], &[pattern_excludes]).is_empty(),
            "A is not a member of the pattern's restricted class"
        );
    }

    #[test]
    fn two_abstract_classes_unify_to_their_intersection() {
        let left = GuessNode { cd_set: CdSet::Members(CdBits::from_ids([1, 2, 3])), ..any_class(false, false) };
        let right = GuessNode { cd_set: CdSet::Members(CdBits::from_ids([2, 3, 4])), ..any_class(false, false) };
        let unified = unify_shape_nodes(&left, &right).expect("2,3 are shared");
        match unified.cd_set {
            CdSet::Members(b) => {
                assert_eq!(b.count(), 2);
                assert!(b.contains(2) && b.contains(3));
                assert!(!b.contains(1) && !b.contains(4));
            }
            CdSet::Unrestricted => panic!("expected a narrowed Members set"),
        }

        let disjoint_right = GuessNode { cd_set: CdSet::Members(CdBits::from_ids([9])), ..any_class(false, false) };
        assert!(unify_shape_nodes(&left, &disjoint_right).is_none(), "disjoint classes cannot unify");
    }

    // =============================================================================================
    // render_match: the rendering half (§1.3 step 3), against a tiny real table.
    // =============================================================================================

    /// Grammar-load-based table probe (mirrors this crate's established test convention, e.g.
    /// `loader_n3_pattern_shapes_gate.rs` / `csharp_port_common`) rather than reaching for
    /// `hc-grammar`'s internal `pub(crate)` raw constructors, which are private outside that
    /// crate.
    fn render_grammar() -> hc_grammar::model::Grammar {
        const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>RenderMatchProbe</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>N</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
  </Language>
</HermitCrabInput>
"#;
        hc_grammar::load(XML).expect("render_match probe grammar loads")
    }

    #[test]
    fn render_match_skips_boundaries_and_renders_concrete_and_class_nodes() {
        let g = render_grammar();
        let table = &g.char_tables[0];
        let a = table.lookup_nfd("a").unwrap().0;
        let i = table.lookup_nfd("i").unwrap().0;
        let plus = table.lookup_nfd("+").unwrap().0;
        let matched = vec![seg(a), boundary(plus), seg(i)];
        assert_eq!(render_match(table, &matched), "ai", "the boundary node must be skipped");
    }

    #[test]
    fn render_match_renders_the_first_matching_table_rep_for_an_abstract_node() {
        let g = render_grammar();
        let table = &g.char_tables[0];
        let a = table.lookup_nfd("a").unwrap().0;
        // An abstract node restricted to {"a"} renders "a" (table document order, first rep).
        let restricted = GuessNode { cd_set: CdSet::Members(CdBits::from_ids([a])), ..any_class(false, false) };
        assert_eq!(render_match(table, &[restricted]), "a");
    }

    #[test]
    fn render_match_of_an_empty_match_is_the_empty_string() {
        let g = render_grammar();
        assert_eq!(render_match(&g.char_tables[0], &[]), "");
    }
}
