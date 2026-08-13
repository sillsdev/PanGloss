## Context

> **Imported-seed correction (2026-08-13):** The design below came from `b1c02f8c`. Wherever it uses
> `CharacteristicKind`, configuration partitions, interaction IDs, or ordering archetypes as the
> denominator, implementation must instead join stable IDs from the pinned Machine atomic,
> within-rule configuration, cross-rule interaction, and schedule ledgers. Existing PanGloss enums
> are projections only. Backend admission and realization are read from the single
> `PlanningOutcome`; the matrix must not retain `selection.rs`, `backend_selection.rs`, or health as
> competing authorities.

PanGloss has three emission backends and a propose-and-confirm correctness invariant: a compiled
backend may over-propose, but it must not omit any analysis returned by the matched Rust HermitCrab
runtime. Current evidence is split across several mechanisms:

- conformance replay checks fixture expectations against `pg_parse::Morpher` but does not run every
  compiled backend;
- `strategy_coverage` classifies each backend/characteristic representation claim but is curated
  evidence, not a semantic run;
- `witnessed_coverage` compiles permissible backend/fixture pairs and calls successful compilation a
  witness without querying a word;
- `cross_compiler_equivalence_gate` performs the required semantic comparison for a small set of
  fixtures and words; and
- plan-interaction coverage records seven structural plan-adjacency tuples plus a narrow
  gate-reordering fuzz slice, not feature/configuration or between-rule ordering completeness.

The change measures the normal named analysis pipelines: Rust HermitCrab-only and each applicable
FST-propose-plus-Rust-HermitCrab-confirm backend. It changes no production selection policy. The
implementation begins in an isolated worktree while the concurrent conformance/backend correction
work continues. PanGloss owns Machine and the conformance grammars, but those surfaces are
concurrently occupied: before overlapping edits, record the other agent's exact branch, accepted
tip, files, and manifest/submodule changes, assign ownership, and rebase onto its integration tip.

`openspec/changes/STAGING.md` remains authoritative. This change owns the backend semantic runner,
case/accounting schema, and integration gates. It does not take ownership of `replace.rs`, `gate.rs`,
`emit.rs`, `preexpand.rs`, `peel.rs`, or `morphotactics.rs`; any backend correction touching those
hotspots is serialized after the other semantic owner finishes.

## Goals / Non-Goals

**Goals:**

- Make every claimed backend semantic obligation executable and red on unexpected failure.
- Compare fixture expectations, direct Rust HermitCrab results, proposals, and confirmed results for
  the same stable assessment cases.
- Distinguish semantic support from honest unsupported/refusal and from compile-only observation.
- Reuse shared conformance grammars and expectations for all observable semantics while keeping
  PanGloss-only implementation and operational assertions in Rust.
- State finite PR and full-suite denominators and fail when a required case disappears, becomes
  vacuous, silently falls back, truncates, or changes outcome.
- Make the existing on-demand C# adapter usable against the selected semantic fixtures and record
  machine-checkable oracle provenance.
- Produce failing acceptance cases before backend implementation and repair all unexpected failures
  before the branch merges.

**Non-Goals:**

- Claiming all Cartesian products of grammar fields or all factorial rule permutations.
- Replacing the future reachable ordered-rule-pair census in
  `docs/rule-interaction-and-ordering-coverage-plan.md`.
- Treating `CannotRepresent` as a failure to add capability. It is an honest blank whose selector
  refusal is tested, not support evidence.
- Adding new grammar constructs, changing backend preference, or weakening capability predicates.
- Requiring C# to reproduce PanGloss resource limits, topology, counters, containment behavior, or
  crashes.
- Using path/discovery multiplicity as semantic equality. Semantic equality is the deduplicated
  structured analysis identity set; duplicate copies remain separate health evidence.

## Decisions

### D1. A matrix case references one shared conformance assessment case

Add an optional opaque `case_id` to `pg_conformance_fixtures::WordEntry` and a structured oracle
provenance declaration to `WordsYaml`. A matrix-selected word must have a non-empty case ID unique
within its fixture. The stable join key is `(fixture category, fixture name, case_id)`; root location,
word string, and ordinal are not identity, so fixture graduation and duplicate surface forms remain
safe.

Machine-root fixtures inherit `hc-csharp` founding-oracle provenance under `PROTOCOL.md`. A staged
fixture used by the matrix must explicitly declare `hc-csharp` or `hc-rust` provenance and name its
evidence. A provenance validator fails if a fixture claims C# without a checked-in expected result or
if a matrix case lacks a stable ID. Existing non-matrix fixtures remain backward compatible.
For a selected Machine-root case, the stable ID lands through a coordinated Machine change and
submodule bump; PanGloss must not maintain an overlay copy or key the case by ordinal. Until that
upstream-compatible ID is integrated, the word is not matrix-selected.

