# 120: A reduplication form token with no usable `^` index skips the allomorph in PanGloss, and crashes or dangles in C#

Kind: behavioural (loader only; C# yields no usable allomorph either way).
Status: permanent. Rust skips with a warning where C# fails or loads a broken allomorph.

## What differs

An affix form written as a reduplication bracket pattern (`[C^1][V^1]`) loads as a process allomorph in
both engines. The two differ only on a malformed token that C# does not handle:

| Input | C# | PanGloss |
|---|---|---|
| A `[C]` token with no `^` index in the form | `LoadReduplicationPatterns` throws `ArgumentOutOfRangeException` on the missing `^`; this is not the `InvalidReduplicationFormException` the loader catches, so it is an uncaught loader exception | the allomorph is skipped with `grammar.allomorph.reduplication-invalid` |
| An output token `[C]` with no `^` where no pattern is named `[C]` | `LoadReduplicationOutputActions` emits a `CopyFromInput` naming a part that does not exist; synthesis cannot use it | the allomorph is skipped with `grammar.allomorph.reduplication-invalid` |

No parse changes where C# runs: a form C# cannot load produces no analyses through that allomorph, and
PanGloss produces none either. The difference is that PanGloss keeps loading the rest of the grammar and
names the allomorph.

A `^N` index with no matching pattern is not a divergence: both engines warn and drop that copy
(`HCLoader.cs:1689-1706`).

## C# site

FieldWorks `HCLoader.cs` `LoadFormAffixProcessAllomorph`, `LoadReduplicationPatterns` and
`LoadReduplicationOutputActions` (1494-1566, 1664-1710); `IsValidEnvironment` (1252-1275).

## Rust site

`pg-grammar/src/compile/reduplication.rs` (`reduplication_patterns`, `output_actions`), reported as
`grammar.allomorph.reduplication-invalid`.

## Coverage and verification

`pg-grammar/src/compile/tests.rs`: `form_token_without_an_index_is_skipped_with_a_warning`,
`malformed_reduplication_pattern_is_skipped_with_a_warning`. Source reading only: hc.dll cannot load a
FieldWorks project on Linux, and HC XML has no form tokens to carry the malformed input.
