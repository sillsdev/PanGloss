# Analysis identity in Machine, HermitCrab, and LibLCM

## Question

What should PanGloss compare when it says that Rust HermitCrab, the FST-plus-Rust pipeline, and C# HermitCrab returned the same set of analyses? In particular, is `IMorpheme.Id` a safe cross-engine key when `<MorphemeId>` is absent or repeated?

## Conclusion

Keep Machine's linguistic equality contract: an analysis is the ordered morpheme sequence, root position, and category/POS. Do **not**, however, serialize `IMorpheme.Id` blindly as the cross-engine morpheme key. Machine documents that property as unique, but its XML loader neither requires nor validates `<MorphemeId>` and real supported grammars omit it. In those grammars, Machine's own `WordAnalysis.Equals` can collapse distinct analyses.

For authoritative PanGloss comparison, project the same Machine semantics onto stable source-object keys:

- Direct HermitCrab XML: each morpheme's XML `id` attribute (`MorphemeInfo.xml_key` in Rust), namespaced by package/grammar identity.
- FieldWorks/LibLCM input: the source LCM object GUID retained by the snapshot/compiler. PanGloss currently models a HermitCrab morpheme from an MSA and uses that MSA GUID as `xml_key`; keep that mapping unless an audited FieldWorks exporter establishes a different mapping.
- POS/category: the stable source feature-symbol ID/GUID, not Rust's grammar-local numeric ordinal.

`<MorphemeId>`, gloss, shape, display text, timing, traces, and duplicate counts remain diagnostic fields. They are not object identity. Rust's dense numeric IDs are efficient runtime handles, but comparison must resolve them back to stable source keys.

This is a narrow compatibility correction in the **comparison projection**, not a proposal to change HermitCrab's linguistic model or Machine's public `WordAnalysis` API.

## Primary-source findings

### 1. Machine defines the linguistic analysis tuple

Machine's `WordAnalysis` stores ordered `Morphemes`, `RootMorphemeIndex`, and `Category`. Its equality method compares, in order, each morpheme's `Id`, then root position, then category. Gloss, surface shape, discovery path, and timing are absent from equality. See [`WordAnalysis.cs`](../../machine/src/SIL.Machine/Morphology/WordAnalysis.cs), especially lines 14–21 and 43–52.

Root position is operational, not decorative. C# synthesis indexes the morpheme sequence at `RootMorphemeIndex`, casts that item to `LexEntry`, and uses the material on each side as non-root morphology. A wrong root can therefore select the wrong lexical entry or make synthesis invalid. See [`Morpher.cs`](../../machine/src/SIL.Machine.Morphology.HermitCrab/Morpher.cs), lines 637–672.

Category is likewise part of Machine equality and is copied by Machine's simple transfer operation. See [`WordAnalysis.cs`](../../machine/src/SIL.Machine/Morphology/WordAnalysis.cs), lines 50–51, and [`SimpleTransferer.cs`](../../machine/src/SIL.Machine/Translation/SimpleTransferer.cs), lines 31–40.

### 2. `IMorpheme.Id` is intended to be unique, but the XML boundary does not enforce that contract

`IMorpheme` documents `Id` as "the unique identifier." It separately exposes `Category` and `Gloss`, confirming that gloss is not identity. See [`IMorpheme.cs`](../../machine/src/SIL.Machine/Morphology/IMorpheme.cs), lines 10–29.

The concrete HermitCrab `Morpheme` class exposes `Id` as an unconstrained nullable `string` setter. It contains no non-empty or uniqueness check. See [`Morpheme.cs`](../../machine/src/SIL.Machine.Morphology.HermitCrab/Morpheme.cs), lines 23–35.

The XML loader assigns `LexEntry.Id` and affix/realizational-rule `Id` directly from the optional `<MorphemeId>` element. It does not reject a missing, empty, or duplicate value. See [`XmlLanguageLoader.cs`](../../machine/src/SIL.Machine.Morphology.HermitCrab/XmlLanguageLoader.cs), lines 435–442, 852–866, and 947–960.

This is not merely theoretical. The repository's documented Indonesian grammar has `<MorphemeId>` unset for every morpheme, and existing conformance notes explain that raw ID-based signatures consequently collapse. See [`natural-glosses-plan.md`](../natural-glosses-plan.md), lines 59–65, and [`04-standard-fst-poc.md`](../../reports/04-standard-fst-poc.md), around line 504. Several checked-in edge grammars intentionally omit rule `MorphemeId` to preserve this production behavior; see [`strrep-identity/words.yaml`](../../machine/conformance/edge-cases/strrep-identity/words.yaml), lines 1–7.

A direct XML audit of the three bundled real-language grammars found that every selected lexical
entry, morphological rule, and realizational rule omitted `<MorphemeId>`, while every corresponding
XML `id` attribute was non-empty and unique:

