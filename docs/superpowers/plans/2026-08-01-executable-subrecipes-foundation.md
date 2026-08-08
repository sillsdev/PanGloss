# Executable Subrecipes Foundation Implementation Plan

> **Superseded for implementation on 2026-08-01:** The first cleanup audit found that this plan
> would add a fourth grammar-truth walker, persist an incomplete/FNV-bound Plan projection, and
> permit a candidate to declare one adapter while executing another. Preserve this document as the
> original design record; execute
> [`2026-08-01-grammar-compiler-and-recipe-parity.md`](2026-08-01-grammar-compiler-and-recipe-parity.md)
> instead. Its semantic/evidence spine and executable-candidate gates must land before mechanism
> extraction resumes.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the fail-closed, typed foundation that turns grammar facts into auditable executable subrecipes, then prove one complete-template morphotactics slice without language-name routing.

**Architecture:** Preserve the closed Plan relation algebra and existing compilers. Add a capability-backed `MechanismGraph`, validated interface contracts, a versioned executable-recipe artifact, and maintained per-mechanism research dossiers. Physical lowering remains a separate adapter; every selectable result still passes complete HermitCrab multiset certification.

**Tech Stack:** Rust 2021, serde/serde_json, pg-grammar model IDs, pg-foma capability/Plan/runtime modules, JSON Schema, OpenSpec Markdown, managed PowerShell build entry point `rust/tools/pg.ps1`.

---

## Scope decomposition

This plan delivers the shared foundation and the first `Morphotactics → BoundaryCleanup` vertical
slice. The remaining mechanisms are independent follow-on plans, in order:

1. `StaticPartition → OrderedPhonology`;
2. `StructuralAllomorph`;
3. real per-stratum morphology/phonology;
4. bounded and peeled `CopyProcess`;
5. remaining orthogonal conformance rows and four-language scale certification.

Each follow-on must update its dossier and add two independent exercises where possible before it
can claim coverage.

## File map

- Create `rust/crates/pg-foma/src/recipe_mechanism.rs`: deep mechanism vocabulary, graph,
  contracts, validation, and capability-backed extraction.
- Modify `rust/crates/pg-foma/src/lib.rs`: export `recipe_mechanism`.
- Create `rust/crates/pg-foma/tests/recipe_mechanism_graph.rs`: graph/extractor validation.
- Modify `rust/crates/pg-foma/src/enumerate.rs`: attach an executable-recipe description without
  changing existing candidate semantics.
- Modify `rust/crates/pg-foma/src/recipe_registry.rs`: make the existing materializer/applicability
  owner construct and validate typed mechanisms; it may not bypass an incompatible contract.
- Modify `rust/crates/pg-cli/src/recipe_optimize.rs`: update direct `CandidatePlan` construction.
- Modify `rust/crates/pg-foma/src/recipe_runtime.rs`: consume/validate the typed description before
  lowering; retain fail-closed corpus certification.
- Create `rust/crates/pg-foma/tests/executable_recipe_artifact.rs`: artifact round-trip and
  corruption tests.
- Create `rust/crates/pg-foma/schemas/executable-recipe.schema.json`: independently maintained
  `pangloss.executable-recipe/v1` schema.
- Create `docs/fst-plan/subrecipes/{morphotactics,static-partition,ordered-phonology,
  structural-allomorph,copy-process,boundary-cleanup}.md`: maintained research dossiers.
- Modify `openspec/changes/cleanup-and-recipe-parity/{design.md,tasks.md}` and
  `docs/fst-plan/recipe-parity-plan-2026-07-30.md`: authoritative decisions, task gates, and
  scoreboard prerequisites.
- Create/extend conformance fixtures only in the established synthetic fixture roots; never check
  private corpus data into Git.

### Task 1: Close the mixed-corpus certification P0 — completed in `7bcbafb`

**Files:**
- Modify: `rust/crates/pg-foma/src/recipe_runtime.rs:906-945`
- Test: `rust/crates/pg-foma/tests/recipe_runtime_oracle_bound_gate.rs`

- [x] **Step 1: Preserve the already-recorded behavioral RED**

