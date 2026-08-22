## Context

Tuned Surface eagerly enumerates root/rule composites.  Amharic's characterization and Aweti's
3,093,412-composite construction demonstrate that increasing that budget is not a semantic fix.
Templated Underlying Tokens instead emits compact underlying morphotactics and composes an ordered
phonological cascade.  The structural layer must therefore lower a closed set of root-independent
relations, not synthesize one surface entry per root.

The current implementation has three separate decisions: `emit.rs` decides what text/marker to
write, `capability.rs` decides what looks supported, and `templated_compile.rs` decides what to
compose.  The design below makes one classifier the source of truth for all three.  It also treats
the selected backend and the realized compiled backend as one conformance fact, not two loosely
related reports.

## Goals / Non-goals

**Goals:**

- Admit only the exact action topologies below and preserve every valid HermitCrab analysis by
  compiling their stated regular supersets; HermitCrab remains authoritative for LHS predicates.
- Make routing, marker placement, capability disposition, emission, and relation compilation use
  the same classifier result.
- Fail closed for every unclassified action, Pattern, table mapping, marker state, or skipped
  phonological rule.
- Prove the mechanism with synthetic witnesses, then prove the actual selected Templated path with
  exact Amharic and Aweti semantic equality.

**Non-goals:**

- A general compiler for arbitrary `OutputAction` graphs, arbitrary copying/reduplication, or
  language-specific rules.
- Eager root enumeration, budget increases, candidate filtering, or a production switch keyed by
  the names Amharic/Aweti.
- Mbugwe support or any Mbugwe-derived fixture/gate.

## Decisions

### 1. Closed classifier grammar: action topology with explicit LHS erasure

The topology classifier operates on one `AffixAllomorphDef`; final prefix/suffix placement also
receives the emission caller's current chain zone.  Let `P[i]` be the i-th non-empty LHS
`Pattern`, `I` be one `InsertSegments` action whose text is representable in the active table, and
`C(i)` be `Copy(PartRef::Input(i))`.  Every RHS input reference must resolve to exactly one real
`P[i]`, and all output literals/classes must translate to the active table.  The classifier
validates those references and the complete RHS action topology; it does **not** compile or claim
to enforce the internal `PatternNode` grammar for the admitted recipes.

That erasure is intentional and recall-preserving.  Whole-root wrapping depends only on copying
all parts in order.  Interior insertion depends only on preserving the input string and inserting
authored literals in order.  Terminal modification can propose the allowed output-class segments
at any segment position, and initial replacement can propose the authored literal after consuming
the first eligible segment.  These relations are supersets of the authored LHS predicates, so
natural classes, contexts, anchors, alpha variables, and quantifiers remain HermitCrab confirmation
conditions rather than being misread as wildcards with exact semantics.  A backend report marks
these recipes `ConfirmOnly`, not `Admit`.  Malformed/empty referenced parts, invalid references,
untranslatable output material, or an RHS topology outside the closed list return `Unsupported`.

For structurally valid referenced parts, the RHS must be exactly one of these forms (and no other
form):

1. **`AwetiWholeRootWrapper`** (`n >= 1`):
   `I* C(0) C(1) ... C(n-1) I*`.
   Every LHS part is copied exactly once in increasing order.  Literal insertions may occur only
   before the first copy or after the last copy; there is no literal between copies.  This is the
   Aweti whole-root wrapper shape.  Because it preserves the whole root, it is emitted as direct
   marker-free prefix/suffix text and traverses only the relation's identity branch; the internal
   node shapes of `P[i]` need not be lowered.  It is not an interior rewrite.  It is distinct from
   a dropped-tail rule: a one-sided drop is admitted only by the separate bounded recipe below,
   never by silently treating omitted copies as a wrapper.
