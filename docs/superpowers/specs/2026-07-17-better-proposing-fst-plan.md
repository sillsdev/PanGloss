# Plan: the better proposing FST (precision via derivation-aware emit)

Status: **Phase 0 DONE 2026-07-17** (census merged at `328b5e8`, harness
`rust/crates/hc-foma/examples/deadend_census.rs`; verdicts below). NEXT = build E5 then E2
(see verdicts); E1/E3/E4 PARKED build-ready, not dead. **This plan is now also driven by a
repeatable skill** — `.claude/skills/dead-end-census` — run it whenever a new grammar is
slow; it is the standing first lever for a new language, and the mechanism that promotes a
parked encoding to built when a grammar's census licenses it. Originally PLANNED 2026-07-17
(John's direction: "put the rest of the complexity that HC is
handling in one or more FSTs" — staged version: ONE tighter proposing FST per grammar,
HC still confirms). This is the concrete execution plan for foma-fst-plan §P6 workstream 1
(replace-rule compilation), widened to cover the non-phonological precision gaps and
disciplined by the same census-first rule the pre-filter plan used — which is what killed
that plan honestly (`2026-07-16-candidate-prefilter-plan.md`, Phase 0 NO-GO at `571b8a3`).
Execute AFTER round-3 perf merges land and the quiet-machine A/B baseline is recorded.

## Why this exists (the money)

- The 2026-07-17 census settled where confirm time goes: **91–98% of failing-candidate
  time is cascade dead-ends** — HC exhaustively proving no derivation exists — on every
  grammar at every sample size. Final-gate validity rejections are 0–3%. No post-propose
  screen can touch dead-ends (that's why the pre-filter died); the only lever left on this
  axis is to stop PROPOSING them.
- Candidate precision today: **Sena 5%** (51.5 candidates/word vs 2.6 real), Amharic 31%,
  Indonesian 65% (knob_probe, 2026-07-16). Sena spends ~97% of confirm time on failing
  candidates. If precision rose to even ~50% with recall intact, Sena confirm cost shrinks
  by most of that 97% — several times more than any remaining micro-optimization.
- Propose gets cheaper too, or at worst stays bounded: fewer accepted paths per word,
  against a (budgeted) larger network.

## What this is NOT (settled decisions that stand)

- **NOT the precision knob.** Three FST-embedded knob mechanisms (flags / eliminate /
  compose-on-pre-resolved-emit) failed structurally and were torn down. This plan has no
  runtime tuning surface: one emitter, one network per grammar, no presets. The settled
  principle "minimal maximally permissive + HC prune" is AMENDED, not repealed: maximally
  permissive stays a hard floor on RECALL; "minimal" now means "as tight as a census
  proves is worth paying for," decided at plan time, not by a knob.
- **NOT retiring HC.** Propose→confirm stays (FST-only decision criterion, SETTLED
  2026-07-15). The correctness asymmetry is the point: an FST precision bug costs speed;
  it can never cost a wrong analysis, because HC still confirms every candidate. Full
  retirement would flip every FST bug into a wrong-analysis bug and require exact
  equivalence (multiplicity included — Sena mbali = 8). Out of scope, on record as such.
- **NOT flags for adjacency.** A left-environment is an adjacency constraint; the knob
  work proved persistent flags cannot encode it (miseru under-generation; 1.5 GB
  micro-lexicon blowup). Everything here that touches environments uses COMPOSITION over
  boundary-marked strings, where adjacency is native. Flags remain legitimate only for
  genuinely long-distance families (feature agreement, E4).

## The soundness contract (unchanged, never negotiable)

- 100% recall at every stage: the new network's candidate set must contain every
  HC-confirmed analysis, for every corpus word and every conformance fixture, both
  engines' fixtures. Uncertain ⇒ emit permissively (approximate only upward).
- Tightening must be monotone: the new candidate set must also be a SUBSET of the current
  emitter's set (never trade one over-proposal for a new one — cheap to check with the
  existing `propose_parity` dump harness, set-containment instead of byte-equality).
- Every encoding ships in SHADOW first: build both networks, propose with both, assert
  (confirmed ⊆ new ⊆ old) over all corpora + conformance before the new emitter becomes
  the default for that grammar. Per-grammar flip, not global.

## Phase 0 — dead-end attribution census (measure BEFORE building; go/no-go per encoding)

