# Executable Subrecipes Design

## Purpose

PanGloss shall tune grammars by composing reusable linguistic mechanisms, not by selecting
language-named presets or attaching linguistic names to equivalent Plan rewrites. A language may
use any compatible subset of the mechanism catalogue. New language evidence may refine an
existing mechanism or add a new one when its semantics and interface cannot be expressed by the
existing catalogue.

The four parity languages remain required scale and integration gates. They are not, by
themselves, evidence of typological generality. Small source-backed conformance grammars provide
the orthogonal semantic evidence.

> **Historical/superseded acceptance scope.** The four-language requirement above is retained as
> scale and integration provenance, not as the current shipping gate. Current acceptance is limited
> to Indonesian, Amharic, and Aweti.

## Decisions

### D1. Preserve Plan as the physical algebra

Do not add one Plan node kind per linguistic construct. `Leaf`, `Union`, `Compose`, `Gate`, and
`Replace` remain the physical relation algebra. Existing emitters, rewrite compilers, gate oracle,
structural compiler, and runtime peeler remain reusable lowering machinery.

### D2. Add a grammar-derived mechanism graph

One shared extractor shall derive a `MechanismGraph` from the grammar and the existing capability
inventory. It must not become a third independent grammar walker beside capability reporting and
recipe facts.

```rust
pub struct MechanismGraph {
    pub nodes: Vec<MechanismSpec>,
    pub edges: Vec<MechanismEdge>,
}

pub struct MechanismSpec {
    pub id: MechanismId,
    pub sources: Vec<ModelLocation>,
    pub stratum: Option<StratumId>,
    pub body: MechanismBody,
}

pub enum MechanismBody {
    Morphotactics(MorphotacticsSpec),
    StaticPartition(StaticPartitionSpec),
    OrderedPhonology(OrderedPhonologySpec),
    StructuralAllomorph(StructuralAllomorphSpec),
    CopyProcess(CopyProcessSpec),
    BoundaryCleanup(BoundaryCleanupSpec),
}

pub struct MechanismEdge {
    pub producer: MechanismId,
    pub consumer: MechanismId,
    pub contract: InterfaceContract,
}
```

Only pipeline precedence and required/provided interface state are generic graph dependencies.
Template slot order, obligatory co-occurrence, mutual exclusion, and allomorph priority remain
inside `MorphotacticsSpec`, where their semantics are known.

### D3. Make interface preservation explicit

Every edge declares:

- symbol space and active character table;
- analysis/tag identity and root identity/index;
- multiplicity policy;
- boundary state;
- POS, MPR, lexical-class, stem-family, and copy-span state;
- stratum/phase;
- execution disposition: `ExactFst`, `ConfirmOnly`, `Peeled`, or `Refused`.

Materialization must fail closed when a consumer's requirements are not provided. Correct surface
strings do not compensate for lost analysis identity, multiplicity, or rule order.

### D4. Separate linguistic mechanisms from physical adapters

`surface-probed`, `underlying-token-cascade`, and plan-composed compilation are lowering adapters,
not subrecipes. Metathesis is an ordered-rule atom. Junction/deletion filtering is a lowering
choice inside ordered phonology. Complete templates and priority allomorphs are parts of
morphotactics. Unbounded copying is a runtime mechanism, not an FST family label.

### D5. Materialize an executable recipe

```rust
pub struct ExecutableRecipe {
    pub mechanisms: MechanismGraph,
    pub plan: Plan,
    pub adapter: LoweringAdapter,
    pub runtime_ops: Vec<RuntimeOp>,
}
```

The pipeline is:

```text
Grammar
  -> capability-backed extract_mechanisms
  -> typed mechanism graph
  -> materialize realizable mechanism choices
  -> executable recipe
  -> physical lowering adapter
  -> proposer
  -> full HermitCrab multiset certification
```

Arbitrary Plan-tree search remains rejected. Exact bounded enumeration is preferred initially.
Memoized dynamic programming is permitted only after typed interfaces are stable and repeated
substructure makes it measurably useful. Plan length is not an admissible lower bound on runtime
work and must not activate branch-and-bound.

## Mechanism catalogue

### Morphotactics