2. **`AmharicInteriorInsertion`** (`n >= 2`):
   `C(0) J(0) C(1) J(1) ... J(n-2) C(n-1)`, where each `J(i)` is a possibly empty ordered run
   of `InsertSegments` actions and at least one `J(i)` is non-empty.  Every LHS part is copied
   exactly once in increasing order.  Literals are strictly between copies;
   no part is omitted, repeated, reordered, or modified.  The compiled relation preserves every
   segment and proposes the literal runs at segment boundaries in authored order; it need not prove
   the LHS partition boundaries, so it may overpropose positions but cannot omit the authored one.
   The `ä` insertion in Amharic's four-part `-ä-1`, `-ä-2`, and the two-insertion `-ä-...` shapes
   are this family.
3. **`AmharicTerminalModify`** (`n >= 2`):
   `C(0) C(1) ... C(n-2) Modify(Input(n-1), Q)`.
   The final LHS part is referenced exactly once by a terminal `ModifyFromInput`; `P[n-1]` must be
   a one-node pattern whose proven cardinality is exactly one segment, and `Q` is a variable-free
   `SimpleContext` whose possible output segments are finite and translatable in the active table.
   The proposal relation may replace one eligible segment at any segment position with one of
   those outputs; HermitCrab enforces the authored terminal part and feature-variable constraints.
   There are no inserted literals, context insertions, omitted/repeated copies, or non-terminal
   modifications.  This covers Amharic's terminal `ModifyFromInput` subrule and nothing broader.
4. **`AmharicInitialVowelReplacement`** (`n == 2`):
   `I+ C(1)`, where `P[0]` is exactly one fixed `CharDef` segment representing the initial vowel
   and `P[1]` is the remainder span.  `P[0]` is consumed and never copied; the non-empty literal
   run replaces it before the copied remainder.  No interior literal, second copy, `Modify`, or
   `InsertContext` is admitted.  Amharic's `ላ`/`ካ`/`ባ` proclitic rules are this shape.
5. **`AdjacentTerminalDrop`** (the existing bounded relation): either `n == 2`, `P[1]` is one
   lowerable terminal atom, and the RHS is `C(0) I*` (terminal drop), or `P[0]` is one lowerable
   initial atom and the RHS is exactly `C(1)` (initial drop).  Neither form is a general
   truncation or arbitrary set difference; the initial-vowel replacement above is a separate
   recipe because it has a non-empty literal run before `C(1)`.

`I*` means zero or more ordered `InsertSegments` actions and `I+` means at least one contiguous,
non-empty insertion run.  Each action is re-encoded from its owning table into the active pipeline
table before classification succeeds.  Ordinary prefix, suffix, or zero-morph text that has no
structural topology remains ordinary templated literal emission and receives no structural marker.
Precisely, `OrdinaryLiteral` accepts only an empty RHS (null) or a non-empty RHS in which every
action is a translatable `InsertSegments`; the emission caller supplies its current chain zone.
A RHS containing any `Copy`, `ModifyFromInput`, or `InsertContext` is never ordinary: it must
match one of the five structural forms above or return `Unsupported`.
The classifier rejects `InsertContext`, any `Modify` outside the terminal form,
repeated/missing/reordered `Copy` references, copied `Head`/`NonHead` references, multiple
independent LHS partitions, reduplication, unbounded copying, an empty or untranslatable
modification output set, unknown output actions, and any shape not listed above.

### 2. One classifier owns routing, marker placement, capability, and emission

`MorphologyRewriteClassifier::classify(g, allomorph, active_table)` returns either:

- `OrdinaryLiteral` (no marker, ordinary zone routing); or
- `DirectWholeRootWrapper { prefix_variants, suffix_variants }` (no marker; its two halves already
  identify their zones and form the full Cartesian product); or
- `MarkedStructural { shape_id, recipe, zone_requirement, marker }`, where the requirement is an
  intrinsic edge (`Prefix`/`Suffix`) or `Caller` for a root-internal recipe; or
- `Unsupported { shape_id, reason, source_rule, allomorph }`.

