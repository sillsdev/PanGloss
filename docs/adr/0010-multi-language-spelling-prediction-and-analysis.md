---
status: accepted
---

# PanGloss is a multi-language spell checker, word predictor, and word analyzer

Owner decision, 2026-11-07. PanGloss's purpose widens to a distributable multi-language spell checker,
word predictor, and word analyzer for arbitrary text, with the morphological parser remaining its
core. It is built to run as WASM, ideally inside Firefox and LibreOffice as well as its existing
hosts. This replaces the parser-only product boundary; grammar analyses retain their existing
correctness contracts and remain distinct from dictionary, language-detection, correction, and
prediction evidence.

## Why

Real text contains vernacular forms, contact-language words, shared forms, and misspellings.
Callers need these observations together to support spelling and language work. Requiring every
host to recreate segmentation, contact-language checking, and prediction would fragment behavior
and leave the portable parser unable to serve the full use case.

## Consequences

- Runtime functionality must have a WASM-compatible path. A hard runtime dependency that prevents
  WASM execution is excluded; native compilation and reference-validation tooling can remain
  native under their existing capability profiles.
- Packs remain separable from the engine and from one another. Data retains its own source,
  version/hash, licence, and notices so that resources with different terms can remain
  licence-separable. An engine's licence does not relicense its dictionaries or models, and a
  package licence declaration remains metadata rather than runtime permission enforcement.
- Packs remain data-only. Language Pack means only the `.pgpack` grammar analysis artifact.
  Dictionary packs are the separate spelling and language-evidence bundles; they do not change
  the `.pgpack` format or require a fabricated grammar. Language Pack is not an umbrella term.
- Word prediction is in scope, separately from correction suggestions and morphological analysis.
  A Hunspell dictionary alone is not a contextual prediction model. No predictor model is selected
  by this decision.
- All deployment and packaging details are deferred: host bindings, Firefox/LibreOffice
  integration, installation, distribution, pack containers, and loading policy need later
  decisions. These targets describe product direction, not completed integrations.

Decision basis: the owner-direction and PanGloss-analyzer sections, including the T9 pack-naming
decision, of `corpus-flow-decisions.md` (2026-11-07), supported by `tokenizer-pangloss-rust.md` and
`hunspell-pangloss.md`. Earlier parser-only recommendations in `r-lang-id.md` and
`corpus-langid-small.md` are superseded on product ownership; their evidence-versus-judgement
distinction remains applicable.

Related: [runtime feature compatibility](0004-runtime-feature-compatibility.md),
[FieldWorks-compatible segmentation](0011-fieldworks-compatible-segmentation.md), and
[multi-language word evidence](0012-multi-language-word-evidence.md). Terms: [CONTEXT.md](../../CONTEXT.md).