Covers templates, ordered slots, obligatory/paired exponence, lexical continuations, zero
morphemes, allomorph priority, and bounded compound/template depth. Complete alternatives lower to
whole morphological relations; paired circumfix/template members are never independently toggled.

Research anchors: Orizaba Nahuatl complete person/number templates and zero person marking;
Caquinte discontinuous future morphology. Scale anchors: Sena and Amharic templates.

Expected complexity: extraction is linear in grammar morphology records. Explicit alternative
enumeration is proportional to the legal template paths; the worst case is exponential in truly
independent optional slots, which is why co-occurrence and mutex facts must prune before
materialization. Epsilon morphology must be acyclic or application-bounded.

### StaticPartition

Covers finite, stable lexical/POS/MPR/class partitions. Partitions must be exhaustive and disjoint.
Facts changed by later morphology are not static and must widen or remain confirmation-only.

Research anchors: Yalálag/Isthmus Zapotec lexical inflection classes and subclass defaults;
Indonesian lexical/MPR exceptions. Scale anchors: Sena noun-class gating.

Expected complexity: fact extraction is `O(entries × tested predicates)`. The number of distinct
groups is bounded by `min(entries, 2^p)` for `p` independent Boolean predicates; construct-specific
facts and canonical signatures must avoid materializing the full Boolean product.

### OrderedPhonology

Covers rule order, morphology/phonology strata, harmony/docking/deletion cascades, and bounded
metathesis atoms. Order may change only under a proved equivalence.

Research anchors: Indonesian nasal assimilation before deletion; Awngi tone docking before
deletion. Huallaga Quechua supplies an independent morphology–derivation–morphology stratal case.

Expected complexity: extraction is linear in ordered rules and strata. Individual rewrite
construction is bounded by the compiler's rule/context caps. Naive relation composition can have
state-space product `O(product |Q_i|)`; stage reports and hard budgets must expose intermediate
growth. Reordering is not a search dimension unless equivalence is proved.

### StructuralAllomorph

Covers ordered LHS/RHS structural actions: insertion, deletion, circumfixation, infixation,
root-pattern interdigitation, and bounded local modification. Unsupported shapes are explicit,
never silently literalized.

Research anchors: Amharic root-pattern interdigitation; Tagalog infixation/partial reduplicative
structural processes. Caquinte supplies discontinuous morphology interacting with epenthesis and
metathesis.

Expected complexity: direct bounded lowering is proportional to the finite action/pattern graph
plus FST composition cost. The current pre-expansion bridge can be
`O(roots × rules^depth)` and must remain measured and capped until replaced by direct lowering.

### CopyProcess

Covers both bounded local copying and productive unbounded copying, with distinct dispositions.
Bounded copying may lower exactly after proving its span bound. Productive arbitrary-length full
copying is not claimed as a one-way FST relation and uses a budgeted peeler plus confirmation.

Research anchors: Tagalog bounded initial-CV reduplication; Indonesian productive full-stem
reduplication and affixed reduplicants.

Expected complexity: a fixed span `k` can require `O(|Sigma|^k)` remembered prefixes in a generic
finite-state construction. An unbounded-copy peeler scans candidate split/rejoin points and must
declare an input-length and chain-depth bound; its practical work is at least linear in the word
per tested hypothesis and may multiply with nested peel candidates. Budget exhaustion is
non-certifying.

### BoundaryCleanup

Covers terminal removal/normalization of boundary symbols after all consumers have run. It is an
explicit mechanism even when it is not a search axis. It must be idempotent and reject a mismatched
symbol-space adapter.

Research anchors: Sena boundary-only/null allomorph behavior; Caquinte boundary-crossing
epenthesis/metathesis. These anchors ensure cleanup happens after, not before, boundary consumers.

Expected complexity: constructing the cleanup relation is linear in relevant boundary definitions;
applying/composing it inherits normal FST traversal/composition cost. Applying cleanup twice must
be relationally equivalent to once.

## Conformance basis

Every mechanism requires at least two independent exercises where a second meaningful exercise is
possible: either two language-backed minimal grammars or one language-backed grammar plus an
orthogonal mutation/negative grammar. Each fixture includes a positive, a negative, and an
identity/multiplicity-sensitive case.