The test `a_mixed_complete_and_capped_oracle_cannot_certify_the_complete_subset` must use one
complete word and one capped word and require every evaluation to be:

```rust
Certification::Truncated { ref stage } if stage == "oracle-capped"
```

- [x] **Step 2: Run the focused test and verify the real failure**

Run:

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma `
  -Filter a_mixed_complete_and_capped_oracle_cannot_certify_the_complete_subset
```

Expected RED: an evaluation is `FullHcConfirmed { words: 1, .. }`, proving subset certification.
Recorded 2026-08-01: exactly `FullHcConfirmed { words: 1, .. }` before the production edit.

- [x] **Step 3: Apply the minimal fail-closed guard**

In `evaluate_plans_marked`, replace the all-excluded-only guard with:

```rust
if oracle_capped || oracle_timed_out {
    let stage = if oracle_timed_out {
        "oracle-timeout"
    } else {
        "oracle-capped"
    };
    return plans
        .iter()
        .map(|marked| RuntimeEvaluation::truncated(marked, stage))
        .collect();
}
```

Use the existing local constructor/shape at that site; do not add a second truncation representation.

- [x] **Step 4: Run focused GREEN and the oracle regression set**

Recorded 2026-08-01: the single regression passed, then the supported name filter ran 53 oracle
tests with 53 passed and 669 skipped:

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter oracle
```

Expected: both tests pass; complete-corpus behavior remains selectable.

- [x] **Step 5: Commit only the P0 files**

```powershell
git add -- rust/crates/pg-foma/src/recipe_runtime.rs `
  rust/crates/pg-foma/tests/recipe_runtime_oracle_bound_gate.rs
git commit -m "fix: reject partially certified recipe corpora"
```

Completed as `7bcbafb`. Do not replay this RED or create a duplicate commit during plan execution.

### Task 2: Correct D4 Pareto and report integrity

**Files:**
- Modify: `rust/crates/pg-foma/src/recipe_optimizer.rs:184-260,740-780`
- Modify: `rust/crates/pg-foma/src/recipe_report.rs:127-152`
- Test: existing unit-test modules in both files

- [x] **Step 1: Complete the existing D4 RED table**

Define one table-driven test where each certified candidate differs only in one coordinate of:

```rust
(confirmation_steps, raw_paths, confirmation, proposals, states, arcs)
```

Require the lower candidate to dominate for each coordinate; timing-only differences must not
change the frontier; an uncertified candidate must never enter it.

- [x] **Step 2: Verify the RED**

Run:

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter pareto
```

Expected: step/raw-path/timing cases fail against the current comparator.

- [x] **Step 3: Add one named deterministic Pareto vector**

Implement:

```rust
impl Score {
    fn pareto_vector(&self) -> [u64; 6] {
        [
            self.confirmation_steps,
            self.raw_paths,
            self.confirmation,
            self.proposals,
            self.states,
            self.arcs,
        ]
    }
}
```

Make `dominates` compare only that componentwise vector. Keep `Score::key()` separate and keep
wall-clock `build`/`apply` out of both policies.

- [x] **Step 4: Make report validation recompute derived decisions**

Add RED tests that corrupt serialized `pareto_frontier` and `winner`; then make
`RecipeOptimizationReport::validate()` recompute both from certified candidates and require exact
agreement. Legacy reports missing `raw_paths` may deserialize, but must not be compared across
reports as if zero were measured evidence.

- [x] **Step 5: Run focused and report tests**

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter recipe_optimizer
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter recipe_report
```

Expected: all focused tests pass.

- [x] **Step 6: Commit the D4/report repair**

```powershell
git add -- rust/crates/pg-foma/src/recipe_optimizer.rs `
  rust/crates/pg-foma/src/recipe_report.rs
git commit -m "fix: validate deterministic recipe frontier"
```

### Task 3: Add the deep mechanism vocabulary

**Files:**
- Create: `rust/crates/pg-foma/src/recipe_mechanism.rs`
- Modify: `rust/crates/pg-foma/src/lib.rs`
- Test: `rust/crates/pg-foma/tests/recipe_mechanism_graph.rs`

- [x] **Step 1: Write graph-validation RED tests**

Test these exact functions using direct `MechanismGraph { nodes, edges }` values:

```text
recipe_mechanism_rejects_missing_producer
  => Err(MechanismGraphError::MissingEndpoint { edge, endpoint: Producer })
