# FST Completeness and Backend Advice Design

**Status:** Proposed implementation contract; architectural direction approved, pending document review.

> **Mbugwe boundary (deferred):** The Mbugwe-specific contract below is future reference only. It
> is explicitly excluded from the current Indonesian/Amharic/Aweti acceptance slice and must not be
> treated as an acceptance blocker or as evidence that Mbugwe has a trusted FST.

## Objective

PanGloss emits an FST only when it can establish that the selected backend preserves every analysis in the grammar's supported language. A large but complete construction may warn. A construction that stops before completeness produces no artifact.

This design replaces arbitrary morphological-chain truncation with three sound mechanisms:

1. a native FST loop for repetition that the backend can represent regularly;
2. exhaustive finite closure when the grammar proves a finite state space; or
3. a typed refusal when neither mechanism is available.

It also defines a structured, backend-specific advice catalog. A refusal must explain which grammar shapes defeated which backends and which grammar changes might make each backend viable, without claiming those changes are linguistically equivalent.

## Decisions

### Depth is evidence, not policy

Neither five nor ten morphological applications is intrinsically a warning or an error. Depth contributes to a cost projection, but it never authorizes truncation.

A finding is based on a predicted or observed resource problem, an unsupported semantic shape, or a violated completeness invariant. For example:

> Projected structural closure is 2.8 million composites. The largest factors are 14,102 roots, five non-loopable reachable rules, and 40 compatible rule-order states.

The diagnostic may mention depth as one factor. It must not say that depth five or ten is itself defective.

### Completeness is known at build time

PanGloss does not wait for a word to expose an omitted path. Completeness is decided from the grammar and the compiler's construction state.

For a finite construction, the compiler maintains a worklist of reachable abstract or concrete states. A successful build ends only when the worklist is empty. If a resource envelope stops the build while the worklist is nonempty, PanGloss knows the construction is incomplete and writes no FST.

For an unsupported or potentially unbounded construction, the characteristics check analyzes the grammar's transition graph at build time. A strongly connected component with no supported FST loop and no decreasing finite counter is not proven finitely enumerable. That backend refuses before expensive construction. Large or uncertain projected cost alone never becomes a correctness refusal; it remains cost evidence and is attempted only within the selected resource envelope.

The old pattern—return at a fixed recursion depth and treat emitted entries as complete—is forbidden.

### Confirmation cannot repair omission

`ConfirmOnly` is valid only when the FST proposer is a proven superset of the full HermitCrab analysis set. The Rust HC confirmer may remove false positives. It cannot create a candidate the proposer omitted.

Consequently, `RepresentsWithKnownGap` may not map to `ConfirmOnly` when “gap” means possible under-proposal. That backend must refuse the affected grammar shape. A separate representation state may describe a proven over-approximation.

### Resource retry and capability override are distinct

An explicit resource retry is a new caller-requested compilation under a named, versioned operational envelope. It may raise an entry, probe, state, arc, memory, or elapsed-time budget and reruns the complete algorithm from a clean state. PanGloss never escalates a budget or retries automatically, and the prior terminal finding remains in the build history.

A resource retry never converts a nonempty worklist into a successful artifact and never relabels incomplete output as trusted.

Separately, the explicit development capability override may force compilation after an Error or Critical characteristics-check result so developers can inspect partial state and build conformance evidence. Any resulting artifact is indelibly `unproven`, carries the degraded trust signal on load and every analysis, and cannot satisfy the conformance gate or become a proven release artifact. Apply-time execution containment remains nonoverrideable.

## Semantic model

### Construction classifications

Each backend classifies every reachable mechanism component as one of:

- `ExactLoop`: lowered to a native FST cycle with proven semantics;
- `ProvenSupersetLoop`: a native productive loop whose excess paths are safely removable by confirmation;
- `FiniteClosure`: exhaustively enumerable under a grammar-derived finite bound;
- `Unsupported`: no recall-preserving lowering is known; or
- `UnboundedUnknown`: a cycle exists but neither supported loop semantics nor a finite decreasing measure is proven.

