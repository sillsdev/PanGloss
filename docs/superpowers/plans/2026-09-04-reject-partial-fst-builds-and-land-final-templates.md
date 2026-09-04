# Reject Partial FST Builds and Land Final Templates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the final-template implementation while making every partial-bearing grammar ineligible for a production FST artifact across all three FST strategies, without changing HermitCrab semantics or preventing contained FST measurements.

**Architecture:** `pg-grammar` will own one validated inventory of every partial lexical entry and partial affix-process rule. FST compilers may still run under the existing contained measurement paths, but a shared post-compile production-admission function will attach a typed `NotProductionReady` finding and prevent a payload or selectable result from escaping; all three strategies must consume that one decision rather than re-deriving partiality. HC-Rust and final-template partial-rescue semantics remain unchanged.

**Tech Stack:** Rust 2021, `pg-grammar`, `pg-rules`, `pg-parse`, `pg-foma`, `pg-cli`, serde, vendored foma, YAML conformance fixtures, PowerShell-managed Cargo through `rust/tools/pg.ps1`.

---

## Scope and non-negotiable classifications

- Partial morphemes are valid HermitCrab grammar semantics. Loading them and running HC-Rust must continue to work.
- Their FST status is **production readiness**, represented by `Severity::NotProductionReady` and `FindingClass::Readiness`. It is not `CannotRepresent`, a capability refusal, or a resource-containment event.
- A contained development/conformance attempt may compile a complete FST so PanGloss can measure it. It cannot publish, serialize, select, or reconstruct that result as a trusted production artifact.
- The FST rule applies to both partial-bearing model shapes: `LexEntryDef.partial` and `AffixProcessRuleDef.partial`. No grammar name is special-cased.
- All three strategies in `strategy_coverage::ALL_STRATEGIES` receive the same post-compile admission decision: `TunedSurfaceProbed`, `TemplatedUnderlyingTokens`, and `PlanComposed`.
- Final-template pruning keeps Machine PR #491's partial-rescue behavior for HC-Rust. The FST production gate is a separate boundary.

## File map

- Modify `rust/crates/pg-grammar/src/model.rs`: define and compute validated `PartialMorphemeFacts`; make final-template facts reuse it; reject out-of-range morpheme strata.
- Modify `rust/crates/pg-grammar/src/load.rs`: add XML regression coverage for entry/rule partial inventory and invalid owner strata.
- Create `rust/crates/pg-foma/src/production_admission.rs`: own the single post-compile partial-readiness finding and publication decision.
- Modify `rust/crates/pg-foma/src/lib.rs`: expose the production-admission module.
- Modify `rust/crates/pg-foma/src/health.rs`: register the partial-morpheme readiness finding code and bump the pre-1.0 report schema.
- Modify `rust/crates/pg-foma/src/completed_build.rs`: split contained compilation from normal admitted compilation and discard partial-bearing production payloads.
- Modify `rust/crates/pg-foma/src/worker.rs`: preserve the typed readiness outcome and never write a raw payload frame for it.
- Modify `rust/crates/pg-foma/src/witnessed_coverage.rs`: keep explicit measurement compilation for all three strategies and apply shared admission only to production observations.
- Modify `rust/crates/pg-foma/src/backend_runtime.rs`: record the shared readiness finding after each strategy's real compile without changing accuracy or containment certification.
- Create `rust/crates/pg-foma/tests/partial_fst_production_admission_gate.rs`: differential, all-strategy, can-fire, and no-artifact gates.
- Modify `rust/crates/pg-foma/tests/envelope_agrees_with_compiler_gate.rs`: preserve both divergence directions and prove readiness rejection did not leak into capability selection.
- Modify `rust/crates/pg-foma/tests/five_language_backend_reports_gate.rs`: assert status from partial facts rather than grammar names.
- Modify `rust/crates/pg-cli/src/stats_cmd.rs`: label final-template prune counters as applying only to newly analyzed words.
- Modify `rust/crates/pg-parse/tests/csharp_port_affix_process.rs`: retain and strengthen HC-Rust final-template partial-rescue controls.
- Modify `machine/conformance/edge-cases/morphotactic-attribute-breadth/words.yaml`: add/clarify final and non-final template discriminator cases.
- Modify `machine/conformance/obligation-triage.tsv` and `machine/conformance/gate-obligations.tsv`: update only obligations proven by fresh executed evidence.
- Modify `docs/superpowers/plans/2026-09-03-final-template-prune.md`: reconcile task status with the implementation.
- Modify `docs/superpowers/plans/2026-09-03-final-template-prune-results.md`: replace the five-success narrative with valid HC evidence and expected FST rejection for every partial-bearing grammar.
- Create `docs/superpowers/plans/2026-09-04-reject-partial-fst-builds-results.md`: record exact fresh commands, commit, inventory, differential counts, and remaining limitations.

