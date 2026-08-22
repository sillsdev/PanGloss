## Why

The eager Tuned Surface construction is the wrong representation for the large Amharic and
Aweti products.  The compact Templated Underlying Tokens construction is the right route, but its
current structural layer admits only a narrow two-part local deletion and a one-part literal
wrapper.  `emit.rs`, `capability.rs`, and `templated_compile.rs` can consequently disagree about
what was routed, emitted, or compiled.  This change closes only the concrete structural shapes
that can be lowered and checked exactly; every other shape remains an honest refusal.

## What Changes

- Add one shared, pure `MorphologyRewriteClassifier` for every structural affix-process allomorph.
  It returns either an admitted recipe (including its zone, marker placement, and validated
  source references) or a stable unsupported reason.  Emission, Templated routing, capability evaluation,
  marker allocation, and morphology-relation compilation all consume that result; no site may
  infer support from `Role` or from the first allomorph alone.
- Define `OrdinaryLiteral` narrowly: an empty/null RHS, or an RHS made entirely of translatable
  `InsertSegments` actions in one authored prefix/suffix zone.  Any `Copy`, `ModifyFromInput`, or
  `InsertContext` action must match a listed structural topology or be refused.
- Admit exactly the regular, recall-preserving proposal topologies listed in the design: the Aweti whole-root wrapper,
  Amharic interior insertion, Amharic terminal `ModifyFromInput`, Amharic initial-vowel
  replacement, and the existing bounded adjacent terminal-drop recipe.  Ordinary literal
  prefix/suffix allomorphs continue through ordinary templated emission and are not structural
  recipes.
- Keep action admission closed while deliberately overapproximating LHS predicates.  A recipe is
  accepted only when every RHS input reference resolves to a real, non-empty LHS part and the RHS
  matches one listed topology exactly.  The proposer does not pretend to enforce natural-class,
  variable, or quantifier predicates inside those parts: for the admitted topologies it can erase
  those predicates safely, propose a regular superset over the whole root, and leave exact LHS
  matching to HermitCrab confirmation.  Malformed references, table-untranslatable output text,
  and every unlisted action topology remain refused.
- Emit a technical marker only for an admitted root-internal structural allomorph.  The Aweti
  whole-root wrapper is the complete direct marker-free prefix-variant × suffix-variant Cartesian
  product because every LHS part is copied once in order; it traverses only the relation's identity
  branch and is never mistaken for an interior rewrite.  Compile one unioned allomorph relation for the remaining recipes, with total
  marker-union invariants: marker-free input has only identity behavior; a structural branch has
  exactly one known marker; foreign or multiple markers are rejected; the known marker is consumed
  exactly once; and no marker can reach the final proposer or structured analysis identity.
- Preserve the existing stratum-ordered underlying-token phonological cascade, including its
  right-to-left and simultaneous semantics.  A skipped rule, uncovered action, missing marker
  subtree, or leftover technical marker makes the Templated candidate incomplete and unavailable;
  it is never converted into a partial successful artifact.
- Replace the coarse Templated `ProcessMorphology = CannotRepresent` registry row with a
  predicate-backed known-gap disposition.  Its evidence points to the closed classifier: admitted
  terminal modification becomes `ConfirmOnly`, while every other process shape still refuses with
  its classifier witness.  The affected circumfix row is resolved through the same predicate rather
  than globally admitting every circumfix.
- Add construct-named synthetic positive/negative witnesses and corpus gates.  The realized
  backend must be the backend selected: `selected_backend == realized_backend ==
  TemplatedUnderlyingTokens`, with the same compiled network used for the reported results.
  Certification requires zero skips, gaps, and remaining markers, exact semantic analysis-set
  equality for all 200 Amharic cases and all 106 Aweti cases, and no residual words.

## Capabilities

### New Capabilities

- None as a standalone `openspec/specs/` capability.  Each admitted recipe is pinned by the
  shared classifier and executable capability/conformance tests.

### Modified Capabilities

- None in an `openspec/specs/` tree; this repository keeps executable behavior in code and tests.

## Dependencies and Non-goals

- Builds on the landed Pattern/environment, replacement-cascade, right-to-left, simultaneous,
  circumfix/null-output, and table-identity work.  It must not weaken those predicates or replace
  an unsupported action with a generic role label.
- Owns the Templated morphology files named by `openspec/changes/STAGING.md`; sibling production
  routing may consume this clean capability result but may not select a different or merely
  partial candidate.
- Does not raise an eager enumeration budget, use candidate filtering to hide a missing proposal,
  add language-specific production switches, broaden the Pattern lowerer without witnesses, or
  copy private language data into synthetic fixtures.
- Mbugwe is explicitly out of scope: no Mbugwe fixture, corpus gate, routing exception, or
  acceptance claim is added by this change.

## Impact

- Primary modules: `rust/crates/pg-foma/src/structural_allomorph.rs`, `emit.rs`, `capability.rs`,
  `strategy_coverage.rs`, and `templated_compile.rs`, plus narrowly scoped relation/profile helpers if needed.
- Tests must pin every admitted and rejected topology, marker isolation/cleanup, selected-versus-
  realized backend identity, and exact structured semantic equality.  Actual Amharic/Aweti data
  remains external/private evidence; synthetic fixtures are construct-shaped and invented.
