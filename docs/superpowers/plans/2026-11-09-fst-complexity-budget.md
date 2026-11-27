# FST complexity budget Implementation Plan

> **For agentic workers:** Use the executing-plans skill to implement this plan
> task by task. The present lane delivers documentation only; no implementation
> work is authorized by this document alone.

**Goal:** Replace exponential FST precision with bounded conservative widening,
then prove that the complete propose/filter/confirm route saves HC parser steps.

**Architecture:** The lowering owner represents alpha output as abstract top and
widens before construction budgets cross. Extend the existing filter/confirmation
seam, preserving all witnesses and HC multiplicity, with a shared per-word step
allowance and complete plain-parser fallback. Instrument the differential first.

**Tech Stack:** Rust, pg-foma, pg-foma-runtime, pg-parse/pg-rules, foma, managed
PowerShell tooling, Python differential summaries, pinned C# HermitCrab oracle.

**Spec:** [ADR 0013](../../adr/0013-fst-complexity-budget.md), especially its budget
table and metric definition, and the
[stress/admission contract](../specs/2026-08-23-stress-grammar-construction-and-production-admission.md).

## Global constraints

This is research/skunkworks: capabilities drive requirements. The numerical budgets
are working guidance to revisit with measurements, not frozen acceptance requirements;
exceeding an expectation warns rather than fails. Keep a finished FST as small as
reasonably possible through minimization, widening and factoring; about 50 MB serialized
is an expected size, not a failure threshold. Independent hard containment and
representability controls still act, and do not turn an advisory warning into refusal.

- No `O(2^n)` or unbounded `O(n^2)` FST feature. Quadratic local dimensions are
  `n < 1024`; count variable-width inputs explicitly. Stages add work, never multiply
  independently admitted budgets. B = 1,000,000 logical entries/states+arcs is the initial expectation.
- Keep the current 100 MiB, 65,536-row, 6,400 combined token and other hard bounds
  as backstops. Widen before crossing a limit; refuse only when inclusion cannot be
  preserved within a bound. Never accept partial output or increase a retry limit.
- Confirmed identity **multisets**, completion statuses and every word occurrence
  must match plain HC-Rust. New filter passes can reject only HC-impossible candidates.
- No memoization or keyed parse/confirmation-result caches. Temporary exact candidate
  grouping is allowed. Grammar-static facts and fixed budget configuration are allowed.
- Initial candidate cap: 1,023 merged identities. Confirmation target: at most 25%
  of the measured plain word's parser steps. Shared runtime step cap S is calibrated
  as specified in the ADR; do not compute a reference parse on each production call.
- The ≤100 rules/phonemes/environments assumption is only derivation context for
  expected bounds. Warn on exceeded observed/projected structure expectations,
  naming size, bound, contributing grammar items and the possibility of a better
  representation through widening, factoring or filtering. Warnings do not stop compilation.
- Read `.claude/skills/oracle-alignment/SKILL.md` before parser work; read
  `.claude/skills/conformance-grammars/SKILL.md` and `conformance/PROTOCOL.md` before
  fixture work. Synthetic fixtures only. Replay suspected bugs on the pinned C#
  oracle and record source/verdict/fixture; XML-only, non-fwdata behavior is out of scope.

## Review focus

1. Zero/absent/default and negated alpha values must survive later matching and
   spelling, including feature-only intermediates (slice 2).
2. Prefix surface order, alternate templates, partial roots and unknown witnesses
   must not become unjustified filter rejections (slices 5–6).
3. Same morphemes with different designated roots or genuine multiplicity must not
   collapse observable analyses (slice 4).
4. One confirmation call may consume the entire word allowance; the stop must act
   inside that call and discard partial results (slice 7).
5. Small minimized machines may have huge temporary products; containment must act
   before allocating those products, including operand construction and minimization
   (slices 2a and 3b).

## Verification and frozen measurements