| Grammar | Identity-bearing nodes | Empty `MorphemeId` | Empty XML `id` | Duplicate XML `id` |
|---|---:|---:|---:|---:|
| [`amharic-hc.xml`](../../samples/data/amharic-hc.xml) | 163 | 163 | 0 | 0 |
| [`indonesian-hc.xml`](../../samples/data/indonesian-hc.xml) | 79 | 79 | 0 | 0 |
| [`sena-hc.xml`](../../samples/data/sena-hc.xml) | 1,503 | 1,503 | 0 | 0 |

The audit selected `LexicalEntry`, `MorphologicalRule`, and `RealizationalRule` elements and counted
their child `MorphemeId` values separately from their `id` attributes. This makes the collision risk
the normal shape of the checked-in production examples, not an isolated malformed fixture.

Therefore, `WordAnalysis.Equals` is the correct upstream model for **which dimensions matter**, but its string-ID projection is not a collision-safe interchange encoding for all supported grammars.

### 3. HermitCrab XML already has a separate source-object identity

The loader reads each lexical entry's XML `id` attribute separately from `<MorphemeId>`. It registers the loaded entry in `_morphemes` under that XML ID, and uses the same registry to resolve co-occurrence references. Morphological rules are likewise indexed by their XML `id`, while their public `Morpheme.Id` comes from `<MorphemeId>`. See [`XmlLanguageLoader.cs`](../../machine/src/SIL.Machine.Morphology.HermitCrab/XmlLanguageLoader.cs), lines 435–442, 488–491, 379–403, and 852–866.

The two identifiers have different jobs:

- XML `id`: grammar object/reference identity.
- `<MorphemeId>`: public morpheme label used by `WordAnalysis.Equals` and the historical batch signature.

Rust already preserves this distinction explicitly in `MorphemeInfo`: `xml_key` is the XML `id` attribute and `morph_id` is `<MorphemeId>`. See [`model.rs`](../../rust/crates/pg-grammar/src/model.rs), lines 476–486, and loader construction in [`load.rs`](../../rust/crates/pg-grammar/src/load.rs), lines 1656–1664 and 2173–2181.

Rust's `MorphemeId(u32)` is a dense index into the loaded grammar's morpheme table, not the source ID. See [`model.rs`](../../rust/crates/pg-grammar/src/model.rs), lines 55–67. It is suitable inside one loaded package but not stable across independently implemented loaders or rebuilt packages.

### 4. LibLCM uses object identity, not spelling or gloss, and its analysis model is richer than `WordAnalysis`

Every LCM object has a GUID. New objects receive a new GUID, persisted objects reload the `guid` attribute, and `CmObject.Id` is documented as the preferred object identifier. See `LibLCM/src/SIL.LCModel/DomainImpl/CmObject.cs`, lines 336–352, 1368–1372, and 1786–1797 in the sibling LibLCM checkout.

LibLCM's own duplicate-analysis check compares ordered morph bundles. A bundle matches only when its `SenseRA`, `MsaRA`, and `MorphRA` object references match and its forms match across writing systems; the analysis category must also match. It deliberately refuses to merge analyses carrying currently uncommon derivation, feature, template, stem, or compound-rule data. See `LibLCM/src/SIL.LCModel/DomainServices/WfiWordformServices.cs`, lines 318–345.

That does **not** mean PanGloss should add all of WfiAnalysis to Machine `WordAnalysis`. It means LCM confirms the underlying principle: stable source objects define identity; mutable gloss text does not. PanGloss's HermitCrab contract is intentionally a smaller projection.

For `.fwdata`, the current compiler constructs both stem and affix morphemes around an LCM MSA and stores the MSA GUID in `MorphemeInfo.xml_key`; `morph_id` is absent. See [`compile/lexicon.rs`](../../rust/crates/pg-grammar/src/compile/lexicon.rs), lines 206–237 and 300–315, and [`compile/affixes.rs`](../../rust/crates/pg-grammar/src/compile/affixes.rs), lines 301–313. This is deterministic and source-stable, but it is a PanGloss projection of LCM—not proof that an MSA GUID alone reproduces every distinction in a persisted LCM `WfiAnalysis`.

### 5. The current Rust result shape mirrors Machine but its parity projections are narrower

Rust `WordAnalysis` contains dense morpheme IDs, root index, POS ordinal, and a Rust-specific `guessed` annotation. See [`pg-parse/src/lib.rs`](../../rust/crates/pg-parse/src/lib.rs), lines 19–37. `structured_analysis` obtains the POS ordinal from the word's syntactic feature structure and constructs those fields. See [`morpher.rs`](../../rust/crates/pg-parse/src/morpher.rs), lines 830–856.

Existing historical signatures resolve dense IDs to `<MorphemeId>` strings and preserve duplicates for diagnostics. That remains useful, but it cannot be the authoritative set-equality key when labels are empty or duplicated. The canonical comparator must resolve dense morpheme and POS ordinals to `xml_key`/source IDs first.

## Recommended comparison contract

Represent one semantic analysis tersely as canonical JSON:

```text
a:{"m":["hc:entryWalk","hc:rulePast"],"p":"hc:posVerb","r":0}
```

For an LCM-derived package, values can carry normalized GUID strings:

```text
a:{"m":["lcm:2fb0…","lcm:94a1…"],"p":"lcm:ec61…","r":0}
```

