# nullable-copy-partition-alpha-rtl

Root `tn`, two nullable parts (Any{0,2}) sharing an alpha `f1` variable, output A B A B p, one obligatory RTL `n` deletion. Pinned C# accepts `ttp` as ROOT+SUFFIX; a capture-pruning candidate (eee7a171) lost it.

Pins which capture partition of a nullable repeated copy reaches root lookup: filtering capture
histories before `RemoveDuplicates`-style canonicalization changes the surviving optional-node
arrangement, so any change to affix-match capture state must keep this fixture green.

C# verdict: replayed with the pinned Release conformance driver at Machine `4d9b454d`; all words PASS.
Source of the bug: Rust port (an unlanded capture-memory candidate for PanGloss #189); current main
matches C#. Synthetic, FieldWorks-producible segments, copies, suffix insertion and deletion.
