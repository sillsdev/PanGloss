//! The guesser's node matcher (P11 §4.3): a faithful, literal port of C#
//! `Morpher.MatchNodesWithPattern` (`Morpher.cs:597-625`) + the rendering half of
//! `Morpher.LexicalGuess` (`Morpher.cs:522-590` step 3, `match.ToString(table, false)`,
//! `HermitCrabExtensions.cs:317-335`).
//!
//! **Why not `pg-fst`.** C# itself does not route this through its `Matcher`/`Pattern` engine
//! — "the Matcher doesn't preserve the unifications of the nodes" (`Morpher.cs:138-140`) — and
//! this port follows: `match_nodes_with_pattern` is a direct recursive walk over a small
//! resolved node view (`GuessNode`), never through `pg_fst`'s FSA/pattern-compile machinery.
//! (The design doc's §3 blesses reuse of pure lane-arithmetic — this module keeps its own tiny
//! `unify_lanes`/`unify_cd_set` helpers rather than importing `pg_fst::lanes`, so there is no
//! dependency edge onto the FST engine from this file at all, not even an arithmetic one.)
//!
//! `GuessNode` is built from a `Shape`'s INTERIOR nodes — segments AND boundaries (anchors
//! excluded; they are not `ShapeNode`s in C#) — via `nodes_of`. This is the crucial difference
//! from `root_trie.rs`'s trie-edge builders, which filter to `Segment` nodes only: §1.3 step 2
//! is explicit that `MatchNodesWithPattern`'s `nodes` parameter is **every** node of the analysis
//! shape, boundaries included (they only drop out later, at `render_match` time, mirroring
//! `match.ToString(table, false)`'s `includeBdry = false`).
//!
//! Unification of an input node against a pattern node (`unify_shape_nodes`) is `root_trie.rs::
//! edge_matches`'s same shape of predicate — kind equality (the `Type` feature, which the trie
//! never needed since it pre-filters to segments), identity (concrete `char_def` equality /
//! `CdSet` membership / `NO_CHAR_DEF`-query wildcard), and phonological-lane unifiability — but
//! genuinely UNIFIES (narrows) rather than just checking compatibility: the returned node's
//! identity and lanes are the intersection of both sides, because a narrowed node is exactly what
//! `render_match` needs to pick the right candidate representations later (§4.3's "the
//! narrowing matters for rendering").

use std::rc::Rc;

use pg_grammar_model::chardef::{CharDefId, CharDefTable};
use pg_grammar_model::model::{AllomorphId, Grammar, LexEntryId, MorphemeId};
use pg_rules::shape_feat::segment_with_features;
use pg_rules::trace::{TraceHandle, TraceSink};
use pg_rules::word::{GuessedRoot, MorphRecord, Word};
use pg_shape::{CdSet, NodeKind, Shape, NO_CHAR_DEF};
use rustc_hash::FxHashSet as HashSet;

use crate::surface::matching_reps_for_node;

/// A resolved, table-independent view of one shape node — the `(kind, char_def, lanes, cd_set,
/// optional, iterative, deleted)` tuple §4.3 calls for. Built from a `Shape`'s interior nodes by
/// `nodes_of`; also the type `match_nodes_with_pattern` both consumes and produces (a matched
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
    /// elsewhere in this crate/`pg-rules`). Kept as a field for fidelity with C#'s
    /// `ShapeNode.IsDeleted()` check at the one call site that reads it (`render_match`) and so
    /// a future deletion-bearing node source needs no signature change here.
    pub deleted: bool,
}

/// Table-derived lanes for a concrete `char_def`; empty for `NO_CHAR_DEF` or a zero-phon-feature table.
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
            pg_shape::EffectiveCdSet::Members(b) => CdSet::Members(b.clone()),
            pg_shape::EffectiveCdSet::Singleton(_) | pg_shape::EffectiveCdSet::Unrestricted => {
                CdSet::Unrestricted
            }
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

/// Unifies two `(char_def, cd_set)` identity dimensions to the narrowed identity, or `None` if incompatible.
fn unify_identity(a_cd: u32, a_set: &CdSet, b_cd: u32, b_set: &CdSet) -> Option<(u32, CdSet)> {
    match (a_cd != NO_CHAR_DEF, b_cd != NO_CHAR_DEF) {
        (true, true) => (a_cd == b_cd).then(|| (a_cd, CdSet::Unrestricted)),
        (true, false) => cd_set_contains(b_set, a_cd).then(|| (a_cd, CdSet::Unrestricted)),
        (false, true) => cd_set_contains(a_set, b_cd).then(|| (b_cd, CdSet::Unrestricted)),
        (false, false) => unify_cd_set(a_set, b_set).map(|s| (NO_CHAR_DEF, s)),
    }
}

/// Intersects two abstract identity sets (class-membership narrowing, not a phonological lane).
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