| Construct row | Primary references in mind | Required disposition |
|---|---|---|
| Complete template, order, co-occurrence | Caquinte; Orizaba Nahuatl | `ExactFst` |
| Serial cascade and strata | Indonesian; Awngi; Huallaga Quechua | `ExactFst` |
| Lexical class/subclass selection | Yalálag/Isthmus Zapotec; Sena | `ExactFst` |
| Disjunctive allomorph priority | English plural pattern; Orizaba null/elsewhere | `ExactFst` |
| Bounded partial copying | Tagalog; an independently shaped bounded-copy mutation | `ExactFst` |
| Productive unbounded copying | Indonesian; a second full-copy family added by dossier research | `Peeled` |
| Bounded metathesis | Selaru; Caquinte | `ExactFst` or declared `ConfirmOnly` |
| Root-pattern interdigitation | Amharic; a second Semitic root-pattern system | bounded `ExactFst` |
| Feature/POS/MPR gates | Indonesian; Sena; Amharic | `ExactFst` when static |
| Compounding and recursive identity | English recursive compounds; an HC compound grammar | bounded exact or confirm |
| Zero morphology and epsilon safety | Orizaba Nahuatl; Sena | bounded `ExactFst` |

The second full-copy and second root-pattern research anchors must be chosen from primary grammar
evidence before those rows leave research status. They are explicit open research obligations, not
placeholders in the implementation contract.

## Parity and certification contract

For every conformance row and parity corpus, compare the normalized multiset of confirmed analyses:

```text
surface + ordered morpheme IDs + root identity/index + relevant rule trace + multiplicity
```

- `ExactFst`: proposal multiset equals the oracle.
- `ConfirmOnly`: proposals contain the oracle; confirmed multiset equals it.
- `Peeled`: peeled and confirmed multiset equals it; reports must not call this FST coverage.
- `Refused`: no selectable candidate is produced.
- Any timeout, cap, truncation, or excluded requested word is non-certifying.

The four-language scoreboard requires zero oracle exclusions and pinned eligible-corpus hashes.

## Per-subrecipe research dossier

Each mechanism owns `docs/fst-plan/subrecipes/<mechanism>.md`. The file is a maintained engineering
reference, not a one-time implementation report. Every dossier must contain:

1. Scope and explicit non-scope.
2. Linguistic phenomena and at least two language/family research anchors.
3. Primary-source bibliography with claim-level links.
4. Extracted grammar facts and source-model locations.
5. Formal relation and regularity/boundedness analysis.
6. Chosen architecture and rejected alternatives with reasons.
7. Interface contracts and interactions with other mechanisms.
8. Complexity model, blow-up axes, caps, and measured stage counters.
9. Conformance fixtures, mutations, and exact expected multisets.
10. Current implementation/certification status.
11. Known gaps and evidence that would trigger refinement or a split.
12. Research log with dated findings after implementation.

New evidence changes the catalogue by one of three explicit decisions: `fits`, `refines`, or
`splits/adds`. A new language never directly creates a language-named production branch.

## Delivery order

1. Correct fail-closed corpus certification and deterministic D4 reporting.
2. Add shared mechanism types, extraction, artifact validation, and dossiers without changing
   compilation behavior.
3. Route complete template-aware morphotactics through the executable-recipe contract.
4. Re-express static partition plus ordered phonology with typed provenance.
5. Promote the narrow structural-allomorph compiler and reject unsupported shapes.
6. Implement real per-stratum pipelines.
7. Wire explicit bounded/unbounded copy contracts and runtime manifest support.
8. Add remaining orthogonal fixtures, then run the four zero-exclusion scale gates.

Each vertical slice follows RED–GREEN–REFACTOR, receives spec and code-quality review, and lands as
an independently revertible commit. A fresh xhigh architecture review is required before merging
the first executable slice and before the final branch merge.

## Rejected alternatives

- More top-level linguistic labels over existing permutations: names without executable semantics.
- A fully general constraint graph and memo optimizer now: premature before interfaces stabilize.
- Language-name routing: non-generalizing and unauditable.
- One graph edge kind per linguistic relation: duplicates semantics better owned by deep modules.
- Arbitrary Plan-tree or equality-saturation search: most trees are linguistically unrealizable.
- Treating full copying as ordinary one-way FST compilation.
- Set-based parity or surface-only recall.