### Task 1: Establish the before-change differential and resource baseline

**Files:**
- Modify: `rust/crates/pg-foma/tests/envelope_agrees_with_compiler_gate.rs`
- Create: `docs/superpowers/plans/2026-09-04-reject-partial-fst-builds-results.md`

- [ ] **Step 1: Measure the host before any build-heavy work**

Run these read-only commands and paste the timestamped results into the results document:

```powershell
Get-CimInstance Win32_OperatingSystem | Select-Object TotalVisibleMemorySize,FreePhysicalMemory
Get-Counter '\Processor(_Total)\% Processor Time' -SampleInterval 1 -MaxSamples 3
Get-Process cargo,rustc,sccache,procgov -ErrorAction SilentlyContinue | Select-Object Name,Id,CPU,WorkingSet64
git rev-parse HEAD
```

Expected: enough free memory for one managed Rust build under the repository's 19 GB cap; if not, stop before Cargo starts and record the live contention.

- [ ] **Step 2: Extend the existing differential census before changing behavior**

Add a row field that distinguishes raw compile success from production admission while preserving the existing two envelope divergence counts:

```rust
#[derive(Default)]
struct DifferentialCounts {
    envelope_refuses_compiler_succeeds: usize,
    envelope_admits_compiler_fails: usize,
    compiler_succeeds_production_rejects: usize,
    compiler_succeeds_production_admits: usize,
}
```

The initial test must count all discovered fixtures × `ALL_STRATEGIES`, print all four counts, and use `FaithfulnessRequirement::NoMoreThan` for the two existing divergence directions. Do not assert the new production counts until the shared admission function exists.

- [ ] **Step 3: Run the baseline census**

```powershell
& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget envelope_agrees_with_compiler_gate
```

Expected: PASS with both existing divergence directions and the total observation count printed. Copy the exact counts into the results document under `Before change`.

- [ ] **Step 4: Commit the measurement before implementation**

```powershell
git add rust/crates/pg-foma/tests/envelope_agrees_with_compiler_gate.rs docs/superpowers/plans/2026-09-04-reject-partial-fst-builds-results.md
git commit -m "test(foma): measure partial production-admission differential"
```

### Task 2: Make partiality a validated grammar-owned fact

**Files:**
- Modify: `rust/crates/pg-grammar/src/model.rs`
- Modify: `rust/crates/pg-grammar/src/load.rs`

- [ ] **Step 1: Write failing model tests**

Add tests proving that the inventory counts both kinds independently, returns stable authored identities, and rejects a partial rule whose morpheme owner uses an out-of-range stratum:

```rust
let facts = grammar.partial_morpheme_facts().expect("valid partial inventory");
assert_eq!(facts.partial_entry_count(), 1);
assert_eq!(facts.partial_rule_count(), 1);
assert_eq!(facts.total_count(), 2);
assert!(facts.authored_ids().any(|id| id == "entry-partial"));
assert!(facts.authored_ids().any(|id| id == "rule-partial"));

grammar.morphemes[rule_morpheme.0 as usize].stratum = StratumId(255);
let error = grammar.partial_morpheme_facts().expect_err("invalid owner stratum must fail");
assert!(error.to_string().contains("stratum 255"));
```

- [ ] **Step 2: Run the focused test and observe RED**

```powershell
& rust/tools/pg.ps1 -Mode test -Package pg-grammar -Filter partial_morpheme_facts
```

Expected: FAIL because `PartialMorphemeFacts` and `Grammar::partial_morpheme_facts` do not exist.

- [ ] **Step 3: Implement the single owner computation**

Add a public immutable fact type with accessors, without exporting mutable collections:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartialMorphemeFacts {
    partial_entry_ids: Vec<String>,
    partial_rule_ids: Vec<String>,
}