Only the first three are normally buildable. `ProvenSupersetLoop` yields `ConfirmOnly`; `ExactLoop` and `FiniteClosure` yield exact proposal for that component. `Unsupported` and `UnboundedUnknown` refuse unless the explicit development capability override is in effect, in which case the result remains unproven.

### Abstract state for finite closure

The finite-closure analysis uses enough state to mirror the grammar's morphotactics:

- current stratum and ordered/unordered rule position;
- live template and slot positions;
- relevant syntactic, POS, MPR, and feature-structure state;
- per-rule application counters;
- root and partial-root admission facts;
- compound head/non-head state when applicable; and
- route kind and structural-operation state.

The first implementation may use a conservative over-approximation. Over-approximation may increase cost but may not remove a reachable state.

For ordinary and compounding rules, `multipleApplication` supplies a finite counter. The default value of one is meaningful evidence. Realizational rules currently expose no authored application cap and therefore require either a backend-native exact loop, a separate termination proof, or refusal.

### Closure certificate

Every successful FST build carries an internal `FstCompletenessCertificate` assembled from backend results. At minimum it records:

- backend and route identifiers;
- component classification;
- grammar-derived application bounds;
- cycle/SCC classification;
- whether zero-surface transitions occur in a cycle;
- explored and completed state counts;
- maximum observed application depth, as a metric only;
- entries, probes, states, arcs, and elapsed time;
- worklist size at termination, which must be zero;
- all deliberate over-approximations and their confirmation predicates; and
- a stable certificate/schema version.

`FomaTier::Full`, package trust, and readiness certification must require a valid completeness certificate in addition to an empty `uncovered` list.

## Backend behavior

### TunedSurfaceProbed

This remains the preferred shipping Foma proposer for grammar components its routes certify.

Ordinary rule chains may use existing lexc structure where it is complete. Structural composites and surface-probed interactions use exhaustive finite closure to the grammar-derived bound. The current fixed `MAX_EXTRA_RULES` and `STRUCT_MAX_EXTRA_RULES` behavior cannot be a success condition.

Surface probing that materializes root-plus-rule spellings remains an enumerated operation until the corresponding phonology is soundly lowered to a transducer. If enumeration cannot finish under the resource envelope, the backend returns Error and no artifact.

### TemplatedUnderlyingTokens

This is the strongest long-term route for composing ordinary morphotactics with regular phonological rewrite transducers. It may represent repeated nonempty affixation with loops or finite counter states instead of root-by-chain enumeration.

It currently has no complete composite pipeline for several circumfix, deletion, and process-morphology shapes. Those components must refuse rather than become `ConfirmOnly` with a known omission.

### PlanComposed

This remains useful for the subset it represents. Its self-looping ordinary prefix and suffix continuations demonstrate that many affixes are not inherently a finite-enumeration problem.

It is not a universal fallback. Its known inability to represent realizational and process morphology remains a backend-specific refusal.

### Restricted HC confirmation

Restricted confirmation remains a sound false-positive filter over a certified proposer superset. It contributes no recall and therefore cannot influence a completeness certificate except by naming the predicate that makes an over-approximation safe.

### Full Rust HC

Full HC remains the semantic oracle and the non-FST engine. It is not silently invoked to rescue an FST artifact. A user may deliberately select the full engine after an FST backend refusal.

## Backend compatibility reports and selection

Every backend emits a compatibility report for every grammar, whether or not that backend is ultimately selected. A report includes its correctness disposition, cost evidence, worst FST-health severity, all stable coded findings, failed capability predicates, affected constructs and shapes, measured or predicted values, applicable thresholds, and remedies. Reports from failed backends are retained alongside reports from viable backends.

The FST-health severities are ordered `Ideal`, `Info`, `Warning`, `Error`, and `Critical`:

