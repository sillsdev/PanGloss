# OrderedPhonology subrecipe dossier

## Scope

OrderedPhonology owns ordered rewrite cascades, morphology/phonology strata, harmony or docking,
deletion, fusion, and bounded metathesis atoms. The order is part of the relation: a candidate that
contains the same rules in a different order is not equivalent without a proof.

**Non-scope:** template legality, static lexical partitioning, structural allomorph actions,
arbitrary productive copying, and terminal boundary cleanup. Metathesis belongs here only when the
grammar supplies a finite switch relation; apparent metathesis that decomposes into rewrites belongs
to those rewrites.

## Languages and families in mind

- **Anchor 1 — Indonesian. Family: Austronesian. Construct:** nasal place assimilation must precede deletion of
  `{p,t,k,s}` in the `meN-` cascade. The construct role is ordered rewrite composition with lexical
  exception state, not a generic “phonology” label.
- **Anchor 2 — Awngi. Family: Cushitic. Construct:** floating high-tone docking must precede deletion of the floating
  tone. The construct role is a two-stage stratal cascade whose trigger would disappear if order
  were reversed.
- **Anchor 3 — Selaru (Austronesian):** a bounded switch exercises explicit metathesis as one
  ordered atom. The repository metathesis tests additionally exercise exact member-pair selection,
  direction, and identity preservation.

The Indonesian/Awngi ordering principles are high-confidence in the local harvest; the Selaru
primary scans are linked in the harvest ledger but were not independently downloaded in this task.

## Primary sources

- [Linguistic construct harvest](../linguistic-recipe-harvest.md) for Indonesian, Awngi, Selaru,
  and the citation ledger.