Alternative rejected: a second Rust registry containing inline XML, words, and expected analyses.
That would duplicate semantic data and make eventual C# checking a migration rather than a rerun.

Tests: `pg-conformance-fixtures` unit tests for uniqueness, graduation-stable identity, duplicate
surface forms, missing IDs, and false C# provenance; `backend_semantic_matrix_gate` asserts every
referenced case resolves exactly once.

### D2. The matrix consumes the capability sources of truth

The matrix does not repeat backend support classifications. Its denominator is derived from
`CharacteristicKind::ALL`, `strategy_coverage::ALL_STRATEGIES`,
`strategy_coverage::representation_of`, the characteristics profile of each fixture, and a finite
configuration/interaction obligation registry.

For a represented configuration, a strict audit path is exercised and must realize the requested
strategy without fallback. `RepresentsWithKnownGap` remains represented and inside the red
denominator: every selected known-gap case must be repaired to semantic parity in this change. It
cannot be downgraded or reclassified as a refusal to make the matrix green. Only already-declared
`CannotRepresent` cells are conceptual blanks.

For `CannotRepresent`, direct forcing is not support evidence. The normal selector must return an
attributable refusal naming the characteristic/configuration. That cell is conceptually blank and
does not contribute to the supported denominator. A compile error, panic, silent absence, or fallback
is not an honest refusal.

Alternative rejected: a matrix-owned expected-status table. Two status ledgers would eventually
disagree, recreating the inheritance trap this change exists to expose.

The strict audit policy is explicit (for example, `ForbidFallback`) and test-only or audit-facing.
It returns a typed fallback outcome instead of silently substituting another strategy. Existing
production fallback remains unchanged and receives a separate regression test so the matrix neither
forbids nor mistakes shipping fallback behavior for direct-backend evidence.

Tests: a denominator/completeness test cross-checks every obligation against the live enums and
representation table; sabotage tests remove a backend observation and change a refusal into an
unattributed failure to prove both paths turn red.

### D3. One grouped runner proves four distinct claims

The shared runner loads each fixture once, resolves its selected cases, and prepares the direct Rust
HermitCrab oracle once per fixture. For every case it separately proves:

1. **Fixture parity:** the direct `pg_parse::Morpher` result matches the checked-in status/signature.
2. **Backend attribution:** under the explicit strict audit policy, the selected/forced candidate's
   requested and realized strategies match and no fallback occurred.
3. **Proposal containment:** every admission identity required by the complete direct-oracle result
   is present in final `propose UNION peel` candidates. Extra proposals are permitted and counted.
4. **Semantic analysis equality:** after confirmation, the deduplicated structured analysis identity
   set equals the direct Rust HermitCrab-only set. Checked-in signature parity remains the portable
   C#-compatible semantic anchor; duplicate discovery counts are reported but do not alter equality.

Positive construct witnesses must have a non-empty direct-oracle result, a nonzero proposal count,
and at least one confirmed expected identity. Their paired negative controls must have a checked-in
empty result and an empty confirmed result; they may have extra proposals because confirm-only
overapproximation is valid. Ambiguity and duplicate-sensitive claims use additional named cases rather
than forcing a literal one-word-per-feature rule.

Use the production backend entry points and query encoders already selected by
`LoweringAdapter`/`backend_runtime`; do not reproduce tokenization in the test. The initial wiring
slice includes plain ASCII, multicharacter-segment, NFD, and boundary-sensitive cases so the test
cannot pass while only one query space works.

Alternative rejected: comparing only final confirmations. That can hide a wrong requested backend,
fallback, truncation, or a zero-proposal path whose positive oracle also became vacuous.

Tests: focused `backend_semantic_matrix_gate` tests for positive, negative, ambiguity, Unicode/token
encoding, boundary cleanup, and a fault-injected removed proposal.

### D4. Outcomes are total and incomplete work is never comparable

Each backend/case observation has one typed outcome: semantically checked, selector refusal,
fallback forbidden, compile failure, oracle incomplete, apply incomplete, or harness error. Only
semantically checked results may be compared. Expected `CannotRepresent` plus attributable selector
refusal is green; every other non-semantic outcome for a represented obligation is red.

An oracle step cap, apply budget, wall timeout, worker termination, missing fixture, zero selected
cases, malformed matrix fixture, or missing output is not an empty analysis result. The ordinary
matrix uses deterministic logical budgets sized so its declared cases complete. Separate operational
tests reuse the same fixtures to pin below/above-boundary typed outcomes without asking C# to share
Rust work units.

