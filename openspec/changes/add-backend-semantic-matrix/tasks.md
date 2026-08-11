## 0. Ownership, coordination, and planning gates

- [ ] 0.1 Before matrix implementation, reconcile
      `add-pairwise-grammar-interaction-coverage`,
      `docs/rule-interaction-and-ordering-coverage-plan.md`, and
      `openspec/changes/STAGING.md`: those artifacts publish stable interaction/ordering obligation
      IDs and own reachability, exclusion, and retirement; this matrix only maps those IDs to
      backend-parity runs.
- [ ] 0.2 Record the clean base SHA and the concurrent agent's exact branch/tip, Machine and
      conformance-grammar files, manifest/submodule changes, and backend hotspots. Assign file
      ownership explicitly; integrate/rebase its accepted tip before overlapping edits. PanGloss
      owns Machine and the grammars, but does not edit concurrently occupied files blindly.
- [ ] 0.3 Record that the repository's code-defined OpenSpec workflow is complete at 3/3 artifacts
      and that generic strict validation reports the known `no deltas found` schema mismatch; do not
      create an unused spec-delta directory to silence it.

## 1. Fixture case identity and oracle provenance

- [ ] 1.1 In `pg-conformance-fixtures`, write failing unit tests for opaque per-fixture `case_id`
      uniqueness, duplicate surface forms with distinct IDs, missing IDs on matrix-selected words,
      graduation-stable `(category, name, case_id)` identity, and staged oracle provenance that cannot
      claim C# without named checked-in evidence; verify with `pg.ps1 -Mode test -Package
      pg-conformance-fixtures` and record that each test fails for the intended missing contract.
- [ ] 1.2 Add the backward-compatible `WordEntry.case_id` and structured fixture oracle-provenance
      fields plus resolver/validator APIs in `rust/crates/pg-conformance-fixtures/src/lib.rs`; make the
      Task 1.1 tests pass without requiring IDs on fixtures outside the semantic matrix. For a
      selected Machine-root case, coordinate the upstream-compatible ID in Machine and integrate its
      submodule change; do not add a PanGloss overlay or ordinal identity.
- [ ] 1.3 Add a falsification test proving one matrix reference that resolves zero or multiple cases is
      rejected with the fixture and case ID named, then implement the root-independent case resolver.
- [ ] 1.4 Add provenance validation for Machine founding-oracle fixtures and explicitly declared
      staged `hc-rust`/`hc-csharp` fixtures; verify false C# provenance, missing evidence, and a real
      C#-provenanced fixture in focused crate tests.

## 2. Matrix contract and complete outcome model

- [ ] 2.1 Create `rust/crates/pg-foma/tests/common/backend_semantic_matrix.rs` with failing contract
      tests for matrix case references, positive/negative witness groups, PR/full scope membership,
      and the total outcome vocabulary (`SemanticChecked`, `SelectorRefused`, `FallbackForbidden`,
      `CompileFailed`, `OracleIncomplete`, `ApplyIncomplete`, `HarnessError`).
- [ ] 2.2 Implement the minimal typed matrix registry and outcome types so Task 2.1 passes; keep all
      test-only configuration under `tests/common` and consume production capability/strategy APIs
      rather than adding a second expected-status ledger.
- [ ] 2.3 Write the failing completeness test that cross-checks `CharacteristicKind::ALL`,
      `strategy_coverage::ALL_STRATEGIES`, `representation_of`, configuration partitions, and every
      matrix obligation; require each obligation to be `required`, `honest-unsupported`, or `retired`
      with evidence and reject an unclassified or silently removed row.
- [ ] 2.4 Implement symbolic PR/full scope membership and fail-closed completeness mechanics,
      including a synthetic missing-row sabotage that proves neither scope derives success from rows
      that happened to run. Defer exact published IDs and counts until fixtures are selected and
      representative runtimes are measured in Task 8.2.
- [ ] 2.5 Add a test that a `CannotRepresent` cell contributes no support witness and is green only
      for a normal selector refusal with a non-empty characteristic/configuration diagnostic; prove
      compile failure, panic, absence, or direct forced undergeneration does not satisfy it.
- [ ] 2.6 Add a test that `RepresentsWithKnownGap` stays in the represented red denominator and
      every selected known-gap case must reach semantic parity. This change cannot downgrade or
      reclassify it as a refusal; only already-declared `CannotRepresent` is a blank.

## 3. Grouped Rust-HermitCrab/backend semantic runner

- [ ] 3.1 Add `rust/crates/pg-foma/tests/backend_semantic_matrix_gate.rs` with pre-written failing
      acceptance tests for one tiny fixture across all three backends: fixture signature parity,
      requested/realized strategy equality, no fallback, positive non-empty oracle/proposals,
      proposal containment, and confirmed semantic analysis equality.
- [ ] 3.2 Extract/reuse the production-path evaluation logic from
      `cross_compiler_equivalence_gate.rs` into the shared test helper: load once per fixture, prepare
      one direct Rust-HermitCrab oracle cache, materialize the requested backend explicitly only after
      selector admission, observe final `propose UNION peel` candidates, and confirm through the
      normal runtime path. Add an explicit strict audit policy/path (for example,
      `ForbidFallback`) that returns a typed failure on fallback, while preserving and separately
      regression-testing existing production fallback behavior.
- [ ] 3.3 Make the three-backend Task 3.1 acceptance test pass through the strict audit path without
      editing its expectations; also prove the separate production-fallback regression remains
      unchanged. Run
      `rust/tools/pg.ps1 -Mode conformance-test -Scope local -Package pg-foma -TestTarget
      backend_semantic_matrix_gate` and capture unfiltered output to a file.
- [ ] 3.4 Add pre-written positive/negative/ambiguity acceptance cases: positive requires non-empty
      direct oracle, nonzero proposals, and confirmed identity; negative requires checked-in and
      confirmed emptiness but permits confirm-only extra proposals; ambiguity compares deduplicated
      structured identity sets and records duplicate copies separately.
- [ ] 3.5 Add pre-written query-space acceptance cases for plain ASCII, a multicharacter segment,
      NFD input, representation-sensitive input, and boundary cleanup; require the requested backend
      to realize and propose non-vacuously for every admitted case.
- [ ] 3.6 Add fault-injection falsifications for a removed required proposal, wrong realized backend,
      attempted fallback (typed by the strict audit policy), zero positive oracle, zero positive
      proposals, compile failure, oracle
      truncation, apply truncation, and missing fixture; each must turn red for its own typed reason.

## 4. Shared semantic fixtures and finite obligation registry

- [ ] 4.1 Before editing Machine or any conformance fixture, complete Task 0.2, integrate/rebase the
      other conformance/backend agent's accepted tip, inspect its Machine, fixture, manifest, and
      submodule changes, and record the new base SHA; do not modify an overlapping file while its
      assigned owner is active.
- [ ] 4.2 Select the smallest existing synthetic conformance cases that discriminate the
      representation floor, add stable case IDs and explicit provenance, and strict-XML-parse every
      selected C#-eligible fixture; fix only selected malformed fixtures in coordination with the
      conformance owner.
- [ ] 4.3 Populate and gate the per-backend representation-floor obligations for affix/process/
      realizational morphology, compounding, ordered/unordered morph rules, MPR append/overwrite,
      rewrite modes/directions, metathesis, epenthesis, quantifiers, subrule gates, circumfix,
      reduplication, co-occurrence, natural classes, multi-table, stem names, and free fluctuation;
      every represented configuration runs semantically and every honest unsupported configuration
      refuses explicitly.
- [ ] 4.4 Add configuration partitions wherever a characteristic is not uniform (accepted/refused
      simultaneous overlap, bounded/unbounded quantifier, recursive/nonrecursive compounding,
      process/circumfix shapes, shared/disjoint multi-table representations, MPR append/overwrite,
      and bounded/unbounded copy); verify each partition has a discriminating construct witness rather
      than inheriting a coarser fixture's credit.
- [ ] 4.5 Consume the externally owned interaction-obligation IDs reconciled in Task 0.1, map every
      required ID to fixture-backed cases and applicable backend runs, and make an omitted required
      mapping fail the completeness gate. Do not redefine reachability, exclusions, or retirement in
      the matrix.
- [ ] 4.6 Consume the externally owned ordering-obligation IDs for feeding, bleeding,
      counterfeeding/counterbleeding, reachable noncommutative `A→B`/`B→A` contrasts, and declared
      invariance permutations; map them to fixture cases and backend runs without redefining the
      representative or future reachable-pair denominator.
- [ ] 4.7 Add boundary/operational semantic stimuli for zero/one/many boundaries and below/above
      deterministic resource thresholds; keep PanGloss work-unit assertions in Rust and verify the
      uncapped semantic result against the shared fixture.

## 5. On-demand C# verification lane

- [ ] 5.1 Write a failing wrapper-contract test using a fake adapter: export exactly the selected
      fixture/case words, preserve stable case attribution, compare five-column TSV semantics, and
      distinguish match, mismatch, unavailable, malformed output, and `not_run`.
- [ ] 5.2 Add the managed PowerShell entry point that invokes the existing Machine
      `hc-dotnet-wrapper` for selected matrix fixtures without introducing a new C# command or oracle
      format; an explicit require-C# invocation must fail on unavailable/not-run evidence.
- [ ] 5.3 Add a provenance gate that rejects `hc-csharp` claims lacking checked-in matching output and
      never promotes an `hc-rust` fixture merely because the optional C# lane was skipped.
- [ ] 5.4 When the Machine adapter is available, run one strict-XML, C#-provenanced matrix fixture end
      to end and record the exact match/mismatch result. When unavailable, record `not_run` without
      blocking implementation, preserving prior provenance, or creating/upgrading any C# claim.
      Verify ordinary Rust gates still work with Machine source/oracle unavailable.

## 6. One meaning of semantic witness

- [ ] 6.1 Write a failing terminology/credit test proving a successful backend compile with no word
      query cannot earn a construct witness in `witnessed_coverage`.
- [ ] 6.2 Refactor `rust/crates/pg-foma/src/witnessed_coverage.rs` and
      `tests/witnessed_strategy_coverage_gate.rs` so compile observations are named and reported as
      compile observations, while semantic witness credit comes only from matrix results; keep one
      authoritative gap/completeness account.
- [ ] 6.3 Migrate the semantic assertions from `cross_compiler_equivalence_gate.rs` only after the
      matrix equivalent and all fault-injection falsifications pass; remove duplicated helpers while
      retaining any independent compounding/headedness cases not yet represented in the registry.

## 7. Red census and backend correction rounds

- [ ] 7.1 Measure physical-memory headroom, commit headroom, CPU load, and live procgov/Cargo trees;
      choose managed build concurrency under the repository's two-19-GB-job rule, then run the full
      matrix once on the rebased integration tip and save the complete unfiltered result as the
      failure census.
- [ ] 7.2 Classify every red observation as harness/query wiring, missing/incorrect selector refusal,
      or genuine backend undergeneration in `docs/research/backend-semantic-matrix.md`; require zero
      unclassified failures and do not add ignored tests, known-gap allowances, or gap-count ratchets.
- [ ] 7.3 Correct every query-wiring/encoder failure in a focused slice without changing the
      pre-written acceptance cases; require requested/realized attribution, nonzero positive
      proposals, and the ASCII/multichar/NFD/boundary cases to pass for all admitted backends.
- [ ] 7.4 Correct every capability/refusal-accounting failure in a focused slice: represented cases
      run, honest unsupported configurations refuse before compile with attributable diagnostics,
      no classification is changed merely inside the matrix to make red disappear, and no
      `RepresentsWithKnownGap` case is reclassified instead of repaired.
- [ ] 7.5 Correct remaining `PlanComposed` undergeneration one invariant at a time, serialized around
      `replace.rs`/`gate.rs`/`emit.rs`/morphology hotspot ownership, with each final commit compiled by
      its focused `backend_semantic_matrix_gate` filter.
- [ ] 7.6 Correct remaining `TunedSurfaceProbed` undergeneration one invariant at a time under the same
      acceptance-test and hotspot rules; preserve the shipping analyzer's existing conformance gates.
- [ ] 7.7 Correct remaining `TemplatedUnderlyingTokens` undergeneration one invariant at a time under
      the same acceptance-test and hotspot rules; require token-cascade query encoding and all
      represented configuration partitions to pass.
- [ ] 7.8 Re-run the full matrix after all corrections and require zero unexpected outcomes; the only
      non-semantic cells allowed are attributable `CannotRepresent` refusals.

## 8. Documentation, managed gates, and integration

- [ ] 8.1 Write `docs/research/backend-semantic-matrix.md` with the declared denominators, oracle
      provenance rules, shared-fixture/Rust-assertion boundary, category taxonomy, failure census and
      resolution, C# invocation, measured PR/full runtime, and the explicit non-claim about arbitrary
      tuples/permutations.
- [ ] 8.2 Measure representative PR/full runtime first, then publish the final exact scope IDs and
      counts and add their managed invocations to repository test tooling/CI using
      `rust/tools/pg.ps1` and `-TestTarget backend_semantic_matrix_gate`; each invocation prints its
      scope and exact obligation/case counts and errors on zero matches.
- [ ] 8.3 Run focused `pg-conformance-fixtures` tests, the local-scope matrix target, the all-scope
      matrix target, and the affected legacy backend/conformance targets; capture output before
      filtering and verify every named test executed rather than relying on a green summary.
- [ ] 8.4 Run the authoritative managed integration suite once on the final rebased tip, inspect every
      delegated diff and claim, and require all represented obligations green with only explicit
      honest-unsupported blanks before merge.
- [ ] 8.5 Verify the Task 0.1 ownership contract did not drift: backend semantic parity remains here,
      while interaction/ordering reachability, exclusions, retirement, plan-node coverage, and the
      future reachable rule-pair census remain in their owning artifacts.
