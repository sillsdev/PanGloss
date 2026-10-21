# 103: A re-inserted segment-class node had no identity on a featureless grammar

Kind: behavioural (search only on every grammar measured; see "Can it change a parse?").
Status: fixed-in-rust. C# was already correct; no Machine issue (Rust-only implementation bug).
Evidence: staged fixture `conformance-staging/edge-cases/class-deletion-reinsertion-strrep/`
(oracle-recorded); counter regressions in `pg-parse`; before/after batch comparisons below.

## C# site

`NarrowAnalysisRewriteRuleSpec.Unapply` re-inserts each LHS constraint of a deletion rule as an
optional node carrying that constraint's feature structure. For a `SimpleContext` over a
`SegmentNaturalClass` that structure is the union of the members' (`SegmentNaturalClass.cs`), and a
segment defined without features has a structure of `Type` plus its `StrRep` alone
(`CharacterDefinitionTable.cs::AddSegment`). So on a featureless grammar the node for `[p b]` is
`StrRep ∈ {p, b}`, and everything later that unifies with it, root lookup included
(`RootAllomorphTrie`, `UseUnification = true`), can take only `p` or `b` there.

## Rust site

`pg-rules/src/rewrite.rs::new_seg_node_dirty` gives a `PatternNode::Context` node no char-def and
an unrestricted char-def set, and a featureless grammar has no feature lanes to fall back on, so the
re-inserted node matched every segment. `pg-parse/src/root_trie.rs` also ignored a query node's
char-def set: a char-def-less query node was a wildcard. A deletion rule over a class therefore let
analysis unapply affixes against, and root lookup find, any segment at every site after its
environment. Synthesis rejected each such candidate, at a large cost: on a private featureless
Austronesian project whose variant deletes two classes of stops after nasals, most of the parse
work went into synthesizing roots C# never looks up.

## What changed

`rewrite.rs::with_class_identity` gives a node re-inserted by `ana_narrow_deletion` the class's member set, only when the grammar has no authored phonological
features, the LHS position is a segment natural class, and the node has no char-def of its own.
`MutShape`, `to_shape` and the affix analysis paths already carried such a set. `RootAllomorphTrie`
now records whether its grammar is featureless and, only then, lets a char-def-less query node with
a member set take an edge only when the edge's char-def (or pattern set) intersects it.

Fire counters: `rewrite::take_class_identity_nodes` (nodes inserted with a member set) and
`root_trie::take_identity_refusals` (trie edges refused by the set alone); `pangloss batch` prints
both per word under `HC_STEP_STATS=1` as `CLASS_IDENTITY_NODES` and `IDENTITY_REFUSALS`.

## Can it change a parse?

Only by removing a match C# never makes. Lost parses: the member set is exactly C#'s `StrRep`
union for the class, it is applied only where C#'s node has that union (a featured grammar's node
has no `StrRep`, so nothing changes there), the node stays optional, and an edge is refused only
when the edge's char-defs and the set are disjoint, which is when C#'s unification fails.
Admitted parses: the change only removes matches and lookups, so it cannot add an analysis.
A removed candidate could have changed a parse only if Rust's synthesis had accepted a root C#
cannot reach, which would itself have been a divergence; no compared row changed.

The trie honours any char-def-less query node's member set, not only this one, so every analysis-side
producer of such a set on a featureless grammar must equal C#'s StrRep set for that node. They are:
this re-insertion (the class union); `morph.rs::untruncate` (an uncaptured `SimpleContext` part,
whose C# node is the class structure, so the same union; a feature class on a featureless grammar
yields `Unrestricted`); `rewrite.rs::MutNode::add_strrep` in a narrowing union (the union of the
two nodes' StrReps, as `FeatureStruct.Union` gives); and copies of an existing node's set through
`morph.rs::cd_set_of` and `MutShape::for_rewrite`. A node whose features or identity a rule
changed is reset to `Unrestricted` (`rewrite_identity`, entry 091). On the measured featureless
lists without a class deletion rule the trie refused no edge at all.

Narrowing leftovers from `ana_narrow_general` keep no identity: no measured grammar or fixture
exercises a class LHS there, so that over-approximation is left as it was.

## Coverage

`pg-parse/tests/conformance_fixtures_gate.rs::a_reinserted_class_node_looks_up_only_class_members_on_a_featureless_grammar`
parses the staged fixture's `amili` and `uma` and requires that `kili` and `umka` are never looked
up, that some node carried the class and that the trie refused an edge. With
`with_class_identity` disabled it fails (`no re-inserted node carried the class`); with the trie's
identity check disabled it fails (`root lookup refused no edge`). `pg-parse`
`root_trie::tests::a_char_def_less_query_node_with_an_identity_takes_only_member_edges` pins the
trie rule directly. The staged fixture itself was recorded from hc.dll and passes before and after:
it guards against an identity narrower than the class.

Parse neutrality and work, release builds of this change and of its parent, `pangloss batch
--threads 1`, default step cap, no wall timeout, `HC_STEP_STATS=1`; `parse_compare.py --multiset`
IDENTICAL on every row, no caps, no timeouts:

| List | Words | Parser work before -> after | Steps before -> after | Trie refusals |
|---|---|---|---|---|
| private featureless project, deletion variant, held | 7,206 | 303,966,311 -> 16,220,190 (94.7% less) | 560,008 -> 365,882 | 211,734 |
| same, dev | 563 | 31,861,632 -> 1,095,331 (96.6% less) | 40,868 -> 27,776 | 15,298 |
| same, nonwords | 200 | 2,774,819 -> 220,705 (92.0% less) | 9,386 -> 8,190 | 2,703 |
| same project, two variants without class deletion, three lists each | 7,969 each | unchanged | unchanged | 0 |
| Indonesian | 70 | unchanged | unchanged | 0 |
| Amharic | 673 | unchanged | unchanged | 0 |
| Sena | 6,146 | unchanged | unchanged | 0 |
| Aweti | 206 | unchanged | unchanged | 0 |

The 16 words of the deletion variant's held list that stopped at a 1 s wall limit before this change
need 99.9% less parser work (96,155,091 -> 57,438 work units); the list's costliest word fell from
14,555,867 to 14,281 work units. Mbugwe was not compared; like the other reference grammars it has phonological features, so neither new branch can run on it.

The PR #480 head fixture run (machine 72dbd4f9) shows no new failure against the recorded baseline:
the same five `clitic-rule-designation` words and the same 14 dual-homed staged fixtures.

## C# transfer

None needed: C# already gives the node its class's structure. Before this change HC-Rust did more
work than C# on such grammars; the change removes that gap rather than adding an optimization.
