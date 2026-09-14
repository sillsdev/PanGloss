# 027 — Guesser matcher bypasses the FST engine, and is bug-compatible with C#'s single-owning-entry fabrication

## Kind
Representational.

## Status
Open (both halves are permanent, deliberate design choices).

## C# site
`Morpher.MatchNodesWithPattern` (`Morpher.cs:597-625`) and `Morpher.LexicalGuess`
(`Morpher.cs:522-590` step 3, `match.ToString(table, false)`,
`HermitCrabExtensions.cs:317-335`).

## Rust site
`pg_parse::guess` (`rust/crates/pg-parse/src/guess.rs`).

## What differs
**Architectural bypass (representational, not behavioural):** C# itself does not route guesser
matching through its `Matcher`/`Pattern` engine — "the Matcher doesn't preserve the unifications of
the nodes" (`Morpher.cs:138-140`) — so this module is, correctly, a from-scratch literal port of
`MatchNodesWithPattern`'s own node-by-node unification loop, not built on `pg-fst`. Anchors are
excluded from the walk in both engines, since anchors are not `ShapeNode`s in C# either.

**A named, documented simplification (bug-compatible, not a bug):** C# takes the
`if (lexicalPattern.Morpheme != null)` branch's owning-entry attribution literally — every guessed
match takes its OWNING entry from the pattern's own `Morpheme` field (`Morpher.cs:564-579`), which in
Rust's data model is always a real, well-defined `LexEntryId` (every Rust lexical pattern has a real
owning entry by construction), so the `if` in C# is unconditionally true in every case this port's
grammars can construct. The module doc calls this out explicitly as "documented, not a gap."

**Table choice, ported literally including its literalness:** the guesser's character table choice
(`table = aw`'s OWN stratum's character-definition table) is C#'s own literal behavior, not a
generalization — the doc explicitly notes this does not merge feature systems from every stratum,
matching C#'s exact (and, by implication, similarly narrow) choice.

## Can it change a parse?
No — this is a faithful, node-for-node port of a mechanism C# itself keeps outside its general
pattern-matching engine, and the "simplification" is provably a no-op given Rust's data model (every
lexical pattern has a real owning entry; the C# conditional it corresponds to is trivially satisfied
in exactly the same way on every reachable Rust grammar).

## Evidence
`pg-parse/src/guess.rs`'s own unit tests port `MorpherTests.TestMatchNodesWithPattern`
(`MorpherTests.cs:349-449`) directly, including "unifying two nodes that each constrain a different
lane must produce a node carrying both constraints." One additional Rust-only test
(`unify_shape_nodes`'s identity-narrowing coverage) is explicitly marked in its own comment as "not
in the C# test" — an intentional coverage addition beyond the port, not a divergence.

## Upstream
Not applicable — this is an architecture-matching port with no observable-behavior gap to converge
on.

## Notes
No dedup is applied across guesser patterns or across one call's own output (`Morpher.cs`'s own
`.Distinct()` is documented as a no-op on fresh clones with no `Equals` override, so both engines
skip deduplication identically here — consuming the raw output directly is faithful, not merely
convenient).
