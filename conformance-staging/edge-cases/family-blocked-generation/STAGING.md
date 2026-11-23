# family-blocked-generation

Pins sillsdev/PanGloss#423. Stem family `famF` holds A (`aa`) and B (`aat`); suffix T appends `t`, so A+T and
B share the surface `aat` and B blocks A+T. Parsing reports only `B` on both engines (the rows in `words.yaml`).
The bug is forward: C# `Morpher.GenerateWords(A, [T])` returns `[aat]` (B's word, the relative that blocked it);
PanGloss returned `[]`. That forward expectation is not expressible in `words.yaml` (parse rows only), so it is
asserted by `family_blocked_generation_returns_the_relative_word` in
`rust/crates/pg-parse/tests/conformance_fixtures_gate.rs`, with the C# value taken from a pinned-C# run at
machine 74ee09bf (reported in the issue). Ask the conformance lead whether the upstream contract can carry a
generation row; if not, only the parse rows go upstream.

No `upstream:` line yet: the conformance lead opens the sillsdev/fieldworks-parser-conformance PR (parse rows from a
fresh pinned-C# replay), then this fixture gains `upstream: <PR URL>` and leaves `upstream-baseline.txt`.
