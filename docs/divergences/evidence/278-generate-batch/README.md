# Batch generation surfaces from C# HermitCrab (sillsdev/motif#278)

`rust/crates/pg-cli/tests/generate_batch_target.rs` takes its surface literals from `hc-generate.tsv`, the output of
C# `Morpher.GenerateWords` (`hc.dll` from the pinned `hc-conformance` driver, machine pin 1ddef2bd) run by `observer.cs`
(a console project: `observer.csproj`, with `<HintPath>` pointing at that driver's directory) over `grammar.xml`.

`grammar.xml` is an HC XML encoding of the test snapshot's `ranna` root, its `-s` inflectional suffix (in a final verb
affix template slot, after a vowel) and the `k` derivational suffix (verb to verb). It is hand-authored from the
snapshot, not exported by FieldWorks, so it carries the same grammar-equivalence caveat as any hand-written fixture.
PanGloss's `generate` on the same `grammar.xml` agrees with `hc-generate.tsv` on all five morpheme lists.

| morphemes (HC order) | C# surfaces |
| --- | --- |
| infl | rannas |
| deriv, infl | none |
| infl, deriv | none |

A first flat encoding (both rules in the stratum, no template) gave `rannask` for `deriv, infl`; the template encoding
is the faithful one for the snapshot, whose suffix sits in a template slot.
