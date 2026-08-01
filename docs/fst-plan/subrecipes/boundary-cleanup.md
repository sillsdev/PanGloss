# BoundaryCleanup subrecipe dossier

## Scope

BoundaryCleanup owns the terminal consumption of boundary and marker symbols after every mechanism
that needs those symbols has run. It removes the declared boundary symbols in the active character
table, preserves analysis identity/multiplicity, and is idempotent when applied twice.

**Non-scope:** deciding template legality, applying phonological rules that consume boundaries,
rerouting null-shaped affix chains before compilation, or coercing surface symbols into another
character-definition table. Cleanup is a terminal adapter/mechanism, not a general repair for an
incompatible graph.

## Languages and families in mind

- **Anchor 1 — Sena. Family: Bantu. Construct:** a boundary-only/null allomorph such as the documented `^0+` shape
  exercises cleanup of marker and separator boundary characters while preserving recall. The
  construct role is terminal boundary deletion plus protection from epsilon-loop collapse.
- **Anchor 2 — Caquinte. Family: Arawakan. Construct:** discontinuous future and boundary-crossing epenthesis/
  metathesis exercise the requirement that cleanup wait until the consumer has seen the boundary.
  The role is ordering and symbol-state preservation, not Caquinte-specific code.
- **Formal/engineering anchor — Foma morphology tutorial:** an intermediate `^` boundary is
  consumed by phonological rules and a final cleanup relation removes it. This is an architecture
  analogy, not a language-family claim.

The Sena behavior is repository-witnessed through a synthetic analogue, but the private corpus is
not checked in. The Caquinte ordering is high-confidence in the local harvest; its exact primary
grammar examples remain a source uncertainty.

## Primary sources