The `DirectWholeRootWrapper` result carries two independently deduplicated translated variant sets;
emission offers every `prefix_variants × suffix_variants` pair and never zips or selects one pair.
The `MarkedStructural` result contains the closed action-topology ID, ordered literal/output-class
material, validated input references, source/active-table translation, an intrinsic-or-caller zone
requirement, and exactly one allomorph-owned marker; it does
not carry a falsely exact LHS matcher.  `emit.rs` calls the classifier to decide whether to write ordinary
text, direct wrapper text, a marker alternative, or an uncovered diagnostic, and to place
prefix/suffix halves in the correct zone.  Intrinsic edge recipes must agree with the caller's zone;
interior insertion and terminal modification use the caller's zone.  This is per-allomorph and
per-zone; it never uses
`rule_role` or the first allomorph as a proxy for later alternatives.  `capability.rs` consumes the
same result and refuses the grammar if any relevant allomorph is `Unsupported`.
`templated_compile.rs` compiles exactly the recall-preserving marked proposal recipes emitted by
those same results and leaves direct wrappers on identity; it must not discover recipes
independently.  Capability records every direct or marked structural recipe as `ConfirmOnly` and
refuses the grammar if any relevant allomorph is `Unsupported`.

The coarse strategy registry participates in the same decision.  Templated
`ProcessMorphology` and the affected multi-part circumfix row become predicate-backed known gaps,
not unconditional support: their evidence names this classifier, and `capability.rs` resolves them
to `ConfirmOnly` only when every relevant allomorph returns a listed recipe.  Any unsupported
process/circumfix result remains `Refuse`.  This change therefore edits `strategy_coverage.rs` as
well as `capability.rs`; leaving the unconditional `CannotRepresent` row in place would make the
new classifier unreachable.

The classifier also owns the Templated structural route decision for this slice.  The sibling
production selector may choose Templated only from a clean capability result and must record the
actual realized route.  A successful compile of a different backend, a stale candidate profile,
or a partial Templated candidate cannot satisfy this change.

### 3. Total marker-union contract

For the unioned morphology relation `M = Identity ∪ R(marker_1) ∪ ... ∪ R(marker_k)` (the
`MarkedStructural` recipes; a `DirectWholeRootWrapper` is emitted outside `M`), the following are
invariants, checked in synthetic tests and in the compile profile:

- **Marker-free identity only:** an input with no technical marker can traverse only the identity
  branch.  No structural recipe may match or rewrite an unmarked form.
- **Exactly one known marker:** a marked structural branch has exactly one marker, and it is one
  of the markers allocated by this classifier for the selected allomorph.  A foreign marker, an
  unknown marker, or multiple markers is a typed rejection, not an identity fallback.  The direct
  Aweti wrapper is the only admitted marker-free structural shape, and it never enters `M`.
- **Consume, do not leak:** the matching relation consumes its one marker exactly once.  Markers
  never appear in a RHS output, in boundary cleanup, in a proposer tape, or in structured analysis
  identity.  The final technical-marker count must be zero.
- **Union isolation:** `R(marker_i)` cannot fire for `marker_j`; each marker binding is unique to
  one `(allomorph, zone)` route.  A convenience lookup by allomorph is valid only when exactly one
  zone was emitted.  Missing marker subtrees, duplicate marker allocation, and
  marker-count disagreement between emission and compilation are hard failures.

The relation is composed after underlying lexc and before phonology.  The existing ordered
right-to-left and simultaneous replacement cascade then sees the structurally realized underlying
form.  Boundary cleanup follows phonology.  A leftover marker or a relation that cannot prove the
contract returns an incomplete/error outcome and cannot produce a trusted artifact.

### 4. Evidence and certification

Synthetic fixtures use invented, construct-named grammars.  Each admitted recipe has a positive
oracle witness, a minimally changed negative witness for every excluded topology, a cross-marker
isolation witness, a marker-free identity witness, a foreign/multiple-marker rejection witness,
and a final zero-marker assertion.  Terminal modification additionally has a one-segment positive
witness and multi-segment and quantified-span negative witnesses, proving that one replacement
cannot stand in for HermitCrab's modification of every segment in a captured span.  A wrapper
witness with at least two prefix variants and two suffix variants proves all four pairs are emitted,
so a zip or first-variant implementation fails.  Mechanism evidence is deterministic: classifier counts,
marker allocations/consumptions, relation fires, unsupported/uncovered items, skipped rules, and
missing subtrees are reported as counters, not inferred from timing.

