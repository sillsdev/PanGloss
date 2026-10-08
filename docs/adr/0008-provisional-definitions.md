---
status: accepted
---

# Underdefined projects run on provisional definitions, beyond C# HermitCrab

C# HermitCrab needs a fully defined phonology: FieldWorks' HC loader silently drops any allomorph
it cannot segment and any environment that names an undefined class, so a project that parsed for
years under XAMPLE loses words, or gains wrong parses, the day it switches parser. PanGloss is
meant to replace both FieldWorks parsers, and to serve projects whose authors never define their
phonology, either because their language does not need it or because they choose not to. So
PanGloss runs every underdefined project, whatever parser FieldWorks has selected, by supplying a
**provisional definition** for each undefined thing. Each provisional definition matches or
exceeds what XAMPLE does with the same project, and is reported in grammar health as an info
statement naming what was undefined and what was assumed. Authors move from underdefined to fully
defined one definition at a time, with no cliff.

## Considered options

- **Match C# exactly (drop and warn).** Rejected: it reproduces the very loss that blocks XAMPLE
  users, and dropping an environment widens an allomorph's distribution into wrong parses.
- **Refuse the grammar until it is fully defined.** Rejected: that is the obstacle this decision
  removes.

## Consequences

- A fully defined project still parses exactly as C# HermitCrab does; the oracle hierarchy in
  `CLAUDE.md` is unchanged for it.
- Every behaviour a provisional definition introduces is a divergence: one ledger entry under
  `docs/divergences/`, one staged conformance fixture recording XAMPLE, C# HermitCrab and PanGloss
  results, and a gate that fails on any difference from either engine the ledger does not list. The
  rules themselves are derived from measuring the real XAMPLE, not assumed.
- A provisional letter belongs to no natural class that names letters or requires a feature value,
  and an authored phoneme with no features belongs to no class that requires a feature value,
  because XAMPLE's classes are explicit letter lists. A class with no conditions (a wildcard) still
  matches both. For constrained classes this is what C# HermitCrab already does in environments and
  forward rules; only its analysis-side unification differs.
- Phonological rules the author wrote apply, even though XAMPLE never ran them. A project tuned on
  XAMPLE may therefore parse some words differently; those differences are reported to the author
  (which stored analyses no longer parse, and which rule causes it), never hidden by also trying
  each word without its rules. "Match or exceed XAMPLE" covers phonology left undefined, not rules
  the author defined.
- Comparisons against analyses stored in a FieldWorks project use FieldWorks' own key (allomorph,
  MSA and inflection type per morph), because FieldWorks does not store root position or category.
- The proposal is raised with `sillsdev/machine` as an issue to start the discussion; the staged
  fixtures graduate upstream only if Machine adopts it.
