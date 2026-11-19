# Singleton compound word-final change (admitted side)

A one-node feature change applies only at a word-final anchor, in the same stratum as the compounding rules. It cannot reach a stem edge inside a compound, so the singleton FST route admits the grammar and must stay exact against Rust HC.

This pins the admitted side of the `compound_junction_phonology` predicate: widening the predicate, or changing how it reads a one-node rule with only a word-boundary environment, moves this cell off `oracle_exact` on the backend scoreboard. The refusal fixtures sit beside it (`...-junction-featchange`, `-junction-deletion`).

The word list holds the words C# accepts over the grammar's symbols, plus every shorter-than-three-symbol word it rejects. Signatures and rule traces come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` with suite `a0094a3f97ddaa4a4dc4c4718ba6ea44869e254d`. All lexemes are synthetic.

Upstream PR: pending.
