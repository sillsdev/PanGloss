# 010 — Root lookup needs unification, not char-def identity, on feature-bearing tables

## Kind
Behavioural.

## Status
Fixed-in-rust.

## C# site
`CharacterDefinitionTable.Add` (`CharacterDefinitionTable.cs`) and `FeatureStruct.IsUnifiable`.
`Add` attaches a `StrRep` disjunction to a segment's char-def **only when the segment has zero
authored phonological features** (e.g. Sena-style grammars); a feature-bearing segment (Indonesian/
Amharic-style) gets `Type + features` and **no `StrRep` at all**.

## Rust site
`pg_grammar::chardef::CharDefTable` (`rust/crates/pg-grammar/src/chardef.rs`, `unif_closure`/
`unifiable_cds`) and its two consumers, `pg_parse::root_trie::edge_matches`'s concrete×concrete arm
and `pg_parse::surface::matching_reps_for_node`'s concrete-identity gate.

## What differs
Because C#'s root lookup is pure `FeatureStruct.IsUnifiable` with no separate char-def-identity
gate, two *distinct* concrete char-defs whose feature structs unify legitimately cross-match root
lookup in C# even within one character table — whenever the table is feature-bearing. Before the
fix, Rust's root lookup and surface-representation matching used char-def identity (or lane
equality against a single node's own lanes) as the gate, which is correct for a zero-feature table
(Sena-style, where C#'s `StrRep`-based identity gate genuinely applies) but wrong for a
feature-bearing one (Indonesian/Amharic-style), where C# has no identity gate at all.

Concretely: root "10"'s allomorph is `"ga̘p"` (ATR-, a distinct `char_def` from surface "gap"'s plain
"a"). The rule under test never touches either char-def directly (neither ever becomes
`NO_CHAR_DEF`), so entry 009's fix does not reach this case — the two char-defs are simply different,
permanently, and only their *feature structs'* unifiability (not their identity) makes them match in
C#.

**Precise rule each side follows:** C# has exactly two regimes selected by whether a table's
segments carry phonological features at all — identity-based matching where `StrRep` exists
(zero-feature tables), unification-based matching where it doesn't (feature-bearing tables). Pre-fix
Rust applied only the identity-based regime universally.

## Can it change a parse?
Yes: on any feature-bearing character table, a lexical root whose char-def differs from — but
whose features unify with — a word's current segment can never be found by root lookup, a category
of recall loss specific to grammars like Indonesian/Amharic that (unlike Sena) author phonological
features on segments.

## Evidence
`csharp_port_rewrite.rs::anchor_rules` sub-case (1) fails (missing root "10") before the fix, passes
after. Fix: a build-time unifiability closure over a feature-bearing table's segment char-defs,
gated on `!PhonFeatureSystem::is_empty()` so zero-feature grammars (Sena) are untouched; both
`root_trie::edge_matches` and `surface::matching_reps_for_node` fall back to that closure on an
equality miss.

**2026-09-02 update (`addbdba7`):** the closure in `matching_reps_for_node` was itself still a
char-def-identity-or-closure GATE applied unconditionally before the feature-lane check ran — correct
for a zero-feature table (identity/`StrRep` is C#'s own regime there) but an unnecessary, occasionally
wrong, extra filter on a feature-bearing table, where C# has no identity gate of any kind
(`GetMatchingStrReps` matches by `FeatureStruct.IsUnifiable` alone). Exposed when
`conformance-staging/edge-cases/segment-natural-class-table-binding` became loadable by hc.dll: for
word `g`, hc.dll reports a SECOND analysis (`"ROOT1|z"`, a cross-table respelling — table t0's "z" and
table t1's "g" are the same feature bundle spelled by two tables) that pre-fix Rust's identity/closure
gate on `matching_reps_for_node` dropped, because the node's table-local char_def index (from table
t0) was compared against table t1's own closure. Fixed by branching on `lanes.is_empty()`: a
zero-feature table keeps the original identity-or-closure gate bit-for-bit; a feature-bearing table
now skips char-def identity entirely and lets the caller's own `flat_unifiable` lane check decide
membership, matching C#'s single, uniform regime exactly. `words.yaml`'s `g` entry now records both
analyses. (Entry 043 documents a similarly-discovered, but mechanistically unrelated, divergence
found the same way on a different fixture in the same investigation.)

## Upstream
None, not applicable — pure Rust-side bug; C# was already correct (its own two-regime split is
implicit in how `CharacterDefinitionTable.Add` decides whether to attach `StrRep` at all).

## Notes
The apparent fix looked, at first, like it would need a multi-table/cross-stratum redesign; the real
root cause was narrower than that. Worth remembering when a similar-looking "root can't be found
across tables" symptom recurs: check whether the grammar's tables are feature-bearing before
assuming a stratum/table-boundary redesign is needed.
