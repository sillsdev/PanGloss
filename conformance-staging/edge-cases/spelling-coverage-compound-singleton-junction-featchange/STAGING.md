# Singleton compound junction feature change

A feature-change rule rewrites a stem-final segment before a stem-initial segment across the compound junction. C# accepts the rewritten compound (`sp`, `s+p`, `ssp` as `Z+Y`). The singleton FST route compiles and misses it, so it must refuse.

The verdict originates in PanGloss FST spelling admission. Signatures and rule traces come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` with suite `a0094a3f97ddaa4a4dc4c4718ba6ea44869e254d`. The grammar is the junction-deletion fixture with only the phonological rule changed. The word list holds the words C# accepts over the grammar's symbols. All lexemes are synthetic.

Upstream PR: pending.