- [Foma morphology tutorial](https://fomafst.github.io/morphtut.html) for intermediate boundary
  symbols followed by terminal cleanup.
- [Linguistic construct harvest](../linguistic-recipe-harvest.md) for Sena and Caquinte and the
  citation ledger.
- [Boundary marker/epsilon gate](../../../rust/crates/pg-foma/tests/boundary_marker_epsilon_collapse_gate.rs)
  for the checked-in synthetic evidence: deleting any boundary family would lose recall, while
  null-shaped chains can otherwise create free epsilon loops.
- [Boundary cleanup net source](../../../rust/crates/pg-foma/src/recipe_mechanism.rs) is the typed
  artifact location for `BoundaryCleanupSpec`; it is not itself linguistic evidence.

## Grammar facts

Boundary symbols have a character-definition table and may have multiple representations. The
cleanup consumer must match the active table and symbol space exactly. A marker-bearing entry must
remain reachable; deleting the marker family at the extraction stage caused a measured multiplicity
loss in the repository gate. Conversely, allowing an all-boundary affix to compile onto a looping
continuation can create a free epsilon insertion point.

**Invariants:** all boundary-consuming consumers run before cleanup; table/symbol-space identity is
preserved; cleanup is terminal and idempotent; analysis/root identity and multiplicity survive; and
resource rerouting for null-shaped chains is distinct from boundary deletion itself.

## Formal model and regularity

For a finite boundary-symbol set `b`, cleanup is a regular homomorphism/rewrite relation that maps
each declared boundary token to epsilon and leaves other tokens unchanged. It is safe only as the
last relation in the relevant pipeline. The graph contract therefore distinguishes a surface table
from char-def-token space and rejects a cleanup-before-consumer edge.

**Correctness obligations:** applying cleanup once and twice yields the same normalized multiset;
every boundary-bearing valid analysis remains reachable; no non-boundary symbol is deleted; and a
candidate is certified only after all preceding consumers have run.

**Failure modes:** early cleanup erases a trigger, table mismatch, deleting a multi-representation
marker family and losing recall, all-boundary epsilon-loop collapse, non-idempotent cleanup, and
reporting a proposal cap as an exact negative.

## Chosen architecture

1. Keep boundary tokens through morphology, structural actions, and boundary-consuming phonology.
2. Validate `BoundaryCleanup` as terminal with an exact table/symbol-space contract.
3. Compile one cleanup relation and allow repeated application only as an idempotence test.
4. Reroute null-shaped affix chains before compilation as a separate resource-safety step.

## Rejected architectures

- Cleanup immediately after morphology: Caquinte-shaped boundary consumers lose their trigger.
- Excluding every marker family from cleanup: the repository gate recorded a recall/multiplicity
  regression for a marker-bearing entry.
- Treating cleanup as a generic surface-string replace: it can delete tokens from the wrong table.
- Compiling all-boundary affixes onto self-looping continuations and relying on later deletion: it
  creates free epsilon loops and proposal explosion.
- Applying a cleanup adapter to a char-def-token producer without a contract: surface agreement does
  not make symbol spaces compatible.

## Interfaces and interactions

Morphotactics and StructuralAllomorph may produce boundaries; OrderedPhonology or an explicit
consumer may require them. The edge into cleanup must preserve the active table, root/analysis
identity, exact multiset, and terminal disposition. Cleanup emits a surface relation with boundary
state `Removed`; no downstream mechanism may require `Present` after that edge.

## Complexity and resource bounds

**Big-O variables:** `n` = token length, `b` = boundary token kinds, `a` = active alphabet size,
`c` = number of cleanup consumer contexts, and `P` = proposal count.

**Time:** token cleanup is `O(n)` per candidate with table lookup; relation construction is
`O(b + a)` for the declared alphabet representation. Null-shaped-chain rerouting may add
`O(c · P)` proposal work and must be budgeted separately.

**Space:** the cleanup relation is `O(b + a)` in a compact table representation and normalized
candidate storage is `O(n · P)`. The repository's synthetic gate measured a small fixed grammar at
`<= 20` proposals after the reroute and documents a much larger broken-path count; the threshold is
fixture evidence, not a universal bound.

## Task 6 evidence status

- **Source ModelLocation/model-ID evidence:** the repository mapping exposes `ModelLocation::AffixAllomorph`,
  `MorphemeCoOccurrence`, and `Stratum`, while table identity is carried by the mechanism's wire
  symbol-space ID; see [`capability.rs`](../../../rust/crates/pg-foma/src/capability.rs) and
  [`recipe_mechanism.rs`](../../../rust/crates/pg-foma/src/recipe_mechanism.rs). A concrete source
  model-ID witness for the named grammar anchors is `Not measured — blocks implementation claim`.
  `BoundaryCleanupSpec.table` is a `TableId`; there is no boundary-specific `ModelLocation::Table`
  variant to fabricate.
- **Resource caps:** boundary-family, proposal, epsilon-loop, and reroute caps are required; a numeric
  Task 6 cap record is `Not measured — blocks implementation claim`.
- **Measured stage counters:** no per-consumer/cleanup normalization counter has been recorded:
  `Not measured — blocks implementation claim`.

## Conformance fixtures

### Exercise 1 — Sena-shaped boundary-only allomorph

Positive: the ordinary prefix and the all-boundary `^0+` prefix both remain reachable; expected
analysis multiplicity for the marker-bearing surface remains two where the oracle has two. Applying
cleanup twice must produce the identical normalized multiset. Negative mutation: exclude the
multi-representation marker from cleanup; expected multiplicity drops and the gate must fail.

### Exercise 2 — Caquinte-shaped boundary consumer

Positive: a boundary-crossing epenthesis/metathesis consumer runs while the marker is present, then
cleanup removes it; expected multiset is the one complete analysis with root, future morpheme, and
surface identity preserved. Mutation: move cleanup before the consumer; expected multiset is empty
or differs from the oracle and graph validation must reject the edge. This is a proposed semantic
exercise; the checked-in boundary gate currently measures recall and epsilon precision separately.

**Positive cases:** the ordinary plus boundary-only prefixes remain reachable, and a boundary consumer
runs before terminal cleanup.
**Negative cases:** excluding the multi-representation marker, moving cleanup before its consumer,
and compiling a looping all-boundary affix must be rejected or fail oracle equality.
**Identity/multiplicity cases:** for the synthetic `s` row, the ordinary and marker-bearing analyses
remain two distinct identities with multiplicity one each; cleanup is idempotent.
**Mutations:** remove one marker representation, move cleanup earlier, apply cleanup twice without an
idempotence check, or use the wrong table/symbol space.
**Exact normalized expected multisets/tuples:**
`s = {(surface=s, prefix=ordinary, source_model_id=proposed:ordinary-prefix, multiplicity=1),
(surface=s, prefix=boundary-only, source_model_id=proposed:boundary-only-prefix, multiplicity=1)}` and
`ps = {(surface=ps, prefix=ordinary, source_model_id=proposed:boundary-consumer, multiplicity=1)}` after cleanup; the early-cleanup and
wrong-table mutations are `{}` or an oracle mismatch, not exact negatives. These are canonical
expected records; the existing gate's `<= 20` proposal result is a separate measured fixture fact.

## Implementation status

The repository has a boundary cleanup gate and a typed `BoundaryCleanupSpec`; its graph validation
rejects nonterminal cleanup and symbol-space mismatch in the existing mechanism tests. The synthetic
gate explicitly documents that its small fixture pins recall but not the full precision fix. Current
status: repository evidence exists, unified executable-recipe routing is incomplete.

## Known gaps and split triggers

The Caquinte primary examples and the private Sena corpus are not fully reverified in this task. A
follow-on must add an independent boundary-consuming conformance row and measure normalization,
identity, multiplicity, and proposal counts. A split/add is required if cleanup must do semantic
repair, consume non-boundary structure, or coordinate with a runtime operation rather than perform
terminal symbol deletion.

The split/adds conditions below are hypothetical future triggers, not dated evidence decisions.

**Trigger matrix:** `fits` for terminal finite boundary deletion with table identity;
`refines` for additional boundary kinds, idempotence witnesses, or measured reroute budgets;
`splits/adds` for semantic repair or a nonterminal/runtime consumer that cannot use the current
contract.

## Research log

| Date | Evidence and direct link | Consequence |
|---|---|---|
| 2026-08-01 | [Foma morphology tutorial](https://fomafst.github.io/morphtut.html) | Intermediate boundary symbols must survive until their consumers; cleanup is last. |
| 2026-08-01 | [boundary gate](../../../rust/crates/pg-foma/tests/boundary_marker_epsilon_collapse_gate.rs) | Marker exclusion loses recall; all-boundary chains can create epsilon-loop proposal growth. |

## Evidence decisions

| Date | Decision | Evidence | Architectural consequence / trigger |
|---|---|---|---|
| 2026-08-01 | fits | Boundary deletion is a finite terminal relation and is independently described in Foma's morphology architecture. | Keep BoundaryCleanup terminal and table-specific. |
| 2026-08-01 | refines | The repository separates recall-preserving deletion from precompile null-chain rerouting. | Keep cleanup and resource rerouting as distinct mechanisms with separate evidence. |