No unexpected failure is ignored or entered into a `KnownGap` allowlist. The branch may remain red
while backend corrections are in progress, but cannot merge until represented obligations are green.

Tests: zero-case, missing-fixture, truncated-oracle, truncated-apply, compile-failure, and silent-
fallback falsifications must each fail for their own reason.

### D5. The denominator has named layers, not an exhaustiveness slogan

The matrix reports these separate obligations:

- **Representation floor:** characteristic/configuration partitions by backend, with honest
  unsupported cells present only as refusal checks.
- **Interaction obligations:** the finite, externally owned registry of admissible non-orthogonal
  feature/configuration combinations. Existing conformance `exercises` tags and plan-interaction
  evidence may satisfy a row but do not define the registry by their own presence.
- **Ordering archetypes:** named feeding/bleeding and `A→B`/`B→A` contrasts plus small permutation
  invariance checks for declared unordered/proven-commutative cases. These are representative until
  the reachable rule-pair census can supply the complete per-grammar obligation set.
- **Boundary/operational obligations:** query encodings, zero/one/many boundaries, accepted/refused
  configuration edges, and deterministic below/above resource thresholds.

Every obligation is `required`, `honest-unsupported`, or `retired` with cited proof. There is no
unclassified state. Early schema work proves fail-closed scope membership symbolically; it does not
guess final counts. After fixture selection and representative runtime measurement, the PR and full
managed scopes publish exact case/obligation IDs and counts and fail if discovery shrinks. The full
suite may be slower, but it is not allowed to be report-only.

Interaction and ordering IDs are externally owned by the reconciled
`add-pairwise-grammar-interaction-coverage` artifacts and
`docs/rule-interaction-and-ordering-coverage-plan.md`. This matrix consumes those stable IDs and
maps them to backend runs. It does not independently define reachability, exclusions, or retirement.

Alternative rejected: raw pairwise generation across every `ConstructKnobs` field. The current
generator activates singular constructs through mutually exclusive assembly, raw pairs are often
unreachable or orthogonal, and a generated row cannot define its own completeness denominator.

Tests: pinned non-vacuity and exact-denominator tests for both scopes, plus a synthetic missing-row
falsification.

### D6. C# is an on-demand semantic provenance lane

No new C# command or oracle format is introduced. The selected fixture grammar and words use the
existing Machine conformance adapter and five-column TSV protocol. A managed wrapper exports the
matrix-selected fixture/case set, invokes the existing adapter when explicitly requested, compares
against checked-in expectations, and records match/mismatch/not-run evidence.

Ordinary Rust PR gates do not require a local C# checkout. The fake-adapter wrapper contract is
required regardless of local availability. A live adapter run is an availability-dependent evidence
checkpoint: when available, an explicit `require-csharp` invocation is red on mismatch or
`not_run`; when unavailable, the result is recorded as `not_run`, preserves prior checked-in
provenance, and cannot create or upgrade a C# claim. C# comparison covers observable signatures only.
PanGloss is allowed—and expected—to differ in typed containment, resource budgets, and safe handling
of C# crash bugs.

The matrix accepts only strict-XML-loadable fixtures for C# verification. The sibling HC-parity
change records that many staged fixtures currently contain illegal XML comments; this change fixes
only selected matrix fixtures after rebasing with the conformance owner and does not take ownership
of the whole malformed-fixture backlog.

Tests: provenance validation plus a wrapper contract test using a fake adapter; the real C# run is an
explicit evidence command, not a mocked semantic gate.

### D7. Reuse fixture stimuli for Rust-only operational assertions

Backend wiring, selector attribution, encoder dispatch, proposal mechanics, FST topology/counters,
budget thresholds, cache behavior, optimizer scores, worker supervision, fault injection, and matrix
accounting remain Rust assertions. When they have observable semantic inputs, they reference the same
fixture/case IDs rather than embedding duplicate grammars.

Small inline grammars remain permitted only for claims that cannot be meaningfully expressed as HC
semantics: malformed/missing files, injected faults, process death, cache call counts, or exact
PanGloss work-unit boundaries. Such tests cannot count as construct or interaction witnesses and
cannot claim C# provenance.

This boundary is documented in `docs/research/backend-semantic-matrix.md` and pinned by registry
validation so an inline operational test cannot accidentally satisfy a semantic obligation.

### D8. Extend or rename compile-only “witnessed” coverage

There must be one meaning of semantic witness. The existing compile-only collector is refactored so
`Compiled` is explicitly compile observation, while semantic witness credit comes only from the
matrix runner's successful proposal-containment and confirmation result. If integrating the semantic
result cleanly would entangle the collector, its public/report labels are narrowed from “witnessed”
to “compiled” and the matrix becomes the sole semantic witness account.