The 2026-07-17 census classified failing candidates as (a) final-gate vs (b) dead-end but
did NOT record WHY cascades die. That attribution decides which encodings below are worth
building — and it is genuinely unknown: Sena has 72 allomorph environment constraints and
zero rewrite rules, so its 95% junk rate must come from environments-during-derivation,
disjunctive allomorph blocks, feature clash, or ordering — each pointing at a DIFFERENT
encoding. Do not guess; the knob taught us plausible increments buy ~nil (AllFlags:
0.0504→0.0506 precision at 8.4x lexc).

Instrument (reusing `confirm_one_traced` / `parse_word_selected_traced`, additive only):
for each failing candidate, record the **deepest failure frontier** — which pinned rule's
unapply/apply attempt got furthest, and what killed the furthest-reaching attempt:

- (d1) allomorph environment check failed against the intermediate shape
- (d2) disjunctive-allomorph block (first-match-wins picked a different allomorph than the
  FST's segmentation assumed)
- (d3) feature unification clash between pinned morphemes (record which features)
- (d4) shape mismatch — no rule sequence reproduces the surface (phonology-order effects;
  expect this to dominate Amharic)
- (d5) ordering/slot violation (stratum or template order excludes the pinned sequence)
- (d6) other/unattributable

Deliverable: table (grammar × d1–d6 × % of failing-candidate wall time + counts), measured
as a counterfactual under the real batched `confirm_batch` (same method as the pre-filter
census — naive per-candidate sums are untrustworthy near a gate). Corpora: Sena sample-300
+ the 1000-cap slice, Amharic ≥400 words (smaller Amharic samples provably mislead —
12–28% inflation seen at <400), Indonesian full.

**Go bars:** an encoding is buildable only if its matching dead-end class is ≥20% of
failing-candidate time on at least one grammar, and the projected end-to-end win (class
share × that grammar's failing-time fraction of confirm) is ≥15% of the grammar's confirm
time. If NOTHING crosses the bar, this plan stops at Phase 0 like its predecessor — write
the NO-GO into memory and the plan doc, and the remaining lever is per-attempt cascade
cost only.

## Phase 0 RESULTS (2026-07-17, merged `328b5e8`; verdicts are the main loop's)

Attribution = deepest failure frontier in one combined analysis+synthesis trace tree per
failing candidate; time shares = counterfactuals under the real `confirm_batch`. The
"shallowest frontier" alternate definition disagrees completely (collapses ~everything to
d4 because cheap per-allomorph shape checks fire first on every branch) — "deepest" is the
plan's literal instruction and the informative one, but d4's share is definition-sensitive.

Projected end-to-end win (class time-share × failing-time fraction of confirm):

| Class | Sena-300 | Sena-1000 | Indonesian | Amharic (236w, full fixture) |
|---|---|---|---|---|
| d1 env | 1.1% | 1.5% | ~4%* | ~0% |
| d2 disj | 1.4% | 1.5% | ~5%* | ~0% |
| d3 feat | 14.1% | 17.2% | ~6%* | 14.4% |
| d4 shape | **22.4%** | **23.1%** | **16.8%** | **28.6%** |
| d5 order | **56.7%** | **49.5%** | **20.3%** | 6.2% |
| d6 other | 0.2% | 1.0% | ~6%* | 0.3% |

(*Indonesian's counterfactual denominator was only ~13ms total — directional only. A
40-word Amharic slice inverted d4/d5 vs the 236-word run, re-confirming the small-sample
trap harder than the plan's cited 12–28%. Both runs now include the pinned worst words —
`samples/data/<grammar>-worst-words.txt`, generated by `examples/worst_words.rs` — union'd
into the census slice so a small cap can never miss the words that dominate confirm time.)

**Verdicts — these are per-corpus licenses, NOT permanent judgments on the encodings.**
John's framing (2026-07-17): the reference grammars are three small samples; an encoding
that earns nothing here may be *exactly* the lever the next slow grammar needs. So the axes
are BUILT (census licensed it on today's corpus) vs. PARKED (design kept build-ready,
re-evaluated the moment a new grammar's census crosses the bar) — never "dead."

- **E2 (d4, replace rules) — BUILD.** Clears both bars on every grammar today; dominant on
  Amharic as predicted. Sequenced after E5 (E5 is cheaper and reshapes the candidate
  distribution E2's census slice would measure).
- **E5 (d5, ordering/PartialParse) — BUILD, FIRST — and it is NEW (no E1–E4 covered it).**
  The single largest class on Sena (~50–57%) and Indonesian (~20%): the FST proposes
  morpheme sets/orders the strata and templates cannot realize into a complete parse.
  Encoding: stratum/template-order-faithful continuation-class structure in the emitted
  lexc — pure morphotactics, no composition, likely the cheapest encoding of all. Verify
  what d5's frontier concretely is (sample real d5 candidates from the harness) before
  building. That E1–E4 missed the dominant class is the census's headline lesson: never
  pre-commit an encoding roster ahead of attribution.
- **E4 (d3, feature bundles) — PARKED (borderline), with a caveat that may redirect it.**
  Projection straddles the 15% bar today, AND the d3 mass is overwhelmingly
  `HeadRequiredSyntacticFeatureStruct` — the COMPOUNDING head gate, not affix features
  (Sena-300: 4,966 of 5,090). Likely spurious root+root compound proposals, plausibly
  fixable in lexc morphotactics for far less than a feature encoding. Split d3 by
  analysis/synthesis side + rule kind before ever building; a future agreement-heavy
  grammar (Bantu noun-class concord, Semitic broken plurals) is the likely trigger.
- **E1 (d1, environments) — PARKED** (≤2% here; Sena's 72 env constraints are NOT where
  *these* dead-ends die — but a grammar dense in allomorph environments could flip this).
  **E3 (d2, disjunctive blocks) — PARKED** (≤2.6% here; a grammar with large first-match
  allomorph blocks could flip it). Neither is built now; both designs stay ready below.

## The build-time encoding registry (the per-grammar lever — NOT the runtime knob)

John asked: "can/should we have a lever per feature as we build?" Yes — but it is a
different mechanism from the torn-down knob, and the distinction must stay explicit or a
future reader will think the teardown was reversed. **The runtime-knob teardown STANDS.**
What failed was a *runtime precision/recall auction* whose *mechanism* (flag density) could
not encode adjacency (miseru under-generation, 1.5 GB micro-lexicon, ~nil precision gain).
What this is: a **build-time, per-grammar selection of which recall-gated encodings compose
into that grammar's proposer.** No runtime tuning surface, no presets, no auction.

- **Shape.** An emitter-side registry of encodings (E2, E5, and any parked design once
  built), each a pure function `Grammar → (lexc/regex contribution, applicability)`. A
  per-grammar config (defaulting to the plain emitter) names which are active. Activation is
  driven by that grammar's census, not by a human turning dials at runtime.
- **Why per-grammar enable is low-risk but NOT "sound by construction."** Each encoding is
  individually recall-gated (100% recall + monotone tightening, below) and only ever
  *removes* over-proposals — so enabling one can only cost speed, never a lost analysis.
  BUT two individually-safe encodings can still interact (composition order, shared boundary
  symbols), so every enabled *combination* a grammar actually uses takes the shadow-first
  recall check (gate 1–2 below) on that grammar before it becomes the default. That
  per-config gate is exactly what the old knob lacked. Do not write "sound by construction."
- **Parked vs. built.** The registry only ever contains encodings a census licensed and we
  therefore built. Parked designs (E1/E3/E4 today) live in this doc as build-ready specs,
  not as dead code in the tree — untested, unexercised encoding code would rot and violate
  the census-first discipline. The `dead-end-census` skill is what promotes a parked design
  to built+registered when a new grammar's census crosses its bar. (If instead we ever want
  code-present-but-disabled, that is a deliberate future decision, recorded here — not the
  default.)
- **Regression guard.** The pinned worst-word set doubles as the acceptance check for any
  newly enabled encoding: did it actually speed up the words that hurt, at 100% recall?

## The encodings (build ONLY what a census licenses, largest attributed share first)

### E5 — order-faithful continuation classes (targets d5) — BUILD FIRST, new this census
The dominant class on Sena and Indonesian, and the one no prior encoding covered. d5 =
the FST proposes a morpheme set/order that no stratum + affix-template ordering can realize
into a complete parse, so HC exhausts the whole cascade proving it. Encoding: make the
emitted lexc's continuation-class graph faithful to stratum order and affix-template slot
order, so an unrealizable order is simply not a path in the network — pure morphotactics,
no composition, no boundary tape. Cheapest encoding of the set; lowest blowup risk. Before
building: sample real d5-frontier candidates from `deadend_census.rs` to confirm the
frontier is genuinely ordering (not a shape effect misattributed by the deepest-frontier
proxy), and confirm which of stratum-order vs template-slot-order carries the mass.

### E1 — boundary-marked emit + composed environment restrictions (PARKED; targets d1)
Emitter v2 keeps a morpheme-boundary symbol in the lexc output instead of stripping it,
emits each allomorph's environment constraint (from `ConstraintCatalog`, preserved by the
knob teardown exactly for this) as a foma context-restriction regex over the boundary-marked
tape, composes all restrictions with the lexicon offline, then composes a final
boundary-deletion transducer. Adjacency is native to composition — this is the mechanism
the flag encoding structurally could not be. Unencodable constraint features (anything not
expressible as a segment-class context) decline permissively, per constraint.

### E2 — replace-rule compilation for phonology (BUILD after E5; targets d4; Amharic)
The foma-fst-plan §P6 sketch, executed: emit UNDERLYING forms; compile each HC prule to a
foma replace rule (feature contexts → segment classes; α-variables → tuple-indexed
expansion, bounded by the grammar's actual feature domains); compose the cascade in stratum
order; project the surface side. This retires the pre-expansion enumeration bridge for
covered rules (its named scale-proof successor) — rules it cannot compile stay pre-expanded
(hybrid emit, upward-safe by construction). Prereq: `f0_viability.rs` already proves the
vendored replace/compose machinery works at toy scale; the blocker was only ever the
pre-resolved emit representation, which E1's boundary-marked emit removes.

### E3 — disjunctive-block priority (PARKED; targets d2)
First-match-wins allomorph blocks are a priority union (`.P.`) over the block's
alternatives with their contexts; compose per block. Only worth building if d2 shows up —
suspected small, but Sena's DisjunctiveAllomorph count in the last census (250–641 by
count) says don't assume.

### E4 — coarse feature-bundle compatibility (PARKED; targets d3)
Two candidate mechanisms, chosen by measured size: partition continuation classes by a
COARSE finite feature signature (only the features d3 says actually clash), or U/R/D-typed
flags (long-distance, the family flags are actually suited for — and the thing whose
absence kept the knob's eliminate auction empty). Feature domains are finite but the
encoding is exponential in interacting features; the census's "which features clash"
breakdown caps the signature to the profitable few. Highest blowup risk of the four;
build last, budget hardest.

## Budgets and kill switches (every encoding, every grammar)

Reuse knob_probe's kill switches: 600 s wall / 64 MB lexc / 3M states / 10 s per-word
propose — any trip = that encoding declines for that grammar (permissive fallback), finding
recorded. Budgets against the post-round-3 quiet-machine baseline (record it first):
compile (emit + fsm_lexc_parse_string + compositions) ≤ 2x baseline per grammar; propose
p95 ≤ 1.5x baseline; network ≤ 4x states. Grammar reload is interactive in FieldWorks —
compile time is a product constraint, not vanity. Scale gate before default-flip: synthetic
10⁴-entry lexicon (build-for-full-scale mandate), same budgets.

## Verification gates (per encoding, before its per-grammar default flip)

1. Recall: confirmed-analysis parity vs the full-HC oracle on all corpora — zero losses.
2. Monotonicity: new candidate set ⊆ old candidate set, per word (propose_parity dumps).
3. Conformance: both engines' suites, zero new divergences.
4. Workspace tests + wasm32 check (build path must stay wasm-clean; no timers in it).
5. Measured end-to-end: analyze_words on the standard corpora, quiet machine, median-of-5,
   reported against the Phase 0 projection — if the realized win is <half the projection,
   stop and re-census before building the next encoding.

## Execution shape

Phase 0 is one Sonnet worktree agent (census instrumentation + tables; foreground gates,
no git stash — see worktree-agent traps memory). Each licensed encoding is its own
worktree agent with the gates above; main loop reviews diffs, owns go/no-go and default
flips. Encodings are sequential, largest first — each changes the candidate distribution
the next one's census slice would measure.

## Risks

| Risk | Level | Mitigation |
|---|---|---|
| Phase 0 says nothing crosses the bar | real (pre-filter precedent) | That IS the deliverable; plan stops cheaply, on record |
| Composition state blowup (Karttunen convexity) | high on E4, medium E1/E2 | kill switches + per-constraint permissive decline; budgets |
| Emitter v2 recall regression (the miseru class of bug) | medium | gates 1–3 are exhaustive, per grammar, shadow-first |
| Compile-time regression breaks interactive reload | medium | 2x budget is a hard gate, measured per grammar |
| α-variable expansion explodes on Amharic rule6/7 (~20 bindings) | medium | tuple expansion bounded by census-observed clashing features; decline permissively above budget |
| Sena dead-ends turn out to be d6/unattributable | possible | improve attribution before building anything; never build on a guess |