Each slice is one reviewable Terra-sized change with its own gate; dependent slices
consume the interfaces named below. New names are proposed interfaces, not existing
APIs. Keep dormant mechanisms unavailable/explicit rather than reporting success when
they cannot act. Run `check` first, focused tests second, conformance where named.
Before a heavy run, measure live physical memory, CPU load and active Cargo trees.
All Rust commands use a fresh PowerShell process under the lane lock:

```bash
pg_gate() {
  flock /home/johnm/work/fst-production-20261103/cargo.lock \
    env CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 \
    PANGLOSS_MACHINE_CHECKOUTS=/mnt/h/repos/machine \
    POWERSHELL_TELEMETRY_OPTOUT=1 \
    pwsh -NoProfile -File rust/tools/pg.ps1 -Jobs 4 -TestThreads 4 "$@"
}
pg_gate -Mode check
# Examples of focused gates, after check:
pg_gate -Mode quick -Package pg-foma
pg_gate -Mode test -Package pg-foma-backend -TestTarget candidate_filter_contract
pg_gate -Mode conformance-test -Scope all
```

Use `-TestTarget` to narrow compilation; `-Filter` narrows execution only. All
commands must exit zero with no unexpected failures/skips before that slice is
accepted. A new regression must fail with the specific implementation removed.
Run full managed `check` before each commit and again on the exact committed SHA.

Freeze separate baseline/changed release binaries through `pg.ps1 -Mode build`
(for the measurement example use
`PANGLOSS_EXTRA_ARGS='--example grammar_pass' pg_gate -Mode build -Package pg-foma`),
and record immutable Rust SHA, Machine pin `4d9b454d` (verify the full pin), grammar
hash, word-file hash, options and all effective bounds. Use these local inputs from
`/home/johnm/work/fst-production-20261103/fst-strategies-inputs.json`:

| Grammar | Source under that workspace | Occurrences |
|---|---|---|
| Cebuano | `inputs/ceb/ceb.fwdata` | `inputs/ceb/words.txt` |
| Amharic | `inputs/main5/amharic.xml` | `inputs/main5/amharic.words.txt` |

Retain the earlier Cebuano Stage 1 XML export and its source hash when reproducing
`prule16`; do not equate a component XML measurement with the fwdata production path.
Run both whole word lists plus frozen worst words, serially with one parser thread,
under the existing 8 GiB address-space / 3,600-second outer guard. Invoke built
binaries through `pg.ps1 -Mode run -Exe`; never bypass managed tooling. The existing
`rust/tools/fst_strategy_run.sh` freezes an additional 3,000 closure-work cap: record
it when reproducing that baseline; for the new design use a direct managed invocation
without that optional advisory cap and record all actual bounds. Do not silently
change the envelope between before/after runs.

Slice 1 adds the explicit **plain** and **accelerated** measurement modes below to
`grammar_pass`; these are planned switches, unavailable on the starting SHA:

```bash
PANGLOSS_EXTRA_ARGS='"<source>" "<words>" "<plain.jsonl>" --measure-route plain' \
  pg_gate -Mode run -Exe '<built-grammar_pass>'
PANGLOSS_EXTRA_ARGS='"<source>" "<words>" "<fst.jsonl>" --measure-route accelerated --fst-strategy <implemented-strategy>' \
  pg_gate -Mode run -Exe '<built-grammar_pass>'
python3 rust/tools/fst_differential.py '<fst.jsonl>' --out '<comparison.json>'
```

Apply the outer guard to each run; substitute real paths with proper shell quoting.
The accelerated measurement performs its separate reference plain parse for comparison,
but never charges that experimental reference to production A. Plain mode bypasses
FST construction and confirmation entirely. Select only a strategy actually wired to
the changed proposing cascade; the starting `surface-enum` production constructor
does not use it and `factored` is `NotImplemented`. A Stage 1-only probe can report
inventory/operation counts but **cannot** report whole-parser savings. Slices 9–12
close this integration gap before claiming Cebuano/Amharic step savings.

