# Candidate Filter Contract

**Status:** Approved direction; implementation contract for the filter-first program.

## Objective

Build a candidate filter before the new local FST generator so that candidate rejection is cheap,
explainable, and recall preserving before the HC confirmer is asked to do expensive derivational
search. The filter is intentionally incomplete: it may keep an invalid candidate, but it may never
reject an analysis that HC would accept.

The five private language projects and their word-to-analysis oracle data are immutable test inputs.
They are never copied, normalized, regenerated, edited, or committed. Committed tests use synthetic
grammars and traces. Corpus tests reference private inputs in place through the fail-closed corpus
harness and refuse to certify anything when an input is absent.

## Names and boundaries

- **FST generator:** produces a recall-preserving set of candidate proposals and symbolic witnesses.
- **Candidate filter:** applies one or more sound rejection passes to those witnesses.
- **HC confirmer:** performs the authoritative full analysis and returns final analyses. “Confirmer”
  is preferred over “verifier” because rejection proofs have their own separate, test-only verifier.

The pipeline is:

```text
FST generator -> CandidateFilter -> HC confirmer
```

The candidate filter is developed and certified first against synthetic candidates and oracle-positive
analyses. The generator is a later project that must implement this contract.

## Mathematical invariant

For a surface word `w`, let `H(w)` be the exact HC-valid analysis set, `P(w)` the generator proposal
set, and `F(P(w))` the retained proposal set.

```text
H(w) subset-of P(w)
H(w) subset-of F(P(w)) subset-of P(w)
HC(F(P(w))) = H(w)
```

Every enforced pass implements a necessary predicate. Rejection is permitted only when a verified
proof establishes that every concrete realization represented by the witness is impossible.
Unknown metadata, an unsupported construct, ambiguity that cannot be exhausted, proof-verification
failure, or filter-budget exhaustion retains the candidate.

## Candidate and witness model

Candidate identity remains the HC-facing morpheme sequence and root position. Filtering operates on
one or more witnesses for that identity:

```rust
pub struct ProposedCandidate {
    pub identity: Candidate,
    pub witnesses: NonEmpty<CandidateWitness>,
}

pub struct CandidateWitness {
    pub witness_id: WitnessId,
    pub lexical_origin: LexicalOrigin,
    pub lexicon_revision: u64,
    pub units: Vec<TraceUnit>,
    pub deferred: FeatureSet,
    pub provenance: ProposalProvenance,
}

pub struct TraceUnit {
    pub morpheme: MorphemeId,
    pub role: TraceFact<TraceRole>,
    pub allomorphs: TraceFact<NonEmpty<AllomorphId>>,
    pub slot: TraceFact<Option<TraceSlotId>>,
    pub stratum: TraceFact<Option<TraceStratumId>>,
    pub surface_span: TraceFact<Option<SurfaceSpan>>,
    pub local_events: TraceFact<Vec<LocalEvent>>,
}

pub enum TraceFact<T> {
    Known(T),
    Deferred(DeferredFactReason),
}

pub struct TraceSlotId(pub u32);
pub struct TraceStratumId(pub u32);

pub enum TraceRole {
    Root,
    Prefix,
    Suffix,
    Infix,
    Stem,
    Other(u16),
}
```

`TraceRole`, `TraceSlotId`, and `TraceStratumId` are candidate-filter contract types with explicit
provenance mappings. They are not assumed to exist in today’s grammar model and cannot be populated
by ordinal coincidence.

The trace is compact proposal metadata, not `pg_rules::trace`'s full HC execution tree. Multiple
witnesses with the same identity are preserved until filtering finishes. A candidate survives when
at least one witness survives. A pass may reject an ambiguous allomorph set only when it proves that
every member is impossible.

## Filter decisions and proof verification

Each pass returns exactly one decision:

```rust
pub enum PassDecision {
    Keep,
    Reject(RejectionProof),
    Defer(DeferReason),
}
```

`Defer` is semantically retained. It differs from `Keep` only diagnostically: `Keep` means the pass
proved no contradiction in the facts it owns, while `Defer` means it lacked sufficient certified
facts.

A rejection proof contains a stable pass ID, rule ID, category, and machine-checkable witness. It
is evidence about a decision, not a precondition for it.

**The pipeline performs no verification.** A pass's `Reject` is acted on directly; the proof is
constructed, carried, and recorded as evidence. There is no verifier in the filter, no check
before enforcement, and no knob selecting how much to check.