- `Ideal` means no health finding;
- `Info` records useful evidence without recommending action;
- `Warning` permits a proven build but identifies cost or maintainability concerns;
- `Error` means a complete strategy exists but the current resource envelope did not complete it; and
- `Critical` means the backend cannot currently prove a recall-preserving representation or complete construction for the grammar shape.

Correctness remains binary and separate from that graded health axis. Correctness uncertainty is a refusal, never a Warning about cost.

The selector reads all backend reports. A normal generation candidate must be correctness-admitted and have a worst severity of Ideal, Info, or Warning; an Error or Critical report is retained but is not selectable for a normal build. The selector first prefers a candidate with no findings, then the candidate with the least severe and least numerous findings. Ties use the committed backend preference order so selection is reproducible.

The selector returns no plan when there is no normal generation candidate, one plan in the ordinary case, or the two highest-ranked plans when the committed selection policy requests a measured comparison between close candidates, such as overlapping projected-cost intervals. The two-plan result preserves primary/secondary order and cannot include an Error or Critical backend. If no plan is selected, PanGloss reports every backend's reasons and remedies. The explicit development capability override is a caller choice, not an automatic selector fallback.

## Outcome policy

### Ideal and Info

Ideal and Info both permit generation. Ideal is a clean report. Info records relevant measurements or characteristics without implying that the grammar should change.

### Warning

A Warning means PanGloss still has a complete, sound route and generation proceeds. Warnings describe observed or projected cost and identify the dominant contributors.

Depth alone never triggers a Warning. A depth metric may appear in a Warning when it materially contributes to projected entries, states, arcs, probes, memory, or time.

### Error

An Error means a semantically sound complete strategy exists, but the current operational envelope is insufficient. PanGloss emits no FST.

The normal path out of Error is a caller-requested retry with a changed named resource envelope and a clean construction state. A proven retry succeeds only with an empty worklist and a complete certificate. The separate development capability override may force an unproven artifact but never turns the Error into a successful proven build.

### Critical

A Critical means the selected backend has no known recall-preserving representation or cannot prove termination/completeness for the reachable grammar shape. Normal production compilation emits no FST. Only the explicit development capability override may force compilation, and it can produce only an indelibly unproven artifact with the degraded trust signal.

Examples include nonregular copying assigned to a pure FST backend, a zero-surface cycle without a finite counter, a known under-proposal route, or a compiler state that contradicts its certificate.

### Development and test override

Development and tests may use the explicit capability override to inspect partial construction state and exercise refused shapes. The override preserves every Error and Critical finding, cannot produce a proven package, and never disables apply-time containment. It is an explicit SDK/build-tool capability, not an automatic retry or hidden environment switch.

## Backend-specific advice

### Principle

Advice is derived from the predicate that prevented a backend from certifying the grammar. It is not generic stylistic advice and not a claim about the language.

For every failed backend, PanGloss reports:

- the backend and failed representation predicate;
- typed evidence identifying rules, templates, slots, strata, cycles, and witnesses;
- whether the obstacle is semantic or operational;
- one or more conditional remedies;
- prerequisites and contraindications for each remedy; and
- whether a remedy may change the linguistic analysis.

This allows a user to compare alternatives. One backend might become viable if rules are ordered, another if a structural operation moves into a slot, and another if deletion is expressed as regular phonology.

Rendered advice is grouped by backend. Within each backend it groups shared remedies, lists every affected shape, and labels each remedy's estimated effort as `easy`, `medium`, or `hard`. For ordering advice, PanGloss computes the smallest cataloged remedy set that addresses every blocking finding for a backend. It sorts those sets lexicographically by number of `hard`, then `medium`, then `easy` remedies; generation-admissible backends have zero blocking effort. Remaining ties use report severity, finding count, and the committed backend order. This advice ordering does not change selector correctness policy, and all findings for every backend remain available.

Every remedy is conditional and uses plain language: the change “would make this backend work for your language” only when its stated prerequisites hold. Every rendered remedy group also states: “Don't make any change that would make your language invalid!” PanGloss never claims that a suggested grammar edit preserves linguistic meaning unless a specific equivalence predicate proves it.