For each occurrence join by word ID and hashes, compare full identities/multiplicities
and statuses, and retain P, C, F, A=C+F, candidate counts, routes and incomplete rows.
Use the ADR's weighted corpus `% less work`; include abandoned work and fallbacks,
and require positive savings plus the per-word 25% target for accelerated rows.
Never drop a difficult row or count a refused build as a zero-work successful FST.
Replay Amharic XML separately with the pinned C# driver using
`rust/tools/fst-strategy-oracle/README.md`; Cebuano fwdata comparison is explicitly
against plain HC-Rust unless an equivalent, hash-linked oracle export is available.
Use `parse_compare.py` for batch before/after multiset comparison; no set-only pass.

## Slices

### 1. Expose the differential before changing behavior

**Files:** Modify `rust/crates/pg-foma/src/differential.rs`,
`rust/crates/pg-foma/examples/grammar_pass.rs`, `rust/tools/fst_differential.py`;
extend the existing differential unit tests and add
`rust/tools/tests/fst-work-metric.tests.ps1` for summary fixtures.

**Interface:** Add serialized `HcWorkEvidence { plain_steps: u64,
confirmation_steps: u64, fallback_steps: u64, actual_steps: u64 }` to each
completed `DifferentialRow`; incomplete values are explicitly absent. Preserve the
existing row ID/status/manifest schema with a schema-version bump. Add
`--measure-route plain|accelerated`; default retains today's differential behavior.

- [ ] Write failing tests: plain mode makes zero proposer/confirm calls; shared
  confirmation groups count work once; C=50P is negative savings; fallback with
  C=10,F=P=100 reports -10% less work; zero denominator is undefined; missing rows
  prevent a complete aggregate. Assert both proposal-miss and spurious-confirmed counts.
- [ ] Read the existing `ParseOutcome::work_steps` and
  `ConfirmBatchDiagnostics::confirmation_work` at their owners; expose remaining
  diagnostic data at the common runtime seam, rather than rerunning to infer it.
- [ ] Run managed check/focused tests and the metric PowerShell test in its own process.
  Freeze the two grammars' plain step baselines and current acceleration refusals.
  Do not invent missing accelerated counts. Commit this measurement-only slice.

### 2. Replace subset expansion with abstract alpha output

**Files:** Modify `rust/crates/pg-foma/src/lower/intermediate.rs`,
`rust/crates/pg-foma/src/lower.rs`, `rust/crates/pg-foma/src/replace/permissive_tests.rs`.
Synthetic inclusion fixtures go in `conformance-staging/edge-cases/` under the protocol.

**Interface:** Keep `IntermediateBundles::build` and `slots`' caller contract;
introduce an internal `AbstractLane::Any | Values(u64)` representation if the full
mask cannot cover zero/absence safely. `Any` is understood by compatibility, transfer
and spelling together. Add row/work/join counts to the owning construction report.

- [ ] Pin failing tests for a three-bit alpha mask, two alpha outputs, negation,
  zero/default/absent lanes, fresh nodes and a feature-only intermediate that feeds
  a later rule. Assert one abstract lane per alpha output and no subset vector.
- [ ] Replace expansion; intern rows with indexed keys, not `rows.iter().any` scans.
  Charge revisits and lane units; use widening joins when the inventory work/byte/token
  budget would cross. No product closure enumeration hidden behind the alpha fix.
- [ ] Prove update/match/spelling inclusion in both modes/directions, test synthetic
  accepted and rejected words and unchanged reports on refusal, then run pg-foma
  focused/conformance gates. Reproduce Cebuano prule16 and Amharic prule6 with bounded
  counts. Keep whole-parser savings pending slices 11–12. Commit.

### 2a. Budget slot membership and final spelling before constructing operands

**Files:** Create `rust/crates/pg-foma/src/replace/complexity.rs` for the shared
budget and its tests; modify `rust/crates/pg-foma/src/lower/intermediate.rs`,
`lower.rs`, `replace.rs` and their focused tests; extend the construction report.