- [Kaplan and Kay, Regular Models of Phonological Rule Systems](https://aclanthology.org/anthology-files/anthology-files/pdf/J/J94/J94-3001.pdf)
  for regular rewrite cascades and composition growth.
- [Selaru sketch](https://openresearch-repository.anu.edu.au/server/api/core/bitstreams/6c39a09e-9bf0-47de-b1ca-9280ade7e514/content)
  as the direct source supplied by the completed research.
- Repository source: [`model.rs`](../../../rust/crates/pg-grammar/src/model.rs) stores rewrite and
  metathesis members in one ordered phonological-rule vocabulary; [`phase_c_metathesis.rs`](../../../rust/crates/pg-foma/tests/phase_c_metathesis.rs)
  checks exact swap behavior. These are implementation sources, not linguistic primary sources.

## Grammar facts

The grammar model preserves a single ordered phonological-rule list, a stratum/table, rule
direction, and the physical positions of metathesis switch members. The loader requires distinct
`leftSwitch` and `rightSwitch` references. The snapshot compiler currently skips metathesis, while
the pg-foma Phase C path has a real swap relation; this is an implementation split, not evidence
that the construct is absent from the grammar model.

**Invariants:** rule order and stratum are preserved; active table/symbol space matches; analysis,
root identity, boundaries, and multiplicity survive each atom; a metathesis atom swaps only the
matched pair; and unsupported anchors, quantifiers, or unbounded contexts are refused or
confirmation-only rather than approximated.

## Formal model and regularity

Each bounded rewrite or switch is a regular relation. A cascade is ordered composition of those
relations. Finite switch patterns with finite natural-class members are regular; a construction
that needs unbounded transposition or nonlocal spreading is outside this exact claim.

**Correctness obligations:** the proposed multiset must be contained in the complete HC oracle and
the selectable result must equal the oracle multiset for certified fixtures. Rule order, direction,
switch identity, stratum, and multiplicity are observable correctness data.

**Failure modes:** rule reordering, trigger deletion before docking, a naive class cross-product that
swaps nonmatching members, direction blindness, unsupported anchors, and dropping analysis identity
while preserving only the surface string.

## Chosen architecture

1. Extract one ordered `OrderedPhonologySpec` per stratum with explicit rewrite/metathesis atoms.
2. Compile bounded atoms through the existing physical relation builder and expose stage counters.
3. Keep metathesis as a dedicated finite swap relation only for admitted patterns.
4. Run full HC multiset certification, with `ConfirmOnly` for overgenerating but safe shapes.

## Rejected architectures

- Sorting rules by type or alphabetic ID: it changes the grammar relation.
- Treating metathesis as a generic one-symbol replace: it accepts the wrong alignment and loses
  switch identity.
- Enumerating arbitrary segment permutations: it grows factorially and overgenerates.
- Claiming every apparent surface transposition is metathesis: some cases decompose into rewrites.
- Falling back to literal output when the ordered relation is unsupported: it converts uncertainty
  into a false negative or false positive.

## Interfaces and interactions

Morphotactics must deliver the selected morphology, boundaries, table, dynamic exception state,
stratum, and identity before this mechanism runs. StaticPartition may provide exception gates;
StructuralAllomorph may add bounded material before the cascade; CopyProcess may rejoin only with
explicit copied-span state. BoundaryCleanup is terminal and must not run before a rule that consumes
its markers.

## Complexity and resource bounds

**Big-O variables:** `q_i` = states for stage `i`, `r` = ordered rules, `n` = token length,
`a` = alphabet/class members, and `B` = compose/probe budget.

**Time:** extraction is `O(r)` in ordered rule records. A bounded local atom is `O(n · a)` for
matching/branch resolution. Naive composition can approach `O(∏_i |Q_i|)` states and corresponding
arc work; budgets and stage counters must expose intermediate growth.

**Space:** a materialized cascade is `O(∏_i |Q_i|)` in the worst product bound, plus `O(r + a)`
metadata. A budget exhaustion or timeout is non-certifying and must not be reported as an exact
negative.

## Conformance fixtures

### Exercise 1 — ordered Indonesian cascade

Positive: `meN- + tulis` yields the documented assimilation-then-deletion surface; the analysis
multiset retains the root and affix identities with multiplicity one. Negatives: reversed order,
unassimilated stop, and a blocked exception missing its MPR state. A mutation that swaps the two
atoms must fail oracle equality, not merely change network size.

### Exercise 2 — precise finite metathesis

Use the repository's multi-member metathesis shape: underlying `qs` must yield surface `sq` with
exactly one analysis; raw `qs` must be empty, and unrelated `sr`, `tq`, and `tr` combinations must
remain empty. The [Phase C test](../../../rust/crates/pg-foma/tests/phase_c_metathesis.rs) is the
implementation analogue. This is a precise relation exercise, not a claim about all metathesis.

## Implementation status

The grammar model and loader preserve ordered mixed rule kinds and switch positions. The snapshot
compiler's explicit skip is documented in [`compile/rules.rs`](../../../rust/crates/pg-grammar/src/compile/rules.rs),
while pg-foma has exact Phase C metathesis tests. Unified executable-recipe extraction and all
ordered-cascade materialization are not claimed complete. Current status: research-ready,
implementation incomplete.

## Known gaps and split triggers

The exact scope of every external Awngi and Selaru citation needs primary-source re-verification.
Metathesis with anchors, quantifiers, or unsupported context must remain refused until a bounded
relation and oracle fixture exist. A split/add is warranted for unbounded transposition,
nonlocal spreading, or a runtime operation outside finite ordered relations.

**Trigger matrix:** `fits` for finite ordered rewrites and admitted finite swaps; `refines` when
stage order, direction, or symbol-space contracts need a typed field; `splits/adds` for unbounded
transposition or nonlocal context.

## Research log

| Date | Evidence and direct link | Consequence |
|---|---|---|
| 2026-08-01 | [Kaplan–Kay](https://aclanthology.org/anthology-files/anthology-files/pdf/J/J94/J94-3001.pdf) and [harvest](../linguistic-recipe-harvest.md) | Ordered regular relations fit; composition growth must be budgeted. |
| 2026-08-01 | [model.rs](../../../rust/crates/pg-grammar/src/model.rs) and [Phase C gate](../../../rust/crates/pg-foma/tests/phase_c_metathesis.rs) | Rewrite and metathesis are ordered model members; finite swaps have an exact repository witness. |

## Evidence decisions

| Date | Decision | Evidence | Architectural consequence / trigger |
|---|---|---|---|
| 2026-08-01 | fits | Indonesian/Awngi ordering and finite Selaru-style swaps are regular bounded relations. | Keep one typed ordered mechanism with explicit atoms. |
| 2026-08-01 | refines | Repository evidence distinguishes physical switch positions, direction, and class-pair precision. | Preserve switch metadata and use exact branch construction. |
| 2026-08-01 | splits/adds | Unbounded transposition or nonlocal spreading cannot be bounded honestly. | Add a separate refused/peeled mechanism; never reorder or literalize. |
