# Singleton compound word-final change in an inner stratum (admitted side)

The same word-final one-node feature change, but the rule belongs to the root stratum and the compounding rules to a later stratum, with lexical entries in both. The compound junction is then a word edge for the inner stratum's rule, and the singleton FST route admits the grammar and must stay exact against Rust HC.

This pins the admitted side of the `compound_junction_phonology` predicate across a stratum boundary. A change to cyclic rule application, or to how the predicate decides which rules a stratum applies, moves this cell off `oracle_exact` on the backend scoreboard.

The word list holds the words C# accepts over the grammar's symbols, plus every shorter-than-three-symbol word it rejects. Signatures and rule traces come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` with suite `a0094a3f97ddaa4a4dc4c4718ba6ea44869e254d`. All lexemes are synthetic.

Upstream PR: pending.
