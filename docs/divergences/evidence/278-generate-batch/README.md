# Batch generation surfaces from C# HermitCrab (sillsdev/motif#278)

`rust/crates/pg-cli/tests/generate_batch_target.rs` takes its surface literals from `hc-generate.tsv`, the output of
C# `Morpher.GenerateWords` (`hc.dll` from the pinned `hc-conformance` driver, machine pin 1ddef2bd) run by `observer.cs`
(a console project: `observer.csproj`, with `<HintPath>` pointing at that driver's directory) over `grammar.xml`.

`grammar.xml` is an HC XML encoding of the test snapshot (`docs/divergences/evidence/071/kat-snapshot.json` plus the
`snapshot()` edits in the test). It is hand-authored, not exported by FieldWorks, so it carries the same
grammar-equivalence caveat as any hand-written fixture. What it models, matching the snapshot:

- the `ranna` root as two lexical entries (`root-id`, `root2-id`), one per stem MSA, because the snapshot sets
  `mergeMsas=false` and gives the entry a second MSA;
- the `-s` inflectional suffix with Number=plural, its `afterVowel` environment as a `RequiredEnvironments` on the
  morphological subrule (an allomorph environment, as FieldWorks' HCLoader encodes it), in the `Tense` slot of the final
  `VerbTemplate` (Verb);
- the `k` derivational suffix (verb to verb) as a stratum rule, carrying the same `afterVowel` environment the
  cloned snapshot allomorph keeps;
- the `Voicing` rule (t to d after a vowel) and the three strata Morphology, Clitics, Surface.

The only intended difference from the snapshot is the phoneme inventory: `grammar.xml` adds `r`, `n` and `s`.

- `r`, `n`: HC cannot segment the root `ranna` without them, while PanGloss reads the snapshot's missing letters as
  provisional segments.
- `s`: HC cannot segment the suffix `s` (and so `rannas`) without it, for the same reason.

`hc-generate.tsv` columns are root, morphemes in HC order, and the sorted surfaces joined by `;` (empty means none).
PanGloss's `generate` on the same `grammar.xml` agrees with it on all ten rows.

| morphemes (HC order) | C# surfaces, either root |
| --- | --- |
| (none) | none |
| infl | rannas |
| deriv | none |
| deriv, infl | none |
| infl, deriv | none |

The template encoding is the faithful one for the snapshot, whose suffix sits in a template slot; a flat encoding (both
rules in the stratum, no template) gave `rannask` for `deriv, infl`.