/// Lane-wise AND of two flat constraints (absent lane = unconstrained); `None` if any lane empties.
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

/// Unifies one input node against one pattern node; a kind mismatch fails (`Morpher.cs:640-647`).
fn unify_shape_nodes(node: &GuessNode, pattern: &GuessNode) -> Option<GuessNode> {
    if node.kind != pattern.kind {
        return None;
    }
    let (char_def, cd_set) = unify_identity(
        node.char_def,
        &node.cd_set,
        pattern.char_def,
        &pattern.cd_set,
    )?;
    let lanes = unify_lanes(&node.lanes, &pattern.lanes)?;
    Some(GuessNode {
        kind: node.kind,
        char_def,
        lanes,
        cd_set,
        // Inert here: the recursion reads optional/iterative off the original pattern slice only, never off `prefix`.
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
/// rendered strings is the caller's job (`pg-parse::guess::lexical_guess`, P11 chunk 5), exactly
/// as the C# doc comment says: "It is up to the caller to eliminate duplicates."
pub fn match_nodes_with_pattern(nodes: &[GuessNode], pattern: &[GuessNode]) -> Vec<Vec<GuessNode>> {
    match_rec(nodes, pattern, 0, 0, false, &[])
}

/// `n`/`p` are cursors into nodes/pattern; `obligatory` suppresses a duplicate skip-after-iterate path; `prefix` is the matched-so-far list.
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
        let reps = matching_reps_for_node(
            table,
            node.kind,
            node.char_def,
            &node.cd_set,
            &node.lanes,
            false,
        );
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
/// surviving string, cloning `aw` onto it exactly as `pg-parse::morpher::Morpher::lexical_lookup`
/// does for a real root.
///
/// **Table choice (§1.3 step 1, literal from C#):** `table` = `aw`'s OWN stratum's character-
/// definition table — used for BOTH matching (`nodes_of` on both the analysis shape and the
/// pattern's stored shape) and rendering (`render_match`), even though `lexical_patterns` draws
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
/// initialized straight from aw) is unreachable here and not implemented.
///
/// No dedup across patterns or across this function's own output (§1.2: C#'s `.Distinct()` on the
/// call site is a documented no-op — every yielded `Word` is a fresh clone with no `Equals`
/// override — the real dedup is the per-pattern `shape_set` here); duplicates across different
/// patterns are real and survive, matching C#.
pub fn lexical_guess(
    g: &Grammar,
    lexical_patterns: &[(AllomorphId, LexEntryId)],
    aw: &Word,
    trace: &dyn TraceSink,
    parent: TraceHandle,
) -> Vec<Word> {
    // Mirrors `Morpher::lexical_lookup_filtered`'s trace hook (Morpher.cs:378-379): once per call, before pattern matching.
    if trace.is_tracing() {
        let node_parent = aw.trace.unwrap_or(parent);
        trace.lexical_lookup(node_parent, aw.stratum, aw);
    }
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

        // Per-pattern dedup by rendered string: cross-pattern homographs are real, so this set resets for every pattern.
        let mut shape_set: HashSet<String> = HashSet::default();
        for m in match_nodes_with_pattern(&input_nodes, &pattern_nodes) {
            let shape_string = render_match(table, &m);
            if !shape_set.insert(shape_string.clone()) {
                continue;
            }

            // Fabricates the owning entry: same as the pattern's, except Id/Gloss and the allomorph list.
            let owning_entry = &g.entries[pattern_entry.0 as usize];
            let fab_stratum = g.morphemes[owning_entry.morpheme.0 as usize].stratum;
            let fab_table = &g.char_tables[g.strata[fab_stratum.0 as usize].table.0 as usize];
            let Ok(fab_shape) = segment_with_features(g, fab_table, &shape_string) else {
                // No stored fallback shape exists for a freshly rendered guess; drop it silently.
                continue;
            };

            // Mirrors `Morpher::lexical_lookup`'s clone-and-reroot exactly.
            let mut nw = aw.clone_without_alternatives();
            nw.source = Some(Rc::new(aw.clone()));
            nw.shape = fab_shape;
            nw.stratum = fab_stratum;
            nw.syn_fs = g.fs_interner.get(owning_entry.syn_fs).clone();
            nw.mpr = owning_entry.mpr;
            nw.flags.is_partial = owning_entry.is_partial();
            nw.root_allomorph = Some(AllomorphId::GUESSED);
            let runtime = pg_rules::word::RuntimeRoot::Guessed(GuessedRoot {
                pattern_allo,
                pattern_entry,
                text: shape_string.clone(),
            });
            nw.root_runtime_id = Some(shape_string);
            nw.morphs = vec![
                MorphRecord::new(AllomorphId::GUESSED, MorphemeId::GUESSED, 0)
                    .with_runtime_root(runtime),
            ];
            out.push(nw);
        }
    }
    out
}

#[cfg(test)]
mod tests;