Alternative rejected: leave both reports named witnessed and document the distinction. Green reports
are cited without their caveats; the repository already records that a green gate that can silently
lie is worse than an advisory one.

### D9. Test-first concurrent integration

Implementation is sequenced as follows:

1. Land the local schema, registry validation, grouped runner, falsification tests, and a
   three-backend wiring slice without editing concurrent fixtures or backend implementation hotspots.
2. Integrate the concurrent Machine/conformance/backend branch, rebase this branch onto that accepted
   tip, assign overlapping file ownership, and resolve fixture IDs/provenance once.
3. Add the full required semantic cases as failing acceptance tests. The acceptance-test author owns
   their direction; backend implementers may not edit them.
4. Classify each remaining red result as harness/query wiring, honest configuration refusal, or real
   backend undergeneration. Fix backend code in hotspot-serialized slices, one invariant per commit.
5. Run one authoritative managed matrix suite on the integrated tip and require zero unexpected
   failures before merge.

This permits concurrent progress but not an independently mergeable red test branch. Test-only
infrastructure may merge earlier only if every declared represented obligation in that commit is
already green; omitted future obligations must be named as later tasks, never hidden in a gap count.

## Risks / Trade-offs

- **[Shared Rust oracle is not independent C# proof]** → Checked-in fixture expectations carry
  explicit provenance; C# is rerun on demand for new/ambiguous semantic claims, and reports never
  promote `hc-rust` evidence to `hc-csharp`.
- **[Bundled fixtures falsely credit multiple characteristics]** → Each obligation names stable
  positive/negative case IDs and structural predicates; a fixture-level compile cannot witness every
  characteristic it merely contains.
- **[Runtime grows with fixtures × backends]** → Group by fixture, reuse oracle preparation, measure
  representative runs before assigning PR/full scopes, and use `rust/tools/pg.ps1` target selection.
  Both scopes remain fail-closed for their stated denominator.
- **[Matrix becomes a second sprawling conformance framework]** → Store no semantic XML or expected
  analyses in the runner; consume fixture discovery, expected signatures, strategy coverage, and
  selector diagnostics. Add only stable references and obligation metadata.
- **[Capability can be downgraded to make red disappear]** → Matrix tests cannot alter dispositions;
  selected `RepresentsWithKnownGap` cases must be repaired here, and only existing
  `CannotRepresent` cells may be blank.
- **[Concurrent Machine or fixture edits conflict]** → Record the other agent's branch and exact file
  ownership, integrate/rebase its accepted tip first, coordinate Machine IDs and manifest/submodule
  changes, and keep backend hotspot ownership serialized per `STAGING.md`.
- **[C# crash or timeout is mistaken for desired behavior]** → C# supplies completed semantic
  signatures only. Rust containment and typed incomplete outcomes are tested independently and may
  deliberately improve on C# operational behavior.
- **[Exact counter assertions become brittle]** → Assert counters only where they prove a mechanism
  engaged or enforce a named deterministic budget; do not use wall clock or incidental state counts as
  semantic evidence.

## Migration Plan

1. Introduce optional fixture schema fields and validation without changing existing fixture replay.
2. Add the matrix registry/runner and migrate the existing cross-compiler equivalence slice into it
   without deleting the old gate until the new falsification tests pass.
3. Add the PR denominator, then the full denominator, keeping every newly declared represented cell
   red until its backend behavior is corrected.
4. Reclassify compile-only witness terminology after the semantic runner is authoritative.
5. Add the on-demand C# wrapper/provenance check and, when the live adapter is available, verify at
   least one C#-provenanced matrix fixture end to end; otherwise record `not_run` without creating or
   upgrading C# provenance.
6. Remove duplicated legacy test helpers only after equivalent matrix assertions and managed gates
   pass on the integrated tip.

Rollback is commit-wise: schema additions are backward compatible; the new runner and fixture IDs can
be reverted without changing production behavior. Backend corrections land in separate focused
commits so any regression can be reverted without removing the matrix that exposed it.

## Open Questions

None. The grill resolved the last scope question: `CannotRepresent` is an honest blank whose explicit
selector refusal is green; this change does not implement missing backend capability.

## OpenSpec validation note

The repository's code-defined workflow declares proposal, design, and tasks only; it has no
spec-delta artifact. `openspec status` reaching 3/3 is the applicable completion check. The generic
strict validator's `no deltas found` error is a known schema mismatch and must not be “fixed” by
inventing an unused spec directory.
