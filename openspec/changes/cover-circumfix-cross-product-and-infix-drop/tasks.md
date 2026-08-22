## 1. Primary-authored red acceptance tests

- [x] 1.1 Primary agent adds `templated_morphology_classifier_gate.rs` defining the wished-for shared classifier API and pinning `OrdinaryLiteral`, whole-root wrapper, interior insertion, one-segment terminal modification, initial-vowel replacement, existing adjacent drop, and stable `Unsupported` shape/reason outcomes; verify RED with `& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget templated_morphology_classifier_gate`.
- [x] 1.2 In the same target, pin default-deny negatives for nonliteral ordinary actions, missing/repeated/reordered copies, `InsertContext`, nonterminal modification, multi-segment/quantified terminal modification spans, invalid references, foreign tables, and every unlisted topology; prove each assertion fails on the current split role/capability implementation rather than passing vacuously.
- [x] 1.3 Primary agent adds `templated_morphology_marker_gate.rs` pinning marker-free identity, exactly-one-known-marker routing, foreign/multiple-marker rejection, marker isolation/consumption, final zero-marker state, nonzero recipe fire counters, and composition before phonology; verify RED with `& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget templated_morphology_marker_gate`.
- [x] 1.4 Primary agent adds a wrapper witness with at least two prefix and two suffix translations and asserts the full four-member Cartesian product; add one-insertion and two-insertion witnesses so a zipped wrapper or mandatory-two-insertion implementation fails.

## 2. Shared closed classifier

- [ ] 2.1 Introduce the pure `MorphologyRewriteClassifier` in `structural_allomorph.rs` with `OrdinaryLiteral`, `DirectWholeRootWrapper`, `MarkedStructural`, and `Unsupported` results; include zone, validated action topology, translated variant sets/output classes, marker identity, source IDs, and stable shape/reason IDs.
- [ ] 2.2 Make `OrdinaryLiteral` accept only empty/null RHS or a one-zone RHS made entirely of translatable `InsertSegments`; route every RHS containing `Copy`, `ModifyFromInput`, or `InsertContext` through the closed structural classifier and refuse when no listed recipe matches.
- [ ] 2.3 Classify whole-root wrappers when all non-empty input parts are copied exactly once in increasing order with literals only outside; return independently deduplicated prefix/suffix sets and emit their complete Cartesian product without lowering internal LHS predicates.
- [ ] 2.4 Classify interior insertion as ordered copies separated by possibly empty insertion runs with at least one non-empty run; classify initial fixed-segment replacement and the existing bounded adjacent drop; validate references/tables while deliberately erasing LHS predicates only where the design proves a recall superset.
- [ ] 2.5 Classify terminal modification only when the modified final input part has proven one-segment cardinality and its variable-free output context yields a finite active-table output set; retain precise refusal for multi-segment, quantified, empty, ambiguous, or untranslatable cases.
- [ ] 2.6 Make the classifier target green without editing the primary-owned acceptance direction; record classifier counts and prove each new admitted mechanism is zero before/nonzero after on its named witness.

## 3. Emission, capability, and morphology relation

- [ ] 3.1 Route every allomorph independently from the shared classifier in `emit.rs`; remove first-allomorph/`Role` inference for these shapes, place direct wrapper halves in their authored zones, and emit exactly one unique technical marker for each marked structural alternative.
- [ ] 3.2 Replace the Templated `ProcessMorphology = CannotRepresent` and affected circumfix registry rows in `strategy_coverage.rs` with predicate-backed known gaps; make `capability.rs` return `ConfirmOnly` only when every relevant allomorph has a listed classifier recipe and preserve `Refuse` with the exact unsupported witness otherwise.
- [ ] 3.3 Compile the interior-insertion regular superset: preserve all segment tokens and propose each authored literal run at possible segment boundaries in order; do not constrain to an invented LHS partition, and exclude tags, boundaries, and technical symbols from segment positions.
- [ ] 3.4 Compile the initial replacement and one-segment terminal-modification supersets, including every finite permitted output segment, while keeping their markers allomorph/zone-specific and leaving exact LHS/context checks to HermitCrab.
- [ ] 3.5 Compile one total unioned morphology relation with marker-free identity only, exact known-marker branches, foreign/multiple-marker rejection, unique consumption, and no universal identity path that can carry a marker through.
- [ ] 3.6 Compose morphology after underlying lexc and before the existing ordered phonological cascade; make the marker target green and verify focused RTL/simultaneous interaction targets through `rust/tools/pg.ps1`.

## 4. Fail-closed templated construction

- [ ] 4.1 Extend `TemplatedCompileProfile` with supported/compiled/fired recipe counts, per-shape counts, marker allocation/consumption/leak counts, unsupported/uncovered counts, missing subtree count, and existing skipped phonological rules.
- [ ] 4.2 Make `compile_templated_morphotactics` return a typed error—not `Ok`—for any unsupported/uncovered allomorph, skipped rule, missing marker subtree, allocation/compilation mismatch, foreign/multiple marker state, or final marker leak; an empty phonological rule list remains a valid identity cascade.
- [ ] 4.3 Add/extend focused tests proving all declared phonological rules compile, morphology output feeds RTL and simultaneous replacement in authored order, and the finalized proposer carries no technical marker or partial-success profile.

## 5. PanGloss-only conformance grammars

- [ ] 5.1 Audit `conformance-staging/` and the machine submodule for stale duplicate PanGloss fixtures; remove only obsolete PanGloss-owned duplicates whose promoted machine equivalent is proven, preserving intentional local behavior fixtures.
- [ ] 5.2 Add exactly two invented construct-named fixtures under `conformance-staging/`: one whole-root wrapper variant-cross-product fixture and one affine structural-process fixture covering one/two interior insertions, one-segment terminal modification, and initial replacement. These fixtures test PanGloss internal backend behavior and are never promoted to machine.
- [ ] 5.3 Register both fixtures in the local conformance coverage inventory and verify them with `& rust/tools/pg.ps1 -Mode conformance-test -Scope local -Package pg-foma` without including any actual-language name, ID, form, or private data.

## 6. Amharic and Aweti route certification

- [ ] 6.1 After sibling Phase B trusted routing is integrated, update the private all-backend gate to require `preferred == realized == TemplatedUnderlyingTokens` for Amharic and Aweti, with zero unsupported/uncovered/skipped/missing-subtree/marker counters and the exact finalized network fingerprint used by assessment.
- [ ] 6.2 Run the Amharic assessment over exactly 200 declared cases and require equality of the complete canonical structured analysis set for every case (`200/200`), not word reachability, first-analysis agreement, or containment alone.
- [ ] 6.3 Run the Aweti assessment over exactly 106 declared alphabet-encodable cases and require equality of the complete canonical structured analysis set for every case (`106/106`), zero residual words, and all 18 phonological rules compiled.
- [ ] 6.4 Capture deterministic FST states/arcs, classifier/recipe fires, proposal/confirmation counts, and backend report findings for both languages; report unavailable private data as `not_run` and never replace it with a synthetic pass.

## 7. Merged-tip verification and documentation

- [ ] 7.1 Run the classifier, marker, templated compile, local conformance, strategy coverage, backend selection, trusted selected-build, and five-language report targets once on the merged integration tip through `rust/tools/pg.ps1`, retaining full output.
- [ ] 7.2 Run the authoritative pg-foma package gate and the two private corpus targets on the same final commit; any final commit after the last compile/test phase is unverified and must be rerun.
- [ ] 7.3 Update the three-language acceptance document with selected/realized routes, exact semantic results, zero-gap evidence, FST measurements, and explicit evidence availability; do not add or imply a Mbugwe result.
