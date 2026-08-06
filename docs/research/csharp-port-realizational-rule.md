# `csharp_port_affix_template::realizational_rule`: what the two grammars pin down

This test ports `AffixTemplateTests.RealizationalRule`: three `RealizationalAffixProcessRule`s
(`ed_suffix` realizing `tense=past`, `s_suffix` realizing `pers=3 & tense=pres`, `evidential`
realizing `evidential=witnessed`) in a two-optional-slot `verb` template, with no ordinary cascade
rules — the realizational rules are template-slot-only in C# too. The C#
`AssertSyntacticFeatureStructsEqual` companions have no public Rust surface (`ParseOutcome` exposes
morphemes and surface, not the word's syntactic feature struct) and are omitted; the morph
assertions are transcribed verbatim.

## What each assertion pins down

- **`sid` parses to nothing.** `si` (`bl1`) plus PAST synthesizes the feature struct
  `{V, tense:past}`, which subsumes family-mate `bl2`'s lexical feature struct. `Word.CheckBlocking`
  swaps in a `sau`-shaped word that no longer matches the surface, killing the parse. This is the
  realizational-rule × lexical-family interaction: the irregular form blocks the over-regularized
  one.
- **`sau` parses to `bl2`.** The irregular form itself: a bare root, both slots optional.
- **The second grammar's `sagzv` realizes `3SG+WIT`.** Here `evidential`'s realizational feature
  struct gains `tense:pres`. For this to parse, `evidential`'s and `s_suffix`'s realizational
  feature structs must **unify** on the shared `tense` feature during analysis, not overwrite one
  another. This is the case that pins the unify-not-overwrite semantics of `real_fs` accumulation.