The namespace is illustrative; the package manifest should define the identity authority once so it need not be repeated verbosely in every in-memory key. Equality is the decoded tuple:

```text
(ordered stable morpheme keys, root position, stable category key)
```

Before set comparison, deduplicate exact tuples. Separately retain duplicate multiplicity and FST proposal provenance as health evidence.

The comparison utility should also emit diagnostic labels alongside the identity:

```json
{
  "analysis": {"m":["hc:entryWalk","hc:rulePast"],"p":"hc:posVerb","r":0},
  "labels": {"gloss":["walk","PST"],"morphemeId":["WALK","PAST"]},
  "duplicates": 24
}
```

Changing gloss or `<MorphemeId>` then changes the explanation but not the semantic source-object identity. Changing morpheme order, root position, or category changes the analysis.

## Implementation implications

1. Add stable-key resolution to the Rust comparison/export path: `MorphemeId -> MorphemeInfo.xml_key` and `pos_id -> FeatureSymbolDef.xml_id` (or the corresponding stable field).
2. Make the C# structured validation harness retain the XML `id -> Morpheme object` mapping at load time and invert it by object reference. `XmlLanguageLoader` currently does not expose that map on `WordAnalysis`; do not use order, reflection, gloss, or `<MorphemeId>` as a fallback.
3. Validate that comparison keys are non-empty and collision-free within their declared authority. A collision makes structured parity `not_comparable` with a typed diagnostic; it must not silently merge analyses.
4. Keep the historical gloss/shape and `<MorphemeId>` signatures as duplicate-sensitive diagnostics.
5. Compare Rust's `guessed` flag as an annotation in Rust-to-Rust validation. C# fabricates guessed entries but does not expose a corresponding field in `WordAnalysis`; do not pretend it is part of the shared core identity.
6. For `.fwdata`, preserve normalized LCM GUIDs through snapshot, compilation, and package serialization. Do not substitute HVOs, array ordinals, forms, glosses, or display names.

## Does C# HermitCrab serialize analyses today?

No general `WordAnalysis` serializer exists in the inspected C# source. `WordAnalysis` is a plain
object with public read-only `Morphemes`, `RootMorphemeIndex`, and `Category` properties and equality
logic, but no JSON/data-contract annotations or purpose-built wire serializer. Serializing its
object graph generically would also be the wrong contract: its `IMorpheme` instances refer into the
loaded language model rather than a compact stable interchange record. See
[`WordAnalysis.cs`](../../machine/src/SIL.Machine/Morphology/WordAnalysis.cs), lines 12–68.

The existing tool `batch` command does serialize a comparison signature, but it calls
`Morpher.ParseWord`, receives internal `Word` results, and emits sorted
`morpheme.Id+morpheme.Id|shape` text. It does not emit root position or category, and its morpheme
labels collapse when `<MorphemeId>` is absent. See
[`BatchCommand.cs`](../../machine/src/SIL.Machine.Morphology.HermitCrab.Tool/BatchCommand.cs), lines
43–63, and [`SignatureFormat.cs`](../../machine/src/SIL.Machine.Morphology.HermitCrab.Tool/SignatureFormat.cs),
lines 27–65.

C# HermitCrab can nevertheless support authoritative machine-delta output with a small, explicit
adapter rather than general object serialization:

1. Run `Morpher.AnalyzeWord`, which already returns public `WordAnalysis` values containing the
   exact Machine equality dimensions.
2. During XML load, retain an object-reference-to-XML-`id` map. The loader already constructs the
   inverse private `_morphemes` map (`id -> Morpheme`) for reference resolution, but does not expose
   it. Add a supported loader result/callback or narrowly scoped validation API; do not use
   reflection, enumeration order, gloss, or `<MorphemeId>` as a substitute.
3. Emit only the compact canonical projection, for example
   `a:{"m":["entry42","mrule7"],"p":"posV","r":0}`. Sort and deduplicate these records for
   semantic set comparison, while retaining duplicate counts and the older gloss/shape signature as
   separate diagnostics.

This adapter is suitable for automated before/after and C#/Rust delta comparison. It does not need
to become a general-purpose human report or a serializer for the full HermitCrab object graph.

## Unresolved facts requiring a focused implementation audit

- The C# XML loader keeps its `_morphemes` source-ID map private. The least invasive supported way for the validation harness to obtain that map must be selected; modifying or wrapping the loader is preferable to reflection.
- PanGloss currently uses MSA GUID as the HC morpheme source key for LCM-derived stems and affixes. This matches the current compiler model, but a direct audit of the FieldWorks HermitCrab exporter is still required before claiming byte-for-byte key compatibility with FieldWorks-generated HC XML.
- LCM `WfiAnalysis` distinguishes Morph, MSA, and Sense objects, plus optional derivational structures. PanGloss's smaller Machine `WordAnalysis` projection cannot be advertised as full `WfiAnalysis` equality.
- Category absence must be represented explicitly (`null`), not conflated with an empty source ID.

These unresolved points do not change the core recommendation. They constrain how strongly the eventual tool may label a C#/Rust comparison as authoritative.
