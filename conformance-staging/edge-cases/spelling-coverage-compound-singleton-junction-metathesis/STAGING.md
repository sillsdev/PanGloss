# Singleton compound junction metathesis

A metathesis rule swaps a stem-final `p` and a stem-initial `t` across a compound junction with no written boundary. C# accepts `tp` as `Z+Y` through the swap. The singleton FST route compiles and misses it, so it must refuse.

The word list omits `tp` and `stp`. For those two rows C# and Rust HC report the same number of derivations, but they differ in morpheme order: after the swap C# lists the morphs in surface order (`Z+Y`), while Rust HC lists them in the order before the swap (`Y+Z` for some occurrences). So Rust HC does not reproduce the oracle's signatures on those rows. That is a separate HC-Rust divergence on metathesis across a morpheme boundary, not part of this change, and is not specific to compounding; it is tracked in #374. The FST misses exactly those rows against Rust HC, and the gate test asserts the refusal.

The verdict originates in PanGloss FST spelling admission. Signatures and rule traces come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` with suite `a0094a3f97ddaa4a4dc4c4718ba6ea44869e254d`. The grammar is the junction-deletion fixture with only the phonological rule changed. The word list holds the words C# accepts over the grammar's symbols. All lexemes are synthetic.

Upstream PR: pending.
