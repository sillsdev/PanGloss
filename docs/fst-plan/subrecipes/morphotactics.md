# Morphotactics subrecipe dossier

## Scope

Morphotactics owns complete morphological alternatives: templates, ordered slots, obligatory
co-occurrence, paired/circumfix exponence, zero morphemes, allomorph priority, lexical
continuations, and bounded template or compound depth. It describes which analyses are legal before
phonological lowering, not merely which surface strings can be generated.

**Non-scope:** static lexical partitions, ordered phonology, local structural actions, arbitrary
copying, and terminal boundary cleanup. Those mechanisms may consume a morphotactics result, but
they are not optional slots in this dossier.

## Languages and families in mind

- **Anchor 1 — Orizaba Nahuatl. Family: Uto-Aztecan. Construct:** the person/number system exercises complete
  template alternatives: an ambiguous prefix such as `ti-` cannot be combined with a plural marker
  unless the template requires that combination. The construct role is obligatory co-occurrence and
  template identity, not a language-named branch. The harvested source records this as a high-
  confidence grammar fact, while the direct URL for Tuggy (1991) was not preserved.
- **Anchor 2 — Caquinte. Family: Arawakan. Construct:** discontinuous future morphology `n-…-e` exercises paired
  exponence across a boundary. The two members must be selected as one morphological unit before
  epenthesis or metathesis can see them; a free prefix/suffix product is the wrong relation.
- **Scale anchor — Huallaga Quechua (Quechuan):** inflection → derivation → inflection exercises
  stratal ordering and a controlled incomplete inner layer. It is a follow-on stress case, not
  evidence that the current implementation already covers multi-layer morphology.

Claim confidence is high for the co-occurrence and ordering principles in the local harvest; the
language-specific bibliographic URLs for Tuggy, Swift, and Weber remain an explicit source
uncertainty and must be reverified before they are used as release evidence.

## Primary sources

- [Linguistic construct harvest and citation ledger](../linguistic-recipe-harvest.md), especially
  the Orizaba, Caquinte, and Huallaga rows and its explicit re-verification warning.