**Interface:** Add owner-local `build_budgeted_membership_and_spelling(patterns,
rows, definitions, alphabet, budget) -> Result<BudgetedOperands, PhonologyLoweringError>`.
The independently configured containment allowance in `ComplexityBudget` charges pattern nodes and lane units, row and definition
units, alphabet units, compatibility probes, emitted pairs, index storage, and native
construction work/temporary bytes. It is the same remaining construction allowance
used by slice 3b, not a fresh allowance for each slot, spelling table or operation. Expose
counts and widening fires. Stream or index compatibility by feature constraints and
factor spelling by shared constraints; never first collect a row × definition product.
When a precise join cannot fit, widen constraints or use a factored relation that
includes every compatible slot and spelling, before the crossing scan or allocation.
Preserve morphology analysis, boundary behavior and private-token cleanup. Refuse the
whole component if a complete bounded superset with cleanup cannot be proved or fit.

- [ ] Write effect tests where rows, alphabet and retained machines each fit, but
  slot/row or row/definition compatibility probes, emitted pairs, or a native
  construction temporary would cross the shared allowance. Assert the crossing scan
  and allocation never happen, and that earlier work reduces the remaining allowance.
- [ ] Test indexed and forced-widened paths against an exhaustive small oracle: every
  concrete match and spelling survives, including absent/negated values, boundaries
  and empty strings; private tokens cannot escape. Count actual probes, allocations
  and widening fires, not just reported warnings.
- [ ] Run focused intermediate/replace and conformance gates; record both Stage 1
  grammars' slot/spelling work and peak storage under the frozen envelope. Commit.

### 3a. Prove the bounded arbitrary-material fallback

**Files:** Modify `rust/crates/pg-foma/src/replace/complexity.rs` and its unit tests;
expose the proved relation for the structural component introduced in slice 10a.

**Interface:** Define a budgeted alphabet-linear arbitrary-material relation and its
analysis-tape contract. Its supported subset is finite admitted segment alphabets,
including empty material and boundary handling; it retains the input analysis language
and permits all required output strings without enumerating alphabet pairs. Prove
inclusion on both tapes and prove private tokens are erased at the spelling boundary.
Unproved alphabets, cleanup or shapes refuse the whole component.

- [ ] Give an explicit construction and inclusion argument for every accepted analysis
  and concrete output, including empty strings, boundaries and private-token cleanup.
  Bound states, arcs, work and temporary bytes; test the proof on exhaustive small
  alphabets and assert real allocations and counts stay within a tiny allowance.
- [ ] Run focused relation and conformance gates. Record construction counts and the
  supported subset; no parser-work savings claim. Commit this reusable proof and code.

### 3b. Integrate budgeted construction and composition

**Files:** Modify `rust/crates/pg-foma/src/replace/complexity.rs` and its unit tests,
`rust/crates/pg-foma/src/replace.rs` and construction reporting in
`rust/crates/pg-foma-runtime/src/construction.rs`.

**Interface:** Add owner-local `compose_with_complexity_budget(opts, left, right,
budget) -> Result<BudgetedRelation, PhonologyLoweringError>`, where the result carries
the complete net and exact/widened evidence. `ComplexityBudget` owns independently
configured containment limits and B-based advisory expectations for work and machine
size, plus byte limits, shared with slice 2a's operand construction;
call sites cannot reset remaining work between slots, operations or cascade stages.
Expose effective limits and widening fire counts.

- [ ] Route rule-regex compilation, insertion boundary/skip cleanup, and final
  spelling construction through conservative pre-construction checks, as well as
  every outer composition and minimization. Account for pinned foma's internal
  minimization, determinization and alphabet-dependent temporaries before each opaque
  call; an output-size check cannot authorize the call that creates the output.
- [ ] Write effect tests with individually fitting operands, alphabet and retained
  machines whose regex, cleanup, spelling, composition or minimization temporary
  would cross the allowance. Assert the crossing call/allocation is prevented, work
  is charged across call sites, and each rule and final spelling is minimized.