### Initial advice predicates

The first catalog covers at least these shapes:

| Detected shape | Why a backend may fail | Conditional advice |
|---|---|---|
| Unordered interacting rules | Application-state permutations cannot be represented or exhaustively closed within the current route | Order the rules if the language licenses an order; split them into strata; separate independent rule groups |
| Structural deletion or truncation | Literal affix loops cannot reconstruct material removed from the current word | Express a regular surface alternation as a phonological rule; make the operation slot-local if linguistically equivalent; constrain its reachable categories |
| Null/zero-surface cycle | An epsilon/tag loop can generate unbounded analyses without consuming input | Remove the cycle; make application finite; use a mandatory slot with a single null realization only when absence and null realization are semantically distinct |
| Structural rule reachable after many ordinary rules | Surface enumeration multiplies roots by reachable rule states | Let ordinary nonempty rules use a loop-capable backend; order or slot-localize rules where valid; constrain the structural rule's reachability |
| Repeated application | Current routes may track rule identity but not application count | Set the smallest correct `multipleApplication`; use finite counter states; avoid unbounded realizational reuse where possible |
| Broad phonological interaction | Surface probing must materialize many context-dependent spellings | Re-express supported regular phonology in the rewrite cascade; narrow environments; split independent strata or tables |
| Optional-slot branching | Independent apply/skip choices multiply reachable states | Combine mutually exclusive choices in one slot; use separate templates for different obligation patterns; make a slot mandatory only when linguistically correct |
| Process morphology/nonregular copy | The backend cannot express the transformation as a finite-state relation | Use a supported finite structural operation; reformulate a regular portion as phonology; select full HC |

Every “move this into a slot,” “order these rules,” or “rewrite this as phonology” remedy must state that it may change the grammar's linguistic meaning unless a specific equivalence predicate has been established.

## Structured advice catalog

The catalog is a versioned TOML resource embedded in PanGloss. Code produces stable shape keys and typed evidence; the catalog supplies user-facing explanations and remedies.

Diagnostic instances contain:

- a stable finding code;
- `shape_key`;
- `catalog_version`;
- severity and build action;
- backend, route, phase, and failed predicate;
- typed factors with units and provenance;
- measured or predicted values and the thresholds they are compared with;
- typed references to grammar entities and witnesses;
- completeness impact; and
- source and conformance-fixture references.

Catalog entries contain:

- stable key and explanation template;
- required evidence fields;
- default severity/action policy;
- ranked remedies;
- remedy effort (`easy`, `medium`, or `hard`) for this remedy-and-shape combination;
- remedy prerequisites, contraindications, and equivalence caveats;
- FieldWorks/PanGloss/Machine source references; and
- version history.

Example shape:

```toml
[[shape]]
key = "correctness.structural-closure-unsupported"
required_evidence = ["backend", "route", "rule_ids", "cycle_kind"]
default_severity = "critical"
default_action = "refuse"
explanation = "{backend} cannot prove a complete FST relation for {route}. The blocking {cycle_kind} cycle involves {rule_ids}."

[[shape.remedy]]
rank = 10
text = "Give the repeated rules a finite application bound."
effort = "easy"
requires = ["cycle_is_rule_repetition"]
linguistic_equivalence = "requires-review"
caveat = "A finite bound changes the grammar if repetition is genuinely unbounded."
```

Wording and remedies may evolve without changing the stable key. Adding optional evidence is backwards-compatible; changing the meaning of a key requires a new key.

## Deferred Mbugwe contract (future reference only; not current acceptance)

The following is retained to preserve the origin of a historical stress case. It is not a delivery
requirement for the current Indonesian/Amharic/Aweti slice, and no current report or artifact may
claim Mbugwe support from it.

The Mbugwe-derived five-rule late-anchor case is treated as plausible finite morphology, not as pathological depth.

PanGloss must first derive or conservatively prove the reachable finite application bound. It must then exhaust every legal structural state through closure, preserving later structural allomorphs and respecting `multipleApplication`. If the closure completes, the FST may be generated. If it exceeds resources, the result is Error with dominant cost factors. If closure cannot be established, the selected backend returns Critical.