- [Black, HermitCrab conceptual introduction](https://downloads.languagetechnology.org/fieldworks/Documentation/Intro%20to%20Parsing/ConceptualIntroduction.htm)
  for the complete-template and paired-exponence architecture preserved by the research results.
- [Template-category-sharing staging evidence](../../../conformance-staging/edge-cases/template-category-sharing/STAGING.md)
  for repository-level cross-template exclusion and multiplicity. This is engineering evidence,
  not a primary linguistic grammar.
- The bibliography entry for Tuggy (1991), Swift (1988), and Weber (1989) is preserved in the
  [harvest ledger](../linguistic-recipe-harvest.md); a stable public scan was not found in the
  supplied research output, so those claims are marked research-backed rather than independently
  reverified here.

## Grammar facts

The extractor must preserve template IDs, ordered slot membership, co-occurrence units, priority
chains, root identity/index, allomorph identity, stratum, boundaries, and multiplicity. A paired
unit is indivisible during alternative enumeration. Zero exponence is still an analysis-bearing
choice and must not be confused with absence of an analysis. Template paths must be exhaustive and
must not include a prefix from one template with a suffix from another.

**Invariants:** every emitted path is a complete legal template path; every required pair is either
present together or absent together; ordered morpheme IDs and root identity survive lowering; a
duplicate surface retains duplicate analyses; and no language name selects an architecture.

## Formal model and regularity

The mechanism is a finite relation over ordered morpheme paths and bounded state. A finite set of
templates is regular when slot depth, epsilon behavior, and continuation choices are bounded. The
semantic oracle is a multiset, not a set: equality requires the same ordered identities and
multiplicity. Epsilon morphology must be acyclic or application-bounded; otherwise “finite
template” does not imply finite application work.

**Correctness obligations:** generated paths must be a subset of legal HC analyses, and the chosen
vertical slice must recover every legal complete path. Confirmation must compare complete
multisets, including root index and morpheme IDs.

**Failure modes:** independently toggled paired members, cross-template mixing, lost zero-morpheme
identity, priority inversion, epsilon cycles, unbounded recursive template depth, and accepting a
surface match whose analysis metadata is wrong.

## Chosen architecture

1. Extract a typed `MorphotacticsSpec` with templates, strata, rules, co-occurrence units, priority
   chains, and a depth cap.
2. Enumerate complete template alternatives first; prune mutex and obligatory co-occurrence facts
   before materialization.
3. Lower each complete relation through the existing physical adapter, then run full HC multiset
   certification.
4. Keep the mechanism generic; language evidence selects conformance rows, never production
   branches.

## Rejected architectures

- A Boolean product of independent optional slots: it admits unattested half-templates and grows
  exponentially before co-occurrence pruning.
- One free-standing continuation class for every rule: repository staging proves it can synthesize
  cross-template mixes (`pakolola`/`takolosa`) that HC rejects.
- Surface-string-only deduplication: it loses distinct `mbili` analyses and root/morpheme identity.
- An unbounded recursive template enumerator without a cap: it makes exhaustion indistinguishable
  from a semantic negative.
- Language-named branches or a silent fallback compiler: neither provides a reusable contract.

## Interfaces and interactions

The producer provides symbol space/table, analysis and root identity, exact multiset behavior,
boundary state, dynamic POS/MPR/class state, stratum, and an execution disposition. A downstream
phonology or cleanup consumer must declare those requirements explicitly; surface agreement cannot
repair a lost identity or changed multiplicity. StructuralAllomorph may consume a complete path,
and BoundaryCleanup is terminal; CopyProcess may rejoin only with explicit copied-span metadata.

## Complexity and resource bounds

**Big-O variables:** `T` = grammar morphology records, `P` = legal template paths after pruning,
`s` = truly independent optional slots, `d` = bounded template depth, `m` = average path length,
and `E` = retained analysis entries.

**Time:** extraction is `O(T)`. Explicit path enumeration is `O(P · m)` after pruning; the raw
worst case is `O(2^s · m)` when every optional slot is independent. Recursive depth expansion can
reach `O(E · r^d)` for `r` applicable recursive choices.

**Space:** the retained path set is `O(P · m)` plus `O(T + E)` identity and source metadata. The
depth and compose budgets are semantic safety controls: a capped or timed-out run is non-certifying,
not an exact negative.

## Task 6 evidence status

- **Source ModelLocation/model-ID evidence:** the repository mapping exposes `ModelLocation::MorphRule`,
  `AffixAllomorph`, `MorphemeCoOccurrence`, and `AllomorphCoOccurrence`, with `MRuleId`, `AllomorphId`,
  and morpheme-index wire IDs in [`capability.rs`](../../../rust/crates/pg-foma/src/capability.rs) and
  [`recipe_mechanism.rs`](../../../rust/crates/pg-foma/src/recipe_mechanism.rs). A concrete source
  model-ID witness for the named grammar anchors is `Not measured — blocks implementation claim`.
  `TemplateId` is a mechanism field; there is no `ModelLocation::Template` variant to claim.
- **Resource caps:** `max_depth`, enumeration, compose, and probe caps are required and exhaustion is
  non-certifying; a numeric Task 6 cap record is `Not measured — blocks implementation claim`.
- **Measured stage counters:** no per-stage extraction/enumeration/lowering counter has been recorded:
  `Not measured — blocks implementation claim`.

## Conformance fixtures

### Exercise 1 — complete-template exclusion

Use the language-neutral `template-category-sharing` staging fixture. Expected oracle multiset:
`pakolosa` and `takolola` each contain their one template-internal analysis; `pakolola` and
`takolosa` are empty; `mbili` contains exactly two analyses, `eMbiliA` and `eMbiliB`. Mutations
that re-add the rules to the stratum-level free rule list must fail the cross-template gate.

### Exercise 2 — complete-template scale and identity

Use `recipe-template-generic`. The complete three-word fixture is the identity/multiplicity
authority; the bounded characterization must not be promoted to full-corpus proof. The staging
record documents the pathological midpoint `C(12,6)=924`; a conformance mutation that splits a
paired unit must change the expected multiset and be rejected. This is a proposed dossier exercise,
not a claim that Task 7 has already passed.

**Positive cases:** `pakolosa`, `takolola`, and the complete-template scale row.
**Negative cases:** `pakolola` and `takolosa` cross-template mixes.
**Identity/multiplicity cases:** `mbili` retains two distinct root analyses for one surface.
**Mutations:** re-add the four template rules to the free stratum rule list, or split a paired unit;
both mutations must fail the contract.
**Exact normalized expected multisets/tuples:** the staging oracle record is
`pakolosa = {(surface=pakolosa, signature=PFXA+KOLO+SFXA, source_model_id=[mrPfxA,mrSfxA], multiplicity=1)}`,
`takolola = {(surface=takolola, signature=PFXB+KOLO+SFXB, source_model_id=[mrPfxB,mrSfxB], multiplicity=1)}`,
`pakolola = {}`, `takolosa = {}`, and
`mbili = {(surface=mbili, signature=MBILIA, source_model_id=eMbiliA, multiplicity=1),
(surface=mbili, signature=MBILIB, source_model_id=eMbiliB, multiplicity=1)}`. These are canonical expected
records, not new measurements.

## Implementation status

The typed `MorphotacticsSpec` exists in [`recipe_mechanism.rs`](../../../rust/crates/pg-foma/src/recipe_mechanism.rs)
and the graph test constructs a valid morphotactics-to-cleanup edge in
[`recipe_mechanism_graph.rs`](../../../rust/crates/pg-foma/tests/recipe_mechanism_graph.rs). The
grammar-derived extractor and complete-template materialization are not claimed complete here;
they belong to later plan tasks. Current status: research-ready, implementation incomplete.

## Known gaps and split triggers

The direct primary URLs for the Orizaba, Caquinte, and Huallaga works need recovery. A second
independent template grammar should be reverified before the first vertical slice claims broad
typological coverage. A split/add is required if future grammars need unbounded template depth,
non-finite dependency, or a runtime operation that cannot preserve ordered paths and identity.

The split/adds conditions below are hypothetical future triggers, not dated evidence decisions.

**Trigger matrix:** `fits` when finite complete paths and bounded epsilon behavior suffice;
`refines` when a new co-occurrence, priority, or identity invariant can be expressed in the typed
spec; `splits/adds` when unbounded dependency or a separate runtime mechanism is required.

## Research log

| Date | Evidence and direct link | Consequence |
|---|---|---|
| 2026-08-01 | [approved design](../../superpowers/specs/2026-08-01-executable-subrecipes-design.md) and [plan Task 6](../../superpowers/plans/2026-08-01-executable-subrecipes-foundation.md) | Complete template alternatives, typed interfaces, and multiset certification are mandatory. |
| 2026-08-01 | [template-category-sharing STAGING.md](../../../conformance-staging/edge-cases/template-category-sharing/STAGING.md) | Cross-template mixing and same-surface multiplicity are repository-witnessed constraints. |

## Evidence decisions

| Date | Decision | Evidence | Architectural consequence / trigger |
|---|---|---|---|
| 2026-08-01 | fits | Complete finite templates and paired exponence recur in the harvest anchors. | Keep one generic typed mechanism; no language branch. |
| 2026-08-01 | refines | The staging fixture demonstrates that template membership alone is not exclusive when rules are also free-standing. | Extract explicit co-occurrence and continuation facts before lowering. |
