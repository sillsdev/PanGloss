# StaticPartition subrecipe dossier

> The section immediately following is new: the dossier proposes an architecture for subject matter
> the shipped compiler already implements, under a different name, and does not say so.

## As shipped — what the mainline actually does

**The shipped answer here is mostly "nothing", and that is the finding.**

**`gate.rs` is computed and thrown away.** The mainline builds a baseline plan on every compile
(`rust/crates/pg-foma/src/enumerate.rs:145`, called from `emit.rs:2014`), and that plan is assembled
from three grammar seams — including `gate::find_gated_subrules`
(`rust/crates/pg-foma/src/gate.rs:186`) and `gate::partition_entries` (`gate.rs:237`), at
`enumerate.rs:170-171`. The emitter reads back only the first two seams (`emit.rs:2015-2016`); the
gate partition is discarded, because the mainline never calls `gate.rs`'s compile path
(`emit.rs:1991-1992`). Consequence: **the shipped engine performs zero propose-side MPR/POS gating.**
MPR correctness on real `--engine=foma` traffic rests entirely on the confirm pass — measured on a
gated fixture as 9 candidates proposed, 8 confirmed, the excluded one rejected by confirm alone.

The trigger the dossier is built around is therefore already detected on the shipped path. Only the
mechanism is missing.

**What *does* ship as a static partition.** `compound_license`
(`rust/crates/pg-foma/src/emit.rs:1158`): head and non-head lexicon eligibility computed by MPR-bitset
overlap against every compounding rule's gates, and materialized as separate lexc sections
(`emit.rs:3165-3170`). That is a real lexical partition by MPR features, on the production path — but
it is scoped to compounding, not to phonological subrules.

**How that differs from the dossier.**

| Dossier | Shipped |
|---|---|
| `Gate(signature, relation)` wrapping *complete morphotactic relations*, with ordered phonology shared after the gate | A lexicon-section split inside one lexc emission, applying only to compound head/non-head eligibility |
| A lifetime-stable canonical group signature with typed per-group predicates | A boolean key vector of "which gated subrules apply" — the dossier already records this narrowing against `gate::partition_entries` |
| Refuses full `2^p` Boolean-product materialization | Not reached: the partition is never compiled on the mainline at all |

**Note the dossier's own honesty here.** It is the one of the six that already names a shipped
mechanism (`gate::partition_entries`) as "the real partition mechanism" and records its own richer
predicate model being cut back against it. `PartitionPredicate` and `DynamicState` are recorded in the
dossier as deleted.

**Verdict.** Both halves of the dossier's subject matter are real, and neither is finished: gating is
detected and unwired; the only shipped partition is the compounding one. If propose-side gating is
ever built, the trigger computation already exists and is already correct.

**Read alongside.** `../mainline-selection-audit.md` §A6 defect 1 and §B3; `../technique-index.md`
§2.16, §2.21; `../README.md` contradiction 15 (why the capability ledger's `Proven` disposition for
subrule gating does not mean the mainline filters on it).

---

## Scope

StaticPartition owns finite, stable lexical, POS, MPR, stem-family, or inflection-class groups
whose membership is known for the lifetime of the grammar. It provides a canonical gate around
complete downstream relations and preserves the facts used to choose the group.

**Non-scope:** facts changed by later morphology, ordered phonology, template legality, structural
actions, arbitrary copying, and boundary deletion. A dynamic fact must remain a required interface
state or become confirmation-only; it must not be frozen into a static partition.

## Languages and families in mind

- **Anchor 1 — Yalálag/Isthmus Zapotec. Family: Oto-Manguean. Construct:** lexical inflection classes, subclass
  defaults, and subject/object-dependent suffix co-occurrence exercise a stable class partition.
  The construct role is inherited/default class membership, not a phonological last-vowel guess.
- **Anchor 2 — Indonesian. Family: Austronesian. Construct:** lexical and MPR exceptions to the `meN-`
  assimilation/deletion cascade exercise a partition that keeps exception state available to the
  ordered phonology consumer. The role is a stable gate key; it does not own the rule order.
- **Scale anchor — Sena (Bantu):** noun-class gating is a useful independent scale row, but the
  private corpus is not part of this repository and is not claimed as a checked-in fixture.

