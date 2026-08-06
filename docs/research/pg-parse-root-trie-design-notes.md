# pg-parse root_trie.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-parse/src/root_trie.rs` implementation comments so
the source can carry a one- or two-line pointer instead of the full argument. Each section
corresponds to one type or function; the section names it so this doc can be found from either
direction.

## `TrieEdge`: wave-4 pattern-derived edges, and a dead branch kept for historical fidelity

A pattern-derived root-allomorph node (`[NatClass]` in a `<PhoneticShape>`) has `char_def ==
NO_CHAR_DEF` and carries its natural class's member set as a `CdSet` instead of a literal
char-def. Matching a concrete query segment against such an edge goes by set membership (the
`StrRep`-compatibility analog: the set is precomputed as exactly the table entries the class
unifies with) plus lane unifiability — together the port's rendering of C#'s arc-condition
`FeatureStruct` unification against a class-only condition.

Stored-node `OPTIONAL`/`ITERATIVE` flags (`([NatClass])` / `[NatClass]*`) are deliberately not
modeled: C#'s own `AddNode` creates one unconditional arc per shape node, so a stored optional
class node is mandatory in the C# trie too, and matching that behavior exactly is the
parity-faithful choice. This scenario can no longer actually arise for a trie-indexed allomorph:
any node carrying `ITERATIVE`, or `OPTIONAL` on a non-boundary node, makes the whole allomorph a
lexical pattern, and `RootAllomorphTrie::build` excludes pattern allomorphs from the trie
entirely. The historical C# behavior is recorded here only in case a `Segment` node with these
flags ever does reach this edge type again — a defense-in-depth note, not a live path.

## `edge_matches`: why a `NO_CHAR_DEF` query segment is a safe over-approximation

A `NO_CHAR_DEF` query segment has no literal identity to compare, so it passes the char-def gate
against any edge (including a pattern edge, where C# would unify the reinserted node's
class-features against the stored class-features). This port's pattern edges carry their class
identity in `cd_set` with empty lanes, so this is an over-approximation for the
(query-pattern x edge-pattern) corner specifically. It is safe because every trie hit is
re-verified by synthesis-confirm downstream, and the phonological-lane unifiability refinement
still applies in every case, so a closure hit that fails the lane conjunct still correctly rejects.

## `search_segs_opt`: the `NO_CHAR_DEF` query wildcard, and why optional segments are skippable

A query segment whose `char_def` is `NO_CHAR_DEF` arises when an analysis-side phonological rule
re-inserts material from a natural-class-only LHS (e.g. Indonesian `prule5`'s "voiceless obstruent
deletion" reinstates a deleted segment typed only as `nc13`, not any specific literal phoneme).
C# unification treats an unspecified `StrRep` as compatible with any value, so such a node must
match every trie edge whose phonological lanes unify, regardless of the edge's own `char_def` —
the module's "char_def equality AND flat_unifiable" shortcut only holds when the query segment
itself carries a concrete literal identity. Without this, a root like Indonesian `pakai` (whose
first phoneme is exactly this kind of reinstated-but-unidentified segment during analysis of
`memakai`) could never be found: the char_def-equality gate would reject every edge outright.

Optional segments are skippable as the faithful analog of C#'s transducer treating an `Optional`
shape annotation as matchable-or-skippable: an optional input segment (e.g. the epenthetic node a
deletion prule's analysis re-inserts) may be consumed by a trie edge or left unconsumed, so the
underlying root `/ajar/` still matches the analysis shape `aj[?]ar`. Without this, prule-bearing
grammars (Indonesian) would find no root.