The historical 59,647-entry run is evidence that exhaustive construction may be tractable. It is not evidence of completeness because probe totals and full oracle parity were not completed, and the produced FST missed `cheefu` through a separate later-allomorph gap.

## PanGloss-only conformance tests

Internal compiler behavior lives outside promotable `conformance-staging`. Use a path such as:

```text
rust/crates/pg-foma/tests/fixtures/pangloss/fst-completeness/
```

The suite includes:

1. a finite late structural-anchor fixture whose worklist exhausts and whose FST matches the full-HC oracle;
2. a later-allomorph structural/reduplication case proving every allomorph participates in classification and closure;
3. a repeated-application case proving counters, rather than rule-ID deduplication, bound closure;
4. a tiny injected resource budget that leaves pending work, returns Error, and writes no artifact;
5. an unsupported/unbounded zero-surface cycle that returns Critical before normal emission and remains unproven under the explicit development capability override;
6. a word with both shallow and deep analyses proving the proposer contains the entire HC result; and
7. negative controls for wrong rule order, root, anchor, and incomplete chains.

These fixtures are permanently PanGloss-specific and are never promoted to Machine.

## Delivery sequence

### 1. Fail closed immediately

- Detect a legal successor at every current fixed-depth return.
- Treat such a return as incomplete and emit no proven FST; the explicit development capability override may expose it only as unproven state.
- Make under-proposing `RepresentsWithKnownGap` refuse rather than `ConfirmOnly`.
- Make Critical refuse normal production compilation; preserve only the explicit unproven development capability override.
- Add the PanGloss-only red/green refusal fixtures.

This slice makes the current product honest while Mbugwe remains deferred.

### 2. Grammar-specific finite closure

- Extend morphotactic state with per-rule application counts.
- Derive finite bounds from reachable strata, templates, slots, features, and application caps.
- Replace fixed-depth recursion with a deterministic worklist that runs to exhaustion.
- Emit the completeness certificate and closure counters.
- Keep any historical five-rule fixture out of the current delivery gate; if future Mbugwe work
  reuses it, require the same closure and certificate evidence as any other grammar.

### 3. Cost diagnostics and catalog

- Project and observe entries, probes, states, arcs, memory, and time.
- Attribute cost to dominant typed factors rather than depth thresholds.
- Embed the versioned advice catalog and render backend-specific remedies.
- Require normal/proven Error retries to name a changed resource envelope and retain the prior finding.

### 4. Loop-capable lowering

- Reuse or generalize ordinary affix continuation loops.
- Add finite counter states where exact application limits must be enforced in the FST.
- Move supported regular phonology from surface enumeration into composed rewrite transducers.
- Keep nonregular or uncertified shapes as explicit refusals.

## Acceptance criteria

- No successful build can terminate with pending construction work.
- No arbitrary depth constant is a semantic success boundary.
- Five and ten applications produce no finding merely because of their numeric depth.
- Every successful `ConfirmOnly` route documents why its proposal is a superset.
- Every failed backend names its failed predicate and conditional remedies.
- Every backend report is retained, and selector ordering is deterministic.
- Warning always permits generation of a certified artifact.
- Error emits no proven artifact. A clean retry with a changed resource envelope may succeed normally; the separate development capability override may produce only an unproven artifact.
- Critical emits no proven artifact; an explicit development capability override can produce only an indelibly unproven artifact with degraded runtime trust.
- Apply-time execution containment cannot be overridden.
- PanGloss-only completeness fixtures cannot enter the Machine promotion workflow.

## Explicitly rejected alternatives

- A universal limit of five, ten, or any other application count.
- Emitting a partial FST with a Warning.
- Falling back to HC only for words where the FST returns no candidate.
- Treating a known under-proposal gap as `ConfirmOnly`.
- Letting a resource override accept partial output.
- Generic grammar advice that is not connected to a backend failure predicate.
