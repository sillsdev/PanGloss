# Fixture notes: a synthetic proposer recall gap, genuinely produced by both engines

`pg-cli/src/assess.rs::a_synthetic_proposer_recall_gap_is_attributed_to_the_proposer_not_the_grammar`
needs a case where HermitCrab produces an analysis that the foma-confirm pipeline cannot, so that
`attribute_causes` has a real `ProposerRecallGap` to classify — not a mocked one.

## The fixture

Reuses `pg-parse/tests/guesser_gate.rs`'s grammar (itself a port of C#'s
`MorpherTests.AnalyzeWord_CanGuess_ReturnsCorrectAnalysis`). Its only lexical entry is a guess
pattern (`[Any]*`, `RootAllomorphDef::is_pattern = true`). Lexical entries marked as patterns are
excluded from both the real-lexicon trie and, by the same exclusion, the compiled foma network,
since the lexc/FST emitter walks the same real, non-pattern entries the trie does.

For the surface word "gag": HermitCrab's guess branch (`ParseOptions::with_guess_root(true)`,
which only runs when the ordinary lexical search returns nothing) fabricates a root and returns
one analysis. `FomaAnalyzer`, built from the identical grammar with no guess capability, has
structurally nothing to propose. Both pipelines are the real, unstubbed engines running against a
real compiled grammar — nothing here is mocked or hand-fed a result.

## Rejected alternative

A hand-built two-rule chained-reduplication grammar was tried first, on the theory that deep nested
reduplication is a harder case for the FST proposer. It was not: `FomaAnalyzer` recovered the exact
same doubly-reduplicated analysis HermitCrab did, so it produces no recall gap and was dropped in
favor of the guess-pattern fixture above.