The class semantics are high-confidence in the local harvest. The supplied research adds a direct
[Yalálag SIL archive record](https://mexico.sil.org/resources/archives/35309), but an archive record
is not a complete grammar; detailed class claims remain subject to primary-source reverification.
That archive-versus-grammar distinction is the main source uncertainty for this dossier.

## Primary sources

- [Yalálag archive record](https://mexico.sil.org/resources/archives/35309) for the primary
  conjugation-pattern source location.
- [Linguistic construct harvest](../../fst-plan/linguistic-recipe-harvest.md), including the Yalálag and
  Indonesian rows and its citation ledger.
- [Indonesian phonology source](https://people.ucsc.edu/~ddbrodki/PDFs/Brodkin_Indonesian.pdf)
  for the interaction between lexical exceptions and the ordered cascade.
- [Recipe mechanism graph types](../../../rust/crates/pg-foma/src/recipe_mechanism.rs) for the
  repository contract: `PartitionGroupSpec` (gate key plus sorted members) and the node's
  `construct_requirements`. This is implementation evidence, not a linguistic primary source.
  Task 7.3 deleted the earlier `PartitionPredicate` enum and the edge-borne `DynamicState`: neither
  was ever populated, and neither could be — the real partition mechanism
  (`gate::partition_entries`) exposes only the boolean key vector of which gated subrules apply,
  not a per-group predicate list. The predicate *facts* survive as typed requirements
  (`SubruleGating`, `MprGroupAppend`, `MprGroupOverwrite`) that resolve through
  `strategy_coverage`. Re-adding a predicate payload needs a derivation that does not exist yet.

## Grammar facts

Membership must be exhaustive and disjoint for the selected predicate signature, and each group
must retain the exact entries and predicate payload that justified it. Canonical signatures should
collapse equivalent inherited/default groups without collapsing distinct analysis identities.

**Invariants:** membership is stable for the partition lifetime; predicates are typed and
language-neutral; required POS/MPR/lexical-class/stem-family state is a superset of what the
consumer needs; every entry is in exactly one group for a partition domain; and identity and
multiplicity pass through the gate unchanged.

## Formal model and regularity

The partition is a finite relation `Gate(signature, relation)` over a stable key. It is regular when
the predicate set is finite and the group membership does not depend on an unbounded later state.
Construct-specific predicates are preferable to materializing a full Boolean product. A partition
that only changes the cost or grouping of an already certified relation is a lowering choice; a
partition that changes legal analyses is a semantic gate and must be oracle-confirmed.

**Correctness obligations:** every source entry maps to a group, no entry maps to two incompatible
groups, group predicates imply the intended downstream relation, and the union of group outputs has
the same identity-bearing multiset as the unpartitioned oracle.

**Failure modes:** stale class facts after morphology, overlapping or uncovered groups, inherited
default lost during canonicalization, POS/MPR state dropped at an edge, accidental partition by
surface spelling, and exponential Boolean-product materialization.

## Chosen architecture

1. Extract typed predicates and stable group signatures from grammar-derived observations.
2. Canonicalize only equivalent signatures; preserve source entries and identity metadata.
3. Put the partition around complete morphotactic relations, with ordered phonology shared after
   the gate when its interface allows it.
4. Require exact multiset certification whenever partitioning changes candidate structure.

## Rejected architectures

- Treating every dynamic feature as static: later morphology can invalidate the group.
- Partitioning by literal surface prefix/suffix: it loses lexical class and can merge homophones.
- Enumerating all `2^p` Boolean combinations: it spends resources on unattested combinations and
  makes budget exhaustion look like evidence.
- Language-specific class switches: they cannot explain a new grammar without code changes.
- Deduplicating only by surface: it destroys distinct roots and multiplicity.

## Interfaces and interactions

StaticPartition provides symbol space/table, stable predicate state, analysis/root identity,
multiplicity, boundaries, stratum, and disposition to the downstream mechanism. It must not claim a
dynamic POS/MPR fact that it cannot prove. Morphotactics consumes the partition to prune complete
alternatives; OrderedPhonology consumes preserved exception state; StructuralAllomorph and
CopyProcess must not infer class membership from a repeated surface substring.

## Complexity and resource bounds

**Big-O variables:** `E` = entries, `p` = independently tested predicates, `G` = retained groups,
`M` = predicate/member metadata, and `R` = downstream relation size.

**Time:** fact extraction and classification are `O(E · p)`; canonicalization is `O(E log E)` if
signatures are sorted, or `O(E)` with a hash/canonical map. The distinct-group bound is
`G <= min(E, 2^p)`, but construct-specific signatures should avoid building the full product.

**Space:** membership and source metadata are `O(E + M)`; canonical group signatures are `O(G · p)`;
the downstream relation remains `O(R)`. A group-cap or probe-cap refusal is non-certifying, never an
exact empty group.

## Task 6 evidence status

- **Source ModelLocation/model-ID evidence:** the repository mapping exposes `ModelLocation::MprGroup`,
  `MorphRule`, `AffixAllomorph`, and `Stratum`, with typed owner/child wire IDs in
  [`capability.rs`](../../../rust/crates/pg-foma/src/capability.rs) and
  [`recipe_mechanism.rs`](../../../rust/crates/pg-foma/src/recipe_mechanism.rs). A concrete source
  model-ID witness for the named grammar anchors is `Not measured — blocks implementation claim`.
  There is no lexical-class, POS, family, or stem-family `ModelLocation` variant; those predicates
  remain an explicit unresolved mapping rather than fabricated source IDs.
- **Resource caps:** entry, group, classification, and probe caps are required; a numeric Task 6 cap
  record is `Not measured — blocks implementation claim`.
- **Measured stage counters:** no per-stage membership, overlap, or gap counter has been recorded:
  `Not measured — blocks implementation claim`.

## Conformance fixtures

### Exercise 1 — inherited class/default partition

Construct two stable classes `C0` and `C1`, with one inherited/default allomorph shared by both and
one subclass-only allomorph. Expected partition multiset is exactly
`{(entry0, C0, 1), (entry1, C1, 1)}`; no entry may appear in both groups and the shared allomorph
must not be duplicated as two identities. Mutation: change one class predicate to overlap both
classes; validation must reject the partition.

### Exercise 2 — exception state survives the gate

Use an Indonesian-shaped `meN-` group with an exception MPR. Expected multiset is
`{(root=tulis, mpr=ordinary, meN=1), (root=loan, mpr=blocked, meN=1)}` before phonology, with
both root identities retained. Mutation: delete the blocked-MPR state at the edge; the typed
contract must reject the consumer rather than silently apply deletion.

**Positive cases:** one entry in each disjoint inherited/default class and one ordinary/blocked-MPR
exception path.
**Negative cases:** overlapping class membership, uncovered entries, and an exception path with no
blocked-MPR state.
**Identity/multiplicity cases:** the shared allomorph remains one identity, while `tulis` and `loan`
remain two distinct root identities with multiplicity one each.
**Mutations:** overlap `C0`/`C1`, drop the shared-allomorph identity during canonicalization, or delete
the blocked-MPR state at the consumer edge; each mutation must be rejected.
**Exact normalized expected multisets/tuples:**
`class-fixture = {(entry=entry0, class=C0, source_model_id=proposed:class-entry-0, multiplicity=1),
(entry=entry1, class=C1, source_model_id=proposed:class-entry-1, multiplicity=1)}` and
`exception-fixture = {(root=tulis, mpr=ordinary, meN=1, source_model_id=proposed:ordinary-mpr-entry, multiplicity=1),
(root=loan, mpr=blocked, meN=1, source_model_id=proposed:blocked-mpr-entry, multiplicity=1)}`. These are canonical expected
records, not measured group counts.

## Implementation status

The repository exposes typed partition structures in [`recipe_mechanism.rs`](../../../rust/crates/pg-foma/src/recipe_mechanism.rs),
but grammar-derived extraction and production partition materialization are not claimed complete
by Task 6. Current status: research-ready, implementation incomplete; no language-name routing is
present in the dossier architecture.

## Known gaps and split triggers

The Yalálag primary grammar content is not checked into this repository, and the Indonesian
exception inventory is summarized by the harvest rather than independently re-derived here. A
follow-on must add measured group counts, overlap/gap diagnostics, and a dynamic-state witness.

The split/adds conditions below are hypothetical future triggers, not dated evidence decisions.

**Trigger matrix:** `fits` when membership is finite, exhaustive, disjoint, and lifetime-stable;
`refines` when a typed predicate or canonical signature is missing; `splits/adds` when membership
depends on unbounded later morphology or requires a separate stateful runtime mechanism.

## Research log

| Date | Evidence and direct link | Consequence |
|---|---|---|
| 2026-08-01 | [Yalálag SIL archive](https://mexico.sil.org/resources/archives/35309) and [harvest](../../fst-plan/linguistic-recipe-harvest.md) | Class-conditioned conjugation is evidence for a stable lexical partition, but archive access is not a full verification. |
| 2026-08-01 | [typed partition model](../../../rust/crates/pg-foma/src/recipe_mechanism.rs) | Predicate/member identity and dynamic-state fields must remain explicit at graph edges. |

## Evidence decisions

| Date | Decision | Evidence | Architectural consequence / trigger |
|---|---|---|---|
| 2026-08-01 | fits | Zapotec class selection and Indonesian lexical exceptions recur as finite groupings. | Keep a generic static gate with typed predicates. |
| 2026-08-01 | refines | The same evidence distinguishes stable lexical class from later-changing morphology. | Preserve dynamic state explicitly and refuse stale static claims. |