Verification is instead a **post-hoc assertion over recorded evidence**. A test runs the filter
with a ledger, then checks that every proof the run emitted re-derives against the witness it was
emitted for. That is a stronger statement than an inline check: it asserts no invalid proof was
ever produced, rather than that the pipeline declined to act on one.

Two intermediate designs were considered and rejected, both for the same reason.

An inline verifier gated by depth kept a checking branch in the pipeline that production would
never take, and left the verifier reachable from nothing — dead code in a production module,
described as a safety mechanism.

An envelope-only check — identity, witness, revisions, cited units — looked nearly free, but the
pipeline hands a witness to a pass, the pass builds a proof from that same witness, and the
envelope then re-verifies the proof against the witness the pipeline just supplied. That is
first-party code checking itself, which is the reasoning that moved claim checking out of
production, applied one level further.

What follows is that verification code lives with the tests that use it, not beside the pipeline,
and that a pass's correctness is established before it ships rather than watched for at runtime.

**`Full` is not a production safety mechanism, and the contract does not pretend otherwise.** Every
pass is first-party compiled code in this crate; no untrusted party authors a proof, so there is no
forgery threat model at runtime. Re-deriving a claim catches exactly one bug class — a pass whose
written claim contradicts the trace it read — and it does *not* catch the dominant risk, a pass
whose grammar reasoning is simply wrong, because such a pass emits a claim perfectly consistent
with the trace. Paying that cost on every rejection would buy narrow protection against an
adversary that does not exist.

Independent re-derivation therefore lives where it is affordable, exhaustive, and genuinely
independent: the model/property tests, which compare each pass against a separate reference
predicate over completely enumerated small domains. That is a second implementation checking the
first, which is what "verified" should mean here.

What the proof carries in production is its *explanation* — the death ledger, shadow correlation,
and the only diagnosis a user has under enforcement. Proofs are carried and recorded always;
`Full` checking of them is a testing and shadow instrument.

**Reproducibility is what replaces runtime checking, and it is therefore a requirement, not a
convenience.** A recorded rejection names its pass, rule, category, candidate identity, and
witness. That, plus the grammar, is enough to re-run the same word offline — with filtering off, or
with post-hoc verification enabled — and re-derive exactly what happened. Paying to check every
rejection is not needed in order to explain any rejection later. This holds only while filtering is
deterministic,
so determinism is pinned by test: the same passes, input, mode, and budget must produce identical
retained sets, identical decisions, and an identical ledger, including event ordinals and pass
order.

It follows that the runtime switch which disables filtering is not a nicety. It is the primary
diagnostic instrument: the first question about a word that stopped parsing is whether it parses
with filtering off, and that answer isolates the filter from the proposer and from HC in one step.

Initial proof categories are finite and versioned:

- malformed candidate identity;
- impossible morpheme ownership;
- forbidden morphotactic transition;
- missing or mismatched finite partner, including a circumfix half;
- static morpheme/allomorph co-occurrence violation;
- no compatible allomorph alternative;
- static POS/MPR/signature conflict;
- impossible exact local surface span;
- impossible certified local environment.

Long-range phonological realization, uncertain harmony, unbounded copy/reduplication, and any rule
whose abstraction loses relevant information always defer to HC.

The existing grammar model does not retain independent circumfix-half identities after it compiles
the cross-product into one affix allomorph. Finite-partner passes can be built and unit-tested against
the contract now, but they belong to no enforced profile until the future generator emits explicit,
stable partner events whose provenance post-hoc verification can check. Unknown is never represented
by a sentinel `AllomorphId`.

## Multiple passes and death traces

Passes run in a declared deterministic order. The audit trail records every pass reached by every
witness:

```rust
pub struct PassEvent {
    pub event_ordinal: u64,
    pub pass_ordinal: u16,
    pub candidate_ordinal: u64,
    pub candidate_identity: Candidate,
    pub pass_id: StablePassId,
    pub witness_id: WitnessId,
    pub outcome: PassOutcome,
}

pub enum PassOutcome {
    Kept,
    Deferred(DeferReason),
    Rejected(RejectionProof),
}
```

`WitnessId` is unique only within one `ProposedCandidate`; the report key is
`(candidate_ordinal, witness_id)` and also carries the candidate identity. Reusing witness ID `1` in
two different candidates is legal and cannot collide in a death ledger. `event_ordinal` and
`candidate_ordinal` use checked `u64` increments. On impossible diagnostic-counter overflow, the
trace sink records `ordinal_overflow`, stops retaining detailed events, and continues filtering with
compact saturating counters; overflow never rejects or drops a candidate.

