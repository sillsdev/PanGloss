# StructuralAllomorph subrecipe dossier

> The section immediately following is new: the dossier proposes an architecture for subject matter
> the shipped compiler already implements, under a different name, and does not say so.

## As shipped — what the mainline actually does

**Structural allomorphy ships as enumeration, not as a typed action graph.** For a rule whose surface
effect cannot be expressed as a two-entry lexc encoding — interior insertion interleaved with copied
root material, or an adjacency that coalesces into a differently-spelled glyph — the mainline seeds a
real word from the root allomorph's own feature-bearing shape, applies the **real rule** via
`pg_rules::morph::synthesize`, runs the **real phonological cascade**, and emits one lexc entry
carrying both tags in the engine's own computed morph order:

- `rust/crates/pg-foma/src/preexpand.rs:199` (`should_run` — the cheap static gate), `:541`
  (`extend` — the recursion), `:956` (`build_composites_with_mode`).
- `rust/crates/pg-foma/src/emit.rs:2203` (`struct_extend`), `:2347` (`build_structural_composites`) for
  truncation, circumfix and probe-refusal composites.
- Bounded by the `MorphotacticIndex` pruning automaton (`morphotactics.rs:443`) and, hard, by
  `EnumerationBudget` (`morphotactics.rs:224`, entry budget `:182`, probe budget `:183`).

**The typed form the dossier describes does exist — on the other pipeline.**
`rust/crates/pg-foma/src/structural_allomorph.rs` is reached only from the `UnderlyingTokens` text mode
(`emit.rs:1480`), i.e. never from a production `--engine=foma` run.

**How that differs from the dossier.**

| Dossier | Shipped |
|---|---|
| Bounded action graph over captured `PartRef` spans, with ordered `Copy`/`InsertSegments`/`Modify`/`InsertContext` | Replay the real engine per (root, rule-chain) to depth 3 and record whatever surface comes back |
| `Refused` for unsupported shapes, never a literal fallback | Over-generate; refuse only when a *budget* trips, which is a resource decision wearing a capability costume |
| Preserves capture positions, action order, table identity and boundaries by construction | Preserves them because the real engine produced the string; nothing in the emitter reasons about them |
| No measured stage counters | Measured, and the numbers are the reason budgets exist: one reference grammar yields 2,930 interdigitation plus 51,023 fusion entries in tens of seconds; another reaches 2,833,559 fusion entries, a 691MB lexc file, and unbounded RSS growth on the first query |

**The cost of the shipped choice, stated plainly.** Enumeration is exactly the construction that made
one reference grammar unusable. Pruning made its search bounded but not its output —
"necessary but not sufficient", in that work's own words — and the actual rescue was substituting a
different whole-grammar compiler. So this dossier's subject matter is the one place where the mainline
construction is known to have a hard scale ceiling, and where a typed, bounded alternative has the
strongest case of the six.

**The dossier's own recorded gap is also real.** It names the shipped bridge as recognising only a
narrow `Copy(Input(0)) + InsertSegments` affine suffix shape, with a literal fallback called out as a
correctness gap rather than coverage.

**Read alongside.** `../mainline-selection-audit.md` §B3; `../technique-index.md` §2.12-2.15, §2.19;
`../../fst-plan/morphotactic-composite-pruning.md`;
`../../conformance/circumfix-structural-composite-census.md` for which circumfix shapes miss this route.

---

## Scope

StructuralAllomorph owns finite morphological actions over captured parts: insertion, deletion,
circumfixation, infixation, bounded local modification, root-pattern interdigitation, and ordered
`Copy`, `InsertSegments`, `Modify`, and `InsertContext` actions. It preserves `PartRef` captures and
source allomorph identity.

**Non-scope:** template slot legality and priority, phonological rule order, arbitrary productive
copying, compounding, and terminal boundary cleanup. A structural action is not a free-standing
prefix/suffix and must not be silently literalized when unsupported.

## Languages and families in mind

- **Anchor 1 — Tagalog. Family: Austronesian. Construct:** `-um-` infixation after the initial consonant and partial
  reduplication exercise capture of the base edge and insertion at a structural position. The
  construct role is bounded structural action, not a language-specific affix branch.
- **Anchor 2 — Amharic. Family: Ethio-Semitic. Construct:** triradical roots interdigitate with vowel templates and
  interact with prefixes/suffixes. The construct role is ordered root-slot capture and reuse.
- **Anchor 3 — Tigrinya (Ethio-Semitic):** root-consonant position and co-occurrence restrictions
  provide an independent Semitic root-pattern edge: preserving positional slots rather than merely
  copying an Amharic-shaped surface.

