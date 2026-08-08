# OrderedPhonology subrecipe dossier

> The section immediately following is new: the dossier proposes an architecture for subject matter
> the shipped compiler already implements, under a different name — and here the shipped construction
> has the better measured record.

## As shipped — what the mainline actually does

**There is no ordered rewrite cascade on the shipped path.** The mainline reaches a similar relation
by a completely different construction, and the cascade the dossier proposes exists in the tree but
loses recall where it has been measured head to head.

**What ships.** A bounded, build-time surface probe that bakes the cascade's *results* into literal
lexc strings:

- `PhonologyProbe` (`rust/crates/pg-foma/src/junctions.rs:45`) is constructed once per grammar, and is
  `None` for a grammar with no phonological rules — a true no-op, byte-for-byte.
- It drives the **real synthesis engine** (`pg_rules::surface_probe::probe_synthesize`, the same
  machinery confirm uses) over a bounded local window: an affix's insert text alone, or with exactly
  one alphabet-representative neighbour on either side. Every discovered surface spelling becomes a
  literal lexc alternative.
- Deletion junctions get their own encoding: per-prefix-variant `{name}Stripped` root lexicons
  (`junctions.rs:275`), deliberately ungated by onset class, because the extra candidate is harmless
  and confirm prunes it.

**What the cascade is.** `replace.rs`'s Kaplan-Kay compiler is real
(`rust/crates/pg-foma/src/replace.rs:1311`), and it is genuinely the better *artifact* where it works —
a whole rule cascade composing in seconds where enumeration OOMs. It is reachable only from
`recipe-optimize`, tests and examples.

**The measured comparison, and it is the reason this section exists.** Run against the fixture that
exercises templatic process morphs, the cascade path **loses 6 of 25 words (24%) of recall outright** —
words both the oracle and the shipped emitter confirm, and the cascade confirms none. Two
source-verified causes: two phonological rules are silently skipped by the rule compiler with no
fallback (the mainline has `junctions.rs` for exactly this), and `InsertSimpleContext`/`ModifyFromInput`
morphs are marked skipped with no resynthesis mechanism at all on that path
(`../../fst-plan/cascade-vs-enumeration-experiment.md`).

**How that differs from the dossier.**

| Dossier | Shipped |
|---|---|
| One ordered `OrderedPhonologySpec` per stratum, compiled as an ordered composition with stage counters | A ±1-neighbour probe that runs the real engine at build time and writes literal strings; no composition at all |
| Metathesis as a dedicated finite switch relation for admitted patterns | No metathesis construction. Metathesis instead trips `probe_would_refuse` (`emit.rs:1939`), which widens *every ordinary affix rule* onto the real-synthesis composite route — a different mechanism keyed on the same grammar property |
| Refuses unsupported anchors, quantifiers and unbounded contexts | Over-generates and lets confirm prune; refusal happens at the capability layer, not in the construction |

**The one split the dossier does name correctly.** It records the snapshot compiler skipping metathesis
(`rust/crates/pg-grammar/src/compile/rules.rs`) while a `pg-foma` path has a real swap relation. That
split is real and still live.

**Verdict.** The mainline's construction is narrower in principle — it is provably blind to a
phenomenon needing to see material from more than one morpheme's own text at once — and better on
every recall measurement taken so far. Do not treat this dossier as a description of a gap to be
filled; treat it as a proposal that has to beat a measured incumbent.

**Read alongside.** `../mainline-selection-audit.md` §B3; `../technique-index.md` §2.10, §2.11, §2.24,
§2.27; `../../fst-plan/p6-prototype-report.md` for the cascade's positive results.

---

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

- [Linguistic construct harvest](../../fst-plan/linguistic-recipe-harvest.md) for Indonesian, Awngi, Selaru,
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

## Task 6 evidence status

- **Source ModelLocation/model-ID evidence:** the repository mapping exposes `ModelLocation::PhonRule`,
  `RewriteSubrule`, `NaturalClass`, and `Stratum`, with `PRuleId`, subrule-index, natural-class, and
  stratum wire IDs in [`capability.rs`](../../../rust/crates/pg-foma/src/capability.rs) and
  [`recipe_mechanism.rs`](../../../rust/crates/pg-foma/src/recipe_mechanism.rs). A concrete source
  model-ID witness for the named grammar anchors is `Not measured — blocks implementation claim`.
- **Resource caps:** compose, probe, branch, and per-stage growth caps are required; a numeric Task 6
  cap record is `Not measured — blocks implementation claim`.
- **Measured stage counters:** no per-stage state/arc counter has been recorded:
  `Not measured — blocks implementation claim`.

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

**Positive cases:** the Indonesian assimilation-then-deletion path and the admitted finite `qs → sq`
metathesis path.
**Negative cases:** reversed Indonesian order, raw `qs`, and unrelated `sr`, `tq`, and `tr` pairs.
**Identity/multiplicity cases:** each positive row retains its root/rule identity with multiplicity
one; no unrelated metathesis pair may acquire an analysis.
**Mutations:** swap the Indonesian atoms, reverse a metathesis direction, or widen a switch class to
an unrelated member; each mutation must fail oracle equality.
**Exact normalized expected multisets/tuples:**
`indonesian = {(surface=assimilated-deleted, root=tulis, affix=meN, source_model_id=proposed:indonesian-ordered-rule, multiplicity=1)}`,
`metathesis(qs) = {(surface=sq, root=entryQS, rule=metathesis, source_model_id=mrAdjacent, multiplicity=1)}`,
and `metathesis(qs|raw=qs|sr|tq|tr) = {}` for the raw/unrelated rows. These are
canonical expected records, not new stage measurements.

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

The split/adds conditions below are hypothetical future triggers, not dated evidence decisions.

**Trigger matrix:** `fits` for finite ordered rewrites and admitted finite swaps; `refines` when
stage order, direction, or symbol-space contracts need a typed field; `splits/adds` for unbounded
transposition or nonlocal context.

## Research log

| Date | Evidence and direct link | Consequence |
|---|---|---|
| 2026-08-01 | [Kaplan–Kay](https://aclanthology.org/anthology-files/anthology-files/pdf/J/J94/J94-3001.pdf) and [harvest](../../fst-plan/linguistic-recipe-harvest.md) | Ordered regular relations fit; composition growth must be budgeted. |
| 2026-08-01 | [model.rs](../../../rust/crates/pg-grammar/src/model.rs) and [Phase C gate](../../../rust/crates/pg-foma/tests/phase_c_metathesis.rs) | Rewrite and metathesis are ordered model members; finite swaps have an exact repository witness. |

## Evidence decisions

| Date | Decision | Evidence | Architectural consequence / trigger |
|---|---|---|---|
| 2026-08-01 | fits | Indonesian/Awngi ordering and finite Selaru-style swaps are regular bounded relations. | Keep one typed ordered mechanism with explicit atoms. |
| 2026-08-01 | refines | Repository evidence distinguishes physical switch positions, direction, and class-pair precision. | Preserve switch metadata and use exact branch construction. |
