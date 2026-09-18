# Formats — written for outside readers

This directory documents the file formats PanGloss produces, for readers outside this repository:
a linguist who has never touched PanGloss's own code, or a chat model reading a Motif Handoff
folder someone dragged in front of it. Every document here explains a format on its own terms —
what the fields mean, what produced them, how to reason about what you're looking at — without
assuming you have this repository open or know its internal vocabulary.

- [`grammar-format.md`](grammar-format.md) — the grammar snapshot (`grammar.json`): every rule,
  part of speech, phoneme, and lexicon entry PanGloss compiled from a FieldWorks project.
- [`hc-mechanics.md`](hc-mechanics.md) — how the HermitCrab engine actually parses a word against
  that grammar, and the ways a grammar author's intent can go wrong without an error message.
- [`trace-format.md`](trace-format.md) — the JSON `pangloss parse --trace` writes: the full,
  unmerged derivation tree behind one word's parse or failure.

This is **not** the same audience, and not the same material, as `../research/`, `../history/`,
and `../divergences/`. Those three are written for people porting or maintaining HermitCrab itself
— they cite Rust file and line numbers, track measured performance, and record where this port is
known or suspected to disagree with the C# original it was built from. A linguist's chat model has
no use for a divergence census or a build-order design note; sending it there instead of here is
how a reader ends up drowning in port-audit material when what they needed was "what does this
field mean."
