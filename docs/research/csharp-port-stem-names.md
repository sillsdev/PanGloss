# `csharp_port_lex_entry::stem_names`: the shared-region exemption

This test ports `LexEntryTests.StemNames`. The `stemname` root has feature struct
`{V, head:{tense:pres}}` and three allomorphs — `san` (unrestricted), `sad` (restricted to stem
name `sn1`, regions `{pers:1}|{pers:2}`), and `sap` (restricted to stem name `sn2`, regions
`{pers:1}|{pers:3}`) — supplied via `build_grammar_w5`'s extra-lexicon block. Three suffix rules
assign `pers` 1/2/3 via `OutputHeadFeatures`, mirroring the C# test's `ed`/`t`/`s` suffixes.

## The two things it proves

- **`sanɯd`/`sant`/`sans` all fail; bare `san` parses.** The unrestricted `san` allomorph is
  blocked wherever either named stem's region claims the suffixed word's `pers` value — it may
  only surface where no named stem name applies at all.
- **`sadɯd` and `sapɯd` are both valid, despite carrying the same `pers=1`.** `sn1` and `sn2` share
  the `{pers:1}` region, and `StemName.IsExcludedMatch` exempts a region two stem names share:
  an allomorph is excluded only by a region *no other applicable stem name also claims*. Without
  that exemption, `sn1` and `sn2` would each block the other's allomorph at `pers=1`, and neither
  `sad` nor `sap` could ever surface there.
