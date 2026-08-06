# The shared C#-suite port fixture: two intentional simplifications

`pg-parse/tests/csharp_port_common/mod.rs` ports `HermitCrabTestBase.cs`'s phonological/syntactic
feature systems and lexicon into one merged XML grammar fragment, reused by every
`csharp_port_*.rs` file in this crate. It makes two simplifications versus the C# base class, both
verified per-test while reading the C# source (not merely assumed) to have no effect on any
assertion any ported test makes.

## One character-definition table, not three

C# spreads segments across `Table1` (has `asp`, no `ATR`) and `Table3` (has `ATR`, no `asp`), and
stratifies `Allophonic`(Table1)/`Morphophonemic`(Table3)/`Surface`(Table1) accordingly.
HermitCrab's rule-application layer operates on `FeatureStruct`s, not table identity;
`CharacterDefinitionTable` only matters at the text-to-shape boundary. This fixture's one table is
the union — every segment gets both an `asp` value and an `ATR` value — so every pattern from
either C# table still matches the same segments.

**Exception:** `cA` ("a") deliberately does not get an `ATR` value pinned. Every ported test
segments surface words the way C# segments against Table1, whose "a" carries no ATR feature at
all; only `cAUnderdot` ("a̘", Table3's ATR- "a") is ATR-pinned. This is what makes the cross-char-def
case (ATR-unspecified "a" vs. ATR- "a̘") expressible. C#'s Table3-"a" (ATR+) is the one thing this
merged-table approximation cannot represent simultaneously — acceptable as long as no ported test
conditions a rule on ATR.

## One stratum, not three

C# rules that live on `Allophonic`/`Surface` here just live on the same stratum as the
`Morphophonemic` ones. Cross-stratum recoding is never observably exercised by any test ported
here: each test's rules are added to one or two of the three strata, but never in a way that
depends on a stratum boundary's own semantics beyond "these rules run before those rules", which a
single unordered stratum preserves.

## Gloss-join convention

`<MorphemeId>` is set to each rule/entry's C# `Gloss`/id (e.g. `"32"`, `"PAST"`) so
`pg_parse::Morpher`'s morpheme-join signature reproduces `AssertMorphsEqual`'s gloss strings —
joined on `"+"` here versus C#'s `" "`; callers translate via `morphs_set`.

## Citation convention

Every lexical entry transcribed in `LEXICON_XML` cites the C# `AddEntry` call it ports
(`HermitCrabTestBase.cs`), so a reader can cross-check spelling, POS, and features against the
source.