impl PartialMorphemeFacts {
    pub fn partial_entry_count(&self) -> usize { self.partial_entry_ids.len() }
    pub fn partial_rule_count(&self) -> usize { self.partial_rule_ids.len() }
    pub fn total_count(&self) -> usize {
        self.partial_entry_ids.len() + self.partial_rule_ids.len()
    }
    pub fn has_partials(&self) -> bool { self.total_count() != 0 }
    pub fn authored_ids(&self) -> impl Iterator<Item = &str> {
        self.partial_entry_ids.iter().chain(&self.partial_rule_ids).map(String::as_str)
    }
}
```

`Grammar::partial_morpheme_facts` must validate every morpheme-owned stratum before collecting either list. Rule identities must come from the grammar's authored registry, not a formatted dense index. `final_template_prune_facts` must call this owner or a shared private validator; it must not repeat the partial-rule predicate.

- [ ] **Step 4: Run focused tests and all-target type checking**

```powershell
& rust/tools/pg.ps1 -Mode test -Package pg-grammar -Filter partial_morpheme_facts
& rust/tools/pg.ps1 -Mode test -Package pg-grammar -Filter final_template_prune
& rust/tools/pg.ps1 -Mode check -Package pg-grammar
```

Expected: all PASS; the invalid-stratum regression names the invalid owner instead of silently disabling no strata.

- [ ] **Step 5: Commit the grammar fact**

```powershell
git add rust/crates/pg-grammar/src/model.rs rust/crates/pg-grammar/src/load.rs
git commit -m "fix(grammar): validate and publish partial morpheme facts"
```

### Task 3: Add one typed post-compile FST production-admission decision

**Files:**
- Create: `rust/crates/pg-foma/src/production_admission.rs`
- Modify: `rust/crates/pg-foma/src/lib.rs`
- Modify: `rust/crates/pg-foma/src/health.rs`
- Test: `rust/crates/pg-foma/tests/partial_fst_production_admission_gate.rs`

- [ ] **Step 1: Write the failing decision tests**

Construct one no-partial control, one partial-entry grammar, and one partial-rule grammar. For each strategy assert:

```rust
let admission = assess_completed_fst(&grammar, strategy).expect("valid grammar facts");
assert_eq!(admission.strategy(), strategy);
assert_eq!(admission.health().admission(), Severity::NotProductionReady);
assert_eq!(admission.health().admission_by_class().readiness, Severity::NotProductionReady);
assert_eq!(admission.health().admission_by_class().representability, Severity::WithinLimits);
assert_eq!(admission.health().admission_by_class().containment, Severity::WithinLimits);
assert!(admission.blocks_publication());
```

The no-partial control must produce `WithinLimits` and `blocks_publication() == false` for all three strategies.

- [ ] **Step 2: Run the new target and observe RED**

```powershell
& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget partial_fst_production_admission_gate
```

Expected: FAIL because the module, finding code, and test target do not exist.

- [ ] **Step 3: Register the health finding**

Add `FindingCode::PartialMorphemeProductionPolicy` with the next unused stable PGF code and add `Metric::PartialMorphemeCount`. Their exhaustive metadata must be:

```rust
FindingCode::PartialMorphemeProductionPolicy => {
    "A completed FST was built from a grammar containing partial morphemes and is not eligible for production publication."
}
```

Classify it as `FindingClass::Readiness`, use `Phase::Compile`, `Severity::NotProductionReady`, `Metric::PartialMorphemeCount`, `MetricValue::Count(facts.total_count() as u64)`, and `ValueProvenance::Observed`. Increment `HEALTH_SCHEMA_VERSION` and update canonical JSON fixtures in `health.rs`.

- [ ] **Step 4: Implement the shared admission module**

Use one result type and one public function:

```rust
pub struct FstProductionAdmission {
    strategy: EmissionStrategy,
    health: HealthReport,
}

