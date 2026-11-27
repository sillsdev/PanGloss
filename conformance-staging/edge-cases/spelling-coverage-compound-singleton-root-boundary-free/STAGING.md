# Singleton root boundary without a compounding rule

A root spells a boundary inside itself and the grammar has no compounding rule. C# accepts `t+a` as `Z` and `st+a` as `PRE+Z`. The singleton FST route drops the boundary from the root spelling and misses them, so it must refuse.

The verdict originates in PanGloss FST spelling admission. Signatures and rule traces come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` with suite `a0094a3f97ddaa4a4dc4c4718ba6ea44869e254d`. The grammar is the junction-deletion fixture with only the phonological rule changed. The word list holds the words C# accepts over the grammar's symbols. All lexemes are synthetic.

upstream: not-producible: row 29 a root spells + between two of its own segments