- [ ] When preflight cannot certify fit, use slice 3a's proved coarser relation before
  construction or composition; retain external containment and typed whole-component
  refusal when even that relation or its cleanup cannot be proved or fit. Test
  inclusion, empty strings, boundaries, private-token nonescape, unbounded repetition
  and complete report atomicity. Do not add a library fork in this slice.
- [ ] Run focused pg-foma and conformance gates. Measure both Stage 1 grammars'
  pre/post-minimize peaks, actual operation counts and widened counts; no parser-work
  savings claim yet. Commit.

### 4. Merge identical candidate identities without losing witnesses

**Files:** Create `rust/crates/pg-foma-runtime/src/candidate_filter/merge.rs` and
unit tests; modify `candidate_filter/mod.rs`, `candidate_filter/model.rs` and
`rust/crates/pg-foma-runtime/src/composite.rs` at `filter_then_confirm`.

**Interface:** `merge_candidates(input: impl IntoIterator<Item = ProposedCandidate>,
budget: &mut CandidateWorkBudget) -> MergeOutcome`. The outcome either contains
stable ordered identities plus alternative witnesses/source ordinals, or explicitly
requests plain fallback; it never contains a successful truncated candidate set.
`CandidateWorkBudget { max_units: usize, consumed_units: usize }` starts with at
most B units; every identity field and witness unit consumes allowance before storage.

- [ ] Test the same ordered morphemes from different intermediates merging once,
  differing root positions remaining separate, unknown versus established witnesses
  remaining alternatives, and true HC multiplicity being recovered once. Assert
  repeated word calls reuse no prior word result. Test budget crossing by effect.
- [ ] Implement exact-key radix/trie grouping over dense identity units in O(total
  units), preserving all confirmer discriminators. Charge witness union storage;
  never form a product or union away incompatible evidence into a stronger claim.
- [ ] Run pg-foma-runtime focused tests and pg-foma-backend's
  `mentanukam_multiplicity_recovered_by_confirm` and `candidate_filter_contract`.
  Keep filters off while measuring the change in both grammars' candidates/C steps
  using slice 1 when integration is available. Commit.

### 5. Add sound slot-order and required-affix facts

**Files:** Modify `rust/crates/pg-foma-runtime/src/morphotactics.rs`,
`candidate_filter/index.rs`, `candidate_filter/passes/structural.rs` and their tests;
extend `rust/crates/pg-foma-backend/tests/candidate_filter_model_check.rs`.

**Interface:** Extract reusable morphotactic facts from the existing owner and use
`SiteVerdict::Admits | Refuses | Unknown`; extend witness facts only where a producer
can supply actual derivation order/template completion. Pass output stays the existing
`PassDecision::Keep | Defer | Reject(proof)`; no new admission predicate.

- [ ] Pin missing required slots, invalid slot order and valid alternatives; add
  prefix-last-but-leftmost, vacuous optional slots, partial roots, multiple templates
  and missing-fact controls. An HC-accepted candidate may never be rejected.
- [ ] Implement linear fact consumption, reusing `MorphotacticIndex` decisions.
  Unknown order defers. A candidate is removed only after all witnesses fail.
  Budget overrun requests fallback rather than scanning more combinations.
- [ ] Run shadow mode against complete HC confirmations, report both divergence
  directions, add synthetic C#-verified conformance cases, and run model/contract and
  conformance gates. Measure C savings versus merge-only on both grammars where
  integrated; enable only proven passes. Commit.

### 6. Add category and co-occurrence checks at the same seam

**Files:** Modify `rust/crates/pg-rules/src/validity.rs`,
`rust/crates/pg-foma-runtime/src/candidate_filter/index.rs`; create
`candidate_filter/passes/morphotactic.rs` with tests and register in `passes/mod.rs`;
extend `rust/crates/pg-foma-backend/tests/candidate_filter_shadow_gate.rs`.

