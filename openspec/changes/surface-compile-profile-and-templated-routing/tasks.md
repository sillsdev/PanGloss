## 1. Phase A acceptance tests

- [x] 1.1 Primary agent adds `named_resource_envelope_contract.rs` pinning the closed full-profile IDs, every effective dimension/digest input, managed default, explicit retry linkage, rejection of arbitrary product limits, and single-attempt default; verify the new target fails for the missing API with `& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget named_resource_envelope_contract`.
- [x] 1.2 Primary agent adds `closure_terminal_parity_gate.rs` with below/at/over-work and live-successor-at-depth-boundary witnesses; require a total terminal result, pending ordinals/counts, empty-worklist completeness, and equal characterization/production traces; verify the new target fails for the missing total/parity behavior with `& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget closure_terminal_parity_gate`.
- [x] 1.3 Record for each new test the expected pre-change failure and reject any assertion that passes on the current implementation; the primary, not an implementation agent, owns these acceptance files through Phase A.

## 2. Phase A envelope and closure implementation

- [x] 2.1 Add the closed `ResourceEnvelopeId`/`ResourceEnvelope` and immutable attempt/evidence value types by composing the existing worker, communication, compose, enumeration, and backend budgets; keep raw closure-limit helpers internal/test-only and make canonical serialization/digest tests from 1.1 pass.
- [x] 2.2 Refactor the pre-expansion and structural closure walks onto shared production transition kernels/traces; make every work, depth, unsupported, and internal exit explicit, record live successors as pending, and require empty worklist plus zero pending successors for `Complete`.
- [x] 2.3 Thread one selected envelope snapshot through characterization and Tuned Surface production construction from clean state; preserve the managed 3,000 default, implement only caller-requested linked retry, and retain the first attempt's terminal evidence.
- [x] 2.4 Make the Phase A targets green without editing their acceptance direction, then run the focused existing closure regressions with `& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget closure_unbounded_realizational` and `& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget backend_selection_contract`.
- [ ] 2.5 Add hidden developer-build-only `--remove-size-limits` stress handling that disables
      only internal deterministic size/work caps. Keep worker isolation, bounded I/O, external
      watchdog/RSS/absolute ceilings, capability checks, empty-worklist completion, finalized
      payload, and parity mandatory; record the effective mode in attempt evidence.
- [ ] 2.6 Add hidden developer-build-only `--allow-unproven` handling as a correctness-only
      override that may omit valid parses. Reject it in production/publication/certification,
      retain an indelible unproven marker, and keep `--no-enforce-capability` legacy,
      developer-only, and non-production.

## 3. Indonesian construction evidence

- [ ] 3.1 Add a corpus-optional Indonesian gate that first proves the managed envelope terminates incomplete, then explicitly retries under `tuned-surface-work-10k-v1` and performs the real Tuned Surface construction with complete/equal transition evidence and no pending, uncovered, skipped, or technical-marker material.
- [ ] 3.2 Record the actual completed FST measurements and mechanism counters; if the private suite is available, write a separate canonical assessment linked by build-attempt/model fingerprint and prove full-HC semantic analysis-set containment/equality, otherwise record assessment evidence as `not_run` without weakening construction acceptance.
- [ ] 3.3 Verify the Indonesian and all-backend report targets through the managed wrapper; use `-Mode corpus-test` only for the private corpus target and retain full unfiltered output as the evidence artifact.
- [ ] 3.4 Add `indonesian_prototype_production_gate.rs` to preserve route provenance in the Indonesian evidence: label the historical 97/97 non-reduplication result as the separate `uflexc + replace` prototype, identify `emit` / `TunedSurfaceProbed` as the still-live production route, and refuse any attempt to use one route's corpus result to certify another route's build.

## 4. Phase B trusted-build acceptance tests

- [ ] 4.1 After the sibling morphology change lands, the primary adds `trusted_selected_build_gate.rs` requiring `preferred == selected == realized`, requested strategy == realized strategy, a complete matching envelope/certificate, exact grammar identity, zero gaps/skips/markers, finalized payload bytes, and matching payload/model fingerprints; verify RED for mismatch, skip, gap, marker, and fallback-attribution cases.
- [ ] 4.2 Primary adds worker protocol tests proving success returns the actual bounded `foma_binary_payload`, parsed-grammar identity, envelope digest, and payload digest; prove oversized payload, digest mismatch, grammar mismatch, and envelope mismatch fail closed before selection.
- [ ] 4.3 Primary extends the canonical build/assessment contract tests: `BuildReport` serializes the lower-layer completed-build evidence but never FST bytes or word results; `AssessmentReport` links the compiled-model/attempt fingerprint and remains the only word-result artifact.

## 5. Phase B routing, worker, and reports

- [ ] 5.1 Implement lower-layer `CompletedBackendBuild { evidence, payload_bytes }`; make the contained worker compile the requested backend exactly once and return the finalized foma binary-memory payload within the named response bound. Placeholder payload bytes are failure/diagnostic-only; a watchdog result without finalized payload is never normal success, selected, trusted, or certified.
- [ ] 5.2 Reconstruct runtime/pack input from those exact bytes, verify grammar/envelope/payload identities, and extend the sole canonical `BuildReport` with the evidence projection only; do not rebuild in the parent or serialize payload bytes into the report. Placeholder payload bytes are failure/diagnostic-only; a watchdog result without finalized payload is never normal success, selected, trusted, or certified.
- [ ] 5.3 Replace check-only normal selection with one selected-build entry point that reports every backend but selects only a matching successful `CompletedBackendBuild`; remove backend fallback attribution and refuse every skip, gap, pending successor, missing subtree, marker leak, or requested/realized mismatch. Placeholder payload bytes are failure/diagnostic-only; a watchdog result without finalized payload is never normal success, selected, trusted, or certified. Stale fallback language/sites are report-only and unambiguous; there is no hidden normal full-engine/FST fallback.
- [ ] 5.4 Replace the raw product-facing `RuntimeBudget.tuned_closure_work_limit` path with the named compile envelope at production call sites while retaining narrowly scoped internal/test boundary helpers.
- [x] 5.5 Correct and explicitly regenerate the Tuned Surface capability card so its switch references the real compile-request envelope field and closed IDs, never the removed environment-variable spelling; verify `backend_capability_cards_contract` and keep every card below 100 lines.

## 6. Verification and publication

- [ ] 6.1 Make `trusted_selected_build_gate`, worker protocol, build-report, assessment-link, backend-selection, backend-card, and Indonesian focused targets green through `rust/tools/pg.ps1`; capture complete outputs and deterministic counter deltas.
- [ ] 6.2 Run the single authoritative merged-tip pg-foma package gate after the morphology phase is integrated; do not claim a language route from a check-only report or a corpus result produced by another network.
- [ ] 6.3 Update the three-language acceptance documentation with the named envelope, immutable attempt/build-report evidence, separate assessment evidence, selected/realized route, and any unavailable corpus evidence; no Mbugwe result is added.
- [ ] 6.4 Verify both developer-only switches are orthogonal: stress does not waive correctness,
      allow-unproven does not waive resource containment, and neither permits partial,
      truncated, skipped, or parity-unverified output to be selected or published.
- [ ] 6.5 Run the separate five-grammar developer stress loop for Indonesian, Amharic, Aweti,
      Sena, and Mbugwe. Keep its complete Error results and backend pain/remedies distinct from the
      three-language production-certification report; Mbugwe remains a stress target, not a
      production-slice blocker.
