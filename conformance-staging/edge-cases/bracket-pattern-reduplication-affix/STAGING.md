# bracket-pattern-reduplication-affix

Pins PanGloss issue 148: an affix allomorph whose Form is a reduplication bracket pattern (`[C^1][V^1]`) is loaded as a process allomorph, as C# HermitCrab loads it, instead of being skipped with `grammar.allomorph.reduplication-invalid`'s predecessor `reduplication-unsupported`.

Oracle: C# hc.dll built from sillsdev/machine `74ee09bf` (`parse` command over every word). It accepts exactly the words marked with parses and rejects the three `expect_fail` words; `kukumama` has two parses there and here. Signatures and `rules:` come from HC-Rust, not hc.dll's `parse` output (which prints morphs, not batch signatures).

FieldWorks producibility: read from `HCLoader.cs` (FieldWorks 936ae91fe7) lines 1494-1566 and 1664-1710; the bracket-pattern importer test is `pg-parse/tests/bracket_pattern_affix_form.rs`, which builds the FieldWorks-side snapshot and checks the same words. No FieldWorks witness project is committed.