**Interface:** Reuse `candidate_morpheme_co_occurrence_ok` where its exact preconditions
hold; extract a shared indexed validity computation if its current scans violate the
linear budget. Category/allomorph unknowns use `Defer`; published refusal facts have
one-way tests proving that any claimed refusal really fails HC confirmation.

- [ ] Pin require/exclude and adjacency restrictions, repeated morphemes, compatible
  and incompatible categories, derivational category changes and unknown allomorphs.
  Include negative controls where a superficially invalid sequence is HC-accepted.
- [ ] Compile grammar-static restriction facts once and evaluate only applicable
  records, charging every record/sequence unit. Reuse/extract owner decisions;
  no independent reading of HC diagnostic strings or all-pairs candidate tests.
- [ ] Run shadow two-sided counts, effect/nonvacuity checks, C# fixture replay,
  pg-rules/pg-foma-runtime focused and backend shadow/conformance gates. Compare
  both grammars' remaining candidates and C steps with slice 5. Enforce only proved
  passes, retaining unknowns. Commit.

### 7. Share a word step allowance and route exhausted words plainly

**Files:** Modify `rust/crates/pg-foma-runtime/src/confirm.rs`, `composite.rs` and
`rust/crates/pg-parse/src/morpher.rs` only to expose the existing HC budget seam;
extend `confirm/tests.rs` and create runtime `word_acceleration_tests.rs`.

**Interface:** `WordAccelerationBudget { max_candidates: usize, max_hc_steps: usize }`
with initial candidate ceiling 1,023. Add budgeted confirmation returning
`Complete(buckets, work) | PlainRequired(reason, spent_work)`; pass remaining allowance
into the existing parser work counter so one HC call cannot overrun the word total.
The common analyzer route emits full plain output once on `PlainRequired`.

- [ ] Test 1,000 costly candidates, a single runaway confirmation group, many cheap
  groups sharing S, candidate overflow, apply/peel/filter exhaustion, exactly-at-limit
  completion, and incomplete fallback. Assert consumed steps, one plain call, no
  leaked partial analyses, no doubled multiplicity and P/C/F/A accounting.
- [ ] Wire the actual shared HC work control, including abandoned work. Leave the
  fallback's existing limits untouched; preserve explicit incomplete status if it
  cannot complete. No cache or automatic escalation. Calibrate S from frozen complete
  plain rows per the ADR; absent/zero calibration selects plain.
- [ ] Run runtime/pg-parse focused gates, `work_cap_gate`, full conformance, and the
  slice 1 metric fixtures. When integrated, compare each Cebuano/Amharic word with
  FST off/on and verify `A=C+F`, the 25% target and complete multiset identity. Commit.

### 8. Add structure warnings and preserve admission distinctions

**Files:** Modify `rust/crates/pg-foma/src/grammar_health.rs`,
`rust/crates/pg-foma-runtime/src/construction.rs`,
`rust/crates/pg-foma/src/grammar_health_tests.rs` and
`rust/crates/pg-foma/src/analyzer_compile_tests/construction_tests.rs`.

**Interface:** Warning evidence carries structure/observed-or-projected size/expected
bound, contributing grammar item IDs, representation advice and actual action;
construction evidence carries exact/widened/refused status and fire counts.
Reuse the existing report and admission dispatcher; no duplicate capability decision.

- [ ] Test a grammar with 101 rules/phonemes/environments and small structures
  produces no count-based warning; a small grammar whose bundle rows, worklist,
  intermediate states+arcs or candidate structure exceeds expectation does warn.
  A final serialized machine above the approximately 50 MB expectation warns but
  completes when independent controls permit; at expectation does not warn. A
  projected crossing warns before the allocation.
  Assert structure name, size, bound, grammar drivers and the exact better-representation
  advice. Test measured C/P above 25% and production S crossing without an invented
  plain denominator; warning emission does not prevent the real compile call. Prove a
  widened complete artifact differs from an incomplete refusal and neither is
  mislabeled `CannotRepresent` by a resource-only finding.
