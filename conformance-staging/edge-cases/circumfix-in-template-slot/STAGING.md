# STAGING: circumfix-in-template-slot

## Why this fixture exists

Makes a currently-UNMEASURED containment hole measurable, per a Fable planning review of
`openspec/changes/surface-compile-profile-and-templated-routing/design.md` (D1). No existing
fixture puts a genuinely circumfix-shaped `MorphologicalRule` (a subrule whose output is
`[InsertSegments, CopyFromInput, InsertSegments]`, classifying `Role::CircumfixPrefix` per
`crate::emit::rule_role`/`classify_affix`) inside an `<AffixTemplate>` `<Slot>`:
`suffixing-vowel-harmony` keeps its own circumfix off-template, on a different part of speech, and
`fusional-realizational-morphology`'s `mrCircumfixGeT` is loose/unordered at the Stratum level,
never template-referenced. This fixture's `mrCircum` is reachable ONLY through
`templateMain`'s `slotMain` (deliberately NOT listed in the Stratum's own `morphologicalRules=`
attribute — `template-category-sharing`'s own proven finding is that Slot membership alone does
not create exclusivity; omitting the Stratum-level listing is what does).

The motivating real-grammar evidence this pins: template-slot circumfix cells are reachable via
the templated backend's own slot-chain emission but never via the tuned backend's structural-
composite probing (`crate::emit::build_structural_composites`, which walks the Stratum's free-
standing rule battery — `AffixTemplate` slots are not part of that walk today). This is an honest,
fail-closed undergeneration hole, not a wrong compile: the affected word is never proposed by the
tuned backend at all.

## What it pins

- `lodi` (bare root): a plain control — `slotMain` is optional, so the bare root is valid with no
  slot filled.
- `talodien` (`mrCircum` occupying `slotMain`): THE load-bearing word — a circumfix rule (`ta-...-
  en`) reachable only through the template. **Measured** (not merely hypothesized): this word
  produces a real `TemplatedUnderlyingTokens` proposal-containment FAILURE against
  `crate::faithfulness_coverage`'s gate — `crate::backend_runtime::word_proposal_containment`
  reports the oracle identity `(morphemes=[0, 2], root_index=1)` at required multiplicity 1, offered
  0 by that backend. `PlanComposed` and `TunedSurfaceProbed` both HOLD on this word. This
  contradicts the planning review's stated hypothesis that `TunedSurfaceProbed` would be the one to
  fail: `crate::emit::structural_candidate_rules` scans EVERY `MorphologicalRule` in the grammar
  (`(0..g.mrules.len())`), not just Stratum-listed ones, so `is_structural_rule`'s
  `Role::CircumfixPrefix` arm admits `mrCircum` into `build_structural_composites` regardless of
  its being template-only — the tuned backend does not actually gate admission on Stratum
  membership the way the hypothesis assumed. The real, measured hole is in the TEMPLATED backend
  instead: a slot-chain emitter built around one underlying token per slot occupant cannot
  represent a circumfix's two-sided insertion, so it undergenerates. That red is the point of this
  fixture, not a defect to fix here — this fixture makes the templated backend's own gap
  measurable, which the planning review predicted existed somewhere in this shape even if it named
  the wrong backend.
- `mulodi` (`mrOrdPfx` occupying the SAME slot): the control proving `templateMain` itself works
  correctly on every backend, including tuned, when the occupying rule is an ordinary
  (non-discontinuous) affix rather than a circumfix.
- `enlodi` (negative control, `expect_fail`): the circumfix's trailing material alone, with no
  matching leading piece — no subrule in `slotMain` produces this shape (a circumfix's two pieces
  are inseparable within one subrule application), so it has no valid derivation. Pins that the
  grammar doesn't over-generate a "half a circumfix" analysis.

## Oracle discipline

**Oracle: the C# founding oracle (`SIL.Machine.Morphology.HermitCrab.Tool`/`hc.dll`), run through
`machine/conformance/adapters/hc-dotnet-wrapper.sh`, cross-checked against `pangloss`
(`pg_parse::Morpher`, driven directly).** `machine/src` (the oracle's own source) was widened into
this worktree's sparse `machine/conformance` checkout for this authoring session only
(`git -C machine sparse-checkout set conformance src`, narrowed back to `conformance` alone once
transcription was done — see the conformance-grammars skill's own oracle-discipline section), and
`dotnet build machine/src/SIL.Machine.Morphology.HermitCrab.Tool` succeeded (dotnet 10.0.302). Both
runs agreed on every word's status and signature.

## Verification

C# oracle run (`hc-dotnet-wrapper.sh batch grammar.xml words.txt out.tsv`):

```
0	lodi	99	ok	ROOT|lodi
1	talodien	27	ok	CIRCUM+ROOT|talodien
2	mulodi	2	ok	ORDPFX+ROOT|mulodi
3	enlodi	1	ok	-
```

Cross-checked against `pg_parse::Morpher` via a throwaway `pg-parse` test
(`zz_throwaway_transcribe_new_fixtures.rs`, deleted once transcription was confirmed), run via
`pg.ps1 -Mode test -Package pg-parse -TestTarget zz_throwaway_transcribe_new_fixtures`. Both
oracles agree exactly; the C# output above is transcribed verbatim into `words.yaml`.

`pg.ps1 -Mode test -Package pg-parse -TestTarget conformance_fixtures_gate` confirms this fixture
replays inside the default suite. `pg.ps1 -Mode test -Package pg-foma -TestTarget
faithfulness_coverage_gate -Filter report_faithfulness_coverage -NoNextest` reports this fixture's
containment outcomes; the gate's own `FaithfulnessRequirement::NonVacuity` reads failures without
breaking the build on them. Isolated per-fixture measurement (a throwaway `pg-foma` test calling
`crate::faithfulness_coverage::observe_fixture_containment` directly, deleted after reporting) found:

```
=== staging:edge-cases/circumfix-in-template-slot ===
kinds: [Affixation, UnorderedMorphRuleApplication, NaturalClassDefinition]
  plan-composed: Held
  tuned-surface-probed: Held
  templated-underlying-tokens: Failed { word: "talodien", detail: "word \"talodien\": oracle identity (morphemes=[0, 2], root_index=1) required multiplicity 1, proposal set offered 0" }
```

The new containment-failure triple this fixture introduces:
`staging:edge-cases/circumfix-in-template-slot	templated-underlying-tokens	talodien`.

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/circumfix-in-template-slot/`. On acceptance, delete this staged
copy in the same change (graduation guard enforces this mechanically).