Tagalog and the Semitic root/template generalization are high-confidence research anchors. The
supplied direct links for Amharic and Tigrinya are a thesis and study; the scope of each grammar's
full action inventory still needs primary-source rechecking.

## Primary sources

- [Tagalog reduplication study](https://brill.com/downloadpdf/journals/bki/106/2/article-p151_2.pdf)
  for partial/base-span and infixation domains.
- [Amharic root-and-pattern thesis](https://uknowledge.uky.edu/ltt_etds/19/) for finite
  root/template interdigitation.
- [Tigrinya root-consonant study](https://joverfelt.net/wp-content/uploads/2023/10/buckley-1997-pennwp-tigrinya-root-consonants-ocp.pdf)
  for an independent Semitic positional constraint.
- [grammar model actions](../../../rust/crates/pg-grammar/src/model.rs) and [structural bridge](../../../rust/crates/pg-foma/src/structural_allomorph.rs)
  for the repository's `PartRef`/action vocabulary and current implementation boundary.

## Grammar facts

Every RHS action must reference a valid captured `PartRef` or a declared character-definition table.
The order of actions is semantic. Root identity/index, source allomorph/rule identity, table, and
boundaries survive until the consuming stage. Circumfix halves are one discontinuous unit; they are
not two independently selectable concatenative rules.

**Invariants:** captures are valid; inserted material retains table identity; action order is
preserved; identity and multiplicity survive; unsupported structural shapes return `Refused`; and a
structural action is never reclassified as ordinary copying merely because its output repeats text.

## Formal model and regularity

A bounded action graph is a finite relation over captured spans and finite insertion/modification
actions. Root-pattern interdigitation is regular when the number of captures, slots, and local
actions is bounded. It is not safe to infer structural semantics from an output substring alone.

**Correctness obligations:** the captured root segments and positions are retained, each emitted
action has the declared source identity and table, boundaries reach the correct phonological or
cleanup consumer, and the confirmed analysis multiset equals the HC oracle for admitted fixtures.

**Failure modes:** wrong `PartRef`, reordered root slots, separated circumfix halves, early boundary
cleanup, `Modify`/`InsertContext` reduced to literal text, action-order loss, root/rule identity
loss, and recursive expansion beyond budget.

## Chosen architecture

1. Extract a typed `StructuralAllomorphSpec` with rule, allomorph IDs, and bounded-shape status.
2. Lower proven finite action graphs directly or through the existing structural composite builder.
3. Preserve boundaries and identity through the interface contract.
4. Confirm the complete multiset; return `Refused` for unsupported or ambiguous shapes.

## Rejected architectures

- A literal prefix/suffix fallback for every structural rule: it changes the relation and hides
  unsupported shapes.
- Splitting a circumfix into two free rules: it creates illegal half-analyses.
- Eager `roots × rules^depth` expansion without caps: the repository explicitly documents that it
  is only workable at small scale.
- Treating structural actions as generic reduplication: it loses capture positions and action order.
- Language-specific branches: they do not generalize to a new root/template grammar.

## Interfaces and interactions

Morphotactics supplies complete units and co-occurrence state. StaticPartition may gate eligible
roots or classes. OrderedPhonology consumes preserved boundaries and applies phonological rules after
the structural action. CopyProcess is separate unless a bounded copy is explicitly a structural
action with a proven span. BoundaryCleanup remains terminal.

## Complexity and resource bounds

**Big-O variables:** `E` = eligible roots/analyses, `M` = structural rules, `D` = recursive depth,
`T` = action/pattern size, `B` = maximum branching width, and `L` = output length.

**Time:** direct bounded compilation is `O(T)` per rule before composition. Pre-expansion can reach
`O(E · M^D · B^D)` in a broad branching model; the repository's documented narrow estimate is
`O(roots × rules^depth)`.

**Space:** direct action metadata is `O(T)`; materialized composites require
`O(number_of_composites × L)` space, with the same exponential depth/branch axes. Enumeration,
probe, and compose budgets are semantic refusal boundaries, not exact-negative proofs.

## Task 6 evidence status

- **Source ModelLocation/model-ID evidence:** the repository mapping exposes `ModelLocation::MorphRule`
  and `AffixAllomorph`, with rule and allomorph-index owner/child wire IDs in
  [`capability.rs`](../../../rust/crates/pg-foma/src/capability.rs) and
  [`recipe_mechanism.rs`](../../../rust/crates/pg-foma/src/recipe_mechanism.rs). A concrete source
  model-ID witness for the named grammar anchors is `Not measured — blocks implementation claim`.
  Inserted material's `TableId` is interface metadata, not an invented `ModelLocation::Table`.
- **Resource caps:** action-size, recursive-depth, branching, enumeration, probe, and compose caps
  are required; a numeric Task 6 cap record is `Not measured — blocks implementation claim`.
- **Measured stage counters:** no per-action capture/lowering/composite counter has been recorded:
  `Not measured — blocks implementation claim`.

## Conformance fixtures

### Exercise 1 — bounded Tagalog-style infixation

Positive: root `gawa` with `-um-` after the initial consonant yields `gumawa`; expected multiset is
`{(root=gawa, affix=um, root_index=0, multiplicity=1)}`. Negatives are insertion after the wrong
segment, literal suffix output, and a root with no eligible capture. A mutation changing the
capture from initial consonant to whole stem must fail.

### Exercise 2 — Semitic root/template slots

Use root `/s,b,r/` with a finite C–V template. Expected multiset contains the one ordered root
identity and template identity with multiplicity one; permuted radical order, missing slot, wrong
root ID, and changed capture count are empty. Use Amharic and Tigrinya as independent source-backed
rows, not as production branches.

**Positive cases:** `gawa + um` inserts after the initial consonant, and `/s,b,r/` fills the finite
C–V root/template slots.
**Negative cases:** wrong insertion position, literal suffix output, missing capture, permuted radicals,
missing slot, wrong root ID, and changed capture count.
**Identity/multiplicity cases:** the normalized positive records retain `root=gawa`, `affix=um`,
`root_index=0`, and one ordered `/s,b,r/` root/template analysis, each with multiplicity one.
**Mutations:** change the capture to the whole stem, reorder radical slots, split a circumfix, or
literalize `Modify`/`InsertContext`; each mutation must be refused or fail oracle equality.
**Exact normalized expected multisets/tuples:**
`tagalog = {(surface=gumawa, root=gawa, affix=um, root_index=0, source_model_id=proposed:tagalog-infix-rule, multiplicity=1)}` and
`semitic = {(root=[s,b,r], template=finite-CV, slot_order=[s,b,r], source_model_id=proposed:semitic-template-rule, multiplicity=1)}`;
each listed negative has `{}`. These are canonical expected records, not measured outputs.

## Implementation status

The grammar model provides `OutputAction` and `PartRef`, and the current bridge recognizes only a
narrow `Copy(Input(0)) + InsertSegments` affine suffix shape. The broader emitter handles additional
composite action forms, but unsupported shapes still have a documented literal fallback in
[`structural_allomorph.rs`](../../../rust/crates/pg-foma/src/structural_allomorph.rs). That is a
known correctness gap, not completed coverage. Current status: research-ready, implementation
incomplete and fail-closed repair pending.

## Known gaps and split triggers

The supplied primary sources do not establish that every cited structural process is bounded in the
current token representation. The narrow bridge must not be generalized without action-level oracle
fixtures. A split/add is required for genuinely unbounded interdigitation, non-finite long-distance
dependencies, or operations that cannot be represented by captured parts and finite actions.

The split/adds conditions below are hypothetical future triggers, not dated evidence decisions.

**Trigger matrix:** `fits` for finite captured action graphs; `refines` when more action kinds or
table-preservation facts can be typed; `splits/adds` for unbounded/nonlocal structure or a runtime
operation outside captured finite actions.

## Research log

| Date | Evidence and direct link | Consequence |
|---|---|---|
| 2026-08-01 | [Tagalog study](https://brill.com/downloadpdf/journals/bki/106/2/article-p151_2.pdf), [Amharic thesis](https://uknowledge.uky.edu/ltt_etds/19/) | Partial insertion and root/template interdigitation exercise distinct finite action roles. |
| 2026-08-01 | [structural bridge](../../../rust/crates/pg-foma/src/structural_allomorph.rs) and [model actions](../../../rust/crates/pg-grammar/src/model.rs) | Current implementation is narrower than the typed mechanism; literal fallback remains an uncertainty boundary. |

## Evidence decisions

| Date | Decision | Evidence | Architectural consequence / trigger |
|---|---|---|---|
| 2026-08-01 | fits | Tagalog insertion and Semitic finite root/template actions share capture-and-reuse semantics. | Keep a generic bounded action mechanism. |
| 2026-08-01 | refines | Repository action vocabulary includes `Modify` and `InsertContext` beyond the current bridge. | Extend typed extraction/lowering; preserve action order and tables. |