The actual-language gates run the selector and the realized compiler in one invocation and assert:

- `selected_backend == realized_backend == TemplatedUnderlyingTokens`, with the realized network
  fingerprint used for every reported word;
- `skipped_rules == 0`, `uncovered == 0`, structural gaps/missing marker subtrees == 0, and
  final technical markers == 0;
- Amharic: exactly 200 declared oracle-bearing cases, each with complete structured analysis-set
  equality (not merely surface reachability or containment), i.e. `200/200`;
- Aweti: exactly 106 declared oracle-bearing, alphabet-encodable cases, each with complete
  structured analysis-set equality and no residual miss, i.e. `106/106`.

The equality is the repository's canonical structured analysis identity (stable morpheme sequence,
root position, and POS/category); output order, duplicate discovery, traces, gloss formatting,
timing, and serialization are not substituted for semantic equality. The gates do not include
Mbugwe.

### 4.1 Historical implementation constraints carried forward

The Indonesian/Amharic/Aweti history adds four non-optional guards to this design. The old
enumeration bridges remain bounded evidence only; the selected route must be the realized,
fingerprinted route. Underlying templated composition is the Aweti scale successor, but its six
remaining misses and the cascade's process-morphology boundary remain refusals until the exact
semantic gates pass. Historical `fsm_intersect` failures from literal-zero tag symbols also require
source-level codec coverage across every consumer, not an `apply_up`-only check. Finally, budget
latches, elapsed time, or a fixed depth may explain cost but cannot establish recall or certificate;
recipe/marker/closure counters and exact sets are the evidence contract.

## Risks / Trade-offs

- **The grammar boundary may be too broad.** Keep the action classifier closed, state the
  recall-superset argument for every deliberate LHS erasure, and add one positive/negative witness
  per new admitted form.
- **Marker relations may under-propose or cross-fire.** The total marker contract, fire counters,
  and foreign/multiple-marker negatives make either defect observable before corpus promotion.
- **Cross-table literals may be misencoded.** Classification must fail unless every literal is
  translated from its owning table into the active table; add a deliberately misaligned two-table
  witness.
- **Template slots may route a later circumfix incorrectly.** Route every allomorph from its own
  classifier result and test a non-first circumfix alternative; first-allomorph-only role checks
  are forbidden.
- **Composition may grow.** Record deterministic recipe/state/arc counters and keep resource
  containment, but treat cost separately from semantic admission.
- **Corpus recall may hide an unexercised construct.** Synthetic witnesses and zero unsupported,
  skipped, gap, and marker counters remain mandatory.

## Migration Plan

1. Write failing classifier and marker-contract tests first, including all five admitted topology
   families and default-deny negatives.  Assert that the Aweti wrapper is direct and marker-free;
   keep the existing narrow recipe behavior unchanged until
   its classifier result is proven equivalent.
2. Implement shared classification and make emission, capability, and relation compilation consume
   it.  Add origin-table translation and per-allomorph zone routing at this boundary.
3. Implement the Aweti wrapper and bounded drop, then Amharic interior insertion, terminal modify,
   and initial-vowel replacement one recipe at a time.  Each step must leave unrelated shapes
   refused rather than falling through to literal text.
4. Add the unioned relation and total marker assertions, then compose it before the existing
   phonology cascade.  Any skipped rule, uncovered item, missing subtree, or marker leakage keeps
   the candidate unavailable.
5. Run the synthetic gates and the one-invocation selected-versus-realized backend gate.  Only a
   clean Templated result may proceed to the exact 200/200 Amharic and 106/106 Aweti semantic gates.
6. Allow the sibling routing change to use Templated only after those gates pass.  Revert is a
   linear change that restores the previous explicit refusals; no fallback silently certifies a
   partial artifact.

## Open Questions

None.  If any actual allomorph fails the exact topology or deterministic table/feature mapping,
the required outcome is a named unsupported finding and a blocked Templated certification, not a
new implicit recipe.
