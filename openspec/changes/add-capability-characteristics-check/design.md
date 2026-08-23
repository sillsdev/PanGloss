## Context

The keystone (ADR 0001): a first-class compile gate that projects a grammar + stem data into a
**characteristics profile**, composes a **capability envelope** from per-stage and interaction
predicates **bottom-up over the reified compilation plan** (`reify-compilation-plans`), matches them,
and **hard-fails** any not-proven-faithful configuration with a typed diagnostic. Co-designed with
the reified `Plan`, which lands first; this change's envelope composes over that DAG.

Frozen model source of truth: `rust/crates/pg-grammar/src/model.rs`. The characterizer is exhaustive
over it with no catch-all — the discipline that would have caught the `Compounding` silent-recall
hole.

## Decisions

### D1. `CharacteristicsProfile` projection (artifact a)

The characterizer walks the `Grammar` (`model.rs:1088`) and every lexical/stem entry and emits a
`CharacteristicsProfile`: a set of observed characteristics, each tagged with the model location(s)
that induced it and a default capability disposition ∈ `{Proven, FailClosed, ConfirmOnly,
ConfigPredicate}`. Exhaustiveness is structural: the characterizer `match`es every frozen-model enum
with **no `_ =>` arm**, so adding a `model.rs` variant breaks the build until it is characterized. A
profile that silently omitted a construct would let the gate pass vacuously — exactly how
`Compounding` slipped through.

Initial dispositions (first act of the keystone), by model location:

| Model location | Variant | Characteristic | Initial disposition |
|---|---|---|---|
| `MorphRuleDef` (542) | `AffixProcess` | affixation (per role) | Proven* |
| | `Compounding` (702/718) | compounding | **FailClosed** (`cover-compounding`) |
| | `Realizational` (601) | realizational | ConfirmOnly → Proven (`cover-realizational-…`) |
| `MorphRuleOrder` (1057) | `Linear` | ordered application | Proven |
| | `Unordered` | order-independent application | **FailClosed** (`cover-unordered-morph-rules`; × chain-depth) |
| `MprGroupOutput` (831) | `Append` | additive MPR group | ConfirmOnly (monotone; safe-filter candidate) |
| | `Overwrite` | history-dependent MPR group | **ConfirmOnly / FailClosed** (naive filter = false-negative trap; `cover-mpr-groups`) |
| `RewriteMode` (385) | `Iterative` | iterative rewrite | Proven |
| | `Simultaneous` | simultaneous rewrite | **ConfigPredicate** (subrule-overlap; D3) |
| `Dir` (391) | `LeftToRight` | L→R | Proven |
| | `RightToLeft` | R→L | **FailClosed** (`compile-right-to-left-rewrites`) |
| `PhonRuleDef` (403) | `Metathesis` (466) | metathesis | **FailClosed** (`compile-fst-metathesis`) |
| `RewriteSubruleDef` (422) | `lhs.len()==0` | epenthesis | ConfigPredicate |
| | `required_pos/_mpr/excluded_mpr` | subrule gating | Proven → drives the `Gate` node |
| `OutputAction` (686) | (each) | output-action kind | ConfigPredicate (`cover-circumfix-null-…`) |
| `ReduplicationHint` (679) | (each) | reduplication | **FailClosed** (chain-depth-bounded) |
| `CoOccurrenceAdjacency` (508) | (each) | co-occurrence constraint | ConfirmOnly |

Cardinality/stem characteristics (feed cost, not the correctness gate): entry/morpheme/stratum
counts, max reachable derivation chain depth (ADR 0003 dimension — the Aweti 24-level chain),
alphabet/series-merger multiplicity, presence of zero-width/truncation mrules.

### D2. The `CapabilityPredicate` trait + `PredicateVerdict` (artifact b)

A predicate is an **oracle-verified proof obligation** that is **conservative**: it may over-refuse
but must never under-refuse (ADR 0001).

```rust
pub trait CapabilityPredicate {
    fn id(&self) -> PredicateId;                    // e.g. "simultaneous.subrule-overlap"
    fn discharges(&self) -> &[CharacteristicKind];  // which characteristic(s) it claims
    fn evaluate(&self, profile: &CharacteristicsProfile, plan_node: &PlanNode) -> PredicateVerdict;
    fn provenance(&self) -> EvidenceProvenance;     // Behavioral | Structural (ADR 0001)
}

pub enum PredicateVerdict {
    Admit,                        // proven faithful; admission-filtering allowed
    ConfirmOnly,                  // propose the superset; no no-false-negative proof (ADR 0001)
    Refuse(CapabilityDiagnostic), // hard compile-time fail (overridable per ADR 0005)
}
```

`ConfirmOnly` is a first-class verdict, not a failure — the confirm-only-by-default landing spot
(ADR 0001). The construct compiles, proposes broadly, confirm prunes; it becomes `Admit` only when a
predicate proves the admission filter has no false negatives. The characterizer guarantees every
`FailClosed`/`ConfigPredicate` characteristic has ≥1 predicate that `discharges` it, else the build
breaks (no silent vacuous pass) — the coverage counterpart of the no-catch-all rule.

### D3. Worked example: `simultaneous.subrule-overlap` (ADR 0001's cited example)

**Claim:** a `RewriteRuleDef` with `mode == Simultaneous` (`model.rs:387`) is faithfully compilable
*unless two of its subrules' environments can match at the same input position*. Simultaneous
application applies all subrules to the same input snapshot in one pass; overlapping matches interact
in a way sequential composition does not reproduce, risking an omitted analysis on the un-application
side.