pub fn assess_completed_fst(
    grammar: &Grammar,
    strategy: EmissionStrategy,
) -> Result<FstProductionAdmission, GrammarError>;
```

The function calls only `grammar.partial_morpheme_facts()` to decide the policy. Its diagnostic includes counts by kind and a bounded sample of authored IDs. It never calls `select_backends`, never emits `CannotRepresent`, and never starts or suppresses compilation.

- [ ] **Step 5: Run the focused tests**

```powershell
& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget partial_fst_production_admission_gate
& rust/tools/pg.ps1 -Mode test -Package pg-foma -Filter finding_code
& rust/tools/pg.ps1 -Mode check -Package pg-foma
```

Expected: PASS for two partial shapes × three strategies and the three non-partial controls.

- [ ] **Step 6: Commit the admission decision**

```powershell
git add rust/crates/pg-foma/src/production_admission.rs rust/crates/pg-foma/src/lib.rs rust/crates/pg-foma/src/health.rs rust/crates/pg-foma/tests/partial_fst_production_admission_gate.rs
git commit -m "feat(foma): reject partial grammars at production admission"
```

### Task 4: Make normal artifact creation fail after a real contained attempt

**Files:**
- Modify: `rust/crates/pg-foma/src/completed_build.rs`
- Modify: `rust/crates/pg-foma/src/worker.rs`
- Modify: `rust/crates/pg-foma/tests/trusted_selected_build_gate.rs`
- Test: `rust/crates/pg-foma/tests/partial_fst_production_admission_gate.rs`

- [ ] **Step 1: Write failing no-artifact and can-fire tests**

For each buildable completed strategy, instrument the test compile seam and assert the compiler ran before admission rejected the result:

```rust
assert_eq!(compile_calls.load(Ordering::SeqCst), 1);
assert!(matches!(
    result,
    Err(CompletedBuildError::NotProductionReady { strategy, .. })
        if strategy == requested_strategy
));
```

At the worker boundary assert the metadata outcome is typed and no second frame exists:

```rust
assert!(matches!(result.outcome, CompileWorkerOutcome::SelectedNotProductionReady { .. }));
assert!(read_frame(&mut cursor, limit).is_err(), "no raw payload frame may escape");
```

- [ ] **Step 2: Run the focused targets and observe RED**

```powershell
& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget partial_fst_production_admission_gate
& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget trusted_selected_build_gate
```

Expected: FAIL because normal completed builds still return payloads for partial-bearing grammars.

- [ ] **Step 3: Split measurement completion from production admission**

Keep the existing backend-specific match in one private function:

```rust
fn compile_completed_backend_for_measurement(
    grammar: &Grammar,
    requested_strategy: EmissionStrategy,
    request: &CompileAttempt,
) -> Result<CompletedBackendBuild, CompletedBuildError>;
```

Make `compile_completed_backend` call it exactly once, then call `assess_completed_fst`. If readiness blocks publication, return:

```rust
CompletedBuildError::NotProductionReady {
    strategy: requested_strategy,
    health,
}
```

Do not include `CompletedBackendBuild`, payload bytes, or a payload fingerprint in that error. Keep the measurement function private to the module; the worker's finite execution envelope remains the only external way to run this attempt.

- [ ] **Step 4: Preserve the typed worker outcome**

Add:

```rust
SelectedNotProductionReady { health: HealthReport }
```

Map only `CompletedBuildError::NotProductionReady` to it. All other errors remain `SelectedCompileFailed`. `write_child_output` must write no payload frame for the new outcome.

- [ ] **Step 5: Run focused tests and check all targets**

```powershell
& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget partial_fst_production_admission_gate
& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget trusted_selected_build_gate
& rust/tools/pg.ps1 -Mode test -Package pg-foma -Filter selected_
& rust/tools/pg.ps1 -Mode check -Package pg-foma
```

Expected: partial grammars compile once under containment, return typed readiness failure, and emit zero payload frames; non-partial selected builds remain byte-for-byte reconstructable.

- [ ] **Step 6: Commit the production boundary**

```powershell
git add rust/crates/pg-foma/src/completed_build.rs rust/crates/pg-foma/src/worker.rs rust/crates/pg-foma/tests/trusted_selected_build_gate.rs rust/crates/pg-foma/tests/partial_fst_production_admission_gate.rs
git commit -m "fix(foma): prevent partial FST artifacts from escaping"
```

### Task 5: Cover all three backend strategies without suppressing measurement

**Files:**
- Modify: `rust/crates/pg-foma/src/witnessed_coverage.rs`
- Modify: `rust/crates/pg-foma/src/backend_runtime.rs`
- Modify: `rust/crates/pg-foma/src/backend_optimizer.rs`
- Modify: `rust/crates/pg-foma/tests/partial_fst_production_admission_gate.rs`
- Modify: `rust/crates/pg-foma/tests/envelope_agrees_with_compiler_gate.rs`

- [ ] **Step 1: Write the failing three-strategy effect test**

Run the real measurement adapter for every `ALL_STRATEGIES` entry. Assert each partial grammar records both facts: the raw attempt completed, and publication admission is `NotProductionReady`. The control grammar must complete and remain production-admissible.

```rust
for &strategy in ALL_STRATEGIES {
    compile_with_backend_for_measurement(&partial_grammar, strategy)
        .unwrap_or_else(|error| panic!("{strategy:?} measurement did not complete: {error}"));
    let admission = assess_completed_fst(&partial_grammar, strategy)
        .expect("partial facts must be valid");
    assert_eq!(admission.health().admission(), Severity::NotProductionReady);
    assert!(admission.blocks_publication());
}
```

- [ ] **Step 2: Run the test and observe RED**

```powershell
& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget partial_fst_production_admission_gate -Filter all_three
```

Expected: FAIL because `PlanComposed` has no shared post-compile readiness evidence.

- [ ] **Step 3: Add orthogonal readiness evidence to runtime evaluation**

Do not add a readiness variant to the accuracy `Certification` enum. Add an independent field:

```rust
pub struct RuntimeEvaluation {
    pub certification: Certification,
    pub production_health: HealthReport,
    pub score: Score,
    pub realized_strategy: EmissionStrategy,
    pub divergence: IdentityDivergence,
}
```

Populate it only after the real adapter returns. A build failure keeps its existing process/containment health. A completed build calls `assess_completed_fst(grammar, realized_strategy)` and merges that report with measured health. Selection requires both `certification.selectable()` and `production_health.admission() < Severity::NotProductionReady`.

- [ ] **Step 4: Keep measurement paths explicit**

Rename the raw witness helper to `compile_with_backend_for_measurement` and update its docs to state that it never returns or publishes an artifact. `observe_grammar` must record the compile witness and then attach the shared production admission; the envelope differential test must call the measurement helper so a readiness policy cannot masquerade as compiler failure.

- [ ] **Step 5: Finish the differential ratchet**

Assert both old divergence directions remain no worse than their recorded baseline. Add exact counts for `compiler_succeeds_production_rejects` and `compiler_succeeds_production_admits`, with non-vacuity assertions that both are greater than zero and that every rejection has `partial_morpheme_facts().has_partials() == true`.

- [ ] **Step 6: Run the three-strategy gates**

```powershell
& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget partial_fst_production_admission_gate
& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget envelope_agrees_with_compiler_gate
& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget witnessed_strategy_coverage_gate
& rust/tools/pg.ps1 -Mode check -Package pg-foma
```

Expected: all PASS; raw compile results remain measurable, all partial-bearing completed strategies are non-publishable, and capability divergence counts do not regress.

- [ ] **Step 7: Commit the three-strategy wiring**

```powershell
git add rust/crates/pg-foma/src/witnessed_coverage.rs rust/crates/pg-foma/src/backend_runtime.rs rust/crates/pg-foma/src/backend_optimizer.rs rust/crates/pg-foma/tests/partial_fst_production_admission_gate.rs rust/crates/pg-foma/tests/envelope_agrees_with_compiler_gate.rs
git commit -m "feat(foma): enforce partial readiness across every backend"
```

### Task 6: Close final-template correctness and reporting defects

**Files:**
- Modify: `rust/crates/pg-parse/tests/csharp_port_affix_process.rs`
- Modify: `rust/crates/pg-cli/src/stats_cmd.rs`

- [ ] **Step 1: Strengthen the HC-Rust partial-rescue regression**

Keep both memo modes and assert default Machine-compatible policy versus explicit always-enforce policy for partial rules. Also assert partial lexical entries do not stand in for partial rules. The test must compare complete analysis identity multisets, not only counts.

- [ ] **Step 2: Run the HC tests before production-gate changes can mask them**

```powershell
& rust/tools/pg.ps1 -Mode test -Package pg-parse -Filter partial_rule
& rust/tools/pg.ps1 -Mode test -Package pg-parse -Filter always_enforce_final_templates
```

Expected: PASS in memoized and unmemoized modes, proving FST policy did not alter HC semantics.

- [ ] **Step 3: Write the failing stats-label test**

Change the deterministic expected prefix to:

```text
FINAL_TEMPLATE_STATS_NEWLY_ANALYZED
```

Add a cached second-run assertion proving the line reports zero counters under that explicit scope while `stats: analyzed=0 skipped=N` agrees.

- [ ] **Step 4: Change the label and run the CLI tests**

```rust
format!(
    "FINAL_TEMPLATE_STATS_NEWLY_ANALYZED\ttemplate_entries={}\ttemplate_batteries_skipped={}\tfinal_templates_skipped={}",
    counters.template_entries,
    counters.template_batteries_skipped,
    counters.final_templates_skipped,
)
```

```powershell
& rust/tools/pg.ps1 -Mode test -Package pg-cli -Filter final_template_stats
& rust/tools/pg.ps1 -Mode test -Package pg-cli -Filter batch_stats_run_twice
& rust/tools/pg.ps1 -Mode check -Package pg-cli
```

Expected: PASS; a fully cached run can no longer look like a fresh run with no pruning activity.

- [ ] **Step 5: Commit HC and reporting fixes**

```powershell
git add rust/crates/pg-parse/tests/csharp_port_affix_process.rs rust/crates/pg-cli/src/stats_cmd.rs
git commit -m "fix(final-templates): preserve rescue semantics and label fresh stats"
```

### Task 7: Prove conformance grammar behavior and real-grammar policy

**Files:**
- Modify: `machine/conformance/edge-cases/morphotactic-attribute-breadth/words.yaml`
- Modify: `machine/conformance/obligation-triage.tsv`
- Modify: `machine/conformance/gate-obligations.tsv`
- Modify: `rust/crates/pg-foma/tests/five_language_backend_reports_gate.rs`
- Modify: `docs/superpowers/plans/2026-09-03-final-template-prune-results.md`
- Modify: `docs/superpowers/plans/2026-09-04-reject-partial-fst-builds-results.md`

- [ ] **Step 1: Add discriminating conformance cases**

The fixture must contain four independently observable cases:

1. a non-partial ordinary rule blocked after a final template;
2. the same ordinary rule permitted when the template is non-final;
3. a partial ordinary rule rejected as the completion step after a non-final template;
4. a non-partial ordinary rule completing that same non-final-template path.

Each word entry must name the expected morpheme sequence and the one changed attribute that would flip it. Do not update either TSV until the conformance runner observes the gate arm.

- [ ] **Step 2: Run the conformance target**

```powershell
& rust/tools/pg.ps1 -Mode conformance-test -Scope all -Package pg-parse -Filter final_template
```

Expected: PASS with non-zero executed Machine fixture cases and both Blocked/Control arms observed for both final-template obligations.

- [ ] **Step 3: Update obligation ledgers from executed evidence**

Change only rows 40–43 whose exact arm fired. Record the fixture word and test citation in `gate-obligations.tsv`; change `Open` only where the severance test produced the required fail-to-pass flip.

- [ ] **Step 4: Derive five-grammar expectations from facts**

In `five_language_backend_reports_gate.rs`, load each available grammar, call `partial_morpheme_facts`, and assert each post-compile admission directly:

```rust
let expected_publishable = !facts.has_partials();
for &strategy in ALL_STRATEGIES {
    let admission = assess_completed_fst(&grammar, strategy).expect("valid grammar facts");
    assert_eq!(!admission.blocks_publication(), expected_publishable);
}
```

Print per grammar: partial entry count, partial rule count, and each strategy's production status. Do not encode Mbugwe, Aweti, or any other grammar name in the decision.

- [ ] **Step 5: Run real-grammar gates under managed containment**

```powershell
& rust/tools/pg.ps1 -Mode corpus-test -Package pg-foma -TestTarget five_language_backend_reports_gate -TestThreads 1
```

Expected: every available partial-bearing grammar is an expected FST production rejection; every no-partial grammar follows its existing backend capability and runtime verdict. If private inputs are unavailable, record `not executed: corpus absent` and do not claim five-grammar verification.

- [ ] **Step 6: Correct the results narrative**

Preserve historical HC timings but label them as five-word, private local evidence. Replace claims that all five grammars are production successes with the fact-derived inventory and expected FST rejection. Record the exact tested commit and commands; explicitly retain the iterative-epenthesis HC divergence as a remaining limitation unless a separate tested fix lands.

- [ ] **Step 7: Commit conformance and evidence**

```powershell
git add machine/conformance/edge-cases/morphotactic-attribute-breadth/words.yaml machine/conformance/obligation-triage.tsv machine/conformance/gate-obligations.tsv rust/crates/pg-foma/tests/five_language_backend_reports_gate.rs docs/superpowers/plans/2026-09-03-final-template-prune-results.md docs/superpowers/plans/2026-09-04-reject-partial-fst-builds-results.md
git commit -m "test(final-templates): prove conformance and partial FST policy"
```

### Task 8: Reconcile the branch and run authoritative landing verification

**Files:**
- Modify: `docs/superpowers/plans/2026-09-03-final-template-prune.md`
- Modify: `docs/superpowers/plans/2026-09-04-reject-partial-fst-builds-results.md`

- [ ] **Step 1: Update plan status truthfully**

Mark completed final-template tasks checked only when their cited tests passed. Add a `Remaining limitations` section listing:

- iterative epenthesis remains an HC-Rust divergence if its ignored test is still ignored;
- private real-grammar evidence is local rather than CI-reproducible;
- any backend capability refusal or parity miss still present after this branch;
- partial-bearing grammars are intentionally FST-production-ineligible, not compiler failures.

- [ ] **Step 2: Rebase onto current main before the expensive suite**

```powershell
git fetch origin
git rebase origin/main
```

Expected: clean rebase. Resolve conflicts by preserving current-main containment and admission contracts; never copy an older generated result over a newer one.

- [ ] **Step 3: Re-measure resources and type-check every target**

```powershell
Get-CimInstance Win32_OperatingSystem | Select-Object TotalVisibleMemorySize,FreePhysicalMemory
Get-Process cargo,rustc,sccache,procgov -ErrorAction SilentlyContinue | Select-Object Name,Id,CPU,WorkingSet64
& rust/tools/pg.ps1 -Mode check
```

Expected: `cargo check --all-targets` succeeds for the workspace.

- [ ] **Step 4: Run fast unit verification**

```powershell
& rust/tools/pg.ps1 -Mode quick
```

Expected: all library and binary unit tests PASS.

- [ ] **Step 5: Run focused integration and conformance verification**

```powershell
& rust/tools/pg.ps1 -Mode test -Package pg-grammar -Filter partial_morpheme_facts
& rust/tools/pg.ps1 -Mode test -Package pg-parse -Filter final_template
& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget partial_fst_production_admission_gate
& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget envelope_agrees_with_compiler_gate
& rust/tools/pg.ps1 -Mode test -Package pg-foma -TestTarget trusted_selected_build_gate
& rust/tools/pg.ps1 -Mode conformance-test -Scope all -Package pg-parse -Filter final_template
```

Expected: all PASS; differential counts are no worse in either direction, all three strategies show the partial readiness effect, and HC conformance remains green.

- [ ] **Step 6: Run the authoritative workspace suite last**

```powershell
& rust/tools/pg.ps1 -Mode test
```

Expected: PASS with no unexpected skipped/failing target. Record total passed, failed, and skipped counts rather than only the exit code.

- [ ] **Step 7: Review the diff and results before claiming readiness**

```powershell
git status --short
git diff --check origin/main...HEAD
git log --oneline origin/main..HEAD
git diff --stat origin/main...HEAD
```

Expected: clean worktree, no whitespace errors, only intended files, and results referencing the actual final `HEAD`.

- [ ] **Step 8: Commit the final evidence update**

```powershell
git add docs/superpowers/plans/2026-09-03-final-template-prune.md docs/superpowers/plans/2026-09-04-reject-partial-fst-builds-results.md
git commit -m "docs: record final-template landing evidence"
```

## Completion criteria

The branch is ready to land only when all of these are true:

- HC-Rust still loads and evaluates partial entries and partial rules with Machine-compatible final-template behavior.
- The conformance fixture proves both arms of both final-template obligations.
- Every real FST compiler can still be exercised as an explicitly contained measurement.
- Every completed partial-bearing FST attempt is `NotProductionReady` and produces no selectable, serialized, published, or reconstructable production artifact.
- Both partial model shapes and all three strategies are covered by non-vacuous tests.
- The old capability-envelope differential is no worse in either direction.
- Non-partial controls continue through their existing capability, compile, and publication paths.
- The invalid morpheme-stratum case fails loudly.
- Stats output says its counters cover newly analyzed words.
- The results document names the exact final commit, exact commands, exact counts, unavailable private evidence, and every remaining limitation.