Evaluation stops for a witness at its first verified rejection. A candidate death record is emitted
only after every witness dies and links to each witness’s terminal `PassEvent`. This answers both
“where did this trace die?” and “why did the candidate die despite alternative FST paths?”

Filtering is incremental. Production callers provide a retained-candidate sink and a trace sink;
the filter does not require the entire proposal set or death ledger in memory. The ordinary trace
sink keeps only compact per-pass counters. An opt-in bounded ledger stores detailed `PassEvent` and
candidate-death records, reports how many records were omitted at its cap, and never changes filter
decisions. If a filter budget expires, the pipeline switches to bypass and forwards the rest of the
input stream unchanged.

Reports are deterministic: candidate/witness IDs, pass order, proof categories, and counters do not
depend on hash-map order or wall clock. Timing may be recorded separately for observation but never
used to certify an optimization.

## Modes and profiles

The filter supports:

```rust
pub enum FilterMode {
    Off,
    Shadow,
    Enforce,
}
```

- `Off` bypasses all passes.
- `Shadow` computes and records death decisions but sends every candidate to HC.
- `Enforce` removes only candidates every witness of which was rejected with a recorded proof.

Public profiles are certified pass bundles, not arbitrary unsafe booleans:

- `StructuralV1`: identity, ownership, and structurally certain transitions.
- `SymbolicV1`: `StructuralV1` plus ordering, co-occurrence, and static signature rules. It
  explicitly excludes finite-partner/circumfix pairing.
- `BoundaryLocalV1`: `SymbolicV1` plus exact-span and certified local-environment checks. It
  remains shadow-only before generator evidence supplies those facts non-vacuously.

Finite-partner filtering is an internal shadow-only pass in this project. A future immutable
`PartnerAwareV1` profile can include it only after the generator provides a proof-verifiable
`PartnerProvenanceCatalog` and passes the complete promotion ladder. Existing profile membership
never changes in place.

An internal pass can exist in shadow mode before it belongs to a public enforced profile.

## Certification ladder

Each pass advances independently:

1. Unit tests pin positive, negative, ambiguous, missing-metadata, and invalid-proof behavior.
2. Model/property tests enumerate small abstract candidate spaces and compare the pass with an
   exhaustive reference predicate.
3. Synthetic integration tests prove the pass actually fires and reduces candidates before HC.
4. Shadow tests prove every would-rejected candidate receives zero HC confirmations.
5. All oracle-positive analyses in each available private language corpus survive.
6. `Off` and `Enforce` return identical deduplicated `AnalysisIdentity` sets and identical exact HC
   output multisets per word occurrence, including duplicate structured analyses and signatures.
7. Deterministic counters prove nonzero filter firing and lower candidates presented to HC,
   confirmation calls/groups, or HC steps on named inputs.

No wall-clock threshold promotes a pass. A pass cannot enter an enforced profile from a vacuous run:
the gate requires both zero oracle-positive kills and at least one verified rejection.

The private corpus gate is fail-closed through `rust/tools/pg.ps1 -Mode corpus-test`; an ordinary
worktree without those inputs may run unit tests but cannot claim corpus certification.

## Hybrid growth path

The first implementation is a Rust pass pipeline. Once a regular pass is stable and measured hot, it
may compile its finite rules into a trace DFA. The DFA proposes a `RejectionProof`; the same Rust proof
verifier remains authoritative. Imperative and compiled passes therefore share one contract, report,
and promotion ladder.

Lazy intersection with the future generator is explicitly deferred. It is considered only if the
post-generation materialized candidate set remains the measured bottleneck after `BoundaryLocalV1`.

## Generator handoff

The later FST-generator plan must satisfy these acceptance conditions:

- emit `ProposedCandidate` values without lossy deduplication of witnesses;
- preserve stable morpheme, allomorph, rule, role, slot, stratum, span, deferred-feature, and
  provenance metadata needed by the certified passes;
- represent a deferred surface-changing feature with a proven surface superset or an explicit
  runtime fallback;
- support revisioned runtime stem origins without compiling root-by-complete-chain entries;
- expose explicit incomplete results rather than silently truncating candidates;
- pass the same five-language oracle-containment gate before replacing the legacy proposer.

## Out of scope

- Building the new local FST generator.
- Changing HC semantics.
- Committing any private language, word list, analysis output, or derivative thereof.
- Resuming eager root-by-chain or monolithic-FST expansion work.
- Ranking-based deletion, top-N truncation, or timeout-as-rejection.