recipe_mechanism_rejects_symbol_space_mismatch
  => Err(MechanismGraphError::SymbolSpaceMismatch { producer, consumer })
recipe_mechanism_rejects_boundary_cleanup_before_boundary_consumer
  => Err(MechanismGraphError::CleanupNotTerminal { cleanup })
recipe_mechanism_rejects_duplicate_mechanism_id
  => Err(MechanismGraphError::DuplicateId { id })
recipe_mechanism_rejects_cycle
  => Err(MechanismGraphError::Cycle { members })
recipe_mechanism_rejects_lost_analysis_or_root_identity
  => Err(MechanismGraphError::UnsatisfiedState { field: Identity, .. })
recipe_mechanism_rejects_multiplicity_weakening
  => Err(MechanismGraphError::UnsatisfiedState { field: Multiplicity, .. })
recipe_mechanism_rejects_dynamic_state_or_stratum_mismatch
  => Err(MechanismGraphError::UnsatisfiedState { field, .. })
recipe_mechanism_rejects_exact_consumer_after_confirm_only_producer
  => Err(MechanismGraphError::DispositionMismatch { producer, consumer })
recipe_mechanism_accepts_composable_morphotactics_cleanup_graph
  => Ok(())
```

Construct every `InterfaceContract` field explicitly in the test so adding a new contract field
forces fixture review. Each invalid graph must return the typed error above, not a stringly panic.

- [x] **Step 2: Verify tests fail because the module/types do not exist**

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter recipe_mechanism_
```

Expected RED: unresolved `pg_foma::recipe_mechanism` import.

- [x] **Step 3: Implement the minimal serializable types**

Create strongly typed IDs and enums from the approved spec:

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MechanismId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionDisposition { ExactFst, ConfirmOnly, Peeled, Refused }

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MechanismKind {
    Morphotactics, StaticPartition, OrderedPhonology,
    StructuralAllomorph, CopyProcess, BoundaryCleanup,
}