- [ ] Collect sizes and provenance at existing structure producers, including
  bundle/rule composition, query candidates and confirmation groups; do not scan
  all combinations or use authored input counts as warning keys. Extend the real
  envelope/compiler differential
  ratchet in both directions with `FaithfulnessRequirement::NoMoreThan` where needed.
- [ ] Run grammar-health/construction and backend-selection gates. Collect both
  grammars' warning/widening counts and actual budget effects. Commit.

### 9. Build the bounded underlying root/affix component

**Files:** Create `rust/crates/pg-foma/src/factored.rs`,
`rust/crates/pg-foma/src/factored/concatenative.rs` and unit tests; register the module
in `rust/crates/pg-foma/src/lib.rs`. Extract helpers from `emit.rs` only where needed
to call its existing allomorph ownership, underlying-material and tag decisions.

**Interface:** `compile_underlying_morphology(g, budget) -> Result<UnderlyingRelation,
FactoredRefusal>` produces a complete morpheme-tag to underlying-token relation for
its declared supported shapes, with additive counts. It is not a production proposer
until slice 11 connects it. Any present unsupported rule returns a typed whole-component
refusal, never a grammar with that rule silently omitted.

- [ ] Write tests for roots, multiple allomorphs, prefixes/suffixes, null affixes as
  FieldWorks boundary material, required/optional slots and alternate templates.
  Assert tags, root identity and underlying output; test a real containment crossing with a
  tiny configured allowance and a present unsupported shape refusing the whole component.
- [ ] Reuse the emitter's authoritative ownership/material decisions and
  `MorphotacticIndex`; emit shared continuation states without enumerating root ×
  affix combinations. Apply the complexity accounting from slices 2a and 3b. Preserve HC
  applicability as a confirmation obligation when it is conservatively widened.
- [ ] Run focused pg-foma/conformance gates. Inventory both grammars' morphology
  shapes and explicitly record which remain refused; collect component work counts,
  not parser savings. Commit this independently testable component.

### 10a. Adapt the arbitrary-material relation to structural analysis

**Files:** Create `rust/crates/pg-foma/src/factored/structural.rs` and unit tests;
modify `factored.rs` and reuse the proved relation from slice 3a.

**Interface:** Add a budgeted structural arbitrary-material relation for finite
admitted alphabets and analysis sequences supplied by the existing morphology owner.
Prove every supported concrete structural analysis tape, including its ordered tags
and designated root, remains in the widened relation; permit arbitrary segment
material on its output tape. This proves inclusion of analyses, not merely surfaces.
Unsupported analysis encodings or unproved cleanup refuse the whole component.

- [ ] Test empty material, boundaries, private-token cleanup and multiple distinct
  analysis tapes; assert actual construction counts and a tiny-budget crossing.
  Reuse slice 3a's proof and extend it explicitly to the structural analysis tape.
- [ ] Run focused structural relation and conformance gates; record the supported
  subset and both grammars' component inventories and counts. Commit.

### 10b. Route structural affixes and copies

**Files:** Modify `factored/structural.rs`, `factored.rs`, and focused tests; reuse
or extract owning decisions in `structural_allomorph.rs` and `emit.rs`.

**Interface:** Extend `compile_underlying_morphology` to the proven affix subset:
copy/reduplication and infix/circumfix forms whose analysis and material routing the
owner can identify. Use the precise bounded relation or slice 10a's widening; HC
confirms applicability. Remaining shapes refuse the whole component.

- [ ] Test copies, infixes and circumfixes, including structural output feeding a
  phonological rule, a finite-bound live successor, recall and spurious-confirmed
  controls. Assert routing uses the emitter's decision and widening preserves tags.
- [ ] Run focused structural/backend and conformance gates; record both grammars'
  component inventories, work and remaining refusals under the frozen envelope. Commit.

### 10c. Route compounds and repeated roots

**Files:** Modify `factored/structural.rs`, `factored.rs`, and focused tests; reuse
or extract the emitter's compound and root ownership decisions.