```
evaluate(rule):
  if rule.mode != Simultaneous: return Admit                 # Iterative is Proven
  for each unordered pair (s_i, s_j) of rule.subrules:
      if mpr_gates_disjoint(s_i, s_j): continue              # cheap orthogonality early-out
      if intersect(span(s_i), span(s_j)) non-empty at a shared focus position:
          return Refuse(SimultaneousSubruleOverlap{ rule, s_i, s_j, witness })
  return Admit
```

- `span(s) = left_env · lhs_focus · right_env`, lowered to an `Fsm` via `lower.rs` (Stage 1B) — one
  pattern semantics, no re-derivation.
- `mpr_gates_disjoint` uses `required_mpr`/`excluded_mpr` (`model.rs:426-427`): if no entry can
  satisfy both subrules' gates they can never co-fire — a cheap *proof of orthogonality* for the
  common well-authored case (parallel-independence, D4), retiring the pair without the automaton
  intersection.
- **Conservative direction:** any approximation rounds toward "overlap possible" (Refuse); if either
  subrule is `self_opaquing` (`model.rs:452` — the fixpoint-reapply, exactly the interacting case) do
  not attempt `Admit`.
- **Provenance:** `Structural` on the controllable composition path (we intersect real lowered
  automata); `Behavioral` on the black-box foma path (automata unobservable → oracle witnesses).
- **Oracle obligation:** pinned by conformance witnesses — a synthetic grammar with two
  non-overlapping simultaneous subrules (must `Admit`, must match the oracle) and one with
  overlapping subrules (must `Refuse`; its force-compiled override must exhibit the recall divergence
  the refusal predicted).

This is the template for every predicate: cheap disjointness/orthogonality early-out first, then a
conservative automaton-level test rounding toward refusal, then a `ConfirmOnly` landing if a safe
*filter* can't be proven, then a `Refuse` (overridable) if even confirm-only recall can't be
guaranteed.

### D4. Envelope composition is bottom-up over the plan DAG; orthogonality = parallel-independence

The capability envelope is composed **bottom-up over the reified `Plan`** (`reify-compilation-plans`
D1). A node's verdict is the meet of its children's verdicts and its own node-level predicate:
`Refuse` dominates; any `ConfirmOnly` among admits demotes the subtree to confirm-only. Interactions
do **not** compose for free (ADR 0001): at a `Union`/`Compose` node whose children carry
independently-safe constructs, the node needs a proven **interaction predicate**, else it fails
closed.

The general interaction predicate is **parallel-independence** (from graph-transformation theory's
Local Church-Rosser theorem): two branches compose safely by `Union` iff neither's rewrite
reads/deletes what the other touches. The proof obligation is structured as **critical-pair
enumeration** — for a candidate `Union(A, B)`, enumerate the construct pairs `(a ∈ A, b ∈ B)` that
could *minimally overlap* (share input strings, adjacent contexts, or write-sites) and show
non-interference for each. The phonology-specific cheap sufficient condition is **feeding/bleeding
disjointness**: each rule's output alphabet/domain never overlaps the other's trigger context. This
converts "proving a set of constructs orthogonal retires whole swaths of the combination space"
(ADR 0001) from a slogan into a finite, bounded (pairwise over constructs actually present) checklist.

### D5. Default-deny characterizer, override + trust signal, conformance-coverage CI gate

- **Default-deny characterizer** (task 2): exhaustive over `model.rs`, no catch-all; first act marks
  `Compounding`, `Unordered`, `MprGroup`, and every unproven config fail-closed.
- **Capability override** (ADR 0005, task 4): a hidden developer-build-only override force-compiles
  a refused grammar for grounding, writing an indelible unproven/recall-unsafe stamp into the
  **pack manifest** override record (who/when/why/which configs) and broadcasting a pack-level
  `unproven` load signal + per-analysis degraded-trust flag. It may omit valid parses, is rejected
  in production/publication/certification, and never passes conformance; only genuine proof + clean
  recompile clears it. It does not remove resource limits and is distinct from developer-only
  stress execution that removes internal size/work caps while preserving external containment.
- **Conformance-coverage CI gate** (task 5): CI cross-checks the **capability registry** (the
  source-controlled supported/unsupported contract) against `machine/conformance/` coverage
  (`constructs.txt` / per-word `exercises:` / `rules.csv`); marking anything supported without a
  covering, passing synthetic fixture breaks the build. (Ground truth is the committed `words.yaml`;
  `expected.tsv` is materialized at runtime by `FixtureMaterializer`.)

## Dependencies

Co-designed with `reify-compilation-plans` (the `Plan`/`PlanNode` type lands first; this envelope
composes over it). `lower-fst-pattern-environments` (Stage 1B) supplies the `Fsm` lowering the
overlap/orthogonality predicates use. `define-grammar-coverage-contract` (demoted) feeds evidence
into the gate. Consumes the ADR 0005 pack-manifest override field and the ADR 0003 chain-depth
dimension. Contract owner of the characteristics gate + capability registry; construct changes
contribute registered predicates and conformance fixtures.

## Novelty / risk (flagged, per research)

Applying critical-pair / parallel-independence machinery to **whole compiled FST branches** (not
individual rewrite-rule applications) is a scale jump the graph-transformation theory was not built
for; mapping "construct-level interaction predicate" onto "critical pair between two compiled
sub-FSTs" is real translation work, not a drop-in citation. Two 2026 papers (*Parallel Rule
Application with Doubling Avoidance*; *Comonadic Morphophonology*) were access-limited during
research and should be read in full before leaning on them for a design decision.