pub enum CopyKind { Prefix, Suffix, FullStem, InternalSpan }
pub enum OrderedRuleAtom {
    Rewrite { rule: WireModelId },
    Metathesis { rule: WireModelId, swap_construction_attempted: bool },
}
pub enum PartitionPredicate {
    Pos(String), Mpr(WireModelId), LexicalClass(String), StemFamily(WireModelId),
}
pub struct PartitionGroupSpec {
    pub id: String,
    pub predicates: Vec<PartitionPredicate>,
    pub members: Vec<WireModelId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum MechanismBody {
    Morphotactics(MorphotacticsSpec),
    StaticPartition(StaticPartitionSpec),
    OrderedPhonology(OrderedPhonologySpec),
    StructuralAllomorph(StructuralAllomorphSpec),
    CopyProcess(CopyProcessSpec),
    BoundaryCleanup(BoundaryCleanupSpec),
}
```

Do not add serde derives to public `pg-grammar` IDs or implicitly serialize `ModelLocation`.
Define local, wire-safe `WireModelId { kind, value }` and
`MechanismSource { kind, owner, child }` wrappers in this module. Add explicit, tested conversions
from every native ID used here and from `ModelLocation`. The `kind` discriminator prevents IDs
from different domains with equal text from colliding. Round-trip tests cover every represented ID
domain; serialization ownership remains in `recipe_mechanism.rs`.

Define focused spec structs rather than a map of arbitrary JSON values:

```rust
pub struct MorphotacticsSpec {
    pub strata: Vec<WireModelId>,
    pub templates: Vec<WireModelId>,
    pub rules: Vec<WireModelId>,
    pub cooccurrence_units: Vec<Vec<String>>,
    pub priority_chains: Vec<Vec<WireModelId>>,
    pub max_depth: Option<usize>,
}

pub struct StaticPartitionSpec {
    pub predicates: Vec<PartitionPredicate>,
    pub groups: Vec<PartitionGroupSpec>,
    pub stable_for_lifetime: bool,
}

pub struct OrderedPhonologySpec {
    pub stratum: WireModelId,
    pub rules: Vec<OrderedRuleAtom>,
}

pub struct StructuralAllomorphSpec {
    pub rule: WireModelId,
    pub allomorphs: Vec<WireModelId>,
    pub bounded_local_shape: bool,
}

pub struct CopyProcessSpec {
    pub rule: WireModelId,
    pub kind: CopyKind,
    pub max_span: Option<usize>,
    pub max_chain_depth: usize,
}

pub struct BoundaryCleanupSpec {
    pub table: WireModelId,
    pub boundary_symbols: Vec<String>,
}
```

The edge contract explicitly separates what the producer guarantees from what the consumer needs:

```rust
pub struct InterfaceContract {
    pub provided: ProvidedInterface,
    pub required: RequiredInterface,
}

pub struct ProvidedInterface {
    pub symbol_space: SymbolSpace,
    pub analysis_identity: IdentityGuarantee,
    pub root_identity: IdentityGuarantee,
    pub multiplicity: MultiplicityGuarantee,
    pub boundaries: BoundaryGuarantee,
    pub dynamic_state: DynamicState,
    pub stratum: Option<WireModelId>,
    pub disposition: ExecutionDisposition,
    pub copy_span: CopySpanGuarantee,
}

pub struct RequiredInterface {
    pub symbol_space: SymbolSpace,
    pub analysis_identity: IdentityRequirement,
    pub root_identity: IdentityRequirement,
    pub multiplicity: MultiplicityRequirement,
    pub boundaries: BoundaryRequirement,
    pub dynamic_state: DynamicState,
    pub stratum: Option<WireModelId>,
    pub accepted_dispositions: BTreeSet<ExecutionDisposition>,
    pub copy_span: CopySpanRequirement,
}

pub struct DynamicState {
    pub pos: BTreeSet<String>,
    pub mpr: BTreeSet<WireModelId>,
    pub lexical_classes: BTreeSet<String>,
    pub stem_families: BTreeSet<WireModelId>,
}

pub enum SymbolSpace { Surface(WireModelId), CharDefTokens(WireModelId) }
pub enum IdentityGuarantee { Unknown, Preserved }
pub enum IdentityRequirement { Any, Preserved }
pub enum MultiplicityGuarantee { Unknown, SetOnly, ExactMultiset }
pub enum MultiplicityRequirement { Any, SetOrBetter, ExactMultiset }
pub enum BoundaryGuarantee { Unknown, Present, Removed }
pub enum BoundaryRequirement { Any, Present, Removed }
pub enum CopySpanGuarantee { None, Bounded(usize), UnboundedPreserved }
pub enum CopySpanRequirement { None, BoundedAtMost(usize), AnyPreserved }
```

`provided` satisfies `required` by these complete rules:

- `SymbolSpace` variants and active `TableId` match exactly.
- `Preserved` identity satisfies `Any` or `Preserved`; `Unknown` satisfies only `Any`.
- multiplicity strength is `ExactMultiset > SetOnly > Unknown`; requirements name the minimum.
- `BoundaryRequirement::Any` accepts any guarantee; other requirements require equality.
- provided POS/MPR/lexical-class/stem-family sets are supersets of required sets.
- required `stratum: None` accepts any stratum; `Some(id)` requires exact `StratumId` equality.
- copy `None` requires `None`; `BoundedAtMost(m)` accepts `None` or `Bounded(n)` where `n <= m`;
  `AnyPreserved` accepts bounded or unbounded preservation, but not `None`.
- `accepted_dispositions` contains the producer disposition; `Refused` never satisfies an edge.
  An exact consumer lists only `ExactFst`, so `ConfirmOnly` and `Peeled` need explicit adapters.

- [x] **Step 4: Implement `MechanismGraph::validate()`**

Validation must check unique/stable IDs, existing edge endpoints, acyclicity, one compatible
contract per edge, required/provided state, and terminal cleanup. It must not encode language names.

- [x] **Step 5: Run tests and rustfmt**

```powershell
Push-Location rust
cargo fmt --all -- --check
Pop-Location
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter recipe_mechanism_
```

Expected: formatter and graph tests pass.

`cargo fmt` is the explicit non-build exception allowed by `CLAUDE.md`; all compilation and tests
still go through `pg.ps1`. Every test function above has the `recipe_mechanism_` prefix, so the
managed filter cannot silently select zero tests.

- [x] **Step 6: Commit the vocabulary**

```powershell
git add -- rust/crates/pg-foma/src/recipe_mechanism.rs `
  rust/crates/pg-foma/src/lib.rs `
  rust/crates/pg-foma/tests/recipe_mechanism_graph.rs
git commit -m "feat: model executable recipe mechanisms"
```

### Task 4: Extract mechanisms from the capability spine

**Files:**
- Modify: `rust/crates/pg-foma/src/recipe_mechanism.rs`
- Test: `rust/crates/pg-foma/tests/recipe_mechanism_graph.rs`

- [ ] **Step 1: Add language-name-free extraction REDs**

For staged synthetic fixtures, assert structural facts rather than labels only:

```rust
let profile = pg_foma::capability::characterize(&grammar);
let graph = extract_mechanisms(&grammar, &profile).unwrap();
assert!(graph.has_kind(MechanismKind::Morphotactics));
assert!(graph.has_kind(MechanismKind::BoundaryCleanup));
```

Add specific tests for a gated fixture, a metathesis fixture, a true-copy fixture, and an inert
reduplication-hint fixture. The inert hint must produce no `CopyProcess` node.

Each RED must also assert model-derived structure:

- the complete-template fixture has `stratum-{id}-morphotactics`, exact `StratumId`, `TemplateId`,
  and `MRuleId` payloads, exact `ModelLocation` sources, and a terminal cleanup edge whose active
  symbol-space `TableId` matches;
- the gated fixture's partition contains exact rewrite/subrule source IDs, predicate payload, and
  required POS/MPR state;
- the metathesis ordered-rule atom contains exact `PRuleId`, stratum, and
  `swap_construction_attempted` admission fact from `MetathesisDetail`;
- true copy contains exact rule/allomorph source, maximum span, disposition, and copy-span contract,
  while the inert hint produces no copy node;
- edges are in stable topological order and every contract field is asserted explicitly.

Load every fixture twice into fresh grammar values, extract twice, and require byte-identical
canonical serialization with sorted unique nodes, edges, and sources. Prefix every test function
with `recipe_mechanism_` so the managed filter below selects them.

- [ ] **Step 2: Verify RED against the absent extractor**

Run the test filter and require unresolved `extract_mechanisms` or failed expected kind.

- [ ] **Step 3: Implement one shared extraction pass**

Use `CharacteristicsProfile::observations()` and their typed `ModelLocation`/`ObservationDetail`
values for construct facts. Read grammar templates, strata, and ordered rule IDs only where the
capability profile lacks the required structural relation. Do not reproduce capability predicates.

Stable IDs must derive from model IDs and mechanism kind, for example:

```rust
MechanismId(format!("stratum-{}-ordered-phonology", stratum.0))
```

Never include a language name or fixture name.

- [ ] **Step 4: Validate and canonicalize output**

Sort nodes/edges by stable IDs, deduplicate identical source locations, call `validate()`, and
return a typed extraction error on unsupported contracts.

- [ ] **Step 5: Run graph and capability regression tests**

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter recipe_mechanism_
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter capability
```

- [ ] **Step 6: Commit extraction**

```powershell
git add -- rust/crates/pg-foma/src/recipe_mechanism.rs `
  rust/crates/pg-foma/tests/recipe_mechanism_graph.rs
git commit -m "feat: extract recipe mechanisms from grammar facts"
```

### Task 5: Add a versioned executable-recipe artifact

**Files:**
- Modify: `rust/crates/pg-foma/src/enumerate.rs`
- Modify: `rust/crates/pg-foma/src/recipe_registry.rs`
- Modify: `rust/crates/pg-cli/src/recipe_optimize.rs`
- Create: `rust/crates/pg-foma/schemas/executable-recipe.schema.json`
- Create: `rust/crates/pg-foma/tests/executable_recipe_artifact.rs`
- Modify: `rust/crates/pg-foma/src/recipe_runtime.rs`

- [ ] **Step 1: Add round-trip and corruption REDs**

Generate a real artifact from a staged grammar. Independently validate JSON against the checked-in
schema. Corrupt each of: version, missing producer, symbol space, multiplicity contract, cleanup
ordering, unknown runtime operation, and bound Plan root. Each corruption must fail closed with its
field named. Add `executable_recipe_registry_cannot_bypass_incompatible_mechanism_contract`: the existing
`Registry::materialize` path must return a typed error rather than selecting an
`EmissionStrategy` that violates the graph.

Prefix every test function in `executable_recipe_artifact.rs` with `executable_recipe_`; the
managed filter below must execute every corruption and bypass case rather than merely the file.

- [ ] **Step 2: Verify RED**

Run:

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter executable_recipe_
```

Expected: artifact/schema API is absent.

- [ ] **Step 3: Define the artifact**

Separate the in-memory executable object from its wire artifact. The in-memory value owns the exact
Plan that will execute:

```rust
pub struct ExecutableRecipe {
    pub mechanisms: MechanismGraph,
    pub plan: Plan,
    pub adapter: LoweringAdapter,
    pub runtime_ops: Vec<RuntimeOp>,
}

pub enum LoweringAdapter {
    PlanComposed,
    TunedSurfaceProbed,
    TemplatedUnderlyingTokens,
}

pub enum RuntimeOp {
    ReduplicationPeel { rule: MRuleId, max_chain_depth: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireRuntimeOp {
    ReduplicationPeel { rule: WireModelId, max_chain_depth: usize },
}
```

The serializable projection binds to `Plan::root().as_u64()`. That root is already a deterministic
content address over the complete child/config graph in the current Plan schema:

```rust
pub const EXECUTABLE_RECIPE_SCHEMA: &str = "pangloss.executable-recipe/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutableRecipeArtifact {
    pub schema: String,
    pub mechanisms: MechanismGraph,
    pub plan_schema: String,
    pub plan_root: u64,
    pub adapter: LoweringAdapter,
    pub runtime_ops: Vec<WireRuntimeOp>,
}
```

Artifact construction converts native `RuntimeOp` values to `WireRuntimeOp`; validation converts
back only after the `WireModelId.kind == "m-rule"` discriminator and referenced rule existence are
verified. Add a corruption test for a runtime-op ID with the wrong domain tag.

Set `plan_schema` to a versioned constant such as `pangloss.plan-root/v1`; it records that the root
is tied to the current deterministic Plan content-addressing contract, not a cryptographic or
cross-toolchain hash. The wire artifact does not duplicate the Plan arena.

- [ ] **Step 4: Validate before lowering**

`CandidatePlan` must carry one in-memory `ExecutableRecipe` (or its graph plus owned Plan and
adapter with an accessor producing that view). `Registry::materialize` remains the decision owner:
it extracts/validates mechanisms first, then chooses an adapter licensed by their contracts. The
CLI's direct candidate constructors follow the same path. `recipe_runtime` recomputes
`candidate.plan.root().map(NodeId::as_u64)` and requires exact equality with the artifact before
lowering. Invalid/mismatched artifacts produce a nonselectable typed failure and never fall back to
another compiler silently.

- [ ] **Step 5: Add and verify the independent schema**

Follow `rust/crates/pg-assess/schemas/README.md`: schema authorship stays independent of the Rust
emitter. Validate real Rust output plus field-specific corruptions.

- [ ] **Step 6: Run focused tests and commit**

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter executable_recipe_
git add -- rust/crates/pg-foma/src/enumerate.rs `
  rust/crates/pg-foma/src/recipe_registry.rs `
  rust/crates/pg-foma/src/recipe_runtime.rs `
  rust/crates/pg-cli/src/recipe_optimize.rs `
  rust/crates/pg-foma/schemas/executable-recipe.schema.json `
  rust/crates/pg-foma/tests/executable_recipe_artifact.rs
git commit -m "feat: validate executable recipe artifacts"
```

### Task 6: Establish maintained research dossiers

**Files:**
- Create: `docs/research/subrecipes/morphotactics.md`
- Create: `docs/research/subrecipes/static-partition.md`
- Create: `docs/research/subrecipes/ordered-phonology.md`
- Create: `docs/research/subrecipes/structural-allomorph.md`
- Create: `docs/research/subrecipes/copy-process.md`
- Create: `docs/research/subrecipes/boundary-cleanup.md`
- Test: `rust/crates/pg-foma/tests/subrecipe_dossier_contract.rs`

- [x] **Step 1: Add a dossier-contract RED**

Create a lightweight test/script that requires all six files and these exact headings:

```text
Scope
Languages and families in mind
Primary sources
Grammar facts
Formal model and regularity
Chosen architecture
Rejected architectures
Interfaces and interactions
Complexity and resource bounds
Conformance fixtures
Implementation status
Known gaps and split triggers
Research log
Evidence decisions
```

Prefix every Rust test function in this target with `subrecipe_dossier_`, including heading,
language-anchor, dated-log, link, and architecture-decision cases.

It must also require at least two non-empty language/family entries and one dated research-log row.
`Evidence decisions` must contain at least one dated row whose decision is exactly `fits`,
`refines`, or `splits/adds`, with the evidence and architectural consequence recorded.

- [x] **Step 2: Verify RED because dossiers are absent**

Run:

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter subrecipe_dossier_
```

Expected RED: the contract test names all six missing files.

- [x] **Step 3: Write source-backed initial dossiers**

Populate every required section using the approved design and the primary-source ledger in
`docs/fst-plan/linguistic-recipe-harvest.md`. Mark claim-level confidence and distinguish current
repository evidence from external linguistic evidence. Do not claim implementation where only
research exists.

- [x] **Step 4: Fill the two explicit research gaps**

Before `CopyProcess` and `StructuralAllomorph` leave research status, identify a second primary
grammar source for productive full copying and a second Semitic/root-pattern system. Record why
each source exercises a different edge from Indonesian or Amharic.

- [x] **Step 5: Run the dossier contract and link checks**

Run the same `subrecipe_dossier_` filter. Expected: all headings, language anchors, dated rows,
evidence decisions, and local links pass.

- [x] **Step 6: Commit dossiers**

```powershell
git add -- docs/fst-plan/subrecipes
git commit -m "docs: establish subrecipe research dossiers"
```

### Task 7: Implement the first complete-template vertical slice

**Files:**
- Modify: `rust/crates/pg-foma/src/enumerate.rs`
- Modify: `rust/crates/pg-foma/src/recipe_registry.rs`
- Modify: `rust/crates/pg-foma/src/recipe_runtime.rs`
- Modify: `rust/crates/pg-foma/src/templated_compile.rs`
- Test: existing/new promoted template fixture tests under `rust/crates/pg-foma/tests/`
- Update: `docs/research/subrecipes/morphotactics.md`

- [ ] **Step 1: Add two independent semantic RED fixtures**

Prefix every new test function in this slice with `recipe_template_`; keep the two cleanup
idempotence cases and the contract-rejection case under the same prefix so the focused command
cannot skip them.

Exercise:

1. paired/discontinuous template members with illegal half-only and reversed-order negatives;
2. competing complete templates sharing an ambiguous exponent, with exact morpheme identity and
   multiplicity.

Use language-neutral fixture identifiers; dossier prose records Caquinte and Orizaba Nahuatl as
research anchors.

Add two BoundaryCleanup exercises in the same slice:

1. a Sena-shaped zero/boundary-only allomorph relation where cleanup is idempotent;
2. a Caquinte-shaped boundary-consuming epenthesis/metathesis relation where cleanup-before-rule
   changes the oracle and must be rejected.

For both, compile cleanup once and twice and require identical normalized apply results. A
surface-symbol producer connected to a char-def-token cleanup adapter must fail validation.

- [ ] **Step 2: Verify current plan-composed path fails semantic parity**

Run the focused promoted-fixture tests. Expected RED must show proposal multiset mismatch or an
unsupported typed mechanism, not merely a different network size.

- [ ] **Step 3: Route the existing template-aware emitter through the typed mechanism**

When `MorphotacticsSpec` contains templates, `Registry::materialize` must select the complete
templated underlying relation through the typed contract. It must not use `uflexc`, split paired
units, invent optional subsets, or allow the old applicability/strategy path to bypass graph
validation. `BoundaryCleanup` remains terminal and adapter-compatible.

- [ ] **Step 4: Assert exact proposal and confirmed multisets**

Require ordered morpheme IDs, root identity/index, and multiplicity. Include a mutation that splits
the paired unit and prove the gate fails.

- [ ] **Step 5: Run focused and cross-compiler gates**

```powershell
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter recipe_template_
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter pipeline
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter observed_evidence
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter template_flattened
```

Expected: exact fixture parity, no proposal-ratio regression, and no change to default emission.

- [ ] **Step 6: Update the dossier with measured complexity**

Record stage counts, states/arcs, legal template paths, rejected paths, and observed versus predicted
growth. Add a dated research-log entry and any refinement trigger discovered.

- [ ] **Step 7: Commit the vertical slice**

```powershell
git add -- rust/crates/pg-foma/src/enumerate.rs `
  rust/crates/pg-foma/src/recipe_registry.rs `
  rust/crates/pg-foma/src/recipe_runtime.rs `
  rust/crates/pg-foma/src/templated_compile.rs `
  rust/crates/pg-foma/tests `
  docs/research/subrecipes/morphotactics.md
git commit -m "feat: materialize complete-template subrecipes"
```

### Task 8: Reconcile plans, run gates, and obtain xhigh review

**Files:**
- Modify: `openspec/changes/cleanup-and-recipe-parity/design.md`
- Modify: `openspec/changes/cleanup-and-recipe-parity/tasks.md`
- Modify: `docs/fst-plan/recipe-parity-plan-2026-07-30.md`

- [ ] **Step 1: Record the approved architecture and task decomposition**

Add the six mechanisms, 11 conformance rows, dossier contract, follow-on plan order, and explicit
zero-exclusion requirement. Mark only tasks backed by current commits/tests as complete.

- [ ] **Step 2: Run managed foundation verification**

```powershell
Push-Location rust
cargo fmt --all -- --check
Pop-Location
& .\rust\tools\pg.ps1 -Mode test -Package pg-foma
& .\rust\tools\pg.ps1 -Mode corpus-test
```

Expected: all managed gates pass. If private corpus inputs are unavailable, record corpus-test as
not run with the exact missing paths; do not substitute a synthetic pass.

- [ ] **Step 3: Dispatch fresh xhigh Sol review**

The reviewer compares the full branch range against the design and checks: fail-closed
certification, deterministic D4, no language-name routing, mechanism/interface depth, schema
independence, fixture orthogonality, dossier completeness, and whether the first slice changes a
real mechanism rather than a label.

- [ ] **Step 4: Fix every blocking review finding and rerun affected gates**

Do not proceed to later mechanism plans while a P0/P1 finding remains open.

- [ ] **Step 5: Commit reconciled ledgers**

```powershell
git add -- openspec/changes/cleanup-and-recipe-parity/design.md `
  openspec/changes/cleanup-and-recipe-parity/tasks.md `
  docs/fst-plan/recipe-parity-plan-2026-07-30.md
git commit -m "docs: schedule executable subrecipe rollout"
```

## Foundation exit gate

The foundation is complete only when:

- mixed oracle truncation cannot certify a subset;
- Pareto/report decisions are deterministic and revalidated;
- graph/artifact corruption fails closed;
- extraction uses grammar facts and contains no language-name branch;
- six dossiers satisfy the maintained contract;
- two complete-template exercises pass exact multiset parity;
- the full managed pg-foma suite is green;
- a fresh xhigh review has no open P0/P1 finding.

Four-language parity remains a later goal and must not be inferred from this foundation gate.