**Interface:** Extend the complete-or-refused component to compounds and repeated
morphemes whose ordered tags, designated root and multiplicity the owner can express.
Use bounded precise routing or slice 10a's analysis-preserving widening; do not
surface-probe cross products. All remaining shapes refuse the whole component.

- [ ] Test compounds with different designated roots, repeated roots/morphemes and
  genuine multiplicity, with positive recall and spurious-confirmed controls. Assert
  distinct analysis identities survive widening and confirmation.
- [ ] Run focused structural/backend and conformance gates; reproduce both grammars'
  component inventories and work counts. Record remaining refusals as evidence, not
  step savings. Commit.

### 11. Connect factored construction at the actual dispatch seam

**Files:** Modify `rust/crates/pg-foma/src/analyzer.rs`, `factored.rs`,
`rust/crates/pg-foma-runtime/src/construction.rs` and
`rust/crates/pg-foma-backend/src/worker_contract.rs`; extend
`rust/crates/pg-foma/src/analyzer_compile_tests/construction_tests.rs` and worker tests.

**Interface:** The shared `compile_proposer_with_config` dispatcher connects the
underlying relation, bounded Stage 1 phonology and spelling and constructs the
runtime proposer. `surface-enum` stays the current distinct strategy. New budget
and evidence fields round-trip through the worker protocol; bump it when required.

- [ ] First write a failing integration test proving `factored` actually calls
  the new cascade and connects its analysis tape, with zero calls to the old subset
  path. Test atomic failure, auto strategy ordering and explicit selection without
  strategy fallback, plus older-worker rejection of unsupported controls.
- [ ] Compose through slice 3b's budget owner and minimize after each connection.
  Apply existing correctness decisions at the per-strategy seam, keeping readiness
  advisory to construction. Wire slice 7's per-word runtime policy through the shared
  analyzer; refuse any configuration whose control cannot be passed through.
- [ ] Run construction/completed-build/worker gates and full conformance. Freeze
  whole-parser binaries and run both grammars' plain and factored modes via slice 1.
  Report incomplete builds as such, and all complete words' P/C/F/A and multiset
  comparisons. This first integrated result may correctly recommend plain parsing.
  Commit; never claim positive savings from the earlier component gates.

### 12. Admit only demonstrated work savings

**Files:** Modify `rust/crates/pg-foma-backend/src/backend_runtime.rs`,
`rust/crates/pg-foma/src/differential.rs` and `rust/tools/fst_differential.py`;
extend backend-runtime and differential tests and update
`docs/development/fst-construction-strategies.md` with measured implemented status.

**Interface:** Consume slice 1's complete step evidence and slice 7's immutable
calibration configuration. Report correct-but-uneconomic artifacts as development
readiness evidence; production uses plain parsing for that grammar. Keep the
selection decision in its current owner, shared by all consumers.

- [ ] Test positive/zero/negative actual savings, complete accelerated rows above
  25%, undefined zero-work ratios, missing calibration and fallback-heavy corpora.
  Assert the production route really becomes plain and no readiness finding blocks
  an explicitly requested contained construction experiment.
- [ ] Implement the ADR's weighted metric, calibration and admission without any
  per-word cache or online reference parse. Preserve incomplete rows and both
  divergence directions in the report; never promote incomplete evidence.
- [ ] On frozen before/after binaries run both whole Cebuano/Amharic lists with
  FST off/on, then the five reference/stress grammars and worst-word controls.
  Require every occurrence, full identity-multiset parity, two-sided proposal/filter
  evidence and C/P maxima. Report weighted `% less work` including fallbacks;
  disable production acceleration on nonpositive savings or a completed accelerated
  word above 25%. Run backend/differential gates, full conformance and final check.
  Commit the measurement manifest and the production routing decision together.

The final review checks that every ADR budget-table row has an acting control,
that large native temporaries cannot escape accounting, and that the work-saving
claim refers to the integrated parser rather than only the Stage 1 component.
